use super::super::{
    async_trait, runtime_issue_name, FfmpegRuntimeReport, HealthCheck, HealthCheckResult,
    HealthCheckStatus, Result,
};

pub(crate) struct FFmpegCapabilitiesHealthCheck {
    runtime: FfmpegRuntimeReport,
}

impl FFmpegCapabilitiesHealthCheck {
    pub(crate) fn new(runtime: FfmpegRuntimeReport) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl HealthCheck for FFmpegCapabilitiesHealthCheck {
    fn title(&self) -> &'static str {
        "FFmpeg Capabilities"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let missing: Vec<String> = self
            .runtime
            .issues
            .iter()
            .filter(|issue| issue.is_error())
            .map(runtime_issue_name)
            .collect();

        if missing.is_empty() {
            if self.runtime.capabilities.hardware_decode_denials.is_empty() {
                return Ok(HealthCheckResult::not_applicable(self.title()));
            }

            let denials = self
                .runtime
                .capabilities
                .hardware_decode_denials
                .iter()
                .map(|denial| {
                    format!(
                        "{:?} {} {}-bit",
                        denial.acceleration, denial.codec, denial.bit_depth
                    )
                })
                .collect::<Vec<_>>();
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "FFmpeg hardware decode fallback is active for: [{}]",
                    denials.join(", ")
                ),
                "FFmpeg hardware decode fallback is active",
            ));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "FFmpeg is missing required capabilities and will NOT work correctly: [{}]",
                missing.join(", ")
            ),
            "FFmpeg is missing required capabilities and will NOT work correctly",
        ))
    }
}
