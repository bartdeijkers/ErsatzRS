use super::*;
use ersatzrs_core::repositories::{EmbyConnectionConfig, JellyfinConnectionConfig};
use ersatzrs_infra::{emby, jellyfin, plex, provider_collection_refresh};
use httpmock::prelude::*;
use serde_json::json;

#[derive(Clone, Copy)]
enum Provider {
    Plex,
    Jellyfin,
    Emby,
}

impl Provider {
    fn name(self) -> &'static str {
        match self {
            Self::Plex => "Plex",
            Self::Jellyfin => "Jellyfin",
            Self::Emby => "Emby",
        }
    }

    async fn sync(self, pool: &SqlitePool, members: &[i64]) {
        self.sync_source(pool, 1, members).await;
    }

    async fn sync_source(self, pool: &SqlitePool, source: i64, members: &[i64]) {
        let server = MockServer::start_async().await;
        let plex = matches!(self, Self::Plex);
        let list = server.mock_async(|when, then| {
            if plex { when.method(GET).path("/library/all").query_param("type", "18"); }
            else { when.method(GET).path("/Items").query_param("parentId", "4").query_param("includeItemTypes", "BoxSet"); }
            then.status(200).json_body(if plex {
                json!({"MediaContainer": {"totalSize": 1, "Metadata": [{"ratingKey": "900", "title": "Provider collection", "librarySectionId": 4}]}})
            } else { json!({"TotalRecordCount": 1, "Items": [{"Id": "900", "Name": "Provider collection"}]}) });
        }).await;
        let items = server.mock_async(|when, then| {
            if plex { when.method(GET).path("/library/collections/900/children"); }
            else { when.method(GET).path("/Items").query_param("parentId", "900"); }
            let rows: Vec<_> = members.iter().map(|id| if plex {
                json!({"key": format!("/library/metadata/{id}"), "type": match id { 300 => "show", 301 => "season", _ => "movie" }})
            } else { json!({"Id": id.to_string(), "Type": match id { 300 => "Series", 301 => "Season", _ => "Movie" }, "LocationType": "FileSystem"}) }).collect();
            then.status(200).json_body(if plex {
                json!({"MediaContainer": {"totalSize": members.len(), "Metadata": rows}})
            } else { json!({"TotalRecordCount": members.len(), "Items": rows}) });
        }).await;
        let count = match self {
            Self::Plex => {
                plex::sync_plex_collection_library(
                    pool,
                    &plex::PlexService::new(),
                    &plex::PlexConnectionConfig {
                        uri: server.base_url(),
                        token: "synthetic".into(),
                    },
                    source,
                    "4",
                )
                .await
            }
            Self::Jellyfin => {
                jellyfin::sync_jellyfin_collection_library(
                    pool,
                    &jellyfin::JellyfinService::new(),
                    &JellyfinConnectionConfig {
                        address: server.base_url(),
                        authorization_header: "synthetic".into(),
                    },
                    source,
                    "4",
                )
                .await
            }
            Self::Emby => {
                emby::sync_emby_collection_library(
                    pool,
                    &emby::EmbyService::new(),
                    &EmbyConnectionConfig {
                        address: server.base_url(),
                        api_key: "synthetic".into(),
                    },
                    source,
                    "4",
                )
                .await
            }
        }
        .unwrap();
        assert_eq!(count, 1);
        list.assert_calls_async(1).await;
        items.assert_calls_async(1).await;
    }
}

async fn seed_provider(pool: &SqlitePool, provider: Provider, order: &str) {
    seed_acceptance_fixture(pool).await;
    for id in [103, 104] {
        common::seed_media_item(pool, id, 1).await;
        seed_movie(pool, id, &format!("Related {id}")).await;
    }
    pool.execute(r#"DELETE FROM "LocalMediaSource" WHERE "Id" = 1"#)
        .await
        .unwrap();
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
        r#"INSERT INTO "{}MediaSource" ("Id") VALUES (1)"#,
        provider.name()
    )))
    .execute(pool)
    .await
    .unwrap();
    for id in [101, 102, 103, 104, 201] {
        let (column, key) = if matches!(provider, Provider::Plex) {
            ("Key", format!("/library/metadata/{id}"))
        } else {
            ("ItemId", id.to_string())
        };
        sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
            r#"INSERT INTO "{}Movie" ("Id", "{column}") VALUES (?, ?)"#,
            provider.name()
        )))
        .bind(id)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    }
    // A second server reuses both the collection ID and media key. Its
    // separately scheduled item must remain isolated during source-one sync.
    seed_second_source(pool, provider).await;
    provider.sync_source(pool, 2, &[101]).await;
    provider.sync(pool, &[101, 103, 104]).await;
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
        r#"UPDATE "ProgramScheduleItem" SET "CollectionType" = 'Collection',
        "CollectionId" = (SELECT "Id" FROM "{}Collection" WHERE "MediaSourceId" = 1),
        "PlaylistId" = NULL, "PlaybackOrder" = ? WHERE "Id" = 401"#,
        provider.name()
    )))
    .bind(order)
    .execute(pool)
    .await
    .unwrap();
    pool.execute(
        r#"UPDATE "ConfigElement" SET "Value" = '2' WHERE "Key" = 'playout.days_to_build'"#,
    )
    .await
    .unwrap();
    provider_collection_refresh::resolve_pending(pool)
        .await
        .unwrap();
}

