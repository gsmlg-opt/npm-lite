//! Metadata and tarball caching for upstream packages.
//!
//! - Metadata cache: stored in the `upstream_cache` DB table with TTL.
//! - Tarball cache: stored in S3 with `upstream/` key prefix.

use chrono::Utc;
use npm_entity::upstream_cache;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{debug, warn};
use uuid::Uuid;

/// Global cache statistics counters.
static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static CACHE_STALE_HITS: AtomicU64 = AtomicU64::new(0);

/// Cache statistics snapshot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub stale_hits: u64,
}

/// Get current cache statistics.
pub fn cache_stats() -> CacheStats {
    CacheStats {
        hits: CACHE_HITS.load(Ordering::Relaxed),
        misses: CACHE_MISSES.load(Ordering::Relaxed),
        stale_hits: CACHE_STALE_HITS.load(Ordering::Relaxed),
    }
}

/// Read a cached packument from the database.
///
/// Returns `Some(packument_json)` if the cache entry exists and is fresh
/// (within `ttl`). Returns the stale entry if `allow_stale` is true and
/// the entry exists but is expired.
pub async fn get_cached_packument(
    db: &DatabaseConnection,
    package_name: &str,
    ttl: Duration,
    allow_stale: bool,
) -> Option<Value> {
    let entry = upstream_cache::Entity::find()
        .filter(upstream_cache::Column::PackageName.eq(package_name))
        .one(db)
        .await
        .ok()
        .flatten()?;

    let age = Utc::now().signed_duration_since(entry.fetched_at.with_timezone(&Utc));
    let ttl_chrono = chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::seconds(300));

    if age <= ttl_chrono {
        CACHE_HITS.fetch_add(1, Ordering::Relaxed);
        debug!(package = %package_name, "upstream cache hit (fresh)");
        Some(entry.packument_json)
    } else if allow_stale {
        CACHE_STALE_HITS.fetch_add(1, Ordering::Relaxed);
        debug!(package = %package_name, age_secs = age.num_seconds(), "upstream cache hit (stale, serving anyway)");
        Some(entry.packument_json)
    } else {
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
        debug!(package = %package_name, age_secs = age.num_seconds(), "upstream cache expired");
        None
    }
}

/// Store or update a cached packument in the database.
pub async fn put_cached_packument(
    db: &DatabaseConnection,
    package_name: &str,
    upstream_url: &str,
    packument: &Value,
) {
    let now = Utc::now().into();

    // Check if entry exists.
    let existing = upstream_cache::Entity::find()
        .filter(upstream_cache::Column::PackageName.eq(package_name))
        .one(db)
        .await;

    match existing {
        Ok(Some(entry)) => {
            // Update existing entry.
            let mut active: upstream_cache::ActiveModel = entry.into();
            active.packument_json = Set(packument.clone());
            active.upstream_url = Set(upstream_url.to_string());
            active.fetched_at = Set(now);
            if let Err(e) = active.update(db).await {
                warn!(package = %package_name, error = %e, "failed to update upstream cache");
            }
        }
        Ok(None) => {
            // Insert new entry.
            let entry = upstream_cache::ActiveModel {
                id: Set(Uuid::new_v4()),
                package_name: Set(package_name.to_string()),
                upstream_url: Set(upstream_url.to_string()),
                packument_json: Set(packument.clone()),
                fetched_at: Set(now),
                created_at: Set(now),
            };
            if let Err(e) = entry.insert(db).await {
                warn!(package = %package_name, error = %e, "failed to insert upstream cache");
            }
        }
        Err(e) => {
            warn!(package = %package_name, error = %e, "failed to query upstream cache");
        }
    }
}

/// Delete a cached packument from the database.
pub async fn delete_cached_packument(
    db: &DatabaseConnection,
    package_name: &str,
) -> Result<bool, sea_orm::DbErr> {
    let result = upstream_cache::Entity::delete_many()
        .filter(upstream_cache::Column::PackageName.eq(package_name))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

/// Delete all cached packuments from the database.
pub async fn delete_all_cached_packuments(db: &DatabaseConnection) -> Result<u64, sea_orm::DbErr> {
    let result = upstream_cache::Entity::delete_many().exec(db).await?;
    Ok(result.rows_affected)
}

/// Count the number of cached packuments.
pub async fn count_cached_packuments(db: &DatabaseConnection) -> Result<u64, sea_orm::DbErr> {
    upstream_cache::Entity::find().count(db).await
}

/// Evict the oldest cached packuments to keep the cache within `max_entries`.
///
/// Deletes entries by oldest `fetched_at` first. Returns the number of entries evicted.
pub async fn evict_oldest_cached_packuments(
    db: &DatabaseConnection,
    max_entries: u64,
) -> Result<u64, sea_orm::DbErr> {
    let current_count = count_cached_packuments(db).await?;
    if current_count <= max_entries {
        return Ok(0);
    }

    let to_evict = current_count - max_entries;

    // Find the oldest entries to delete.
    let oldest = upstream_cache::Entity::find()
        .order_by(upstream_cache::Column::FetchedAt, Order::Asc)
        .limit(to_evict)
        .all(db)
        .await?;

    let mut evicted = 0u64;
    for entry in oldest {
        if upstream_cache::Entity::delete_by_id(entry.id)
            .exec(db)
            .await
            .is_ok()
        {
            evicted += 1;
        }
    }

    if evicted > 0 {
        debug!(evicted, max_entries, "evicted oldest cached packuments");
    }

    Ok(evicted)
}

/// Build the S3 key for a cached upstream tarball.
pub fn upstream_tarball_s3_key(package_name: &str, version: &str) -> String {
    format!("upstream/{}/{}.tgz", package_name, version)
}
