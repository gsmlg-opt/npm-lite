use std::collections::HashSet;

use chrono::{Duration, Utc};
use tracing::{debug, info, instrument, warn};

use crate::{
    error::{Result, StorageError},
    S3Storage,
};

/// Summary of a single garbage-collection run.
#[derive(Debug, Default)]
pub struct GcReport {
    /// Keys that were identified as orphans and eligible for deletion.
    pub orphan_keys: Vec<String>,
    /// Number of orphans successfully deleted.
    pub deleted_count: usize,
    /// Keys that could not be deleted, along with the associated error message.
    pub errors: Vec<(String, String)>,
}

/// Garbage-collects S3 blobs that are no longer referenced by the database.
///
/// An object is considered an orphan when:
/// 1. Its key is **not** present in `known_keys` (i.e. the registry has no
///    record of it), **and**
/// 2. It was last modified more than `threshold` ago (to avoid deleting blobs
///    that were just uploaded but whose database record hasn't been committed
///    yet).
pub struct OrphanGc<'a> {
    storage: &'a S3Storage,
    /// How old an unreferenced object must be before it is deleted.
    threshold: Duration,
    /// Optional S3 key prefix to restrict the scan (e.g. `"tarballs/"`).
    prefix: Option<String>,
}

impl<'a> OrphanGc<'a> {
    /// Create a new `OrphanGc` with the default 24-hour threshold.
    pub fn new(storage: &'a S3Storage) -> Self {
        Self {
            storage,
            threshold: Duration::hours(24),
            prefix: None,
        }
    }

    /// Override the age threshold.
    pub fn with_threshold(mut self, threshold: Duration) -> Self {
        self.threshold = threshold;
        self
    }

    /// Restrict the S3 scan to keys that start with `prefix`.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// List all S3 objects that are not in `known_keys` and are older than the
    /// configured threshold.
    ///
    /// This does **not** delete anything; use [`cleanup_orphans`](Self::cleanup_orphans)
    /// for that.
    #[instrument(skip(self, known_keys), fields(threshold_hours = self.threshold.num_hours(), prefix = ?self.prefix))]
    pub async fn find_orphans(&self, known_keys: &HashSet<String>) -> Result<Vec<String>> {
        let cutoff = Utc::now() - self.threshold;
        debug!(%cutoff, "scanning S3 for orphan blobs");

        let objects = self
            .storage
            .list_objects(self.prefix.as_deref())
            .await?;

        let orphans: Vec<String> = objects
            .into_iter()
            .filter(|obj| {
                if known_keys.contains(&obj.key) {
                    return false;
                }
                if obj.last_modified >= cutoff {
                    debug!(key = %obj.key, last_modified = %obj.last_modified, "skipping recently-uploaded orphan candidate");
                    return false;
                }
                true
            })
            .map(|obj| obj.key)
            .collect();

        info!(count = orphans.len(), "orphan blobs found");
        Ok(orphans)
    }

    /// Delete all orphan blobs and return a report summarising the run.
    ///
    /// Deletion errors are captured in [`GcReport::errors`] rather than
    /// aborting the entire run so that a single unreadable object cannot block
    /// the rest of the cleanup.
    #[instrument(skip(self, known_keys))]
    pub async fn cleanup_orphans(&self, known_keys: &HashSet<String>) -> Result<GcReport> {
        let orphan_keys = self.find_orphans(known_keys).await?;

        let mut report = GcReport {
            orphan_keys: orphan_keys.clone(),
            ..Default::default()
        };

        for key in &orphan_keys {
            match self.storage.delete(key).await {
                Ok(()) => {
                    debug!(%key, "deleted orphan blob");
                    report.deleted_count += 1;
                }
                Err(StorageError::DeleteFailed { key: k, source }) => {
                    warn!(%k, error = %source, "failed to delete orphan blob");
                    report.errors.push((k, source.to_string()));
                }
                Err(e) => {
                    warn!(%key, error = %e, "unexpected error deleting orphan blob");
                    report.errors.push((key.clone(), e.to_string()));
                }
            }
        }

        info!(
            deleted = report.deleted_count,
            errors = report.errors.len(),
            "garbage collection complete"
        );

        Ok(report)
    }
}
