use crate::repositories::SqlxResultExt as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
};

use crate::db::DbRow;
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use ersatzrs_core::repositories::{
    AdminSearchEnrichment, AdminSearchKind, AdminSearchRepository, ProgramScheduleRepository,
    RepositoryError, Result,
};
use ersatzrs_core::scheduling::{
    block_item_smart_shuffle_config_key, classic_schedule_item_key,
    classic_schedule_item_policy_key, legacy_playout_cooldown_config_key,
    legacy_program_schedule_cooldown_config_key, program_schedule_item_smart_shuffle_config_key,
    BlockDefinition, BlockPreviewRepository, BlockTemplate, BlockTemplateItem,
    BlockTemplateSchedule, EffectiveBlock, EffectiveBlockItem, EffectiveBreakContent,
    EffectiveDeco, EffectiveDecoSelection, EffectiveDecoTemplateItem, SchedulingCollectionKey,
    SchedulingContentRepository, SchedulingContentSource, SchedulingFallbackPreset,
    SchedulingFillerPreset, SchedulingMediaItem, SchedulingPlaylistItem, SchedulingReferenceData,
    SchedulingSmartShufflePolicy, SchedulingSmartShuffleRuleKind,
};
use ersatzrs_core::search::{
    search_page_map, SearchDocument, SearchExpression, SearchField, SearchPageMap,
    SearchProviderGuid,
};
use ersatzrs_domain::{
    normalize_sub_kind_token, ChannelSubtitleMode, CollectionEnumeratorState, CollectionType,
    DecoMode, EntityId, PlayoutHistory, PlayoutItem, PlayoutProgramScheduleAnchor,
    PlayoutScheduleItemFillGroupIndex, ProgramScheduleAlternate, ProgramScheduleItem,
    CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID, CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
};
use sqlx::{Pool, Row};

use super::common::{
    decode_bool_column, decode_datetime, decode_duration_text, decode_enum_column, decode_message,
    decode_optional_datetime, decode_optional_duration_text, decode_optional_enum_column, load_ids,
};

async fn fetch_admin_search_rows(
    pool: &Pool<crate::db::Db>,
    sql: &str,
    ids: &[EntityId],
) -> Result<Vec<DbRow>> {
    let mut query = sqlx::query(crate::db::assert_sql_safe(sql));
    for id in ids {
        query = query.bind(*id);
    }
    query.fetch_all(pool).await.repository()
}

fn admin_search_state(value: i32) -> Result<ersatzrs_domain::MediaItemState> {
    match value {
        0 => Ok(ersatzrs_domain::MediaItemState::Normal),
        1 => Ok(ersatzrs_domain::MediaItemState::FileNotFound),
        2 => Ok(ersatzrs_domain::MediaItemState::Unavailable),
        3 => Ok(ersatzrs_domain::MediaItemState::RemoteOnly),
        value => Err(RepositoryError::Decode {
            kind: "media item state",
            value: value.to_string(),
        }),
    }
}

#[derive(Clone)]
pub struct SqliteSchedulingContentRepository(
    pub Pool<crate::db::Db>,
    /// Memoized `playout.skip_missing_items` config flag, resolved on first use.
    tokio::sync::OnceCell<bool>,
);

impl SqliteSchedulingContentRepository {
    /// Construct the repository over a database pool.
    #[must_use]
    pub fn new(pool: Pool<crate::db::Db>) -> Self {
        Self(pool, tokio::sync::OnceCell::new())
    }

    /// Resolve and memoize the `playout.skip_missing_items` config flag
    /// (default `false`). Mirrors ErsatzTV's `PlayoutSkipMissingItems` gate in
    /// `PlayoutBuilder`: when enabled, media items flagged `FileNotFound` or
    /// `Unavailable` are excluded from scheduling reference data.
    async fn skip_missing_items(&self) -> Result<bool> {
        let flag = self
            .1
            .get_or_try_init(|| async {
                let value = sqlx::query_scalar::<_, String>(
                    r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
                )
                .bind(ersatzrs_domain::CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS)
                .fetch_optional(&self.0)
                .await
                .repository()?;
                Ok::<bool, ersatzrs_core::repositories::RepositoryError>(matches!(
                    value.as_deref().map(str::trim),
                    Some("True" | "true" | "1" | "on")
                ))
            })
            .await?;
        Ok(*flag)
    }
}

const SEARCH_NEED_GENRES: u64 = 1 << 0;
const SEARCH_NEED_TAGS: u64 = 1 << 1;
const SEARCH_NEED_STUDIOS: u64 = 1 << 2;
const SEARCH_NEED_ACTORS: u64 = 1 << 3;
const SEARCH_NEED_DIRECTORS: u64 = 1 << 4;
const SEARCH_NEED_WRITERS: u64 = 1 << 5;
const SEARCH_NEED_LANGUAGES: u64 = 1 << 6;
const SEARCH_NEED_LANGUAGE_TAGS: u64 = 1 << 7;
const SEARCH_NEED_SUBTITLE_LANGUAGES: u64 = 1 << 8;
const SEARCH_NEED_SUBTITLE_LANGUAGE_TAGS: u64 = 1 << 9;
const SEARCH_NEED_VIDEO_CODECS: u64 = 1 << 10;
const SEARCH_NEED_AUDIO_CODECS: u64 = 1 << 11;
const SEARCH_NEED_SUBTITLE_CODECS: u64 = 1 << 12;
const SEARCH_NEED_WIDTH: u64 = 1 << 13;
const SEARCH_NEED_HEIGHT: u64 = 1 << 14;
const SEARCH_NEED_CHAPTERS: u64 = 1 << 15;
const SEARCH_NEED_DURATIONS: u64 = 1 << 16;
const SEARCH_NEED_SHOW_GENRES: u64 = 1 << 17;
const SEARCH_NEED_SHOW_TAGS: u64 = 1 << 18;
const SEARCH_NEED_SHOW_STUDIOS: u64 = 1 << 19;
const SEARCH_NEED_TRAKT_LISTS: u64 = 1 << 20;
const SEARCH_NEED_PROVIDER_GUIDS: u64 = 1 << 21;
const SEARCH_NEED_COLLECTIONS: u64 = 1 << 22;
const SEARCH_NEED_NETWORKS: u64 = 1 << 23;
const SEARCH_NEED_VIDEO_BIT_DEPTHS: u64 = 1 << 24;
const SEARCH_NEED_VIDEO_DYNAMIC_RANGES: u64 = 1 << 25;
const SEARCH_NEED_LIBRARY_FOLDER_IDS: u64 = 1 << 26;
const SEARCH_NEED_CONTENT_RATINGS: u64 = 1 << 27;
const SEARCH_NEED_SHOW_NETWORKS: u64 = 1 << 28;
const SEARCH_NEED_SHOW_CONTENT_RATINGS: u64 = 1 << 29;
const SEARCH_NEED_STYLES: u64 = 1 << 30;
const SEARCH_NEED_MOODS: u64 = 1 << 31;
const SEARCH_NEED_ALBUMS: u64 = 1 << 32;
const SEARCH_NEED_ARTISTS: u64 = 1 << 33;
const SEARCH_NEED_ALBUM_ARTISTS: u64 = 1 << 34;
const SEARCH_NEED_PARENT_TITLE: u64 = 1 << 35;
const SEARCH_NEED_AUDIO_TITLES: u64 = 1 << 36;
const SEARCH_NEED_SUBTITLE_TITLES: u64 = 1 << 37;
const SEARCH_NEED_MUSIC_LABELS: u64 = 1 << 38;

const SEARCH_NEED_STREAM_FIELDS: u64 = SEARCH_NEED_LANGUAGES
    | SEARCH_NEED_LANGUAGE_TAGS
    | SEARCH_NEED_SUBTITLE_LANGUAGES
    | SEARCH_NEED_SUBTITLE_LANGUAGE_TAGS
    | SEARCH_NEED_VIDEO_CODECS
    | SEARCH_NEED_AUDIO_CODECS
    | SEARCH_NEED_SUBTITLE_CODECS
    | SEARCH_NEED_WIDTH
    | SEARCH_NEED_HEIGHT
    | SEARCH_NEED_CHAPTERS
    | SEARCH_NEED_DURATIONS
    | SEARCH_NEED_VIDEO_BIT_DEPTHS
    | SEARCH_NEED_VIDEO_DYNAMIC_RANGES
    | SEARCH_NEED_LIBRARY_FOLDER_IDS
    | SEARCH_NEED_AUDIO_TITLES
    | SEARCH_NEED_SUBTITLE_TITLES;

#[derive(Clone, Copy, Debug, Default)]
struct SearchHydrationNeeds {
    flags: u64,
}

impl SearchHydrationNeeds {
    fn for_expression(expression: &SearchExpression) -> Self {
        let mut needs = Self::default();
        needs.visit_expression(expression);
        needs
    }

    fn insert(&mut self, flag: u64) {
        self.flags |= flag;
    }

    fn contains(self, flag: u64) -> bool {
        self.flags & flag != 0
    }

    fn visit_expression(&mut self, expression: &SearchExpression) {
        match expression {
            SearchExpression::MatchAll => {}
            SearchExpression::Term(term) => self.visit_field(&term.field),
            SearchExpression::And(expressions) | SearchExpression::Or(expressions) => {
                for expression in expressions {
                    self.visit_expression(expression);
                }
            }
            SearchExpression::Not(expression) => self.visit_expression(expression),
        }
    }

    fn visit_field(&mut self, field: &SearchField) {
        match field {
            SearchField::Default
            | SearchField::Title
            | SearchField::SortTitle
            | SearchField::OriginalTitle
            | SearchField::Year
            | SearchField::ReleaseDate
            | SearchField::AddedDate
            | SearchField::MediaKind
            | SearchField::LibraryName
            | SearchField::ShowTitle
            | SearchField::SeasonNumber
            | SearchField::EpisodeNumber
            | SearchField::MetadataKind
            | SearchField::LibraryId
            | SearchField::ContentKind
            | SearchField::SubKind
            | SearchField::ReleasedOnThisDay
            | SearchField::ReleasedInTheLast
            | SearchField::ReleasedNotInTheLast
            | SearchField::AddedInTheLast
            | SearchField::AddedNotInTheLast => {}
            SearchField::Genre => self.insert(SEARCH_NEED_GENRES),
            SearchField::Tag => self.insert(SEARCH_NEED_TAGS),
            SearchField::Collection => self.insert(SEARCH_NEED_COLLECTIONS),
            SearchField::Studio => self.insert(SEARCH_NEED_STUDIOS),
            SearchField::MusicLabel => self.insert(SEARCH_NEED_MUSIC_LABELS),
            SearchField::Network => self.insert(SEARCH_NEED_NETWORKS),
            SearchField::Actor => self.insert(SEARCH_NEED_ACTORS),
            SearchField::Director => self.insert(SEARCH_NEED_DIRECTORS),
            SearchField::Writer => self.insert(SEARCH_NEED_WRITERS),
            SearchField::Language => self.insert(SEARCH_NEED_LANGUAGES),
            SearchField::LanguageTag => self.insert(SEARCH_NEED_LANGUAGE_TAGS),
            SearchField::SubtitleLanguage => self.insert(SEARCH_NEED_SUBTITLE_LANGUAGES),
            SearchField::SubtitleLanguageTag => self.insert(SEARCH_NEED_SUBTITLE_LANGUAGE_TAGS),
            SearchField::VideoCodec => self.insert(SEARCH_NEED_VIDEO_CODECS),
            SearchField::VideoBitDepth => self.insert(SEARCH_NEED_VIDEO_BIT_DEPTHS),
            SearchField::VideoDynamicRange => self.insert(SEARCH_NEED_VIDEO_DYNAMIC_RANGES),
            SearchField::AudioCodec => self.insert(SEARCH_NEED_AUDIO_CODECS),
            SearchField::AudioTitle => self.insert(SEARCH_NEED_AUDIO_TITLES),
            SearchField::SubtitleCodec => self.insert(SEARCH_NEED_SUBTITLE_CODECS),
            SearchField::SubtitleTitle => self.insert(SEARCH_NEED_SUBTITLE_TITLES),
            SearchField::HasCommentary => {
                self.insert(SEARCH_NEED_AUDIO_TITLES);
                self.insert(SEARCH_NEED_SUBTITLE_TITLES);
            }
            SearchField::Width => self.insert(SEARCH_NEED_WIDTH),
            SearchField::Height => self.insert(SEARCH_NEED_HEIGHT),
            SearchField::Chapters => self.insert(SEARCH_NEED_CHAPTERS),
            SearchField::Minutes | SearchField::Seconds => self.insert(SEARCH_NEED_DURATIONS),
            SearchField::LibraryFolderId => self.insert(SEARCH_NEED_LIBRARY_FOLDER_IDS),
            SearchField::ParentTitle => self.insert(SEARCH_NEED_PARENT_TITLE),
            SearchField::ContentRating => self.insert(SEARCH_NEED_CONTENT_RATINGS),
            SearchField::ShowGenre => self.insert(SEARCH_NEED_SHOW_GENRES),
            SearchField::ShowTag => self.insert(SEARCH_NEED_SHOW_TAGS),
            SearchField::ShowStudio => self.insert(SEARCH_NEED_SHOW_STUDIOS),
            SearchField::ShowNetwork => self.insert(SEARCH_NEED_SHOW_NETWORKS),
            SearchField::ShowContentRating => self.insert(SEARCH_NEED_SHOW_CONTENT_RATINGS),
            SearchField::TraktList => self.insert(SEARCH_NEED_TRAKT_LISTS),
            SearchField::Style => self.insert(SEARCH_NEED_STYLES),
            SearchField::Mood => self.insert(SEARCH_NEED_MOODS),
            SearchField::Album => self.insert(SEARCH_NEED_ALBUMS),
            SearchField::Artist => self.insert(SEARCH_NEED_ARTISTS),
            SearchField::AlbumArtist => self.insert(SEARCH_NEED_ALBUM_ARTISTS),
            SearchField::ProviderGuid(_) => self.insert(SEARCH_NEED_PROVIDER_GUIDS),
        }
    }
}

#[async_trait]
impl SchedulingContentRepository for SqliteSchedulingContentRepository {
    async fn media_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Vec<SchedulingMediaItem>> {
        self.media_items_for_source(source).await
    }

    async fn playlist_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Option<Vec<SchedulingPlaylistItem>>> {
        self.playlist_items_for_source(source).await
    }

    async fn watermark_id_by_name(&self, name: &str) -> Result<Option<EntityId>> {
        self.watermark_id_by_name(name).await
    }

    async fn graphics_element_id_by_path(&self, path: &str) -> Result<Option<EntityId>> {
        self.graphics_element_id_by_path(path).await
    }
}

#[async_trait]
impl BlockPreviewRepository for SqliteSchedulingContentRepository {
    async fn load_block_preview_content(
        &self,
        items: &[EffectiveBlockItem],
        policies: &BTreeMap<String, SchedulingSmartShufflePolicy>,
    ) -> Result<SchedulingReferenceData> {
        let mut reference_data = SchedulingReferenceData::default();
        let mut target_ids = BTreeMap::new();
        let mut seen = BTreeSet::new();

        for item in items {
            if !seen.insert(item.id) {
                continue;
            }

            let media_items = self
                .items_for_collection_key(&item.collection_key, 0)
                .await?;
            target_ids.insert(
                item.id.to_string(),
                media_items.iter().map(|media| media.id).collect(),
            );
            for media in &media_items {
                reference_data
                    .media_items_by_id
                    .entry(media.id)
                    .or_insert_with(|| media.clone());
            }
            reference_data
                .media_by_key
                .insert(item.content.clone(), media_items);
        }

        self.with_smart_shuffle_policies(
            reference_data,
            SchedulingSmartShufflePolicy::default(),
            policies.clone(),
            target_ids,
        )
        .await
    }
}

impl SqliteSchedulingContentRepository {
    /// Prepare SQL-backed reference data for Classic program scheduling.
    ///
    /// # Errors
    ///
    /// Returns an error when the playout has no program schedule, when the
    /// schedule cannot be loaded, or when any `SQLite` query fails.
    pub async fn classic_reference_data(
        &self,
        playout_id: EntityId,
        start: DateTime<Utc>,
        max_playout_offset: Duration,
    ) -> Result<SchedulingReferenceData> {
        let default_schedule_id = sqlx::query_scalar::<_, Option<EntityId>>(
            r#"SELECT "ProgramScheduleId" FROM "Playout" WHERE "Id" = ?"#,
        )
        .bind(playout_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .flatten()
        .ok_or_else(|| {
            super::common::decode_message("classic playout", "missing program schedule")
        })?;

        let schedule_repo =
            super::program_schedule::SqliteProgramScheduleRepository(self.0.clone());
        let schedule_id = self
            .active_classic_schedule_id(playout_id, default_schedule_id, start)
            .await?;
        let schedule = schedule_repo.get_with_items(schedule_id).await?;
        let schedule_items = schedule_repo.list_items(schedule_id).await?;

        let mut media_by_key = BTreeMap::new();
        for item in &schedule_items {
            media_by_key.insert(
                classic_schedule_item_key(item),
                self.items_for_program_schedule_item(item).await?,
            );
        }
        let mut classic_filler_presets = BTreeMap::new();
        for filler_id in schedule_items
            .iter()
            .flat_map(program_schedule_item_filler_ids)
            .collect::<BTreeSet<_>>()
        {
            if let Some(preset) = self.scheduling_filler_preset(filler_id).await? {
                classic_filler_presets.insert(filler_id, preset);
            }
        }

        let reference_data = self
            .with_configured_playout_options(SchedulingReferenceData {
                media_by_key,
                classic_schedule_id: Some(schedule.id),
                classic_schedule_name: Some(schedule.name),
                classic_schedule_items: schedule_items,
                classic_shuffle_schedule_items: schedule.shuffle_schedule_items,
                classic_random_start_point: schedule.random_start_point,
                classic_fixed_start_time_behavior: schedule.fixed_start_time_behavior,
                existing_program_schedule_anchors: self
                    .program_schedule_anchors(playout_id)
                    .await?,
                existing_fill_group_indices: self.fill_group_indices(playout_id).await?,
                existing_items: self.playout_items(playout_id).await?,
                existing_history: self.playout_history(playout_id).await?,
                max_playout_offset,
                classic_filler_presets,
                ..SchedulingReferenceData::default()
            })
            .await?;
        let legacy_policy = self
            .load_smart_shuffle_policy(&legacy_program_schedule_cooldown_config_key(schedule.id))
            .await?;
        let mut policies = BTreeMap::new();
        let mut target_ids = BTreeMap::new();
        for item in &reference_data.classic_schedule_items {
            let content_key = classic_schedule_item_key(item);
            let policy_key = classic_schedule_item_policy_key(item);
            let policy = self
                .load_optional_smart_shuffle_policy(
                    &program_schedule_item_smart_shuffle_config_key(item.id),
                )
                .await?
                .unwrap_or_else(|| legacy_policy.clone());
            policies.insert(policy_key.clone(), policy);
            target_ids.insert(
                policy_key,
                reference_data
                    .media_by_key
                    .get(&content_key)
                    .into_iter()
                    .flatten()
                    .map(|media| media.id)
                    .collect(),
            );
        }
        self.with_smart_shuffle_policies(reference_data, legacy_policy, policies, target_ids)
            .await
    }

    async fn active_classic_schedule_id(
        &self,
        playout_id: EntityId,
        default_schedule_id: EntityId,
        start: DateTime<Utc>,
    ) -> Result<EntityId> {
        let alternates = self.program_schedule_alternates(playout_id).await?;
        Ok(alternates
            .iter()
            .find(|alternate| alternate_applies_to(alternate, start))
            .map_or(default_schedule_id, |alternate| {
                alternate.program_schedule_id
            }))
    }

    async fn program_schedule_alternates(
        &self,
        playout_id: EntityId,
    ) -> Result<Vec<ProgramScheduleAlternate>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "ProgramScheduleAlternate"
             WHERE "PlayoutId" = ?
             ORDER BY "Index", "Id""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        rows.iter().map(map_program_schedule_alternate).collect()
    }

