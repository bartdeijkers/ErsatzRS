//! Final-output ownership, with fixed bindings or explicit accepted originals.

use std::sync::Arc;

use ersatzrs_app::ApplicationServices;
use ersatzrs_ffmpeg::{pipeline::OutputFormat, FfmpegCommand};
use tokio::time::Instant;

use super::{TeletextOutputBinding, TeletextOutputRole, TeletextPumpError};
use crate::transport::iptv::teletext_accepted_output::TeletextAcceptedOutputIntent;
use crate::transport::iptv::teletext_live_output::TeletextLiveOutputIntent;

pub(crate) struct LiveTeletextAdmission {
    pub intent: Arc<TeletextLiveOutputIntent>,
    pub deadline: Instant,
}

/// Original route ownership; a command snapshot belongs to a particular child.
pub(crate) struct AcceptedTeletextAdmission {
    pub intent: Arc<TeletextAcceptedOutputIntent>,
    pub application: Arc<ApplicationServices>,
    pub deadline: Instant,
}

/// An explicit process role, optionally carrying one authored output authority.
/// Private constructors keep accepted originals and fixed bindings exclusive.
#[derive(Clone)]
pub struct TeletextOutputAdmission {
    role: TeletextOutputRole,
    binding: Option<Arc<TeletextOutputBinding>>,
    accepted: Option<Arc<AcceptedTeletextAdmission>>,
    live: Option<Arc<LiveTeletextAdmission>>,
    command: Option<Arc<FfmpegCommand>>,
    optional_accepted: bool,
}

impl TeletextOutputAdmission {
    /// Describe an ordinary output without an authored service.
    #[must_use]
    pub const fn plain(role: TeletextOutputRole) -> Self {
        Self {
            role,
            binding: None,
            accepted: None,
            live: None,
            command: None,
            optional_accepted: false,
        }
    }

    /// Admit an immutable authored service on a final MPEG-TS boundary.
    /// # Errors
    /// Internal, worker, other-container and slate roles cannot carry bindings.
    pub fn with_binding(
        role: TeletextOutputRole,
        binding: Arc<TeletextOutputBinding>,
    ) -> Result<Self, TeletextPumpError> {
        if !matches!(
            role,
            TeletextOutputRole::FinalDirectMpegTs | TeletextOutputRole::FinalWrapperMpegTs
        ) {
            return Err(TeletextPumpError::Configuration);
        }
        Ok(Self {
            binding: Some(binding),
            ..Self::plain(role)
        })
    }

    /// Explicit accepted-original admission retains the direct final contract.
    /// Optional wrapper recovery qualifies its own actual command and source.
    pub(crate) fn with_accepted(
        role: TeletextOutputRole,
        intent: Arc<TeletextAcceptedOutputIntent>,
        application: Arc<ApplicationServices>,
        deadline: Instant,
    ) -> Result<Self, TeletextPumpError> {
        if role != TeletextOutputRole::FinalDirectMpegTs {
            return Err(TeletextPumpError::Configuration);
        }
        Ok(Self {
            accepted: Some(Arc::new(AcceptedTeletextAdmission {
                intent,
                application,
                deadline,
            })),
            ..Self::plain(role)
        })
    }

    /// Ordinary output may abandon automatic authored attachment when actual
    /// source discovery rejects the final wrapper or direct source epoch.
    pub(crate) fn with_optional_accepted(
        role: TeletextOutputRole,
        intent: Arc<TeletextAcceptedOutputIntent>,
        application: Arc<ApplicationServices>,
        deadline: Instant,
    ) -> Result<Self, TeletextPumpError> {
        Self::validate_optional_role(role)?;
        Ok(Self {
            accepted: Some(Arc::new(AcceptedTeletextAdmission {
                intent,
                application,
                deadline,
            })),
            optional_accepted: true,
            ..Self::plain(role)
        })
    }

    /// Preserve explicit accepted/fixed contracts; only automatic attachment
    /// can become plain before starting an unsupported source epoch.
    pub(crate) fn without_optional_accepted(self) -> Self {
        if self.optional_accepted {
            Self::plain(self.role)
        } else {
            self
        }
    }

