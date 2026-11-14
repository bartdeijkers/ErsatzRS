//! Reconstruct a selected guide recipe without creating manual authoring state.

use std::time::Instant;

use ersatzrs_app::{
    teletext_design_documents::{
        decode_teletext_design_documents, decode_teletext_effective_design_documents,
        TeletextDesignDocumentBudget, TeletextSavedResourceTransmission, TeletextSavedV1Region,
    },
    ApplicationServices,
};
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{
        TeletextGuideGenerationCheck, TeletextGuideRefreshClaim, TeletextGuideRefreshRole,
        TeletextGuideRefreshSelection, TeletextGuideSourceCheckLimits,
        TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
        TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
    },
};
use ersatzrs_domain::{TeletextEffectiveBindingSnapshot, TeletextPageAddress};

use crate::host::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_designs::encode_original_candidate_document,
    teletext_effective_designs::encode_effective_candidate_document,
    teletext_guide_refresh_native::{FreshGuideBaseline, GuideRefreshNativeError as Error},
    teletext_runtime_policy::{
        parse_native_teletext_authoring_input, NativeTeletextAuthoringPolicy,
    },
    teletext_work,
};

/// A coordinator lends one existing admission from claim through actual cleanup.
/// No preparation allowance or capacity admission is renewed by this context.
pub(crate) struct GuideRefreshPreparationContext {
    policy: NativeTeletextAuthoringPolicy,
    deadline: Instant,
    hold: TeletextWorkHold,
}

impl GuideRefreshPreparationContext {
    /// The host may already have parsed its trusted policy under a separate
    /// settings-read bound. Count that read/admission time in the original
    /// producer operation; never reuse the short viewer-recovery timeout here.
    pub(crate) fn from_policy(
        policy: NativeTeletextAuthoringPolicy,
        started: Instant,
        hold: TeletextWorkHold,
    ) -> Result<Self, Error> {
        if policy.guide_limits.is_none() {
            return Err(Error::InvalidPolicy);
        }
        let deadline = started
            .checked_add(policy.operation_timeout)
            .ok_or(Error::InvalidPolicy)?;
        Error::check_deadline(deadline)?;
        Ok(Self {
            policy,
            deadline,
            hold,
        })
    }

    pub(crate) const fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(crate) const fn design_budget(&self) -> TeletextDesignDocumentBudget {
        self.policy.design
    }

    pub(crate) fn hold(&self) -> TeletextWorkHold {
        self.hold.clone()
    }

    pub(crate) fn guide_limits(&self) -> Result<TeletextGuideSourceCheckLimits, Error> {
        self.policy.guide_limits.ok_or(Error::InvalidPolicy)
    }

    pub(crate) fn source_check(&self) -> Result<TeletextGuideGenerationCheck, Error> {
        Ok(TeletextGuideGenerationCheck {
            limits: self.guide_limits()?,
            deadline: self.deadline,
            hold: self.hold.clone(),
        })
    }

    pub(crate) fn encoding_policy(&self) -> TeletextChannelDocumentEncodingPolicy {
        self.policy.reload_encoding_policy()
    }
}

struct Recipe {
    entry_addon_id: String,
    entry: TeletextPageAddress,
    region: Option<TeletextSavedV1Region>,
    transmissions: Vec<TeletextSavedResourceTransmission>,
    original_selection: TeletextGuideRefreshSelection,
    custom_pages: Option<ersatzrs_domain::TeletextCustomPagesWire>,
    document_bytes: usize,
}

