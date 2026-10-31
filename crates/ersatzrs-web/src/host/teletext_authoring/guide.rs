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
        EncodedEffectiveChannelTeletextDocumentCandidate,
    },
    teletext_guide::{
        capture_source_bound_teletext_guide_with_hold, CapturedTeletextGuide,
        TeletextGuideCaptureError,
    },
    teletext_runtime_policy::NativeTeletextAuthoringInput,
};
use ersatzrs_infra::{
    db::DbPool,
    xmltv::{
        GuideExpansionLimits, GuideReadBudget, GuideReadLimits, GuideSnapshotDigestLimits,
        GuideSnapshotFailure,
    },
};

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
    operation: &Operation,
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
        operation.guard.hold_work(),
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
    let (captured, mut read) = capture(pool, input, operation).await?;
    captured
        .recheck_context(&mut read)
        .await
        .map_err(capture_error)?;
    let channels = captured.borrow_channels();
    let locales = super::super::teletext_guide::supported_locales();
    let generation = uuid::Uuid::new_v4().to_string();
    let context = captured.context(&generation, &channels, &locales);
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
            deadline: operation.deadline,
        },
        policy,
        operation.guard.hold_work(),
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
    Ok(candidate.with_guide_source(source.clone()))
}

pub(super) async fn prepare_effective(
    application: &ApplicationServices,
    pool: &DbPool,
    input: &NativeTeletextAuthoringInput,
    selection: &TeletextEffectiveChannelSelectionSnapshot,
    operation: &Operation,
) -> Result<EncodedEffectiveChannelTeletextDocumentCandidate, TeletextAuthoringError> {
    let (captured, mut read) = capture(pool, input, operation).await?;
    captured
        .recheck_context(&mut read)
        .await
        .map_err(capture_error)?;
    let channels = captured.borrow_channels();
    let locales = super::super::teletext_guide::supported_locales();
    let generation = uuid::Uuid::new_v4().to_string();
    let context = captured.context(&generation, &channels, &locales);
    let policy = operation
        .policy
        .encoding_policy(input)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let candidate = prepare_effective_channel_teletext_document_candidate_checked_with_hold(
        application,
        input.channel_id(),
        Some(input.entry_addon_id()),
        &context,
        policy,
        selection,
        (operation.deadline, operation.guard.hold_work()),
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
    Ok(candidate.with_guide_source(source.clone()))
}
