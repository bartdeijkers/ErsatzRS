use std::sync::Arc;

use ersatzrs_core::hls::HlsSessionRegistry;
use ersatzrs_infra::hls::InMemoryHlsSessionRegistry;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub repositories: Arc<Repositories>,
    pub pool: SqlitePool,
    pub device_uuid: Uuid,
    pub tuner_count: i32,
    pub hls_sessions: Arc<dyn HlsSessionRegistry>,
}

impl AppState {
    #[must_use]
    pub fn new(pool: SqlitePool, device_uuid: Uuid, tuner_count: i32) -> Self {
        let repositories = Arc::new(make_repositories(pool.clone()));
        let hls_sessions: Arc<dyn HlsSessionRegistry> = Arc::new(InMemoryHlsSessionRegistry::new());
        Self {
            repositories,
            pool,
            device_uuid,
            tuner_count,
            hls_sessions,
        }
    }
}
