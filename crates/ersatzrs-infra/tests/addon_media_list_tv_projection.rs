//! Coverage for the add-on media list television projection.
//!
//! `reconcile_tv_projection` builds real `Show`/`Season`/`Episode` rows out of a
//! managed add-on list. It had no test, which is how a projection that omitted
//! `SeasonMetadata` — leaving projected seasons invisible to every consumer that
//! joins metadata to resolve a season title — reached a running system.

mod common;

use std::{collections::BTreeSet, fs, path::Path, path::PathBuf};

use chrono::{Duration, Utc};
use ersatzrs_core::repositories::{
    AddonMediaListArtwork, AddonMediaListArtworkRole, AddonMediaListNfoMetadata,
    AddonMediaListNfoPerson, AddonMediaListPublicationContinuation,
    AddonMediaListPublicationEnqueue, AddonMediaListRepository, ManagedLibraryOperationClaim,
    ManagedLibraryOperationRepository, TelevisionRepository,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListPresentationKind, AddonMediaListPublicationOutcome,
    AddonMediaListPublicationPhase, AddonMediaListRefreshCadence,
};
use ersatzrs_infra::addon_media_list_sync::AddonMediaListSynchronizer;
use ersatzrs_infra::repositories::{
    make_repositories, project_one_pending_detail_batch, Repositories,
};
use sqlx::{Executor, SqlitePool};
use tempfile::TempDir;

const LIST_ID: i64 = 1;

/// Seed a managed add-on list whose items are backed by on-disk stream files,
/// which is what the projection reads NFO metadata from.
async fn seed_managed_list(pool: &SqlitePool) -> TempDir {
    seed_managed_list_items(
        pool,
        &[
            (10_i64, 1_i32, 1_i32, "Pilot".to_owned()),
            (11, 1, 2, "Second".to_owned()),
            (12, 0, 1, "Special".to_owned()),
        ],
    )
    .await
}

async fn seed_managed_list_items(
    pool: &SqlitePool,
    episodes: &[(i64, i32, i32, String)],
) -> TempDir {
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
    sqlx::query(
        r#"INSERT INTO "AddonMediaList" (
              "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "Description", "ItemCount",
              "ProviderMetadataJson", "ReviewState", "PublishedRevision"
           ) VALUES (
              1, 'org.example.lists', 'fixture/show',
              'https://example.invalid/fixture/show', 'Fixture Show', 'Provider introduction', ?,
              '{"title":"Fixture Show","plot":"Provider introduction"}', 2, 1
           )"#,
    )
    .bind(i64::try_from(episodes.len()).expect("fixture episode count fits i64"))
    .execute(pool)
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

    for (item_id, season, episode, title) in episodes {
        seed_list_item(pool, directory.path(), *item_id, *season, *episode, title).await;
    }

    directory
}

#[allow(clippy::too_many_lines)]
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

    let artwork_url = format!("https://example.invalid/artwork/{item_id}.jpg");
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
        artwork: vec![AddonMediaListArtwork {
            url: artwork_url.clone(),
            role: AddonMediaListArtworkRole::Thumb,
            ..AddonMediaListArtwork::default()
        }],
        artwork_urls: vec![artwork_url],
        guids: vec![format!("fixture://episode-{item_id}")],
        ..AddonMediaListNfoMetadata::default()
    };
    // The provider-facing metadata must remain a bounded HTTPS URL for review
    // validation. The already-materialized NFO uses the managed sibling file,
    // reproducing the real publication shape that used to collide with the
    // scanner-known fallback path.
    let mut disk_nfo = nfo.clone();
    disk_nfo.artwork[0].url = stream_path
        .with_extension("jpg")
        .file_name()
        .expect("fixture artwork file name")
        .to_string_lossy()
        .into_owned();
    fs::write(
        &nfo_path,
        ersatzrs_infra::addon_media_nfo::serialize("episodedetails", &disk_nfo)
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
        r#"INSERT INTO "RemoteStream" (
               "Id", "Url", "AddonId", "ProviderId", "Duration", "Liveness", "IsLive"
           ) VALUES (?, ?, 'org.example.lists', ?, '00:22:00', 1, 0)"#,
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
              "CollectionRole", "ProviderMetadataJson", "DurationSeconds", "ProviderLiveness",
              "EffectiveLiveness", "PublishedLiveness"
           ) VALUES (?, 1, ?, ?, 4, ?, ?, ?, ?, ?, 0, 1, ?, ?, 1320, 1, 1, 1)"#,
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

#[tokio::test]
async fn completed_details_are_projected_once_after_a_quiet_batch_window() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;

    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "DetailProjectionPendingAt" = '2999-01-01T00:00:00Z'
           WHERE "Id" = ?"#,
    )
    .bind(LIST_ID)
    .execute(&pool)
    .await
    .expect("mark fresh detail projection");
    assert_eq!(
        project_one_pending_detail_batch(&pool)
            .await
            .expect("check fresh detail batch"),
        None,
        "a detail completion inside the quiet window must not scan immediately"
    );

    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "DetailProjectionPendingAt" = '2026-01-01T00:00:00Z'
           WHERE "Id" = ?"#,
    )
    .bind(LIST_ID)
    .execute(&pool)
    .await
    .expect("age detail projection batch");
    assert_eq!(
        project_one_pending_detail_batch(&pool)
            .await
            .expect("project quiet detail batch"),
        Some((LIST_ID, 1))
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "AddonMediaList"
               WHERE "Id" = 1 AND "DetailProjectionPendingAt" IS NOT NULL"#,
        )
        .await,
        0,
        "the projected batch must be cleared before one coalesced scan is queued"
    );
    assert_eq!(
        project_one_pending_detail_batch(&pool)
            .await
            .expect("check cleared detail batch"),
        None,
        "one quiet batch must not schedule a second scan"
    );
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

