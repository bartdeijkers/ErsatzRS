use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveTime, Utc};
use ersatzrs_core::scheduling::{
    BlockDefinition, BlockKey, BlockSchedulingEngine, BlockTemplate, BlockTemplateItem,
    BlockTemplateSchedule, EffectiveBlock, EffectiveBlockItem, EffectiveDeco, PlayoutBuildMode,
    PlayoutSchedulingEngine, SchedulingCollectionKey, SchedulingMediaItem, SchedulingReferenceData,
    SchedulingRequest,
};
use ersatzrs_domain::{
    BlockStopScheduling, CollectionType, FillerKind, PlaybackOrder, Playout, PlayoutItem,
};
use serde_json::Value;

#[test]
fn basic_chronological_block_matches_ersatztv_fixture() {
    // Golden sequence captured from ErsatzTV BlockPlayoutBuilder.cs lines 248-275
    // using chronological block items and a fixed UTC anchor.
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
            EffectiveBlockItem {
                watermark_ids: vec![42],
                ..block_item(1, 10, "movies", true)
            },
            EffectiveBlockItem {
                id: 2,
                block_id: 10,
                content: "shorts".to_owned(),
                collection_key: collection_key(CollectionType::Collection, 2),
                playback_order: PlaybackOrder::Chronological,
                include_in_program_guide: false,
                disable_watermarks: true,
                watermark_ids: Vec::new(),
                graphics_element_ids: vec![77],
            },
        ],
        deco: None,
    });

    assert_fixture(
        "basic_chronological_block.json",
        &schedule(reference_data, start, Duration::hours(2), 100, 0),
    );
}

#[test]
fn deco_default_filler_matches_ersatztv_fixture() {
    // Golden sequence follows ErsatzTV BlockPlayoutFillerBuilder.cs lines
    // 316-390: default deco filler fills the gap between consecutive block
    // items and trims the final filler item when requested.
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
                default_filler_collection_key: Some(collection_key(CollectionType::Collection, 80)),
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

    assert_fixture(
        "deco_default_filler.json",
        &schedule(reference_data, start, Duration::hours(1), 100, 0),
    );
}

#[test]
fn guide_group_locking_matches_ersatztv_fixture() {
    // ErsatzTV EffectiveBlock.cs lines 76-87 normalizes a block when every
    // block item is hidden from the guide, so all generated PlayoutItems remain
    // guide-visible and locked to the TemplateItem guide group.
    let start = utc("2026-04-15T09:30:00Z");
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.media_by_key.insert(
        "hidden-a".to_owned(),
        vec![SchedulingMediaItem::new(301, Duration::minutes(15))],
    );
    reference_data.media_by_key.insert(
        "hidden-b".to_owned(),
        vec![SchedulingMediaItem::new(302, Duration::minutes(10))],
    );
    reference_data.effective_blocks = EffectiveBlock::from_templates(
        &[BlockTemplateSchedule {
            id: 101,
            updated_ticks: 1001,
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
                id: 200,
                updated_ticks: 201,
                items: vec![BlockTemplateItem {
                    id: 40,
                    block_id: 30,
                    start_time: Some(NaiveTime::from_hms_opt(10, 0, 0).expect("valid time")),
                    sort_order: 0,
                }],
            },
            blocks: BTreeMap::from([(
                30,
                BlockDefinition {
                    id: 30,
                    updated_ticks: 301,
                    name: "Hidden Guide Group".to_owned(),
                    duration: Duration::minutes(40),
                    stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
                    items: vec![
                        block_item(31, 30, "hidden-a", false),
                        block_item(32, 30, "hidden-b", false),
                    ],
                },
            )]),
            deco: None,
            deco_template_items: Vec::new(),
        }],
        start,
        1,
    );

    assert_fixture(
        "guide_group_locking.json",
        &schedule(reference_data, start, Duration::hours(2), 100, 0),
    );
}

fn assert_fixture(fixture_name: &str, result: &ersatzrs_core::scheduling::SchedulingResult) {
    let actual = serde_json::json!({
        "items": playout_sequence(&result.added_items),
    });
    let expected: Value = serde_json::from_str(fixture(fixture_name)).expect("valid fixture JSON");
    assert_eq!(expected, actual, "fixture {fixture_name} should match");
}

fn fixture(name: &str) -> &'static str {
    match name {
        "basic_chronological_block.json" => {
            include_str!("fixtures/block_scheduling/basic_chronological_block.json")
        }
        "deco_default_filler.json" => {
            include_str!("fixtures/block_scheduling/deco_default_filler.json")
        }
        "guide_group_locking.json" => {
            include_str!("fixtures/block_scheduling/guide_group_locking.json")
        }
        _ => panic!("unknown fixture {name}"),
    }
}

fn schedule(
    reference_data: SchedulingReferenceData,
    start: DateTime<Utc>,
    duration: Duration,
    playout_id: i64,
    seed: i32,
) -> ersatzrs_core::scheduling::SchedulingResult {
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

fn playout_sequence(items: &[PlayoutItem]) -> Vec<Value> {
    items
        .iter()
        .map(|item| {
            serde_json::json!({
                "media": item.media_item_id,
                "start": item.start.to_rfc3339(),
                "finish": item.finish.to_rfc3339(),
                "filler": filler_kind(item.filler_kind),
                "guide_group": item.guide_group,
                "guide_start": item.guide_start.map(|start| start.to_rfc3339()),
                "guide_finish": item.guide_finish.map(|finish| finish.to_rfc3339()),
                "disable_watermarks": item.disable_watermarks,
                "watermarks": item.watermark_ids,
                "graphics": item.graphics_element_ids,
                "block_key": item.block_key,
                "collection_key": item.collection_key,
                "scheduling_context": item.scheduling_context,
            })
        })
        .collect()
}

fn filler_kind(kind: FillerKind) -> &'static str {
    match kind {
        FillerKind::None => "None",
        FillerKind::PreRoll => "PreRoll",
        FillerKind::MidRoll => "MidRoll",
        FillerKind::PostRoll => "PostRoll",
        FillerKind::Tail => "Tail",
        FillerKind::Fallback => "Fallback",
        FillerKind::GuideMode => "GuideMode",
        FillerKind::DecoDefault => "DecoDefault",
    }
}

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid time")
        .with_timezone(&Utc)
}

fn block_item(
    id: i64,
    block_id: i64,
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

fn collection_key(collection_type: CollectionType, id: i64) -> SchedulingCollectionKey {
    SchedulingCollectionKey {
        collection_type,
        collection_id: (collection_type == CollectionType::Collection).then_some(id),
        playlist_id: (collection_type == CollectionType::Playlist).then_some(id),
        ..SchedulingCollectionKey::default()
    }
}

fn block_key(block_id: i64, template_item_id: i64, playout_template_id: i64) -> BlockKey {
    BlockKey {
        block_id,
        block_updated_ticks: block_id * 10,
        template_id: template_item_id,
        template_updated_ticks: template_item_id * 10,
        playout_template_id,
        playout_template_updated_ticks: playout_template_id * 10,
    }
}
