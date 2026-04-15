mod block;
mod scripted;
mod yaml;

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use nastv_domain::{
    CollectionType, EntityId, PlaybackOrder, Playout, PlayoutAnchor, PlayoutGap, PlayoutHistory,
    PlayoutItem,
};
use serde::{Deserialize, Serialize};

use crate::repositories::Result as RepositoryResult;

pub use block::{BlockSchedulingEngine, EffectiveBlock, EffectiveBlockItem};
pub use scripted::ScriptedSchedulingEngine;
pub use yaml::{
    YamlInstruction, YamlPlayoutDefinition, YamlPlayoutEngine, YamlPlayoutSequenceItem,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayoutBuildMode {
    Refresh,
    Reset,
    #[default]
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingMediaItem {
    pub id: EntityId,
    pub duration: Duration,
    pub title: Option<String>,
    pub show_id: Option<EntityId>,
    pub season_id: Option<EntityId>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub release_date: Option<DateTime<Utc>>,
}

impl SchedulingMediaItem {
    #[must_use]
    pub fn new(id: EntityId, duration: Duration) -> Self {
        Self {
            id,
            duration,
            title: None,
            show_id: None,
            season_id: None,
            season: None,
            episode: None,
            release_date: None,
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

impl SchedulingContentSource {
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchedulingReferenceData {
    pub media_by_key: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub collections_by_name: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub multi_collections_by_name: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub smart_collections_by_name: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub playlists_by_group_and_name: BTreeMap<(Option<String>, String), Vec<SchedulingMediaItem>>,
    pub playlist_items_by_group_and_name:
        BTreeMap<(Option<String>, String), Vec<SchedulingPlaylistItem>>,
    pub searches_by_query: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub shows_by_name: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub media_by_show_guid: BTreeMap<String, Vec<SchedulingMediaItem>>,
    pub media_items_by_id: BTreeMap<EntityId, SchedulingMediaItem>,
    pub watermark_ids_by_name: BTreeMap<String, EntityId>,
    pub graphics_ids_by_path: BTreeMap<String, EntityId>,
    pub effective_blocks: Vec<EffectiveBlock>,
    pub existing_history: Vec<PlayoutHistory>,
    pub max_playout_offset: Duration,
}

impl SchedulingReferenceData {
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

        let mut groups: Vec<(SchedulingCollectionKey, Vec<SchedulingMediaItem>)> = Vec::new();
        for media_item in media_items {
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
                    count: None,
                    media_items: group_items,
                }
            })
            .collect()
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

fn marathon_collection_key(
    media_item: &SchedulingMediaItem,
    group_by: &str,
) -> Option<SchedulingCollectionKey> {
    match group_by.to_ascii_lowercase().as_str() {
        "show" => media_item.show_id.map(|show_id| SchedulingCollectionKey {
            collection_type: CollectionType::TelevisionShow,
            media_item_id: Some(show_id),
            ..SchedulingCollectionKey::default()
        }),
        "season" => media_item
            .season_id
            .map(|season_id| SchedulingCollectionKey {
                collection_type: CollectionType::TelevisionSeason,
                media_item_id: Some(season_id),
                ..SchedulingCollectionKey::default()
            }),
        _ => None,
    }
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
    pub clear_items: bool,
    pub remove_before: Option<DateTime<Utc>>,
    pub added_items: Vec<PlayoutItem>,
    pub added_gaps: Vec<PlayoutGap>,
    pub added_history: Vec<PlayoutHistory>,
    pub history_to_remove: Vec<EntityId>,
    pub items_to_remove: Vec<EntityId>,
    pub anchor: Option<PlayoutAnchor>,
    pub warnings: Vec<String>,
}

pub trait PlayoutSchedulingEngine {
    /// Build a playout timeline for the supplied request.
    ///
    /// # Errors
    ///
    /// Returns an error when the engine cannot load or evaluate its schedule
    /// definition.
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulingError {
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
    #[error("invalid duration: {0}")]
    InvalidDuration(String),
    #[error("invalid time: {0}")]
    InvalidTime(String),
    #[error("script error: {0}")]
    Script(String),
}

#[derive(Debug, Clone)]
struct MediaEnumerator {
    key: String,
    items: Vec<SchedulingMediaItem>,
    index: usize,
    playback_order: PlaybackOrder,
}

impl MediaEnumerator {
    fn new(key: impl Into<String>, items: Vec<SchedulingMediaItem>) -> Self {
        Self {
            key: key.into(),
            items,
            index: 0,
            playback_order: PlaybackOrder::None,
        }
    }

    fn with_playback_order(mut self, playback_order: PlaybackOrder) -> Self {
        self.playback_order = playback_order;
        self
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

    fn set_index(&mut self, index: usize) {
        self.index = if self.items.is_empty() {
            0
        } else {
            index % self.items.len()
        };
    }

    fn advance(&mut self) {
        if !self.items.is_empty() {
            self.index = (self.index + 1) % self.items.len();
        }
    }

    fn current_index(&self) -> usize {
        if self.items.is_empty() {
            0
        } else {
            self.index % self.items.len()
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
            return self.items.iter().position(|item| item.id == media_item_id);
        }
        if let (Some(season), Some(episode)) = (details.season_number, details.episode_number) {
            return self
                .items
                .iter()
                .position(|item| item.season == Some(season) && item.episode == Some(episode));
        }
        details.release_date.and_then(|release_date| {
            self.items
                .iter()
                .position(|item| item.release_date == Some(release_date))
        })
    }
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
}

fn history_details_for_media_item(media_item: &SchedulingMediaItem) -> String {
    let details = HistoryDetails {
        media_item_id: Some(media_item.id),
        release_date: media_item.release_date,
        season_number: media_item.season,
        episode_number: media_item.episode,
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
    filler_kind: nastv_domain::FillerKind,
    custom_title: Option<String>,
    disable_watermarks: bool,
    watermark_ids: Vec<EntityId>,
    graphics_element_ids: Vec<EntityId>,
    scheduling_context: Option<String>,
) -> PlayoutItem {
    PlayoutItem {
        playout_id,
        media_item_id: media_item.id,
        start,
        finish: start + duration,
        guide_group,
        in_point: Duration::zero(),
        out_point: duration,
        filler_kind,
        custom_title,
        disable_watermarks,
        watermark_ids,
        graphics_element_ids,
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
