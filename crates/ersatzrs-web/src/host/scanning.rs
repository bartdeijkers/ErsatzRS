use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_app::{
    commands::{ExecuteManagedLibraryOperationOutcome, ScanWorkRequest},
    ApplicationServices, ScanPlayoutRefresh, ScanRuntime, ScanRuntimeError, ScanRuntimeReport,
    ScanWorkQueue, ScanWorkQueueError,
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
    async fn preview_television_folders(
        &self,
        paths: Vec<String>,
    ) -> Result<Vec<ersatzrs_core::television_discovery::TelevisionFolderPreview>, ScanRuntimeError>
    {
        static CAPACITY: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> =
            std::sync::OnceLock::new();
        let permit = CAPACITY
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(2)))
            .clone()
            .try_acquire_owned()
            .map_err(|_| ScanRuntimeError::new("Folder preview capacity is busy"))?;
        let task = tokio::task::spawn_blocking(move || {
            // An OS filesystem call cannot be cancelled. Keep the slot owned
            // by the worker even if its HTTP reader times out or disconnects.
            let _permit = permit;
            ersatzrs_scanner::local_television::discovery::preview_television_folders(&paths)
        });
        tokio::time::timeout(std::time::Duration::from_secs(3), task)
            .await
            .map_err(|_| ScanRuntimeError::new("Folder preview timed out"))?
            .map_err(|_| ScanRuntimeError::new("Folder preview worker failed"))
    }

    async fn interrupted_scan_library_ids(&self) -> Result<Vec<EntityId>, ScanRuntimeError> {
        ersatzrs_infra::scan_refresh::interrupted_resolutions(&self.pool)
            .await
            .map(|resolutions| {
                resolutions
                    .into_iter()
                    .map(|resolution| resolution.library_id)
                    .collect()
            })
            .map_err(scan_error)
    }

    async fn recover_playout_refreshes(&self) -> Result<ScanRuntimeReport, ScanRuntimeError> {
        ersatzrs_infra::provider_collection_refresh::resolve_pending(&self.pool)
            .await
            .map_err(scan_error)?;
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
        ersatzrs_infra::provider_collection_refresh::resolve_pending(&self.pool)
            .await
            .map_err(scan_error)?;
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
                let interrupted_generation =
                    ersatzrs_infra::scan_refresh::interrupted_resolutions(&self.pool)
                        .await
                        .map_err(scan_error)?
                        .into_iter()
                        .find(|resolution| resolution.library_id == library_id)
                        .map(|resolution| resolution.generation);
                let result = async {
                    let mut report = self.scan_local_library_report(library_id, progress).await?;
                    if report.failed_files > 0 {
                        return Err(ScanRuntimeError::new(format!(
                            "Local scan reported {} failed file(s)",
                            report.failed_files
                        )));
                    }
                    let match_report = self.match_all_addon_media_lists().await?;
                    report
                        .playout_refreshes
                        .extend(match_report.playout_refreshes);
                    Ok(ScanRuntimeReport {
                        playout_refreshes: report.playout_refreshes,
                    })
                }
                .await;
                if let Some(generation) = interrupted_generation {
                    let persisted = if result.is_ok() {
                        ersatzrs_infra::scan_refresh::complete_interrupted_replay(
                            &self.pool, library_id, generation,
                        )
                        .await
                    } else {
                        ersatzrs_infra::scan_refresh::fail_interrupted_replay(
                            &self.pool, library_id, generation,
                        )
                        .await
                    };
                    persisted.map_err(scan_error)?;
                }
                result
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

fn heartbeat_keeps_operation_running<E>(result: &Result<bool, E>) -> bool {
    matches!(result, Ok(true))
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

/// Claim durable managed-library work until coordinated shutdown.
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

    let mut poll = tokio::time::interval(std::time::Duration::from_millis(250));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut receiver_closed = false;

    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = poll.tick() => {}
            work = receiver.recv(), if !receiver_closed => {
                receiver_closed = work.is_none();
            }
        };

        let claim = match application.claim_managed_library_operation().await {
            Ok(Some(claim)) => claim,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(error = %error, "managed-library operation claim failed");
                continue;
            }
        };
        let operation_id = claim.operation.id;
        let library_id = claim.operation.library_id;
        let execution = application.execute_managed_library_operation(claim.clone());
        tokio::pin!(execution);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        heartbeat.tick().await;
        let outcome = loop {
            tokio::select! {
                () = shutdown.cancelled() => break None,
                outcome = &mut execution => break Some(outcome),
                _ = heartbeat.tick() => {
                    let heartbeat_result = application
                        .heartbeat_managed_library_operation(&claim)
                        .await;
                    match &heartbeat_result {
                        Ok(true) => {}
                        Ok(false) => {
                            tracing::warn!(operation_id, library_id, "managed-library lease was lost");
                        }
                        Err(error) => {
                            tracing::warn!(operation_id, library_id, error = %error, "managed-library heartbeat failed");
                        }
                    }
                    if !heartbeat_keeps_operation_running(&heartbeat_result) {
                        break Some(ExecuteManagedLibraryOperationOutcome::LeaseLost);
                    }
                }
            }
        };
        let Some(outcome) = outcome else {
            break;
        };
        match outcome {
            ExecuteManagedLibraryOperationOutcome::Completed => {
                tracing::info!(
                    operation_id,
                    library_id,
                    "managed-library operation completed"
                );
            }
            ExecuteManagedLibraryOperationOutcome::Deferred => {
                tracing::warn!(
                    operation_id,
                    library_id,
                    "managed-library recovery deferred"
                );
            }
            ExecuteManagedLibraryOperationOutcome::Failed { message } => {
                tracing::warn!(
                    operation_id,
                    library_id,
                    error = %message,
                    "managed-library operation failed"
                );
            }
            ExecuteManagedLibraryOperationOutcome::LeaseLost => {
                tracing::warn!(
                    operation_id,
                    library_id,
                    "managed-library operation stopped after losing its lease"
                );
            }
        }
    }

    tracing::info!("scanner worker shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::ScanPlayoutRefreshReason;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor as _;

    #[test]
    fn heartbeat_error_stops_the_claimed_operation() {
        assert!(heartbeat_keeps_operation_running(&Ok::<_, &str>(true)));
        assert!(!heartbeat_keeps_operation_running(&Ok::<_, &str>(false)));
        assert!(!heartbeat_keeps_operation_running(&Err::<bool, _>(
            "heartbeat unavailable"
        )));
    }

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
    async fn durable_startup_replay_completes_an_interrupted_local_scan_marker() {
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

        let runtime = NativeScanRuntime::new(pool.clone());
        let libraries = runtime
            .interrupted_scan_library_ids()
            .await
            .expect("load interrupted Local scan");
        let report = runtime
            .recover_playout_refreshes()
            .await
            .expect("recover completed refresh intents");

        assert_eq!(libraries, [1]);
        assert!(report.playout_refreshes.is_empty());
        runtime
            .scan_library(MediaSourceKind::Local, 1, true, &())
            .await
            .expect("run admitted startup replay");
        let state: i64 = sqlx::query_scalar(
            r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load completed replay marker");
        assert_eq!(state, 1);
    }

    #[tokio::test]
    async fn durable_startup_replay_retains_a_failed_marker() {
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

        let runtime = NativeScanRuntime::new(pool.clone());
        let libraries = runtime
            .interrupted_scan_library_ids()
            .await
            .expect("discover interrupted scan");
        let report = runtime
            .recover_playout_refreshes()
            .await
            .expect("recover other durable refreshes");

        assert_eq!(libraries, [1]);
        assert!(report.playout_refreshes.is_empty());
        runtime
            .scan_library(MediaSourceKind::Local, 1, true, &())
            .await
            .expect_err("malformed managed input must fail replay");
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
