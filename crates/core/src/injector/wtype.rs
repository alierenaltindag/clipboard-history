use crate::injector::traits::PasteInjector;
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

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
        let mut cmd = Command::new("wtype");
        cmd.args(args).stdin(Stdio::null()).kill_on_drop(true);

        match tokio::time::timeout(Duration::from_secs(2), cmd.status()).await {
            Ok(Ok(status)) => status.success(),
            Ok(Err(e)) => {
                debug!("wtype execution failed: {}", e);
                false
            }
            Err(_) => {
                warn!("wtype timed out after 2s");
                false
            }
        }
    }
}
