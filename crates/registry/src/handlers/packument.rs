//! Handlers for the packument (package metadata) endpoints.
//!
//! - `GET /{package}`          – unscoped package packument
//! - `GET /@{scope}/{name}`    – scoped package packument

use axum::{
    Json,
    extract::{Path, State},
};
use npm_entity::{dist_tags, package_versions, packages};
use sea_orm::{ColumnTrait, EntityTrait, ModelTrait, QueryFilter};
use serde_json::{Value, json};
use tracing::debug;

use crate::{
    auth::AuthUser,
    error::{RegistryError, Result},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// `GET /{package}` – plain (non-scoped) package packument.
pub async fn get_packument(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(package): Path<String>,
) -> Result<Json<Value>> {
    build_packument(&state, &package).await
}

/// `GET /@{scope}/{name}` – scoped package packument.
pub async fn get_scoped_packument(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((scope, name)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let full_name = format!("@{}/{}", scope, name);
    build_packument(&state, &full_name).await
}

// ---------------------------------------------------------------------------
// Core packument builder
// ---------------------------------------------------------------------------

async fn build_packument(state: &AppState, package_name: &str) -> Result<Json<Value>> {
    // Look up the package record.
    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(package_name))
        .one(&state.db)
        .await?;

    match pkg {
        Some(pkg) => {
            let local = build_local_packument(state, package_name, pkg).await?;

            // Attempt to merge with upstream if configured and the route allows.
            if let Some(upstream) = &state.upstream {
                let route =
                    npm_upstream::resolve_upstream(upstream.config(), package_name);
                if let npm_upstream::RouteTarget::Upstream(upstream_url) = route
                    && let Some(mut upstream_packument) =
                        try_fetch_upstream_for_merge(state, package_name, &upstream_url).await
                {
                    npm_upstream::proxy::rewrite_tarball_urls(
                        &mut upstream_packument,
                        &state.config.registry_url,
                    );
                    return Ok(Json(merge_packuments(local.0, upstream_packument)));
                }
            }
            Ok(local)
        }
        None => {
            // Package not found locally — try upstream if configured.
            fetch_upstream_packument(state, package_name).await
        }
    }
}

/// Build a packument from locally stored package data.
async fn build_local_packument(
    state: &AppState,
    package_name: &str,
    pkg: packages::Model,
) -> Result<Json<Value>> {
    // Fetch all non-deleted versions.
    let versions: Vec<package_versions::Model> = pkg
        .find_related(package_versions::Entity)
        .filter(package_versions::Column::DeletedAt.is_null())
        .all(&state.db)
        .await?;

    // Fetch dist-tags.
    let dist_tag_rows: Vec<dist_tags::Model> =
        pkg.find_related(dist_tags::Entity).all(&state.db).await?;

    // Build the versions map: { "1.0.0": { ...version metadata... } }
    let mut versions_map = serde_json::Map::new();
    for v in &versions {
        let tarball_url = build_tarball_url(state, package_name, &v.version);

        // Start from the stored metadata (package.json).
        let mut meta: Value = v.metadata.clone();

        // Inject/override the dist object.
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "dist".to_string(),
                json!({
                    "tarball": tarball_url,
                    "shasum": v.shasum,
                    "integrity": v.integrity,
                }),
            );
        }

        versions_map.insert(v.version.clone(), meta);
    }

    // Build dist-tags map: { "latest": "1.2.3" }
    // We need version strings, so we build a lookup from version_id -> version string.
    let version_id_to_str: std::collections::HashMap<uuid::Uuid, String> =
        versions.iter().map(|v| (v.id, v.version.clone())).collect();

    let mut dist_tags_map = serde_json::Map::new();
    for tag_row in &dist_tag_rows {
        if let Some(ver_str) = version_id_to_str.get(&tag_row.version_id) {
            dist_tags_map.insert(tag_row.tag.clone(), Value::String(ver_str.clone()));
        }
    }

    // Assemble the packument.
    let packument = json!({
        "_id": package_name,
        "name": package_name,
        "description": pkg.description,
        "dist-tags": dist_tags_map,
        "versions": versions_map,
        "time": {
            "created": pkg.created_at.to_rfc3339(),
            "modified": pkg.updated_at.to_rfc3339(),
        },
    });

    Ok(Json(packument))
}

/// Attempt to fetch a packument from the configured upstream registry.
async fn fetch_upstream_packument(
    state: &AppState,
    package_name: &str,
) -> Result<Json<Value>> {
    let upstream = state.upstream.as_ref().ok_or_else(|| {
        RegistryError::NotFound(format!("package '{}' not found", package_name))
    })?;

    // Use the routing system to determine which upstream to use.
    let route = npm_upstream::resolve_upstream(upstream.config(), package_name);
    let upstream_url = match route {
        npm_upstream::RouteTarget::Local => {
            return Err(RegistryError::NotFound(format!(
                "package '{}' not found",
                package_name
            )));
        }
        npm_upstream::RouteTarget::None => {
            return Err(RegistryError::NotFound(format!(
                "package '{}' not found",
                package_name
            )));
        }
        npm_upstream::RouteTarget::Upstream(url) => url,
    };

    debug!(package = %package_name, upstream = %upstream_url, "package not found locally, trying upstream");

    // Check metadata cache if caching is enabled.
    let config = upstream.config();
    if config.cache_enabled {
        // Try fresh cache first.
        if let Some(cached) =
            npm_upstream::get_cached_packument(&state.db, package_name, config.cache_ttl, false)
                .await
        {
            let mut packument = cached;
            npm_upstream::proxy::rewrite_tarball_urls(&mut packument, &state.config.registry_url);
            return Ok(Json(packument));
        }
    }

    // Fetch from upstream.
    let fetch_result = upstream
        .fetch_packument_from(package_name, &upstream_url)
        .await;

    match fetch_result {
        Ok(mut packument) => {
            // Cache the raw packument (before URL rewriting) if caching is enabled.
            if config.cache_enabled {
                npm_upstream::put_cached_packument(
                    &state.db,
                    package_name,
                    &upstream_url,
                    &packument,
                )
                .await;
            }

            // Rewrite tarball URLs so the client fetches tarballs through this registry.
            npm_upstream::proxy::rewrite_tarball_urls(
                &mut packument,
                &state.config.registry_url,
            );
            Ok(Json(packument))
        }
        Err(e) => {
            // If upstream fails and we have a stale cache, serve it (stale-while-error).
            if config.cache_enabled
                && let Some(cached) = npm_upstream::get_cached_packument(
                    &state.db,
                    package_name,
                    config.cache_ttl,
                    true, // allow stale
                )
                .await
                {
                    tracing::warn!(
                        package = %package_name,
                        error = %e,
                        "upstream failed, serving stale cache"
                    );
                    let mut packument = cached;
                    npm_upstream::proxy::rewrite_tarball_urls(
                        &mut packument,
                        &state.config.registry_url,
                    );
                    return Ok(Json(packument));
                }
            Err(upstream_error_to_registry(e, package_name))
        }
    }
}

