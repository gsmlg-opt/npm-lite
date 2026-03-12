//! Cache warming: periodically re-fetches cached packuments that are
//! approaching their TTL to keep the cache fresh.
//!
//! When enabled via `UPSTREAM_CACHE_WARMUP=true`, a background task
//! periodically scans the `upstream_cache` table for entries that are
//! more than 80% through their TTL and refreshes them.

use npm_entity::upstream_cache;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder, QuerySelect,
};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Run one cache warming pass: refresh entries older than `stale_threshold`.
///
/// Returns the number of entries refreshed.
pub async fn warm_stale_entries(
    db: &DatabaseConnection,
    client: &crate::client::UpstreamClient,
    ttl: Duration,
) -> u64 {
    // Calculate the threshold: entries fetched before this time need warming.
    // We warm entries that are at least 80% of their TTL age.
    let threshold_secs = (ttl.as_secs() as f64 * 0.8) as i64;
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(threshold_secs);

    let entries: Vec<upstream_cache::Model> = match upstream_cache::Entity::find()
        .filter(upstream_cache::Column::FetchedAt.lt(cutoff))
        .order_by(upstream_cache::Column::FetchedAt, Order::Asc)
        .limit(50) // Warm at most 50 entries per pass.
        .all(db)
        .await
    {
        Ok(entries) => entries,
        Err(e) => {
            warn!(error = %e, "cache warmup: failed to query stale entries");
            return 0;
        }
    };

    if entries.is_empty() {
        debug!("cache warmup: no stale entries to refresh");
        return 0;
    }

    let mut refreshed = 0u64;

    for entry in &entries {
        let package_name = &entry.package_name;
        let upstream_url = &entry.upstream_url;

        match client
            .fetch_packument_from(package_name, upstream_url)
            .await
        {
            Ok(packument) => {
                crate::cache::put_cached_packument(db, package_name, upstream_url, &packument)
                    .await;
                refreshed += 1;
                debug!(package = %package_name, "cache warmup: refreshed");
            }
            Err(e) => {
                debug!(
                    package = %package_name,
                    error = %e,
                    "cache warmup: failed to refresh (will try again next pass)"
                );
            }
        }
    }

    if refreshed > 0 {
        info!(
            refreshed,
            total_stale = entries.len(),
            "cache warmup pass completed"
        );
    }

    refreshed
}

/// Spawn a background cache warming task.
///
/// The task runs every `interval` and refreshes stale cache entries.
pub fn spawn_warmup_task(
    db: DatabaseConnection,
    client: crate::client::UpstreamClient,
    ttl: Duration,
    interval: Duration,
) {
    tokio::spawn(async move {
        info!(
            interval_secs = interval.as_secs(),
            "cache warmup task started"
        );
        loop {
            tokio::time::sleep(interval).await;
            warm_stale_entries(&db, &client, ttl).await;
        }
    });
}
