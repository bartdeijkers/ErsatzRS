use std::path::PathBuf;

use chrono::{Duration, Utc};
use ersatzrs_core::repositories::{
    BlockRepository, ChannelRepository, CollectionRepository, ConfigRepository, DecoRepository,
    FfmpegProfileRepository, FillerRepository, GraphicsRepository, LibraryRepository,
    MediaItemRepository, MediaSourceRepository, MetadataRepository, PlayoutRepository,
    ProgramScheduleRepository, TemplateRepository,
};
use ersatzrs_core::scheduling::SchedulingContentSource;
use ersatzrs_domain::MediaSourceKind;
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
        r#"SELECT COUNT(*) FROM "sqlite_master" WHERE "type" = 'table' AND "name" NOT LIKE 'sqlite_%'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("count tables");
    assert_eq!(table_count, 138);

    assert_profiles_decode(&pool, &repos).await;
    assert_channels_decode(&repos).await;
    assert_libraries_decode(&pool, &repos).await;
    assert_media_sources_decode(&repos).await;
    assert_media_items_decode(&pool, &repos).await;
    assert_metadata_decode(&pool, &repos).await;
    assert_collections_decode(&pool, &repos).await;
    assert_playouts_decode(&pool, &repos).await;
    assert_program_schedules_decode(&pool, &repos).await;
    assert_block_scheduling_decode(&pool, &repos).await;
    assert_overlays_and_fillers_decode(&pool, &repos).await;
    assert_config_decode(&pool, &repos).await;
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

async fn assert_media_sources_decode(repos: &Repositories) {
    for source in repos
        .media_source
        .list_all()
        .await
        .expect("media sources should list")
    {
        match source.kind {
            MediaSourceKind::Plex => {
                repos
                    .media_source
                    .get_plex(source.id)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("PlexMediaSource {} should decode: {error}", source.id)
                    });
            }
            MediaSourceKind::Jellyfin => {
                repos
                    .media_source
                    .get_jellyfin(source.id)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("JellyfinMediaSource {} should decode: {error}", source.id)
                    });
            }
            MediaSourceKind::Emby => {
                repos
                    .media_source
                    .get_emby(source.id)
                    .await
                    .unwrap_or_else(|error| {
                        panic!("EmbyMediaSource {} should decode: {error}", source.id)
                    });
            }
            MediaSourceKind::Local => {}
        }
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

async fn assert_metadata_decode(pool: &SqlitePool, repos: &Repositories) {
    for id in foreign_ids(pool, "MovieMetadata", "MovieId")
        .await
        .into_iter()
        .take(25)
    {
        repos
            .metadata
            .get_movie_metadata(id)
            .await
            .unwrap_or_else(|error| panic!("MovieMetadata for Movie {id} should decode: {error}"));
    }

    for id in foreign_ids(pool, "EpisodeMetadata", "EpisodeId")
        .await
        .into_iter()
        .take(25)
    {
        repos
            .metadata
            .get_episode_metadata(id)
            .await
            .unwrap_or_else(|error| {
                panic!("EpisodeMetadata for Episode {id} should decode: {error}")
            });
    }

    for id in foreign_ids(pool, "MusicVideoMetadata", "MusicVideoId")
        .await
        .into_iter()
        .take(25)
    {
        repos
            .metadata
            .get_music_video_metadata(id)
            .await
            .unwrap_or_else(|error| {
                panic!("MusicVideoMetadata for MusicVideo {id} should decode: {error}")
            });
    }
}

async fn assert_collections_decode(pool: &SqlitePool, repos: &Repositories) {
    for id in ids(pool, "Collection").await.into_iter().take(25) {
        repos
            .collection
            .get_with_items(id)
            .await
            .unwrap_or_else(|error| panic!("Collection {id} should decode: {error}"));
    }
    for id in ids(pool, "SmartCollection").await.into_iter().take(25) {
        repos
            .collection
            .get_smart(id)
            .await
            .unwrap_or_else(|error| panic!("SmartCollection {id} should decode: {error}"));
    }
    for id in ids(pool, "Playlist").await.into_iter().take(25) {
        repos
            .collection
            .get_playlist_with_items(id)
            .await
            .unwrap_or_else(|error| panic!("Playlist {id} should decode: {error}"));
    }
}

