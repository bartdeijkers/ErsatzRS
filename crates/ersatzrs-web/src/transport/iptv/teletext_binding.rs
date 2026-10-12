//! Explicit admission of authored teletext on final encoded output only.

use std::{sync::Arc, time::Duration};

use ersatzrs_ffmpeg::{
    pipeline::OutputFormat,
    teletext::{
        EncodedTeletextAuthoredService, EncodedTeletextPageSet, TeletextOutputConfig,
        TeletextOutputError, TeletextOutputGate, TeletextServiceDescriptor,
    },
};
use tokio::time::Instant;

use super::teletext_output::TeletextPumpError;

#[path = "teletext_control.rs"]
pub(super) mod control;
pub use control::{
    teletext_output_control_channel, TeletextOutputControl, TeletextOutputTransitionPhase,
    TeletextOutputTransitionRejection, TeletextOutputTransitionStatus, TeletextOutputUpdate,
    TeletextOutputUpdateError,
};

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
    control: Option<TeletextOutputControl>,
    optional_startup_bytes: Option<usize>,
    source_discovery: Option<TeletextSourceDiscovery>,
}

#[derive(Clone, Copy)]
pub(super) struct TeletextSourceDiscovery {
    pub maximum_bytes: usize,
    pub maximum_time: Duration,
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
        Self::new_authored_shared(config, Arc::new(service), first_byte_timeout)
    }

    /// Share the exact complete service without copying its compiled contents.
    /// Each forwarding gate still owns its independent transport/output epoch.
    /// This provides no publication or current channel-selection authority.
    /// # Errors
    /// Rejects the same timing, gate policy and authored cycle bounds.
    pub fn new_authored_shared(
        config: TeletextOutputConfig,
        service: Arc<EncodedTeletextAuthoredService>,
        first_byte_timeout: Duration,
    ) -> Result<Self, TeletextPumpError> {
        Self::new_inner(
            config,
            TeletextOutputContent::Authored(service),
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
            control: None,
            optional_startup_bytes: None,
            source_discovery: None,
        })
    }

    /// Attach latest trusted engine proposals, without activating a publisher.
    /// Each output gate observes updates independently; shared latest statuses
    /// do not certify that all outputs acknowledged a replacement.
    #[must_use]
    pub fn with_control(mut self, control: TeletextOutputControl) -> Self {
        self.control = Some(control);
        self
    }

    /// Permit exact raw playback if authored admission fails before any sink write.
    /// Every later failure remains terminal for this output epoch.
    /// # Errors
    /// Rejects a zero journal or one beyond the gate capacity or 32 MiB.
    pub fn with_optional_startup(
        mut self,
        maximum_bytes: usize,
    ) -> Result<Self, TeletextPumpError> {
        if maximum_bytes == 0
            || maximum_bytes > 32 * 1024 * 1024
            || self
                .config
                .max_buffer_packets
                .checked_mul(188)
                .is_none_or(|capacity| maximum_bytes > capacity)
            || self
                .source_discovery
                .is_some_and(|policy| policy.maximum_bytes > maximum_bytes)
        {
            return Err(TeletextPumpError::Configuration);
        }
        self.optional_startup_bytes = Some(maximum_bytes);
        Ok(self)
    }

    pub(super) const fn optional_startup_bytes(&self) -> Option<usize> {
        self.optional_startup_bytes
    }

    /// Discover the sole actual source program and PCR before optional admission.
    /// Other clock/rate/retention values remain explicit host policy. Startup
    /// rejection preserves raw playback before the first sink-write attempt.
    /// # Errors
    /// Rejects unbounded discovery time/bytes or incompatible journal capacity.
    pub fn with_source_discovery(
        self,
        maximum_bytes: usize,
        maximum_time: Duration,
    ) -> Result<Self, TeletextPumpError> {
        if maximum_bytes == 0
            || maximum_bytes > 32_768 * 188
            || maximum_time.is_zero()
            || Instant::now().checked_add(maximum_time).is_none()
        {
            return Err(TeletextPumpError::Configuration);
        }
        let mut binding = if let Some(journal) = self.optional_startup_bytes {
            if journal < maximum_bytes {
                return Err(TeletextPumpError::Configuration);
            }
            self
        } else {
            self.with_optional_startup(maximum_bytes)?
        };
        binding.source_discovery = Some(TeletextSourceDiscovery {
            maximum_bytes,
            maximum_time,
        });
        Ok(binding)
    }

    pub(super) const fn source_discovery(&self) -> Option<TeletextSourceDiscovery> {
        self.source_discovery
    }

    pub(super) fn service_descriptor(&self) -> TeletextServiceDescriptor {
        match &self.content {
            TeletextOutputContent::Base(pages) => TeletextServiceDescriptor::from_page_set(pages),
            TeletextOutputContent::Authored(service) => {
                TeletextServiceDescriptor::from_authored_service(service)
            }
        }
    }

    pub(super) fn with_discovered_source(
        &self,
        program: u16,
        pcr: u16,
    ) -> Result<Self, TeletextPumpError> {
        let mut config = self.config;
        config.program = program;
        config.clock.pcr_pid = pcr;
        let mut binding = Self::new_inner(config, self.content.clone(), self.first_byte_timeout)?;
        binding.control.clone_from(&self.control);
        binding.optional_startup_bytes = self.optional_startup_bytes;
        Ok(binding)
    }

    pub(crate) fn cloned_control(&self) -> Option<TeletextOutputControl> {
        self.control.clone()
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
pub(super) mod authored_tests;
