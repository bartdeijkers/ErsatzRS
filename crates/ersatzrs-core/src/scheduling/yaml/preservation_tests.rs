use super::*;
use crate::scheduling::PlayoutPreservation;
use ersatzrs_domain::PlayoutScheduleKind;

const SERIES: &str = "content:\n  - key: main\n    search: all\n    order: chronological\nplayout:\n  - count: 1\n    content: main\n  - repeat: true\n";

#[test]
fn yaml_preservation_retains_prefetched_preroll_main_and_postroll() {
    let yaml = "sequence:\n  - key: roll\n    items:\n      - count: 1\n        content: bumpers\nplayout:\n  - pre_roll: true\n    sequence: roll\n  - post_roll: true\n    sequence: roll\n  - count: 3\n    content: main\n  - repeat: true\n";
    let (definition, mut original) = request(yaml);
    original.playout.schedule_kind = PlayoutScheduleKind::Sequential;
    let full = YamlPlayoutEngine
        .schedule_definition(definition.clone(), original.clone())
        .unwrap();
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let mut rebuild = original.clone();
        rebuild.start += Duration::seconds(1);
        rebuild.mode = mode;
        rebuild.playout.anchor = full.anchor.clone();
        rebuild.reference_data.preservation = PlayoutPreservation::KeepCurrent;
        rebuild.reference_data.existing_items = full.added_items.clone();
        rebuild.reference_data.existing_history = full.added_history.clone();
        rebuild.reference_data.existing_yaml_checkpoints = full.yaml_checkpoints.clone();
        assert_eq!(full.added_items[3].filler_kind, FillerKind::PreRoll);
        rebuild.reference_data.preservation_not_before =
            Some(full.added_items[3].start + Duration::seconds(1));
        let result = YamlPlayoutEngine
            .schedule_definition(definition.clone(), rebuild)
            .unwrap();
        assert_eq!(result.retain_through, Some(full.added_items[5].finish));
        assert_eq!(result.added_items[0].start, full.added_items[5].finish);
        if mode == PlayoutBuildMode::Refresh {
            assert_eq!(result.added_items, full.added_items[6..]);
        }
    }
}

#[test]
fn yaml_refresh_resumes_inside_count_all_and_timed_instructions() {
    for instruction in [
        "count: 3",
        "all: main",
        "duration: '00:40:00'",
        "pad_to_next: 60",
    ] {
        let yaml = format!("content:\n  - key: main\n    search: all\n    order: chronological\nplayout:\n  - watermark: true\n    name: bug\n  - graphics_on: lower-third\n  - {instruction}\n    content: main\n  - count: 1\n    content: bumpers\n  - repeat: true\n");
        let definition = YamlPlayoutEngine::definition_from_str(&yaml).unwrap();
        let (_, request, _) = initial();
        let uninterrupted = YamlPlayoutEngine
            .schedule_definition(definition.clone(), request.clone())
            .unwrap();
        for active in 0..3 {
            let current = &uninterrupted.added_items[active];
            let mut refresh = request.clone();
            refresh.mode = PlayoutBuildMode::Refresh;
            refresh.start = current.start + Duration::seconds(1);
            refresh.reference_data.preservation = PlayoutPreservation::KeepCurrent;
            refresh.reference_data.existing_items = uninterrupted.added_items.clone();
            refresh.reference_data.existing_history = uninterrupted.added_history.clone();
            refresh.reference_data.existing_yaml_checkpoints = serde_json::from_str(
                &serde_json::to_string(&uninterrupted.yaml_checkpoints).unwrap(),
            )
            .unwrap();
            let result = YamlPlayoutEngine
                .schedule_definition(definition.clone(), refresh)
                .unwrap();
            assert_eq!(
                result.retain_through,
                Some(current.finish),
                "{instruction}, item {active}"
            );
            assert_eq!(
                result.added_items,
                uninterrupted.added_items[active + 1..],
                "{instruction}, item {active}"
            );
        }
    }
}

