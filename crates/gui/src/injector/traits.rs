use async_trait::async_trait;

#[async_trait]
pub trait PasteInjector: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    async fn inject(&self) -> bool;
}