async fn assert_playouts_decode(pool: &SqlitePool, repos: &Repositories) {
    let now = Utc::now();
    for id in ids(pool, "Playout").await {
        let playout = repos
            .playout
            .get_with_anchor(id)
            .await
            .unwrap_or_else(|error| {
                panic!("Playout {id} should decode from real ErsatzTV data: {error}")
            });

        let gap_count = child_count(pool, "PlayoutGap", "PlayoutId", id).await;
        let history_count = child_count(pool, "PlayoutHistory", "PlayoutId", id).await;
        let program_anchor_count =
            child_count(pool, "PlayoutProgramScheduleAnchor", "PlayoutId", id).await;
        let fill_group_count =
            child_count(pool, "PlayoutScheduleItemFillGroupIndex", "PlayoutId", id).await;
        let template_count = child_count(pool, "PlayoutTemplate", "PlayoutId", id).await;
        let anchor_count = child_count(pool, "PlayoutAnchor", "PlayoutId", id).await;

        assert_eq!(playout.gap_ids.len(), gap_count);
        assert_eq!(playout.playout_history_ids.len(), history_count);
        assert_eq!(
            playout.program_schedule_anchor_ids.len(),
            program_anchor_count
        );
        assert_eq!(playout.fill_group_index_ids.len(), fill_group_count);
        assert_eq!(playout.templates.len(), template_count);
        assert_eq!(usize::from(playout.anchor.is_some()), anchor_count);

        repos
            .playout
            .items_in_range(id, now - Duration::days(7), now + Duration::days(7))
            .await
            .unwrap_or_else(|error| {
                panic!("PlayoutItem range for playout {id} should decode: {error}")
            });
    }
}

async fn assert_program_schedules_decode(pool: &SqlitePool, repos: &Repositories) {
    for id in ids(pool, "ProgramSchedule").await {
        repos
            .program_schedule
            .get_with_items(id)
            .await
            .unwrap_or_else(|error| panic!("ProgramSchedule {id} should decode: {error}"));
    }
}

async fn assert_block_scheduling_decode(pool: &SqlitePool, repos: &Repositories) {
    for id in ids(pool, "Block").await {
        repos
            .block
            .get_with_items(id)
            .await
            .unwrap_or_else(|error| panic!("Block {id} should decode: {error}"));
    }
    for id in ids(pool, "Template").await {
        repos
            .template
            .get_with_items(id)
            .await
            .unwrap_or_else(|error| panic!("Template {id} should decode: {error}"));
    }
    for id in ids(pool, "Deco").await {
        repos
            .deco
            .get_with_watermarks(id)
            .await
            .unwrap_or_else(|error| panic!("Deco {id} should decode: {error}"));
    }
}

async fn assert_overlays_and_fillers_decode(pool: &SqlitePool, repos: &Repositories) {
    for id in ids(pool, "FillerPreset").await {
        repos
            .filler
            .get(id)
            .await
            .unwrap_or_else(|error| panic!("FillerPreset {id} should decode: {error}"));
    }
    for id in ids(pool, "GraphicsElement").await {
        repos
            .graphics
            .get_element(id)
            .await
            .unwrap_or_else(|error| panic!("GraphicsElement {id} should decode: {error}"));
    }
}

async fn assert_config_decode(pool: &SqlitePool, repos: &Repositories) {
    for key in keys(pool, "ConfigElement").await {
        repos
            .config
            .get(&key)
            .await
            .unwrap_or_else(|error| panic!("ConfigElement {key} should decode: {error}"));
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
        r#"SELECT "pg"."Name" AS "GroupName", "p"."Name" AS "Name"
           FROM "Playlist" AS "p"
           LEFT JOIN "PlaylistGroup" AS "pg" ON "pg"."Id" = "p"."PlaylistGroupId"
           ORDER BY "p"."Id"
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

async fn child_count(pool: &SqlitePool, table: &str, foreign_key: &str, id: i64) -> usize {
    let sql = format!(r#"SELECT COUNT(*) FROM "{table}" WHERE "{foreign_key}" = ?"#);
    let count: i64 = sqlx::query_scalar(&sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|error| panic!("count rows in {table} for {foreign_key}={id}: {error}"));
    usize::try_from(count).expect("row count should fit in usize")
}

async fn foreign_ids(pool: &SqlitePool, table: &str, column: &str) -> Vec<i64> {
    let sql = format!(r#"SELECT "{column}" FROM "{table}" ORDER BY "Id""#);
    sqlx::query_scalar(&sql)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("load foreign ids from {table}.{column}: {error}"))
}

async fn keys(pool: &SqlitePool, table: &str) -> Vec<String> {
    let sql = format!(r#"SELECT "Key" FROM "{table}" ORDER BY "Key""#);
    sqlx::query_scalar(&sql)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("load keys from {table}: {error}"))
}

async fn names(pool: &SqlitePool, table: &str) -> Vec<String> {
    let sql = format!(r#"SELECT "Name" FROM "{table}" ORDER BY "Id""#);
    sqlx::query_scalar(&sql)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|error| panic!("load names from {table}: {error}"))
}
