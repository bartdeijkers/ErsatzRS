use std::sync::Arc;

use ersatzrs_infra::repositories::{make_repositories, Repositories};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub repositories: Arc<Repositories>,
    pub pool: SqlitePool,
}

impl AppState {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        let repositories = Arc::new(make_repositories(pool.clone()));
        Self { repositories, pool }
    }
}
