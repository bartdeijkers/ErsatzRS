use async_trait::async_trait;
use ersatzrs_app::commands::ExecuteManagedLibraryOperationOutcome;
use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_core::repositories::{
    AddonMediaListSourceProbeRepository as _, ManagedLibraryOperation, ManagedLibraryOperationClaim,
};
use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_domain::{EntityId, ManagedLibraryOperationState};
use ersatzrs_infra::addon_media_list_import::AddonMediaListImportCoordinator;
use ersatzrs_infra::addon_media_list_refresh::AddonMediaListRefreshCoordinator;
use ersatzrs_infra::addon_media_list_source_probe::AddonMediaListSourceProbeCoordinator;
use ersatzrs_infra::addon_media_list_sync::{
    AddonMediaListMatchReport, AddonMediaListSynchronizer,
};
use ersatzrs_infra::db::DbPool;
use ersatzrs_scheduler::runner::{run_scheduler, SchedulerOptions};
use std::collections::BTreeSet;
use std::future::Future;
use std::sync::{atomic::AtomicI32, Arc};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Replay interrupted scan work and admit every durable playout refresh before
/// mutating background workers are allowed to start.
///
/// # Errors
///
/// Returns an error when scan recovery or playout refresh admission cannot
/// complete through the configured application ports.
pub async fn run_background_task_preflight(
    application: Arc<ersatzrs_app::ApplicationServices>,
) -> ersatzrs_app::Result<usize> {
    run_background_task_preflight_inner(application.as_ref()).await
}

