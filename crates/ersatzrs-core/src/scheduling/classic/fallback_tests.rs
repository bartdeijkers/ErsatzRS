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

#[test]
fn mid_roll_pad_distributes_fallback_clips_between_chapters_before_post_roll() {
    let (mut ctx, mut item) = context();
    item.mid_roll_filler_id = Some(95);
    item.post_roll_filler_id = Some(96);
    let mut mid = filler_preset(95, FillerKind::MidRoll, FillerMode::Pad, None, None);
    mid.pad_to_nearest_minute = Some(60);
    ctx.request
        .reference_data
        .classic_filler_presets
        .insert(95, mid);
    let post_media = SchedulingMediaItem::new(10, Duration::minutes(5));
    let mut post = filler_preset(96, FillerKind::PostRoll, FillerMode::Count, Some(1), None);
    post.media_items = vec![post_media.clone()];
    ctx.request
        .reference_data
        .classic_filler_presets
        .insert(96, post);
    ctx.filler_media
        .insert(96, MediaEnumerator::new("post", vec![post_media]));
    let chapters = [(0, 6), (6, 20), (20, 45)]
        .into_iter()
        .map(|(start, end)| ersatzrs_domain::MediaChapter {
            start_time: Duration::minutes(start),
            end_time: Duration::minutes(end),
            ..ersatzrs_domain::MediaChapter::default()
        })
        .collect();
    ctx.request
        .reference_data
        .classic_chapters_by_media_id
        .insert((100, None), chapters);
    let start = ctx.state.current_time;
    let mut media = SchedulingMediaItem::new(100, Duration::minutes(45));
    media.preferred_audio_language_code = Some("nld".into());
    let rows = ctx
        .build_media_item_block(&item, &media, start, None, "One")
        .unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(
        rows.iter().map(|row| row.media_item_id).collect::<Vec<_>>(),
        [100, 5, 6, 5, 100, 6, 5, 6, 100, 10]
    );
    assert!(rows.windows(2).all(|pair| pair[0].finish == pair[1].start));
    for (index, offset, finish) in [(0, 0, 6), (4, 6, 20), (8, 20, 45)] {
        assert_eq!(rows[index].in_point, Duration::minutes(offset));
        assert_eq!(rows[index].out_point, Duration::minutes(finish));
        assert_eq!(
            rows[index].preferred_audio_language_code.as_deref(),
            Some("nld")
        );
    }
    assert_eq!(rows[3].out_point, Duration::minutes(1));
    assert_eq!(rows[7].out_point, Duration::minutes(1));
    assert_eq!(rows[9].start, start + Duration::minutes(55));
    assert_eq!(rows[9].finish, start + Duration::hours(1));
    assert!(
        ctx.result.warnings.is_empty(),
        "an empty pad source intentionally uses fallback"
    );
}

#[test]
fn pad_rounds_wall_clock_and_always_adds_at_exact_boundary() {
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 7, 30).unwrap();
    assert_eq!(
        super::mid_roll::pad_budget(start, Duration::minutes(5), Some(15)),
        Duration::seconds(150)
    );
    assert_eq!(
        super::mid_roll::pad_budget(start, Duration::seconds(450), Some(15)),
        Duration::minutes(15)
    );
}

#[test]
fn chapter_slices_are_one_smart_shuffle_selection_unit() {
    let (mut ctx, mut item) = context();
    item.mid_roll_filler_id = Some(95);
    item.playback_order = PlaybackOrder::Shuffle;
    let mid = filler_preset(95, FillerKind::MidRoll, FillerMode::Count, Some(0), None);
    ctx.request
        .reference_data
        .classic_filler_presets
        .insert(95, mid);
    ctx.request
        .reference_data
        .smart_shuffle_policies_by_unit
        .insert(
            item.id.to_string(),
            super::super::SchedulingSmartShufflePolicy::for_preset(
                super::super::SchedulingSmartShufflePreset::Light,
            ),
        );
    ctx.request
        .reference_data
        .classic_chapters_by_media_id
        .insert(
            (100, None),
            [(0, 10), (10, 20), (20, 30)]
                .into_iter()
                .map(|(start, end)| ersatzrs_domain::MediaChapter {
                    start_time: Duration::minutes(start),
                    end_time: Duration::minutes(end),
                    ..ersatzrs_domain::MediaChapter::default()
                })
                .collect(),
        );
    let start = ctx.state.current_time;
    assert!(ctx.schedule_media_item(
        &item,
        &SchedulingMediaItem::new(100, Duration::minutes(30)),
        start,
        None,
        "One"
    ));
    assert_eq!(ctx.result.added_items.len(), 3);
    let state = serde_json::to_value(&ctx.smart_shuffle_state).unwrap();
    let buckets = state["buckets"].as_object().unwrap();
    assert!(!buckets.is_empty());
    assert!(buckets
        .values()
        .all(|bucket| bucket["completed_items"] == 1));
}

