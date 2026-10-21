use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_app::{
    commands::{ExecuteManagedLibraryOperationOutcome, ScanWorkRequest},
    ApplicationServices, ScanCleanupConfirmation, ScanPlayoutRefresh, ScanRuntime,
    ScanRuntimeError, ScanRuntimeReport, ScanWorkQueue, ScanWorkQueueError,
};
use ersatzrs_domain::{EntityId, MediaSourceKind, ScanProgressSink};
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::ffprobe::LocalProbeCleanupScope;
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
        deep_scan: bool,
        progress: &dyn ScanProgressSink,
        managed_claim: Option<&ersatzrs_core::repositories::ManagedLibraryOperationClaim>,
    ) -> Result<ersatzrs_scanner::LocalScanReport, ScanRuntimeError> {
        let config = ersatzrs_scanner::LocalScanConfig {
            library_id,
            ffprobe_path: ersatzrs_ffmpeg::ffprobe_path(),
            deep_scan,
            managed_claim: managed_claim.cloned(),
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
    async fn confirm_cleanup(&self) -> ScanCleanupConfirmation {
        let Some(scope) = LocalProbeCleanupScope::current() else {
            return ScanCleanupConfirmation::Unconfirmed;
        };
        match scope.wait_cleanup().await {
            Ok(()) => ScanCleanupConfirmation::Confirmed,
            Err(_) => ScanCleanupConfirmation::Unconfirmed,
        }
    }

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
        ersatzrs_infra::scripted_dependencies::resolve_pending(&self.pool, chrono::Utc::now())
            .await
            .map_err(scan_error)?;
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
        ersatzrs_infra::scripted_dependencies::resolve_pending(&self.pool, chrono::Utc::now())
            .await
            .map_err(scan_error)?;
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
        managed_claim: Option<&ersatzrs_core::repositories::ManagedLibraryOperationClaim>,
    ) -> Result<ScanRuntimeReport, ScanRuntimeError> {
        match provider {
            MediaSourceKind::Plex => ersatzrs_infra::plex::sync_library_by_id_with_managed_claim(
                &self.pool,
                library_id,
                deep_scan,
                Some(progress),
                managed_claim,
            )
            .await
            .map(|_| ScanRuntimeReport::default())
            .map_err(scan_error),
            MediaSourceKind::Jellyfin => {
                ersatzrs_infra::jellyfin::sync_library_by_id_with_managed_claim(
                    &self.pool,
                    library_id,
                    deep_scan,
                    Some(progress),
                    managed_claim,
                )
                .await
                .map(|_| ScanRuntimeReport::default())
                .map_err(scan_error)
            }
            MediaSourceKind::Emby => ersatzrs_infra::emby::sync_library_by_id_with_managed_claim(
                &self.pool,
                library_id,
                deep_scan,
                Some(progress),
                managed_claim,
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
                    let mut report = self
                        .scan_local_library_report(library_id, deep_scan, progress, managed_claim)
                        .await?;
                    if self.confirm_cleanup().await == ScanCleanupConfirmation::Unconfirmed {
                        return Err(ScanRuntimeError::new(
                            "Local probe cleanup was not confirmed",
                        ));
                    }
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
                if self.confirm_cleanup().await == ScanCleanupConfirmation::Unconfirmed {
                    return Err(ScanRuntimeError::new(
                        "Local probe cleanup was not confirmed",
                    ));
                }
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
        managed_claim: Option<&ersatzrs_core::repositories::ManagedLibraryOperationClaim>,
    ) -> Result<(), ScanRuntimeError> {
        match provider {
            MediaSourceKind::Plex => ersatzrs_infra::plex::sync_plex_show_by_id_with_managed_claim(
                &self.pool,
                library_id,
                show_id,
                deep_scan,
                Some(progress),
                managed_claim,
            )
            .await
            .map(|_| ())
            .map_err(scan_error),
            MediaSourceKind::Jellyfin => {
                ersatzrs_infra::jellyfin::sync_jellyfin_show_by_id_with_managed_claim(
                    &self.pool,
                    library_id,
                    show_id,
                    deep_scan,
                    Some(progress),
                    managed_claim,
                )
                .await
                .map(|_| ())
                .map_err(scan_error)
            }
            MediaSourceKind::Emby => ersatzrs_infra::emby::sync_emby_show_by_id_with_managed_claim(
                &self.pool,
                library_id,
                show_id,
                deep_scan,
                Some(progress),
                managed_claim,
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

async fn run_claimed_operation(
    execution: impl std::future::Future<Output = ExecuteManagedLibraryOperationOutcome>,
    heartbeat: impl std::future::Future<Output = ()>,
    shutdown: &CancellationToken,
) -> Option<ExecuteManagedLibraryOperationOutcome> {
    // Keep polling execution while a heartbeat waits for SQLite's writer lock:
    // execution may own that lock and must be able to finish its transaction.
    let scope = LocalProbeCleanupScope::new();
    tokio::pin!(heartbeat);
    let mut heartbeat_owned = true;
    let mut outcome = {
        let execution = scope.scope(execution);
        tokio::pin!(execution);
        let outcome = tokio::select! {
            () = shutdown.cancelled() => None,
            outcome = &mut execution => Some(outcome),
            () = &mut heartbeat => {
                heartbeat_owned = false;
                Some(ExecuteManagedLibraryOperationOutcome::LeaseLost)
            },
        };
        // Fence registrations before destroying the backing execution future.
        scope.close();
        outcome
    };
    let cleanup = scope.wait_cleanup();
    tokio::pin!(cleanup);
    loop {
        tokio::select! {
            biased;
            result = &mut cleanup => {
                if result.is_err() {
                    return Some(ExecuteManagedLibraryOperationOutcome::CleanupUnconfirmed);
                }
                return outcome;
            }
            () = &mut heartbeat, if heartbeat_owned => {
                heartbeat_owned = false;
                outcome = Some(ExecuteManagedLibraryOperationOutcome::LeaseLost);
            }
        }
    }
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
        let heartbeat = async {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                interval.tick().await;
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
                    return;
                }
            }
        };
        let outcome = run_claimed_operation(execution, heartbeat, &shutdown).await;
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
            ExecuteManagedLibraryOperationOutcome::CleanupUnconfirmed => {
                tracing::error!("local probe cleanup was not confirmed; scanner worker stopped");
                break;
            }
        }
    }

    tracing::info!("scanner worker shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::ScanPlayoutRefreshReason;
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    use sqlx::Executor as _;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::sync::oneshot;

    #[test]
    fn heartbeat_error_stops_the_claimed_operation() {
        assert!(heartbeat_keeps_operation_running(&Ok::<_, &str>(true)));
        assert!(!heartbeat_keeps_operation_running(&Ok::<_, &str>(false)));
        assert!(!heartbeat_keeps_operation_running(&Err::<bool, _>(
            "heartbeat unavailable"
        )));
    }

    async fn worker_test_pool() -> (tempfile::TempDir, DbPool) {
        let directory = tempfile::tempdir().expect("temporary scan worker database");
        let options = SqliteConnectOptions::new()
            .filename(directory.path().join("worker.sqlite"))
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_millis(100));
        let pool = SqlitePoolOptions::new()
            .min_connections(2)
            .max_connections(2)
            .connect_with(options)
            .await
            .expect("connect two SQLite workers");
        pool.execute(r#"CREATE TABLE "WorkerWrite" ("Value" INTEGER NOT NULL)"#)
            .await
            .expect("create worker regression table");
        (directory, pool)
    }

    #[tokio::test]
    async fn heartbeat_writer_wait_keeps_scan_execution_polled() {
        let (_directory, pool) = worker_test_pool().await;
        let (acquired_sender, acquired_receiver) = oneshot::channel();
        let (started_sender, started_receiver) = oneshot::channel();
        let (committed_sender, committed_receiver) = oneshot::channel();
        let execution = async {
            let mut transaction = pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .expect("scan owns SQLite writer lock");
            sqlx::query(r#"INSERT INTO "WorkerWrite" VALUES (1)"#)
                .execute(&mut *transaction)
                .await
                .expect("write scan progress");
            acquired_sender.send(()).expect("signal scan writer lock");
            started_receiver.await.expect("heartbeat started writing");
            transaction.commit().await.expect("commit scan transaction");
            committed_receiver.await.expect("heartbeat committed");
            ExecuteManagedLibraryOperationOutcome::Completed
        };
        let heartbeat = async {
            acquired_receiver.await.expect("scan acquired writer lock");
            started_sender.send(()).expect("signal heartbeat start");
            let mut transaction = pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .expect("heartbeat acquires writer lock while scan stays polled");
            sqlx::query(r#"INSERT INTO "WorkerWrite" VALUES (2)"#)
                .execute(&mut *transaction)
                .await
                .expect("write heartbeat");
            transaction.commit().await.expect("commit heartbeat");
            committed_sender.send(()).expect("signal heartbeat commit");
            std::future::pending::<()>().await;
        };
        let shutdown = CancellationToken::new();
        let outcome = tokio::time::timeout(
            Duration::from_secs(2),
            run_claimed_operation(execution, heartbeat, &shutdown),
        )
        .await
        .expect("scan and heartbeat do not deadlock");
        assert!(matches!(
            outcome,
            Some(ExecuteManagedLibraryOperationOutcome::Completed)
        ));
        let values: Vec<i64> =
            sqlx::query_scalar(r#"SELECT "Value" FROM "WorkerWrite" ORDER BY "Value""#)
                .fetch_all(&pool)
                .await
                .expect("read committed scan and heartbeat writes");
        assert_eq!(values, [1, 2]);
        pool.close().await;
    }

    struct ExecutionDropGuard(Arc<AtomicBool>);

    impl Drop for ExecutionDropGuard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    async fn assert_cancelled_execution_rolls_back(heartbeat_result: Option<Result<bool, &str>>) {
        let (_directory, pool) = worker_test_pool().await;
        let shutdown = CancellationToken::new();
        let dropped = Arc::new(AtomicBool::new(false));
        let (acquired_sender, acquired_receiver) = oneshot::channel();
        let execution = async {
            let _drop_guard = ExecutionDropGuard(dropped.clone());
            let mut transaction = pool
                .begin_with("BEGIN IMMEDIATE")
                .await
                .expect("scan acquires writer lock before cancellation");
            sqlx::query(r#"INSERT INTO "WorkerWrite" VALUES (1)"#)
                .execute(&mut *transaction)
                .await
                .expect("write uncommitted scan progress");
            acquired_sender.send(()).expect("signal scan write");
            std::future::pending::<ExecuteManagedLibraryOperationOutcome>().await
        };
        let heartbeat = async {
            acquired_receiver.await.expect("scan wrote before stopping");
            if let Some(result) = heartbeat_result {
                assert!(!heartbeat_keeps_operation_running(&result));
                return;
            }
            shutdown.cancel();
            std::future::pending::<()>().await;
        };
        let outcome = tokio::time::timeout(
            Duration::from_secs(2),
            run_claimed_operation(execution, heartbeat, &shutdown),
        )
        .await
        .expect("cancellation drops in-flight scan");
        if heartbeat_result.is_some() {
            assert!(matches!(
                outcome,
                Some(ExecuteManagedLibraryOperationOutcome::LeaseLost)
            ));
        } else {
            assert!(outcome.is_none());
        }
        assert!(dropped.load(Ordering::SeqCst), "execution was dropped");
        let mut transaction =
            tokio::time::timeout(Duration::from_secs(2), pool.begin_with("BEGIN IMMEDIATE"))
                .await
                .expect("cancelled scan releases SQLite writer lock")
                .expect("next writer acquires SQLite lock");
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "WorkerWrite""#)
            .fetch_one(&mut *transaction)
            .await
            .expect("read rolled-back scan write");
        assert_eq!(count, 0, "cancelled scan did not commit its write");
        transaction.rollback().await.expect("release next writer");
        pool.close().await;
    }

    #[tokio::test]
    async fn worker_shutdown_drops_execution_and_rolls_back() {
        assert_cancelled_execution_rolls_back(None).await;
    }

    #[tokio::test]
    async fn heartbeat_failure_drops_execution_and_rolls_back() {
        assert_cancelled_execution_rolls_back(Some(Err("heartbeat unavailable"))).await;
    }

    #[tokio::test]
    async fn native_cleanup_confirmation_requires_managed_scope() {
        let (_directory, pool) = worker_test_pool().await;
        let runtime = NativeScanRuntime::new(pool);
        assert_eq!(
            runtime.confirm_cleanup().await,
            ScanCleanupConfirmation::Unconfirmed
        );
        let scope = LocalProbeCleanupScope::new();
        assert_eq!(
            scope.scope(runtime.confirm_cleanup()).await,
            ScanCleanupConfirmation::Confirmed
        );
        // Confirmation does not close a healthy reusable scope.
        assert_eq!(
            scope.scope(runtime.confirm_cleanup()).await,
            ScanCleanupConfirmation::Confirmed
        );
    }

    // Unix shell fixtures intentionally exercise real managed process groups.
    // Windows JobObject ownership is covered by native platform acceptance.
    #[cfg(unix)]
    async fn assert_local_probe_reaped_before_operation_returns(lose_lease: bool) {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir().expect("temporary local probe fixture");
        let program = directory.path().join("probe.sh");
        let ready = directory.path().join("probe.sh.ready");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s' \"$$\" > \"$0.ready\"\nexec sleep 60\n",
        )
        .expect("write original probe fixture");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
            .expect("make probe fixture executable");
        let completed = Arc::new(AtomicBool::new(false));
        let (scope_sender, scope_receiver) = oneshot::channel();
        let extractor = ersatzrs_infra::ffprobe::FfprobeExtractor::new(program.to_string_lossy());
        let execution = async {
            scope_sender
                .send(LocalProbeCleanupScope::current().expect("managed execution scope"))
                .expect("retain scope evidence");
            let _ = extractor.extract("synthetic.mkv").await;
            completed.store(true, Ordering::SeqCst);
            ExecuteManagedLibraryOperationOutcome::Completed
        };
        let shutdown = CancellationToken::new();
        let heartbeat = async {
            tokio::time::timeout(Duration::from_secs(2), async {
                while std::fs::read_to_string(&ready)
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok())
                    .is_none()
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("local probe became active");
            if lose_lease {
                return;
            }
            shutdown.cancel();
            std::future::pending::<()>().await;
        };
        let outcome = tokio::time::timeout(
            Duration::from_secs(9),
            run_claimed_operation(execution, heartbeat, &shutdown),
        )
        .await
        .expect("owned probe cleanup finishes");
        if lose_lease {
            assert!(matches!(
                outcome,
                Some(ExecuteManagedLibraryOperationOutcome::LeaseLost)
            ));
        } else {
            assert!(outcome.is_none());
        }
        assert!(
            !completed.load(Ordering::SeqCst),
            "cancelled execution did not finish"
        );
        assert_eq!(
            scope_receiver
                .await
                .expect("scope evidence")
                .cleanup_status(),
            Ok(true)
        );
        let pid = std::fs::read_to_string(&ready).expect("active process identity");
        let alive = tokio::process::Command::new("/bin/sh")
            .args(["-c", "kill -0 \"$1\"", "probe-check", pid.as_str()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .expect("inspect probe process")
            .success();
        assert!(!alive, "process was reaped before operation returned");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_waits_for_actual_local_probe_cleanup() {
        assert_local_probe_reaped_before_operation_returns(false).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn lost_heartbeat_waits_for_actual_local_probe_cleanup() {
        assert_local_probe_reaped_before_operation_returns(true).await;
    }

    #[tokio::test]
    async fn heartbeat_lease_loss_drops_execution_and_rolls_back() {
        assert_cancelled_execution_rolls_back(Some(Ok(false))).await;
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
        LocalProbeCleanupScope::new()
            .scope(runtime.scan_library(MediaSourceKind::Local, 1, true, &(), None))
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
        LocalProbeCleanupScope::new()
            .scope(runtime.scan_library(MediaSourceKind::Local, 1, true, &(), None))
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