async fn run_background_task_preflight_inner(
    application: &(dyn StartupReplayApplication + '_),
) -> ersatzrs_app::Result<usize> {
    loop {
        let replay_operation_ids = application.prepare_recovery().await?;
        if replay_operation_ids.is_empty() {
            break;
        }
        await_terminal_startup_replays(application, replay_operation_ids).await?;
    }
    application.recover_refreshes().await
}

#[async_trait]
trait StartupReplayApplication: Sync {
    async fn prepare_recovery(&self) -> ersatzrs_app::Result<Vec<EntityId>>;

    async fn recover_refreshes(&self) -> ersatzrs_app::Result<usize>;

    async fn operation(
        &self,
        operation_id: EntityId,
    ) -> ersatzrs_app::Result<Option<ManagedLibraryOperation>>;

    async fn claim_operation(
        &self,
        operation_id: EntityId,
    ) -> ersatzrs_app::Result<Option<ManagedLibraryOperationClaim>>;

    async fn heartbeat(&self, claim: &ManagedLibraryOperationClaim) -> ersatzrs_app::Result<bool>;

    async fn execute(
        &self,
        claim: ManagedLibraryOperationClaim,
    ) -> ExecuteManagedLibraryOperationOutcome;
}

#[async_trait]
impl StartupReplayApplication for ersatzrs_app::ApplicationServices {
    async fn prepare_recovery(&self) -> ersatzrs_app::Result<Vec<EntityId>> {
        self.prepare_scan_playout_recovery().await
    }

    async fn recover_refreshes(&self) -> ersatzrs_app::Result<usize> {
        self.recover_scan_playout_refreshes().await
    }

    async fn operation(
        &self,
        operation_id: EntityId,
    ) -> ersatzrs_app::Result<Option<ManagedLibraryOperation>> {
        self.managed_library_operation(operation_id).await
    }

    async fn claim_operation(
        &self,
        operation_id: EntityId,
    ) -> ersatzrs_app::Result<Option<ManagedLibraryOperationClaim>> {
        self.claim_managed_library_operation_by_id(operation_id)
            .await
    }

    async fn heartbeat(&self, claim: &ManagedLibraryOperationClaim) -> ersatzrs_app::Result<bool> {
        self.heartbeat_managed_library_operation(claim).await
    }

    async fn execute(
        &self,
        claim: ManagedLibraryOperationClaim,
    ) -> ExecuteManagedLibraryOperationOutcome {
        self.execute_managed_library_operation(claim).await
    }
}

async fn await_terminal_startup_replays(
    application: &(dyn StartupReplayApplication + '_),
    operation_ids: Vec<EntityId>,
) -> ersatzrs_app::Result<()> {
    let mut pending = operation_ids.into_iter().collect::<BTreeSet<_>>();
    while !pending.is_empty() {
        let mut made_progress = false;
        for operation_id in pending.iter().copied().collect::<Vec<_>>() {
            let operation = application.operation(operation_id).await?.ok_or(
                ersatzrs_app::ApplicationError::InvalidData {
                    kind: "managed-library operation",
                    value: format!("startup replay operation {operation_id} disappeared"),
                },
            )?;
            if matches!(operation.state, ManagedLibraryOperationState::Completed) {
                pending.remove(&operation_id);
                made_progress = true;
                continue;
            }
            if matches!(operation.state, ManagedLibraryOperationState::Failed) {
                return Err(ersatzrs_app::ApplicationError::Unavailable {
                    operation: "replay interrupted library scan",
                    message: format!("startup replay operation {operation_id} failed"),
                });
            }
            if let Some(claim) = application.claim_operation(operation_id).await? {
                match execute_startup_replay_with_heartbeat(application, claim).await? {
                    ExecuteManagedLibraryOperationOutcome::Completed
                    | ExecuteManagedLibraryOperationOutcome::LeaseLost => {}
                    ExecuteManagedLibraryOperationOutcome::Deferred => {
                        return Err(ersatzrs_app::ApplicationError::Unavailable {
                            operation: "replay interrupted library scan",
                            message: format!(
                                "startup replay operation {operation_id} was unexpectedly deferred"
                            ),
                        });
                    }
                    ExecuteManagedLibraryOperationOutcome::Failed { message } => {
                        return Err(ersatzrs_app::ApplicationError::Unavailable {
                            operation: "replay interrupted library scan",
                            message,
                        });
                    }
                }
                made_progress = true;
            }
        }
        if !pending.is_empty() && !made_progress {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
    Ok(())
}

async fn execute_startup_replay_with_heartbeat(
    application: &(dyn StartupReplayApplication + '_),
    claim: ManagedLibraryOperationClaim,
) -> ersatzrs_app::Result<ExecuteManagedLibraryOperationOutcome> {
    let execution = application.execute(claim.clone());
    tokio::pin!(execution);
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            outcome = &mut execution => return Ok(outcome),
            _ = heartbeat.tick() => {
                match application.heartbeat(&claim).await {
                    Ok(true) => {}
                    Ok(false) => return Ok(ExecuteManagedLibraryOperationOutcome::LeaseLost),
                    Err(error) => {
                        return Err(ersatzrs_app::ApplicationError::Unavailable {
                            operation: "heartbeat startup library scan replay",
                            message: error.to_string(),
                        });
                    }
                }
            }
        }
    }
}

/// Spawn the long-running background tasks and return their join handles so the
/// host can await a clean drain on shutdown. Every task observes `shutdown` and
/// exits promptly when it is cancelled. Call
/// [`run_background_task_preflight`] successfully before invoking this function.
pub fn spawn_background_tasks(
    hdhr: HdhrTaskInputs,
    maintenance: MaintenanceTaskInputs,
    shutdown: CancellationToken,
) -> Vec<JoinHandle<()>> {
    vec![
        tokio::spawn(crate::transport::hdhr::run_udp_discovery(
            hdhr.device_uuid,
            hdhr.tuner_count,
            hdhr.listen_port,
            shutdown.clone(),
        )),
        tokio::spawn(run_addon_media_list_maintenance(
            maintenance.pool.clone(),
            maintenance.application.clone(),
            shutdown.clone(),
        )),
        tokio::spawn(run_addon_media_list_imports(
            maintenance.pool.clone(),
            maintenance.application.clone(),
            shutdown.clone(),
        )),
        tokio::spawn(run_addon_item_operations(
            maintenance.pool.clone(),
            shutdown.clone(),
        )),
        tokio::spawn(crate::host::preparation_worker::supervise(
            maintenance.pool.clone(),
            shutdown.clone(),
        )),
        tokio::spawn(run_maintenance_scheduler(
            maintenance.playout_build_rx,
            maintenance.application.clone(),
            maintenance.hls_sessions,
            shutdown.clone(),
        )),
        tokio::spawn(crate::host::scanning::run_scan_worker(
            maintenance.scan_work_rx,
            maintenance.application,
            shutdown,
        )),
    ]
}

async fn run_addon_media_list_imports(
    pool: DbPool,
    application: Arc<ersatzrs_app::ApplicationServices>,
    shutdown: CancellationToken,
) {
    let coordinator = AddonMediaListImportCoordinator::new(pool.clone());
    let refresh_coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
    let source_probes = AddonMediaListSourceProbeCoordinator::new(pool.clone());
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }
        let work = async {
            let refreshed = refresh_coordinator.process_one(chrono::Utc::now()).await?;
            let probed = source_probes
                .process_one_source_probe(chrono::Utc::now())
                .await?;
            let imported = coordinator.process_one_import_page().await?;
            let detailed = coordinator.process_one_detail().await?;
            let auto_published = ersatzrs_infra::addon_media_list_auto_publication::process_one(
                &pool,
                chrono::Utc::now(),
            )
            .await?;
            let projected =
                ersatzrs_infra::repositories::project_one_pending_detail_batch(&pool).await?;
            if let Some((list_id, library_id)) = projected {
                let outcome = application
                    .request_library_scan(ersatzrs_app::commands::RequestLibraryScan::new(
                        library_id, false,
                    ))
                    .await;
                if !matches!(
                    outcome,
                    ersatzrs_app::commands::RequestScanOutcome::Accepted { .. }
                ) {
                    tracing::warn!(
                        ?outcome,
                        addon_media_list_id = list_id,
                        library_id,
                        "failed to queue coalesced detail scan"
                    );
                }
            }
            Ok::<_, ersatzrs_core::repositories::RepositoryError>(
                refreshed
                    || probed
                    || imported
                    || detailed
                    || auto_published
                    || projected.is_some(),
            )
        };
        tokio::select! {
            () = shutdown.cancelled() => break,
            result = work => {
                if let Err(error) = result {
                    tracing::warn!(error = %error, "add-on list import worker iteration failed");
                }
            }
        }
    }
}

async fn run_addon_item_operations(pool: DbPool, shutdown: CancellationToken) {
    let coordinator =
        ersatzrs_infra::addon_media_list_item_coordinator::AddonMediaListItemCoordinator::new(pool);
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
    let mut metadata_turn = false;
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }
        // Give both metadata kinds a turn; file-writing work uses its supervised worker.
        metadata_turn = !metadata_turn;
        if metadata_turn {
            let (cancel, cancellation) = tokio::sync::oneshot::channel();
            let work = coordinator.process_one_interval_metadata(cancellation);
            tokio::pin!(work);
            tokio::select! {
                () = shutdown.cancelled() => {
                    let _ = cancel.send(());
                    let _ = work.await; // Do not drop the provider's process-cleanup future.
                    break;
                }
                result = &mut work => {
                    if result.is_err() {
                        tracing::warn!("add-on item metadata worker iteration failed");
                    }
                }
            }
            continue;
        }
        tokio::select! {
            () = shutdown.cancelled() => break,
            result = coordinator.process_one_access() => {
                if result.is_err() {
                    tracing::warn!("add-on item access worker iteration failed");
                }
            }
        }
    }
}

