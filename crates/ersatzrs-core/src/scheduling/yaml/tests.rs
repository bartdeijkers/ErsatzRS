use super::*;
use crate::scheduling::{
    SchedulingCollectionKey, SchedulingMediaItem, SchedulingPlaylistItem, SchedulingReferenceData,
};
use ersatzrs_domain::{PlaybackOrder, Playout};

macro_rules! assert_yaml_json_snapshot {
    ($($tokens:tt)*) => {
        insta::with_settings!({ snapshot_path => "../snapshots" }, {
            insta::assert_json_snapshot!($($tokens)*);
        });
    };
}

#[test]
fn yaml_instruction_shape_round_trips_all_upstream_fields() {
    let instruction = YamlInstruction::default();
    let value = serde_yaml::to_value(&instruction).expect("serialize instruction");
    let mapping = value.as_mapping().expect("instruction mapping");
    let actual = mapping
        .keys()
        .map(|key| key.as_str().expect("string key"))
        .collect::<BTreeSet<_>>();
    let expected = [
        "advance",
        "all",
        "content",
        "count",
        "custom_title",
        "disable_watermarks",
        "discard_attempts",
        "duration",
        "epg_group",
        "episode",
        "expression",
        "fallback",
        "filler_kind",
        "graphics_off",
        "graphics_on",
        "mid_roll",
        "name",
        "offline_tail",
        "pad_to_next",
        "pad_until",
        "post_roll",
        "pre_roll",
        "repeat",
        "rewind",
        "rewind_on_reset",
        "season",
        "sequence",
        "sequence_guid",
        "sequence_key",
        "shuffle_sequence",
        "skip_items",
        "skip_to_item",
        "stop_before_end",
        "tomorrow",
        "trim",
        "variables",
        "wait_until",
        "watermark",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual, expected);

    let serialized = serde_yaml::to_string(&instruction).expect("serialize instruction");
    let round_trip: YamlInstruction =
        serde_yaml::from_str(&serialized).expect("deserialize instruction");
    assert_eq!(round_trip, instruction);
}

#[test]
fn malformed_multi_field_instruction_uses_upstream_precedence() {
    let mut instruction: YamlInstruction = serde_yaml::from_str(
        r#"
wait_until: "10:30"
rewind: "00:05:00"
epg_group: true
watermark: true
graphics_on: lower-third
graphics_off: lower-third
pre_roll: true
post_roll: true
mid_roll: true
repeat: true
sequence: bumper
shuffle_sequence: shuffled
skip_items: "1"
skip_to_item: main
all: main
count: "1"
duration: "00:10:00"
pad_to_next: 15
pad_until: "11:00"
"#,
    )
    .expect("deserialize multi-field instruction");

    assert_eq!(instruction.kind(), YamlInstructionKind::WaitUntil);
    instruction.wait_until = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::Rewind);
    instruction.rewind = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::EpgGroup);
    instruction.epg_group = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::Watermark);
    instruction.watermark = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::GraphicsOn);
    instruction.graphics_on = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::GraphicsOff);
    instruction.graphics_off = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::PreRoll);
    instruction.pre_roll = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::PostRoll);
    instruction.post_roll = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::MidRoll);
    instruction.mid_roll = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::Repeat);
    instruction.repeat = Some(false);
    assert_eq!(instruction.kind(), YamlInstructionKind::Sequence);
    instruction.sequence = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::ShuffleSequence);
    instruction.shuffle_sequence = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::SkipItems);
    instruction.skip_items = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::SkipToItem);
    instruction.skip_to_item = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::All);
    instruction.all = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::Count);
    instruction.count = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::Duration);
    instruction.duration = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::PadToNext);
    instruction.pad_to_next = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::PadUntil);
    instruction.pad_until = None;
    assert_eq!(instruction.kind(), YamlInstructionKind::Unsupported);
}

fn request(yaml: &str) -> (YamlPlayoutDefinition, SchedulingRequest) {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.media_by_key.insert(
        "main".to_owned(),
        vec![
            SchedulingMediaItem::new(1, Duration::minutes(10)),
            SchedulingMediaItem::new(2, Duration::minutes(15)),
        ],
    );
    reference_data.media_by_key.insert(
        "bumpers".to_owned(),
        vec![SchedulingMediaItem::new(9, Duration::minutes(1))],
    );
    reference_data
        .watermark_ids_by_name
        .insert("bug".to_owned(), 42);
    reference_data
        .graphics_ids_by_path
        .insert("lower-third".to_owned(), 77);
    let definition = YamlPlayoutEngine::definition_from_str(yaml).expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:07:00Z")
        .expect("valid time")
        .with_timezone(&Utc);
    (
        definition,
        SchedulingRequest {
            playout: Playout {
                id: 100,
                ..Playout::default()
            },
            start,
            finish: start + Duration::hours(2),
            mode: PlayoutBuildMode::Reset,
            reference_data,
        },
    )
}

