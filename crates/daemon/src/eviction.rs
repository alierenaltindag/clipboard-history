use chrono::{Duration, Utc};
use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::config::AppConfig;
use clipboard_history_core::storage::SqliteRepository;
use std::sync::Arc;
use tokio::time::sleep;
use tracing::{error, info};

pub struct EvictionWorker {
    repo: SqliteRepository,
    blob_store: BlobStore,
    config: Arc<tokio::sync::RwLock<AppConfig>>,
}

impl EvictionWorker {
    pub fn new(
        repo: SqliteRepository,
        blob_store: BlobStore,
        config: Arc<tokio::sync::RwLock<AppConfig>>,
    ) -> Self {
        Self {
            repo,
            blob_store,
            config,
        }
    }

    pub async fn run_loop(self) {
        info!("Starting background retention and eviction worker");
        let mut loop_count: u64 = 0;

        loop {
            // Run eviction cycle
            self.execute_cleanup().await;

            // Sleep 15 minutes between cycles
            sleep(std::time::Duration::from_secs(15 * 60)).await;
            loop_count += 1;

            // Trigger SQLite vacuum once every 24 hours (96 * 15min)
            if loop_count.is_multiple_of(96) {
                let repo_clone = self.repo.clone();
                let vacuum_res = tokio::task::spawn_blocking(move || repo_clone.vacuum()).await;
                match vacuum_res {
                    Ok(Err(e)) => error!("Scheduled database vacuum failed: {}", e),
                    Ok(Ok(_)) => info!("Scheduled database vacuum completed successfully"),
                    Err(e) => error!("Vacuum task spawn error: {}", e),
                }
            }
        }
    }

    pub async fn execute_cleanup(&self) {
        let (max_entries, retention_days) = {
            let cfg = self.config.read().await;
            (cfg.general.max_entries, cfg.general.retention_days)
        };

        let repo_clone = self.repo.clone();
        let blob_clone = self.blob_store.clone();

        let _ = tokio::task::spawn_blocking(move || {
            // 1. Evict expired entries by TTL
            let cutoff = Utc::now() - Duration::days(retention_days as i64);
            match repo_clone.evict_expired(cutoff) {
                Ok(count) if count > 0 => {
                    info!(
                        "Evicted {} expired clipboard entries (>{} days old)",
                        count, retention_days
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to evict expired clipboard entries: {}", e);
                }
            }

            // 2. Evict capacity excess
            match repo_clone.evict_capacity(max_entries) {
                Ok(count) if count > 0 => {
                    info!(
                        "Evicted {} excess clipboard entries (capacity limit {})",
                        count, max_entries
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to evict excess clipboard entries: {}", e);
                }
            }

            // 3. Clean up orphaned blobs
            match repo_clone.get_all_blob_hashes() {
                Ok(active_hashes) => {
                    if let Err(e) = blob_clone.cleanup_orphans(&active_hashes) {
                        error!("Failed to clean up orphaned blobs: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to get active blob hashes: {}", e);
                }
            }
        })
        .await;
    }
}
