//! Explicit selected SQL-guide inputs under the author's existing work lease.
//! A source-bound concept remains historical data, never live guide authority.

use super::{
    channel_error, failure, prepare_channel_teletext_document_candidate_with_hold, work_error,
    ApplicationServices, Operation, TeletextAuthoringError, TeletextAuthoringStatus,
    TeletextChannelDocumentPreparation, TeletextChannelSelectionSnapshot,
    TeletextEffectiveChannelSelectionSnapshot, Utc,
};
use crate::host::{
    teletext_channel_documents::EncodedChannelTeletextDocumentCandidate,
    teletext_effective_channel_documents::{
        prepare_effective_channel_teletext_document_candidate_checked_with_hold,
        prepare_effective_channel_teletext_document_candidate_for_refresh,
        EncodedEffectiveChannelTeletextDocumentCandidate,
    },
    teletext_guide::{
        capture_source_bound_teletext_guide_with_hold, CapturedTeletextGuide,
        TeletextGuideCaptureError,
    },
    teletext_runtime_policy::{NativeTeletextAuthoringInput, NativeTeletextAuthoringPolicy},
};
use ersatzrs_app::{
    teletext_channel_documents::TeletextCustomPageSource,
    teletext_effective_channel_documents::TeletextEffectiveChannelDocumentRefresh,
};
use ersatzrs_core::{
    locking::TeletextWorkHold, repositories::TeletextGuideGenerationQualification,
};
use ersatzrs_infra::{
    db::DbPool,
    xmltv::{
        GuideExpansionLimits, GuideReadBudget, GuideReadLimits, GuideSnapshotDigestLimits,
        GuideSnapshotFailure,
    },
};
use std::time::Instant;

/// Borrow trusted bounds and an already admitted operation; never reacquire work.
pub(super) struct GuidePreparationContext<'a> {
    pub(super) policy: &'a NativeTeletextAuthoringPolicy,
    pub(super) deadline: Instant,
    pub(super) hold: TeletextWorkHold,
    pub(super) custom_page_source: TeletextCustomPageSource<'a>,
}

impl<'a> GuidePreparationContext<'a> {
    fn from_operation(operation: &'a Operation) -> Self {
        Self {
            policy: &operation.policy,
            deadline: operation.deadline,
            hold: operation.guard.hold_work(),
            custom_page_source: TeletextCustomPageSource::Current,
        }
    }
}

fn capture_error(error: TeletextGuideCaptureError) -> TeletextAuthoringError {
    use TeletextGuideCaptureError as E;
    let status = match error {
        E::Work(error) => return work_error(&error),
        E::Candidate(error) => return channel_error(error),
        E::TimedOut => TeletextAuthoringStatus::TimedOut,
        E::SourceChanged { .. } => TeletextAuthoringStatus::SelectionChanged,
        E::ChannelLimit | E::Digest(_) => TeletextAuthoringStatus::ResourceLimit,
        E::Snapshot(error) if matches!(error.reason, GuideSnapshotFailure::Admission) => {
            TeletextAuthoringStatus::ResourceLimit
        }
        E::Worker => TeletextAuthoringStatus::WorkerFailed,
        E::Clock | E::CalendarWindow | E::Locale | E::Database | E::Snapshot(_) => {
            TeletextAuthoringStatus::Unavailable
        }
        E::Association | E::Request(_) => TeletextAuthoringStatus::InvalidInput,
    };
    failure(status)
}

async fn capture(
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    operation: &GuidePreparationContext<'_>,
) -> Result<(CapturedTeletextGuide, GuideReadBudget), TeletextAuthoringError> {
    let limits = operation
        .policy
        .guide_limits
        .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidPolicy))?;
    let ids = input
        .guide_channel_ids()
        .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidInput))?;
    let mut read = GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: limits.max_records,
            max_source_bytes: limits.max_source_bytes,
        },
        GuideExpansionLimits {
            max_nodes: limits.max_model_nodes,
            max_bytes: limits.max_model_bytes,
        },
    );
    let captured = capture_source_bound_teletext_guide_with_hold(
        pool,
        ids,
        Utc::now(),
        &mut read,
        operation.deadline,
        GuideSnapshotDigestLimits {
            max_fields: limits.max_digest_fields,
            max_bytes: limits.max_digest_bytes,
        },
        operation.hold.clone(),
    )
    .await
    .map_err(capture_error)?;
    Ok((captured, read))
}

pub(super) async fn prepare_base(
    application: &ApplicationServices,
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    selection: &TeletextChannelSelectionSnapshot,
    operation: &Operation,
) -> Result<EncodedChannelTeletextDocumentCandidate, TeletextAuthoringError> {
    let context = GuidePreparationContext::from_operation(operation);
    let (candidate, _) = Box::pin(prepare_base_inner(
        application,
        pool,
        input,
        selection,
        &context,
    ))
    .await?;
    Ok(candidate)
}

