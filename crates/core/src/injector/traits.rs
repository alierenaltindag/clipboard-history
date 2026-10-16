use async_trait::async_trait;

#[async_trait]
pub trait PasteInjector: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    async fn inject(&self) -> bool {
        self.inject_context(false).await
    }
    async fn inject_context(&self, _is_terminal: bool) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockInjector;

    #[async_trait]
    impl PasteInjector for MockInjector {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn is_available(&self) -> bool {
            true
        }
        async fn inject_context(&self, is_terminal: bool) -> bool {
            !is_terminal
        }
    }

    #[tokio::test]
    async fn test_default_inject_delegates_to_inject_context_false() {
        let mock = MockInjector;
        // Default inject() should call inject_context(false) without mutual recursion
        assert!(mock.inject().await);
        assert!(!mock.inject_context(true).await);
    }
}
