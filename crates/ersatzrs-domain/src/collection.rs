use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{EntityId, PlaybackOrder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CollectionType {
    #[default]
    Collection,
    TelevisionShow,
    TelevisionSeason,
    Artist,
    MultiCollection,
    SmartCollection,
    Playlist,
    RerunFirstRun,
    RerunRerun,
    SearchQuery,
    Movie,
    Episode,
    MusicVideo,
    OtherVideo,
    Song,
    Image,
    RemoteStream,
    FakeCollection,
    FakePlaylistItem,
}

/// Media kinds accepted from a provider-neutral add-on list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListItemKind {
    #[default]
    Movie,
    Show,
    Season,
    Episode,
    RemoteStream,
}

/// Provider-reported availability for an add-on list item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListItemAvailability {
    /// The provider confirms that the item can currently be played.
    #[default]
    Available,
    /// The provider confirms that the item cannot currently be played.
    Unavailable,
    /// The provider could not determine availability without a costly probe.
    Unknown,
}

/// Provider-neutral terminal reason for an unavailable add-on list item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListAvailabilityReason {
    AuthenticationRequired,
    ContentRestricted,
    Unavailable,
}

/// Provider-neutral playback behavior for a Remote Stream.
///
/// This is deliberately three-state: a missing provider decision must remain
/// distinguishable from a source that is confirmed finite. The value is
/// independent from the Remote Stream collection type, which contains both
/// finite and live media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RemoteStreamLiveness {
    /// No trustworthy provider or operator decision exists.
    #[default]
    Unknown,
    /// The source is finite/on-demand media.
    Finite,
    /// The source is live or is intended to become live.
    Live,
}

impl RemoteStreamLiveness {
    /// Adapt an optional legacy boolean and duration without guessing that an
    /// omitted durationless value is finite.
    #[must_use]
    pub const fn from_legacy(is_live: Option<bool>, duration_seconds: Option<u64>) -> Self {
        match is_live {
            Some(true) => Self::Live,
            Some(false) => Self::Finite,
            None if matches!(duration_seconds, Some(value) if value > 0) => Self::Finite,
            None => Self::Unknown,
        }
    }

    /// Compatibility value for the legacy `RemoteStream.IsLive` mirror.
    #[must_use]
    pub const fn legacy_is_live(self) -> bool {
        matches!(self, Self::Live)
    }
}

/// Lifecycle of provider metadata before it becomes playable library content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListReviewState {
    /// Provider data is editable and has not been published.
    #[default]
    Draft,
    /// Review is complete, but publication has not yet committed.
    Ready,
    /// The current included snapshot is available to scanners and playout.
    Published,
    /// One reviewed revision is staged for file generation and projection.
    ///
    /// This persisted state makes an interrupted cross-file/database
    /// publication visible to recovery and health checks instead of exposing
    /// it as a completed revision.
    Publishing,
}

/// Durable operation type sharing one managed-library exclusion boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedLibraryOperationKind {
    ManualScan,
    ScheduledScan,
    StartupReplayScan,
    Publication,
    PublicationChildScan,
    ProviderRefresh,
    Recovery,
    Reset,
    Reconcile,
}

/// Durable lifecycle shared by managed-library scans and publication work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ManagedLibraryOperationState {
    #[default]
    Queued,
    Running,
    WaitingToRetry,
    Completed,
    Failed,
}

/// Restart checkpoint for one exact add-on-list publication generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationPhase {
    #[default]
    AwaitingLease,
    Staging,
    Scanning,
    Committing,
    RestoringFiles,
    RestoringProjection,
    Reconciling,
    /// Re-scan and validate the chosen previous or candidate revision.
    Verifying,
    /// Remove journal-owned scratch trees before clearing the journal.
    CleaningUp,
    Complete,
}

/// Safe terminal result of the publication state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationOutcome {
    Published,
    RestoredAfterFailure,
}

/// Operator-facing state of one durable add-on-list publication generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationState {
    Queued,
    Waiting,
    Publishing,
    Restoring,
    Expired,
    Failed,
    Complete,
}

/// Explicit recovery transition requested by an operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListRecoveryAction {
    Retry,
    RestorePreviousRevision,
}

/// Typed causal boundary retained for one add-on-list publication job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationEventKind {
    ExistingJobAdopted,
    Enqueued,
    LeaseWaitStarted,
    LeaseAcquired,
    LeaseReclaimed,
    PhaseAdvanced,
    ChildScanStarted,
    ChildScanCompleted,
    ChildScanFailed,
    OriginalFailure,
    RecoveryFailure,
    RetryScheduled,
    RecoveryRequested,
    Completed,
    HistoryTruncated,
}

/// Source that admitted a publication into the durable queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationEnqueueTrigger {
    Operator,
    RestoreRevision,
    AutomaticRefresh,
    SavedFolder,
}

/// Filesystem boundary represented by a publication phase event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationFilesystemTransition {
    PreviousBackedUp,
    CandidateCommitted,
    PreviousRestored,
    ProjectionRestored,
    RevisionVerified,
    CleanupCompleted,
}

