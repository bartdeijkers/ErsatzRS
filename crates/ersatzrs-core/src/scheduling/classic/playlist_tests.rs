use super::*;
use crate::scheduling::{SchedulingCollectionKey, SchedulingPlaylistItem};
use chrono::TimeZone;
use ersatzrs_domain::{CollectionType, Playout, PlayoutMode};

fn fixture(shuffle: bool) -> (ProgramScheduleItem, SchedulingReferenceData) {
    let item = ProgramScheduleItem {
        id: 1,
        program_schedule_id: 1,
        collection_type: CollectionType::Playlist,
        playlist_id: Some(99),
        playout_mode: PlayoutMode::Flood,
        marathon_shuffle_groups: shuffle,
        ..ProgramScheduleItem::default()
    };
    let children = [
        (1, vec![10], false),
        (2, vec![20, 21], true),
        (3, vec![30, 31], false),
    ]
    .into_iter()
    .map(|(id, ids, play_all)| SchedulingPlaylistItem {
        collection_key: SchedulingCollectionKey::fake_playlist_item(id),
        playback_order: PlaybackOrder::Chronological,
        play_all,
        count: None,
        media_items: ids
            .into_iter()
            .map(|id| SchedulingMediaItem {
                preferred_audio_language_code: Some("nld".to_owned()),
                preferred_subtitle_language_code: Some("eng".to_owned()),
                ..SchedulingMediaItem::new(id, Duration::minutes(10))
            })
            .collect(),
    })
    .collect();
    let data = SchedulingReferenceData {
        classic_schedule_items: vec![item.clone()],
        playlist_items_by_key: BTreeMap::from([(item_key(&item), children)]),
        ..SchedulingReferenceData::default()
    };
    (item, data)
}

fn build(
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    data: SchedulingReferenceData,
    anchor: Option<PlayoutAnchor>,
) -> SchedulingResult {
    ClassicSchedulingEngine
        .schedule(SchedulingRequest {
            playout: Playout {
                id: 50,
                seed: 12345,
                schedule_kind: ersatzrs_domain::PlayoutScheduleKind::Classic,
                anchor,
                ..Playout::default()
            },
            start,
            finish,
            mode: PlayoutBuildMode::Continue,
            reference_data: data,
        })
        .expect("playlist build")
}

#[test]
fn classic_playlist_continue_matches_uninterrupted_across_cycles() {
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
    for (shuffle, shuffle_children) in [(false, false), (true, false), (false, true), (true, true)]
    {
        let (_, mut data) = fixture(shuffle);
        if shuffle_children {
            for child in data.playlist_items_by_key.values_mut().flatten() {
                child.playback_order = PlaybackOrder::Shuffle;
            }
        }
        let full = build(start, start + Duration::hours(8), data.clone(), None);
        for split in [4, 9, 17, 31] {
            let seam = start + Duration::minutes(split * 10);
            let first = build(start, seam, data.clone(), None);
            let mut continued_data = data.clone();
            continued_data.existing_items = first.added_items.clone();
            continued_data.existing_program_schedule_anchors = first.program_schedule_anchors;
            continued_data.existing_classic_checkpoints = first.classic_checkpoints;
            let continued = build(
                seam,
                start + Duration::hours(8),
                continued_data,
                first.anchor,
            );
            assert_eq!(
                continued.added_items,
                full.added_items[first.added_items.len()..],
                "shuffle={shuffle}, split={split}"
            );
        }
    }
}