#[test]
fn yaml_refresh_preserves_the_programme_with_its_rolls() {
    let yaml = "sequence:\n  - key: roll\n    items:\n      - count: 1\n        content: bumpers\nplayout:\n  - pre_roll: true\n    sequence: roll\n  - post_roll: true\n    sequence: roll\n  - count: 3\n    content: main\n  - repeat: true\n";
    for grouped in [false, true] {
        let (mut definition, mut request) = request(yaml);
        request.playout.schedule_kind = PlayoutScheduleKind::Sequential;
        if grouped {
            definition.playout.insert(
                0,
                YamlInstruction {
                    epg_group: Some(true),
                    ..YamlInstruction::default()
                },
            );
            definition.playout.insert(
                0,
                YamlInstruction {
                    epg_group: Some(false),
                    ..YamlInstruction::default()
                },
            );
        }
        let retained_count = if grouped { 9 } else { 3 };
        let original = YamlPlayoutEngine
            .schedule_definition(definition.clone(), request.clone())
            .unwrap();
        assert_eq!(original.added_items[0].filler_kind, FillerKind::PreRoll);
        assert_eq!(original.added_items[1].filler_kind, FillerKind::None);
        assert_eq!(original.added_items[2].filler_kind, FillerKind::PostRoll);
        for active in 0..3 {
            request.start = original.added_items[active].start + Duration::seconds(1);
            request.mode = PlayoutBuildMode::Refresh;
            request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
            request.reference_data.existing_items = original.added_items.clone();
            request.reference_data.existing_history = original.added_history.clone();
            request.reference_data.existing_yaml_checkpoints = original.yaml_checkpoints.clone();
            let refreshed = YamlPlayoutEngine
                .schedule_definition(definition.clone(), request.clone())
                .unwrap();
            assert_eq!(
                refreshed.retain_through,
                Some(original.added_items[retained_count - 1].finish)
            );
            assert_eq!(
                refreshed.added_items,
                original.added_items[retained_count..]
            );
        }
    }
}

#[test]
fn yaml_refresh_preserves_marathon_rotation_and_reconciles_new_members() {
    let (definition, mut request) = marathon_history_fixture();
    request.playout.schedule_kind = PlayoutScheduleKind::Sequential;
    let original = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    let current = &original.added_items[2];
    request.mode = PlayoutBuildMode::Refresh;
    request.start = current.start + Duration::seconds(1);
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    request.reference_data.existing_items = original.added_items.clone();
    request.reference_data.existing_history = original.added_history;
    request.reference_data.existing_yaml_checkpoints = original.yaml_checkpoints;
    let same = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    assert_eq!(same.added_items, original.added_items[3..]);
    request
        .reference_data
        .searches_by_query
        .get_mut("episodes")
        .unwrap()
        .push(episode_item(7, 40, 4001, 1, 1));
    let changed = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert_eq!(
        changed.added_items[0].media_item_id,
        original.added_items[3].media_item_id
    );
    assert!(changed
        .added_items
        .windows(2)
        .all(|pair| pair[0].finish == pair[1].start));
}

#[test]
fn yaml_checkpoint_rejects_corrupt_instruction_cursor() {
    let (_, _, original) = initial();
    let mut checkpoint = original.yaml_checkpoints[1].clone();
    let mut value: serde_json::Value = serde_json::from_str(&checkpoint.context).unwrap();
    let mut context: serde_json::Value =
        serde_json::from_str(value["context"].as_str().unwrap()).unwrap();
    context["instruction_index"] = serde_json::json!(999_999);
    value["context"] = serde_json::json!(serde_json::to_string(&context).unwrap());
    checkpoint.context = serde_json::to_string(&value).unwrap();
    assert!(validate_yaml_checkpoint(&checkpoint, checkpoint.playout_id).is_err());
}

#[test]
fn yaml_refresh_can_explicitly_replace_current_programme_without_resetting_rotation() {
    let (definition, request, original) = initial();
    for index in 0..2 {
        let current = &original.added_items[index];
        let mut request = request.clone();
        request.start = current.start + Duration::minutes(1);
        let requested_start = request.start;
        request.mode = PlayoutBuildMode::Refresh;
        request.reference_data.preservation = PlayoutPreservation::ReplaceCurrent;
        request.reference_data.existing_items = original.added_items.clone();
        request.reference_data.existing_yaml_checkpoints = original.yaml_checkpoints.clone();
        let refreshed = YamlPlayoutEngine
            .schedule_definition(definition.clone(), request)
            .unwrap();
        assert!(refreshed.clear_items);
        assert_eq!(refreshed.retain_through, None);
        assert_eq!(refreshed.added_items[0].start, requested_start);
        assert_eq!(
            refreshed.added_items[0].media_item_id,
            current.media_item_id
        );
    }
}

