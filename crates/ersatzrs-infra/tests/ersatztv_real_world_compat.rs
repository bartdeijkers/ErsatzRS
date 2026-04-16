use std::path::PathBuf;

use chrono::{Duration, Utc};
use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, LibraryRepository, MediaItemRepository,
    PlayoutRepository,
};
use ersatzrs_core::scheduling::SchedulingContentSource;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

#[tokio::test]
async fn real_world_ersatztv_sqlite_database_is_readable() {
    let Some(db_path) = real_world_db_path() else {
        eprintln!(
            "skipping real-world ErsatzTV compatibility test; set ERSATZRS_REAL_WORLD_SQLITE"
        );
        return;
    };
    if !db_path.exists() {
        eprintln!(
            "skipping real-world ErsatzTV compatibility test; ERSATZRS_REAL_WORLD_SQLITE does not exist"
        );
        return;
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true)
        .create_if_missing(false)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open real-world ErsatzTV sqlite database read-only");
    let repos = make_repositories(pool.clone());

    let table_count: i64 = sqlx::query_scalar(
        r"SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&pool)
    .await
    .expect("count tables");
    assert_eq!(table_count, 138);

    assert_profiles_decode(&pool, &repos).await;
    assert_channels_decode(&repos).await;
    assert_libraries_decode(&pool, &repos).await;
    assert_media_items_decode(&pool, &repos).await;
    assert_playouts_decode(&pool, &repos).await;
    assert_scheduling_sources_resolve(&pool, &repos).await;
}

#[allow(clippy::disallowed_methods)]
fn real_world_db_path() -> Option<PathBuf> {
    std::env::var_os("ERSATZRS_REAL_WORLD_SQLITE").map(PathBuf::from)
}

async fn assert_profiles_decode(pool: &SqlitePool, repos: &Repositories) {
    for id in ids(pool, "FFmpegProfile").await {
        repos.ffmpeg_profile.get(id).await.unwrap_or_else(|error| {
            panic!("FFmpegProfile {id} should decode from real ErsatzTV data: {error}")
        });
    }
}

async fn assert_channels_decode(repos: &Repositories) {
    let channels = repos
        .channel
        .list_enabled()
        .await
        .expect("enabled channels should decode");
    assert!(!channels.is_empty());
    for channel in channels {
        repos
            .channel
            .get_with_profile(channel.id)
            .await
            .unwrap_or_else(|error| {
                panic!("Channel {} should decode with profile: {error}", channel.id)
            });
        repos
            .channel
            .get_by_number(&channel.number)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "Channel {} should decode by number: {error}",
                    channel.number
                )
            });
    }
}

async fn assert_libraries_decode(pool: &SqlitePool, repos: &Repositories) {
    for id in ids(pool, "Library").await {
        repos
            .library
            .get_with_paths(id)
            .await
            .unwrap_or_else(|error| {
                panic!("Library {id} should decode from real ErsatzTV data: {error}")
            });
    }
}

async fn assert_media_items_decode(pool: &SqlitePool, repos: &Repositories) {
    for id in ids(pool, "MediaItem").await.into_iter().take(25) {
        repos
            .media_item
            .get_with_version(id)
            .await
            .unwrap_or_else(|error| {
                panic!("MediaItem {id} should decode from real ErsatzTV data: {error}")
            });
    }
}

async fn assert_playouts_decode(pool: &SqlitePool, repos: &Repositories) {
    let now = Utc::now();
    for id in ids(pool, "Playout").await {
        repos
            .playout
            .get_with_anchor(id)
            .await
            .unwrap_or_else(|error| {
                panic!("Playout {id} should decode from real ErsatzTV data: {error}")
            });
        repos
            .playout
            .items_in_range(id, now - Duration::days(7), now + Duration::days(7))
            .await
            .unwrap_or_else(|error| {
                panic!("PlayoutItem range for playout {id} should decode: {error}")
            });
    }
}

async fn assert_scheduling_sources_resolve(pool: &SqlitePool, repos: &Repositories) {
    for name in names(pool, "Collection").await.into_iter().take(10) {
        repos
            .scheduling
            .media_items_for_source(&SchedulingContentSource::Collection(name.clone()))
            .await
            .unwrap_or_else(|error| panic!("Collection {name} should resolve: {error}"));
    }

    for row in sqlx::query(
        r#"SELECT pg."Name" AS "GroupName", p."Name" AS "Name"
           FROM "Playlist" p
           LEFT JOIN "PlaylistGroup" pg ON pg."Id" = p."PlaylistGroupId"
           ORDER BY p."Id"
           LIMIT 10"#,
    )
    .fetch_all(pool)
    .await
    .expect("load playlist names")
    {
        let group: Option<String> = row.try_get("GroupName").expect("playlist group");
        let name: String = row.try_get("Name").expect("playlist name");
        repos
            .scheduling
            .playlist_items_for_source(&SchedulingContentSource::Playlist { group, name })
            .await
            .expect("playlist should resolve");
    }
}

async fn ids(pool: &SqlitePool, table: &str) -> Vec<i64> {
    let sql = format!(r#"SELECT "Id" FROM "{table}" ORDER BY "Id""#);
    sqlx::query_scalar(&sql)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("load ids from {table}: {error}"))
}

async fn names(pool: &SqlitePool, table: &str) -> Vec<String> {
    let sql = format!(r#"SELECT "Name" FROM "{table}" ORDER BY "Id""#);
    sqlx::query_scalar(&sql)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("load names from {table}: {error}"))
}