async fn nest_scanned_sources(pool: &SqlitePool, directory: &TempDir) -> PathBuf {
    // The default scanner stores streams beneath the manifest directory.
    // First-publication parent NFO resolution must follow the newly bound
    // stream paths, not the pre-scan manifest fallback.
    let streams = directory.path().join("streams");
    fs::create_dir(&streams).expect("create scanner stream directory");
    for entry in fs::read_dir(directory.path()).expect("read seeded stream files") {
        let entry = entry.expect("read seeded entry");
        if entry.file_type().expect("read entry type").is_file() {
            fs::rename(entry.path(), streams.join(entry.file_name())).expect("move stream sidecar");
        }
    }
    let files: Vec<(i64, String)> = sqlx::query_as(r#"SELECT "Id", "Path" FROM "MediaFile""#)
        .fetch_all(pool)
        .await
        .expect("load scanner paths");
    for (id, path) in files {
        sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=?"#)
            .bind(
                streams
                    .join(Path::new(&path).file_name().expect("stream filename"))
                    .to_string_lossy()
                    .as_ref(),
            )
            .bind(id)
            .execute(pool)
            .await
            .expect("record nested scanner path");
    }
    sqlx::query(
        r#"UPDATE "AddonMediaListLibrary" SET "ManifestPath"=? WHERE "AddonMediaListId"=1"#,
    )
    .bind(
        directory
            .path()
            .join("list.playlist.yml")
            .to_string_lossy()
            .as_ref(),
    )
    .execute(pool)
    .await
    .expect("record manifest file path");
    streams
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Exercise both preservation and generated bypass at the publication boundary.
async fn first_parent_publication_preserves_fallback_metadata_before_writing_nested_parent() {
    for (presentation, root, filename) in [
        (
            AddonMediaListPresentationKind::TelevisionShow,
            "tvshow",
            "tvshow.nfo",
        ),
        (
            AddonMediaListPresentationKind::MusicVideos,
            "artist",
            "artist.nfo",
        ),
    ] {
        for state in [
            "user",
            "invalid",
            "untracked-generated",
            "recorded-generated",
            "alias-user",
        ] {
            let pool = common::test_db().await;
            let directory = seed_managed_list(&pool).await;
            let streams = nest_scanned_sources(&pool, &directory).await;
            let repositories = make_repositories(pool.clone());
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PublishedRevision"=0, "ReviewState"=0 WHERE "Id"=1"#)
                .execute(&pool).await.expect("seed first publication");
            sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Published"=0, "MediaItemId"=NULL WHERE "AddonMediaListId"=1"#)
                .execute(&pool).await.expect("leave scanned items unmatched");
            let parent = if state == "alias-user" {
                sqlx::query(r#"UPDATE "AddonMediaListLibrary" SET "ManifestPath"=? WHERE "AddonMediaListId"=1"#)
                    .bind(streams.join("..").join("streams").join("list.playlist.yml").to_string_lossy().as_ref())
                    .execute(&pool).await.expect("equivalent manifest directory spelling");
                &streams
            } else {
                directory.path()
            };
            let path = parent.join(filename);
            let state_path = parent.join(".ersatzrs-nfo-state.json");
            let generated = ersatzrs_infra::addon_media_nfo::serialize(
                root,
                &AddonMediaListNfoMetadata {
                    title: "Fixture Show".to_owned(),
                    plot: Some("Provider introduction".to_owned()),
                    ..AddonMediaListNfoMetadata::default()
                },
            )
            .expect("generated parent");
            fs::write(&path, &generated).expect("fallback document");
            if state != "untracked-generated" {
                ersatzrs_infra::addon_media_nfo::reconcile_generated(&path, &generated)
                    .expect("record generated fallback");
            }
            if matches!(state, "user" | "alias-user") {
                let edited = ersatzrs_infra::addon_media_nfo::serialize(
                    root,
                    &AddonMediaListNfoMetadata {
                        title: "External reviewed title".to_owned(),
                        ..AddonMediaListNfoMetadata::default()
                    },
                )
                .expect("external parent");
                fs::write(&path, edited).expect("external edit");
                ersatzrs_infra::addon_media_nfo::reconcile_generated(&path, &generated)
                    .expect("adopt external edit");
            } else if state == "invalid" {
                fs::write(&path, b"<invalid").expect("invalid external edit");
                assert!(
                    ersatzrs_infra::addon_media_nfo::reconcile_generated(&path, &generated)
                        .is_err()
                );
            }
            let original = fs::read(&path).expect("original fallback");
            let original_state = fs::read(&state_path).ok();
            repositories
                .addon_media_lists
                .request_presentation(LIST_ID, presentation, None)
                .await
                .expect("request presentation");
            repositories
                .addon_media_lists
                .stage_publication(LIST_ID)
                .await
                .expect("stage first publication");
            if matches!(state, "user" | "invalid") {
                for _ in 0..2 {
                    let error = repositories
                        .addon_media_lists
                        .commit_staged_publication(LIST_ID)
                        .await
                        .expect_err("must not bypass original parent metadata, including on retry");
                    assert!(
                        matches!(error,
                            ersatzrs_core::repositories::RepositoryError::Conflict(ref code)
                                if code == "parent_nfo_preservation_required"
                        ),
                        "{error}"
                    );
                    assert!(
                        !streams.join(filename).exists(),
                        "no new parent before preservation check"
                    );
                }
            } else {
                repositories
                    .addon_media_lists
                    .commit_staged_publication(LIST_ID)
                    .await
                    .expect("unchanged generated fallback is safe");
                assert!(streams.join(filename).exists());
            }
            assert_eq!(fs::read(&path).expect("retained fallback"), original);
            if state == "alias-user" {
                // A successful projection adds item records in this same
                // folder. The existing parent ownership itself must not change.
                let before: serde_json::Value = serde_json::from_slice(
                    original_state
                        .as_ref()
                        .expect("parent ownership before publication"),
                )
                .expect("decode original ownership");
                let after: serde_json::Value = serde_json::from_slice(
                    &fs::read(&state_path).expect("ownership after publication"),
                )
                .expect("decode final ownership");
                assert_eq!(after["files"][filename], before["files"][filename]);
            } else {
                assert_eq!(fs::read(&state_path).ok(), original_state);
            }
        }
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // Follow fresh and mixed bindings through staging and repeated publication.
async fn first_tv_publication_binds_scanned_sources_before_projection_without_extra_playlist_work()
{
    for (generate_playlist, missing_bindings) in [(false, 3), (true, 3), (true, 1)] {
        let pool = common::test_db().await;
        let directory = seed_managed_list(&pool).await;
        let streams = nest_scanned_sources(&pool, &directory).await;
        let repositories = make_repositories(pool.clone());
        sqlx::query(
            r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem")
               SELECT 'System', 1 WHERE NOT EXISTS (
                   SELECT 1 FROM "PlaylistGroup" WHERE "IsSystem" = 1
               )"#,
        )
        .execute(&pool)
        .await
        .expect("seed generated playlist group");
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "PublishedRevision"=0,
               "GeneratePlaylist"=?, "ReviewState"=0 WHERE "Id"=1"#,
        )
        .bind(generate_playlist)
        .execute(&pool)
        .await
        .expect("seed first publication");
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "Published"=0,
               "MediaItemId"=CASE WHEN "Id" < 10 + ? THEN NULL ELSE "MediaItemId" END
               WHERE "AddonMediaListId"=1"#,
        )
        .bind(missing_bindings)
        .execute(&pool)
        .await
        .expect("leave scanned items unmatched");
        let expected_title = if missing_bindings == 1 {
            save_collection_title(&repositories, "Reviewed fixture title").await;
            "Reviewed fixture title"
        } else {
            "Fixture Show"
        };
        repositories
            .addon_media_lists
            .request_presentation(
                LIST_ID,
                AddonMediaListPresentationKind::TelevisionShow,
                None,
            )
            .await
            .expect("request first TV presentation");
        repositories
            .addon_media_lists
            .stage_publication(LIST_ID)
            .await
            .expect("stage first publication");

        AddonMediaListSynchronizer::new(pool.clone())
            .match_list_items(LIST_ID)
            .await
            .expect("ordinary matching remains frozen by the journal");
        assert_eq!(scalar::<i64>(&pool,
            r#"SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=1 AND "MediaItemId" IS NULL"#,
        ).await, missing_bindings);
        repositories
            .addon_media_lists
            .commit_staged_publication(LIST_ID)
            .await
            .expect("first TV projection binds already scanned sources");
        assert_eq!(
            ersatzrs_infra::addon_media_nfo::read_form(Some(&streams.join("tvshow.nfo")))
                .expect("read generated parent beside scanned streams")
                .metadata
                .title,
            expected_title,
            "parent generation must retain reviewed metadata at the scanned location"
        );
        assert_eq!(
            scalar::<i64>(
                &pool,
                r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=1"#,
            )
            .await,
            1
        );
        assert_eq!(scalar::<i64>(&pool,
            r#"SELECT COUNT(*) FROM "AddonMediaListItemEpisode" AS "projection"
               JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="projection"."AddonMediaListItemId"
               WHERE "item"."AddonMediaListId"=1 AND "item"."MediaItemId" IS NOT NULL"#,
        ).await, 3);
        let playlist_count = scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Playlist""#).await;
        assert_eq!(playlist_count, i64::from(generate_playlist));
        assert_eq!(scalar::<i64>(&pool,
            r#"SELECT COUNT(*) FROM "PlaylistItem" AS "item"
               JOIN "AddonMediaListItemEpisode" AS "projection" ON "projection"."EpisodeId"="item"."MediaItemId""#,
        ).await, if generate_playlist { 3 } else { 0 });
        let resolutions =
            scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "ScanRefreshResolution""#).await;
        let episode_ids: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode" ORDER BY "AddonMediaListItemId""#,
        ).fetch_all(&pool).await.expect("capture projected episode identities");

        request_and_publish(
            &repositories,
            AddonMediaListPresentationKind::TelevisionShow,
        )
        .await
        .expect("repeat TV publication");
        assert_eq!(
            scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Playlist""#).await,
            playlist_count
        );
        assert_eq!(
            scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "ScanRefreshResolution""#).await,
            resolutions,
            "source binding must not generate intermediate playlist refreshes"
        );
        let repeated: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode" ORDER BY "AddonMediaListItemId""#,
        ).fetch_all(&pool).await.expect("capture repeated episode identities");
        assert_eq!(repeated, episode_ids);
    }
}

#[tokio::test]
async fn first_music_video_and_movie_publications_bind_scanned_sources() {
    for presentation in [
        AddonMediaListPresentationKind::MusicVideos,
        AddonMediaListPresentationKind::Movies,
    ] {
        let pool = common::test_db().await;
        let directory = seed_managed_list(&pool).await;
        let streams = nest_scanned_sources(&pool, &directory).await;
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "PublishedRevision"=0,"ReviewState"=0 WHERE "Id"=1"#,
        )
        .execute(&pool)
        .await
        .expect("seed fresh sibling publication");
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "MediaItemId"=NULL,"Published"=0 WHERE "AddonMediaListId"=1"#)
            .execute(&pool).await.expect("leave sibling sources unmatched");
        let repositories = make_repositories(pool.clone());
        request_and_publish(&repositories, presentation)
            .await
            .expect("publish fresh typed media");
        assert_eq!(
            scalar::<i64>(
                &pool,
                r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=1"#
            )
            .await,
            1
        );
        let count_sql = if presentation == AddonMediaListPresentationKind::MusicVideos {
            assert_eq!(
                ersatzrs_infra::addon_media_nfo::read_form(Some(&streams.join("artist.nfo")))
                    .expect("read artist metadata beside scanned streams")
                    .metadata
                    .title,
                "Fixture Show"
            );
            r#"SELECT COUNT(*) FROM "AddonMediaListItemMusicVideo""#
        } else {
            r#"SELECT COUNT(*) FROM "AddonMediaListItemMovie""#
        };
        assert_eq!(scalar::<i64>(&pool, count_sql).await, 3);
        request_and_publish(&repositories, presentation)
            .await
            .expect("repeat sibling publication");
        assert_eq!(scalar::<i64>(&pool, count_sql).await, 3);
    }
}

#[tokio::test]
async fn restricted_draft_preserves_published_item_until_explicit_publication() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "Availability" = 1, "AvailabilityReason" = 'provider_restriction',
               "AvailabilityReasonCode" = 1
           WHERE "Id" = 10"#,
    )
    .execute(&pool)
    .await
    .expect("mark the draft item restricted");

    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = 10"#,
        )
        .await,
        1,
        "refreshing the draft must preserve current playout membership",
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT "PublishedAvailability" FROM "AddonMediaListItem" WHERE "Id" = 10"#,
        )
        .await,
        0,
        "refreshing the draft must preserve the last published availability snapshot",
    );

    request_and_publish(&repositories, AddonMediaListPresentationKind::RemoteStreams)
        .await
        .expect("publish remaining usable items");

    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "AddonMediaListItem"
               WHERE "ParentProviderId" IS NULL AND "Published" = 1"#,
        )
        .await,
        2,
    );
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = 10"#,
        )
        .await,
        0,
        "the restricted item is excluded only at the explicit publication boundary",
    );
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
async fn television_projection_keeps_unnumbered_nfo_absent_and_uses_relational_sentinel() {
    let pool = common::test_db().await;
    let directory = seed_managed_list_items(&pool, &[(10, 1, 1, "Unnumbered".to_owned())]).await;
    let repositories = make_repositories(pool.clone());
    let metadata = AddonMediaListNfoMetadata {
        title: "Unnumbered".to_owned(),
        ..AddonMediaListNfoMetadata::default()
    };
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "Season" = NULL, "Episode" = NULL, "ProviderMetadataJson" = ?
           WHERE "Id" = 10"#,
    )
    .bind(serde_json::to_string(&metadata).expect("serialize unnumbered metadata"))
    .execute(&pool)
    .await
    .expect("clear provider numbering");
    let nfo_path = directory.path().join("S01E01.nfo");
    fs::write(
        &nfo_path,
        ersatzrs_infra::addon_media_nfo::serialize("episodedetails", &metadata)
            .expect("serialize unnumbered NFO"),
    )
    .expect("write unnumbered NFO");

    request_and_publish(
        &repositories,
        AddonMediaListPresentationKind::TelevisionShow,
    )
    .await
    .expect("project the unnumbered episode");

    let projected_nfo = fs::read_to_string(nfo_path).expect("read projected NFO");
    assert!(!projected_nfo.contains("<season>"), "{projected_nfo}");
    assert!(!projected_nfo.contains("<episode>"), "{projected_nfo}");
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "season"."SeasonNumber"
               FROM "AddonMediaListItemEpisode" AS "projection"
               JOIN "Episode" AS "episode" ON "episode"."Id" = "projection"."EpisodeId"
               JOIN "Season" AS "season" ON "season"."Id" = "episode"."SeasonId"
               WHERE "projection"."AddonMediaListItemId" = 10"#,
        )
        .await,
        0,
    );
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "metadata"."EpisodeNumber"
               FROM "AddonMediaListItemEpisode" AS "projection"
               JOIN "EpisodeMetadata" AS "metadata"
                 ON "metadata"."EpisodeId" = "projection"."EpisodeId"
               WHERE "projection"."AddonMediaListItemId" = 10"#,
        )
        .await,
        0,
    );
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
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "Artwork" WHERE "EpisodeMetadataId" IS NOT NULL"#,
        )
        .await,
        3,
        "one materialized thumbnail must remain per projected episode"
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
    assert_eq!(
        scalar::<i64>(
            &pool,
            &format!(r#"SELECT COUNT(*) FROM "Artwork" WHERE "EpisodeMetadataId" = {metadata_id}"#),
        )
        .await,
        1,
        "the role-aware candidate and legacy fallback resolve to one identity"
    );
}

