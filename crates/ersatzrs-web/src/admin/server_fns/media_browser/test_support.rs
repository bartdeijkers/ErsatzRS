use super::*;
use super::{
    collections::manual::*, collections::multi::*, collections::rerun::*, collections::smart::*,
    kinds::movies::*, playlists::*,
};

#[cfg(all(test, feature = "ssr"))]
mod remote_stream_tests {
    use super::{list_remote_streams, remote_stream_page_map};
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_ffmpeg::FfmpegRuntimeReport;
    use ersatzrs_infra::db::{self, DatabaseBackend, DatabaseSchemaStatus};
    use leptos::prelude::{provide_context, Owner};
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    #[tokio::test(flavor = "current_thread")]
    async fn remote_stream_list_uses_search_index_query_and_projects_cards() {
        let (_db_file, pool) = test_pool().await;
        seed_remote_stream_fixture(&pool).await;

        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();

        let (cards, total) = list_remote_streams("library_id:7".to_owned(), 0, 100)
            .await
            .unwrap();

        assert_eq!(total, 1);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].id, 701);
        assert_eq!(cards[0].title, "Nurburgring broadcast");
        assert_eq!(cards[0].sort_title, "nurburgring broadcast");
        assert_eq!(cards[0].subtitle.as_deref(), Some("Racing"));

        let page_map = remote_stream_page_map("library_id:7".to_owned(), 100)
            .await
            .unwrap();
        assert_eq!(page_map.pages[14], 1); // n

        owner.cleanup();
    }

    async fn test_pool() -> (NamedTempFile, ersatzrs_infra::db::DbPool) {
        let file = NamedTempFile::new().unwrap();
        let database_url = format!("sqlite:{}", file.path().display());
        let pool = db::init(&database_url).await.unwrap();
        (file, pool)
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_remote_stream_fixture(pool: &ersatzrs_infra::db::DbPool) {
        let now = "2026-06-14T00:00:00Z";
        for (source_id, library_id, path_id, item_id, title, original_title, sort_title) in [
            (
                70,
                7,
                700,
                701,
                "Nurburgring broadcast",
                "Racing",
                "nurburgring broadcast",
            ),
            (80, 8, 800, 801, "Visdeurbel", "Vis", "visdeurbel"),
        ] {
            sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (?)"#)
                .bind(source_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (?)"#)
                .bind(source_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query(
                r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
                   VALUES (?, ?, 7, ?)"#,
            )
            .bind(library_id)
            .bind(format!("Remote Streams {library_id}"))
            .bind(source_id)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (?)"#)
                .bind(library_id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query(
                r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
                   VALUES (?, ?, ?)"#,
            )
            .bind(path_id)
            .bind(format!("/remote/{library_id}"))
            .bind(library_id)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
                   VALUES (?, ?, 0)"#,
            )
            .bind(item_id)
            .bind(path_id)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "RemoteStream" ("Id", "Url", "Script", "IsLive")
                   VALUES (?, ?, NULL, 1)"#,
            )
            .bind(item_id)
            .bind(format!("https://example.test/{item_id}.m3u8"))
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "RemoteStreamMetadata" (
                       "RemoteStreamId", "MetadataKind", "Title", "OriginalTitle",
                       "SortTitle", "DateAdded", "DateUpdated"
                   )
                   VALUES (?, 0, ?, ?, ?, ?, ?)"#,
            )
            .bind(item_id)
            .bind(title)
            .bind(original_title)
            .bind(sort_title)
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

#[cfg(test)]
mod playlist_tests {
    use super::{
        validate_playlist_items, PlaylistItemForm, PLAYLIST_COLLECTION_TYPE_COLLECTION,
        PLAYLIST_COLLECTION_TYPE_MOVIE, PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM,
    };

    #[test]
    fn playlist_item_validation_requires_matching_target_id() {
        let mut item = PlaylistItemForm {
            collection_type: PLAYLIST_COLLECTION_TYPE_COLLECTION,
            ..PlaylistItemForm::default()
        };
        assert!(validate_playlist_items(&[item.clone()]).is_err());

        item.collection_id = Some(10);
        assert!(validate_playlist_items(&[item.clone()]).is_ok());

        item.collection_type = PLAYLIST_COLLECTION_TYPE_MOVIE;
        item.collection_id = None;
        assert!(validate_playlist_items(&[item.clone()]).is_err());

        item.media_item_id = Some(20);
        assert!(validate_playlist_items(&[item]).is_ok());
    }

    #[test]
    fn playlist_item_validation_accepts_remote_stream_items_created_elsewhere() {
        let item = PlaylistItemForm {
            collection_type: PLAYLIST_COLLECTION_TYPE_REMOTE_STREAM,
            media_item_id: Some(70),
            ..PlaylistItemForm::default()
        };

        assert!(validate_playlist_items(&[item]).is_ok());
    }
}

#[cfg(test)]
mod season_detail_tests {
    use super::{
        get_media_item_info_json, get_season_details, list_other_videos, list_trash_items,
    };
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_ffmpeg::FfmpegRuntimeReport;
    use ersatzrs_infra::db::{self, DatabaseBackend, DatabaseSchemaStatus};
    use leptos::prelude::{provide_context, Owner};
    use serde_json::Value;
    use sqlx::Executor;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    #[tokio::test(flavor = "current_thread")]
    async fn season_details_and_episode_media_info_use_television_repository_projection() {
        let (_db_file, pool) = test_pool().await;
        seed_season_detail_fixture(&pool).await;

        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();

        let season = get_season_details(20).await.unwrap().unwrap();
        assert_eq!(season.id, 20);
        assert_eq!(season.show_id, 10);
        assert_eq!(season.show_title, "Example Show");
        assert_eq!(season.name, "Season 1");
        assert_eq!(
            season.poster_url.as_deref(),
            Some("/artwork/posters/season-poster.jpg")
        );
        assert_eq!(
            season.fanart_url.as_deref(),
            Some("/artwork/fanart/show-fanart.jpg")
        );
        assert_eq!(season.episode_count, 2);
        assert_eq!(season.episodes.len(), 2);
        assert_eq!(season.episodes[0].episode_id, 30);
        assert_eq!(season.episodes[0].episode_number, 1);
        assert_eq!(season.episodes[0].title, "Pilot");
        assert_eq!(
            season.episodes[0].thumbnail_url.as_deref(),
            Some("/artwork/thumbnails/pilot-thumb.jpg")
        );
        assert_eq!(season.episodes[0].aired, "1-5-2026");
        assert_eq!(season.episodes[0].media_item_state, 1);
        assert_eq!(
            season.episodes[0].file_path.as_deref(),
            Some("/media/show/s01e01.mkv")
        );
        assert_eq!(
            season.episodes[0].directors,
            vec!["Director A", "Director B"]
        );
        assert_eq!(season.episodes[0].writers, vec!["Writer A"]);
        assert_eq!(season.episodes[1].aired, "1-1-0001");

        let json = get_media_item_info_json(30).await.unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["Kind"], "Episode");
        assert_eq!(value["Title"], "Example Show - s01e01 - Pilot");
        assert_eq!(value["LibraryKind"], "LocalLibrary");
        assert_eq!(value["State"], "FileNotFound");
        assert_eq!(value["Duration"], "00:22:00");
        assert_eq!(value["Width"], 1920);
        assert_eq!(value["Height"], 1080);
        assert_eq!(value["Streams"][0]["Kind"], "Video");
        assert_eq!(value["Streams"][1]["Kind"], "Subtitle");
        assert_eq!(value["Streams"][1]["IsExtracted"], true);
        assert_eq!(value["Chapters"][0]["Title"], "Opening");

        owner.cleanup();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trash_items_include_missing_episodes() {
        let (_db_file, pool) = test_pool().await;
        seed_season_detail_fixture(&pool).await;

        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();

        let trash = list_trash_items().await.unwrap();

        assert!(trash.shows.is_empty());
        assert_eq!(trash.episodes.len(), 1);
        assert_eq!(trash.episodes[0].id, 30);
        assert_eq!(trash.episodes[0].title, "Pilot");
        assert_eq!(
            trash.episodes[0].subtitle.as_deref(),
            Some("Example Show - S1 E1")
        );

        owner.cleanup();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn linked_extras_load_on_detail_and_server_filter_supports_primary_only() {
        let (_db_file, pool) = test_pool().await;
        seed_season_detail_fixture(&pool).await;

        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();

        let season = get_season_details(20).await.unwrap().unwrap();
        assert_eq!(season.extras.len(), 1);
        assert_eq!(season.extras[0].id, 40);
        assert_eq!(season.extras[0].title, "Featurette");
        assert_eq!(
            season.extras[0].variant.content_kind,
            ersatzrs_domain::ContentKind::Extra
        );
        assert_eq!(
            season.extras[0].variant.sub_kind.as_deref(),
            Some("featurette")
        );

        let (hidden_cards, hidden_total) = list_other_videos(String::new(), 0, 50, false)
            .await
            .unwrap();
        assert_eq!(hidden_total, 0);
        assert!(hidden_cards.is_empty());

        let (visible_cards, visible_total) =
            list_other_videos(String::new(), 0, 50, true).await.unwrap();
        assert_eq!(visible_total, 1);
        assert_eq!(visible_cards[0].id, 40);
        assert_eq!(
            visible_cards[0].variant.content_kind,
            ersatzrs_domain::ContentKind::Extra
        );

        owner.cleanup();
    }

    async fn test_pool() -> (NamedTempFile, ersatzrs_infra::db::DbPool) {
        let file = NamedTempFile::new().unwrap();
        let database_url = format!("sqlite:{}", file.path().display());
        let pool = db::init(&database_url).await.unwrap();
        (file, pool)
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_season_detail_fixture(pool: &ersatzrs_infra::db::DbPool) {
        let now = "2026-05-22T00:00:00Z";
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (10, 'TV', 1, 10)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (10, '/media', 10)"#,
        )
        .await
        .unwrap();

        for (id, state) in [(10, 0), (20, 0), (30, 1), (31, 0)] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
                   VALUES (?, 10, ?)"#,
            )
            .bind(id)
            .bind(state)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            r#"INSERT INTO "MediaItem" (
                   "Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId"
               ) VALUES (40, 10, 0, 1, 'featurette', 20)"#,
        )
        .execute(pool)
        .await
        .unwrap();

        pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (10)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (20, 10, 1)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (30, 20), (31, 20)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "OtherVideo" ("Id") VALUES (40)"#)
            .await
            .unwrap();

        sqlx::query(
            r#"INSERT INTO "ShowMetadata" (
                   "Id", "ShowId", "MetadataKind", "Title", "SortTitle", "Year",
                   "DateAdded", "DateUpdated"
               ) VALUES (100, 10, 0, 'Example Show', 'example show', 2026, ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "SeasonMetadata" (
                   "Id", "SeasonId", "MetadataKind", "Title", "SortTitle",
                   "DateAdded", "DateUpdated"
               ) VALUES (200, 20, 0, 'Season 1', 'season 1', ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "EpisodeMetadata" (
                   "Id", "EpisodeId", "MetadataKind", "Title", "SortTitle",
                   "ReleaseDate", "DateAdded", "DateUpdated", "EpisodeNumber", "Plot"
               ) VALUES
                   (300, 30, 0, 'Pilot', 'pilot', '2026-05-01T00:00:00Z', ?, ?, 1, 'The beginning.'),
                   (301, 31, 0, 'Finale', 'finale', NULL, ?, ?, 2, 'The ending.')"#,
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "OtherVideoMetadata" (
                   "Id", "OtherVideoId", "MetadataKind", "Title", "OriginalTitle",
                   "SortTitle", "Year", "DateAdded", "DateUpdated"
               ) VALUES (
                   400, 40, 0, 'Featurette', 'Behind the scenes', 'featurette',
                   2026, ?, ?
               )"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO "Artwork" (
                   "Id", "ArtworkKind", "Path", "SeasonMetadataId", "ShowMetadataId",
                   "EpisodeMetadataId", "OtherVideoMetadataId", "DateAdded", "DateUpdated"
               ) VALUES
                   (1, 0, 'season-poster.jpg', 200, NULL, NULL, NULL, ?, ?),
                   (2, 3, 'show-fanart.jpg', NULL, 100, NULL, NULL, ?, ?),
                   (3, 1, 'pilot-thumb.jpg', NULL, NULL, 300, NULL, ?, ?),
                   (4, 1, 'featurette-thumb.jpg', NULL, NULL, NULL, 400, ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();

        pool.execute(
            r#"INSERT INTO "Director" ("EpisodeMetadataId", "Name")
               VALUES (300, 'Director A'), (300, 'Director B')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Writer" ("EpisodeMetadataId", "Name")
               VALUES (300, 'Writer A')"#,
        )
        .await
        .unwrap();

        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "EpisodeId",
                   "Height", "Width", "VideoScanKind", "SampleAspectRatio",
                   "DisplayAspectRatio", "RFrameRate"
               ) VALUES (3000, ?, ?, '00:22:00', 30, 1080, 1920, 1, '1:1', '16:9', '24000/1001')"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
               VALUES (4000, 3000, '/media/show/s01e01.mkv', 'hash')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Language",
                   "Channels", "Title", "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (5000, 0, 'h264', 'High', 1, NULL, 0, NULL, 1, 0, 3000),
                   (5001, 1, 'srt', NULL, 3, 'eng', 0, 'English', 1, 0, 3000)"#,
        )
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "Subtitle" (
                   "Id", "EpisodeMetadataId", "StreamIndex", "SubtitleKind", "Title",
                   "Codec", "Language", "Default", "Forced", "Path", "IsExtracted",
                   "DateAdded", "DateUpdated"
               ) VALUES (6000, 300, 1, 0, 'English', 'srt', 'eng', 1, 0, '/cache/sub.srt', 1, ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "MediaChapter" (
                   "Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
               ) VALUES (7000, 3000, 1, '00:00:00', '00:01:00', 'Opening')"#,
        )
        .await
        .unwrap();
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::admin::server_fns::search::SearchAllIds;
    use ersatzrs_core::repositories::CollectionRepository;

    #[test]
    fn variant_browser_query_disables_the_search_contract_primary_only_default() {
        use ersatzrs_core::search::{SearchDocument, SearchExpression};

        let extra = SearchDocument {
            media_item_id: 40,
            media_kind: "other_video".to_owned(),
            content_kind: ersatzrs_domain::ContentKind::Extra,
            ..SearchDocument::default()
        };

        assert!(
            !SearchExpression::parse(&media_card_search_query("other_video", "", false,))
                .matches(&extra)
        );
        assert!(
            SearchExpression::parse(&media_card_search_query("other_video", "", true,))
                .matches(&extra)
        );
    }

    #[test]
    fn multi_collection_transport_mapping_preserves_item_fields() {
        let item = MultiCollectionItemForm {
            collection_id: Some(3),
            smart_collection_id: None,
            name: "Collection".to_owned(),
            collection_type: 0,
            use_custom_playback_order: true,
            schedule_as_group: true,
            playback_order: 2,
        };

        assert_eq!(
            multi_collection_item_form(multi_collection_item(item.clone())),
            item
        );
    }

    #[test]
    fn rerun_collection_transport_mapping_preserves_form_fields() {
        let form = RerunCollectionForm {
            id: 4,
            name: "Reruns".to_owned(),
            collection_type: 5,
            collection_id: None,
            media_item_id: None,
            multi_collection_id: None,
            smart_collection_id: Some(9),
            first_run_playback_order: 2,
            rerun_playback_order: 3,
        };

        assert_eq!(
            rerun_collection_form(rerun_collection_admin(form.clone())),
            form
        );
    }

    #[tokio::test]
    async fn save_collection_with_state_creates_manual_collection() {
        let (_database_dir, pool) = collection_test_pool("collection-create.db").await;
        let state = collection_test_state(pool.clone());

        let id = save_collection_with_state(
            &state,
            CollectionForm {
                id: 0,
                name: "  New Collection  ".to_string(),
                query: String::new(),
            },
        )
        .await
        .expect("create collection");

        let row: (String, i64) = sqlx::query_as(
            r#"SELECT "Name", "UseCustomPlaybackOrder" FROM "Collection" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("load created collection");

        assert_eq!(row.0, "New Collection");
        assert_eq!(row.1, 0);
    }

    #[tokio::test]
    async fn save_collection_with_state_rejects_case_insensitive_duplicate_name() {
        let (_database_dir, pool) = collection_test_pool("collection-duplicate.db").await;
        sqlx::query(
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
               VALUES (1, 'Favorites', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed collection");
        let state = collection_test_state(pool);

        let error = save_collection_with_state(
            &state,
            CollectionForm {
                id: 0,
                name: "favorites".to_string(),
                query: String::new(),
            },
        )
        .await
        .expect_err("duplicate collection name should fail");

        assert!(error.to_string().contains("Collection name must be unique"));
    }

    #[tokio::test]
    async fn save_collection_with_state_updates_existing_collection() {
        let (_database_dir, pool) = collection_test_pool("collection-update.db").await;
        sqlx::query(
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
               VALUES (1, 'Old Name', 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed collection");
        let state = collection_test_state(pool.clone());

        let id = save_collection_with_state(
            &state,
            CollectionForm {
                id: 1,
                name: "New Name".to_string(),
                query: String::new(),
            },
        )
        .await
        .expect("update collection");

        let name: String = sqlx::query_scalar(r#"SELECT "Name" FROM "Collection" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .expect("load updated collection");

        assert_eq!(id, 1);
        assert_eq!(name, "New Name");
    }

    #[tokio::test]
    async fn save_smart_collection_with_state_updates_and_refreshes_playouts() {
        let (_database_dir, pool) = collection_test_pool("smart-collection-update.db").await;
        seed_playout_refresh_fixture(&pool).await;
        let (state, mut playout_rx) = collection_test_state_with_playout_rx(pool.clone());

        let id = save_smart_collection_with_state(
            &state,
            CollectionForm {
                id: 40,
                name: "Updated Smart".to_string(),
                query: "movie".to_string(),
            },
        )
        .await
        .expect("update smart collection");

        let name: String =
            sqlx::query_scalar(r#"SELECT "Name" FROM "SmartCollection" WHERE "Id" = 40"#)
                .fetch_one(&pool)
                .await
                .expect("load updated smart collection");
        let first_refresh = playout_rx.recv().await.expect("first playout refresh");
        let second_refresh = playout_rx.recv().await.expect("second playout refresh");

        assert_eq!(id, 40);
        assert_eq!(name, "Updated Smart");
        assert_eq!(first_refresh.playout_id, 100);
        assert_eq!(second_refresh.playout_id, 101);
        assert_eq!(
            first_refresh.mode,
            ersatzrs_core::scheduling::PlayoutBuildMode::Refresh
        );
        assert_eq!(
            second_refresh.mode,
            ersatzrs_core::scheduling::PlayoutBuildMode::Refresh
        );
    }

    #[tokio::test]
    async fn load_show_languages_resolves_episode_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("show-languages.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_show_language_fixture(&pool).await;

        let languages = load_show_languages(&pool, 10)
            .await
            .expect("load show languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[tokio::test]
    async fn load_movie_languages_resolves_head_version_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("movie-languages.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_movie_language_fixture(&pool).await;

        let languages = load_movie_languages(&pool, 10)
            .await
            .expect("load movie languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[tokio::test]
    async fn load_movie_metadata_facts_resolves_studios_genres_and_tags() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("movie-facts.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_movie_metadata_fact_fixture(&pool).await;

        let studios = load_strings(&pool, r#""Studio""#, "MovieMetadataId", 100)
            .await
            .expect("load studios");
        let genres = load_strings(&pool, r#""Genre""#, "MovieMetadataId", 100)
            .await
            .expect("load genres");
        let tags = load_strings(&pool, r#""Tag""#, "MovieMetadataId", 100)
            .await
            .expect("load tags");

        assert_eq!(
            studios,
            vec!["A Studio".to_string(), "Z Studio".to_string()]
        );
        assert_eq!(
            genres,
            vec!["Drama".to_string(), "Science Fiction".to_string()]
        );
        assert_eq!(tags, vec!["Cult".to_string(), "Director's Cut".to_string()]);
    }

    #[tokio::test]
    async fn movie_detail_loads_extras_versions_and_audio_tracks() {
        let (_database_dir, pool) = collection_test_pool("movie-detail-variants.db").await;
        seed_movie_detail_variant_fixture(&pool).await;

        let movie = get_movie_details_with_pool(&pool, 10)
            .await
            .expect("load movie details")
            .expect("movie exists");

        assert_eq!(movie.title, "Example Movie");
        assert_eq!(movie.extras.len(), 1);
        assert_eq!(movie.extras[0].id, 30);
        assert_eq!(movie.extras[0].title, "Behind the Scenes");
        assert_eq!(
            movie.extras[0].variant.content_kind,
            ersatzrs_domain::ContentKind::Extra
        );
        assert_eq!(
            movie.extras[0].variant.sub_kind.as_deref(),
            Some("featurette")
        );

        assert_eq!(movie.alternate_versions.len(), 1);
        assert_eq!(movie.alternate_versions[0].id, 11);
        assert_eq!(
            movie.alternate_versions[0].title,
            "Example Movie Director's Cut"
        );
        assert_eq!(
            movie.alternate_versions[0].variant.content_kind,
            ersatzrs_domain::ContentKind::AlternateVersion
        );
        assert_eq!(
            movie.alternate_versions[0].variant.sub_kind.as_deref(),
            Some("directors_cut")
        );
        assert_eq!(
            movie.alternate_versions[0].variant.parent_title.as_deref(),
            Some("Example Movie")
        );

        assert_eq!(movie.audio_tracks.len(), 2);
        assert_eq!(movie.audio_tracks[0].media_version_id, 1000);
        assert_eq!(movie.audio_tracks[0].stream_index, 1);
        assert_eq!(movie.audio_tracks[0].title, "Main Audio");
        assert!(movie.audio_tracks[0].is_default);
        assert!(!movie.audio_tracks[0].is_commentary);
        assert_eq!(movie.audio_tracks[1].media_version_id, 1000);
        assert_eq!(movie.audio_tracks[1].stream_index, 2);
        assert_eq!(movie.audio_tracks[1].title, "Director Commentary");
        assert_eq!(movie.audio_tracks[1].language.as_deref(), Some("eng"));
        assert!(movie.audio_tracks[1].is_commentary);

        assert_eq!(movie.commentary_subtitles.len(), 1);
        assert_eq!(movie.commentary_subtitles[0].id, 1200);
        assert_eq!(movie.commentary_subtitles[0].stream_index, 3);
        assert_eq!(
            movie.commentary_subtitles[0].title,
            "Director Commentary Subtitles"
        );
        assert_eq!(
            movie.commentary_subtitles[0].language.as_deref(),
            Some("eng")
        );
    }

    #[tokio::test]
    async fn movie_extra_collection_and_playlist_actions_use_other_video_type() {
        let (_database_dir, pool) = collection_test_pool("movie-extra-actions.db").await;
        seed_movie_detail_variant_fixture(&pool).await;
        let state = collection_test_state(pool.clone());
        let owner = leptos::prelude::Owner::new();
        owner.with(|| leptos::prelude::provide_context(state));
        owner.set();

        let collection_added = add_media_items_to_collection(900, vec![30])
            .await
            .expect("add extra to collection");
        let playlist_added =
            add_media_items_to_playlist(MediaBrowserKind::OtherVideo, 902, vec![30])
                .await
                .expect("add extra to playlist");

        let collection_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "CollectionItem"
               WHERE "CollectionId" = 900 AND "MediaItemId" = 30"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load collection item count");
        let collection_type: i64 = sqlx::query_scalar(
            r#"SELECT "CollectionType" FROM "PlaylistItem"
               WHERE "PlaylistId" = 902 AND "MediaItemId" = 30"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load playlist item collection type");

        assert_eq!(collection_added, 1);
        assert_eq!(playlist_added, 1);
        assert_eq!(collection_count, 1);
        assert_eq!(collection_type, 40);

        owner.cleanup();
    }

    #[tokio::test]
    async fn media_item_collection_and_playlist_actions_persist_playback_overrides() {
        let (_database_dir, pool) = collection_test_pool("movie-playback-overrides.db").await;
        seed_movie_detail_variant_fixture(&pool).await;
        let state = collection_test_state(pool.clone());
        let owner = leptos::prelude::Owner::new();
        owner.with(|| leptos::prelude::provide_context(state));
        owner.set();

        let playback = PlaybackOverrideForm {
            preferred_audio_language_code: Some("eng".to_string()),
            preferred_audio_title: Some("Director Commentary".to_string()),
            preferred_subtitle_language_code: Some("eng".to_string()),
            subtitle_mode: Some(3),
        };
        let collection_added =
            add_media_items_to_collection_with_playback(900, vec![10], Some(playback.clone()))
                .await
                .expect("add movie to collection with playback override");
        let playlist_added = add_media_items_to_playlist_with_playback(
            MediaBrowserKind::Movie,
            902,
            vec![10],
            Some(playback),
        )
        .await
        .expect("add movie to playlist with playback override");

        let collection_row = sqlx::query(
            r#"SELECT "PreferredAudioLanguageCode", "PreferredAudioTitle",
                      "PreferredSubtitleLanguageCode", "SubtitleMode"
               FROM "CollectionItem"
               WHERE "CollectionId" = 900 AND "MediaItemId" = 10"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load collection item override");
        let playlist_row = sqlx::query(
            r#"SELECT "PreferredAudioLanguageCode", "PreferredAudioTitle",
                      "PreferredSubtitleLanguageCode", "SubtitleMode"
               FROM "PlaylistItem"
               WHERE "PlaylistId" = 902 AND "MediaItemId" = 10"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load playlist item override");

        assert_eq!(collection_added, 1);
        assert_eq!(playlist_added, 1);
        assert_playback_override_row(&collection_row);
        assert_playback_override_row(&playlist_row);

        let collection_detail = get_collection_detail(900)
            .await
            .expect("load collection detail")
            .expect("collection exists");
        let collection_card = collection_detail
            .items
            .iter()
            .find(|item| item.media_item_id == 10)
            .expect("collection item card exists");
        assert_eq!(
            collection_card.preferred_audio_title.as_deref(),
            Some("Director Commentary")
        );
        assert_eq!(
            collection_card.preferred_subtitle_language_code.as_deref(),
            Some("eng")
        );
        assert_eq!(collection_card.subtitle_mode, Some(3));

        owner.cleanup();
    }

    #[tokio::test]
    async fn duplicate_collection_rows_with_distinct_overrides_persist_and_remove_independently() {
        let (_database_dir, pool) = collection_test_pool("collection-duplicate-rows.db").await;
        seed_movie_detail_variant_fixture(&pool).await;
        let state = collection_test_state(pool.clone());
        let owner = leptos::prelude::Owner::new();
        owner.with(|| leptos::prelude::provide_context(state));
        owner.set();

        // Default add is idempotent.
        let default_added = add_media_items_to_collection(900, vec![10])
            .await
            .expect("add default row");
        let default_repeat = add_media_items_to_collection(900, vec![10])
            .await
            .expect("repeat default add");

        // Same movie again with two distinct commentary overrides — both must
        // persist as separate rows; an identical override tuple is skipped.
        let commentary_a = PlaybackOverrideForm {
            preferred_audio_title: Some("Director Commentary".to_string()),
            ..PlaybackOverrideForm::default()
        };
        let commentary_b = PlaybackOverrideForm {
            preferred_audio_title: Some("Isolated Score".to_string()),
            ..PlaybackOverrideForm::default()
        };
        let a_added =
            add_media_items_to_collection_with_playback(900, vec![10], Some(commentary_a.clone()))
                .await
                .expect("add commentary A row");
        let a_repeat =
            add_media_items_to_collection_with_playback(900, vec![10], Some(commentary_a))
                .await
                .expect("repeat commentary A add");
        let b_added =
            add_media_items_to_collection_with_playback(900, vec![10], Some(commentary_b))
                .await
                .expect("add commentary B row");

        assert_eq!(default_added, 1);
        assert_eq!(default_repeat, 0);
        assert_eq!(a_added, 1);
        assert_eq!(a_repeat, 0);
        assert_eq!(b_added, 1);

        // Collection detail projects one card per row with distinct row ids
        // and each card's own override summary fields.
        let detail = get_collection_detail(900)
            .await
            .expect("load collection detail")
            .expect("collection exists");
        let movie_cards: Vec<_> = detail
            .items
            .iter()
            .filter(|card| card.media_item_id == 10)
            .collect();
        assert_eq!(movie_cards.len(), 3, "one card per duplicate row");
        let mut row_ids: Vec<i64> = movie_cards
            .iter()
            .map(|card| card.collection_item_id)
            .collect();
        row_ids.sort_unstable();
        row_ids.dedup();
        assert_eq!(row_ids.len(), 3, "row ids must be distinct");
        assert!(movie_cards
            .iter()
            .any(|card| card.preferred_audio_title.is_none()));
        assert!(movie_cards
            .iter()
            .any(|card| card.preferred_audio_title.as_deref() == Some("Director Commentary")));

        // Removing one duplicate row leaves its siblings untouched.
        let commentary_a_row: i64 = sqlx::query_scalar(
            r#"SELECT "Id" FROM "CollectionItem"
               WHERE "CollectionId" = 900 AND "MediaItemId" = 10
                 AND "PreferredAudioTitle" = 'Director Commentary'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load commentary A row id");
        remove_items_from_collection(900, vec![commentary_a_row])
            .await
            .expect("remove commentary A row");

        let remaining: Vec<Option<String>> = sqlx::query_scalar(
            r#"SELECT "PreferredAudioTitle" FROM "CollectionItem"
               WHERE "CollectionId" = 900 AND "MediaItemId" = 10
               ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .expect("load remaining rows");
        assert_eq!(
            remaining,
            vec![None, Some("Isolated Score".to_string())],
            "only the removed duplicate row disappears"
        );

        owner.cleanup();
    }

    fn assert_playback_override_row(row: &ersatzrs_infra::db::DbRow) {
        use sqlx::Row;

        assert_eq!(
            row.try_get::<Option<String>, _>("PreferredAudioLanguageCode")
                .unwrap()
                .as_deref(),
            Some("eng")
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("PreferredAudioTitle")
                .unwrap()
                .as_deref(),
            Some("Director Commentary")
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("PreferredSubtitleLanguageCode")
                .unwrap()
                .as_deref(),
            Some("eng")
        );
        assert_eq!(
            row.try_get::<Option<i32>, _>("SubtitleMode").unwrap(),
            Some(3)
        );
    }

    #[tokio::test]
    async fn load_artist_languages_resolves_music_video_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("artist-languages.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_artist_language_fixture(&pool).await;

        let languages = load_artist_languages(&pool, 10)
            .await
            .expect("load artist languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[tokio::test]
    async fn load_other_video_languages_resolves_head_version_audio_stream_codes() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir
                .path()
                .join("other-video-languages.db")
                .display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_other_video_media_info_fixture(&pool).await;

        let languages = load_other_video_languages(&pool, 10)
            .await
            .expect("load other-video languages");

        assert_eq!(languages, vec!["Dutch".to_string(), "English".to_string()]);
    }

    #[tokio::test]
    async fn media_item_info_json_supports_other_videos() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join("other-video-info.db").display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_other_video_media_info_fixture(&pool).await;

        let json = get_media_item_info_json_with_pool(&pool, 10)
            .await
            .expect("load other-video media info");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("media info should be JSON");

        assert_eq!(value["Kind"], "OtherVideo");
        assert_eq!(value["Title"], "Example Other Video");
        assert_eq!(value["LibraryKind"], "LocalLibrary");
        assert_eq!(value["Streams"][0]["Kind"], "Video");
        assert_eq!(value["Streams"][1]["Language"], "eng");
        let subtitle_stream = value["Streams"]
            .as_array()
            .expect("streams should be an array")
            .iter()
            .find(|stream| stream["Kind"] == "Subtitle")
            .expect("embedded subtitle stream");
        assert_eq!(subtitle_stream["IsExtracted"], true);
        assert_eq!(subtitle_stream["FileName"], "/cache/other.srt");
        assert_eq!(value["Chapters"][0]["Title"], "Opening");
    }

    #[tokio::test]
    async fn playout_refresh_lookups_match_schedule_and_collection_contracts() {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir
                .path()
                .join("playout-refresh-lookups.db")
                .display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");

        seed_playout_refresh_fixture(&pool).await;

        assert_eq!(
            playout_ids_using_schedule(&pool, 10).await.unwrap(),
            vec![100, 101]
        );
        assert_eq!(
            playout_ids_using_collection(&pool, 20).await.unwrap(),
            vec![101, 102]
        );
        assert_eq!(
            ersatzrs_infra::repositories::SqliteCollectionRepository(pool.clone())
                .playout_ids_using_smart(40)
                .await
                .unwrap(),
            vec![100, 101]
        );
        assert_eq!(
            ersatzrs_infra::repositories::SqliteCollectionRepository(pool.clone())
                .playout_ids_using_rerun(30)
                .await
                .unwrap(),
            vec![100, 102]
        );
    }

    #[test]
    fn plan_mixed_playlist_inserts_is_empty_for_empty_payload() {
        let ids = SearchAllIds::default();
        assert!(plan_mixed_playlist_inserts(0, &ids).is_empty());
    }

    #[test]
    fn plan_mixed_playlist_inserts_stamps_collection_type_per_kind() {
        let ids = SearchAllIds {
            movie_ids: vec![101],
            show_ids: vec![201],
            season_ids: vec![301],
            episode_ids: vec![401],
            artist_ids: vec![501],
            music_video_ids: vec![601],
            other_video_ids: vec![701],
            song_ids: vec![801],
            image_ids: vec![901],
            remote_stream_ids: vec![1001],
        };

        let plan = plan_mixed_playlist_inserts(0, &ids);

        // (collection_type, media_item_id) in ErsatzTV iteration order.
        let observed: Vec<(i32, i64)> = plan
            .iter()
            .map(|row| (row.collection_type, row.media_item_id))
            .collect();
        assert_eq!(
            observed,
            vec![
                (10, 101),  // Movie
                (1, 201),   // TelevisionShow
                (2, 301),   // TelevisionSeason
                (20, 401),  // Episode
                (3, 501),   // Artist
                (30, 601),  // MusicVideo
                (40, 701),  // OtherVideo
                (50, 801),  // Song
                (60, 901),  // Image
                (70, 1001), // RemoteStream
            ]
        );
    }

    #[test]
    fn media_browser_kind_stamps_song_and_image_playlist_types() {
        assert_eq!(MediaBrowserKind::Song.collection_type(), 50);
        assert_eq!(MediaBrowserKind::Image.collection_type(), 60);
    }

    #[test]
    fn plan_mixed_playlist_inserts_indexes_are_contiguous_across_kinds() {
        let ids = SearchAllIds {
            movie_ids: vec![1, 2],
            show_ids: vec![3],
            episode_ids: vec![4, 5, 6],
            song_ids: vec![7],
            ..SearchAllIds::default()
        };

        let plan = plan_mixed_playlist_inserts(42, &ids);

        let indexes: Vec<i64> = plan.iter().map(|row| row.index).collect();
        assert_eq!(indexes, vec![42, 43, 44, 45, 46, 47, 48]);
        assert_eq!(plan.len(), 7);
        // Empty kinds (seasons/artists/etc.) must not consume an index slot.
        let kinds: Vec<i32> = plan.iter().map(|row| row.collection_type).collect();
        assert_eq!(kinds, vec![10, 10, 1, 20, 20, 20, 50]);
    }

    #[test]
    fn plan_mixed_playlist_inserts_respects_start_index() {
        let ids = SearchAllIds {
            movie_ids: vec![99],
            ..SearchAllIds::default()
        };

        let plan = plan_mixed_playlist_inserts(1000, &ids);
        assert_eq!(
            plan,
            vec![MixedPlaylistInsert {
                index: 1000,
                collection_type: 10,
                media_item_id: 99,
            }]
        );
    }

    #[test]
    fn movie_fact_search_links_match_ersatztv_lowercase_queries() {
        assert_eq!(
            crate::admin::pages::media::movies::search_link("studio", "Z Studio", true),
            "/search?query=studio%3A%22z%20studio%22"
        );
        assert_eq!(
            crate::admin::pages::media::movies::search_link("genre", "Science Fiction", true),
            "/search?query=genre%3A%22science%20fiction%22"
        );
        assert_eq!(
            crate::admin::pages::media::movies::search_link("tag", "Director's Cut", true),
            "/search?query=tag%3A%22director%27s%20cut%22"
        );
    }

    async fn seed_show_language_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Shows', 2, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/tv', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");

        for id in [10_i64, 11, 12] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(pool)
            .await
            .expect("seed media item");
        }

        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed show");
        sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (11, 10, 1)"#)
            .execute(pool)
            .await
            .expect("seed season");
        sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (12, 11)"#)
            .execute(pool)
            .await
            .expect("seed episode");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "EpisodeId",
                   "Height", "VideoScanKind", "Width"
               ) VALUES (
                   20, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                   '00:22:00', 12, 1080, 0, 1920
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (30, 0, 'aac', 2, 'eng', 2, 1, 0, 20),
                   (31, 1, 'aac', 2, 'en', 2, 0, 0, 20),
                   (32, 2, 'aac', 2, 'nld', 2, 0, 0, 20),
                   (33, 3, 'ass', 3, 'eng', 0, 0, 0, 20),
                   (34, 4, 'aac', 2, NULL, 2, 0, 0, 20)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
    }

    async fn seed_artist_language_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Music Videos', 3, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/music', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");

        for id in [10_i64, 11] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(pool)
            .await
            .expect("seed media item");
        }

        sqlx::query(r#"INSERT INTO "Artist" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed artist");
        sqlx::query(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (11, 10)"#)
            .execute(pool)
            .await
            .expect("seed music video");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "MusicVideoId",
                   "Height", "VideoScanKind", "Width"
               ) VALUES (
                   20, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                   '00:04:00', 11, 1080, 0, 1920
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (30, 0, 'aac', 2, 'eng', 2, 1, 0, 20),
                   (31, 1, 'aac', 2, 'en', 2, 0, 0, 20),
                   (32, 2, 'aac', 2, 'nld', 2, 0, 0, 20),
                   (33, 3, 'ass', 3, 'eng', 0, 0, 0, 20),
                   (34, 4, 'aac', 2, NULL, 2, 0, 0, 20)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
    }

    async fn seed_movie_metadata_fact_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Movies', 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/movies', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
               VALUES (10, 1, 0), (11, 1, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed media items");
        sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (10), (11)"#)
            .execute(pool)
            .await
            .expect("seed movies");
        sqlx::query(
            r#"INSERT INTO "MovieMetadata" (
                   "Id", "DateAdded", "DateUpdated", "MetadataKind", "MovieId", "Title"
               ) VALUES (
                   100, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z', 0,
                   10, 'Example Movie'
               ), (
                   101, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z', 0,
                   11, 'Other Movie'
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed movie metadata");
        sqlx::query(
            r#"INSERT INTO "Studio" ("Name", "MovieMetadataId") VALUES
                   ('Z Studio', 100),
                   ('A Studio', 100),
                   ('', 100),
                   (NULL, 100),
                   ('Other Movie Studio', 101)"#,
        )
        .execute(pool)
        .await
        .expect("seed studios");
        sqlx::query(
            r#"INSERT INTO "Genre" ("Name", "MovieMetadataId") VALUES
                   ('Science Fiction', 100),
                   ('Drama', 100),
                   ('Other Movie Genre', 101)"#,
        )
        .execute(pool)
        .await
        .expect("seed genres");
        sqlx::query(
            r#"INSERT INTO "Tag" ("Name", "MovieMetadataId") VALUES
                   ('Director''s Cut', 100),
                   ('Cult', 100),
                   ('Other Movie Tag', 101)"#,
        )
        .execute(pool)
        .await
        .expect("seed tags");
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_movie_detail_variant_fixture(pool: &ersatzrs_infra::db::DbPool) {
        let now = "2026-07-01T00:00:00Z";
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed local media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Movies', 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed local library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/movies', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        sqlx::query(
            r#"INSERT INTO "MediaItem" (
                   "Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId"
               ) VALUES
                   (10, 1, 0, 0, NULL, NULL),
                   (11, 1, 0, 2, 'directors_cut', 10),
                   (30, 1, 0, 1, 'featurette', 10)"#,
        )
        .execute(pool)
        .await
        .expect("seed media items");
        sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (10), (11)"#)
            .execute(pool)
            .await
            .expect("seed movies");
        sqlx::query(r#"INSERT INTO "OtherVideo" ("Id") VALUES (30)"#)
            .execute(pool)
            .await
            .expect("seed other video");
        sqlx::query(
            r#"INSERT INTO "MovieMetadata" (
                   "Id", "MovieId", "MetadataKind", "Title", "SortTitle", "Year",
                   "Plot", "DateAdded", "DateUpdated"
               ) VALUES
                   (100, 10, 0, 'Example Movie', 'example movie', 2026,
                    'Primary movie.', ?, ?),
                   (101, 11, 0, 'Example Movie Director''s Cut',
                    'example movie directors cut', 2026, 'Alternate edition.', ?, ?)"#,
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed movie metadata");
        sqlx::query(
            r#"INSERT INTO "OtherVideoMetadata" (
                   "Id", "OtherVideoId", "MetadataKind", "Title", "OriginalTitle",
                   "SortTitle", "Year", "DateAdded", "DateUpdated"
               ) VALUES (
                   300, 30, 0, 'Behind the Scenes', 'Featurette',
                   'behind the scenes', 2026, ?, ?
               )"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed other-video metadata");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "MovieId",
                   "Height", "Width", "VideoScanKind", "SampleAspectRatio",
                   "DisplayAspectRatio", "RFrameRate"
               ) VALUES (
                   1000, ?, ?, '02:00:00', 10, 1080, 1920, 1, '1:1', '16:9', '24000/1001'
               )"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Language",
                   "Channels", "Title", "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (1100, 0, 'h264', 'High', 1, NULL, 0, NULL, 1, 0, 1000),
                   (1101, 1, 'aac', NULL, 2, 'eng', 2, 'Main Audio', 1, 0, 1000),
                   (1102, 2, 'aac', NULL, 2, 'eng', 2, 'Director Commentary', 0, 0, 1000),
                   (1103, 3, 'srt', NULL, 3, 'eng', 0, 'Director Commentary Subtitles', 0, 0, 1000)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
        sqlx::query(
            r#"INSERT INTO "Subtitle" (
                   "Id", "MovieMetadataId", "StreamIndex", "SubtitleKind", "Title",
                   "Codec", "Language", "Default", "Forced", "Path", "IsExtracted",
                   "DateAdded", "DateUpdated"
               ) VALUES (
                   1200, 100, 3, 0, 'Director Commentary Subtitles', 'srt', 'eng',
                   0, 0, '/cache/movie-commentary.srt', 1, ?, ?
               )"#,
        )
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("seed commentary subtitle");
        sqlx::query(
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
               VALUES (900, 'Favorites', 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed collection");
        sqlx::query(
            r#"INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
               VALUES (901, 'Movies', 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed playlist group");
        sqlx::query(
            r#"INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
               VALUES (902, 'Featurettes', 901, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed playlist");
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_other_video_media_info_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed local media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Other Videos', 4, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed local library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/other', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (10, 1, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed media item");
        sqlx::query(r#"INSERT INTO "OtherVideo" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed other video");
        sqlx::query(
            r#"INSERT INTO "OtherVideoMetadata" (
                   "Id", "DateAdded", "DateUpdated", "MetadataKind", "OtherVideoId",
                   "Title", "OriginalTitle"
               ) VALUES (
                   100, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z', 0,
                   10, 'Example Other Video', 'folder/example.mkv'
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed other-video metadata");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "OtherVideoId",
                   "Height", "Width", "VideoScanKind", "SampleAspectRatio",
                   "DisplayAspectRatio", "RFrameRate"
               ) VALUES (
                   20, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z',
                   '00:30:00', 10, 1080, 1920, 1, '1:1', '16:9', '24000/1001'
               ), (
                   21, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z',
                   '00:30:00', 10, 1080, 1920, 1, '1:1', '16:9', '24000/1001'
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed media versions");
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
               VALUES (200, 20, '/other/example.mkv', 'hash')"#,
        )
        .execute(pool)
        .await
        .expect("seed media file");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Language",
                   "Channels", "Title", "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (300, 0, 'h264', 'High', 1, NULL, 0, NULL, 1, 0, 20),
                   (301, 1, 'aac', NULL, 2, 'eng', 2, 'English', 1, 0, 20),
                   (302, 2, 'aac', NULL, 2, 'en', 2, 'English Alt', 0, 0, 20),
                   (303, 3, 'aac', NULL, 2, 'nld', 2, 'Dutch', 0, 0, 20),
                   (304, 4, 'srt', NULL, 3, 'eng', 0, 'English SDH', 0, 0, 20),
                   (305, 0, 'aac', NULL, 2, 'fra', 2, 'French', 1, 0, 21)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
        sqlx::query(
            r#"INSERT INTO "Subtitle" (
                   "Id", "OtherVideoMetadataId", "StreamIndex", "SubtitleKind", "Title",
                   "Codec", "Language", "Default", "Forced", "Path", "IsExtracted",
                   "DateAdded", "DateUpdated"
               ) VALUES (
                   400, 100, 4, 0, 'English SDH', 'srt', 'eng', 0, 0,
                   '/cache/other.srt', 1, '2026-05-22T00:00:00Z', '2026-05-22T00:00:00Z'
               )"#,
        )
        .execute(pool)
        .await
        .expect("seed subtitle");
        sqlx::query(
            r#"INSERT INTO "MediaChapter" (
                   "Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
               ) VALUES (500, 20, 1, '00:00:00', '00:01:00', 'Opening')"#,
        )
        .execute(pool)
        .await
        .expect("seed chapter");
    }

    async fn seed_movie_language_fixture(pool: &ersatzrs_infra::db::DbPool) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .expect("seed media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Movies', 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library");
        sqlx::query(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, '/movies', 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed library path");
        sqlx::query(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (10, 1, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed media item");
        sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (10)"#)
            .execute(pool)
            .await
            .expect("seed movie");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                   "Id", "DateAdded", "DateUpdated", "Duration", "MovieId",
                   "Height", "VideoScanKind", "Width"
               ) VALUES
                   (20, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                    '02:20:00', 10, 1080, 0, 1920),
                   (21, '2026-05-18T00:00:00Z', '2026-05-18T00:00:00Z',
                    '02:20:00', 10, 1080, 0, 1920)"#,
        )
        .execute(pool)
        .await
        .expect("seed media versions");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                   "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
                   "Default", "Forced", "MediaVersionId"
               ) VALUES
                   (30, 0, 'aac', 2, 'eng', 2, 1, 0, 20),
                   (31, 1, 'aac', 2, 'en', 2, 0, 0, 20),
                   (32, 2, 'aac', 2, 'nld', 2, 0, 0, 20),
                   (33, 3, 'ass', 3, 'eng', 0, 0, 0, 20),
                   (34, 0, 'aac', 2, 'fra', 2, 1, 0, 21)"#,
        )
        .execute(pool)
        .await
        .expect("seed media streams");
    }

    async fn seed_playout_refresh_fixture(pool: &ersatzrs_infra::db::DbPool) {
        use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};

        sqlx::query(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height")
               VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .execute(pool)
        .await
        .expect("seed resolution");

        let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
        repos
            .ffmpeg_profile
            .upsert(&ersatzrs_domain::FFmpegProfile {
                id: 1,
                name: "default".to_string(),
                resolution_id: 1,
                ..ersatzrs_domain::FFmpegProfile::default()
            })
            .await
            .expect("seed ffmpeg profile");
        repos
            .channel
            .upsert(&ersatzrs_domain::Channel {
                id: 1,
                unique_id: uuid::Uuid::from_u128(1),
                number: "1".to_string(),
                sort_number: 1.0,
                name: "Channel 1".to_string(),
                ffmpeg_profile_id: 1,
                is_enabled: true,
                ..ersatzrs_domain::Channel::default()
            })
            .await
            .expect("seed channel");

        sqlx::query(
            r#"INSERT INTO "ProgramSchedule" (
                   "Id", "FixedStartTimeBehavior", "KeepMultiPartEpisodesTogether",
                   "Name", "RandomStartPoint", "ShuffleScheduleItems",
                   "TreatCollectionsAsShows"
               ) VALUES
                   (10, 0, 0, 'Schedule 10', 0, 0, 0),
                   (11, 0, 0, 'Schedule 11', 0, 0, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed schedules");
        sqlx::query(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (20, 'Collection 20', 0)"#)
            .execute(pool)
            .await
            .expect("seed collection");
        sqlx::query(r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query") VALUES (40, 'Smart 40', 'movie')"#)
            .execute(pool)
            .await
            .expect("seed smart collection");
        sqlx::query(
            r#"INSERT INTO "RerunCollection" (
                   "Id", "Name", "CollectionType", "CollectionId",
                   "FirstRunPlaybackOrder", "RerunPlaybackOrder"
               ) VALUES (30, 'Rerun 30', 0, 20, 1, 1)"#,
        )
        .execute(pool)
        .await
        .expect("seed rerun collection");
        sqlx::query(
            r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
               VALUES
                   (100, 1, 10, 1, 0),
                   (101, 1, 10, 1, 0),
                   (102, 1, 11, 1, 0)"#,
        )
        .execute(pool)
        .await
        .expect("seed playouts");
        sqlx::query(
            r#"INSERT INTO "PlayoutProgramScheduleAnchor" (
                   "Id", "CollectionId", "RerunCollectionId", "SmartCollectionId", "CollectionType", "PlayoutId"
               ) VALUES
                   (200, 20, NULL, NULL, 10, 101),
                   (201, 20, NULL, NULL, 10, 102),
                   (202, 20, NULL, NULL, 10, 102),
                   (203, NULL, 30, NULL, 7, 100),
                   (204, NULL, 30, NULL, 8, 102),
                   (205, NULL, NULL, 40, 5, 100),
                   (206, NULL, NULL, 40, 5, 101)"#,
        )
        .execute(pool)
            .await
            .expect("seed playout anchors");
    }

    async fn collection_test_pool(
        database_name: &str,
    ) -> (tempfile::TempDir, ersatzrs_infra::db::DbPool) {
        let database_dir = tempfile::tempdir().expect("temp database dir");
        let database_url = format!(
            "sqlite:{}",
            database_dir.path().join(database_name).display()
        );
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database");
        (database_dir, pool)
    }

    fn collection_test_state(pool: ersatzrs_infra::db::DbPool) -> crate::host::state::AppState {
        collection_test_state_with_playout_rx(pool).0
    }

    fn collection_test_state_with_playout_rx(
        pool: ersatzrs_infra::db::DbPool,
    ) -> (
        crate::host::state::AppState,
        tokio::sync::mpsc::UnboundedReceiver<ersatzrs_core::scheduling::PlayoutBuildRequest>,
    ) {
        let (playout_build_tx, playout_build_rx) = tokio::sync::mpsc::unbounded_channel();
        let state = crate::host::state::AppState::new(
            pool,
            ersatzrs_infra::db::DatabaseBackend::Sqlite,
            crate::host::state::SystemStartup {
                database_ready: true,
                schema_status: ersatzrs_infra::db::DatabaseSchemaStatus::default(),
            },
            ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
            uuid::Uuid::nil(),
            1,
            crate::host::state::IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );
        (state, playout_build_rx)
    }
}
