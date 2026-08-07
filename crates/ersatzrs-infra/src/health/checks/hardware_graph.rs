use super::super::{
    async_trait, FfmpegRuntimeReport, HealthCheck, HealthCheckResult, HealthCheckStatus, Result,
};

/// Surface runtime-learned hardware-graph denials (ADR 0014, milestone
/// `p8b-01`).
///
/// A denial is not an error condition by itself — ErsatzRS falls back to a
/// proven graph and keeps playing. It is reported because the operator
/// otherwise has no way to know that their GPU is doing less work than the
/// profile implies, and because the underlying cause is often fixable (a
/// driver, a container without the NVIDIA Vulkan ICD, a WSL2 host exposing only
/// llvmpipe).
pub(crate) struct HardwareGraphHealthCheck {
    runtime: FfmpegRuntimeReport,
}

impl HardwareGraphHealthCheck {
    pub(crate) fn new(runtime: FfmpegRuntimeReport) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl HealthCheck for HardwareGraphHealthCheck {
    fn title(&self) -> &'static str {
        "Hardware Graph Capability"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let denials = &self.runtime.capabilities.hardware_graph_denials;
        if denials.is_empty() {
            // No Vulkan on this platform is a normal capability result, not a
            // failure, so "nothing denied" is simply a pass.
            return Ok(HealthCheckResult::pass(self.title()));
        }

        let evidence: Vec<String> = denials
            .iter()
            .map(ersatzrs_ffmpeg::HardwareGraphDenial::evidence)
            .collect();
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Info,
            format!(
                "{} hardware graph{} unavailable and replaced by a proven fallback: {}",
                denials.len(),
                if denials.len() > 1 { "s are" } else { " is" },
                evidence.join("; ")
            ),
            "A hardware graph fell back to a slower proven path",
        ))
    }
}
