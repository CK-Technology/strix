//! Background cleanup tasks for storage maintenance.
//!
//! Handles cleanup of stale multipart uploads and other maintenance tasks.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::time::interval;

use crate::LocalFsStore;

/// Configuration for the cleanup task.
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// How often to run cleanup (default: 1 hour).
    pub interval: Duration,
    /// Maximum age for multipart uploads before cleanup (default: 7 days).
    pub multipart_max_age: Duration,
    /// Whether cleanup is enabled.
    pub enabled: bool,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3600),                   // 1 hour
            multipart_max_age: Duration::from_secs(7 * 24 * 3600), // 7 days
            enabled: true,
        }
    }
}

impl CleanupConfig {
    /// Create a new cleanup config.
    pub fn new(interval: Duration, multipart_max_age: Duration) -> Self {
        Self {
            interval,
            multipart_max_age,
            enabled: true,
        }
    }

    /// Disable cleanup.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

/// Start the background cleanup task.
///
/// This spawns a tokio task that periodically cleans up stale multipart uploads.
/// Returns a handle that can be used to abort the task.
pub fn start_cleanup_task(
    store: Arc<LocalFsStore>,
    config: CleanupConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if !config.enabled {
            tracing::info!("Cleanup task disabled");
            return;
        }

        tracing::info!(
            "Starting cleanup task: interval={}s, multipart_max_age={}s",
            config.interval.as_secs(),
            config.multipart_max_age.as_secs()
        );

        let mut ticker = interval(config.interval);

        loop {
            ticker.tick().await;

            if let Err(e) = cleanup_stale_multipart_uploads(&store, config.multipart_max_age).await
            {
                tracing::error!("Multipart cleanup failed: {}", e);
            }
        }
    })
}

/// Clean up stale multipart uploads.
///
/// Finds all multipart uploads older than `max_age` and aborts them,
/// cleaning up their parts and freeing disk space.
async fn cleanup_stale_multipart_uploads(
    store: &LocalFsStore,
    max_age: Duration,
) -> strix_core::Result<()> {
    let cutoff =
        Utc::now() - chrono::Duration::from_std(max_age).unwrap_or(chrono::Duration::days(7));
    let cutoff_str = cutoff.to_rfc3339();

    tracing::debug!("Cleaning up multipart uploads older than {}", cutoff_str);

    // Find stale uploads
    let stale_uploads = store.find_stale_multipart_uploads(&cutoff_str).await?;

    if stale_uploads.is_empty() {
        tracing::debug!("No stale multipart uploads found");
        return Ok(());
    }

    tracing::info!(
        "Found {} stale multipart uploads to clean up",
        stale_uploads.len()
    );

    let mut cleaned = 0;
    let mut failed = 0;

    for upload in stale_uploads {
        match store.abort_stale_multipart(&upload.upload_id).await {
            Ok(()) => {
                tracing::debug!(
                    "Cleaned up stale multipart upload: {} (bucket={}, key={})",
                    upload.upload_id,
                    upload.bucket,
                    upload.key
                );
                cleaned += 1;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to clean up multipart upload {}: {}",
                    upload.upload_id,
                    e
                );
                failed += 1;
            }
        }
    }

    tracing::info!(
        "Multipart cleanup complete: {} cleaned, {} failed",
        cleaned,
        failed
    );

    Ok(())
}

/// Stale upload info for cleanup.
#[derive(Debug)]
pub struct StaleUpload {
    pub upload_id: String,
    pub bucket: String,
    pub key: String,
    pub initiated_at: String,
}