pub struct HdhrTaskInputs {
    pub device_uuid: uuid::Uuid,
    pub tuner_count: Arc<AtomicI32>,
    pub listen_port: u16,
}

pub struct MaintenanceTaskInputs {
    pub pool: DbPool,
    pub playout_build_rx: mpsc::UnboundedReceiver<PlayoutBuildRequest>,
    pub scan_work_rx: mpsc::UnboundedReceiver<ersatzrs_app::commands::ScanWorkRequest>,
    pub application: Arc<ersatzrs_app::ApplicationServices>,
    pub hls_sessions: Arc<dyn HlsSessionRegistry>,
}

async fn run_maintenance_scheduler(
    playout_build_rx: mpsc::UnboundedReceiver<PlayoutBuildRequest>,
    application: Arc<ersatzrs_app::ApplicationServices>,
    hls_sessions: Arc<dyn HlsSessionRegistry>,
    shutdown: CancellationToken,
) {
    let operations = application.operations();
    if let Err(error) = run_scheduler(SchedulerOptions {
        tick: std::time::Duration::from_mins(1),
        build_requests: Some(playout_build_rx),
        application: Some(application.clone()),
        build_activity: Some(operations.playout_activity()),
        hls_sessions: Some(hls_sessions),
        locker: Some(operations.entity_locker()),
        shutdown: Some(shutdown),
        ..SchedulerOptions::default()
    })
    .await
    {
        tracing::error!(%error, "maintenance scheduler exited");
    }
}