pub(super) async fn preview_base(
    application: &ApplicationServices,
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    selected: &TeletextChannelSelectionSnapshot,
    operation: &Operation,
    elements: &[ersatzrs_domain::TeletextAtlasElement],
) -> Result<EncodedChannelTeletextDocumentCandidate, TeletextAuthoringError> {
    let plan = super::before(
        operation.deadline,
        application.preview_teletext_instances(
            input.channel_id(),
            elements,
            input.page_layout.as_deref(),
            Utc::now(),
        ),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::CustomPagesBlocked))?;
    if !plan.is_ready() {
        return Err(failure(TeletextAuthoringStatus::CustomPagesBlocked));
    }
    let wire = (!plan.pages.is_empty()).then(|| ersatzrs_domain::TeletextCustomPagesWire {
        schema: ersatzrs_domain::TELETEXT_CUSTOM_PAGES_SCHEMA.into(),
        generation: "preview".into(),
        pages: plan.pages,
    });
    let mut selected = selected.clone();
    selected
        .bindings
        .retain(|b| !ersatzrs_domain::is_teletext_custom_pages_producer(&b.addon_id));
    if wire.is_some() {
        selected
            .bindings
            .push(ersatzrs_domain::teletext_custom_pages_selection(
                input.channel_id(),
            ));
        selected
            .bindings
            .sort_by(|a, b| a.addon_id.cmp(&b.addon_id));
    }
    let mut context = GuidePreparationContext::from_operation(operation);
    context.custom_page_source = TeletextCustomPageSource::Published(wire.as_ref());
    let (candidate, _) = Box::pin(prepare_base_inner(
        application,
        pool,
        input,
        &selected,
        &context,
    ))
    .await?;
    Ok(candidate)
}

/// Produce the same V3 document before the channel has an enabled binding.
/// The bounded invocation returns only a draft. Its configuration cannot be published.
pub(super) async fn preview_unbound_guide(
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    operation: &Operation,
) -> Result<(Vec<u8>, String), TeletextAuthoringError> {
    let context = GuidePreparationContext::from_operation(operation);
    let (captured, mut read) = capture(pool, input, &context).await?;
    let channels = captured.borrow_channels();
    let locales = super::super::teletext_guide::supported_locales();
    let generation = uuid::Uuid::new_v4().to_string();
    let mut source = captured.context(&generation, &channels, &locales);
    source.page_layout = input.page_layout.as_deref();
    let request = ersatzrs_app::teletext_guide::build_request_for_schema(&source, 3)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let bytes = ersatzrs_infra::addons::teletext_invocation::preview_atlas_guide(
        pool.clone(),
        input.channel_id(),
        crate::host::addons::runtime_context(),
        request,
        operation.deadline,
        context.hold,
    )
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::ProducerFailed))?;
    captured
        .recheck_context(&mut read)
        .await
        .map_err(capture_error)?;
    Ok((bytes, generation))
}

pub(super) async fn prepare_base_current(
    application: &ApplicationServices,
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    selection: &TeletextChannelSelectionSnapshot,
    context: &GuidePreparationContext<'_>,
) -> Result<
    (
        EncodedChannelTeletextDocumentCandidate,
        TeletextGuideGenerationQualification,
    ),
    TeletextAuthoringError,
> {
    let (candidate, captured) = Box::pin(prepare_base_inner(
        application,
        pool,
        input,
        selection,
        context,
    ))
    .await?;
    let deadline = context.deadline;
    let qualification =
        crate::host::teletext_work::spawn_blocking(context.hold.clone(), move || {
            current_qualification(&captured, None, deadline)
        })
        .await
        .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))??;
    Ok((candidate, qualification))
}

async fn prepare_base_inner(
    application: &ApplicationServices,
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    selection: &TeletextChannelSelectionSnapshot,
    operation: &GuidePreparationContext<'_>,
) -> Result<
    (
        EncodedChannelTeletextDocumentCandidate,
        CapturedTeletextGuide,
    ),
    TeletextAuthoringError,
> {
    let (captured, mut read) = capture(pool, input, operation).await?;
    captured
        .recheck_context(&mut read)
        .await
        .map_err(capture_error)?;
    let channels = captured.borrow_channels();
    let locales = super::super::teletext_guide::supported_locales();
    let generation = uuid::Uuid::new_v4().to_string();
    let mut context = captured.context(&generation, &channels, &locales);
    context.page_layout = input.page_layout.as_deref();
    let policy = operation
        .policy
        .encoding_policy(input)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let candidate = prepare_channel_teletext_document_candidate_with_hold(
        application,
        input.channel_id(),
        Some(input.entry_addon_id()),
        TeletextChannelDocumentPreparation {
            context: &context,
            expected_selection: Some(selection),
            custom_page_source: operation.custom_page_source,
            deadline: operation.deadline,
        },
        policy,
        operation.hold.clone(),
    )
    .await
    .map_err(channel_error)?;
    captured
        .recheck_context(&mut read)
        .await
        .map_err(capture_error)?;
    let source = captured
        .guide_source()
        .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidInput))?;
    let candidate = candidate.with_guide_source(source.clone());
    Ok((candidate, captured))
}