    /// Reject optional authority before configuration or recovery side effects.
    pub(crate) fn validate_optional_role(
        role: TeletextOutputRole,
    ) -> Result<(), TeletextPumpError> {
        if matches!(
            role,
            TeletextOutputRole::FinalDirectMpegTs | TeletextOutputRole::FinalWrapperMpegTs
        ) {
            Ok(())
        } else {
            Err(TeletextPumpError::Configuration)
        }
    }

    /// Opt-in response-owned selection is tied to one final source clock.
    pub(crate) fn with_optional_live(
        role: TeletextOutputRole,
        intent: Arc<TeletextLiveOutputIntent>,
        deadline: Instant,
    ) -> Result<Self, TeletextPumpError> {
        Self::validate_optional_role(role)?;
        Ok(Self {
            live: Some(Arc::new(LiveTeletextAdmission { intent, deadline })),
            optional_accepted: true,
            ..Self::plain(role)
        })
    }

    pub(crate) fn live(&self) -> Option<&LiveTeletextAdmission> {
        self.live.as_deref()
    }

    /// Explicit boundary identity supplied by the process owner.
    #[must_use]
    pub const fn role(&self) -> TeletextOutputRole {
        self.role
    }

    /// Optional immutable fixed service; cloning preserves its identity.
    #[must_use]
    pub fn binding(&self) -> Option<&TeletextOutputBinding> {
        self.binding.as_deref()
    }

    pub(crate) fn cloned_binding(&self) -> Option<Arc<TeletextOutputBinding>> {
        self.binding.clone()
    }

    /// Authored output requires terminal supervision even after raw fallback.
    #[must_use]
    pub const fn has_authored_output(&self) -> bool {
        self.binding.is_some() || self.accepted.is_some() || self.live.is_some()
    }

    pub(crate) fn accepted(&self) -> Option<&AcceptedTeletextAdmission> {
        self.accepted.as_deref()
    }

    pub(crate) fn command(&self) -> Option<&FfmpegCommand> {
        self.command.as_deref()
    }

    /// Snapshot this attempt's actual argv for forwarding or finite completion.
    /// Opaque retry commands replace earlier hints too; no policy is inferred.
    pub(crate) fn with_command(&self, command: &FfmpegCommand) -> Self {
        let mut next = self.clone();
        let finite_progress = next.role == TeletextOutputRole::InternalItem
            && command
                .mpegts_output_policy()
                .is_some_and(ersatzrs_ffmpeg::MpegTsOutputPolicy::reports_stderr_progress);
        next.command = (next.accepted.is_some() || next.live.is_some() || finite_progress)
            .then(|| Arc::new(command.clone()));
        next
    }

    pub(crate) fn matches_channel(&self, channel_id: i64) -> bool {
        self.live
            .as_ref()
            .is_none_or(|live| live.intent.channel_id() == channel_id)
            && self.accepted.as_ref().is_none_or(|accepted| {
                accepted.intent.candidate().generation().request.channel_id == channel_id
            })
    }

