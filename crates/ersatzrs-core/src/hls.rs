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

    /// Requests cancellation of the running session.
    ///
    /// Ports [`IHlsSessionWorker.Cancel`]; the concrete worker kills its
    /// `FFmpeg` process and removes itself from the registry.
    ///
    /// [`IHlsSessionWorker.Cancel`]: /home/bart/ErsatzTV/ErsatzTV.Core/Interfaces/FFmpeg/IHlsSessionWorker.cs
    async fn cancel(&self);
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

    /// Channel numbers that currently have a live session.
    ///
    /// Ports the keys of `IFFmpegSegmenterService.Workers`; used by the
    /// `/channels` admin page to decide which rows show a stop button.
    async fn active_channels(&self) -> Vec<String>;

    /// Whether a session is active for `channel_number`.
    ///
    /// Ports `IFFmpegSegmenterService.IsActive`.
    async fn is_active(&self, channel_number: &str) -> bool {
        self.try_get(channel_number).await.is_some()
    }

    /// Cancels the active session for `channel_number`, returning whether a
    /// session was found and cancelled.
    ///
    /// Ports `IFFmpegSegmenterService.StopChannel`.
    async fn stop_channel(&self, channel_number: &str) -> bool {
        if let Some(handle) = self.try_get(channel_number).await {
            handle.cancel().await;
            true
        } else {
            false
        }
    }
}
