use super::*;
use crate::scheduling::SchedulingMediaItem;
use ersatzrs_domain::Playout;

pub(super) fn request() -> SchedulingRequest {
    let start = DateTime::parse_from_rfc3339("2026-04-15T23:59:00.1234567Z")
        .unwrap()
        .with_timezone(&Utc);
    let mut reference_data = SchedulingReferenceData::default();
    reference_data.collections_by_name.insert(
        "series".to_owned(),
        (1..=7)
            .map(|id| SchedulingMediaItem::new(id, Duration::minutes(3)))
            .collect(),
    );
    SchedulingRequest {
        playout: Playout {
            id: 100,
            seed: 42,
            ..Playout::default()
        },
        start,
        finish: start + Duration::hours(50),
        mode: PlayoutBuildMode::Reset,
        reference_data,
    }
}

#[test]
fn scripted_future_cursor_restores_rotation_across_day_and_fractional_seams() {
    for order in ["chronological", "shuffle", "random"] {
        let script = format!("add_collection('a', 'series', '{order}')\nwhile not is_done() do add_count('a', 1) end");
        let original = request();
        let full = ScriptedSchedulingEngine
            .schedule_script(&script, original.clone())
            .unwrap();
        let mut first_request = original.clone();
        first_request.finish = original.start + Duration::hours(48);
        let first = ScriptedSchedulingEngine
            .schedule_script(&script, first_request)
            .unwrap();
        for missing_anchor in [false, true] {
            let mut next = original.clone();
            next.mode = PlayoutBuildMode::Continue;
            next.start += Duration::hours(1);
            next.playout.anchor = if missing_anchor {
                None
            } else {
                first.anchor.clone()
            };
            next.reference_data.existing_items = first.added_items.clone();
            next.reference_data.existing_history = first.added_history.clone();
            let continued = ScriptedSchedulingEngine
                .schedule_script(&script, next)
                .unwrap();
            let combined = first
                .added_items
                .iter()
                .chain(&continued.added_items)
                .map(|item| (item.media_item_id, item.start, item.finish))
                .collect::<Vec<_>>();
            let expected = full
                .added_items
                .iter()
                .map(|item| (item.media_item_id, item.start, item.finish))
                .collect::<Vec<_>>();
            assert_eq!(
                combined, expected,
                "{order}, missing anchor: {missing_anchor}"
            );
            assert_eq!(
                continued.remove_before,
                Some(original.start - Duration::hours(3))
            );
        }
    }
}

#[test]
fn scripted_context_exposes_mode_and_effective_build_start() {
    let original = request();
    let first = ScriptedSchedulingEngine
        .schedule_script(
            "add_collection('a', 'series')\nadd_count('a', 1)",
            original.clone(),
        )
        .unwrap();
    for (mode, name) in [
        (PlayoutBuildMode::Continue, "continue"),
        (PlayoutBuildMode::Refresh, "refresh"),
        (PlayoutBuildMode::Reset, "reset"),
    ] {
        let mut next = original.clone();
        next.mode = mode;
        next.playout.anchor = first.anchor.clone();
        next.reference_data.existing_items = first.added_items.clone();
        next.reference_data.existing_history = first.added_history.clone();
        next.reference_data.existing_scripted_checkpoints = first.scripted_checkpoints.clone();
        let expected_start = if mode == PlayoutBuildMode::Reset {
            original.start
        } else {
            first.added_items[0].finish
        };
        let script = format!("assert(build_mode() == '{name}')\nassert(start_time() == '{}')\nassert(current_time() == start_time())\nadd_collection('a', 'series')\nadd_count('a', 1)\nassert(start_time() ~= current_time())", expected_start.to_rfc3339());
        let result = ScriptedSchedulingEngine
            .schedule_script(&script, next)
            .unwrap();
        assert_eq!(result.clear_items, mode != PlayoutBuildMode::Continue);
        assert_eq!(result.added_items[0].start, expected_start);
    }
}

#[test]
fn scripted_rejects_backward_emission_and_conflicting_persisted_state() {
    for script in [
        "add_collection('a', 'series')\nwait_until('23:00')\nadd_count('a', 1)",
        "add_collection('a', 'series')\nadd_count('a', 2)\nwait_until('00:00')\nadd_count('a', 1)",
    ] {
        let error = ScriptedSchedulingEngine
            .schedule_script(script, request())
            .unwrap_err();
        assert!(matches!(error, SchedulingError::InvalidScriptedTimeline(_)));
    }
    let original = request();
    let first = ScriptedSchedulingEngine
        .schedule_script(
            "add_collection('a', 'series')\nadd_count('a', 1)",
            original.clone(),
        )
        .unwrap();
    for offset in [-1, 1] {
        let mut next = original.clone();
        next.mode = PlayoutBuildMode::Continue;
        next.playout.anchor = first.anchor.clone();
        next.playout.anchor.as_mut().unwrap().next_start += Duration::seconds(offset);
        next.reference_data.existing_items = first.added_items.clone();
        let error = ScriptedSchedulingEngine
            .schedule_script("", next)
            .unwrap_err();
        assert!(error.to_string().contains("disagrees with anchor"));
    }
}

#[test]
fn scripted_interval_validation_preserves_forward_waits_but_rejects_zero_and_overlap() {
    let start = request().start;
    let later = start + Duration::minutes(1);
    assert!(validate_scripted_timeline([(start, start)], None).is_err());
    assert!(validate_scripted_timeline([(later, start)], None).is_err());
    assert!(validate_scripted_timeline([(start, later), (start, later)], None).is_err());
    assert!(validate_scripted_timeline(
        [
            (start, later),
            (later + Duration::minutes(1), later + Duration::minutes(2))
        ],
        None
    )
    .is_ok());
}

#[test]
fn scripted_continue_refuses_malformed_or_incompatible_anchor_context() {
    for context in ["{broken", r#"{"version":999}"#] {
        let mut request = request();
        request.mode = PlayoutBuildMode::Continue;
        request.playout.anchor = Some(PlayoutAnchor {
            next_start: request.start,
            context: Some(context.to_owned()),
            ..PlayoutAnchor::default()
        });
        assert!(matches!(
            ScriptedSchedulingEngine.schedule_script("", request),
            Err(SchedulingError::InvalidScriptedTimeline(_))
        ));
    }
}