    /// Prepare SQL-backed reference data for block scheduling.
    ///
    /// # Errors
    ///
    /// Returns an error when a `SQLite` query fails or a persisted enum/date
    /// value cannot be decoded.
    pub async fn block_reference_data(
        &self,
        playout_id: EntityId,
        start: DateTime<Utc>,
        days_to_build: i64,
        max_playout_offset: Duration,
    ) -> Result<SchedulingReferenceData> {
        let mut media_by_key = BTreeMap::new();
        let schedules = self
            .block_template_schedules(playout_id, &mut media_by_key)
            .await?;
        let effective_blocks = EffectiveBlock::from_templates(&schedules, start, days_to_build);

        let reference_data = self
            .with_configured_playout_options(SchedulingReferenceData {
                media_by_key,
                effective_blocks,
                existing_items: self.playout_items(playout_id).await?,
                existing_history: self.playout_history(playout_id).await?,
                max_playout_offset,
                ..SchedulingReferenceData::default()
            })
            .await?;
        let legacy_policy = self
            .load_smart_shuffle_policy(&legacy_playout_cooldown_config_key(playout_id))
            .await?;
        let mut policies = BTreeMap::new();
        let mut target_ids = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for item in reference_data
            .effective_blocks
            .iter()
            .flat_map(|block| &block.items)
            .filter(|item| seen.insert(item.id))
        {
            let unit_key = item.id.to_string();
            let policy = self
                .load_optional_smart_shuffle_policy(&block_item_smart_shuffle_config_key(item.id))
                .await?
                .unwrap_or_else(|| legacy_policy.clone());
            policies.insert(unit_key.clone(), policy);
            target_ids.insert(
                unit_key,
                reference_data
                    .media_by_key
                    .get(&item.content)
                    .into_iter()
                    .flatten()
                    .map(|media| media.id)
                    .collect(),
            );
        }
        self.with_smart_shuffle_policies(reference_data, legacy_policy, policies, target_ids)
            .await
    }

    /// Prepare SQL-backed reference data for a YAML/Sequential scripted
    /// playout.  Parses the inline YAML content, loads media items for every
    /// content section declared in the definition, and also loads existing
    /// playout items and history so the engine can apply cleanup and resume
    /// semantics correctly.
    ///
    /// # Errors
    ///
    /// Returns an error when the YAML is structurally invalid or any `SQLite`
    /// query fails.
    pub async fn yaml_reference_data(
        &self,
        playout_id: EntityId,
        yaml_content: &str,
        max_playout_offset: Duration,
    ) -> Result<SchedulingReferenceData> {
        use ersatzrs_core::scheduling::YamlPlayoutEngine;

        let definition = YamlPlayoutEngine::definition_from_str(yaml_content).map_err(|e| {
            ersatzrs_core::repositories::RepositoryError::Decode {
                kind: "yaml",
                value: e.to_string(),
            }
        })?;

        let mut media_by_key = BTreeMap::new();
        for item in &definition.content {
            let source = item.source();
            let items = self.media_items_for_source(&source).await?;
            media_by_key.insert(item.key.clone(), items);
        }

        let reference_data = self
            .with_configured_playout_options(SchedulingReferenceData {
                media_by_key,
                existing_items: self.playout_items(playout_id).await?,
                existing_history: self.playout_history(playout_id).await?,
                max_playout_offset,
                ..SchedulingReferenceData::default()
            })
            .await?;
        let legacy_policy = definition.legacy_smart_shuffle;
        let mut policies = BTreeMap::new();
        let mut target_ids = BTreeMap::new();
        for item in definition.content {
            let policy = if item.smart_shuffle.is_disabled() {
                legacy_policy.clone()
            } else {
                item.smart_shuffle
            };
            target_ids.insert(
                item.key.clone(),
                reference_data
                    .media_by_key
                    .get(&item.key)
                    .into_iter()
                    .flatten()
                    .map(|media| media.id)
                    .collect(),
            );
            policies.insert(item.key, policy);
        }
        self.with_smart_shuffle_policies(reference_data, legacy_policy, policies, target_ids)
            .await
    }

    /// Load only persisted playout-wide config reference data.
    ///
    /// This is used by schedule kinds whose engine does not need SQL-backed
    /// content expansion for its own instructions, but still participates in
    /// global watermark/fallback behavior.
    ///
    /// # Errors
    ///
    /// Returns an error when config or fallback filler queries fail.
    pub async fn configured_playout_options(&self) -> Result<SchedulingReferenceData> {
        self.with_configured_playout_options(SchedulingReferenceData::default())
            .await
    }

    /// Prepare Scripted reference data, using an in-file policy declaration
    /// ahead of the additive playout attachment when one is present.
    ///
    /// # Errors
    ///
    /// Returns an error when configured options, the policy attachment, or
    /// similarity-tag hydration cannot be loaded.
    pub async fn scripted_reference_data(
        &self,
        playout_id: EntityId,
        definition_policies: BTreeMap<String, SchedulingSmartShufflePolicy>,
        definition_legacy_policy: Option<SchedulingSmartShufflePolicy>,
    ) -> Result<SchedulingReferenceData> {
        let reference_data = self.configured_playout_options().await?;
        let legacy_policy = if let Some(policy) = definition_legacy_policy {
            policy
        } else {
            self.load_smart_shuffle_policy(&legacy_playout_cooldown_config_key(playout_id))
                .await?
        };
        self.with_smart_shuffle_policies(
            reference_data,
            legacy_policy,
            definition_policies,
            BTreeMap::new(),
        )
        .await
    }

    async fn with_configured_playout_options(
        &self,
        mut reference_data: SchedulingReferenceData,
    ) -> Result<SchedulingReferenceData> {
        reference_data.global_watermark_id = self
            .load_config_entity_id(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID)
            .await?;
        reference_data.global_fallback = self.global_fallback_preset().await?;
        Ok(reference_data)
    }

    async fn load_smart_shuffle_policy(&self, key: &str) -> Result<SchedulingSmartShufflePolicy> {
        Ok(self
            .load_optional_smart_shuffle_policy(key)
            .await?
            .unwrap_or_default())
    }

    async fn load_optional_smart_shuffle_policy(
        &self,
        key: &str,
    ) -> Result<Option<SchedulingSmartShufflePolicy>> {
        let value = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(key)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        value
            .map(|json| {
                serde_json::from_str(&json).map_err(|error| {
                    ersatzrs_core::repositories::RepositoryError::Decode {
                        kind: "scheduling Smart Shuffle policy",
                        value: error.to_string(),
                    }
                })
            })
            .transpose()
    }

    async fn with_smart_shuffle_policies(
        &self,
        mut reference_data: SchedulingReferenceData,
        legacy_policy: SchedulingSmartShufflePolicy,
        policies: BTreeMap<String, SchedulingSmartShufflePolicy>,
        target_ids: BTreeMap<String, BTreeSet<EntityId>>,
    ) -> Result<SchedulingReferenceData> {
        reference_data.smart_shuffle_policy = legacy_policy;
        reference_data.smart_shuffle_policies_by_unit = policies;
        for (unit_key, policy) in &reference_data.smart_shuffle_policies_by_unit {
            if !policy.is_active() {
                continue;
            }
            let tags = self
                .smart_shuffle_tags_for_policy(
                    policy,
                    target_ids.get(unit_key).cloned().unwrap_or_default(),
                    Some(unit_key),
                )
                .await?;
            reference_data
                .smart_shuffle_tags_by_unit
                .insert(unit_key.clone(), tags);
        }
        if reference_data.smart_shuffle_policy.is_active() {
            reference_data.smart_shuffle_tags_by_media_id = self
                .smart_shuffle_tags_for_policy(
                    &reference_data.smart_shuffle_policy,
                    reference_data_media_ids(&reference_data),
                    None,
                )
                .await?;
        }
        Ok(reference_data)
    }

