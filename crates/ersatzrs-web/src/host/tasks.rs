use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::trakt::{NoopTraktApiClient, TraktApiClient};
use ersatzrs_infra::trakt_sync::TraktSynchronizer;

pub fn spawn_background_tasks(
    pool: DbPool,
    device_uuid: uuid::Uuid,
    tuner_count: i32,
    listen_port: u16,
) {
    tokio::spawn(crate::transport::hdhr::run_udp_discovery(
        device_uuid,
        tuner_count,
        listen_port,
    ));
    tokio::spawn(run_trakt_maintenance(pool));
}

async fn run_trakt_maintenance(pool: DbPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
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
