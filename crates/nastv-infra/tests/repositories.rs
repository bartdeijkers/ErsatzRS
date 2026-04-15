mod common;

use chrono::{Duration, TimeZone, Utc};
use nastv_core::repositories::{
    BlockRepository, ChannelRepository, CollectionRepository, ConfigRepository,
    FfmpegProfileRepository, FillerRepository, LibraryRepository, MediaItemRepository,
    MediaSourceRepository, PlayoutRepository, ProgramScheduleRepository,
};
use nastv_domain::{
    Block, Channel, Collection, CollectionType, FFmpegProfile, FillerPreset, Library,
    LibraryMediaKind, MediaItem, MediaSource, MediaSourceKind, PlayoutBuildStatus, PlayoutItem,
    PlexMediaSource, ProgramSchedule,
};
use nastv_infra::repositories::make_repositories;
use sqlx::Executor;
use uuid::Uuid;

#[tokio::test]
async fn config_repository_sets_and_gets_values() {
    let repos = make_repositories(common::test_db().await);

    repos.config.set("database.path", "nastv.db").await.unwrap();

    assert_eq!(
        repos.config.get("database.path").await.unwrap(),
        Some("nastv.db".to_owned())
    );
    assert_eq!(repos.config.get("missing").await.unwrap(), None);
}

#[tokio::test]
async fn channel_repository_round_trips_and_filters_enabled_channels() {
    let repos = make_repositories(common::test_db().await);
    repos
        .ffmpeg_profile
        .upsert(&profile(1, "default"))
        .await
        .unwrap();

    let mut enabled = channel(10, "10.1", true);
    enabled.artwork_ids = vec![100, 101];
    let disabled = channel(11, "11", false);

    repos.channel.upsert(&enabled).await.unwrap();
    repos.channel.upsert(&disabled).await.unwrap();

    let loaded = repos.channel.get_with_profile(10).await.unwrap();
    assert_eq!(loaded.number, "10.1");
    assert_eq!(loaded.artwork_ids, vec![100, 101]);

    let by_number = repos.channel.get_by_number("10.1").await.unwrap();
    assert_eq!(by_number.id, 10);

    let enabled_channels = repos.channel.list_enabled().await.unwrap();
    assert_eq!(enabled_channels.len(), 1);
    assert_eq!(enabled_channels[0].id, 10);
    assert!(enabled_channels[0].artwork_ids.is_empty());
}