async fn run_addon_media_list_maintenance(
    pool: DbPool,
    application: Arc<ersatzrs_app::ApplicationServices>,
    shutdown: CancellationToken,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_hours(1));
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }

        if let Err(error) = application
            .prune_addon_media_list_publication_history()
            .await
        {
            tracing::warn!(error = %error, "failed to prune add-on publication history");
        }

        if let Err(error) = application.prune_addon_media_list_revisions().await {
            tracing::warn!(error = %error, "failed to prune add-on publication revisions");
        }

        let coordinator = AddonMediaListRefreshCoordinator::new(pool.clone());
        if let Err(error) = coordinator.admit_due(chrono::Utc::now()).await {
            tracing::warn!(error = %error, "failed to admit due add-on media-list refreshes");
        }

        let sync = AddonMediaListSynchronizer::new(pool.clone());
        let match_result = sync.match_due_lists_with_report().await;
        let (matched_lists, reported_refreshes) = match &match_result {
            Ok(report) => (report.matched_list_count, report.playout_refreshes.len()),
            Err(error) => {
                tracing::warn!(error = %error, "failed to match due Add-on media lists");
                (0, 0)
            }
        };
        let match_failed = match_result.is_err();
        match admit_periodic_addon_playlist_refreshes(&match_result, || {
            application.retry_pending_scan_playout_refreshes()
        })
        .await
        {
            Ok(admitted_refreshes) if reported_refreshes > 0 || admitted_refreshes > 0 => {
                tracing::info!(
                    matched_lists,
                    reported_refreshes,
                    admitted_refreshes,
                    match_failed,
                    "admitted periodic add-on playlist refreshes"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(
                reported_refreshes,
                match_failed,
                %error,
                "failed to admit periodic add-on playlist refreshes"
            ),
        }
    }
}

