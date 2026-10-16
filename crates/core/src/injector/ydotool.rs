use crate::injector::traits::PasteInjector;
use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

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
        debug!("Injecting paste via ydotool");
        // Keycode 29 = KEY_LEFTCTRL, 47 = KEY_V
        // 29:1 (press ctrl), 47:1 (press v), 47:0 (release v), 29:0 (release ctrl)
        Command::new("ydotool")
            .args(["key", "29:1", "47:1", "47:0", "29:0"])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