fn snapshot(yaml: &str) -> serde_json::Value {
    let result = schedule_yaml(yaml);
    snapshot_result(&result)
}

fn snapshot_result(result: &SchedulingResult) -> serde_json::Value {
    serde_json::json!({
        "items": result.added_items.iter().map(|item| serde_json::json!({
            "media": item.media_item_id,
            "start": item.start.to_rfc3339(),
            "finish": item.finish.to_rfc3339(),
            "filler": format!("{:?}", item.filler_kind),
            "guide_group": item.guide_group,
            "watermarks": item.watermark_ids,
            "graphics": item.graphics_element_ids,
            "title": item.custom_title,
        })).collect::<Vec<_>>(),
        "gaps": result.added_gaps.iter().map(|gap| serde_json::json!({
            "start": gap.start.to_rfc3339(),
            "finish": gap.finish.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "anchor": result.anchor.as_ref().map(|anchor| anchor.next_start.to_rfc3339()),
    })
}

#[test]
fn yaml_smart_shuffle_policy_round_trip_is_additive_and_absent_by_default() {
    let without_policy = YamlPlayoutEngine::definition_from_str(
            "content:\n  - key: main\n    search: all\n    order: shuffle\nplayout:\n  - count: 1\n    content: main\n",
        )
        .expect("definition without policy");
    assert!(without_policy.content[0].smart_shuffle.is_disabled());
    assert!(!serde_yaml::to_string(&without_policy)
        .expect("serialize definition")
        .contains("cooldown:"));

    let with_policy = YamlPlayoutEngine::definition_from_str(
            "cooldown:\n  version: 1\n  enabled: true\n  preset: custom\n  scope: playout\n  mode: soft\n  after_items: 3\n  after_duration: 01:00:00\n  include_filler: false\n  rules:\n    - id: show\n      kind: show\ncontent:\n  - key: main\n    search: all\n    order: shuffle\nplayout:\n  - count: 1\n    content: main\n",
        )
        .expect("definition with policy");
    assert!(with_policy.content[0].smart_shuffle.is_active());
    assert_eq!(with_policy.content[0].smart_shuffle.after_items, Some(3));
    assert_eq!(
        with_policy.content[0]
            .smart_shuffle
            .after_duration
            .as_deref(),
        Some("01:00:00")
    );
    let serialized = serde_yaml::to_string(&with_policy).expect("serialize policy");
    assert!(serialized.contains("smart_shuffle:"));
    assert!(!serialized.contains("cooldown:"));
    let round_trip =
        YamlPlayoutEngine::definition_from_str(&serialized).expect("deserialize policy");
    assert_eq!(
        round_trip.content[0].smart_shuffle,
        with_policy.content[0].smart_shuffle
    );
}

#[test]
fn yaml_explicit_off_item_does_not_inherit_legacy_policy() {
    let definition = YamlPlayoutEngine::definition_from_str(
            "cooldown:\n  enabled: true\n  preset: custom\n  after_items: 3\n  rules:\n    - id: show\n      kind: show\ncontent:\n  - key: inherited\n    search: all\n  - key: opted-out\n    search: all\n    smart_shuffle: {}\n",
        )
        .expect("definition with an explicit item opt-out");

    assert!(definition.content[0].smart_shuffle.is_active());
    assert!(definition.content[1].smart_shuffle.is_disabled());
    assert!(definition.legacy_smart_shuffle.is_disabled());
}

fn schedule_yaml(yaml: &str) -> SchedulingResult {
    let (definition, request) = request(yaml);
    YamlPlayoutEngine
        .schedule_definition(definition, request)
        .expect("schedule builds")
}

fn episode_item(
    id: EntityId,
    show_id: EntityId,
    season_id: EntityId,
    season: i32,
    episode: i32,
) -> SchedulingMediaItem {
    SchedulingMediaItem {
        show_id: Some(show_id),
        season_id: Some(season_id),
        season: Some(season),
        episode: Some(episode),
        ..SchedulingMediaItem::new(id, Duration::minutes(10))
    }
}

#[test]
fn yaml_count_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - count: "2"
    content: main
"#
    ));
}

#[test]
fn yaml_duration_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - duration: "00:20:00"
    content: main
    trim: true
"#
    ));
}

#[test]
fn yaml_all_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r"
playout:
  - all: main
"
    ));
}

