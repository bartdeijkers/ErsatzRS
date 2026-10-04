mod block;
mod block_sources;
pub use block_sources::validate_block_history_checkpoint;
mod classic;
#[cfg(test)]
mod playlist_tests;
mod programme;
mod scripted;
mod smart_shuffle;
mod yaml;

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::{
    ChannelPlayoutMode, CollectionType, EntityId, FillerKind, FillerMode, FixedStartTimeBehavior,
    MarathonGroupBy, PlaybackOrder, Playout, PlayoutAnchor, PlayoutClassicCheckpoint, PlayoutGap,
    PlayoutHistory, PlayoutItem, PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex,
    PlayoutScheduleKind, PlayoutScriptedCheckpoint, ProgramScheduleItem,
};
use serde::{Deserialize, Serialize};

use crate::repositories::Result as RepositoryResult;

pub use block::{
    BlockDefinition, BlockKey, BlockSchedulingEngine, BlockTemplate, BlockTemplateItem,
    BlockTemplateSchedule, EffectiveBlock, EffectiveBlockItem, EffectiveBreakContent,
    EffectiveDeco, EffectiveDecoSelection, EffectiveDecoTemplateItem,
};
pub use classic::{
    audit_classic_refresh_checkpoint, audit_classic_runtime_state,
    classic_playlist_continuation_is_complete, classic_schedule_item_key,
    classic_schedule_item_policy_key, truncate_to_persisted_precision, ClassicClock,
    ClassicRefreshCheckpointAudit, ClassicRuntimeStateAudit, ClassicSchedulingContext,
    ClassicSchedulingEngine,
};
pub use scripted::{
    validate_scripted_checkpoint, validate_scripted_timeline, ScriptedContentResolver,
    ScriptedScheduleCommand, ScriptedSchedulingEngine,
};
pub use smart_shuffle::{
    block_item_smart_shuffle_config_key, legacy_playout_cooldown_config_key,
    legacy_program_schedule_cooldown_config_key, program_schedule_item_smart_shuffle_config_key,
    SchedulingSmartShuffleCandidate, SchedulingSmartShuffleMode, SchedulingSmartShufflePolicy,
    SchedulingSmartShufflePreset, SchedulingSmartShuffleRecordContext, SchedulingSmartShuffleRule,
    SchedulingSmartShuffleRuleKind, SchedulingSmartShuffleScope, SchedulingSmartShuffleState,
    SchedulingSmartShuffleUnit,
};
pub use yaml::{
    validate_yaml_checkpoint, YamlInstruction, YamlPlayoutDefinition, YamlPlayoutEngine,
    YamlPlayoutScheduleItem, YamlPlayoutSequenceItem, YamlRepeat,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayoutBuildMode {
    Refresh,
    Reset,
    #[default]
    Continue,
}

/// Current-programme policy for a scheduler-owned rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayoutPreservation {
    /// Preserve the existing automatic/legacy scheduler contract.
    #[default]
    EngineDefault,
    /// Retain the published programme through its complete guide boundary.
    KeepCurrent,
    /// The operator permits replacement of the programme currently on air.
    ReplaceCurrent,
}

/// Request for the scheduler-owned playout builder.
///
/// This is the in-process equivalent of ErsatzTV's
/// `Application/Playouts/Commands/BuildPlayout.cs` background-service request:
/// callers identify one playout and the build mode, while the scheduler keeps
/// ownership of locking, reference-data loading, and persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayoutBuildRequest {
    pub playout_id: EntityId,
    pub mode: PlayoutBuildMode,
    pub preservation: PlayoutPreservation,
}

impl PlayoutBuildRequest {
    /// Build using the scheduler's established automatic/legacy semantics.
    #[must_use]
    pub const fn new(playout_id: EntityId, mode: PlayoutBuildMode) -> Self {
        Self {
            playout_id,
            mode,
            preservation: PlayoutPreservation::EngineDefault,
        }
    }

    /// Attach the operator's explicit current-programme choice.
    #[must_use]
    pub const fn with_preservation(mut self, preservation: PlayoutPreservation) -> Self {
        self.preservation = preservation;
        self
    }

