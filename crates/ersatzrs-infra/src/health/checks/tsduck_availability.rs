use super::super::{async_trait, HealthCheck, HealthCheckResult, HealthCheckStatus, Result};

pub(crate) struct TsduckAvailabilityHealthCheck;

impl TsduckAvailabilityHealthCheck {
    pub(crate) fn new() -> Self {
        Self
    }
}

#[async_trait]
impl HealthCheck for TsduckAvailabilityHealthCheck {
    fn title(&self) -> &'static str {
        "TSDuck MPEG-TS Troubleshooting"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let availability = crate::tsduck::detect_tsduck().await;
        if availability.available {
            let version = availability
                .version
                .unwrap_or_else(|| "available".to_owned());
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Info,
                format!("TSDuck is available for in-app MPEG-TS troubleshooting: {version}"),
                "TSDuck MPEG-TS troubleshooting is available",
            )
            .with_link("/system/troubleshooting/mpeg"));
        }

        let detail = availability
            .error
            .unwrap_or_else(|| "tsp was not found on PATH".to_owned());
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::NotApplicable,
            format!("TSDuck is not available; MPEG-TS troubleshooting is disabled: {detail}"),
            "TSDuck MPEG-TS troubleshooting is not available",
        ))
    }
}