/// Bounded retry policy classification exposed without an error message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationRetryClass {
    TransientRecovery,
}

/// Sanitized publication failure category safe for operator diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPublicationErrorCategory {
    FilesystemIo,
    PermissionDenied,
    SharingViolation,
    UnsafeFilesystemState,
    InvalidPublicationMarker,
    PublicationIntegrity,
}

/// Host policy for optional item-detail enrichment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListDetailPolicy {
    /// Queue missing details only after the list is explicitly published.
    #[default]
    Automatic,
    /// Retain the overview until an operator requests individual items.
    ManualOnly,
}

/// Durable phase of overview-first list discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListImportState {
    #[default]
    Queued,
    Discovering,
    Verifying,
    Ready,
    Paused,
    NeedsAttention,
    Cancelled,
    Superseded,
}

/// Durable per-item state for deferred detail enrichment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListDetailState {
    #[default]
    NotRequested,
    Queued,
    WaitingForSlot,
    Running,
    WaitingToRetry,
    Complete,
    Skipped,
    Failed,
}

/// Descriptive content classification for an add-on supplied stream.
///
/// This does not change the storage identity: generated streams remain Remote
/// Streams. It controls metadata projection and compatible NFO roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListContentKind {
    /// Let the provider classify each item, retaining `Auto` when uncertain.
    #[default]
    Auto,
    /// One television episode; season and episode numbers remain optional.
    TelevisionEpisode,
    Movie,
    MusicVideo,
    /// Includes commercials, archive fragments, and uncategorized videos.
    OtherVideo,
    /// One music track whose playable source is audio or a performance video.
    Song,
    /// One still image; it has no duration of its own.
    Image,
}

/// Semantic role of one source inside the selected collection presentation.
///
/// This is deliberately independent from provider rank. A trailer can precede
/// the feature in the original order, so movie publication must never infer
/// its main feature from the first row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListItemRole {
    /// No role was supplied or selected yet.
    #[default]
    Auto,
    /// The primary source, such as the feature in a movie collection.
    Primary,
    /// A secondary source, such as a trailer, interview, or featurette.
    Extra,
}

/// Host-owned library presentation for an imported add-on list.
///
/// This is intentionally separate from [`AddonMediaListContentKind`]: that type
/// classifies one entry, while this one selects the library hierarchy the whole
/// list is projected into. An episode-shaped NFO may still belong to a Remote
/// Streams library, while a television-show presentation projects the same
/// provider playback into the normal Show/Season/Episode hierarchy.
///
/// The variants mirror [`crate::LibraryMediaKind`] so an imported list can live
/// in the same library kinds a scanned local library can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListPresentationKind {
    /// Derive the presentation from the classification of the items themselves.
    #[default]
    Auto,
    /// Retain provider Remote Streams without a second media identity.
    RemoteStreams,
    /// One managed Show with Seasons and Episodes.
    TelevisionShow,
    Movies,
    MusicVideos,
    OtherVideos,
    Songs,
    Images,
}

impl AddonMediaListPresentationKind {
    /// The presentation implied by one item classification, if unambiguous.
    ///
    /// `Auto` items carry no signal, so a list of only `Auto` items has no
    /// implied presentation and retains its provider Remote Streams.
    #[must_use]
    pub const fn from_content_kind(kind: AddonMediaListContentKind) -> Option<Self> {
        match kind {
            AddonMediaListContentKind::Auto => None,
            AddonMediaListContentKind::TelevisionEpisode => Some(Self::TelevisionShow),
            AddonMediaListContentKind::Movie => Some(Self::Movies),
            AddonMediaListContentKind::MusicVideo => Some(Self::MusicVideos),
            AddonMediaListContentKind::OtherVideo => Some(Self::OtherVideos),
            AddonMediaListContentKind::Song => Some(Self::Songs),
            AddonMediaListContentKind::Image => Some(Self::Images),
        }
    }

