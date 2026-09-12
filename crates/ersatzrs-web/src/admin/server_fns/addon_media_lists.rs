use ersatzrs_domain::{
    AddonMediaListAutoPublicationHoldReason, AddonMediaListAvailabilityReason,
    AddonMediaListContentKind, AddonMediaListDeliveryMode, AddonMediaListDetailPolicy,
    AddonMediaListDetailState, AddonMediaListImportMode, AddonMediaListImportPurpose,
    AddonMediaListImportState, AddonMediaListIncrementalStrategy, AddonMediaListItemAvailability,
    AddonMediaListItemKind, AddonMediaListItemRole, AddonMediaListPresentationKind,
    AddonMediaListPublicationEnqueueTrigger, AddonMediaListPublicationErrorCategory,
    AddonMediaListPublicationEventKind, AddonMediaListPublicationFilesystemTransition,
    AddonMediaListPublicationOutcome, AddonMediaListPublicationPhase,
    AddonMediaListPublicationRetryClass, AddonMediaListPublicationState,
    AddonMediaListRecoveryAction, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, AddonMediaListReviewState, RemoteStreamLiveness,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListAdminSortField {
    #[default]
    Name,
    Id,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListAdminSortDirection {
    #[default]
    Ascending,
    Descending,
}

#[cfg(feature = "ssr")]
impl From<AddonMediaListAdminSortField>
    for ersatzrs_core::repositories::AddonMediaListAdminSortField
{
    fn from(value: AddonMediaListAdminSortField) -> Self {
        match value {
            AddonMediaListAdminSortField::Name => Self::Name,
            AddonMediaListAdminSortField::Id => Self::Id,
        }
    }
}

#[cfg(feature = "ssr")]
impl From<AddonMediaListAdminSortDirection>
    for ersatzrs_core::repositories::AddonMediaListAdminSortDirection
{
    fn from(value: AddonMediaListAdminSortDirection) -> Self {
        match value {
            AddonMediaListAdminSortDirection::Ascending => Self::Ascending,
            AddonMediaListAdminSortDirection::Descending => Self::Descending,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AddonMediaListPublishResult {
    Queued {
        operation_id: i64,
        generation: i32,
        existing: bool,
    },
    PreparingPlaybackDetails {
        item_count: u64,
    },
    PreparingMedia {
        item_count: u64,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AddonMediaListResetResult {
    Queued {
        operation_id: i64,
        generation: i32,
        existing: bool,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AddonMediaListReconcileResult {
    Queued {
        operation_id: i64,
        generation: i32,
        existing: bool,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListRestoreResult {
    pub operation_id: i64,
    pub generation: i32,
    pub saved_revision: i32,
    pub target_published_revision: i32,
    pub existing: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListSourceProbeState {
    Queued,
    Running,
    Complete,
    Stale,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListSourceProbeOutcome {
    Available,
    Unsupported,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListSourceProbeRow {
    pub state: AddonMediaListSourceProbeState,
    pub saved_draft_revision: i32,
    pub source_revision: Option<String>,
    pub capabilities: Vec<String>,
    pub outcome: Option<AddonMediaListSourceProbeOutcome>,
    pub provider_id: Option<String>,
    pub list_title: Option<String>,
    pub count_hint: Option<u32>,
    pub returned_item_count: Option<u32>,
    pub page_complete: Option<bool>,
    pub duration_milliseconds: Option<u64>,
    pub diagnostic_code: Option<String>,
    pub diagnostic: Option<String>,
    pub attempt_count: u32,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListDraftDiffRow {
    pub draft_revision: i32,
    pub published_revision: Option<i32>,
    pub exact: bool,
    pub added: u64,
    pub metadata_changed: u64,
    pub excluded: u64,
    pub unavailable: u64,
    pub conflicted: u64,
    pub pending_removals: u64,
    pub reappeared: u64,
    pub removal_candidates: u64,
    pub added_item_ids: Vec<i64>,
    pub metadata_changed_item_ids: Vec<i64>,
    pub excluded_item_ids: Vec<i64>,
    pub unavailable_item_ids: Vec<i64>,
    pub conflicted_item_ids: Vec<i64>,
    pub pending_removal_item_ids: Vec<i64>,
    pub reappeared_item_ids: Vec<i64>,
    pub removal_candidate_item_ids: Vec<i64>,
    pub pending_removal_generations: BTreeMap<i64, i32>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListResetImpactRow {
    pub draft_revision: i32,
    pub published_revision: i32,
    pub published_items: u64,
    pub generated_playlists: u64,
    pub channels: u64,
    pub schedules: u64,
    pub active_or_future_playouts: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListRevisionSummaryRow {
    pub revision: i32,
    pub source_draft_revision: Option<i32>,
    pub completed_at: Option<String>,
    pub restorable: bool,
    pub published_item_count: u64,
    pub file_count: u64,
    pub artifact_count: u64,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListRevisionDiffRow {
    pub list_metadata_changed: bool,
    pub effective_options_changed: bool,
    pub items_added: u64,
    pub items_removed: u64,
    pub items_changed: u64,
    pub files_added: u64,
    pub files_removed: u64,
    pub files_changed: u64,
    pub artifacts_added: u64,
    pub artifacts_removed: u64,
}

#[cfg(not(target_arch = "wasm32"))]
mod lifecycle_transport {
    use super::{
        AddonMediaListDraftDiffRow, AddonMediaListResetImpactRow, AddonMediaListRevisionDiffRow,
        AddonMediaListRevisionSummaryRow, AddonMediaListSourceProbeOutcome,
        AddonMediaListSourceProbeRow, AddonMediaListSourceProbeState,
    };
    use ersatzrs_core::repositories::{
        AddonMediaListDraftDiff, AddonMediaListResetImpact, AddonMediaListRevisionDiff,
        AddonMediaListRevisionSummary, AddonMediaListSourceProbe,
    };

    impl From<AddonMediaListSourceProbe> for AddonMediaListSourceProbeRow {
        fn from(value: AddonMediaListSourceProbe) -> Self {
            Self {
                state: match value.state {
                    ersatzrs_core::repositories::AddonMediaListSourceProbeState::Queued => {
                        AddonMediaListSourceProbeState::Queued
                    }
                    ersatzrs_core::repositories::AddonMediaListSourceProbeState::Running => {
                        AddonMediaListSourceProbeState::Running
                    }
                    ersatzrs_core::repositories::AddonMediaListSourceProbeState::Complete => {
                        AddonMediaListSourceProbeState::Complete
                    }
                    ersatzrs_core::repositories::AddonMediaListSourceProbeState::Stale => {
                        AddonMediaListSourceProbeState::Stale
                    }
                },
                saved_draft_revision: value.saved_draft_revision,
                source_revision: value.source_revision,
                capabilities: value.capabilities,
                outcome: value.outcome.map(|outcome| match outcome {
                    ersatzrs_core::repositories::AddonMediaListSourceProbeOutcome::Available => {
                        AddonMediaListSourceProbeOutcome::Available
                    }
                    ersatzrs_core::repositories::AddonMediaListSourceProbeOutcome::Unsupported => {
                        AddonMediaListSourceProbeOutcome::Unsupported
                    }
                    ersatzrs_core::repositories::AddonMediaListSourceProbeOutcome::Failed => {
                        AddonMediaListSourceProbeOutcome::Failed
                    }
                }),
                provider_id: value.provider_id,
                list_title: value.list_title,
                count_hint: value.count_hint,
                returned_item_count: value.returned_item_count,
                page_complete: value.page_complete,
                duration_milliseconds: value.duration_milliseconds,
                diagnostic_code: value.diagnostic_code,
                diagnostic: value.diagnostic,
                attempt_count: value.attempt_count,
                updated_at: value.updated_at.to_rfc3339(),
                completed_at: value.completed_at.map(|date| date.to_rfc3339()),
            }
        }
    }

    impl From<AddonMediaListDraftDiff> for AddonMediaListDraftDiffRow {
        fn from(value: AddonMediaListDraftDiff) -> Self {
            Self {
                draft_revision: value.draft_revision,
                published_revision: value.published_revision,
                exact: value.exact,
                added: value.added,
                metadata_changed: value.metadata_changed,
                excluded: value.excluded,
                unavailable: value.unavailable,
                conflicted: value.conflicted,
                pending_removals: value.pending_removals,
                reappeared: value.reappeared,
                removal_candidates: value.removal_candidates,
                added_item_ids: value.added_item_ids,
                metadata_changed_item_ids: value.metadata_changed_item_ids,
                excluded_item_ids: value.excluded_item_ids,
                unavailable_item_ids: value.unavailable_item_ids,
                conflicted_item_ids: value.conflicted_item_ids,
                pending_removal_item_ids: value.pending_removal_item_ids,
                reappeared_item_ids: value.reappeared_item_ids,
                removal_candidate_item_ids: value.removal_candidate_item_ids,
                pending_removal_generations: value.pending_removal_generations,
            }
        }
    }

    impl From<AddonMediaListResetImpact> for AddonMediaListResetImpactRow {
        fn from(value: AddonMediaListResetImpact) -> Self {
            Self {
                draft_revision: value.draft_revision,
                published_revision: value.published_revision,
                published_items: value.published_items,
                generated_playlists: value.generated_playlists,
                channels: value.channels,
                schedules: value.schedules,
                active_or_future_playouts: value.active_or_future_playouts,
            }
        }
    }

    impl From<AddonMediaListRevisionSummary> for AddonMediaListRevisionSummaryRow {
        fn from(value: AddonMediaListRevisionSummary) -> Self {
            Self {
                revision: value.revision,
                source_draft_revision: value.source_draft_revision,
                completed_at: value.completed_at.map(|date| date.to_rfc3339()),
                restorable: value.restorable,
                published_item_count: value.published_item_count,
                file_count: value.file_count,
                artifact_count: value.artifact_count,
            }
        }
    }

    impl From<AddonMediaListRevisionDiff> for AddonMediaListRevisionDiffRow {
        fn from(value: AddonMediaListRevisionDiff) -> Self {
            Self {
                list_metadata_changed: value.list_metadata_changed,
                effective_options_changed: value.effective_options_changed,
                items_added: value.items_added,
                items_removed: value.items_removed,
                items_changed: value.items_changed,
                files_added: value.files_added,
                files_removed: value.files_removed,
                files_changed: value.files_changed,
                artifacts_added: value.artifacts_added,
                artifacts_removed: value.artifacts_removed,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationBlockerKind {
    OverviewIncomplete,
    PublicationInProgress,
    RecoveryRequired,
    CollectionTitleMissing,
    InvalidArtwork,
    NoIncludedItems,
    MoviePrimaryCount,
    PlaybackDecisionRequired,
    PlaybackDetailsUnavailable,
    PlaybackDetailsFailed,
    UnavailableWithoutReplacement,
    ItemTitleMissing,
    SeasonEpisodeMissing,
    ArtistMissing,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPublicationBlocker {
    pub kind: AddonMediaListPublicationBlockerKind,
    pub item_rank: Option<i32>,
    pub item_title: Option<String>,
    pub count: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPublicationReadiness {
    pub included_count: u64,
    pub restricted_excluded_count: u64,
    pub blockers: Vec<AddonMediaListPublicationBlocker>,
    pub recommendation_count: u64,
    pub required_playback_detail_count: u64,
    pub required_playback_detail_pending: u64,
    #[serde(default)]
    pub required_prepared_media_count: u64,
    #[serde(default)]
    pub required_prepared_media_pending: u64,
    /// Required media whose latest attempt at the saved revision failed.
    #[serde(default)]
    pub required_prepared_media_failed: u64,
    /// Latest failed attempts, including earlier saved revisions as history.
    #[serde(default)]
    pub prepared_media_failures: Vec<AddonMediaListPreparationFailure>,
    pub optional_detail_total: u64,
    pub optional_detail_complete: u64,
    pub optional_detail_pending: u64,
    pub saved_revision: i32,
    pub review_state: AddonMediaListReviewState,
    pub publication: Option<AddonMediaListPublicationStatus>,
}

/// Closed preparation failure category; never provider text.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPreparationFailureReason {
    Failed,
    InputChanged,
    InvalidRequest,
    Cancelled,
    AttemptsExhausted,
    StorageFull,
    ProcessRecoveryRequired,
    Unsupported,
}

/// Latest failed preparation attempts for one saved revision and category.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPreparationFailure {
    pub saved_revision: i32,
    pub reason: AddonMediaListPreparationFailureReason,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPublicationStatus {
    pub job_id: i64,
    pub operation_id: i64,
    pub generation: i32,
    pub saved_revision: i32,
    pub target_published_revision: i32,
    pub phase: AddonMediaListPublicationPhase,
    pub state: AddonMediaListPublicationState,
    #[serde(default)]
    pub outcome: Option<AddonMediaListPublicationOutcome>,
    pub attempt_count: i32,
    pub last_progress_at: String,
    pub diagnostic_code: Option<String>,
    pub recovery_diagnostic_present: bool,
    pub can_retry: bool,
    pub can_restore: bool,
}

/// Redacted, typed transport projection for all retained publication jobs.
///
/// Keep this shape free of provider text and filesystem or network values: it
/// is the single payload serialized by both the hydrated Copy action and the
/// no-JavaScript diagnostics download.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPublicationHistoryRow {
    pub list_id: i64,
    pub jobs: Vec<AddonMediaListPublicationHistoryJobRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPublicationHistoryJobRow {
    pub job_id: i64,
    pub operation_id: i64,
    pub generation: i32,
    pub saved_revision: i32,
    pub target_published_revision: i32,
    pub phase: AddonMediaListPublicationPhase,
    pub state: AddonMediaListPublicationState,
    pub outcome: Option<AddonMediaListPublicationOutcome>,
    pub events: Vec<AddonMediaListPublicationEventRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListPublicationEventRow {
    pub sequence: i32,
    pub kind: AddonMediaListPublicationEventKind,
    pub enqueue_trigger: Option<AddonMediaListPublicationEnqueueTrigger>,
    pub phase: Option<AddonMediaListPublicationPhase>,
    pub attempt: Option<i32>,
    pub related_operation_id: Option<i64>,
    pub filesystem_transition: Option<AddonMediaListPublicationFilesystemTransition>,
    pub retry_class: Option<AddonMediaListPublicationRetryClass>,
    pub recovery_action: Option<AddonMediaListRecoveryAction>,
    pub error_category: Option<AddonMediaListPublicationErrorCategory>,
    pub os_error_code: Option<i32>,
    pub published_item_count: Option<i64>,
    pub playlist_item_count: Option<i64>,
    pub artwork_count: Option<i64>,
    pub outcome: Option<AddonMediaListPublicationOutcome>,
    pub occurred_at: String,
}

pub fn redacted_publication_diagnostics_json(
    history: &AddonMediaListPublicationHistoryRow,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(history)
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPublicationHistory>
    for AddonMediaListPublicationHistoryRow
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPublicationHistory) -> Self {
        Self {
            list_id: value.list_id,
            jobs: value.jobs.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPublicationHistoryJob>
    for AddonMediaListPublicationHistoryJobRow
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPublicationHistoryJob) -> Self {
        Self {
            job_id: value.job_id,
            operation_id: value.operation_id,
            generation: value.generation,
            saved_revision: value.saved_revision,
            target_published_revision: value.target_published_revision,
            phase: value.phase,
            state: value.state,
            outcome: value.outcome,
            events: value.events.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPublicationEvent>
    for AddonMediaListPublicationEventRow
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPublicationEvent) -> Self {
        Self {
            sequence: value.sequence,
            kind: value.kind,
            enqueue_trigger: value.enqueue_trigger,
            phase: value.phase,
            attempt: value.attempt,
            related_operation_id: value.related_operation_id,
            filesystem_transition: value.filesystem_transition,
            retry_class: value.retry_class,
            recovery_action: value.recovery_action,
            error_category: value.error_category,
            os_error_code: value.os_error_code,
            published_item_count: value.published_item_count,
            playlist_item_count: value.playlist_item_count,
            artwork_count: value.artwork_count,
            outcome: value.outcome,
            occurred_at: value.occurred_at.to_rfc3339(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPublicationStatus>
    for AddonMediaListPublicationStatus
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPublicationStatus) -> Self {
        Self {
            outcome: value.outcome,
            job_id: value.job_id,
            operation_id: value.operation_id,
            generation: value.generation,
            saved_revision: value.saved_revision,
            target_published_revision: value.target_published_revision,
            phase: value.phase,
            state: value.state,
            attempt_count: value.attempt_count,
            last_progress_at: value.last_progress_at.to_rfc3339(),
            diagnostic_code: value.diagnostic_code,
            recovery_diagnostic_present: value.recovery_diagnostic_present,
            can_retry: value.can_retry,
            can_restore: value.can_restore,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPublicationReadiness>
    for AddonMediaListPublicationReadiness
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPublicationReadiness) -> Self {
        Self {
            included_count: value.included_count,
            restricted_excluded_count: value.restricted_excluded_count,
            blockers: value.blockers.into_iter().map(Into::into).collect(),
            recommendation_count: value.recommendation_count,
            required_playback_detail_count: value.required_playback_detail_count,
            required_playback_detail_pending: value.required_playback_detail_pending,
            required_prepared_media_count: value.required_prepared_media_count,
            required_prepared_media_pending: value.required_prepared_media_pending,
            required_prepared_media_failed: value.required_prepared_media_failed,
            prepared_media_failures: value
                .prepared_media_failures
                .into_iter()
                .map(Into::into)
                .collect(),
            optional_detail_total: value.optional_detail_total,
            optional_detail_complete: value.optional_detail_complete,
            optional_detail_pending: value.optional_detail_pending,
            saved_revision: value.saved_revision,
            review_state: value.review_state,
            publication: value.publication.map(Into::into),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPreparationFailure>
    for AddonMediaListPreparationFailure
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPreparationFailure) -> Self {
        use ersatzrs_core::repositories::AddonMediaListPreparationFailureReason as Reason;
        Self {
            saved_revision: value.saved_revision,
            reason: match value.reason {
                Reason::Failed => AddonMediaListPreparationFailureReason::Failed,
                Reason::InputChanged => AddonMediaListPreparationFailureReason::InputChanged,
                Reason::InvalidRequest => AddonMediaListPreparationFailureReason::InvalidRequest,
                Reason::Cancelled => AddonMediaListPreparationFailureReason::Cancelled,
                Reason::AttemptsExhausted => {
                    AddonMediaListPreparationFailureReason::AttemptsExhausted
                }
                Reason::StorageFull => AddonMediaListPreparationFailureReason::StorageFull,
                Reason::ProcessRecoveryRequired => {
                    AddonMediaListPreparationFailureReason::ProcessRecoveryRequired
                }
                Reason::Unsupported => AddonMediaListPreparationFailureReason::Unsupported,
            },
            count: value.count,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListPublicationBlocker>
    for AddonMediaListPublicationBlocker
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListPublicationBlocker) -> Self {
        Self {
            kind: match value.kind {
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::OverviewIncomplete => AddonMediaListPublicationBlockerKind::OverviewIncomplete,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PublicationInProgress => AddonMediaListPublicationBlockerKind::PublicationInProgress,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::RecoveryRequired => AddonMediaListPublicationBlockerKind::RecoveryRequired,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::CollectionTitleMissing => AddonMediaListPublicationBlockerKind::CollectionTitleMissing,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::InvalidArtwork => AddonMediaListPublicationBlockerKind::InvalidArtwork,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::NoIncludedItems => AddonMediaListPublicationBlockerKind::NoIncludedItems,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::MoviePrimaryCount => AddonMediaListPublicationBlockerKind::MoviePrimaryCount,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PlaybackDecisionRequired => AddonMediaListPublicationBlockerKind::PlaybackDecisionRequired,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable => AddonMediaListPublicationBlockerKind::PlaybackDetailsUnavailable,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed => AddonMediaListPublicationBlockerKind::PlaybackDetailsFailed,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement => AddonMediaListPublicationBlockerKind::UnavailableWithoutReplacement,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::ItemTitleMissing => AddonMediaListPublicationBlockerKind::ItemTitleMissing,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing => AddonMediaListPublicationBlockerKind::SeasonEpisodeMissing,
                ersatzrs_core::repositories::AddonMediaListPublicationBlockerKind::ArtistMissing => AddonMediaListPublicationBlockerKind::ArtistMissing,
            },
            item_rank: value.item_rank,
            item_title: value.item_title,
            count: value.count,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListRow {
    pub id: i64,
    pub addon_id: String,
    pub provider_id: String,
    pub source_url: String,
    pub name: String,
    pub item_count: i32,
    pub match_count: i32,
    pub refresh_cadence: AddonMediaListRefreshCadence,
    pub last_refresh_succeeded_at: Option<String>,
    pub next_refresh_at: Option<String>,
    pub refresh_retry_at: Option<String>,
    pub generate_playlist: bool,
    pub presentation_kind: AddonMediaListPresentationKind,
    pub requested_presentation_kind: AddonMediaListPresentationKind,
    pub presentation_error: Option<String>,
    pub review_state: AddonMediaListReviewState,
    pub review_error: Option<String>,
    pub published_revision: i32,
    pub draft_revision: i32,
    pub detail_import_policy: AddonMediaListDetailPolicy,
    pub import_progress: Option<AddonMediaListImportProgressRow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListForm {
    #[serde(default)]
    pub delivery_mode: AddonMediaListDeliveryMode,
    #[serde(default)]
    pub delivery_cutoff_at: Option<String>,
    #[serde(default)]
    pub auto_publication_hold: Option<AddonMediaListAutoPublicationHoldRow>,
    pub id: i64,
    pub name: String,
    pub addon_id: String,
    pub source_url: String,
    pub folder_name: String,
    pub refresh_cadence: AddonMediaListRefreshCadence,
    #[serde(default)]
    pub refresh_publication_policy: AddonMediaListRefreshPublicationPolicy,
    pub last_refresh_succeeded_at: Option<String>,
    pub next_refresh_at: Option<String>,
    pub refresh_retry_at: Option<String>,
    pub generate_playlist: bool,
    pub content_kind_override: AddonMediaListContentKind,
    pub presentation_kind: AddonMediaListPresentationKind,
    pub requested_presentation_kind: AddonMediaListPresentationKind,
    pub presentation_error: Option<String>,
    pub review_state: AddonMediaListReviewState,
    pub review_error: Option<String>,
    pub published_revision: i32,
    pub draft_revision: i32,
    pub legacy_chapter_input: String,
    pub detail_import_policy: AddonMediaListDetailPolicy,
    pub import_progress: Option<AddonMediaListImportProgressRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListAutoPublicationHoldRow {
    pub reason: AddonMediaListAutoPublicationHoldReason,
    pub draft_revision: i32,
    pub published_revision: i32,
    pub recorded_at: String,
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_core::repositories::AddonMediaListAutoPublicationHold>
    for AddonMediaListAutoPublicationHoldRow
{
    fn from(value: ersatzrs_core::repositories::AddonMediaListAutoPublicationHold) -> Self {
        Self {
            reason: value.reason,
            draft_revision: value.draft_revision,
            published_revision: value.published_revision,
            recorded_at: value.recorded_at.to_rfc3339(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListImportProgressRow {
    pub state: AddonMediaListImportState,
    pub purpose: AddonMediaListImportPurpose,
    pub mode: AddonMediaListImportMode,
    pub strategy: AddonMediaListIncrementalStrategy,
    pub started_at: String,
    pub discovered_count: u64,
    pub examined_count: u64,
    pub archived_skipped_count: u64,
    pub emitted_count: u64,
    pub total_hint: Option<u64>,
    pub retained_item_count: u64,
    pub detail_total: u64,
    pub detail_complete: u64,
    pub detail_skipped: u64,
    pub detail_failed: u64,
    pub detail_pending: u64,
    pub current_title: Option<String>,
    pub last_progress_at: Option<String>,
    pub diagnostic: Option<String>,
}

/// Browser-safe wire representation of an operator action for an overview import.
///
/// The matching repository type is intentionally native-only; keeping this small
/// enum in the web contract prevents the hydrated bundle from depending on
/// `ersatzrs-core`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListImportAction {
    Pause,
    Resume,
    Retry,
    Cancel,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<AddonMediaListImportAction> for ersatzrs_core::repositories::AddonMediaListImportAction {
    fn from(action: AddonMediaListImportAction) -> Self {
        match action {
            AddonMediaListImportAction::Pause => Self::Pause,
            AddonMediaListImportAction::Resume => Self::Resume,
            AddonMediaListImportAction::Retry => Self::Retry,
            AddonMediaListImportAction::Cancel => Self::Cancel,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListImportProgress>
    for AddonMediaListImportProgressRow
{
    fn from(progress: ersatzrs_core::repositories::AddonMediaListImportProgress) -> Self {
        Self {
            state: progress.state,
            purpose: progress.purpose,
            mode: progress.mode,
            strategy: progress.strategy,
            started_at: progress.started_at,
            discovered_count: progress.discovered_count,
            examined_count: progress.examined_count,
            archived_skipped_count: progress.archived_skipped_count,
            emitted_count: progress.emitted_count,
            total_hint: progress.total_hint,
            retained_item_count: progress.retained_item_count,
            detail_total: progress.detail_total,
            detail_complete: progress.detail_complete,
            detail_skipped: progress.detail_skipped,
            detail_failed: progress.detail_failed,
            detail_pending: progress.detail_pending,
            current_title: progress.current_title,
            last_progress_at: progress.last_progress_at,
            diagnostic: progress.diagnostic,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListProviderRow {
    pub addon_id: String,
    pub name: String,
    pub supports_adaptive_refresh: bool,
    #[serde(default)]
    pub supports_download_ahead: bool,
}

// These independent transport projections mirror the admin read model; they
// are not alternate states of one boolean state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemRow {
    pub id: i64,
    pub rank: i32,
    pub kind: AddonMediaListItemKind,
    pub display_title: String,
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason: Option<String>,
    pub availability_reason_code: Option<AddonMediaListAvailabilityReason>,
    pub replacement_media_item_id: Option<i64>,
    pub replacement_media_version_id: Option<i64>,
    pub replacement_title: Option<String>,
    pub replacement_path: Option<String>,
    pub replacement_valid: bool,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub first_aired: Option<String>,
    pub source_url: Option<String>,
    pub duration_seconds: Option<u64>,
    pub provider_liveness: RemoteStreamLiveness,
    pub liveness_override: Option<RemoteStreamLiveness>,
    pub effective_liveness: RemoteStreamLiveness,
    pub liveness_conflict: bool,
    pub published_liveness: RemoteStreamLiveness,
    pub parent_provider_id: Option<String>,
    pub chapter_input: String,
    pub provider_chapter_input: String,
    pub remote_stream_id: Option<i64>,
    pub projected_episode_id: Option<i64>,
    pub content_kind: AddonMediaListContentKind,
    pub collection_role: AddonMediaListItemRole,
    pub thumbnail_url: Option<String>,
    pub tags: Vec<String>,
    pub people: Vec<String>,
    pub included: bool,
    pub published: bool,
    pub metadata_conflict_count: u32,
    pub detail_state: AddonMediaListDetailState,
    pub detail_diagnostic: Option<String>,
}

/// Exact operator-owned values for one quick table edit or undo snapshot.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemReviewEdit {
    pub item_id: i64,
    pub content_kind: AddonMediaListContentKind,
    pub included: bool,
    pub liveness_override: Option<RemoteStreamLiveness>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<AddonMediaListItemReviewEdit>
    for ersatzrs_core::repositories::AddonMediaListItemReviewEdit
{
    fn from(edit: AddonMediaListItemReviewEdit) -> Self {
        Self {
            item_id: edit.item_id,
            content_kind: edit.content_kind,
            included: edit.included,
            liveness_override: edit.liveness_override,
            season: edit.season,
            episode: edit.episode,
        }
    }
}

/// Per-availability entry totals for one list.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemCountsRow {
    pub available: u64,
    pub unavailable: u64,
    pub unknown: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListNfoPerson {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i32>,
    pub thumb: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListArtworkRole {
    Poster,
    Fanart,
    Thumb,
    #[default]
    Other,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListArtwork {
    pub url: String,
    pub role: AddonMediaListArtworkRole,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub language: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AddonMediaListNfoMetadata {
    pub title: String,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub outline: Option<String>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub show_title: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub year: Option<i32>,
    pub release_date: Option<String>,
    pub content_ratings: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub people: Vec<AddonMediaListNfoPerson>,
    pub writers: Vec<String>,
    pub directors: Vec<String>,
    pub artists: Vec<String>,
    pub producers: Vec<String>,
    pub original_broadcasters: Vec<String>,
    pub broadcasters: Vec<String>,
    pub collection: Option<String>,
    pub artwork: Vec<AddonMediaListArtwork>,
    pub artwork_urls: Vec<String>,
    pub guids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListNfoOwnership {
    #[default]
    Generated,
    User,
    Invalid,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListNfoForm {
    pub metadata: AddonMediaListNfoMetadata,
    pub etag: String,
    pub ownership: AddonMediaListNfoOwnership,
    pub error: Option<String>,
    pub path_available: bool,
    pub conflicts: BTreeMap<String, AddonMediaListMetadataConflict>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListMetadataConflict {
    pub provider_before: serde_json::Value,
    pub provider_after: serde_json::Value,
    pub operator_value: serde_json::Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListItemDetail {
    pub item: AddonMediaListItemRow,
    pub nfo: AddonMediaListNfoForm,
    pub chapters: Vec<AddonMediaListItemRow>,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListItemAdminRow> for AddonMediaListItemRow {
    fn from(row: ersatzrs_core::repositories::AddonMediaListItemAdminRow) -> Self {
        Self {
            id: row.id,
            rank: row.rank,
            kind: row.kind,
            display_title: row.display_title,
            availability: row.availability,
            availability_reason: row.availability_reason,
            availability_reason_code: row.availability_reason_code,
            replacement_media_item_id: row.replacement_media_item_id,
            replacement_media_version_id: row.replacement_media_version_id,
            replacement_title: row.replacement_title,
            replacement_path: row.replacement_path,
            replacement_valid: row.replacement_valid,
            title: row.title,
            year: row.year,
            season: row.season,
            episode: row.episode,
            first_aired: row.first_aired,
            source_url: row.source_url,
            duration_seconds: row.duration_seconds,
            provider_liveness: row.provider_liveness,
            liveness_override: row.liveness_override,
            effective_liveness: row.effective_liveness,
            liveness_conflict: row.liveness_conflict,
            published_liveness: row.published_liveness,
            parent_provider_id: row.parent_provider_id,
            chapter_input: row.chapter_input,
            provider_chapter_input: row.provider_chapter_input,
            projected_episode_id: row.projected_episode_id,
            remote_stream_id: row.remote_stream_id,
            content_kind: row.content_kind,
            collection_role: row.collection_role,
            thumbnail_url: row.thumbnail_url,
            tags: row.tags,
            people: row.people,
            included: row.included,
            published: row.published,
            metadata_conflict_count: row.metadata_conflict_count,
            detail_state: row.detail_state,
            detail_diagnostic: row.detail_diagnostic,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListNfoMetadata> for AddonMediaListNfoMetadata {
    fn from(metadata: ersatzrs_core::repositories::AddonMediaListNfoMetadata) -> Self {
        Self {
            title: metadata.title,
            original_title: metadata.original_title,
            sort_title: metadata.sort_title,
            outline: metadata.outline,
            plot: metadata.plot,
            tagline: metadata.tagline,
            show_title: metadata.show_title,
            season: metadata.season,
            episode: metadata.episode,
            year: metadata.year,
            release_date: metadata.release_date,
            content_ratings: metadata.content_ratings,
            genres: metadata.genres,
            tags: metadata.tags,
            studios: metadata.studios,
            music_labels: metadata.music_labels,
            languages: metadata.languages,
            people: metadata
                .people
                .into_iter()
                .map(|person| AddonMediaListNfoPerson {
                    name: person.name,
                    role: person.role,
                    order: person.order,
                    thumb: person.thumb,
                })
                .collect(),
            writers: metadata.writers,
            directors: metadata.directors,
            artists: metadata.artists,
            producers: metadata.producers,
            original_broadcasters: metadata.original_broadcasters,
            broadcasters: metadata.broadcasters,
            collection: metadata.collection,
            artwork: metadata
                .artwork
                .into_iter()
                .map(|artwork| AddonMediaListArtwork {
                    url: artwork.url,
                    role: match artwork.role {
                        ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster => {
                            AddonMediaListArtworkRole::Poster
                        }
                        ersatzrs_core::repositories::AddonMediaListArtworkRole::Fanart => {
                            AddonMediaListArtworkRole::Fanart
                        }
                        ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb => {
                            AddonMediaListArtworkRole::Thumb
                        }
                        ersatzrs_core::repositories::AddonMediaListArtworkRole::Other => {
                            AddonMediaListArtworkRole::Other
                        }
                    },
                    width: artwork.width,
                    height: artwork.height,
                    language: artwork.language,
                })
                .collect(),
            artwork_urls: metadata.artwork_urls,
            guids: metadata.guids,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<AddonMediaListNfoMetadata> for ersatzrs_core::repositories::AddonMediaListNfoMetadata {
    fn from(metadata: AddonMediaListNfoMetadata) -> Self {
        Self {
            title: metadata.title,
            original_title: metadata.original_title,
            sort_title: metadata.sort_title,
            outline: metadata.outline,
            plot: metadata.plot,
            tagline: metadata.tagline,
            show_title: metadata.show_title,
            season: metadata.season,
            episode: metadata.episode,
            year: metadata.year,
            release_date: metadata.release_date,
            content_ratings: metadata.content_ratings,
            genres: metadata.genres,
            tags: metadata.tags,
            studios: metadata.studios,
            music_labels: metadata.music_labels,
            languages: metadata.languages,
            people: metadata
                .people
                .into_iter()
                .map(
                    |person| ersatzrs_core::repositories::AddonMediaListNfoPerson {
                        name: person.name,
                        role: person.role,
                        order: person.order,
                        thumb: person.thumb,
                    },
                )
                .collect(),
            writers: metadata.writers,
            directors: metadata.directors,
            artists: metadata.artists,
            producers: metadata.producers,
            original_broadcasters: metadata.original_broadcasters,
            broadcasters: metadata.broadcasters,
            collection: metadata.collection,
            artwork: metadata
                .artwork
                .into_iter()
                .map(
                    |artwork| ersatzrs_core::repositories::AddonMediaListArtwork {
                        url: artwork.url,
                        role: match artwork.role {
                            AddonMediaListArtworkRole::Poster => {
                                ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster
                            }
                            AddonMediaListArtworkRole::Fanart => {
                                ersatzrs_core::repositories::AddonMediaListArtworkRole::Fanart
                            }
                            AddonMediaListArtworkRole::Thumb => {
                                ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb
                            }
                            AddonMediaListArtworkRole::Other => {
                                ersatzrs_core::repositories::AddonMediaListArtworkRole::Other
                            }
                        },
                        width: artwork.width,
                        height: artwork.height,
                        language: artwork.language,
                    },
                )
                .collect(),
            artwork_urls: metadata.artwork_urls,
            guids: metadata.guids,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListNfoForm> for AddonMediaListNfoForm {
    fn from(form: ersatzrs_core::repositories::AddonMediaListNfoForm) -> Self {
        use ersatzrs_core::repositories::AddonMediaListNfoOwnership as CoreOwnership;
        Self {
            metadata: form.metadata.into(),
            etag: form.etag,
            ownership: match form.ownership {
                CoreOwnership::Generated => AddonMediaListNfoOwnership::Generated,
                CoreOwnership::User => AddonMediaListNfoOwnership::User,
                CoreOwnership::Invalid => AddonMediaListNfoOwnership::Invalid,
            },
            error: form.error,
            path_available: form.path_available,
            conflicts: form
                .conflicts
                .into_iter()
                .map(|(field, conflict)| {
                    (
                        field,
                        AddonMediaListMetadataConflict {
                            provider_before: conflict.provider_before,
                            provider_after: conflict.provider_after,
                            operator_value: conflict.operator_value,
                        },
                    )
                })
                .collect(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ersatzrs_core::repositories::AddonMediaListItemDetail> for AddonMediaListItemDetail {
    fn from(detail: ersatzrs_core::repositories::AddonMediaListItemDetail) -> Self {
        Self {
            item: detail.item.into(),
            nfo: detail.nfo.into(),
            chapters: detail.chapters.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddonMediaListReplacementRow {
    pub media_item_id: i64,
    pub media_version_id: i64,
    pub title: String,
    pub path: String,
}

#[server]
pub async fn list_addon_media_list_providers(
) -> Result<Vec<AddonMediaListProviderRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let overview = state
        .application
        .addon_overview()
        .await
        .map_err(ServerFnError::new)?;
    let mut providers = overview
        .installed
        .into_iter()
        .filter(|item| item.enabled)
        .filter(|item| {
            item.manifest.capabilities.iter().any(|capability| {
                capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V1
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V2
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V3
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V4
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V5
                    || capability.id == ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V6
            })
        })
        .map(|item| AddonMediaListProviderRow {
            supports_adaptive_refresh: item.manifest.has_media_list_source_check_capability(),
            supports_download_ahead: item
                .manifest
                .has_capability(ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_PREPARE_V2)
                && item
                    .manifest
                    .has_capability(ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1),
            addon_id: item.addon_id,
            name: item
                .manifest
                .name
                .get("en-US")
                .or_else(|| item.manifest.name.values().next())
                .cloned()
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(providers)
}

/// The add-on that would process one operator source URL, if exactly one does.
///
/// The dialog calls this while the operator edits the URL so the resolved
/// choice is visible before saving, and remains overridable.
#[server]
pub async fn resolve_addon_media_list_provider(
    source_url: String,
) -> Result<Option<AddonMediaListProviderRow>, ServerFnError> {
    use crate::host::state::AppState;

    let source_url = source_url.trim().to_owned();
    if source_url.is_empty() {
        return Ok(None);
    }
    let state = expect_context::<AppState>();
    let routing = state
        .application
        .resolve_source_addon(
            source_url,
            MEDIA_LIST_CAPABILITIES
                .iter()
                .map(|capability| (*capability).to_owned())
                .collect(),
        )
        .await
        .map_err(ServerFnError::new)?;
    let Some(addon_id) = routing.addon_id().map(str::to_owned) else {
        return Ok(None);
    };
    Ok(list_addon_media_list_providers()
        .await?
        .into_iter()
        .find(|provider| provider.addon_id == addon_id))
}

#[server]
pub async fn list_addon_media_lists(
    page: u32,
    page_size: u32,
    sort_field: AddonMediaListAdminSortField,
    sort_direction: AddonMediaListAdminSortDirection,
) -> Result<(Vec<AddonMediaListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_addon_media_lists_with_state(&state, page, page_size, sort_field, sort_direction)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn list_addon_media_lists_with_state(
    state: &crate::host::state::AppState,
    page: u32,
    page_size: u32,
    sort_field: AddonMediaListAdminSortField,
    sort_direction: AddonMediaListAdminSortDirection,
) -> ersatzrs_app::Result<(Vec<AddonMediaListRow>, u64)> {
    list_addon_media_lists_by_visibility_with_state(
        state,
        page,
        page_size,
        sort_field,
        sort_direction,
        false,
    )
    .await
}

/// Browse visible or hidden lists without changing their publication state.
#[server]
pub async fn list_addon_media_lists_by_visibility(
    page: u32,
    page_size: u32,
    sort_field: AddonMediaListAdminSortField,
    sort_direction: AddonMediaListAdminSortDirection,
    hidden: bool,
) -> Result<(Vec<AddonMediaListRow>, u64), ServerFnError> {
    let state = expect_context::<crate::host::state::AppState>();
    list_addon_media_lists_by_visibility_with_state(
        &state,
        page,
        page_size,
        sort_field,
        sort_direction,
        hidden,
    )
    .await
    .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
async fn list_addon_media_lists_by_visibility_with_state(
    state: &crate::host::state::AppState,
    page: u32,
    page_size: u32,
    sort_field: AddonMediaListAdminSortField,
    sort_direction: AddonMediaListAdminSortDirection,
    hidden: bool,
) -> ersatzrs_app::Result<(Vec<AddonMediaListRow>, u64)> {
    let (rows, total) = state
        .application
        .list_addon_media_lists_by_visibility(
            ersatzrs_app::queries::ListAddonMediaLists {
                page,
                page_size,
                sort_field: sort_field.into(),
                sort_direction: sort_direction.into(),
            },
            hidden,
        )
        .await?;
    Ok((
        rows.into_iter()
            .map(|row| AddonMediaListRow {
                id: row.id,
                addon_id: row.addon_id,
                provider_id: row.provider_id,
                source_url: row.source_url,
                name: row.name,
                item_count: row.item_count,
                match_count: row.match_count,
                refresh_cadence: row.refresh_cadence,
                last_refresh_succeeded_at: row.last_refresh_succeeded_at,
                next_refresh_at: row.next_refresh_at,
                refresh_retry_at: row.refresh_retry_at,
                generate_playlist: row.generate_playlist,
                presentation_kind: row.presentation_kind,
                requested_presentation_kind: row.requested_presentation_kind,
                presentation_error: row.presentation_error,
                review_state: row.review_state,
                review_error: row.review_error,
                published_revision: row.published_revision,
                draft_revision: row.draft_revision,
                detail_import_policy: row.detail_import_policy,
                import_progress: row.import_progress.map(Into::into),
            })
            .collect(),
        total,
    ))
}

#[server]
pub async fn get_addon_media_lists_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_addon_media_lists_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_form(
    id: i64,
) -> Result<Option<AddonMediaListForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_addon_media_list(id)
        .await
        .map_err(ServerFnError::new)
        .map(|form| {
            form.map(|form| AddonMediaListForm {
                delivery_mode: form.delivery_mode,
                delivery_cutoff_at: form.delivery_cutoff_at,
                auto_publication_hold: form.auto_publication_hold.map(Into::into),
                id: form.id,
                name: form.name,
                addon_id: form.addon_id,
                source_url: form.source_url,
                folder_name: form.folder_name,
                refresh_cadence: form.refresh_cadence,
                refresh_publication_policy: form.refresh_publication_policy,
                last_refresh_succeeded_at: form.last_refresh_succeeded_at,
                next_refresh_at: form.next_refresh_at,
                refresh_retry_at: form.refresh_retry_at,
                generate_playlist: form.generate_playlist,
                content_kind_override: form.content_kind_override,
                presentation_kind: form.presentation_kind,
                requested_presentation_kind: form.requested_presentation_kind,
                presentation_error: form.presentation_error,
                review_state: form.review_state,
                review_error: form.review_error,
                published_revision: form.published_revision,
                draft_revision: form.draft_revision,
                legacy_chapter_input: form.legacy_chapter_input,
                detail_import_policy: form.detail_import_policy,
                import_progress: form.import_progress.map(Into::into),
            })
        })
}

#[server]
pub async fn get_addon_media_list_publication_readiness(
    id: i64,
) -> Result<AddonMediaListPublicationReadiness, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_addon_media_list_publication_readiness_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn get_addon_media_list_publication_readiness_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<AddonMediaListPublicationReadiness> {
    state
        .application
        .addon_media_list_publication_readiness(id)
        .await
        .map(Into::into)
}

#[server]
pub async fn get_addon_media_list_publication_history(
    id: i64,
) -> Result<AddonMediaListPublicationHistoryRow, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_addon_media_list_publication_history_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn get_addon_media_list_publication_history_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<AddonMediaListPublicationHistoryRow> {
    state
        .application
        .addon_media_list_publication_history(id)
        .await
        .map(Into::into)
}

#[server]
pub async fn get_addon_media_list_source_probe(
    id: i64,
) -> Result<Option<AddonMediaListSourceProbeRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .addon_media_list_source_probe(id)
        .await
        .map(|probe| probe.map(Into::into))
        .map_err(ServerFnError::new)
}

#[server]
pub async fn test_addon_media_list_source(
    id: i64,
    saved_revision: i32,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .test_addon_media_list_source(ersatzrs_app::commands::TestAddonMediaListSource {
            id,
            saved_revision,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_saved_draft_diff(
    id: i64,
) -> Result<AddonMediaListDraftDiffRow, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .addon_media_list_saved_draft_diff(id)
        .await
        .map(Into::into)
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_reset_impact(
    id: i64,
) -> Result<AddonMediaListResetImpactRow, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .addon_media_list_reset_impact(id)
        .await
        .map(Into::into)
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_revision_history(
    id: i64,
) -> Result<Vec<AddonMediaListRevisionSummaryRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .addon_media_list_revision_history(id)
        .await
        .map(|history| history.into_iter().map(Into::into).collect())
        .map_err(ServerFnError::new)
}

#[server]
pub async fn compare_addon_media_list_revisions(
    id: i64,
    left_revision: i32,
    right_revision: i32,
) -> Result<AddonMediaListRevisionDiffRow, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .compare_addon_media_list_revisions(id, left_revision, right_revision)
        .await
        .map(Into::into)
        .map_err(ServerFnError::new)
}

#[server]
pub async fn restore_addon_media_list_revision(
    id: i64,
    source_published_revision: i32,
    expected_draft_revision: i32,
    expected_published_revision: i32,
) -> Result<AddonMediaListRestoreResult, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .restore_addon_media_list_revision(ersatzrs_app::commands::RestoreAddonMediaListRevision {
            id,
            source_published_revision,
            expected_draft_revision,
            expected_published_revision,
        })
        .await
        .map(|outcome| AddonMediaListRestoreResult {
            operation_id: outcome.operation_id,
            generation: outcome.generation,
            saved_revision: outcome.saved_revision,
            target_published_revision: outcome.target_published_revision,
            existing: outcome.existing,
        })
        .map_err(ServerFnError::new)
}

/// Retry or restore one exact failed/expired publication generation.
#[server]
pub async fn recover_addon_media_list_publication(
    list_id: i64,
    job_id: i64,
    generation: i32,
    action: AddonMediaListRecoveryAction,
) -> Result<AddonMediaListPublicationStatus, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    recover_addon_media_list_publication_with_state(&state, list_id, job_id, generation, action)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn recover_addon_media_list_publication_with_state(
    state: &crate::host::state::AppState,
    list_id: i64,
    job_id: i64,
    generation: i32,
    action: AddonMediaListRecoveryAction,
) -> ersatzrs_app::Result<AddonMediaListPublicationStatus> {
    state
        .application
        .recover_addon_media_list_publication(
            ersatzrs_app::commands::RecoverAddonMediaListPublication {
                list_id,
                job_id,
                generation,
                action,
            },
        )
        .await
        .map(Into::into)
}

#[server]
pub async fn list_addon_media_list_items(
    id: i64,
) -> Result<Vec<AddonMediaListItemRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_addon_media_list_items(id)
        .await
        .map_err(ServerFnError::new)
        .map(|rows| {
            rows.into_iter()
                .map(|row| AddonMediaListItemRow {
                    id: row.id,
                    rank: row.rank,
                    kind: row.kind,
                    display_title: row.display_title,
                    availability: row.availability,
                    availability_reason: row.availability_reason,
                    availability_reason_code: row.availability_reason_code,
                    replacement_media_item_id: row.replacement_media_item_id,
                    replacement_media_version_id: row.replacement_media_version_id,
                    replacement_title: row.replacement_title,
                    replacement_path: row.replacement_path,
                    replacement_valid: row.replacement_valid,
                    title: row.title,
                    year: row.year,
                    season: row.season,
                    episode: row.episode,
                    first_aired: row.first_aired,
                    source_url: row.source_url,
                    duration_seconds: row.duration_seconds,
                    provider_liveness: row.provider_liveness,
                    liveness_override: row.liveness_override,
                    effective_liveness: row.effective_liveness,
                    liveness_conflict: row.liveness_conflict,
                    published_liveness: row.published_liveness,
                    parent_provider_id: row.parent_provider_id,
                    chapter_input: row.chapter_input,
                    provider_chapter_input: row.provider_chapter_input,
                    projected_episode_id: row.projected_episode_id,
                    remote_stream_id: row.remote_stream_id,
                    content_kind: row.content_kind,
                    collection_role: row.collection_role,
                    thumbnail_url: row.thumbnail_url,
                    tags: row.tags,
                    people: row.people,
                    included: row.included,
                    published: row.published,
                    metadata_conflict_count: row.metadata_conflict_count,
                    detail_state: row.detail_state,
                    detail_diagnostic: row.detail_diagnostic,
                })
                .collect()
        })
}

/// Load one page of entries in one availability group.
///
/// A `page_size` of zero means "every entry in this group", which the editor
/// offers explicitly.
#[server]
pub async fn list_addon_media_list_items_page(
    id: i64,
    availability: AddonMediaListItemAvailability,
    page: u32,
    page_size: u32,
) -> Result<Vec<AddonMediaListItemRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_addon_media_list_items_page(
            id,
            availability,
            page,
            (page_size > 0).then_some(page_size),
        )
        .await
        .map_err(ServerFnError::new)
        .map(|rows| rows.into_iter().map(Into::into).collect())
}

/// Count the entries of one list per availability group.
#[server]
pub async fn get_addon_media_list_item_counts(
    id: i64,
) -> Result<AddonMediaListItemCountsRow, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .addon_media_list_item_counts(id)
        .await
        .map_err(ServerFnError::new)
        .map(|counts| AddonMediaListItemCountsRow {
            available: counts.available,
            unavailable: counts.unavailable,
            unknown: counts.unknown,
        })
}

/// Set the descriptive classification of one entry.
#[server]
pub async fn set_addon_media_list_item_content_kind(
    item_id: i64,
    content_kind: AddonMediaListContentKind,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_addon_media_list_item_content_kind(
            ersatzrs_app::commands::SetAddonMediaListItemContentKind {
                item_id,
                content_kind,
            },
        )
        .await
        .map_err(ServerFnError::new)
}

/// Set or restore the playback behavior of one Remote Stream entry.
#[server]
pub async fn set_addon_media_list_item_liveness(
    list_id: i64,
    item_id: i64,
    liveness: Option<RemoteStreamLiveness>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_addon_media_list_item_liveness(ersatzrs_app::commands::SetAddonMediaListItemLiveness {
            list_id,
            item_id,
            liveness,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Apply or restore playback behavior for selected or all Remote Stream entries.
#[server]
pub async fn apply_addon_media_list_liveness(
    id: i64,
    liveness: Option<RemoteStreamLiveness>,
    item_ids: Option<Vec<i64>>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .apply_addon_media_list_liveness(ersatzrs_app::commands::ApplyAddonMediaListLiveness {
            id,
            liveness,
            item_ids: item_ids.unwrap_or_default(),
        })
        .await
        .map_err(ServerFnError::new)
}

/// Include or exclude one entry from the next publication.
#[server]
pub async fn set_addon_media_list_item_included(
    list_id: i64,
    item_id: i64,
    included: bool,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_addon_media_list_item_included(ersatzrs_app::commands::SetAddonMediaListItemIncluded {
            list_id,
            item_id,
            included,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Set the collection-aware role of one source for the next publication.
#[server]
pub async fn set_addon_media_list_item_role(
    list_id: i64,
    item_id: i64,
    role: AddonMediaListItemRole,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .set_addon_media_list_item_role(ersatzrs_app::commands::SetAddonMediaListItemRole {
            list_id,
            item_id,
            role,
        })
        .await
        .map_err(ServerFnError::new)
}

/// Apply one classification to every entry of a list as a bulk default.
#[server]
pub async fn apply_addon_media_list_content_kind(
    id: i64,
    content_kind: AddonMediaListContentKind,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .apply_addon_media_list_content_kind(
            ersatzrs_app::commands::ApplyAddonMediaListContentKind { id, content_kind },
        )
        .await
        .map_err(ServerFnError::new)
}

/// Atomically apply quick-review values or a prior undo snapshot.
#[server]
pub async fn apply_addon_media_list_item_review_edits(
    id: i64,
    edits: Vec<AddonMediaListItemReviewEdit>,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .apply_addon_media_list_item_review_edits(
            ersatzrs_app::commands::ApplyAddonMediaListItemReviewEdits {
                id,
                edits: edits.into_iter().map(Into::into).collect(),
            },
        )
        .await
        .map_err(ServerFnError::new)
}

/// Assign one season and consecutive episode numbers in original provider order.
#[server]
pub async fn assign_addon_media_list_episode_numbers(
    id: i64,
    season: i32,
    starting_episode: i32,
    item_count: Option<u32>,
    missing_only: bool,
) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .assign_addon_media_list_episode_numbers(
            ersatzrs_app::commands::AssignAddonMediaListEpisodeNumbers {
                id,
                season,
                starting_episode,
                item_count,
                missing_only,
            },
        )
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn search_addon_media_list_replacements(
    query: String,
) -> Result<Vec<AddonMediaListReplacementRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .search_addon_media_list_replacements(&query)
        .await
        .map_err(ServerFnError::new)
        .map(|rows| {
            rows.into_iter()
                .map(|row| AddonMediaListReplacementRow {
                    media_item_id: row.media_item_id,
                    media_version_id: row.media_version_id,
                    title: row.title,
                    path: row.path,
                })
                .collect()
        })
}

#[server]
pub async fn set_addon_media_list_item_replacement(
    item_id: i64,
    media_item_id: Option<i64>,
    media_version_id: Option<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .set_addon_media_list_item_replacement(
            ersatzrs_app::commands::SetAddonMediaListItemReplacement {
                item_id,
                media_item_id,
                media_version_id,
            },
        )
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_item(
    list_id: i64,
    item_id: i64,
) -> Result<Option<AddonMediaListItemDetail>, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .get_addon_media_list_item(list_id, item_id)
        .await
        .map(|detail| detail.map(Into::into))
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_addon_media_list_show_nfo(
    id: i64,
) -> Result<AddonMediaListNfoForm, ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .get_addon_media_list_show_nfo(id)
        .await
        .map(Into::into)
        .map_err(ServerFnError::new)
}

#[server]
pub async fn request_addon_media_list_presentation(
    id: i64,
    presentation_kind: AddonMediaListPresentationKind,
    show_title: Option<String>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .request_addon_media_list_presentation(
            ersatzrs_app::commands::RequestAddonMediaListPresentation {
                id,
                presentation_kind,
                show_title,
            },
        )
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn save_addon_media_list_show_nfo(
    id: i64,
    metadata: AddonMediaListNfoMetadata,
    etag: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .save_addon_media_list_show_nfo(ersatzrs_app::commands::SaveAddonMediaListNfo {
            id,
            list_id: None,
            metadata: metadata.into(),
            etag,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn save_addon_media_list_item_nfo(
    list_id: i64,
    id: i64,
    metadata: AddonMediaListNfoMetadata,
    etag: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .save_addon_media_list_item_nfo(ersatzrs_app::commands::SaveAddonMediaListNfo {
            id,
            list_id: Some(list_id),
            metadata: metadata.into(),
            etag,
        })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn reset_addon_media_list_nfo(
    list_id: i64,
    item_id: Option<i64>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .reset_addon_media_list_nfo(list_id, item_id)
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn update_addon_media_list_item_chapters(
    id: i64,
    chapter_input: String,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .update_addon_media_list_item_chapters(
            ersatzrs_app::commands::UpdateAddonMediaListItemChapters { id, chapter_input },
        )
        .await
        .map_err(ServerFnError::new)
}

/// Validate chapter text before a combined metadata/chapter save begins.
///
/// The application command validates again at the write boundary. This
/// read-only preflight prevents a valid NFO update from committing before an
/// invalid chapter edit is rejected by the second command.
#[server]
#[allow(clippy::unused_async)] // Leptos server-function transport requires an async signature.
pub async fn validate_addon_media_list_item_chapters(
    chapter_input: String,
) -> Result<(), ServerFnError> {
    ersatzrs_core::media_chapters::parse_chapter_input(&chapter_input, None)
        .map(|_| ())
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server]
#[allow(clippy::too_many_arguments)] // Mirrors the native add form's persisted settings.
pub async fn add_addon_media_list(
    addon_id: String,
    source_url: String,
    folder_name: String,
    content_kind_override: AddonMediaListContentKind,
    chapter_input: String,
    load_item_details: bool,
    refresh_cadence: AddonMediaListRefreshCadence,
    delivery_mode: AddonMediaListDeliveryMode,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let id = add_addon_media_list_with_state(
        &state,
        addon_id,
        source_url,
        folder_name,
        content_kind_override,
        chapter_input,
        if load_item_details {
            AddonMediaListDetailPolicy::Automatic
        } else {
            AddonMediaListDetailPolicy::ManualOnly
        },
        refresh_cadence,
        delivery_mode,
    )
    .await
    .map_err(ServerFnError::new)?;
    let mut form = state
        .application
        .get_addon_media_list(id)
        .await
        .map_err(ServerFnError::new)?
        .ok_or_else(|| ServerFnError::new("imported add-on list was not found"))?;
    form.refresh_cadence = refresh_cadence;
    state
        .application
        .update_addon_media_list(ersatzrs_app::commands::UpdateAddonMediaList {
            id,
            delivery_mode: form.delivery_mode,
            refresh_cadence: form.refresh_cadence,
            refresh_publication_policy: form.refresh_publication_policy,
            generate_playlist: form.generate_playlist,
            content_kind_override: form.content_kind_override,
            chapter_input: form.legacy_chapter_input,
            detail_import_policy: form.detail_import_policy,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok(id)
}

#[server]
pub async fn match_addon_media_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    match_addon_media_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

/// Publish the reviewed metadata snapshot, then scan its managed library.
#[server]
pub async fn publish_addon_media_list(
    id: i64,
    saved_revision: i32,
) -> Result<AddonMediaListPublishResult, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    publish_addon_media_list_with_state(&state, id, saved_revision)
        .await
        .map_err(ServerFnError::new)
}

/// Remove all derived state for one list and queue a full import while
/// retaining the list's operator-owned identity and settings.
#[server]
pub async fn reset_addon_media_list(
    id: i64,
    confirmed_name: String,
) -> Result<AddonMediaListResetResult, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    reset_addon_media_list_with_state(&state, id, confirmed_name)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn reset_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
    confirmed_name: String,
) -> ersatzrs_app::Result<AddonMediaListResetResult> {
    state
        .application
        .reset_addon_media_list(ersatzrs_app::commands::ResetAddonMediaList { id, confirmed_name })
        .await
        .map(|outcome| match outcome {
            ersatzrs_app::commands::ResetAddonMediaListOutcome::Queued {
                operation_id,
                generation,
                existing,
            } => AddonMediaListResetResult::Queued {
                operation_id,
                generation,
                existing,
            },
        })
}

/// Enumerate the complete source into a review draft without changing the
/// current publication or operator-owned item state.
#[server]
pub async fn reconcile_addon_media_list(
    id: i64,
) -> Result<AddonMediaListReconcileResult, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    reconcile_addon_media_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn reconcile_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<AddonMediaListReconcileResult> {
    state
        .application
        .reconcile_addon_media_list(ersatzrs_app::commands::ReconcileAddonMediaList { id })
        .await
        .map(|outcome| match outcome {
            ersatzrs_app::commands::ReconcileAddonMediaListOutcome::Queued {
                operation_id,
                generation,
                existing,
            } => AddonMediaListReconcileResult::Queued {
                operation_id,
                generation,
                existing,
            },
        })
}

#[server]
pub async fn confirm_addon_media_list_removal(
    list_id: i64,
    item_id: i64,
    expected_generation: i32,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .confirm_addon_media_list_removal(ersatzrs_app::commands::ConfirmAddonMediaListRemoval {
            list_id,
            item_id,
            expected_generation,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn publish_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
    saved_revision: i32,
) -> ersatzrs_app::Result<AddonMediaListPublishResult> {
    let outcome = state
        .application
        .publish_addon_media_list(ersatzrs_app::commands::PublishAddonMediaList {
            id,
            saved_revision,
        })
        .await?;
    match outcome {
        ersatzrs_app::commands::PublishAddonMediaListOutcome::PreparingMedia {
            pending_items,
            ..
        } => Ok(AddonMediaListPublishResult::PreparingMedia {
            item_count: pending_items,
        }),
        ersatzrs_app::commands::PublishAddonMediaListOutcome::Preparing {
            pending_details, ..
        } => Ok(AddonMediaListPublishResult::PreparingPlaybackDetails {
            item_count: pending_details,
        }),
        ersatzrs_app::commands::PublishAddonMediaListOutcome::Queued {
            operation_id,
            generation,
            existing,
            ..
        } => Ok(AddonMediaListPublishResult::Queued {
            operation_id,
            generation,
            existing,
        }),
    }
}

#[server]
pub async fn update_addon_media_list(form: AddonMediaListForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    update_addon_media_list_with_state(&state, form)
        .await
        .map_err(ServerFnError::new)
}

/// Queue one top-level item for detail enrichment with manual priority.
#[server]
pub async fn request_addon_media_list_item_details(
    list_id: i64,
    item_id: i64,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .request_addon_media_list_item_details(
            ersatzrs_app::commands::RequestAddonMediaListItemDetails { list_id, item_id },
        )
        .await
        .map_err(ServerFnError::new)
}

/// Pause, resume, retry, or cancel the current overview generation.
#[server]
pub async fn control_addon_media_list_import(
    id: i64,
    action: AddonMediaListImportAction,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    expect_context::<AppState>()
        .application
        .control_addon_media_list_import(ersatzrs_app::commands::ControlAddonMediaListImport {
            id,
            action: action.into(),
        })
        .await
        .map_err(ServerFnError::new)
}

/// Move one saved list to a different add-on and re-import it through it.
#[server]
pub async fn change_addon_media_list_addon(id: i64, addon_id: String) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let logged_addon_id = addon_id.clone();
    state
        .application
        .change_addon_media_list_addon(ersatzrs_app::commands::ChangeAddonMediaListAddon {
            id,
            addon_id,
        })
        .await
        .inspect_err(|error| {
            log_addon_media_list_failure("change-addon", Some(&logged_addon_id), Some(id), error);
        })
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_addon_media_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    delete_addon_media_list_with_state(&state, id)
        .await
        .map_err(ServerFnError::new)
}

/// Change admin visibility only; playback and background work remain active.
#[server]
pub async fn set_addon_media_list_admin_hidden(id: i64, hidden: bool) -> Result<(), ServerFnError> {
    expect_context::<crate::host::state::AppState>()
        .application
        .set_addon_media_list_admin_hidden(id, hidden)
        .await
        .map_err(ServerFnError::new)
}

/// Record a failed add-on media list operation with its provider diagnostic.
///
/// The admin UI surfaces the same message to the operator, and add-on
/// diagnostics reaching this layer are already bounded and redacted by the
/// contract boundary, so no URL, secret, or local path is emitted here beyond
/// what the operator already sees.
#[cfg(feature = "ssr")]
fn log_addon_media_list_failure(
    operation: &'static str,
    addon_id: Option<&str>,
    list_id: Option<i64>,
    error: &ersatzrs_app::ApplicationError,
) {
    tracing::warn!(
        operation,
        addon_id = addon_id.unwrap_or("n/a"),
        list_id = list_id.unwrap_or_default(),
        failure_kind = "addon-media-list",
        operation_message = %error,
        "add-on media list operation failed"
    );
}

/// Every contract version that can enumerate an add-on media list.
#[cfg(feature = "ssr")]
const MEDIA_LIST_CAPABILITIES: [&str; 6] = [
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V1,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V2,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V3,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V4,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V5,
    ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V6,
];

/// Select the add-on for one operator source URL when none was chosen.
///
/// Both the interactive dialog and the plain HTML form reach this seam, so the
/// stored add-on is identical whichever path the operator used.
#[cfg(feature = "ssr")]
async fn resolve_media_list_addon(
    state: &crate::host::state::AppState,
    source_url: &str,
) -> ersatzrs_app::Result<String> {
    use ersatzrs_app::SourceAddonRouting;

    let routing = state
        .application
        .resolve_source_addon(
            source_url.to_owned(),
            MEDIA_LIST_CAPABILITIES
                .iter()
                .map(|capability| (*capability).to_owned())
                .collect(),
        )
        .await
        .map_err(|error| ersatzrs_app::ApplicationError::Validation {
            field: "addon_id",
            message: error.to_string(),
        })?;
    match routing {
        SourceAddonRouting::Declared(addon_id) | SourceAddonRouting::Default(addon_id) => {
            Ok(addon_id)
        }
        SourceAddonRouting::None => Err(ersatzrs_app::ApplicationError::Validation {
            field: "addon_id",
            message: "No installed add-on handles this URL".to_owned(),
        }),
        SourceAddonRouting::Ambiguous(addon_ids) => {
            Err(ersatzrs_app::ApplicationError::Validation {
                field: "addon_id",
                message: format!(
                    "Several add-ons handle this URL; choose one: {}",
                    addon_ids.join(", ")
                ),
            })
        }
    }
}

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_arguments)] // Mirrors the add form's provider and persisted settings.
pub async fn add_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    addon_id: String,
    source_url: String,
    folder_name: String,
    content_kind_override: AddonMediaListContentKind,
    chapter_input: String,
    detail_import_policy: AddonMediaListDetailPolicy,
    refresh_cadence: AddonMediaListRefreshCadence,
    delivery_mode: AddonMediaListDeliveryMode,
) -> ersatzrs_app::Result<i64> {
    // An omitted choice is resolved from the URL rather than rejected, so the
    // operator only has to name an add-on when nothing claims the host.
    let addon_id = if addon_id.trim().is_empty() {
        resolve_media_list_addon(state, source_url.trim()).await?
    } else {
        addon_id
    };
    if refresh_cadence == AddonMediaListRefreshCadence::Adaptive {
        let supports_adaptive = state
            .application
            .addon_overview()
            .await
            .map_err(|error| ersatzrs_app::ApplicationError::Unavailable {
                operation: "validate adaptive add-on media-list refresh",
                message: error.to_string(),
            })?
            .installed
            .into_iter()
            .any(|item| {
                item.enabled
                    && item.addon_id == addon_id
                    && item.manifest.has_media_list_source_check_capability()
            });
        if !supports_adaptive {
            return Err(ersatzrs_app::ApplicationError::Validation {
                field: "refresh_cadence",
                message:
                    "Adaptive refresh requires an enabled add-on with media-list.source-check.v1"
                        .to_owned(),
            });
        }
    }
    let logged_addon_id = addon_id.clone();
    let list_id = state
        .application
        .add_addon_media_list(ersatzrs_app::commands::AddAddonMediaList {
            delivery_mode,
            addon_id,
            source_url,
            folder_name,
            content_kind_override,
            chapter_input,
            detail_import_policy,
        })
        .await
        .inspect_err(|error| {
            log_addon_media_list_failure("add", Some(&logged_addon_id), None, error);
        })?;
    let published = state
        .application
        .get_addon_media_list(list_id)
        .await?
        .is_some_and(|form| form.review_state == AddonMediaListReviewState::Published);
    if published {
        if let Some(library_id) = state
            .application
            .addon_media_list_library_id(list_id)
            .await?
        {
            let outcome = state
                .application
                .request_library_scan(ersatzrs_app::commands::RequestLibraryScan::new(
                    library_id, false,
                ))
                .await;
            tracing::info!(
                list_id,
                library_id,
                ?outcome,
                "queued managed add-on list library scan"
            );
        }
    }
    Ok(list_id)
}

#[cfg(feature = "ssr")]
pub async fn match_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .match_addon_media_list(ersatzrs_app::commands::MatchAddonMediaList { id })
        .await
        .inspect_err(|error| log_addon_media_list_failure("match", None, Some(id), error))
}

#[cfg(feature = "ssr")]
pub async fn update_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    form: AddonMediaListForm,
) -> ersatzrs_app::Result<()> {
    let id = form.id;
    state
        .application
        .update_addon_media_list(ersatzrs_app::commands::UpdateAddonMediaList {
            id: form.id,
            delivery_mode: form.delivery_mode,
            refresh_cadence: form.refresh_cadence,
            refresh_publication_policy: form.refresh_publication_policy,
            generate_playlist: form.generate_playlist,
            content_kind_override: form.content_kind_override,
            chapter_input: form.legacy_chapter_input,
            detail_import_policy: form.detail_import_policy,
        })
        .await
        .inspect_err(|error| log_addon_media_list_failure("update", None, Some(id), error))?;
    // Save is the review boundary. Publication stages the durable imported
    // snapshot and performs the one required library scan, so settings edits
    // must not queue an eager scan (or indirectly enumerate the add-on) here.
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn delete_addon_media_list_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> ersatzrs_app::Result<()> {
    state
        .application
        .set_addon_media_list_admin_hidden(id, true)
        .await
        .inspect_err(|error| log_addon_media_list_failure("hide", None, Some(id), error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_publication_diagnostics_have_only_the_typed_visible_shape() {
        let history = AddonMediaListPublicationHistoryRow {
            list_id: 42,
            jobs: vec![AddonMediaListPublicationHistoryJobRow {
                job_id: 7,
                operation_id: 9,
                generation: 3,
                saved_revision: 4,
                target_published_revision: 5,
                phase: AddonMediaListPublicationPhase::RestoringFiles,
                state: AddonMediaListPublicationState::Restoring,
                outcome: None,
                events: vec![AddonMediaListPublicationEventRow {
                    sequence: 1,
                    kind: AddonMediaListPublicationEventKind::RecoveryFailure,
                    enqueue_trigger: None,
                    phase: Some(AddonMediaListPublicationPhase::RestoringFiles),
                    attempt: Some(2),
                    related_operation_id: Some(11),
                    filesystem_transition: None,
                    retry_class: Some(AddonMediaListPublicationRetryClass::TransientRecovery),
                    recovery_action: Some(AddonMediaListRecoveryAction::Retry),
                    error_category: Some(AddonMediaListPublicationErrorCategory::SharingViolation),
                    os_error_code: Some(32),
                    published_item_count: Some(165),
                    playlist_item_count: Some(165),
                    artwork_count: Some(3),
                    outcome: None,
                    occurred_at: "2026-09-04T12:00:00Z".to_owned(),
                }],
            }],
        };

        let diagnostics = redacted_publication_diagnostics_json(&history).unwrap();
        let value: serde_json::Value = serde_json::from_str(&diagnostics).unwrap();
        assert_eq!(value["list_id"], 42);
        assert_eq!(value["jobs"][0]["operation_id"], 9);
        assert_eq!(
            value["jobs"][0]["events"][0]["error_category"],
            "sharing_violation"
        );
        assert_eq!(value["jobs"][0]["events"][0]["os_error_code"], 32);
        for forbidden in [
            "source_url",
            "path",
            "title",
            "cookie",
            "header",
            "stderr",
            "diagnostic",
            "message",
        ] {
            assert!(!diagnostics.contains(forbidden), "leaked key {forbidden}");
        }
    }
}
