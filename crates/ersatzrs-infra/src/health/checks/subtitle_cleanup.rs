use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};
use crate::subtitle_cleanup::INVALID_SUBTITLE;

pub(crate) struct SubtitleCleanupHealthCheck {
    pool: Pool<Db>,
}

impl SubtitleCleanupHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for SubtitleCleanupHealthCheck {
    fn title(&self) -> &'static str {
        "Subtitle Cleanup"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        // Aggregate only: do not expose private media paths or identities.
        let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(format!(
            r#"SELECT COUNT(*) FROM "Subtitle" WHERE {INVALID_SUBTITLE}"#,
        )))
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if count == 0 {
            return Ok(HealthCheckResult::pass(self.title()));
        }
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!("Subtitle cleanup pending: {count} ownerless subtitle or empty-path sidecar row(s). Automatic cleanup runs hourly. Cached subtitle files are preserved."),
            format!("{count} subtitle row(s) awaiting cleanup"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subtitle_cleanup_health_counts_without_mutation_or_private_data() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        let check = SubtitleCleanupHealthCheck::new(pool.clone());
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
        sqlx::query(r#"INSERT INTO "Subtitle" ("SubtitleKind","StreamIndex","Path","Default","Forced","DateAdded","DateUpdated")
            VALUES (0,0,'synthetic/private.srt',0,0,'2026-09-24','2026-09-24')"#)
            .execute(&pool).await.unwrap();
        let result = check.check().await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(result.brief_message, "1 subtitle row(s) awaiting cleanup");
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("private.srt"));
        assert_eq!(
            crate::subtitle_cleanup::clean_orphaned_subtitles(&pool, 100_000)
                .await
                .unwrap(),
            1
        );
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
    }
}
