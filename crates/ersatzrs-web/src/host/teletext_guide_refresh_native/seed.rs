//! First last-good output has a real manual seed, not a fabricated runtime ID.

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{
    TeletextGuideRefreshRole, TeletextGuideRefreshSeed, TeletextGuideRefreshSelection,
    TeletextGuideRefreshStatus, TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
};

use super::{
    output_deadline, GuideRefreshNativeError as Error, GuideRefreshReloadContext,
    NativeGuideOutputOrigin, NativeGuideRuntimeOutput,
};
use crate::host::{
    teletext_guide_generations::{capture, qualify, GuideGenerationContext},
    teletext_stored_designs::{reload_stored_with_hold, RestoredStoredTeletextDesign},
    teletext_work,
};

pub(crate) async fn reload_guide_refresh_seed(
    seed: &TeletextGuideRefreshSeed,
    status: TeletextGuideRefreshStatus,
    available_until: DateTime<Utc>,
    context: GuideRefreshReloadContext<'_>,
) -> Result<NativeGuideRuntimeOutput, Error> {
    Error::check_deadline(context.deadline)?;
    if !matches!(
        status,
        TeletextGuideRefreshStatus::Fresh | TeletextGuideRefreshStatus::Stale
    ) || seed.id <= 0
        || seed.generation.id <= 0
    {
        return Err(Error::NeedsReview);
    }
    let deadline = output_deadline(&seed.qualification, available_until)?;
    let restored = restore_seed(seed, &context, deadline).await?;
    let captured = if status == TeletextGuideRefreshStatus::Fresh {
        Some(capture_seed_source(seed, &context, deadline).await?)
    } else {
        None
    };
    let owned_seed = seed.clone();
    let work_deadline = context.deadline.min(deadline);
    let (selection, service) = teletext_work::spawn_blocking(context.hold, move || {
        Error::check_deadline(work_deadline)?;
        if let Some(captured) = captured {
            let (inputs, scope) = match &restored {
                RestoredStoredTeletextDesign::Base(design) => (design.guide_inputs(), None),
                RestoredStoredTeletextDesign::Effective(design) => {
                    (design.guide_inputs(), design.selection().valid_until)
                }
            };
            let actual = qualify(
                &captured,
                &owned_seed.qualification.source,
                inputs,
                scope,
                work_deadline,
            )
            .map_err(|_| Error::Unavailable)?;
            if actual != owned_seed.qualification {
                return Err(Error::NeedsReview);
            }
        }
        let (selection, service) = match restored {
            RestoredStoredTeletextDesign::Base(design) => {
                let design = design
                    .with_resource_lifecycle_variants(work_deadline)
                    .map_err(|_| Error::NeedsReview)?;
                let selection = TeletextGuideRefreshSelection::Base(design.selection().clone());
                (
                    selection,
                    design.into_encoded_service().ok_or(Error::NeedsReview)?,
                )
            }
            RestoredStoredTeletextDesign::Effective(design) => {
                let design = design
                    .with_resource_lifecycle_variants(work_deadline)
                    .map_err(|_| Error::NeedsReview)?;
                let selection =
                    TeletextGuideRefreshSelection::Effective(design.selection().clone());
                (
                    selection,
                    design.into_encoded_service().ok_or(Error::NeedsReview)?,
                )
            }
        };
        let digest = crate::host::teletext_guide_generations::fingerprint(
            &service,
            &owned_seed.qualification,
            owned_seed.generation.artifact_version,
            work_deadline,
        )
        .map_err(|_| Error::NeedsReview)?;
        if digest != owned_seed.generation.artifact_digest {
            return Err(Error::NeedsReview);
        }
        Error::check_deadline(work_deadline)?;
        Ok::<_, Error>((selection, std::sync::Arc::new(service)))
    })
    .await
    .map_err(|_| Error::WorkerFailed)??;
    Ok(NativeGuideRuntimeOutput {
        seed: seed.clone(),
        origin: NativeGuideOutputOrigin::Seed,
        selection,
        service,
        deadline,
        status,
        available_until,
    })
}

async fn capture_seed_source(
    seed: &TeletextGuideRefreshSeed,
    context: &GuideRefreshReloadContext<'_>,
    deadline: std::time::Instant,
) -> Result<crate::host::teletext_guide::CapturedTeletextGuide, Error> {
    let operation = GuideGenerationContext {
        application: context.application,
        pool: context.pool,
        budget: context.budget,
        encoding: context.encoding.clone(),
        limits: context.limits,
        deadline: context.deadline.min(deadline),
        hold: context.hold.clone(),
    };
    capture(&operation, &seed.qualification.source)
        .await
        .map_err(|_| Error::Unavailable)
}

async fn restore_seed(
    seed: &TeletextGuideRefreshSeed,
    context: &GuideRefreshReloadContext<'_>,
    deadline: std::time::Instant,
) -> Result<RestoredStoredTeletextDesign, Error> {
    let request = &seed.generation.request;
    let restored = reload_stored_with_hold(
        context.application,
        request.design_id,
        request.revision,
        context.budget,
        context.encoding.clone(),
        context.deadline.min(deadline),
        context.hold.clone(),
    )
    .await
    .map_err(|error| Error::from_design(&error))?
    .ok_or(Error::NeedsReview)?;
    let view = restored.view();
    if view.design_id() != request.design_id
        || view.revision() != request.revision
        || view.channel_id() != request.channel_id
        || view.digest() != request.digest
    {
        return Err(Error::NeedsReview);
    }
    let source = match (&restored, seed.role) {
        (RestoredStoredTeletextDesign::Base(design), TeletextGuideRefreshRole::Base)
            if seed.generation.artifact_version == TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION =>
        {
            design.guide_source()
        }
        (RestoredStoredTeletextDesign::Effective(design), TeletextGuideRefreshRole::Effective)
            if seed.generation.artifact_version
                == TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION =>
        {
            design.guide_source()
        }
        _ => return Err(Error::NeedsReview),
    };
    if source != Some(&seed.qualification.source) {
        return Err(Error::NeedsReview);
    }
    Ok(restored)
}
