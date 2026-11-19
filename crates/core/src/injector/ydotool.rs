use crate::injector::traits::PasteInjector;
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

pub struct YdotoolInjector;

impl YdotoolInjector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for YdotoolInjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PasteInjector for YdotoolInjector {
    fn name(&self) -> &'static str {
        "ydotool"
    }

    fn is_available(&self) -> bool {
        which::which("ydotool").is_ok()
    }

    async fn inject(&self) -> bool {
        self.inject_context(false).await
    }

    async fn inject_context(&self, is_terminal: bool) -> bool {
        debug!("Injecting paste via ydotool (is_terminal={})", is_terminal);
        // Keycode 29 = KEY_LEFTCTRL, 42 = KEY_LEFTSHIFT, 47 = KEY_V
        let args: &[&str] = if is_terminal {
            &["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"]
        } else {
            &["key", "29:1", "47:1", "47:0", "29:0"]
        };
        let mut cmd = Command::new("ydotool");
        cmd.args(args).stdin(Stdio::null()).kill_on_drop(true);

        match tokio::time::timeout(Duration::from_secs(2), cmd.status()).await {
            Ok(Ok(status)) => status.success(),
            Ok(Err(e)) => {
                debug!("ydotool execution failed: {}", e);
                false
            }
            Err(_) => {
                warn!("ydotool timed out after 2s");
                false
            }
        }
    }
}
