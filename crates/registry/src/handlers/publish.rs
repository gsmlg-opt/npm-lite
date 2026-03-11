//! `PUT /{package}` – npm publish endpoint.
//!
//! The npm CLI serialises a publish operation as a JSON body with this shape:
//!
//! ```json
//! {
//!   "_id": "my-package",
//!   "name": "my-package",
//!   "description": "...",
//!   "dist-tags": { "latest": "1.0.0" },
//!   "versions": {
//!     "1.0.0": { /* full package.json */ }
//!   },
//!   "_attachments": {
//!     "my-package-1.0.0.tgz": {
//!       "content_type": "application/octet-stream",
//!       "data": "<base64-encoded tarball>",
//!       "length": 12345
//!     }
//!   }
//! }
//! ```

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use bytes::Bytes;
use npm_core::{integrity::compute_integrity, validation::validate_package_name};
use npm_db::AclRepo;
use npm_entity::{dist_tags, package_versions, packages, publish_events};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    TransactionTrait,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    auth::PublishUser,
    error::{RegistryError, Result},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request body types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PublishBody {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "dist-tags", default)]
    pub dist_tags: HashMap<String, String>,
    #[serde(default)]
    pub versions: HashMap<String, Value>,
    #[serde(rename = "_attachments", default)]
    pub attachments: HashMap<String, AttachmentEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AttachmentEntry {
    pub content_type: Option<String>,
    /// Base64-encoded tarball bytes.
    pub data: String,
    pub length: Option<u64>,
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// `PUT /{package}` – publish a new package version.
pub async fn publish_package(
    State(state): State<AppState>,
    PublishUser(user): PublishUser,
    Path(package): Path<String>,
    Json(body): Json<PublishBody>,
) -> Result<(StatusCode, Json<Value>)> {
    do_publish(state, user.user_id, package, body).await
}

/// `PUT /@{scope}/{name}` – publish a scoped package.
pub async fn publish_scoped_package(
    State(state): State<AppState>,
    PublishUser(user): PublishUser,
    Path((scope, name)): Path<(String, String)>,
    Json(body): Json<PublishBody>,
) -> Result<(StatusCode, Json<Value>)> {
    let full_name = format!("@{}/{}", scope, name);
    do_publish(state, user.user_id, full_name, body).await
}

// ---------------------------------------------------------------------------
// Core publish logic
// ---------------------------------------------------------------------------

async fn do_publish(
    state: AppState,
    actor_id: Uuid,
    package_name: String,
    body: PublishBody,
) -> Result<(StatusCode, Json<Value>)> {
    // 1. Validate the package name.
    validate_package_name(&package_name)
        .map_err(|e| RegistryError::BadRequest(e.to_string()))?;

    // 2. Ensure the name in the path matches the name in the body.
    if body.name != package_name {
        return Err(RegistryError::BadRequest(format!(
            "body 'name' ('{}') does not match path package name ('{}')",
            body.name, package_name
        )));
    }

    // 3. Exactly one version must be published at a time.
    if body.versions.len() != 1 {
        return Err(RegistryError::BadRequest(
            "publish body must contain exactly one version".to_string(),
        ));
    }

    // 4. Exactly one attachment is expected.
    if body.attachments.len() != 1 {
        return Err(RegistryError::BadRequest(
            "publish body must contain exactly one attachment".to_string(),
        ));
    }

    // 5. ACL check: if the package already exists and has ACL entries, verify
    //    the publishing user holds at least "publish" permission.  If there are
    //    NO ACL entries at all we allow the operation (permissive default for
    //    new packages or packages that have not yet been access-controlled).
    {
        use npm_entity::package_acl::Column as AclCol;
        use npm_entity::package_acl::Entity as AclEntity;

        let pkg_row = packages::Entity::find()
            .filter(packages::Column::Name.eq(&package_name))
            .one(&state.db)
            .await?;

        if let Some(ref pkg) = pkg_row {
            // Package already exists – check if any ACL entries are configured.
            let acl_count = AclEntity::find()
                .filter(AclCol::PackageId.eq(pkg.id))
                .count(&state.db)
                .await
                .map_err(|e| RegistryError::Internal(e.to_string()))?;

            if acl_count > 0 {
                // ACL entries exist – enforce permission.
                let allowed = AclRepo::check_permission(
                    &state.db,
                    actor_id,
                    &package_name,
                    "publish",
                )
                .await
                .map_err(|e| RegistryError::Internal(e.to_string()))?;

                if !allowed {
                    return Err(RegistryError::Forbidden(format!(
                        "user does not have publish permission on package '{}'",
                        package_name
                    )));
                }
            }
            // If acl_count == 0: no entries configured – allow (permissive default).
        }
        // If pkg_row is None: new package – allow.
    }

    // Destructure body into owned parts to avoid borrow issues.
    let PublishBody {
        description,
        versions,
        attachments,
        dist_tags,
        ..
    } = body;

    let (version_str, version_meta) = versions.into_iter().next().unwrap();
    let (_attachment_name, attachment) = attachments.into_iter().next().unwrap();

    // 6. Decode the tarball.
    let tarball_bytes: Bytes = STANDARD
        .decode(&attachment.data)
        .map(Bytes::from)
        .map_err(|e| RegistryError::BadRequest(format!("attachment data is not valid base64: {}", e)))?;

    // 7. Compute integrity hashes.
    let hashes = compute_integrity(&tarball_bytes);

    // 8. Determine S3 key.
    let s3_key = build_s3_key(&package_name, &version_str);

    // 9. Upload tarball to S3.
    state
        .storage
        .upload(
            &s3_key,
            tarball_bytes.clone(),
            "application/octet-stream",
        )
        .await
        .map_err(RegistryError::Storage)?;

    // 10. Persist to DB in a transaction. On failure, attempt a compensating S3
    //    delete so the bucket doesn't accumulate orphaned blobs.
    let db_result = persist_publish(
        &state,
        &actor_id,
        &package_name,
        &description,
        &version_str,
        &version_meta,
        &s3_key,
        &hashes,
        tarball_bytes.len() as i64,
        &dist_tags,
    )
    .await;

    if let Err(ref e) = db_result {
        tracing::error!(
            error = %e,
            package = %package_name,
            version = %version_str,
            s3_key = %s3_key,
            "DB publish failed – attempting compensating S3 delete",
        );
        if let Err(del_err) = state.storage.delete(&s3_key).await {
            tracing::error!(
                error = %del_err,
                s3_key = %s3_key,
                "compensating S3 delete also failed – manual cleanup required",
            );
        }
    }

    db_result?;

    // 11. Return npm-compatible success response.
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "ok": true,
            "id": package_name,
            "rev": format!("1-{}", Uuid::new_v4()),
        })),
    ))
}

