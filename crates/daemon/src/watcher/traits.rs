use async_trait::async_trait;
use clipboard_history_core::error::Result;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub struct RawClipboardEvent {
    pub mime_types: Vec<String>,
    pub text: Option<String>,
    pub html: Option<String>,
    pub image_data: Option<Vec<u8>>,
    pub source_app: Option<String>,
}

#[async_trait]
pub trait ClipboardWatcher: Send + Sync {
    async fn run(&mut self, sender: Sender<RawClipboardEvent>) -> Result<()>;
}
