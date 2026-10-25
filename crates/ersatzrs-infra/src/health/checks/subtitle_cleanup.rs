use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};
use crate::subtitle_cache_nul::REPAIR_PENDING_KEY;
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
        let repair_pending: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "ConfigElement"
               WHERE "Key" = ? AND LOWER(TRIM("Value")) IN ('true', '1'))"#,
        )
        .bind(REPAIR_PENDING_KEY)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if repair_pending {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                "Subtitle cache repair has not finished. Some cached subtitles may not display. Automatic cache maintenance will retry. If this warning persists, check write access to the subtitle cache and available disk space.",
                "Subtitle cache repair pending",
            ));
        }
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

    #[tokio::test]
    async fn subtitle_cache_repair_health_warns_without_mutation_and_recovers() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        let check = SubtitleCleanupHealthCheck::new(pool.clone());
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, '1')"#)
            .bind(REPAIR_PENDING_KEY)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "Subtitle" ("SubtitleKind","StreamIndex","Path","Default","Forced","DateAdded","DateUpdated")
            VALUES (0,0,'synthetic/private.srt',0,0,'2026-09-24','2026-09-24')"#)
            .execute(&pool).await.unwrap();

        for value in ["1", "true"] {
            sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
                .bind(value)
                .bind(REPAIR_PENDING_KEY)
                .execute(&pool)
                .await
                .unwrap();
            let result = check.check().await.unwrap();
            assert_eq!(result.status, HealthCheckStatus::Warning);
            assert_eq!(result.brief_message, "Subtitle cache repair pending");
            assert!(result
                .message
                .contains("Automatic cache maintenance will retry"));
            assert!(!serde_json::to_string(&result)
                .unwrap()
                .contains("private.srt"));
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#
                )
                .bind(REPAIR_PENDING_KEY)
                .fetch_one(&pool)
                .await
                .unwrap(),
                value
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Subtitle""#)
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = 'false' WHERE "Key" = ?"#)
            .bind(REPAIR_PENDING_KEY)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            check.check().await.unwrap().brief_message,
            "1 subtitle row(s) awaiting cleanup"
        );
        crate::subtitle_cleanup::clean_orphaned_subtitles(&pool, 100_000)
            .await
            .unwrap();
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
    }
}