async fn assert_television_cardinality(pool: &SqlitePool) {
    assert_eq!(
        scalar::<i64>(pool, r#"SELECT COUNT(*) FROM "AddonMediaListItem""#).await,
        27
    );
    assert_eq!(
        scalar::<i64>(
            pool,
            r#"SELECT COUNT(DISTINCT "ProviderId") FROM "AddonMediaListItem""#,
        )
        .await,
        27
    );
    assert_eq!(
        scalar::<i64>(pool, r#"SELECT COUNT(*) FROM "AddonMediaListItemEpisode""#).await,
        27
    );
    assert_eq!(
        scalar::<i64>(
            pool,
            r#"SELECT COUNT(DISTINCT "EpisodeId") FROM "AddonMediaListItemEpisode""#,
        )
        .await,
        27
    );
    assert_eq!(
        scalar::<i64>(pool, r#"SELECT COUNT(*) FROM "EpisodeMetadata""#).await,
        27
    );
    assert_eq!(
        scalar::<i64>(
            pool,
            r#"SELECT COUNT(*) FROM "Artwork" WHERE "EpisodeMetadataId" IS NOT NULL"#,
        )
        .await,
        27,
        "publication must retain one thumbnail identity per episode"
    );

    let seasons = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT season."Id", COUNT(episode."Id")
           FROM "Season" AS season
           LEFT JOIN "Episode" AS episode ON episode."SeasonId" = season."Id"
           GROUP BY season."Id", season."SeasonNumber"
           ORDER BY season."SeasonNumber""#,
    )
    .fetch_all(pool)
    .await
    .expect("load projected season cardinalities");
    assert_eq!(
        seasons.iter().map(|(_, count)| *count).collect::<Vec<_>>(),
        [13, 14]
    );

    let repositories = make_repositories(pool.clone());
    let mut all_episode_ids = BTreeSet::new();
    for (season_id, expected) in seasons {
        assert_eq!(
            repositories
                .television
                .get_episode_count(season_id)
                .await
                .expect("count projected season"),
            expected
        );
        let mut season_episode_ids = BTreeSet::new();
        for page in 1..=4 {
            let cards = repositories
                .television
                .get_paged_episodes(season_id, page, 5)
                .await
                .expect("page projected season");
            for card in cards {
                assert!(
                    season_episode_ids.insert(card.episode_id),
                    "an episode crossed a repository page boundary twice"
                );
                assert!(
                    all_episode_ids.insert(card.episode_id),
                    "an episode appeared in two projected seasons"
                );
            }
        }
        assert_eq!(
            i64::try_from(season_episode_ids.len()).expect("season size fits i64"),
            expected
        );
    }
    assert_eq!(all_episode_ids.len(), 27);
}

/// Release proof for the observed 27 -> 54 failure: two seasons containing 13
/// and 14 episodes remain cardinality-stable across publication, detail refresh,
/// repository pagination, and a real `SQLite` close/reopen boundary.
#[tokio::test]
async fn television_projection_keeps_27_unique_episodes_across_publish_refresh_and_restart() {
    let database_directory = tempfile::tempdir().expect("cardinality database directory");
    let database_path = database_directory.path().join("cardinality.db");
    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("open cardinality database");
    pool.execute("PRAGMA foreign_keys = ON")
        .await
        .expect("enable cardinality foreign keys");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("migrate cardinality database");

    let episodes = (1..=13)
        .map(|episode| {
            (
                1_000_i64 + i64::from(episode),
                1,
                episode,
                format!("Season 1 Episode {episode}"),
            )
        })
        .chain((1..=14).map(|episode| {
            (
                1_100_i64 + i64::from(episode),
                2,
                episode,
                format!("Season 2 Episode {episode}"),
            )
        }))
        .collect::<Vec<_>>();
    let _managed_directory = seed_managed_list_items(&pool, &episodes).await;
    let repositories = make_repositories(pool.clone());

    request_and_publish(
        &repositories,
        AddonMediaListPresentationKind::TelevisionShow,
    )
    .await
    .expect("publish 27-episode projection");
    assert_television_cardinality(&pool).await;

    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "DetailProjectionPendingAt" = '2026-01-01T00:00:00Z'
           WHERE "Id" = ?"#,
    )
    .bind(LIST_ID)
    .execute(&pool)
    .await
    .expect("queue detail projection refresh");
    assert_eq!(
        project_one_pending_detail_batch(&pool)
            .await
            .expect("run detail projection refresh"),
        Some((LIST_ID, 1))
    );
    request_and_publish(
        &repositories,
        AddonMediaListPresentationKind::TelevisionShow,
    )
    .await
    .expect("republish refreshed 27-episode projection");
    assert_television_cardinality(&pool).await;

    drop(repositories);
    pool.close().await;
    let reopened = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("reopen cardinality database");
    reopened
        .execute("PRAGMA foreign_keys = ON")
        .await
        .expect("enable reopened foreign keys");
    assert_television_cardinality(&reopened).await;
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

/// Reclassification is a publication of the existing control-plane draft, not
/// another provider import. Operator metadata and provider identity must stay
/// intact while the projected media hierarchy changes underneath them.
#[tokio::test]
async fn published_list_can_change_media_type_without_reimporting_or_losing_edits() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());

    let detail = repositories
        .addon_media_lists
        .get_item_detail(LIST_ID, 10)
        .await
        .expect("load item detail")
        .expect("seeded item");
    let mut edited = detail.nfo.metadata;
    edited.title = "Operator title".to_owned();
    edited.tags.push("Operator tag".to_owned());
    repositories
        .addon_media_lists
        .save_item_nfo(LIST_ID, 10, &edited, &detail.nfo.etag)
        .await
        .expect("save operator metadata");

    let control_plane_before =
        sqlx::query_as::<_, (i64, String, String, i64, i64, i64, String, String)>(
            r#"SELECT "Id", "ProviderId", "SourceUrl", "Rank", "Included", "CollectionRole",
                  "ProviderMetadataJson", "MetadataOverrideJson"
           FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 1 ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .expect("snapshot imported items");

    request_and_publish(
        &repositories,
        AddonMediaListPresentationKind::TelevisionShow,
    )
    .await
    .expect("reclassify remote streams as television");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Episode""#).await,
        3
    );

    request_and_publish(&repositories, AddonMediaListPresentationKind::Movies)
        .await
        .expect("reclassify the same imported items as movies");
    let reclassified = repositories
        .addon_media_lists
        .get_admin_form(LIST_ID)
        .await
        .expect("reload list classification")
        .expect("seeded list");
    assert_eq!(
        reclassified.presentation_kind,
        AddonMediaListPresentationKind::Movies
    );
    assert_eq!(
        reclassified.requested_presentation_kind,
        AddonMediaListPresentationKind::Movies,
        "publication must not replace the operator's type choice while reverting the old projection"
    );
    let control_plane_after =
        sqlx::query_as::<_, (i64, String, String, i64, i64, i64, String, String)>(
            r#"SELECT "Id", "ProviderId", "SourceUrl", "Rank", "Included", "CollectionRole",
                  "ProviderMetadataJson", "MetadataOverrideJson"
           FROM "AddonMediaListItem" WHERE "AddonMediaListId" = 1 ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .expect("reload imported items");
    assert_eq!(control_plane_after, control_plane_before);
    let retained = repositories
        .addon_media_lists
        .get_item_detail(LIST_ID, 10)
        .await
        .expect("reload item detail")
        .expect("retained item");
    assert_eq!(retained.nfo.metadata.title, "Operator title");
    assert!(retained
        .nfo
        .metadata
        .tags
        .contains(&"Operator tag".to_owned()));
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Movie""#).await,
        3
    );
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "Show""#).await,
        0
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
#[allow(clippy::too_many_lines)]
async fn staged_publication_rejects_concurrent_draft_mutations() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (revision, nfo, playlist_items) =
        prepare_published_revision(&pool, &repositories, &directory).await;

    save_collection_title(&repositories, "Unpublished title").await;
    let draft = repositories
        .addon_media_lists
        .get_admin_form(LIST_ID)
        .await
        .expect("load draft")
        .expect("list exists");
    let enqueued = repositories
        .managed_library_operations
        .enqueue_publication(
            LIST_ID,
            1,
            draft.draft_revision,
            draft.published_revision.saturating_add(1),
        )
        .await
        .expect("enqueue publication");
    let claim = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, Utc::now())
        .await
        .expect("claim publication")
        .expect("publication is claimable");
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &claim,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .expect("enter staging"));
    // Automatic work cannot use the ordinary publication lease to bypass
    // additions-only validation, even when the saved revision still matches.
    sqlx::query(r#"UPDATE "AddonMediaListPublicationEvent" SET "EnqueueTrigger" = 2 WHERE "PublicationJobId" = ? AND "EnqueueTrigger" = 0"#)
        .bind(enqueued.job.id).execute(&pool).await.expect("mark automatic admission");
    sqlx::query(r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy" = 1 WHERE "Id" = 1"#)
        .execute(&pool)
        .await
        .expect("enable automatic publication");
    sqlx::query(r#"INSERT INTO "AddonMediaListAutoPublication" (
        "AddonMediaListId", "DraftRevision", "PublishedRevision", "State", "RecordedAt", "OperationId"
    ) VALUES (1, ?, ?, 'queued', ?, ?)"#)
        .bind(draft.draft_revision).bind(draft.published_revision)
        .bind(Utc::now().to_rfc3339()).bind(enqueued.operation.id)
        .execute(&pool).await.expect("seed exact automatic candidate");
    let error = repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            enqueued.job.saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect_err("existing title changes must remain review-only");
    assert!(error.to_string().contains("requires review before staging"));
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "AddonMediaList" WHERE "PublicationJournalJson" IS NOT NULL"#
        )
        .await,
        0
    );
    sqlx::query(r#"UPDATE "AddonMediaListPublicationEvent" SET "EnqueueTrigger" = 0 WHERE "PublicationJobId" = ? AND "EnqueueTrigger" = 2"#)
        .bind(enqueued.job.id).execute(&pool).await.expect("restore explicit operator admission");
    sqlx::query(r#"UPDATE "AddonMediaList" SET "RefreshPublicationPolicy" = 0 WHERE "Id" = 1"#)
        .execute(&pool)
        .await
        .expect("restore review policy");
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            enqueued.job.saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
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
            AddonMediaListRefreshCadence::Daily,
            ersatzrs_domain::AddonMediaListRefreshPublicationPolicy::AutoPublishAdditions,
            ersatzrs_domain::AddonMediaListDeliveryMode::Stream,
            false,
            AddonMediaListContentKind::TelevisionEpisode,
            "",
        )
        .await
        .expect_err("settings mutation must be locked during publication");
    assert!(settings_error.to_string().contains("being published"));
    let unchanged = repositories
        .addon_media_lists
        .get_admin_form(LIST_ID)
        .await
        .expect("read locked settings")
        .expect("list exists");
    assert_eq!(
        unchanged.refresh_publication_policy,
        ersatzrs_domain::AddonMediaListRefreshPublicationPolicy::Review
    );
    let item_error = repositories
        .addon_media_lists
        .set_item_included(LIST_ID, 10, true)
        .await
        .expect_err("item mutation must be locked during publication");
    assert!(item_error.to_string().contains("being published"));

    sqlx::query(
        r#"UPDATE "ManagedLibraryLease"
           SET "LeaseExpiresAt" = '2020-01-01T00:00:00Z'
           WHERE "OperationId" = ?"#,
    )
    .bind(enqueued.operation.id)
    .execute(&pool)
    .await
    .expect("expire the publication lease");
    let expired_form = repositories
        .addon_media_lists
        .get_show_nfo(LIST_ID)
        .await
        .expect("load metadata after expiry");
    let mut expired_change = expired_form.metadata;
    expired_change.title = "Saved while recovery is required".to_owned();
    repositories
        .addon_media_lists
        .save_show_nfo(LIST_ID, &expired_change, &expired_form.etag)
        .await
        .expect("an expired lease must not keep the draft locked");
    repositories
        .addon_media_lists
        .update_settings(
            LIST_ID,
            AddonMediaListRefreshCadence::Daily,
            ersatzrs_domain::AddonMediaListRefreshPublicationPolicy::Review,
            ersatzrs_domain::AddonMediaListDeliveryMode::Stream,
            false,
            AddonMediaListContentKind::TelevisionEpisode,
            "",
        )
        .await
        .expect("list settings must save while recovery is required");
    repositories
        .addon_media_lists
        .set_item_included(LIST_ID, 10, true)
        .await
        .expect("item mutations must save while recovery is required");
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "AddonMediaList"
               WHERE "Id" = 1 AND "ReviewState" = 1
                 AND "PublicationJournalJson" IS NOT NULL"#,
        )
        .await,
        1,
    );

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
#[allow(clippy::too_many_lines)] // Follow one stale worker through both publication boundaries.
async fn durable_publication_fences_stale_workers_at_stage_and_commit() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    pool.execute(r#"UPDATE "AddonMediaListItem" SET "InDraft"=0 WHERE "Id"=11"#)
        .await
        .expect("retire an item from the new draft");
    let retained: (bool, bool) =
        sqlx::query_as(r#"SELECT "Included", "Published" FROM "AddonMediaListItem" WHERE "Id"=11"#)
            .fetch_one(&pool)
            .await
            .expect("previous publication remains selected before staging");
    assert_eq!(retained, (true, true));
    let (enqueued, stale, current) = claim_and_reclaim_publication(&repositories).await;
    assert_stale_publication_claim_is_fenced(&repositories, &enqueued, &stale, &current).await;
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT "Published" FROM "AddonMediaListItem" WHERE "Id"=11"#,
        )
        .await,
        0,
        "the new publication excludes retained owners absent from its draft"
    );

    repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("the current claimant commits publication");
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        enqueued.job.target_published_revision
    );
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        3,
        "a durable commit remains publishing until cleanup finalizes"
    );
    let journal_json = scalar::<String>(
        &pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode retained publication journal");
    let backup = journal["backup_folder"]
        .as_str()
        .map(PathBuf::from)
        .expect("journal backup folder");
    assert!(backup.is_dir());

    assert!(repositories
        .managed_library_operations
        .begin_publication_verification(
            &current,
            AddonMediaListPublicationPhase::Staging,
            AddonMediaListPublicationOutcome::Published,
        )
        .await
        .expect("enter publication verification"));
    repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("verify the committed revision");
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &current,
            AddonMediaListPublicationPhase::Verifying,
            AddonMediaListPublicationPhase::CleaningUp,
        )
        .await
        .expect("enter publication cleanup"));
    finalize_published_revision(&repositories, &enqueued, &current).await;
    assert_active_publication_handoff_blocks_delete(&repositories).await;
    finalize_published_revision(&repositories, &enqueued, &current).await;
    assert!(!backup.exists());
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "AddonMediaList"
               WHERE "Id" = 1 AND "PublicationJournalJson" IS NOT NULL"#,
        )
        .await,
        0
    );
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        2,
        "successful cleanup publishes atomically with journal removal"
    );
    assert_eq!(
        scalar::<String>(
            &pool,
            r#"SELECT "revision"."CompletedAt"
               FROM "AddonMediaListRevision" AS "revision"
               WHERE "revision"."AddonMediaListId" = 1
                 AND "revision"."Revision" = 2"#,
        )
        .await,
        scalar::<String>(
            &pool,
            r#"SELECT "event"."OccurredAt"
               FROM "AddonMediaListPublicationEvent" AS "event"
               JOIN "AddonMediaListPublicationJob" AS "job"
                 ON "job"."Id" = "event"."PublicationJobId"
               WHERE "job"."AddonMediaListId" = 1
                 AND "job"."TargetPublishedRevision" = 2
                 AND "event"."FilesystemTransition" = 4"#,
        )
        .await,
        "the retained revision uses the stable verification event timestamp"
    );
}

