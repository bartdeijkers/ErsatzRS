use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};
use crate::db::SQLITE_BUSY_TIMEOUT_MS;

pub(crate) struct SqliteConnectionDefaultsHealthCheck {
    pool: Pool<Db>,
}

impl SqliteConnectionDefaultsHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for SqliteConnectionDefaultsHealthCheck {
    fn title(&self) -> &'static str {
        "SQLite Connection Defaults"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let mut connection = self.pool.acquire().await.map_err(sqlx_error)?;

        #[cfg(feature = "mysql")]
        if connection.backend_name() != "SQLite" {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&mut *connection)
            .await
            .map_err(sqlx_error)?;
        let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(&mut *connection)
            .await
            .map_err(sqlx_error)?;
        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut *connection)
            .await
            .map_err(sqlx_error)?;
        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(&mut *connection)
            .await
            .map_err(sqlx_error)?;

        let is_ready = foreign_keys == 1
            && busy_timeout == i64::from(SQLITE_BUSY_TIMEOUT_MS)
            && journal_mode.eq_ignore_ascii_case("wal")
            && synchronous == 1;

        if is_ready {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Pass,
                "SQLite connections enforce foreign keys, a 30-second busy timeout, WAL journaling, and NORMAL synchronization.",
                "SQLite connection defaults ready",
            ));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "SQLite connection defaults are incorrect: foreign_keys={foreign_keys}, busy_timeout={busy_timeout}, journal_mode={journal_mode}, synchronous={synchronous}. Expected 1, 30000, wal, and 1 (NORMAL)."
            ),
            "SQLite connection defaults incorrect",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_ready_defaults_for_initialized_sqlite_pool() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ersatzrs-sqlite-defaults-health-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp health db dir");
        let database_url = format!("sqlite:{}", temp_dir.join("ErsatzRS.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize SQLite database");

        let result = SqliteConnectionDefaultsHealthCheck::new(pool)
            .check()
            .await
            .expect("run SQLite defaults health check");

        assert_eq!(result.status, HealthCheckStatus::Pass);
        assert_eq!(result.brief_message, "SQLite connection defaults ready");
    }
}
