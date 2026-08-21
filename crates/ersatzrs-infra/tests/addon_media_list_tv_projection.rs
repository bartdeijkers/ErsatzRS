//! Coverage for the add-on media list television projection.
//!
//! `reconcile_tv_projection` builds real `Show`/`Season`/`Episode` rows out of a
//! managed add-on list. It had no test, which is how a projection that omitted
//! `SeasonMetadata` — leaving projected seasons invisible to every consumer that
//! joins metadata to resolve a season title — reached a running system.
#![cfg(not(feature = "mysql"))]

mod common;

use std::{fs, path::Path};

use ersatzrs_core::repositories::{
    AddonMediaListArtworkRole, AddonMediaListNfoMetadata, AddonMediaListNfoPerson,
    AddonMediaListRepository,
};
use ersatzrs_domain::{AddonMediaListContentKind, AddonMediaListPresentationKind};
use ersatzrs_infra::repositories::{make_repositories, Repositories};
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
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "Description", "ItemCount",
              "ProviderMetadataJson", "ReviewState", "PublishedRevision"
           ) VALUES (
              1, 'org.example.lists', 'fixture/show',
              'https://example.invalid/fixture/show', 'Fixture Show', 'Provider introduction', 3,
              '{"title":"Fixture Show","plot":"Provider introduction"}', 2, 1
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
    let nfo_path = stream_path.with_extension("nfo");
    let media_item_id = 100 + item_id;

    let nfo = AddonMediaListNfoMetadata {
        title: title.to_owned(),
        season: Some(season),
        episode: Some(episode),
        year: Some(2026),
        genres: vec!["Education".to_owned()],
        tags: vec!["Archive".to_owned(), "Teleac".to_owned()],
        studios: vec!["Fixture Studio".to_owned()],
        music_labels: vec!["Fixture Label".to_owned()],
        languages: vec!["nl".to_owned()],
        people: vec![AddonMediaListNfoPerson {
            name: "Fixture Presenter".to_owned(),
            role: Some("Host".to_owned()),
            order: Some(0),
            thumb: None,
        }],
        directors: vec!["Fixture Director".to_owned()],
        writers: vec!["Fixture Writer".to_owned()],
        artists: vec!["Fixture Artist".to_owned()],
        artwork_urls: vec![format!("https://example.invalid/artwork/{item_id}.jpg")],
        guids: vec![format!("fixture://episode-{item_id}")],
        ..AddonMediaListNfoMetadata::default()
    };
    fs::write(
        &nfo_path,
        ersatzrs_infra::addon_media_nfo::serialize("episodedetails", &nfo)
            .expect("serialize managed item NFO"),
    )
    .expect("write managed item NFO");
    fs::write(stream_path.with_extension("jpg"), b"fixture thumbnail")
        .expect("write materialized artwork");

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
              "MediaItemId", "Rank", "Title", "Season", "Episode", "Availability", "Published",
              "CollectionRole", "ProviderMetadataJson"
           ) VALUES (?, 1, ?, ?, 4, ?, ?, ?, ?, ?, 0, 1, ?, ?)"#,
    )
    .bind(item_id)
    .bind(format!("episode:{item_id}"))
    .bind(title)
    .bind(media_item_id)
    .bind(item_id)
    .bind(title)
    .bind(season)
    .bind(episode)
    .bind(if item_id == 10 { 1 } else { 2 })
    .bind(serde_json::to_string(&nfo).expect("serialize provider metadata"))
    .execute(pool)
    .await
    .expect("seed addon media list item");
}

/// Presentation selection edits the review draft; publication is the explicit
/// boundary that changes the managed library hierarchy.
async fn request_and_publish(
    repositories: &Repositories,
    presentation: AddonMediaListPresentationKind,
) -> ersatzrs_core::repositories::Result<()> {
    repositories
        .addon_media_lists
        .request_presentation(LIST_ID, presentation, None)
        .await?;
    repositories.addon_media_lists.publish(LIST_ID).await
}