#[cfg(unix)]
#[tokio::test]
#[allow(clippy::too_many_lines)] // Keep failure injection and the subsequent monotonic restore together.
async fn cleanup_failure_restore_does_not_reuse_the_retained_revision_number() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = prepare_verifiable_tv_publication().await;
    verify_publication_candidate(&fixture)
        .await
        .expect("verify first candidate");
    assert!(fixture
        .repositories
        .managed_library_operations
        .transition_publication(
            &fixture.claim,
            AddonMediaListPublicationPhase::Verifying,
            AddonMediaListPublicationPhase::CleaningUp,
        )
        .await
        .expect("enter first cleanup"));
    let journal_json = scalar::<String>(
        &fixture.pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode first publication journal");
    let backup = PathBuf::from(journal["backup_folder"].as_str().expect("backup path"));
    let original_permissions = fs::metadata(&backup)
        .expect("backup metadata")
        .permissions();
    let mut blocked_permissions = original_permissions.clone();
    blocked_permissions.set_mode(0o0);
    fs::set_permissions(&backup, blocked_permissions).expect("block backup cleanup");
    let cleanup_error = fixture
        .repositories
        .addon_media_lists
        .finalize_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            fixture.enqueued.operation.id,
            fixture.enqueued.job.generation,
            &fixture.claim.operation.owner_token,
            &fixture.claim.lease_owner,
        )
        .await
        .expect_err("inaccessible backup must interrupt cleanup after retention");
    fs::set_permissions(&backup, original_permissions).expect("unblock backup recovery");
    assert!(cleanup_error.to_string().contains("publication"));
    assert_eq!(
        scalar::<i64>(
            &fixture.pool,
            r#"SELECT COUNT(*) FROM "AddonMediaListRevision"
               WHERE "AddonMediaListId" = 1 AND "Revision" = 2"#,
        )
        .await,
        1,
        "the rolled-back candidate snapshot remains immutable history; cleanup error: {cleanup_error}"
    );

    fixture
        .repositories
        .addon_media_lists
        .restore_staged_publication_revision(
            LIST_ID,
            "injected cleanup failure",
            fixture.enqueued.operation.id,
            fixture.enqueued.job.generation,
            &fixture.claim.operation.owner_token,
            &fixture.claim.lease_owner,
        )
        .await
        .expect("restore previous files");
    fixture
        .repositories
        .addon_media_lists
        .reconcile_restored_publication_revision(
            LIST_ID,
            fixture.enqueued.operation.id,
            fixture.enqueued.job.generation,
            &fixture.claim.operation.owner_token,
            &fixture.claim.lease_owner,
        )
        .await
        .expect("restore previous projection");
    assert!(fixture
        .repositories
        .managed_library_operations
        .begin_publication_verification(
            &fixture.claim,
            AddonMediaListPublicationPhase::CleaningUp,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
        )
        .await
        .expect("enter restored verification"));
    fixture
        .repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            fixture.enqueued.operation.id,
            fixture.enqueued.job.generation,
            &fixture.claim.operation.owner_token,
            &fixture.claim.lease_owner,
        )
        .await
        .expect("verify restored publication");
    assert!(fixture
        .repositories
        .managed_library_operations
        .transition_publication(
            &fixture.claim,
            AddonMediaListPublicationPhase::Verifying,
            AddonMediaListPublicationPhase::CleaningUp,
        )
        .await
        .expect("enter restored cleanup"));
    fixture
        .repositories
        .addon_media_lists
        .finalize_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            fixture.enqueued.operation.id,
            fixture.enqueued.job.generation,
            &fixture.claim.operation.owner_token,
            &fixture.claim.lease_owner,
        )
        .await
        .expect("finalize restored publication");
    assert!(fixture
        .repositories
        .managed_library_operations
        .complete_publication(
            &fixture.claim,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
        )
        .await
        .expect("complete restored publication"));

    save_collection_title(&fixture.repositories, "Different publication").await;
    let form = fixture
        .repositories
        .addon_media_lists
        .get_admin_form(LIST_ID)
        .await
        .expect("load different draft")
        .expect("list exists");
    let next = fixture
        .repositories
        .managed_library_operations
        .enqueue_publication(LIST_ID, 1, form.draft_revision, form.published_revision + 1)
        .await
        .expect("enqueue different publication");
    assert_eq!(form.published_revision, 1);
    assert_eq!(next.job.target_published_revision, 3);
    let next_claim = fixture
        .repositories
        .managed_library_operations
        .claim_operation(next.operation.id, Utc::now())
        .await
        .expect("claim different publication")
        .expect("different publication is claimable");
    assert!(fixture
        .repositories
        .managed_library_operations
        .transition_publication(
            &next_claim,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .expect("enter different staging"));
    fixture
        .repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            next.job.saved_revision,
            next.operation.id,
            next.job.generation,
            &next_claim.operation.owner_token,
            &next_claim.lease_owner,
        )
        .await
        .expect("stage different publication at queued target");
    fixture
        .repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            next.job.target_published_revision,
            next.operation.id,
            next.job.generation,
            &next_claim.operation.owner_token,
            &next_claim.lease_owner,
        )
        .await
        .expect("commit different monotonic revision");
    assert!(fixture
        .repositories
        .managed_library_operations
        .begin_publication_verification(
            &next_claim,
            AddonMediaListPublicationPhase::Staging,
            AddonMediaListPublicationOutcome::Published,
        )
        .await
        .expect("enter different verification"));
    fixture
        .repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            next.operation.id,
            next.job.generation,
            &next_claim.operation.owner_token,
            &next_claim.lease_owner,
        )
        .await
        .expect("verify different publication");
    assert!(fixture
        .repositories
        .managed_library_operations
        .transition_publication(
            &next_claim,
            AddonMediaListPublicationPhase::Verifying,
            AddonMediaListPublicationPhase::CleaningUp,
        )
        .await
        .expect("enter different cleanup"));
    fixture
        .repositories
        .addon_media_lists
        .finalize_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            next.operation.id,
            next.job.generation,
            &next_claim.operation.owner_token,
            &next_claim.lease_owner,
        )
        .await
        .expect("finalize different publication");
    assert!(fixture
        .repositories
        .managed_library_operations
        .complete_publication(&next_claim, AddonMediaListPublicationOutcome::Published)
        .await
        .expect("complete different publication"));
    assert_eq!(
        scalar::<i32>(
            &fixture.pool,
            r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        3
    );
    assert_eq!(
        scalar::<i64>(
            &fixture.pool,
            r#"SELECT COUNT(*) FROM "AddonMediaListRevision"
               WHERE "AddonMediaListId" = 1 AND "Revision" IN (2, 3)"#,
        )
        .await,
        2,
        "the restored-away revision and later success retain distinct identities"
    );
}

#[tokio::test]
async fn journal_less_legacy_commit_is_terminal_only_for_the_current_exact_claim() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (enqueued, stale, current) = claim_and_reclaim_publication(&repositories).await;
    assert_stale_publication_claim_is_fenced(&repositories, &enqueued, &stale, &current).await;

    let continuation = repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("commit the durable publication");
    assert_eq!(
        continuation,
        AddonMediaListPublicationContinuation::VerificationRequired
    );
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "ReviewState" = 2, "PublicationJournalJson" = NULL,
               "PublicationStartedAt" = NULL
           WHERE "Id" = 1"#,
    )
    .execute(&pool)
    .await
    .expect("simulate AMM-J-01 terminal commit cleanup");

    let stale_error = repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &stale.operation.owner_token,
            &stale.lease_owner,
        )
        .await
        .expect_err("a stale claimant cannot adopt the terminal legacy state");
    assert!(stale_error
        .to_string()
        .contains("lease is no longer current"));
    let continuation = repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("the current claimant adopts the terminal legacy state");
    assert_eq!(
        continuation,
        AddonMediaListPublicationContinuation::CompletedLegacy
    );
}

