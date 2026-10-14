//! UCU-10 row230: nested filler repeats, titles and shared enumerator state.
use super::*;

fn fixture(roll: &str) -> (YamlPlayoutDefinition, SchedulingRequest) {
    let yaml = format!(
        "content:\n  - key: main\n    order: chronological\n  - key: ident\n    order: chronological\n  - key: ad\n    order: chronological\nsequence:\n  - key: up-next\n    items:\n      - count: '1'\n        content: ident\n  - key: break\n    items:\n      - count: '1'\n        content: ad\n      - sequence: up-next\n        repeat: 2\n        custom_title: Up Next\n      - count: '1'\n        content: ad\nplayout:\n  - {roll}: true\n    sequence: break\n  - count: '1'\n    content: main\n  - repeat: true\n"
    );
    let (definition, mut request) = request(&yaml);
    request.playout.schedule_kind = ersatzrs_domain::PlayoutScheduleKind::Sequential;
    request.finish = request.start + Duration::minutes(25);
    request.reference_data.media_by_key.insert(
        "main".into(),
        vec![
            SchedulingMediaItem::new(1, Duration::minutes(10)),
            SchedulingMediaItem::new(5, Duration::minutes(10)),
        ],
    );
    request.reference_data.media_by_key.insert(
        "ident".into(),
        [2, 4, 6]
            .map(|id| SchedulingMediaItem::new(id, Duration::seconds(10)))
            .to_vec(),
    );
    request.reference_data.media_by_key.insert(
        "ad".into(),
        vec![SchedulingMediaItem::new(3, Duration::seconds(10))],
    );
    (definition, request)
}

#[test]
fn yaml_nested_filler_repeats_titles_and_advances_shared_content() {
    for (roll, kind) in [
        ("pre_roll", FillerKind::PreRoll),
        ("mid_roll", FillerKind::MidRoll),
        ("post_roll", FillerKind::PostRoll),
    ] {
        let (definition, request) = fixture(roll);
        let result = YamlPlayoutEngine
            .schedule_definition(definition, request)
            .unwrap();
        let expected = if roll == "pre_roll" {
            vec![3, 2, 4, 3, 1, 3, 6, 2, 3, 5, 3, 4, 6, 3, 1]
        } else {
            vec![1, 3, 2, 4, 3, 5, 3, 6, 2, 3, 1, 3, 4, 6, 3]
        };
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>(),
            expected,
            "{roll}: repeat count and advancing filler selection"
        );
        for item in &result.added_items {
            let ident = [2, 4, 6].contains(&item.media_item_id);
            assert_eq!(item.custom_title.as_deref(), ident.then_some("Up Next"));
            assert_eq!(
                item.filler_kind,
                if [1, 5].contains(&item.media_item_id) {
                    FillerKind::None
                } else {
                    kind
                }
            );
        }
    }
}

#[test]
fn yaml_nested_filler_restart_continue_and_refresh_match_uninterrupted() {
    for roll in ["pre_roll", "mid_roll", "post_roll"] {
        let (definition, request) = fixture(roll);
        let oracle = YamlPlayoutEngine
            .schedule_definition(definition.clone(), request.clone())
            .unwrap();
        let mut short = request.clone();
        short.finish = short.start + Duration::minutes(1);
        let first = YamlPlayoutEngine
            .schedule_definition(definition.clone(), short)
            .unwrap();
        assert_eq!(
            first.added_items.len(),
            5,
            "{roll}: complete programme and rolls"
        );
        let mut resumed = request.clone();
        resumed.mode = PlayoutBuildMode::Continue;
        resumed.playout.anchor =
            serde_json::from_str(&serde_json::to_string(&first.anchor).unwrap()).unwrap();
        resumed.start = resumed.playout.anchor.as_ref().unwrap().next_start;
        resumed.reference_data.existing_history =
            serde_json::from_str(&serde_json::to_string(&first.added_history).unwrap()).unwrap();
        let continued = YamlPlayoutEngine
            .schedule_definition(definition.clone(), resumed)
            .unwrap();
        assert!(!continued.clear_items);
        assert_eq!(
            first
                .added_items
                .into_iter()
                .chain(continued.added_items)
                .collect::<Vec<_>>(),
            oracle.added_items,
            "{roll}: restart and Continue preserve future selection/timestamps/settings"
        );
        let mut refresh = request;
        refresh.mode = PlayoutBuildMode::Refresh;
        refresh.start = oracle.added_items[0].start + Duration::seconds(1);
        refresh.reference_data.preservation = crate::scheduling::PlayoutPreservation::KeepCurrent;
        refresh.reference_data.existing_items = oracle.added_items.clone();
        refresh.reference_data.existing_history = oracle.added_history.clone();
        refresh.reference_data.existing_yaml_checkpoints =
            serde_json::from_str(&serde_json::to_string(&oracle.yaml_checkpoints).unwrap())
                .unwrap();
        let result = YamlPlayoutEngine
            .schedule_definition(definition, refresh)
            .unwrap();
        assert_eq!(result.retain_through, Some(oracle.added_items[4].finish));
        assert_eq!(
            result.added_items,
            oracle.added_items[5..],
            "{roll}: Refresh keeps the programme unit"
        );
    }
}

#[test]
fn yaml_nested_filler_rejects_cycles_before_scheduling() {
    for cycle in ["break", "up-next"] {
        let (mut definition, request) = fixture("pre_roll");
        definition.sequence[0].items.push(YamlInstruction {
            sequence: Some(cycle.into()),
            ..Default::default()
        });
        assert!(matches!(
            YamlPlayoutEngine.schedule_definition(definition, request),
            Err(SchedulingError::SequenceCycle)
        ));
    }
}
