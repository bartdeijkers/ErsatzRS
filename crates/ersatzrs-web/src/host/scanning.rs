use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_app::{
    commands::{ExecuteScanOutcome, ScanWorkRequest},
    ApplicationServices, ScanPlayoutRefresh, ScanRuntime, ScanRuntimeError, ScanRuntimeReport,
    ScanWorkQueue, ScanWorkQueueError,
};
use ersatzrs_domain::{EntityId, MediaSourceKind, ScanProgressSink};
use ersatzrs_infra::db::DbPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::warn;

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

    async fn scan_local_library_report(
        &self,
        library_id: EntityId,
        progress: &dyn ScanProgressSink,
    ) -> Result<ersatzrs_scanner::LocalScanReport, ScanRuntimeError> {
        let config = ersatzrs_scanner::LocalScanConfig {
            library_id,
            ffprobe_path: ersatzrs_ffmpeg::ffprobe_path(),
        };
        ersatzrs_scanner::scan_local_library_with_progress(&self.pool, &config, Some(progress))
            .await
            .map_err(scan_error)
    }

    async fn match_all_addon_media_lists(
        &self,
    ) -> Result<ersatzrs_infra::addon_media_list_sync::AddonMediaListMatchReport, ScanRuntimeError>
    {
        ersatzrs_infra::addon_media_list_sync::AddonMediaListSynchronizer::new(self.pool.clone())
            .match_all_lists_with_report()
            .await
            .map_err(scan_error)
    }
}

#[async_trait]
impl ScanRuntime for NativeScanRuntime {
    async fn recover_playout_refreshes(&self) -> Result<ScanRuntimeReport, ScanRuntimeError> {
        let interrupted = ersatzrs_infra::scan_refresh::interrupted_resolutions(&self.pool)
            .await
            .map_err(scan_error)?;
        for resolution in interrupted {
            let replay = async {
                let report = self
                    .scan_local_library_report(resolution.library_id, &())
                    .await?;
                if report.failed_files > 0 {
                    return Err(ScanRuntimeError::new(format!(
                        "interrupted Local scan replay reported {} failed file(s)",
                        report.failed_files
                    )));
                }
                self.match_all_addon_media_lists().await?;
                Ok(())
            }
            .await;
            match replay {
                Ok(()) => {
                    ersatzrs_infra::scan_refresh::complete_interrupted_replay(
                        &self.pool,
                        resolution.library_id,
                        resolution.generation,
                    )
                    .await
                    .map_err(scan_error)?;
                }
                Err(error) => {
                    if let Err(state_error) = ersatzrs_infra::scan_refresh::fail_interrupted_replay(
                        &self.pool,
                        resolution.library_id,
                        resolution.generation,
                    )
                    .await
                    {
                        warn!(
                            library_id = resolution.library_id,
                            error = %state_error,
                            "failed to retain interrupted scan replay failure"
                        );
                    }
                    warn!(
                        library_id = resolution.library_id,
                        %error,
                        "interrupted scan-impact resolution replay failed"
                    );
                }
            }
        }
        let refreshes = ersatzrs_infra::scan_refresh::recoverable_refreshes(&self.pool)
            .await
            .map_err(scan_error)?;
        Ok(ScanRuntimeReport {
            playout_refreshes: refreshes
                .into_iter()
                .map(|refresh| ScanPlayoutRefresh {
                    playout_id: refresh.playout_id,
                    reason: refresh.reason,
                })
                .collect(),
        })
    }

    async fn pending_playout_refreshes(&self) -> Result<ScanRuntimeReport, ScanRuntimeError> {
        let refreshes = ersatzrs_infra::scan_refresh::pending_refreshes(&self.pool)
            .await
            .map_err(scan_error)?;
        Ok(ScanRuntimeReport {
            playout_refreshes: refreshes
                .into_iter()
                .map(|refresh| ScanPlayoutRefresh {
                    playout_id: refresh.playout_id,
                    reason: refresh.reason,
                })
                .collect(),
        })
    }