#[test]
fn yaml_content_sources_snapshot() {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.collections_by_name.insert(
        "Movies".to_owned(),
        vec![SchedulingMediaItem::new(10, Duration::minutes(10))],
    );
    reference_data.multi_collections_by_name.insert(
        "Saturday".to_owned(),
        vec![SchedulingMediaItem::new(20, Duration::minutes(10))],
    );
    reference_data.smart_collections_by_name.insert(
        "Unwatched".to_owned(),
        vec![SchedulingMediaItem::new(30, Duration::minutes(10))],
    );
    reference_data.playlists_by_group_and_name.insert(
        (Some("Prime".to_owned()), "Block A".to_owned()),
        vec![SchedulingMediaItem::new(40, Duration::minutes(10))],
    );
    reference_data.searches_by_query.insert(
        "genre = horror".to_owned(),
        vec![SchedulingMediaItem::new(50, Duration::minutes(10))],
    );
    reference_data
        .media_items_by_id
        .insert(60, SchedulingMediaItem::new(60, Duration::minutes(10)));

    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: collection
    collection: Movies
    order: chronological
  - key: multi
    multi_collection: Saturday
    order: shuffle
  - key: smart
    smart_collection: Unwatched
    order: shuffle
  - key: playlist
    playlist_group: Prime
    playlist: Block A
  - key: search
    search: genre = horror
    order: shuffle
  - key: single
    media_item: 60
    order: none
playout:
  - count: "1"
    content: collection
  - count: "1"
    content: multi
  - count: "1"
    content: smart
  - count: "1"
    content: playlist
  - count: "1"
    content: search
  - count: "1"
    content: single
"#,
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);

    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(2),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        "history": result.added_history.iter().map(|history| &history.key).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_show_and_guid_content_source_resolution_snapshot() {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.shows_by_name.insert(
        "Top Gear".to_owned(),
        vec![
            episode_item(101, 10, 1001, 1, 1),
            episode_item(102, 10, 1001, 1, 2),
        ],
    );
    reference_data.media_by_show_guid.insert(
        "tmdb://200".to_owned(),
        vec![
            episode_item(201, 20, 2001, 1, 1),
            episode_item(202, 20, 2001, 1, 2),
        ],
    );

    let definition = YamlPlayoutEngine::definition_from_str(
        r"
content:
  - key: named_show
    show: Top Gear
    order: season_episode
  - key: guided_show
    guids:
      - source: tmdb
        value: 200
    order: season_episode
playout:
  - all: named_show
  - all: guided_show
",
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);

    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(2),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        "history": result.added_history.iter().map(|history| serde_json::json!({
            "key": history.key,
            "index": history.index,
            "details": history.details,
        })).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_pad_to_next_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r"
playout:
  - pad_to_next: 15
    content: bumpers
    trim: true
"
    ));
}

#[test]
fn yaml_pad_until_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - pad_until: "10:10"
    content: bumpers
    trim: true
"#
    ));
}

#[test]
fn yaml_wait_until_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - wait_until: "10:30"
  - count: "1"
    content: main
"#
    ));
}

#[test]
fn yaml_rewind_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - count: "1"
    content: main
  - rewind: "00:05:00"
  - count: "1"
    content: main
"#
    ));
}

#[test]
fn yaml_pre_and_post_roll_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
sequence:
  - key: bumper
    items:
      - count: "1"
        content: bumpers
playout:
  - pre_roll: true
    sequence: bumper
  - post_roll: true
    sequence: bumper
  - count: "1"
    content: main
"#
    ));
}

#[test]
fn yaml_mid_roll_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
sequence:
  - key: bumper
    items:
      - count: "1"
        content: bumpers
playout:
  - mid_roll: true
    sequence: bumper
  - count: "1"
    content: main
"#
    ));
}

#[test]
fn yaml_watermark_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - watermark: true
    name: bug
  - count: "1"
    content: main
  - watermark: false
    name: bug
  - count: "1"
    content: main
"#
    ));
}

#[test]
fn yaml_graphics_instruction_snapshot() {
    let yaml = r#"
playout:
  - graphics_on: lower-third
    variables:
      title: Now
  - count: "1"
    content: main
  - graphics_off: lower-third
  - count: "1"
    content: main
"#;
    let result = schedule_yaml(yaml);
    assert_eq!(result.added_items[0].graphics_element_ids, vec![77]);
    assert_eq!(
        result.added_items[0].graphics_elements[0]
            .variables
            .as_deref(),
        Some(r#"{"title":"Now"}"#)
    );
    assert!(result.added_items[1].graphics_elements.is_empty());
    assert_yaml_json_snapshot!(snapshot_result(&result));
}

#[test]
fn yaml_epg_group_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - epg_group: true
  - count: "2"
    content: main
  - epg_group: false
  - count: "1"
    content: main
"#
    ));
}