    #[allow(clippy::too_many_lines)]
    async fn smart_shuffle_tags_for_policy(
        &self,
        policy: &SchedulingSmartShufflePolicy,
        target_ids: BTreeSet<EntityId>,
        filter_namespace: Option<&str>,
    ) -> Result<BTreeMap<EntityId, BTreeSet<String>>> {
        let mut hydration = SearchHydrationNeeds::default();
        let mut filters = Vec::new();
        for rule in &policy.rules {
            match rule.kind {
                SchedulingSmartShuffleRuleKind::Artist => hydration.insert(SEARCH_NEED_ARTISTS),
                SchedulingSmartShuffleRuleKind::AlbumArtist => {
                    hydration.insert(SEARCH_NEED_ALBUM_ARTISTS);
                }
                SchedulingSmartShuffleRuleKind::Collection => {
                    hydration.insert(SEARCH_NEED_COLLECTIONS);
                }
                SchedulingSmartShuffleRuleKind::Tag => hydration.insert(SEARCH_NEED_TAGS),
                SchedulingSmartShuffleRuleKind::Genre => hydration.insert(SEARCH_NEED_GENRES),
                SchedulingSmartShuffleRuleKind::Studio => hydration.insert(SEARCH_NEED_STUDIOS),
                SchedulingSmartShuffleRuleKind::ContentRating => {
                    hydration.insert(SEARCH_NEED_CONTENT_RATINGS);
                }
                SchedulingSmartShuffleRuleKind::Language => {
                    hydration.insert(SEARCH_NEED_LANGUAGE_TAGS);
                }
                SchedulingSmartShuffleRuleKind::MusicLabel => {
                    hydration.insert(SEARCH_NEED_MUSIC_LABELS);
                }
                SchedulingSmartShuffleRuleKind::Filter => {
                    if let Some(search) = rule.search.as_deref() {
                        let expression = SearchExpression::parse(search);
                        hydration.visit_expression(&expression);
                        filters.push((rule.id.as_str(), expression));
                    }
                }
                SchedulingSmartShuffleRuleKind::Show
                | SchedulingSmartShuffleRuleKind::Season
                | SchedulingSmartShuffleRuleKind::ContentParent => {}
            }
        }

        let documents = self.search_documents(&hydration).await?;
        let mut result = BTreeMap::<EntityId, BTreeSet<String>>::new();
        for document in documents.iter().filter(|document| {
            target_ids.is_empty() || target_ids.contains(&document.media_item_id)
        }) {
            let tags = result.entry(document.media_item_id).or_default();
            for rule in &policy.rules {
                match rule.kind {
                    SchedulingSmartShuffleRuleKind::Artist => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.artists);
                    }
                    SchedulingSmartShuffleRuleKind::AlbumArtist => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.album_artists);
                    }
                    SchedulingSmartShuffleRuleKind::Show => {
                        extend_optional_smart_shuffle_tag(
                            tags,
                            &rule.id,
                            document.show_title.as_deref(),
                        );
                    }
                    SchedulingSmartShuffleRuleKind::Season => {
                        if let (Some(show), Some(season)) =
                            (document.show_title.as_deref(), document.season_number)
                        {
                            tags.insert(format!(
                                "{}:{}:{}",
                                rule.id,
                                normalize_smart_shuffle_value(show),
                                season
                            ));
                        }
                    }
                    SchedulingSmartShuffleRuleKind::ContentParent => {
                        let root = document
                            .parent_media_item_id
                            .unwrap_or(document.media_item_id);
                        tags.insert(format!("{}:{root}", rule.id));
                    }
                    SchedulingSmartShuffleRuleKind::Collection => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.collections);
                    }
                    SchedulingSmartShuffleRuleKind::Tag => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.tags);
                    }
                    SchedulingSmartShuffleRuleKind::Genre => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.genres);
                    }
                    SchedulingSmartShuffleRuleKind::Studio => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.studios);
                    }
                    SchedulingSmartShuffleRuleKind::ContentRating => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.content_ratings);
                    }
                    SchedulingSmartShuffleRuleKind::Language => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.language_tags);
                    }
                    SchedulingSmartShuffleRuleKind::MusicLabel => {
                        extend_smart_shuffle_tags(tags, &rule.id, &document.music_labels);
                    }
                    SchedulingSmartShuffleRuleKind::Filter => {
                        if filters.iter().any(|(id, expression)| {
                            *id == rule.id.as_str() && expression.matches(document)
                        }) {
                            tags.insert(filter_namespace.map_or_else(
                                || format!("filter:{}", rule.id),
                                |namespace| format!("filter:{namespace}:{}", rule.id),
                            ));
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    async fn global_fallback_preset(&self) -> Result<Option<SchedulingFallbackPreset>> {
        let Some(filler_id) = self
            .load_config_entity_id(CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID)
            .await?
        else {
            return Ok(None);
        };

        let row = sqlx::query(r#"SELECT * FROM "FillerPreset" WHERE "Id" = ?"#)
            .bind(filler_id)
            .fetch_optional(&self.0)
            .await
            .repository()?;

        let Some(row) = row else {
            return Ok(None);
        };

        let collection_key = SchedulingCollectionKey {
            collection_type: decode_enum_column(
                "global fallback filler collection_type",
                &row,
                "CollectionType",
            )?,
            collection_id: row.try_get("CollectionId").repository()?,
            media_item_id: row.try_get("MediaItemId").repository()?,
            multi_collection_id: row.try_get("MultiCollectionId").repository()?,
            smart_collection_id: row.try_get("SmartCollectionId").repository()?,
            playlist_id: row.try_get("PlaylistId").repository()?,
            ..SchedulingCollectionKey::default()
        };

        let media_items = self.items_for_collection_key(&collection_key, 0).await?;
        Ok(Some(SchedulingFallbackPreset {
            id: filler_id,
            allow_watermarks: row.try_get("AllowWatermarks").repository()?,
            media_items,
        }))
    }

    async fn scheduling_filler_preset(
        &self,
        filler_id: EntityId,
    ) -> Result<Option<SchedulingFillerPreset>> {
        let row = sqlx::query(r#"SELECT * FROM "FillerPreset" WHERE "Id" = ?"#)
            .bind(filler_id)
            .fetch_optional(&self.0)
            .await
            .repository()?;

        let Some(row) = row else {
            return Ok(None);
        };

        let collection_key = SchedulingCollectionKey {
            collection_type: decode_enum_column(
                "schedule item filler collection_type",
                &row,
                "CollectionType",
            )?,
            collection_id: row.try_get("CollectionId").repository()?,
            media_item_id: row.try_get("MediaItemId").repository()?,
            multi_collection_id: row.try_get("MultiCollectionId").repository()?,
            smart_collection_id: row.try_get("SmartCollectionId").repository()?,
            playlist_id: row.try_get("PlaylistId").repository()?,
            ..SchedulingCollectionKey::default()
        };
        let media_items = self.items_for_collection_key(&collection_key, 0).await?;

        Ok(Some(SchedulingFillerPreset {
            id: filler_id,
            filler_kind: decode_enum_column("schedule item filler kind", &row, "FillerKind")?,
            filler_mode: decode_enum_column("schedule item filler mode", &row, "FillerMode")?,
            duration: decode_optional_duration_text(
                row.try_get::<Option<String>, _>("Duration")
                    .repository()?
                    .as_deref(),
            ),
            count: row.try_get("Count").repository()?,
            pad_to_nearest_minute: row.try_get("PadToNearestMinute").repository()?,
            allow_watermarks: row.try_get("AllowWatermarks").repository()?,
            media_items,
        }))
    }

    async fn load_config_entity_id(&self, key: &str) -> Result<Option<EntityId>> {
        let value = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(key)
        .fetch_optional(&self.0)
        .await
        .repository()?;

        Ok(value.and_then(|value| value.trim().parse::<EntityId>().ok()))
    }

    /// Resolve playable media items for a scheduling content source.
    ///
    /// # Errors
    ///
    /// Returns an error when a `SQLite` query fails or a persisted enum/date
    /// value cannot be decoded.
    pub async fn media_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Vec<SchedulingMediaItem>> {
        match source {
            SchedulingContentSource::DirectKey(_) => Ok(Vec::new()),
            SchedulingContentSource::Collection(name) => self.collection_items_by_name(name).await,
            SchedulingContentSource::MultiCollection(name) => {
                self.multi_collection_items_by_name(name).await
            }
            SchedulingContentSource::SmartCollection(name) => {
                self.smart_collection_items_by_name(name).await
            }
            SchedulingContentSource::Playlist { group, name } => {
                self.playlist_media_items(group.as_deref(), name).await
            }
            SchedulingContentSource::Search(query) => self.search_items(query).await,
            SchedulingContentSource::Show { name, guids } => {
                let mut items = Vec::new();
                let mut seen = BTreeSet::new();

                if let Some(name) = name {
                    for show_id in self.show_ids_by_name(name).await? {
                        let show_items = self.items_for_show_id(show_id).await?;
                        append_unique(&mut items, &mut seen, show_items);
                    }
                }

                for guid in guids {
                    if !seen.is_empty() && name.is_none() {
                        break;
                    }
                    let show_items = self.show_items_by_guid(&guid.source, &guid.value).await?;
                    append_unique(&mut items, &mut seen, show_items);
                }

                Ok(items)
            }
            SchedulingContentSource::Marathon {
                name,
                guids,
                searches,
                ..
            } => {
                let mut items =
                    Box::pin(self.media_items_for_source(&SchedulingContentSource::Show {
                        name: name.clone(),
                        guids: guids.clone(),
                    }))
                    .await?;
                let mut seen = items
                    .iter()
                    .map(scheduling_item_key)
                    .collect::<BTreeSet<_>>();
                for query in searches {
                    let search_items = self.search_items(query).await?;
                    append_unique(&mut items, &mut seen, search_items);
                }
                Ok(items)
            }
            SchedulingContentSource::MediaItem(id) => self.media_item_by_id(*id).await,
        }
    }

    /// Resolve structured playlist children for a playlist content source.
    ///
    /// # Errors
    ///
    /// Returns an error when a `SQLite` query fails or a playlist child row
    /// contains an invalid enum, date, or count value.
    pub async fn playlist_items_for_source(
        &self,
        source: &SchedulingContentSource,
    ) -> Result<Option<Vec<SchedulingPlaylistItem>>> {
        let SchedulingContentSource::Playlist { group, name } = source else {
            return Ok(None);
        };

        self.playlist_items(group.as_deref(), name).await.map(Some)
    }

    /// Find the first channel watermark whose name contains the supplied value.
    ///
    /// # Errors
    ///
    /// Returns an error when the `SQLite` lookup fails.
    pub async fn watermark_id_by_name(&self, name: &str) -> Result<Option<EntityId>> {
        sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "ChannelWatermark" WHERE "Name" LIKE ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(format!("%{name}%"))
        .fetch_optional(&self.0)
        .await
        .repository()
    }

    /// Find a graphics element by exact or template-relative path.
    ///
    /// # Errors
    ///
    /// Returns an error when the `SQLite` lookup fails.
    pub async fn graphics_element_id_by_path(&self, path: &str) -> Result<Option<EntityId>> {
        let normalized = normalize_graphics_path(path);
        sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id"
             FROM "GraphicsElement"
             WHERE "Path" = ? OR "Path" = ? OR "Path" LIKE ?
             ORDER BY "Id"
             LIMIT 1"#,
        )
        .bind(path)
        .bind(&normalized)
        .bind(format!("%/{normalized}"))
        .fetch_optional(&self.0)
        .await
        .repository()
    }

    async fn collection_items_by_name(&self, name: &str) -> Result<Vec<SchedulingMediaItem>> {
        let collection_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "Collection" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await
        .repository()?;

        match collection_id {
            Some(id) => self.items_for_collection_id(id).await,
            None => Ok(Vec::new()),
        }
    }

    async fn items_for_collection_id(
        &self,
        collection_id: EntityId,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "CollectionItem"
             WHERE "CollectionId" = ?
             ORDER BY COALESCE("CustomIndex", "MediaItemId"), "MediaItemId", "Id""#,
        )
        .bind(collection_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        self.expand_collection_item_rows(rows, 0).await
    }

    async fn multi_collection_items_by_name(&self, name: &str) -> Result<Vec<SchedulingMediaItem>> {
        let multi_collection_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "MultiCollection" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await
        .repository()?;

        match multi_collection_id {
            Some(id) => self.items_for_multi_collection_id(id, 0).await,
            None => Ok(Vec::new()),
        }
    }

    async fn items_for_multi_collection_id(
        &self,
        multi_collection_id: EntityId,
        depth: usize,
    ) -> Result<Vec<SchedulingMediaItem>> {
        if depth > 16 {
            return Ok(Vec::new());
        }

        // MultiCollectionItem has composite PK (MultiCollectionId, CollectionId), no id/sort_order
        let collection_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "CollectionId"
             FROM "MultiCollectionItem"
             WHERE "MultiCollectionId" = ?
             ORDER BY "CollectionId""#,
        )
        .bind(multi_collection_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for collection_id in collection_ids {
            let child_items = self.items_for_collection_id(collection_id).await?;
            append_unique(&mut items, &mut seen, child_items);
        }

        // MultiCollectionSmartItem has composite PK (MultiCollectionId, SmartCollectionId)
        let smart_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "SmartCollectionId"
             FROM "MultiCollectionSmartItem"
             WHERE "MultiCollectionId" = ?
             ORDER BY "SmartCollectionId""#,
        )
        .bind(multi_collection_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        for smart_id in smart_ids {
            let smart_items = self.items_for_smart_collection_id(smart_id).await?;
            append_unique(&mut items, &mut seen, smart_items);
        }

        Ok(items)
    }

    async fn smart_collection_items_by_name(&self, name: &str) -> Result<Vec<SchedulingMediaItem>> {
        let row = sqlx::query(
            r#"SELECT "Id", "Query" FROM "SmartCollection" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await
        .repository()?;

        match row {
            Some(row) => {
                let query = row.try_get::<String, _>("Query").repository()?;
                self.search_items(&query).await
            }
            None => Ok(Vec::new()),
        }
    }

    async fn items_for_smart_collection_id(
        &self,
        smart_collection_id: EntityId,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let query = sqlx::query_scalar::<_, String>(
            r#"SELECT "Query" FROM "SmartCollection" WHERE "Id" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(smart_collection_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;

        match query {
            Some(query) => self.search_items(&query).await,
            None => Ok(Vec::new()),
        }
    }

    async fn playlist_media_items(
        &self,
        group: Option<&str>,
        name: &str,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let playlist_items = Box::pin(self.playlist_items(group, name)).await?;
        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for playlist_item in playlist_items {
            append_unique(&mut items, &mut seen, playlist_item.media_items);
        }
        Ok(items)
    }

    async fn playlist_items(
        &self,
        group: Option<&str>,
        name: &str,
    ) -> Result<Vec<SchedulingPlaylistItem>> {
        let playlist_id = if let Some(group) = group {
            sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "p"."Id"
                 FROM "Playlist" AS "p"
                 JOIN "PlaylistGroup" AS "pg" ON "pg"."Id" = "p"."PlaylistGroupId"
                 WHERE "p"."Name" = ? AND "pg"."Name" = ?
                 ORDER BY "p"."Id"
                 LIMIT 1"#,
            )
            .bind(name)
            .bind(group)
            .fetch_optional(&self.0)
            .await
            .repository()?
        } else {
            sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "Id" FROM "Playlist" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
            )
            .bind(name)
            .fetch_optional(&self.0)
            .await
            .repository()?
        };

        let Some(playlist_id) = playlist_id else {
            return Ok(Vec::new());
        };

        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlaylistItem"
             WHERE "PlaylistId" = ?
             ORDER BY "Index", "Id""#,
        )
        .bind(playlist_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let playback = playback_override_from_row(&row)?;
            let mut media_items = Box::pin(self.items_for_collection_key_row(&row, 0)).await?;
            apply_playback_override(&mut media_items, &playback);
            result.push(SchedulingPlaylistItem {
                collection_key: collection_key_from_row(&row)?,
                playback_order: decode_enum_column(
                    "playlist item playback_order",
                    &row,
                    "PlaybackOrder",
                )?,
                play_all: decode_bool_column(&row, "PlayAll")?,
                count: row
                    .try_get::<Option<i64>, _>("Count")
                    .repository()?
                    .map(usize::try_from)
                    .transpose()
                    .map_err(|error| decode_message("playlist item count", &error.to_string()))?,
                media_items,
            });
        }

        Ok(result)
    }

    async fn items_for_collection_key_row(
        &self,
        row: &DbRow,
        depth: usize,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let collection_key = collection_key_from_row(row)?;
        self.items_for_collection_key(&collection_key, depth).await
    }

    async fn playlist_media_items_by_id(
        &self,
        playlist_id: EntityId,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let name =
            sqlx::query_scalar::<_, String>(r#"SELECT "Name" FROM "Playlist" WHERE "Id" = ?"#)
                .bind(playlist_id)
                .fetch_optional(&self.0)
                .await
                .repository()?;

        match name {
            Some(name) => Box::pin(self.playlist_media_items(None, &name)).await,
            None => Ok(Vec::new()),
        }
    }

    async fn media_item_by_id(&self, id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        self.load_media_item(id)
            .await
            .map(|item| item.into_iter().collect())
    }

    async fn items_for_program_schedule_item(
        &self,
        item: &ProgramScheduleItem,
    ) -> Result<Vec<SchedulingMediaItem>> {
        self.items_for_collection_key(
            &SchedulingCollectionKey {
                collection_type: item.collection_type,
                collection_id: item.collection_id,
                multi_collection_id: item.multi_collection_id,
                smart_collection_id: item.smart_collection_id,
                media_item_id: item.media_item_id,
                playlist_id: item.playlist_id,
                search_query: item.search_query.clone(),
                fake_collection_key: None,
            },
            0,
        )
        .await
    }

    async fn expand_collection_item_rows(
        &self,
        rows: Vec<DbRow>,
        depth: usize,
    ) -> Result<Vec<SchedulingMediaItem>> {
        if depth > 16 {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for row in rows {
            let id: EntityId = row.try_get("MediaItemId").repository()?;
            let playback = playback_override_from_row(&row)?;
            let mut items = if self.is_show(id).await? {
                self.items_for_show_id(id).await?
            } else if self.is_season(id).await? {
                self.items_for_season_id(id).await?
            } else if self.is_artist(id).await? {
                self.items_for_artist_id(id).await?
            } else {
                self.media_item_by_id(id).await?
            };
            apply_playback_override(&mut items, &playback);
            append_unique(&mut result, &mut seen, items);
        }
        Ok(result)
    }

    async fn is_show(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "Show", id).await
    }

    async fn is_season(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "Season", id).await
    }

    async fn is_artist(&self, id: EntityId) -> Result<bool> {
        exists_by_id(&self.0, "Artist", id).await
    }

    async fn items_for_show_id(&self, show_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        // Episodes are linked via Season -> Show. Season has SeasonNumber.
        // EpisodeMetadata has EpisodeNumber. We need to join through Season for ordering.
        let ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "e"."Id"
             FROM "Episode" AS "e"
             JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId"
             LEFT JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "e"."Id"
             WHERE "s"."ShowId" = ?
             ORDER BY "s"."SeasonNumber", "em"."EpisodeNumber", "e"."Id""#,
        )
        .bind(show_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        self.load_media_items(ids).await
    }

    async fn items_for_season_id(&self, season_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        let ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "e"."Id"
             FROM "Episode" AS "e"
             LEFT JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "e"."Id"
             WHERE "e"."SeasonId" = ?
             ORDER BY "em"."EpisodeNumber", "e"."Id""#,
        )
        .bind(season_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        self.load_media_items(ids).await
    }

    async fn items_for_artist_id(&self, artist_id: EntityId) -> Result<Vec<SchedulingMediaItem>> {
        // MusicVideo has ArtistId FK; join through MusicVideoMetadata for ordering
        let ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "mv"."Id"
             FROM "MusicVideo" AS "mv"
             WHERE "mv"."ArtistId" = ?
             ORDER BY "mv"."Id""#,
        )
        .bind(artist_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        self.load_media_items(ids).await
    }

    async fn show_ids_by_name(&self, name: &str) -> Result<Vec<EntityId>> {
        sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "s"."Id"
             FROM "Show" AS "s"
             JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
             WHERE "sm"."Title" = ?
             ORDER BY "s"."Id""#,
        )
        .bind(name)
        .fetch_all(&self.0)
        .await
        .repository()
    }

    async fn show_items_by_guid(
        &self,
        source: &str,
        value: &str,
    ) -> Result<Vec<SchedulingMediaItem>> {
        // MetadataGuid has ShowMetadataId FK (nullable); join to ShowMetadata to get ShowId
        let show_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "sm"."ShowId"
             FROM "MetadataGuid" AS "mg"
             JOIN "ShowMetadata" AS "sm" ON "sm"."Id" = "mg"."ShowMetadataId"
             WHERE "mg"."Guid" = ?
             ORDER BY "sm"."ShowId"
             LIMIT 1"#,
        )
        .bind(format!("{source}://{value}"))
        .fetch_optional(&self.0)
        .await
        .repository()?;

        // Also try plain guid format
        let show_id = if show_id.is_none() {
            sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "sm"."ShowId"
                 FROM "MetadataGuid" AS "mg"
                 JOIN "ShowMetadata" AS "sm" ON "sm"."Id" = "mg"."ShowMetadataId"
                 WHERE "mg"."Guid" LIKE ?
                 ORDER BY "sm"."ShowId"
                 LIMIT 1"#,
            )
            .bind(format!("%{value}%"))
            .fetch_optional(&self.0)
            .await
            .repository()?
        } else {
            show_id
        };

        match show_id {
            Some(id) => self.items_for_show_id(id).await,
            None => Ok(Vec::new()),
        }
    }

    async fn search_items(&self, query: &str) -> Result<Vec<SchedulingMediaItem>> {
        let (hits, _, _) = self.search_documents_page(query, 0, 0).await?;

        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for hit in hits {
            let hit_items = match hit.media_kind.as_str() {
                "show" => self.items_for_show_id(hit.media_item_id).await?,
                "season" => self.items_for_season_id(hit.media_item_id).await?,
                "artist" => self.items_for_artist_id(hit.media_item_id).await?,
                _ => self.media_item_by_id(hit.media_item_id).await?,
            };
            append_unique(&mut items, &mut seen, hit_items);
        }

        Ok(items)
    }

    /// Return sorted search documents plus total hit count and a Lucene-style page map.
    ///
    /// # Errors
    ///
    /// Returns an error when the SQL-backed search projection cannot be loaded.
    pub async fn search_documents_page(
        &self,
        query: &str,
        skip: usize,
        limit: usize,
    ) -> Result<(Vec<SearchDocument>, usize, Option<SearchPageMap>)> {
        let expanded_query = self
            .expand_smart_collection_query(query, &mut Vec::new(), 0)
            .await?;
        let expression = SearchExpression::parse(&expanded_query);
        let hydration = SearchHydrationNeeds::for_expression(&expression);
        let documents = self.search_documents(&hydration).await?;
        let mut hits = documents
            .into_iter()
            .filter(|document| expression.matches(document))
            .collect::<Vec<_>>();

        hits.sort_by(|left, right| {
            search_sort_title(left)
                .cmp(&search_sort_title(right))
                .then_with(|| left.media_item_id.cmp(&right.media_item_id))
        });

        let total_count = hits.len();
        let page_map = (limit > 0 && limit < 10_000).then(|| search_page_map(hits.iter(), limit));
        let selected = if limit == 0 {
            hits.into_iter().skip(skip).collect()
        } else {
            hits.into_iter().skip(skip).take(limit).collect()
        };

        Ok((selected, total_count, page_map))
    }

    fn expand_smart_collection_query<'a>(
        &'a self,
        query: &'a str,
        stack: &'a mut Vec<String>,
        depth: usize,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            if depth > 100 {
                return Ok(query.to_owned());
            }

            let references = smart_collection_references(query);
            if references.is_empty() {
                return Ok(query.to_owned());
            }

            let mut result = String::with_capacity(query.len());
            let mut cursor = 0usize;
            for reference in references {
                result.push_str(&query[cursor..reference.start]);
                if stack
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&reference.name))
                {
                    result.push_str("(type:bad_query)");
                } else if let Some(child_query) =
                    self.smart_collection_query_by_name(&reference.name).await?
                {
                    stack.push(reference.name.clone());
                    let expanded = self
                        .expand_smart_collection_query(&child_query, stack, depth + 1)
                        .await?;
                    let _ = stack.pop();
                    result.push('(');
                    result.push_str(&expanded);
                    result.push(')');
                } else {
                    result.push_str("(type:bad_query)");
                }
                cursor = reference.end;
            }
            result.push_str(&query[cursor..]);

            if result.contains("smart_collection:\"") && result != query {
                self.expand_smart_collection_query(&result, stack, depth + 1)
                    .await
            } else {
                Ok(result)
            }
        })
    }

    async fn smart_collection_query_by_name(&self, name: &str) -> Result<Option<String>> {
        sqlx::query_scalar::<_, String>(
            r#"SELECT "Query" FROM "SmartCollection" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(name)
        .fetch_optional(&self.0)
        .await
        .repository()
    }

    async fn search_documents(
        &self,
        hydration: &SearchHydrationNeeds,
    ) -> Result<Vec<SearchDocument>> {
        let rows = sqlx::query(SEARCH_DOCUMENT_SQL)
            .fetch_all(&self.0)
            .await
            .repository()?;

        let mut documents = Vec::with_capacity(rows.len());
        for row in rows {
            documents.push(self.search_document_from_row(&row, hydration).await?);
        }

        Ok(documents)
    }

    #[allow(clippy::too_many_lines)]
    async fn search_document_from_row(
        &self,
        row: &DbRow,
        hydration: &SearchHydrationNeeds,
    ) -> Result<SearchDocument> {
        let metadata_kind = row.try_get::<String, _>("metadata_kind").repository()?;
        let metadata_id = row.try_get::<EntityId, _>("metadata_id").repository()?;
        let show_id = row.try_get::<Option<EntityId>, _>("show_id").repository()?;
        let release_date = row
            .try_get::<Option<String>, _>("release_date")
            .repository()?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_datetime("search release_date", &value))
            .transpose()?;
        let added_date = row
            .try_get::<Option<String>, _>("added_date")
            .repository()?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_datetime("search added_date", &value))
            .transpose()?;
        let item_id = row.try_get::<EntityId, _>("item_id").repository()?;
        let parent_media_item_id = row
            .try_get::<Option<EntityId>, _>("parent_media_item_id")
            .repository()?;
        let stream_fields = if hydration.contains(SEARCH_NEED_STREAM_FIELDS) {
            self.stream_search_fields(&metadata_kind, metadata_id, item_id)
                .await?
        } else {
            StreamSearchFields::default()
        };
        let language_codes = if hydration.contains(SEARCH_NEED_LANGUAGES)
            || hydration.contains(SEARCH_NEED_LANGUAGE_TAGS)
        {
            let explicit = self
                .metadata_language_values(&metadata_kind, metadata_id)
                .await?;
            ersatzrs_core::metadata_facets::union_facet_values(
                &ersatzrs_core::metadata_facets::normalize_language_codes(explicit),
                &ersatzrs_core::metadata_facets::normalize_language_codes(
                    &stream_fields.audio_languages,
                ),
            )
        } else {
            Vec::new()
        };

        Ok(SearchDocument {
            media_item_id: item_id,
            parent_media_item_id,
            media_kind: row.try_get("media_kind").repository()?,
            metadata_kind: metadata_kind.clone(),
            title: row.try_get("title").repository()?,
            sort_title: row.try_get("sort_title").repository()?,
            original_title: row.try_get("original_title").repository()?,
            year: row.try_get("year").repository()?,
            release_date,
            added_date,
            genres: if hydration.contains(SEARCH_NEED_GENRES) {
                self.metadata_lookup_values_fk(
                    "Genre",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?
            } else {
                Vec::new()
            },
            tags: if hydration.contains(SEARCH_NEED_TAGS) {
                self.metadata_lookup_values_fk(
                    "Tag",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?
            } else {
                Vec::new()
            },
            collections: if hydration.contains(SEARCH_NEED_COLLECTIONS) {
                self.collection_values(item_id).await?
            } else {
                Vec::new()
            },
            studios: if hydration.contains(SEARCH_NEED_STUDIOS) {
                self.metadata_lookup_values_fk(
                    "Studio",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?
            } else {
                Vec::new()
            },
            music_labels: if hydration.contains(SEARCH_NEED_MUSIC_LABELS) {
                self.metadata_lookup_values_fk(
                    "MusicLabel",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?
            } else {
                Vec::new()
            },
            networks: if hydration.contains(SEARCH_NEED_NETWORKS) {
                self.metadata_tag_values_by_external_type(&metadata_kind, metadata_id, "319")
                    .await?
            } else {
                Vec::new()
            },
            actors: if hydration.contains(SEARCH_NEED_ACTORS) {
                self.metadata_actor_values(&metadata_kind, metadata_id)
                    .await?
            } else {
                Vec::new()
            },
            directors: if hydration.contains(SEARCH_NEED_DIRECTORS) {
                self.metadata_credit_values("Director", &metadata_kind, metadata_id)
                    .await?
            } else {
                Vec::new()
            },
            writers: if hydration.contains(SEARCH_NEED_WRITERS) {
                self.metadata_credit_values("Writer", &metadata_kind, metadata_id)
                    .await?
            } else {
                Vec::new()
            },
            languages: if hydration.contains(SEARCH_NEED_LANGUAGES) {
                self.language_names_for_codes(&language_codes).await?
            } else {
                Vec::new()
            },
            language_tags: if hydration.contains(SEARCH_NEED_LANGUAGE_TAGS) {
                language_codes
            } else {
                Vec::new()
            },
            subtitle_languages: if hydration.contains(SEARCH_NEED_SUBTITLE_LANGUAGES) {
                self.language_names_for_codes(&stream_fields.subtitle_languages)
                    .await?
            } else {
                Vec::new()
            },
            subtitle_language_tags: if hydration.contains(SEARCH_NEED_SUBTITLE_LANGUAGE_TAGS) {
                stream_fields.subtitle_languages.clone()
            } else {
                Vec::new()
            },
            video_codecs: if hydration.contains(SEARCH_NEED_VIDEO_CODECS) {
                stream_fields.video_codecs.clone()
            } else {
                Vec::new()
            },
            video_bit_depths: if hydration.contains(SEARCH_NEED_VIDEO_BIT_DEPTHS) {
                stream_fields.video_bit_depths.clone()
            } else {
                Vec::new()
            },
            video_dynamic_ranges: if hydration.contains(SEARCH_NEED_VIDEO_DYNAMIC_RANGES) {
                stream_fields.video_dynamic_ranges.clone()
            } else {
                Vec::new()
            },
            audio_codecs: if hydration.contains(SEARCH_NEED_AUDIO_CODECS) {
                stream_fields.audio_codecs.clone()
            } else {
                Vec::new()
            },
            audio_titles: if hydration.contains(SEARCH_NEED_AUDIO_TITLES) {
                stream_fields.audio_titles.clone()
            } else {
                Vec::new()
            },
            subtitle_codecs: if hydration.contains(SEARCH_NEED_SUBTITLE_CODECS) {
                stream_fields.subtitle_codecs.clone()
            } else {
                Vec::new()
            },
            subtitle_titles: if hydration.contains(SEARCH_NEED_SUBTITLE_TITLES) {
                stream_fields.subtitle_titles.clone()
            } else {
                Vec::new()
            },
            has_commentary: stream_fields.has_commentary,
            width: hydration
                .contains(SEARCH_NEED_WIDTH)
                .then_some(stream_fields.width)
                .flatten(),
            height: hydration
                .contains(SEARCH_NEED_HEIGHT)
                .then_some(stream_fields.height)
                .flatten(),
            chapter_counts: if hydration.contains(SEARCH_NEED_CHAPTERS) {
                stream_fields.chapter_counts.clone()
            } else {
                Vec::new()
            },
            durations_seconds: if hydration.contains(SEARCH_NEED_DURATIONS) {
                stream_fields.durations_seconds.clone()
            } else {
                Vec::new()
            },
            library_name: row.try_get("library_name").repository()?,
            library_folder_ids: if hydration.contains(SEARCH_NEED_LIBRARY_FOLDER_IDS) {
                stream_fields.library_folder_ids.clone()
            } else {
                Vec::new()
            },
            content_kind: decode_enum_column("content kind", row, "content_kind")?,
            sub_kind: row
                .try_get::<Option<String>, _>("sub_kind")
                .repository()?
                .and_then(|value| normalize_sub_kind_token(&value)),
            parent_title: if hydration.contains(SEARCH_NEED_PARENT_TITLE) {
                match parent_media_item_id {
                    Some(parent_id) => self.media_item_title(parent_id).await?,
                    None => None,
                }
            } else {
                None
            },
            content_ratings: if hydration.contains(SEARCH_NEED_CONTENT_RATINGS) {
                split_content_ratings(
                    row.try_get::<Option<String>, _>("content_rating")
                        .repository()?,
                )
            } else {
                Vec::new()
            },
            show_title: row.try_get("show_title").repository()?,
            show_genres: if hydration.contains(SEARCH_NEED_SHOW_GENRES) {
                self.show_lookup_values("Genre", show_id).await?
            } else {
                Vec::new()
            },
            show_tags: if hydration.contains(SEARCH_NEED_SHOW_TAGS) {
                self.show_lookup_values("Tag", show_id).await?
            } else {
                Vec::new()
            },
            show_studios: if hydration.contains(SEARCH_NEED_SHOW_STUDIOS) {
                self.show_lookup_values("Studio", show_id).await?
            } else {
                Vec::new()
            },
            show_networks: if hydration.contains(SEARCH_NEED_SHOW_NETWORKS) {
                self.show_tag_values_by_external_type(show_id, "319")
                    .await?
            } else {
                Vec::new()
            },
            show_content_ratings: if hydration.contains(SEARCH_NEED_SHOW_CONTENT_RATINGS) {
                self.show_content_rating_values(show_id).await?
            } else {
                Vec::new()
            },
            season_number: row.try_get("season_number").repository()?,
            episode_number: row.try_get("episode_number").repository()?,
            trakt_lists: if hydration.contains(SEARCH_NEED_TRAKT_LISTS) {
                self.trakt_list_values(item_id).await?
            } else {
                Vec::new()
            },
            styles: if hydration.contains(SEARCH_NEED_STYLES) {
                self.metadata_lookup_values_fk(
                    "Style",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?
            } else {
                Vec::new()
            },
            moods: if hydration.contains(SEARCH_NEED_MOODS) {
                self.metadata_lookup_values_fk(
                    "Mood",
                    "MovieMetadataId",
                    "EpisodeMetadataId",
                    "ShowMetadataId",
                    &metadata_kind,
                    metadata_id,
                )
                .await?
            } else {
                Vec::new()
            },
            albums: if hydration.contains(SEARCH_NEED_ALBUMS) {
                option_values_vec([row.try_get::<Option<String>, _>("album").repository()?])
            } else {
                Vec::new()
            },
            artists: if hydration.contains(SEARCH_NEED_ARTISTS) {
                self.music_artist_values(&metadata_kind, metadata_id, item_id)
                    .await?
            } else {
                Vec::new()
            },
            album_artists: if hydration.contains(SEARCH_NEED_ALBUM_ARTISTS) {
                parse_string_list(
                    row.try_get::<Option<String>, _>("album_artists")
                        .repository()?,
                )
            } else {
                Vec::new()
            },
            library_id: row
                .try_get::<Option<EntityId>, _>("library_id")
                .repository()?,
            provider_guids: if hydration.contains(SEARCH_NEED_PROVIDER_GUIDS) {
                self.metadata_guids(&metadata_kind, metadata_id).await?
            } else {
                Vec::new()
            },
        })
    }

    /// Look up names from tables that use nullable FK columns per metadata type.
    /// `ErsatzTV` uses `"Genre"."MovieMetadataId"`, `"Genre"."EpisodeMetadataId"`, etc.
    async fn metadata_lookup_values_fk(
        &self,
        table: &str,
        movie_fk: &str,
        episode_fk: &str,
        show_fk: &str,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<String>> {
        let fk_column = match metadata_kind {
            "Movie" => movie_fk,
            "Episode" => episode_fk,
            "Show" => show_fk,
            "Season" => "SeasonMetadataId",
            "MusicVideo" => "MusicVideoMetadataId",
            "Artist" => "ArtistMetadataId",
            "OtherVideo" => "OtherVideoMetadataId",
            "Song" => "SongMetadataId",
            "Image" => "ImageMetadataId",
            "RemoteStream" => "RemoteStreamMetadataId",
            _ => return Ok(Vec::new()),
        };
        let sql =
            format!(r#"SELECT "Name" FROM "{table}" WHERE "{fk_column}" = ? ORDER BY "Name""#);
        sqlx::query_scalar::<_, String>(crate::db::assert_sql_safe(&sql))
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await
            .repository()
    }

    async fn metadata_language_values(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<String>> {
        let Some(fk_column) = metadata_fk(metadata_kind) else {
            return Ok(Vec::new());
        };
        let sql = format!(
            r#"SELECT "Code" FROM "MetadataLanguage" WHERE "{fk_column}" = ? ORDER BY "Id""#
        );
        sqlx::query_scalar::<_, String>(crate::db::assert_sql_safe(&sql))
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await
            .repository()
    }

    async fn trakt_list_values(&self, item_id: EntityId) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"SELECT "tl"."TraktId"
               FROM "TraktListItem" AS "tli"
               JOIN "TraktList" AS "tl" ON "tl"."Id" = "tli"."TraktListId"
               WHERE "tli"."MediaItemId" = ?
               ORDER BY "tl"."TraktId""#,
        )
        .bind(item_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                let trakt_id: i32 = row.try_get("TraktId").repository()?;
                Ok(trakt_id.to_string())
            })
            .collect()
    }

    async fn collection_values(&self, item_id: EntityId) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            r#"SELECT "c"."Name"
               FROM "CollectionItem" AS "ci"
               JOIN "Collection" AS "c" ON "c"."Id" = "ci"."CollectionId"
               WHERE "ci"."MediaItemId" = ?
               ORDER BY "c"."Name""#,
        )
        .bind(item_id)
        .fetch_all(&self.0)
        .await
        .repository()
    }

    async fn media_item_title(&self, item_id: EntityId) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT COALESCE(
                   "mm"."Title", "sm"."Title", "sem"."Title", "em"."Title",
                   "mvm"."Title", "am"."Title", "ovm"."Title", "som"."Title",
                   "im"."Title", "rsm"."Title"
               )
               FROM "MediaItem" AS "mi"
               LEFT JOIN "MovieMetadata" AS "mm" ON "mm"."MovieId" = "mi"."Id"
               LEFT JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "mi"."Id"
               LEFT JOIN "SeasonMetadata" AS "sem" ON "sem"."SeasonId" = "mi"."Id"
               LEFT JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "mi"."Id"
               LEFT JOIN "MusicVideoMetadata" AS "mvm" ON "mvm"."MusicVideoId" = "mi"."Id"
               LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "mi"."Id"
               LEFT JOIN "OtherVideoMetadata" AS "ovm" ON "ovm"."OtherVideoId" = "mi"."Id"
               LEFT JOIN "SongMetadata" AS "som" ON "som"."SongId" = "mi"."Id"
               LEFT JOIN "ImageMetadata" AS "im" ON "im"."ImageId" = "mi"."Id"
               LEFT JOIN "RemoteStreamMetadata" AS "rsm" ON "rsm"."RemoteStreamId" = "mi"."Id"
               WHERE "mi"."Id" = ?
               ORDER BY "mi"."Id"
               LIMIT 1"#,
        )
        .bind(item_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .flatten())
    }

    async fn metadata_actor_values(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<String>> {
        let fk_column = match metadata_kind {
            "Movie" => "MovieMetadataId",
            "Episode" => "EpisodeMetadataId",
            "Show" => "ShowMetadataId",
            "Season" => "SeasonMetadataId",
            "MusicVideo" => "MusicVideoMetadataId",
            "Artist" => "ArtistMetadataId",
            "OtherVideo" => "OtherVideoMetadataId",
            "Song" => "SongMetadataId",
            _ => return Ok(Vec::new()),
        };
        let sql =
            format!(r#"SELECT "Name" FROM "Actor" WHERE "{fk_column}" = ? ORDER BY "Order", "Id""#);
        sqlx::query_scalar::<_, String>(crate::db::assert_sql_safe(&sql))
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await
            .repository()
    }

    async fn metadata_credit_values(
        &self,
        table: &str,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<String>> {
        let fk_column = match (table, metadata_kind) {
            ("Director" | "Writer", "Movie") => "MovieMetadataId",
            ("Director" | "Writer", "Episode") => "EpisodeMetadataId",
            ("Director", "MusicVideo") => "MusicVideoMetadataId",
            ("Director" | "Writer", "OtherVideo") => "OtherVideoMetadataId",
            _ => return Ok(Vec::new()),
        };
        let sql = format!(r#"SELECT "Name" FROM "{table}" WHERE "{fk_column}" = ? ORDER BY "Id""#);
        sqlx::query_scalar::<_, String>(crate::db::assert_sql_safe(&sql))
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await
            .repository()
    }

    async fn metadata_tag_values_by_external_type(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
        external_type_id: &str,
    ) -> Result<Vec<String>> {
        let Some(fk_column) = metadata_fk(metadata_kind) else {
            return Ok(Vec::new());
        };
        let sql = format!(
            r#"SELECT "Name" FROM "Tag"
               WHERE "{fk_column}" = ? AND "ExternalTypeId" = ?
               ORDER BY "Name""#
        );
        sqlx::query_scalar::<_, String>(crate::db::assert_sql_safe(&sql))
            .bind(metadata_id)
            .bind(external_type_id)
            .fetch_all(&self.0)
            .await
            .repository()
    }

    #[allow(clippy::too_many_lines)]
    async fn stream_search_fields(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
        item_id: EntityId,
    ) -> Result<StreamSearchFields> {
        let mut result = StreamSearchFields::default();

        let Some(version_fk) = media_version_fk(metadata_kind) else {
            return Ok(result);
        };
        let version_sql = format!(
            r#"SELECT "Id", "Width", "Height", "Duration"
             FROM "MediaVersion"
             WHERE "{version_fk}" = ?
             ORDER BY "Id""#
        );
        let version_rows = sqlx::query(crate::db::assert_sql_safe(&version_sql))
            .bind(item_id)
            .fetch_all(&self.0)
            .await
            .repository()?;
        let version_ids = version_rows
            .iter()
            .map(|row| row.try_get::<EntityId, _>("Id"))
            .collect::<std::result::Result<Vec<_>, _>>()
            .repository()?;

        if let Some(row) = version_rows.first() {
            result.width = row.try_get("Width").ok();
            result.height = row.try_get("Height").ok();
        }

        for row in &version_rows {
            if let Ok(duration) = row.try_get::<String, _>("Duration") {
                let seconds = decode_duration_text(&duration).num_seconds();
                result.durations_seconds.push(seconds);
            }
        }

        for version_id in version_ids {
            let library_folder_ids = sqlx::query_scalar::<_, Option<EntityId>>(
                r#"SELECT "LibraryFolderId"
                 FROM "MediaFile"
                 WHERE "MediaVersionId" = ?
                 ORDER BY "Id""#,
            )
            .bind(version_id)
            .fetch_all(&self.0)
            .await
            .repository()?;
            for library_folder_id in library_folder_ids.into_iter().flatten() {
                push_unique_i64(&mut result.library_folder_ids, library_folder_id);
            }

            let chapter_count = sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*)
                 FROM "MediaChapter"
                 WHERE "MediaVersionId" = ?"#,
            )
            .bind(version_id)
            .fetch_one(&self.0)
            .await
            .repository()?;
            result
                .chapter_counts
                .push(i32::try_from(chapter_count).unwrap_or(i32::MAX));

            let rows = sqlx::query(
                r#"SELECT "MediaStreamKind", "Codec", "Language", "Title", "PixelFormat",
                          "BitsPerRawSample", "ColorTransfer"
                 FROM "MediaStream"
                 WHERE "MediaVersionId" = ?
                 ORDER BY "Index", "Id""#,
            )
            .bind(version_id)
            .fetch_all(&self.0)
            .await
            .repository()?;
            for row in rows {
                let kind = media_stream_kind_from_row(&row);
                let codec = row.try_get::<Option<String>, _>("Codec").repository()?;
                let language = row.try_get::<Option<String>, _>("Language").repository()?;
                let title = row.try_get::<Option<String>, _>("Title").repository()?;
                match kind.as_deref() {
                    Some("video") => {
                        push_unique_optional(&mut result.video_codecs, codec);
                        if let Some(bit_depth) = video_bit_depth_from_row(&row) {
                            push_unique_i32(&mut result.video_bit_depths, bit_depth);
                        }
                        push_unique(
                            &mut result.video_dynamic_ranges,
                            video_dynamic_range_from_row(&row).to_owned(),
                        );
                    }
                    Some("audio") => {
                        push_unique_optional(&mut result.audio_codecs, codec);
                        push_unique_optional(&mut result.audio_languages, language);
                        push_stream_title(
                            &mut result.audio_titles,
                            &mut result.has_commentary,
                            title,
                        );
                    }
                    Some("subtitle" | "external_subtitle") => {
                        push_unique_optional(&mut result.subtitle_codecs, codec);
                        push_unique_optional(&mut result.subtitle_languages, language);
                        push_stream_title(
                            &mut result.subtitle_titles,
                            &mut result.has_commentary,
                            title,
                        );
                    }
                    _ => {}
                }
            }
        }

        let subtitle_fk = metadata_fk(metadata_kind);
        if let Some(subtitle_fk) = subtitle_fk {
            let subtitle_sql = format!(
                r#"SELECT "Codec", "Language", "Title"
                 FROM "Subtitle"
                 WHERE "{subtitle_fk}" = ?
                 ORDER BY "StreamIndex", "Id""#
            );
            let rows = sqlx::query(crate::db::assert_sql_safe(&subtitle_sql))
                .bind(metadata_id)
                .fetch_all(&self.0)
                .await
                .repository()?;
            for row in rows {
                push_unique_optional(
                    &mut result.subtitle_codecs,
                    row.try_get::<Option<String>, _>("Codec").repository()?,
                );
                push_unique_optional(
                    &mut result.subtitle_languages,
                    row.try_get::<Option<String>, _>("Language").repository()?,
                );
                push_stream_title(
                    &mut result.subtitle_titles,
                    &mut result.has_commentary,
                    row.try_get::<Option<String>, _>("Title").repository()?,
                );
            }
        }

        Ok(result)
    }

    async fn language_names_for_codes(&self, codes: &[String]) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for code in codes {
            let rows = sqlx::query_scalar::<_, String>(
                r#"SELECT "EnglishName"
                 FROM "LanguageCode"
                 WHERE "ThreeCode1" = ? OR "ThreeCode2" = ? OR "TwoCode" = ?
                 ORDER BY "Id""#,
            )
            .bind(code)
            .bind(code)
            .bind(code)
            .fetch_all(&self.0)
            .await
            .repository()?;
            for row in rows {
                push_unique(&mut names, row);
            }
            push_unique(&mut names, code.clone());
        }
        Ok(names)
    }

    async fn show_lookup_values(
        &self,
        table: &str,
        show_id: Option<EntityId>,
    ) -> Result<Vec<String>> {
        let Some(show_id) = show_id else {
            return Ok(Vec::new());
        };

        let Some(metadata_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "ShowMetadata" WHERE "ShowId" = ?"#,
        )
        .bind(show_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(Vec::new());
        };

        self.metadata_lookup_values_fk(
            table,
            "MovieMetadataId",
            "EpisodeMetadataId",
            "ShowMetadataId",
            "Show",
            metadata_id,
        )
        .await
    }

    async fn show_tag_values_by_external_type(
        &self,
        show_id: Option<EntityId>,
        external_type_id: &str,
    ) -> Result<Vec<String>> {
        let Some(show_id) = show_id else {
            return Ok(Vec::new());
        };

        let Some(metadata_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "ShowMetadata" WHERE "ShowId" = ?"#,
        )
        .bind(show_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(Vec::new());
        };

        self.metadata_tag_values_by_external_type("Show", metadata_id, external_type_id)
            .await
    }

    async fn show_content_rating_values(&self, show_id: Option<EntityId>) -> Result<Vec<String>> {
        let Some(show_id) = show_id else {
            return Ok(Vec::new());
        };

        let rating = sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT "ContentRating" FROM "ShowMetadata" WHERE "ShowId" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(show_id)
        .fetch_optional(&self.0)
        .await.repository()?
        .flatten();
        Ok(split_content_ratings(rating))
    }

    async fn music_artist_values(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
        item_id: EntityId,
    ) -> Result<Vec<String>> {
        match metadata_kind {
            "MusicVideo" => {
                let mut values = Vec::new();
                let linked_artists = sqlx::query_scalar::<_, String>(
                    r#"SELECT "am"."Title"
                       FROM "MusicVideo" AS "mv"
                       JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "mv"."ArtistId"
                       WHERE "mv"."Id" = ?
                       ORDER BY "am"."Id""#,
                )
                .bind(item_id)
                .fetch_all(&self.0)
                .await
                .repository()?;
                for artist in linked_artists {
                    push_unique(&mut values, artist);
                }

                let metadata_artists = sqlx::query_scalar::<_, String>(
                    r#"SELECT "Name"
                       FROM "MusicVideoArtist"
                       WHERE "MusicVideoMetadataId" = ?
                       ORDER BY "Id""#,
                )
                .bind(metadata_id)
                .fetch_all(&self.0)
                .await
                .repository()?;
                for artist in metadata_artists {
                    push_unique(&mut values, artist);
                }
                Ok(values)
            }
            "Song" => {
                let raw = sqlx::query_scalar::<_, Option<String>>(
                    r#"SELECT "Artists" FROM "SongMetadata" WHERE "Id" = ?"#,
                )
                .bind(metadata_id)
                .fetch_optional(&self.0)
                .await
                .repository()?
                .flatten();
                Ok(parse_string_list(raw))
            }
            _ => Ok(Vec::new()),
        }
    }

    async fn metadata_guids(
        &self,
        metadata_kind: &str,
        metadata_id: EntityId,
    ) -> Result<Vec<SearchProviderGuid>> {
        let fk_column = match metadata_kind {
            "Movie" => "MovieMetadataId",
            "Episode" => "EpisodeMetadataId",
            "Show" => "ShowMetadataId",
            "Season" => "SeasonMetadataId",
            "MusicVideo" => "MusicVideoMetadataId",
            "Artist" => "ArtistMetadataId",
            "OtherVideo" => "OtherVideoMetadataId",
            "Song" => "SongMetadataId",
            "Image" => "ImageMetadataId",
            "RemoteStream" => "RemoteStreamMetadataId",
            _ => return Ok(Vec::new()),
        };
        let sql =
            format!(r#"SELECT "Guid" FROM "MetadataGuid" WHERE "{fk_column}" = ? ORDER BY "Guid""#);
        let guids = sqlx::query_scalar::<_, String>(crate::db::assert_sql_safe(&sql))
            .bind(metadata_id)
            .fetch_all(&self.0)
            .await
            .repository()?;

        // ErsatzTV stores GUIDs as "source://value" or similar format
        Ok(guids
            .into_iter()
            .map(|guid| {
                if let Some((source, value)) = guid.split_once("://") {
                    SearchProviderGuid {
                        source: source.to_owned(),
                        value: value.to_owned(),
                    }
                } else {
                    SearchProviderGuid {
                        source: String::new(),
                        value: guid,
                    }
                }
            })
            .collect())
    }

    async fn load_media_items(&self, ids: Vec<EntityId>) -> Result<Vec<SchedulingMediaItem>> {
        let mut result = Vec::new();
        for id in ids {
            if let Some(item) = self.load_media_item(id).await? {
                result.push(item);
            }
        }
        Ok(result)
    }

    async fn load_media_item(&self, id: EntityId) -> Result<Option<SchedulingMediaItem>> {
        let row = sqlx::query(
            r#"SELECT
                "mi"."Id",
                COALESCE(
                    NULLIF((SELECT MAX("mv"."Duration") FROM "MediaVersion" AS "mv" WHERE
                        "mv"."MovieId" = "mi"."Id" OR "mv"."EpisodeId" = "mi"."Id" OR
                        "mv"."MusicVideoId" = "mi"."Id" OR "mv"."OtherVideoId" = "mi"."Id" OR
                        "mv"."SongId" = "mi"."Id" OR "mv"."ImageId" = "mi"."Id" OR
                        "mv"."RemoteStreamId" = "mi"."Id"), '00:00:00'),
                    (SELECT "rs"."Duration" FROM "RemoteStream" AS "rs"
                     WHERE "rs"."Id" = "mi"."Id" LIMIT 1),
                    '00:00:00'
                ) AS "duration_text",
                COALESCE(
                    (SELECT "em"."Title" FROM "EpisodeMetadata" AS "em" WHERE "em"."EpisodeId" = "mi"."Id" LIMIT 1),
                    (SELECT "mm"."Title" FROM "MovieMetadata" AS "mm" WHERE "mm"."MovieId" = "mi"."Id" LIMIT 1),
                    (SELECT "mvm"."Title" FROM "MusicVideoMetadata" AS "mvm" WHERE "mvm"."MusicVideoId" = "mi"."Id" LIMIT 1),
                    (SELECT "ovm"."Title" FROM "OtherVideoMetadata" AS "ovm" WHERE "ovm"."OtherVideoId" = "mi"."Id" LIMIT 1),
                    (SELECT "som"."Title" FROM "SongMetadata" AS "som" WHERE "som"."SongId" = "mi"."Id" LIMIT 1),
                    (SELECT "shm"."Title" FROM "ShowMetadata" AS "shm" WHERE "shm"."ShowId" = "mi"."Id" LIMIT 1),
                    (SELECT "sem"."Title" FROM "SeasonMetadata" AS "sem" WHERE "sem"."SeasonId" = "mi"."Id" LIMIT 1),
                    (SELECT "am"."Title" FROM "ArtistMetadata" AS "am" WHERE "am"."ArtistId" = "mi"."Id" LIMIT 1),
                    (SELECT "rsm"."Title" FROM "RemoteStreamMetadata" AS "rsm"
                     WHERE "rsm"."RemoteStreamId" = "mi"."Id" LIMIT 1)
                ) AS "title",
                (SELECT "s"."ShowId" FROM "Episode" AS "e" JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId" WHERE "e"."Id" = "mi"."Id" LIMIT 1) AS "show_id",
                (SELECT "e"."SeasonId" FROM "Episode" AS "e" WHERE "e"."Id" = "mi"."Id" LIMIT 1) AS "season_id",
                (SELECT "mv"."ArtistId" FROM "MusicVideo" AS "mv" WHERE "mv"."Id" = "mi"."Id" LIMIT 1) AS "artist_id",
                (SELECT "s"."SeasonNumber" FROM "Episode" AS "e" JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId" WHERE "e"."Id" = "mi"."Id" LIMIT 1) AS "season_number",
                (SELECT "em"."EpisodeNumber" FROM "EpisodeMetadata" AS "em" WHERE "em"."EpisodeId" = "mi"."Id" LIMIT 1) AS "episode_number",
                COALESCE(
                    (SELECT "em"."ReleaseDate" FROM "EpisodeMetadata" AS "em" WHERE "em"."EpisodeId" = "mi"."Id" LIMIT 1),
                    (SELECT "mm"."ReleaseDate" FROM "MovieMetadata" AS "mm" WHERE "mm"."MovieId" = "mi"."Id" LIMIT 1),
                    (SELECT "mvm"."ReleaseDate" FROM "MusicVideoMetadata" AS "mvm" WHERE "mvm"."MusicVideoId" = "mi"."Id" LIMIT 1)
                ) AS "release_date",
                "mi"."State" AS "state"
             FROM "MediaItem" AS "mi"
             WHERE "mi"."Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await.repository()?;

        let Some(row) = row else {
            return Ok(None);
        };

        // ErsatzTV's PlayoutBuilder drops FileNotFound/Unavailable items when
        // `playout.skip_missing_items` is enabled. RemoteOnly (3) stays
        // eligible because it remains playable via the media server.
        if self.skip_missing_items().await? {
            let state: i64 = row.try_get("state").unwrap_or(0);
            if state == 1 || state == 2 {
                return Ok(None);
            }
        }

        map_scheduling_media_item(&row).map(Some)
    }

    async fn block_template_schedules(
        &self,
        playout_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Vec<BlockTemplateSchedule>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlayoutTemplate"
             WHERE "PlayoutId" = ?
             ORDER BY "Index", "TemplateId""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let deco = self
            .effective_deco_for_playout(playout_id, media_by_key)
            .await?;
        let mut schedules = Vec::with_capacity(rows.len());
        for row in rows {
            let template_id = row.try_get::<EntityId, _>("TemplateId").repository()?;
            let template = self.block_template(template_id).await?;
            let mut blocks = BTreeMap::new();
            for item in &template.items {
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    blocks.entry(item.block_id)
                {
                    entry.insert(self.block_definition(item.block_id, media_by_key).await?);
                }
            }
            let deco_template_items = match row
                .try_get::<Option<EntityId>, _>("DecoTemplateId")
                .repository()?
            {
                Some(deco_template_id) => {
                    self.effective_deco_template_items(deco_template_id, media_by_key)
                        .await?
                }
                None => Vec::new(),
            };

            // DateUpdated is TEXT in ErsatzTV; parse as i64 (stored as string ticks) or 0
            let updated_ticks: i64 = row
                .try_get::<String, _>("DateUpdated")
                .ok()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);

            schedules.push(BlockTemplateSchedule {
                id: template_id,
                updated_ticks,
                start: None,
                finish: None,
                index: row.try_get("Index").repository()?,
                days_of_week: decode_integer_list(
                    "playout template DaysOfWeek",
                    &row.try_get::<String, _>("DaysOfWeek").unwrap_or_default(),
                )?,
                days_of_month: decode_integer_list(
                    "playout template DaysOfMonth",
                    &row.try_get::<String, _>("DaysOfMonth").unwrap_or_default(),
                )?,
                months_of_year: decode_integer_list(
                    "playout template MonthsOfYear",
                    &row.try_get::<String, _>("MonthsOfYear").unwrap_or_default(),
                )?,
                limit_to_date_range: row.try_get("LimitToDateRange").repository()?,
                start_month: row.try_get("StartMonth").repository()?,
                start_day: row.try_get("StartDay").repository()?,
                start_year: row.try_get("StartYear").repository()?,
                end_month: row.try_get("EndMonth").repository()?,
                end_day: row.try_get("EndDay").repository()?,
                end_year: row.try_get("EndYear").repository()?,
                template,
                blocks,
                deco: deco.clone(),
                deco_template_items,
            });
        }

        Ok(schedules)
    }

    async fn block_template(&self, template_id: EntityId) -> Result<BlockTemplate> {
        let rows = sqlx::query(
            r#"SELECT "Id", "BlockId", "StartTime"
             FROM "TemplateItem"
             WHERE "TemplateId" = ?
             ORDER BY "StartTime", "Id""#,
        )
        .bind(template_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let items = rows
            .iter()
            .map(|row| {
                Ok(BlockTemplateItem {
                    id: row.try_get("Id").repository()?,
                    block_id: row.try_get("BlockId").repository()?,
                    start_time: decode_optional_time_of_day_text(
                        "template item StartTime",
                        row.try_get::<Option<String>, _>("StartTime").repository()?,
                    )?,
                    sort_order: 0,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // DateUpdated is TEXT in ErsatzTV; parse as i64 or 0
        let updated_ticks: i64 = sqlx::query_scalar::<_, String>(
            r#"SELECT "DateUpdated" FROM "Template" WHERE "Id" = ?"#,
        )
        .bind(template_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

        Ok(BlockTemplate {
            id: template_id,
            updated_ticks,
            items,
        })
    }

    async fn block_definition(
        &self,
        block_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<BlockDefinition> {
        let row = sqlx::query(
            r#"SELECT "Id", "Name", "DateUpdated", "Minutes", "StopScheduling"
             FROM "Block"
             WHERE "Id" = ?"#,
        )
        .bind(block_id)
        .fetch_one(&self.0)
        .await
        .repository()?;

        // DateUpdated is TEXT; Minutes is INTEGER (minutes, not ms)
        let updated_ticks: i64 = row
            .try_get::<String, _>("DateUpdated")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let minutes: i64 = row.try_get("Minutes").repository()?;
        let duration = Duration::minutes(minutes);

        Ok(BlockDefinition {
            id: row.try_get("Id").repository()?,
            updated_ticks,
            name: row.try_get("Name").repository()?,
            duration,
            stop_scheduling: decode_enum_column("block stop_scheduling", &row, "StopScheduling")?,
            items: self.effective_block_items(block_id, media_by_key).await?,
        })
    }

    async fn effective_block_items(
        &self,
        block_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Vec<EffectiveBlockItem>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "BlockItem"
             WHERE "BlockId" = ?
             ORDER BY "Index", "Id""#,
        )
        .bind(block_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.try_get::<EntityId, _>("Id").repository()?;
            let content = block_item_content_key(id);
            media_by_key.insert(
                content.clone(),
                Box::pin(self.items_for_collection_key_row(&row, 0)).await?,
            );
            items.push(EffectiveBlockItem {
                id,
                block_id,
                content,
                collection_key: collection_key_from_row(&row)?,
                playback_order: decode_enum_column(
                    "block item playback_order",
                    &row,
                    "PlaybackOrder",
                )?,
                include_in_program_guide: decode_bool_column(&row, "IncludeInProgramGuide")?,
                disable_watermarks: decode_bool_column(&row, "DisableWatermarks")?,
                watermark_ids: load_ids(
                    &self.0,
                    r#"SELECT "WatermarkId"
                     FROM "BlockItemWatermark"
                     WHERE "BlockItemId" = ?
                     ORDER BY "WatermarkId""#,
                    id,
                )
                .await?,
                graphics_element_ids: load_ids(
                    &self.0,
                    r#"SELECT "GraphicsElementId"
                     FROM "BlockItemGraphicsElement"
                     WHERE "BlockItemId" = ?
                     ORDER BY "GraphicsElementId""#,
                    id,
                )
                .await?,
            });
        }

        Ok(items)
    }

    async fn items_for_collection_key(
        &self,
        collection_key: &SchedulingCollectionKey,
        depth: usize,
    ) -> Result<Vec<SchedulingMediaItem>> {
        match collection_key.collection_type {
            CollectionType::Collection => match collection_key.collection_id {
                Some(id) => self.items_for_collection_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::TelevisionShow => match collection_key.media_item_id {
                Some(id) => self.items_for_show_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::TelevisionSeason => match collection_key.media_item_id {
                Some(id) => self.items_for_season_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::Artist => match collection_key.media_item_id {
                Some(id) => self.items_for_artist_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::MultiCollection => match collection_key.multi_collection_id {
                Some(id) => Box::pin(self.items_for_multi_collection_id(id, depth)).await,
                None => Ok(Vec::new()),
            },
            CollectionType::SmartCollection => match collection_key.smart_collection_id {
                Some(id) => self.items_for_smart_collection_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::Playlist => match collection_key.playlist_id {
                Some(id) => Box::pin(self.playlist_media_items_by_id(id)).await,
                None => Ok(Vec::new()),
            },
            CollectionType::SearchQuery => match collection_key.search_query.as_deref() {
                Some(query) => self.search_items(query).await,
                None => Ok(Vec::new()),
            },
            CollectionType::Movie
            | CollectionType::Episode
            | CollectionType::MusicVideo
            | CollectionType::OtherVideo
            | CollectionType::Song
            | CollectionType::Image
            | CollectionType::RemoteStream => match collection_key.media_item_id {
                Some(id) => self.media_item_by_id(id).await,
                None => Ok(Vec::new()),
            },
            CollectionType::RerunFirstRun
            | CollectionType::RerunRerun
            | CollectionType::FakeCollection
            | CollectionType::FakePlaylistItem => Ok(Vec::new()),
        }
    }

    async fn effective_deco_for_playout(
        &self,
        playout_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Option<EffectiveDeco>> {
        let deco_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "DecoId" FROM "Playout" WHERE "Id" = ? AND "DecoId" IS NOT NULL"#,
        )
        .bind(playout_id)
        .fetch_optional(&self.0)
        .await
        .repository()?;

        let Some(deco_id) = deco_id else {
            return Ok(None);
        };

        self.effective_deco(deco_id, media_by_key).await
    }

    async fn effective_deco(
        &self,
        deco_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Option<EffectiveDeco>> {
        let row = sqlx::query(
            r#"SELECT "BreakContentMode", "DefaultFillerCollectionType", "DefaultFillerCollectionId",
                    "DefaultFillerMediaItemId", "DefaultFillerMultiCollectionId",
                    "DefaultFillerSmartCollectionId", "DefaultFillerTrimToFit"
             FROM "Deco"
             WHERE "Id" = ?"#,
        )
        .bind(deco_id)
        .fetch_one(&self.0)
        .await.repository()?;
        let mode: DecoMode = decode_enum_column("deco mode", &row, "BreakContentMode")?;
        if mode == DecoMode::Disable {
            return Ok(None);
        }

        let rows = sqlx::query(
            r#"SELECT *
             FROM "DecoBreakContent"
             WHERE "DecoId" = ?
             ORDER BY "Id""#,
        )
        .bind(deco_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let mut break_content = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.try_get::<EntityId, _>("Id").repository()?;
            let content = deco_break_content_key(id);
            media_by_key.insert(
                content.clone(),
                self.items_for_deco_break_content_row(&row).await?,
            );
            break_content.push(EffectiveBreakContent {
                id,
                placement: decode_enum_column("deco break placement", &row, "Placement")?,
                content,
                collection_key: collection_key_from_deco_break_content_row(&row)?,
            });
        }

        let (default_filler_content, default_filler_collection_key) = self
            .effective_default_filler(deco_id, &row, media_by_key)
            .await?;

        Ok(Some(EffectiveDeco {
            id: deco_id,
            default_filler_content,
            default_filler_collection_key,
            default_filler_trim_to_fit: row.try_get("DefaultFillerTrimToFit").repository()?,
            break_content,
        }))
    }

    async fn effective_deco_template_items(
        &self,
        deco_template_id: EntityId,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<Vec<EffectiveDecoTemplateItem>> {
        let rows = sqlx::query(
            r#"SELECT "dti"."StartTime", "dti"."EndTime", "dti"."DecoId", "d"."BreakContentMode"
             FROM "DecoTemplateItem" AS "dti"
             JOIN "Deco" AS "d" ON "d"."Id" = "dti"."DecoId"
             WHERE "dti"."DecoTemplateId" = ?
             ORDER BY "dti"."StartTime", "dti"."Id""#,
        )
        .bind(deco_template_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let deco_id = row.try_get::<EntityId, _>("DecoId").repository()?;
            let mode: DecoMode = decode_enum_column(
                "deco template item BreakContentMode",
                &row,
                "BreakContentMode",
            )?;
            let selection = match mode {
                DecoMode::Inherit => EffectiveDecoSelection::Inherit,
                DecoMode::Override => EffectiveDecoSelection::Override(
                    self.effective_deco(deco_id, media_by_key).await?,
                ),
                DecoMode::Disable | DecoMode::Merge => EffectiveDecoSelection::Disable,
            };
            result.push(EffectiveDecoTemplateItem {
                start_time: decode_time_of_day_text(
                    "deco template item StartTime",
                    &row.try_get::<String, _>("StartTime").repository()?,
                )?,
                end_time: decode_time_of_day_text(
                    "deco template item EndTime",
                    &row.try_get::<String, _>("EndTime").repository()?,
                )?,
                selection,
            });
        }

        Ok(result)
    }

    async fn effective_default_filler(
        &self,
        deco_id: EntityId,
        row: &DbRow,
        media_by_key: &mut BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<(Option<String>, Option<SchedulingCollectionKey>)> {
        let collection_type = decode_optional_enum_column(
            "deco default_filler_collection_type",
            row,
            "DefaultFillerCollectionType",
        )?;
        let Some(collection_type) = collection_type else {
            return Ok((None, None));
        };

        let collection_key = SchedulingCollectionKey {
            collection_type,
            collection_id: row.try_get("DefaultFillerCollectionId").repository()?,
            media_item_id: row.try_get("DefaultFillerMediaItemId").repository()?,
            multi_collection_id: row.try_get("DefaultFillerMultiCollectionId").repository()?,
            smart_collection_id: row.try_get("DefaultFillerSmartCollectionId").repository()?,
            ..SchedulingCollectionKey::default()
        };
        let items = self.items_for_collection_key(&collection_key, 0).await?;
        if items.is_empty() {
            return Ok((None, None));
        }

        let content = deco_default_filler_key(deco_id);
        media_by_key.insert(content.clone(), items);
        Ok((Some(content), Some(collection_key)))
    }

    async fn items_for_deco_break_content_row(
        &self,
        row: &DbRow,
    ) -> Result<Vec<SchedulingMediaItem>> {
        let collection_key = collection_key_from_deco_break_content_row(row)?;
        self.items_for_collection_key(&collection_key, 0).await
    }

    async fn playout_items(&self, playout_id: EntityId) -> Result<Vec<PlayoutItem>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlayoutItem"
             WHERE "PlayoutId" = ?
             ORDER BY "Start", "Id""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.try_get::<EntityId, _>("Id").repository()?;
            let mut item = map_playout_item(&row)?;
            item.watermark_ids = load_ids(
                &self.0,
                r#"SELECT "WatermarkId"
                 FROM "PlayoutItemWatermark"
                 WHERE "PlayoutItemId" = ?
                 ORDER BY "WatermarkId""#,
                id,
            )
            .await?;
            item.graphics_element_ids = load_ids(
                &self.0,
                r#"SELECT "GraphicsElementId"
                 FROM "PlayoutItemGraphicsElement"
                 WHERE "PlayoutItemId" = ?
                 ORDER BY "GraphicsElementId""#,
                id,
            )
            .await?;
            items.push(item);
        }

        Ok(items)
    }

    async fn playout_history(&self, playout_id: EntityId) -> Result<Vec<PlayoutHistory>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlayoutHistory"
             WHERE "PlayoutId" = ?
             ORDER BY "When", "Id""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        rows.iter().map(map_playout_history).collect()
    }

    async fn program_schedule_anchors(
        &self,
        playout_id: EntityId,
    ) -> Result<Vec<PlayoutProgramScheduleAnchor>> {
        let rows = sqlx::query(
            r#"SELECT "a".*,
                      "s"."Seed" AS "EnumeratorSeed",
                      "s"."Index" AS "EnumeratorIndex"
             FROM "PlayoutProgramScheduleAnchor" AS "a"
             LEFT JOIN "CollectionEnumeratorState" AS "s"
                ON "s"."PlayoutProgramScheduleAnchorId" = "a"."Id"
             WHERE "a"."PlayoutId" = ?
             ORDER BY "a"."Id""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                Ok(PlayoutProgramScheduleAnchor {
                    id: row.try_get("Id").repository()?,
                    playout_id: row.try_get("PlayoutId").repository()?,
                    anchor_date: decode_optional_datetime(
                        "playout program schedule anchor anchor_date",
                        row.try_get::<Option<String>, _>("AnchorDate")
                            .repository()?,
                    )?,
                    collection_type: decode_enum_column(
                        "playout program schedule anchor collection_type",
                        row,
                        "CollectionType",
                    )?,
                    collection_id: row.try_get("CollectionId").repository()?,
                    multi_collection_id: row.try_get("MultiCollectionId").repository()?,
                    smart_collection_id: row.try_get("SmartCollectionId").repository()?,
                    rerun_collection_id: row.try_get("RerunCollectionId").repository()?,
                    media_item_id: row.try_get("MediaItemId").repository()?,
                    playlist_id: row.try_get("PlaylistId").repository()?,
                    search_query: row.try_get("SearchQuery").repository()?,
                    fake_collection_key: row.try_get("FakeCollectionKey").repository()?,
                    enumerator_state: CollectionEnumeratorState {
                        seed: row
                            .try_get::<Option<i32>, _>("EnumeratorSeed")
                            .repository()?
                            .unwrap_or_default(),
                        index: row
                            .try_get::<Option<i32>, _>("EnumeratorIndex")
                            .repository()?
                            .unwrap_or_default(),
                    },
                })
            })
            .collect()
    }

    async fn fill_group_indices(
        &self,
        playout_id: EntityId,
    ) -> Result<Vec<PlayoutScheduleItemFillGroupIndex>> {
        let rows = sqlx::query(
            r#"SELECT "i".*,
                      "s"."Seed" AS "EnumeratorSeed",
                      "s"."Index" AS "EnumeratorIndex"
             FROM "PlayoutScheduleItemFillGroupIndex" AS "i"
             LEFT JOIN "FillGroupEnumeratorState" AS "s"
                ON "s"."PlayoutScheduleItemFillGroupIndexId" = "i"."Id"
             WHERE "i"."PlayoutId" = ?
             ORDER BY "i"."Id""#,
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                Ok(PlayoutScheduleItemFillGroupIndex {
                    id: row.try_get("Id").repository()?,
                    playout_id: row.try_get("PlayoutId").repository()?,
                    program_schedule_item_id: row.try_get("ProgramScheduleItemId").repository()?,
                    enumerator_state: CollectionEnumeratorState {
                        seed: row
                            .try_get::<Option<i32>, _>("EnumeratorSeed")
                            .repository()?
                            .unwrap_or_default(),
                        index: row
                            .try_get::<Option<i32>, _>("EnumeratorIndex")
                            .repository()?
                            .unwrap_or_default(),
                    },
                })
            })
            .collect()
    }
}

fn reference_data_media_ids(reference_data: &SchedulingReferenceData) -> BTreeSet<EntityId> {
    let mut ids = BTreeSet::new();
    for items in reference_data
        .media_by_key
        .values()
        .chain(reference_data.collections_by_name.values())
        .chain(reference_data.multi_collections_by_name.values())
        .chain(reference_data.smart_collections_by_name.values())
        .chain(reference_data.playlists_by_group_and_name.values())
        .chain(reference_data.searches_by_query.values())
        .chain(reference_data.shows_by_name.values())
        .chain(reference_data.media_by_show_guid.values())
    {
        extend_media_item_ids(&mut ids, items);
    }
    for playlist_items in reference_data
        .playlist_items_by_group_and_name
        .values()
        .chain(reference_data.playlist_items_by_key.values())
    {
        for item in playlist_items {
            extend_media_item_ids(&mut ids, &item.media_items);
        }
    }
    ids.extend(reference_data.media_items_by_id.keys().copied());
    if let Some(fallback) = &reference_data.global_fallback {
        extend_media_item_ids(&mut ids, &fallback.media_items);
    }
    for preset in reference_data.classic_filler_presets.values() {
        extend_media_item_ids(&mut ids, &preset.media_items);
    }
    ids
}

fn extend_media_item_ids(ids: &mut BTreeSet<EntityId>, items: &[SchedulingMediaItem]) {
    ids.extend(items.iter().map(|item| item.id));
}

fn extend_smart_shuffle_tags(tags: &mut BTreeSet<String>, rule_id: &str, values: &[String]) {
    for value in values {
        extend_optional_smart_shuffle_tag(tags, rule_id, Some(value));
    }
}

fn extend_optional_smart_shuffle_tag(
    tags: &mut BTreeSet<String>,
    rule_id: &str,
    value: Option<&str>,
) {
    let Some(value) = value
        .map(normalize_smart_shuffle_value)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    tags.insert(format!("{rule_id}:{value}"));
}

fn normalize_smart_shuffle_value(value: &str) -> String {
    value.trim().to_lowercase()
}

fn program_schedule_item_filler_ids(item: &ProgramScheduleItem) -> impl Iterator<Item = EntityId> {
    [
        item.pre_roll_filler_id,
        item.mid_roll_filler_id,
        item.post_roll_filler_id,
        item.tail_filler_id,
        item.fallback_filler_id,
    ]
    .into_iter()
    .flatten()
}

const SEARCH_DOCUMENT_SQL: &str = r#"
SELECT
    "documents".*,
    "lp"."LibraryId" AS "library_id",
    "l"."Name" AS "library_name",
    "mi"."ContentKind" AS "content_kind",
    "mi"."SubKind" AS "sub_kind",
    "mi"."ParentMediaItemId" AS "parent_media_item_id"
FROM (
    SELECT
        "mm"."MovieId" AS "item_id",
        'movie' AS "media_kind",
        'Movie' AS "metadata_kind",
        "mm"."Id" AS "metadata_id",
        "mm"."Title" AS "title",
        "mm"."OriginalTitle" AS "original_title",
        "mm"."SortTitle" AS "sort_title",
        "mm"."Year" AS "year",
        "mm"."ReleaseDate" AS "release_date",
        "mm"."DateAdded" AS "added_date",
        "mm"."ContentRating" AS "content_rating",
        NULL AS "album",
        NULL AS "album_artists",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "MovieMetadata" AS "mm"
    UNION ALL
    SELECT
        "sm"."ShowId" AS "item_id",
        'show' AS "media_kind",
        'Show' AS "metadata_kind",
        "sm"."Id" AS "metadata_id",
        "sm"."Title" AS "title",
        "sm"."OriginalTitle" AS "original_title",
        "sm"."SortTitle" AS "sort_title",
        "sm"."Year" AS "year",
        "sm"."ReleaseDate" AS "release_date",
        "sm"."DateAdded" AS "added_date",
        "sm"."ContentRating" AS "content_rating",
        NULL AS "album",
        NULL AS "album_artists",
        "sm"."ShowId" AS "show_id",
        "sm"."Title" AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "ShowMetadata" AS "sm"
    UNION ALL
    SELECT
        "sem"."SeasonId" AS "item_id",
        'season' AS "media_kind",
        'Season' AS "metadata_kind",
        "sem"."Id" AS "metadata_id",
        "sem"."Title" AS "title",
        "sem"."OriginalTitle" AS "original_title",
        "sem"."SortTitle" AS "sort_title",
        "sem"."Year" AS "year",
        "sem"."ReleaseDate" AS "release_date",
        "sem"."DateAdded" AS "added_date",
        "shm"."ContentRating" AS "content_rating",
        NULL AS "album",
        NULL AS "album_artists",
        "se"."ShowId" AS "show_id",
        "shm"."Title" AS "show_title",
        "se"."SeasonNumber" AS "season_number",
        NULL AS "episode_number"
    FROM "SeasonMetadata" AS "sem"
    JOIN "Season" AS "se" ON "se"."Id" = "sem"."SeasonId"
    LEFT JOIN "ShowMetadata" AS "shm" ON "shm"."ShowId" = "se"."ShowId"
    UNION ALL
    SELECT
        "em"."EpisodeId" AS "item_id",
        'episode' AS "media_kind",
        'Episode' AS "metadata_kind",
        "em"."Id" AS "metadata_id",
        "em"."Title" AS "title",
        "em"."OriginalTitle" AS "original_title",
        "em"."SortTitle" AS "sort_title",
        "em"."Year" AS "year",
        "em"."ReleaseDate" AS "release_date",
        "em"."DateAdded" AS "added_date",
        COALESCE("em"."ContentRating", "shm"."ContentRating") AS "content_rating",
        NULL AS "album",
        NULL AS "album_artists",
        "s"."ShowId" AS "show_id",
        "shm"."Title" AS "show_title",
        "s"."SeasonNumber" AS "season_number",
        "em"."EpisodeNumber" AS "episode_number"
    FROM "EpisodeMetadata" AS "em"
    JOIN "Episode" AS "e" ON "e"."Id" = "em"."EpisodeId"
    JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId"
    LEFT JOIN "ShowMetadata" AS "shm" ON "shm"."ShowId" = "s"."ShowId"
    UNION ALL
    SELECT
        "mvm"."MusicVideoId" AS "item_id",
        'music_video' AS "media_kind",
        'MusicVideo' AS "metadata_kind",
        "mvm"."Id" AS "metadata_id",
        "mvm"."Title" AS "title",
        "mvm"."OriginalTitle" AS "original_title",
        "mvm"."SortTitle" AS "sort_title",
        "mvm"."Year" AS "year",
        "mvm"."ReleaseDate" AS "release_date",
        "mvm"."DateAdded" AS "added_date",
        "mvm"."ContentRating" AS "content_rating",
        "mvm"."Album" AS "album",
        NULL AS "album_artists",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "MusicVideoMetadata" AS "mvm"
    UNION ALL
    SELECT
        "am"."ArtistId" AS "item_id",
        'artist' AS "media_kind",
        'Artist' AS "metadata_kind",
        "am"."Id" AS "metadata_id",
        "am"."Title" AS "title",
        "am"."OriginalTitle" AS "original_title",
        "am"."SortTitle" AS "sort_title",
        "am"."Year" AS "year",
        "am"."ReleaseDate" AS "release_date",
        "am"."DateAdded" AS "added_date",
        NULL AS "content_rating",
        NULL AS "album",
        NULL AS "album_artists",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "ArtistMetadata" AS "am"
    UNION ALL
    SELECT
        "ovm"."OtherVideoId" AS "item_id",
        'other_video' AS "media_kind",
        'OtherVideo' AS "metadata_kind",
        "ovm"."Id" AS "metadata_id",
        "ovm"."Title" AS "title",
        "ovm"."OriginalTitle" AS "original_title",
        "ovm"."SortTitle" AS "sort_title",
        "ovm"."Year" AS "year",
        "ovm"."ReleaseDate" AS "release_date",
        "ovm"."DateAdded" AS "added_date",
        "ovm"."ContentRating" AS "content_rating",
        NULL AS "album",
        NULL AS "album_artists",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "OtherVideoMetadata" AS "ovm"
    UNION ALL
    SELECT
        "som"."SongId" AS "item_id",
        'song' AS "media_kind",
        'Song' AS "metadata_kind",
        "som"."Id" AS "metadata_id",
        "som"."Title" AS "title",
        "som"."OriginalTitle" AS "original_title",
        "som"."SortTitle" AS "sort_title",
        "som"."Year" AS "year",
        "som"."ReleaseDate" AS "release_date",
        "som"."DateAdded" AS "added_date",
        "som"."ContentRating" AS "content_rating",
        "som"."Album" AS "album",
        "som"."AlbumArtists" AS "album_artists",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "SongMetadata" AS "som"
    UNION ALL
    SELECT
        "im"."ImageId" AS "item_id",
        'image' AS "media_kind",
        'Image' AS "metadata_kind",
        "im"."Id" AS "metadata_id",
        "im"."Title" AS "title",
        "im"."OriginalTitle" AS "original_title",
        "im"."SortTitle" AS "sort_title",
        "im"."Year" AS "year",
        "im"."ReleaseDate" AS "release_date",
        "im"."DateAdded" AS "added_date",
        "im"."ContentRating" AS "content_rating",
        NULL AS "album",
        NULL AS "album_artists",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "ImageMetadata" AS "im"
    UNION ALL
    SELECT
        "rsm"."RemoteStreamId" AS "item_id",
        'remote_stream' AS "media_kind",
        'RemoteStream' AS "metadata_kind",
        "rsm"."Id" AS "metadata_id",
        "rsm"."Title" AS "title",
        "rsm"."OriginalTitle" AS "original_title",
        "rsm"."SortTitle" AS "sort_title",
        "rsm"."Year" AS "year",
        "rsm"."ReleaseDate" AS "release_date",
        "rsm"."DateAdded" AS "added_date",
        "rsm"."ContentRating" AS "content_rating",
        NULL AS "album",
        NULL AS "album_artists",
        NULL AS "show_id",
        NULL AS "show_title",
        NULL AS "season_number",
        NULL AS "episode_number"
    FROM "RemoteStreamMetadata" AS "rsm"
) AS "documents"
LEFT JOIN "MediaItem" AS "mi" ON "mi"."Id" = "documents"."item_id"
LEFT JOIN "LibraryPath" AS "lp" ON "lp"."Id" = "mi"."LibraryPathId"
LEFT JOIN "Library" AS "l" ON "l"."Id" = "lp"."LibraryId"
ORDER BY COALESCE("sort_title", "title"), "item_id""#;

fn collection_key_from_row(row: &DbRow) -> Result<SchedulingCollectionKey> {
    Ok(SchedulingCollectionKey {
        collection_type: decode_enum_column("collection type", row, "CollectionType")?,
        collection_id: optional_id(row, "CollectionId")?,
        multi_collection_id: optional_id(row, "MultiCollectionId")?,
        smart_collection_id: optional_id(row, "SmartCollectionId")?,
        media_item_id: optional_id(row, "MediaItemId")?,
        playlist_id: optional_id(row, "PlaylistId")?,
        search_query: row
            .try_get::<Option<String>, _>("SearchQuery")
            .ok()
            .flatten(),
        fake_collection_key: None,
    })
}

fn collection_key_from_deco_break_content_row(row: &DbRow) -> Result<SchedulingCollectionKey> {
    Ok(SchedulingCollectionKey {
        collection_type: decode_enum_column(
            "deco break content collection type",
            row,
            "CollectionType",
        )?,
        collection_id: optional_id(row, "CollectionId")?,
        media_item_id: optional_id(row, "MediaItemId")?,
        playlist_id: optional_id(row, "PlaylistId")?,
        ..SchedulingCollectionKey::default()
    })
}

fn optional_id(row: &DbRow, column: &'static str) -> Result<Option<EntityId>> {
    match row.try_get::<Option<EntityId>, _>(column) {
        Ok(value) => Ok(value),
        Err(sqlx::Error::ColumnNotFound(_)) => Ok(None),
        Err(error) => Err(super::common::map_data_access_error(column, &error)),
    }
}

fn map_scheduling_media_item(row: &DbRow) -> Result<SchedulingMediaItem> {
    let duration_text: String = row.try_get("duration_text").unwrap_or_default();
    let duration = decode_duration_text(&duration_text);
    Ok(SchedulingMediaItem {
        id: row.try_get("Id").repository()?,
        duration,
        title: row.try_get("title").repository()?,
        show_id: row.try_get("show_id").repository()?,
        season_id: row.try_get("season_id").repository()?,
        artist_id: row.try_get("artist_id").repository()?,
        season: row.try_get("season_number").repository()?,
        episode: row.try_get("episode_number").repository()?,
        release_date: row
            .try_get::<Option<String>, _>("release_date")
            .repository()?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_datetime("media item release_date", &value))
            .transpose()?,
        preferred_audio_language_code: None,
        preferred_audio_title: None,
        preferred_subtitle_language_code: None,
        subtitle_mode: None,
        pinned_stream_selection: None,
    })
}

fn map_playout_item(row: &DbRow) -> Result<PlayoutItem> {
    Ok(PlayoutItem {
        id: row.try_get("Id").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        start: decode_datetime(
            "playout item start",
            &row.try_get::<String, _>("Start").repository()?,
        )?,
        finish: decode_datetime(
            "playout item finish",
            &row.try_get::<String, _>("Finish").repository()?,
        )?,
        guide_start: decode_optional_datetime(
            "playout item guide_start",
            row.try_get("GuideStart").repository()?,
        )?,
        guide_finish: decode_optional_datetime(
            "playout item guide_finish",
            row.try_get("GuideFinish").repository()?,
        )?,
        custom_title: row.try_get("CustomTitle").repository()?,
        guide_group: row.try_get("GuideGroup").repository()?,
        filler_kind: decode_enum_column("playout item filler_kind", row, "FillerKind")?,
        playout_id: row.try_get("PlayoutId").repository()?,
        in_point: decode_duration_text(&row.try_get::<String, _>("InPoint").repository()?),
        out_point: decode_duration_text(&row.try_get::<String, _>("OutPoint").repository()?),
        chapter_title: row.try_get("ChapterTitle").repository()?,
        watermark_ids: Vec::new(),
        disable_watermarks: decode_bool_column(row, "DisableWatermarks")?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode").repository()?,
        preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
        preferred_subtitle_language_code: row
            .try_get("PreferredSubtitleLanguageCode")
            .repository()?,
        subtitle_mode: decode_optional_enum_column(
            "playout item subtitle_mode",
            row,
            "SubtitleMode",
        )?,
        pinned_stream_selection: pinned_stream_selection_from_row(row)?,
        block_key: row.try_get("BlockKey").repository()?,
        collection_key: row.try_get("CollectionKey").repository()?,
        collection_etag: row.try_get("CollectionEtag").repository()?,
        graphics_element_ids: Vec::new(),
        graphics_elements: Vec::new(),
        scheduling_context: row.try_get("SchedulingContext").repository()?,
    })
}

fn map_playout_history(row: &DbRow) -> Result<PlayoutHistory> {
    Ok(PlayoutHistory {
        id: row.try_get("Id").repository()?,
        playout_id: row.try_get("PlayoutId").repository()?,
        block_id: row.try_get("BlockId").repository()?,
        playback_order: decode_enum_column("playout history playback_order", row, "PlaybackOrder")?,
        key: row.try_get("Key").repository()?,
        child_key: row.try_get("ChildKey").repository()?,
        when: decode_datetime(
            "playout history When",
            &row.try_get::<String, _>("When").repository()?,
        )?,
        finish: decode_optional_datetime(
            "playout history Finish",
            row.try_get("Finish").repository()?,
        )?,
        index: row.try_get("Index").repository()?,
        details: row.try_get("Details").repository()?,
        is_current_child: row.try_get("IsCurrentChild").repository()?,
    })
}

fn map_program_schedule_alternate(row: &DbRow) -> Result<ProgramScheduleAlternate> {
    Ok(ProgramScheduleAlternate {
        id: row.try_get("Id").repository()?,
        program_schedule_id: row.try_get("ProgramScheduleId").repository()?,
        playout_id: row.try_get("PlayoutId").repository()?,
        sort_order: row.try_get("Index").repository()?,
        days_of_week: decode_string_list(
            row.try_get::<Option<String>, _>("DaysOfWeek")
                .repository()?,
        ),
        days_of_month: decode_int_list(
            row.try_get::<Option<String>, _>("DaysOfMonth")
                .repository()?,
        ),
        months_of_year: decode_int_list(
            row.try_get::<Option<String>, _>("MonthsOfYear")
                .repository()?,
        ),
        limit_to_date_range: decode_bool_column(row, "LimitToDateRange")?,
        start_month: row.try_get("StartMonth").repository()?,
        start_day: row.try_get("StartDay").repository()?,
        start_year: row.try_get("StartYear").repository()?,
        end_month: row.try_get("EndMonth").repository()?,
        end_day: row.try_get("EndDay").repository()?,
        end_year: row.try_get("EndYear").repository()?,
    })
}

fn alternate_applies_to(alternate: &ProgramScheduleAlternate, start: DateTime<Utc>) -> bool {
    let date = start.date_naive();
    let day_of_week = i32::try_from(date.weekday().num_days_from_sunday()).unwrap_or_default();
    if !alternate.days_of_week.is_empty()
        && !alternate
            .days_of_week
            .iter()
            .any(|value| day_of_week_matches(value, day_of_week))
    {
        return false;
    }
    if !alternate.days_of_month.is_empty()
        && !alternate
            .days_of_month
            .contains(&i32::try_from(date.day()).unwrap_or_default())
    {
        return false;
    }
    if !alternate.months_of_year.is_empty()
        && !alternate
            .months_of_year
            .contains(&i32::try_from(date.month()).unwrap_or_default())
    {
        return false;
    }
    if !alternate.limit_to_date_range {
        return true;
    }

    let Some(range_start) = NaiveDate::from_ymd_opt(
        alternate.start_year.unwrap_or(date.year()),
        u32::try_from(alternate.start_month).unwrap_or_default(),
        u32::try_from(alternate.start_day).unwrap_or_default(),
    ) else {
        return false;
    };
    let Some(range_end) = NaiveDate::from_ymd_opt(
        alternate.end_year.unwrap_or(date.year()),
        u32::try_from(alternate.end_month).unwrap_or_default(),
        u32::try_from(alternate.end_day).unwrap_or_default(),
    ) else {
        return false;
    };

    if range_start <= range_end {
        date >= range_start && date <= range_end
    } else {
        date >= range_start || date <= range_end
    }
}

fn day_of_week_matches(value: &str, day_of_week: i32) -> bool {
    value.trim().parse::<i32>().map_or_else(
        |_| {
            matches!(
                (value.trim().to_ascii_lowercase().as_str(), day_of_week),
                ("sunday", 0)
                    | ("monday", 1)
                    | ("tuesday", 2)
                    | ("wednesday", 3)
                    | ("thursday", 4)
                    | ("friday", 5)
                    | ("saturday", 6)
            )
        },
        |parsed| parsed == day_of_week,
    )
}

fn decode_int_list(value: Option<String>) -> Vec<i32> {
    value
        .unwrap_or_default()
        .split(',')
        .filter_map(|part| part.parse::<i32>().ok())
        .collect()
}

fn decode_string_list(value: Option<String>) -> Vec<String> {
    value
        .and_then(|inner| serde_json::from_str(&inner).ok())
        .unwrap_or_default()
}

fn block_item_content_key(id: EntityId) -> String {
    format!("block-item:{id}")
}

fn deco_break_content_key(id: EntityId) -> String {
    format!("deco-break-content:{id}")
}

fn deco_default_filler_key(id: EntityId) -> String {
    format!("deco-default-filler:{id}")
}

fn decode_integer_list(kind: &'static str, value: &str) -> Result<Vec<i32>> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(value).map_err(|source| decode_message(kind, &source.to_string()))
}

fn decode_optional_time_of_day_text(
    kind: &'static str,
    value: Option<String>,
) -> Result<Option<NaiveTime>> {
    value
        .map(|inner| decode_time_of_day_text(kind, &inner))
        .transpose()
}

fn decode_time_of_day_text(kind: &'static str, value: &str) -> Result<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M:%S%.f"))
        .map_err(|source| decode_message(kind, &source.to_string()))
}

fn parse_datetime(kind: &'static str, value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .map(|parsed| parsed.and_utc())
        })
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map(|parsed| parsed.and_hms_opt(0, 0, 0).unwrap().and_utc())
        })
        .map_err(|source| decode_message(kind, &source.to_string()))
}

async fn exists_by_id(pool: &Pool<crate::db::Db>, table: &str, id: EntityId) -> Result<bool> {
    let sql = format!(r#"SELECT EXISTS(SELECT 1 FROM "{table}" WHERE "Id" = ?)"#);
    let exists = sqlx::query_scalar::<_, i64>(crate::db::assert_sql_safe(&sql))
        .bind(id)
        .fetch_one(pool)
        .await
        .repository()?;
    Ok(exists != 0)
}

/// Dedupe key for expanded scheduling items. ADR 0048 allows the same media
/// item to appear more than once in a collection when the rows carry different
/// playback overrides, so the override tuple is part of the identity — two
/// rows for one movie with distinct commentary tracks must both schedule.
type SchedulingItemKey = (
    EntityId,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<u8>,
    Option<ersatzrs_domain::PinnedStreamSelection>,
);

fn scheduling_item_key(item: &SchedulingMediaItem) -> SchedulingItemKey {
    (
        item.id,
        item.preferred_audio_language_code.clone(),
        item.preferred_audio_title.clone(),
        item.preferred_subtitle_language_code.clone(),
        item.subtitle_mode.map(|mode| match mode {
            ChannelSubtitleMode::None => 0,
            ChannelSubtitleMode::Forced => 1,
            ChannelSubtitleMode::Default => 2,
            ChannelSubtitleMode::Any => 3,
        }),
        item.pinned_stream_selection,
    )
}

fn append_unique(
    target: &mut Vec<SchedulingMediaItem>,
    seen: &mut BTreeSet<SchedulingItemKey>,
    items: Vec<SchedulingMediaItem>,
) {
    for item in items {
        if seen.insert(scheduling_item_key(&item)) {
            target.push(item);
        }
    }
}

#[derive(Debug, Clone, Default)]
struct PlaybackOverride {
    preferred_audio_language_code: Option<String>,
    preferred_audio_title: Option<String>,
    preferred_subtitle_language_code: Option<String>,
    subtitle_mode: Option<ChannelSubtitleMode>,
    pinned_stream_selection: Option<ersatzrs_domain::PinnedStreamSelection>,
}

fn playback_override_from_row(row: &DbRow) -> Result<PlaybackOverride> {
    Ok(PlaybackOverride {
        preferred_audio_language_code: optional_string_column(row, "PreferredAudioLanguageCode")?,
        preferred_audio_title: optional_string_column(row, "PreferredAudioTitle")?,
        preferred_subtitle_language_code: optional_string_column(
            row,
            "PreferredSubtitleLanguageCode",
        )?,
        subtitle_mode: optional_subtitle_mode_column(row, "SubtitleMode")?,
        pinned_stream_selection: pinned_stream_selection_from_row(row)?,
    })
}

fn optional_string_column(row: &DbRow, column: &'static str) -> Result<Option<String>> {
    match row.try_get::<Option<String>, _>(column) {
        Ok(value) => Ok(value.filter(|value| !value.trim().is_empty())),
        Err(sqlx::Error::ColumnNotFound(_)) => Ok(None),
        Err(error) => Err(super::common::map_data_access_error(column, &error)),
    }
}

fn optional_i32_column(row: &DbRow, column: &'static str) -> Result<Option<i32>> {
    match row.try_get::<Option<i32>, _>(column) {
        Ok(value) => Ok(value),
        Err(sqlx::Error::ColumnNotFound(_)) => Ok(None),
        Err(error) => Err(super::common::map_data_access_error(column, &error)),
    }
}

fn pinned_stream_selection_from_row(
    row: &DbRow,
) -> Result<Option<ersatzrs_domain::PinnedStreamSelection>> {
    let Some(media_version_id) = optional_id(row, "MediaVersionId")? else {
        return Ok(None);
    };
    let Some(video_stream_index) = optional_i32_column(row, "VideoStreamIndex")? else {
        return Ok(None);
    };
    Ok(Some(ersatzrs_domain::PinnedStreamSelection {
        media_version_id,
        video_stream_index,
        audio_stream_index: optional_i32_column(row, "AudioStreamIndex")?,
        subtitle_stream_index: optional_i32_column(row, "SubtitleStreamIndex")?,
    }))
}

fn optional_subtitle_mode_column(
    row: &DbRow,
    column: &'static str,
) -> Result<Option<ChannelSubtitleMode>> {
    match row.try_get::<Option<i32>, _>(column) {
        Ok(Some(0) | None) | Err(sqlx::Error::ColumnNotFound(_)) => Ok(None),
        Ok(Some(1)) => Ok(Some(ChannelSubtitleMode::Forced)),
        Ok(Some(2)) => Ok(Some(ChannelSubtitleMode::Default)),
        Ok(Some(3)) => Ok(Some(ChannelSubtitleMode::Any)),
        Ok(Some(value)) => Err(decode_message(
            "playback override subtitle_mode",
            &format!("unknown enum value {value}"),
        )),
        Err(error) => Err(super::common::map_data_access_error(column, &error)),
    }
}

fn apply_playback_override(items: &mut [SchedulingMediaItem], playback: &PlaybackOverride) {
    if playback.preferred_audio_language_code.is_none()
        && playback.preferred_audio_title.is_none()
        && playback.preferred_subtitle_language_code.is_none()
        && playback.subtitle_mode.is_none()
        && playback.pinned_stream_selection.is_none()
    {
        return;
    }

    for item in items {
        item.preferred_audio_language_code
            .clone_from(&playback.preferred_audio_language_code);
        item.preferred_audio_title
            .clone_from(&playback.preferred_audio_title);
        item.preferred_subtitle_language_code
            .clone_from(&playback.preferred_subtitle_language_code);
        item.subtitle_mode = playback.subtitle_mode;
        item.pinned_stream_selection = playback.pinned_stream_selection;
    }
}

#[derive(Debug, Clone, Default)]
struct StreamSearchFields {
    audio_languages: Vec<String>,
    subtitle_languages: Vec<String>,
    video_codecs: Vec<String>,
    video_bit_depths: Vec<i32>,
    video_dynamic_ranges: Vec<String>,
    audio_codecs: Vec<String>,
    audio_titles: Vec<String>,
    subtitle_codecs: Vec<String>,
    subtitle_titles: Vec<String>,
    has_commentary: bool,
    library_folder_ids: Vec<i64>,
    width: Option<i32>,
    height: Option<i32>,
    chapter_counts: Vec<i32>,
    durations_seconds: Vec<i64>,
}

fn metadata_fk(metadata_kind: &str) -> Option<&'static str> {
    match metadata_kind {
        "Movie" => Some("MovieMetadataId"),
        "Episode" => Some("EpisodeMetadataId"),
        "Show" => Some("ShowMetadataId"),
        "Season" => Some("SeasonMetadataId"),
        "MusicVideo" => Some("MusicVideoMetadataId"),
        "Artist" => Some("ArtistMetadataId"),
        "OtherVideo" => Some("OtherVideoMetadataId"),
        "Song" => Some("SongMetadataId"),
        "Image" => Some("ImageMetadataId"),
        "RemoteStream" => Some("RemoteStreamMetadataId"),
        _ => None,
    }
}

fn media_version_fk(metadata_kind: &str) -> Option<&'static str> {
    match metadata_kind {
        "Movie" => Some("MovieId"),
        "Episode" => Some("EpisodeId"),
        "MusicVideo" => Some("MusicVideoId"),
        "OtherVideo" => Some("OtherVideoId"),
        "Song" => Some("SongId"),
        "Image" => Some("ImageId"),
        "RemoteStream" => Some("RemoteStreamId"),
        _ => None,
    }
}

fn media_stream_kind_from_row(row: &DbRow) -> Option<String> {
    if let Ok(value) = row.try_get::<i64, _>("MediaStreamKind") {
        return match value {
            1 => Some("video".to_owned()),
            2 => Some("audio".to_owned()),
            3 => Some("subtitle".to_owned()),
            5 => Some("external_subtitle".to_owned()),
            _ => None,
        };
    }
    row.try_get::<String, _>("MediaStreamKind")
        .ok()
        .map(|value| value.to_ascii_lowercase())
}

fn push_unique_optional(target: &mut Vec<String>, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        push_unique(target, value);
    }
}

fn push_stream_title(target: &mut Vec<String>, has_commentary: &mut bool, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        if value.to_ascii_lowercase().contains("commentary") {
            *has_commentary = true;
        }
        push_unique(target, value);
    }
}

fn option_values_vec<const N: usize>(values: [Option<String>; N]) -> Vec<String> {
    values
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn split_content_ratings(value: Option<String>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| {
            value
                .split('/')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn parse_string_list(value: Option<String>) -> Vec<String> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Vec::new();
    };

    serde_json::from_str::<Vec<String>>(&value)
        .unwrap_or_else(|_| {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect()
}

#[derive(Debug, Clone)]
struct SmartCollectionReference {
    start: usize,
    end: usize,
    name: String,
}

fn smart_collection_references(query: &str) -> Vec<SmartCollectionReference> {
    const NEEDLE: &str = "smart_collection:\"";

    let mut references = Vec::new();
    let mut offset = 0usize;
    while let Some(relative_start) = query[offset..].find(NEEDLE) {
        let start = offset + relative_start;
        let name_start = start + NEEDLE.len();
        let Some(relative_end) = query[name_start..].find('"') else {
            break;
        };
        let name_end = name_start + relative_end;
        references.push(SmartCollectionReference {
            start,
            end: name_end + 1,
            name: query[name_start..name_end].to_owned(),
        });
        offset = name_end + 1;
    }

    references
}

fn push_unique(target: &mut Vec<String>, value: String) {
    if !target
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        target.push(value);
    }
}

fn push_unique_i32(target: &mut Vec<i32>, value: i32) {
    if !target.contains(&value) {
        target.push(value);
    }
}

fn push_unique_i64(target: &mut Vec<i64>, value: i64) {
    if !target.contains(&value) {
        target.push(value);
    }
}

fn video_bit_depth_from_row(row: &DbRow) -> Option<i32> {
    let pixel_format = row
        .try_get::<Option<String>, _>("PixelFormat")
        .ok()
        .flatten();
    if let Some(pixel_format) = pixel_format.as_deref().map(str::to_ascii_lowercase) {
        if pixel_format.contains("10") {
            return Some(10);
        }
        if pixel_format.contains("12") {
            return Some(12);
        }
        if pixel_format.contains("16") {
            return Some(16);
        }
        if matches!(
            pixel_format.trim(),
            "yuv420p" | "yuvj420p" | "nv12" | "vaapi" | "cuda"
        ) {
            return Some(8);
        }
    }

    row.try_get::<i32, _>("BitsPerRawSample")
        .ok()
        .filter(|value| *value > 0)
}

fn video_dynamic_range_from_row(row: &DbRow) -> &'static str {
    let transfer = row
        .try_get::<Option<String>, _>("ColorTransfer")
        .ok()
        .flatten()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(transfer.as_str(), "arib-std-b67" | "smpte2084") {
        "hdr"
    } else {
        "sdr"
    }
}

fn search_sort_title(document: &SearchDocument) -> String {
    document
        .sort_title
        .as_deref()
        .or(document.title.as_deref())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn normalize_graphics_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if normalized.contains("templates/graphics-elements/") {
        normalized
    } else {
        format!("templates/graphics-elements/{normalized}")
    }
}

#[async_trait]
impl AdminSearchRepository for SqliteSchedulingContentRepository {
    async fn search_admin_documents(&self, query: &str) -> Result<Vec<SearchDocument>> {
        let (documents, _, _) = self.search_documents_page(query, 0, 0).await?;
        Ok(documents)
    }

    #[allow(clippy::too_many_lines)]
    async fn enrich_admin_search(
        &self,
        kind: AdminSearchKind,
        ids: &[EntityId],
    ) -> Result<AdminSearchEnrichment> {
        let mut enrichment = AdminSearchEnrichment::default();
        if ids.is_empty() {
            return Ok(enrichment);
        }

        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let state_sql =
            format!(r#"SELECT "Id", "State" FROM "MediaItem" WHERE "Id" IN ({placeholders})"#);
        for row in fetch_admin_search_rows(&self.0, &state_sql, ids).await? {
            let id = row.try_get("Id").repository()?;
            let state_value = row.try_get("State").repository()?;
            enrichment
                .state
                .insert(id, admin_search_state(state_value)?);
        }

        match kind {
            AdminSearchKind::Movie => {
                let sql = format!(
                    r#"SELECT mm."MovieId" AS "ItemId", a."Path" AS "Path"
                       FROM "MovieMetadata" mm
                       JOIN "Artwork" a
                         ON a."MovieMetadataId" = mm."Id" AND a."ArtworkKind" = 0
                       WHERE mm."MovieId" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    enrichment.poster.insert(
                        row.try_get("ItemId").repository()?,
                        row.try_get("Path").repository()?,
                    );
                }
            }
            AdminSearchKind::Show => {
                let sql = format!(
                    r#"SELECT sm."ShowId" AS "ItemId", a."Path" AS "Path"
                       FROM "ShowMetadata" sm
                       JOIN "Artwork" a
                         ON a."ShowMetadataId" = sm."Id" AND a."ArtworkKind" = 0
                       WHERE sm."ShowId" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    enrichment.poster.insert(
                        row.try_get("ItemId").repository()?,
                        row.try_get("Path").repository()?,
                    );
                }
            }
            AdminSearchKind::Season => {
                let sql = format!(
                    r#"SELECT sea."Id" AS "SeasonId",
                              sea_art."Path" AS "OwnPath",
                              show_art."Path" AS "ShowPath"
                       FROM "Season" sea
                       LEFT JOIN "SeasonMetadata" sem ON sem."SeasonId" = sea."Id"
                       LEFT JOIN "Artwork" sea_art
                         ON sea_art."SeasonMetadataId" = sem."Id"
                         AND sea_art."ArtworkKind" = 0
                       LEFT JOIN "ShowMetadata" shm ON shm."ShowId" = sea."ShowId"
                       LEFT JOIN "Artwork" show_art
                         ON show_art."ShowMetadataId" = shm."Id"
                         AND show_art."ArtworkKind" = 0
                       WHERE sea."Id" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    let id = row.try_get("SeasonId").repository()?;
                    if let Some(path) = row.try_get("OwnPath").repository()? {
                        enrichment.poster.insert(id, path);
                    }
                    if let Some(path) = row.try_get("ShowPath").repository()? {
                        enrichment.season_poster.insert(id, path);
                    }
                }
            }
            AdminSearchKind::Episode => {
                let sql = format!(
                    r#"SELECT ep."Id" AS "EpisodeId",
                              ep."SeasonId" AS "SeasonId",
                              ep_art."Path" AS "OwnPath",
                              sea_art."Path" AS "SeasonPath",
                              show_art."Path" AS "ShowPath"
                       FROM "Episode" ep
                       LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = ep."Id"
                       LEFT JOIN "Artwork" ep_art
                         ON ep_art."EpisodeMetadataId" = em."Id"
                         AND ep_art."ArtworkKind" = 0
                       LEFT JOIN "Season" sea ON sea."Id" = ep."SeasonId"
                       LEFT JOIN "SeasonMetadata" sem ON sem."SeasonId" = sea."Id"
                       LEFT JOIN "Artwork" sea_art
                         ON sea_art."SeasonMetadataId" = sem."Id"
                         AND sea_art."ArtworkKind" = 0
                       LEFT JOIN "ShowMetadata" shm ON shm."ShowId" = sea."ShowId"
                       LEFT JOIN "Artwork" show_art
                         ON show_art."ShowMetadataId" = shm."Id"
                         AND show_art."ArtworkKind" = 0
                       WHERE ep."Id" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    let id = row.try_get("EpisodeId").repository()?;
                    enrichment
                        .episode_season_id
                        .insert(id, row.try_get("SeasonId").repository()?);
                    if let Some(path) = row.try_get("OwnPath").repository()? {
                        enrichment.poster.insert(id, path);
                    }
                    if let Some(path) = row.try_get("SeasonPath").repository()? {
                        enrichment.season_poster.insert(id, path);
                    }
                    if let Some(path) = row.try_get("ShowPath").repository()? {
                        enrichment.show_poster.insert(id, path);
                    }
                }
            }
            AdminSearchKind::Artist => {
                let sql = format!(
                    r#"SELECT am."ArtistId" AS "ItemId",
                              am."Disambiguation" AS "Disambiguation",
                              a."Path" AS "Path"
                       FROM "ArtistMetadata" am
                       LEFT JOIN "Artwork" a
                         ON a."ArtistMetadataId" = am."Id" AND a."ArtworkKind" = 1
                       WHERE am."ArtistId" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    let id = row.try_get("ItemId").repository()?;
                    if let Some(path) = row.try_get("Path").repository()? {
                        enrichment.thumbnail.insert(id, path);
                    }
                    if let Some(value) = row.try_get("Disambiguation").repository()? {
                        enrichment.artist_disambiguation.insert(id, value);
                    }
                }
            }
            AdminSearchKind::MusicVideo => {
                let sql = format!(
                    r#"SELECT mv."Id" AS "ItemId",
                              mv."ArtistId" AS "ArtistId",
                              a."Path" AS "Path",
                              am."Title" AS "ArtistTitle"
                       FROM "MusicVideo" mv
                       LEFT JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = mv."Id"
                       LEFT JOIN "Artwork" a
                         ON a."MusicVideoMetadataId" = mvm."Id" AND a."ArtworkKind" = 1
                       LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = mv."ArtistId"
                       WHERE mv."Id" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    let id = row.try_get("ItemId").repository()?;
                    if let Some(value) = row.try_get("ArtistId").repository()? {
                        enrichment.music_video_artist_id.insert(id, value);
                    }
                    if let Some(path) = row.try_get("Path").repository()? {
                        enrichment.thumbnail.insert(id, path);
                    }
                    if let Some(value) = row.try_get("ArtistTitle").repository()? {
                        enrichment.music_video_artist.insert(id, value);
                    }
                }
            }
            AdminSearchKind::OtherVideo => {
                let sql = format!(
                    r#"SELECT ovm."OtherVideoId" AS "ItemId",
                              thumb."Path" AS "ThumbPath",
                              poster."Path" AS "PosterPath"
                       FROM "OtherVideoMetadata" ovm
                       LEFT JOIN "Artwork" thumb
                         ON thumb."OtherVideoMetadataId" = ovm."Id"
                         AND thumb."ArtworkKind" = 1
                       LEFT JOIN "Artwork" poster
                         ON poster."OtherVideoMetadataId" = ovm."Id"
                         AND poster."ArtworkKind" = 0
                       WHERE ovm."OtherVideoId" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    let id = row.try_get("ItemId").repository()?;
                    if let Some(path) = row.try_get("ThumbPath").repository()? {
                        enrichment.thumbnail.insert(id, path);
                    }
                    if let Some(path) = row.try_get("PosterPath").repository()? {
                        enrichment.poster.insert(id, path);
                    }
                }
            }
            AdminSearchKind::Song => {
                let sql = format!(
                    r#"SELECT som."SongId" AS "ItemId",
                              a."Path" AS "Path",
                              som."Artists" AS "Artists",
                              som."Album" AS "Album"
                       FROM "SongMetadata" som
                       LEFT JOIN "Artwork" a
                         ON a."SongMetadataId" = som."Id" AND a."ArtworkKind" = 1
                       WHERE som."SongId" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    let id = row.try_get("ItemId").repository()?;
                    if let Some(path) = row.try_get("Path").repository()? {
                        enrichment.thumbnail.insert(id, path);
                    }
                    enrichment.song_meta.insert(
                        id,
                        (
                            row.try_get("Artists").repository()?,
                            row.try_get("Album").repository()?,
                        ),
                    );
                }
            }
            AdminSearchKind::Image => {
                let sql = format!(
                    r#"SELECT im."ImageId" AS "ItemId", a."Path" AS "Path"
                       FROM "ImageMetadata" im
                       LEFT JOIN "Artwork" a
                         ON a."ImageMetadataId" = im."Id" AND a."ArtworkKind" = 1
                       WHERE im."ImageId" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    let id = row.try_get("ItemId").repository()?;
                    if let Some(path) = row.try_get("Path").repository()? {
                        enrichment.thumbnail.insert(id, path);
                    }
                }
            }
            AdminSearchKind::RemoteStream => {
                let sql = format!(
                    r#"SELECT rsm."RemoteStreamId" AS "ItemId", a."Path" AS "Path"
                       FROM "RemoteStreamMetadata" rsm
                       LEFT JOIN "Artwork" a
                         ON a."RemoteStreamMetadataId" = rsm."Id"
                         AND a."ArtworkKind" = 1
                       WHERE rsm."RemoteStreamId" IN ({placeholders})"#
                );
                for row in fetch_admin_search_rows(&self.0, &sql, ids).await? {
                    let id = row.try_get("ItemId").repository()?;
                    if let Some(path) = row.try_get("Path").repository()? {
                        enrichment.thumbnail.insert(id, path);
                    }
                }
            }
        }

        Ok(enrichment)
    }
}
