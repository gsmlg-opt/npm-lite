//! Publish transaction: atomically creates or updates a package, version, dist-tags,
//! and records a publish event – all inside a single database transaction.

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    TransactionTrait,
};
use serde_json::Value;
use uuid::Uuid;

use npm_entity::{
    dist_tags, package_acl,
    package_versions::{self, Column as VersionCol, Entity as VersionEntity},
    packages::{self, Column as PkgCol, Entity as PkgEntity},
    publish_events,
};

use crate::error::{DbError, Result};

/// The outcome of a successful publish operation.
#[derive(Debug, Clone)]
pub struct PublishResult {
    /// Database ID of the package record (new or existing).
    pub package_id: Uuid,
    /// Database ID of the newly created version record.
    pub version_id: Uuid,
}

/// Execute a publish inside a single database transaction.
///
/// Steps performed atomically:
/// 1. Reject if the version already exists (even if soft-deleted – npm forbids re-publishing).
/// 2. Create the package row if it does not exist yet.
/// 3. Create the version row.
/// 4. Upsert the dist-tag (e.g. "latest") so it points to the new version.
/// 5. Record a success event in `publish_events`.
///
/// On failure the transaction is rolled back and a failure event is **not** recorded
/// (the caller should record it separately if desired).
#[allow(clippy::too_many_arguments)]
pub async fn execute_publish(
    db: &DatabaseConnection,
    package_name: &str,
    version: &str,
    s3_key: impl Into<String>,
    sha512: Vec<u8>,
    shasum: impl Into<String>,
    integrity: impl Into<String>,
    size: i64,
    metadata: Value,
    dist_tag: &str,
    actor_id: Uuid,
) -> Result<PublishResult> {
    let package_name_log = package_name.to_string();
    let version_log = version.to_string();
    let package_name = package_name_log.clone();
    let version = version_log.clone();
    let s3_key = s3_key.into();
    let shasum = shasum.into();
    let integrity = integrity.into();
    let dist_tag = dist_tag.to_string();

    let result = db
        .transaction::<_, PublishResult, DbError>(|txn| {
            Box::pin(async move {
                // --- Step 1: check version uniqueness -----------------------
                // First resolve (or will create) the package to know its ID.
                let existing_pkg = PkgEntity::find()
                    .filter(PkgCol::Name.eq(&package_name))
                    .one(txn)
                    .await?;

                if let Some(ref pkg) = existing_pkg {
                    // Check for any version row with this version string,
                    // including soft-deleted ones (npm policy: once published,
                    // a version string can never be reused).
                    let existing_ver = VersionEntity::find()
                        .filter(VersionCol::PackageId.eq(pkg.id))
                        .filter(VersionCol::Version.eq(&version))
                        .one(txn)
                        .await?;

                    if existing_ver.is_some() {
                        return Err(DbError::Conflict(format!(
                            "version '{}' of package '{}' already exists",
                            version, package_name
                        )));
                    }
                }

                // --- Step 2: get-or-create the package ----------------------
                let now = chrono::Utc::now().fixed_offset();

                let package: packages::Model = match existing_pkg {
                    Some(p) => p,
                    None => {
                        // Derive scope from the name (e.g. "@myorg/pkg" → "myorg").
                        let scope = if package_name.starts_with('@') {
                            package_name
                                .split('/')
                                .next()
                                .map(|s| s.trim_start_matches('@').to_string())
                        } else {
                            None
                        };

                        let active = packages::ActiveModel {
                            id: Set(Uuid::new_v4()),
                            name: Set(package_name.clone()),
                            scope: Set(scope),
                            description: Set(metadata
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_string)),
                            created_at: Set(now),
                            updated_at: Set(now),
                        };
                        let inserted_pkg = active.insert(txn).await?;

                        // Grant the publishing user admin permission on the new package.
                        let acl_entry = package_acl::ActiveModel {
                            id: Set(Uuid::new_v4()),
                            package_id: Set(Some(inserted_pkg.id)),
                            scope: Set(None),
                            user_id: Set(Some(actor_id)),
                            team_id: Set(None),
                            permission: Set("admin".to_string()),
                            created_at: Set(now),
                        };
                        acl_entry.insert(txn).await?;

                        inserted_pkg
                    }
                };

                // --- Step 3: create the version row -------------------------
                let version_id = Uuid::new_v4();
                let ver_active = package_versions::ActiveModel {
                    id: Set(version_id),
                    package_id: Set(package.id),
                    version: Set(version.clone()),
                    s3_key: Set(s3_key),
                    sha512: Set(sha512),
                    shasum: Set(shasum),
                    integrity: Set(integrity),
                    size: Set(size),
                    metadata: Set(metadata),
                    deleted_at: Set(None),
                    created_at: Set(now),
                };
                ver_active.insert(txn).await?;

                // --- Step 4: upsert the dist-tag ----------------------------
                use npm_entity::dist_tags::{Column as TagCol, Entity as TagEntity};

                let existing_tag = TagEntity::find()
                    .filter(TagCol::PackageId.eq(package.id))
                    .filter(TagCol::Tag.eq(&dist_tag))
                    .one(txn)
                    .await?;

                match existing_tag {
                    Some(tag) => {
                        let mut active: dist_tags::ActiveModel = tag.into();
                        active.version_id = Set(version_id);
                        active.updated_at = Set(now);
                        active.update(txn).await?;
                    }
                    None => {
                        let active = dist_tags::ActiveModel {
                            id: Set(Uuid::new_v4()),
                            package_id: Set(package.id),
                            tag: Set(dist_tag.clone()),
                            version_id: Set(version_id),
                            created_at: Set(now),
                            updated_at: Set(now),
                        };
                        active.insert(txn).await?;
                    }
                }

                // --- Step 5: record publish event ---------------------------
                let event_active = publish_events::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    package_id: Set(package.id),
                    version_id: Set(Some(version_id)),
                    action: Set("publish".to_string()),
                    actor_id: Set(actor_id),
                    success: Set(true),
                    error_message: Set(None),
                    created_at: Set(now),
                };
                event_active.insert(txn).await?;

                Ok(PublishResult {
                    package_id: package.id,
                    version_id,
                })
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Transaction(db_err) => db_err,
            sea_orm::TransactionError::Connection(db_err) => DbError::SeaOrm(db_err),
        })?;

    tracing::info!(
        package = %package_name_log,
        version = %version_log,
        package_id = %result.package_id,
        version_id = %result.version_id,
        "publish transaction committed",
    );

    Ok(result)
}
