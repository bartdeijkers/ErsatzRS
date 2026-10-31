//! Base guide-role acceptance and exact restart reconstruction.

use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use ersatzrs_app::{
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_generations::TeletextGuideGenerationValidator,
    teletext_publications::TeletextPublicationValidationError, ApplicationServices,
};
use ersatzrs_core::repositories::{
    TeletextGenerationMutation as M, TeletextGenerationRejection as R, TeletextGenerationSave,
    TeletextGuideGenerationCheck, TeletextGuideGenerationSave, TeletextGuideSourceCheckLimits,
    TeletextPublicationRejection as P, TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::EntityId;
use ersatzrs_infra::db::DbPool;

use super::{
    capture, check_deadline, fingerprint, qualify, GuideGenerationContext, GuideOutputQualification,
};
use crate::host::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_designs::{
        reload_original_teletext_design_revision_with_hold, NativeTeletextDesignError,
        RestoredOriginalTeletextDesign,
    },
    teletext_generations::{
        AcceptedTeletextOutputCandidate, NativeTeletextGenerationError,
        PreparedTeletextOutputCandidate, TeletextGenerationAcceptanceTarget,
    },
};

struct Confirmation<'a> {
    design: &'a RestoredOriginalTeletextDesign,
    proof: &'a GuideOutputQualification,
    digest: &'a str,
    deadline: Instant,
}

#[async_trait]
impl TeletextGuideGenerationValidator for Confirmation<'_> {
    async fn confirm_guide_generation(
        &self,
        save: &TeletextGuideGenerationSave,
        deadline: Instant,
    ) -> Result<(), TeletextPublicationValidationError> {
        if Instant::now() >= self.deadline.min(deadline).min(self.proof.deadline) {
            return Err(TeletextPublicationValidationError::TimedOut);
        }
        let original = &save.original;
        if self.design.encoded().is_none()
            || self.design.guide_source() != Some(&save.qualification.source)
            || save.qualification != self.proof.qualification
            || original.request.design_id != self.design.design_id()
            || original.request.revision != self.design.revision()
            || original.request.digest != self.design.digest()
            || original.request.channel_id != self.design.selection().channel_id
            || original.selection != *self.design.selection()
            || original.artifact_version != TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION
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
    design: RestoredOriginalTeletextDesign,
) -> Result<
    (
        RestoredOriginalTeletextDesign,
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
        let qualification = qualify(&captured, &source, design.guide_inputs(), None, deadline)?;
        let design = design.with_resource_lifecycle_variants(deadline)?;
        let digest = fingerprint(
            design
                .encoded()
                .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
            &qualification,
            TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
            deadline,
        )?;
        let proof = GuideOutputQualification::new(qualification, limits)?;
        Ok((design, proof, digest))
    })
    .await
    .map_err(|_| NativeTeletextGenerationError::WorkerFailed)?
}

/// Accept one exact reviewed source-bearing Base original using its existing hold.
/// Source, clock, native artifact and transactional CAS must all still agree.
pub(crate) async fn accept_original_guide_teletext_generation_with_hold(
    operation: GuideGenerationContext<'_>,
    target: TeletextGenerationAcceptanceTarget,
) -> Result<M<AcceptedTeletextOutputCandidate>, NativeTeletextGenerationError> {
    check_deadline(operation.deadline)?;
    let TeletextGenerationAcceptanceTarget {
        target,
        expected_channel_id,
    } = target;
    if expected_channel_id.is_some_and(|id| id <= 0 || id != target.request.channel_id) {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    let Some(design) = reload_original_teletext_design_revision_with_hold(
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
    if design.selection().channel_id != target.request.channel_id
        || expected_channel_id.is_some_and(|id| id != design.selection().channel_id)
    {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    if design.digest() != target.request.digest {
        return Ok(M::Rejected(R::Publication(P::DigestConflict)));
    }
    let (design, proof, digest) = compile(&operation, design).await?;
    let save = TeletextGuideGenerationSave {
        original: TeletextGenerationSave {
            request: target.request,
            expected_design_head: target.expected_design_head,
            expected_generation_id: target.expected_generation_id,
            selection: design.selection().clone(),
            artifact_version: TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
            artifact_digest: digest,
        },
        qualification: proof.qualification.clone(),
    };
    let selection = save.original.selection.clone();
    let digest = save.original.artifact_digest.clone();
    let validator = Confirmation {
        design: &design,
        proof: &proof,
        digest: &digest,
        deadline: operation.deadline,
    };
    let mutation = operation
        .application
        .accept_guide_teletext_generation(
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
        M::Accepted(generation) => M::Accepted(AcceptedTeletextOutputCandidate::from_guide(
            generation,
            selection,
            Arc::new(
                design
                    .into_encoded_service()
                    .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
            ),
            proof,
        )),
        M::Rejected(reason) => M::Rejected(reason),
    })
}

/// Recover exact Base5 metadata/artifact without granting stream authority yet.
/// No producer runs and neither capture time nor role expiry is renewed.
pub(crate) async fn prepare_accepted_guide_teletext_generation(
    application: &ApplicationServices,
    pool: &DbPool,
    channel_id: EntityId,
    budget: TeletextDesignDocumentBudget,
    encoding: TeletextChannelDocumentEncodingPolicy,
    limits: TeletextGuideSourceCheckLimits,
    deadline: Instant,
) -> Result<Option<PreparedTeletextOutputCandidate>, NativeTeletextGenerationError> {
    check_deadline(deadline)?;
    let guard = super::super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(NativeTeletextDesignError::from)?;
    let operation = GuideGenerationContext {
        application,
        pool,
        budget,
        encoding: encoding.clone(),
        limits,
        deadline,
        hold: guard.hold_work(),
    };
    let Some(generation) = application.accepted_teletext_generation(channel_id).await? else {
        return Ok(None);
    };
    if generation.artifact_version != TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let Some(design) = reload_original_teletext_design_revision_with_hold(
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
    if design.digest() != generation.request.digest || design.selection().channel_id != channel_id {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    let (design, proof, actual) = compile(&operation, design).await?;
    if actual != generation.artifact_digest {
        return Err(NativeTeletextGenerationError::ArtifactMismatch);
    }
    Ok(Some(PreparedTeletextOutputCandidate::from_guide(
        generation,
        design.selection().clone(),
        Arc::new(
            design
                .into_encoded_service()
                .ok_or(NativeTeletextGenerationError::ArtifactMismatch)?,
        ),
        proof,
    )))
}

/// Reconstruct and freshly confirm a recorded guide Base generation.
pub(crate) async fn reload_accepted_guide_teletext_generation(
    application: &ApplicationServices,
    pool: &DbPool,
    channel_id: EntityId,
    budget: TeletextDesignDocumentBudget,
    encoding: TeletextChannelDocumentEncodingPolicy,
    limits: TeletextGuideSourceCheckLimits,
    deadline: Instant,
) -> Result<Option<AcceptedTeletextOutputCandidate>, NativeTeletextGenerationError> {
    let Some(prepared) = prepare_accepted_guide_teletext_generation(
        application,
        pool,
        channel_id,
        budget,
        encoding,
        limits,
        deadline,
    )
    .await?
    else {
        return Ok(None);
    };
    prepared
        .confirm(application, budget.max_serialized_bytes, deadline)
        .await
}
