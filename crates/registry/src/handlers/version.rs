//! Handler for the specific-version metadata endpoint.
//!
//! - `GET /{package}/{version}`

use axum::{
    Json,
    extract::{Path, State},
};
use npm_entity::{package_versions, packages};
use sea_orm::{ColumnTrait, EntityTrait, ModelTrait, QueryFilter};
use serde_json::{Value, json};

use crate::{
    auth::AuthUser,
    error::{RegistryError, Result},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Route handler
// ---------------------------------------------------------------------------

/// `GET /{package}/{version}` – return a specific version's metadata.
///
/// Both plain packages (`express/1.2.3`) and scoped packages
/// (`@babel/core/7.0.0` – handled by the scoped variant below) are supported.
pub async fn get_version(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((package, version)): Path<(String, String)>,
) -> Result<Json<Value>> {
    fetch_version_metadata(&state, &package, &version).await
}

/// `GET /@{scope}/{name}/{version}` – scoped package version metadata.
pub async fn get_scoped_version(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((scope, name, version)): Path<(String, String, String)>,
) -> Result<Json<Value>> {
    let full_name = format!("@{}/{}", scope, name);
    fetch_version_metadata(&state, &full_name, &version).await
}

// ---------------------------------------------------------------------------
// Core helper
// ---------------------------------------------------------------------------

async fn fetch_version_metadata(
    state: &AppState,
    package_name: &str,
    version: &str,
) -> Result<Json<Value>> {
    // Resolve package.
    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(package_name))
        .one(&state.db)
        .await?
        .ok_or_else(|| RegistryError::NotFound(format!("package '{}' not found", package_name)))?;

    // Resolve the specific version (not soft-deleted).
    let ver = pkg
        .find_related(package_versions::Entity)
        .filter(package_versions::Column::Version.eq(version))
        .filter(package_versions::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| {
            RegistryError::NotFound(format!(
                "version '{}' of package '{}' not found",
                version, package_name
            ))
        })?;

    // Build the dist URL.
    let base = state.config.registry_url.trim_end_matches('/');
    let tarball_url = if let Some(rest) = package_name.strip_prefix('@') {
        let slash = rest.find('/').unwrap_or(rest.len());
        let scope = &rest[..slash];
        let name = &rest[slash + 1..];
        format!("{}/@{}/{}/-/{}-{}.tgz", base, scope, name, name, version)
    } else {
        format!(
            "{}/{}/-/{}-{}.tgz",
            base, package_name, package_name, version
        )
    };

    // Start from stored metadata and inject dist object.
    let mut meta: Value = ver.metadata.clone();
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "dist".to_string(),
            json!({
                "tarball": tarball_url,
                "shasum": ver.shasum,
                "integrity": ver.integrity,
            }),
        );
    }

    Ok(Json(meta))
}
