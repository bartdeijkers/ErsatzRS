mod common;

use nastv_core::scheduling::{SchedulingContentGuid, SchedulingContentSource};
use nastv_domain::{CollectionType, PlaybackOrder};
use nastv_infra::repositories::make_repositories;
use sqlx::{Executor, SqlitePool};

#[tokio::test]
async fn scheduling_repository_resolves_sql_content_sources() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;

    let collection = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Collection("prime".to_owned()))
        .await
        .unwrap();
    assert_eq!(
        collection
            .iter()
            .map(|item| (item.id, item.title.as_deref(), item.season, item.episode))
            .collect::<Vec<_>>(),
        vec![
            (12, Some("Pilot"), Some(1), Some(1)),
            (13, Some("Finale"), Some(1), Some(2)),
            (20, Some("Feature"), None, None),
        ]
    );

    let multi = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::MultiCollection(
            "multi-prime".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        multi.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![12, 13, 20, 21]
    );

    let smart = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::SmartCollection(
            "smart-bonus".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        smart.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![21]
    );

    let search = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Search(
            "title contains Pilot".to_owned(),
        ))
        .await
        .unwrap();
    assert_eq!(
        search.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![12]
    );

    let show = repos
        .scheduling
        .media_items_for_source(&SchedulingContentSource::Show {
            name: None,
            guids: vec![SchedulingContentGuid {
                source: "tvdb".to_owned(),
                value: "show-guid".to_owned(),
            }],
        })
        .await
        .unwrap();
    assert_eq!(
        show.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![12, 13]
    );
}

#[tokio::test]
async fn scheduling_repository_resolves_playlist_children_and_overlays() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    seed_scheduling_fixture(&pool).await;

    let playlist_items = repos
        .scheduling
        .playlist_items_for_source(&SchedulingContentSource::Playlist {
            group: Some("prime".to_owned()),
            name: "nightly".to_owned(),
        })
        .await
        .unwrap()
        .unwrap();

    assert_eq!(playlist_items.len(), 2);
    assert_eq!(
        playlist_items[0].collection_key.collection_type,
        CollectionType::Collection
    );
    assert_eq!(playlist_items[0].collection_key.collection_id, Some(1));
    assert_eq!(
        playlist_items[0].playback_order,
        PlaybackOrder::SeasonEpisode
    );
    assert!(playlist_items[0].play_all);
    assert_eq!(playlist_items[0].count, Some(2));
    assert_eq!(
        playlist_items[0]
            .media_items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![12, 13, 20]
    );
    assert_eq!(
        playlist_items[1]
            .media_items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![21]
    );

    assert_eq!(
        repos.scheduling.watermark_id_by_name("bug").await.unwrap(),
        Some(501)
    );
    assert_eq!(
        repos
            .scheduling
            .graphics_element_id_by_path("lower-third.svg")
            .await
            .unwrap(),
        Some(601)
    );
}

async fn seed_scheduling_fixture(pool: &SqlitePool) {
    seed_media_fixture(pool).await;
    seed_collection_fixture(pool).await;
    seed_overlay_fixture(pool).await;
}

async fn seed_media_fixture(pool: &SqlitePool) {
    common::seed_media_path(pool, 1, 1, 1).await;
    for id in [10, 11, 12, 13, 20, 21] {
        common::seed_media_item(pool, id, 1).await;
    }

    pool.execute("INSERT INTO shows (id, metadata_id) VALUES (10, 100)")
        .await
        .unwrap();
    pool.execute(
        "INSERT INTO show_metadata (
            id, item_id, metadata_kind, title, date_added, date_updated
         ) VALUES (100, 10, 'Show', 'The Show', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO metadata_guids (id, metadata_kind, metadata_id, source, guid)
         VALUES (1000, 'Show', 100, 'tvdb', 'show-guid')",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO seasons (id, show_id, metadata_id, season_number) VALUES (11, 10, 110, 1)",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO season_metadata (
            id, item_id, metadata_kind, title, date_added, date_updated, season_number
         ) VALUES (110, 11, 'Season', 'Season 1', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 1)",
    )
    .await
    .unwrap();

    insert_episode(pool, 12, 120, "Pilot", 1, "2026-01-01T00:00:00Z").await;
    insert_episode(pool, 13, 130, "Finale", 2, "2026-01-08T00:00:00Z").await;
    insert_movie(pool, 20, 200, "Feature", "2025-12-24T00:00:00Z").await;
    insert_movie(pool, 21, 210, "Bonus", "2026-02-01T00:00:00Z").await;
}

