use clipboard_history_core::config::AppConfig;
use clipboard_history_core::updater::{check_for_updates, is_newer_version};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Spawns the background periodic update check task.
pub fn start_update_checker(config_arc: Arc<RwLock<AppConfig>>) {
    tokio::spawn(async move {
        // Initial delay on startup (30 seconds) so daemon startup and watcher attachment are not delayed
        tokio::time::sleep(Duration::from_secs(30)).await;

        loop {
            let (auto_check, interval_hours, last_check, last_notified) = {
                let cfg = config_arc.read().await;
                (
                    cfg.updates.auto_check,
                    cfg.updates.check_interval_hours.max(1),
                    cfg.updates.last_check_timestamp,
                    cfg.updates.last_notified_version.clone(),
                )
            };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let elapsed_hours = if now > last_check {
                (now - last_check) / 3600
            } else {
                0
            };

            if auto_check && (last_check == 0 || elapsed_hours >= interval_hours) {
                info!("Running periodic automatic update check...");
                let current_ver = env!("CARGO_PKG_VERSION");
                match check_for_updates(current_ver).await {
                    Ok(Some(release)) => {
                        info!("Update check found new version: {}", release.version);
                        let should_notify = match &last_notified {
                            Some(v) => is_newer_version(v, &release.version),
                            None => true,
                        };

                        if should_notify {
                            {
                                let mut cfg = config_arc.write().await;
                                cfg.updates.last_check_timestamp = now;
                                cfg.updates.last_notified_version = Some(release.version.clone());
                                let _ = cfg.save(AppConfig::config_path());
                            }
                            crate::notification::dispatch_update_notification(release).await;
                        } else {
                            debug!(
                                "Already notified for version {}, skipping prompt",
                                release.version
                            );
                            let mut cfg = config_arc.write().await;
                            cfg.updates.last_check_timestamp = now;
                            let _ = cfg.save(AppConfig::config_path());
                        }
                    }
                    Ok(None) => {
                        debug!("Clipboard History is up to date (v{})", current_ver);
                        let mut cfg = config_arc.write().await;
                        cfg.updates.last_check_timestamp = now;
                        let _ = cfg.save(AppConfig::config_path());
                    }
                    Err(e) => {
                        debug!("Background update check could not contact GitHub: {}", e);
                    }
                }
            }

            // Check condition every hour
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}
