//! Final-output ownership, with fixed bindings or explicit accepted originals.

use std::sync::Arc;

use ersatzrs_app::ApplicationServices;
use ersatzrs_ffmpeg::{pipeline::OutputFormat, FfmpegCommand};
use tokio::time::Instant;

use super::{TeletextOutputBinding, TeletextOutputRole, TeletextPumpError};
use crate::transport::iptv::teletext_accepted_output::TeletextAcceptedOutputIntent;

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

    /// Accepted-original supervision currently supports the direct final owner.
    /// Retained/wrapper command provenance remains a separate implementation.
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
    /// source discovery finds a role whose provenance is not supported yet.
    pub(crate) fn with_optional_accepted(
        intent: Arc<TeletextAcceptedOutputIntent>,
        application: Arc<ApplicationServices>,
        deadline: Instant,
    ) -> Result<Self, TeletextPumpError> {
        let mut output = Self::with_accepted(
            TeletextOutputRole::FinalDirectMpegTs,
            intent,
            application,
            deadline,
        )?;
        output.optional_accepted = true;
        Ok(output)
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
        self.binding.is_some() || self.accepted.is_some()
    }

    pub(crate) fn accepted(&self) -> Option<&AcceptedTeletextAdmission> {
        self.accepted.as_deref()
    }

    pub(crate) fn command(&self) -> Option<&FfmpegCommand> {
        self.command.as_deref()
    }

    /// Snapshot this attempt's actual argv only for accepted-original forwarding.
    /// Opaque retry commands replace earlier hints too; no policy is inferred.
    pub(crate) fn with_command(&self, command: &FfmpegCommand) -> Self {
        let mut next = self.clone();
        if next.accepted.is_some() {
            next.command = Some(Arc::new(command.clone()));
        }
        next
    }

    pub(crate) fn matches_channel(&self, channel_id: i64) -> bool {
        self.accepted.as_ref().is_none_or(|accepted| {
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
