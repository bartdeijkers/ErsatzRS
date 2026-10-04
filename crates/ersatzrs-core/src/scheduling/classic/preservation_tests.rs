use super::*;
use chrono::TimeZone;
use ersatzrs_domain::{Playout, PlayoutMode, PlayoutScheduleKind};

#[test]
fn classic_preservation_retains_prefetched_programmes_without_advancing_cleanup_clock() {
    let original = request(PlayoutMode::One, PlaybackOrder::Shuffle, false);
    let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let mut rebuild = published_request(&original, &full);
        rebuild.mode = mode;
        rebuild.start += Duration::seconds(1);
        let now = rebuild.start;
        rebuild.reference_data.preservation_not_before =
            Some(full.added_items[2].start + Duration::seconds(1));
        let seam = full.added_items[2].finish;
        let result = ClassicSchedulingEngine.schedule(rebuild).unwrap();
        assert_eq!(result.retain_through, Some(seam));
        assert_eq!(result.added_items[0].start, seam);
        assert_eq!(result.remove_before, Some(now - Duration::hours(4)));
        if mode == PlayoutBuildMode::Refresh {
            assert_eq!(result.added_items, full.added_items[3..]);
        }
    }
}

fn request(mode: PlayoutMode, order: PlaybackOrder, grouped: bool) -> SchedulingRequest {
    let item = ProgramScheduleItem {
        playback_order: order,
        custom_title: grouped.then(|| "Programme group".to_owned()),
        duration: Some(Duration::minutes(15)),
        ..tests::schedule_item(1, mode, 1, Some("3"))
    };
    let key = item_key(&item);
    let reference_data = SchedulingReferenceData {
        classic_schedule_items: vec![item],
        media_by_key: BTreeMap::from([(
            key,
            (1..=7)
                .map(|id| SchedulingMediaItem::new(id, Duration::minutes(5)))
                .collect(),
        )]),
        ..SchedulingReferenceData::default()
    };
    let start =
        Utc.with_ymd_and_hms(2026, 4, 15, 23, 48, 0).unwrap() + Duration::nanoseconds(123_456_700);
    SchedulingRequest {
        playout: Playout {
            id: 71,
            seed: 42,
            schedule_kind: PlayoutScheduleKind::Classic,
            ..Playout::default()
        },
        start,
        finish: start + Duration::hours(2),
        mode: PlayoutBuildMode::Reset,
        reference_data,
    }
}

pub(super) fn published_request(
    original: &SchedulingRequest,
    result: &SchedulingResult,
) -> SchedulingRequest {
    let mut request = original.clone();
    request.playout.anchor = result.anchor.clone();
    request.reference_data.existing_items = result.added_items.clone();
    request.reference_data.existing_history = result.added_history.clone();
    request.reference_data.existing_classic_checkpoints = result.classic_checkpoints.clone();
    request.reference_data.existing_program_schedule_anchors =
        result.program_schedule_anchors.clone();
    request.reference_data.existing_fill_group_indices = result.fill_group_indices.clone();
    request.reference_data.preservation = PlayoutPreservation::KeepCurrent;
    request.mode = PlayoutBuildMode::Refresh;
    request
}

#[test]
fn classic_preserving_refresh_keeps_rolls_and_reconciles_changed_shuffle_membership() {
    let mut original = request(PlayoutMode::One, PlaybackOrder::Shuffle, false);
    for (id, kind) in [(91, FillerKind::PreRoll), (92, FillerKind::PostRoll)] {
        let mut preset = tests::filler_preset(id, kind, FillerMode::Count, Some(1), None);
        preset.media_items = vec![SchedulingMediaItem::new(id, Duration::seconds(30))];
        original
            .reference_data
            .classic_filler_presets
            .insert(id, preset);
    }
    let item = &mut original.reference_data.classic_schedule_items[0];
    item.pre_roll_filler_id = Some(91);
    item.post_roll_filler_id = Some(92);
    let key = item_key(item);
    let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    let mut refresh = published_request(&original, &full);
    refresh.start += Duration::minutes(7);
    let unchanged = ClassicSchedulingEngine.schedule(refresh.clone()).unwrap();
    let seam = unchanged.retain_through.unwrap();
    assert_eq!(seam, original.start + Duration::minutes(12));
    assert_eq!(
        unchanged.added_items,
        full.added_items
            .iter()
            .filter(|item| item.start >= seam)
            .cloned()
            .collect::<Vec<_>>()
    );
    let programmes = full
        .added_items
        .iter()
        .filter(|item| item.media_item_id < 91)
        .collect::<Vec<_>>();
    let consumed = BTreeSet::from([programmes[0].media_item_id, programmes[1].media_item_id]);
    let removed = programmes[2].media_item_id;
    let members = refresh.reference_data.media_by_key.get_mut(&key).unwrap();
    members.retain(|item| item.id != removed);
    members.push(SchedulingMediaItem::new(999, Duration::minutes(5)));
    let changed = ClassicSchedulingEngine.schedule(refresh).unwrap();
    let first_cycle = changed
        .added_items
        .iter()
        .filter(|item| ![91, 92].contains(&item.media_item_id))
        .take(5)
        .map(|item| item.media_item_id)
        .collect::<Vec<_>>();
    assert!(first_cycle.contains(&999));
    assert!(!first_cycle.contains(&removed));
    assert!(first_cycle.iter().all(|id| !consumed.contains(id)));
}

