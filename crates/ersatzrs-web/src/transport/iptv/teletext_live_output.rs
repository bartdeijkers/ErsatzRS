//! Response-owned accepted selection on one qualified final MPEG-TS clock.
//!
//! Initially absent output keeps its original PSI and AV. Changes use existing
//! application validation and safe pump boundaries; pending documents never
//! become output. Each response owns its own bounded recovery monitor.

use std::sync::Arc;

use ersatzrs_app::{teletext_design_documents::TeletextDesignDocumentBudget, ApplicationServices};
use ersatzrs_ffmpeg::FfmpegCommand;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::Instant,
};

use crate::host::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_generations::AcceptedTeletextOutputCandidate,
};

use super::{
    teletext_accepted_output::source_error,
    teletext_binding::TeletextOutputBinding,
    teletext_optional_output::{
        forward_optional_seeded_teletext_output, replay_direct_prefix, OptionalTeletextOutput,
    },
    teletext_output::TeletextPumpError,
    teletext_selected_output::SelectedTeletextOutputCandidate,
    teletext_source_policy::TeletextSourcePolicy,
    teletext_source_probe::{probe_teletext_source_optional, TeletextSourceProbePolicy},
};

#[path = "teletext_accepted_updates.rs"]
mod updates;
use updates::{start_accepted_updates, TeletextAcceptedUpdatesContext};
pub use updates::{TeletextAcceptedUpdatePolicy, TeletextAcceptedUpdatesConfigurationError};

/// Explicit native dependencies and initial sealed content, without a running
/// task. Forwarding starts a fresh monitor owned by that actual response.
pub struct TeletextLiveOutputIntent {
    trace_id: uuid::Uuid,
    context: TeletextAcceptedUpdatesContext,
    initial: Option<Arc<SelectedTeletextOutputCandidate>>,
}

impl TeletextLiveOutputIntent {
    /// Keep host-selected bounds and optional already accepted native content.
    /// This does not accept, install or invoke a package.
    /// # Errors
    /// Rejects invalid channel/budget and candidates belonging to another channel.
    pub fn new(
        application: Arc<ApplicationServices>,
        channel_id: i64,
        budget: TeletextDesignDocumentBudget,
        encoding: TeletextChannelDocumentEncodingPolicy,
        source: TeletextSourcePolicy,
        timing: TeletextAcceptedUpdatePolicy,
        initial: Option<Arc<AcceptedTeletextOutputCandidate>>,
    ) -> Result<Self, TeletextPumpError> {
        Self::new_selected(
            application,
            channel_id,
            budget,
            encoding,
            source,
            timing,
            initial
                .map(|candidate| Arc::new(SelectedTeletextOutputCandidate::Base(candidate, None))),
        )
    }

    /// Internal ordinary-route selection includes sealed scoped absence.
    pub(crate) fn new_selected(
        application: Arc<ApplicationServices>,
        channel_id: i64,
        budget: TeletextDesignDocumentBudget,
        encoding: TeletextChannelDocumentEncodingPolicy,
        source: TeletextSourcePolicy,
        timing: TeletextAcceptedUpdatePolicy,
        initial: Option<Arc<SelectedTeletextOutputCandidate>>,
    ) -> Result<Self, TeletextPumpError> {
        if channel_id <= 0
            || budget.max_serialized_bytes == 0
            || initial.as_ref().is_some_and(|candidate| {
                candidate.generation().request.channel_id != channel_id
                    || candidate.channel_id() != channel_id
            })
        {
            return Err(TeletextPumpError::Configuration);
        }
        Ok(Self {
            trace_id: uuid::Uuid::new_v4(),
            context: TeletextAcceptedUpdatesContext {
                application,
                channel_id,
                budget,
                encoding,
                source,
                timing,
                guide: None,
            },
            initial,
        })
    }

    /// Opt in to independently bounded guide-source recovery; legacy constructors
    /// remain static-only unless the composition root supplies these dependencies.
    pub(super) fn with_guide_runtime(
        mut self,
        guide: Option<Arc<super::teletext_selected_output::GuideRuntime>>,
    ) -> Self {
        self.context.guide = guide;
        self
    }

    pub(in crate::transport::iptv) const fn trace_id(&self) -> uuid::Uuid {
        self.trace_id
    }

    /// Channel isolation is checked again by the actual route owner.
    #[must_use]
    pub const fn channel_id(&self) -> i64 {
        self.context.channel_id
    }

