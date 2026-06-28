use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};

pub(crate) struct SearchProjectionHealthCheck {
    pool: Pool<Db>,
}

impl SearchProjectionHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for SearchProjectionHealthCheck {
    fn title(&self) -> &'static str {
        "Search Projection"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        for sql in [
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId" FROM "MediaItem" LIMIT 1"#,
            r#"SELECT "Title" FROM "MediaStream" LIMIT 1"#,
            r#"SELECT "Title" FROM "Subtitle" LIMIT 1"#,
        ] {
            sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map_err(sqlx_error)?;
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Pass,
            "Search projection includes content variant, parent linkage, and stream-title fields.",
            "Search projection ready",
        ))
    }
}