async fn save_collection_title(repositories: &Repositories, title: &str) {
    let form = repositories
        .addon_media_lists
        .get_show_nfo(LIST_ID)
        .await
        .expect("load collection metadata");
    let mut metadata = form.metadata;
    metadata.title = title.to_owned();
    repositories
        .addon_media_lists
        .save_show_nfo(LIST_ID, &metadata, &form.etag)
        .await
        .expect("save collection title");
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

async fn prepare_published_revision(
    pool: &SqlitePool,
    repositories: &Repositories,
    directory: &TempDir,
) -> (i32, Vec<u8>, i64) {
    sqlx::query(
        r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem")
           SELECT 'System', 1 WHERE NOT EXISTS (
               SELECT 1 FROM "PlaylistGroup" WHERE "IsSystem" = 1
           )"#,
    )
    .execute(pool)
    .await
    .expect("seed system playlist group");
    sqlx::query(r#"UPDATE "AddonMediaList" SET "GeneratePlaylist" = 1 WHERE "Id" = 1"#)
        .execute(pool)
        .await
        .expect("enable managed playlist");
    request_and_publish(repositories, AddonMediaListPresentationKind::TelevisionShow)
        .await
        .expect("publish baseline revision");
    let revision = scalar(pool, r#"SELECT "PublishedRevision" FROM "AddonMediaList""#).await;
    let nfo = fs::read(directory.path().join("tvshow.nfo")).expect("baseline tvshow NFO");
    let playlist_items = scalar(
        pool,
        r#"SELECT COUNT(*) FROM "PlaylistItem"
           WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 1)"#,
    )
    .await;
    (revision, nfo, playlist_items)
}

async fn recover_failed_publication(
    repositories: &Repositories,
    error: &ersatzrs_core::repositories::RepositoryError,
) {
    repositories
        .addon_media_lists
        .restore_staged_publication(LIST_ID, &error.to_string())
        .await
        .expect("restore publication snapshot");
    repositories
        .addon_media_lists
        .reconcile_restored_publication(LIST_ID)
        .await
        .expect("reconcile restored projection");
}

async fn assert_publication_was_restored(
    pool: &SqlitePool,
    directory: &TempDir,
    revision: i32,
    nfo: &[u8],
    playlist_items: i64,
) {
    assert_eq!(
        scalar::<i32>(pool, r#"SELECT "PublishedRevision" FROM "AddonMediaList""#).await,
        revision
    );
    assert_eq!(
        scalar::<i64>(pool, r#"SELECT "ReviewState" FROM "AddonMediaList""#).await,
        1,
        "the editable draft must remain ready after rollback"
    );
    assert_eq!(
        scalar::<i64>(
            pool,
            r#"SELECT COUNT(*) FROM "AddonMediaList" WHERE "PublicationJournalJson" IS NOT NULL"#,
        )
        .await,
        0
    );
    assert_eq!(
        fs::read(directory.path().join("tvshow.nfo")).expect("restored tvshow NFO"),
        nfo
    );
    assert_eq!(
        scalar::<i64>(
            pool,
            r#"SELECT COUNT(*) FROM "PlaylistItem"
               WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 1)"#,
        )
        .await,
        playlist_items
    );
    assert_eq!(
        scalar::<String>(pool, r#"SELECT "Title" FROM "ShowMetadata" LIMIT 1"#).await,
        "Fixture Show"
    );
}

/// Every projection has a distinct metadata table, but all of them must carry
/// the common NFO facets and the materialized thumbnail for the same list item.
async fn assert_projected_item_facets(
    pool: &SqlitePool,
    metadata_table: &str,
    link_table: &str,
    owner_column: &str,
) {
    let metadata_id: i64 = scalar(
        pool,
        &format!(
            r#"SELECT "metadata"."Id"
               FROM "{metadata_table}" AS "metadata"
               JOIN "{link_table}" AS "link" ON "link"."{owner_column}" = "metadata"."{owner_column}"
               WHERE "link"."AddonMediaListItemId" = 10"#
        ),
    )
    .await;
    let fk_column = format!("{}MetadataId", owner_column.trim_end_matches("Id"));
    for table in ["Genre", "Tag", "Studio", "Actor"] {
        assert_eq!(
            scalar::<i64>(
                pool,
                &format!(r#"SELECT COUNT(*) FROM "{table}" WHERE "{fk_column}" = {metadata_id}"#),
            )
            .await,
            if table == "Tag" { 2 } else { 1 },
            "{table} was not projected for {metadata_table}"
        );
    }
    let artwork_path: String = scalar(
        pool,
        &format!(r#"SELECT "Path" FROM "Artwork" WHERE "{fk_column}" = {metadata_id} LIMIT 1"#),
    )
    .await;
    assert!(
        Path::new(&artwork_path).is_file(),
        "{metadata_table} artwork is not local"
    );
}

#[tokio::test]
async fn television_projection_creates_show_seasons_and_episodes() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    request_and_publish(
        &repositories,
        AddonMediaListPresentationKind::TelevisionShow,
    )
    .await
    .expect("project the list as a television show");

    let show_nfo = repositories
        .addon_media_lists
        .get_show_nfo(LIST_ID)
        .await
        .expect("load generated show NFO");
    assert_eq!(show_nfo.metadata.title, "Fixture Show");
    assert_eq!(
        show_nfo.metadata.plot.as_deref(),
        Some("Provider introduction")
    );
    assert_eq!(
        show_nfo.ownership,
        ersatzrs_core::repositories::AddonMediaListNfoOwnership::Generated
    );

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
        request_and_publish(
            &repositories,
            AddonMediaListPresentationKind::TelevisionShow,
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

/// The projection consumes the NFO rather than merely preserving it on disk:
/// faceted search and artwork consumers read these database rows. Repeating the
/// request proves the related-row replacement cannot accumulate duplicates.
#[tokio::test]
async fn television_projection_projects_nfo_facets_and_materialized_artwork() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    for _ in 0..2 {
        request_and_publish(
            &repositories,
            AddonMediaListPresentationKind::TelevisionShow,
        )
        .await
        .expect("project managed NFO metadata");
    }

    let metadata_id: i64 = scalar(
        &pool,
        r#"SELECT "metadata"."Id"
           FROM "EpisodeMetadata" AS "metadata"
           JOIN "AddonMediaListItemEpisode" AS "link" ON "link"."EpisodeId" = "metadata"."EpisodeId"
           WHERE "link"."AddonMediaListItemId" = 10"#,
    )
    .await;
    for (table, expected) in [
        ("Genre", "Education"),
        ("Tag", "Archive"),
        ("Studio", "Fixture Studio"),
    ] {
        assert_eq!(
            scalar::<String>(
                &pool,
                &format!(
                    r#"SELECT "Name" FROM "{table}" WHERE "EpisodeMetadataId" = {metadata_id} ORDER BY "Id" LIMIT 1"#
                ),
            )
            .await,
            expected,
            "{table} was not projected from the NFO"
        );
        assert_eq!(
            scalar::<i64>(
                &pool,
                &format!(
                    r#"SELECT COUNT(*) FROM "{table}" WHERE "EpisodeMetadataId" = {metadata_id}"#
                ),
            )
            .await,
            if table == "Tag" { 2 } else { 1 },
            "{table} rows accumulated on refresh"
        );
    }
    assert_eq!(
        scalar::<String>(
            &pool,
            &format!(r#"SELECT "Name" FROM "Actor" WHERE "EpisodeMetadataId" = {metadata_id}"#),
        )
        .await,
        "Fixture Presenter"
    );
    assert_eq!(
        scalar::<String>(
            &pool,
            &format!(r#"SELECT "Role" FROM "Actor" WHERE "EpisodeMetadataId" = {metadata_id}"#),
        )
        .await,
        "Host"
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            &format!(r#"SELECT COUNT(*) FROM "Actor" WHERE "EpisodeMetadataId" = {metadata_id}"#),
        )
        .await,
        1
    );

    let artwork_path: String = scalar(
        &pool,
        &format!(
            r#"SELECT "Path" FROM "Artwork" WHERE "EpisodeMetadataId" = {metadata_id} LIMIT 1"#
        ),
    )
    .await;
    assert!(
        Path::new(&artwork_path).is_file(),
        "projected artwork is not a materialized file: {artwork_path}"
    );
}

#[tokio::test]
async fn reverting_to_remote_streams_removes_the_projection() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    request_and_publish(
        &repositories,
        AddonMediaListPresentationKind::TelevisionShow,
    )
    .await
    .expect("project the list as a television show");
    request_and_publish(&repositories, AddonMediaListPresentationKind::RemoteStreams)
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

    request_and_publish(&repositories, AddonMediaListPresentationKind::Movies)
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

    // Available provider items remain available through the projected identity.
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "MediaItem" AS "item"
               JOIN "AddonMediaListItemMovie" AS "link" ON "link"."MovieId" = "item"."Id"
               WHERE "item"."State" = 0"#
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
        request_and_publish(&repositories, AddonMediaListPresentationKind::Movies)
            .await
            .expect("project the list as movies");
    }
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        3
    );

    request_and_publish(&repositories, AddonMediaListPresentationKind::RemoteStreams)
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

    request_and_publish(&repositories, AddonMediaListPresentationKind::Movies)
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

    let error = request_and_publish(&repositories, AddonMediaListPresentationKind::RemoteStreams)
        .await
        .expect_err("a referenced projection must not be reverted");
    assert!(error.to_string().contains("referenced"), "{error}");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        3
    );
}

/// Other Videos is the same parentless shape as Movies, and switching between
/// two projections must leave exactly one live hierarchy over the same items.
#[tokio::test]
async fn switching_between_flat_projections_leaves_only_the_requested_one() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    request_and_publish(&repositories, AddonMediaListPresentationKind::OtherVideos)
        .await
        .expect("project the list as other videos");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "OtherVideo""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "OtherVideoMetadata""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "MediaVersion" WHERE "OtherVideoId" IS NOT NULL"#
        )
        .await,
        3
    );

    // Switching to Movies must detach the Other Video projection rather than
    // leaving two hierarchies over the same provider items.
    request_and_publish(&repositories, AddonMediaListPresentationKind::Movies)
        .await
        .expect("switch the list to movies");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "OtherVideo""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "AddonMediaListItemOtherVideo""#
        )
        .await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "MediaVersion" WHERE "MovieId" IS NOT NULL"#
        )
        .await,
        3
    );
    // Switching back from television must be equally clean.
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Show""#).await,
        0
    );
    // The provider items survive every switch.
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "RemoteStream""#).await,
        3
    );
}