// ---------------------------------------------------------------------------
// DB transaction
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn persist_publish(
    state: &AppState,
    actor_id: &Uuid,
    package_name: &str,
    description: &Option<String>,
    version_str: &str,
    version_meta: &Value,
    s3_key: &str,
    hashes: &npm_core::integrity::IntegrityHashes,
    size: i64,
    dist_tags_map: &HashMap<String, String>,
) -> Result<()> {
    let txn = state.db.begin().await?;

    // Upsert the package record.
    let pkg = packages::Entity::find()
        .filter(packages::Column::Name.eq(package_name))
        .one(&txn)
        .await?;

    let (pkg_id, is_new_pkg) = match pkg {
        Some(existing) => {
            // Update description if provided.
            if description.is_some() {
                let mut active: packages::ActiveModel = existing.clone().into();
                active.description = Set(description.clone());
                active.updated_at = Set(chrono::Utc::now().fixed_offset());
                active.update(&txn).await?;
            }
            (existing.id, false)
        }
        None => {
            // Determine scope from name.
            let scope = if let Some(rest) = package_name.strip_prefix('@') {
                rest.find('/').map(|i| rest[..i].to_string())
            } else {
                None
            };

            let now = chrono::Utc::now().fixed_offset();
            let new_pkg = packages::ActiveModel {
                id: Set(Uuid::new_v4()),
                name: Set(package_name.to_string()),
                scope: Set(scope),
                description: Set(description.clone()),
                created_at: Set(now),
                updated_at: Set(now),
            };
            let inserted = new_pkg.insert(&txn).await?;

            // Grant the publishing user admin permission on the new package.
            let acl_entry = npm_entity::package_acl::ActiveModel {
                id: Set(Uuid::new_v4()),
                package_id: Set(Some(inserted.id)),
                scope: Set(None),
                user_id: Set(Some(*actor_id)),
                team_id: Set(None),
                permission: Set("admin".to_string()),
                created_at: Set(now),
            };
            acl_entry.insert(&txn).await?;

            (inserted.id, true)
        }
    };

    // Check for duplicate version.
    let existing_version = package_versions::Entity::find()
        .filter(package_versions::Column::PackageId.eq(pkg_id))
        .filter(package_versions::Column::Version.eq(version_str))
        .one(&txn)
        .await?;

    if existing_version.is_some() {
        txn.rollback().await.ok();
        return Err(RegistryError::Conflict(format!(
            "version '{}' of package '{}' already exists",
            version_str, package_name
        )));
    }

    // Insert the package version.
    let version_id = Uuid::new_v4();
    let new_ver = package_versions::ActiveModel {
        id: Set(version_id),
        package_id: Set(pkg_id),
        version: Set(version_str.to_string()),
        s3_key: Set(s3_key.to_string()),
        sha512: Set(hashes.sha512.clone()),
        shasum: Set(hashes.shasum.clone()),
        integrity: Set(hashes.integrity.clone()),
        size: Set(size),
        metadata: Set(version_meta.clone()),
        deleted_at: Set(None),
        created_at: Set(chrono::Utc::now().fixed_offset()),
    };
    new_ver.insert(&txn).await?;

    // Upsert dist-tags.
    for (tag, tag_version_str) in dist_tags_map {
        // Find the version_id for the tagged version (might be the one we just inserted).
        let tag_version_id = if tag_version_str == version_str {
            version_id
        } else {
            let existing_tagged_ver = package_versions::Entity::find()
                .filter(package_versions::Column::PackageId.eq(pkg_id))
                .filter(package_versions::Column::Version.eq(tag_version_str))
                .filter(package_versions::Column::DeletedAt.is_null())
                .one(&txn)
                .await?
                .ok_or_else(|| {
                    RegistryError::BadRequest(format!(
                        "dist-tag '{}' points to unknown version '{}'",
                        tag, tag_version_str
                    ))
                })?;
            existing_tagged_ver.id
        };

        let now = chrono::Utc::now().fixed_offset();

        // Check if the tag already exists for this package.
        let existing_tag = dist_tags::Entity::find()
            .filter(dist_tags::Column::PackageId.eq(pkg_id))
            .filter(dist_tags::Column::Tag.eq(tag.as_str()))
            .one(&txn)
            .await?;

        match existing_tag {
            Some(tag_row) => {
                let mut active: dist_tags::ActiveModel = tag_row.into();
                active.version_id = Set(tag_version_id);
                active.updated_at = Set(now);
                active.update(&txn).await?;
            }
            None => {
                let new_tag = dist_tags::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    package_id: Set(pkg_id),
                    tag: Set(tag.clone()),
                    version_id: Set(tag_version_id),
                    created_at: Set(now),
                    updated_at: Set(now),
                };
                new_tag.insert(&txn).await?;
            }
        }
    }

    // Record publish event.
    let event = publish_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        package_id: Set(pkg_id),
        version_id: Set(Some(version_id)),
        action: Set("publish".to_string()),
        actor_id: Set(*actor_id),
        success: Set(true),
        error_message: Set(None),
        created_at: Set(chrono::Utc::now().fixed_offset()),
    };
    event.insert(&txn).await?;

    txn.commit().await?;

    tracing::info!(
        package = %package_name,
        version = %version_str,
        is_new_pkg,
        "package published successfully",
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the S3 object key for a tarball.
///
/// Follows the convention: `packages/{name}/{name}-{version}.tgz`
/// For scoped packages: `packages/@scope/name/@scope/name-{version}.tgz`
fn build_s3_key(package_name: &str, version: &str) -> String {
    let bare = if let Some(rest) = package_name.strip_prefix('@') {
        let slash = rest.find('/').unwrap_or(rest.len());
        rest[slash + 1..].to_string()
    } else {
        package_name.to_string()
    };
    format!("packages/{}/{}-{}.tgz", package_name, bare, version)
}
