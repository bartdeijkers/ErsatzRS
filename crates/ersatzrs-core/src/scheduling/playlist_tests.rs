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
