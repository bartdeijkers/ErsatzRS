use super::*;

#[test]
fn playlist_history_restores_cycle_batches_and_all_children() {
    for order in [PlaybackOrder::Chronological, PlaybackOrder::None] {
        for cutoff in [1, 7, 11, 17] {
            let mut items = vec![
                child(1, &[10, 11, 12, 13], false),
                child(2, &[20, 21, 22], true),
                child(3, &[30, 31, 32, 33], false),
            ];
            items[0].count = Some(2);
            for item in &mut items {
                item.playback_order = order;
            }
            let mut uninterrupted = PlaylistEnumerator::new(items.clone());
            for _ in 0..cutoff - 1 {
                uninterrupted.advance();
            }
            let mut history = vec![PlayoutHistory {
                index: i32::try_from(uninterrupted.index).unwrap(),
                ..PlayoutHistory::default()
            }];
            history.extend(
                uninterrupted
                    .children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| PlayoutHistory {
                        child_key: Some(history_key_for_collection_key(&child.collection_key)),
                        index: i32::try_from(child.enumerator.current_index()).unwrap(),
                        is_current_child: index == uninterrupted.child_index,
                        details: history_details_for_media_item(
                            child.enumerator.current().unwrap(),
                        ),
                        ..PlayoutHistory::default()
                    }),
            );
            uninterrupted.advance();
            let mut restored = PlaylistEnumerator::new(items);
            restored.apply_history(&history);
            for _ in 0..24 {
                assert_eq!(
                    restored.current(),
                    uninterrupted.current(),
                    "{order:?}, cutoff={cutoff}"
                );
                assert_eq!(restored.index, uninterrupted.index);
                assert_eq!(restored.child_index, uninterrupted.child_index);
                assert_eq!(
                    restored.remaining_media_ids,
                    uninterrupted.remaining_media_ids
                );
                restored.advance();
                uninterrupted.advance();
            }
        }
    }
}

fn child(id: i64, ids: &[i64], play_all: bool) -> SchedulingPlaylistItem {
    SchedulingPlaylistItem {
        collection_key: SchedulingCollectionKey::fake_playlist_item(id),
        playback_order: PlaybackOrder::Chronological,
        play_all,
        count: None,
        media_items: ids
            .iter()
            .map(|id| SchedulingMediaItem::new(*id, Duration::minutes(10)))
            .collect(),
    }
}

#[test]
fn playlist_history_restores_shuffled_cycles() {
    for order in [PlaybackOrder::Chronological, PlaybackOrder::Shuffle] {
        for sizes in [vec![2; 6], vec![2, 3, 2, 4]] {
            let items = sizes
                .iter()
                .enumerate()
                .map(|(index, size)| {
                    let id = i64::try_from(index).unwrap() + 1;
                    let mut item = child(
                        id,
                        &(0..*size)
                            .map(|offset| id * 100 + offset)
                            .collect::<Vec<_>>(),
                        false,
                    );
                    item.playback_order = order;
                    item
                })
                .collect::<Vec<_>>();
            let mut live = PlaylistEnumerator::new(items.clone()).with_child_shuffle_seed(6789);
            live.shuffle_groups(12345);
            for _ in 0..1000 {
                if live.cycle >= 2 && live.index >= 5 {
                    break;
                }
                live.advance();
            }
            assert!(live.cycle >= 2 && live.index >= 5);
            let history = live.history_for_item(1, "playlist", &PlayoutItem::default());
            live.advance();
            let mut restored = PlaylistEnumerator::new(items).with_child_shuffle_seed(6789);
            restored.shuffle_groups(12345);
            restored.apply_history(&history);
            for position in 0..24 {
                assert_eq!(
                    restored.current(),
                    live.current(),
                    "sizes={sizes:?}, position={position}"
                );
                restored.advance();
                live.advance();
            }
        }
    }
}

#[test]
fn playlist_cycle_waits_for_every_playable_item() {
    let mut playlist = PlaylistEnumerator::new(vec![
        child(1, &[10], false),
        child(2, &[20, 21], true),
        child(3, &[30, 31], false),
    ]);
    let mut played = Vec::new();
    for position in 0..8 {
        played.push(playlist.current().expect("playlist item").id);
        playlist.advance();
        if position < 7 {
            assert_ne!(playlist.index, 0, "cycle ended before all media played");
        }
    }
    assert_eq!(played, [10, 20, 21, 30, 10, 20, 21, 31]);
    assert_eq!(playlist.index, 0);
}

#[test]
fn playlist_cycle_excludes_unplayable_specials_and_empty_children() {
    let episode = |id, season| SchedulingMediaItem {
        show_id: Some(1),
        season: Some(season),
        episode: Some(1),
        ..SchedulingMediaItem::new(id, Duration::minutes(10))
    };
    let mut regular = child(1, &[], true);
    regular.playback_order = PlaybackOrder::SeasonEpisode;
    regular.media_items = vec![episode(10, 1), episode(11, 0), episode(12, 2)];
    let mut specials = child(2, &[], true);
    specials.playback_order = PlaybackOrder::SeasonEpisode;
    specials.media_items = vec![episode(20, 0)];
    let mut playlist = PlaylistEnumerator::new(vec![regular, specials]);
    assert_eq!(playlist.unique_media_len(), 2);
    assert_eq!(playlist.children.len(), 1);
    assert_eq!(playlist.current().expect("first regular episode").id, 10);
    playlist.advance();
    assert_eq!(playlist.current().expect("second regular episode").id, 12);
    playlist.advance();
    assert_eq!(playlist.index, 0);
}