/// A song metadata table has no rating or description columns, so the shared
/// projection must write the narrower column set. Naming a column that does not
/// exist would fail only for this kind, which is exactly what a shared
/// implementation risks.
#[tokio::test]
async fn song_projection_writes_the_narrow_metadata_column_set() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    request_and_publish(&repositories, AddonMediaListPresentationKind::Songs)
        .await
        .expect("project the list as songs");

    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Song""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "SongMetadata""#).await,
        3
    );
    assert_eq!(
        scalar::<String>(
            &pool,
            r#"SELECT "meta"."Title"
               FROM "SongMetadata" AS "meta"
               JOIN "AddonMediaListItemSong" AS "link" ON "link"."SongId" = "meta"."SongId"
               WHERE "link"."AddonMediaListItemId" = 10"#
        )
        .await,
        "Pilot"
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "MediaVersion" WHERE "SongId" IS NOT NULL"#
        )
        .await,
        3
    );

    request_and_publish(&repositories, AddonMediaListPresentationKind::RemoteStreams)
        .await
        .expect("revert to remote streams");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Song""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "RemoteStream""#).await,
        3
    );
}

/// Every parentless kind must round-trip through project and revert. This runs
/// them all so one shared implementation cannot work for the kind it was
/// written against and quietly fail for the others.
#[tokio::test]
async fn every_flat_projection_projects_and_reverts() {
    for (presentation, media_table, metadata_table, link_table, version_column) in [
        (
            AddonMediaListPresentationKind::Movies,
            "Movie",
            "MovieMetadata",
            "AddonMediaListItemMovie",
            "MovieId",
        ),
        (
            AddonMediaListPresentationKind::OtherVideos,
            "OtherVideo",
            "OtherVideoMetadata",
            "AddonMediaListItemOtherVideo",
            "OtherVideoId",
        ),
        (
            AddonMediaListPresentationKind::Songs,
            "Song",
            "SongMetadata",
            "AddonMediaListItemSong",
            "SongId",
        ),
        (
            AddonMediaListPresentationKind::Images,
            "Image",
            "ImageMetadata",
            "AddonMediaListItemImage",
            "ImageId",
        ),
    ] {
        let pool = common::test_db().await;
        let _directory = seed_managed_list(&pool).await;
        let repositories = make_repositories(pool.clone());

        request_and_publish(&repositories, presentation)
            .await
            .unwrap_or_else(|error| panic!("{presentation:?} projection failed: {error}"));
        assert_eq!(
            scalar::<i64>(&pool, &format!(r#"SELECT COUNT(*) FROM "{media_table}""#)).await,
            3,
            "{presentation:?} did not create its media items"
        );
        assert_eq!(
            scalar::<i64>(&pool, &format!(r#"SELECT COUNT(*) FROM "{link_table}""#)).await,
            3,
            "{presentation:?} did not link its media items"
        );
        assert_eq!(
            scalar::<i64>(
                &pool,
                &format!(
                    r#"SELECT COUNT(*) FROM "MediaVersion" WHERE "{version_column}" IS NOT NULL"#
                )
            )
            .await,
            3,
            "{presentation:?} did not attach the shared remote stream version"
        );
        assert_projected_item_facets(&pool, metadata_table, link_table, version_column).await;

        request_and_publish(&repositories, AddonMediaListPresentationKind::RemoteStreams)
            .await
            .unwrap_or_else(|error| panic!("{presentation:?} revert failed: {error}"));
        assert_eq!(
            scalar::<i64>(&pool, &format!(r#"SELECT COUNT(*) FROM "{media_table}""#)).await,
            0,
            "{presentation:?} left media items behind"
        );
        assert_eq!(
            scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "RemoteStream""#).await,
            3,
            "{presentation:?} revert damaged the provider items"
        );
    }
}

/// Music Videos is the only projected kind with a parent, because
/// `MusicVideo.ArtistId` is NOT NULL. The managed Artist is named by
/// `artist.nfo` the way a show is named by `tvshow.nfo`.
#[tokio::test]
async fn music_video_projection_creates_one_managed_artist_for_the_list() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    // Without a name there is nothing to call the artist, so publication fails
    // rather than projecting an unnamed parent.
    save_collection_title(&repositories, "").await;
    let error = request_and_publish(&repositories, AddonMediaListPresentationKind::MusicVideos)
        .await
        .expect_err("an unnamed artist must be refused");
    assert!(error.to_string().contains("title"), "{error}");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Artist""#).await,
        0
    );

    save_collection_title(&repositories, "Fixture Artist").await;
    request_and_publish(&repositories, AddonMediaListPresentationKind::MusicVideos)
        .await
        .expect("project the list as music videos");

    // Exactly one artist for the whole list, and every entry hangs off it.
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Artist""#).await,
        1
    );
    assert_eq!(
        scalar::<String>(&pool, r#"SELECT "Title" FROM "ArtistMetadata""#).await,
        "Fixture Artist"
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "MusicVideo""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(DISTINCT "ArtistId") FROM "MusicVideo""#
        )
        .await,
        1
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "MusicVideoMetadata""#).await,
        3
    );
    assert_projected_item_facets(
        &pool,
        "MusicVideoMetadata",
        "AddonMediaListItemMusicVideo",
        "MusicVideoId",
    )
    .await;
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "MediaVersion" WHERE "MusicVideoId" IS NOT NULL"#
        )
        .await,
        3
    );

    // Reverting removes the artist too, and leaves the provider items alone.
    request_and_publish(&repositories, AddonMediaListPresentationKind::RemoteStreams)
        .await
        .expect("revert to remote streams");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Artist""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "MusicVideo""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "RemoteStream""#).await,
        3
    );
}

