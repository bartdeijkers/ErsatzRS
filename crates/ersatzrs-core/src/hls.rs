use std::sync::Arc;

use async_trait::async_trait;

/// Runtime-only retained-media startup gate, shared by master and variant routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlsStartupReadiness {
    /// Ordinary streams retain the configured segment-count readiness policy.
    Legacy,
    /// Intent is not classified yet, or retained media has not built its reserve.
    Pending { deadline: std::time::Instant },
    /// The retained media reserve was completed before the original deadline.
    Ready,
    /// Startup failed; a late encoder result must not reopen the gate.
    Failed,
}

/// Admission state for an HLS segmenter session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlsAdmissionStatus {
    /// The worker is alive but is not currently holding a transcode slot.
    Waiting,
    /// The worker is holding a transcode slot and may have an `FFmpeg` child.
    Transcoding,
    /// The worker needs a transcode slot, but the configured capacity is full.
    AtCapacity,
}

impl HlsAdmissionStatus {
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Waiting => 0,
            Self::Transcoding => 1,
            Self::AtCapacity => 2,
        }
    }

    #[must_use]
    pub const fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Transcoding,
            2 => Self::AtCapacity,
            _ => Self::Waiting,
        }
    }
}

/// A live handle to a running HLS segmenter worker.
///
/// Ports the parts of [`IHlsSessionWorker`] that the HTTP layer needs today.
/// Wider behaviour — cancellation and state reporting — remains for later
/// slices.
///
/// [`IHlsSessionWorker`]: $ERSATZTV_SRC/ErsatzTV.Core/Interfaces/FFmpeg/IHlsSessionWorker.cs
#[async_trait]
pub trait HlsSessionHandle: Send + Sync {
    fn channel_number(&self) -> &str;
    fn touch(&self);
    fn admission_status(&self) -> HlsAdmissionStatus {
        HlsAdmissionStatus::Waiting
    }

    fn startup_readiness(&self) -> HlsStartupReadiness {
        HlsStartupReadiness::Legacy
    }

    async fn trim_playlist(&self) -> Option<String>;

    /// Requests cancellation of the running session.
    ///
    /// Ports [`IHlsSessionWorker.Cancel`]; the concrete worker kills its
    /// `FFmpeg` process and removes itself from the registry.
    ///
    /// [`IHlsSessionWorker.Cancel`]: $ERSATZTV_SRC/ErsatzTV.Core/Interfaces/FFmpeg/IHlsSessionWorker.cs
    async fn cancel(&self);

    /// Marks the active playout as modified so the segmenter seeks on its next
    /// iteration instead of assuming the already-buffered timeline is still
    /// valid.
    ///
    /// Ports [`IHlsSessionWorker.PlayoutUpdated`].
    ///
    /// [`IHlsSessionWorker.PlayoutUpdated`]: $ERSATZTV_SRC/ErsatzTV.Core/Interfaces/Streaming/IHlsSessionWorker.cs
    fn playout_updated(&self);
}

/// Tracks the set of running HLS sessions keyed by channel number.
///
/// `ErsatzTV` exposes this as `IFFmpegSegmenterService.TryGetWorker`. The
/// `ErsatzRS` shape is a trait so the web crate depends on the port while
/// `ersatzrs-infra` owns the concrete in-memory implementation.
#[async_trait]
pub trait HlsSessionRegistry: Send + Sync {
    /// Find only a playlist-producing worker, never a direct response process.
    async fn try_get(&self, channel_number: &str) -> Option<Arc<dyn HlsSessionHandle>>;
    async fn try_insert(&self, handle: Arc<dyn HlsSessionHandle>) -> bool;
    async fn insert(&self, handle: Arc<dyn HlsSessionHandle>);
    async fn remove(&self, channel_number: &str);

    /// Remove a worker only if this exact handle still owns its channel slot.
    /// Late cleanup must never unregister a replacement worker.
    async fn remove_if_current(&self, handle: &Arc<dyn HlsSessionHandle>);

    /// Read cached readiness only for this exact registered worker.
    /// None means the worker has ended or has been replaced.
    async fn initial_segments_ready(&self, handle: &Arc<dyn HlsSessionHandle>) -> Option<bool> {
        self.try_get(handle.channel_number())
            .await
            .filter(|current| Arc::ptr_eq(current, handle))
            .map(|_| false)
    }

    /// Publish readiness only while this exact worker still owns its slot.
    async fn mark_initial_segments_ready(&self, handle: &Arc<dyn HlsSessionHandle>) -> bool {
        self.try_get(handle.channel_number())
            .await
            .is_some_and(|current| Arc::ptr_eq(&current, handle))
    }

    /// Track a direct HTTP response for activity reporting and channel Stop.
    /// Direct responses must not occupy or replace the HLS worker slot.
    async fn register_direct(&self, handle: Arc<dyn HlsSessionHandle>);

    /// Remove only this direct response, preserving other viewers and workers.
    async fn unregister_direct(&self, handle: &Arc<dyn HlsSessionHandle>);

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

    /// Notifies an active session that its playout was rebuilt.
    ///
    /// Ports `IFFmpegSegmenterService.PlayoutUpdated`.
    async fn playout_updated(&self, channel_number: &str) -> bool {
        if let Some(handle) = self.try_get(channel_number).await {
            handle.playout_updated();
            true
        } else {
            false
        }
    }
}
