//! Seeded Random Count is a persisted sequence, rather than one constant per preset.
use super::tests::{filler_preset, schedule_item};
use super::*;
use chrono::TimeZone;
use ersatzrs_domain::{Playout, PlayoutMode, PlayoutScheduleKind};

fn request() -> SchedulingRequest {
    let item = ProgramScheduleItem {
        pre_roll_filler_id: Some(91),
        ..schedule_item(1, PlayoutMode::One, 1, None)
    };
    let mut filler = filler_preset(
        91,
        FillerKind::PreRoll,
        FillerMode::RandomCount,
        Some(100),
        None,
    );
    filler.media_items = vec![SchedulingMediaItem::new(901, Duration::seconds(1))];
    let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 58, 0).unwrap();
    SchedulingRequest {
        playout: Playout {
            id: 50,
            seed: 12345,
            schedule_kind: PlayoutScheduleKind::Classic,
            ..Playout::default()
        },
        start,
        finish: start + Duration::minutes(20),
        mode: PlayoutBuildMode::Continue,
        reference_data: SchedulingReferenceData {
            classic_schedule_items: vec![item.clone()],
            media_by_key: BTreeMap::from([(
                item_key(&item),
                vec![SchedulingMediaItem::new(101, Duration::minutes(1))],
            )]),
            classic_filler_presets: BTreeMap::from([(91, filler)]),
            ..SchedulingReferenceData::default()
        },
    }
}

fn counts(result: &SchedulingResult) -> Vec<usize> {
    let mut counts = Vec::new();
    let mut fillers = 0;
    for item in &result.added_items {
        if item.media_item_id == 901 {
            fillers += 1;
        } else {
            counts.push(fillers);
            fillers = 0;
        }
    }
    counts
}

#[test]
fn random_count_uses_repeatable_advancing_playout_seeded_draws() {
    let original = request();
    let first = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    let second = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    assert_eq!(first.added_items, second.added_items);
    let actual = counts(&first);
    assert!(actual.len() > 4);
    assert!(
        actual.windows(2).any(|pair| pair[0] != pair[1]),
        "{actual:?}"
    );
    let mut state = seed_to_u64(original.playout.seed);
    let expected = actual
        .iter()
        .map(|_| {
            state = next_random_state(state);
            usize::try_from((state >> 33) % 101).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn random_count_continue_matches_uninterrupted_draws_across_midnight() {
    let original = request();
    let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    let mut partial = original.clone();
    partial.finish = original.start + Duration::minutes(3);
    let first = ClassicSchedulingEngine.schedule(partial).unwrap();
    let mut continued = super::preservation_tests::published_request(&original, &first);
    continued.mode = PlayoutBuildMode::Continue;
    continued.playout.anchor =
        serde_json::from_str(&serde_json::to_string(&continued.playout.anchor).unwrap()).unwrap();
    let second = ClassicSchedulingEngine.schedule(continued).unwrap();
    assert_eq!(
        second.added_items,
        full.added_items[first.added_items.len()..]
    );
    assert_eq!(second.anchor, full.anchor);
}

#[test]
fn random_count_rejected_plain_and_chapter_blocks_restore_draw_and_filler_state() {
    for with_chapters in [false, true] {
        let mut original = request();
        let item = original.reference_data.classic_schedule_items[0].clone();
        if with_chapters {
            original.reference_data.classic_chapters_by_media_id.insert(
                (101, None),
                [(0, 30), (30, 60)]
                    .into_iter()
                    .map(|(start, end)| ersatzrs_domain::MediaChapter {
                        start_time: Duration::seconds(start),
                        end_time: Duration::seconds(end),
                        ..ersatzrs_domain::MediaChapter::default()
                    })
                    .collect(),
            );
            original.reference_data.classic_schedule_items[0].mid_roll_filler_id = Some(92);
            let mut mid = filler_preset(92, FillerKind::MidRoll, FillerMode::Count, Some(1), None);
            mid.media_items = vec![SchedulingMediaItem::new(902, Duration::seconds(1))];
            original
                .reference_data
                .classic_filler_presets
                .insert(92, mid);
        }
        let item = ProgramScheduleItem {
            mid_roll_filler_id: with_chapters.then_some(92),
            ..item
        };
        let items = original.reference_data.classic_schedule_items.clone();
        let state = state_for_mode(&original, &items);
        let mut context = ClassicContext::new(original.clone(), items, state);
        let before = context.random_count_state;
        assert!(context
            .build_media_item_block(
                &item,
                &SchedulingMediaItem::new(101, Duration::minutes(1)),
                original.start,
                Some(original.start + Duration::seconds(30)),
                "Duration"
            )
            .is_none());
        assert_eq!(context.random_count_state, before);
        assert!(context
            .filler_media
            .values()
            .all(|media| media.current_index() == 0));
    }
}

#[test]
fn random_count_missing_legacy_state_refuses_continue() {
    let original = request();
    let first = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    let mut continued = super::preservation_tests::published_request(&original, &first);
    continued.mode = PlayoutBuildMode::Continue;
    continued.finish += Duration::minutes(10);
    let anchor = continued.playout.anchor.as_mut().unwrap();
    let mut value: serde_json::Value =
        serde_json::from_str(anchor.context.as_deref().unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("random_count_state");
    anchor.context = Some(value.to_string());
    assert!(!classic_playlist_continuation_is_complete(
        &continued.reference_data,
        anchor.context.as_deref()
    ));
    let error = ClassicSchedulingEngine.schedule(continued).unwrap_err();
    assert!(error.to_string().contains("Random Count state is missing"));
}

#[test]
fn random_count_zero_and_maximum_counts_advance_without_integer_overflow() {
    let original = request();
    let item = original.reference_data.classic_schedule_items[0].clone();
    let items = vec![item.clone()];
    let state = state_for_mode(&original, &items);
    let mut context = ClassicContext::new(original.clone(), items, state);
    context.filler_media.get_mut(&91).unwrap().items.clear();
    for count in [0, i32::MAX] {
        context
            .request
            .reference_data
            .classic_filler_presets
            .get_mut(&91)
            .unwrap()
            .count = Some(count);
        let before = context.random_count_state;
        assert!(context
            .add_count_or_duration_filler(&item, 91, original.start, FillerKind::PreRoll)
            .is_empty());
        assert_ne!(context.random_count_state, before);
    }
}