#[tokio::test]
async fn journal_less_legacy_reconcile_is_terminal_only_at_the_prior_revision() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (enqueued, _stale, current) = claim_and_reclaim_publication(&repositories).await;
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            enqueued.job.saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("stage publication recovery fixture");
    repositories
        .addon_media_lists
        .restore_staged_publication_revision(
            LIST_ID,
            "injected legacy recovery",
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("restore the prior files and snapshot");
    let continuation = repositories
        .addon_media_lists
        .reconcile_restored_publication_revision(
            LIST_ID,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("reconcile the restored projection");
    assert_eq!(
        continuation,
        AddonMediaListPublicationContinuation::VerificationRequired
    );
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "ReviewState" = 1, "PublicationJournalJson" = NULL,
               "PublicationStartedAt" = NULL
           WHERE "Id" = 1"#,
    )
    .execute(&pool)
    .await
    .expect("simulate AMM-J-01 terminal reconcile cleanup");

    let continuation = repositories
        .addon_media_lists
        .reconcile_restored_publication_revision(
            LIST_ID,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("adopt the exact terminal legacy recovery state");
    assert_eq!(
        continuation,
        AddonMediaListPublicationContinuation::CompletedLegacy
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn durable_publication_commit_rechecks_the_lease_in_its_write_transaction() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    prepare_published_revision(&pool, &repositories, &directory).await;
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "GeneratePlaylist" = 0, "ReviewState" = 1,
               "DraftRevision" = "DraftRevision" + 1
           WHERE "Id" = 1"#,
    )
    .execute(&pool)
    .await
    .expect("prepare a draft that removes the generated playlist");
    let saved_revision = scalar::<i32>(
        &pool,
        r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let published_revision = scalar::<i32>(
        &pool,
        r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let enqueued = repositories
        .managed_library_operations
        .enqueue_publication(
            LIST_ID,
            1,
            saved_revision,
            published_revision.saturating_add(1),
        )
        .await
        .expect("enqueue exact publication revision");
    let claim = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, Utc::now())
        .await
        .expect("claim publication")
        .expect("publication is claimable");
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &claim,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .expect("enter staging"));
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("stage publication");

    let trigger = format!(
        r#"CREATE TRIGGER "ExpirePublicationLeaseDuringCommit"
           AFTER UPDATE OF "PlaylistId" ON "AddonMediaList"
           WHEN OLD."PlaylistId" IS NOT NULL AND NEW."PlaylistId" IS NULL
           BEGIN
             UPDATE "ManagedLibraryLease"
             SET "LeaseExpiresAt" = '2000-01-01T00:00:00Z'
             WHERE "OperationId" = {};
           END"#,
        enqueued.operation.id
    );
    sqlx::query(sqlx::AssertSqlSafe(trigger))
        .execute(&pool)
        .await
        .expect("install lease-expiry trigger");
    let error = repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect_err("the final commit must reject a lease lost inside its transaction");
    assert!(error.to_string().contains("changed before commit"));
    sqlx::query(r#"DROP TRIGGER "ExpirePublicationLeaseDuringCommit""#)
        .execute(&pool)
        .await
        .expect("remove lease-expiry trigger");
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        published_revision
    );
    assert!(scalar::<Option<i64>>(
        &pool,
        r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await
    .is_some());

    repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("the current claimant can retry the rolled-back commit");
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn durable_publication_restore_rechecks_the_lease_in_its_write_transaction() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (enqueued, _stale, current) = claim_and_reclaim_publication(&repositories).await;
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            enqueued.job.saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("stage publication");

    let trigger = format!(
        r#"CREATE TRIGGER "ExpirePublicationLeaseDuringRestore"
           AFTER UPDATE OF "Published" ON "AddonMediaListItem"
           BEGIN
             UPDATE "ManagedLibraryLease"
             SET "LeaseExpiresAt" = '2000-01-01T00:00:00Z'
             WHERE "OperationId" = {};
           END"#,
        enqueued.operation.id
    );
    sqlx::query(sqlx::AssertSqlSafe(trigger))
        .execute(&pool)
        .await
        .expect("install lease-expiry trigger");
    let error = repositories
        .addon_media_lists
        .restore_staged_publication_revision(
            LIST_ID,
            "injected recovery",
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect_err("the final restore must reject a lease lost inside its transaction");
    assert!(error.to_string().contains("changed before restoration"));
    sqlx::query(r#"DROP TRIGGER "ExpirePublicationLeaseDuringRestore""#)
        .execute(&pool)
        .await
        .expect("remove lease-expiry trigger");
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        3
    );

    repositories
        .addon_media_lists
        .restore_staged_publication_revision(
            LIST_ID,
            "injected recovery",
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("the current claimant can retry the rolled-back restoration");
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn durable_publication_reconcile_fences_a_lease_lost_after_projection() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    prepare_published_revision(&pool, &repositories, &directory).await;
    save_collection_title(&repositories, "Unpublished title").await;
    let saved_revision = scalar::<i32>(
        &pool,
        r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let published_revision = scalar::<i32>(
        &pool,
        r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let enqueued = repositories
        .managed_library_operations
        .enqueue_publication(
            LIST_ID,
            1,
            saved_revision,
            published_revision.saturating_add(1),
        )
        .await
        .expect("enqueue exact publication revision");
    let claim = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, Utc::now())
        .await
        .expect("claim publication")
        .expect("publication is claimable");
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &claim,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .expect("enter staging"));
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("stage publication");
    repositories
        .addon_media_lists
        .restore_staged_publication_revision(
            LIST_ID,
            "injected recovery",
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("restore staged publication");

    let trigger = format!(
        r#"CREATE TRIGGER "ExpirePublicationLeaseDuringReconcile"
           AFTER UPDATE OF "PlaylistId" ON "AddonMediaList"
           BEGIN
             UPDATE "ManagedLibraryLease"
             SET "LeaseExpiresAt" = '2000-01-01T00:00:00Z'
             WHERE "OperationId" = {};
           END"#,
        enqueued.operation.id
    );
    sqlx::query(sqlx::AssertSqlSafe(trigger))
        .execute(&pool)
        .await
        .expect("install lease-expiry trigger");
    let error = repositories
        .addon_media_lists
        .reconcile_restored_publication_revision(
            LIST_ID,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect_err("the final reconciliation must reject a lease lost after projection");
    assert!(error.to_string().contains("lease is no longer current"));
    sqlx::query(r#"DROP TRIGGER "ExpirePublicationLeaseDuringReconcile""#)
        .execute(&pool)
        .await
        .expect("remove lease-expiry trigger");
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "AddonMediaList"
               WHERE "Id" = 1 AND "PublicationJournalJson" IS NOT NULL"#,
        )
        .await,
        1
    );

    let reclaimed = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, Utc::now() + Duration::minutes(2))
        .await
        .expect("reclaim publication")
        .expect("expired publication is reclaimable");
    repositories
        .addon_media_lists
        .reconcile_restored_publication_revision(
            LIST_ID,
            enqueued.operation.id,
            enqueued.job.generation,
            &reclaimed.operation.owner_token,
            &reclaimed.lease_owner,
        )
        .await
        .expect("the new claimant finishes reconciliation");
    assert!(repositories
        .managed_library_operations
        .begin_publication_verification(
            &reclaimed,
            AddonMediaListPublicationPhase::Staging,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
        )
        .await
        .expect("enter restored revision verification"));
    repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            enqueued.operation.id,
            enqueued.job.generation,
            &reclaimed.operation.owner_token,
            &reclaimed.lease_owner,
        )
        .await
        .expect("verify the restored revision");
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        3,
        "restored durable work remains publishing through verification"
    );
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &reclaimed,
            AddonMediaListPublicationPhase::Verifying,
            AddonMediaListPublicationPhase::CleaningUp,
        )
        .await
        .expect("enter restored revision cleanup"));
    repositories
        .addon_media_lists
        .finalize_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            enqueued.operation.id,
            enqueued.job.generation,
            &reclaimed.operation.owner_token,
            &reclaimed.lease_owner,
        )
        .await
        .expect("finalize the restored revision");
    assert_eq!(
        scalar::<i64>(
            &pool,
            r#"SELECT COUNT(*) FROM "AddonMediaList"
               WHERE "Id" = 1 AND "PublicationJournalJson" IS NOT NULL"#,
        )
        .await,
        0
    );
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        1,
        "restored cleanup atomically returns the prior draft to review"
    );
}

async fn claim_and_reclaim_publication(
    repositories: &Repositories,
) -> (
    AddonMediaListPublicationEnqueue,
    ManagedLibraryOperationClaim,
    ManagedLibraryOperationClaim,
) {
    let enqueued = repositories
        .managed_library_operations
        .enqueue_publication(LIST_ID, 1, 0, 2)
        .await
        .expect("enqueue exact publication revision");
    let claimed_at = Utc::now();
    let stale = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, claimed_at)
        .await
        .expect("claim publication")
        .expect("publication is claimable");
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &stale,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .expect("enter staging"));
    let current = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, claimed_at + Duration::minutes(2))
        .await
        .expect("reclaim expired publication")
        .expect("expired publication is reclaimable");

    (enqueued, stale, current)
}

async fn finalize_published_revision(
    repositories: &Repositories,
    enqueued: &AddonMediaListPublicationEnqueue,
    claim: &ManagedLibraryOperationClaim,
) {
    repositories
        .addon_media_lists
        .finalize_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("finalize verified publication");
}

async fn assert_active_publication_handoff_blocks_delete(repositories: &Repositories) {
    let error = repositories
        .addon_media_lists
        .delete(LIST_ID)
        .await
        .expect_err("the active operation owns the terminal handoff window");
    assert!(error.to_string().contains("being published"));
}

struct VerifiablePublication {
    pool: SqlitePool,
    directory: TempDir,
    repositories: Repositories,
    enqueued: AddonMediaListPublicationEnqueue,
    claim: ManagedLibraryOperationClaim,
}

