use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::trakt::{NoopTraktApiClient, TraktApiClient};
use ersatzrs_infra::trakt_sync::TraktSynchronizer;
use ersatzrs_scheduler::runner::{run_scheduler, SchedulerOptions};
use std::sync::{atomic::AtomicI32, Arc};
use tokio::sync::mpsc;

pub fn spawn_background_tasks(
    pool: DbPool,
    device_uuid: uuid::Uuid,
    tuner_count: Arc<AtomicI32>,
    listen_port: u16,
    playout_build_rx: mpsc::Receiver<PlayoutBuildRequest>,
) {
    tokio::spawn(crate::transport::hdhr::run_udp_discovery(
        device_uuid,
        tuner_count,
        listen_port,
    ));
    tokio::spawn(run_trakt_maintenance(pool));
    tokio::spawn(run_maintenance_scheduler(playout_build_rx));
}

async fn run_maintenance_scheduler(playout_build_rx: mpsc::Receiver<PlayoutBuildRequest>) {
    if let Err(error) = run_scheduler(SchedulerOptions {
        tick: std::time::Duration::from_mins(1),
        build_requests: Some(playout_build_rx),
        ..SchedulerOptions::default()
    })
    .await
    {
        tracing::error!(%error, "maintenance scheduler exited");
    }
}

async fn run_trakt_maintenance(pool: DbPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_hours(1));
    loop {
        interval.tick().await;

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
