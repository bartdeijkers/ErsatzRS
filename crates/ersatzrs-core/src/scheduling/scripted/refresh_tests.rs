use super::continuation_tests::request;
use super::*;
use crate::scheduling::SchedulingMediaItem;

fn refresh_request(
    original: &SchedulingRequest,
    published: &SchedulingResult,
) -> SchedulingRequest {
    let mut refreshed = original.clone();
    refreshed.mode = PlayoutBuildMode::Refresh;
    refreshed.start += Duration::minutes(4);
    refreshed.playout.anchor = published.anchor.clone();
    refreshed.reference_data.existing_items = published.added_items.clone();
    refreshed.reference_data.existing_history = published.added_history.clone();
    refreshed.reference_data.existing_scripted_checkpoints = published.scripted_checkpoints.clone();
    refreshed
}

#[test]
fn scripted_refresh_preserves_programme_groups_and_source_rotation() {
    for order in ["chronological", "shuffle", "random"] {
        for group in [false, true] {
            let body = if group {
                "lock_guide_group() add_count('a', 2) unlock_guide_group()"
            } else {
                "add_count('a', 1)"
            };
            let script = format!(
                "add_collection('a', 'series', '{order}') while not is_done() do {body} end"
            );
            let mut original = request();
            original.finish = original.start + Duration::hours(2);
            let full = ScriptedSchedulingEngine
                .schedule_script(&script, original.clone())
                .unwrap();
            let refreshed = ScriptedSchedulingEngine
                .schedule_script(&script, refresh_request(&original, &full))
                .unwrap();
            let boundary = original.start + Duration::minutes(6);
            assert_eq!(refreshed.retain_through, Some(boundary));
            assert_eq!(refreshed.added_items.first().unwrap().start, boundary);
            let combined = full
                .added_items
                .iter()
                .filter(|item| item.finish <= boundary)
                .chain(&refreshed.added_items)
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(combined, full.added_items, "order={order}, grouped={group}");
            let history = full
                .added_history
                .iter()
                .filter(|history| history.when < boundary)
                .chain(&refreshed.added_history)
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(history, full.added_history);
        }
    }
}

#[test]
fn scripted_refresh_reconciles_added_removed_shuffle_members_without_consumed_replay() {
    let script =
        "add_collection('a', 'series', 'shuffle') while not is_done() do add_count('a', 1) end";
    let mut original = request();
    original.finish = original.start + Duration::minutes(27);
    let first = ScriptedSchedulingEngine
        .schedule_script(script, original.clone())
        .unwrap();
    let mut refresh = refresh_request(&original, &first);
    let members = refresh
        .reference_data
        .collections_by_name
        .get_mut("series")
        .unwrap();
    members.retain(|item| item.id != 3);
    members.push(SchedulingMediaItem::new(8, Duration::minutes(3)));
    let result = ScriptedSchedulingEngine
        .schedule_script(script, refresh)
        .unwrap();
    assert_eq!(
        result
            .added_items
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>(),
        [4, 5, 6, 7, 8, 1, 2]
    );
}

#[test]
fn scripted_refresh_refuses_missing_malformed_or_incompatible_boundary_state() {
    let script = "add_collection('a', 'series') while not is_done() do add_count('a', 1) end";
    let mut original = request();
    original.finish = original.start + Duration::hours(1);
    let first = ScriptedSchedulingEngine
        .schedule_script(script, original.clone())
        .unwrap();
    for fault in 0..5 {
        let mut refresh = refresh_request(&original, &first);
        let checkpoints = &mut refresh.reference_data.existing_scripted_checkpoints;
        match fault {
            0 => checkpoints.clear(),
            1 => checkpoints[1].context = "{broken".into(),
            2 => {
                checkpoints[1].context = checkpoints[1]
                    .context
                    .replace("\"version\":1", "\"version\":999");
            }
            3 => checkpoints[1].playout_id += 1,
            _ => {
                let mut state: serde_json::Value =
                    serde_json::from_str(&checkpoints[1].context).unwrap();
                state["sources"]["a"]["Media"]["state"]["CurrentIndex"] = 999.into();
                checkpoints[1].context = state.to_string();
            }
        }
        let error = ScriptedSchedulingEngine
            .schedule_script(script, refresh)
            .unwrap_err();
        assert!(error.to_string().contains("checkpoint"), "{fault}: {error}");
    }
    assert!(ScriptedSchedulingEngine
        .schedule_script("", refresh_request(&original, &first))
        .is_err());
    let error = ScriptedSchedulingEngine
        .schedule_script(
            &script.replace("'series')", "'series', 'shuffle')"),
            refresh_request(&original, &first),
        )
        .unwrap_err();
    assert!(error.to_string().contains("playback order"));
}

#[test]
fn scripted_refresh_restores_smart_shuffle_at_fractional_day_boundary() {
    let script = r#"
set_smart_shuffle_policy("a", [[{"version":1,"enabled":true,"preset":"custom","scope":"playout","mode":"soft","after_items":2,"include_filler":false,"rules":[{"id":"artist","kind":"artist"}]}]])
add_collection('a', 'series', 'shuffle')
while not is_done() do add_count('a', 1) end
"#;
    let mut original = request();
    original.finish = original.start + Duration::hours(1);
    for id in 1..=7 {
        original
            .reference_data
            .smart_shuffle_tags_by_media_id
            .insert(
                id,
                std::collections::BTreeSet::from([format!("artist:{}", id % 3)]),
            );
    }
    let full = ScriptedSchedulingEngine
        .schedule_script(script, original.clone())
        .unwrap();
    let refreshed = ScriptedSchedulingEngine
        .schedule_script(script, refresh_request(&original, &full))
        .unwrap();
    let boundary = refreshed.retain_through.unwrap();
    assert_eq!(
        full.added_items
            .iter()
            .filter(|item| item.start >= boundary)
            .collect::<Vec<_>>(),
        refreshed.added_items.iter().collect::<Vec<_>>()
    );
    assert_eq!(full.anchor, refreshed.anchor);
}