async fn configure_verifiable_tv_publication(pool: &SqlitePool, repositories: &Repositories) {
    sqlx::query(r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem") VALUES ('System', 1)"#)
        .execute(pool)
        .await
        .expect("seed system playlist group");
    repositories
        .addon_media_lists
        .request_presentation(
            LIST_ID,
            AddonMediaListPresentationKind::TelevisionShow,
            None,
        )
        .await
        .expect("request television presentation");
    repositories
        .addon_media_lists
        .update_settings(
            LIST_ID,
            AddonMediaListRefreshCadence::Daily,
            ersatzrs_domain::AddonMediaListRefreshPublicationPolicy::Review,
            ersatzrs_domain::AddonMediaListDeliveryMode::Stream,
            true,
            AddonMediaListContentKind::TelevisionEpisode,
            "",
        )
        .await
        .expect("enable generated playlist");
    let form = repositories
        .addon_media_lists
        .get_show_nfo(LIST_ID)
        .await
        .expect("load list metadata");
    repositories
        .addon_media_lists
        .upload_artwork(
            LIST_ID,
            None,
            AddonMediaListArtworkRole::Poster,
            "image/png",
            b"\x89PNG\r\n\x1a\npublication verifier poster",
            &form.etag,
        )
        .await
        .expect("upload parent artwork");
}

async fn prepare_verifiable_tv_publication() -> VerifiablePublication {
    prepare_verifiable_tv_publication_with_nested_streams(false).await
}

#[allow(clippy::too_many_lines)] // Keep the optional scanner layout and sealed candidate setup together.
async fn prepare_verifiable_tv_publication_with_nested_streams(
    nested_streams: bool,
) -> VerifiablePublication {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    if nested_streams {
        // The real playlist scanner writes stream definitions, NFOs and item
        // artwork under the manifest's `folder: streams`, not beside the manifest.
        let streams = directory.path().join("streams");
        fs::create_dir(&streams).expect("create scanner-style stream directory");
        for entry in fs::read_dir(directory.path()).expect("list seeded stream files") {
            let entry = entry.expect("read seeded stream entry");
            if entry
                .file_type()
                .expect("read fixture entry type")
                .is_file()
            {
                fs::rename(entry.path(), streams.join(entry.file_name()))
                    .expect("move seeded stream file into scanner directory");
            }
        }
        let files: Vec<(i64, String)> =
            sqlx::query_as(r#"SELECT "Id", "Path" FROM "MediaFile" ORDER BY "Id""#)
                .fetch_all(&pool)
                .await
                .expect("load scanner fixture media paths");
        for (id, path) in files {
            let path = streams.join(Path::new(&path).file_name().expect("stream file name"));
            sqlx::query(r#"UPDATE "MediaFile" SET "Path" = ? WHERE "Id" = ?"#)
                .bind(path.to_string_lossy().as_ref())
                .bind(id)
                .execute(&pool)
                .await
                .expect("record scanner-style media path");
        }
        sqlx::query(
            r#"UPDATE "AddonMediaListLibrary" SET "ManifestPath" = ? WHERE "AddonMediaListId" = 1"#,
        )
        .bind(
            directory
                .path()
                .join("list.playlist.yml")
                .to_string_lossy()
                .as_ref(),
        )
        .execute(&pool)
        .await
        .expect("record manifest beside its streams directory");
    }
    let repositories = make_repositories(pool.clone());
    configure_verifiable_tv_publication(&pool, &repositories).await;
    let saved_revision = scalar::<i32>(
        &pool,
        r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let published_revision = scalar::<i32>(
        &pool,
        r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let enqueued = repositories
        .managed_library_operations
        .enqueue_publication(
            LIST_ID,
            1,
            saved_revision,
            published_revision.saturating_add(1),
        )
        .await
        .expect("enqueue exact publication revision");
    let claim = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, Utc::now())
        .await
        .expect("claim publication")
        .expect("publication is claimable");
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &claim,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .expect("enter staging"));
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("stage exact publication");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListArtworkMaterialization" (
               "AddonMediaListId", "AddonMediaListItemId", "ScopeItemId", "Role",
               "Source", "FileName", "Sha256", "PublishedRevision"
           ) VALUES (1, 10, 10, 3, 'https://example.invalid/item-thumb.jpg',
                     'S01E01.jpg',
                     'e32e48ce92baa22a98f435f806b65ca40c52a6ccd4acf689cb107bcb29925989', 1)"#,
    )
    .execute(&pool)
    .await
    .expect("retain an item-scoped materialization into the candidate revision");
    assert_eq!(
        repositories
            .addon_media_lists
            .commit_staged_publication_revision(
                LIST_ID,
                enqueued.job.target_published_revision,
                enqueued.operation.id,
                enqueued.job.generation,
                &claim.operation.owner_token,
                &claim.lease_owner,
            )
            .await
            .expect("commit publication candidate"),
        AddonMediaListPublicationContinuation::VerificationRequired
    );
    assert!(repositories
        .managed_library_operations
        .begin_publication_verification(
            &claim,
            AddonMediaListPublicationPhase::Staging,
            AddonMediaListPublicationOutcome::Published,
        )
        .await
        .expect("enter candidate verification"));
    VerifiablePublication {
        pool,
        directory,
        repositories,
        enqueued,
        claim,
    }
}

async fn verify_publication_candidate(
    fixture: &VerifiablePublication,
) -> ersatzrs_core::repositories::Result<()> {
    fixture
        .repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            fixture.enqueued.operation.id,
            fixture.enqueued.job.generation,
            &fixture.claim.operation.owner_token,
            &fixture.claim.lease_owner,
        )
        .await
}

#[tokio::test]
async fn journal_present_legacy_commit_bootstraps_and_seals_before_verification() {
    let fixture = prepare_verifiable_tv_publication().await;
    let journal_json = scalar::<String>(
        &fixture.pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let mut journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode current publication journal");
    let backup = PathBuf::from(journal["backup_folder"].as_str().expect("backup path"));
    for field in [
        "failed_folder",
        "previous_published_revision",
        "target_published_revision",
        "target_database",
    ] {
        journal[field] = serde_json::Value::Null;
    }
    fs::remove_file(fixture.directory.path().join(".ersatzrs-publication.json"))
        .expect("remove candidate marker from the legacy fixture");
    fs::remove_file(backup.join(".ersatzrs-publication.json"))
        .expect("remove backup marker from the legacy fixture");
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson" = ? WHERE "Id" = 1"#)
        .bind(serde_json::to_string(&journal).expect("encode legacy publication journal"))
        .execute(&fixture.pool)
        .await
        .expect("persist the pre-verification journal shape");

    verify_publication_candidate(&fixture)
        .await
        .expect("legacy committed state must bootstrap, seal, and verify");
    let marker: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.directory.path().join(".ersatzrs-publication.json"))
            .expect("read adopted candidate marker"),
    )
    .expect("decode adopted candidate marker");
    assert_eq!(marker["state"], "complete");
}

