//! Source-aware scoped activation; Base output remains an independent role.

use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_generations::TeletextEffectiveGuideGenerationValidator,
    teletext_publications::TeletextPublicationValidationError, ApplicationServices,
};
use ersatzrs_core::repositories::{
    TeletextEffectiveGenerationSave, TeletextEffectiveGuideGenerationSave,
    TeletextGenerationMutation as M, TeletextGenerationRejection as R,
    TeletextGuideGenerationCheck, TeletextGuideSourceCheckLimits,
    TeletextPublicationRejection as P, TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::EntityId;
use ersatzrs_infra::db::DbPool;

use super::{capture, fingerprint, qualify, GuideGenerationContext, GuideOutputQualification};
use crate::host::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_designs::NativeTeletextDesignError,
    teletext_effective_designs::{
        reload_effective_original_for_publication_with_hold, RestoredEffectiveOriginalDesign,
    },
    teletext_effective_generations::{
        compiled_shape, operation_deadline, AcceptedEffectiveTeletextOutputCandidate,
    },
    teletext_generations::{NativeTeletextGenerationError, TeletextGenerationTarget},
};

struct Confirmation<'a> {
    design: &'a RestoredEffectiveOriginalDesign,
    proof: &'a GuideOutputQualification,
    digest: &'a str,
    deadline: Instant,
}

#[async_trait]
impl TeletextEffectiveGuideGenerationValidator for Confirmation<'_> {
    async fn confirm_guide_generation(
        &self,
        save: &TeletextEffectiveGuideGenerationSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        if Instant::now() >= self.deadline.min(deadline).min(self.proof.deadline) {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        let original = &save.original;
        if !compiled_shape(self.design)
            || self.design.encoded().is_none()
            || self.design.guide_source() != Some(&save.qualification.source)
            || save.qualification != self.proof.qualification
            || original.request.design_id != self.design.design_id()
            || original.request.revision != self.design.revision()
            || original.request.digest != self.design.digest()
            || original.request.channel_id != self.design.selection().channel_id
            || original.selection != *self.design.selection()
            || original.artifact_version != TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION
            || original.artifact_digest != self.digest
            || save
                .qualification
                .validate_current(chrono::Utc::now())
                .is_err()
        {
            return Err(TeletextPublicationValidationError::Association);
        }
        Ok(())
    }
}

async fn compile(
    operation: &GuideGenerationContext<'_>,
    design: RestoredEffectiveOriginalDesign,
) -> Result<
    (
        RestoredEffectiveOriginalDesign,
        GuideOutputQualification,
        String,
    ),
    NativeTeletextGenerationError,
> {
    let source = design
        .guide_source()
        .cloned()
        .ok_or(NativeTeletextGenerationError::GuideFreshnessRequired)?;
    let captured = capture(operation, &source).await?;
    let deadline = operation.deadline;
    let limits = operation.limits;
    super::super::teletext_work::spawn_blocking(operation.hold.clone(), move || {
        if !compiled_shape(&design) || design.encoded().is_none() {
            return Err(NativeTeletextGenerationError::ArtifactMismatch);
        }
        let qualification = qualify(
            &captured,
            &source,
            design.guide_inputs(),
            design.selection().valid_until,
            deadline,
        )?;
        let design = design.with_resource_lifecycle_variants(deadline)?;
        let digest = fingerprint(
            design
                .encoded()
                .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
            &qualification,
            TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
            deadline,
        )?;
        let proof = GuideOutputQualification::new(qualification, limits)?;
        Ok((design, proof, digest))
    })
    .await
    .map_err(|_| NativeTeletextGenerationError::WorkerFailed)?
}