#[test]
fn yaml_skip_items_instruction_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - skip_items: "1"
    content: main
  - count: "1"
    content: main
"#
    ));
}

#[test]
fn yaml_skip_items_expression_snapshot() {
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - skip_items: "count - 1"
    content: main
  - count: "1"
    content: main
"#
    ));
}

#[test]
fn yaml_skip_to_item_instruction_snapshot() {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.media_by_key.insert(
        "main".to_owned(),
        vec![
            SchedulingMediaItem {
                season: Some(1),
                episode: Some(1),
                ..SchedulingMediaItem::new(1, Duration::minutes(10))
            },
            SchedulingMediaItem {
                season: Some(1),
                episode: Some(2),
                ..SchedulingMediaItem::new(2, Duration::minutes(10))
            },
        ],
    );
    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
playout:
  - skip_to_item: main
    content: main
    season: 1
    episode: 2
  - count: "1"
    content: main
"#,
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");
    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>()
    }));
}

#[test]
fn yaml_apply_history_and_cleanup_snapshot() {
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);
    let history_key = history_key_for_content("collection", PlaybackOrder::Chronological);
    let mut reference_data = SchedulingReferenceData {
        max_playout_offset: Duration::zero(),
        ..SchedulingReferenceData::default()
    };
    reference_data.collections_by_name.insert(
        "Movies".to_owned(),
        vec![
            SchedulingMediaItem::new(1, Duration::minutes(10)),
            SchedulingMediaItem::new(2, Duration::minutes(10)),
            SchedulingMediaItem::new(3, Duration::minutes(10)),
        ],
    );
    reference_data.existing_history = vec![
        ersatzrs_domain::PlayoutHistory {
            id: 10,
            key: history_key.clone(),
            when: start - Duration::hours(3),
            finish: Some(start - Duration::hours(3) + Duration::minutes(10)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
        ersatzrs_domain::PlayoutHistory {
            id: 11,
            key: history_key.clone(),
            when: start - Duration::hours(2),
            finish: Some(start - Duration::hours(2) + Duration::minutes(10)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
        ersatzrs_domain::PlayoutHistory {
            id: 12,
            key: history_key,
            when: start,
            finish: Some(start + Duration::minutes(5)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
    ];

    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: collection
    collection: Movies
    order: chronological
playout:
  - count: "1"
    content: collection
"#,
    )
    .expect("valid yaml");
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        "history_to_remove": result.history_to_remove,
    }));
}

#[test]
fn yaml_chronological_history_resume_by_media_item_details_snapshot() {
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);
    let history_key = history_key_for_content("collection", PlaybackOrder::Chronological);
    let mut reference_data = SchedulingReferenceData::default();
    let item_1 = SchedulingMediaItem::new(1, Duration::minutes(10));
    let item_2 = SchedulingMediaItem::new(2, Duration::minutes(10));
    let item_3 = SchedulingMediaItem::new(3, Duration::minutes(10));
    reference_data
        .collections_by_name
        .insert("Movies".to_owned(), vec![item_1, item_2.clone(), item_3]);
    reference_data.existing_history = vec![ersatzrs_domain::PlayoutHistory {
        id: 20,
        key: history_key,
        when: start - Duration::hours(1),
        finish: Some(start - Duration::hours(1) + Duration::minutes(10)),
        index: 0,
        details: history_details_for_media_item(&item_2),
        ..ersatzrs_domain::PlayoutHistory::default()
    }];

    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: collection
    collection: Movies
    order: chronological
playout:
  - count: "1"
    content: collection
"#,
    )
    .expect("valid yaml");
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        "history": result.added_history.iter().map(|history| serde_json::json!({
            "key": history.key,
            "index": history.index,
            "details": history.details,
        })).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_playlist_history_keys_and_current_child_snapshot() {
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);
    let playlist_key = history_key_for_content("playlist", PlaybackOrder::None);
    let child_0_key = SchedulingCollectionKey::fake_playlist_item(0);
    let child_1_key = SchedulingCollectionKey::fake_playlist_item(1);
    let child_0_history_key = history_key_for_collection_key(&child_0_key);
    let mut reference_data = SchedulingReferenceData::default();
    let item_1 = SchedulingMediaItem {
        season: Some(1),
        episode: Some(1),
        ..SchedulingMediaItem::new(1, Duration::minutes(10))
    };
    reference_data.playlist_items_by_group_and_name.insert(
        (Some("Prime".to_owned()), "Block A".to_owned()),
        vec![
            SchedulingPlaylistItem {
                collection_key: child_0_key,
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: None,
                media_items: vec![
                    item_1.clone(),
                    SchedulingMediaItem {
                        season: Some(1),
                        episode: Some(2),
                        ..SchedulingMediaItem::new(2, Duration::minutes(10))
                    },
                ],
            },
            SchedulingPlaylistItem {
                collection_key: child_1_key,
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: None,
                media_items: vec![SchedulingMediaItem::new(10, Duration::minutes(10))],
            },
        ],
    );
    reference_data.existing_history = vec![
        ersatzrs_domain::PlayoutHistory {
            id: 30,
            key: playlist_key.clone(),
            when: start - Duration::hours(1),
            finish: Some(start - Duration::hours(1) + Duration::minutes(10)),
            index: 0,
            details: history_details_for_media_item(&item_1),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
        ersatzrs_domain::PlayoutHistory {
            id: 31,
            key: playlist_key,
            child_key: Some(child_0_history_key),
            when: start - Duration::hours(1),
            finish: Some(start - Duration::hours(1) + Duration::minutes(10)),
            index: 0,
            details: history_details_for_media_item(&item_1),
            is_current_child: true,
            ..ersatzrs_domain::PlayoutHistory::default()
        },
    ];

    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: playlist
    playlist_group: Prime
    playlist: Block A
playout:
  - count: "1"
    content: playlist
"#,
    )
    .expect("valid yaml");
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        "history": result.added_history.iter().map(|history| serde_json::json!({
            "key": history.key,
            "child_key": history.child_key,
            "index": history.index,
            "details": history.details,
            "is_current_child": history.is_current_child,
        })).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_marathon_grouping_by_show_and_season_snapshot() {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.searches_by_query.insert(
        "sitcom".to_owned(),
        vec![
            episode_item(2, 10, 1001, 1, 2),
            episode_item(1, 10, 1001, 1, 1),
            episode_item(3, 20, 2001, 1, 1),
            episode_item(4, 10, 1002, 2, 1),
        ],
    );

    let definition = YamlPlayoutEngine::definition_from_str(
        r"
content:
  - key: by_show
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
  - key: by_season
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: season
    item_order: season_episode
playout:
  - all: by_show
  - all: by_season
",
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);

    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(2),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
        "history_keys": result.added_history.iter().map(|history| history.key.clone()).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_marathon_child_collection_keys_and_history_details_snapshot() {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.searches_by_query.insert(
        "sitcom".to_owned(),
        vec![
            episode_item(2, 10, 1001, 1, 2),
            episode_item(1, 10, 1001, 1, 1),
            episode_item(3, 20, 2001, 1, 1),
        ],
    );

    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: by_show
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
playout:
  - count: "1"
    content: by_show
"#,
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);

    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "history": result.added_history.iter().map(|history| serde_json::json!({
            "key": history.key,
            "child_key": history.child_key,
            "index": history.index,
            "details": history.details,
            "is_current_child": history.is_current_child,
        })).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_marathon_play_all_items_and_item_order_snapshot() {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.searches_by_query.insert(
        "sitcom".to_owned(),
        vec![
            episode_item(2, 10, 1001, 1, 2),
            episode_item(1, 10, 1001, 1, 1),
            episode_item(3, 20, 2001, 1, 1),
            episode_item(4, 10, 1002, 2, 1),
        ],
    );

    let definition = YamlPlayoutEngine::definition_from_str(
        r"
content:
  - key: by_show
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
  - key: by_show_all
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
    play_all_items: true
playout:
  - all: by_show
  - all: by_show_all
",
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);

    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(2),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_marathon_empty_source_uses_fallback_snapshot() {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.collections_by_name.insert(
        "Fallback".to_owned(),
        vec![SchedulingMediaItem::new(900, Duration::minutes(10))],
    );

    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: empty_marathon
    marathon: Missing Marathon
    searches:
      - missing
    group_by: show
    item_order: season_episode
  - key: fallback
    collection: Fallback
    order: chronological
playout:
  - duration: "00:05:00"
    content: empty_marathon
    fallback: fallback
"#,
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);

    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "items": result.added_items.iter().map(|item| serde_json::json!({
            "media": item.media_item_id,
            "start": item.start.to_rfc3339(),
            "finish": item.finish.to_rfc3339(),
            "filler": format!("{:?}", item.filler_kind),
        })).collect::<Vec<_>>(),
        "history": result.added_history.iter().map(|history| serde_json::json!({
            "key": history.key,
            "index": history.index,
            "details": history.details,
        })).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_reset_removes_future_and_current_history_snapshot() {
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);
    let history_key = history_key_for_content("main", PlaybackOrder::None);
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.media_by_key.insert(
        "main".to_owned(),
        vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
    );
    reference_data.existing_history = vec![
        ersatzrs_domain::PlayoutHistory {
            id: 40,
            key: history_key.clone(),
            when: start - Duration::hours(2),
            finish: Some(start - Duration::hours(2) + Duration::minutes(10)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
        ersatzrs_domain::PlayoutHistory {
            id: 41,
            key: history_key.clone(),
            when: start - Duration::minutes(5),
            finish: Some(start + Duration::minutes(5)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
        ersatzrs_domain::PlayoutHistory {
            id: 42,
            key: history_key,
            when: start + Duration::minutes(5),
            finish: Some(start + Duration::minutes(15)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
    ];
    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
playout:
  - count: "1"
    content: main
"#,
    )
    .expect("valid yaml");
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "history_to_remove": result.history_to_remove,
    }));
}

#[test]
fn yaml_cleanup_keeps_latest_completed_history_per_key_snapshot() {
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);
    let key_a = history_key_for_content("a", PlaybackOrder::None);
    let key_b = history_key_for_content("b", PlaybackOrder::None);
    let mut reference_data = SchedulingReferenceData {
        max_playout_offset: Duration::zero(),
        ..SchedulingReferenceData::default()
    };
    reference_data.media_by_key.insert(
        "a".to_owned(),
        vec![SchedulingMediaItem::new(1, Duration::minutes(10))],
    );
    reference_data.existing_history = vec![
        ersatzrs_domain::PlayoutHistory {
            id: 50,
            key: key_a.clone(),
            when: start - Duration::hours(3),
            finish: Some(start - Duration::hours(3) + Duration::minutes(10)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
        ersatzrs_domain::PlayoutHistory {
            id: 51,
            key: key_a,
            when: start - Duration::hours(2),
            finish: Some(start - Duration::hours(2) + Duration::minutes(10)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
        ersatzrs_domain::PlayoutHistory {
            id: 52,
            key: key_b.clone(),
            when: start - Duration::hours(4),
            finish: Some(start - Duration::hours(4) + Duration::minutes(10)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
        ersatzrs_domain::PlayoutHistory {
            id: 53,
            key: key_b,
            when: start - Duration::hours(1),
            finish: Some(start - Duration::hours(1) + Duration::minutes(10)),
            ..ersatzrs_domain::PlayoutHistory::default()
        },
    ];
    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
playout:
  - count: "1"
    content: a
"#,
    )
    .expect("valid yaml");
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Continue,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "history_to_remove": result.history_to_remove,
    }));
}

#[test]
fn yaml_repeat_instruction_detects_empty_loop() {
    let (definition, request) = request(
        r"
playout:
  - repeat: true
",
    );
    let result = YamlPlayoutEngine.schedule_definition(definition, request);
    assert!(result.is_err());
}

// ── count expression tests ────────────────────────────────────────────────

#[test]
fn count_expression_literal() {
    // Plain integer literals still work (must not break existing snapshots).
    assert_eq!(eval_count_expression("3", 10, 0), 3);
    assert_eq!(eval_count_expression("0", 10, 0), 0);
}

#[test]
fn count_expression_count_variable() {
    // "count" expands to the enumerator length.
    assert_eq!(eval_count_expression("count", 4, 0), 4);
    assert_eq!(eval_count_expression("count", 0, 0), 0);
}

#[test]
fn count_expression_arithmetic() {
    assert_eq!(eval_count_expression("count / 2", 8, 0), 4);
    assert_eq!(eval_count_expression("count + 1", 3, 0), 4);
    assert_eq!(eval_count_expression("count * 2", 3, 0), 6);
    assert_eq!(eval_count_expression("2 + 3", 0, 0), 5);
}

#[test]
fn count_expression_random_variable() {
    // "random" must be in [0, count). Same seed + same enumerator length ⇒
    // same result (determinism).
    let v1 = eval_count_expression("random", 10, 42);
    let v2 = eval_count_expression("random", 10, 42);
    assert_eq!(v1, v2);
    assert!(v1 < 10);
}

#[test]
fn count_expression_zero_for_invalid() {
    // Non-parseable input falls back to 0.
    assert_eq!(eval_count_expression("bogus_var", 5, 0), 0);
    assert_eq!(eval_count_expression("1bogus", 5, 0), 0);
    assert_eq!(eval_count_expression("(count + 1", 5, 0), 0);
    assert_eq!(eval_count_expression("", 5, 0), 0);
}

#[test]
fn count_expression_negative_results_clamp_to_zero() {
    assert_eq!(eval_count_expression("-1", 5, 0), 0);
    assert_eq!(eval_count_expression("count - 10", 5, 0), 0);
}

#[test]
fn count_expression_in_schedule_uses_enumerator_length() {
    // count: "count" schedules all items in the enumerator.
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - count: "count"
    content: main
"#
    ));
}

#[test]
fn count_expression_arithmetic_in_schedule_snapshot() {
    // count: "count / 2" with 2 items → schedules 1 item.
    assert_yaml_json_snapshot!(snapshot(
        r#"
playout:
  - count: "count / 2"
    content: main
"#
    ));
}

#[test]
fn count_expression_for_playlist_uses_unique_media_count_snapshot() {
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.playlist_items_by_group_and_name.insert(
        (None, "nightly".to_owned()),
        vec![
            SchedulingPlaylistItem {
                collection_key: SchedulingCollectionKey::fake_playlist_item(0),
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: Some(3),
                media_items: vec![
                    SchedulingMediaItem::new(1, Duration::minutes(10)),
                    SchedulingMediaItem::new(2, Duration::minutes(10)),
                ],
            },
            SchedulingPlaylistItem {
                collection_key: SchedulingCollectionKey::fake_playlist_item(1),
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: Some(3),
                media_items: vec![SchedulingMediaItem::new(3, Duration::minutes(10))],
            },
        ],
    );
    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: playlist
    playlist: nightly
playout:
  - count: "count"
    content: playlist
"#,
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .expect("valid time")
        .with_timezone(&Utc);
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
    }));
}