async fn seed_second_source(pool: &SqlitePool, provider: Provider) {
    common::seed_media_path(pool, 2, 2, 2).await;
    pool.execute(
        r#"DELETE FROM "LocalMediaSource" WHERE "Id" = 2;
        UPDATE "MediaItem" SET "LibraryPathId" = 2 WHERE "Id" = 201"#,
    )
    .await
    .unwrap();
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
        r#"INSERT INTO "{}MediaSource" ("Id") VALUES (2)"#,
        provider.name()
    )))
    .execute(pool)
    .await
    .unwrap();
    let (column, key) = if matches!(provider, Provider::Plex) {
        ("Key", "/library/metadata/101")
    } else {
        ("ItemId", "101")
    };
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
        r#"UPDATE "{}Movie" SET "{column}" = ? WHERE "Id" = 201"#,
        provider.name()
    )))
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
}

async fn publish_and_resolve(pool: &SqlitePool, provider: Provider) {
    provider.sync(pool, &[104, 101, 102]).await;
    assert_eq!(
        provider_collection_refresh::resolve_pending(pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(intended_playout_ids(pool).await, [RELATED_PLAYOUT_ID]);
}

async fn continuity(provider: Provider, order: &str) {
    let dir = tempfile::tempdir().unwrap();
    let baseline_path = dir.path().join("baseline.db");
    let oracle_path = dir.path().join("oracle.db");
    let restarted_path = dir.path().join("restarted.db");
    let baseline = open_database(&baseline_path, true).await;
    seed_provider(&baseline, provider, order).await;
    let start = common::fixed_time();
    let refresh = start + Duration::days(1) + Duration::minutes(1);
    for id in [RELATED_PLAYOUT_ID, UNRELATED_PLAYOUT_ID] {
        run_classic_build(&baseline, id, PlayoutBuildMode::Reset, start).await;
    }
    let unrelated = persisted_classic_rows(&baseline, UNRELATED_PLAYOUT_ID).await;
    let related = persisted_classic_rows(&baseline, RELATED_PLAYOUT_ID).await;
    assert!(!related.classic_checkpoints.is_empty());
    assert!(intended_playout_ids(&baseline).await.is_empty());
    provider.sync(&baseline, &[101, 103, 104]).await;
    assert_eq!(
        provider_collection_refresh::resolve_pending(&baseline)
            .await
            .unwrap(),
        0
    );
    assert!(related == persisted_classic_rows(&baseline, RELATED_PLAYOUT_ID).await);
    baseline.close().await;
    tokio::fs::copy(&baseline_path, &oracle_path).await.unwrap();
    tokio::fs::copy(&baseline_path, &restarted_path)
        .await
        .unwrap();
    let oracle = open_database(&oracle_path, false).await;
    publish_and_resolve(&oracle, provider).await;
    run_classic_build(
        &oracle,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Refresh,
        refresh,
    )
    .await;
    let expected = persisted_classic_rows(&oracle, RELATED_PLAYOUT_ID).await;
    let restarted = open_database(&restarted_path, false).await;
    provider.sync(&restarted, &[104, 101, 102]).await;
    assert!(intended_playout_ids(&restarted).await.is_empty());
    restarted.close().await;
    let reopened = open_database(&restarted_path, false).await;
    assert_eq!(
        provider_collection_refresh::resolve_pending(&reopened)
            .await
            .unwrap(),
        1
    );
    assert_eq!(intended_playout_ids(&reopened).await, [RELATED_PLAYOUT_ID]);
    run_classic_build(
        &reopened,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Refresh,
        refresh,
    )
    .await;
    assert!(expected == persisted_classic_rows(&reopened, RELATED_PLAYOUT_ID).await);
    assert!(playout_media_count(&reopened, RELATED_PLAYOUT_ID, ADDED_MEDIA_ID).await > 0);
    assert_eq!(
        playout_media_count(&reopened, RELATED_PLAYOUT_ID, 103).await,
        0
    );
    assert_unrelated_rows_unchanged(
        &unrelated,
        &persisted_classic_rows(&reopened, UNRELATED_PLAYOUT_ID).await,
    );
    assert!(intended_playout_ids(&reopened).await.is_empty());
}

#[tokio::test]
async fn plex_provider_publication_preserves_classic_continuity() {
    for order in ["Shuffle", "Random", "RandomRotation"] {
        continuity(Provider::Plex, order).await;
    }
}
#[tokio::test]
async fn jellyfin_provider_publication_preserves_classic_continuity() {
    for order in ["Shuffle", "Random", "RandomRotation"] {
        continuity(Provider::Jellyfin, order).await;
    }
}
#[tokio::test]
async fn emby_provider_publication_preserves_classic_continuity() {
    for order in ["Shuffle", "Random", "RandomRotation"] {
        continuity(Provider::Emby, order).await;
    }
}

async fn seed_provider_show(pool: &SqlitePool, provider: Provider) {
    for id in [300, 301, 302] {
        common::seed_media_item(pool, id, 1).await;
    }
    pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (300);
        INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (301, 300, 1);
        INSERT INTO "Episode" ("Id", "SeasonId") VALUES (302, 301);
        INSERT INTO "ShowMetadata" ("Id", "ShowId", "MetadataKind", "Title", "DateAdded", "DateUpdated")
          VALUES (300, 300, 0, 'Parent', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z');
        INSERT INTO "SeasonMetadata" ("Id", "SeasonId", "MetadataKind", "DateAdded", "DateUpdated")
          VALUES (301, 301, 0, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z');
        INSERT INTO "EpisodeMetadata" ("Id", "EpisodeId", "MetadataKind", "Title", "EpisodeNumber", "DateAdded", "DateUpdated")
          VALUES (302, 302, 0, 'Child', 1, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z');
        INSERT INTO "MediaVersion" ("Id", "EpisodeId", "Name", "Duration", "VideoScanKind", "DateAdded", "DateUpdated", "Width", "Height")
          VALUES (3020, 302, 'main', '00:10:00', 'Progressive', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 1920, 1080)"#).await.unwrap();
    for (id, kind) in [(300, "Show"), (301, "Season")] {
        let (column, key) = if matches!(provider, Provider::Plex) {
            ("Key", format!("/library/metadata/{id}"))
        } else {
            ("ItemId", id.to_string())
        };
        sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
            r#"INSERT INTO "{}{kind}" ("Id", "{column}") VALUES (?, ?)"#,
            provider.name()
        )))
        .bind(id)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    }
    let repositories = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repositories, 903).await;
    seed_classic_playout(pool, &repositories, 903, 303, 403, RELATED_PLAYLIST_ID).await;
    pool.execute(r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query")
        VALUES (8, 'Inherited collection tag', 'title:Child AND show_tag:"Provider collection"');
        UPDATE "ProgramScheduleItem" SET "CollectionType" = 'SmartCollection', "SmartCollectionId" = 8,
        "PlaylistId" = NULL WHERE "Id" = 403"#).await.unwrap();
}

#[tokio::test]
async fn provider_publication_invalidates_show_and_season_descendants_and_inherited_search() {
    for provider in [Provider::Plex, Provider::Jellyfin, Provider::Emby] {
        let dir = tempfile::tempdir().unwrap();
        let pool = open_database(&dir.path().join("descendants.db"), true).await;
        seed_provider(&pool, provider, "Shuffle").await;
        // Consume initial fixture intents through real builds before changing
        // only a container member; no changed movie can mask a missing expansion.
        for id in [RELATED_PLAYOUT_ID, UNRELATED_PLAYOUT_ID] {
            run_classic_build(&pool, id, PlayoutBuildMode::Reset, common::fixed_time()).await;
        }
        seed_provider_show(&pool, provider).await;
        provider.sync(&pool, &[101, 103, 104, 300]).await;
        assert_eq!(
            provider_collection_refresh::resolve_pending(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(intended_playout_ids(&pool).await, [RELATED_PLAYOUT_ID, 903]);
        for id in [RELATED_PLAYOUT_ID, 903] {
            run_classic_build(&pool, id, PlayoutBuildMode::Reset, common::fixed_time()).await;
        }
        assert!(playout_media_count(&pool, 903, 302).await > 0);
        provider.sync(&pool, &[101, 103, 104, 301]).await;
        assert_eq!(
            provider_collection_refresh::resolve_pending(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(intended_playout_ids(&pool).await, [RELATED_PLAYOUT_ID, 903]);
    }
}