#[test]
fn shuffled_playlist_rebuild_matches_after_multiple_cycles() {
    let items = (1..=4).map(|id| child(id, &[id], true)).collect::<Vec<_>>();
    let mut live = PlaylistEnumerator::new(items.clone());
    live.shuffle_groups(12345);
    for _ in 0..14 {
        live.advance();
    }
    let seed = live.shuffle_seed.expect("shuffle seed");
    assert_ne!(seed, 12345, "completed cycles must select a new order");
    let mut restored = PlaylistEnumerator::new(items);
    restored.shuffle_groups(seed);
    restored.replay_to(live.index);
    for _ in 0..12 {
        assert_eq!(restored.current(), live.current());
        assert_eq!(restored.shuffle_seed, live.shuffle_seed);
        assert_eq!(restored.remaining_media_ids, live.remaining_media_ids);
        restored.advance();
        live.advance();
    }
}

#[test]
fn repeated_playlist_source_shares_child_progress() {
    let mut playlist = PlaylistEnumerator::new(vec![
        child(1, &[10, 11], false),
        child(1, &[10, 11], false),
        child(2, &[20], false),
    ]);
    let played = (0..6)
        .map(|_| {
            let id = playlist.current().unwrap().id;
            playlist.advance();
            id
        })
        .collect::<Vec<_>>();
    assert_eq!(played, [10, 11, 20, 10, 11, 20]);
}

#[test]
fn repeated_playlist_history_keeps_current_slot_and_batch_count() {
    let mut first = child(1, &[10, 11, 12, 13], false);
    first.count = Some(2);
    let mut second = first.clone();
    second.count = Some(3);
    let items = vec![first, second, child(2, &[20, 21], false)];
    let mut live = PlaylistEnumerator::new(items.clone());
    for _ in 0..3 {
        live.advance();
    }
    assert_eq!(live.child_index, 1);
    let history = live.history_for_item(1, "repeated", &PlayoutItem::default());
    live.advance();
    let mut restored = PlaylistEnumerator::new(items);
    restored.apply_history(&history);
    for _ in 0..20 {
        assert_eq!(restored.current(), live.current());
        assert_eq!(restored.child_index, live.child_index);
        restored.advance();
        live.advance();
    }
}

#[test]
fn shuffled_equal_and_empty_playlists_remain_bounded() {
    let item = child(1, &[10, 11], false);
    let mut playlist = PlaylistEnumerator::new(vec![item; 3]);
    playlist.shuffle_groups(12345);
    for _ in 0..100 {
        assert!(playlist.current().is_some());
        assert_eq!(
            playlist.current(),
            playlist.children[playlist.child_index].enumerator.current()
        );
        playlist.advance();
    }
    let mut empty = PlaylistEnumerator::new(vec![child(1, &[], true)]);
    empty.shuffle_groups(12345);
    empty.advance();
    empty.replay_to(usize::MAX);
    assert!(empty.current().is_none());
}

#[test]
fn shuffled_children_avoid_boundary_repeats_and_allow_equal_ids() {
    for ids in [vec![1, 2, 3], vec![1, 1, 1]] {
        let mut item = child(1, &ids, true);
        item.playback_order = PlaybackOrder::Shuffle;
        let mut playlist = PlaylistEnumerator::new(vec![item]).with_child_shuffle_seed(12345);
        for _ in 0..60 {
            let previous_id = playlist.current().unwrap().id;
            playlist.advance();
            if ids.len() == ids.iter().collect::<BTreeSet<_>>().len() {
                assert_ne!(playlist.current().unwrap().id, previous_id);
            }
        }
    }
}

#[test]
fn history_position_clamps_removed_items_and_excludes_specials() {
    let mut definition = child(1, &[1, 2, 3], true);
    definition.playback_order = PlaybackOrder::SeasonEpisode;
    for (season, item) in definition.media_items.iter_mut().enumerate() {
        item.show_id = Some(1);
        item.season = Some(i32::try_from(season).unwrap());
        item.episode = Some(1);
    }
    let mut playlist = PlaylistEnumerator::new(vec![definition]);
    let enumerator = &mut playlist.children[0].enumerator;
    assert_eq!(enumerator.len(), 2);
    enumerator
        .move_to_history_details(r#"{"MediaItemId":999,"SeasonNumber":99,"EpisodeNumber":1}"#);
    assert_eq!(enumerator.current().unwrap().id, 2);
    enumerator.move_to_history_details(r#"{"MediaItemId":999,"SeasonNumber":2,"EpisodeNumber":1}"#);
    assert_eq!(enumerator.current().unwrap().id, 3);
    enumerator.set_index(usize::MAX);
    assert!(enumerator.current().is_some());
}

#[test]
fn marathon_deduplicates_overlapping_sources_and_handles_missing_album() {
    let first = SchedulingMediaItem::new(1, Duration::minutes(1));
    let second = SchedulingMediaItem::new(2, Duration::minutes(1));
    let groups = group_marathon_media(
        vec![first.clone(), second, first],
        "ALBUM",
        PlaybackOrder::Chronological,
        false,
        None,
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0]
            .media_items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert!(!valid_marathon_group_by("invalid"));
    assert!(valid_marathon_group_by("DiReCtOr"));
}

#[test]
fn scripted_marathon_rejects_invalid_group_before_scheduling() {
    let start = DateTime::from_timestamp(0, 0).unwrap();
    let request = SchedulingRequest {
        playout: ersatzrs_domain::Playout::default(),
        start,
        finish: start + Duration::minutes(1),
        mode: PlayoutBuildMode::Reset,
        reference_data: SchedulingReferenceData::default(),
    };
    let error = ScriptedSchedulingEngine
        .schedule_script("add_marathon('a', {}, {}, 'invalid')", request)
        .unwrap_err();
    assert!(error.to_string().contains("Invalid marathon group_by"));
}