#[tokio::test]
async fn source_library_and_media_item_queries_work() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());

    let source = PlexMediaSource {
        media_source: MediaSource {
            id: 1,
            name: "plex".to_owned(),
            kind: MediaSourceKind::Plex,
            last_scan: None,
        },
        ..PlexMediaSource::default()
    };
    repos.media_source.upsert_plex(&source).await.unwrap();

    let library = Library {
        id: 2,
        name: "Movies".to_owned(),
        media_kind: LibraryMediaKind::Movies,
        media_source_id: 1,
        media_source_kind: MediaSourceKind::Plex,
        path_ids: vec![3],
        ..Library::default()
    };
    repos.library.upsert(&library).await.unwrap();

    let item = MediaItem {
        id: 4,
        library_path_id: 3,
        ..MediaItem::default()
    };
    repos.media_item.upsert(&item).await.unwrap();
    pool.execute("INSERT INTO media_versions (id, media_item_id, data) VALUES (5, 4, '{}')")
        .await
        .unwrap();
    pool.execute(
        "INSERT INTO media_files (id, media_version_id, path, data) VALUES (6, 5, '/media/movie.mkv', '{}')",
    )
    .await
    .unwrap();

    assert_eq!(repos.media_source.list_all().await.unwrap()[0].id, 1);
    assert_eq!(repos.library.list_for_source(1).await.unwrap()[0].id, 2);
    assert_eq!(
        repos.library.get_with_paths(2).await.unwrap().path_ids,
        vec![3]
    );
    assert_eq!(
        repos.media_item.list_for_library_path(3).await.unwrap()[0].id,
        4
    );
    assert_eq!(
        repos
            .media_item
            .find_by_path("/media/movie.mkv")
            .await
            .unwrap()
            .id,
        4
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn collection_block_schedule_and_playout_relations_load() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());

    repos
        .ffmpeg_profile
        .upsert(&profile(1, "default"))
        .await
        .unwrap();
    repos.channel.upsert(&channel(1, "1", true)).await.unwrap();
    pool.execute("INSERT OR REPLACE INTO media_sources (id, name, kind, data) VALUES (1, 'local', 'Local', '{}')")
        .await
        .unwrap();
    pool.execute(
        "INSERT OR REPLACE INTO libraries (id, name, media_kind, media_source_id, media_source_kind, data)
         VALUES (1, 'local', 'Movies', 1, 'Local', '{}')",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT OR REPLACE INTO library_paths (id, library_id, path) VALUES (1, 1, '/media')",
    )
    .await
    .unwrap();
    repos
        .media_item
        .upsert(&MediaItem {
            id: 1,
            library_path_id: 1,
            ..MediaItem::default()
        })
        .await
        .unwrap();

    let collection = Collection {
        id: 1,
        name: "collection".to_owned(),
        collection_type: CollectionType::Collection,
        media_item_ids: vec![1],
        item_ids: vec![10],
    };
    repos.collection.upsert(&collection).await.unwrap();
    assert_eq!(
        repos
            .collection
            .get_with_items(1)
            .await
            .unwrap()
            .media_item_ids,
        vec![1]
    );
    assert!(repos.collection.list_for_media_item(1).await.unwrap()[0]
        .media_item_ids
        .is_empty());

    let block = Block {
        id: 1,
        block_group_id: None,
        name: "block".to_owned(),
        duration: Duration::minutes(30),
        item_ids: vec![20],
        ..Block::default()
    };
    repos.block.upsert(&block).await.unwrap();
    assert_eq!(
        repos.block.get_with_items(1).await.unwrap().item_ids,
        vec![20]
    );

    let schedule = ProgramSchedule {
        id: 1,
        name: "schedule".to_owned(),
        item_ids: vec![30],
        ..ProgramSchedule::default()
    };
    repos.program_schedule.upsert(&schedule).await.unwrap();
    pool.execute(
        "INSERT OR REPLACE INTO playouts (id, channel_id, program_schedule_id, schedule_kind, data)
         VALUES (1, 1, 1, 'Classic', '{\"id\":1,\"channel_id\":1,\"program_schedule_id\":1,\"schedule_file\":null,\"program_schedule_alternate_ids\":[],\"schedule_kind\":\"Classic\",\"item_ids\":[],\"gap_ids\":[],\"anchor\":null,\"program_schedule_anchor_ids\":[],\"fill_group_index_ids\":[],\"templates\":[],\"playout_history_ids\":[],\"seed\":0,\"daily_rebuild_time\":null,\"deco_id\":null,\"on_demand_checkpoint\":null,\"build_status\":null}')",
    )
    .await
    .unwrap();
    assert_eq!(
        repos.program_schedule.list_for_playout(1).await.unwrap()[0].id,
        1
    );

    let start = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let item = PlayoutItem {
        id: 1,
        media_item_id: 1,
        playout_id: 1,
        start,
        finish: start + Duration::minutes(30),
        ..PlayoutItem::default()
    };
    repos.playout.upsert_item(&item).await.unwrap();
    repos
        .playout
        .upsert_build_status(&PlayoutBuildStatus {
            playout_id: 1,
            last_build: start,
            success: true,
            message: None,
        })
        .await
        .unwrap();

    let in_range = repos
        .playout
        .items_in_range(
            1,
            start + Duration::minutes(5),
            start + Duration::minutes(10),
        )
        .await
        .unwrap();
    assert_eq!(in_range[0].id, 1);

    let loaded_playout = repos.playout.get_with_anchor(1).await.unwrap();
    assert_eq!(loaded_playout.item_ids, vec![1]);
    assert_eq!(loaded_playout.build_status.unwrap().playout_id, 1);
}

#[tokio::test]
async fn simple_list_repositories_round_trip() {
    let repos = make_repositories(common::test_db().await);

    repos
        .ffmpeg_profile
        .upsert(&profile(1, "default"))
        .await
        .unwrap();
    repos
        .filler
        .upsert(&FillerPreset {
            id: 1,
            name: "ads".to_owned(),
            ..FillerPreset::default()
        })
        .await
        .unwrap();

    assert_eq!(repos.ffmpeg_profile.get(1).await.unwrap().name, "default");
    assert_eq!(repos.ffmpeg_profile.list().await.unwrap().len(), 1);
    assert_eq!(repos.filler.get(1).await.unwrap().name, "ads");
    assert_eq!(repos.filler.list().await.unwrap().len(), 1);
}

fn profile(id: i64, name: &str) -> FFmpegProfile {
    FFmpegProfile {
        id,
        name: name.to_owned(),
        ..FFmpegProfile::default()
    }
}

fn channel(id: i64, number: &str, is_enabled: bool) -> Channel {
    Channel {
        id,
        unique_id: Uuid::from_u128(u128::try_from(id).unwrap_or_default()),
        number: number.to_owned(),
        sort_number: f64::from(i32::try_from(id).unwrap_or_default()),
        name: format!("Channel {number}"),
        ffmpeg_profile_id: 1,
        is_enabled,
        ..Channel::default()
    }
}
