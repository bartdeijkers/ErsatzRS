//! Duration-discard contracts from upstream e51ac60c (UCU-16 row287).
use super::tests::{filler_preset, schedule_item};
use super::*;
use chrono::TimeZone;
use ersatzrs_domain::{Playout, PlayoutMode, PlayoutScheduleKind};

fn request(durations: &[i64], attempts: i32, tail_mode: TailMode) -> SchedulingRequest {
    let duration = ProgramScheduleItem {
        duration: Some(Duration::hours(1)),
        discard_to_fill_attempts: attempts,
        tail_mode,
        ..schedule_item(1, PlayoutMode::Duration, 1, None)
    };
    let next = schedule_item(2, PlayoutMode::One, 2, None);
    let data = SchedulingReferenceData {
        classic_schedule_items: vec![duration.clone(), next.clone()],
        media_by_key: BTreeMap::from([
            (
                item_key(&duration),
                durations
                    .iter()
                    .enumerate()
                    .map(|(i, minutes)| {
                        SchedulingMediaItem::new(
                            i64::try_from(i).unwrap() + 101,
                            Duration::minutes(*minutes),
                        )
                    })
                    .collect(),
            ),
            (
                item_key(&next),
                vec![SchedulingMediaItem::new(201, Duration::minutes(10))],
            ),
        ]),
        ..SchedulingReferenceData::default()
    };
    let start = Utc.with_ymd_and_hms(2026, 11, 11, 6, 0, 0).unwrap();
    SchedulingRequest {
        playout: Playout {
            id: 50,
            seed: 7,
            schedule_kind: PlayoutScheduleKind::Classic,
            ..Playout::default()
        },
        start,
        finish: start + Duration::minutes(70),
        mode: PlayoutBuildMode::Continue,
        reference_data: data,
    }
}

fn first_block(request: SchedulingRequest) -> ClassicContext {
    let items = request.reference_data.classic_schedule_items.clone();
    let state = state_for_mode(&request, &items);
    let mut context = ClassicContext::new(request, items.clone(), state);
    context.schedule_duration(&items[0]);
    context
}

#[test]
fn duration_discard_exhaustion_stops_at_upstream_boundary_and_keeps_rejected_cursor() {
    let request = request(&[40, 40, 40, 15], 1, TailMode::Offline);
    let context = first_block(request.clone());
    assert_eq!(
        context
            .result
            .added_items
            .iter()
            .map(|i| i.media_item_id)
            .collect::<Vec<_>>(),
        [101]
    );
    assert_eq!(
        context.state.current_time,
        request.start + Duration::hours(1)
    );
    assert!(context.result.added_items[0].guide_finish.is_none());
    assert!(context.state.duration_finish.is_none());
    assert_eq!(context.current_schedule_index(), 1);
    assert_eq!(context.media(&context.schedule_items[0]).current_index(), 2);
    let result = ClassicSchedulingEngine.schedule(request.clone()).unwrap();
    assert_eq!(
        result
            .added_items
            .iter()
            .map(|i| i.media_item_id)
            .collect::<Vec<_>>(),
        [101, 201]
    );
    assert_eq!(
        result.added_items[1].start,
        request.start + Duration::hours(1)
    );
}

#[test]
fn duration_discard_reaches_fitting_content_within_budget_and_resets_after_success() {
    let context = first_block(request(&[20, 50, 20, 50, 20], 1, TailMode::Offline));
    assert_eq!(
        context
            .result
            .added_items
            .iter()
            .map(|i| i.media_item_id)
            .collect::<Vec<_>>(),
        [101, 103, 105]
    );
    assert_eq!(context.current_schedule_index(), 1);
    assert!(context.state.duration_finish.is_none());
}

#[test]
fn duration_discard_does_not_skip_when_disabled_or_no_remaining_item_can_fit() {
    for (durations, attempts, index) in [(&[40, 40, 15][..], 0, 1), (&[40, 40][..], 3, 1)] {
        let context = first_block(request(durations, attempts, TailMode::Offline));
        assert_eq!(
            context.media(&context.schedule_items[0]).current_index(),
            index
        );
        assert!(context.state.duration_finish.is_none());
        assert_eq!(context.current_schedule_index(), 1);
    }
}

