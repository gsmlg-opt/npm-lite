//! Handlers for dist-tag management.
//!
//! - `GET    /-/package/{package}/dist-tags`          – list dist-tags
//! - `PUT    /-/package/{package}/dist-tags/{tag}`    – set a dist-tag
//! - `DELETE /-/package/{package}/dist-tags/{tag}`    – remove a dist-tag

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use npm_entity::{dist_tags, package_versions, packages};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, ModelTrait, QueryFilter,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    auth::{AuthUser, PublishUser},
    error::{RegistryError, Result},
    state::AppState,
};

// ---------------------------------------------------------------------------
// GET /-/package/{package}/dist-tags
// ---------------------------------------------------------------------------

pub async fn list_dist_tags(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(package): Path<String>,
) -> Result<Json<Value>> {
    let pkg = resolve_package(&state, &package).await?;

    let tag_rows: Vec<dist_tags::Model> = pkg.find_related(dist_tags::Entity).all(&state.db).await?;

    // Build { tag -> version_str } map.
    let version_ids: Vec<Uuid> = tag_rows.iter().map(|t| t.version_id).collect();
    let versions: Vec<package_versions::Model> = package_versions::Entity::find()
        .filter(package_versions::Column::Id.is_in(version_ids))
        .all(&state.db)
        .await?;

    let id_to_ver: HashMap<Uuid, String> = versions.iter().map(|v| (v.id, v.version.clone())).collect();

    let mut map = serde_json::Map::new();
    for tag in &tag_rows {
        if let Some(ver) = id_to_ver.get(&tag.version_id) {
            map.insert(tag.tag.clone(), Value::String(ver.clone()));
        }
    }

    Ok(Json(Value::Object(map)))
}

// ---------------------------------------------------------------------------
// PUT /-/package/{package}/dist-tags/{tag}
// ---------------------------------------------------------------------------

/// Body is just a plain JSON string: `"1.2.3"`.
pub async fn set_dist_tag(
    State(state): State<AppState>,
    PublishUser(_user): PublishUser,
    Path((package, tag)): Path<(String, String)>,
    Json(version_str): Json<String>,
) -> Result<Json<Value>> {
    let pkg = resolve_package(&state, &package).await?;

    // Find the version the tag should point to.
    let ver = pkg
        .find_related(package_versions::Entity)
        .filter(package_versions::Column::Version.eq(&version_str))
        .filter(package_versions::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| {
            RegistryError::NotFound(format!(
                "version '{}' of package '{}' not found",
                version_str, package
            ))
        })?;

    // Upsert the dist-tag.
    let existing = dist_tags::Entity::find()
        .filter(dist_tags::Column::PackageId.eq(pkg.id))
        .filter(dist_tags::Column::Tag.eq(&tag))
        .one(&state.db)
        .await?;

    match existing {
        Some(row) => {
            let mut active: dist_tags::ActiveModel = row.into();
            active.version_id = Set(ver.id);
            active.updated_at = Set(chrono::Utc::now().fixed_offset());
            active.update(&state.db).await?;
        }
        None => {
            let now = chrono::Utc::now().fixed_offset();
            let new_tag = dist_tags::ActiveModel {
                id: Set(Uuid::new_v4()),
                package_id: Set(pkg.id),
                tag: Set(tag.clone()),
                version_id: Set(ver.id),
                created_at: Set(now),
                updated_at: Set(now),
            };
            new_tag.insert(&state.db).await?;
        }
    }

    Ok(Json(json!({ "ok": true, tag: version_str })))
}

// ---------------------------------------------------------------------------
// DELETE /-/package/{package}/dist-tags/{tag}
// ---------------------------------------------------------------------------

pub async fn delete_dist_tag(
    State(state): State<AppState>,
    PublishUser(_user): PublishUser,
    Path((package, tag)): Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>)> {
    // Refuse to remove the "latest" tag.
    if tag == "latest" {
        return Err(RegistryError::BadRequest(
            "cannot remove the 'latest' dist-tag".to_string(),
        ));
    }

    let pkg = resolve_package(&state, &package).await?;

    let existing = dist_tags::Entity::find()
        .filter(dist_tags::Column::PackageId.eq(pkg.id))
        .filter(dist_tags::Column::Tag.eq(&tag))
        .one(&state.db)
        .await?
        .ok_or_else(|| RegistryError::NotFound(format!("dist-tag '{}' not found", tag)))?;

    existing.delete(&state.db).await?;

    Ok((StatusCode::OK, Json(json!({ "ok": true }))))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn resolve_package(
    state: &AppState,
    package_name: &str,
) -> Result<packages::Model> {
    packages::Entity::find()
        .filter(packages::Column::Name.eq(package_name))
        .one(&state.db)
        .await?
        .ok_or_else(|| RegistryError::NotFound(format!("package '{}' not found", package_name)))
}