#[test]
fn classic_distinct_rerun_collections_keep_separate_progress() {
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 23, 50, 0).unwrap();
    let mut data = SchedulingReferenceData::default();
    for id in [1, 2] {
        let item = ProgramScheduleItem {
            id,
            program_schedule_id: 1,
            sort_order: i32::try_from(id).unwrap(),
            collection_type: CollectionType::RerunFirstRun,
            rerun_collection_id: Some(id),
            playout_mode: PlayoutMode::One,
            ..ProgramScheduleItem::default()
        };
        data.media_by_key.insert(
            item_key(&item),
            vec![
                SchedulingMediaItem::new(id * 10, Duration::minutes(10)),
                SchedulingMediaItem::new(id * 10 + 1, Duration::minutes(10)),
            ],
        );
        data.classic_schedule_items.push(item);
    }
    let full = build(start, start + Duration::minutes(40), data.clone(), None);
    assert_eq!(
        full.added_items
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>(),
        [10, 20, 11, 21]
    );
    let first = build(start, start + Duration::minutes(20), data.clone(), None);
    data.existing_items = first.added_items;
    data.existing_program_schedule_anchors = first.program_schedule_anchors;
    data.existing_classic_checkpoints = first.classic_checkpoints;
    let continued = build(
        start + Duration::minutes(20),
        start + Duration::minutes(40),
        data,
        first.anchor,
    );
    assert_eq!(continued.added_items, full.added_items[2..]);
    let rerun_ids = continued
        .program_schedule_anchors
        .iter()
        .filter(|anchor| anchor.anchor_date.is_none())
        .map(|anchor| anchor.rerun_collection_id)
        .collect::<BTreeSet<_>>();
    assert_eq!(rerun_ids, BTreeSet::from([Some(1), Some(2)]));
}

#[test]
fn classic_playlist_random_start_continue_matches_uninterrupted() {
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
    let (_, mut data) = fixture(true);
    data.classic_random_start_point = true;
    let full = build(start, start + Duration::hours(8), data.clone(), None);
    for split in [4, 9, 17, 31] {
        let first = build(
            start,
            start + Duration::minutes(split * 10),
            data.clone(),
            None,
        );
        let mut next = data.clone();
        next.existing_items = first.added_items.clone();
        next.existing_program_schedule_anchors = first.program_schedule_anchors;
        let continued = build(
            start + Duration::minutes(split * 10),
            start + Duration::hours(8),
            next,
            first.anchor,
        );
        assert_eq!(
            continued.added_items,
            full.added_items[first.added_items.len()..],
            "split={split}"
        );
    }
}

#[test]
fn classic_playlist_duration_rejection_keeps_next_content_across_continue() {
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap();
    let (mut item, mut data) = fixture(false);
    item.playout_mode = PlayoutMode::Duration;
    item.duration = Some(Duration::minutes(15));
    let bumper = ProgramScheduleItem {
        id: 2,
        program_schedule_id: 1,
        collection_type: CollectionType::Collection,
        collection_id: Some(5),
        playout_mode: PlayoutMode::One,
        sort_order: 1,
        ..ProgramScheduleItem::default()
    };
    data.media_by_key.insert(
        item_key(&bumper),
        vec![SchedulingMediaItem::new(100, Duration::minutes(5))],
    );
    data.classic_schedule_items = vec![item, bumper];
    let full = build(start, start + Duration::minutes(30), data.clone(), None);
    assert_eq!(
        full.added_items
            .iter()
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>(),
        [10, 100, 20]
    );
    let first = build(start, start + Duration::minutes(20), data.clone(), None);
    data.existing_items = first.added_items;
    data.existing_program_schedule_anchors = first.program_schedule_anchors;
    let continued = build(
        start + Duration::minutes(20),
        start + Duration::minutes(30),
        data,
        first.anchor,
    );
    assert_eq!(continued.added_items, full.added_items[2..]);
}

