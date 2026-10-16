use crate::injector::traits::PasteInjector;
use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

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
        which::which("xdotool").is_ok()
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
        Command::new("xdotool")
            .args(["key", "--clearmodifiers", key_combination])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
