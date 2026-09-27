use super::*;
use ersatzrs_core::scheduling::{
    EffectiveBlock, EffectiveBreakContent, EffectiveDeco, SchedulingCollectionKey,
    SchedulingMediaItem, SchedulingPlaylistItem,
};
use ersatzrs_domain::{
    BlockStopScheduling, ChannelSubtitleMode, CollectionType, DecoBreakPlacement, PlaybackOrder,
};
use std::{collections::BTreeMap, fmt::Write as _};

fn block_fixture(start: chrono::DateTime<Utc>) -> SchedulingReferenceData {
    let mut reference = SchedulingReferenceData::default();
    let children = (0..3)
        .map(|child| SchedulingPlaylistItem {
            collection_key: SchedulingCollectionKey::fake_playlist_item(child),
            playback_order: PlaybackOrder::Chronological,
            play_all: false,
            count: Some(2),
            media_items: (0..4)
                .map(|index| {
                    let mut media =
                        SchedulingMediaItem::new(1 + child * 4 + index, Duration::minutes(1));
                    media.preferred_audio_language_code = Some("nld".to_owned());
                    media.preferred_subtitle_language_code = Some("eng".to_owned());
                    media.subtitle_mode = Some(ChannelSubtitleMode::Forced);
                    media
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    reference
        .playlist_items_by_key
        .insert("bumper".to_owned(), children);
    reference.effective_blocks = (0..18)
        .map(|index| EffectiveBlock {
            block_id: 10,
            block_key: None,
            template_item_id: 20,
            name: "Synthetic playlist block".to_owned(),
            start: start + Duration::minutes(index * 30),
            duration: Duration::minutes(30),
            stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
            items: Vec::new(),
            deco: Some(EffectiveDeco {
                id: 70,
                default_filler_content: None,
                default_filler_collection_key: None,
                default_filler_trim_to_fit: false,
                break_content: vec![EffectiveBreakContent {
                    id: 71,
                    placement: DecoBreakPlacement::BlockStart,
                    content: "bumper".to_owned(),
                    collection_key: SchedulingCollectionKey {
                        collection_type: CollectionType::Playlist,
                        playlist_id: Some(71),
                        ..SchedulingCollectionKey::default()
                    },
                }],
            }),
        })
        .collect();
    reference
}

async fn database(url: &str) -> sqlx::SqlitePool {
    let pool = ersatzrs_infra::db::init(url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_graph(&pool).await;
    for id in 2..=12 {
        common::seed_media_item(&pool, id, 1).await;
    }
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, 'Block', 0)"#).await.unwrap();
    pool
}

fn scripted_fixture(marathon: bool) -> (String, SchedulingReferenceData) {
    let mut reference = SchedulingReferenceData::default();
    let mut script = String::new();
    for (index, provider) in ["plex", "tvdb", "tmdb"].into_iter().enumerate() {
        let items = (0..4)
            .map(|episode| {
                let id = i64::try_from(1 + index * 4 + episode).unwrap();
                let mut item = SchedulingMediaItem::new(id, Duration::minutes(1));
                item.show_id = Some(i64::try_from(index + 1).unwrap());
                item.season = Some(1);
                item.episode = Some(i32::try_from(episode + 1).unwrap());
                item.preferred_audio_language_code = Some("nld".to_owned());
                item.preferred_subtitle_language_code = Some("eng".to_owned());
                item.subtitle_mode = Some(ChannelSubtitleMode::Forced);
                item
            })
            .collect::<Vec<_>>();
        reference
            .collections_by_name
            .insert(provider.to_owned(), items.clone());
        reference
            .media_by_show_guid
            .insert(format!("{provider}://show"), items);
        writeln!(
            script,
            "add_collection(\"{provider}\", \"{provider}\", \"chronological\")"
        )
        .unwrap();
    }
    if marathon {
        script.push_str(
            "add_marathon(\"pl\", {plex=\"show\", tvdb=\"show\", tmdb=\"show\"}, {}, \"show\")\n",
        );
    } else {
        script.push_str("create_playlist(\"pl\", {plex=1, tvdb=1, tmdb=1})\n");
    }
    script.push_str("while not is_done() do add_count(\"pl\", 1) end\n");
    (script, reference)
}

#[tokio::test]
async fn scripted_playlist_and_marathon_restore_sqlite_history() {
    for marathon in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}",
            directory.path().join("scripted.db").display()
        );
        let pool = database(&url).await;
        let repos = make_repositories(pool.clone());
        let (script, reference) = scripted_fixture(marathon);
        let path = directory.path().join("schedule.lua");
        std::fs::write(&path, script).unwrap();
        sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind" = 'Scripted', "ScheduleFile" = ? WHERE "Id" = 1"#)
            .bind(path.to_str().unwrap()).execute(&pool).await.unwrap();
        let playout = repos.playout.get_with_anchor(1).await.unwrap();
        let start = common::fixed_time();
        let split = start + Duration::minutes(7);
        let finish = start + Duration::minutes(24);
        let build = |reference_data, start, finish, playout, mode| {
            PlayoutBuilder::default()
                .build(SchedulingRequest {
                    playout,
                    start,
                    finish,
                    mode,
                    reference_data,
                })
                .unwrap()
        };
        let full = build(
            reference.clone(),
            start,
            finish,
            playout.clone(),
            PlayoutBuildMode::Reset,
        );
        assert_eq!(full.added_items.len(), 24);
        let first = build(
            reference.clone(),
            start,
            split,
            playout.clone(),
            PlayoutBuildMode::Reset,
        );
        repos
            .playout
            .apply_build_result(&first, &playout, Duration::zero())
            .await
            .unwrap();
        drop(repos);
        pool.close().await;

        let pool = ersatzrs_infra::db::init(&url).await.unwrap();
        let repos = make_repositories(pool.clone());
        let loaded = repos.playout.get_with_anchor(1).await.unwrap();
        let mut restored = repos
            .scheduling
            .scripted_reference_data(1, BTreeMap::new(), None)
            .await
            .unwrap();
        assert_eq!(restored.existing_history.len(), 7 * 4);
        // Synthetic source membership is injected explicitly; persisted state
        // must come from the production scripted reference-data loader.
        restored.collections_by_name = reference.collections_by_name;
        restored.media_by_show_guid = reference.media_by_show_guid;
        let continued = build(
            restored,
            split,
            finish,
            loaded.clone(),
            PlayoutBuildMode::Continue,
        );
        assert_eq!(continued.added_items.len(), 17);
        for (actual, expected) in continued.added_items.iter().zip(&full.added_items[7..]) {
            assert_eq!(
                (actual.media_item_id, actual.start, actual.finish),
                (expected.media_item_id, expected.start, expected.finish)
            );
            assert_eq!(
                actual.preferred_audio_language_code,
                expected.preferred_audio_language_code
            );
            assert_eq!(
                actual.preferred_subtitle_language_code,
                expected.preferred_subtitle_language_code
            );
            assert_eq!(actual.subtitle_mode, expected.subtitle_mode);
        }
        repos
            .playout
            .apply_build_result(&continued, &loaded, Duration::zero())
            .await
            .unwrap();
        drop(repos);
        pool.close().await;
    }
}

#[tokio::test]
async fn block_deco_playlist_matches_uninterrupted_after_sqlite_restart() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("playlist.db").display()
    );
    let pool = database(&url).await;
    let repos = make_repositories(pool.clone());
    let start = common::fixed_time();
    let finish = start + Duration::hours(9);
    let split = start + Duration::minutes(7 * 30);
    let reference = block_fixture(start);
    let playout = repos.playout.get_with_anchor(1).await.unwrap();
    let build = |reference_data, start, finish, playout| {
        PlayoutBuilder::default()
            .build(SchedulingRequest {
                playout,
                start,
                finish,
                mode: PlayoutBuildMode::Reset,
                reference_data,
            })
            .unwrap()
    };
    let full = build(reference.clone(), start, finish, playout.clone());
    assert_eq!(full.added_items.len(), 18);
    let mut initial = reference.clone();
    initial.effective_blocks.retain(|block| block.start < split);
    let first = build(initial, start, split, playout.clone());
    assert_eq!(first.added_items.len(), 7);
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    drop(repos);
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let repos = make_repositories(pool.clone());
    let loaded = repos.playout.get_with_anchor(1).await.unwrap();
    let persisted = repos
        .scheduling
        .block_reference_data(1, split, 1, Duration::zero())
        .await
        .unwrap();
    assert_eq!(persisted.existing_history.len(), 7 * 4);
    let mut resumed = reference;
    resumed
        .effective_blocks
        .retain(|block| block.start >= split);
    resumed.existing_history = persisted.existing_history;
    resumed.existing_items = persisted.existing_items;
    let continued = build(resumed, split, finish, loaded.clone());
    assert_eq!(continued.added_items, full.added_items[7..]);
    repos
        .playout
        .apply_build_result(&continued, &loaded, Duration::zero())
        .await
        .unwrap();
    let stored = repos
        .playout
        .items_in_range(1, split, finish)
        .await
        .unwrap();
    assert_eq!(stored.len(), 11);
    for (mut actual, expected) in stored.into_iter().zip(continued.added_items) {
        actual.id = expected.id;
        assert_eq!(actual, expected);
    }
    drop(repos);
    pool.close().await;
}