/// Switching from the artist-parented kind to a parentless one must remove the
/// artist as well, or the library keeps an artist with no music videos.
#[tokio::test]
async fn switching_away_from_music_videos_removes_the_managed_artist() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    save_collection_title(&repositories, "Fixture Artist").await;
    request_and_publish(&repositories, AddonMediaListPresentationKind::MusicVideos)
        .await
        .expect("project the list as music videos");
    request_and_publish(&repositories, AddonMediaListPresentationKind::Movies)
        .await
        .expect("switch the list to movies");

    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Artist""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "MusicVideo""#).await,
        0
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        3
    );
}

#[tokio::test]
async fn nfo_write_failure_restores_files_projection_and_playlist_as_one_revision() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (revision, nfo, playlist_items) =
        prepare_published_revision(&pool, &repositories, &directory).await;

    save_collection_title(&repositories, "Unpublished title").await;
    repositories
        .addon_media_lists
        .stage_publication(LIST_ID)
        .await
        .expect("stage draft");
    let nfo_path = directory.path().join("tvshow.nfo");
    fs::remove_file(&nfo_path).expect("replace staged NFO with failure fixture");
    fs::create_dir(&nfo_path).expect("create unwritable NFO target shape");

    let error = repositories
        .addon_media_lists
        .commit_staged_publication(LIST_ID)
        .await
        .expect_err("NFO write must fail");
    recover_failed_publication(&repositories, &error).await;
    assert_publication_was_restored(&pool, &directory, revision, &nfo, playlist_items).await;
}