// ── import merge tests ────────────────────────────────────────────────────

#[test]
fn yaml_import_merge_adds_content_from_imported_definition() {
    let mut base = YamlPlayoutEngine::definition_from_str(
        r"
content:
  - key: main
    collection: Movies
",
    )
    .unwrap();
    let imported = YamlPlayoutEngine::definition_from_str(
        r"
content:
  - key: bumpers
    collection: Bumpers
  - key: main
    collection: Ignored
",
    )
    .unwrap();

    YamlPlayoutEngine::merge_import(&mut base, imported);

    // "bumpers" must be added; "main" must NOT be overridden.
    assert_eq!(base.content.len(), 2);
    let main = base.content.iter().find(|c| c.key == "main").unwrap();
    assert_eq!(main.collection.as_deref(), Some("Movies"));
    let bumpers = base.content.iter().find(|c| c.key == "bumpers").unwrap();
    assert_eq!(bumpers.collection.as_deref(), Some("Bumpers"));
}

#[test]
fn yaml_import_merge_adds_sequences_from_imported_definition() {
    let mut base = YamlPlayoutEngine::definition_from_str(
        r#"
sequence:
  - key: intro
    items:
      - count: "1"
        content: main
"#,
    )
    .unwrap();
    let imported = YamlPlayoutEngine::definition_from_str(
        r#"
sequence:
  - key: outro
    items:
      - count: "1"
        content: bumpers
  - key: intro
    items:
      - count: "99"
        content: ignored
"#,
    )
    .unwrap();

    YamlPlayoutEngine::merge_import(&mut base, imported);

    // "outro" is added; "intro" is kept from base.
    assert_eq!(base.sequence.len(), 2);
    let intro = base.sequence.iter().find(|s| s.key == "intro").unwrap();
    assert_eq!(
        intro.items[0].count.as_deref(),
        Some("1"),
        "base intro must not be overridden"
    );
}