#[test]
fn yaml_refresh_preserves_future_sequence_shuffles() {
    let yaml = "sequence:\n  - key: blocks\n    items:\n      - count: 1\n        content: a\n      - count: 1\n        content: b\n      - count: 1\n        content: c\nplayout:\n  - sequence: blocks\n  - shuffle_sequence: blocks\n  - repeat: true\n";
    let (definition, mut request) = request(yaml);
    request.playout.schedule_kind = PlayoutScheduleKind::Sequential;
    request.playout.seed = 12345;
    for (key, id) in [("a", 1), ("b", 2), ("c", 3)] {
        request.reference_data.media_by_key.insert(
            key.into(),
            vec![SchedulingMediaItem::new(id, Duration::minutes(5))],
        );
    }
    let original = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    request.mode = PlayoutBuildMode::Refresh;
    request.start = original.added_items[4].start + Duration::seconds(1);
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    request.reference_data.existing_items = original.added_items.clone();
    request.reference_data.existing_yaml_checkpoints = original.yaml_checkpoints;
    let refreshed = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert_eq!(refreshed.added_items, original.added_items[5..]);
}

#[test]
fn yaml_refresh_finishes_pending_instruction_before_midnight_schedule_switch() {
    use chrono::{Local, TimeZone};
    let yaml = "playout:\n  - count: 3\n    content: main\n  - repeat: true\nschedules:\n  - name: Christmas\n    start_date: 12-25\n    end_date: 12-25\n    playout:\n      - count: 1\n        content: bumpers\n      - repeat: true\n";
    let (definition, mut request) = request(yaml);
    request.playout.schedule_kind = PlayoutScheduleKind::Sequential;
    request.start = Local
        .with_ymd_and_hms(2026, 12, 24, 23, 55, 0)
        .single()
        .unwrap()
        .with_timezone(&Utc);
    request.finish = request.start + Duration::hours(1);
    let full = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    request.mode = PlayoutBuildMode::Refresh;
    request.start += Duration::minutes(1);
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    request.reference_data.existing_items = full.added_items.clone();
    request.reference_data.existing_yaml_checkpoints = full.yaml_checkpoints;
    let refreshed = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert_eq!(refreshed.added_items, full.added_items[1..]);
}

#[test]
fn yaml_refresh_in_gap_resumes_wait_without_replaying_consumed_media() {
    use chrono::{Local, TimeZone};
    let (definition, mut request) = request("playout:\n  - count: 1\n    content: main\n  - wait_until: '10:30'\n  - count: 3\n    content: main\n");
    request.playout.schedule_kind = PlayoutScheduleKind::Sequential;
    request.start = Local
        .with_ymd_and_hms(2026, 4, 15, 10, 0, 0)
        .single()
        .unwrap()
        .with_timezone(&Utc);
    request.finish = request.start + Duration::hours(1);
    let original = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    request.start += Duration::minutes(15);
    request.mode = PlayoutBuildMode::Refresh;
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    request.reference_data.existing_items = original.added_items.clone();
    request.reference_data.existing_yaml_checkpoints = original.yaml_checkpoints;
    let refreshed = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert_eq!(refreshed.added_items, original.added_items[1..]);
}

#[test]
fn yaml_refresh_activates_empty_source_without_consuming_unpublished_count_iterations() {
    let (definition, mut request) = request("playout:\n  - count: 1\n    content: main\n  - count: 3\n    content: late\n  - count: 1\n    content: bumpers\n  - repeat: true\n");
    request.playout.schedule_kind = PlayoutScheduleKind::Sequential;
    let original = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    request.start += Duration::seconds(1);
    request.mode = PlayoutBuildMode::Refresh;
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    request.reference_data.existing_items = original.added_items;
    request.reference_data.existing_yaml_checkpoints = original.yaml_checkpoints;
    request.reference_data.media_by_key.insert(
        "late".into(),
        vec![SchedulingMediaItem::new(11, Duration::minutes(1))],
    );
    let refreshed = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert_eq!(
        refreshed.added_items[..4]
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>(),
        [11, 11, 11, 9]
    );
}