#[test]
fn classic_playlist_refresh_restores_cycle_and_rejects_legacy_cycle_state() {
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 23, 30, 0).unwrap();
    let finish = start + Duration::hours(3);
    for shuffle in [false, true] {
        let (_, mut data) = fixture(shuffle);
        let full = build(start, finish, data.clone(), None);
        data.existing_items = full.added_items.clone();
        data.existing_program_schedule_anchors = full.program_schedule_anchors;
        data.existing_classic_checkpoints = full.classic_checkpoints;
        let request = SchedulingRequest {
            playout: Playout {
                id: 50,
                seed: 12345,
                schedule_kind: ersatzrs_domain::PlayoutScheduleKind::Classic,
                anchor: full.anchor,
                ..Playout::default()
            },
            start: start + Duration::minutes(40),
            finish,
            mode: PlayoutBuildMode::Refresh,
            reference_data: data.clone(),
        };
        let refreshed = ClassicSchedulingEngine
            .schedule(request.clone())
            .expect("playlist Refresh");
        let seam = refreshed.retain_through.expect("retained checkpoint seam");
        let expected = full
            .added_items
            .into_iter()
            .filter(|item| item.start >= seam)
            .collect::<Vec<_>>();
        assert_eq!(refreshed.added_items, expected, "shuffle={shuffle}");

        for checkpoint in &mut data.existing_classic_checkpoints {
            let mut json: serde_json::Value = serde_json::from_str(&checkpoint.context).unwrap();
            for source in json["Sources"].as_object_mut().unwrap().values_mut() {
                source["Playlist"]
                    .as_object_mut()
                    .unwrap()
                    .remove("RemainingMediaIds");
            }
            checkpoint.context = json.to_string();
        }
        let audit = audit_classic_refresh_checkpoint(&data, 50, request.start);
        assert!(audit
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("playable-cycle state")));
    }
}

#[test]
fn classic_marathon_groups_and_batches_preserve_continue() {
    use ersatzrs_domain::MarathonGroupBy;
    let start = Utc.with_ymd_and_hms(2026, 4, 26, 23, 0, 0).unwrap();
    for group_by in [
        MarathonGroupBy::Show,
        MarathonGroupBy::Season,
        MarathonGroupBy::Artist,
        MarathonGroupBy::Album,
        MarathonGroupBy::Director,
    ] {
        for (batch, shuffle) in [
            (None, false),
            (Some(2), false),
            (None, true),
            (Some(2), true),
        ] {
            let item = ProgramScheduleItem {
                id: 1,
                program_schedule_id: 1,
                collection_type: CollectionType::Collection,
                collection_id: Some(1),
                playback_order: PlaybackOrder::Marathon,
                marathon_group_by: group_by,
                marathon_batch_size: batch,
                marathon_shuffle_groups: shuffle,
                marathon_shuffle_items: shuffle,
                playout_mode: PlayoutMode::Flood,
                ..ProgramScheduleItem::default()
            };
            let media = [11, 12, 13, 21, 22, 31]
                .into_iter()
                .map(|id| SchedulingMediaItem {
                    show_id: Some(id / 10),
                    season_id: Some(id / 10),
                    artist_id: Some(id / 10),
                    album: Some(format!("Album {}", id / 10)),
                    first_director: Some(format!("Director {}", id / 10)),
                    season: Some(1),
                    episode: Some((id % 10) as i32),
                    preferred_audio_language_code: Some("nld".into()),
                    preferred_subtitle_language_code: Some("eng".into()),
                    ..SchedulingMediaItem::new(id, Duration::minutes(10))
                })
                .collect();
            let data = SchedulingReferenceData {
                classic_random_start_point: true,
                classic_schedule_items: vec![item.clone()],
                media_by_key: BTreeMap::from([(item_key(&item), media)]),
                ..SchedulingReferenceData::default()
            };
            let full = build(start, start + Duration::hours(3), data.clone(), None);
            let first_six = full
                .added_items
                .iter()
                .take(6)
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>();
            if !shuffle {
                assert_eq!(
                    first_six,
                    if batch.is_some() {
                        vec![11, 12, 21, 22, 31, 31]
                    } else {
                        vec![11, 12, 13, 21, 22, 31]
                    },
                    "group={group_by:?}, batch={batch:?}"
                );
            }
            for split in [7, 11] {
                let first = build(
                    start,
                    start + Duration::minutes(split * 10),
                    data.clone(),
                    None,
                );
                let mut next = data.clone();
                next.existing_items = first.added_items.clone();
                next.existing_program_schedule_anchors = first.program_schedule_anchors;
                let continued = build(
                    start + Duration::minutes(split * 10),
                    start + Duration::hours(3),
                    next,
                    first.anchor,
                );
                assert_eq!(
                    continued.added_items,
                    full.added_items[first.added_items.len()..],
                    "group={group_by:?}, batch={batch:?}, split={split}"
                );
            }
        }
    }
}