async fn admit_periodic_addon_playlist_refreshes<F, Fut>(
    match_result: &ersatzrs_core::repositories::Result<AddonMediaListMatchReport>,
    admit_pending: F,
) -> ersatzrs_app::Result<usize>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ersatzrs_app::Result<usize>>,
{
    let admission_required = match match_result {
        Ok(report) => !report.playout_refreshes.is_empty(),
        // Earlier lists may already have committed durable intents before a
        // later list makes the aggregate operation fail.
        Err(_) => true,
    };
    if !admission_required {
        return Ok(0);
    }
    admit_pending().await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ersatzrs_app::commands::ExecuteManagedLibraryOperationOutcome;
    use ersatzrs_core::repositories::{
        ManagedLibraryOperation, ManagedLibraryOperationClaim, ManagedLibraryScanSpec,
        RepositoryError,
    };
    use ersatzrs_domain::{
        ManagedLibraryOperationKind, ManagedLibraryOperationState, MediaSourceKind,
        ScanPlayoutRefresh, ScanPlayoutRefreshReason,
    };

    use super::{
        admit_periodic_addon_playlist_refreshes, await_terminal_startup_replays,
        run_background_task_preflight_inner, AddonMediaListMatchReport, StartupReplayApplication,
    };

    struct ReplayApplication {
        operation: Mutex<ManagedLibraryOperation>,
        claims: AtomicUsize,
        fail: bool,
        lease_loss_once: bool,
        lease_lost: AtomicBool,
        preparation_rounds: usize,
        preparation_calls: AtomicUsize,
    }

    impl ReplayApplication {
        fn new() -> Self {
            Self {
                operation: Mutex::new(ManagedLibraryOperation {
                    id: 17,
                    library_id: 4,
                    parent_operation_id: None,
                    owner_token: "owner".to_owned(),
                    kind: ManagedLibraryOperationKind::StartupReplayScan,
                    state: ManagedLibraryOperationState::Running,
                    scan: Some(ManagedLibraryScanSpec {
                        provider: MediaSourceKind::Local,
                        show_id: None,
                        deep_scan: true,
                    }),
                    attempt_count: 1,
                    next_eligible_at: None,
                }),
                claims: AtomicUsize::new(0),
                fail: false,
                lease_loss_once: true,
                lease_lost: AtomicBool::new(false),
                preparation_rounds: 0,
                preparation_calls: AtomicUsize::new(0),
            }
        }

        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::new()
            }
        }

        fn repeating() -> Self {
            Self {
                lease_loss_once: false,
                preparation_rounds: 2,
                ..Self::new()
            }
        }
    }

    #[async_trait]
    impl StartupReplayApplication for ReplayApplication {
        async fn prepare_recovery(&self) -> ersatzrs_app::Result<Vec<ersatzrs_domain::EntityId>> {
            let round = self.preparation_calls.fetch_add(1, Ordering::SeqCst) + 1;
            if round > self.preparation_rounds {
                return Ok(Vec::new());
            }
            let mut operation = self.operation.lock().expect("operation");
            operation.id = 16 + i64::try_from(round).expect("round id");
            operation.state = ManagedLibraryOperationState::Running;
            Ok(vec![operation.id])
        }

        async fn recover_refreshes(&self) -> ersatzrs_app::Result<usize> {
            Ok(3)
        }

        async fn operation(
            &self,
            operation_id: ersatzrs_domain::EntityId,
        ) -> ersatzrs_app::Result<Option<ManagedLibraryOperation>> {
            let operation = self.operation.lock().expect("operation").clone();
            Ok((operation.id == operation_id).then_some(operation))
        }

        async fn claim_operation(
            &self,
            operation_id: ersatzrs_domain::EntityId,
        ) -> ersatzrs_app::Result<Option<ManagedLibraryOperationClaim>> {
            let operation = self.operation.lock().expect("operation").clone();
            if operation.id != operation_id {
                return Ok(None);
            }
            self.claims.fetch_add(1, Ordering::SeqCst);
            Ok(Some(ManagedLibraryOperationClaim {
                operation,
                lease_owner: "lease".to_owned(),
            }))
        }

        async fn heartbeat(
            &self,
            _claim: &ManagedLibraryOperationClaim,
        ) -> ersatzrs_app::Result<bool> {
            Ok(true)
        }

        async fn execute(
            &self,
            _claim: ManagedLibraryOperationClaim,
        ) -> ExecuteManagedLibraryOperationOutcome {
            if self.fail {
                self.operation.lock().expect("operation").state =
                    ManagedLibraryOperationState::Failed;
                return ExecuteManagedLibraryOperationOutcome::Failed {
                    message: "replay scan failed".to_owned(),
                };
            }
            if self.lease_loss_once && !self.lease_lost.swap(true, Ordering::SeqCst) {
                ExecuteManagedLibraryOperationOutcome::LeaseLost
            } else {
                self.operation.lock().expect("operation").state =
                    ManagedLibraryOperationState::Completed;
                ExecuteManagedLibraryOperationOutcome::Completed
            }
        }
    }

    #[tokio::test]
    async fn startup_replay_waits_for_persisted_terminal_state_after_lease_loss() {
        let application = ReplayApplication::new();

        await_terminal_startup_replays(&application, vec![17])
            .await
            .expect("wait for durable replay completion");

        assert_eq!(
            application.claims.load(Ordering::SeqCst),
            2,
            "an execution outcome must not finish preflight before persistence is terminal"
        );
    }

    #[tokio::test]
    async fn failed_startup_replay_rejects_preflight() {
        let application = ReplayApplication::failing();

        let error = await_terminal_startup_replays(&application, vec![17])
            .await
            .expect_err("failed durable replay must stop preflight");

        assert!(error.to_string().contains("replay scan failed"));
        assert_eq!(application.claims.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn preflight_prepares_again_until_no_interrupted_marker_remains() {
        let application = ReplayApplication::repeating();

        let recovered = run_background_task_preflight_inner(&application)
            .await
            .expect("complete every startup replay generation");

        assert_eq!(recovered, 3);
        assert_eq!(application.preparation_calls.load(Ordering::SeqCst), 3);
        assert_eq!(application.claims.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn periodic_match_report_immediately_admits_durable_refreshes() {
        let match_result = Ok(AddonMediaListMatchReport {
            matched_list_count: 1,
            playout_refreshes: vec![ScanPlayoutRefresh {
                playout_id: 42,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
            }],
        });
        let admission_calls = AtomicUsize::new(0);

        let admitted = admit_periodic_addon_playlist_refreshes(&match_result, || async {
            admission_calls.fetch_add(1, Ordering::SeqCst);
            Ok(1)
        })
        .await
        .expect("admit periodic refresh report");

        assert_eq!(admitted, 1);
        assert_eq!(admission_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn aggregate_match_error_still_admits_previously_committed_refreshes() {
        let match_result = Err(RepositoryError::unavailable(
            "match due add-on media lists",
            "later list failed",
        ));
        let admission_calls = AtomicUsize::new(0);

        let admitted = admit_periodic_addon_playlist_refreshes(&match_result, || async {
            admission_calls.fetch_add(1, Ordering::SeqCst);
            Ok(2)
        })
        .await
        .expect("admit durable refreshes committed before aggregate failure");

        assert_eq!(admitted, 2);
        assert_eq!(admission_calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            match_result,
            Err(RepositoryError::Unavailable {
                operation: "match due add-on media lists",
                ref message,
            }) if message == "later list failed"
        ));
    }
}
