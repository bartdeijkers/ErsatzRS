use chrono::{Duration, Utc};

use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};
use crate::hls_delivery::{
    HlsDeliveryFallback, HlsDeliveryObservation, HlsDeliveryState,
    CONFIG_KEY_HLS_DELIVERY_FALLBACK_PREFIX, CONFIG_KEY_HLS_DELIVERY_PREFIX,
    HLS_DELIVERY_FALLBACK_HOURS, HLS_DELIVERY_RECENT_MINUTES,
};

pub(crate) struct HlsDeliveryHealthCheck {
    pool: Pool<Db>,
}

impl HlsDeliveryHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for HlsDeliveryHealthCheck {
    fn title(&self) -> &'static str {
        "HLS Live Delivery"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT "Value"
               FROM "ConfigElement"
               WHERE "Key" LIKE ?"#,
        )
        .bind(format!("{CONFIG_KEY_HLS_DELIVERY_PREFIX}%"))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        let fallback_rows = sqlx::query(
            r#"SELECT "Value"
               FROM "ConfigElement"
               WHERE "Key" LIKE ?"#,
        )
        .bind(format!("{CONFIG_KEY_HLS_DELIVERY_FALLBACK_PREFIX}%"))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if rows.is_empty() && fallback_rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let recent_after = Utc::now() - Duration::minutes(HLS_DELIVERY_RECENT_MINUTES);
        let mut recent_under_runs = 0_u64;
        let mut active_under_runs = 0_u64;
        let mut malformed = 0_u64;
        let mut active_fallbacks = 0_u64;

        for row in rows {
            let value = row.try_get::<String, _>("Value").map_err(sqlx_error)?;
            let Ok(observation) = serde_json::from_str::<HlsDeliveryObservation>(&value) else {
                malformed += 1;
                continue;
            };
            if observation.last_under_run_at < recent_after {
                continue;
            }
            recent_under_runs += 1;
            if observation.state == HlsDeliveryState::UnderRun {
                active_under_runs += 1;
            }
        }

        for row in fallback_rows {
            let value = row.try_get::<String, _>("Value").map_err(sqlx_error)?;
            let Ok(fallback) = serde_json::from_str::<HlsDeliveryFallback>(&value) else {
                malformed += 1;
                continue;
            };
            if fallback.expires_at > Utc::now() {
                active_fallbacks += 1;
            }
        }

        if malformed > 0 {
            return Err(HealthCheckError::Serialization(format!(
                "{malformed} persisted HLS delivery observation(s) are invalid"
            )));
        }
        if recent_under_runs == 0 && active_fallbacks == 0 {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "{recent_under_runs} channel(s) delivered live HLS media slower than wall time in the last {HLS_DELIVERY_RECENT_MINUTES} minutes; {active_under_runs} had not recovered when last observed; {active_fallbacks} channel(s) are using a verified 720p fallback for up to {HLS_DELIVERY_FALLBACK_HOURS} hours. Review the streaming logs and run the delivery-aware TSDuck playback gate before changing provider settings."
            ),
            format!(
                "{recent_under_runs} recent live HLS delivery under-run(s); {active_fallbacks} temporary quality fallback(s)"
            ),
        )
        .with_link("/system/troubleshooting/mpeg"))
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::hls_delivery::{
        persist_hls_delivery_fallback, persist_hls_delivery_observation, HlsDeliveryObservation,
        HlsDeliveryState,
    };

    async fn test_pool() -> Pool<Db> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT NOT NULL PRIMARY KEY,
                "Value" TEXT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create ConfigElement");
        pool
    }

    fn observation(
        state: HlsDeliveryState,
        last_under_run_at: chrono::DateTime<Utc>,
    ) -> HlsDeliveryObservation {
        HlsDeliveryObservation {
            transitioned: false,
            observed_at: Utc::now(),
            state,
            last_under_run_at,
            wall_seconds: 60.0,
            media_seconds: 28.0,
            delivery_ratio_percent: 46.7,
            no_progress_seconds: 0.0,
        }
    }

    #[tokio::test]
    async fn no_delivery_observation_is_not_applicable() {
        let pool = test_pool().await;
        let result = HlsDeliveryHealthCheck::new(pool)
            .check()
            .await
            .expect("run health check");

        assert_eq!(result.status, HealthCheckStatus::NotApplicable);
    }

    #[tokio::test]
    async fn recent_under_run_remains_visible_after_recovery() {
        let pool = test_pool().await;
        persist_hls_delivery_observation(
            &pool,
            10,
            &observation(HlsDeliveryState::Recovered, Utc::now()),
        )
        .await
        .expect("persist observation");

        let result = HlsDeliveryHealthCheck::new(pool)
            .check()
            .await
            .expect("run health check");

        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.contains("1 channel(s)"));
        assert!(result.message.contains("0 had not recovered"));
    }

    #[tokio::test]
    async fn stale_under_run_does_not_keep_health_warning_active() {
        let pool = test_pool().await;
        persist_hls_delivery_observation(
            &pool,
            10,
            &observation(
                HlsDeliveryState::UnderRun,
                Utc::now() - Duration::minutes(HLS_DELIVERY_RECENT_MINUTES + 1),
            ),
        )
        .await
        .expect("persist observation");

        let result = HlsDeliveryHealthCheck::new(pool)
            .check()
            .await
            .expect("run health check");

        assert_eq!(result.status, HealthCheckStatus::Pass);
    }

    #[tokio::test]
    async fn verified_quality_fallback_remains_visible_while_delivery_is_stable() {
        let pool = test_pool().await;
        let detected_at = Utc::now() - Duration::minutes(2);
        persist_hls_delivery_fallback(&pool, 10, detected_at, Utc::now(), 720)
            .await
            .expect("persist fallback");

        let result = HlsDeliveryHealthCheck::new(pool)
            .check()
            .await
            .expect("run health check");

        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.contains("1 channel(s) are using"));
        assert!(result.message.contains("720p"));
    }
}
