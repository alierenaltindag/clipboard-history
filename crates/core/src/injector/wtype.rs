use crate::injector::traits::PasteInjector;
use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

pub struct WtypeInjector;

impl WtypeInjector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WtypeInjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PasteInjector for WtypeInjector {
    fn name(&self) -> &'static str {
        "wtype"
    }

    fn is_available(&self) -> bool {
        which::which("wtype").is_ok()
    }

    async fn inject(&self) -> bool {
        self.inject_context(false).await
    }

    async fn inject_context(&self, is_terminal: bool) -> bool {
        debug!("Injecting paste via wtype (is_terminal={})", is_terminal);
        let args: &[&str] = if is_terminal {
            &[
                "-M", "ctrl", "-M", "shift", "v", "-m", "shift", "-m", "ctrl",
            ]
        } else {
            &["-M", "ctrl", "v", "-m", "ctrl"]
        };
        Command::new("wtype")
            .args(args)
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