async fn recipe(
    application: &ApplicationServices,
    seed: &ersatzrs_core::repositories::TeletextGuideRefreshSeed,
    budget: TeletextDesignDocumentBudget,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<Recipe, Error> {
    let request = &seed.generation.request;
    let stored = super::before(
        deadline,
        application.teletext_design_revision(
            request.design_id,
            request.revision,
            budget.max_serialized_bytes,
        ),
    )
    .await
    .map_err(|error| Error::from_authoring(&error))?
    .map_err(|_| Error::Unavailable)?
    .ok_or(Error::NeedsReview)?;
    if stored.design_id != request.design_id
        || stored.revision != request.revision
        || stored.digest != request.digest
    {
        return Err(Error::NeedsReview);
    }
    let seed = seed.clone();
    teletext_work::spawn_blocking(hold, move || {
        Error::check_deadline(deadline)?;
        if crate::host::teletext_guide_refresh_native::digest(&stored.document)
            != seed.generation.request.digest
        {
            return Err(Error::NeedsReview);
        }
        let document_bytes = stored.document.len();
        let result = match seed.role {
            TeletextGuideRefreshRole::Base => {
                if seed.generation.artifact_version != TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION {
                    return Err(Error::NeedsReview);
                }
                let parts = decode_teletext_design_documents(&stored.document, budget)
                    .map_err(|_| Error::NeedsReview)?
                    .into_parts();
                if parts.selection.channel_id != request_channel(&seed)
                    || parts.guide_source.as_ref() != Some(&seed.qualification.source)
                {
                    return Err(Error::NeedsReview);
                }
                Ok(Recipe {
                    custom_pages: parts
                        .selection
                        .bindings
                        .iter()
                        .position(|binding| {
                            ersatzrs_domain::is_teletext_custom_pages_producer(&binding.addon_id)
                        })
                        .map(|index| {
                            serde_json::from_slice(parts.sources[index].bytes())
                                .map_err(|_| Error::NeedsReview)
                        })
                        .transpose()?,
                    entry_addon_id: parts.entry_addon_id.ok_or(Error::NeedsReview)?,
                    entry: parts.entry.ok_or(Error::NeedsReview)?,
                    region: parts.v1_region,
                    transmissions: parts.resource_transmissions,
                    original_selection: TeletextGuideRefreshSelection::Base(parts.selection),
                    document_bytes,
                })
            }
            TeletextGuideRefreshRole::Effective => {
                if seed.generation.artifact_version
                    != TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION
                {
                    return Err(Error::NeedsReview);
                }
                let parts = decode_teletext_effective_design_documents(&stored.document, budget)
                    .map_err(|_| Error::NeedsReview)?
                    .into_parts();
                if parts.selection.channel_id != request_channel(&seed)
                    || parts.guide_source.as_ref() != Some(&seed.qualification.source)
                {
                    return Err(Error::NeedsReview);
                }
                Ok(Recipe {
                    entry_addon_id: parts.entry_addon_id.ok_or(Error::NeedsReview)?,
                    entry: parts.entry.ok_or(Error::NeedsReview)?,
                    region: parts.v1_region,
                    transmissions: parts.resource_transmissions,
                    original_selection: TeletextGuideRefreshSelection::Effective(parts.selection),
                    custom_pages: None,
                    document_bytes,
                })
            }
        };
        Error::check_deadline(deadline)?;
        result
    })
    .await
    .map_err(|_| Error::WorkerFailed)?
}

/// Read only an immutable seed's bounded configuration; never invoke producers.
/// Charge the exact source document when retaining its decoded selection.
pub(crate) async fn observe_guide_refresh_seed_configuration_with_charge(
    application: &ApplicationServices,
    seed: &ersatzrs_core::repositories::TeletextGuideRefreshSeed,
    budget: TeletextDesignDocumentBudget,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<(TeletextGuideRefreshSelection, usize), Error> {
    let recipe = recipe(application, seed, budget, deadline, hold).await?;
    Ok((recipe.original_selection, recipe.document_bytes))
}

fn request_channel(seed: &ersatzrs_core::repositories::TeletextGuideRefreshSeed) -> i64 {
    seed.generation.request.channel_id
}

pub(in crate::host) async fn prepare_baseline(
    application: &ApplicationServices,
    pool: &ersatzrs_infra::db::DbPool,
    claim: &TeletextGuideRefreshClaim,
    context: &GuideRefreshPreparationContext,
) -> Result<FreshGuideBaseline, Error> {
    let recipe = recipe(
        application,
        &claim.seed,
        context.policy.design,
        context.deadline,
        context.hold.clone(),
    )
    .await?;
    let input = serde_json::json!({
        "input_version": 2,
        "channel_id": request_channel(&claim.seed),
        "entry_addon_id": recipe.entry_addon_id,
        "entry": recipe.entry,
        // The mandatory factory field is unused for native-only V2/V3 input.
        // A newly required V1 lift below must match an actual saved region.
        "v1_region": recipe.region.unwrap_or(TeletextSavedV1Region::WesternLatin),
        "resource_transmissions": recipe.transmissions,
        "guide_channel_ids": claim.seed.qualification.source.channel_ids,
    });
    let input = parse_native_teletext_authoring_input(&input.to_string(), &context.policy)
        .map_err(|_| Error::NeedsReview)?;
    let preparation = super::guide::GuidePreparationContext {
        policy: &context.policy,
        deadline: context.deadline,
        hold: context.hold.clone(),
        published_custom_pages: recipe.custom_pages.as_ref(),
    };
    match claim.seed.role {
        TeletextGuideRefreshRole::Base => {
            let selection = super::observed_selection(
                application,
                request_channel(&claim.seed),
                context.deadline,
            )
            .await
            .map_err(|error| Error::from_authoring(&error))?;
            let TeletextGuideRefreshSelection::Base(original) = &recipe.original_selection else {
                return Err(Error::NeedsReview);
            };
            if !original
                .bindings
                .iter()
                .map(|binding| &binding.addon_id)
                .eq(selection.bindings.iter().map(|binding| &binding.addon_id))
            {
                return Err(Error::NeedsReview);
            }
            let (candidate, qualification) = super::guide::prepare_base_current(
                application,
                pool,
                &input,
                &selection,
                &preparation,
            )
            .await
            .map_err(|error| Error::from_authoring(&error))?;
            if candidate.v1_region().is_some() && recipe.region.is_none() {
                return Err(Error::NeedsReview);
            }
            let budget = context.policy.design;
            let deadline = context.deadline;
            let (document, _) = teletext_work::spawn_blocking(context.hold.clone(), move || {
                encode_original_candidate_document(&candidate, budget, deadline)
            })
            .await
            .map_err(|_| Error::WorkerFailed)?
            .map_err(|error| Error::from_design(&error))?;
            Ok(FreshGuideBaseline {
                document_version: document.version(),
                document: document.into_bytes(),
                qualification,
                selection: TeletextGuideRefreshSelection::Base(selection),
            })
        }
        TeletextGuideRefreshRole::Effective => {
            prepare_effective_baseline(
                application,
                pool,
                claim,
                &input,
                &recipe,
                context,
                &preparation,
            )
            .await
        }
    }
}

async fn prepare_effective_baseline(
    application: &ApplicationServices,
    pool: &ersatzrs_infra::db::DbPool,
    claim: &TeletextGuideRefreshClaim,
    input: &crate::host::teletext_runtime_policy::NativeTeletextAuthoringInput,
    recipe: &Recipe,
    context: &GuideRefreshPreparationContext,
    preparation: &super::guide::GuidePreparationContext<'_>,
) -> Result<FreshGuideBaseline, Error> {
    let selection =
        super::scope::observed(application, request_channel(&claim.seed), context.deadline)
            .await
            .map_err(|error| Error::from_authoring(&error))?;
    let TeletextGuideRefreshSelection::Effective(original) = &recipe.original_selection else {
        return Err(Error::NeedsReview);
    };
    if !original
        .bindings
        .iter()
        .map(TeletextEffectiveBindingSnapshot::addon_id)
        .eq(selection
            .bindings
            .iter()
            .map(TeletextEffectiveBindingSnapshot::addon_id))
        || !same_original_scope(original, &selection)
    {
        return Err(Error::NeedsReview);
    }
    let (candidate, qualification) =
        super::guide::prepare_effective_current(application, pool, input, &selection, preparation)
            .await
            .map_err(|error| Error::from_authoring(&error))?;
    if candidate.v1_region().is_some() && recipe.region.is_none() {
        return Err(Error::NeedsReview);
    }
    let budget = context.policy.design;
    let deadline = context.deadline;
    let (document, _) = teletext_work::spawn_blocking(context.hold.clone(), move || {
        encode_effective_candidate_document(&candidate, budget, deadline)
    })
    .await
    .map_err(|_| Error::WorkerFailed)?
    .map_err(|error| Error::from_design(&error))?;
    Ok(FreshGuideBaseline {
        document_version: document.version(),
        document: document.into_bytes(),
        qualification,
        selection: TeletextGuideRefreshSelection::Effective(selection),
    })
}

fn same_original_scope(
    original: &ersatzrs_domain::TeletextEffectiveChannelSelectionSnapshot,
    current: &ersatzrs_domain::TeletextEffectiveChannelSelectionSnapshot,
) -> bool {
    original.channel_id == current.channel_id
        && original.valid_until == current.valid_until
        && original
            .valid_until
            .is_some_and(|until| chrono::Utc::now() < until)
        && original
            .bindings
            .iter()
            .filter_map(|binding| match binding {
                TeletextEffectiveBindingSnapshot::Scripted(value) => Some(value),
                TeletextEffectiveBindingSnapshot::Base(_) => None,
            })
            .eq(current.bindings.iter().filter_map(|binding| match binding {
                TeletextEffectiveBindingSnapshot::Scripted(value) => Some(value),
                TeletextEffectiveBindingSnapshot::Base(_) => None,
            }))
}
