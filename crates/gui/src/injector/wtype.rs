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

#[async_trait]
impl PasteInjector for WtypeInjector {
    fn name(&self) -> &'static str {
        "wtype"
    }

    fn is_available(&self) -> bool {
        which::which("wtype").is_ok()
    }

    async fn inject(&self) -> bool {
        debug!("Injecting paste via wtype");
        Command::new("wtype")
            .args(["-M", "ctrl", "v", "-m", "ctrl"])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