    async fn current_initial(&self, deadline: Instant) -> Option<Arc<updates::NativeCandidate>> {
        let candidate = self.initial.as_ref()?;
        if !candidate
            .confirm(
                &self.context.application,
                self.context.budget.max_serialized_bytes,
                deadline.into_std(),
            )
            .await
        {
            return None;
        }
        let prepared =
            updates::NativeCandidate::prepare(&self.context, Arc::clone(candidate), None, deadline)
                .await
                .ok()?;
        let confirmed = candidate
            .confirm(
                &self.context.application,
                self.context.budget.max_serialized_bytes,
                deadline.into_std(),
            )
            .await;
        (confirmed && prepared.fields_current_at(chrono::Utc::now())).then_some(prepared)
    }
}

/// Forward one source and one sink with response-owned accepted-state recovery.
/// Qualification/probe rejection before commitment replays the original prefix.
/// Off remains a qualified absent gate, allowing later explicit accepted output
/// without restarting `FFmpeg` or rebasing source PCR. Dropping this future aborts
/// its monitor; real blocking workers retain their existing admission ownership.
/// # Errors
/// Source/sink failures and faults after the first write are terminal.
pub async fn forward_live_teletext_output(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    command: &FfmpegCommand,
    intent: &TeletextLiveOutputIntent,
    deadline: Instant,
) -> Result<OptionalTeletextOutput, TeletextPumpError> {
    let qualified = command
        .mpegts_output_policy()
        .and_then(|command| intent.context.source.qualify(command).ok());
    let Some(qualified) = qualified else {
        return Ok(OptionalTeletextOutput {
            bytes: replay_direct_prefix(source, sink, Vec::new()).await?,
            rejection: Some(TeletextPumpError::Configuration),
        });
    };
    let limits = qualified.limits();
    let probe_deadline = Instant::now()
        .checked_add(limits.probe_maximum_time)
        .ok_or(TeletextPumpError::Configuration)?
        .min(deadline);
    // Discover the actual unchanged source independently of optional content.
    // The seeded gate checks authored PMT capacity before its first sink write.
    let admitted = match probe_teletext_source_optional(
        source,
        None,
        limits.elementary_pid,
        TeletextSourceProbePolicy {
            maximum_bytes: limits.probe_maximum_bytes,
            deadline: probe_deadline,
            first_byte_timeout: limits.first_byte_timeout,
        },
    )
    .await
    {
        Ok(admitted) => admitted,
        Err(failure) => {
            let (error, terminal) = source_error(failure.reason);
            if terminal {
                return Err(error);
            }
            return Ok(OptionalTeletextOutput {
                bytes: replay_direct_prefix(source, sink, failure.raw_prefix).await?,
                rejection: Some(error),
            });
        }
    };
    // Source discovery awaited bytes. Fence the initial authority afterwards.
    let initial = intent.current_initial(deadline).await;
    let binding = qualified
        .output_config(&admitted.plan)
        .map_err(|_| TeletextPumpError::Configuration)
        .and_then(
            |config| match initial.as_ref().and_then(|candidate| candidate.service()) {
                Some(service) => TeletextOutputBinding::new_authored_shared(
                    config,
                    Arc::clone(service),
                    limits.first_byte_timeout,
                ),
                None => TeletextOutputBinding::new_absent(config, limits.first_byte_timeout),
            },
        );
    let binding = match binding {
        Ok(binding) => binding,
        Err(error) => {
            return Ok(OptionalTeletextOutput {
                bytes: replay_direct_prefix(source, sink, admitted.prefix.into_bytes()).await?,
                rejection: Some(error),
            });
        }
    };
    let (control, _owner) = start_accepted_updates(intent.context.clone(), initial)
        .map_err(|_| TeletextPumpError::Configuration)?;
    tracing::debug!(
        target: "ersatzrs_web::transport::iptv::teletext_provenance",
        event = "monitor_owner_link",
        intent_id = %intent.trace_id(),
        proposal_slot = %control.trace_id(),
        channel_id = intent.channel_id(),
        "teletext monitor belongs to this live output intent"
    );
    let binding = binding.with_control(control);
    forward_optional_seeded_teletext_output(
        source,
        sink,
        &binding,
        admitted.prefix,
        limits.startup_maximum_bytes,
    )
    .await
}
