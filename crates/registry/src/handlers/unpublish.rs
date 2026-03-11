//! `DELETE /-/admin/package/{package}/{version}` – admin-only soft-delete of a
//! package version.
//!
//! This endpoint:
//! - Sets `deleted_at` on the `package_versions` row.
//! - Records an `unpublish` event in `publish_events`.
//! - Does **not** delete the underlying S3 blob (blobs are cleaned up by the
//!   garbage-collection job).
//!
//! Only users with the `admin` role may call this endpoint.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use npm_entity::{package_versions, packages, publish_events};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, ModelTrait, QueryFilter,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth::AdminUser,
    error::{RegistryError, Result},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `DELETE /-/admin/package/{package}/{version}` – soft-delete a version.
///
/// Admin-only. Sets `deleted_at` on the version row and records a publish
/// event; does not remove the S3 blob.
pub async fn unpublish_version(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path((package, version)): Path<(String, String)>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    // Resolve the package.
    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(&package))
        .one(&state.db)
        .await?
        .ok_or_else(|| RegistryError::NotFound(format!("package '{}' not found", package)))?;

    // Resolve the version (must not already be deleted).
    let ver = pkg
        .find_related(package_versions::Entity)
        .filter(package_versions::Column::Version.eq(&version))
        .filter(package_versions::Column::DeletedAt.is_null())
        .one(&state.db)
        .await?
        .ok_or_else(|| {
            RegistryError::NotFound(format!(
                "version '{}' of package '{}' not found or already unpublished",
                version, package
            ))
        })?;

    let version_id = ver.id;

    // Soft-delete: set deleted_at.
    let now = chrono::Utc::now().fixed_offset();
    let mut active: package_versions::ActiveModel = ver.into();
    active.deleted_at = Set(Some(now));
    active.update(&state.db).await?;

    // Record the unpublish event.
    let event = publish_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        package_id: Set(pkg.id),
        version_id: Set(Some(version_id)),
        action: Set("unpublish".to_string()),
        actor_id: Set(admin.user_id),
        success: Set(true),
        error_message: Set(None),
        created_at: Set(now),
    };
    event.insert(&state.db).await?;

    tracing::info!(
        package = %package,
        version = %version,
        admin = %admin.username,
        "version soft-deleted (unpublished)",
    );

    Ok((
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "id": format!("{}@{}", package, version),
        })),
    ))
}

/// `DELETE /-/admin/package/{package}` – soft-delete all versions of a package.
pub async fn unpublish_package(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(package): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(&package))
        .one(&state.db)
        .await?
        .ok_or_else(|| RegistryError::NotFound(format!("package '{}' not found", package)))?;

    // Fetch all non-deleted versions.
    let versions: Vec<package_versions::Model> = pkg
        .find_related(package_versions::Entity)
        .filter(package_versions::Column::DeletedAt.is_null())
        .all(&state.db)
        .await?;

    if versions.is_empty() {
        return Err(RegistryError::NotFound(format!(
            "package '{}' has no active versions",
            package
        )));
    }

    let now = chrono::Utc::now().fixed_offset();
    let mut unpublished_count = 0u32;

    for ver in versions {
        let version_id = ver.id;
        let version_str = ver.version.clone();

        // Soft-delete.
        let mut active: package_versions::ActiveModel = ver.into();
        active.deleted_at = Set(Some(now));
        active.update(&state.db).await?;

        // Record event.
        let event = publish_events::ActiveModel {
            id: Set(Uuid::new_v4()),
            package_id: Set(pkg.id),
            version_id: Set(Some(version_id)),
            action: Set("unpublish".to_string()),
            actor_id: Set(admin.user_id),
            success: Set(true),
            error_message: Set(None),
            created_at: Set(now),
        };
        event.insert(&state.db).await?;

        tracing::info!(
            package = %package,
            version = %version_str,
            admin = %admin.username,
            "version soft-deleted as part of full package unpublish",
        );

        unpublished_count += 1;
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "id": package,
            "versions_removed": unpublished_count,
        })),
    ))
}
