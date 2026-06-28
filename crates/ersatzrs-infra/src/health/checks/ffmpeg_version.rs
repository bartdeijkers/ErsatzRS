use super::super::{
    async_trait, FfmpegRuntimeReport, HealthCheck, HealthCheckResult, HealthCheckStatus, Result,
    REQUIRED_MAJOR, REQUIRED_MINOR,
};

pub(crate) struct FFmpegVersionHealthCheck {
    runtime: FfmpegRuntimeReport,
}

impl FFmpegVersionHealthCheck {
    pub(crate) fn new(runtime: FfmpegRuntimeReport) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl HealthCheck for FFmpegVersionHealthCheck {
    fn title(&self) -> &'static str {
        "FFmpeg Version"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        for tool in [&self.runtime.ffmpeg, &self.runtime.ffprobe] {
            if !tool.version.satisfies(REQUIRED_MAJOR, REQUIRED_MINOR) {
                return Ok(HealthCheckResult::new(
                    self.title(),
                    HealthCheckStatus::Fail,
                    format!(
                        "{} version {} is too old; please install {}.{} or newer!",
                        tool.tool, tool.version, REQUIRED_MAJOR, REQUIRED_MINOR
                    ),
                    format!("{} version is too old", tool.tool),
                ));
            }
        }

        Ok(HealthCheckResult::pass(self.title()))
    }
}
