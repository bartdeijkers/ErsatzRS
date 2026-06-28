use super::super::{
    async_trait, missing_metadata_result, Db, HealthCheck, HealthCheckResult, Pool, Result,
};

pub(crate) struct EpisodeMetadataHealthCheck {
    pool: Pool<Db>,
}

impl EpisodeMetadataHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for EpisodeMetadataHealthCheck {
    fn title(&self) -> &'static str {
        "Episode Metadata"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        missing_metadata_result(
            &self.pool,
            self.title(),
            "Episode",
            "EpisodeMetadata",
            "EpisodeId",
            "episodes",
        )
        .await
    }
}
