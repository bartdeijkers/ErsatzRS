//! Explicit admission of authored teletext on final encoded output only.

use std::{sync::Arc, time::Duration};

use ersatzrs_ffmpeg::{
    pipeline::OutputFormat,
    teletext::{EncodedTeletextPageSet, TeletextOutputConfig, TeletextOutputGate},
};
use tokio::time::Instant;

use super::teletext_output::TeletextPumpError;

/// The host-owned output boundary to which a process belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextOutputRole {
    /// Final direct MPEG-TS output delivered to the response sink.
    FinalDirectMpegTs,
    /// Final MPEG-TS wrapper output after all source remuxing.
    FinalWrapperMpegTs,
    /// An internal per-item stream that another process will remux.
    InternalItem,
    /// A worker writing HLS segments rather than final response stdout.
    HlsWorker,
    /// A final container whose format is not MPEG-TS.
    OtherContainer,
    /// A temporary capacity-wait slate with a separate source clock.
    CapacitySlate,
}

/// Immutable authored pages and explicit policy for one final-output admission.
///
/// This does not establish source PSI/PCR ownership, sink delivery or a durable
/// publication generation. The forwarding gate verifies the actual stream.
pub struct TeletextOutputBinding {
    config: TeletextOutputConfig,
    pages: EncodedTeletextPageSet,
    first_byte_timeout: Duration,
}

impl TeletextOutputBinding {
    /// Validate the encoded page set's gate and asynchronous policy together.
    /// # Errors
    /// Invalid gate bounds or unrepresentable/zero timing fail before admission.
    pub fn new(
        config: TeletextOutputConfig,
        pages: EncodedTeletextPageSet,
        first_byte_timeout: Duration,
    ) -> Result<Self, TeletextPumpError> {
        let now = Instant::now();
        if first_byte_timeout.is_zero()
            || now.checked_add(first_byte_timeout).is_none()
            || now.checked_add(config.max_hold).is_none()
        {
            return Err(TeletextPumpError::Configuration);
        }
        drop(TeletextOutputGate::new(config, pages.clone())?);
        Ok(Self {
            config,
            pages,
            first_byte_timeout,
        })
    }

    /// Explicit immutable transport and retention policy.
    #[must_use]
    pub const fn config(&self) -> TeletextOutputConfig {
        self.config
    }

    /// Completely encoded authored content admitted for this output.
    #[must_use]
    pub const fn pages(&self) -> &EncodedTeletextPageSet {
        &self.pages
    }

    /// Caller-supplied bound on the producer's first output byte.
    #[must_use]
    pub const fn first_byte_timeout(&self) -> Duration {
        self.first_byte_timeout
    }
}

/// An explicit process role, optionally carrying an immutable authored service.
#[derive(Clone)]
pub struct TeletextOutputAdmission {
    role: TeletextOutputRole,
    binding: Option<Arc<TeletextOutputBinding>>,
}

impl TeletextOutputAdmission {
    /// Describe an ordinary output without an authored service.
    #[must_use]
    pub const fn plain(role: TeletextOutputRole) -> Self {
        Self {
            role,
            binding: None,
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
            role,
            binding: Some(binding),
        })
    }

    /// Explicit boundary identity supplied by the process owner.
    #[must_use]
    pub const fn role(&self) -> TeletextOutputRole {
        self.role
    }

    /// Optional immutable service; cloning admission preserves its identity.
    #[must_use]
    pub fn binding(&self) -> Option<&TeletextOutputBinding> {
        self.binding.as_deref()
    }

    pub(crate) fn cloned_binding(&self) -> Option<Arc<TeletextOutputBinding>> {
        self.binding.clone()
    }

    /// Verify the actual generated format before a process is launched.
    /// # Errors
    /// A role/format mismatch or a binding outside MPEG-TS fails admission.
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
        if !matches_role || (self.binding.is_some() && format != OutputFormat::MpegTs) {
            return Err(TeletextPumpError::Configuration);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "teletext_binding_tests.rs"]
mod tests;
