use std::sync::Arc;

use async_trait::async_trait;

/// A live handle to a running HLS segmenter worker.
///
/// Ports the parts of [`IHlsSessionWorker`] that the HTTP layer needs today.
/// Wider behaviour — trimming, cancellation, state reporting — will be added
/// as the worker itself lands in a later slice.
///
/// [`IHlsSessionWorker`]: /home/bart/ErsatzTV/ErsatzTV.Core/Interfaces/FFmpeg/IHlsSessionWorker.cs
pub trait HlsSessionHandle: Send + Sync {
    fn channel_number(&self) -> &str;
    fn touch(&self);
}

/// Tracks the set of running HLS sessions keyed by channel number.
///
/// `ErsatzTV` exposes this as `IFFmpegSegmenterService.TryGetWorker`. The
/// `ErsatzRS` shape is a trait so the web crate depends on the port while
/// `ersatzrs-infra` owns the concrete in-memory implementation.
#[async_trait]
pub trait HlsSessionRegistry: Send + Sync {
    async fn try_get(&self, channel_number: &str) -> Option<Arc<dyn HlsSessionHandle>>;
}
