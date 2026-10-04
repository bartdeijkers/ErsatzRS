use super::tests::{randomized_reference, schedule, utc};
use super::*;
use crate::scheduling::{PlayoutBuildMode, PlayoutPreservation, SchedulingReferenceData};
use ersatzrs_domain::{Playout, PlayoutScheduleKind};

#[test]
fn block_preservation_retains_prefetched_blocks_and_restores_their_boundary() {
    let start = utc("2026-04-15T23:40:00.123Z");
    let reference = randomized_reference(start, PlaybackOrder::Shuffle);
    let full = schedule(reference.clone(), start, Duration::hours(6), 100, 127);
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let mut stored = reference.clone();
        publish(&mut stored, &full);
        stored.preservation = PlayoutPreservation::KeepCurrent;
        stored.preservation_not_before = Some(full.added_items[2].start + Duration::seconds(1));
        let result = BlockSchedulingEngine
            .schedule(request(stored, start + Duration::seconds(1), mode))
            .unwrap();
        assert_eq!(result.retain_through, full.added_items[2].guide_finish);
        assert_eq!(result.added_items[0].start, result.retain_through.unwrap());
        if mode == PlayoutBuildMode::Refresh {
            assert_eq!(result.added_items, full.added_items[3..]);
        }
    }
}

fn publish(reference: &mut SchedulingReferenceData, result: &SchedulingResult) {
    if result.clear_items {
        reference.existing_items.retain(|item| {
            result
                .retain_through
                .is_some_and(|seam| item.finish <= seam)
        });
        reference.existing_history.retain(|entry| {
            result.retain_through.is_some_and(|seam| {
                entry.when < seam && entry.finish.is_none_or(|finish| finish <= seam)
            })
        });
    }
    reference.existing_items.extend(result.added_items.clone());
    reference
        .existing_history
        .extend(result.added_history.clone());
    for (index, item) in reference.existing_items.iter_mut().enumerate() {
        item.id = i64::try_from(index + 1).unwrap();
    }
    for (index, entry) in reference.existing_history.iter_mut().enumerate() {
        entry.id = i64::try_from(index + 1).unwrap();
    }
}

fn request(
    reference_data: SchedulingReferenceData,
    now: DateTime<Utc>,
    mode: PlayoutBuildMode,
) -> SchedulingRequest {
    SchedulingRequest {
        playout: Playout {
            id: 100,
            seed: 127,
            schedule_kind: PlayoutScheduleKind::Block,
            ..Playout::default()
        },
        start: now,
        finish: now + Duration::hours(6),
        mode,
        reference_data,
    }
}

#[test]
fn block_preserving_refresh_matches_each_order_at_reused_guide_groups() {
    let start = utc("2026-04-15T23:40:00.123Z");
    for order in [
        PlaybackOrder::Chronological,
        PlaybackOrder::SeasonEpisode,
        PlaybackOrder::Shuffle,
        PlaybackOrder::Random,
        PlaybackOrder::RandomRotation,
    ] {
        let reference = randomized_reference(start, order);
        let uninterrupted = schedule(reference.clone(), start, Duration::hours(6), 100, 127);
        for index in [0, 2, 5] {
            let mut stored = reference.clone();
            publish(&mut stored, &uninterrupted);
            stored.preservation = PlayoutPreservation::KeepCurrent;
            let now = uninterrupted.added_items[index].start + Duration::milliseconds(1234);
            let result = BlockSchedulingEngine
                .schedule(request(stored, now, PlayoutBuildMode::Refresh))
                .unwrap();
            assert!(result.clear_items);
            assert_eq!(
                result.retain_through,
                uninterrupted.added_items[index].guide_finish
            );
            assert_eq!(
                result.added_items,
                uninterrupted.added_items[index + 1..],
                "{order:?}, index {index}"
            );
            assert!(result.items_to_remove.is_empty());
        }
    }
}

#[test]
fn block_preserving_reset_keeps_prefix_and_future_refresh_uses_new_rotation() {
    let start = utc("2026-04-15T23:40:00.123Z");
    for order in [
        PlaybackOrder::Chronological,
        PlaybackOrder::Shuffle,
        PlaybackOrder::RandomRotation,
    ] {
        let reference = randomized_reference(start, order);
        let original = schedule(reference.clone(), start, Duration::hours(6), 100, 127);
        let seam = original.added_items[2].finish;
        let mut stored = reference.clone();
        publish(&mut stored, &original);
        stored.preservation = PlayoutPreservation::KeepCurrent;
        let mut reset_request = request(
            stored.clone(),
            original.added_items[2].start + Duration::seconds(1),
            PlayoutBuildMode::Reset,
        );
        reset_request.playout.seed = -953;
        let reset = BlockSchedulingEngine.schedule(reset_request).unwrap();
        let mut fresh = reference.clone();
        fresh.effective_blocks.retain(|block| block.start >= seam);
        let expected = schedule(fresh, seam, Duration::hours(6), 100, -953);
        assert_eq!(reset.added_items, expected.added_items, "{order:?}");
        assert_eq!(reset.retain_through, Some(seam));
        publish(&mut stored, &reset);
        let mut refresh = request(
            stored,
            reset.added_items[0].start + Duration::seconds(1),
            PlayoutBuildMode::Refresh,
        );
        refresh.playout.seed = -953;
        refresh.playout.anchor = reset.anchor;
        let rebuilt = BlockSchedulingEngine.schedule(refresh).unwrap();
        assert_eq!(rebuilt.added_items, expected.added_items[1..], "{order:?}");
    }
}