#[test]
fn yaml_import_merge_case_insensitive_key_comparison() {
    let mut base = YamlPlayoutEngine::definition_from_str(
        r"
content:
  - key: Main
    collection: Movies
",
    )
    .unwrap();
    let imported = YamlPlayoutEngine::definition_from_str(
        r"
content:
  - key: main
    collection: ShouldBeIgnored
",
    )
    .unwrap();

    YamlPlayoutEngine::merge_import(&mut base, imported);

    assert_eq!(
        base.content.len(),
        1,
        "duplicate key (different case) must not be added"
    );
}

#[test]
fn yaml_import_merged_content_used_in_schedule_snapshot() {
    // Build a base definition manually and merge imported content into it,
    // then schedule to verify the merged content is used.
    let mut definition = YamlPlayoutEngine::definition_from_str(
        r#"
playout:
  - count: "2"
    content: imported
"#,
    )
    .unwrap();
    let imported_content = YamlPlayoutEngine::definition_from_str(
        r"
content:
  - key: imported
    collection: Movies
",
    )
    .unwrap();
    YamlPlayoutEngine::merge_import(&mut definition, imported_content);

    let mut reference_data = SchedulingReferenceData::default();
    reference_data.collections_by_name.insert(
        "Movies".to_owned(),
        vec![
            SchedulingMediaItem::new(11, Duration::minutes(10)),
            SchedulingMediaItem::new(12, Duration::minutes(10)),
        ],
    );
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
    }));
}

