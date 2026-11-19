use crate::injector::traits::PasteInjector;
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

pub struct XdotoolInjector;

impl XdotoolInjector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for XdotoolInjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PasteInjector for XdotoolInjector {
    fn name(&self) -> &'static str {
        "xdotool"
    }

    fn is_available(&self) -> bool {
        std::env::var("DISPLAY").is_ok() && which::which("xdotool").is_ok()
    }

    async fn inject(&self) -> bool {
        self.inject_context(false).await
    }

    async fn inject_context(&self, is_terminal: bool) -> bool {
        debug!("Injecting paste via xdotool (is_terminal={})", is_terminal);
        let key_combination = if is_terminal {
            "ctrl+shift+v"
        } else {
            "ctrl+v"
        };
        let mut cmd = Command::new("xdotool");
        cmd.args(["key", "--clearmodifiers", key_combination])
            .stdin(Stdio::null())
            .kill_on_drop(true);

        match tokio::time::timeout(Duration::from_secs(2), cmd.status()).await {
            Ok(Ok(status)) => status.success(),
            Ok(Err(e)) => {
                debug!("xdotool execution failed: {}", e);
                false
            }
            Err(_) => {
                warn!("xdotool timed out after 2s");
                false
            }
        }
    }
}
