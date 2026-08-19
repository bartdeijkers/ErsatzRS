//! Coverage for the add-on media list television projection.
//!
//! `reconcile_tv_projection` builds real `Show`/`Season`/`Episode` rows out of a
//! managed add-on list. It had no test, which is how a projection that omitted
//! `SeasonMetadata` — leaving projected seasons invisible to every consumer that
//! joins metadata to resolve a season title — reached a running system.
#![cfg(not(feature = "mysql"))]

mod common;

use std::path::Path;

use ersatzrs_core::repositories::AddonMediaListRepository;
use ersatzrs_domain::AddonMediaListPresentationKind;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, SqlitePool};
use tempfile::TempDir;

const LIST_ID: i64 = 1;

/// Seed a managed add-on list whose items are backed by on-disk stream files,
/// which is what the projection reads NFO metadata from.
async fn seed_managed_list(pool: &SqlitePool) -> TempDir {
    let directory = tempfile::tempdir().expect("managed list temp directory");

    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .expect("seed media source");
    pool.execute(
        r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
           VALUES (1, 2, 1, 'Add-on lists')"#,
    )
    .await
    .expect("seed library");
    pool.execute(
        r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/managed', 1)"#,
    )
    .await
    .expect("seed library path");
    pool.execute(
        r#"INSERT INTO "AddonInstallation" (
              "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
              "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
           ) VALUES (
              'org.example.lists', '1.0.0', 1, 'package', 'content', '{}',
              'addons/org.example.lists', '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z'
           )"#,
    )
    .await
    .expect("seed addon installation");
    pool.execute(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount"
           ) VALUES (
              1, 'org.example.lists', 'fixture/show',
              'https://example.invalid/fixture/show', 'Fixture Show', 3
           )"#,
    )
    .await
    .expect("seed addon media list");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListLibrary"
              ("AddonMediaListId", "LibraryId", "FolderName", "ManifestPath")
           VALUES (1, 1, 'Fixture Show', ?)"#,
    )
    .bind(directory.path().to_string_lossy().as_ref())
    .execute(pool)
    .await
    .expect("seed managed library");

    // Season 1 episodes 1 and 2, plus a specials episode, so the projection has
    // to create two distinct seasons.
    for (item_id, season, episode, title) in [
        (10_i64, 1_i32, 1_i32, "Pilot"),
        (11, 1, 2, "Second"),
        (12, 0, 1, "Special"),
    ] {
        seed_list_item(pool, directory.path(), item_id, season, episode, title).await;
    }

    directory
}

async fn seed_list_item(
    pool: &SqlitePool,
    root: &Path,
    item_id: i64,
    season: i32,
    episode: i32,
    title: &str,
) {
    let stream_path = root.join(format!("S{season:02}E{episode:02}.strm"));
    let media_item_id = 100 + item_id;

    sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#)
        .bind(media_item_id)
        .execute(pool)
        .await
        .expect("seed remote stream media item");
    // The list matcher re-identifies each item by (AddonId, ProviderId) once the
    // projection has run, so a managed remote stream must carry both.
    sqlx::query(
        r#"INSERT INTO "RemoteStream" ("Id", "Url", "AddonId", "ProviderId")
           VALUES (?, ?, 'org.example.lists', ?)"#,
    )
    .bind(media_item_id)
    .bind(format!("https://example.invalid/episode/{item_id}"))
    .bind(format!("episode:{item_id}"))
    .execute(pool)
    .await
    .expect("seed remote stream");
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
              "Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width",
              "VideoScanKind", "RemoteStreamId"
           ) VALUES (?, '2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z', '00:22:00', 1080, 1920, 0, ?)"#,
    )
    .bind(item_id)
    .bind(media_item_id)
    .execute(pool)
    .await
    .expect("seed media version");
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (?, ?, ?)"#,
    )
    .bind(item_id)
    .bind(stream_path.to_string_lossy().as_ref())
    .bind(format!("hash-{item_id}"))
    .execute(pool)
    .await
    .expect("seed media file");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItem" (
              "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
              "MediaItemId", "Rank", "Title", "Season", "Episode", "Availability"
           ) VALUES (?, 1, ?, ?, 4, ?, ?, ?, ?, ?, 1)"#,
    )
    .bind(item_id)
    .bind(format!("episode:{item_id}"))
    .bind(title)
    .bind(media_item_id)
    .bind(item_id)
    .bind(title)
    .bind(season)
    .bind(episode)
    .execute(pool)
    .await
    .expect("seed addon media list item");
}

