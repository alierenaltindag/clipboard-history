use crate::injector::traits::PasteInjector;
use async_trait::async_trait;
use tracing::info;

pub struct CopyOnlyInjector;

impl CopyOnlyInjector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CopyOnlyInjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PasteInjector for CopyOnlyInjector {
    fn name(&self) -> &'static str {
        "copy-only-fallback"
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn inject(&self) -> bool {
        info!("Item copied to system clipboard. Ready for manual paste (Ctrl+V).");
        true
    }
}