#[test]
fn duration_filler_accepts_more_than_sixty_four_valid_clips() {
    let (mut ctx, item) = context();
    let start = ctx.state.current_time;
    let rows = ctx.add_duration_filler(
        &item,
        94,
        start,
        FillerKind::PreRoll,
        Duration::minutes(140),
        false,
    );
    assert_eq!(rows.len(), 70);
    assert_eq!(rows.last().unwrap().finish, start + Duration::minutes(140));
    assert!(ctx.result.warnings.is_empty());
}

#[test]
fn duration_filler_stops_after_one_unusable_cycle() {
    let (mut ctx, item) = context();
    let start = ctx.state.current_time;
    let rows = ctx.add_duration_filler(
        &item,
        94,
        start,
        FillerKind::PreRoll,
        Duration::seconds(1),
        false,
    );
    assert!(rows.is_empty());
    assert_eq!(ctx.filler_media[&94].current_index(), 0);
    assert_eq!(ctx.result.warnings.len(), 1);
}

#[test]
fn preroll_pad_places_fallback_before_regular_filler() {
    let (mut ctx, mut item) = context();
    item.pre_roll_filler_id = Some(95);
    let mut pad = filler_preset(95, FillerKind::PreRoll, FillerMode::Pad, None, None);
    pad.pad_to_nearest_minute = Some(60);
    pad.media_items = vec![SchedulingMediaItem::new(8, Duration::minutes(4))];
    ctx.filler_media
        .insert(95, MediaEnumerator::new("pad", pad.media_items.clone()));
    ctx.request
        .reference_data
        .classic_filler_presets
        .insert(95, pad);
    let start = ctx.state.current_time;
    let rows = ctx
        .build_media_item_block(
            &item,
            &SchedulingMediaItem::new(1, Duration::minutes(53)),
            start,
            None,
            "One",
        )
        .unwrap();
    assert_eq!(
        rows.iter().map(|row| row.media_item_id).collect::<Vec<_>>(),
        [5, 6, 8, 1]
    );
    assert_eq!(rows[0].start, start);
    assert_eq!(rows[2].start, start + Duration::minutes(3));
    assert_eq!(rows[3].start, start + Duration::minutes(7));
    assert_eq!(rows[3].finish, start + Duration::hours(1));
    assert_eq!(rows[1].out_point, Duration::minutes(1));
}

#[test]
fn unsplit_midroll_pad_follows_regular_postroll() {
    let (mut ctx, mut item) = context();
    item.mid_roll_filler_id = Some(95);
    item.post_roll_filler_id = Some(96);
    let mut pad = filler_preset(95, FillerKind::MidRoll, FillerMode::Pad, None, None);
    pad.pad_to_nearest_minute = Some(60);
    let mut post = filler_preset(96, FillerKind::PostRoll, FillerMode::Count, Some(1), None);
    post.media_items = vec![SchedulingMediaItem::new(8, Duration::minutes(4))];
    ctx.filler_media
        .insert(96, MediaEnumerator::new("post", post.media_items.clone()));
    ctx.request
        .reference_data
        .classic_filler_presets
        .extend([(95, pad), (96, post)]);
    let start = ctx.state.current_time;
    let rows = ctx
        .build_media_item_block(
            &item,
            &SchedulingMediaItem::new(1, Duration::minutes(53)),
            start,
            None,
            "One",
        )
        .unwrap();
    assert_eq!(
        rows.iter().map(|row| row.media_item_id).collect::<Vec<_>>(),
        [1, 8, 5, 6]
    );
    assert_eq!(rows.last().unwrap().finish, start + Duration::hours(1));
}

#[test]
fn chapter_expression_selects_breaks_and_preserves_unsplit_content_without_matches() {
    for mode in [FillerMode::Count, FillerMode::Pad] {
        for expression in ["num = 2", "false", "(", "num = 1 or missing"] {
            let (mut ctx, mut item) = context();
            item.mid_roll_filler_id = Some(95);
            let mut mid = filler_preset(95, FillerKind::MidRoll, mode, Some(1), None);
            mid.expression = Some(expression.into());
            mid.pad_to_nearest_minute = Some(60);
            mid.media_items = vec![SchedulingMediaItem::new(8, Duration::minutes(5))];
            ctx.filler_media
                .insert(95, MediaEnumerator::new("mid", mid.media_items.clone()));
            ctx.request
                .reference_data
                .classic_filler_presets
                .insert(95, mid);
            ctx.request
                .reference_data
                .classic_chapters_by_media_id
                .insert(
                    (100, None),
                    [(0, 10), (10, 20), (20, 45)]
                        .into_iter()
                        .map(|(start, end)| ersatzrs_domain::MediaChapter {
                            start_time: Duration::minutes(start),
                            end_time: Duration::minutes(end),
                            ..ersatzrs_domain::MediaChapter::default()
                        })
                        .collect(),
                );
            let start = ctx.state.current_time;
            let rows = ctx
                .build_media_item_block(
                    &item,
                    &SchedulingMediaItem::new(100, Duration::minutes(45)),
                    start,
                    None,
                    "One",
                )
                .unwrap();
            let primary = rows
                .iter()
                .filter(|row| row.media_item_id == 100)
                .collect::<Vec<_>>();
            if expression == "num = 2" {
                assert_eq!(primary.len(), 2);
                assert_eq!(primary[0].out_point, Duration::minutes(20));
                assert_eq!(primary[1].in_point, Duration::minutes(20));
                assert_eq!(primary[1].out_point, Duration::minutes(45));
                assert_eq!(rows[1].media_item_id, 8);
            } else {
                assert_eq!(primary.len(), 1);
                assert_eq!(
                    (primary[0].in_point, primary[0].out_point),
                    (Duration::zero(), Duration::minutes(45))
                );
                assert_eq!(rows[0].media_item_id, 100);
                assert_eq!(rows.len(), if mode == FillerMode::Count { 1 } else { 4 });
            }
            assert!(rows.windows(2).all(|pair| pair[0].finish == pair[1].start));
            if expression != "num = 2" {
                assert_eq!(
                    rows.last().unwrap().finish,
                    start + Duration::minutes(if mode == FillerMode::Pad { 60 } else { 45 })
                );
            }
            assert_eq!(
                ctx.result.warnings.is_empty(),
                expression != "(" && expression != "num = 1 or missing"
            );
        }
    }
}