async fn scalar<T>(pool: &SqlitePool, sql: &str) -> T
where
    T: for<'row> sqlx::Decode<'row, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
{
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .expect("query scalar")
}

#[tokio::test]
async fn television_projection_creates_show_seasons_and_episodes() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    repositories
        .addon_media_lists
        .request_presentation(
            LIST_ID,
            AddonMediaListPresentationKind::TelevisionShow,
            Some("Fixture Show"),
        )
        .await
        .expect("project the list as a television show");

    let show_id: i64 = scalar(
        &pool,
        r#"SELECT "ShowId" FROM "AddonMediaListTelevisionShow" WHERE "AddonMediaListId" = 1"#,
    )
    .await;
    let show_title: String = scalar(
        &pool,
        r#"SELECT "Title" FROM "ShowMetadata" ORDER BY "Id" LIMIT 1"#,
    )
    .await;
    assert_eq!(show_title, "Fixture Show");

    let seasons: Vec<i32> = sqlx::query_scalar(
        r#"SELECT "SeasonNumber" FROM "Season" WHERE "ShowId" = ? ORDER BY "SeasonNumber""#,
    )
    .bind(show_id)
    .fetch_all(&pool)
    .await
    .expect("load projected seasons");
    assert_eq!(seasons, [0, 1]);

    // The regression: every other season source carries metadata, and consumers
    // join it to resolve the season title, so a projected season without one is
    // invisible to them — including the show detail page.
    let season_titles: Vec<String> = sqlx::query_scalar(
        r#"SELECT "metadata"."Title"
           FROM "SeasonMetadata" AS "metadata"
           JOIN "Season" AS "season" ON "season"."Id" = "metadata"."SeasonId"
           WHERE "season"."ShowId" = ?
           ORDER BY "season"."SeasonNumber""#,
    )
    .bind(show_id)
    .fetch_all(&pool)
    .await
    .expect("load projected season metadata");
    assert_eq!(season_titles, ["Specials", "Season 1"]);

    let episode_titles: Vec<String> = sqlx::query_scalar(
        r#"SELECT "metadata"."Title"
           FROM "EpisodeMetadata" AS "metadata"
           JOIN "Episode" AS "episode" ON "episode"."Id" = "metadata"."EpisodeId"
           JOIN "Season" AS "season" ON "season"."Id" = "episode"."SeasonId"
           WHERE "season"."ShowId" = ?
           ORDER BY "season"."SeasonNumber", "metadata"."EpisodeNumber""#,
    )
    .bind(show_id)
    .fetch_all(&pool)
    .await
    .expect("load projected episode metadata");
    assert_eq!(episode_titles, ["Special", "Pilot", "Second"]);

    // Each projected episode is linked back to its list item and carries the
    // media version of the remote stream it came from.
    let linked_versions: i64 = scalar(
        &pool,
        r#"SELECT COUNT(*)
           FROM "AddonMediaListItemEpisode" AS "projection"
           JOIN "MediaVersion" AS "version" ON "version"."EpisodeId" = "projection"."EpisodeId""#,
    )
    .await;
    assert_eq!(linked_versions, 3);
}

#[tokio::test]
async fn television_projection_is_idempotent() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    for _ in 0..2 {
        repositories
            .addon_media_lists
            .request_presentation(
                LIST_ID,
                AddonMediaListPresentationKind::TelevisionShow,
                Some("Fixture Show"),
            )
            .await
            .expect("project the list as a television show");
    }

    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Show""#).await,
        1
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Season""#).await,
        2
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "SeasonMetadata""#).await,
        2
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Episode""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "EpisodeMetadata""#).await,
        3
    );
}