    /// Refuse an explicit policy without a supported restore/publication path.
    ///
    /// # Errors
    /// Returns an error rather than silently discarding the operator's choice.
    pub fn validate_preservation(self, kind: PlayoutScheduleKind) -> Result<(), SchedulingError> {
        if self.preservation != PlayoutPreservation::EngineDefault
            && kind == PlayoutScheduleKind::None
        {
            return Err(SchedulingError::UnsupportedScheduleKind(kind));
        }
        let supported = match self.preservation {
            PlayoutPreservation::EngineDefault => true,
            PlayoutPreservation::KeepCurrent => {
                self.mode == PlayoutBuildMode::Continue
                    || matches!(
                        kind,
                        PlayoutScheduleKind::Scripted
                            | PlayoutScheduleKind::Classic
                            | PlayoutScheduleKind::Block
                            | PlayoutScheduleKind::ExternalJson
                    )
                    || kind == PlayoutScheduleKind::Sequential
            }
            PlayoutPreservation::ReplaceCurrent => self.mode != PlayoutBuildMode::Continue,
        };
        if supported {
            Ok(())
        } else {
            Err(SchedulingError::InvalidRebuild(
                "the requested programme-preservation policy is not supported for this rebuild"
                    .into(),
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingMediaItem {
    pub id: EntityId,
    pub duration: Duration,
    /// Source interval for a filler chapter; absent for ordinary media.
    #[serde(default)]
    pub chapter: Option<SchedulingMediaChapter>,
    /// Present only for Remote Streams; unknown remains explicit so schedule
    /// validation can refuse it instead of treating zero duration as finite.
    #[serde(default)]
    pub liveness: Option<ersatzrs_domain::RemoteStreamLiveness>,
    pub title: Option<String>,
    pub show_id: Option<EntityId>,
    pub season_id: Option<EntityId>,
    pub artist_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_director: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub release_date: Option<DateTime<Utc>>,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<ersatzrs_domain::ChannelSubtitleMode>,
    pub pinned_stream_selection: Option<ersatzrs_domain::PinnedStreamSelection>,
}

/// Chapter metadata retained while Classic filler enumerates source intervals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingMediaChapter {
    pub start: Duration,
    pub title: Option<String>,
}

/// Media identity and optional exact version used for chapter boundaries.
pub type SchedulingChapterKey = (EntityId, Option<EntityId>);

impl SchedulingMediaItem {
    /// Schedule pins override playlist-item pins, as in playback selection.
    #[must_use]
    pub fn chapter_key(
        &self,
        schedule_pin: Option<ersatzrs_domain::PinnedStreamSelection>,
    ) -> SchedulingChapterKey {
        (
            self.id,
            schedule_pin
                .or(self.pinned_stream_selection)
                .map(|pin| pin.media_version_id),
        )
    }

    #[must_use]
    pub fn new(id: EntityId, duration: Duration) -> Self {
        Self {
            id,
            duration,
            chapter: None,
            liveness: None,
            title: None,
            show_id: None,
            season_id: None,
            artist_id: None,
            album: None,
            first_director: None,
            season: None,
            episode: None,
            release_date: None,
            preferred_audio_language_code: None,
            preferred_audio_title: None,
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
            pinned_stream_selection: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SchedulingContentGuid {
    pub source: String,
    pub value: String,
}

impl SchedulingContentGuid {
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}://{}", self.source, self.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SchedulingCollectionKey {
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerun_collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub search_query: Option<String>,
    pub fake_collection_key: Option<String>,
}

impl SchedulingCollectionKey {
    #[must_use]
    pub fn fake_playlist_item(collection_id: EntityId) -> Self {
        Self {
            collection_type: CollectionType::FakePlaylistItem,
            collection_id: Some(collection_id),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingPlaylistItem {
    pub collection_key: SchedulingCollectionKey,
    pub playback_order: PlaybackOrder,
    pub play_all: bool,
    pub count: Option<usize>,
    pub media_items: Vec<SchedulingMediaItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedulingContentSource {
    DirectKey(String),
    Collection(String),
    MultiCollection(String),
    SmartCollection(String),
    Playlist {
        group: Option<String>,
        name: String,
    },
    Search(String),
    Show {
        name: Option<String>,
        guids: Vec<SchedulingContentGuid>,
    },
    Marathon {
        name: Option<String>,
        guids: Vec<SchedulingContentGuid>,
        searches: Vec<String>,
        group_by: String,
        shuffle_groups: bool,
        item_order: PlaybackOrder,
        play_all_items: bool,
    },
    MediaItem(EntityId),
}

/// The source lookup actually performed by the Scripted engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptedSourceResolution {
    Media,
    Playlist,
}

/// Semantic source state captured with a successful Scripted build.
/// Per-library projections let successful scans affect only their own changes,
/// including additions to a previously empty selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptedSourceDependency {
    pub source: SchedulingContentSource,
    pub resolution: ScriptedSourceResolution,
    pub fingerprint: String,
    pub library_fingerprints: BTreeMap<EntityId, String>,
}

impl SchedulingContentSource {
    /// Group already-resolved Marathon media using the shared playlist rules.
    #[must_use]
    pub fn marathon_playlist_items(
        &self,
        media_items: Vec<SchedulingMediaItem>,
    ) -> Option<Vec<SchedulingPlaylistItem>> {
        match self {
            Self::Marathon {
                group_by,
                item_order,
                play_all_items,
                ..
            } => Some(group_marathon_media(
                media_items,
                group_by,
                *item_order,
                *play_all_items,
                None,
            )),
            _ => None,
        }
    }

    #[must_use]
    pub fn direct(key: impl Into<String>) -> Self {
        Self::DirectKey(key.into())
    }
}

pub trait SchedulingContentResolver {
    fn media_items_for_source(&self, source: &SchedulingContentSource) -> Vec<SchedulingMediaItem>;

    fn playlist_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Option<Vec<SchedulingPlaylistItem>> {
        let _ = source;
        None
    }
}

#[async_trait]
pub trait SchedulingContentRepository: Send + Sync {
    async fn media_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> RepositoryResult<Vec<SchedulingMediaItem>>;

    async fn playlist_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> RepositoryResult<Option<Vec<SchedulingPlaylistItem>>>;

    async fn watermark_id_by_name(&self, name: &str) -> RepositoryResult<Option<EntityId>>;

    async fn graphics_element_id_by_path(&self, path: &str) -> RepositoryResult<Option<EntityId>>;
}

/// Storage-neutral content loader for an unsaved block-preview draft.
///
/// The application layer constructs the temporary template, playout, and
/// effective block graph. Implementations hydrate only the real media and
/// Smart Shuffle reference data required by that graph.
#[async_trait]
pub trait BlockPreviewRepository: Send + Sync {
    /// Load real scheduling content for temporary block items.
    ///
    /// # Errors
    ///
    /// Returns a repository error when any referenced collection, search, or
    /// media lookup cannot be completed.
    async fn load_block_preview_content(
        &self,
        items: &[EffectiveBlockItem],
        policies: &BTreeMap<String, SchedulingSmartShufflePolicy>,
    ) -> RepositoryResult<SchedulingReferenceData>;
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SchedulingReferenceData {
    /// Explicit rebuild policy supplied by the scheduler owner after admission.
    pub preservation: PlayoutPreservation,
    /// Host-selected earliest handover, accounting for producer work ahead and
    /// leading mirrors. This changes boundary selection, not the cleanup clock.
    pub preservation_not_before: Option<DateTime<Utc>>,
    /// Production Classic loaders supply the host civil clock; pure inputs default to UTC.
    pub classic_clock: ClassicClock,
    pub channel_playout_mode: ChannelPlayoutMode,
    /// Legacy schedule/playout-wide policy used only when an item has not yet
    /// materialized its own Smart Shuffle attachment.
    pub smart_shuffle_policy: SchedulingSmartShufflePolicy,
    /// Legacy schedule/playout-wide tags. New loaders populate the item-keyed
    /// map below so rules with the same id cannot bleed between units.
    pub smart_shuffle_tags_by_media_id: BTreeMap<EntityId, BTreeSet<String>>,
    pub smart_shuffle_policies_by_unit: BTreeMap<String, SchedulingSmartShufflePolicy>,
    pub smart_shuffle_tags_by_unit: BTreeMap<String, BTreeMap<EntityId, BTreeSet<String>>>,
    pub media_by_key: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub collections_by_name: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub multi_collections_by_name: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub smart_collections_by_name: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub playlists_by_group_and_name: BTreeMap<(Option<String>, String), Vec<SchedulingMediaItem>>,
    pub playlist_items_by_group_and_name:
        BTreeMap<(Option<String>, String), Vec<SchedulingPlaylistItem>>,
    pub playlist_items_by_key: BTreeMap<String, Vec<SchedulingPlaylistItem>>,
    pub searches_by_query: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub shows_by_name: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub media_by_show_guid: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub media_items_by_id: BTreeMap<EntityId, SchedulingMediaItem>,
    pub watermark_ids_by_name: BTreeMap<String, EntityId>,
    pub graphics_ids_by_path: BTreeMap<String, EntityId>,
    pub effective_blocks: Vec<EffectiveBlock>,
    pub classic_schedule_id: Option<EntityId>,
    pub classic_schedule_name: Option<String>,
    pub classic_schedule_items: Vec<ProgramScheduleItem>,
    pub classic_shuffle_schedule_items: bool,
    pub classic_random_start_point: bool,
    pub classic_fixed_start_time_behavior: FixedStartTimeBehavior,
    pub existing_program_schedule_anchors: Vec<PlayoutProgramScheduleAnchor>,
    pub existing_classic_checkpoints: Vec<PlayoutClassicCheckpoint>,
    pub existing_scripted_checkpoints: Vec<PlayoutScriptedCheckpoint>,
    pub existing_yaml_checkpoints: Vec<ersatzrs_domain::PlayoutYamlCheckpoint>,
    pub existing_fill_group_indices: Vec<PlayoutScheduleItemFillGroupIndex>,
    pub existing_items: Vec<PlayoutItem>,
    pub existing_history: Vec<PlayoutHistory>,
    pub max_playout_offset: Duration,
    pub global_watermark_id: Option<EntityId>,
    pub global_fallback: Option<SchedulingFallbackPreset>,
    pub classic_filler_presets: BTreeMap<EntityId, SchedulingFillerPreset>,
    /// Source chapter boundaries for Classic mid-roll placement.
    pub classic_chapters_by_media_id:
        BTreeMap<SchedulingChapterKey, Vec<ersatzrs_domain::MediaChapter>>,
}

/// Configured fallback filler media available to the playout post-processor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulingFallbackPreset {
    pub id: EntityId,
    pub allow_watermarks: bool,
    pub media_items: Vec<SchedulingMediaItem>,
}

/// Schedule-item filler preset resolved from an imported `FillerPreset` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulingFillerPreset {
    pub expression: Option<String>,
    pub id: EntityId,
    pub filler_kind: FillerKind,
    pub filler_mode: FillerMode,
    pub duration: Option<Duration>,
    pub count: Option<i32>,
    pub pad_to_nearest_minute: Option<i32>,
    pub allow_watermarks: bool,
    pub media_items: Vec<SchedulingMediaItem>,
}

impl SchedulingReferenceData {
    #[must_use]
    pub fn has_active_smart_shuffle(&self) -> bool {
        self.smart_shuffle_policy.is_active()
            || self
                .smart_shuffle_policies_by_unit
                .values()
                .any(SchedulingSmartShufflePolicy::is_active)
    }

    #[must_use]
    pub fn smart_shuffle_policy_for_unit(&self, unit_key: &str) -> &SchedulingSmartShufflePolicy {
        self.smart_shuffle_policies_by_unit
            .get(unit_key)
            .unwrap_or(&self.smart_shuffle_policy)
    }

    #[must_use]
    pub fn smart_shuffle_tags_for_unit(
        &self,
        unit_key: &str,
    ) -> &BTreeMap<EntityId, BTreeSet<String>> {
        self.smart_shuffle_tags_by_unit
            .get(unit_key)
            .unwrap_or(&self.smart_shuffle_tags_by_media_id)
    }

    #[must_use]
    pub fn smart_shuffle_tags(&self, media_item_id: EntityId) -> &BTreeSet<String> {
        static EMPTY: std::sync::LazyLock<BTreeSet<String>> =
            std::sync::LazyLock::new(BTreeSet::new);
        self.smart_shuffle_tags_by_media_id
            .get(&media_item_id)
            .unwrap_or(&EMPTY)
    }

    #[must_use]
    pub fn smart_shuffle_tags_for_media(
        &self,
        unit_key: &str,
        media_item_id: EntityId,
    ) -> &BTreeSet<String> {
        static EMPTY: std::sync::LazyLock<BTreeSet<String>> =
            std::sync::LazyLock::new(BTreeSet::new);
        self.smart_shuffle_tags_for_unit(unit_key)
            .get(&media_item_id)
            .unwrap_or(&EMPTY)
    }

    fn media_items_for_show(
        &self,
        name: Option<&String>,
        guids: &[SchedulingContentGuid],
    ) -> Vec<SchedulingMediaItem> {
        let mut result = Vec::new();
        let mut seen_ids = BTreeSet::new();

        if let Some(name) = name {
            if let Some(items) = self.shows_by_name.get(name) {
                append_unique_items(&mut result, &mut seen_ids, items);
            }
        }

        for guid in guids {
            if let Some(items) = self.media_by_show_guid.get(&guid.key()) {
                append_unique_items(&mut result, &mut seen_ids, items);
            }
        }

        result
    }

    fn marathon_playlist_items(
        &self,
        name: Option<&String>,
        guids: &[SchedulingContentGuid],
        searches: &[String],
        group_by: &str,
        item_order: PlaybackOrder,
        play_all_items: bool,
    ) -> Vec<SchedulingPlaylistItem> {
        let mut media_items = self.media_items_for_show(name, guids);
        let mut seen_ids = media_items
            .iter()
            .map(|item| item.id)
            .collect::<BTreeSet<_>>();

        for search in searches {
            if let Some(items) = self.searches_by_query.get(search) {
                append_unique_items(&mut media_items, &mut seen_ids, items);
            }
        }

        group_marathon_media(media_items, group_by, item_order, play_all_items, None)
    }
}

impl SchedulingContentResolver for SchedulingReferenceData {
    fn media_items_for_source(&self, source: &SchedulingContentSource) -> Vec<SchedulingMediaItem> {
        match source {
            SchedulingContentSource::DirectKey(key) => {
                self.media_by_key.get(key).cloned().unwrap_or_default()
            }
            SchedulingContentSource::Collection(name) => self
                .collections_by_name
                .get(name)
                .cloned()
                .unwrap_or_default(),
            SchedulingContentSource::MultiCollection(name) => self
                .multi_collections_by_name
                .get(name)
                .cloned()
                .unwrap_or_default(),
            SchedulingContentSource::SmartCollection(name) => self
                .smart_collections_by_name
                .get(name)
                .cloned()
                .unwrap_or_default(),
            SchedulingContentSource::Playlist { group, name } => self
                .playlists_by_group_and_name
                .get(&(group.clone(), name.clone()))
                .or_else(|| self.playlists_by_group_and_name.get(&(None, name.clone())))
                .cloned()
                .unwrap_or_default(),
            SchedulingContentSource::Search(query) => self
                .searches_by_query
                .get(query)
                .cloned()
                .unwrap_or_default(),
            SchedulingContentSource::Show { name, guids } => {
                self.media_items_for_show(name.as_ref(), guids)
            }
            SchedulingContentSource::Marathon {
                name,
                guids,
                searches,
                ..
            } => self
                .media_items_for_show(name.as_ref(), guids)
                .into_iter()
                .chain(
                    searches
                        .iter()
                        .filter_map(|query| self.searches_by_query.get(query))
                        .flat_map(|items| items.iter().cloned()),
                )
                .collect(),
            SchedulingContentSource::MediaItem(id) => self
                .media_items_by_id
                .get(id)
                .cloned()
                .into_iter()
                .collect(),
        }
    }

    fn playlist_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Option<Vec<SchedulingPlaylistItem>> {
        match source {
            SchedulingContentSource::Playlist { group, name } => self
                .playlist_items_by_group_and_name
                .get(&(group.clone(), name.clone()))
                .or_else(|| {
                    self.playlist_items_by_group_and_name
                        .get(&(None, name.clone()))
                })
                .cloned(),
            SchedulingContentSource::Marathon {
                name,
                guids,
                searches,
                group_by,
                shuffle_groups: _,
                item_order,
                play_all_items,
            } => Some(self.marathon_playlist_items(
                name.as_ref(),
                guids,
                searches,
                group_by,
                *item_order,
                *play_all_items,
            )),
            _ => None,
        }
    }
}

fn append_unique_items(
    target: &mut Vec<SchedulingMediaItem>,
    seen_ids: &mut BTreeSet<EntityId>,
    items: &[SchedulingMediaItem],
) {
    for item in items {
        if seen_ids.insert(item.id) {
            target.push(item.clone());
        }
    }
}

/// Build the grouped playlist used by Classic Marathon, matching ErsatzTV's
/// `MarathonHelper`. A batch replaces play-all with a per-group item count.
#[must_use]
pub fn classic_marathon_playlist_items(
    item: &ProgramScheduleItem,
    media_items: Vec<SchedulingMediaItem>,
) -> Vec<SchedulingPlaylistItem> {
    let group_by = match item.marathon_group_by {
        MarathonGroupBy::None => return Vec::new(),
        MarathonGroupBy::Show => "show",
        MarathonGroupBy::Season => "season",
        MarathonGroupBy::Artist => "artist",
        MarathonGroupBy::Album => "album",
        MarathonGroupBy::Director => "director",
    };
    let order = if item.marathon_shuffle_items {
        PlaybackOrder::Shuffle
    } else if matches!(
        item.marathon_group_by,
        MarathonGroupBy::Show | MarathonGroupBy::Season
    ) {
        PlaybackOrder::SeasonEpisode
    } else {
        PlaybackOrder::Chronological
    };
    let count = item
        .marathon_batch_size
        .and_then(|count| usize::try_from(count).ok())
        .filter(|count| *count > 0);
    group_marathon_media(media_items, group_by, order, count.is_none(), count)
}

fn valid_marathon_group_by(value: &str) -> bool {
    ["show", "season", "artist", "album", "director"]
        .iter()
        .any(|valid| value.eq_ignore_ascii_case(valid))
}

fn group_marathon_media(
    media_items: Vec<SchedulingMediaItem>,
    group_by: &str,
    item_order: PlaybackOrder,
    play_all_items: bool,
    count: Option<usize>,
) -> Vec<SchedulingPlaylistItem> {
    let mut groups: Vec<(SchedulingCollectionKey, Vec<SchedulingMediaItem>)> = Vec::new();
    let mut seen_ids = BTreeSet::new();
    for media_item in media_items {
        if !seen_ids.insert(media_item.id) {
            continue;
        }
        let Some(collection_key) = marathon_collection_key(&media_item, group_by) else {
            continue;
        };

        if let Some((_, group_items)) = groups
            .iter_mut()
            .find(|(existing_key, _)| *existing_key == collection_key)
        {
            group_items.push(media_item);
        } else {
            groups.push((collection_key, vec![media_item]));
        }
    }

    groups
        .into_iter()
        .map(|(collection_key, mut group_items)| {
            sort_media_items_for_playback_order(&mut group_items, item_order);

            SchedulingPlaylistItem {
                collection_key,
                playback_order: item_order,
                play_all: play_all_items,
                count,
                media_items: group_items,
            }
        })
        .collect()
}

fn marathon_collection_key(
    media_item: &SchedulingMediaItem,
    group_by: &str,
) -> Option<SchedulingCollectionKey> {
    let (collection_type, media_item_id, fake_collection_key) =
        match group_by.to_ascii_lowercase().as_str() {
            "show" => (
                CollectionType::TelevisionShow,
                Some(media_item.show_id.unwrap_or(0)),
                None,
            ),
            "season" => (
                CollectionType::TelevisionSeason,
                Some(media_item.season_id.unwrap_or(0)),
                None,
            ),
            "artist" => (
                CollectionType::Artist,
                Some(media_item.artist_id.unwrap_or(0)),
                None,
            ),
            // Stable textual keys avoid collisions between album or director names.
            "album" => (
                CollectionType::FakePlaylistItem,
                None,
                Some(format!(
                    "marathon:album:{:?}:{:?}",
                    media_item.artist_id, media_item.album
                )),
            ),
            "director" => (
                CollectionType::FakePlaylistItem,
                None,
                Some(format!("marathon:director:{:?}", media_item.first_director)),
            ),
            _ => return None,
        };
    Some(SchedulingCollectionKey {
        collection_type,
        media_item_id,
        fake_collection_key,
        ..SchedulingCollectionKey::default()
    })
}

fn sort_media_items_for_playback_order(
    media_items: &mut [SchedulingMediaItem],
    playback_order: PlaybackOrder,
) {
    match playback_order {
        PlaybackOrder::Chronological => media_items.sort_by(|left, right| {
            left.release_date
                .cmp(&right.release_date)
                .then(left.season.cmp(&right.season))
                .then(left.episode.cmp(&right.episode))
                .then(left.id.cmp(&right.id))
        }),
        PlaybackOrder::SeasonEpisode => media_items.sort_by(|left, right| {
            left.season
                .cmp(&right.season)
                .then(left.episode.cmp(&right.episode))
                .then(left.release_date.cmp(&right.release_date))
                .then(left.id.cmp(&right.id))
        }),
        _ => {}
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchedulingRequest {
    pub playout: Playout,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub mode: PlayoutBuildMode,
    pub reference_data: SchedulingReferenceData,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SchedulingResult {
    /// Storage revision captured before loading build inputs. Production hosts
    /// attach this guard; publication checks it before any persisted mutation.
    pub publication_guard: Option<PlayoutPublicationGuard>,
    /// Process-local deadline for replacing the future after a retained
    /// programme. Production accounts for the request clock, elapsed build time,
    /// producer prefetch and mirror offsets; storage refuses expired candidates.
    pub publication_deadline: Option<std::time::Instant>,
    pub clear_items: bool,
    /// Classic Refresh keeps the already-built prefix through this checkpoint.
    pub retain_through: Option<DateTime<Utc>>,
    pub remove_before: Option<DateTime<Utc>>,
    pub added_items: Vec<PlayoutItem>,
    pub added_gaps: Vec<PlayoutGap>,
    pub added_history: Vec<PlayoutHistory>,
    pub history_to_remove: Vec<EntityId>,
    pub items_to_remove: Vec<EntityId>,
    pub anchor: Option<PlayoutAnchor>,
    /// Collection-enumerator anchors produced by Classic scheduling.
    /// Block/YAML/Scripted engines leave this empty; state is in `anchor.context`.
    pub program_schedule_anchors: Vec<PlayoutProgramScheduleAnchor>,
    /// Complete Classic state at each dated rebuild seam. Other engines leave
    /// this empty.
    pub classic_checkpoints: Vec<PlayoutClassicCheckpoint>,
    /// Scripted source state at every retained and generated item boundary.
    pub scripted_checkpoints: Vec<PlayoutScriptedCheckpoint>,
    /// YAML instruction and source state at complete programme boundaries.
    pub yaml_checkpoints: Vec<ersatzrs_domain::PlayoutYamlCheckpoint>,
    /// Runtime-resolved SQL sources, published atomically with their timeline.
    pub scripted_sources: Vec<ScriptedSourceDependency>,
    /// Fill-group enumerator state produced by Classic scheduling.
    pub fill_group_indices: Vec<PlayoutScheduleItemFillGroupIndex>,
    pub warnings: Vec<String>,
}

/// Source revision observed before a playout build reads its inputs.
/// An absent revision also participates in the check: a newly queued source
/// change invalidates a candidate built when no refresh was pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayoutPublicationGuard {
    /// Playout whose inputs were observed.
    pub playout_id: EntityId,
    /// Identity and generation of pending source work, if any.
    pub source_revision: Option<PlayoutSourceRevision>,
}

/// A generation together with the identity of its durable refresh lifecycle.
/// Identity prevents a deleted and recreated generation from comparing equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayoutSourceRevision {
    /// Stable identity of this refresh lifecycle, supplied by storage.
    pub identity: String,
    /// Monotonically increasing generation within that lifecycle.
    pub generation: i64,
}

pub trait PlayoutSchedulingEngine {
    /// Build a playout timeline for the supplied request.
    ///
    /// # Errors
    ///
    /// Returns an error when the engine cannot load or evaluate its schedule
    /// definition.
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError>;

    /// Build with cooperative cancellation. Engines can additionally check
    /// inside long-running loops; the default rejects cancelled results.
    ///
    /// # Errors
    /// Returns `Cancelled` when cancellation is requested, or a scheduling error.
    fn schedule_cancellable(
        &self,
        request: SchedulingRequest,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<SchedulingResult, SchedulingError> {
        if cancelled() {
            return Err(SchedulingError::Cancelled);
        }
        let result = self.schedule(request)?;
        if cancelled() {
            return Err(SchedulingError::Cancelled);
        }
        Ok(result)
    }
}

/// `ErsatzTV`'s external JSON playout builder is a no-op: external playout rows
/// are owned by the external producer, so a build request succeeds without
/// mutating generated items or runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalJsonSchedulingEngine;

impl PlayoutSchedulingEngine for ExternalJsonSchedulingEngine {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        if request.reference_data.preservation != PlayoutPreservation::EngineDefault {
            return Err(SchedulingError::InvalidRebuild(
                "explicit external JSON adaptation requires the snapshot publication worker".into(),
            ));
        }
        Ok(SchedulingResult::default())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulingError {
    #[error("invalid playout rebuild: {0}")]
    InvalidRebuild(String),
    #[error("playout build cancelled")]
    Cancelled,
    #[error("cannot reposition Block item {0}: scheduling history is missing")]
    MissingBlockHistory(EntityId),
    #[error("missing schedule file")]
    MissingScheduleFile,
    #[error("schedule file {0} does not exist")]
    ScheduleFileMissing(String),
    #[error("invalid YAML schedule: {0}")]
    InvalidYaml(String),
    #[error("YAML sequence contains a cycle")]
    SequenceCycle,
    #[error("YAML sequence nesting is too deep")]
    SequenceNestingTooDeep,
    #[error("unsupported YAML instruction")]
    UnsupportedYamlInstruction,
    #[error("Remote Stream {0} has unknown playback behavior")]
    UnknownRemoteStreamLiveness(EntityId),
    #[error("finite Remote Stream {0} requires a positive duration")]
    FiniteRemoteStreamMissingDuration(EntityId),
    #[error("live Remote Stream {0} requires Classic Flood mode with a deterministic boundary")]
    UnsupportedLiveSourcePlacement(EntityId),
    #[error(
        "Classic schedule item {0} has no schedulable media after invalid Remote Streams were filtered"
    )]
    EmptyClassicSourceAfterRemoteStreamFilter(EntityId),
    #[error("invalid duration: {0}")]
    InvalidDuration(String),
    #[error("invalid time: {0}")]
    InvalidTime(String),
    #[error("script error: {0}")]
    Script(String),
    #[error("Scripted timeline rejected: {0}. The published timeline was not changed")]
    InvalidScriptedTimeline(String),
    #[error("{0}")]
    UnsafeClassicRefresh(String),
    #[error("invalid chapter filler expression for Classic schedule item {0}")]
    InvalidClassicFillerExpression(EntityId),
    #[error("unsupported playout schedule kind: {0:?}")]
    UnsupportedScheduleKind(PlayoutScheduleKind),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct MediaEnumerator {
    items: Vec<SchedulingMediaItem>,
    index: usize,
    playback_order: PlaybackOrder,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shuffle_seed: Option<i32>,
}

impl MediaEnumerator {
    fn new(_key: impl Into<String>, items: Vec<SchedulingMediaItem>) -> Self {
        Self {
            items,
            index: 0,
            playback_order: PlaybackOrder::None,
            shuffle_seed: None,
        }
    }

    fn with_playback_order(mut self, playback_order: PlaybackOrder) -> Self {
        self.playback_order = playback_order;
        self
    }

    fn playback_order(&self) -> PlaybackOrder {
        self.playback_order
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn current(&self) -> Option<&SchedulingMediaItem> {
        if self.items.is_empty() {
            None
        } else {
            self.items.get(self.index % self.items.len())
        }
    }

    fn select_with_smart_shuffle(
        &mut self,
        policy: &SchedulingSmartShufflePolicy,
        state: &SchedulingSmartShuffleState,
        scope_key: &str,
        scheduled_at: DateTime<Utc>,
        tags_by_media_id: &BTreeMap<EntityId, BTreeSet<String>>,
    ) {
        if !policy.is_active() || !is_randomized_playback_order(self.playback_order) {
            return;
        }
        let empty = BTreeSet::new();
        let candidates = (0..self.items.len())
            .map(|offset| {
                let index = (self.index + offset) % self.items.len();
                let item = &self.items[index];
                SchedulingSmartShuffleCandidate {
                    media_item_id: item.id,
                    tags: tags_by_media_id.get(&item.id).unwrap_or(&empty),
                }
            })
            .collect::<Vec<_>>();
        let Some(offset) = state.select_candidate(policy, scope_key, scheduled_at, &candidates)
        else {
            return;
        };
        let selected = (self.index + offset) % self.items.len();
        self.items.swap(self.index, selected);
    }

    fn set_index(&mut self, index: usize) {
        self.index = if self.items.is_empty() {
            0
        } else {
            index % self.items.len()
        };
    }

    fn advance(&mut self) {
        if !self.items.is_empty() {
            let previous_id = self.current().map(|item| item.id);
            self.index = (self.index + 1) % self.items.len();
            if self.index == 0 {
                if let Some(seed) = self.shuffle_seed {
                    let mut next_seed = classic::next_i32_seed(seed);
                    self.restore_shuffle_seed(next_seed);
                    if self.items.iter().any(|item| Some(item.id) != previous_id) {
                        while self
                            .current()
                            .is_some_and(|item| Some(item.id) == previous_id)
                        {
                            next_seed = classic::next_i32_seed(next_seed);
                            self.restore_shuffle_seed(next_seed);
                        }
                    }
                }
            }
        }
    }

    fn restore_shuffle_seed(&mut self, seed: i32) {
        if self.playback_order == PlaybackOrder::Shuffle {
            self.shuffle_seed = Some(seed);
            self.items.sort_by_key(|item| item.id);
            shuffle_with_seed(&mut self.items, i64::from(seed).cast_unsigned());
        }
    }

    fn current_index(&self) -> usize {
        if self.items.is_empty() {
            0
        } else {
            self.index % self.items.len()
        }
    }

    pub(super) fn scheduling_context_name(&self) -> &'static str {
        match self.playback_order {
            PlaybackOrder::Chronological => "Chronological",
            PlaybackOrder::SeasonEpisode => "Season, Episode",
            PlaybackOrder::Shuffle => "Block Shuffle",
            PlaybackOrder::RandomRotation => "Random Rotating",
            PlaybackOrder::Random
            | PlaybackOrder::None
            | PlaybackOrder::ShuffleInOrder
            | PlaybackOrder::MultiEpisodeShuffle
            | PlaybackOrder::Marathon => "Random",
        }
    }

    fn apply_history(&mut self, history: &PlayoutHistory) {
        if self.items.is_empty() {
            return;
        }
        let next_index = history.index.saturating_add(1);
        self.set_index(usize::try_from(next_index).unwrap_or_default());
        if self.playback_order == PlaybackOrder::Chronological {
            self.move_after_history_details(&history.details);
        }
    }

    fn move_after_history_details(&mut self, details: &str) {
        let Ok(details) = serde_json::from_str::<HistoryDetails>(details) else {
            return;
        };
        if let Some(index) = self.index_for_details(&details) {
            self.set_index(index.saturating_add(1));
        }
    }

    fn move_to_history_details(&mut self, details: &str) {
        let Ok(details) = serde_json::from_str::<HistoryDetails>(details) else {
            return;
        };
        if let Some(index) = self.index_for_details(&details) {
            self.set_index(index);
        }
    }

    fn index_for_details(&self, details: &HistoryDetails) -> Option<usize> {
        if let Some(media_item_id) = details.media_item_id {
            if let Some(index) = self.items.iter().position(|item| item.id == media_item_id) {
                return Some(index);
            }
        }
        if let (Some(season), Some(episode)) = (details.season_number, details.episode_number) {
            let exact = self
                .items
                .iter()
                .position(|item| item.season == Some(season) && item.episode == Some(episode));
            return Some(exact.unwrap_or_else(|| {
                self.items
                    .iter()
                    .position(|item| (item.season, item.episode) > (Some(season), Some(episode)))
                    .unwrap_or_default()
            }));
        }
        details.release_date.map(|release_date| {
            self.items
                .iter()
                .position(|item| item.release_date.is_some_and(|date| date >= release_date))
                .unwrap_or_default()
        })
    }
}

fn is_randomized_playback_order(playback_order: PlaybackOrder) -> bool {
    matches!(
        playback_order,
        PlaybackOrder::Random
            | PlaybackOrder::Shuffle
            | PlaybackOrder::ShuffleInOrder
            | PlaybackOrder::MultiEpisodeShuffle
            | PlaybackOrder::RandomRotation
            | PlaybackOrder::Marathon
    )
}

fn lookup_media(reference_data: &SchedulingReferenceData, key: &str) -> MediaEnumerator {
    lookup_media_source(reference_data, key, &SchedulingContentSource::direct(key))
}

fn lookup_media_source(
    reference_data: &SchedulingReferenceData,
    key: &str,
    source: &SchedulingContentSource,
) -> MediaEnumerator {
    MediaEnumerator::new(key, reference_data.media_items_for_source(source))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct HistoryDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    media_item_id: Option<EntityId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    season_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_cycle: Option<PlaylistCycleHistory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlaylistCycleHistory {
    all_media_ids: BTreeSet<EntityId>,
    remaining_media_ids: BTreeSet<EntityId>,
}

fn history_details_for_media_item(media_item: &SchedulingMediaItem) -> String {
    let details = HistoryDetails {
        media_item_id: Some(media_item.id),
        release_date: media_item.release_date,
        season_number: media_item.season,
        episode_number: media_item.episode,
        playlist_cycle: None,
    };
    serde_json::to_string(&details).unwrap_or_default()
}

fn history_key_for_collection_key(collection_key: &SchedulingCollectionKey) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct Key<'a> {
        collection_type: CollectionType,
        #[serde(skip_serializing_if = "Option::is_none")]
        collection_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        multi_collection_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        smart_collection_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rerun_collection_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        media_item_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        playlist_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_query: Option<&'a str>,
        #[serde(rename = "FakeCollectionKey", skip_serializing_if = "Option::is_none")]
        fake_collection_token: Option<&'a str>,
    }

    serde_json::to_string(&Key {
        collection_type: collection_key.collection_type,
        collection_id: collection_key.collection_id,
        multi_collection_id: collection_key.multi_collection_id,
        smart_collection_id: collection_key.smart_collection_id,
        rerun_collection_id: collection_key.rerun_collection_id,
        media_item_id: collection_key.media_item_id,
        playlist_id: collection_key.playlist_id,
        search_query: collection_key.search_query.as_deref(),
        fake_collection_token: collection_key.fake_collection_key.as_deref(),
    })
    .unwrap_or_default()
}

fn history_key_for_content(key: &str, playback_order: PlaybackOrder) -> String {
    serde_json::json!({
        "Key": key,
        "Order": format!("{:?}", playback_order),
    })
    .to_string()
}

fn cleanup_history(
    existing_history: &[PlayoutHistory],
    start: DateTime<Utc>,
    max_playout_offset: Duration,
) -> Vec<EntityId> {
    let mut groups = BTreeMap::<&str, Vec<&PlayoutHistory>>::new();
    for history in existing_history {
        groups.entry(&history.key).or_default().push(history);
    }

    let cutoff = start - max_playout_offset;
    let mut remove = Vec::new();
    for group in groups.values() {
        let keep_when = group
            .iter()
            .filter(|history| history.finish.is_some_and(|finish| finish < cutoff))
            .map(|history| history.when)
            .max();

        if let Some(keep_when) = keep_when {
            remove.extend(
                group
                    .iter()
                    .filter(|history| history.finish.is_some_and(|finish| finish < cutoff))
                    .filter(|history| history.when != keep_when)
                    .map(|history| history.id),
            );
        }
    }

    remove.sort_unstable();
    remove.dedup();
    remove
}

#[allow(clippy::too_many_arguments)]
fn playout_item(
    playout_id: EntityId,
    media_item: &SchedulingMediaItem,
    start: DateTime<Utc>,
    duration: Duration,
    guide_group: i32,
    filler_kind: ersatzrs_domain::FillerKind,
    custom_title: Option<String>,
    disable_watermarks: bool,
    watermark_ids: Vec<EntityId>,
    graphics_element_ids: Vec<EntityId>,
    scheduling_context: Option<String>,
) -> PlayoutItem {
    let graphics_elements = graphics_element_ids
        .iter()
        .copied()
        .map(
            |graphics_element_id| ersatzrs_domain::PlayoutItemGraphicsElement {
                playout_item_id: 0,
                graphics_element_id: Some(graphics_element_id),
                variables: None,
            },
        )
        .collect();
    PlayoutItem {
        playout_id,
        media_item_id: media_item.id,
        start,
        finish: start + duration,
        guide_group,
        in_point: media_item
            .chapter
            .as_ref()
            .map_or(Duration::zero(), |chapter| chapter.start),
        out_point: media_item
            .chapter
            .as_ref()
            .map_or(Duration::zero(), |chapter| chapter.start)
            + duration,
        chapter_title: media_item
            .chapter
            .as_ref()
            .and_then(|chapter| chapter.title.clone()),
        filler_kind,
        custom_title,
        disable_watermarks,
        watermark_ids,
        preferred_audio_language_code: media_item.preferred_audio_language_code.clone(),
        preferred_audio_title: media_item.preferred_audio_title.clone(),
        preferred_subtitle_language_code: media_item.preferred_subtitle_language_code.clone(),
        subtitle_mode: media_item.subtitle_mode,
        pinned_stream_selection: media_item.pinned_stream_selection,
        graphics_element_ids,
        graphics_elements,
        scheduling_context,
        ..PlayoutItem::default()
    }
}

fn history(
    playout_id: EntityId,
    key: impl Into<String>,
    item: &PlayoutItem,
    child_key: Option<String>,
    index: usize,
    details: String,
    is_current_child: bool,
) -> PlayoutHistory {
    PlayoutHistory {
        playout_id,
        key: key.into(),
        child_key,
        when: item.start,
        finish: Some(item.finish),
        index: i32::try_from(index).unwrap_or(i32::MAX),
        details,
        is_current_child,
        ..PlayoutHistory::default()
    }
}

fn parse_duration(value: &str) -> Result<Duration, SchedulingError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SchedulingError::InvalidDuration(value.to_owned()));
    }

    let parts = trimmed
        .split(':')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SchedulingError::InvalidDuration(value.to_owned()))?;

    match parts.as_slice() {
        [seconds] => Ok(Duration::seconds(*seconds)),
        [minutes, seconds] => Ok(Duration::minutes(*minutes) + Duration::seconds(*seconds)),
        [hours, minutes, seconds] => {
            Ok(Duration::hours(*hours) + Duration::minutes(*minutes) + Duration::seconds(*seconds))
        }
        _ => Err(SchedulingError::InvalidDuration(value.to_owned())),
    }
}

fn parse_hhmm(value: &str) -> Result<(u32, u32, u32), SchedulingError> {
    let parts = value
        .trim()
        .split(':')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SchedulingError::InvalidTime(value.to_owned()))?;

    match parts.as_slice() {
        [hour, minute] if *hour < 24 && *minute < 60 => Ok((*hour, *minute, 0)),
        [hour, minute, second] if *hour < 24 && *minute < 60 && *second < 60 => {
            Ok((*hour, *minute, *second))
        }
        _ => Err(SchedulingError::InvalidTime(value.to_owned())),
    }
}

// ── shared playlist types ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(super) enum CurrentHistoryContext {
    Single {
        index: usize,
    },
    Playlist {
        index: usize,
        child_key: String,
        child_index: usize,
        child_details: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PlaylistChildEnumerator {
    playlist_index: usize,
    pub collection_key: SchedulingCollectionKey,
    pub play_all: bool,
    pub count: Option<usize>,
    pub items_taken_from_current: usize,
    pub enumerator: MediaEnumerator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PlaylistEnumerator {
    pub cycle: u64,
    pub index: usize,
    pub child_index: usize,
    pub children: Vec<PlaylistChildEnumerator>,
    all_media_ids: BTreeSet<EntityId>,
    pub remaining_media_ids: BTreeSet<EntityId>,
    pub shuffle_seed: Option<i32>,
}

impl PlaylistEnumerator {
    pub fn with_child_shuffle_seed(mut self, seed: i32) -> Self {
        for child in &mut self.children {
            child.enumerator.restore_shuffle_seed(seed);
        }
        self
    }

    pub fn new(playlist_items: Vec<SchedulingPlaylistItem>) -> Self {
        let mut children = playlist_items
            .into_iter()
            .enumerate()
            .map(|(index, mut item)| {
                if item.playback_order == PlaybackOrder::SeasonEpisode {
                    item.media_items.retain(|media| {
                        media.show_id.is_none() || media.season.is_some_and(|season| season > 0)
                    });
                }
                sort_media_items_for_playback_order(&mut item.media_items, item.playback_order);
                PlaylistChildEnumerator {
                    playlist_index: index,
                    collection_key: item.collection_key,
                    play_all: item.play_all,
                    count: item.count,
                    items_taken_from_current: 0,
                    enumerator: MediaEnumerator::new(
                        format!("playlist-child-{index}"),
                        item.media_items,
                    )
                    .with_playback_order(item.playback_order),
                }
            })
            .filter(|child| !child.enumerator.is_empty())
            .collect::<Vec<_>>();
        for index in 0..children.len() {
            if let Some(previous) = children[..index]
                .iter()
                .find(|child| child.collection_key == children[index].collection_key)
            {
                children[index].enumerator = previous.enumerator.clone();
            }
        }
        let all_media_ids = children
            .iter()
            .flat_map(|child| child.enumerator.items.iter().map(|media| media.id))
            .collect::<BTreeSet<_>>();
        Self {
            cycle: 0,
            index: 0,
            child_index: 0,
            children,
            remaining_media_ids: all_media_ids.clone(),
            all_media_ids,
            shuffle_seed: None,
        }
    }

    pub fn shuffle_groups(&mut self, seed: i32) {
        self.shuffle_seed = Some(seed);
        // Reconstruct from definition order, never from the previous shuffle.
        self.children.sort_by_key(|child| child.playlist_index);
        if self.children.len() >= 3 {
            let last = self.children.last().map(|child| child.playlist_index);
            shuffle_with_seed(&mut self.children, i64::from(seed).cast_unsigned());
            if self.children.first().map(|child| child.playlist_index) == last {
                self.children.rotate_left(1);
            }
        }
    }

    pub fn replay_to(&mut self, index: usize) {
        if self.children.is_empty() {
            return;
        }
        while self.index < index {
            self.advance();
            // Membership can shrink between builds, making the saved index
            // unreachable in the current cycle.
            if self.index == 0 {
                break;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.children
            .iter()
            .map(|child| {
                if child.play_all {
                    child.enumerator.len().max(1)
                } else {
                    child.count.unwrap_or(1)
                }
            })
            .sum()
    }

    pub fn unique_media_len(&self) -> usize {
        self.children
            .iter()
            .flat_map(|child| child.enumerator.items.iter().map(|item| item.id))
            .collect::<BTreeSet<_>>()
            .len()
    }

    fn current_child(&self) -> Option<&PlaylistChildEnumerator> {
        if self.children.is_empty() {
            None
        } else {
            self.children.get(self.child_index % self.children.len())
        }
    }

    fn current_child_mut(&mut self) -> Option<&mut PlaylistChildEnumerator> {
        if self.children.is_empty() {
            None
        } else {
            let index = self.child_index % self.children.len();
            self.children.get_mut(index)
        }
    }

    // ErsatzTV shares one enumerator for repeated collection references. Keep
    // our owned copies synchronized so candidate snapshots stay independent.
    fn synchronize_current_source(&mut self) {
        let Some(current) = self.children.get(self.child_index) else {
            return;
        };
        let duplicates = self
            .children
            .iter()
            .enumerate()
            .filter_map(|(index, child)| {
                (index != self.child_index && child.collection_key == current.collection_key)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        if duplicates.is_empty() {
            return;
        }
        let enumerator = current.enumerator.clone();
        for index in duplicates {
            self.children[index].enumerator.clone_from(&enumerator);
        }
    }

    fn current_playback_order(&self) -> PlaybackOrder {
        self.current_child().map_or(PlaybackOrder::None, |child| {
            child.enumerator.playback_order()
        })
    }

    pub fn current(&self) -> Option<&SchedulingMediaItem> {
        self.current_child()
            .and_then(|child| child.enumerator.current())
    }

    fn select_with_smart_shuffle(
        &mut self,
        policy: &SchedulingSmartShufflePolicy,
        state: &SchedulingSmartShuffleState,
        scope_key: &str,
        scheduled_at: DateTime<Utc>,
        tags_by_media_id: &BTreeMap<EntityId, BTreeSet<String>>,
    ) {
        if let Some(child) = self.current_child_mut() {
            if child.play_all || child.count.is_some_and(|count| count > 1) {
                return;
            }
            child.enumerator.select_with_smart_shuffle(
                policy,
                state,
                scope_key,
                scheduled_at,
                tags_by_media_id,
            );
        }
        self.synchronize_current_source();
    }

    fn smart_shuffle_unit(
        &self,
        tags_by_media_id: &BTreeMap<EntityId, BTreeSet<String>>,
    ) -> Option<SchedulingSmartShuffleUnit> {
        let child = self.current_child()?;
        let grouped = child.play_all || child.count.is_some_and(|count| count > 1);
        if grouped && child.items_taken_from_current > 0 {
            return None;
        }
        let take = if child.play_all {
            child.enumerator.len()
        } else {
            child.count.unwrap_or(1).max(1)
        };
        let children = (0..take).filter_map(|offset| {
            let len = child.enumerator.items.len();
            let item = child
                .enumerator
                .items
                .get((child.enumerator.index + offset) % len)?;
            Some(SchedulingSmartShuffleUnit::single(
                item.id,
                tags_by_media_id.get(&item.id).cloned().unwrap_or_default(),
                item.duration,
            ))
        });
        Some(if grouped {
            SchedulingSmartShuffleUnit::grouped(children)
        } else {
            children.into_iter().next()?
        })
    }

    pub fn current_history_context(&self) -> CurrentHistoryContext {
        let Some(child) = self.current_child() else {
            return CurrentHistoryContext::Single { index: self.index };
        };
        let child_details = child
            .enumerator
            .current()
            .map(history_details_for_media_item)
            .unwrap_or_default();
        CurrentHistoryContext::Playlist {
            index: self.index,
            child_key: history_key_for_collection_key(&child.collection_key),
            child_index: child.enumerator.current_index(),
            child_details,
        }
    }

    pub fn history_for_item(
        &self,
        playout_id: EntityId,
        key: &str,
        item: &PlayoutItem,
    ) -> Vec<PlayoutHistory> {
        let Some(media) = self.current() else {
            return Vec::new();
        };
        let mut records = vec![history(
            playout_id,
            key,
            item,
            None,
            self.index,
            history_details_for_media_item(media),
            false,
        )];
        records[0].seed = self.shuffle_seed;
        if self
            .children
            .iter()
            .any(|child| child.enumerator.shuffle_seed.is_some())
        {
            // Child shuffles can wrap independently of the parent cycle. Replaying
            // with today's child seeds cannot reconstruct which IDs were consumed.
            let details = HistoryDetails {
                media_item_id: Some(media.id),
                release_date: media.release_date,
                season_number: media.season,
                episode_number: media.episode,
                playlist_cycle: Some(PlaylistCycleHistory {
                    all_media_ids: self.all_media_ids.clone(),
                    remaining_media_ids: self.remaining_media_ids.clone(),
                }),
            };
            records[0].details =
                serde_json::to_string(&details).expect("playlist history serializes");
        }
        for (position, child) in self.children.iter().enumerate() {
            let Some(current) = child.enumerator.current() else {
                continue;
            };
            records.push(history(
                playout_id,
                key,
                item,
                Some(history_key_for_collection_key(&child.collection_key)),
                child.enumerator.current_index(),
                history_details_for_media_item(current),
                position == self.child_index,
            ));
            records.last_mut().expect("child history was inserted").seed =
                child.enumerator.shuffle_seed;
        }
        records
    }

    pub fn advance(&mut self) {
        if let Some(id) = self.current().map(|media| media.id) {
            self.remaining_media_ids.remove(&id);
        }
        let Some(child) = self.current_child_mut() else {
            return;
        };
        child.enumerator.advance();
        child.items_taken_from_current = child.items_taken_from_current.saturating_add(1);

        let should_switch = if child.play_all {
            child.enumerator.current_index() == 0
        } else if let Some(count) = child.count {
            child.items_taken_from_current >= count
        } else {
            true
        };

        self.synchronize_current_source();

        if should_switch {
            self.child_index = (self.child_index + 1) % self.children.len();
            if let Some(next_child) = self.current_child_mut() {
                next_child.items_taken_from_current = 0;
            }
        }
        self.index = self.index.saturating_add(1);
        if self.remaining_media_ids.is_empty()
            && self.child_index == 0
            && self
                .children
                .first()
                .is_some_and(|child| child.enumerator.current_index() == 0)
        {
            self.index = 0;
            self.cycle = self.cycle.saturating_add(1);
            self.remaining_media_ids.clone_from(&self.all_media_ids);
            if let Some(seed) = self.shuffle_seed {
                self.shuffle_groups(classic::next_i32_seed(seed));
            }
        }
    }

    pub fn apply_history(&mut self, history: &[PlayoutHistory]) {
        let Some(primary) = history.iter().find(|history| history.child_key.is_none()) else {
            return;
        };
        // The primary index counts the whole playlist, including counted and
        // play-all batches. Replay once before reconciling individual children.
        if let Some(seed) = primary.seed {
            self.shuffle_groups(seed);
        }
        self.replay_to(usize::try_from(primary.index).unwrap_or_default());
        for child_history in history.iter().filter(|history| history.child_key.is_some()) {
            let Some(child_key) = child_history.child_key.as_deref() else {
                continue;
            };
            for child in self
                .children
                .iter_mut()
                .filter(|child| history_key_for_collection_key(&child.collection_key) == child_key)
            {
                if let Some(seed) = child_history.seed {
                    child.enumerator.restore_shuffle_seed(seed);
                }
                child
                    .enumerator
                    .set_index(usize::try_from(child_history.index).unwrap_or_default());
                if child.enumerator.playback_order == PlaybackOrder::Chronological {
                    child
                        .enumerator
                        .move_to_history_details(&child_history.details);
                }
            }
            if child_history.is_current_child
                && self.current_child().is_none_or(|child| {
                    history_key_for_collection_key(&child.collection_key) != child_key
                })
            {
                if let Some(index) = self.children.iter().position(|child| {
                    history_key_for_collection_key(&child.collection_key) == child_key
                }) {
                    self.child_index = index;
                    self.children[index].items_taken_from_current = 0;
                }
            }
        }
        // Saved children describe the position before the last emitted item.
        // Advance its active child exactly once after all restoration is done.
        if let Ok(details) = serde_json::from_str::<HistoryDetails>(&primary.details) {
            if let Some(cycle) = details.playlist_cycle {
                self.remaining_media_ids = cycle
                    .remaining_media_ids
                    .intersection(&self.all_media_ids)
                    .copied()
                    .chain(self.all_media_ids.difference(&cycle.all_media_ids).copied())
                    .collect();
            }
        }
        self.advance();
    }
}

/// Fresh, non-negative playout seed source, shared by playout creation and
/// every place that needs to give a playout a brand-new shuffle: nanosecond
/// time mixed through a `RandomState` hasher, masked into `i32`'s positive
/// range so downstream seed arithmetic never has to reason about sign.
#[must_use]
pub fn next_playout_seed() -> i32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(nanos);
    i32::try_from(hasher.finish() & i32::MAX as u64).unwrap_or_default()
}

/// Fisher-Yates shuffle with a simple LCG for deterministic ordering.
pub(super) fn shuffle_with_seed<T>(items: &mut [T], seed: u64) {
    let len = items.len();
    if len <= 1 {
        return;
    }
    let mut state = seed;
    for i in (1..len).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let j = (state >> 33) as usize % (i + 1);
        items.swap(i, j);
    }
}

pub(super) fn history_key_for_marathon_content(
    key: &str,
    item_order: PlaybackOrder,
    group_by: &str,
) -> String {
    serde_json::json!({
        "Key": key,
        "Order": format!("{:?}", PlaybackOrder::None),
        "ItemOrder": format!("{:?}", item_order),
        "GroupBy": group_by,
    })
    .to_string()
}
