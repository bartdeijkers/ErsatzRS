use super::continuation_tests::request;
use super::*;
use crate::scheduling::SchedulingMediaItem;

#[test]
fn scripted_preservation_retains_the_complete_prefetched_guide_group() {
    let script = "add_collection('a', 'series', 'shuffle') while not is_done() do lock_guide_group() add_count('a', 2) unlock_guide_group() end";
    let mut original = request();
    original.playout.schedule_kind = ersatzrs_domain::PlayoutScheduleKind::Scripted;
    original.finish = original.start + Duration::hours(1);
    let full = ScriptedSchedulingEngine
        .schedule_script(script, original.clone())
        .unwrap();
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let mut rebuild = refresh_request(&original, &full);
        rebuild.start = original.start + Duration::seconds(1);
        rebuild.mode = mode;
        rebuild.reference_data.preservation = crate::scheduling::PlayoutPreservation::KeepCurrent;
        rebuild.reference_data.preservation_not_before =
            Some(full.added_items[2].start + Duration::seconds(1));
        let seam = full.added_items[3].finish;
        let result = ScriptedSchedulingEngine
            .schedule_script(script, rebuild)
            .unwrap();
        assert_eq!(result.retain_through, Some(seam));
        assert_eq!(result.added_items[0].start, seam);
        if mode == PlayoutBuildMode::Refresh {
            assert_eq!(result.added_items, full.added_items[4..]);
        }
    }
}

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
fn scripted_preserved_reset_starts_new_rotation_and_survives_later_refresh() {
    use crate::scheduling::PlayoutPreservation;
    for order in ["chronological", "shuffle", "random"] {
        let script = format!(
            "add_collection('a', 'series', '{order}') while not is_done() do lock_guide_group() add_count('a', 2) unlock_guide_group() end"
        );
        let mut original = request();
        original.playout.schedule_kind = ersatzrs_domain::PlayoutScheduleKind::Scripted;
        original.finish = original.start + Duration::hours(1);
        let published = ScriptedSchedulingEngine
            .schedule_script(&script, original.clone())
            .unwrap();
        let mut reset = refresh_request(&original, &published);
        reset.mode = PlayoutBuildMode::Reset;
        reset.playout.seed += 100;
        reset.reference_data.preservation = PlayoutPreservation::KeepCurrent;
        let boundary = published.added_items[1].finish;
        let result = ScriptedSchedulingEngine
            .schedule_script(&script, reset.clone())
            .unwrap();
        assert_eq!(result.retain_through, Some(boundary));
        assert_eq!(result.added_items[0].start, boundary);
        assert_ne!(
            result.added_items[0].guide_group,
            published.added_items[1].guide_group
        );
        let mut fresh = original.clone();
        fresh.start = boundary;
        fresh.playout.seed = reset.playout.seed;
        let oracle = ScriptedSchedulingEngine
            .schedule_script(&script, fresh)
            .unwrap();
        let identities = |items: &[ersatzrs_domain::PlayoutItem]| {
            items
                .iter()
                .map(|item| (item.media_item_id, item.start, item.finish))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            identities(&result.added_items),
            identities(&oracle.added_items),
            "{order}"
        );

        // Simulate persisted state, then Refresh both before and after the
        // retained programme finishes. Neither may revive the pre-Reset cursor.
        let mut refreshed = reset.clone();
        refreshed.mode = PlayoutBuildMode::Refresh;
        refreshed
            .reference_data
            .existing_items
            .retain(|item| item.finish <= boundary);
        refreshed
            .reference_data
            .existing_items
            .extend(result.added_items.clone());
        refreshed
            .reference_data
            .existing_history
            .retain(|history| history.when < boundary);
        refreshed
            .reference_data
            .existing_history
            .extend(result.added_history.clone());
        refreshed.reference_data.existing_scripted_checkpoints =
            result.scripted_checkpoints.clone();
        refreshed.playout.anchor = result.anchor.clone();
        for now in [reset.start, boundary + Duration::seconds(1)] {
            refreshed.start = now;
            let rebuilt = ScriptedSchedulingEngine
                .schedule_script(&script, refreshed.clone())
                .unwrap();
            let seam = rebuilt.retain_through.unwrap();
            let expected = result
                .added_items
                .iter()
                .filter(|item| item.start >= seam)
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(rebuilt.added_items, expected, "{order}, {now}");
        }
    }
}

