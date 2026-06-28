use super::super::{
    async_trait, FfmpegRuntimeReport, HealthCheck, HealthCheckResult, HealthCheckStatus, Result,
};

pub(crate) struct FFmpegRuntimeAcquisitionHealthCheck {
    runtime: FfmpegRuntimeReport,
}

impl FFmpegRuntimeAcquisitionHealthCheck {
    pub(crate) fn new(runtime: FfmpegRuntimeReport) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl HealthCheck for FFmpegRuntimeAcquisitionHealthCheck {
    fn title(&self) -> &'static str {
        "FFmpeg Runtime"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let acquisition = &self.runtime.acquisition;
        let source = acquisition.source.to_string();
        let installed = acquisition
            .installed_tag
            .as_deref()
            .unwrap_or("not managed");
        let latest = acquisition.latest_tag.as_deref().unwrap_or("unknown");
        let version = self.runtime.ffmpeg.version.to_string();

        if let Some(error) = acquisition.update_check_error.as_deref() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "FFmpeg runtime resolved from {source}; installed ErsatzRS-ffmpeg {installed}, latest {latest}, FFmpeg {version}; update failed: {error}"
                ),
                "FFmpeg runtime update failed",
            ));
        }

        if acquisition.update_available {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "FFmpeg runtime resolved from {source}; installed ErsatzRS-ffmpeg {installed}, latest {latest}, FFmpeg {version}"
                ),
                "FFmpeg update available",
            )
            .with_link("https://github.com/bartdeijkers/ErsatzRS-ffmpeg/releases"));
        }

        if acquisition.downloaded_update {
            let previous = acquisition.previous_tag.as_deref().unwrap_or("unknown");
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Pass,
                format!(
                    "FFmpeg runtime updated from ErsatzRS-ffmpeg {previous} to {installed}; FFmpeg {version}"
                ),
                "FFmpeg runtime updated",
            ));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Pass,
            format!(
                "FFmpeg runtime resolved from {source}; installed ErsatzRS-ffmpeg {installed}, latest {latest}, FFmpeg {version}"
            ),
            "FFmpeg runtime is ready",
        ))
    }
}