#[tokio::test]
async fn journal_present_legacy_restore_verifies_while_still_publishing() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let (enqueued, stale, claim) = claim_and_reclaim_publication(&repositories).await;
    assert_stale_publication_claim_is_fenced(&repositories, &enqueued, &stale, &claim).await;
    repositories
        .addon_media_lists
        .restore_staged_publication_revision(
            LIST_ID,
            "legacy restored fixture",
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("restore legacy publication snapshot");
    repositories
        .addon_media_lists
        .reconcile_restored_publication_revision(
            LIST_ID,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("reconcile legacy restored snapshot");
    assert!(repositories
        .managed_library_operations
        .begin_publication_verification(
            &claim,
            AddonMediaListPublicationPhase::Staging,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
        )
        .await
        .expect("enter legacy restored verification"));
    let journal_json = scalar::<String>(
        &pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let mut journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode restored publication journal");
    for field in [
        "failed_folder",
        "previous_published_revision",
        "target_published_revision",
    ] {
        journal[field] = serde_json::Value::Null;
    }
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson" = ? WHERE "Id" = 1"#)
        .bind(serde_json::to_string(&journal).expect("encode legacy restored journal"))
        .execute(&pool)
        .await
        .expect("persist legacy restored journal shape");

    repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("legacy restored state must bootstrap and verify");
    assert_eq!(
        scalar::<i32>(
            &pool,
            r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        3
    );
}

#[tokio::test]
async fn cleanup_revalidates_database_and_retains_backup_after_late_mutation() {
    let fixture = prepare_verifiable_tv_publication().await;
    verify_publication_candidate(&fixture)
        .await
        .expect("verify intact candidate");
    assert!(fixture
        .repositories
        .managed_library_operations
        .transition_publication(
            &fixture.claim,
            AddonMediaListPublicationPhase::Verifying,
            AddonMediaListPublicationPhase::CleaningUp,
        )
        .await
        .expect("enter publication cleanup"));
    let journal_json = scalar::<String>(
        &fixture.pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode publication journal");
    let backup = PathBuf::from(journal["backup_folder"].as_str().expect("backup path"));
    sqlx::query(r#"DELETE FROM "AddonMediaListItemEpisode" WHERE "AddonMediaListItemId" = 10"#)
        .execute(&fixture.pool)
        .await
        .expect("mutate projection after verification");
    let error = fixture
        .repositories
        .addon_media_lists
        .finalize_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            fixture.enqueued.operation.id,
            fixture.enqueued.job.generation,
            &fixture.claim.operation.owner_token,
            &fixture.claim.lease_owner,
        )
        .await
        .expect_err("late database mutation must block destructive cleanup");
    assert!(error.to_string().contains("convergence verification"));
    assert!(backup.is_dir(), "failed cleanup must retain the backup");
    assert_eq!(
        scalar::<Option<String>>(
            &fixture.pool,
            r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        Some(journal_json)
    );
}

#[tokio::test]
async fn ordinary_match_and_detail_writers_freeze_while_publication_is_active() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "MediaItemId" = NULL WHERE "Id" = 10"#)
        .execute(&fixture.pool)
        .await
        .expect("clear the matched identity");
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "LastMatch" = NULL, "DetailProjectionPendingAt" = '2000-01-01T00:00:00Z'
           WHERE "Id" = 1"#,
    )
    .execute(&fixture.pool)
    .await
    .expect("prepare frozen due and detail writers");
    fixture
        .repositories
        .addon_media_lists
        .match_items(LIST_ID)
        .await
        .expect_err("manual matching defers during publication");
    let synchronizer = AddonMediaListSynchronizer::new(fixture.pool.clone());
    assert_eq!(
        synchronizer.match_all_lists().await.expect("global match"),
        0
    );
    assert_eq!(synchronizer.match_due_lists().await.expect("due match"), 0);
    assert_eq!(
        project_one_pending_detail_batch(&fixture.pool)
            .await
            .expect("detail projection lookup"),
        None
    );
    assert_eq!(
        scalar::<Option<i64>>(
            &fixture.pool,
            r#"SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id" = 10"#,
        )
        .await,
        None
    );
}

#[tokio::test]
async fn deletion_is_rejected_before_mutating_a_verifying_publication_projection() {
    let fixture = prepare_verifiable_tv_publication().await;
    let projected_before = scalar::<i64>(
        &fixture.pool,
        r#"SELECT COUNT(*) FROM "AddonMediaListItemEpisode""#,
    )
    .await;

    let error = fixture
        .repositories
        .addon_media_lists
        .delete(LIST_ID)
        .await
        .expect_err("verification owns the list and its projection");
    assert!(error.to_string().contains("being published"));
    assert_eq!(
        scalar::<i64>(
            &fixture.pool,
            r#"SELECT COUNT(*) FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .await,
        1
    );
    assert_eq!(
        scalar::<i64>(
            &fixture.pool,
            r#"SELECT COUNT(*) FROM "AddonMediaListItemEpisode""#
        )
        .await,
        projected_before,
        "the deletion guard must run before projection reconciliation"
    );
}

#[tokio::test]
async fn candidate_verification_rejects_a_wrong_existing_provider_match() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "MediaItemId" = 111 WHERE "Id" = 10"#)
        .execute(&fixture.pool)
        .await
        .expect("inject a wrong but existing provider match");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("wrong provider identity must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_a_wrong_existing_non_remote_match() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem" SET "Kind" = 0, "MediaItemId" = 111
           WHERE "Id" = 10"#,
    )
    .execute(&fixture.pool)
    .await
    .expect("inject a wrong but existing non-remote match");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("wrong non-remote target identity must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_a_kind_only_identity_change() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "Kind" = 0 WHERE "Id" = 10"#)
        .execute(&fixture.pool)
        .await
        .expect("change only the durable provider-item kind");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("kind-only identity drift must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

async fn seed_native_movie_match(pool: &SqlitePool) {
    sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (500, 1, 0)"#)
        .execute(pool)
        .await
        .expect("seed native movie media item");
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (500)"#)
        .execute(pool)
        .await
        .expect("seed native movie");
    sqlx::query(
        r#"INSERT INTO "MovieMetadata" (
               "DateAdded", "DateUpdated", "MetadataKind", "MovieId", "Title"
           ) VALUES ('2026-08-14T00:00:00Z', '2026-08-14T00:00:00Z', 1, 500, 'Pilot')"#,
    )
    .execute(pool)
    .await
    .expect("seed native movie metadata");
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "Included" = CASE WHEN "Id" = 10 THEN 1 ELSE 0 END,
               "Kind" = CASE WHEN "Id" = 10 THEN 0 ELSE "Kind" END,
               "MediaItemId" = CASE WHEN "Id" = 10 THEN 500 ELSE "MediaItemId" END
           WHERE "AddonMediaListId" = 1"#,
    )
    .execute(pool)
    .await
    .expect("seed one native matched provider item");
}

#[tokio::test]
async fn candidate_verification_accepts_a_native_movie_without_owned_projection() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    seed_native_movie_match(&pool).await;
    repositories
        .addon_media_lists
        .request_presentation(LIST_ID, AddonMediaListPresentationKind::Movies, None)
        .await
        .expect("request movie presentation");
    let saved_revision = scalar::<i32>(
        &pool,
        r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let enqueued = repositories
        .managed_library_operations
        .enqueue_publication(LIST_ID, 1, saved_revision, 2)
        .await
        .expect("enqueue native movie publication");
    let claim = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, Utc::now())
        .await
        .expect("claim native movie publication")
        .expect("native movie publication is claimable");
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &claim,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .expect("enter native movie staging"));
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("stage native movie publication");
    repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("commit native movie publication");
    assert!(repositories
        .managed_library_operations
        .begin_publication_verification(
            &claim,
            AddonMediaListPublicationPhase::Staging,
            AddonMediaListPublicationOutcome::Published,
        )
        .await
        .expect("enter native movie verification"));
    repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("native movie direct match must verify");
    assert_eq!(
        scalar::<i64>(&pool, r#"SELECT COUNT(*) FROM "AddonMediaListItemMovie""#).await,
        0,
        "native media must never be adopted as a list-owned projection"
    );
    drop(directory);
}

#[tokio::test]
async fn candidate_verification_allows_duplicate_exact_item_and_playlist_matches() {
    let pool = common::test_db().await;
    let _directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    sqlx::query(r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem") VALUES ('System', 1)"#)
        .execute(&pool)
        .await
        .expect("seed system playlist group");
    sqlx::query(r#"UPDATE "AddonMediaList" SET "GeneratePlaylist" = 1 WHERE "Id" = 1"#)
        .execute(&pool)
        .await
        .expect("enable a generated playlist");
    let (enqueued, _stale, claim) = claim_and_reclaim_publication(&repositories).await;
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            enqueued.job.saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("stage duplicate matching fixture");
    sqlx::query(
        r#"CREATE TRIGGER "DuplicateExactCandidateMatches"
           AFTER UPDATE OF "LastMatch" ON "AddonMediaList"
           BEGIN
             UPDATE "AddonMediaListItem"
             SET "Kind" = 0, "MediaItemId" = 110 WHERE "Id" IN (10, 11);
           END"#,
    )
    .execute(&pool)
    .await
    .expect("install duplicate matching fixture trigger");
    repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("commit duplicate exact matches");
    sqlx::query(r#"DROP TRIGGER "DuplicateExactCandidateMatches""#)
        .execute(&pool)
        .await
        .expect("remove duplicate matching fixture trigger");
    let journal_json = scalar::<String>(
        &pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let mut journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode duplicate target fixture");
    journal["target_database"]["playlist"]["items"][1]["media_item_id"] =
        serde_json::json!(110_i64);
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson" = ? WHERE "Id" = 1"#)
        .bind(serde_json::to_string(&journal).expect("encode duplicate target fixture"))
        .execute(&pool)
        .await
        .expect("record duplicate exact playlist selection");
    sqlx::query(
        r#"UPDATE "PlaylistItem" SET "MediaItemId" = 110
           WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 1)
             AND "Index" = 11"#,
    )
    .execute(&pool)
    .await
    .expect("apply duplicate canonical playlist selection");
    assert!(repositories
        .managed_library_operations
        .begin_publication_verification(
            &claim,
            AddonMediaListPublicationPhase::Staging,
            AddonMediaListPublicationOutcome::Published,
        )
        .await
        .expect("enter duplicate candidate verification"));
    repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::Published,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("duplicate exact item and playlist matches must preserve cardinality and verify");
}

#[tokio::test]
async fn candidate_verification_accepts_complete_item_scoped_materializations() {
    let fixture = prepare_verifiable_tv_publication().await;
    verify_publication_candidate(&fixture)
        .await
        .expect("complete parent and item artwork materializations must verify");
}

#[tokio::test]
async fn candidate_verification_accepts_nested_stream_artwork_paths() {
    let fixture = prepare_verifiable_tv_publication_with_nested_streams(true).await;
    let files: Vec<String> = sqlx::query_scalar(
        r#"SELECT "FileName" FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = 1 ORDER BY "ScopeItemId""#,
    )
    .fetch_all(&fixture.pool)
    .await
    .expect("load parent and item materialization file names");
    assert_eq!(files.len(), 2);
    for file in files {
        assert!(fixture
            .directory
            .path()
            .join("streams")
            .join(&file)
            .is_file());
        assert!(!fixture.directory.path().join(file).exists());
    }
    verify_publication_candidate(&fixture)
        .await
        .expect("scanner-style nested parent and item artwork must converge");
}

#[tokio::test]
async fn candidate_verification_rejects_artwork_owner_outside_managed_root() {
    let fixture = prepare_verifiable_tv_publication_with_nested_streams(true).await;
    let outside = tempfile::tempdir().expect("outside artwork directory");
    // Matching bytes exist both inside and outside the sealed tree. Only the
    // database owner path changes, so this tests containment rather than a
    // missing-file or sealed-tree-digest failure.
    let files: Vec<String> = sqlx::query_scalar(
        r#"SELECT "FileName" FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = 1"#,
    )
    .fetch_all(&fixture.pool)
    .await
    .expect("load materialization file names");
    for file in files {
        fs::copy(
            fixture.directory.path().join("streams").join(&file),
            outside.path().join(file),
        )
        .expect("copy matching artwork outside the managed root");
    }
    sqlx::query(
        r#"UPDATE "MediaFile" SET "Path" = ?
           WHERE "MediaVersionId" IN (
               SELECT "Id" FROM "MediaVersion" WHERE "RemoteStreamId" = 110
           )"#,
    )
    .bind(
        outside
            .path()
            .join("S01E01.strm")
            .to_string_lossy()
            .as_ref(),
    )
    .execute(&fixture.pool)
    .await
    .expect("move only the materialization owner path outside the managed root");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("matching artwork outside the publication must not verify");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_modified_nested_artwork_bytes() {
    let fixture = prepare_verifiable_tv_publication_with_nested_streams(true).await;
    fs::write(
        fixture.directory.path().join("streams").join("S01E01.jpg"),
        b"modified artwork bytes",
    )
    .expect("modify only nested item artwork bytes");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("modified nested artwork must not verify");
    assert!(error.to_string().contains("publication tree"));
}

#[tokio::test]
async fn candidate_verification_accepts_a_file_valued_manifest_path() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(
        r#"UPDATE "AddonMediaListLibrary" SET "ManifestPath" = ? WHERE "AddonMediaListId" = 1"#,
    )
    .bind(
        fixture
            .directory
            .path()
            .join("list.playlist.yml")
            .to_string_lossy()
            .as_ref(),
    )
    .execute(&fixture.pool)
    .await
    .expect("store the production manifest-file path shape");
    verify_publication_candidate(&fixture)
        .await
        .expect("materialized files resolve relative to the manifest parent");
}

#[tokio::test]
async fn candidate_verification_rejects_materialization_hash_not_matching_sealed_bytes() {
    let fixture = prepare_verifiable_tv_publication_with_nested_streams(true).await;
    let fake_sha = "0000000000000000000000000000000000000000000000000000000000000000";
    sqlx::query(
        r#"UPDATE "AddonMediaListArtworkMaterialization" SET "Sha256" = ?
           WHERE "AddonMediaListId" = 1 AND "ScopeItemId" = 0"#,
    )
    .bind(fake_sha)
    .execute(&fixture.pool)
    .await
    .expect("replace the database artwork digest only");
    let journal_json = scalar::<String>(
        &fixture.pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let mut journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode materialization target");
    let target = journal["target_database"]["materializations"]
        .as_array_mut()
        .expect("target materializations")
        .iter_mut()
        .find(|row| row["scope_item_id"] == 0)
        .expect("parent materialization target");
    target["sha256"] = serde_json::json!(fake_sha);
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson" = ? WHERE "Id" = 1"#)
        .bind(serde_json::to_string(&journal).expect("encode materialization target"))
        .execute(&fixture.pool)
        .await
        .expect("keep logical target and current rows identical");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("database artwork digest must match the managed bytes");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_a_missing_projection_row() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(r#"DELETE FROM "AddonMediaListItemEpisode" WHERE "AddonMediaListItemId" = 10"#)
        .execute(&fixture.pool)
        .await
        .expect("remove one projected episode");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("incomplete projection must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_missing_projection_metadata() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(
        r#"DELETE FROM "EpisodeMetadata"
           WHERE "EpisodeId" = (SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
                                  WHERE "AddonMediaListItemId" = 10)"#,
    )
    .execute(&fixture.pool)
    .await
    .expect("remove one projected episode metadata row");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("projected media without metadata must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_missing_parent_projection_metadata() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(
        r#"DELETE FROM "ShowMetadata"
           WHERE "ShowId" = (SELECT "ShowId" FROM "AddonMediaListTelevisionShow"
                              WHERE "AddonMediaListId" = 1)"#,
    )
    .execute(&fixture.pool)
    .await
    .expect("remove the projected parent metadata row");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("projected parent without metadata must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_same_cardinality_wrong_projection_without_playlist() {
    let fixture = prepare_verifiable_tv_publication().await;
    let journal_json = scalar::<String>(
        &fixture.pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let mut journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode target database snapshot");
    journal["target_database"]["playlist"] = serde_json::Value::Null;
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "GeneratePlaylist" = 0, "PlaylistId" = NULL, "PublicationJournalJson" = ?
           WHERE "Id" = 1"#,
    )
    .bind(serde_json::to_string(&journal).expect("encode playlist-free target fixture"))
    .execute(&fixture.pool)
    .await
    .expect("remove playlist from the independent projection fixture");
    let first_episode = scalar::<i64>(
        &fixture.pool,
        r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
           WHERE "AddonMediaListItemId" = 10"#,
    )
    .await;
    let second_episode = scalar::<i64>(
        &fixture.pool,
        r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
           WHERE "AddonMediaListItemId" = 11"#,
    )
    .await;
    sqlx::query(
        r#"DELETE FROM "AddonMediaListItemEpisode" WHERE "AddonMediaListItemId" IN (10, 11)"#,
    )
    .execute(&fixture.pool)
    .await
    .expect("detach two projection rows");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItemEpisode" ("AddonMediaListItemId", "EpisodeId")
           VALUES (10, ?), (11, ?)"#,
    )
    .bind(second_episode)
    .bind(first_episode)
    .execute(&fixture.pool)
    .await
    .expect("swap two valid projected episode identities");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("wrong projection source linkage must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_missing_playlist_membership() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(
        r#"DELETE FROM "PlaylistItem"
           WHERE "Id" = (SELECT MIN("Id") FROM "PlaylistItem"
                          WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList"
                                                WHERE "Id" = 1))"#,
    )
    .execute(&fixture.pool)
    .await
    .expect("remove one generated playlist member");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("incomplete playlist must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_wrong_same_cardinality_playlist_membership() {
    let fixture = prepare_verifiable_tv_publication().await;
    sqlx::query(
        r#"UPDATE "PlaylistItem" SET "MediaItemId" = 110
           WHERE "Id" = (SELECT MIN("Id") FROM "PlaylistItem"
                          WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList"
                                                WHERE "Id" = 1))"#,
    )
    .execute(&fixture.pool)
    .await
    .expect("replace one member with a wrong existing media row");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("wrong playlist membership must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_missing_expected_artwork_materialization() {
    let fixture = prepare_verifiable_tv_publication().await;
    assert_eq!(
        scalar::<i64>(
            &fixture.pool,
            r#"SELECT COUNT(*) FROM "AddonMediaListArtworkMaterialization"
               WHERE "AddonMediaListId" = 1"#,
        )
        .await,
        2
    );
    sqlx::query(
        r#"DELETE FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = 1"#,
    )
    .execute(&fixture.pool)
    .await
    .expect("remove expected parent artwork materialization");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("missing artwork materialization must fail verification");
    assert!(error.to_string().contains("convergence verification"));
}

#[tokio::test]
async fn candidate_verification_rejects_a_missing_materialized_artwork_file() {
    let fixture = prepare_verifiable_tv_publication_with_nested_streams(true).await;
    let file_name = scalar::<String>(
        &fixture.pool,
        r#"SELECT "FileName" FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = 1 AND "ScopeItemId" = 0"#,
    )
    .await;
    fs::remove_file(fixture.directory.path().join("streams").join(file_name))
        .expect("remove materialized parent artwork file");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("missing materialized artwork bytes must fail verification");
    assert!(error.to_string().contains("publication tree"));
}

#[tokio::test]
async fn candidate_verification_rejects_a_tampered_sealed_nfo_and_keeps_backup() {
    let fixture = prepare_verifiable_tv_publication().await;
    let journal_json = scalar::<String>(
        &fixture.pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode sealed publication journal");
    let backup = PathBuf::from(
        journal["backup_folder"]
            .as_str()
            .expect("publication backup path"),
    );
    fs::write(fixture.directory.path().join("tvshow.nfo"), b"tampered NFO")
        .expect("tamper with the sealed candidate tree");
    let error = verify_publication_candidate(&fixture)
        .await
        .expect_err("tampered sealed candidate must fail verification");
    assert!(error.to_string().contains("publication tree"));
    assert!(
        backup.is_dir(),
        "failed verification must retain the backup"
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn restored_verification_accepts_logical_content_with_regenerated_database_ids() {
    let pool = common::test_db().await;
    let directory = seed_managed_list(&pool).await;
    let repositories = make_repositories(pool.clone());
    let _ = prepare_published_revision(&pool, &repositories, &directory).await;
    let previous_episode_ids = sqlx::query_scalar::<_, i64>(
        r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode" ORDER BY "AddonMediaListItemId""#,
    )
    .fetch_all(&pool)
    .await
    .expect("load previous episode identities");
    let previous_playlist_item_ids = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "PlaylistItem"
           WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 1)
           ORDER BY "Index", "Id""#,
    )
    .fetch_all(&pool)
    .await
    .expect("load previous playlist item identities");

    repositories
        .addon_media_lists
        .request_presentation(LIST_ID, AddonMediaListPresentationKind::Movies, None)
        .await
        .expect("request failed candidate presentation");
    repositories
        .addon_media_lists
        .update_settings(
            LIST_ID,
            AddonMediaListRefreshCadence::Daily,
            ersatzrs_domain::AddonMediaListRefreshPublicationPolicy::Review,
            ersatzrs_domain::AddonMediaListDeliveryMode::Stream,
            false,
            AddonMediaListContentKind::Movie,
            "",
        )
        .await
        .expect("disable the candidate generated playlist");
    let saved_revision = scalar::<i32>(
        &pool,
        r#"SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let published_revision = scalar::<i32>(
        &pool,
        r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let enqueued = repositories
        .managed_library_operations
        .enqueue_publication(
            LIST_ID,
            1,
            saved_revision,
            published_revision.saturating_add(1),
        )
        .await
        .expect("enqueue failed candidate revision");
    let claim = repositories
        .managed_library_operations
        .claim_operation(enqueued.operation.id, Utc::now())
        .await
        .expect("claim failed candidate")
        .expect("candidate is claimable");
    assert!(repositories
        .managed_library_operations
        .transition_publication(
            &claim,
            AddonMediaListPublicationPhase::AwaitingLease,
            AddonMediaListPublicationPhase::Staging,
        )
        .await
        .expect("enter candidate staging"));
    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("stage failed candidate");
    repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("commit candidate before injected downstream failure");
    repositories
        .addon_media_lists
        .restore_staged_publication_revision(
            LIST_ID,
            "injected post-commit failure",
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("restore prior snapshot");
    repositories
        .addon_media_lists
        .reconcile_restored_publication_revision(
            LIST_ID,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("rebuild prior projection and playlist");

    let restored_episode_ids = sqlx::query_scalar::<_, i64>(
        r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode" ORDER BY "AddonMediaListItemId""#,
    )
    .fetch_all(&pool)
    .await
    .expect("load restored episode identities");
    let restored_playlist_item_ids = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "PlaylistItem"
           WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 1)
           ORDER BY "Index", "Id""#,
    )
    .fetch_all(&pool)
    .await
    .expect("load restored playlist item identities");
    assert_ne!(restored_episode_ids, previous_episode_ids);
    assert_ne!(restored_playlist_item_ids, previous_playlist_item_ids);

    let journal_json = scalar::<String>(
        &pool,
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .await;
    let mut journal: serde_json::Value =
        serde_json::from_str(&journal_json).expect("decode publication journal");
    journal["previous_items"][0]["media_item_id"] = serde_json::json!(999_999_i64);
    sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson" = ? WHERE "Id" = 1"#)
        .bind(serde_json::to_string(&journal).expect("encode regenerated identity fixture"))
        .execute(&pool)
        .await
        .expect("record the pre-regeneration provider identity");

    assert!(repositories
        .managed_library_operations
        .begin_publication_verification(
            &claim,
            AddonMediaListPublicationPhase::Staging,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
        )
        .await
        .expect("enter restored verification"));
    repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect("logical restored state must ignore regenerated row identities");

    let first_episode = scalar::<i64>(
        &pool,
        r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
           WHERE "AddonMediaListItemId" = 10"#,
    )
    .await;
    let second_episode = scalar::<i64>(
        &pool,
        r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
           WHERE "AddonMediaListItemId" = 11"#,
    )
    .await;
    sqlx::query(
        r#"DELETE FROM "AddonMediaListItemEpisode" WHERE "AddonMediaListItemId" IN (10, 11)"#,
    )
    .execute(&pool)
    .await
    .expect("detach restored projection rows");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItemEpisode" ("AddonMediaListItemId", "EpisodeId")
           VALUES (10, ?), (11, ?)"#,
    )
    .bind(second_episode)
    .bind(first_episode)
    .execute(&pool)
    .await
    .expect("swap restored projection identities");
    let projection_error = repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect_err("wrong restored projection content must fail verification");
    assert!(projection_error
        .to_string()
        .contains("matching or projection structure"));

    sqlx::query(
        r#"DELETE FROM "AddonMediaListItemEpisode" WHERE "AddonMediaListItemId" IN (10, 11)"#,
    )
    .execute(&pool)
    .await
    .expect("detach swapped projection rows");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItemEpisode" ("AddonMediaListItemId", "EpisodeId")
           VALUES (10, ?), (11, ?)"#,
    )
    .bind(first_episode)
    .bind(second_episode)
    .execute(&pool)
    .await
    .expect("restore correct projection identities");
    sqlx::query(
        r#"UPDATE "PlaylistItem" SET "MediaItemId" = 110
           WHERE "Id" = (SELECT MIN("Id") FROM "PlaylistItem"
                          WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList"
                                                WHERE "Id" = 1))"#,
    )
    .execute(&pool)
    .await
    .expect("inject wrong restored playlist media");
    let playlist_error = repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect_err("wrong restored playlist content must fail verification");
    assert!(playlist_error
        .to_string()
        .contains("convergence verification"));

    sqlx::query(
        r#"UPDATE "PlaylistItem"
           SET "MediaItemId" = CASE "Index" WHEN 10 THEN ? WHEN 11 THEN ? ELSE "MediaItemId" END
           WHERE "PlaylistId" = (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = 1)"#,
    )
    .bind(second_episode)
    .bind(first_episode)
    .execute(&pool)
    .await
    .expect("swap two valid restored playlist members");
    let permutation_error = repositories
        .addon_media_lists
        .verify_staged_publication_revision(
            LIST_ID,
            AddonMediaListPublicationOutcome::RestoredAfterFailure,
            enqueued.operation.id,
            enqueued.job.generation,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
        .expect_err("restored playlist permutation must fail verification");
    assert!(permutation_error
        .to_string()
        .contains("matching or projection structure"));
}

async fn assert_stale_publication_claim_is_fenced(
    repositories: &Repositories,
    enqueued: &AddonMediaListPublicationEnqueue,
    stale: &ManagedLibraryOperationClaim,
    current: &ManagedLibraryOperationClaim,
) {
    let stale_stage = repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            enqueued.job.saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &stale.operation.owner_token,
            &stale.lease_owner,
        )
        .await
        .expect_err("an expired claimant must not stage publication");
    assert!(stale_stage
        .to_string()
        .contains("lease is no longer current"));

    repositories
        .addon_media_lists
        .stage_publication_revision(
            LIST_ID,
            enqueued.job.saved_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &current.operation.owner_token,
            &current.lease_owner,
        )
        .await
        .expect("the current claimant stages publication");
    let stale_restore = repositories
        .addon_media_lists
        .restore_staged_publication_revision(
            LIST_ID,
            "stale recovery must not touch managed files",
            enqueued.operation.id,
            enqueued.job.generation,
            &stale.operation.owner_token,
            &stale.lease_owner,
        )
        .await
        .expect_err("an expired claimant must not restore publication files");
    assert!(stale_restore
        .to_string()
        .contains("lease is no longer current"));
    let stale_commit = repositories
        .addon_media_lists
        .commit_staged_publication_revision(
            LIST_ID,
            enqueued.job.target_published_revision,
            enqueued.operation.id,
            enqueued.job.generation,
            &stale.operation.owner_token,
            &stale.lease_owner,
        )
        .await
        .expect_err("an expired claimant must not commit publication");
    assert!(stale_commit
        .to_string()
        .contains("lease is no longer current"));
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
    let poster_url = &poster
        .metadata
        .artwork
        .iter()
        .find(|candidate| candidate.role == AddonMediaListArtworkRole::Poster)
        .expect("selected poster")
        .url;
    let poster_upload_id =
        ersatzrs_infra::addon_artwork::upload_id(poster_url).expect("opaque poster upload id");
    let poster_upload = repositories
        .addon_media_lists
        .get_artwork_upload(poster_upload_id)
        .await
        .expect("load poster upload")
        .expect("stored poster upload");
    assert_eq!(poster_upload.content_type, "image/png");
    assert_eq!(poster_upload.bytes, b"\x89PNG\r\n\x1a\nposter one");
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
