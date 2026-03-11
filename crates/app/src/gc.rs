use std::{collections::HashSet, time::Duration};

use sea_orm::EntityTrait;
use tracing::{error, info, instrument, warn};

use npm_entity::package_versions;
use npm_registry::AppState;
use npm_storage::OrphanGc;

/// Spawn a background task that periodically garbage-collects orphaned S3 blobs.
///
/// Returns immediately; the task runs until the process exits.
///
/// If `interval_secs` is 0 this function is a no-op.
pub fn spawn_gc_task(state: AppState, interval_secs: u64) {
    if interval_secs == 0 {
        info!("GC disabled (GC_INTERVAL_SECS=0)");
        return;
    }

    tokio::spawn(async move {
        let interval = Duration::from_secs(interval_secs);
        info!(?interval, "background GC task started");

        loop {
            tokio::time::sleep(interval).await;
            run_gc_cycle(&state).await;
        }
    });
}

/// Execute a single GC cycle: collect all known S3 keys from the DB and delete
/// any S3 blobs that are no longer referenced.
#[instrument(skip(state), name = "gc_cycle")]
async fn run_gc_cycle(state: &AppState) {
    info!("GC cycle starting");

    // Collect every s3_key that is currently referenced by a package version
    // (including soft-deleted ones, to avoid racing with an in-progress delete).
    let versions = match package_versions::Entity::find().all(&state.db).await {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "GC: failed to query package versions");
            return;
        }
    };

    let known_keys: HashSet<String> = versions.into_iter().map(|v| v.s3_key).collect();

    info!(known_keys = known_keys.len(), "GC: collected known S3 keys");

    let gc = OrphanGc::new(&state.storage).with_prefix("packages/");

    match gc.cleanup_orphans(&known_keys).await {
        Ok(report) => {
            info!(
                orphans = report.orphan_keys.len(),
                deleted = report.deleted_count,
                errors = report.errors.len(),
                "GC cycle complete"
            );
            for (key, err) in &report.errors {
                warn!(key = %key, error = %err, "GC: failed to delete orphan blob");
            }
        }
        Err(e) => {
            error!(error = %e, "GC cycle failed");
        }
    }
}