// ── shuffle_groups tests ──────────────────────────────────────────────────

#[test]
fn yaml_marathon_shuffle_groups_produces_deterministic_order_snapshot() {
    // With shuffle_groups: true the children (groups) are reordered by seed.
    // The test verifies the ordering is stable across runs and distinct
    // from the insertion order of the groups when seed is non-trivial.
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.searches_by_query.insert(
        "sitcom".to_owned(),
        vec![
            episode_item(1, 10, 1001, 1, 1),
            episode_item(2, 20, 2001, 1, 1),
            episode_item(3, 30, 3001, 1, 1),
        ],
    );

    let yaml = r"
content:
  - key: by_show_shuffled
    marathon: Sitcom Marathon
    searches:
      - sitcom
    group_by: show
    item_order: season_episode
    shuffle_groups: true
playout:
  - all: by_show_shuffled
";
    let definition = YamlPlayoutEngine::definition_from_str(yaml).expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
    }));
}

// ── playlist batch/count child semantics tests ────────────────────────────

#[test]
fn yaml_playlist_count_child_takes_n_items_before_switching_snapshot() {
    // A playlist child with count=2 takes 2 items before moving to the
    // next child, matching ErsatzTV's PlaylistEnumerator batch semantics.
    let child_0_key = SchedulingCollectionKey::fake_playlist_item(0);
    let child_1_key = SchedulingCollectionKey::fake_playlist_item(1);
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.playlist_items_by_group_and_name.insert(
        (None, "MyPlaylist".to_owned()),
        vec![
            SchedulingPlaylistItem {
                collection_key: child_0_key,
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: Some(2),
                media_items: vec![
                    SchedulingMediaItem::new(1, Duration::minutes(5)),
                    SchedulingMediaItem::new(2, Duration::minutes(5)),
                    SchedulingMediaItem::new(3, Duration::minutes(5)),
                ],
            },
            SchedulingPlaylistItem {
                collection_key: child_1_key,
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: Some(1),
                media_items: vec![SchedulingMediaItem::new(10, Duration::minutes(5))],
            },
        ],
    );

    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: pl
    playlist: MyPlaylist
playout:
  - count: "6"
    content: pl
"#,
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(2),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    // Expected interleaving: child0(1), child0(2), child1(10), child0(3),
    // child0(1), child1(10) — child0 wraps, count=2 each time.
    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
    }));
}

