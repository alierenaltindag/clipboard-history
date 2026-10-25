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

pub struct WaylandWatcher {
    capture_primary: bool,
}

impl Default for WaylandWatcher {
    fn default() -> Self {
        Self::new(true)
    }
}

impl WaylandWatcher {
    pub fn new(capture_primary: bool) -> Self {
        Self { capture_primary }
    }

    async fn run_wl_paste(args: &[&str]) -> Option<Vec<u8>> {
        let mut cmd = Command::new("wl-paste");
        cmd.args(args);
        cmd.kill_on_drop(true);

        match tokio::time::timeout(Duration::from_millis(2500), cmd.output()).await {
            Ok(Ok(output)) if output.status.success() => Some(output.stdout),
            Ok(Ok(output)) => {
                debug!(
                    "`wl-paste {}` failed with status {:?}",
                    args.join(" "),
                    output.status
                );
                None
            }
            Ok(Err(e)) => {
                warn!("`wl-paste {}` execution error: {}", args.join(" "), e);
                None
            }
            Err(_) => {
                warn!("`wl-paste {}` timed out after 2.5s", args.join(" "));
                None
            }
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

        let mime_types: Vec<String> = match Self::run_wl_paste(list_args).await {
            Some(types_bytes) => {
                let types_str = String::from_utf8_lossy(&types_bytes);
                types_str
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            None => Vec::new(),
        };

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
        }

        // Resilient fallback: If text is still None and no image was captured, ALWAYS try generic wl-paste --no-newline!
        if text.is_none() && image_data.is_none() {
            let args = if is_primary {
                vec!["--primary", "--no-newline"]
            } else {
                vec!["--no-newline"]
            };
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

        let final_mime_types = if mime_types.is_empty() {
            if text.is_some() {
                vec!["text/plain".to_string()]
            } else if image_data.is_some() {
                vec!["image/png".to_string()]
            } else {
                vec![]
            }
        } else {
            mime_types
        };

        Some(RawClipboardEvent {
            selection,
            mime_types: final_mime_types,
            text,
            html,
            image_data,
            source_app: None,
        })
    }

    async fn poll_loop(selection: SelectionTarget, sender: Sender<RawClipboardEvent>) {
        let mut last_hash = String::new();
        loop {
            sleep(Duration::from_millis(400)).await;
            if let Some(event) = Self::fetch_clipboard_content(selection).await {
                let current_hash = if let Some(ref text) = event.text {
                    clipboard_history_core::blob::BlobStore::compute_hash(text.as_bytes())
                } else if let Some(ref img) = event.image_data {
                    clipboard_history_core::blob::BlobStore::compute_hash(img)
                } else {
                    continue;
                };

                if current_hash != last_hash {
                    last_hash = current_hash;
                    let _ = sender.send(event).await;
                }
            }
        }
    }

    async fn watch_loop(selection: SelectionTarget, sender: Sender<RawClipboardEvent>) {
        let is_primary = selection == SelectionTarget::Primary;
        let mut consecutive_failures = 0;

        loop {
            clipboard_history_core::clipboard::ensure_display_environment();
            let spawn_time = std::time::Instant::now();
            let args = if is_primary {
                vec![
                    "--primary",
                    "--watch",
                    "sh",
                    "-c",
                    "cat > /dev/null; echo change",
                ]
            } else {
                vec!["--watch", "sh", "-c", "cat > /dev/null; echo change"]
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
                                    // Trailing debounce: reset timer to 600ms to avoid typing stutter
                                    debounce_timer = Some(tokio::time::Instant::now() + Duration::from_millis(600));
                                } else {
                                    // Fast debounce for standard clipboard (80ms)
                                    sleep(Duration::from_millis(80)).await;
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
                            // Filter single-character and short noise (require at least 3 chars)
                            if let Some(ref t) = event.text {
                                if t.trim().len() >= 3 {
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
                    if err_msg.contains("data-control") {
                        warn!(
                            "Compositor does not support data-control watch mode for {:?}. Falling back to active clipboard polling.",
                            selection
                        );
                        Self::poll_loop(selection, sender).await;
                        return;
                    }

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
        info!(
            "Starting Wayland clipboard watcher (capture_primary={})",
            self.capture_primary
        );

        if which::which("wl-paste").is_err() {
            warn!("`wl-paste` binary not found in PATH. Please install wl-clipboard for Wayland support.");
            return Err(CoreError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "wl-paste not found",
            )));
        }

        let clipboard_sender = sender.clone();
        let clipboard_handle = tokio::spawn(async move {
            Self::watch_loop(SelectionTarget::Clipboard, clipboard_sender).await;
        });

        if self.capture_primary {
            let primary_sender = sender;
            let primary_handle = tokio::spawn(async move {
                Self::watch_loop(SelectionTarget::Primary, primary_sender).await;
            });
            let _ = tokio::join!(primary_handle, clipboard_handle);
        } else {
            let _ = clipboard_handle.await;
        }

        Ok(())
    }
}
