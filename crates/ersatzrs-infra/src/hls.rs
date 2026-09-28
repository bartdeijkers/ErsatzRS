use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use tokio::sync::RwLock;

/// In-memory [`HlsSessionRegistry`] backed by an `RwLock<HashMap>`.
///
/// Matches `ErsatzTV`'s `FFmpegSegmenterService` shape: handles are indexed
/// by channel number and looked up via [`HlsSessionRegistry::try_get`].
/// Direct HTTP responses have independent registrations for channel activity
/// and cancellation; they cannot occupy a playlist worker's slot.
#[derive(Default)]
pub struct InMemoryHlsSessionRegistry {
    inner: RwLock<HashMap<String, RegisteredSession>>,
    direct: RwLock<HashMap<String, Vec<Arc<dyn HlsSessionHandle>>>>,
    playout_updates_to: Option<Arc<dyn HlsSessionRegistry>>,
}

struct RegisteredSession {
    handle: Arc<dyn HlsSessionHandle>,
    ready: bool,
}

impl InMemoryHlsSessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    async fn channel_handles(&self, channel_number: &str) -> Vec<Arc<dyn HlsSessionHandle>> {
        let mut handles = self
            .direct
            .read()
            .await
            .get(channel_number)
            .cloned()
            .unwrap_or_default();
        if let Some(worker) = self.try_get(channel_number).await {
            handles.push(worker);
        }
        handles
    }

    /// Forward rebuild notifications to an independently owned preview registry.
    /// Session lookup, admission and cancellation remain isolated.
    #[must_use]
    pub fn with_playout_updates_to(previews: Arc<dyn HlsSessionRegistry>) -> Self {
        Self {
            direct: RwLock::new(HashMap::new()),
            playout_updates_to: Some(previews),
            ..Self::default()
        }
    }
}

