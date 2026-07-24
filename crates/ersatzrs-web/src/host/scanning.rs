use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_app::{
    commands::{ExecuteScanOutcome, ScanWorkRequest},
    ApplicationServices, ScanRuntime, ScanRuntimeError, ScanWorkQueue, ScanWorkQueueError,
};
use ersatzrs_domain::{EntityId, MediaSourceKind, ScanProgressSink};
use ersatzrs_infra::db::DbPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Tokio adapter for the application-owned FIFO scan admission boundary.
#[derive(Clone)]
pub struct TokioScanWorkQueue {
    sender: mpsc::UnboundedSender<ScanWorkRequest>,
}

impl TokioScanWorkQueue {
    #[must_use]
    pub const fn new(sender: mpsc::UnboundedSender<ScanWorkRequest>) -> Self {
        Self { sender }
    }
}

impl ScanWorkQueue for TokioScanWorkQueue {
    fn enqueue(&self, request: ScanWorkRequest) -> Result<(), ScanWorkQueueError> {
        self.sender
            .send(request)
            .map_err(|error| ScanWorkQueueError::unavailable(error.to_string()))
    }
}

/// Native provider/local scanner adapter assembled by the web composition root.
#[derive(Clone)]
pub struct NativeScanRuntime {
    pool: DbPool,
}

impl NativeScanRuntime {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScanRuntime for NativeScanRuntime {
    async fn scan_library(
        &self,
        provider: MediaSourceKind,
        library_id: EntityId,
        deep_scan: bool,
        progress: &dyn ScanProgressSink,
    ) -> Result<(), ScanRuntimeError> {
        match provider {
            MediaSourceKind::Plex => ersatzrs_infra::plex::sync_library_by_id_with_progress(
                &self.pool,
                library_id,
                deep_scan,
                Some(progress),
            )
            .await
            .map(|_| ())
            .map_err(scan_error),
            MediaSourceKind::Jellyfin => {
                ersatzrs_infra::jellyfin::sync_library_by_id_with_progress(
                    &self.pool,
                    library_id,
                    deep_scan,
                    Some(progress),
                )
                .await
                .map(|_| ())
                .map_err(scan_error)
            }
            MediaSourceKind::Emby => ersatzrs_infra::emby::sync_library_by_id_with_progress(
                &self.pool,
                library_id,
                deep_scan,
                Some(progress),
            )
            .await
            .map(|_| ())
            .map_err(scan_error),
            MediaSourceKind::Local => {
                let config = ersatzrs_scanner::LocalScanConfig {
                    library_id,
                    ffprobe_path: ersatzrs_ffmpeg::ffprobe_path(),
                };
                ersatzrs_scanner::scan_local_library_with_progress(
                    &self.pool,
                    &config,
                    Some(progress),
                )
                .await
                .map(|_| ())
                .map_err(scan_error)
            }
        }
    }

    async fn scan_show(
        &self,
        provider: MediaSourceKind,
        library_id: EntityId,
        show_id: EntityId,
        deep_scan: bool,
        progress: &dyn ScanProgressSink,
    ) -> Result<(), ScanRuntimeError> {
        match provider {
            MediaSourceKind::Plex => ersatzrs_infra::plex::sync_plex_show_by_id(
                &self.pool,
                library_id,
                show_id,
                deep_scan,
                Some(progress),
            )
            .await
            .map(|_| ())
            .map_err(scan_error),
            MediaSourceKind::Jellyfin => ersatzrs_infra::jellyfin::sync_jellyfin_show_by_id(
                &self.pool,
                library_id,
                show_id,
                deep_scan,
                Some(progress),
            )
            .await
            .map(|_| ())
            .map_err(scan_error),
            MediaSourceKind::Emby => ersatzrs_infra::emby::sync_emby_show_by_id(
                &self.pool,
                library_id,
                show_id,
                deep_scan,
                Some(progress),
            )
            .await
            .map(|_| ())
            .map_err(scan_error),
            MediaSourceKind::Local => Err(ScanRuntimeError::new(
                "local show scans must fall back to full library scans before execution",
            )),
        }
    }

    async fn extract_embedded_show_subtitles(
        &self,
        show_id: EntityId,
    ) -> Result<(), ScanRuntimeError> {
        ersatzrs_infra::subtitles::extract_embedded_show_subtitles(&self.pool, show_id)
            .await
            .map_err(scan_error)
    }
}

fn scan_error(error: impl std::fmt::Display) -> ScanRuntimeError {
    ScanRuntimeError::new(error.to_string())
}

struct WorkerLifecycle {
    application: Arc<ApplicationServices>,
}

impl Drop for WorkerLifecycle {
    fn drop(&mut self) {
        self.application
            .scan_worker_stopped("scanner worker stopped");
    }
}

/// Consume the one process-wide FIFO scan queue until coordinated shutdown.
pub async fn run_scan_worker(
    mut receiver: mpsc::UnboundedReceiver<ScanWorkRequest>,
    application: Arc<ApplicationServices>,
    shutdown: CancellationToken,
) {
    application.scan_worker_started();
    let _lifecycle = WorkerLifecycle {
        application: application.clone(),
    };
    tracing::info!("scanner worker started");

    loop {
        let work = tokio::select! {
            () = shutdown.cancelled() => break,
            work = receiver.recv() => {
                let Some(work) = work else {
                    break;
                };
                work
            }
        };
        let operation_id = work.operation_id.0;
        let library_id = work.request.library_id();
        let outcome = tokio::select! {
            () = shutdown.cancelled() => break,
            outcome = application.execute_scan_work(work) => outcome,
        };
        match outcome {
            ExecuteScanOutcome::Completed => {
                tracing::info!(operation_id, library_id, "scan operation completed");
            }
            ExecuteScanOutcome::Failed { message } => {
                tracing::warn!(
                    operation_id,
                    library_id,
                    error = %message,
                    "scan operation failed"
                );
            }
            ExecuteScanOutcome::Missing => {
                tracing::warn!(
                    operation_id,
                    library_id,
                    "scan queue token has no queued application operation"
                );
            }
        }
    }

    tracing::info!("scanner worker shutting down");
}
