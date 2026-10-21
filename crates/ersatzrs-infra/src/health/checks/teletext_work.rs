//! Native-work configuration validity, separate from teletext/media readiness.

use std::num::NonZeroUsize;

use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};
use crate::{provider_source_storage::Policy, repositories::common::decode_string_column};

const KEY: &str = "teletext.work.max_concurrent";
const DEFAULT_MESSAGE: &str = "Native teletext work is serialized by default. This is not a memory limit or proof that authored pages are ready.";
const INVALID_MESSAGE: &str = "Teletext work concurrency must be a positive integer. Native teletext operations are unavailable until this setting is corrected.";
const INVALID_STORAGE_MESSAGE: &str = "Provider source storage configuration is invalid or unavailable. Optional provider teletext captions are unavailable; ordinary audio/video playback remains available.";

pub(crate) struct TeletextWorkHealthCheck {
    pool: Pool<Db>,
}

impl TeletextWorkHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for TeletextWorkHealthCheck {
    fn title(&self) -> &'static str {
        "Teletext work configuration"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"=?"#)
            .bind(KEY)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlx_error)?;
        let (mut status, mut message) = match row {
            None => (HealthCheckStatus::Pass, DEFAULT_MESSAGE.to_owned()),
            Some(row) => match decode_string_column(&row, "Value").ok()
                .and_then(|value| value.parse::<NonZeroUsize>().ok()) {
                Some(maximum) => (HealthCheckStatus::Pass, format!(
                    "The configured limit is {maximum} concurrent native teletext operations. This is not a memory limit or proof that authored pages are ready."
                )),
                None => (HealthCheckStatus::Warning, INVALID_MESSAGE.to_owned()),
            },
        };
        if status == HealthCheckStatus::Pass && Policy::load(&self.pool).await.is_err() {
            status = HealthCheckStatus::Warning;
            INVALID_STORAGE_MESSAGE.clone_into(&mut message);
        }
        Ok(HealthCheckResult::new(
            self.title(),
            status,
            &message,
            &message,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::health::HealthCheckSummary;
    use sqlx::sqlite::SqlitePoolOptions;

    #[path = "storage.rs"]
    mod storage;

    async fn pool() -> Pool<Db> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(r#"CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT)"#)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn absence_reports_conservative_serialization_without_readiness_or_memory_claims() {
        let pool = pool().await;
        let result = TeletextWorkHealthCheck::new(pool.clone())
            .check()
            .await
            .unwrap();
        assert_eq!(result.title, "Teletext work configuration");
        assert_eq!(result.status, HealthCheckStatus::Pass);
        assert_eq!(result.message, DEFAULT_MESSAGE);
        assert_eq!(result.brief_message, result.message);
        assert_eq!(
            HealthCheckSummary::from_results(&[result]),
            HealthCheckSummary::default()
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn fresh_valid_integer_settings_report_only_the_configured_operation_limit() {
        let pool = pool().await;
        let check = TeletextWorkHealthCheck::new(pool.clone());
        for (value, expected) in [("1", 1), ("3", 3), ("+2", 2)] {
            sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" VALUES (?,?)"#)
                .bind(KEY)
                .bind(value)
                .execute(&pool)
                .await
                .unwrap();
            let result = check.check().await.unwrap();
            assert_eq!(result.status, HealthCheckStatus::Pass);
            assert_eq!(result.message, format!(
                "The configured limit is {expected} concurrent native teletext operations. This is not a memory limit or proof that authored pages are ready."
            ));
        }
        // ConfigRepository's supported UTF-8 BLOB decoding remains consistent.
        sqlx::query(r#"UPDATE "ConfigElement" SET "Value"=X'32' WHERE "Key"=?"#)
            .bind(KEY)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
        pool.close().await;
    }

    #[tokio::test]
    async fn zero_malformed_null_and_invalid_utf8_warn_without_exposing_values() {
        let pool = pool().await;
        let check = TeletextWorkHealthCheck::new(pool.clone());
        for value in [
            Some("0"),
            Some(""),
            Some("-1"),
            Some(" 2"),
            Some("2 "),
            Some("1.5"),
            Some("private-invalid-value"),
            Some("184467440737095516160"),
            None,
        ] {
            sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" VALUES (?,?)"#)
                .bind(KEY)
                .bind(value)
                .execute(&pool)
                .await
                .unwrap();
            let result = check.check().await.unwrap();
            assert_eq!(result.status, HealthCheckStatus::Warning);
            assert_eq!(result.message, INVALID_MESSAGE);
            assert_eq!(result.brief_message, INVALID_MESSAGE);
            assert_eq!(HealthCheckSummary::from_results(&[result]).errors, 0);
        }
        sqlx::query(r#"UPDATE "ConfigElement" SET "Value"=X'ff' WHERE "Key"=?"#)
            .bind(KEY)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            check.check().await.unwrap().status,
            HealthCheckStatus::Warning
        );
        pool.close().await;
    }
}