/// Activate a reviewed source-bearing effective original without changing Base.
pub(crate) async fn accept_effective_original_guide_teletext_generation_with_hold(
    mut operation: GuideGenerationContext<'_>,
    target: TeletextGenerationTarget,
) -> Result<M<AcceptedEffectiveTeletextOutputCandidate>, NativeTeletextGenerationError> {
    operation.deadline = operation_deadline(operation.deadline)?;
    let Some((design, deadline)) = reload_effective_original_for_publication_with_hold(
        operation.application,
        target.request.design_id,
        target.request.revision,
        operation.budget,
        operation.encoding.clone(),
        operation.deadline,
        operation.hold.clone(),
    )
    .await?
    else {
        return Ok(M::Rejected(R::Publication(P::MissingRevision)));
    };
    operation.deadline = deadline;
    if design.selection().channel_id != target.request.channel_id {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    if design.digest() != target.request.digest {
        return Ok(M::Rejected(R::Publication(P::DigestConflict)));
    }
    let (design, proof, digest) = compile(&operation, design).await?;
    let save = TeletextEffectiveGuideGenerationSave {
        original: TeletextEffectiveGenerationSave {
            request: target.request,
            expected_design_head: target.expected_design_head,
            expected_generation_id: target.expected_generation_id,
            selection: design.selection().clone(),
            artifact_version: TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
            artifact_digest: digest,
        },
        qualification: proof.qualification.clone(),
    };
    let selection = Arc::new(save.original.selection.clone());
    let digest = save.original.artifact_digest.clone();
    let validator = Confirmation {
        design: &design,
        proof: &proof,
        digest: &digest,
        deadline: operation.deadline,
    };
    let mutation = operation
        .application
        .accept_effective_guide_teletext_generation(
            save,
            &validator,
            TeletextGuideGenerationCheck {
                limits: operation.limits,
                deadline: operation.deadline.min(proof.deadline),
                hold: operation.hold.clone(),
            },
            operation.budget.max_serialized_bytes,
        )
        .await?;
    Ok(match mutation {
        M::Accepted(generation) => {
            M::Accepted(AcceptedEffectiveTeletextOutputCandidate::from_guide(
                generation,
                selection,
                Arc::new(
                    design
                        .into_encoded_service()
                        .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
                ),
                proof,
            ))
        }
        M::Rejected(reason) => M::Rejected(reason),
    })
}

/// Recover and confirm the exact recorded Effective6 original, preserving expiry.
pub(crate) async fn reload_accepted_effective_guide_teletext_generation(
    application: &ApplicationServices,
    pool: &DbPool,
    channel_id: EntityId,
    budget: TeletextDesignDocumentBudget,
    encoding: TeletextChannelDocumentEncodingPolicy,
    limits: TeletextGuideSourceCheckLimits,
    deadline: Instant,
) -> Result<Option<AcceptedEffectiveTeletextOutputCandidate>, NativeTeletextGenerationError> {
    let deadline = operation_deadline(deadline)?;
    let guard = super::super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    let mut operation = GuideGenerationContext {
        application,
        pool,
        budget,
        encoding: encoding.clone(),
        limits,
        deadline,
        hold: guard.hold_work(),
    };
    let Some(generation) = application
        .effective_teletext_accepted_generation(channel_id)
        .await?
    else {
        return Ok(None);
    };
    if generation.artifact_version != TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let Some((design, deadline)) = reload_effective_original_for_publication_with_hold(
        application,
        generation.request.design_id,
        generation.request.revision,
        budget,
        encoding,
        deadline,
        operation.hold.clone(),
    )
    .await?
    else {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    };
    operation.deadline = deadline;
    if design.digest() != generation.request.digest || design.selection().channel_id != channel_id {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let (design, proof, actual) = compile(&operation, design).await?;
    if actual != generation.artifact_digest {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let confirmation = application
        .confirm_current_effective_guide_teletext_generation(
            &generation,
            design.selection(),
            &proof.qualification,
            TeletextGuideGenerationCheck {
                limits,
                deadline: deadline.min(proof.deadline),
                hold: operation.hold.clone(),
            },
            budget.max_serialized_bytes,
        )
        .await?;
    operation_deadline(deadline)?;
    if !matches!(confirmation, M::Accepted(())) {
        return Ok(None);
    }
    Ok(Some(AcceptedEffectiveTeletextOutputCandidate::from_guide(
        generation,
        Arc::new(design.selection().clone()),
        Arc::new(
            design
                .into_encoded_service()
                .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
        ),
        proof,
    )))
}
