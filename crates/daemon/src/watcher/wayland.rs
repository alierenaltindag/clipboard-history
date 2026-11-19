use crate::watcher::traits::{ClipboardWatcher, RawClipboardEvent, SelectionTarget};
use async_trait::async_trait;
use clipboard_history_core::blob::BlobStore;
use clipboard_history_core::error::{CoreError, Result};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

const WATCH_SCRIPT: &str = r#"if [ "$CLIPBOARD_STATE" = "sensitive" ]; then echo sensitive; elif [ "$CLIPBOARD_STATE" = "nil" ]; then echo nil; else cat > /dev/null; echo change; fi"#;

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

    pub(crate) fn select_preferred_text_mime(mime_types: &[String]) -> Option<&str> {
        mime_types
            .iter()
            .find(|m| m.eq_ignore_ascii_case("text/plain;charset=utf-8"))
            .or_else(|| {
                mime_types
                    .iter()
                    .find(|m| m.eq_ignore_ascii_case("text/plain"))
            })
            .or_else(|| {
                mime_types
                    .iter()
                    .find(|m| m.eq_ignore_ascii_case("UTF8_STRING"))
            })
            .or_else(|| {
                mime_types
                    .iter()
                    .find(|m| m.eq_ignore_ascii_case("text/uri-list"))
            })
            .or_else(|| mime_types.iter().find(|m| m.eq_ignore_ascii_case("STRING")))
            .or_else(|| mime_types.iter().find(|m| m.eq_ignore_ascii_case("text")))
            .or_else(|| {
                mime_types.iter().find(|m| {
                    let lower = m.to_ascii_lowercase();
                    lower.starts_with("text/plain;") && !lower.contains("utf-16")
                })
            })
            .or_else(|| {
                mime_types
                    .iter()
                    .find(|m| m.to_ascii_lowercase().starts_with("text/plain"))
            })
            .map(String::as_str)
    }

    pub(crate) fn is_data_control_unsupported(stderr: &str) -> bool {
        let lower = stderr.to_ascii_lowercase();
        const PATTERNS: &[&str] = &[
            "data-control",
            "data_control",
            "wlr-data-control",
            "ext-data-control",
            "watch mode requires",
            "not supported",
            "missing protocol",
        ];
        PATTERNS.iter().any(|p| lower.contains(p))
    }

    async fn run_wl_paste(args: &[&str]) -> Option<Vec<u8>> {
        let mut cmd = Command::new("wl-paste");
        cmd.args(args).stdin(Stdio::null()).kill_on_drop(true);

        match tokio::time::timeout(Duration::from_millis(2500), cmd.output()).await {
            Ok(Ok(output)) if output.status.success() => Some(output.stdout),
            Ok(Ok(output)) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                debug!(
                    "`wl-paste {}` failed with status {:?}: {}",
                    args.join(" "),
                    output.status,
                    stderr.trim()
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

        // 1. Fetch text FIRST using prioritized text MIME selection
        if let Some(text_mime) = Self::select_preferred_text_mime(&mime_types) {
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

        // 2. Fetch text/html if advertised
        if mime_types
            .iter()
            .any(|m| m.eq_ignore_ascii_case("text/html"))
        {
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

        // 3. ONLY fetch image/* when text is None
        if text.is_none() {
            let preferred_img_mime = mime_types
                .iter()
                .find(|m| m.eq_ignore_ascii_case("image/png"))
                .or_else(|| {
                    mime_types
                        .iter()
                        .find(|m| m.eq_ignore_ascii_case("image/webp"))
                })
                .or_else(|| {
                    mime_types
                        .iter()
                        .find(|m| m.eq_ignore_ascii_case("image/jpeg"))
                })
                .or_else(|| {
                    mime_types
                        .iter()
                        .find(|m| m.to_ascii_lowercase().starts_with("image/"))
                });

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
        }

        // 4. Resilient fallback: If text is still None and no image was captured, try generic wl-paste --no-newline
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

    fn event_hash(event: &RawClipboardEvent) -> Option<String> {
        event
            .text
            .as_ref()
            .map(|text| BlobStore::compute_hash(text.as_bytes()))
            .or_else(|| {
                event
                    .image_data
                    .as_ref()
                    .map(|img| BlobStore::compute_hash(img))
            })
    }

    async fn fetch_poll_event(selection: SelectionTarget) -> (Option<RawClipboardEvent>, bool) {
        if let Some(event) = Self::fetch_clipboard_content(selection).await {
            return (Some(event), true);
        }

        if selection == SelectionTarget::Clipboard {
            if let Some(text) = crate::watcher::klipper::read_klipper_clipboard().await {
                let event = RawClipboardEvent {
                    selection: SelectionTarget::Clipboard,
                    mime_types: vec!["text/plain".to_string()],
                    text: Some(text),
                    html: None,
                    image_data: None,
                    source_app: None,
                };
                return (Some(event), false);
            }
        }

        (None, false)
    }

    async fn poll_loop(selection: SelectionTarget, sender: Sender<RawClipboardEvent>) {
        // Seed initial clipboard hash on startup without emitting stale boot content
        let (initial_event, _) = Self::fetch_poll_event(selection).await;
        let mut last_hash = initial_event
            .as_ref()
            .and_then(Self::event_hash)
            .unwrap_or_default();
        let mut last_env_refresh = tokio::time::Instant::now();

        loop {
            sleep(Duration::from_millis(400)).await;

            let (maybe_event, wl_paste_ok) = Self::fetch_poll_event(selection).await;
            if wl_paste_ok {
                last_env_refresh = tokio::time::Instant::now();
            } else if last_env_refresh.elapsed() >= Duration::from_secs(15) {
                clipboard_history_core::clipboard::ensure_display_environment();
                last_env_refresh = tokio::time::Instant::now();
            }

            if let Some(event) = maybe_event {
                let Some(current_hash) = Self::event_hash(&event) else {
                    continue;
                };

                if current_hash != last_hash {
                    last_hash = current_hash;
                    if sender.send(event).await.is_err() {
                        return;
                    }
                }
            }
        }
    }

    async fn watch_loop(selection: SelectionTarget, sender: Sender<RawClipboardEvent>) {
        let is_primary = selection == SelectionTarget::Primary;
        let debounce_duration = if is_primary {
            Duration::from_millis(500)
        } else {
            Duration::from_millis(60)
        };
        let mut consecutive_failures = 0;

        loop {
            clipboard_history_core::clipboard::ensure_display_environment_force();
            let spawn_time = std::time::Instant::now();
            let args: Vec<&str> = if is_primary {
                vec!["--primary", "--watch", "sh", "-c", WATCH_SCRIPT]
            } else {
                vec!["--watch", "sh", "-c", WATCH_SCRIPT]
            };

            let mut child = match Command::new("wl-paste")
                .args(&args)
                .stdin(Stdio::null())
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
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            // Drain stderr concurrently in a bounded buffer so the 64KB pipe never deadlocks wl-paste
            let stderr_buf = Arc::new(Mutex::new(String::new()));
            let stderr_task = child.stderr.take().map(|mut stderr| {
                let buf = Arc::clone(&stderr_buf);
                tokio::spawn(async move {
                    let mut chunk = [0u8; 1024];
                    const MAX_ERR_BYTES: usize = 4096;
                    loop {
                        match stderr.read(&mut chunk).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                let mut guard = buf.lock().await;
                                if guard.len() < MAX_ERR_BYTES {
                                    let remaining = MAX_ERR_BYTES - guard.len();
                                    let s = String::from_utf8_lossy(&chunk[..n]);
                                    if s.len() <= remaining {
                                        guard.push_str(&s);
                                    } else {
                                        let mut cut = remaining;
                                        while cut > 0 && !s.is_char_boundary(cut) {
                                            cut -= 1;
                                        }
                                        guard.push_str(&s[..cut]);
                                    }
                                }
                            }
                        }
                    }
                })
            });

            let mut reader = tokio::io::BufReader::new(stdout).lines();

            debug!("wl-paste ({:?}) watcher spawned successfully", selection);

            let mut debounce_timer: Option<tokio::time::Instant> = None;

            loop {
                tokio::select! {
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(line_str)) => {
                                consecutive_failures = 0;
                                let trimmed = line_str.trim();
                                if trimmed == "sensitive" {
                                    debug!(
                                        "Wayland ({:?}) clipboard state is sensitive; ignoring",
                                        selection
                                    );
                                } else if trimmed == "nil" {
                                    // Nil / cleared clipboard state; ignore
                                } else {
                                    debounce_timer =
                                        Some(tokio::time::Instant::now() + debounce_duration);
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
                        if let Some(event) = Self::fetch_clipboard_content(selection).await {
                            if is_primary {
                                if let Some(ref t) = event.text {
                                    if t.trim().len() >= 3 {
                                        info!(
                                            "Wayland primary selection captured ({} chars)",
                                            t.len()
                                        );
                                        if sender.send(event).await.is_err() {
                                            return;
                                        }
                                    }
                                } else if event.image_data.is_some()
                                    && sender.send(event).await.is_err()
                                {
                                    return;
                                }
                            } else {
                                let has_text = event.text.is_some();
                                let has_img = event.image_data.is_some();
                                info!(
                                    "Wayland clipboard event captured (text: {}, image: {})",
                                    has_text, has_img
                                );
                                if sender.send(event).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            let exit_status =
                match tokio::time::timeout(Duration::from_millis(500), child.wait()).await {
                    Ok(Ok(status)) => Some(status),
                    Ok(Err(e)) => {
                        debug!("Failed waiting on `wl-paste` ({:?}): {}", selection, e);
                        None
                    }
                    Err(_) => {
                        let _ = child.kill().await;
                        child.wait().await.ok()
                    }
                };

            if let Some(task) = stderr_task {
                let _ = tokio::time::timeout(Duration::from_millis(250), task).await;
            }
            let err_msg = stderr_buf.lock().await.clone();

            let failed = exit_status.map(|s| !s.success()).unwrap_or(true);
            if failed {
                warn!(
                    "`wl-paste` ({:?}) exited with error: {}",
                    selection,
                    err_msg.trim()
                );
                if Self::is_data_control_unsupported(&err_msg) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mimes(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn test_select_preferred_text_mime_priority() {
        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&[
                "text/plain;charset=utf-16",
                "text/plain",
                "text/plain;charset=utf-8",
            ])),
            Some("text/plain;charset=utf-8")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&[
                "TEXT/PLAIN;CHARSET=UTF-8",
                "text/plain",
            ])),
            Some("TEXT/PLAIN;CHARSET=UTF-8")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&[
                "text/plain;charset=utf-16",
                "UTF8_STRING",
                "text/plain",
            ])),
            Some("text/plain")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&[
                "STRING",
                "text/uri-list",
                "UTF8_STRING",
            ])),
            Some("UTF8_STRING")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&["STRING", "text/uri-list"])),
            Some("text/uri-list")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&["text", "STRING"])),
            Some("STRING")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&[
                "text/plain;charset=utf-16le",
                "text",
            ])),
            Some("text")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&[
                "text/plain;charset=utf-16",
                "text/plain;charset=iso-8859-1",
            ])),
            Some("text/plain;charset=iso-8859-1")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&["text/plain;charset=utf-16"])),
            Some("text/plain;charset=utf-16")
        );

        assert_eq!(
            WaylandWatcher::select_preferred_text_mime(&mimes(&["image/png", "image/jpeg"])),
            None
        );
    }

    #[test]
    fn test_is_data_control_unsupported() {
        assert!(WaylandWatcher::is_data_control_unsupported(
            "Compositor doesn't support the data-control protocol"
        ));
        assert!(WaylandWatcher::is_data_control_unsupported(
            "Error: Missing zwlr_data_control_manager_v1 (wlr-data-control)"
        ));
        assert!(WaylandWatcher::is_data_control_unsupported(
            "ext-data-control is unavailable"
        ));
        assert!(WaylandWatcher::is_data_control_unsupported(
            "DATA_CONTROL not available"
        ));
        assert!(WaylandWatcher::is_data_control_unsupported(
            "Watch mode requires a compositor supporting data control"
        ));
        assert!(WaylandWatcher::is_data_control_unsupported(
            "Protocol is Not Supported on this compositor"
        ));
        assert!(WaylandWatcher::is_data_control_unsupported(
            "Missing protocol implementation"
        ));
        assert!(!WaylandWatcher::is_data_control_unsupported(
            "Failed to connect to a Wayland compositor"
        ));
    }
}