#[test]
fn scripted_reset_boundary_does_not_restore_old_history_for_late_sources() {
    use crate::scheduling::PlayoutPreservation;
    let old_script = "add_collection('old', 'series', 'chronological') while not is_done() do add_count('old', 1) end";
    let new_script = "add_collection('new', 'series', 'chronological') add_count('new', 1) add_collection('old', 'series', 'chronological') while not is_done() do add_count('old', 1) end";
    let mut original = request();
    original.playout.schedule_kind = ersatzrs_domain::PlayoutScheduleKind::Scripted;
    original.finish = original.start + Duration::minutes(18);
    let published = ScriptedSchedulingEngine
        .schedule_script(old_script, original.clone())
        .unwrap();
    let mut reset = refresh_request(&original, &published);
    reset.mode = PlayoutBuildMode::Reset;
    reset.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    let result = ScriptedSchedulingEngine
        .schedule_script(new_script, reset.clone())
        .unwrap();
    assert_eq!(result.added_items[0].media_item_id, 1);
    assert_eq!(result.added_items[1].media_item_id, 1);
    let mut refreshed = reset;
    refreshed.mode = PlayoutBuildMode::Refresh;
    refreshed
        .reference_data
        .existing_items
        .retain(|item| item.finish <= result.retain_through.unwrap());
    refreshed
        .reference_data
        .existing_items
        .extend(result.added_items.clone());
    refreshed
        .reference_data
        .existing_history
        .retain(|history| history.when < result.retain_through.unwrap());
    refreshed
        .reference_data
        .existing_history
        .extend(result.added_history.clone());
    refreshed.reference_data.existing_scripted_checkpoints = result.scripted_checkpoints.clone();
    // At the initial seam the checkpoint has no sources yet. The persisted
    // history floor is what distinguishes an explicit Reset from a Refresh.
    let rebuilt = ScriptedSchedulingEngine
        .schedule_script(new_script, refreshed)
        .unwrap();
    assert_eq!(rebuilt.added_items, result.added_items);
}

#[test]
fn scripted_explicit_refresh_can_replace_current_without_resetting_series() {
    use crate::scheduling::PlayoutPreservation;
    let script = "add_collection('a', 'series', 'chronological') while not is_done() do add_count('a', 1) end";
    let mut original = request();
    original.playout.schedule_kind = ersatzrs_domain::PlayoutScheduleKind::Scripted;
    original.finish = original.start + Duration::hours(1);
    let full = ScriptedSchedulingEngine
        .schedule_script(script, original.clone())
        .unwrap();
    let mut refresh = refresh_request(&original, &full);
    refresh.reference_data.preservation = PlayoutPreservation::ReplaceCurrent;
    let start = refresh.start;
    let result = ScriptedSchedulingEngine
        .schedule_script(script, refresh)
        .unwrap();
    assert_eq!(result.retain_through, Some(start));
    assert_eq!(result.added_items[0].start, start);
    assert_eq!(
        result.added_items[0].media_item_id,
        full.added_items[1].media_item_id
    );
    assert_eq!(
        result.added_items[1].media_item_id,
        full.added_items[2].media_item_id
    );

    let mut refresh = refresh_request(&original, &full);
    refresh.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    let kept = ScriptedSchedulingEngine
        .schedule_script(script, refresh)
        .unwrap();
    assert_eq!(kept.retain_through, Some(full.added_items[1].finish));
    assert_eq!(
        kept.added_items[0].media_item_id,
        full.added_items[2].media_item_id
    );
    let mut ambiguous = refresh_request(&original, &full);
    ambiguous.reference_data.preservation = PlayoutPreservation::ReplaceCurrent;
    ambiguous
        .reference_data
        .existing_items
        .push(full.added_items[0].clone());
    assert!(ScriptedSchedulingEngine
        .schedule_script(script, ambiguous)
        .is_err());
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
    for fault in 0..6 {
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
            4 => {
                let mut state: serde_json::Value =
                    serde_json::from_str(&checkpoints[1].context).unwrap();
                state["sources"]["a"]["Media"]["state"]["CurrentIndex"] = 999.into();
                checkpoints[1].context = state.to_string();
            }
            _ => {
                let mut state: serde_json::Value =
                    serde_json::from_str(&checkpoints[1].context).unwrap();
                state["history_start"] = (checkpoints[1].checkpoint_date + Duration::seconds(1))
                    .to_rfc3339()
                    .into();
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
