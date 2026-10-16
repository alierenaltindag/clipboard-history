use crate::watcher::traits::{ClipboardWatcher, RawClipboardEvent, SelectionTarget};
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

    async fn run_wl_paste(args: &[&str]) -> Option<Vec<u8>> {
        let mut cmd = Command::new("wl-paste");
        cmd.args(args);
        cmd.kill_on_drop(true);

        match tokio::time::timeout(Duration::from_millis(1500), cmd.output()).await {
            Ok(Ok(output)) if output.status.success() => Some(output.stdout),
            _ => None,
        }
    }

    async fn fetch_clipboard_content(selection: SelectionTarget) -> Option<RawClipboardEvent> {
        let is_primary = selection == SelectionTarget::Primary;

        // Query types with timeout
        let list_args: &[&str] = if is_primary {
            &["--primary", "--list-types"]
        } else {
            &["--list-types"]
        };

        let types_bytes = Self::run_wl_paste(list_args).await?;
        let types_str = String::from_utf8_lossy(&types_bytes);
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

        let preferred_img_mime = mime_types
            .iter()
            .find(|m| m.as_str() == "image/png")
            .or_else(|| mime_types.iter().find(|m| m.as_str() == "image/webp"))
            .or_else(|| mime_types.iter().find(|m| m.as_str() == "image/jpeg"))
            .or_else(|| mime_types.iter().find(|m| m.starts_with("image/")));

        if let Some(img_mime) = preferred_img_mime {
            let mut args = if is_primary {
                vec!["--primary"]
            } else {
                vec![]
            };
            args.extend_from_slice(&["--type", img_mime, "--no-newline"]);
            if let Some(bytes) = Self::run_wl_paste(&args).await {
                if !bytes.is_empty() {
                    image_data = Some(bytes);
                }
            }
        }

        if mime_types.iter().any(|m| m == "text/html") {
            let mut args = if is_primary {
                vec!["--primary"]
            } else {
                vec![]
            };
            args.extend_from_slice(&["--type", "text/html"]);
            if let Some(bytes) = Self::run_wl_paste(&args).await {
                let s = String::from_utf8_lossy(&bytes).to_string();
                if !s.is_empty() {
                    html = Some(s);
                }
            }
        }

        // Fetch text with explicit text type or URI-list
        if let Some(text_mime) = mime_types.iter().find(|m| {
            m.starts_with("text/plain")
                || *m == "UTF8_STRING"
                || *m == "STRING"
                || *m == "text"
                || *m == "text/uri-list"
        }) {
            let mut args = if is_primary {
                vec!["--primary"]
            } else {
                vec![]
            };
            args.extend_from_slice(&["--type", text_mime, "--no-newline"]);
            if let Some(bytes) = Self::run_wl_paste(&args).await {
                let s = String::from_utf8_lossy(&bytes)
                    .trim_matches('\0')
                    .to_string();
                if !s.is_empty() {
                    text = Some(s);
                }
            }
        } else if image_data.is_none() && html.is_none() {
            let mut args = if is_primary {
                vec!["--primary"]
            } else {
                vec![]
            };
            args.push("--no-newline");
            if let Some(bytes) = Self::run_wl_paste(&args).await {
                let s = String::from_utf8_lossy(&bytes)
                    .trim_matches('\0')
                    .to_string();
                if !s.is_empty() {
                    text = Some(s);
                }
            }
        }

        if text.is_none() && html.is_none() && image_data.is_none() {
            return None;
        }

        Some(RawClipboardEvent {
            selection,
            mime_types,
            text,
            html,
            image_data,
            source_app: None,
        })
    }

    async fn watch_loop(selection: SelectionTarget, sender: Sender<RawClipboardEvent>) {
        let is_primary = selection == SelectionTarget::Primary;
        let mut consecutive_failures = 0;

        loop {
            let spawn_time = std::time::Instant::now();
            let args = if is_primary {
                vec!["--primary", "--watch", "echo", "change"]
            } else {
                vec!["--watch", "echo", "change"]
            };

            let mut child = match Command::new("wl-paste")
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to spawn `wl-paste {}`: {}", args.join(" "), e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let stdout = match child.stdout.take() {
                Some(s) => s,
                None => {
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            let mut reader = tokio::io::BufReader::new(stdout).lines();

            debug!("wl-paste ({:?}) watcher spawned successfully", selection);

            let mut debounce_timer: Option<tokio::time::Instant> = None;

            loop {
                tokio::select! {
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(_)) => {
                                consecutive_failures = 0;
                                if is_primary {
                                    // Trailing debounce: reset timer to 300ms
                                    debounce_timer = Some(tokio::time::Instant::now() + Duration::from_millis(300));
                                } else {
                                    // Fast debounce for standard clipboard (60ms)
                                    sleep(Duration::from_millis(60)).await;
                                    if let Some(event) = Self::fetch_clipboard_content(SelectionTarget::Clipboard).await {
                                        let has_text = event.text.is_some();
                                        let has_img = event.image_data.is_some();
                                        info!(
                                            "Wayland clipboard event captured (text: {}, image: {})",
                                            has_text, has_img
                                        );
                                        let _ = sender.send(event).await;
                                    }
                                }
                            }
                            _ => break,
                        }
                    }
                    _ = async {
                        match debounce_timer {
                            Some(deadline) => tokio::time::sleep_until(deadline).await,
                            None => std::future::pending().await,
                        }
                    }, if debounce_timer.is_some() => {
                        debounce_timer = None;
                        if let Some(event) = Self::fetch_clipboard_content(SelectionTarget::Primary).await {
                            // Filter single-character noise
                            if let Some(ref t) = event.text {
                                if t.trim().len() >= 2 {
                                    info!("Wayland primary selection captured ({} chars)", t.len());
                                    let _ = sender.send(event).await;
                                }
                            } else if event.image_data.is_some() {
                                let _ = sender.send(event).await;
                            }
                        }
                    }
                }
            }

            let _ = child.kill().await;
            if let Ok(output) = child.wait_with_output().await {
                if !output.status.success() {
                    let err_msg = String::from_utf8_lossy(&output.stderr);
                    warn!(
                        "`wl-paste` ({:?}) exited with error: {}",
                        selection,
                        err_msg.trim()
                    );
                    if spawn_time.elapsed() < Duration::from_millis(2000) {
                        consecutive_failures += 1;
                    } else {
                        consecutive_failures = 0;
                    }
                } else {
                    consecutive_failures = 0;
                }
            }

            let backoff = if consecutive_failures >= 3 {
                warn!(
                    "`wl-paste` ({:?}) failed repeatedly. Backing off for 10s...",
                    selection
                );
                Duration::from_secs(10)
            } else {
                Duration::from_secs(1)
            };
            sleep(backoff).await;
        }
    }
}

#[async_trait]
impl ClipboardWatcher for WaylandWatcher {
    async fn run(&mut self, sender: Sender<RawClipboardEvent>) -> Result<()> {
        info!("Starting Wayland dual clipboard & primary watcher");

        if which::which("wl-paste").is_err() {
            warn!("`wl-paste` binary not found in PATH. Please install wl-clipboard for Wayland support.");
            return Err(CoreError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "wl-paste not found",
            )));
        }

        let sender_primary = sender.clone();
        let primary_handle = tokio::spawn(async move {
            Self::watch_loop(SelectionTarget::Primary, sender_primary).await;
        });

        let clipboard_handle = tokio::spawn(async move {
            Self::watch_loop(SelectionTarget::Clipboard, sender).await;
        });

        let _ = tokio::join!(primary_handle, clipboard_handle);
        Ok(())
    }
}
