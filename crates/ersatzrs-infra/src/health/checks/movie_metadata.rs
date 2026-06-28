use super::super::{
    async_trait, missing_metadata_result, Db, HealthCheck, HealthCheckResult, Pool, Result,
};

pub(crate) struct MovieMetadataHealthCheck {
    pool: Pool<Db>,
}

impl MovieMetadataHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for MovieMetadataHealthCheck {
    fn title(&self) -> &'static str {
        "Movie Metadata"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        missing_metadata_result(
            &self.pool,
            self.title(),
            "Movie",
            "MovieMetadata",
            "MovieId",
            "movies",
        )
        .await
    }
}
