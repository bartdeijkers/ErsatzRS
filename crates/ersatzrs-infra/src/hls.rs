use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use tokio::sync::RwLock;

/// In-memory [`HlsSessionRegistry`] backed by an `RwLock<HashMap>`.
///
/// Matches `ErsatzTV`'s `FFmpegSegmenterService` shape: handles are indexed
/// by channel number and looked up via [`HlsSessionRegistry::try_get`].
#[derive(Default)]
pub struct InMemoryHlsSessionRegistry {
    inner: RwLock<HashMap<String, Arc<dyn HlsSessionHandle>>>,
}

impl InMemoryHlsSessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, handle: Arc<dyn HlsSessionHandle>) {
        let mut guard = self.inner.write().await;
        guard.insert(handle.channel_number().to_string(), handle);
    }

    pub async fn remove(&self, channel_number: &str) {
        let mut guard = self.inner.write().await;
        guard.remove(channel_number);
    }
}

#[async_trait]
impl HlsSessionRegistry for InMemoryHlsSessionRegistry {
    async fn try_get(&self, channel_number: &str) -> Option<Arc<dyn HlsSessionHandle>> {
        let guard = self.inner.read().await;
        guard.get(channel_number).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeHandle {
        channel: String,
        touched: AtomicUsize,
    }

    impl HlsSessionHandle for FakeHandle {
        fn channel_number(&self) -> &str {
            &self.channel
        }

        fn touch(&self) {
            self.touched.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn try_get_returns_handle_after_insert() {
        let registry = InMemoryHlsSessionRegistry::new();
        let handle = Arc::new(FakeHandle {
            channel: "42".to_string(),
            touched: AtomicUsize::new(0),
        });
        registry.insert(handle.clone()).await;

        let retrieved = registry.try_get("42").await.expect("handle present");
        retrieved.touch();
        assert_eq!(handle.touched.load(Ordering::SeqCst), 1);

        registry.remove("42").await;
        assert!(registry.try_get("42").await.is_none());
    }

    #[tokio::test]
    async fn try_get_returns_none_for_unknown_channel() {
        let registry = InMemoryHlsSessionRegistry::new();
        assert!(registry.try_get("missing").await.is_none());
    }
}
