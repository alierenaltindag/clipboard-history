use crate::watcher::traits::{ClipboardWatcher, RawClipboardEvent, SelectionTarget};
use async_trait::async_trait;
use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::error::Result;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tracing::{debug, info};

const KLIPPER_DEST: &str = "org.kde.klipper";
const KLIPPER_PATH: &str = "/klipper";
const KLIPPER_IFACE: &str = "org.kde.klipper.klipper";

pub struct KlipperWatcher;

impl KlipperWatcher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KlipperWatcher {
    fn default() -> Self {
        Self::new()
    }
}

async fn connect_and_probe() -> std::result::Result<(zbus::Proxy<'static>, String), zbus::Error> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&conn, KLIPPER_DEST, KLIPPER_PATH, KLIPPER_IFACE).await?;
    let initial: String = proxy.call("getClipboardContents", &()).await?;
    Ok((proxy, initial))
}

/// Reads the current plain-text clipboard contents from KDE Klipper over D-Bus, if available.
pub async fn read_klipper_clipboard() -> Option<String> {
    let fut = async {
        let (_proxy, content) = connect_and_probe().await.ok()?;
        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    };

    tokio::time::timeout(Duration::from_millis(1500), fut)
        .await
        .ok()
        .flatten()
}

#[async_trait]
impl ClipboardWatcher for KlipperWatcher {
    async fn run(&mut self, sender: Sender<RawClipboardEvent>) -> Result<()> {
        let is_kde = std::env::var("XDG_CURRENT_DESKTOP")
            .map(|v| {
                let lower = v.to_ascii_lowercase();
                lower.contains("kde") || lower.contains("plasma")
            })
            .unwrap_or(false);

        let max_attempts = if is_kde { 6 } else { 1 };
        let mut connected = None;

        for attempt in 1..=max_attempts {
            match connect_and_probe().await {
                Ok(res) => {
                    connected = Some(res);
                    break;
                }
                Err(e) => {
                    if attempt < max_attempts {
                        debug!(
                            "Klipper D-Bus probe attempt {}/{} failed ({}); retrying in 5s...",
                            attempt, max_attempts, e
                        );
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    } else {
                        debug!(
                            "Klipper D-Bus service unavailable ({}); exiting KlipperWatcher cleanly",
                            e
                        );
                    }
                }
            }
        }

        let Some((proxy, initial_content)) = connected else {
            return Ok(());
        };

        info!("Connected to KDE Klipper D-Bus clipboard service");

        // Seed last_hash from initial clipboard contents so stale boot clipboard is not re-emitted
        let mut last_hash = if initial_content.is_empty() {
            String::new()
        } else {
            BlobStore::compute_hash(initial_content.as_bytes())
        };

        let mut signal_stream = match proxy.receive_signal("clipboardHistoryUpdated").await {
            Ok(stream) => Some(stream),
            Err(e) => {
                debug!(
                    "Failed to subscribe to Klipper clipboardHistoryUpdated signal ({}); using interval polling",
                    e
                );
                None
            }
        };

        let mut interval = tokio::time::interval(Duration::from_millis(1500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;

        loop {
            tokio::select! {
                sig = async {
                    match signal_stream.as_mut() {
                        Some(s) => s.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if sig.is_none() {
                        debug!("Klipper D-Bus signal stream disconnected; stopping KlipperWatcher cleanly");
                        break;
                    }
                }
                _ = interval.tick() => {}
            }

            match proxy
                .call::<_, _, String>("getClipboardContents", &())
                .await
            {
                Ok(content) => {
                    if content.is_empty() {
                        continue;
                    }
                    let hash = BlobStore::compute_hash(content.as_bytes());
                    if hash != last_hash {
                        last_hash = hash;
                        let event = RawClipboardEvent {
                            selection: SelectionTarget::Clipboard,
                            mime_types: vec!["text/plain".to_string()],
                            text: Some(content),
                            html: None,
                            image_data: None,
                            source_app: None,
                        };
                        if sender.send(event).await.is_err() {
                            debug!("Clipboard event receiver closed; stopping KlipperWatcher");
                            break;
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        "Klipper D-Bus getClipboardContents failed ({}); stopping KlipperWatcher cleanly",
                        e
                    );
                    break;
                }
            }
        }

        Ok(())
    }
}
