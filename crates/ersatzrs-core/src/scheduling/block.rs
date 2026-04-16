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
    Override(Option<EffectiveDeco>),
}

impl EffectiveDecoSelection {
    fn resolve(&self, channel_deco: Option<&EffectiveDeco>) -> Option<EffectiveDeco> {
        match self {
            Self::Inherit => channel_deco.cloned(),
            Self::Disable => None,
            Self::Override(deco) => deco.clone(),
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

impl PlayoutSchedulingEngine for BlockSchedulingEngine {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        let mut result = SchedulingResult::default();

        let mut blocks = request.reference_data.effective_blocks.clone();
        blocks.sort_by_key(|block| block.start);
        result.remove_before = blocks
            .first()
            .map(|block| block.start)
            .or(Some(request.start));

        let collection_etags = collection_etags(&request.reference_data, &blocks);
        let blocks_to_update = blocks_to_update(&request, &blocks, &collection_etags);
        let block_starts_to_update = blocks_to_update
            .iter()
            .map(|block| block.start)
            .collect::<BTreeSet<_>>();

        schedule_updated_blocks(
            &request,
            blocks,
            &block_starts_to_update,
            &collection_etags,
            &mut result,
        );

        apply_block_deco_filler(
            request.playout.id,
            request.playout.seed,
            request.finish,
            &request.reference_data,
            &mut result,
        );

        result.history_to_remove.extend(cleanup_history(
            &request.reference_data.existing_history,
            request.start,
            request.reference_data.max_playout_offset,
        ));
        Ok(result)
    }
}

fn schedule_updated_blocks(
    request: &SchedulingRequest,
    blocks: Vec<EffectiveBlock>,
    block_starts_to_update: &BTreeSet<DateTime<Utc>>,
    collection_etags: &BTreeMap<String, String>,
    result: &mut SchedulingResult,
) {
    for block in blocks {
        if !block_starts_to_update.contains(&block.start) || block.start >= request.finish {
            continue;
        }
        schedule_block_items(request, &block, collection_etags, result);
    }
}

fn schedule_block_items(
    request: &SchedulingRequest,
    block: &EffectiveBlock,
    collection_etags: &BTreeMap<String, String>,
    result: &mut SchedulingResult,
) {
    let block_finish = block.start + block.duration;
    let mut current_time = block.start;

    for block_item in &block.items {
        if current_time >= block_finish {
            break;
        }

        let mut enumerator = lookup_media(&request.reference_data, &block_item.content)
            .with_playback_order(block_item.playback_order);
        if enumerator.is_empty() {
            result.warnings.push(format!(
                "block item {} skipped empty content {}",
                block_item.id, block_item.content
            ));
            continue;
        }

        let Some(media_item) = enumerator.current().cloned() else {
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
            &enumerator,
        );
        result.added_history.push(history(
            request.playout.id,
            history_key_for_block_item(block_item),
            &item,
            None,
            enumerator.current_index(),
            super::history_details_for_media_item(&media_item),
            false,
        ));
        current_time = item.finish;
        result.added_items.push(item);
        enumerator.advance();
    }
}

fn effective_block_playout_item(
    request: &SchedulingRequest,
    block: &EffectiveBlock,
    block_item: &EffectiveBlockItem,
    media_item: &SchedulingMediaItem,
    current_time: DateTime<Utc>,
    collection_etags: &BTreeMap<String, String>,
    enumerator: &super::MediaEnumerator,
) -> PlayoutItem {
    let block_finish = block.start + block.duration;
    let collection_key = serde_json::to_string(&block_item.collection_key).unwrap_or_default();
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
            r#"{{"blockId":{},"blockItemId":{},"enumerator":"{}","seed":{},"index":{}}}"#,
            block.block_id, block_item.id, enumerator.key, request.playout.seed, enumerator.index
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
    serde_json::to_string(collection_key).unwrap_or_default()
}

fn collection_etag_for_items(items: &[SchedulingMediaItem]) -> String {
    let mut value: u64 = 14_695_981_039_346_656_037;
    for item in items {
        for byte in item.id.to_le_bytes() {
            value ^= u64::from(byte);
            value = value.wrapping_mul(1_099_511_628_211);
        }
        for byte in item.duration.num_milliseconds().to_le_bytes() {
            value ^= u64::from(byte);
            value = value.wrapping_mul(1_099_511_628_211);
        }
    }
    format!("{value:016x}")
}

fn blocks_to_update(
    request: &SchedulingRequest,
    blocks: &[EffectiveBlock],
    collection_etags: &BTreeMap<String, String>,
) -> Vec<EffectiveBlock> {
    if request.reference_data.existing_items.is_empty() {
        return blocks.to_vec();
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
        let key_affected = earliest_block_by_key
            .get(block_key)
            .is_some_and(|start| *start <= item.start);
        let id_affected = earliest_block_by_id
            .get(&block_key.block_id)
            .is_some_and(|start| *start <= item.start);
        if item.start >= request.start
            && (!block_keys_to_schedule.contains(block_key)
                || collection_changed
                || key_affected
                || id_affected)
        {
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

    blocks
        .iter()
        .filter(|block| updated_starts.contains(&block.start))
        .cloned()
        .collect()
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

fn apply_block_deco_filler(
    playout_id: EntityId,
    playout_seed: i32,
    schedule_finish: DateTime<Utc>,
    reference_data: &super::SchedulingReferenceData,
    result: &mut SchedulingResult,
) {
    let mut blocks = reference_data.effective_blocks.clone();
    blocks.sort_by_key(|block| block.start);

    for block in blocks {
        if block.start >= schedule_finish {
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
        );
    }

    let mut all_items = reference_data.existing_items.clone();
    all_items.retain(|item| !result.items_to_remove.contains(&item.id));
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
            one.finish,
            two.start,
        );
    }
}

fn add_block_start_break_content(
    playout_id: EntityId,
    playout_seed: i32,
    reference_data: &super::SchedulingReferenceData,
    result: &mut SchedulingResult,
    block: &EffectiveBlock,
    deco: &EffectiveDeco,
    block_finish: DateTime<Utc>,
) {
    let remaining = block.duration
        - result
            .added_items
            .iter()
            .filter(|item| {
                item.guide_group == i32::try_from(block.template_item_id).unwrap_or_default()
                    && item.guide_start == Some(block.start)
            })
            .map(|item| item.finish - item.start)
            .fold(Duration::zero(), |left, right| left + right);
    let mut inserted = Vec::new();
    let mut inserted_duration = Duration::zero();

    for break_content in deco
        .break_content
        .iter()
        .filter(|content| content.placement == DecoBreakPlacement::BlockStart)
    {
        let mut enumerator = lookup_media(reference_data, &break_content.content);
        if enumerator.is_empty() {
            continue;
        }
        if playout_seed != 0 {
            enumerator.set_index(usize::try_from(playout_seed.unsigned_abs()).unwrap_or_default());
        }
        let Some(media_item) = enumerator.current().cloned() else {
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
        filler.collection_key = serde_json::to_string(&break_content.collection_key).ok();
        inserted_duration += media_item.duration;
        result.added_history.push(history(
            playout_id,
            history_key_for_break_content(break_content),
            &filler,
            None,
            enumerator.current_index(),
            super::history_details_for_media_item(&media_item),
            false,
        ));
        inserted.push(filler);
    }

    if inserted.is_empty() {
        return;
    }

    for item in result.added_items.iter_mut().filter(|item| {
        item.guide_group == i32::try_from(block.template_item_id).unwrap_or_default()
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
        if history.key.contains("\"DecoBreakContentId\"") {
            continue;
        }
        history.when += inserted_duration;
        if let Some(finish) = history.finish.as_mut() {
            *finish += inserted_duration;
        }
    }
    result.added_items.extend(inserted);
    result.added_items.sort_by_key(|item| item.start);
}

fn add_default_filler(
    runtime: FillerRuntime,
    reference_data: &super::SchedulingReferenceData,
    result: &mut SchedulingResult,
    deco: &EffectiveDeco,
    guide_item: &PlayoutItem,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) {
    let Some(content) = &deco.default_filler_content else {
        return;
    };
    let Some(collection_key) = &deco.default_filler_collection_key else {
        return;
    };
    let mut enumerator = lookup_media(reference_data, content);
    if enumerator.is_empty() {
        return;
    }
    if runtime.playout_seed != 0 {
        enumerator
            .set_index(usize::try_from(runtime.playout_seed.unsigned_abs()).unwrap_or_default());
    }

    let mut current = start;
    while current < finish {
        let Some(media_item) = enumerator.current().cloned() else {
            break;
        };
        let mut duration = media_item.duration;
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
        filler.collection_key = serde_json::to_string(collection_key).ok();
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
}

#[derive(Debug, Clone, Copy)]
struct FillerRuntime {
    playout_id: EntityId,
    playout_seed: i32,
}

fn history_key_for_block_item(block_item: &EffectiveBlockItem) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct Key<'a> {
        block_id: EntityId,
        playback_order: PlaybackOrder,
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
        search_query: Option<&'a str>,
    }

    serde_json::to_string(&Key {
        block_id: block_item.block_id,
        playback_order: block_item.playback_order,
        collection_type: block_item.collection_key.collection_type,
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
        "PlaybackOrder": "Shuffle",
        "CollectionType": format!("{:?}", break_content.collection_key.collection_type),
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
        "PlaybackOrder": "Shuffle",
        "CollectionType": format!("{:?}", collection_key.collection_type),
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
                    selection: EffectiveDecoSelection::Override(Some(override_deco.clone())),
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

    fn schedule(
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

    fn utc(value: &str) -> DateTime<Utc> {
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