#[test]
fn invalid_chapter_expression_builds_whole_programmes_with_redacted_preset_warning() {
    let (ctx, mut item) = context();
    item.mid_roll_filler_id = Some(95);
    let mut request = ctx.request;
    let mut mid = filler_preset(95, FillerKind::MidRoll, FillerMode::Count, Some(1), None);
    mid.expression = Some("private chapter title (".into());
    mid.media_items = vec![SchedulingMediaItem::new(7, Duration::minutes(1))];
    request
        .reference_data
        .classic_filler_presets
        .insert(95, mid);
    request.reference_data.media_by_key.insert(
        item_key(&item),
        vec![SchedulingMediaItem::new(100, Duration::minutes(20))],
    );
    request.reference_data.classic_schedule_items = vec![item.clone()];
    request.reference_data.classic_chapters_by_media_id.insert(
        (100, None),
        [(0, 10), (10, 20)]
            .into_iter()
            .map(|(start, end)| ersatzrs_domain::MediaChapter {
                start_time: Duration::minutes(start),
                end_time: Duration::minutes(end),
                ..ersatzrs_domain::MediaChapter::default()
            })
            .collect(),
    );
    request.mode = PlayoutBuildMode::Reset;
    let original_chapters = request.reference_data.classic_chapters_by_media_id.clone();
    let result = ClassicSchedulingEngine.schedule(request.clone()).unwrap();
    assert_eq!(result.added_items.len(), 3);
    assert!(result.added_items.iter().all(|row| row.media_item_id == 100
        && row.in_point == Duration::zero()
        && row.out_point == Duration::minutes(20)));
    assert_eq!(result.warnings.len(), 1);
    let warning = &result.warnings[0];
    assert!(warning.contains("95 (Synthetic filler 95)"));
    assert!(warning.contains(&format!("schedule item {}", item.id)));
    assert!(!warning.contains("private chapter title"));
    assert_eq!(
        request.reference_data.classic_chapters_by_media_id,
        original_chapters
    );
}

#[test]
fn preroll_pad_expression_controls_placement_only_when_midroll_exposes_chapters() {
    for (with_midroll, mid_expression, expression, expected_first) in [
        (true, None, "false", 100),
        (true, Some("false"), "false", 100),
        (true, Some("false"), "(", 100),
        (true, None, "(", 100),
        (false, None, "(", 5),
    ] {
        let (mut ctx, mut item) = context();
        item.pre_roll_filler_id = Some(95);
        let mut pad = filler_preset(95, FillerKind::PreRoll, FillerMode::Pad, None, None);
        pad.expression = Some(expression.into());
        pad.pad_to_nearest_minute = Some(60);
        ctx.request
            .reference_data
            .classic_filler_presets
            .insert(95, pad);
        if with_midroll {
            item.mid_roll_filler_id = Some(96);
            let mut mid = filler_preset(96, FillerKind::MidRoll, FillerMode::Count, Some(0), None);
            mid.expression = mid_expression.map(str::to_owned);
            ctx.request
                .reference_data
                .classic_filler_presets
                .insert(96, mid);
        }
        ctx.request
            .reference_data
            .classic_chapters_by_media_id
            .insert(
                (100, None),
                [(0, 20), (20, 45)]
                    .into_iter()
                    .map(|(start, end)| ersatzrs_domain::MediaChapter {
                        start_time: Duration::minutes(start),
                        end_time: Duration::minutes(end),
                        ..ersatzrs_domain::MediaChapter::default()
                    })
                    .collect(),
            );
        let start = ctx.state.current_time;
        let rows = ctx.build_media_item_block(
            &item,
            &SchedulingMediaItem::new(100, Duration::minutes(45)),
            start,
            None,
            "One",
        );
        let rows = rows.unwrap();
        assert_eq!(rows.first().unwrap().media_item_id, expected_first);
        assert_eq!(rows.last().unwrap().finish, start + Duration::hours(1));
        assert!(ctx.error.is_none());
    }
}
