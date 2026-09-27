use super::super::{
    async_trait, load_config, runtime_issue_name, Db, FfmpegRuntimeReport, HealthCheck,
    HealthCheckResult, HealthCheckStatus, Pool, Result,
};

pub(crate) struct FFmpegCapabilitiesHealthCheck {
    runtime: FfmpegRuntimeReport,
    pool: Pool<Db>,
}

impl FFmpegCapabilitiesHealthCheck {
    pub(crate) fn new(runtime: FfmpegRuntimeReport, pool: Pool<Db>) -> Self {
        Self { runtime, pool }
    }
}

#[async_trait]
impl HealthCheck for FFmpegCapabilitiesHealthCheck {
    fn title(&self) -> &'static str {
        "FFmpeg Capabilities"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let mut missing: Vec<String> = self
            .runtime
            .issues
            .iter()
            .filter(|issue| issue.is_error())
            .map(runtime_issue_name)
            .collect();

        let probe_enabled = load_config(
            &self.pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES,
        )
        .await?
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1");
        if probe_enabled && !self.runtime.capabilities.filters.contains("idet") {
            missing.push("filter idet".to_owned());
        }

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