/// Best-effort fetch of upstream packument for merging with local.
/// Returns `None` on any failure (we already have a local packument to serve).
async fn try_fetch_upstream_for_merge(
    state: &AppState,
    package_name: &str,
    upstream_url: &str,
) -> Option<Value> {
    let upstream = state.upstream.as_ref()?;
    let config = upstream.config();

    // Check cache first.
    if config.cache_enabled
        && let Some(cached) =
            npm_upstream::get_cached_packument(&state.db, package_name, config.cache_ttl, true)
                .await
    {
        return Some(cached);
    }

    // Fetch from upstream — best-effort, don't fail the request.
    match upstream.fetch_packument_from(package_name, upstream_url).await {
        Ok(packument) => {
            if config.cache_enabled {
                npm_upstream::put_cached_packument(
                    &state.db,
                    package_name,
                    upstream_url,
                    &packument,
                )
                .await;
            }
            Some(packument)
        }
        Err(e) => {
            debug!(
                package = %package_name,
                error = %e,
                "could not fetch upstream for merge, serving local-only"
            );
            None
        }
    }
}

/// Merge a local packument with an upstream packument.
///
/// Per PRD §4.1:
/// - Local versions are included as-is
/// - Upstream versions are included only if no local version with the same version string exists
/// - Dist-tags from local always override upstream dist-tags
fn merge_packuments(local: Value, upstream: Value) -> Value {
    let mut merged = local;

    // Merge versions: add upstream versions that don't exist locally.
    if let (Some(local_versions), Some(upstream_versions)) = (
        merged
            .get_mut("versions")
            .and_then(|v| v.as_object_mut()),
        upstream.get("versions").and_then(|v| v.as_object()),
    ) {
        for (version, meta) in upstream_versions {
            if !local_versions.contains_key(version) {
                local_versions.insert(version.clone(), meta.clone());
            }
        }
    }

    // Merge dist-tags: upstream tags are included only if not already in local.
    if let (Some(local_tags), Some(upstream_tags)) = (
        merged
            .get_mut("dist-tags")
            .and_then(|v| v.as_object_mut()),
        upstream.get("dist-tags").and_then(|v| v.as_object()),
    ) {
        for (tag, version) in upstream_tags {
            if !local_tags.contains_key(tag) {
                local_tags.insert(tag.clone(), version.clone());
            }
        }
    }

    merged
}

/// Map upstream errors to appropriate HTTP status codes per PRD section 4.4.
pub(crate) fn upstream_error_to_registry(
    e: npm_upstream::UpstreamError,
    package_name: &str,
) -> RegistryError {
    match e {
        npm_upstream::UpstreamError::NotFound(_) => {
            RegistryError::NotFound(format!("package '{}' not found", package_name))
        }
        npm_upstream::UpstreamError::Timeout(_) => {
            RegistryError::GatewayTimeout("upstream request timed out".to_string())
        }
        npm_upstream::UpstreamError::UpstreamServerError { .. } => {
            RegistryError::BadGateway("upstream server error".to_string())
        }
        npm_upstream::UpstreamError::InvalidResponse(msg) => {
            tracing::error!(error = %msg, "invalid upstream response");
            RegistryError::BadGateway("invalid upstream response".to_string())
        }
        npm_upstream::UpstreamError::CircuitOpen(url) => {
            tracing::warn!(upstream = %url, "circuit breaker open, upstream unavailable");
            RegistryError::BadGateway("upstream temporarily unavailable".to_string())
        }
        other => {
            tracing::error!(error = %other, "upstream proxy error");
            RegistryError::BadGateway("upstream proxy error".to_string())
        }
    }
}

/// Construct the tarball download URL for a given package + version.
fn build_tarball_url(state: &AppState, package_name: &str, version: &str) -> String {
    let base = state.config.registry_url.trim_end_matches('/');
    if let Some(rest) = package_name.strip_prefix('@') {
        // Scoped: @scope/name  →  registry/  @scope/name/-/name-version.tgz
        let slash = rest.find('/').unwrap_or(rest.len());
        let scope = &rest[..slash];
        let name = &rest[slash + 1..];
        format!("{}/@{}/{}/-/{}-{}.tgz", base, scope, name, name, version)
    } else {
        format!(
            "{}/{}/-/{}-{}.tgz",
            base, package_name, package_name, version
        )
    }
}
