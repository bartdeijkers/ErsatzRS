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
    playout_updates_to: Option<Arc<dyn HlsSessionRegistry>>,
}

impl InMemoryHlsSessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forward rebuild notifications to an independently owned preview registry.
    /// Session lookup, admission and cancellation remain isolated.
    #[must_use]
    pub fn with_playout_updates_to(previews: Arc<dyn HlsSessionRegistry>) -> Self {
        Self {
            playout_updates_to: Some(previews),
            ..Self::default()
        }
    }
}

#[async_trait]
impl HlsSessionRegistry for InMemoryHlsSessionRegistry {
    async fn playout_updated(&self, channel_number: &str) -> bool {
        let mut notified = false;
        if let Some(handle) = self.try_get(channel_number).await {
            handle.playout_updated();
            notified = true;
        }
        if let Some(previews) = &self.playout_updates_to {
            notified |= previews.playout_updated(channel_number).await;
        }
        notified
    }

    async fn try_get(&self, channel_number: &str) -> Option<Arc<dyn HlsSessionHandle>> {
        let guard = self.inner.read().await;
        guard.get(channel_number).cloned()
    }

    async fn try_insert(&self, handle: Arc<dyn HlsSessionHandle>) -> bool {
        let mut guard = self.inner.write().await;
        let key = handle.channel_number().to_string();
        if guard.contains_key(&key) {
            return false;
        }

        guard.insert(key, handle);
        true
    }

    async fn insert(&self, handle: Arc<dyn HlsSessionHandle>) {
        let mut guard = self.inner.write().await;
        guard.insert(handle.channel_number().to_string(), handle);
    }

    async fn remove(&self, channel_number: &str) {
        let mut guard = self.inner.write().await;
        guard.remove(channel_number);
    }

    async fn active_channels(&self) -> Vec<String> {
        let guard = self.inner.read().await;
        guard.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeHandle {
        channel: String,
        touched: AtomicUsize,
        cancelled: AtomicUsize,
        playout_updated: AtomicUsize,
    }

    impl FakeHandle {
        fn new(channel: &str) -> Arc<Self> {
            Arc::new(Self {
                channel: channel.to_string(),
                touched: AtomicUsize::new(0),
                cancelled: AtomicUsize::new(0),
                playout_updated: AtomicUsize::new(0),
            })
        }
    }

    #[async_trait]
    impl HlsSessionHandle for FakeHandle {
        fn channel_number(&self) -> &str {
            &self.channel
        }

        fn touch(&self) {
            self.touched.fetch_add(1, Ordering::SeqCst);
        }

        async fn trim_playlist(&self) -> Option<String> {
            None
        }

        async fn cancel(&self) {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        }

        fn playout_updated(&self) {
            self.playout_updated.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn rebuild_reaches_preview_without_a_normal_session_and_keeps_lifetimes_separate() {
        let previews = Arc::new(InMemoryHlsSessionRegistry::new());
        let normal = InMemoryHlsSessionRegistry::with_playout_updates_to(previews.clone());
        let preview = FakeHandle::new("42");
        previews.insert(preview.clone()).await;
        assert!(normal.playout_updated("42").await);
        assert_eq!(preview.playout_updated.load(Ordering::SeqCst), 1);
        assert!(!normal.stop_channel("42").await);
        assert!(normal.active_channels().await.is_empty());
        let live = FakeHandle::new("42");
        normal.insert(live.clone()).await;
        assert!(normal.playout_updated("42").await);
        assert_eq!(live.playout_updated.load(Ordering::SeqCst), 1);
        assert_eq!(preview.playout_updated.load(Ordering::SeqCst), 2);
        assert!(normal.stop_channel("42").await);
        assert_eq!(preview.cancelled.load(Ordering::SeqCst), 0);
        assert!(!normal.playout_updated("absent").await);
    }

    #[tokio::test]
    async fn try_get_returns_handle_after_insert() {
        let registry = InMemoryHlsSessionRegistry::new();
        let handle = FakeHandle::new("42");
        registry.insert(handle.clone()).await;

        let retrieved = registry.try_get("42").await.expect("handle present");
        retrieved.touch();
        assert_eq!(handle.touched.load(Ordering::SeqCst), 1);

        registry.remove("42").await;
        assert!(registry.try_get("42").await.is_none());
    }

    #[tokio::test]
    async fn try_insert_returns_false_when_channel_exists() {
        let registry = InMemoryHlsSessionRegistry::new();
        let first = FakeHandle::new("42");
        let second = FakeHandle::new("42");

        assert!(registry.try_insert(first).await);
        assert!(!registry.try_insert(second).await);
    }

    #[tokio::test]
    async fn is_active_and_active_channels_track_registered_sessions() {
        let registry = InMemoryHlsSessionRegistry::new();
        assert!(!registry.is_active("7").await);
        assert!(registry.active_channels().await.is_empty());

        registry.insert(FakeHandle::new("7")).await;
        assert!(registry.is_active("7").await);
        assert_eq!(registry.active_channels().await, vec!["7".to_string()]);

        registry.remove("7").await;
        assert!(!registry.is_active("7").await);
        assert!(registry.active_channels().await.is_empty());
    }

    #[tokio::test]
    async fn stop_channel_cancels_handle_and_reports_presence() {
        let registry = InMemoryHlsSessionRegistry::new();
        let handle = FakeHandle::new("9");
        registry.insert(handle.clone()).await;

        assert!(registry.stop_channel("9").await);
        assert_eq!(handle.cancelled.load(Ordering::SeqCst), 1);

        // Unknown channels report no session to stop.
        assert!(!registry.stop_channel("missing").await);
    }

    #[tokio::test]
    async fn playout_updated_notifies_active_handle_and_reports_presence() {
        let registry = InMemoryHlsSessionRegistry::new();
        let handle = FakeHandle::new("12");
        registry.insert(handle.clone()).await;

        assert!(registry.playout_updated("12").await);
        assert_eq!(handle.playout_updated.load(Ordering::SeqCst), 1);
        assert!(!registry.playout_updated("missing").await);
    }

    #[tokio::test]
    async fn try_get_returns_none_for_unknown_channel() {
        let registry = InMemoryHlsSessionRegistry::new();
        assert!(registry.try_get("missing").await.is_none());
    }
}