#[test]
fn block_preserving_refresh_refuses_legacy_history_but_reset_recovers() {
    let start = utc("2026-04-15T23:40:00Z");
    let mut reference = randomized_reference(start, PlaybackOrder::Shuffle);
    let first = schedule(reference.clone(), start, Duration::hours(6), 100, 127);
    publish(&mut reference, &first);
    reference.existing_history[0].details = "{}".into();
    reference.preservation = PlayoutPreservation::KeepCurrent;
    let now = start + Duration::minutes(1);
    assert!(BlockSchedulingEngine
        .schedule(request(reference.clone(), now, PlayoutBuildMode::Refresh))
        .unwrap_err()
        .to_string()
        .contains("missing history checkpoint"));
    let reset = BlockSchedulingEngine
        .schedule(request(reference, now, PlayoutBuildMode::Reset))
        .unwrap();
    assert_eq!(reset.retain_through, first.added_items[0].guide_finish);
}

#[test]
fn block_preserving_refresh_retains_start_break_and_filler_remainder() {
    let start = utc("2026-04-15T23:50:00Z");
    let mut reference = super::tests::filler_reference_data(start);
    reference.effective_blocks[0].duration = Duration::minutes(20);
    reference.media_by_key.insert(
        "roll".into(),
        vec![SchedulingMediaItem::new(91, Duration::minutes(2))],
    );
    reference.effective_blocks[0]
        .deco
        .as_mut()
        .unwrap()
        .break_content
        .push(EffectiveBreakContent {
            id: 81,
            placement: DecoBreakPlacement::BlockStart,
            content: "roll".into(),
            collection_key: SchedulingCollectionKey {
                collection_type: CollectionType::Collection,
                collection_id: Some(81),
                ..SchedulingCollectionKey::default()
            },
        });
    let initial = schedule(reference.clone(), start, Duration::hours(1), 100, 127);
    assert_eq!(initial.added_items[0].media_item_id, 91);
    for minutes in [1, 5, 21] {
        let mut stored = reference.clone();
        publish(&mut stored, &initial);
        stored.preservation = PlayoutPreservation::KeepCurrent;
        let rebuilt = BlockSchedulingEngine
            .schedule(request(
                stored,
                start + Duration::minutes(minutes),
                PlayoutBuildMode::Refresh,
            ))
            .unwrap();
        let seam = start + Duration::minutes(25);
        assert_eq!(rebuilt.retain_through, Some(seam));
        assert_eq!(
            rebuilt.added_items,
            initial
                .added_items
                .iter()
                .filter(|item| item.start >= seam)
                .cloned()
                .collect::<Vec<_>>()
        );
        assert!(rebuilt.items_to_remove.is_empty());
    }
}

#[test]
fn block_explicit_replacement_starts_at_clock_and_keep_refuses_ambiguous_timeline() {
    let start = utc("2026-04-15T23:40:00Z");
    let mut reference = randomized_reference(start, PlaybackOrder::Chronological);
    // Replacement must fit the remainder of the fixed block; BeforeDurationEnd
    // correctly defers an oversized programme to the next configured block.
    for item in reference.media_by_key.get_mut("series").unwrap() {
        item.duration = Duration::minutes(10);
    }
    let initial = schedule(reference.clone(), start, Duration::hours(6), 100, 127);
    publish(&mut reference, &initial);
    let now = start + Duration::minutes(1);
    reference.existing_items[1].start = now;
    reference.preservation = PlayoutPreservation::KeepCurrent;
    assert!(BlockSchedulingEngine
        .schedule(request(reference.clone(), now, PlayoutBuildMode::Reset))
        .unwrap_err()
        .to_string()
        .contains("overlapping"));

    reference.preservation = PlayoutPreservation::ReplaceCurrent;
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let result = BlockSchedulingEngine
            .schedule(request(reference.clone(), now, mode))
            .unwrap();
        assert!(result.clear_items);
        assert_eq!(result.retain_through, None);
        assert_eq!(result.added_items[0].start, now);
        assert!(result
            .added_items
            .windows(2)
            .all(|pair| pair[0].finish <= pair[1].start));
    }
}

#[test]
fn block_incompatible_anchor_is_refused_and_explicit_reset_recovers() {
    let start = utc("2026-04-15T23:40:00Z");
    let mut reference = randomized_reference(start, PlaybackOrder::Shuffle);
    let initial = schedule(reference.clone(), start, Duration::hours(6), 100, 127);
    publish(&mut reference, &initial);
    reference.preservation = PlayoutPreservation::KeepCurrent;
    for context in [
        "not json",
        r#"{"version":99}"#,
        r#"{"smart_shuffle":{"version":99,"recent":[]}}"#,
    ] {
        let mut refresh = request(
            reference.clone(),
            start + Duration::minutes(1),
            PlayoutBuildMode::Refresh,
        );
        refresh.playout.anchor = Some(ersatzrs_domain::PlayoutAnchor {
            context: Some(context.into()),
            ..ersatzrs_domain::PlayoutAnchor::default()
        });
        assert!(BlockSchedulingEngine
            .schedule(refresh.clone())
            .unwrap_err()
            .to_string()
            .contains("anchor"));
        refresh.mode = PlayoutBuildMode::Reset;
        let reset = BlockSchedulingEngine.schedule(refresh).unwrap();
        assert_eq!(reset.retain_through, initial.added_items[0].guide_finish);
    }
}