fn initial() -> (YamlPlayoutDefinition, SchedulingRequest, SchedulingResult) {
    let (definition, mut request) = request(SERIES);
    request.playout.schedule_kind = PlayoutScheduleKind::Sequential;
    request.reference_data.searches_by_query.insert(
        "all".into(),
        request.reference_data.media_by_key["main"].clone(),
    );
    request.start = DateTime::parse_from_rfc3339("2026-04-15T23:53:00.1234567Z")
        .unwrap()
        .with_timezone(&Utc);
    request.finish = request.start + Duration::hours(2);
    let mut result = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    for (index, item) in result.added_items.iter_mut().enumerate() {
        item.id = i64::try_from(index + 1).unwrap();
    }
    for (index, history) in result.added_history.iter_mut().enumerate() {
        history.id = i64::try_from(index + 1).unwrap();
    }
    (definition, request, result)
}

#[test]
fn yaml_explicit_reset_keeps_programme_then_starts_fresh_rotation() {
    let (definition, mut request, original) = initial();
    // Reset while the first episode is playing across midnight; fresh rotation
    // must restart it after the retained row, not advance to episode two.
    let current = &original.added_items[0];
    request.start = current.start + Duration::minutes(3);
    request.reference_data.existing_items = original.added_items.clone();
    request.reference_data.existing_history = original.added_history.clone();
    request.playout.anchor = original.anchor.clone();
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    let reset = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    assert_eq!(reset.retain_through, Some(current.finish));
    assert_eq!(reset.added_items[0].start, current.finish);
    assert_eq!(reset.added_items[0].media_item_id, 1);
    assert!(reset.added_items[0].guide_group > current.guide_group);
    for history in &original.added_history {
        if history.when >= current.start && history.finish <= Some(current.finish) {
            assert!(!reset.history_to_remove.contains(&history.id));
        } else if history.when >= current.finish {
            assert!(reset.history_to_remove.contains(&history.id));
        }
    }
    // A fresh build at the retained seam is the oracle for new rotation.
    let mut fresh = request.clone();
    fresh.start = current.finish;
    fresh.playout.anchor = None;
    fresh.reference_data.existing_items.clear();
    fresh.reference_data.existing_history.clear();
    fresh.reference_data.preservation = PlayoutPreservation::ReplaceCurrent;
    let oracle = YamlPlayoutEngine
        .schedule_definition(definition, fresh)
        .unwrap();
    let timeline = |result: &SchedulingResult| {
        result
            .added_items
            .iter()
            .map(|item| (item.media_item_id, item.start, item.finish))
            .collect::<Vec<_>>()
    };
    assert_eq!(timeline(&reset), timeline(&oracle));
}

#[test]
fn yaml_reset_cutoff_survives_serialization_and_ignores_archived_source_history() {
    let (definition, mut request, original) = initial();
    request.start = original.added_items[0].start + Duration::minutes(1);
    request.reference_data.existing_items = original.added_items.clone();
    request.reference_data.existing_history = original.added_history.clone();
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    // Do not use the old source during the first post-Reset build.
    let idle_definition = YamlPlayoutEngine::definition_from_str(
        "playout:\n  - count: 1\n    content: bumpers\n  - repeat: true\n",
    )
    .unwrap();
    request.finish = original.added_items[0].finish + Duration::minutes(2);
    let reset = YamlPlayoutEngine
        .schedule_definition(idle_definition, request.clone())
        .unwrap();
    request.mode = PlayoutBuildMode::Continue;
    request.playout.anchor = Some(
        serde_json::from_str(&serde_json::to_string(&reset.anchor.unwrap()).unwrap()).unwrap(),
    );
    request.start = request.playout.anchor.as_ref().unwrap().next_start;
    request.finish = request.start + Duration::minutes(1);
    request
        .reference_data
        .existing_history
        .retain(|history| !reset.history_to_remove.contains(&history.id));
    request
        .reference_data
        .existing_history
        .extend(reset.added_history);
    request
        .reference_data
        .existing_items
        .retain(|item| item.finish <= reset.retain_through.unwrap());
    request
        .reference_data
        .existing_items
        .extend(reset.added_items);
    let continued = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert_eq!(
        continued.added_items[0].media_item_id, 1,
        "a source first used after Reset must start fresh despite archived history"
    );
}

