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

#[async_trait]
impl PasteInjector for XdotoolInjector {
    fn name(&self) -> &'static str {
        "xdotool"
    }

    fn is_available(&self) -> bool {
        which::which("xdotool").is_ok()
    }

    async fn inject(&self) -> bool {
        debug!("Injecting paste via xdotool");
        Command::new("xdotool")
            .args(["key", "--clearmodifiers", "ctrl+v"])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
