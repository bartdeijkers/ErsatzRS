use super::tests::{filler_preset, schedule_item};
use super::*;
use chrono::TimeZone;
use ersatzrs_domain::{Playout, PlayoutMode, PlayoutScheduleKind};

fn context() -> (ClassicContext, ProgramScheduleItem) {
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
    let item = ProgramScheduleItem {
        fallback_filler_id: Some(94),
        ..schedule_item(1, PlayoutMode::One, 1, None)
    };
    let mut fallback = filler_preset(94, FillerKind::Fallback, FillerMode::Count, Some(1), None);
    fallback.allow_watermarks = false;
    fallback.media_items = vec![
        SchedulingMediaItem::new(5, Duration::minutes(2)),
        SchedulingMediaItem::new(6, Duration::minutes(2)),
    ];
    let request = SchedulingRequest {
        playout: Playout {
            id: 50,
            schedule_kind: PlayoutScheduleKind::Classic,
            ..Playout::default()
        },
        start,
        finish: start + Duration::hours(1),
        mode: PlayoutBuildMode::Continue,
        reference_data: SchedulingReferenceData {
            classic_schedule_items: vec![item.clone()],
            classic_filler_presets: BTreeMap::from([(94, fallback)]),
            ..SchedulingReferenceData::default()
        },
    };
    let state = state_for_mode(&request, std::slice::from_ref(&item));
    (
        ClassicContext::new(request, vec![item.clone()], state),
        item,
    )
}

#[test]
fn fallback_fills_large_gap_and_advances_every_clip() {
    let (mut ctx, item) = context();
    let start = ctx.state.current_time;
    ctx.add_fallback(&item, start + Duration::minutes(5));
    let rows = &ctx.result.added_items;
    assert_eq!(
        rows.iter().map(|row| row.media_item_id).collect::<Vec<_>>(),
        [5, 6, 5]
    );
    for (index, row) in rows.iter().enumerate() {
        let offset = Duration::minutes(i64::try_from(index).unwrap() * 2);
        let duration = Duration::minutes(if index == 2 { 1 } else { 2 });
        assert_eq!(
            (row.start, row.finish),
            (start + offset, start + offset + duration)
        );
        assert_eq!((row.in_point, row.out_point), (Duration::zero(), duration));
        assert_eq!(row.filler_kind, FillerKind::Fallback);
        assert_eq!(row.guide_group, rows[0].guide_group);
        assert!(row.guide_finish.is_none());
        assert!(row.disable_watermarks);
    }
    assert_eq!(ctx.filler_media[&94].current().unwrap().id, 6);
    assert_eq!(ctx.result.added_history.len(), 3);
    assert_eq!(ctx.state.current_time, start + Duration::minutes(5));
}

#[test]
fn fallback_pad_gap_uses_eight_clips_and_clips_last() {
    let (mut ctx, mut item) = context();
    item.post_roll_filler_id = Some(95);
    let mut pad = filler_preset(95, FillerKind::PostRoll, FillerMode::Pad, None, None);
    pad.pad_to_nearest_minute = Some(60);
    ctx.request
        .reference_data
        .classic_filler_presets
        .insert(95, pad);
    let start = ctx.state.current_time;
    let rows = ctx
        .build_media_item_block(
            &item,
            &SchedulingMediaItem::new(1, Duration::minutes(45)),
            start,
            None,
            "One",
        )
        .unwrap();
    assert_eq!(rows.len(), 9);
    assert_eq!(rows[1].start, start + Duration::minutes(45));
    assert_eq!(rows[8].media_item_id, 6);
    assert_eq!(rows[8].start, start + Duration::minutes(59));
    assert_eq!(rows[8].finish, start + Duration::hours(1));
    assert_eq!(rows[8].out_point, Duration::minutes(1));
}

