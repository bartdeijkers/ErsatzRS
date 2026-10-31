//! Explicit scoped pending intents; saving/preview never changes accepted output.

use std::{fmt, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_publications::{
        TeletextEffectivePublicationValidator, TeletextPublicationStoreError,
        TeletextPublicationValidationError,
    },
    ApplicationServices,
};
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{
        TeletextEffectivePublicationRequestSave, TeletextGuideSourceCheckLimits,
        TeletextGuideSourceFence, TeletextPublicationMutation as M,
        TeletextPublicationRejection as R,
    },
};
use ersatzrs_domain::{EntityId, TeletextPublicationRequest};

use super::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_designs::NativeTeletextDesignError,
    teletext_effective_designs::{
        reload_effective_original_for_publication_with_hold, RestoredEffectiveOriginalDesign,
    },
    teletext_effective_generations::{compiled_shape, frozen_original, operation_deadline},
};

/// Exact rendered target and independent head/scoped-pending CAS.
pub struct TeletextEffectivePublicationTarget {
    pub channel_id: EntityId,
    pub design_id: EntityId,
    pub revision: i64,
    pub expected_digest: String,
    pub expected_design_head: i64,
    pub expected_request_id: Option<EntityId>,
}
impl fmt::Debug for TeletextEffectivePublicationTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextEffectivePublicationTarget")
            .field("channel_id", &self.channel_id)
            .field("design_id", &self.design_id)
            .field("revision", &self.revision)
            .field("expected_design_head", &self.expected_design_head)
            .field("expected_request_id", &self.expected_request_id)
            .finish_non_exhaustive()
    }
}
impl TeletextEffectivePublicationTarget {
    fn valid(&self) -> bool {
        self.channel_id > 0
            && self.design_id > 0
            && self.revision > 0
            && self.expected_design_head > 0
            && self.expected_request_id.is_none_or(|id| id > 0)
            && self.expected_digest.len() == 64
            && self
                .expected_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum NativeEffectiveTeletextPublicationError {
    #[error(transparent)]
    Design(#[from] NativeTeletextDesignError),
    #[error(transparent)]
    Storage(#[from] TeletextPublicationStoreError),
    #[error("teletext guide content requires current-role freshness before publication")]
    GuideFreshnessRequired,
}

/// Durable pending intent and exact privately recompiled effective original.
/// This carries no accepted generation or acknowledgement from a running output.
pub struct RequestedEffectiveTeletextPublication {
    request: TeletextPublicationRequest,
    design: RestoredEffectiveOriginalDesign,
}
impl RequestedEffectiveTeletextPublication {
    #[must_use]
    pub const fn request(&self) -> &TeletextPublicationRequest {
        &self.request
    }
    #[must_use]
    pub const fn design(&self) -> &RestoredEffectiveOriginalDesign {
        &self.design
    }
}
impl fmt::Debug for RequestedEffectiveTeletextPublication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestedEffectiveTeletextPublication")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

struct NativeEffectivePublicationConfirmation<'a> {
    design: &'a RestoredEffectiveOriginalDesign,
    deadline: Instant,
}
#[async_trait]
impl TeletextEffectivePublicationValidator for NativeEffectivePublicationConfirmation<'_> {
    async fn confirm_request(
        &self,
        request: &TeletextEffectivePublicationRequestSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        if Instant::now() >= self.deadline.min(deadline) {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        if !frozen_original(self.design)
            || !compiled_shape(self.design)
            || request.design_id != self.design.design_id()
            || request.revision != self.design.revision()
            || request.digest != self.design.digest()
            || request.selection != *self.design.selection()
        {
            return Err(TeletextPublicationValidationError::Association);
        }
        if Instant::now() >= self.deadline.min(deadline) {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        Ok(())
    }
}

// A distinct source role cannot confirm a static-only request. The existing
// frozen validator above remains unchanged for all legacy callers.
struct NativeSourceEffectivePublicationConfirmation<'a> {
    design: &'a RestoredEffectiveOriginalDesign,
    deadline: Instant,
}

#[async_trait]
impl TeletextEffectivePublicationValidator for NativeSourceEffectivePublicationConfirmation<'_> {
    async fn confirm_request(
        &self,
        _request: &TeletextEffectivePublicationRequestSave,
        _deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        Err(TeletextPublicationValidationError::Association)
    }

    async fn confirm_request_with_source(
        &self,
        request: &TeletextEffectivePublicationRequestSave,
        source: &TeletextGuideSourceFence,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        if Instant::now() >= self.deadline.min(deadline) {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        if !compiled_shape(self.design)
            || self.design.encoded().is_none()
            || self.design.guide_source() != Some(source)
            || !crate::admin::languages::LANGUAGES
                .iter()
                .any(|language| language.code == source.locale)
            || request.design_id != self.design.design_id()
            || request.revision != self.design.revision()
            || request.digest != self.design.digest()
            || request.selection != *self.design.selection()
        {
            return Err(TeletextPublicationValidationError::Association);
        }
        if Instant::now() >= self.deadline.min(deadline) {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        Ok(())
    }
}

/// Recompile and explicitly request a scoped pending intent under fresh admission,
/// capped by the original saved transition boundary. No producer or Lua runs.
/// # Errors
/// Rejects scope/guide/deadline/native/storage failures; CAS rejection writes nothing.
pub async fn request_effective_original_teletext_design_publication(
    application: &ApplicationServices,
    target: TeletextEffectivePublicationTarget,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<M<RequestedEffectiveTeletextPublication>, NativeEffectiveTeletextPublicationError> {
    if !target.valid() {
        return Ok(M::Rejected(R::InvalidValues));
    }
    let deadline = operation_deadline(deadline)?;
    let guard = super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    request_effective_original_publication_with_hold(
        application,
        target,
        budget,
        policy,
        deadline,
        guard.hold_work(),
    )
    .await
}

pub(super) async fn request_effective_original_publication_with_hold(
    application: &ApplicationServices,
    target: TeletextEffectivePublicationTarget,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<M<RequestedEffectiveTeletextPublication>, NativeEffectiveTeletextPublicationError> {
    if !target.valid() {
        return Ok(M::Rejected(R::InvalidValues));
    }
    let deadline = operation_deadline(deadline)?;
    let Some((design, deadline)) = reload_effective_original_for_publication_with_hold(
        application,
        target.design_id,
        target.revision,
        budget,
        policy,
        deadline,
        hold.clone(),
    )
    .await?
    else {
        return Ok(M::Rejected(R::MissingRevision));
    };
    if design.selection().channel_id != target.channel_id {
        return Ok(M::Rejected(R::InvalidValues));
    }
    if design.digest() != target.expected_digest {
        return Ok(M::Rejected(R::DigestConflict));
    }
    if !frozen_original(&design) {
        return Err(NativeEffectiveTeletextPublicationError::GuideFreshnessRequired);
    }
    if !compiled_shape(&design) {
        return Ok(M::Rejected(R::InvalidValues));
    }
    let request = TeletextEffectivePublicationRequestSave {
        design_id: target.design_id,
        revision: target.revision,
        digest: target.expected_digest,
        expected_design_head: target.expected_design_head,
        expected_request_id: target.expected_request_id,
        selection: design.selection().clone(),
    };
    let validator = NativeEffectivePublicationConfirmation {
        design: &design,
        deadline,
    };
    let mutation = application
        .request_effective_teletext_design_publication(
            request,
            &validator,
            budget.max_serialized_bytes,
            deadline,
        )
        .await?;
    drop(hold);
    Ok(match mutation {
        M::Requested(request) => {
            M::Requested(RequestedEffectiveTeletextPublication { request, design })
        }
        M::Rejected(reason) => M::Rejected(reason),
    })
}

/// Request an exact guide-bearing scoped original without relabelling static
/// validation. The repository rechecks its historical source and all CAS fences;
/// this pending intent grants no present-role or active-output authority.
pub(super) async fn request_source_bound_effective_original_publication_with_hold(
    application: &ApplicationServices,
    target: TeletextEffectivePublicationTarget,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    limits: TeletextGuideSourceCheckLimits,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<M<RequestedEffectiveTeletextPublication>, NativeEffectiveTeletextPublicationError> {
    if !target.valid() {
        return Ok(M::Rejected(R::InvalidValues));
    }
    let deadline = operation_deadline(deadline)?;
    let Some((design, deadline)) = reload_effective_original_for_publication_with_hold(
        application,
        target.design_id,
        target.revision,
        budget,
        policy,
        deadline,
        hold.clone(),
    )
    .await?
    else {
        return Ok(M::Rejected(R::MissingRevision));
    };
    if design.selection().channel_id != target.channel_id {
        return Ok(M::Rejected(R::InvalidValues));
    }
    if design.digest() != target.expected_digest {
        return Ok(M::Rejected(R::DigestConflict));
    }
    let Some(source) = design.guide_source() else {
        return Ok(M::Rejected(R::GuideFenceRequired));
    };
    if !compiled_shape(&design) || design.encoded().is_none() {
        return Ok(M::Rejected(R::InvalidValues));
    }
    let request = TeletextEffectivePublicationRequestSave {
        design_id: target.design_id,
        revision: target.revision,
        digest: target.expected_digest,
        expected_design_head: target.expected_design_head,
        expected_request_id: target.expected_request_id,
        selection: design.selection().clone(),
    };
    let validator = NativeSourceEffectivePublicationConfirmation {
        design: &design,
        deadline,
    };
    let mutation = application
        .request_source_bound_effective_teletext_design_publication_with_work(
            request,
            &validator,
            source,
            ersatzrs_core::repositories::TeletextGuideGenerationCheck {
                limits,
                deadline,
                hold: hold.clone(),
            },
            budget.max_serialized_bytes,
        )
        .await?;
    drop(hold);
    Ok(match mutation {
        M::Requested(request) => {
            M::Requested(RequestedEffectiveTeletextPublication { request, design })
        }
        M::Rejected(reason) => M::Rejected(reason),
    })
}