#[test]
fn yaml_playlist_play_all_child_exhausts_items_snapshot() {
    // A child with play_all=true exhausts its full enumerator before moving
    // to the next child (mirrors ErsatzTV play_all_items behavior).
    let child_0_key = SchedulingCollectionKey::fake_playlist_item(0);
    let child_1_key = SchedulingCollectionKey::fake_playlist_item(1);
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.playlist_items_by_group_and_name.insert(
        (None, "AllPlaylist".to_owned()),
        vec![
            SchedulingPlaylistItem {
                collection_key: child_0_key,
                playback_order: PlaybackOrder::Chronological,
                play_all: true,
                count: None,
                media_items: vec![
                    SchedulingMediaItem::new(1, Duration::minutes(5)),
                    SchedulingMediaItem::new(2, Duration::minutes(5)),
                ],
            },
            SchedulingPlaylistItem {
                collection_key: child_1_key,
                playback_order: PlaybackOrder::Chronological,
                play_all: false,
                count: Some(1),
                media_items: vec![SchedulingMediaItem::new(10, Duration::minutes(5))],
            },
        ],
    );

    let definition = YamlPlayoutEngine::definition_from_str(
        r#"
content:
  - key: pl
    playlist: AllPlaylist
playout:
  - count: "4"
    content: pl
"#,
    )
    .expect("valid yaml");
    let start = DateTime::parse_from_rfc3339("2026-04-15T10:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let result = YamlPlayoutEngine
        .schedule_definition(
            definition,
            SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Reset,
                reference_data,
            },
        )
        .expect("schedule builds");

    // Expected: child0(1), child0(2) [play_all exhausts 2], child1(10), child0(1)
    assert_yaml_json_snapshot!(serde_json::json!({
        "media": result.added_items.iter().map(|item| item.media_item_id).collect::<Vec<_>>(),
    }));
}