#[tokio::test]
async fn reverting_to_remote_streams_removes_the_projection() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    repositories
        .addon_media_lists
        .request_presentation(
            LIST_ID,
            AddonMediaListPresentationKind::TelevisionShow,
            Some("Fixture Show"),
        )
        .await
        .expect("project the list as a television show");
    repositories
        .addon_media_lists
        .request_presentation(LIST_ID, AddonMediaListPresentationKind::RemoteStreams, None)
        .await
        .expect("revert the projection");

    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Show""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Season""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Episode""#).await,
        0
    );
    // The remote stream media items themselves survive the revert.
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "RemoteStream""#).await,
        3
    );
}

/// The Movie projection is the television one without the Show and Season
/// levels: each entry becomes a `Movie` over the same Remote Stream version.
#[tokio::test]
async fn movie_projection_creates_movies_over_the_shared_remote_stream_version() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    repositories
        .addon_media_lists
        .request_presentation(LIST_ID, AddonMediaListPresentationKind::Movies, None)
        .await
        .expect("project the list as movies");

    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "AddonMediaListItemMovie""#).await,
        3
    );
    // No parent container is invented for a movie list.
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Show""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Season""#).await,
        0
    );

    // Every projected movie carries metadata; without it the movie is invisible
    // to every consumer that joins metadata to resolve a title.
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "MovieMetadata""#).await,
        3
    );
    assert_eq!(
        scalar::<String>(
            &pool,
            r#"SELECT "meta"."Title"
               FROM "MovieMetadata" AS "meta"
               JOIN "AddonMediaListItemMovie" AS "link" ON "link"."MovieId" = "meta"."MovieId"
               WHERE "link"."AddonMediaListItemId" = 10"#
        )
        .await,
        "Pilot"
    );

    // The provider version is shared, not copied: the same MediaVersion now
    // resolves through the movie identity.
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "MediaVersion" WHERE "MovieId" IS NOT NULL"#
        )
        .await,
        3
    );

    // Unavailable provider items keep their unavailable media-item state.
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "MediaItem" AS "item"
               JOIN "AddonMediaListItemMovie" AS "link" ON "link"."MovieId" = "item"."Id"
               WHERE "item"."State" = 2"#
        )
        .await,
        3
    );
}

#[tokio::test]
async fn movie_projection_is_idempotent_and_reverts_without_deleting_the_source() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    for _ in 0..2 {
        repositories
            .addon_media_lists
            .request_presentation(LIST_ID, AddonMediaListPresentationKind::Movies, None)
            .await
            .expect("project the list as movies");
    }
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        3
    );

    repositories
        .addon_media_lists
        .request_presentation(LIST_ID, AddonMediaListPresentationKind::RemoteStreams, None)
        .await
        .expect("revert to remote streams");

    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "AddonMediaListItemMovie""#).await,
        0
    );
    // Reverting detaches the projection; the provider items stay playable.
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "RemoteStream""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "MediaVersion" WHERE "RemoteStreamId" IS NOT NULL"#
        )
        .await,
        3
    );
}

/// A projected movie referenced from outside the list must block the revert,
/// or a schedule silently loses its target.
#[tokio::test]
async fn movie_projection_revert_is_refused_while_a_projected_movie_is_referenced() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    repositories
        .addon_media_lists
        .request_presentation(LIST_ID, AddonMediaListPresentationKind::Movies, None)
        .await
        .expect("project the list as movies");

    let movie_id: i64 = scalar(
        &pool,
        r#"SELECT "MovieId" FROM "AddonMediaListItemMovie" WHERE "AddonMediaListItemId" = 10"#,
    )
    .await;
    pool.execute(
        r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
           VALUES (1, 'Operator picks', 0)"#,
    )
    .await
    .expect("seed collection");
    sqlx::query(r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (1, ?)"#)
        .bind(movie_id)
        .execute(&pool)
        .await
        .expect("reference the projected movie");

    let error = repositories
        .addon_media_lists
        .request_presentation(LIST_ID, AddonMediaListPresentationKind::RemoteStreams, None)
        .await
        .expect_err("a referenced projection must not be reverted");
    assert!(error.to_string().contains("referenced"), "{error}");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        3
    );
}
