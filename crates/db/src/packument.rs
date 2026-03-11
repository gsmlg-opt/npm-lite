//! Packument reconstruction from database records.
//!
//! An npm packument is the full metadata document returned by the registry
//! for a given package name (e.g. `GET /@scope/pkg`).  It contains every
//! version's `package.json`, a `dist-tags` map, and top-level summary fields.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::{json, Value};

use npm_entity::{
    package_versions::{Column as VersionCol, Entity as VersionEntity},
    packages::{Column as PkgCol, Entity as PkgEntity},
};

use crate::error::{DbError, Result};

/// Construct the fully-qualified tarball URL for `package_name` at `version`.
fn build_tarball_url(registry_url: &str, package_name: &str, version: &str) -> String {
    let base = registry_url.trim_end_matches('/');
    if let Some(rest) = package_name.strip_prefix('@') {
        // Scoped: @scope/name  →  {base}/@scope/name/-/name-{version}.tgz
        let slash = rest.find('/').unwrap_or(rest.len());
        let name = &rest[slash + 1..];
        let scope = &rest[..slash];
        format!("{}/@{}/{}/-/{}-{}.tgz", base, scope, name, name, version)
    } else {
        // Plain: {base}/{name}/-/{name}-{version}.tgz
        format!("{}/{}/-/{}-{}.tgz", base, package_name, package_name, version)
    }
}

/// Build the npm packument JSON for `package_name`.
///
/// `registry_url` is the base URL of the registry (e.g. `https://registry.example.com`)
/// used to construct fully-qualified tarball download URLs.
///
/// Returns `DbError::NotFound` when the package does not exist in the database.
pub async fn build_packument(
    db: &DatabaseConnection,
    package_name: &str,
    registry_url: &str,
) -> Result<Value> {
    // 1. Look up the package row.
    let package = PkgEntity::find()
        .filter(PkgCol::Name.eq(package_name))
        .one(db)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("package '{}' not found", package_name)))?;

    // 2. Load all non-deleted versions.
    let versions = VersionEntity::find()
        .filter(VersionCol::PackageId.eq(package.id))
        .filter(VersionCol::DeletedAt.is_null())
        .all(db)
        .await?;

    // 3. Load dist-tags for this package.
    use npm_entity::dist_tags::{Column as TagCol, Entity as TagEntity};
    let dist_tag_rows = TagEntity::find()
        .filter(TagCol::PackageId.eq(package.id))
        .all(db)
        .await?;

    // Build a version-id → version-string lookup so we can resolve tag refs.
    let id_to_version: std::collections::HashMap<uuid::Uuid, String> = versions
        .iter()
        .map(|v| (v.id, v.version.clone()))
        .collect();

    // 4. Build the "versions" map: { "1.0.0": { ...package.json with dist... } }
    let mut versions_map = serde_json::Map::new();
    let mut times_map = serde_json::Map::new();
    let mut latest_version: Option<String> = None;
    let mut latest_created: Option<chrono::DateTime<chrono::FixedOffset>> = None;

    for ver in &versions {
        // The stored metadata is the full package.json object.
        let mut ver_obj = match &ver.metadata {
            Value::Object(o) => o.clone(),
            other => {
                // Wrap non-object metadata – shouldn't normally happen.
                let mut m = serde_json::Map::new();
                m.insert("_raw".to_string(), other.clone());
                m
            }
        };

        // Ensure "name" and "version" are set.
        ver_obj
            .entry("name")
            .or_insert_with(|| Value::String(package_name.to_string()));
        ver_obj
            .entry("version")
            .or_insert_with(|| Value::String(ver.version.clone()));

        // Inject the `dist` block with tarball information.
        let tarball_url = build_tarball_url(registry_url, package_name, &ver.version);
        let dist = json!({
            "shasum": ver.shasum,
            "integrity": ver.integrity,
            "tarball": tarball_url,
        });
        ver_obj.insert("dist".to_string(), dist);

        times_map.insert(
            ver.version.clone(),
            Value::String(ver.created_at.to_rfc3339()),
        );

        // Track which version is the most recently published (to fall back for
        // "latest" if no explicit dist-tag exists).
        if latest_created.map_or(true, |prev| ver.created_at > prev) {
            latest_version = Some(ver.version.clone());
            latest_created = Some(ver.created_at);
        }

        versions_map.insert(ver.version.clone(), Value::Object(ver_obj));
    }

    // 5. Build dist-tags map.
    let mut dist_tags_map = serde_json::Map::new();
    for tag in &dist_tag_rows {
        if let Some(version_str) = id_to_version.get(&tag.version_id) {
            dist_tags_map.insert(tag.tag.clone(), Value::String(version_str.clone()));
        }
    }
    // Ensure "latest" is always present.
    if !dist_tags_map.contains_key("latest") {
        if let Some(v) = latest_version {
            dist_tags_map.insert("latest".to_string(), Value::String(v));
        }
    }

    // 6. Assemble the packument.
    let packument = json!({
        "_id": package_name,
        "name": package_name,
        "description": package.description,
        "dist-tags": Value::Object(dist_tags_map),
        "versions": Value::Object(versions_map),
        "time": Value::Object(times_map),
        // Top-level fields npm clients expect.
        "repository": null,
        "homepage": null,
        "bugs": null,
        "license": null,
        "readme": "",
        "readmeFilename": "",
    });

    Ok(packument)
}