    async fn scan_library(
        &self,
        provider: MediaSourceKind,
        library_id: EntityId,
        deep_scan: bool,
        progress: &dyn ScanProgressSink,
    ) -> Result<ScanRuntimeReport, ScanRuntimeError> {
        match provider {
            MediaSourceKind::Plex => ersatzrs_infra::plex::sync_library_by_id_with_progress(
                &self.pool,
                library_id,
                deep_scan,
                Some(progress),
            )
            .await
            .map(|_| ScanRuntimeReport::default())
            .map_err(scan_error),
            MediaSourceKind::Jellyfin => {
                ersatzrs_infra::jellyfin::sync_library_by_id_with_progress(
                    &self.pool,
                    library_id,
                    deep_scan,
                    Some(progress),
                )
                .await
                .map(|_| ScanRuntimeReport::default())
                .map_err(scan_error)
            }
            MediaSourceKind::Emby => ersatzrs_infra::emby::sync_library_by_id_with_progress(
                &self.pool,
                library_id,
                deep_scan,
                Some(progress),
            )
            .await
            .map(|_| ScanRuntimeReport::default())
            .map_err(scan_error),
            MediaSourceKind::Local => {
                let mut report = self.scan_local_library_report(library_id, progress).await?;
                let match_report = self.match_all_addon_media_lists().await?;
                report
                    .playout_refreshes
                    .extend(match_report.playout_refreshes);
                Ok(ScanRuntimeReport {
                    playout_refreshes: report.playout_refreshes,
                })
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

#[cfg(all(test, not(feature = "mysql")))]
mod tests {
    use super::*;
    use ersatzrs_domain::ScanPlayoutRefreshReason;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor as _;

    async fn seed_local_library(pool: &DbPool, root: &std::path::Path, media_kind: i32) {
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .await
            .expect("seed media source");
        pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .await
            .expect("seed Local media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Interrupted scan', ?, 1)"#,
        )
        .bind(media_kind)
        .execute(pool)
        .await
        .expect("seed Local library");
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .await
            .expect("seed Local library subtype");
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
            .bind(root.display().to_string())
            .execute(pool)
            .await
            .expect("seed Local library path");
    }

    #[tokio::test]
    async fn startup_replays_and_completes_an_interrupted_local_scan_marker() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect SQLite");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate SQLite");
        let library_root = tempfile::tempdir().expect("temporary empty Local library");
        seed_local_library(&pool, library_root.path(), 1).await;
        ersatzrs_infra::scan_refresh::begin_resolution(
            &pool,
            1,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        )
        .await
        .expect("persist interrupted scan marker");

        let report = NativeScanRuntime::new(pool.clone())
            .recover_playout_refreshes()
            .await
            .expect("replay interrupted Local scan");

        assert!(report.playout_refreshes.is_empty());
        assert!(ersatzrs_infra::scan_refresh::interrupted_resolutions(&pool)
            .await
            .expect("load remaining interrupted markers")
            .is_empty());
        let state: i64 = sqlx::query_scalar(
            r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load completed replay marker");
        assert_eq!(state, 1);
    }

    #[tokio::test]
    async fn startup_retains_a_failed_marker_when_interrupted_replay_has_file_failures() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect SQLite");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate SQLite");
        let library_root = tempfile::tempdir().expect("temporary Remote Streams library");
        std::fs::write(
            library_root.path().join("Broken.playlist.yml"),
            "version: [",
        )
        .expect("write malformed playlist manifest");
        seed_local_library(&pool, library_root.path(), 7).await;
        ersatzrs_infra::scan_refresh::begin_resolution(
            &pool,
            1,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        )
        .await
        .expect("persist interrupted scan marker");

        let report = NativeScanRuntime::new(pool.clone())
            .recover_playout_refreshes()
            .await
            .expect("recover other durable refreshes after replay failure");

        assert!(report.playout_refreshes.is_empty());
        let state: i64 = sqlx::query_scalar(
            r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load failed replay marker");
        assert_eq!(state, 2);
        let diagnostic: Option<String> = sqlx::query_scalar(
            r#"SELECT "Diagnostic" FROM "ScanRefreshResolution" WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load failed replay diagnostic");
        assert_eq!(diagnostic.as_deref(), Some("scan-impact-resolution-failed"));
    }
}