#[tokio::test]
async fn artwork_failure_restores_files_projection_and_playlist_as_one_revision() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (revision, nfo, playlist_items) =
        prepare_published_revision(&pool, &repositories, &directory).await;

    let form = repositories
        .addon_media_lists
        .get_show_nfo(LIST_ID)
        .await
        .expect("collection metadata");
    repositories
        .addon_media_lists
        .upload_artwork(
            LIST_ID,
            None,
            AddonMediaListArtworkRole::Fanart,
            "image/png",
            b"\x89PNG\r\n\x1a\nvalid fixture",
            &form.etag,
        )
        .await
        .expect("upload selected fanart");
    repositories
        .addon_media_lists
        .stage_publication(LIST_ID)
        .await
        .expect("stage draft");
    sqlx::query(r#"UPDATE "AddonMediaListArtworkUpload" SET "Bytes" = x'00'"#)
        .execute(&pool)
        .await
        .expect("inject corrupt artwork payload");

    let error = repositories
        .addon_media_lists
        .commit_staged_publication(LIST_ID)
        .await
        .expect_err("invalid staged artwork must fail");
    recover_failed_publication(&repositories, &error).await;
    assert_publication_was_restored(&pool, &directory, revision, &nfo, playlist_items).await;
}

#[tokio::test]
async fn database_commit_failure_restores_files_projection_and_playlist_as_one_revision() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (revision, nfo, playlist_items) =
        prepare_published_revision(&pool, &repositories, &directory).await;

    save_collection_title(&repositories, "Unpublished title").await;
    repositories
        .addon_media_lists
        .stage_publication(LIST_ID)
        .await
        .expect("stage draft");
    sqlx::query(
        r#"CREATE TRIGGER "FailAddonPublicationCommit"
           BEFORE UPDATE OF "ReviewState" ON "AddonMediaList"
           WHEN NEW."ReviewState" = 2
           BEGIN SELECT RAISE(ABORT, 'injected publication commit failure'); END"#,
    )
    .execute(&pool)
    .await
    .expect("install commit failure trigger");

    let error = repositories
        .addon_media_lists
        .commit_staged_publication(LIST_ID)
        .await
        .expect_err("database commit must fail");
    sqlx::query(r#"DROP TRIGGER "FailAddonPublicationCommit""#)
        .execute(&pool)
        .await
        .expect("remove commit failure trigger");
    recover_failed_publication(&repositories, &error).await;
    assert_publication_was_restored(&pool, &directory, revision, &nfo, playlist_items).await;
}

