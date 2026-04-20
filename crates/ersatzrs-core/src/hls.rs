use std::sync::Arc;

use async_trait::async_trait;

/// A live handle to a running HLS segmenter worker.
///
/// Ports the parts of [`IHlsSessionWorker`] that the HTTP layer needs today.
/// Wider behaviour — cancellation and state reporting — remains for later
/// slices.
///
/// [`IHlsSessionWorker`]: /home/bart/ErsatzTV/ErsatzTV.Core/Interfaces/FFmpeg/IHlsSessionWorker.cs
#[async_trait]
pub trait HlsSessionHandle: Send + Sync {
    fn channel_number(&self) -> &str;
    fn touch(&self);
    async fn trim_playlist(&self) -> Option<String>;
}

/// Tracks the set of running HLS sessions keyed by channel number.
///
/// `ErsatzTV` exposes this as `IFFmpegSegmenterService.TryGetWorker`. The
/// `ErsatzRS` shape is a trait so the web crate depends on the port while
/// `ersatzrs-infra` owns the concrete in-memory implementation.
#[async_trait]
pub trait HlsSessionRegistry: Send + Sync {
    async fn try_get(&self, channel_number: &str) -> Option<Arc<dyn HlsSessionHandle>>;
    async fn try_insert(&self, handle: Arc<dyn HlsSessionHandle>) -> bool;
    async fn insert(&self, handle: Arc<dyn HlsSessionHandle>);
    async fn remove(&self, channel_number: &str);
}
