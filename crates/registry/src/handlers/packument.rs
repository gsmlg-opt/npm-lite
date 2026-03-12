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
        .await?
        .ok_or_else(|| RegistryError::NotFound(format!("package '{}' not found", package_name)))?;

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