#[test]
fn fallback_chapter_preserves_source_offsets_title_and_tracks() {
    let (mut ctx, item) = context();
    let mut chapter = SchedulingMediaItem::new(5, Duration::minutes(2));
    chapter.chapter = Some(super::super::SchedulingMediaChapter {
        start: Duration::minutes(10),
        title: Some("Chapter two".into()),
    });
    chapter.preferred_audio_language_code = Some("nld".into());
    chapter.preferred_subtitle_language_code = Some("eng".into());
    ctx.filler_media.get_mut(&94).unwrap().items = vec![chapter];
    let start = ctx.state.current_time;
    let rows = ctx.fallback_items(&item, start, start + Duration::minutes(3));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].media_item_id, 5);
    assert_eq!(
        (rows[1].in_point, rows[1].out_point),
        (Duration::minutes(10), Duration::minutes(11))
    );
    assert_eq!(rows[1].chapter_title.as_deref(), Some("Chapter two"));
    assert_eq!(
        rows[1].preferred_audio_language_code.as_deref(),
        Some("nld")
    );
    assert_eq!(
        rows[1].preferred_subtitle_language_code.as_deref(),
        Some("eng")
    );
}

#[test]
fn fallback_invalid_durations_terminate_without_stretched_rows() {
    let (mut ctx, item) = context();
    ctx.filler_media.get_mut(&94).unwrap().items =
        vec![SchedulingMediaItem::new(5, Duration::zero())];
    let start = ctx.state.current_time;
    assert!(ctx
        .fallback_items(&item, start, start + Duration::hours(1))
        .is_empty());
    assert_eq!(ctx.result.warnings.len(), 1);
}

#[test]
fn fallback_continue_round_trip_matches_uninterrupted_sequence() {
    let (mut first, item) = context();
    let start = first.state.current_time;
    first.add_fallback(&item, start + Duration::minutes(5));
    let mut request = first.request.clone();
    request.playout.anchor = Some(first.playout_anchor(first.state.current_time));
    // Exercise the serialized anchor boundary used by persistence/restart.
    let json = serde_json::to_string(&request.playout.anchor).unwrap();
    request.playout.anchor = serde_json::from_str(&json).unwrap();
    let state = state_for_mode(&request, std::slice::from_ref(&item));
    let mut continued = ClassicContext::new(request, vec![item.clone()], state);
    let expected = first.fallback_items(
        &item,
        start + Duration::minutes(5),
        start + Duration::minutes(10),
    );
    let actual = continued.fallback_items(
        &item,
        start + Duration::minutes(5),
        start + Duration::minutes(10),
    );
    assert_eq!(actual, expected);
    assert_eq!(actual[0].media_item_id, 6);
}

#[test]
fn fallback_all_classic_modes_fill_fixed_start_gap() {
    for mode in [
        PlayoutMode::One,
        PlayoutMode::Multiple,
        PlayoutMode::Duration,
        PlayoutMode::Flood,
    ] {
        let (ctx, mut item) = context();
        item.playout_mode = mode;
        item.multiple_count = Some("1".into());
        item.start_type = StartType::Fixed;
        item.start_time = Some(Duration::hours(6));
        item.duration = Some(Duration::hours(3));
        item.tail_mode = TailMode::Filler;
        let next = ProgramScheduleItem {
            start_type: StartType::Fixed,
            start_time: Some(Duration::hours(9)),
            ..schedule_item(2, PlayoutMode::One, 2, None)
        };
        let mut request = ctx.request;
        request.finish = request.start + Duration::hours(3) + Duration::minutes(15);
        request.reference_data.media_by_key.insert(
            item_key(&item),
            vec![SchedulingMediaItem::new(100, Duration::hours(2))],
        );
        request.reference_data.media_by_key.insert(
            item_key(&next),
            vec![SchedulingMediaItem::new(200, Duration::minutes(15))],
        );
        request.reference_data.classic_schedule_items = vec![item, next];
        let result = ClassicSchedulingEngine.schedule(request).unwrap();
        let filler = result
            .added_items
            .iter()
            .filter(|item| item.filler_kind == FillerKind::Fallback)
            .collect::<Vec<_>>();
        assert_eq!(filler.len(), 30, "{mode:?}");
        assert!(filler
            .windows(2)
            .all(|rows| rows[0].finish == rows[1].start));
        assert!(filler
            .iter()
            .all(|row| row.out_point == Duration::minutes(2)));
        assert_eq!(filler.last().unwrap().finish.hour(), 9);
    }
}