#[test]
fn classic_preserving_refresh_matches_uninterrupted_rotation_at_programme_boundaries() {
    for mode in [
        PlayoutMode::One,
        PlayoutMode::Multiple,
        PlayoutMode::Duration,
        PlayoutMode::Flood,
    ] {
        for order in [
            PlaybackOrder::Chronological,
            PlaybackOrder::Shuffle,
            PlaybackOrder::Random,
            PlaybackOrder::RandomRotation,
        ] {
            for grouped in [false, true] {
                let original = request(mode, order, grouped);
                let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
                assert!(
                    full.added_items
                        .windows(2)
                        .all(|pair| pair[0].finish <= pair[1].start),
                    "overlapping generated intervals: {:?}",
                    full.added_items
                        .windows(2)
                        .filter(|pair| pair[0].finish > pair[1].start)
                        .map(|pair| (pair[0].start, pair[0].finish, pair[1].start, pair[1].finish))
                        .collect::<Vec<_>>()
                );
                let mut refresh = published_request(&original, &full);
                for clock in [
                    original.start + Duration::minutes(6),
                    original.start + Duration::minutes(26),
                ] {
                    refresh.start = clock;
                    let rebuilt = ClassicSchedulingEngine
                        .schedule(refresh.clone())
                        .unwrap_or_else(|error| {
                            panic!("{mode:?}, {order:?}, {grouped}, {clock}: {error}")
                        });
                    let seam = rebuilt.retain_through.unwrap();
                    assert!(seam > clock);
                    let expected = full
                        .added_items
                        .iter()
                        .filter(|item| item.start >= seam)
                        .cloned()
                        .collect::<Vec<_>>();
                    assert_eq!(
                        rebuilt.added_items, expected,
                        "{mode:?}, {order:?}, {grouped}, {clock}"
                    );
                    assert_eq!(
                        rebuilt.anchor, full.anchor,
                        "{mode:?}, {order:?}, {grouped}, {clock}"
                    );
                    assert_eq!(rebuilt.remove_before, Some(clock - Duration::hours(4)));
                }
            }
        }
    }
}

#[test]
fn classic_explicit_reset_restarts_fill_groups_after_the_preserved_programme() {
    for fill_mode in [
        FillWithGroupMode::FillWithOrderedGroups,
        FillWithGroupMode::FillWithShuffledGroups,
    ] {
        let mut original = request(PlayoutMode::One, PlaybackOrder::Chronological, false);
        original.reference_data.classic_schedule_items[0].fill_with_group_mode = fill_mode;
        let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
        let mut reset = published_request(&original, &full);
        reset.start += Duration::minutes(6);
        reset.mode = PlayoutBuildMode::Reset;
        reset.playout.seed = 127;
        let result = ClassicSchedulingEngine.schedule(reset).unwrap();
        let mut fresh = original;
        fresh.start = result.retain_through.unwrap();
        fresh.playout.seed = 127;
        let expected = ClassicSchedulingEngine.schedule(fresh).unwrap();
        assert_eq!(result.fill_group_indices, expected.fill_group_indices);
    }
}

#[test]
fn classic_preserving_reset_keeps_boundary_and_starts_fresh_rotation() {
    let original = request(PlayoutMode::One, PlaybackOrder::Chronological, false);
    let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    let mut reset = published_request(&original, &full);
    reset.start += Duration::minutes(6);
    reset.mode = PlayoutBuildMode::Reset;
    let result = ClassicSchedulingEngine.schedule(reset).unwrap();
    assert_eq!(result.retain_through, Some(full.added_items[1].finish));
    assert_eq!(
        result.added_items[0].media_item_id,
        full.added_items[0].media_item_id
    );
    assert_ne!(
        result.added_items[0].guide_group,
        full.added_items[1].guide_group
    );
}

#[test]
fn classic_preserving_refresh_refuses_a_missing_programme_checkpoint() {
    let original = request(PlayoutMode::One, PlaybackOrder::Chronological, false);
    let full = ClassicSchedulingEngine.schedule(original.clone()).unwrap();
    let mut refresh = published_request(&original, &full);
    refresh.start += Duration::minutes(6);
    refresh
        .reference_data
        .existing_classic_checkpoints
        .retain(|checkpoint| checkpoint.checkpoint_date == original.start);
    let error = ClassicSchedulingEngine.schedule(refresh).unwrap_err();
    assert!(error.to_string().contains("checkpoint"));
}
