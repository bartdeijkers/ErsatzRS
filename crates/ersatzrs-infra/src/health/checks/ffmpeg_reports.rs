use super::super::{
    async_trait, load_config, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    CONFIG_KEY_FFMPEG_SAVE_REPORTS,
};

pub(crate) struct FFmpegReportsHealthCheck {
    pool: Pool<Db>,
}

impl FFmpegReportsHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for FFmpegReportsHealthCheck {
    fn title(&self) -> &'static str {
        "FFmpeg Reports"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let value = load_config(&self.pool, CONFIG_KEY_FFMPEG_SAVE_REPORTS).await?;
        let enabled = value
            .as_deref()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("true") || raw == "1");

        if enabled {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                "FFmpeg troubleshooting reports are enabled and may use a lot of disk space",
                "FFmpeg troubleshooting reports are enabled",
            ));
        }

        Ok(HealthCheckResult::pass(self.title()))
    }
}
