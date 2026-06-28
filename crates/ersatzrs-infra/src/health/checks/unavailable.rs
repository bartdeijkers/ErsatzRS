use super::super::{
    async_trait, media_state_result, Db, HealthCheck, HealthCheckResult, Pool, Result,
    STATUS_UNAVAILABLE,
};

pub(crate) struct UnavailableHealthCheck {
    pool: Pool<Db>,
}

impl UnavailableHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for UnavailableHealthCheck {
    fn title(&self) -> &'static str {
        "Unavailable"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        media_state_result(
            &self.pool,
            self.title(),
            STATUS_UNAVAILABLE,
            "There are {count} items that are unavailable because ErsatzTV cannot find them on disk, including the following: {items}",
            "There are {count} items that are unavailable",
            Some("search?query=state%3aUnavailable"),
        )
        .await
    }
}