    /// Whether a projection exists for this presentation.
    ///
    /// Every presentation is projected today. The check is retained because it
    /// is the one place a newly added kind must be listed before it can be
    /// requested, rather than being stored as a presentation nothing implements.
    #[must_use]
    pub const fn is_projected(self) -> bool {
        matches!(
            self,
            Self::Auto
                | Self::RemoteStreams
                | Self::TelevisionShow
                | Self::Movies
                | Self::MusicVideos
                | Self::OtherVideos
                | Self::Songs
                | Self::Images
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Collection {
    pub id: EntityId,
    pub name: String,
    pub collection_type: CollectionType,
    pub media_item_ids: Vec<EntityId>,
    pub item_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexCollection {
    pub collection: Collection,
    pub key: String,
    pub plex_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JellyfinCollection {
    pub collection: Collection,
    pub item_id: String,
    pub jellyfin_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmbyCollection {
    pub collection: Collection,
    pub item_id: String,
    pub emby_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CollectionItem {
    pub id: EntityId,
    pub collection_id: EntityId,
    pub media_item_id: EntityId,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MultiCollection {
    pub id: EntityId,
    pub name: String,
    pub item_ids: Vec<EntityId>,
    pub smart_item_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MultiCollectionItem {
    pub id: EntityId,
    pub multi_collection_id: EntityId,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub playback_order: PlaybackOrder,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MultiCollectionSmartItem {
    pub id: EntityId,
    pub multi_collection_id: EntityId,
    pub smart_collection_id: EntityId,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SmartCollection {
    pub id: EntityId,
    pub name: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RerunCollection {
    pub id: EntityId,
    pub name: String,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Playlist {
    pub id: EntityId,
    pub name: String,
    pub item_ids: Vec<EntityId>,
    pub group_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlaylistGroup {
    pub id: EntityId,
    pub playlist_id: EntityId,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlaylistItem {
    pub id: EntityId,
    pub playlist_id: EntityId,
    pub playlist_group_id: Option<EntityId>,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playback_order: PlaybackOrder,
    pub sort_order: i32,
    pub pinned_stream_selection: Option<crate::PinnedStreamSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlexTag {
    pub id: EntityId,
    pub name: String,
    pub key: String,
    pub plex_media_source_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AddonMediaList {
    pub id: EntityId,
    pub addon_id: String,
    pub source_url: String,
    pub provider_id: String,
    pub name: String,
    pub description: Option<String>,
    pub item_count: i32,
    pub refresh_cadence: crate::AddonMediaListRefreshCadence,
    pub generate_playlist: bool,
    pub content_kind_override: AddonMediaListContentKind,
    pub presentation_kind: AddonMediaListPresentationKind,
    pub requested_presentation_kind: AddonMediaListPresentationKind,
    pub presentation_error: Option<String>,
    pub chapter_input: String,
    pub playlist_id: Option<EntityId>,
    pub last_update: Option<DateTime<Utc>>,
    pub last_match: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AddonMediaListItem {
    pub id: EntityId,
    pub addon_media_list_id: EntityId,
    pub provider_id: String,
    pub rank: i32,
    pub display_title: String,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub kind: AddonMediaListItemKind,
    pub content_kind: AddonMediaListContentKind,
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason: Option<String>,
    pub availability_reason_code: Option<AddonMediaListAvailabilityReason>,
    pub media_item_id: Option<EntityId>,
    pub duration_seconds: Option<u64>,
    pub parent_provider_id: Option<String>,
    pub fragment_start_seconds: Option<u64>,
    pub fragment_end_seconds: Option<u64>,
    pub additional_image_urls: Vec<String>,
    pub chapter_input: String,
}

/// One user-authored chapter linked to its full and fragment add-on items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AddonMediaListChapter {
    pub id: EntityId,
    pub addon_media_list_id: EntityId,
    pub source_item_id: EntityId,
    pub fragment_item_id: EntityId,
    pub chapter_index: i32,
    pub title: String,
    pub start_seconds: u64,
    pub end_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AddonMediaListItemGuid {
    pub id: EntityId,
    pub addon_media_list_item_id: EntityId,
    pub guid: String,
}

#[cfg(test)]
mod tests {
    use super::{AddonMediaListPresentationKind, RemoteStreamLiveness};

    #[test]
    fn remote_stream_liveness_adapts_legacy_values_conservatively() {
        assert_eq!(
            RemoteStreamLiveness::from_legacy(Some(true), None),
            RemoteStreamLiveness::Live
        );
        assert_eq!(
            RemoteStreamLiveness::from_legacy(Some(false), None),
            RemoteStreamLiveness::Finite
        );
        assert_eq!(
            RemoteStreamLiveness::from_legacy(None, Some(60)),
            RemoteStreamLiveness::Finite
        );
        assert_eq!(
            RemoteStreamLiveness::from_legacy(None, Some(0)),
            RemoteStreamLiveness::Unknown
        );
        assert_eq!(
            RemoteStreamLiveness::from_legacy(None, None),
            RemoteStreamLiveness::Unknown
        );
        assert!(RemoteStreamLiveness::Live.legacy_is_live());
        assert!(!RemoteStreamLiveness::Finite.legacy_is_live());
        assert!(!RemoteStreamLiveness::Unknown.legacy_is_live());
    }

    /// `is_projected` is the one gate a presentation must pass before it can be
    /// requested. A variant added without listing it here would be refused at
    /// runtime with no explanation; a variant added *and* listed here without a
    /// projection would be stored as a presentation nothing implements. This
    /// test fails on the first, and the exhaustive dispatcher on the second.
    #[test]
    fn every_presentation_declares_whether_it_is_projected() {
        for kind in [
            AddonMediaListPresentationKind::Auto,
            AddonMediaListPresentationKind::RemoteStreams,
            AddonMediaListPresentationKind::TelevisionShow,
            AddonMediaListPresentationKind::Movies,
            AddonMediaListPresentationKind::MusicVideos,
            AddonMediaListPresentationKind::OtherVideos,
            AddonMediaListPresentationKind::Songs,
            AddonMediaListPresentationKind::Images,
        ] {
            assert!(kind.is_projected(), "{kind:?} has no projection");
        }
    }
}
