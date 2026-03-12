//! Handlers for tarball download endpoints.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};
use futures::StreamExt;
use npm_entity::{package_versions, packages};
use sea_orm::{ColumnTrait, EntityTrait, ModelTrait, QueryFilter};
use tracing::debug;

use crate::{auth::AuthUser, error::RegistryError, state::AppState};

/// `GET /{package}/-/{filename}`
pub async fn get_tarball(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((package, filename)): Path<(String, String)>,
) -> Result<Response, RegistryError> {
    let version = version_from_filename(&package, &filename).ok_or_else(|| {
        RegistryError::BadRequest(format!("cannot parse version from filename '{}'", filename))
    })?;
    do_stream(state, &package, &version, &filename).await
}

/// `GET /@{scope}/{name}/-/{filename}`
pub async fn get_scoped_tarball(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((scope, name, filename)): Path<(String, String, String)>,
) -> Result<Response, RegistryError> {
    let full_name = format!("@{}/{}", scope, name);
    let version = version_from_filename(&full_name, &filename).ok_or_else(|| {
        RegistryError::BadRequest(format!("cannot parse version from filename '{}'", filename))
    })?;
    do_stream(state, &full_name, &version, &filename).await
}

async fn do_stream(
    state: AppState,
    package_name: &str,
    version: &str,
    filename: &str,
) -> Result<Response, RegistryError> {
    // Try local first.
    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(package_name))
        .one(&state.db)
        .await?;

    if let Some(pkg) = pkg {
        let ver = pkg
            .find_related(package_versions::Entity)
            .filter(package_versions::Column::Version.eq(version))
            .filter(package_versions::Column::DeletedAt.is_null())
            .one(&state.db)
            .await?;

        if let Some(ver) = ver {
            return stream_from_s3(&state, &ver.s3_key, ver.size, filename).await;
        }
    }

    // Check if we have a cached upstream tarball in S3.
    let cache_key = npm_upstream::upstream_tarball_s3_key(package_name, version);
    if let Some(upstream) = &state.upstream
        && upstream.config().cache_enabled
            && let Some(meta) = state
                .storage
                .head_object(&cache_key)
                .await
                .map_err(RegistryError::Storage)?
            {
                debug!(
                    package = %package_name,
                    version = %version,
                    "serving cached upstream tarball from S3"
                );
                return stream_from_s3(&state, &cache_key, meta.size, filename).await;
            }

    // Not found locally or in cache — try upstream.
    stream_from_upstream(&state, package_name, version, filename).await
}

/// Stream a tarball from local S3 storage.
async fn stream_from_s3(
    state: &AppState,
    s3_key: &str,
    size: i64,
    filename: &str,
) -> Result<Response, RegistryError> {
    let stream = state
        .storage
        .download_stream(s3_key)
        .await
        .map_err(RegistryError::Storage)?;

    let body_stream = stream.map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string())));

    let body = Body::from_stream(body_stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", sanitize_filename(filename)),
        )
        .header(header::CONTENT_LENGTH, size.to_string())
        .body(body)
        .map_err(|e| RegistryError::Internal(e.to_string()))?;

    Ok(response)
}

/// Stream a tarball from the upstream registry.
async fn stream_from_upstream(
    state: &AppState,
    package_name: &str,
    version: &str,
    filename: &str,
) -> Result<Response, RegistryError> {
    let upstream = state.upstream.as_ref().ok_or_else(|| {
        RegistryError::NotFound(format!(
            "version '{}' of package '{}' not found",
            version, package_name
        ))
    })?;

    // Use the routing system.
    let route = npm_upstream::resolve_upstream(upstream.config(), package_name);
    let upstream_url = match route {
        npm_upstream::RouteTarget::Local | npm_upstream::RouteTarget::None => {
            return Err(RegistryError::NotFound(format!(
                "version '{}' of package '{}' not found",
                version, package_name
            )));
        }
        npm_upstream::RouteTarget::Upstream(url) => url,
    };

    debug!(
        package = %package_name,
        version = %version,
        upstream = %upstream_url,
        "tarball not found locally, trying upstream"
    );

    // Fetch the packument from upstream to discover the original tarball URL.
    let packument = upstream
        .fetch_packument_from(package_name, &upstream_url)
        .await
        .map_err(|e| super::packument::upstream_error_to_registry(e, package_name))?;

    let tarball_url =
        npm_upstream::proxy::extract_upstream_tarball_url(&packument, version).ok_or_else(
            || {
                RegistryError::NotFound(format!(
                    "version '{}' of package '{}' not found on upstream",
                    version, package_name
                ))
            },
        )?;

    // If caching is enabled, download the full tarball, cache to S3, then serve from memory.
    let config = upstream.config();
    if config.cache_enabled {
        match upstream.download_tarball(&tarball_url).await {
            Ok(data) => {
                let cache_key = npm_upstream::upstream_tarball_s3_key(package_name, version);
                let data_len = data.len();

                // Upload to S3 cache in background (best-effort).
                let storage = state.storage.clone();
                let cache_key_owned = cache_key.clone();
                let data_clone = data.clone();
                tokio::spawn(async move {
                    if let Err(e) = storage
                        .upload(&cache_key_owned, data_clone, "application/octet-stream")
                        .await
                    {
                        tracing::warn!(key = %cache_key_owned, error = %e, "failed to cache upstream tarball");
                    }
                });

                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/octet-stream")
                    .header(
                        header::CONTENT_DISPOSITION,
                        format!(
                            "attachment; filename=\"{}\"",
                            sanitize_filename(filename)
                        ),
                    )
                    .header(header::CONTENT_LENGTH, data_len.to_string())
                    .body(Body::from(data))
                    .map_err(|e| RegistryError::Internal(e.to_string()))?;

                return Ok(response);
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to download tarball for caching, falling back to streaming");
                // Fall through to streaming below.
            }
        }
    }

    // Stream the tarball from upstream (no caching).
    let (stream, content_length) = upstream
        .stream_tarball(&tarball_url)
        .await
        .map_err(|e| super::packument::upstream_error_to_registry(e, package_name))?;

    let body_stream = stream.map(|chunk| chunk.map_err(|e| std::io::Error::other(e.to_string())));
    let body = Body::from_stream(body_stream);

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", sanitize_filename(filename)),
        );

    if let Some(len) = content_length {
        builder = builder.header(header::CONTENT_LENGTH, len.to_string());
    }

    let response = builder
        .body(body)
        .map_err(|e| RegistryError::Internal(e.to_string()))?;

    Ok(response)
}

/// Strip characters that could be used for header injection or path traversal.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@'))
        .collect()
}

fn version_from_filename(package_name: &str, filename: &str) -> Option<String> {
    let stem = filename.strip_suffix(".tgz")?;
    let bare = if let Some(rest) = package_name.strip_prefix('@') {
        let slash = rest.find('/')?;
        &rest[slash + 1..]
    } else {
        package_name
    };
    let prefix = format!("{}-", bare);
    let version = stem.strip_prefix(&prefix)?;
    Some(version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_filename() {
        assert_eq!(
            version_from_filename("express", "express-4.18.2.tgz"),
            Some("4.18.2".to_string())
        );
    }

    #[test]
    fn parse_scoped_filename() {
        assert_eq!(
            version_from_filename("@babel/core", "core-7.21.0.tgz"),
            Some("7.21.0".to_string())
        );
    }

    #[test]
    fn rejects_wrong_suffix() {
        assert_eq!(version_from_filename("pkg", "pkg-1.0.0.tar.gz"), None);
    }
}
