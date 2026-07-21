use ersatzrs_core::health::{
    HealthCheck, HealthCheckError, HealthCheckResult, HealthCheckStatus, Result,
};
use ersatzrs_core::repositories::RepositoryError;

use crate::db::Db;
use crate::hls_worker::load_hls_work_ahead_limit;
use crate::transcode_capacity::load_active_transcode_limit;
use sqlx::Pool;

pub(crate) struct PlaybackDataAccessHealthCheck {
    pool: Pool<Db>,
}

impl PlaybackDataAccessHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }

    fn invalid_value(setting: &str, error: &RepositoryError) -> HealthCheckResult {
        HealthCheckResult::new(
            "Playback Data Access",
            HealthCheckStatus::Fail,
            format!("The persisted {setting} is invalid: {error}"),
            format!("Invalid {setting}"),
        )
        .with_link("/system/ffmpeg")
    }

    fn operational_error(setting: &str, error: &RepositoryError) -> HealthCheckError {
        HealthCheckError::Database(format!("failed to load {setting}: {error}"))
    }
}

#[async_trait::async_trait]
impl HealthCheck for PlaybackDataAccessHealthCheck {
    fn title(&self) -> &'static str {
        "Playback Data Access"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        if let Err(error) = load_active_transcode_limit(&self.pool).await {
            return match error {
                error @ RepositoryError::Decode { .. } => {
                    Ok(Self::invalid_value("active transcode limit", &error))
                }
                error => Err(Self::operational_error("active transcode limit", &error)),
            };
        }

        if let Err(error) = load_hls_work_ahead_limit(&self.pool).await {
            return match error {
                error @ RepositoryError::Decode { .. } => {
                    Ok(Self::invalid_value("HLS work-ahead limit", &error))
                }
                error => Err(Self::operational_error("HLS work-ahead limit", &error)),
            };
        }

        Ok(HealthCheckResult::pass(self.title()))
    }
}