    /// Verify the actual generated format before a process is launched.
    /// # Errors
    /// A role/format mismatch or authored content outside MPEG-TS fails admission.
    pub fn validate_format(&self, format: OutputFormat) -> Result<(), TeletextPumpError> {
        let matches_role = match self.role {
            TeletextOutputRole::FinalDirectMpegTs
            | TeletextOutputRole::FinalWrapperMpegTs
            | TeletextOutputRole::InternalItem
            | TeletextOutputRole::CapacitySlate => format == OutputFormat::MpegTs,
            TeletextOutputRole::HlsWorker => {
                matches!(format, OutputFormat::Hls | OutputFormat::HlsMp4)
            }
            TeletextOutputRole::OtherContainer => format != OutputFormat::MpegTs,
        };
        if !matches_role || (self.has_authored_output() && format != OutputFormat::MpegTs) {
            return Err(TeletextPumpError::Configuration);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{
        state::{AppState, IptvAccessTokenVerifier},
        teletext_runtime_policy::parse_native_teletext_runtime_policy,
    };
    use crate::transport::iptv::teletext_live_output::TeletextAcceptedUpdatePolicy;
    use ersatzrs_ffmpeg::{build_mpegts_concat_command, OutputMetadata};
    use std::time::Duration;

    fn absent_intent() -> Arc<TeletextLiveOutputIntent> {
        let state = AppState::for_tests(IptvAccessTokenVerifier::disabled());
        let policy = parse_native_teletext_runtime_policy(include_str!(
            "../../../tests/fixtures/teletext-output-policy-v1.json"
        ))
        .unwrap();
        Arc::new(
            TeletextLiveOutputIntent::new_selected(
                state.application,
                1,
                policy.design,
                policy.encoding,
                policy.source,
                TeletextAcceptedUpdatePolicy::new(
                    Duration::from_millis(25),
                    Duration::from_secs(2),
                )
                .unwrap(),
                None,
            )
            .unwrap(),
        )
    }

    #[test]
    fn finite_internal_completion_keeps_only_the_current_sealed_command() {
        let mut request = ersatzrs_ffmpeg::PipelineRequest::new(
            ersatzrs_ffmpeg::InputFile::video("synthetic.mkv"),
            ersatzrs_ffmpeg::DesiredVideoState::h264_1080p(),
        );
        request.state.progress_output = ersatzrs_ffmpeg::FfmpegProgressOutput::Stderr;
        let command = ersatzrs_ffmpeg::build_pipeline_for_acceleration(
            ersatzrs_domain::HardwareAccelerationKind::None,
            request,
        )
        .command(&[]);
        let plain = TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem);
        let captured = plain.with_command(&command);
        assert_eq!(captured.command(), Some(&command));
        assert!(!captured.has_authored_output());
        assert!(plain.command().is_none());

        let mut changed = command.clone();
        let target = changed
            .arguments
            .iter_mut()
            .find(|arg| *arg == "pipe:2")
            .unwrap();
        *target = "pipe:1".into();
        assert!(captured.with_command(&changed).command().is_none());
        let ordinary = build_mpegts_concat_command(
            "synthetic.ffconcat",
            &OutputMetadata::default(),
            Some(5_000_000),
        );
        assert!(captured.with_command(&ordinary).command().is_none());
        assert!(
            TeletextOutputAdmission::plain(TeletextOutputRole::HlsWorker)
                .with_command(&command)
                .command()
                .is_none()
        );
    }

    #[tokio::test]
    async fn optional_live_admission_preserves_final_role_intent_deadline_and_command() {
        let intent = absent_intent();
        let deadline = Instant::now() + Duration::from_secs(20);
        let command = build_mpegts_concat_command(
            "synthetic.ffconcat",
            &OutputMetadata::default(),
            Some(5_000_000),
        );
        assert!(command.mpegts_output_policy().is_some());
        for role in [
            TeletextOutputRole::FinalDirectMpegTs,
            TeletextOutputRole::FinalWrapperMpegTs,
        ] {
            let output =
                TeletextOutputAdmission::with_optional_live(role, Arc::clone(&intent), deadline)
                    .unwrap();
            assert_eq!(output.role(), role);
            assert!(Arc::ptr_eq(&output.live().unwrap().intent, &intent));
            assert_eq!(output.live().unwrap().deadline, deadline);
            assert!(output.validate_format(OutputFormat::MpegTs).is_ok());
            assert!(output.validate_format(OutputFormat::Mp4).is_err());
            assert!(output.matches_channel(1));
            assert!(!output.matches_channel(2));
            assert!(output.binding().is_none() && output.accepted().is_none());
            let captured = output.with_command(&command);
            assert!(Arc::ptr_eq(&captured.live().unwrap().intent, &intent));
            assert_eq!(captured.command().unwrap(), &command);
            let plain = captured.without_optional_accepted();
            assert_eq!(plain.role(), role);
            assert!(!plain.has_authored_output());
            assert!(plain.command().is_none());
        }
    }

    #[tokio::test]
    async fn optional_live_admission_cannot_attach_to_internal_worker_slate_or_other_container() {
        let intent = absent_intent();
        let deadline = Instant::now() + Duration::from_secs(20);
        for role in [
            TeletextOutputRole::InternalItem,
            TeletextOutputRole::HlsWorker,
            TeletextOutputRole::CapacitySlate,
            TeletextOutputRole::OtherContainer,
        ] {
            assert!(matches!(
                TeletextOutputAdmission::with_optional_live(role, Arc::clone(&intent), deadline),
                Err(TeletextPumpError::Configuration)
            ));
        }
    }
}
