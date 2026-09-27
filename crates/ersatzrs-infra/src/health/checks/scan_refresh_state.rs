use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};
use chrono::{Duration, Utc};

const STALE_AFTER_MINUTES: i64 = 15;
const RESOLUTION_STATE_RESOLVING: i64 = 0;
const RESOLUTION_STATE_RESOLVED: i64 = 1;
const INTENT_STATE_PENDING: i64 = 0;
const INTENT_STATE_BUILDING: i64 = 1;

#[derive(Default)]
struct ScanRefreshIssueCounts {
    unresolved_resolutions: u64,
    stale_resolutions: u64,
    failed_intents: u64,
    stale_intents: u64,
}

impl ScanRefreshIssueCounts {
    fn total(&self) -> u64 {
        self.unresolved_resolutions
            + self.stale_resolutions
            + self.failed_intents
            + self.stale_intents
    }
}

pub(crate) struct ScanRefreshStateHealthCheck {
    pool: Pool<Db>,
}

impl ScanRefreshStateHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ScanRefreshStateHealthCheck {
    fn title(&self) -> &'static str {
        "Scan Refresh State"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let stale_before = Utc::now() - Duration::minutes(STALE_AFTER_MINUTES);
        let mut issues = ScanRefreshIssueCounts::default();

        // Keep this projection intentionally aggregate-safe. Reasons, diagnostics,
        // and provider-owned identifiers must never reach the admin health result.
        let resolutions = sqlx::query(
            r#"SELECT "State", "UpdatedAt"
               FROM "ScanRefreshResolution"
               WHERE "State" <> ?"#,
        )
        .bind(RESOLUTION_STATE_RESOLVED)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        for row in resolutions {
            let state = row.try_get::<i64, _>("State").map_err(sqlx_error)?;
            if state != RESOLUTION_STATE_RESOLVING {
                issues.unresolved_resolutions += 1;
                continue;
            }
            let updated_at = row.try_get::<String, _>("UpdatedAt").map_err(sqlx_error)?;
            let updated_at = crate::repositories::common::decode_datetime(
                "scan refresh resolution update",
                &updated_at,
            )
            .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            if updated_at <= stale_before {
                issues.stale_resolutions += 1;
            }
        }

        let publications: Vec<String> =
            sqlx::query_scalar(r#"SELECT "CreatedAt" FROM "ProviderCollectionRefreshResolution""#)
                .fetch_all(&self.pool)
                .await
                .map_err(sqlx_error)?;
        for created_at in publications {
            let created_at = crate::repositories::common::decode_datetime(
                "provider collection refresh publication",
                &created_at,
            )
            .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            if created_at <= stale_before {
                issues.stale_resolutions += 1;
            }
        }

        let intents = sqlx::query(
            r#"SELECT "State", "UpdatedAt"
               FROM "ScanPlayoutRefreshIntent""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        for row in intents {
            let state = row.try_get::<i64, _>("State").map_err(sqlx_error)?;
            if !matches!(state, INTENT_STATE_PENDING | INTENT_STATE_BUILDING) {
                issues.failed_intents += 1;
                continue;
            }
            let updated_at = row.try_get::<String, _>("UpdatedAt").map_err(sqlx_error)?;
            let updated_at = crate::repositories::common::decode_datetime(
                "scan playout refresh intent update",
                &updated_at,
            )
            .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            if updated_at <= stale_before {
                issues.stale_intents += 1;
            }
        }

        if issues.total() == 0 {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "Scan refresh state needs attention: {} unresolved impact resolution(s), {} stale impact resolution(s), {} failed playout refresh intent(s), and {} stale playout refresh intent(s). Review scanner and scheduling logs, then retry the affected scan or playout refresh.",
                issues.unresolved_resolutions,
                issues.stale_resolutions,
                issues.failed_intents,
                issues.stale_intents,
            ),
            format!("{} scan refresh state error(s)", issues.total()),
        )
        .with_link("/playouts"))
    }
}
