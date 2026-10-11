//! Explicit admission of authored teletext on final encoded output only.

use std::{sync::Arc, time::Duration};

use ersatzrs_ffmpeg::{
    pipeline::OutputFormat,
    teletext::{
        EncodedTeletextAuthoredService, EncodedTeletextPageSet, TeletextOutputConfig,
        TeletextOutputError, TeletextOutputGate,
    },
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

/// Exact typed service content retained by one final-output admission.
///
/// This does not establish source PSI/PCR ownership, sink delivery or a durable
/// publication generation. The forwarding gate verifies the actual stream.
#[derive(Clone)]
pub enum TeletextOutputContent {
    Base(EncodedTeletextPageSet),
    Authored(Arc<EncodedTeletextAuthoredService>),
}

impl TeletextOutputContent {
    pub(super) fn new_gate(
        self,
        config: TeletextOutputConfig,
        retained: bool,
    ) -> Result<TeletextOutputGate, TeletextOutputError> {
        match (self, retained) {
            (Self::Base(pages), false) => TeletextOutputGate::new(config, pages),
            (Self::Base(pages), true) => TeletextOutputGate::new_retained(config, pages),
            (Self::Authored(service), false) => {
                TeletextOutputGate::new_authored_shared(config, service)
            }
            (Self::Authored(service), true) => {
                TeletextOutputGate::new_authored_retained_shared(config, service)
            }
        }
    }
}

/// Immutable complete input, shared without relabelling resources as base pages.
pub struct TeletextOutputBinding {
    config: TeletextOutputConfig,
    content: TeletextOutputContent,
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
        Self::new_inner(
            config,
            TeletextOutputContent::Base(pages),
            first_byte_timeout,
        )
    }

    /// Bind all actual authored displays, M29 packets and resources together.
    /// # Errors
    /// Rejects the same timing and gate policy, plus authored cycle bounds.
    pub fn new_authored(
        config: TeletextOutputConfig,
        service: EncodedTeletextAuthoredService,
        first_byte_timeout: Duration,
    ) -> Result<Self, TeletextPumpError> {
        Self::new_inner(
            config,
            TeletextOutputContent::Authored(Arc::new(service)),
            first_byte_timeout,
        )
    }

    fn new_inner(
        config: TeletextOutputConfig,
        content: TeletextOutputContent,
        first_byte_timeout: Duration,
    ) -> Result<Self, TeletextPumpError> {
        let now = Instant::now();
        if first_byte_timeout.is_zero()
            || now.checked_add(first_byte_timeout).is_none()
            || now.checked_add(config.max_hold).is_none()
        {
            return Err(TeletextPumpError::Configuration);
        }
        drop(content.clone().new_gate(config, false)?);
        Ok(Self {
            config,
            content,
            first_byte_timeout,
        })
    }

    /// Explicit immutable transport and retention policy.
    #[must_use]
    pub const fn config(&self) -> TeletextOutputConfig {
        self.config
    }

    /// Base-only encoded pages; complete authored bindings return `None`.
    #[must_use]
    pub fn pages(&self) -> Option<&EncodedTeletextPageSet> {
        match &self.content {
            TeletextOutputContent::Base(pages) => Some(pages),
            TeletextOutputContent::Authored(_) => None,
        }
    }

    /// Complete authored service; base-only bindings return no authored service.
    #[must_use]
    pub fn authored_service(&self) -> Option<&EncodedTeletextAuthoredService> {
        match &self.content {
            TeletextOutputContent::Authored(service) => Some(service),
            TeletextOutputContent::Base(_) => None,
        }
    }

    /// Exact typed immutable input for a fresh output epoch.
    #[must_use]
    pub const fn content(&self) -> &TeletextOutputContent {
        &self.content
    }

    pub(super) fn new_gate(
        &self,
        retained: bool,
    ) -> Result<TeletextOutputGate, TeletextOutputError> {
        self.content.clone().new_gate(self.config, retained)
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

#[cfg(test)]
#[path = "teletext_authored_output_tests.rs"]
mod authored_tests;
