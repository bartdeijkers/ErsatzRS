use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::trakt::{NoopTraktApiClient, TraktApiClient};
use ersatzrs_infra::trakt_sync::TraktSynchronizer;
use ersatzrs_scheduler::runner::{run_scheduler, SchedulerOptions};
use std::sync::{atomic::AtomicI32, Arc};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Spawn the long-running background tasks and return their join handles so the
/// host can await a clean drain on shutdown. Every task observes `shutdown` and
/// exits promptly when it is cancelled.
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
        tokio::spawn(run_trakt_maintenance(maintenance.pool, shutdown.clone())),
        tokio::spawn(run_maintenance_scheduler(
            maintenance.playout_build_rx,
            maintenance.playout_locker,
            maintenance.hls_sessions,
            shutdown,
        )),
    ]
}

pub struct HdhrTaskInputs {
    pub device_uuid: uuid::Uuid,
    pub tuner_count: Arc<AtomicI32>,
    pub listen_port: u16,
}

pub struct MaintenanceTaskInputs {
    pub pool: DbPool,
    pub playout_build_rx: mpsc::Receiver<PlayoutBuildRequest>,
    pub playout_locker: EntityLocker,
    pub hls_sessions: Arc<dyn HlsSessionRegistry>,
}

async fn run_maintenance_scheduler(
    playout_build_rx: mpsc::Receiver<PlayoutBuildRequest>,
    playout_locker: EntityLocker,
    hls_sessions: Arc<dyn HlsSessionRegistry>,
    shutdown: CancellationToken,
) {
    if let Err(error) = run_scheduler(SchedulerOptions {
        tick: std::time::Duration::from_mins(1),
        build_requests: Some(playout_build_rx),
        hls_sessions: Some(hls_sessions),
        locker: Some(playout_locker),
        shutdown: Some(shutdown),
        ..SchedulerOptions::default()
    })
    .await
    {
        tracing::error!(%error, "maintenance scheduler exited");
    }
}

async fn run_trakt_maintenance(pool: DbPool, shutdown: CancellationToken) {
    let mut interval = tokio::time::interval(std::time::Duration::from_hours(1));
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }

        if let Ok(client) = TraktApiClient::from_env() {
            let sync = TraktSynchronizer::new(pool.clone(), client);
            if let Err(error) = sync.refresh_due_auto_lists().await {
                tracing::warn!(error = %error, "failed to refresh due Trakt lists");
            }
        }

        let sync = TraktSynchronizer::new(pool.clone(), NoopTraktApiClient);
        if let Err(error) = sync.match_due_lists().await {
            tracing::warn!(error = %error, "failed to match due Trakt lists");
        }
    }
}