#[async_trait]
impl HlsSessionRegistry for InMemoryHlsSessionRegistry {
    async fn playout_updated(&self, channel_number: &str) -> bool {
        let mut notified = false;
        for handle in self.channel_handles(channel_number).await {
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
        guard
            .get(channel_number)
            .map(|session| session.handle.clone())
    }

    async fn try_insert(&self, handle: Arc<dyn HlsSessionHandle>) -> bool {
        let mut guard = self.inner.write().await;
        let key = handle.channel_number().to_string();
        if guard.contains_key(&key) {
            return false;
        }

        guard.insert(
            key,
            RegisteredSession {
                handle,
                ready: false,
            },
        );
        true
    }

    async fn insert(&self, handle: Arc<dyn HlsSessionHandle>) {
        let mut guard = self.inner.write().await;
        guard.insert(
            handle.channel_number().to_string(),
            RegisteredSession {
                handle,
                ready: false,
            },
        );
    }

    async fn remove(&self, channel_number: &str) {
        let mut guard = self.inner.write().await;
        guard.remove(channel_number);
    }

    async fn remove_if_current(&self, handle: &Arc<dyn HlsSessionHandle>) {
        let mut guard = self.inner.write().await;
        if guard
            .get(handle.channel_number())
            .is_some_and(|registered| Arc::ptr_eq(&registered.handle, handle))
        {
            guard.remove(handle.channel_number());
        }
    }

    async fn initial_segments_ready(&self, handle: &Arc<dyn HlsSessionHandle>) -> Option<bool> {
        self.inner
            .read()
            .await
            .get(handle.channel_number())
            .filter(|session| Arc::ptr_eq(&session.handle, handle))
            .map(|session| session.ready)
    }

    async fn mark_initial_segments_ready(&self, handle: &Arc<dyn HlsSessionHandle>) -> bool {
        let mut sessions = self.inner.write().await;
        let Some(session) = sessions
            .get_mut(handle.channel_number())
            .filter(|session| Arc::ptr_eq(&session.handle, handle))
        else {
            return false;
        };
        session.ready = true;
        true
    }

    async fn register_direct(&self, handle: Arc<dyn HlsSessionHandle>) {
        self.direct
            .write()
            .await
            .entry(handle.channel_number().to_owned())
            .or_default()
            .push(handle);
    }

    async fn unregister_direct(&self, handle: &Arc<dyn HlsSessionHandle>) {
        let mut direct = self.direct.write().await;
        if let Some(handles) = direct.get_mut(handle.channel_number()) {
            handles.retain(|registered| !Arc::ptr_eq(registered, handle));
            if handles.is_empty() {
                direct.remove(handle.channel_number());
            }
        }
    }

    async fn active_channels(&self) -> Vec<String> {
        let mut channels: BTreeSet<_> = self.inner.read().await.keys().cloned().collect();
        channels.extend(self.direct.read().await.keys().cloned());
        channels.into_iter().collect()
    }

    async fn session_models(&self) -> Vec<ersatzrs_core::hls::HlsSessionModel> {
        let mut reports: Vec<_> = self
            .inner
            .read()
            .await
            .values()
            .filter_map(|session| session.handle.session_model())
            .collect();
        reports.sort_by(|left, right| left.channel_number.cmp(&right.channel_number));
        reports
    }

    async fn is_active(&self, channel_number: &str) -> bool {
        if self.inner.read().await.contains_key(channel_number) {
            return true;
        }
        self.direct.read().await.contains_key(channel_number)
    }

    async fn stop_channel(&self, channel_number: &str) -> bool {
        let handles = self.channel_handles(channel_number).await;
        let found = !handles.is_empty();
        // Release registry locks before calling into a worker's cancellation path.
        for handle in handles {
            handle.cancel().await;
        }
        found
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
    async fn stale_worker_cleanup_preserves_replacement_and_direct_viewers() {
        let registry = InMemoryHlsSessionRegistry::new();
        let first: Arc<dyn HlsSessionHandle> = FakeHandle::new("7");
        let replacement: Arc<dyn HlsSessionHandle> = FakeHandle::new("7");
        let viewer: Arc<dyn HlsSessionHandle> = FakeHandle::new("7");
        assert!(registry.try_insert(first.clone()).await);
        registry.register_direct(viewer.clone()).await;
        registry.remove_if_current(&first).await;
        assert!(registry.try_get("7").await.is_none());
        assert!(registry.try_insert(replacement.clone()).await);
        registry.remove_if_current(&first).await;
        assert!(Arc::ptr_eq(
            &registry.try_get("7").await.unwrap(),
            &replacement
        ));
        registry.remove_if_current(&replacement).await;
        registry.remove_if_current(&replacement).await;
        assert!(registry.try_get("7").await.is_none());
        assert!(registry.is_active("7").await);
        registry.unregister_direct(&viewer).await;
        assert!(!registry.is_active("7").await);
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
    async fn direct_viewers_and_hls_workers_have_independent_admission_and_cleanup() {
        let registry = InMemoryHlsSessionRegistry::new();
        let first: Arc<dyn HlsSessionHandle> = FakeHandle::new("7");
        let second: Arc<dyn HlsSessionHandle> = FakeHandle::new("7");
        registry.register_direct(first.clone()).await;
        registry.register_direct(second.clone()).await;
        assert!(registry.is_active("7").await);
        assert!(registry.try_get("7").await.is_none());
        let worker: Arc<dyn HlsSessionHandle> = FakeHandle::new("7");
        assert!(registry.try_insert(worker.clone()).await);
        assert_eq!(registry.active_channels().await, vec!["7"]);
        registry.unregister_direct(&first).await;
        registry.unregister_direct(&first).await;
        assert!(Arc::ptr_eq(&registry.try_get("7").await.unwrap(), &worker));
        registry.remove("7").await;
        assert!(registry.try_get("7").await.is_none());
        assert!(registry.is_active("7").await);
        registry.unregister_direct(&second).await;
        assert!(!registry.is_active("7").await);
        assert!(registry.active_channels().await.is_empty());
    }

    #[tokio::test]
    async fn channel_stop_and_rebuild_reach_all_direct_viewers_and_the_worker() {
        let registry = InMemoryHlsSessionRegistry::new();
        let first = FakeHandle::new("7");
        let second = FakeHandle::new("7");
        let worker = FakeHandle::new("7");
        let other = FakeHandle::new("8");
        registry.register_direct(first.clone()).await;
        registry.register_direct(second.clone()).await;
        registry.register_direct(other.clone()).await;
        assert!(registry.try_insert(worker.clone()).await);
        assert!(registry.playout_updated("7").await);
        assert!(registry.stop_channel("7").await);
        for handle in [&first, &second, &worker] {
            assert_eq!(handle.playout_updated.load(Ordering::SeqCst), 1);
            assert_eq!(handle.cancelled.load(Ordering::SeqCst), 1);
        }
        assert_eq!(other.playout_updated.load(Ordering::SeqCst), 0);
        assert_eq!(other.cancelled.load(Ordering::SeqCst), 0);
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