#[test]
fn yaml_preserving_reset_retains_locked_guide_group() {
    let (mut definition, mut request, _) = initial();
    request.finish = request.start + Duration::hours(6);
    definition.playout.insert(
        0,
        YamlInstruction {
            epg_group: Some(true),
            advance: Some(false),
            ..YamlInstruction::default()
        },
    );
    let original = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    let boundary = original.added_items.last().unwrap().finish;
    let old_group = original.added_items.last().unwrap().guide_group;
    request.start += Duration::minutes(1);
    request.finish = boundary + Duration::minutes(20);
    request.reference_data.existing_items = original.added_items;
    let mut retained_history = original.added_history;
    for (index, history) in retained_history.iter_mut().enumerate() {
        history.id = i64::try_from(index + 1).unwrap();
    }
    request.reference_data.existing_history = retained_history.clone();
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    let reset = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert_eq!(reset.retain_through, Some(boundary));
    assert_eq!(reset.added_items[0].start, boundary);
    assert_ne!(reset.added_items[0].guide_group, old_group);
    assert!(retained_history
        .iter()
        .all(|history| !reset.history_to_remove.contains(&history.id)));
}

#[test]
fn yaml_reset_cutoff_does_not_revive_archived_marathon_after_restart() {
    let (definition, mut request) = marathon_history_fixture();
    request.playout.schedule_kind = PlayoutScheduleKind::Sequential;
    let mut original = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    for (index, history) in original.added_history.iter_mut().enumerate() {
        history.id = i64::try_from(index + 1).unwrap();
    }
    let boundary = original.added_items[0].finish;
    request.mode = PlayoutBuildMode::Reset;
    request.start += Duration::minutes(1);
    request.finish = boundary + Duration::minutes(2);
    request.reference_data.existing_items = original.added_items;
    request.reference_data.existing_history = original.added_history;
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    request.reference_data.media_by_key.insert(
        "bumper".into(),
        vec![SchedulingMediaItem::new(999, Duration::minutes(1))],
    );
    let bumper = YamlPlayoutEngine::definition_from_str(
        "playout:\n  - count: 1\n    content: bumper\n  - repeat: true\n",
    )
    .unwrap();
    let reset = YamlPlayoutEngine
        .schedule_definition(bumper, request.clone())
        .unwrap();
    request.playout.anchor =
        serde_json::from_str(&serde_json::to_string(&reset.anchor).unwrap()).unwrap();
    request.mode = PlayoutBuildMode::Continue;
    request.start = request.playout.anchor.as_ref().unwrap().next_start;
    request.finish = request.start + Duration::hours(1);
    request
        .reference_data
        .existing_history
        .retain(|history| !reset.history_to_remove.contains(&history.id));
    request
        .reference_data
        .existing_items
        .retain(|item| item.finish <= boundary);
    request
        .reference_data
        .existing_items
        .extend(reset.added_items);
    request
        .reference_data
        .existing_history
        .extend(reset.added_history);
    let continued = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    // A fresh post-Reset source must behave exactly like one with no archive.
    request.reference_data.existing_history.clear();
    request.reference_data.existing_items.clear();
    let fresh = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert_eq!(continued.added_items, fresh.added_items);
    assert_eq!(continued.added_history, fresh.added_history);
}

#[test]
fn yaml_preserving_reset_rejects_rewind_into_retained_programme() {
    let (mut definition, mut request, original) = initial();
    definition.reset.push(YamlInstruction {
        rewind: Some("00:05:00".into()),
        ..YamlInstruction::default()
    });
    request.start += Duration::minutes(1);
    request.reference_data.existing_items = original.added_items;
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    let error = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap_err();
    assert!(matches!(error, SchedulingError::InvalidRebuild(_)));
    assert!(error
        .to_string()
        .contains("rewound into the retained programme"));
}
