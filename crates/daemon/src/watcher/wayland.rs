use crate::watcher::traits::{ClipboardWatcher, RawClipboardEvent};
use async_trait::async_trait;
use clipboard_history_core::error::{CoreError, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

pub struct WaylandWatcher;

impl WaylandWatcher {
    pub fn new() -> Self {
        Self
    }

    async fn fetch_clipboard_content() -> Option<RawClipboardEvent> {
        // Query types
        let types_output = Command::new("wl-paste")
            .arg("--list-types")
            .output()
            .await
            .ok()?;

        let types_str = String::from_utf8_lossy(&types_output.stdout);
        let mime_types: Vec<String> = types_str
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        if mime_types.is_empty() {
            return None;
        }

        let mut text = None;
        let mut html = None;
        let mut image_data = None;

        if let Some(img_mime) = mime_types.iter().find(|m| m.starts_with("image/")) {
            // Fetch image bytes with explicit MIME type
            if let Ok(output) = Command::new("wl-paste")
                .args(["--type", img_mime, "--no-newline"])
                .output()
                .await
            {
                if output.status.success() && !output.stdout.is_empty() {
                    image_data = Some(output.stdout);
                }
            }
        }

        if mime_types.iter().any(|m| m == "text/html") {
            if let Ok(output) = Command::new("wl-paste")
                .args(["--type", "text/html"])
                .output()
                .await
            {
                if output.status.success() {
                    if let Ok(s) = String::from_utf8(output.stdout) {
                        html = Some(s);
                    }
                }
            }
        }

        // Fetch text with explicit text type to avoid reading raw binary images as utf-8
        if let Some(text_mime) = mime_types.iter().find(|m| {
            m.starts_with("text/plain") || *m == "UTF8_STRING" || *m == "STRING" || *m == "text"
        }) {
            if let Ok(output) = Command::new("wl-paste")
                .args(["--type", text_mime, "--no-newline"])
                .output()
                .await
            {
                if output.status.success() {
                    if let Ok(s) = String::from_utf8(output.stdout) {
                        if !s.is_empty() {
                            text = Some(s);
                        }
                    }
                }
            }
        } else if image_data.is_none() && html.is_none() {
            // Fallback for generic text if no image or html
            if let Ok(output) = Command::new("wl-paste").arg("--no-newline").output().await {
                if output.status.success() {
                    if let Ok(s) = String::from_utf8(output.stdout) {
                        if !s.is_empty() {
                            text = Some(s);
                        }
                    }
                }
            }
        }

        Some(RawClipboardEvent {
            mime_types,
            text,
            html,
            image_data,
            source_app: None,
        })
    }
}

#[async_trait]
impl ClipboardWatcher for WaylandWatcher {
    async fn run(&mut self, sender: Sender<RawClipboardEvent>) -> Result<()> {
        info!("Starting Wayland clipboard watcher");

        // Check if wl-paste is installed
        if which::which("wl-paste").is_err() {
            warn!("`wl-paste` binary not found in PATH. Please install wl-clipboard for Wayland support.");
            return Err(CoreError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "wl-paste not found",
            )));
        }

        loop {
            // Run wl-paste --watch echo "change"
            let mut child = match Command::new("wl-paste")
                .args(["--watch", "echo", "change"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to spawn `wl-paste --watch`: {}", e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let stdout = child.stdout.take().expect("Child must have stdout");
            let mut reader = tokio::io::BufReader::new(stdout).lines();

            debug!("wl-paste --watch process spawned successfully");

            while let Ok(Some(_line)) = reader.next_line().await {
                // Short debounce to avoid catching interim clips
                sleep(Duration::from_millis(50)).await;

                if let Some(event) = Self::fetch_clipboard_content().await {
                    let _ = sender.send(event).await;
                }
            }

            let _ = child.kill().await;
            let _ = child.wait().await;
            warn!("`wl-paste --watch` exited unexpectedly. Restarting in 1s...");
            sleep(Duration::from_secs(1)).await;
        }
    }
}