pub(super) async fn prepare_effective(
    application: &ApplicationServices,
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    selection: &TeletextEffectiveChannelSelectionSnapshot,
    operation: &Operation,
) -> Result<EncodedEffectiveChannelTeletextDocumentCandidate, TeletextAuthoringError> {
    let context = GuidePreparationContext::from_operation(operation);
    let (candidate, _) = Box::pin(prepare_effective_inner(
        application,
        pool,
        input,
        selection,
        &context,
    ))
    .await?;
    Ok(candidate)
}

pub(super) async fn prepare_effective_current(
    application: &ApplicationServices,
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    selection: &TeletextEffectiveChannelSelectionSnapshot,
    context: &GuidePreparationContext<'_>,
) -> Result<
    (
        EncodedEffectiveChannelTeletextDocumentCandidate,
        TeletextGuideGenerationQualification,
    ),
    TeletextAuthoringError,
> {
    let (candidate, captured) = Box::pin(prepare_effective_inner(
        application,
        pool,
        input,
        selection,
        context,
    ))
    .await?;
    let deadline = context.deadline;
    let scope_expiry = selection.valid_until;
    let qualification =
        crate::host::teletext_work::spawn_blocking(context.hold.clone(), move || {
            current_qualification(&captured, scope_expiry, deadline)
        })
        .await
        .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))??;
    Ok((candidate, qualification))
}

async fn prepare_effective_inner(
    application: &ApplicationServices,
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    selection: &TeletextEffectiveChannelSelectionSnapshot,
    operation: &GuidePreparationContext<'_>,
) -> Result<
    (
        EncodedEffectiveChannelTeletextDocumentCandidate,
        CapturedTeletextGuide,
    ),
    TeletextAuthoringError,
> {
    let (captured, mut read) = capture(pool, input, operation).await?;
    captured
        .recheck_context(&mut read)
        .await
        .map_err(capture_error)?;
    let channels = captured.borrow_channels();
    let locales = super::super::teletext_guide::supported_locales();
    let generation = uuid::Uuid::new_v4().to_string();
    let mut context = captured.context(&generation, &channels, &locales);
    context.page_layout = input.page_layout.as_deref();
    let policy = operation
        .policy
        .encoding_policy(input)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let candidate = match operation.custom_page_source {
        TeletextCustomPageSource::Current => {
            prepare_effective_channel_teletext_document_candidate_checked_with_hold(
                application,
                input.channel_id(),
                Some(input.entry_addon_id()),
                &context,
                policy,
                selection,
                (operation.deadline, operation.hold.clone()),
            )
            .await
            .map_err(channel_error)?
        }
        TeletextCustomPageSource::Published(custom_pages) => {
            prepare_effective_channel_teletext_document_candidate_for_refresh(
                application,
                input.channel_id(),
                Some(input.entry_addon_id()),
                &context,
                policy,
                TeletextEffectiveChannelDocumentRefresh {
                    selection,
                    custom_pages,
                    deadline: operation.deadline,
                    hold: operation.hold.clone(),
                },
            )
            .await
            .map_err(channel_error)?
        }
    };
    captured
        .recheck_context(&mut read)
        .await
        .map_err(capture_error)?;
    let source = captured
        .guide_source()
        .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidInput))?;
    let candidate = candidate.with_guide_source(source.clone());
    Ok((candidate, captured))
}

fn current_qualification(
    captured: &CapturedTeletextGuide,
    scope_expiry: Option<ersatzrs_time::DateTime<Utc>>,
    deadline: Instant,
) -> Result<TeletextGuideGenerationQualification, TeletextAuthoringError> {
    super::check_deadline(deadline)?;
    let source = captured
        .guide_source()
        .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidInput))?;
    let mut qualification = source
        .qualify_current(
            captured
                .snapshot()
                .channels()
                .iter()
                .flat_map(ersatzrs_infra::xmltv::CapturedGuideChannel::occurrences),
            Utc::now(),
        )
        .map_err(|_| failure(TeletextAuthoringStatus::SelectionChanged))?;
    if let Some(expiry) = scope_expiry {
        qualification.valid_until = qualification.valid_until.min(expiry);
    }
    qualification
        .validate_current(Utc::now())
        .map_err(|_| failure(TeletextAuthoringStatus::SelectionChanged))?;
    super::check_deadline(deadline)?;
    Ok(qualification)
}
