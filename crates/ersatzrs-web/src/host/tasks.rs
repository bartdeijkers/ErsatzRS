use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_infra::addon_media_list_import::AddonMediaListImportCoordinator;
use ersatzrs_infra::addon_media_list_sync::{
    AddonMediaListMatchReport, AddonMediaListSynchronizer,
};
use ersatzrs_infra::db::DbPool;
use ersatzrs_scheduler::runner::{run_scheduler, SchedulerOptions};
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
    application.recover_scan_playout_refreshes().await
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
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }
        let work = async {
            let imported = coordinator.process_one_import_page().await?;
            let detailed = coordinator.process_one_detail().await?;
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
                imported || detailed || projected.is_some(),
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

        let coordinator = AddonMediaListImportCoordinator::new(pool.clone());
        if let Err(error) = coordinator.refresh_due_auto_lists().await {
            tracing::warn!(error = %error, "failed to refresh due add-on media lists");
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use ersatzrs_core::repositories::RepositoryError;
    use ersatzrs_domain::{ScanPlayoutRefresh, ScanPlayoutRefreshReason};

    use super::{admit_periodic_addon_playlist_refreshes, AddonMediaListMatchReport};

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
