#[cfg(test)]
#[path = "block_preservation_tests.rs"]
mod preservation_tests;

#[path = "block_preservation.rs"]
mod preservation;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use ersatzrs_domain::{
    BlockStopScheduling, CollectionType, DecoBreakPlacement, EntityId, FillerKind, PlaybackOrder,
    PlayoutItem,
};
use serde::{Deserialize, Serialize};

use super::{
    cleanup_history, history, lookup_media, playout_item, PlayoutSchedulingEngine,
    SchedulingCollectionKey, SchedulingError, SchedulingMediaItem, SchedulingRequest,
    SchedulingResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveBlock {
    pub block_id: EntityId,
    pub block_key: Option<BlockKey>,
    pub template_item_id: EntityId,
    pub name: String,
    pub start: DateTime<Utc>,
    pub duration: Duration,
    pub stop_scheduling: BlockStopScheduling,
    pub items: Vec<EffectiveBlockItem>,
    pub deco: Option<EffectiveDeco>,
}

impl EffectiveBlock {
    #[must_use]
    pub fn from_templates(
        schedules: &[BlockTemplateSchedule],
        start: DateTime<Utc>,
        days_to_build: i64,
    ) -> Vec<Self> {
        let finish = start + Duration::days(days_to_build);
        let Some(mut day) = Utc
            .with_ymd_and_hms(start.year(), start.month(), start.day(), 0, 0, 0)
            .single()
        else {
            return Vec::new();
        };

        let mut effective_blocks = Vec::new();
        while day < finish {
            if let Some(schedule) = schedules
                .iter()
                .filter(|schedule| schedule.applies_to(day))
                .min_by_key(|schedule| schedule.index)
            {
                for item in &schedule.template.items {
                    if let Some(block) = schedule.blocks.get(&item.block_id) {
                        let start_time = item.start_time.unwrap_or_else(|| {
                            let minutes = item.sort_order.max(0);
                            NaiveTime::from_num_seconds_from_midnight_opt(
                                u32::try_from(minutes).unwrap_or_default() * 60,
                                0,
                            )
                            .unwrap_or(NaiveTime::MIN)
                        });
                        let block_start = day
                            + Duration::hours(i64::from(start_time.hour()))
                            + Duration::minutes(i64::from(start_time.minute()));
                        effective_blocks.push(Self {
                            block_id: block.id,
                            block_key: Some(BlockKey {
                                block_id: block.id,
                                block_updated_ticks: block.updated_ticks,
                                template_id: schedule.template.id,
                                template_updated_ticks: schedule.template.updated_ticks,
                                playout_template_id: schedule.id,
                                playout_template_updated_ticks: schedule.updated_ticks,
                            }),
                            template_item_id: item.id,
                            name: block.name.clone(),
                            start: block_start,
                            duration: block.duration,
                            stop_scheduling: block.stop_scheduling,
                            items: normalize_guide_mode(block.items.clone()),
                            deco: schedule.deco_for(block_start),
                        });
                    }
                }
            }
            day += Duration::days(1);
        }

        effective_blocks
            .retain(|block| block.start + block.duration >= start && block.start <= finish);
        effective_blocks.sort_by_key(|block| block.start);
        effective_blocks
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveBlockItem {
    pub id: EntityId,
    pub block_id: EntityId,
    pub content: String,
    pub collection_key: SchedulingCollectionKey,
    pub playback_order: PlaybackOrder,
    pub include_in_program_guide: bool,
    pub disable_watermarks: bool,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockKey {
    #[serde(rename = "b")]
    pub block_id: EntityId,
    #[serde(rename = "bt")]
    pub block_updated_ticks: i64,
    #[serde(rename = "t")]
    pub template_id: EntityId,
    #[serde(rename = "tt")]
    pub template_updated_ticks: i64,
    #[serde(rename = "pt")]
    pub playout_template_id: EntityId,
    #[serde(rename = "ptt")]
    pub playout_template_updated_ticks: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveDeco {
    pub id: EntityId,
    pub default_filler_content: Option<String>,
    pub default_filler_collection_key: Option<SchedulingCollectionKey>,
    pub default_filler_trim_to_fit: bool,
    pub break_content: Vec<EffectiveBreakContent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveBreakContent {
    pub id: EntityId,
    pub placement: DecoBreakPlacement,
    pub content: String,
    pub collection_key: SchedulingCollectionKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveDecoTemplateItem {
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub selection: EffectiveDecoSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectiveDecoSelection {
    Inherit,
    Disable,
    Override(Option<Box<EffectiveDeco>>),
}

impl EffectiveDecoSelection {
    fn resolve(&self, channel_deco: Option<&EffectiveDeco>) -> Option<EffectiveDeco> {
        match self {
            Self::Inherit => channel_deco.cloned(),
            Self::Disable => None,
            Self::Override(deco) => deco.as_deref().cloned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockTemplateSchedule {
    pub id: EntityId,
    pub updated_ticks: i64,
    pub start: Option<DateTime<Utc>>,
    pub finish: Option<DateTime<Utc>>,
    pub index: i32,
    pub days_of_week: Vec<i32>,
    pub days_of_month: Vec<i32>,
    pub months_of_year: Vec<i32>,
    pub limit_to_date_range: bool,
    pub start_month: i32,
    pub start_day: i32,
    pub start_year: Option<i32>,
    pub end_month: i32,
    pub end_day: i32,
    pub end_year: Option<i32>,
    pub template: BlockTemplate,
    pub blocks: BTreeMap<EntityId, BlockDefinition>,
    pub deco: Option<EffectiveDeco>,
    pub deco_template_items: Vec<EffectiveDecoTemplateItem>,
}

impl BlockTemplateSchedule {
    fn applies_to(&self, day: DateTime<Utc>) -> bool {
        if !(self.start.is_none_or(|start| day >= start)
            && self.finish.is_none_or(|finish| day < finish))
        {
            return false;
        }

        let day_date = day.date_naive();
        if self.limit_to_date_range && !date_matches_range(self, day_date) {
            return false;
        }

        matches_integer_filter(&self.days_of_week, csharp_day_of_week(day.weekday()))
            && matches_integer_filter(
                &self.days_of_month,
                i32::try_from(day.day()).unwrap_or_default(),
            )
            && matches_integer_filter(
                &self.months_of_year,
                i32::try_from(day.month()).unwrap_or_default(),
            )
    }

    fn deco_for(&self, block_start: DateTime<Utc>) -> Option<EffectiveDeco> {
        let time_of_day = block_start.time();
        for item in &self.deco_template_items {
            if (item.start_time <= time_of_day && item.end_time == NaiveTime::MIN)
                || item.end_time > time_of_day
            {
                return item.selection.resolve(self.deco.as_ref());
            }
        }

        self.deco.clone()
    }
}

fn matches_integer_filter(values: &[i32], needle: i32) -> bool {
    values.is_empty() || values.contains(&needle)
}

fn csharp_day_of_week(weekday: chrono::Weekday) -> i32 {
    match weekday {
        chrono::Weekday::Mon => 1,
        chrono::Weekday::Tue => 2,
        chrono::Weekday::Wed => 3,
        chrono::Weekday::Thu => 4,
        chrono::Weekday::Fri => 5,
        chrono::Weekday::Sat => 6,
        chrono::Weekday::Sun => 0,
    }
}

fn date_matches_range(schedule: &BlockTemplateSchedule, day: NaiveDate) -> bool {
    let mut reverse = schedule.start_month * 100 + schedule.start_day
        > schedule.end_month * 100 + schedule.end_day;

    let mut start_year = day.year();
    let mut end_year = day.year();
    if let (Some(explicit_start_year), Some(explicit_end_year)) =
        (schedule.start_year, schedule.end_year)
    {
        start_year = explicit_start_year;
        end_year = explicit_end_year;
        reverse = false;
    }

    let mut start = start_range_date(start_year, schedule.start_month, schedule.start_day);
    let mut end = end_range_date(end_year, schedule.end_month, schedule.end_day);

    if reverse {
        std::mem::swap(&mut start, &mut end);
        !(day > start && day < end)
    } else {
        day >= start && day <= end
    }
}

fn start_range_date(year: i32, month: i32, day: i32) -> NaiveDate {
    let month = month.clamp(1, 12).cast_unsigned();
    let day = day.max(1).cast_unsigned();
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or_else(|| {
        let month_start = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month start");
        month_start
            + Duration::days(i64::from(days_in_month(
                month_start.year(),
                month_start.month(),
            )))
    })
}

fn end_range_date(year: i32, month: i32, day: i32) -> NaiveDate {
    let month = month.clamp(1, 12).cast_unsigned();
    let day = day.max(1).cast_unsigned();
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or_else(|| {
        NaiveDate::from_ymd_opt(year, month, days_in_month(year, month)).expect("valid month end")
    })
}

fn days_in_month(year: i32, month: u32) -> u32 {
    (28..=31)
        .rev()
        .find(|day| NaiveDate::from_ymd_opt(year, month, *day).is_some())
        .unwrap_or(28)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub id: EntityId,
    pub updated_ticks: i64,
    pub items: Vec<BlockTemplateItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockTemplateItem {
    pub id: EntityId,
    pub block_id: EntityId,
    pub start_time: Option<NaiveTime>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockDefinition {
    pub id: EntityId,
    pub updated_ticks: i64,
    pub name: String,
    pub duration: Duration,
    pub stop_scheduling: BlockStopScheduling,
    pub items: Vec<EffectiveBlockItem>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BlockSchedulingEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlockAnchorContext {
    #[serde(default = "block_anchor_context_version")]
    version: u32,
    #[serde(default, alias = "cooldown")]
    smart_shuffle: super::SchedulingSmartShuffleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    history_start: Option<DateTime<Utc>>,
}

impl Default for BlockAnchorContext {
    fn default() -> Self {
        Self {
            version: block_anchor_context_version(),
            smart_shuffle: super::SchedulingSmartShuffleState::default(),
            history_start: None,
        }
    }
}

fn block_anchor_context_version() -> u32 {
    1
}

impl PlayoutSchedulingEngine for BlockSchedulingEngine {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        self.schedule_cancellable(request, &|| false)
    }

    fn schedule_cancellable(
        &self,
        mut request: SchedulingRequest,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<SchedulingResult, SchedulingError> {
        if cancelled() {
            return Err(SchedulingError::Cancelled);
        }
        super::PlayoutBuildRequest::new(request.playout.id, request.mode)
            .with_preservation(request.reference_data.preservation)
            .validate_preservation(request.playout.schedule_kind)?;
        let preservation = preservation::Preservation::prepare(&mut request)?;
        let mut result = SchedulingResult::default();
        let mut smart_shuffle_state = preservation.smart_shuffle.clone();

        let mut blocks = request.reference_data.effective_blocks.clone();
        blocks.sort_by_key(|block| block.start);
        result.remove_before = blocks
            .first()
            .map(|block| block.start.min(request.start))
            .or(Some(request.start));

        let collection_etags = collection_etags(&request.reference_data, &blocks);
        let change_set = if preservation.rebuild {
            BlockChangeSet {
                blocks_to_update: blocks.clone(),
                items_to_remove: Vec::new(),
            }
        } else {
            block_changes(&request, &blocks, &collection_etags)
        };
        result.items_to_remove.extend(change_set.items_to_remove);
        if request.mode == super::PlayoutBuildMode::Reset && !preservation.rebuild {
            // Rebuild filler from the current deco while retaining primary and
            // guide-hidden content. Mirror-retained filler before the rebuild
            // boundary remains available to delayed channels.
            for item in &request.reference_data.existing_items {
                if !matches!(item.filler_kind, FillerKind::None | FillerKind::GuideMode)
                    && result
                        .remove_before
                        .is_some_and(|start| item.finish >= start)
                    && !result.items_to_remove.contains(&item.id)
                {
                    result.items_to_remove.push(item.id);
                }
            }
        }
        result.history_to_remove.extend(history_to_remove_for_items(
            &request,
            &result.items_to_remove,
        ));
        let block_starts_to_update = change_set
            .blocks_to_update
            .iter()
            .map(|block| block.start)
            .collect::<BTreeSet<_>>();

        schedule_updated_blocks(
            &request,
            blocks,
            &block_starts_to_update,
            &collection_etags,
            &mut smart_shuffle_state,
            &mut result,
        )?;

        apply_block_deco_filler(
            request.playout.id,
            request.playout.seed,
            request.finish,
            &request.reference_data,
            &mut result,
            cancelled,
            if preservation.rebuild {
                super::PlayoutBuildMode::Reset
            } else {
                request.mode
            },
        )?;
        preservation.finish(&request, &mut result, smart_shuffle_state)?;
        Ok(result)
    }
}

fn schedule_updated_blocks(
    request: &SchedulingRequest,
    blocks: Vec<EffectiveBlock>,
    block_starts_to_update: &BTreeSet<DateTime<Utc>>,
    collection_etags: &BTreeMap<String, String>,
    smart_shuffle_state: &mut super::SchedulingSmartShuffleState,
    result: &mut SchedulingResult,
) -> Result<(), SchedulingError> {
    for block in blocks {
        if !block_starts_to_update.contains(&block.start) || block.start >= request.finish {
            continue;
        }
        schedule_block_items(
            request,
            &block,
            collection_etags,
            smart_shuffle_state,
            result,
        )?;
    }
    Ok(())
}

fn schedule_block_items(
    request: &SchedulingRequest,
    block: &EffectiveBlock,
    collection_etags: &BTreeMap<String, String>,
    smart_shuffle_state: &mut super::SchedulingSmartShuffleState,
    result: &mut SchedulingResult,
) -> Result<(), SchedulingError> {
    let block_finish = block.start + block.duration;
    let mut current_time = request
        .reference_data
        .existing_items
        .iter()
        .filter(|item| item.start < block.start && !result.items_to_remove.contains(&item.id))
        .chain(result.added_items.iter())
        .map(|item| item.finish)
        .max()
        .unwrap_or(block.start)
        .max(block.start);

    for block_item in &block.items {
        if current_time >= block_finish {
            break;
        }

        let mut source =
            super::block_sources::BlockSource::restore(request, block_item, current_time, result)?;
        if source.enumerator.is_empty() {
            result.warnings.push(format!(
                "block item {} skipped empty content {}",
                block_item.id, block_item.content
            ));
            continue;
        }

        let unit_key = block_item.id.to_string();
        let policy = request
            .reference_data
            .smart_shuffle_policy_for_unit(&unit_key);
        let tags_by_media_id = request
            .reference_data
            .smart_shuffle_tags_for_unit(&unit_key);
        let scope_key = policy.scope_key(
            request.playout.id,
            Some(block_item.id),
            Some(&block_item.content),
        );
        source.enumerator.select_with_smart_shuffle(
            policy,
            smart_shuffle_state,
            &scope_key,
            current_time,
            tags_by_media_id,
        );
        let Some(media_item) = source.enumerator.current().cloned() else {
            break;
        };
        let item_finish = current_time + media_item.duration;
        if block.stop_scheduling == BlockStopScheduling::BeforeDurationEnd
            && item_finish > block_finish
        {
            break;
        }

        let item = effective_block_playout_item(
            request,
            block,
            block_item,
            &media_item,
            current_time,
            collection_etags,
            &source,
        );
        let selected_index = source.enumerator.current_index();
        current_time = item.finish;
        smart_shuffle_state.record_unit(
            policy,
            request.playout.id,
            &scope_key,
            item.start,
            &super::SchedulingSmartShuffleUnit::single(
                media_item.id,
                request
                    .reference_data
                    .smart_shuffle_tags_for_media(&unit_key, media_item.id)
                    .clone(),
                media_item.duration,
            ),
            super::SchedulingSmartShuffleRecordContext::new(
                super::is_randomized_playback_order(block_item.playback_order),
                false,
            ),
        );
        result.added_history.push(history(
            request.playout.id,
            history_key_for_block_item(block_item),
            &item,
            None,
            selected_index,
            source.history_details(&media_item, smart_shuffle_state)?,
            false,
        ));
        result.added_items.push(item);
    }
    Ok(())
}

fn effective_block_playout_item(
    request: &SchedulingRequest,
    block: &EffectiveBlock,
    block_item: &EffectiveBlockItem,
    media_item: &SchedulingMediaItem,
    current_time: DateTime<Utc>,
    collection_etags: &BTreeMap<String, String>,
    source: &super::block_sources::BlockSource,
) -> PlayoutItem {
    let enumerator = &source.enumerator;
    let block_finish = block.start + block.duration;
    let collection_key = collection_key_json(&block_item.collection_key);
    let collection_etag = collection_etags
        .get(&collection_key)
        .cloned()
        .unwrap_or_default();
    let block_key = block
        .block_key
        .as_ref()
        .and_then(|key| serde_json::to_string(key).ok());
    let mut item = playout_item(
        request.playout.id,
        media_item,
        current_time,
        media_item.duration,
        i32::try_from(block.template_item_id).unwrap_or_default(),
        if block_item.include_in_program_guide {
            FillerKind::None
        } else {
            FillerKind::GuideMode
        },
        None,
        block_item.disable_watermarks,
        block_item.watermark_ids.clone(),
        block_item.graphics_element_ids.clone(),
        Some(format!(
            r#"{{"BlockId":{},"BlockItemId":{},"Enumerator":"{}","Seed":{},"Index":{}}}"#,
            block.block_id,
            block_item.id,
            enumerator.scheduling_context_name(),
            if super::is_randomized_playback_order(block_item.playback_order) {
                source.seed
            } else {
                0
            },
            enumerator.index
        )),
    );
    item.guide_start = Some(block.start);
    item.guide_finish = Some(block_finish);
    item.block_key = block_key;
    item.collection_key = Some(collection_key);
    item.collection_etag = Some(collection_etag);
    item
}

fn normalize_guide_mode(mut items: Vec<EffectiveBlockItem>) -> Vec<EffectiveBlockItem> {
    if !items.is_empty() && items.iter().all(|item| !item.include_in_program_guide) {
        for item in &mut items {
            item.include_in_program_guide = true;
        }
    }
    items.sort_by_key(|item| item.id);
    items
}

fn collection_etags(
    reference_data: &super::SchedulingReferenceData,
    blocks: &[EffectiveBlock],
) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    for block in blocks {
        for item in &block.items {
            let media_items = lookup_media(reference_data, &item.content).items;
            result.insert(
                collection_key_json(&item.collection_key),
                collection_etag_for_items(&media_items),
            );
        }
        if let Some(deco) = &block.deco {
            if let (Some(content), Some(collection_key)) = (
                &deco.default_filler_content,
                &deco.default_filler_collection_key,
            ) {
                let media_items = lookup_media(reference_data, content).items;
                result.insert(
                    collection_key_json(collection_key),
                    collection_etag_for_items(&media_items),
                );
            }
            for break_content in &deco.break_content {
                let media_items = lookup_media(reference_data, &break_content.content).items;
                result.insert(
                    collection_key_json(&break_content.collection_key),
                    collection_etag_for_items(&media_items),
                );
            }
        }
    }
    result
}

fn collection_key_json(collection_key: &SchedulingCollectionKey) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct Key<'a> {
        collection_type: i32,
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
        fake_collection_tag: Option<&'a str>,
    }

    // Mirrors ErsatzTV's CollectionKey JSON written by JsonConvert with
    // NullValueHandling.Ignore in CollectionKey.ForBlockItem/ForDefaultFiller.
    serde_json::to_string(&Key {
        collection_type: csharp_collection_type_value(collection_key.collection_type),
        collection_id: collection_key.collection_id,
        multi_collection_id: collection_key.multi_collection_id,
        smart_collection_id: collection_key.smart_collection_id,
        media_item_id: collection_key.media_item_id,
        playlist_id: collection_key.playlist_id,
        search_query: collection_key.search_query.as_deref(),
        fake_collection_tag: collection_key.fake_collection_key.as_deref(),
    })
    .unwrap_or_default()
}

fn csharp_collection_type_value(collection_type: CollectionType) -> i32 {
    match collection_type {
        CollectionType::Collection => 0,
        CollectionType::TelevisionShow => 1,
        CollectionType::TelevisionSeason => 2,
        CollectionType::Artist => 3,
        CollectionType::MultiCollection => 4,
        CollectionType::SmartCollection => 5,
        CollectionType::Playlist => 6,
        CollectionType::RerunFirstRun => 7,
        CollectionType::RerunRerun => 8,
        CollectionType::SearchQuery => 9,
        CollectionType::Movie => 10,
        CollectionType::Episode => 20,
        CollectionType::MusicVideo => 30,
        CollectionType::OtherVideo => 40,
        CollectionType::Song => 50,
        CollectionType::Image => 60,
        CollectionType::RemoteStream => 70,
        CollectionType::FakeCollection => 100,
        CollectionType::FakePlaylistItem => 101,
    }
}

fn csharp_playback_order_value(playback_order: PlaybackOrder) -> i32 {
    match playback_order {
        PlaybackOrder::None => 0,
        PlaybackOrder::Chronological => 1,
        PlaybackOrder::Random => 2,
        PlaybackOrder::Shuffle => 3,
        PlaybackOrder::ShuffleInOrder => 4,
        PlaybackOrder::MultiEpisodeShuffle => 5,
        PlaybackOrder::SeasonEpisode => 6,
        PlaybackOrder::RandomRotation => 7,
        PlaybackOrder::Marathon => 8,
    }
}

fn collection_etag_for_items(items: &[SchedulingMediaItem]) -> String {
    let mut value: u64 = 14_695_981_039_346_656_037;
    for item in items {
        hash_bytes(&mut value, &item.id.to_le_bytes());
        hash_bytes(&mut value, &item.duration.num_milliseconds().to_le_bytes());
        if item.preferred_audio_language_code.is_some()
            || item.preferred_audio_title.is_some()
            || item.preferred_subtitle_language_code.is_some()
            || item.subtitle_mode.is_some()
        {
            hash_bytes(&mut value, &[1]);
            hash_optional_string(&mut value, item.preferred_audio_language_code.as_deref());
            hash_optional_string(&mut value, item.preferred_audio_title.as_deref());
            hash_optional_string(&mut value, item.preferred_subtitle_language_code.as_deref());
            hash_bytes(
                &mut value,
                &item
                    .subtitle_mode
                    .map(csharp_subtitle_mode_value)
                    .unwrap_or_default()
                    .to_le_bytes(),
            );
        }
    }
    format!("{value:016x}")
}

fn hash_optional_string(value: &mut u64, item: Option<&str>) {
    match item {
        Some(item) => {
            hash_bytes(value, &[1]);
            hash_bytes(value, item.as_bytes());
        }
        None => hash_bytes(value, &[0]),
    }
}

fn hash_bytes(value: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *value ^= u64::from(*byte);
        *value = value.wrapping_mul(1_099_511_628_211);
    }
}

fn csharp_subtitle_mode_value(mode: ersatzrs_domain::ChannelSubtitleMode) -> i32 {
    match mode {
        ersatzrs_domain::ChannelSubtitleMode::None => 0,
        ersatzrs_domain::ChannelSubtitleMode::Forced => 1,
        ersatzrs_domain::ChannelSubtitleMode::Default => 2,
        ersatzrs_domain::ChannelSubtitleMode::Any => 3,
    }
}

#[derive(Debug, Clone, Default)]
struct BlockChangeSet {
    blocks_to_update: Vec<EffectiveBlock>,
    items_to_remove: Vec<EntityId>,
}

fn block_changes(
    request: &SchedulingRequest,
    blocks: &[EffectiveBlock],
    collection_etags: &BTreeMap<String, String>,
) -> BlockChangeSet {
    if request.reference_data.existing_items.is_empty() {
        return BlockChangeSet {
            blocks_to_update: blocks.to_vec(),
            items_to_remove: Vec::new(),
        };
    }

    let item_block_keys = item_block_keys(&request.reference_data.existing_items);
    let last_scheduled_item = request
        .reference_data
        .existing_items
        .iter()
        .map(|item| item.start)
        .max()
        .unwrap_or(request.start);
    let block_keys_to_schedule = blocks
        .iter()
        .filter_map(|block| block.block_key.as_ref())
        .collect::<BTreeSet<_>>();
    let existing_block_keys = item_block_keys.values().collect::<BTreeSet<_>>();

    let mut updated_starts = BTreeSet::new();
    let mut updated_item_ids = BTreeSet::new();
    let mut earliest_block_by_key = BTreeMap::<BlockKey, DateTime<Utc>>::new();
    let mut earliest_block_by_id = BTreeMap::<EntityId, DateTime<Utc>>::new();

    for block in blocks
        .iter()
        .filter(|block| block.start <= last_scheduled_item)
    {
        let block_key = block.block_key.as_ref();
        let is_new_or_changed = block_key.is_some_and(|key| !existing_block_keys.contains(key));
        if is_new_or_changed {
            updated_starts.insert(block.start);
            if let Some(key) = block_key {
                earliest_block_by_key
                    .entry(key.clone())
                    .or_insert(block.start);
            }
            earliest_block_by_id
                .entry(block.block_id)
                .or_insert(block.start);
        }
    }

    for item in &request.reference_data.existing_items {
        let Some(block_key) = item_block_keys.get(&item.id) else {
            if item.filler_kind != FillerKind::DecoDefault {
                continue;
            }
            continue;
        };

        let collection_changed = collection_changed(item, collection_etags);
        let effective_item_changed = effective_block_item_changed(item, block_key, blocks);
        let key_affected = earliest_block_by_key
            .get(block_key)
            .is_some_and(|start| *start <= item.start);
        let id_affected = earliest_block_by_id
            .get(&block_key.block_id)
            .is_some_and(|start| *start <= item.start);
        if item.start >= request.start
            && (!block_keys_to_schedule.contains(block_key)
                || collection_changed
                || effective_item_changed
                || key_affected
                || id_affected)
        {
            updated_item_ids.insert(item.id);
            if let Some(block) = blocks
                .iter()
                .filter(|block| block.block_id == block_key.block_id)
                .find(|block| {
                    block.start <= item.start && item.start < block.start + block.duration
                })
            {
                updated_starts.insert(block.start);
            }
        }
    }

    for block in blocks
        .iter()
        .filter(|block| block.start > last_scheduled_item)
    {
        updated_starts.insert(block.start);
    }

    let blocks_to_update = blocks
        .iter()
        .filter(|block| updated_starts.contains(&block.start))
        .cloned()
        .collect();
    BlockChangeSet {
        blocks_to_update,
        items_to_remove: updated_item_ids.into_iter().collect(),
    }
}

fn item_block_keys(items: &[PlayoutItem]) -> BTreeMap<EntityId, BlockKey> {
    items
        .iter()
        .filter_map(|item| {
            item.block_key
                .as_deref()
                .and_then(|json| serde_json::from_str::<BlockKey>(json).ok())
                .map(|key| (item.id, key))
        })
        .collect()
}

fn collection_changed(item: &PlayoutItem, collection_etags: &BTreeMap<String, String>) -> bool {
    let Some(collection_key) = item.collection_key.as_deref() else {
        return true;
    };

    collection_etags
        .get(collection_key)
        .is_none_or(|etag| Some(etag) != item.collection_etag.as_ref())
}

fn effective_block_item_changed(
    item: &PlayoutItem,
    block_key: &BlockKey,
    blocks: &[EffectiveBlock],
) -> bool {
    let Some(block) = blocks.iter().find(|block| {
        block.block_key.as_ref() == Some(block_key)
            && block.start <= item.start
            && item.start < block.start + block.duration
    }) else {
        return true;
    };

    let Some(collection_key) = item.collection_key.as_deref() else {
        return true;
    };
    let Some(block_item) = block
        .items
        .iter()
        .find(|block_item| collection_key_json(&block_item.collection_key) == collection_key)
    else {
        return true;
    };

    let expected_guide_group = i32::try_from(block.template_item_id).unwrap_or_default();
    let expected_filler_kind = if block_item.include_in_program_guide {
        FillerKind::None
    } else {
        FillerKind::GuideMode
    };

    item.guide_group != expected_guide_group
        || item.guide_start != Some(block.start)
        || item.guide_finish != Some(block.start + block.duration)
        || item.filler_kind != expected_filler_kind
        || item.disable_watermarks != block_item.disable_watermarks
        || sorted_ids(&item.watermark_ids) != sorted_ids(&block_item.watermark_ids)
        || sorted_ids(&item.graphics_element_ids) != sorted_ids(&block_item.graphics_element_ids)
}

fn sorted_ids(ids: &[EntityId]) -> Vec<EntityId> {
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    sorted
}

fn history_to_remove_for_items(
    request: &SchedulingRequest,
    item_ids_to_remove: &[EntityId],
) -> Vec<EntityId> {
    let starts = request
        .reference_data
        .existing_items
        .iter()
        .filter(|item| item_ids_to_remove.contains(&item.id))
        .map(|item| item.start)
        .collect::<BTreeSet<_>>();

    request
        .reference_data
        .existing_history
        .iter()
        .filter(|history| starts.contains(&history.when))
        .map(|history| history.id)
        .collect()
}

fn apply_block_deco_filler(
    playout_id: EntityId,
    playout_seed: i32,
    schedule_finish: DateTime<Utc>,
    reference_data: &super::SchedulingReferenceData,
    result: &mut SchedulingResult,
    cancelled: &(dyn Fn() -> bool + Sync),
    mode: super::PlayoutBuildMode,
) -> Result<(), SchedulingError> {
    let mut blocks = reference_data.effective_blocks.clone();
    blocks.sort_by_key(|block| block.start);

    for block in blocks {
        if cancelled() {
            return Err(SchedulingError::Cancelled);
        }
        if block.start >= schedule_finish || mode != super::PlayoutBuildMode::Reset {
            continue;
        }
        let Some(deco) = &block.deco else {
            continue;
        };
        let block_finish = block.start + block.duration;
        add_block_start_break_content(
            playout_id,
            playout_seed,
            reference_data,
            result,
            &block,
            deco,
            block_finish,
        )?;
    }

    let mut all_items = reference_data.existing_items.clone();
    // Use the same retention boundary as apply_build_result so expired rows
    // cannot create gaps that this build fills again.
    let deleted_before = result
        .remove_before
        .map(|start| start - reference_data.max_playout_offset.max(Duration::zero()));
    all_items.retain(|item| {
        !result.items_to_remove.contains(&item.id)
            && deleted_before.is_none_or(|cutoff| item.finish >= cutoff)
    });
    all_items.extend(result.added_items.clone());
    all_items.sort_by_key(|item| item.start);

    for window in all_items.windows(2) {
        let one = &window[0];
        let two = &window[1];
        if one.finish >= two.start {
            continue;
        }
        let Some(block) = reference_data.effective_blocks.iter().find(|block| {
            one.guide_group == i32::try_from(block.template_item_id).unwrap_or_default()
                && one.guide_start == Some(block.start)
        }) else {
            continue;
        };
        let Some(deco) = &block.deco else {
            continue;
        };
        add_default_filler(
            FillerRuntime {
                playout_id,
                playout_seed,
            },
            reference_data,
            result,
            deco,
            one,
            one.finish..two.start,
            cancelled,
        )?;
    }
    Ok(())
}

fn add_block_start_break_content(
    playout_id: EntityId,
    playout_seed: i32,
    reference_data: &super::SchedulingReferenceData,
    result: &mut SchedulingResult,
    block: &EffectiveBlock,
    deco: &EffectiveDeco,
    block_finish: DateTime<Utc>,
) -> Result<(), SchedulingError> {
    let retained = reference_data
        .existing_items
        .iter()
        .filter(|item| {
            item.guide_group == i32::try_from(block.template_item_id).unwrap_or_default()
                && item.guide_start == Some(block.start)
                && matches!(item.filler_kind, FillerKind::None | FillerKind::GuideMode)
                && !result.items_to_remove.contains(&item.id)
        })
        .collect::<Vec<_>>();
    let remaining = block_remaining_duration(block, &retained, &result.added_items);
    let mut inserted = Vec::new();
    let mut inserted_duration = Duration::zero();

    for break_content in deco
        .break_content
        .iter()
        .filter(|content| content.placement == DecoBreakPlacement::BlockStart)
    {
        let playlist = restored_break_playlist(
            reference_data,
            result,
            break_content,
            block.start + inserted_duration,
            playout_seed,
        );
        let mut enumerator = lookup_media(reference_data, &break_content.content);
        enumerator.set_index(usize::try_from(playout_seed.unsigned_abs()).unwrap_or_default());
        let Some(media_item) = playlist
            .as_ref()
            .map_or_else(|| enumerator.current(), super::PlaylistEnumerator::current)
            .cloned()
        else {
            continue;
        };
        if media_item.duration > remaining - inserted_duration {
            continue;
        }
        let mut filler = playout_item(
            playout_id,
            &media_item,
            block.start + inserted_duration,
            media_item.duration,
            i32::try_from(block.template_item_id).unwrap_or_default(),
            FillerKind::DecoDefault,
            None,
            false,
            Vec::new(),
            Vec::new(),
            None,
        );
        filler.guide_start = Some(block.start);
        filler.guide_finish = Some(block_finish);
        filler.collection_key = Some(collection_key_json(&break_content.collection_key));
        inserted_duration += media_item.duration;
        result.added_history.extend(break_content_history(
            playout_id,
            break_content,
            &filler,
            &enumerator,
            playlist.as_ref(),
        ));
        inserted.push(filler);
    }

    if deco.break_content.is_empty() {
        return Ok(());
    }

    reposition_retained_block_items(reference_data, result, block, &retained, inserted_duration)?;

    for item in result.added_items.iter_mut().filter(|item| {
        item.id == 0
            && item.guide_group == i32::try_from(block.template_item_id).unwrap_or_default()
            && item.guide_start == Some(block.start)
    }) {
        item.start += inserted_duration;
        item.finish += inserted_duration;
    }
    for history in result
        .added_history
        .iter_mut()
        .filter(|history| history.when >= block.start && history.when < block_finish)
    {
        if history.id != 0 || history.key.contains("\"DecoBreakContentId\"") {
            continue;
        }
        history.when += inserted_duration;
        if let Some(finish) = history.finish.as_mut() {
            *finish += inserted_duration;
        }
    }
    result.added_items.extend(inserted);
    result.added_items.sort_by_key(|item| item.start);
    Ok(())
}

fn reposition_retained_block_items(
    reference: &super::SchedulingReferenceData,
    result: &mut SchedulingResult,
    block: &EffectiveBlock,
    retained: &[&PlayoutItem],
    inserted_duration: Duration,
) -> Result<(), SchedulingError> {
    let mut primary = retained
        .iter()
        .map(|item| (*item).clone())
        .chain(
            result
                .added_items
                .iter()
                .filter(|item| {
                    item.guide_group == i32::try_from(block.template_item_id).unwrap_or_default()
                        && item.guide_start == Some(block.start)
                        && matches!(item.filler_kind, FillerKind::None | FillerKind::GuideMode)
                })
                .cloned(),
        )
        .collect::<Vec<_>>();
    primary.sort_by_key(|item| item.start);
    let mut next = block.start + inserted_duration;
    for mut item in primary {
        let duration = item.finish - item.start;
        if item.id != 0 && item.start != next {
            let Some(history) = reference.existing_history.iter().find(|history| {
                history.when == item.start && !result.history_to_remove.contains(&history.id)
            }) else {
                return Err(SchedulingError::MissingBlockHistory(item.id));
            };
            let mut history = history.clone();
            result.items_to_remove.push(item.id);
            result.history_to_remove.push(history.id);
            item.start = next;
            item.finish = next + duration;
            history.when = item.start;
            history.finish = Some(item.finish);
            result.added_items.push(item);
            result.added_history.push(history);
        }
        next += duration;
    }
    Ok(())
}

fn add_default_filler(
    runtime: FillerRuntime,
    reference_data: &super::SchedulingReferenceData,
    result: &mut SchedulingResult,
    deco: &EffectiveDeco,
    guide_item: &PlayoutItem,
    window: std::ops::Range<DateTime<Utc>>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<(), SchedulingError> {
    let start = window.start;
    let finish = window.end;
    let Some(content) = &deco.default_filler_content else {
        return Ok(());
    };
    let Some(collection_key) = &deco.default_filler_collection_key else {
        return Ok(());
    };
    let mut enumerator = lookup_media(reference_data, content);
    if enumerator.is_empty() {
        return Ok(());
    }
    if runtime.playout_seed != 0 {
        enumerator
            .set_index(usize::try_from(runtime.playout_seed.unsigned_abs()).unwrap_or_default());
    }

    let mut current = start;
    while current < finish {
        if cancelled() {
            return Err(SchedulingError::Cancelled);
        }
        let Some(media_item) = enumerator.current().cloned() else {
            break;
        };
        let mut duration = media_item.duration;
        if duration <= Duration::zero() {
            result.warnings.push(format!(
                "default filler item {} has no positive duration; stopped filling gap at {current}",
                media_item.id
            ));
            break;
        }
        if current + duration > finish {
            if deco.default_filler_trim_to_fit {
                duration = finish - current;
            } else {
                break;
            }
        }
        let mut filler = playout_item(
            runtime.playout_id,
            &media_item,
            current,
            duration,
            guide_item.guide_group,
            FillerKind::DecoDefault,
            None,
            false,
            Vec::new(),
            Vec::new(),
            None,
        );
        filler.guide_start = guide_item.guide_start;
        filler.guide_finish = guide_item.guide_finish;
        filler.collection_key = Some(collection_key_json(collection_key));
        result.added_history.push(history(
            runtime.playout_id,
            history_key_for_default_filler(deco),
            &filler,
            None,
            enumerator.current_index(),
            super::history_details_for_media_item(&media_item),
            false,
        ));
        result.added_items.push(filler);
        current += duration;
        enumerator.advance();
    }
    result.added_items.sort_by_key(|item| item.start);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct FillerRuntime {
    playout_id: EntityId,
    playout_seed: i32,
}

fn block_remaining_duration(
    block: &EffectiveBlock,
    retained: &[&ersatzrs_domain::PlayoutItem],
    added: &[ersatzrs_domain::PlayoutItem],
) -> Duration {
    let retained_duration = retained
        .iter()
        .map(|item| item.finish - item.start)
        .fold(Duration::zero(), |sum, duration| sum + duration);
    let added_duration = added
        .iter()
        .filter(|item| {
            item.guide_group == i32::try_from(block.template_item_id).unwrap_or_default()
                && item.guide_start == Some(block.start)
        })
        .map(|item| item.finish - item.start)
        .fold(Duration::zero(), |sum, duration| sum + duration);
    block.duration - retained_duration - added_duration
}

fn break_content_history(
    playout_id: EntityId,
    content: &EffectiveBreakContent,
    filler: &ersatzrs_domain::PlayoutItem,
    enumerator: &super::MediaEnumerator,
    playlist: Option<&super::PlaylistEnumerator>,
) -> Vec<ersatzrs_domain::PlayoutHistory> {
    let key = history_key_for_break_content(content);
    if let Some(playlist) = playlist {
        return playlist.history_for_item(playout_id, &key, filler);
    }
    enumerator
        .current()
        .map(|media| {
            history(
                playout_id,
                key,
                filler,
                None,
                enumerator.current_index(),
                super::history_details_for_media_item(media),
                false,
            )
        })
        .into_iter()
        .collect()
}

fn restored_break_playlist(
    reference: &super::SchedulingReferenceData,
    result: &SchedulingResult,
    content: &EffectiveBreakContent,
    start: DateTime<Utc>,
    seed: i32,
) -> Option<super::PlaylistEnumerator> {
    let children = reference.playlist_items_by_key.get(&content.content)?;
    let mut playlist =
        super::PlaylistEnumerator::new(children.clone()).with_child_shuffle_seed(seed);
    let key = history_key_for_break_content(content);
    let mut histories = reference
        .existing_history
        .iter()
        .filter(|entry| !result.history_to_remove.contains(&entry.id))
        .chain(result.added_history.iter())
        .filter(|entry| entry.key == key && entry.when < start)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(latest) = histories.iter().map(|entry| entry.when).max() {
        histories.retain(|entry| entry.when == latest);
        playlist.apply_history(&histories);
    }
    Some(playlist)
}

pub(super) fn history_key_for_block_item(block_item: &EffectiveBlockItem) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct Key<'a> {
        block_id: EntityId,
        playback_order: i32,
        collection_type: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        collection_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        multi_collection_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        smart_collection_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        media_item_id: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_query: Option<&'a str>,
    }

    serde_json::to_string(&Key {
        block_id: block_item.block_id,
        playback_order: csharp_playback_order_value(block_item.playback_order),
        collection_type: csharp_collection_type_value(block_item.collection_key.collection_type),
        collection_id: block_item.collection_key.collection_id,
        multi_collection_id: block_item.collection_key.multi_collection_id,
        smart_collection_id: block_item.collection_key.smart_collection_id,
        media_item_id: block_item.collection_key.media_item_id,
        search_query: block_item.collection_key.search_query.as_deref(),
    })
    .unwrap_or_default()
}

fn history_key_for_break_content(break_content: &EffectiveBreakContent) -> String {
    serde_json::json!({
        "DecoBreakContentId": break_content.id,
        "PlaybackOrder": csharp_playback_order_value(PlaybackOrder::Shuffle),
        "CollectionType": csharp_collection_type_value(break_content.collection_key.collection_type),
        "CollectionId": break_content.collection_key.collection_id,
        "MultiCollectionId": break_content.collection_key.multi_collection_id,
        "SmartCollectionId": break_content.collection_key.smart_collection_id,
        "MediaItemId": break_content.collection_key.media_item_id,
        "PlaylistId": break_content.collection_key.playlist_id,
    })
    .to_string()
}

fn history_key_for_default_filler(deco: &EffectiveDeco) -> String {
    let Some(collection_key) = &deco.default_filler_collection_key else {
        return String::new();
    };
    serde_json::json!({
        "DecoId": deco.id,
        "PlaybackOrder": csharp_playback_order_value(PlaybackOrder::Shuffle),
        "CollectionType": csharp_collection_type_value(collection_key.collection_type),
        "CollectionId": collection_key.collection_id,
        "MultiCollectionId": collection_key.multi_collection_id,
        "SmartCollectionId": collection_key.smart_collection_id,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Duration, NaiveTime, Utc};
    use ersatzrs_domain::{Playout, PlayoutItem};

    use super::*;
    use crate::scheduling::{
        PlayoutBuildMode, SchedulingMediaItem, SchedulingReferenceData, SchedulingRequest,
    };

    pub(super) fn randomized_reference(
        start: DateTime<Utc>,
        order: PlaybackOrder,
    ) -> SchedulingReferenceData {
        let mut reference = SchedulingReferenceData::default();
        reference.media_by_key.insert(
            "series".into(),
            (1..=6)
                .map(|id| {
                    let mut media = SchedulingMediaItem::new(id, Duration::minutes(20));
                    media.show_id = Some((id - 1) % 3);
                    media
                })
                .collect(),
        );
        reference.effective_blocks = (0..18)
            .map(|index| {
                let mut block =
                    effective_block(10, 20, "series", start + Duration::minutes(index * 20));
                block.items[0].playback_order = order;
                block
            })
            .collect();
        reference
    }

    #[test]
    fn block_randomized_sources_resume_mid_cycle_and_at_wrap() {
        let start = utc("2026-04-15T23:40:00Z");
        for order in [
            PlaybackOrder::Shuffle,
            PlaybackOrder::Random,
            PlaybackOrder::RandomRotation,
        ] {
            let reference = randomized_reference(start, order);
            let uninterrupted = schedule(reference.clone(), start, Duration::hours(6), 100, 127);
            let ids = uninterrupted
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>();
            assert!(
                ids.iter().collect::<BTreeSet<_>>().len() > 1,
                "{order:?}: {ids:?}"
            );
            if order == PlaybackOrder::Shuffle {
                for cycle in ids.chunks(6) {
                    assert_eq!(cycle.iter().collect::<BTreeSet<_>>().len(), 6);
                }
                assert_ne!(ids[5], ids[6]);
            }
            if order == PlaybackOrder::RandomRotation {
                assert!(
                    ids.windows(2)
                        .all(|pair| (pair[0] - 1) % 3 != (pair[1] - 1) % 3),
                    "{ids:?}"
                );
            }
            for retained in [5, 6] {
                let mut first_reference = reference.clone();
                first_reference.effective_blocks.truncate(retained);
                let split = start + Duration::minutes(i64::try_from(retained).unwrap() * 20);
                let first = schedule(first_reference, start, split - start, 100, 127);
                let mut after = reference.clone();
                after.existing_items = first.added_items;
                after.existing_history = first.added_history;
                for (index, item) in after.existing_items.iter_mut().enumerate() {
                    item.id = i64::try_from(index + 1).unwrap();
                }
                for (index, entry) in after.existing_history.iter_mut().enumerate() {
                    entry.id = i64::try_from(index + 1).unwrap();
                }
                let continued = BlockSchedulingEngine
                    .schedule(SchedulingRequest {
                        playout: Playout {
                            id: 100,
                            seed: 127,
                            ..Playout::default()
                        },
                        start: split,
                        finish: start + Duration::hours(6),
                        mode: PlayoutBuildMode::Continue,
                        reference_data: after,
                    })
                    .unwrap();
                assert_eq!(
                    continued.added_items,
                    uninterrupted.added_items[retained..],
                    "{order:?}, split {retained}"
                );
            }
        }
    }

    #[test]
    fn block_randomized_source_respects_a_new_reset_seed_with_archived_history() {
        let start = utc("2026-04-15T23:40:00Z");
        let split = start + Duration::minutes(40);
        for order in [
            PlaybackOrder::Shuffle,
            PlaybackOrder::Random,
            PlaybackOrder::RandomRotation,
        ] {
            let mut reference = randomized_reference(start, order);
            let before_reset = schedule(reference.clone(), start, split - start, 100, 127);
            reference
                .effective_blocks
                .retain(|block| block.start >= split);
            let fresh = schedule(reference.clone(), split, Duration::hours(4), 100, -953);
            reference.existing_history = before_reset.added_history;
            for (index, entry) in reference.existing_history.iter_mut().enumerate() {
                entry.id = i64::try_from(index + 1).unwrap();
            }
            let resumed = schedule(reference, split, Duration::hours(4), 100, -953);
            assert_eq!(resumed.added_items, fresh.added_items, "{order:?}");
        }
    }

    #[test]
    fn block_shuffle_membership_change_keeps_unconsumed_survivors_first() {
        let start = utc("2026-04-15T23:40:00Z");
        for order in [PlaybackOrder::Shuffle, PlaybackOrder::RandomRotation] {
            let mut reference = randomized_reference(start, order);
            let uninterrupted = schedule(reference.clone(), start, Duration::hours(6), 100, 127);
            let removed = uninterrupted.added_items[4].media_item_id;
            let first = schedule(reference.clone(), start, Duration::minutes(40), 100, 127);
            reference.existing_items = first.added_items;
            reference.existing_history = first.added_history;
            for (index, item) in reference.existing_items.iter_mut().enumerate() {
                item.id = i64::try_from(index + 1).unwrap();
            }
            for (index, entry) in reference.existing_history.iter_mut().enumerate() {
                entry.id = i64::try_from(index + 1).unwrap();
            }
            let media = reference.media_by_key.get_mut("series").unwrap();
            media.retain(|item| item.id != removed);
            media.push(SchedulingMediaItem::new(7, Duration::minutes(20)));
            let resumed = BlockSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        seed: 127,
                        ..Playout::default()
                    },
                    start: start + Duration::minutes(40),
                    finish: start + Duration::hours(6),
                    mode: PlayoutBuildMode::Continue,
                    reference_data: reference,
                })
                .unwrap();
            let mut expected = uninterrupted.added_items[2..6]
                .iter()
                .map(|item| item.media_item_id)
                .filter(|id| *id != removed)
                .collect::<Vec<_>>();
            expected.push(7);
            assert_eq!(
                resumed
                    .added_items
                    .iter()
                    .take(expected.len())
                    .map(|item| item.media_item_id)
                    .collect::<Vec<_>>(),
                expected,
                "{order:?}"
            );
        }
    }

    #[test]
    fn block_ordered_sources_resume_across_blocks_and_history_restart() {
        let start = utc("2026-04-15T23:40:00Z");
        for order in [PlaybackOrder::Chronological, PlaybackOrder::SeasonEpisode] {
            let mut reference = SchedulingReferenceData::default();
            reference.media_by_key.insert(
                "series".into(),
                (1..=3)
                    .map(|id| {
                        let mut media = SchedulingMediaItem::new(id, Duration::minutes(20));
                        media.release_date = Some(start + Duration::days(id));
                        media.season = Some(1);
                        media.episode = Some(i32::try_from(id).unwrap());
                        media
                    })
                    .collect(),
            );
            reference.effective_blocks = (0..4)
                .map(|index| {
                    let mut block =
                        effective_block(10, 20, "series", start + Duration::minutes(index * 20));
                    block.items[0].playback_order = order;
                    block
                })
                .collect();
            let uninterrupted = schedule(reference.clone(), start, Duration::hours(2), 100, 0);
            assert_eq!(
                uninterrupted
                    .added_items
                    .iter()
                    .map(|item| item.media_item_id)
                    .collect::<Vec<_>>(),
                vec![1, 2, 3, 1],
                "{order:?}"
            );

            let mut first_reference = reference.clone();
            first_reference.effective_blocks.truncate(2);
            let first = schedule(first_reference, start, Duration::minutes(40), 100, 0);
            reference.existing_items = first.added_items;
            reference.existing_history = first.added_history;
            for (index, item) in reference.existing_items.iter_mut().enumerate() {
                item.id = i64::try_from(index + 1).unwrap();
            }
            for (index, entry) in reference.existing_history.iter_mut().enumerate() {
                entry.id = i64::try_from(index + 1).unwrap();
            }
            let continued = BlockSchedulingEngine
                .schedule(SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        ..Playout::default()
                    },
                    start: start + Duration::minutes(40),
                    finish: start + Duration::hours(2),
                    mode: PlayoutBuildMode::Continue,
                    reference_data: reference,
                })
                .unwrap();
            assert_eq!(
                continued.added_items,
                uninterrupted.added_items[2..],
                "restart: {order:?}"
            );
        }
    }

    #[test]
    fn collection_etag_changes_only_when_playback_override_is_present() {
        let base_item = SchedulingMediaItem::new(1, Duration::minutes(10));
        let base_etag = collection_etag_for_items(std::slice::from_ref(&base_item));

        let mut commentary_item = base_item.clone();
        commentary_item.preferred_audio_title = Some("Director Commentary".to_owned());
        let commentary_etag = collection_etag_for_items(&[commentary_item.clone()]);

        let mut alternate_item = commentary_item;
        alternate_item.preferred_audio_title = Some("Isolated Score".to_owned());
        let alternate_etag = collection_etag_for_items(&[alternate_item]);

        assert_ne!(base_etag, commentary_etag);
        assert_ne!(commentary_etag, alternate_etag);
    }

    #[test]
    fn block_retention_cutoff_never_moves_after_build_start() {
        let start = utc("2026-04-15T09:00:00Z");
        for mode in [
            PlayoutBuildMode::Continue,
            PlayoutBuildMode::Refresh,
            PlayoutBuildMode::Reset,
        ] {
            for offset in [None, Some(-10), Some(0), Some(60), Some(1380)] {
                let mut reference_data = SchedulingReferenceData::default();
                if let Some(minutes) = offset {
                    reference_data.effective_blocks.push(effective_block(
                        10,
                        20,
                        "movies",
                        start + Duration::minutes(minutes),
                    ));
                    reference_data.media_by_key.insert(
                        "movies".to_owned(),
                        vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
                    );
                }
                let result = BlockSchedulingEngine
                    .schedule(SchedulingRequest {
                        playout: Playout::default(),
                        start,
                        finish: start + Duration::days(2),
                        mode,
                        reference_data,
                    })
                    .unwrap();
                let expected = start + Duration::minutes(offset.unwrap_or(0).min(0));
                assert_eq!(result.remove_before, Some(expected), "{mode:?}, {offset:?}");
                assert!(!result.clear_items);
            }
        }
    }

    #[test]
    fn block_scheduling_matches_ersatztv_one_item_per_block_item_snapshot() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "movies".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(30)),
                SchedulingMediaItem::new(2, Duration::minutes(45)),
            ],
        );
        reference_data.media_by_key.insert(
            "shorts".to_owned(),
            vec![SchedulingMediaItem::new(9, Duration::minutes(5))],
        );
        reference_data.effective_blocks.push(EffectiveBlock {
            block_id: 10,
            block_key: Some(block_key(10, 20, 30)),
            template_item_id: 20,
            name: "Morning".to_owned(),
            start,
            duration: Duration::hours(1),
            stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
            items: vec![
                block_item(1, 10, "movies", true),
                EffectiveBlockItem {
                    id: 2,
                    block_id: 10,
                    content: "shorts".to_owned(),
                    collection_key: collection_key(CollectionType::Collection, 2),
                    playback_order: PlaybackOrder::Chronological,
                    include_in_program_guide: false,
                    disable_watermarks: true,
                    watermark_ids: vec![],
                    graphics_element_ids: vec![77],
                },
            ],
            deco: None,
        });
        reference_data.effective_blocks[0].items[0].watermark_ids = vec![42];

        let result = schedule(reference_data, start, Duration::hours(2), 100, 0);

        insta::assert_json_snapshot!(serde_json::json!({
            "items": timeline(&result.added_items),
            "history": result.added_history.iter().map(|history| serde_json::json!({
                "key": history.key,
                "when": history.when.to_rfc3339(),
                "finish": history.finish.map(|finish| finish.to_rfc3339()),
                "index": history.index,
            })).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn block_shuffle_uses_shared_cooldown_across_effective_blocks() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference_data = SchedulingReferenceData {
            smart_shuffle_policy: super::super::SchedulingSmartShufflePolicy {
                enabled: true,
                preset: super::super::SchedulingSmartShufflePreset::Custom,
                after_items: Some(2),
                rules: vec![super::super::SchedulingSmartShuffleRule {
                    id: "show".to_owned(),
                    kind: super::super::SchedulingSmartShuffleRuleKind::Show,
                    search: None,
                }],
                ..super::super::SchedulingSmartShufflePolicy::default()
            },
            ..SchedulingReferenceData::default()
        };
        reference_data.media_by_key.insert(
            "shows".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(10)),
                SchedulingMediaItem::new(2, Duration::minutes(10)),
            ],
        );
        reference_data
            .smart_shuffle_tags_by_media_id
            .insert(1, BTreeSet::from(["show:same".to_owned()]));
        reference_data
            .smart_shuffle_tags_by_media_id
            .insert(2, BTreeSet::from(["show:other".to_owned()]));
        for (index, block_start) in [start, start + Duration::minutes(30)]
            .into_iter()
            .enumerate()
        {
            let mut item = block_item(
                i64::try_from(index + 1).unwrap(),
                i64::try_from(index + 10).unwrap(),
                "shows",
                true,
            );
            item.playback_order = PlaybackOrder::Shuffle;
            reference_data.effective_blocks.push(EffectiveBlock {
                block_id: i64::try_from(index + 10).unwrap(),
                block_key: None,
                template_item_id: i64::try_from(index + 20).unwrap(),
                name: format!("Block {index}"),
                start: block_start,
                duration: Duration::minutes(20),
                stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
                items: vec![item],
                deco: None,
            });
        }

        let result = schedule(reference_data, start, Duration::hours(1), 100, 0);
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(result
            .anchor
            .and_then(|anchor| anchor.context)
            .is_some_and(|context| context.contains("show:other")));
    }

    #[test]
    fn effective_blocks_are_derived_from_templates_and_normalize_guide_mode_snapshot() {
        let start = utc("2026-04-15T08:00:00Z");
        let schedule = BlockTemplateSchedule {
            id: 1000,
            updated_ticks: 11,
            start: None,
            finish: None,
            index: 0,
            days_of_week: vec![1, 2, 3, 4, 5, 6, 0],
            days_of_month: (1..=31).collect(),
            months_of_year: (1..=12).collect(),
            limit_to_date_range: false,
            start_month: 1,
            start_day: 1,
            start_year: None,
            end_month: 12,
            end_day: 31,
            end_year: None,
            template: BlockTemplate {
                id: 2000,
                updated_ticks: 22,
                items: vec![BlockTemplateItem {
                    id: 3000,
                    block_id: 4000,
                    start_time: Some(NaiveTime::from_hms_opt(7, 30, 0).expect("valid time")),
                    sort_order: 0,
                }],
            },
            blocks: BTreeMap::from([(
                4000,
                BlockDefinition {
                    id: 4000,
                    updated_ticks: 44,
                    name: "Morning".to_owned(),
                    duration: Duration::minutes(45),
                    stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
                    items: vec![EffectiveBlockItem {
                        include_in_program_guide: false,
                        ..block_item(5000, 4000, "movies", false)
                    }],
                },
            )]),
            deco: Some(EffectiveDeco {
                id: 50,
                default_filler_content: Some("filler".to_owned()),
                default_filler_collection_key: Some(collection_key(CollectionType::Collection, 50)),
                default_filler_trim_to_fit: true,
                break_content: Vec::new(),
            }),
            deco_template_items: Vec::new(),
        };

        let blocks = EffectiveBlock::from_templates(&[schedule], start, 2);

        insta::assert_json_snapshot!(serde_json::json!({
            "blocks": blocks.iter().map(|block| serde_json::json!({
                "id": block.block_id,
                "key": block.block_key,
                "template_item": block.template_item_id,
                "start": block.start.to_rfc3339(),
                "finish": (block.start + block.duration).to_rfc3339(),
                "deco": block.deco.as_ref().map(|deco| deco.id),
                "include_in_guide": block.items.iter().map(|item| item.include_in_program_guide).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn block_deco_break_content_shifts_block_items_snapshot() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "movies".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::minutes(20))],
        );
        reference_data.media_by_key.insert(
            "bumper".to_owned(),
            vec![SchedulingMediaItem::new(7, Duration::minutes(5))],
        );
        reference_data.effective_blocks.push(EffectiveBlock {
            block_id: 10,
            block_key: Some(block_key(10, 20, 30)),
            template_item_id: 20,
            name: "Morning".to_owned(),
            start,
            duration: Duration::minutes(30),
            stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
            items: vec![block_item(1, 10, "movies", true)],
            deco: Some(EffectiveDeco {
                id: 70,
                default_filler_content: None,
                default_filler_collection_key: None,
                default_filler_trim_to_fit: false,
                break_content: vec![EffectiveBreakContent {
                    id: 71,
                    placement: DecoBreakPlacement::BlockStart,
                    content: "bumper".to_owned(),
                    collection_key: collection_key(CollectionType::Playlist, 71),
                }],
            }),
        });

        let result = schedule(reference_data, start, Duration::hours(1), 100, 0);

        insta::assert_json_snapshot!(serde_json::json!({
            "items": timeline(&result.added_items),
            "history": result.added_history.iter().map(|history| serde_json::json!({
                "key": history.key,
                "when": history.when.to_rfc3339(),
                "finish": history.finish.map(|finish| finish.to_rfc3339()),
            })).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn block_deco_playlist_rebuild_preserves_every_child_position() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference = SchedulingReferenceData::default();
        reference.media_by_key.insert(
            "movies".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::minutes(20))],
        );
        let children = (0..3)
            .map(|child| super::super::SchedulingPlaylistItem {
                collection_key: SchedulingCollectionKey::fake_playlist_item(child),
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: Some(2),
                media_items: (0..4)
                    .map(|index| {
                        SchedulingMediaItem::new(10 + child * 10 + index, Duration::minutes(1))
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        reference.media_by_key.insert(
            "bumper".to_owned(),
            children
                .iter()
                .flat_map(|child| child.media_items.clone())
                .collect(),
        );
        reference
            .playlist_items_by_key
            .insert("bumper".to_owned(), children);
        for index in 0..18 {
            reference.effective_blocks.push(EffectiveBlock {
                block_id: 10,
                block_key: Some(block_key(10, 20, 30)),
                template_item_id: 20,
                name: "Synthetic playlist block".to_owned(),
                start: start + Duration::minutes(index * 30),
                duration: Duration::minutes(30),
                stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
                items: vec![block_item(1, 10, "movies", true)],
                deco: Some(EffectiveDeco {
                    id: 70,
                    default_filler_content: None,
                    default_filler_collection_key: None,
                    default_filler_trim_to_fit: false,
                    break_content: vec![EffectiveBreakContent {
                        id: 71,
                        placement: DecoBreakPlacement::BlockStart,
                        content: "bumper".to_owned(),
                        collection_key: collection_key(CollectionType::Playlist, 71),
                    }],
                }),
            });
        }
        let full = schedule(reference.clone(), start, Duration::hours(9), 100, 0);
        let split = start + Duration::minutes(7 * 30);
        let mut initial = reference.clone();
        initial.effective_blocks.retain(|block| block.start < split);
        let first = schedule(initial, start, split - start, 100, 0);
        reference
            .effective_blocks
            .retain(|block| block.start >= split);
        reference.existing_history = first.added_history.clone();
        let continued = schedule(reference, split, start + Duration::hours(9) - split, 100, 0);
        let fillers = |items: &[PlayoutItem]| {
            items
                .iter()
                .filter(|item| item.filler_kind == FillerKind::DecoDefault)
                .map(|item| (item.media_item_id, item.start, item.finish))
                .collect::<Vec<_>>()
        };
        let mut actual = fillers(&first.added_items);
        actual.extend(fillers(&continued.added_items));
        assert_eq!(actual.len(), 18);
        assert_eq!(actual, fillers(&full.added_items));
        assert_eq!(
            actual.iter().take(8).map(|item| item.0).collect::<Vec<_>>(),
            [10, 11, 20, 21, 30, 31, 12, 13]
        );
    }

    #[test]
    fn block_deco_default_filler_fills_gap_and_trims_snapshot() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "movies".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
        );
        reference_data.media_by_key.insert(
            "filler".to_owned(),
            vec![SchedulingMediaItem::new(90, Duration::minutes(7))],
        );
        reference_data.effective_blocks = vec![
            EffectiveBlock {
                block_id: 10,
                block_key: Some(block_key(10, 20, 30)),
                template_item_id: 20,
                name: "A".to_owned(),
                start,
                duration: Duration::minutes(10),
                stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
                items: vec![block_item(1, 10, "movies", true)],
                deco: Some(EffectiveDeco {
                    id: 80,
                    default_filler_content: Some("filler".to_owned()),
                    default_filler_collection_key: Some(collection_key(
                        CollectionType::Collection,
                        80,
                    )),
                    default_filler_trim_to_fit: true,
                    break_content: Vec::new(),
                }),
            },
            EffectiveBlock {
                block_id: 11,
                block_key: Some(block_key(11, 21, 31)),
                template_item_id: 21,
                name: "B".to_owned(),
                start: start + Duration::minutes(25),
                duration: Duration::minutes(10),
                stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
                items: vec![block_item(2, 11, "movies", true)],
                deco: None,
            },
        ];

        let result = schedule(reference_data, start, Duration::hours(1), 100, 0);

        insta::assert_json_snapshot!(serde_json::json!({
            "items": timeline(&result.added_items),
            "history_keys": result.added_history.iter().map(|history| &history.key).collect::<Vec<_>>(),
        }));
    }

    #[test]
    fn block_reset_replaces_filler_and_history_without_rebuilding_primary_items() {
        let start = utc("2026-04-15T08:00:00Z");
        for hidden_from_guide in [false, true] {
            let mut reference = filler_reference_data(start);
            reference.effective_blocks[0].items[0].include_in_program_guide = !hidden_from_guide;
            let original = schedule(reference.clone(), start, Duration::hours(1), 100, 0);
            reference.existing_items = original.added_items;
            reference.existing_history = original.added_history;
            for (index, item) in reference.existing_items.iter_mut().enumerate() {
                item.id = i64::try_from(index + 1).unwrap();
            }
            for (index, entry) in reference.existing_history.iter_mut().enumerate() {
                entry.id = i64::try_from(index + 100).unwrap();
            }
            reference.media_by_key.insert(
                "filler".to_owned(),
                vec![SchedulingMediaItem::new(91, Duration::minutes(5))],
            );
            let primary_ids = reference
                .existing_items
                .iter()
                .filter(|item| matches!(item.filler_kind, FillerKind::None | FillerKind::GuideMode))
                .map(|item| item.id)
                .collect::<Vec<_>>();
            let filler_ids = reference
                .existing_items
                .iter()
                .filter(|item| item.filler_kind == FillerKind::DecoDefault)
                .map(|item| item.id)
                .collect::<Vec<_>>();
            let filler_history_ids = reference
                .existing_history
                .iter()
                .filter(|entry| {
                    reference.existing_items.iter().any(|item| {
                        item.filler_kind == FillerKind::DecoDefault && item.start == entry.when
                    })
                })
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            assert_eq!(filler_ids.len(), 3);
            assert_eq!(filler_history_ids.len(), 3);
            for mode in [PlayoutBuildMode::Continue, PlayoutBuildMode::Reset] {
                let result = BlockSchedulingEngine
                    .schedule(SchedulingRequest {
                        playout: Playout {
                            id: 100,
                            ..Playout::default()
                        },
                        start,
                        finish: start + Duration::hours(1),
                        mode,
                        reference_data: reference.clone(),
                    })
                    .unwrap();
                assert!(primary_ids
                    .iter()
                    .all(|id| !result.items_to_remove.contains(id)));
                if mode == PlayoutBuildMode::Reset {
                    assert_eq!(result.items_to_remove, filler_ids);
                    assert_eq!(result.history_to_remove, filler_history_ids);
                    assert_eq!(result.added_items.len(), 3);
                    assert!(result
                        .added_items
                        .iter()
                        .all(|item| item.media_item_id == 91));
                    assert_eq!(
                        result.added_items.first().unwrap().start,
                        start + Duration::minutes(10)
                    );
                    assert_eq!(
                        result.added_items.last().unwrap().finish,
                        start + Duration::minutes(25)
                    );
                } else {
                    assert!(result.items_to_remove.is_empty());
                    assert!(result.history_to_remove.is_empty());
                    assert!(result.added_items.is_empty());
                }
            }
        }
    }

    #[test]
    fn block_reset_repositions_retained_primary_with_changed_break_duration() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference = filler_reference_data(start);
        reference.effective_blocks[0].duration = Duration::minutes(20);
        reference.effective_blocks[0]
            .deco
            .as_mut()
            .unwrap()
            .break_content
            .push(EffectiveBreakContent {
                id: 81,
                placement: DecoBreakPlacement::BlockStart,
                content: "bumper".to_owned(),
                collection_key: collection_key(CollectionType::Playlist, 81),
            });
        reference.media_by_key.insert(
            "bumper".to_owned(),
            vec![SchedulingMediaItem::new(99, Duration::minutes(2))],
        );
        let original = schedule(reference.clone(), start, Duration::hours(1), 100, 0);
        reference.existing_items = original.added_items;
        reference.existing_history = original.added_history;
        for (index, item) in reference.existing_items.iter_mut().enumerate() {
            item.id = i64::try_from(index + 1).unwrap();
            item.preferred_audio_language_code = Some("nld".to_owned());
        }
        for (index, entry) in reference.existing_history.iter_mut().enumerate() {
            entry.id = i64::try_from(index + 100).unwrap();
        }
        reference.media_by_key.insert(
            "bumper".to_owned(),
            vec![SchedulingMediaItem::new(99, Duration::minutes(3))],
        );
        let request = SchedulingRequest {
            playout: Playout {
                id: 100,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(1),
            mode: PlayoutBuildMode::Reset,
            reference_data: reference,
        };
        let result = BlockSchedulingEngine.schedule(request.clone()).unwrap();
        let primary = result
            .added_items
            .iter()
            .find(|item| item.filler_kind == FillerKind::None)
            .unwrap();
        assert_eq!(primary.start, start + Duration::minutes(3));
        assert_eq!(primary.finish, start + Duration::minutes(13));
        assert_eq!(
            primary.preferred_audio_language_code.as_deref(),
            Some("nld")
        );
        assert!(result
            .added_items
            .windows(2)
            .all(|pair| pair[0].finish <= pair[1].start));
        assert!(
            result
                .added_history
                .iter()
                .any(|history| history.when == primary.start
                    && history.finish == Some(primary.finish))
        );
        let mut missing_history = request;
        missing_history
            .reference_data
            .existing_history
            .retain(|entry| entry.when != start + Duration::minutes(2));
        assert!(matches!(
            BlockSchedulingEngine.schedule(missing_history),
            Err(SchedulingError::MissingBlockHistory(_))
        ));
    }

    #[test]
    fn block_reset_retains_filler_before_utc_boundary_for_delayed_mirrors() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference = SchedulingReferenceData {
            max_playout_offset: Duration::hours(1),
            ..SchedulingReferenceData::default()
        };
        for (index, finish) in [
            start - Duration::seconds(1),
            start,
            start + Duration::seconds(1),
        ]
        .into_iter()
        .enumerate()
        {
            reference.existing_items.push(PlayoutItem {
                id: i64::try_from(index + 1).unwrap(),
                start: finish - Duration::minutes(1),
                finish,
                filler_kind: FillerKind::DecoDefault,
                ..PlayoutItem::default()
            });
        }
        let result = schedule(reference, start, Duration::hours(1), 100, 0);
        assert_eq!(result.items_to_remove, vec![2, 3]);
    }

    #[test]
    fn block_cancellation_during_filler_returns_no_partial_build() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference = filler_reference_data(start);
        reference.media_by_key.insert(
            "filler".to_owned(),
            vec![SchedulingMediaItem::new(90, Duration::milliseconds(1))],
        );
        for cancel_after in [1, 100] {
            let checks = AtomicUsize::new(0);
            let result = crate::playout_builder::PlayoutBuilder::default().build_cancellable(
                SchedulingRequest {
                    playout: Playout {
                        id: 100,
                        schedule_kind: ersatzrs_domain::PlayoutScheduleKind::Block,
                        ..Playout::default()
                    },
                    start,
                    finish: start + Duration::hours(1),
                    mode: PlayoutBuildMode::Reset,
                    reference_data: reference.clone(),
                },
                &|| checks.fetch_add(1, Ordering::SeqCst) + 1 >= cancel_after,
            );
            assert!(matches!(result, Err(SchedulingError::Cancelled)));
            assert_eq!(checks.load(Ordering::SeqCst), cancel_after);
        }
    }

    #[test]
    fn block_default_filler_stops_at_nonpositive_duration() {
        let start = utc("2026-04-15T08:00:00Z");
        for invalid_duration in [Duration::zero(), Duration::seconds(-1)] {
            let mut reference_data = filler_reference_data(start);
            reference_data.media_by_key.insert(
                "filler".to_owned(),
                vec![
                    SchedulingMediaItem::new(90, Duration::minutes(1)),
                    SchedulingMediaItem::new(91, invalid_duration),
                    SchedulingMediaItem::new(92, Duration::minutes(1)),
                ],
            );
            let result = schedule(reference_data, start, Duration::hours(1), 100, 0);
            let filler = result
                .added_items
                .iter()
                .filter(|item| item.filler_kind == FillerKind::DecoDefault)
                .collect::<Vec<_>>();
            assert_eq!(filler.len(), 1);
            assert_eq!(filler[0].media_item_id, 90);
            assert_eq!(filler[0].finish, start + Duration::minutes(11));
            assert_eq!(result.warnings.len(), 1);
            assert!(result.warnings[0].contains("item 91 has no positive duration"));
            assert!(!result
                .added_history
                .iter()
                .any(|entry| entry.when >= filler[0].finish
                    && entry.when < start + Duration::minutes(25)));
        }
    }

    #[test]
    fn block_filler_uses_persistence_retention_boundary() {
        let start = utc("2026-04-15T08:00:00Z");
        for (offset, remove_before, removed, expected_count) in [
            (
                Duration::zero(),
                Some(start + Duration::minutes(10) + Duration::seconds(1)),
                false,
                0,
            ),
            (
                Duration::zero(),
                Some(start + Duration::minutes(10)),
                false,
                3,
            ),
            (
                Duration::minutes(5),
                Some(start + Duration::minutes(15)),
                false,
                3,
            ),
            (
                Duration::minutes(-5),
                Some(start + Duration::minutes(10)),
                false,
                3,
            ),
            (Duration::zero(), None, false, 3),
            (Duration::zero(), None, true, 0),
        ] {
            let mut reference_data = filler_reference_data(start);
            reference_data.max_playout_offset = offset;
            let first = existing_item(1, 10, 20, start, String::new(), String::new());
            let second = existing_item(
                2,
                11,
                21,
                start + Duration::minutes(25),
                String::new(),
                String::new(),
            );
            reference_data.existing_items = vec![first, second];
            let mut result = SchedulingResult {
                remove_before,
                items_to_remove: if removed { vec![1] } else { Vec::new() },
                ..SchedulingResult::default()
            };
            apply_block_deco_filler(
                100,
                0,
                start + Duration::hours(1),
                &reference_data,
                &mut result,
                &|| false,
                PlayoutBuildMode::Continue,
            )
            .unwrap();
            assert_eq!(
                result.added_items.len(),
                expected_count,
                "offset={offset}, cutoff={remove_before:?}, removed={removed}"
            );
            assert_eq!(result.added_history.len(), expected_count);
            assert!(result
                .added_items
                .iter()
                .all(|item| item.start >= start + Duration::minutes(10)
                    && item.finish <= start + Duration::minutes(25)));
        }
    }

    #[test]
    fn block_oversized_primary_item_stops_without_retrying() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference_data = filler_reference_data(start);
        reference_data.effective_blocks.truncate(1);
        reference_data.effective_blocks[0].deco = None;
        reference_data.media_by_key.insert(
            "movies".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::hours(1))],
        );
        let result = schedule(reference_data, start, Duration::hours(1), 100, 0);
        assert!(result.added_items.is_empty());
        assert!(result.added_history.is_empty());
    }

    pub(super) fn filler_reference_data(start: DateTime<Utc>) -> SchedulingReferenceData {
        let mut first = effective_block(10, 20, "movies", start);
        first.duration = Duration::minutes(10);
        first.deco = Some(EffectiveDeco {
            id: 80,
            default_filler_content: Some("filler".to_owned()),
            default_filler_collection_key: Some(collection_key(CollectionType::Collection, 80)),
            default_filler_trim_to_fit: true,
            break_content: Vec::new(),
        });
        SchedulingReferenceData {
            effective_blocks: vec![
                first,
                effective_block(11, 21, "movies", start + Duration::minutes(25)),
            ],
            media_by_key: BTreeMap::from([
                (
                    "movies".to_owned(),
                    vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
                ),
                (
                    "filler".to_owned(),
                    vec![SchedulingMediaItem::new(90, Duration::minutes(7))],
                ),
            ]),
            ..SchedulingReferenceData::default()
        }
    }

    #[test]
    fn block_collection_etag_change_schedules_only_changed_block_snapshot() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "a".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
        );
        reference_data.media_by_key.insert(
            "b".to_owned(),
            vec![SchedulingMediaItem::new(2, Duration::minutes(10))],
        );
        reference_data.effective_blocks = vec![
            effective_block(10, 20, "a", start),
            effective_block(11, 21, "b", start + Duration::minutes(30)),
        ];
        let unchanged_key =
            collection_key_json(&reference_data.effective_blocks[0].items[0].collection_key);
        let changed_key =
            collection_key_json(&reference_data.effective_blocks[1].items[0].collection_key);
        reference_data.existing_items = vec![
            existing_item(
                1,
                10,
                20,
                start,
                unchanged_key,
                collection_etag_for_items(&[SchedulingMediaItem::new(1, Duration::minutes(10))]),
            ),
            existing_item(
                2,
                11,
                21,
                start + Duration::minutes(30),
                changed_key,
                "old".to_owned(),
            ),
        ];

        let result = schedule(reference_data, start, Duration::hours(1), 100, 0);

        insta::assert_json_snapshot!(serde_json::json!({
            "items": timeline(&result.added_items),
            "items_to_remove": result.items_to_remove,
            "history_to_remove": result.history_to_remove,
        }));
    }

    #[test]
    fn block_item_flag_change_removes_existing_item_and_history_snapshot() {
        let start = utc("2026-04-15T08:00:00Z");
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "a".to_owned(),
            vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
        );
        let mut block = effective_block(10, 20, "a", start);
        block.items[0].include_in_program_guide = false;
        block.items[0].disable_watermarks = true;
        block.items[0].watermark_ids = vec![42];
        block.items[0].graphics_element_ids = vec![77];
        let collection_key = collection_key_json(&block.items[0].collection_key);
        reference_data.effective_blocks = vec![block];
        reference_data.existing_items = vec![PlayoutItem {
            filler_kind: FillerKind::None,
            disable_watermarks: false,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
            ..existing_item(
                1,
                10,
                20,
                start,
                collection_key,
                collection_etag_for_items(&[SchedulingMediaItem::new(1, Duration::minutes(10))]),
            )
        }];
        reference_data.existing_history = vec![ersatzrs_domain::PlayoutHistory {
            id: 9001,
            playout_id: 100,
            when: start,
            finish: Some(start + Duration::minutes(10)),
            key: "old-block-history".to_owned(),
            ..ersatzrs_domain::PlayoutHistory::default()
        }];

        let result = schedule(reference_data, start, Duration::hours(1), 100, 0);

        insta::assert_json_snapshot!(serde_json::json!({
            "items": timeline(&result.added_items),
            "items_to_remove": result.items_to_remove,
            "history_to_remove": result.history_to_remove,
        }));
    }

    #[test]
    fn alternate_schedule_selector_prefers_first_matching_index_and_reverse_date_ranges() {
        let start = utc("2026-01-05T08:00:00Z");
        let schedules = vec![
            BlockTemplateSchedule {
                id: 2,
                updated_ticks: 2,
                start: None,
                finish: None,
                index: 1,
                days_of_week: vec![1],
                days_of_month: (1..=31).collect(),
                months_of_year: vec![1],
                limit_to_date_range: true,
                start_month: 12,
                start_day: 25,
                start_year: None,
                end_month: 1,
                end_day: 10,
                end_year: None,
                template: BlockTemplate {
                    id: 20,
                    updated_ticks: 20,
                    items: vec![BlockTemplateItem {
                        id: 200,
                        block_id: 2000,
                        start_time: Some(NaiveTime::from_hms_opt(8, 0, 0).expect("valid time")),
                        sort_order: 0,
                    }],
                },
                blocks: BTreeMap::from([(
                    2000,
                    BlockDefinition {
                        id: 2000,
                        updated_ticks: 2000,
                        name: "holiday".to_owned(),
                        duration: Duration::minutes(30),
                        stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
                        items: vec![block_item(2001, 2000, "holiday", true)],
                    },
                )]),
                deco: None,
                deco_template_items: Vec::new(),
            },
            BlockTemplateSchedule {
                id: 1,
                updated_ticks: 1,
                start: None,
                finish: None,
                index: 0,
                days_of_week: vec![1],
                days_of_month: (1..=31).collect(),
                months_of_year: vec![1],
                limit_to_date_range: false,
                start_month: 1,
                start_day: 1,
                start_year: None,
                end_month: 12,
                end_day: 31,
                end_year: None,
                template: BlockTemplate {
                    id: 10,
                    updated_ticks: 10,
                    items: vec![BlockTemplateItem {
                        id: 100,
                        block_id: 1000,
                        start_time: Some(NaiveTime::from_hms_opt(8, 0, 0).expect("valid time")),
                        sort_order: 0,
                    }],
                },
                blocks: BTreeMap::from([(
                    1000,
                    BlockDefinition {
                        id: 1000,
                        updated_ticks: 1000,
                        name: "default".to_owned(),
                        duration: Duration::minutes(30),
                        stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
                        items: vec![block_item(1001, 1000, "default", true)],
                    },
                )]),
                deco: None,
                deco_template_items: Vec::new(),
            },
        ];

        let blocks = EffectiveBlock::from_templates(&schedules, start, 1);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_id, 1000);
        assert_eq!(
            blocks[0]
                .block_key
                .as_ref()
                .map(|key| key.playout_template_id),
            Some(1)
        );
    }

    #[test]
    fn deco_template_items_can_override_or_disable_channel_deco() {
        let channel_deco = EffectiveDeco {
            id: 10,
            default_filler_content: Some("channel-filler".to_owned()),
            default_filler_collection_key: Some(collection_key(CollectionType::Collection, 10)),
            default_filler_trim_to_fit: true,
            break_content: Vec::new(),
        };
        let override_deco = EffectiveDeco {
            id: 11,
            default_filler_content: Some("override-filler".to_owned()),
            default_filler_collection_key: Some(collection_key(CollectionType::Collection, 11)),
            default_filler_trim_to_fit: false,
            break_content: Vec::new(),
        };
        let schedule = BlockTemplateSchedule {
            id: 1,
            updated_ticks: 1,
            start: None,
            finish: None,
            index: 0,
            days_of_week: vec![1, 2, 3, 4, 5, 6, 0],
            days_of_month: (1..=31).collect(),
            months_of_year: (1..=12).collect(),
            limit_to_date_range: false,
            start_month: 1,
            start_day: 1,
            start_year: None,
            end_month: 12,
            end_day: 31,
            end_year: None,
            template: BlockTemplate {
                id: 1,
                updated_ticks: 1,
                items: Vec::new(),
            },
            blocks: BTreeMap::new(),
            deco: Some(channel_deco.clone()),
            deco_template_items: vec![
                EffectiveDecoTemplateItem {
                    start_time: NaiveTime::from_hms_opt(6, 0, 0).expect("valid time"),
                    end_time: NaiveTime::from_hms_opt(12, 0, 0).expect("valid time"),
                    selection: EffectiveDecoSelection::Override(Some(Box::new(
                        override_deco.clone(),
                    ))),
                },
                EffectiveDecoTemplateItem {
                    start_time: NaiveTime::from_hms_opt(12, 0, 0).expect("valid time"),
                    end_time: NaiveTime::MIN,
                    selection: EffectiveDecoSelection::Disable,
                },
            ],
        };

        assert_eq!(
            schedule
                .deco_for(utc("2026-04-15T08:00:00Z"))
                .as_ref()
                .map(|deco| deco.id),
            Some(11)
        );
        assert_eq!(schedule.deco_for(utc("2026-04-15T14:00:00Z")), None);
    }

    pub(super) fn schedule(
        reference_data: SchedulingReferenceData,
        start: DateTime<Utc>,
        duration: Duration,
        playout_id: EntityId,
        seed: i32,
    ) -> SchedulingResult {
        BlockSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: playout_id,
                    seed,
                    ..Playout::default()
                },
                start,
                finish: start + duration,
                mode: PlayoutBuildMode::Reset,
                reference_data,
            })
            .expect("block schedule builds")
    }

    pub(super) fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("valid time")
            .with_timezone(&Utc)
    }

    fn block_item(
        id: EntityId,
        block_id: EntityId,
        content: &str,
        include_in_program_guide: bool,
    ) -> EffectiveBlockItem {
        EffectiveBlockItem {
            id,
            block_id,
            content: content.to_owned(),
            collection_key: collection_key(CollectionType::Collection, id),
            playback_order: PlaybackOrder::Chronological,
            include_in_program_guide,
            disable_watermarks: false,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
        }
    }

    fn collection_key(collection_type: CollectionType, id: EntityId) -> SchedulingCollectionKey {
        SchedulingCollectionKey {
            collection_type,
            collection_id: (collection_type == CollectionType::Collection).then_some(id),
            playlist_id: (collection_type == CollectionType::Playlist).then_some(id),
            ..SchedulingCollectionKey::default()
        }
    }

    fn block_key(
        block_id: EntityId,
        template_item_id: EntityId,
        playout_template_id: EntityId,
    ) -> BlockKey {
        BlockKey {
            block_id,
            block_updated_ticks: block_id * 10,
            template_id: template_item_id,
            template_updated_ticks: template_item_id * 10,
            playout_template_id,
            playout_template_updated_ticks: playout_template_id * 10,
        }
    }

    fn effective_block(
        block_id: EntityId,
        template_item_id: EntityId,
        content: &str,
        start: DateTime<Utc>,
    ) -> EffectiveBlock {
        EffectiveBlock {
            block_id,
            block_key: Some(block_key(block_id, template_item_id, template_item_id + 10)),
            template_item_id,
            name: content.to_owned(),
            start,
            duration: Duration::minutes(20),
            stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
            items: vec![block_item(block_id, block_id, content, true)],
            deco: None,
        }
    }

    fn existing_item(
        id: EntityId,
        block_id: EntityId,
        template_item_id: EntityId,
        start: DateTime<Utc>,
        collection_key: String,
        collection_etag: String,
    ) -> PlayoutItem {
        PlayoutItem {
            id,
            media_item_id: id,
            start,
            finish: start + Duration::minutes(10),
            guide_start: Some(start),
            guide_finish: Some(start + Duration::minutes(20)),
            guide_group: i32::try_from(template_item_id).unwrap_or_default(),
            block_key: serde_json::to_string(&block_key(
                block_id,
                template_item_id,
                template_item_id + 10,
            ))
            .ok(),
            collection_key: Some(collection_key),
            collection_etag: Some(collection_etag),
            ..PlayoutItem::default()
        }
    }

    fn timeline(items: &[PlayoutItem]) -> Vec<serde_json::Value> {
        items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "media": item.media_item_id,
                    "start": item.start.to_rfc3339(),
                    "finish": item.finish.to_rfc3339(),
                    "filler": format!("{:?}", item.filler_kind),
                    "guide_group": item.guide_group,
                    "guide_start": item.guide_start.map(|start| start.to_rfc3339()),
                    "guide_finish": item.guide_finish.map(|finish| finish.to_rfc3339()),
                    "watermarks": item.watermark_ids,
                    "graphics": item.graphics_element_ids,
                    "disable_watermarks": item.disable_watermarks,
                    "block_key": item.block_key,
                    "collection_key": item.collection_key,
                    "collection_etag": item.collection_etag,
                    "context": item.scheduling_context,
                })
            })
            .collect()
    }
}
