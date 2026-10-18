use ersatzrs_ffmpeg::{TeletextDecoderCapability, TeletextDecoderStatus};

use super::super::{async_trait, HealthCheck, HealthCheckResult, HealthCheckStatus, Result};

#[path = "teletext_decoder_carriage.rs"]
mod carriage;
use carriage::CarriageContext;

/// ADR 0121: decoder readiness is separate from authored-page delivery.
pub(crate) struct TeletextDecoderHealthCheck {
    capability: TeletextDecoderCapability,
    carriage: Option<CarriageContext>,
}

impl TeletextDecoderHealthCheck {
    pub(crate) fn new(capability: TeletextDecoderCapability) -> Self {
        Self {
            capability,
            carriage: None,
        }
    }

    pub(crate) fn with_channel_delivery(
        mut self,
        pool: crate::db::DbPool,
        capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
    ) -> Self {
        self.carriage = Some(CarriageContext::new(pool, capabilities));
        self
    }
}

#[async_trait]
impl HealthCheck for TeletextDecoderHealthCheck {
    fn title(&self) -> &'static str {
        "Teletext decoding"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        if let Some(context) = &self.carriage {
            if let Some((status, message)) = context.check(&self.capability).await? {
                return Ok(
                    HealthCheckResult::new(self.title(), status, message, message)
                        .with_link("/channels"),
                );
            }
        }
        let (status, message) = match self.capability.status {
            TeletextDecoderStatus::Ready => (
                HealthCheckStatus::Pass,
                "The selected FFmpeg runtime passed teletext page selection, subtitle text and colour bitmap decoding.",
            ),
            TeletextDecoderStatus::MissingDecoder => (
                HealthCheckStatus::Info,
                "Teletext decoding is unavailable. Select an FFmpeg 9 runtime with libzvbi teletext support.",
            ),
            TeletextDecoderStatus::MissingOptions => (
                HealthCheckStatus::Warning,
                "The teletext decoder lacks required page-selection or rendering options. Select a compatible FFmpeg 9 runtime.",
            ),
            TeletextDecoderStatus::HelpProbeFailed => (
                HealthCheckStatus::Warning,
                "The teletext decoder check could not finish. Review FFmpeg runtime availability and permissions.",
            ),
            TeletextDecoderStatus::TextProbeFailed => (
                HealthCheckStatus::Warning,
                "Teletext page and subtitle text decoding failed. Select a compatible FFmpeg 9 runtime.",
            ),
            TeletextDecoderStatus::BitmapProbeFailed => (
                HealthCheckStatus::Warning,
                "Teletext colour bitmap decoding failed. Select a compatible FFmpeg 9 runtime.",
            ),
        };
        Ok(
            HealthCheckResult::new(self.title(), status, message, message)
                .with_link("/settings/ffmpeg"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::health::HealthCheckSummary;

    #[tokio::test]
    async fn optional_absence_is_visible_without_failing_unrelated_playback() {
        let result = TeletextDecoderHealthCheck::new(TeletextDecoderCapability::default())
            .check()
            .await
            .unwrap();
        assert_eq!(result.title, "Teletext decoding");
        assert_eq!(result.status, HealthCheckStatus::Info);
        assert!(result.message.contains("libzvbi"));
        assert_eq!(result.link.as_ref().unwrap().link, "/settings/ffmpeg");
        assert_eq!(
            HealthCheckSummary::from_results(&[result]),
            HealthCheckSummary::default()
        );
    }

    #[tokio::test]
    async fn failed_probes_warn_about_the_affected_operation() {
        for (status, message_fragment) in [
            (TeletextDecoderStatus::MissingOptions, "options"),
            (TeletextDecoderStatus::HelpProbeFailed, "permissions"),
            (TeletextDecoderStatus::TextProbeFailed, "subtitle text"),
            (TeletextDecoderStatus::BitmapProbeFailed, "colour bitmap"),
        ] {
            let result = TeletextDecoderHealthCheck::new(TeletextDecoderCapability {
                status,
                missing_options: vec!["txt_page".into()],
            })
            .check()
            .await
            .unwrap();
            assert_eq!(result.status, HealthCheckStatus::Warning, "{status:?}");
            assert!(result.message.contains(message_fragment), "{status:?}");
            assert_eq!(HealthCheckSummary::from_results(&[result]).errors, 0);
        }
    }

    #[tokio::test]
    async fn successful_decode_does_not_claim_authored_service_readiness() {
        let result = TeletextDecoderHealthCheck::new(TeletextDecoderCapability {
            status: TeletextDecoderStatus::Ready,
            missing_options: Vec::new(),
        })
        .check()
        .await
        .unwrap();
        assert_eq!(result.status, HealthCheckStatus::Pass);
        assert!(result.message.contains("decoding"));
        assert!(!result.message.contains("service"));
    }
}