async fn seed_collection_fixture(pool: &SqlitePool) {
    pool.execute(
        "INSERT INTO collections (id, name, collection_type) VALUES (1, 'prime', 'Collection')",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO collection_items (id, collection_id, media_item_id, sort_order)
         VALUES (1, 1, 10, 0), (2, 1, 20, 1)",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO smart_collections (id, name, query)
         VALUES (2, 'smart-bonus', 'title = Bonus')",
    )
    .await
    .unwrap();
    pool.execute("INSERT INTO multi_collections (id, name) VALUES (3, 'multi-prime')")
        .await
        .unwrap();
    pool.execute(
        "INSERT INTO multi_collection_items (
            id, multi_collection_id, collection_type, collection_id, playback_order, sort_order
         ) VALUES (31, 3, 'Collection', 1, 'None', 0)",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO multi_collection_smart_items (id, multi_collection_id, smart_collection_id, sort_order)
         VALUES (32, 3, 2, 1)",
    )
    .await
    .unwrap();

    pool.execute("INSERT INTO playlists (id, name) VALUES (4, 'nightly')")
        .await
        .unwrap();
    pool.execute(
        "INSERT INTO playlist_groups (id, playlist_id, name, sort_order) VALUES (41, 4, 'prime', 0)",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO playlist_items (
            id, playlist_id, playlist_group_id, collection_type, collection_id,
            smart_collection_id, playback_order, count, play_all, sort_order
         ) VALUES
            (401, 4, 41, 'Collection', 1, NULL, 'SeasonEpisode', 2, 1, 0),
            (402, 4, 41, 'SmartCollection', NULL, 2, 'Chronological', NULL, 0, 1)",
    )
    .await
    .unwrap();
}

async fn seed_overlay_fixture(pool: &SqlitePool) {
    pool.execute(
        "INSERT INTO channel_watermarks (
            id, name, mode, image_source, location, size, width_percent,
            horizontal_margin_percent, vertical_margin_percent, frequency_minutes,
            duration_seconds, opacity, place_within_source_content, z_index
         ) VALUES (
            501, 'corner bug', 'Permanent', 'Local', 'TopRight', 'Scaled', 10.0,
            1.0, 1.0, 10, 30, 100, 0, 1
         )",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO graphics_elements (
            id, name, kind, x, y, width, height, z_index, path
         ) VALUES (
            601, 'lower third', 'Image', 0, 0, 100, 100, 1,
            '/var/lib/nastv/templates/graphics-elements/lower-third.svg'
         )",
    )
    .await
    .unwrap();
}

async fn insert_episode(
    pool: &SqlitePool,
    id: i64,
    metadata_id: i64,
    title: &str,
    episode_number: i32,
    release_date: &str,
) {
    sqlx::query(
        "INSERT INTO episodes (
            id, show_id, season_id, metadata_id, season_number, episode_number
         ) VALUES (?, 10, 11, ?, 1, ?)",
    )
    .bind(id)
    .bind(metadata_id)
    .bind(episode_number)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO episode_metadata (
            id, item_id, metadata_kind, title, release_date, date_added, date_updated,
            season_number, episode_number
         ) VALUES (?, ?, 'Episode', ?, ?, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 1, ?)",
    )
    .bind(metadata_id)
    .bind(id)
    .bind(title)
    .bind(release_date)
    .bind(episode_number)
    .execute(pool)
    .await
    .unwrap();
    insert_version(pool, id, 1_800_000).await;
}

async fn insert_movie(
    pool: &SqlitePool,
    id: i64,
    metadata_id: i64,
    title: &str,
    release_date: &str,
) {
    sqlx::query("INSERT INTO movies (id, metadata_id) VALUES (?, ?)")
        .bind(id)
        .bind(metadata_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO movie_metadata (
            id, item_id, metadata_kind, title, release_date, date_added, date_updated
         ) VALUES (?, ?, 'Movie', ?, ?, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')",
    )
    .bind(metadata_id)
    .bind(id)
    .bind(title)
    .bind(release_date)
    .execute(pool)
    .await
    .unwrap();
    insert_version(pool, id, 3_600_000).await;
}

async fn insert_version(pool: &SqlitePool, media_item_id: i64, duration_ms: i64) {
    sqlx::query(
        "INSERT INTO media_versions (
            id, media_item_id, name, duration_ms, video_scan_kind, date_added,
            date_updated, width, height
         ) VALUES (?, ?, 'main', ?, 'Progressive', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 1920, 1080)",
    )
    .bind(media_item_id * 10)
    .bind(media_item_id)
    .bind(duration_ms)
    .execute(pool)
    .await
    .unwrap();
}
