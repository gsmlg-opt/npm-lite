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

    // Not found locally — try upstream.
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
///
/// For Phase 1, we fetch the upstream packument first to discover the original
/// tarball URL, then stream that tarball through to the client.
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

    debug!(
        package = %package_name,
        version = %version,
        "tarball not found locally, trying upstream"
    );

    // Fetch the packument from upstream to discover the original tarball URL.
    let packument = upstream
        .fetch_packument(package_name)
        .await
        .map_err(|e| match e {
            npm_upstream::UpstreamError::NotFound(_) => {
                RegistryError::NotFound(format!("package '{}' not found", package_name))
            }
            other => {
                tracing::error!(error = %other, "upstream proxy error");
                RegistryError::Internal("upstream proxy error".to_string())
            }
        })?;

    let tarball_url =
        npm_upstream::proxy::extract_upstream_tarball_url(&packument, version).ok_or_else(
            || {
                RegistryError::NotFound(format!(
                    "version '{}' of package '{}' not found on upstream",
                    version, package_name
                ))
            },
        )?;

    // Stream the tarball from upstream.
    let (stream, content_length) = upstream
        .stream_tarball(&tarball_url)
        .await
        .map_err(|e| match e {
            npm_upstream::UpstreamError::NotFound(_) => RegistryError::NotFound(format!(
                "tarball for '{}@{}' not found on upstream",
                package_name, version
            )),
            npm_upstream::UpstreamError::Timeout(_) => {
                RegistryError::Internal("upstream request timed out".to_string())
            }
            other => {
                tracing::error!(error = %other, "upstream tarball proxy error");
                RegistryError::Internal("upstream proxy error".to_string())
            }
        })?;

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
