//! ADR 0137: expose pending or structurally invalid physical routing maps.
use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};

pub(crate) struct JellyfinStreamIndexesHealthCheck {
    pool: Pool<Db>,
}

impl JellyfinStreamIndexesHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for JellyfinStreamIndexesHealthCheck {
    fn title(&self) -> &'static str {
        "Jellyfin stream indexes"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let invalid_map = crate::jellyfin::INVALID_SOURCE_INDEX_MAP;
        let sql = format!(
            r#"SELECT COUNT(*) FROM "MediaVersion" v
            WHERE (EXISTS (SELECT 1 FROM "JellyfinMovie" m WHERE m."Id"=v."MovieId")
                OR EXISTS (SELECT 1 FROM "JellyfinEpisode" e WHERE e."Id"=v."EpisodeId"))
            AND ({invalid_map})"#
        );
        let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(&sql))
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_error)?;
        if count == 0 {
            return Ok(HealthCheckResult::pass(self.title()));
        }
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!("{count} Jellyfin media versions need stream index repair. Rescan their libraries to repair audio, video and embedded subtitle routing. Saved track choices are retained. Disabled libraries remain disabled; enable scanning only if wanted."),
            "Jellyfin stream index repair needed",
        ))
    }
}

#[cfg(test)]
mod tests;