#[tokio::test]
async fn staged_publication_rejects_concurrent_draft_mutations() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (revision, nfo, playlist_items) =
        prepare_published_revision(&pool, &repositories, &directory).await;

    save_collection_title(&repositories, "Unpublished title").await;
    repositories
        .addon_media_lists
        .stage_publication(LIST_ID)
        .await
        .expect("stage draft");

    let form = repositories
        .addon_media_lists
        .get_show_nfo(LIST_ID)
        .await
        .expect("load staged metadata");
    let mut changed = form.metadata;
    changed.title = "Racing edit".to_owned();
    let metadata_error = repositories
        .addon_media_lists
        .save_show_nfo(LIST_ID, &changed, &form.etag)
        .await
        .expect_err("metadata mutation must be locked during publication");
    assert!(metadata_error.to_string().contains("being published"));

    let settings_error = repositories
        .addon_media_lists
        .update_settings(
            LIST_ID,
            true,
            false,
            AddonMediaListContentKind::TelevisionEpisode,
            "",
        )
        .await
        .expect_err("settings mutation must be locked during publication");
    assert!(settings_error.to_string().contains("being published"));

    let second_stage_error = repositories
        .addon_media_lists
        .stage_publication(LIST_ID)
        .await
        .expect_err("a second publication must not replace the active journal");
    assert!(second_stage_error.to_string().contains("being published"));

    recover_failed_publication(&repositories, &metadata_error).await;
    assert_publication_was_restored(&pool, &directory, revision, &nfo, playlist_items).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn uploaded_parent_artwork_is_role_correct_and_preserves_operator_replacements() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let initial = repositories
        .addon_media_lists
        .get_show_nfo(LIST_ID)
        .await
        .expect("collection metadata");
    let poster = repositories
        .addon_media_lists
        .upload_artwork(
            LIST_ID,
            None,
            AddonMediaListArtworkRole::Poster,
            "image/png",
            b"\x89PNG\r\n\x1a\nposter one",
            &initial.etag,
        )
        .await
        .expect("upload poster");
    let stale_error = repositories
        .addon_media_lists
        .upload_artwork(
            LIST_ID,
            None,
            AddonMediaListArtworkRole::Thumb,
            "image/png",
            b"\x89PNG\r\n\x1a\nstale",
            &initial.etag,
        )
        .await
        .expect_err("stale artwork metadata ETag");
    assert!(stale_error.to_string().contains("changed"), "{stale_error}");
    let fanart = repositories
        .addon_media_lists
        .upload_artwork(
            LIST_ID,
            None,
            AddonMediaListArtworkRole::Fanart,
            "image/png",
            b"\x89PNG\r\n\x1a\nfanart one",
            &poster.etag,
        )
        .await
        .expect("upload fanart");
    let thumb = repositories
        .addon_media_lists
        .upload_artwork(
            LIST_ID,
            None,
            AddonMediaListArtworkRole::Thumb,
            "image/png",
            b"\x89PNG\r\n\x1a\nthumb one",
            &fanart.etag,
        )
        .await
        .expect("upload thumbnail");
    assert!(thumb
        .metadata
        .artwork
        .iter()
        .all(|candidate| candidate.url.starts_with("ersatzrs-upload://")));

    request_and_publish(
        &repositories,
        AddonMediaListPresentationKind::TelevisionShow,
    )
    .await
    .expect("publish role artwork");
    for file_name in ["poster.png", "fanart.png", "thumb.png"] {
        assert!(directory.path().join(file_name).is_file(), "{file_name}");
    }
    let nfo = ersatzrs_infra::addon_media_nfo::read_form(Some(
        directory.path().join("tvshow.nfo").as_path(),
    ))
    .expect("published collection NFO");
    for (role, file_name) in [
        (AddonMediaListArtworkRole::Poster, "poster.png"),
        (AddonMediaListArtworkRole::Fanart, "fanart.png"),
        (AddonMediaListArtworkRole::Thumb, "thumb.png"),
    ] {
        assert!(nfo
            .metadata
            .artwork
            .iter()
            .any(|candidate| candidate.role == role && candidate.url == file_name));
    }
    let projected: Vec<(i32, String)> = sqlx::query_as(
        r#"SELECT "artwork"."ArtworkKind", "artwork"."Path"
           FROM "Artwork" AS "artwork"
           JOIN "ShowMetadata" AS "metadata" ON "metadata"."Id" = "artwork"."ShowMetadataId"
           ORDER BY "artwork"."ArtworkKind""#,
    )
    .fetch_all(&pool)
    .await
    .expect("projected role artwork");
    assert_eq!(projected.len(), 3);
    assert_eq!(
        projected.iter().map(|value| value.0).collect::<Vec<_>>(),
        [0, 1, 3]
    );
    assert!(projected.iter().all(|(_, path)| Path::new(path).is_file()));

    let operator_poster = b"\x89PNG\r\n\x1a\noperator replacement";
    fs::write(directory.path().join("poster.png"), operator_poster)
        .expect("replace generated poster");
    let draft = repositories
        .addon_media_lists
        .get_show_nfo(LIST_ID)
        .await
        .expect("draft metadata after publication");
    repositories
        .addon_media_lists
        .upload_artwork(
            LIST_ID,
            None,
            AddonMediaListArtworkRole::Poster,
            "image/png",
            b"\x89PNG\r\n\x1a\nposter two",
            &draft.etag,
        )
        .await
        .expect("select replacement poster");
    repositories
        .addon_media_lists
        .publish(LIST_ID)
        .await
        .expect("republish with operator replacement");
    assert_eq!(
        fs::read(directory.path().join("poster.png")).expect("preserved poster"),
        operator_poster
    );
}
