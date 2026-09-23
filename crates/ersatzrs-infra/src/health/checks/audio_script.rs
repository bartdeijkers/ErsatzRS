use super::super::{async_trait, HealthCheck, HealthCheckResult, HealthCheckStatus, Result};

pub(crate) struct AudioScriptHealthCheck;

#[async_trait]
impl HealthCheck for AudioScriptHealthCheck {
    fn title(&self) -> &'static str {
        "JavaScript Audio Selection"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let failures = crate::audio_script::failures_since_startup();
        Ok(if failures == 0 {
            HealthCheckResult::new(self.title(), HealthCheckStatus::Pass,
                "No JavaScript audio-selector loading or execution failures recorded since startup; this does not prove unexecuted scripts are valid",
                "No selector failures recorded")
        } else {
            HealthCheckResult::new(self.title(), HealthCheckStatus::Warning,
                format!("{failures} JavaScript audio-selector loading or execution failures recorded since startup. Playback used built-in selection. Review scripts/audio-stream-selector and the ADR 0096 execution limits; the counter resets on restart."),
                "JavaScript audio selection fell back")
        })
    }
}
