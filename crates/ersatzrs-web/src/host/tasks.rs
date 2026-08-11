use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_core::scheduling::PlayoutBuildRequest;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::trakt::TraktApiClient;
use ersatzrs_infra::trakt_sync::{TraktRemoteSynchronizer, TraktSynchronizer};
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
        tokio::spawn(run_trakt_maintenance(
            maintenance.pool.clone(),
            shutdown.clone(),
        )),
        tokio::spawn(run_addon_repository_maintenance(
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

async fn run_trakt_maintenance(pool: DbPool, shutdown: CancellationToken) {
    let mut interval = tokio::time::interval(std::time::Duration::from_hours(1));
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }

        if let Ok(client) = TraktApiClient::from_env() {
            let sync = TraktRemoteSynchronizer::new(pool.clone(), client);
            if let Err(error) = sync.refresh_due_auto_lists().await {
                tracing::warn!(error = %error, "failed to refresh due Trakt lists");
            }
        }

        let sync = TraktSynchronizer::new(pool.clone());
        if let Err(error) = sync.match_due_lists().await {
            tracing::warn!(error = %error, "failed to match due Trakt lists");
        }
    }
}

async fn run_addon_repository_maintenance(pool: DbPool, shutdown: CancellationToken) {
    let client = super::addons::addon_repository_client();
    let mut interval = tokio::time::interval(std::time::Duration::from_hours(6));
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {}
        }

        let repositories = match ersatzrs_infra::addons::list_trusted_repositories(&pool).await {
            Ok(repositories) => repositories,
            Err(error) => {
                tracing::warn!(%error, "failed to load trusted add-on repositories");
                continue;
            }
        };
        for repository in repositories.into_iter().filter(|repository| {
            repository.is_enabled
                && !repository.public_key.trim().is_empty()
                && repository_refresh_is_due(repository.last_refresh.as_deref())
        }) {
            if let Err(error) =
                ersatzrs_infra::addons::refresh_repository(&pool, &client, &repository.id).await
            {
                tracing::warn!(
                    repository_id = %repository.id,
                    %error,
                    "automatic add-on repository refresh failed"
                );
            }
        }
    }
}

fn repository_refresh_is_due(last_refresh: Option<&str>) -> bool {
    let Some(last_refresh) =
        last_refresh.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
    else {
        return true;
    };
    last_refresh.with_timezone(&chrono::Utc) <= chrono::Utc::now() - chrono::Duration::hours(12)
}

#[cfg(test)]
mod tests {
    use super::repository_refresh_is_due;

    #[test]
    fn repository_refresh_is_due_for_missing_or_old_state() {
        let old = (chrono::Utc::now() - chrono::Duration::hours(13)).to_rfc3339();
        let recent = chrono::Utc::now().to_rfc3339();

        assert!(repository_refresh_is_due(None));
        assert!(repository_refresh_is_due(Some("invalid")));
        assert!(repository_refresh_is_due(Some(&old)));
        assert!(!repository_refresh_is_due(Some(&recent)));
    }
}