#[test]
fn duration_discard_exhaustion_uses_tail_and_fallback_before_next_item() {
    for tail_mode in [TailMode::Offline, TailMode::Filler] {
        let mut request = request(&[40, 40, 40, 15], 1, tail_mode);
        let item = &mut request.reference_data.classic_schedule_items[0];
        item.tail_filler_id = Some(91);
        item.fallback_filler_id = Some(92);
        let mut tail = filler_preset(91, FillerKind::Tail, FillerMode::Duration, None, None);
        tail.media_items = vec![SchedulingMediaItem::new(891, Duration::minutes(15))];
        let mut fallback =
            filler_preset(92, FillerKind::Fallback, FillerMode::Count, Some(1), None);
        fallback.media_items = vec![SchedulingMediaItem::new(892, Duration::minutes(10))];
        request.reference_data.classic_filler_presets =
            BTreeMap::from([(91, tail), (92, fallback)]);
        let result = ClassicSchedulingEngine.schedule(request.clone()).unwrap();
        let expected = if tail_mode == TailMode::Filler {
            vec![101, 891, 892, 201]
        } else {
            vec![101, 892, 892, 201]
        };
        assert_eq!(
            result
                .added_items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            result.added_items.last().unwrap().start,
            request.start + Duration::hours(1)
        );
        assert!(result
            .added_items
            .windows(2)
            .all(|w| w[0].finish == w[1].start));
    }
}

#[test]
fn duration_discard_preserves_tail_none_cursor_for_following_item() {
    let request = request(&[40, 40, 40, 15], 1, TailMode::None);
    let context = first_block(request.clone());
    assert_eq!(
        context.state.current_time,
        request.start + Duration::minutes(40)
    );
    assert_eq!(context.current_schedule_index(), 1);
    assert!(context.state.duration_finish.is_none());
}

#[test]
fn duration_discard_skips_collection_that_can_never_fit_without_looping() {
    let context = first_block(request(&[80, 90], 1, TailMode::Offline));
    assert!(context.result.added_items.is_empty());
    assert!(context.state.duration_finish.is_none());
    assert_eq!(context.current_schedule_index(), 1);
}

#[test]
fn duration_discard_continue_at_content_boundary_matches_uninterrupted() {
    for (attempts, title) in [(1, None), (2, None), (2, Some("Duration block"))] {
        let mut original = request(&[40, 40, 40, 15], attempts, TailMode::Offline);
        original.reference_data.classic_schedule_items[0].custom_title = title.map(str::to_owned);
        let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
        let mut partial = original.clone();
        partial.finish = original.start + Duration::minutes(40);
        let first = ClassicSchedulingEngine.schedule(partial).unwrap();
        let mut continued = super::preservation_tests::published_request(&original, &first);
        continued.mode = PlayoutBuildMode::Continue;
        continued.playout.anchor =
            serde_json::from_str(&serde_json::to_string(&continued.playout.anchor).unwrap())
                .unwrap();
        let second = ClassicSchedulingEngine.schedule(continued).unwrap();
        let actual = first
            .added_items
            .iter()
            .chain(&second.added_items)
            .map(|i| (i.media_item_id, i.start, i.finish))
            .collect::<Vec<_>>();
        let expected = full
            .added_items
            .iter()
            .map(|i| (i.media_item_id, i.start, i.finish))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "attempt budget {attempts}");
        assert_eq!(second.anchor, full.anchor);
        assert_eq!(
            second
                .program_schedule_anchors
                .iter()
                .filter(|a| a.anchor_date.is_none())
                .map(|a| (&a.enumerator_state, &a.fake_collection_key))
                .collect::<Vec<_>>(),
            full.program_schedule_anchors
                .iter()
                .filter(|a| a.anchor_date.is_none())
                .map(|a| (&a.enumerator_state, &a.fake_collection_key))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn duration_discard_rejects_filler_overrun_without_consuming_content_or_filler() {
    let mut original = request(&[40, 15], 1, TailMode::Offline);
    original.reference_data.classic_schedule_items[0].post_roll_filler_id = Some(93);
    let mut filler = filler_preset(93, FillerKind::PostRoll, FillerMode::Count, Some(1), None);
    filler.media_items = vec![SchedulingMediaItem::new(893, Duration::minutes(30))];
    original
        .reference_data
        .classic_filler_presets
        .insert(93, filler);
    let context = first_block(original.clone());
    assert!(context.result.added_items.is_empty());
    assert_eq!(
        context.state.current_time,
        original.start + Duration::hours(1)
    );
    assert!(context.state.duration_finish.is_none());
    assert_eq!(context.current_schedule_index(), 1);
    assert_eq!(context.media(&context.schedule_items[0]).current_index(), 0);
    assert_eq!(context.filler_media[&93].current_index(), 0);
    let result = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    assert_eq!(result.added_items[0].media_item_id, 201);
    assert_eq!(
        result.added_items[0].start,
        original.start + Duration::hours(1)
    );
}
