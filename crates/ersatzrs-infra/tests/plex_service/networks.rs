use super::*;
use ersatzrs_infra::plex::sync_plex_networks;
use sqlx::Executor;

async fn seeded_library() -> SqlitePool {
    let server = MockServer::start_async().await;
    mock_plex_show_pages(&server).await;
    let pool = common::test_db().await;
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".into(),
    };
    for (source, library) in [(2, 99), (1, 77), (1, 1)] {
        sync_plex_show_library(
            &pool,
            &PlexService::new(),
            &config,
            source,
            library,
            "2",
            true,
        )
        .await
        .unwrap();
    }
    for owner in ["319", "plex/collection"] {
        sqlx::query(r#"INSERT INTO "Tag" ("Name", "ExternalTypeId", "ShowMetadataId") SELECT 'Stale', ?, "Id" FROM "ShowMetadata""#)
            .bind(owner).execute(&pool).await.unwrap();
    }
    sqlx::query(r#"UPDATE "Library" SET "MediaKind" = 2 WHERE "Id" IN (1, 77, 99)"#)
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn tags(pool: &SqlitePool) -> Vec<(i64, String, String, i64)> {
    sqlx::query_as(
        r#"SELECT t."Id", t."Name", t."ExternalTypeId", p."LibraryId" FROM "Tag" t
        JOIN "ShowMetadata" s ON s."Id" = t."ShowMetadataId"
        JOIN "MediaItem" m ON m."Id" = s."ShowId" JOIN "LibraryPath" p ON p."Id" = m."LibraryPathId"
        ORDER BY t."Id""#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn matching_network_episodes(pool: &SqlitePool) -> usize {
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    repos
        .scheduling
        .media_items_for_source(&ersatzrs_core::scheduling::SchedulingContentSource::Search(
            "show_network:Network".into(),
        ))
        .await
        .unwrap()
        .len()
}

async fn network_list(server: &MockServer) {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/tags")
                .query_param("type", "319");
            then.json_body(json!({"MediaContainer": {"totalSize": 1, "Directory": [
                {"tag": "Network", "tagType": 319, "filter": "network=42%2C43"}
            ]}}));
        })
        .await;
}

#[tokio::test]
async fn network_refresh_is_library_scoped_and_preserves_other_tag_owners() {
    let pool = seeded_library().await;
    let server = MockServer::start_async().await;
    network_list(&server).await;
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".into(),
    };
    let service = PlexService::new();
    let members = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/2/all")
                .query_param("network", "42,43");
            then.json_body(json!({"MediaContainer": {"totalSize": 2, "Metadata": [
                {"ratingKey": "200"}, {"key": "/library/metadata/unknown/children"}
            ]}}));
        })
        .await;
    let before = tags(&pool).await;
    assert_eq!(
        sync_plex_networks(&pool, &service, &config, 1)
            .await
            .unwrap(),
        1
    );
    let first = tags(&pool).await;
    assert_eq!(matching_network_episodes(&pool).await, 1);
    assert_eq!(
        first
            .iter()
            .filter(|(_, _, owner, library)| owner == "319" && *library == 1)
            .map(|(_, name, _, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["Stale", "Network"]
    );
    for retained in &before {
        assert!(first.contains(retained));
    }
    assert_eq!(
        sync_plex_networks(&pool, &service, &config, 1)
            .await
            .unwrap(),
        0
    );
    assert_eq!(tags(&pool).await, first);
    let timestamp: String =
        sqlx::query_scalar(r#"SELECT "LastNetworksScan" FROM "PlexLibrary" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    members.delete_async().await;
    let failure = server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all");
            then.status(503);
        })
        .await;
    assert!(sync_plex_networks(&pool, &service, &config, 1)
        .await
        .is_err());
    assert_eq!(tags(&pool).await, first);
    let after_failure: String =
        sqlx::query_scalar(r#"SELECT "LastNetworksScan" FROM "PlexLibrary" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_failure, timestamp);
    failure.delete_async().await;
    server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all");
            then.json_body(json!({"MediaContainer": {"totalSize": 0, "Metadata": []}}));
        })
        .await;
    assert_eq!(
        sync_plex_networks(&pool, &service, &config, 1)
            .await
            .unwrap(),
        1
    );
    assert_eq!(tags(&pool).await, before);
    assert_eq!(matching_network_episodes(&pool).await, 0);
}

#[tokio::test]
async fn network_refresh_requires_complete_membership_pages_before_removing_tags() {
    let pool = seeded_library().await;
    let server = MockServer::start_async().await;
    network_list(&server).await;
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".into(),
    };
    let service = PlexService::new();
    let before = tags(&pool).await;
    let partial = server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all")
                .query_param("X-Plex-Container-Start", "0");
            then.json_body(
                json!({"MediaContainer": {"totalSize": 101, "Metadata": [{"ratingKey":"200"}]}}),
            );
        })
        .await;
    assert!(sync_plex_networks(&pool, &service, &config, 1)
        .await
        .is_err());
    assert_eq!(tags(&pool).await, before);
    partial.delete_async().await;
    server.mock_async(|when, then| {
        when.path("/library/sections/2/all").query_param("X-Plex-Container-Start", "0");
        then.json_body(json!({"MediaContainer": {"totalSize": 101, "Metadata": (200..300).map(|id| json!({"ratingKey": id.to_string()})).collect::<Vec<_>>()}}));
    }).await;
    let last = server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all")
                .query_param("X-Plex-Container-Start", "100");
            then.json_body(
                json!({"MediaContainer": {"totalSize": 101, "Metadata": [{"ratingKey":"300"}]}}),
            );
        })
        .await;
    assert_eq!(
        sync_plex_networks(&pool, &service, &config, 1)
            .await
            .unwrap(),
        1
    );
    last.assert_async().await;
}

#[tokio::test]
async fn network_refresh_rolls_back_removed_tags_when_timestamp_write_fails() {
    let pool = seeded_library().await;
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.path("/library/tags");
            then.json_body(json!({"MediaContainer": {"totalSize": 1, "Directory": [
                {"tag": "Stale", "tagType": 319, "filter": "network=42"}
            ]}}));
        })
        .await;
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".into(),
    };
    server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all");
            then.json_body(json!({"MediaContainer": {"totalSize": 0, "Metadata": []}}));
        })
        .await;
    let before = tags(&pool).await;
    sqlx::query(
        r#"CREATE TRIGGER reject_network BEFORE UPDATE ON "PlexLibrary"
        BEGIN SELECT RAISE(ABORT, 'synthetic network failure'); END"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(sync_plex_networks(&pool, &PlexService::new(), &config, 1)
        .await
        .is_err());
    assert_eq!(tags(&pool).await, before);
    let last_scan: Option<String> =
        sqlx::query_scalar(r#"SELECT "LastNetworksScan" FROM "PlexLibrary" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(last_scan.is_none());
    sqlx::query("DROP TRIGGER reject_network")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sync_plex_networks(&pool, &PlexService::new(), &config, 1)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn tv_library_dispatch_refreshes_networks_after_show_metadata() {
    let pool = common::test_db().await;
    let server = MockServer::start_async().await;
    seed_plex_show_scan_config(&pool, &server.base_url()).await;
    mock_plex_show_pages(&server).await;
    network_list(&server).await;
    let members = server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all")
                .query_param("network", "42,43");
            then.json_body(
                json!({"MediaContainer": {"totalSize": 1, "Metadata": [{"ratingKey":"200"}]}}),
            );
        })
        .await;
    ersatzrs_infra::plex::sync_library_by_id(&pool, 1)
        .await
        .unwrap();
    members.assert_async().await;
    let actual = tags(&pool).await;
    assert!(actual
        .iter()
        .any(|(_, name, owner, library)| name == "Network" && owner == "319" && *library == 1));
    assert!(actual
        .iter()
        .any(|(_, name, owner, _)| name == "Show label" && owner == "plex/label"));
}

#[tokio::test]
async fn network_refresh_tolerates_unidentified_legacy_shows() {
    let pool = seeded_library().await;
    sqlx::query(
        r#"UPDATE "PlexShow" SET "Key" = NULL WHERE "Id" IN (
        SELECT m."Id" FROM "MediaItem" m JOIN "LibraryPath" p ON p."Id" = m."LibraryPathId"
        WHERE p."LibraryId" = 1)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let server = MockServer::start_async().await;
    network_list(&server).await;
    server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all");
            then.json_body(
                json!({"MediaContainer": {"totalSize": 1, "Metadata": [{"ratingKey":"200"}]}}),
            );
        })
        .await;
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".into(),
    };
    let before = tags(&pool).await;
    assert_eq!(
        sync_plex_networks(&pool, &PlexService::new(), &config, 1)
            .await
            .unwrap(),
        0
    );
    assert_eq!(tags(&pool).await, before);
}

async fn seed_network_playouts(pool: &SqlitePool) {
    use ersatzrs_core::repositories::ProgramScheduleRepository;
    use ersatzrs_domain::{CollectionType, ProgramScheduleItem};
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let foreign_episode: i64 = sqlx::query_scalar(
        r#"SELECT e."Id" FROM "Episode" e
        JOIN "MediaItem" m ON m."Id" = e."Id" JOIN "LibraryPath" p ON p."Id" = m."LibraryPathId"
        WHERE p."LibraryId" = 77 LIMIT 1"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query") VALUES (880, 'Network episodes', 'show_network:Network')"#).await.unwrap();
    for id in [881, 882] {
        common::seed_profile_and_channel(&repos, id).await;
        repos
            .program_schedule
            .upsert(&common::schedule(id, vec![id]))
            .await
            .unwrap();
        repos
            .program_schedule
            .upsert_item(&ProgramScheduleItem {
                id,
                program_schedule_id: id,
                collection_type: if id == 881 {
                    CollectionType::SmartCollection
                } else {
                    CollectionType::Episode
                },
                smart_collection_id: (id == 881).then_some(880),
                media_item_id: (id == 882).then_some(foreign_episode),
                ..Default::default()
            })
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
            VALUES (?, ?, ?, 1, 123)"#).bind(id).bind(id).bind(id).execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn network_refresh_persists_old_and_new_episode_dependencies_atomically() {
    use ersatzrs_infra::{provider_collection_refresh, scan_refresh};
    let pool = seeded_library().await;
    seed_network_playouts(&pool).await;
    let server = MockServer::start_async().await;
    network_list(&server).await;
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "synthetic".into(),
    };
    let members = server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all");
            then.json_body(
                json!({"MediaContainer": {"totalSize": 1, "Metadata": [{"ratingKey": "200"}]}}),
            );
        })
        .await;
    sync_plex_networks(&pool, &PlexService::new(), &config, 1)
        .await
        .unwrap();
    assert_eq!(
        provider_collection_refresh::resolve_pending(&pool)
            .await
            .unwrap(),
        1
    );
    let pending = scan_refresh::pending_refreshes(&pool).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].playout_id, 881);
    assert_eq!(
        pending[0].reason,
        ersatzrs_domain::ScanPlayoutRefreshReason::ProviderNetworkChanged
    );
    sync_plex_networks(&pool, &PlexService::new(), &config, 1)
        .await
        .unwrap();
    assert_eq!(
        provider_collection_refresh::resolve_pending(&pool)
            .await
            .unwrap(),
        0
    );
    members.delete_async().await;
    server
        .mock_async(|when, then| {
            when.path("/library/sections/2/all");
            then.json_body(json!({"MediaContainer": {"totalSize": 0, "Metadata": []}}));
        })
        .await;
    let before = tags(&pool).await;
    pool.execute(r#"CREATE TRIGGER fail_network_handoff BEFORE INSERT ON "ProviderCollectionRefreshResolution"
        BEGIN SELECT RAISE(ABORT, 'synthetic network handoff failure'); END"#).await.unwrap();
    assert!(sync_plex_networks(&pool, &PlexService::new(), &config, 1)
        .await
        .is_err());
    assert_eq!(tags(&pool).await, before);
    pool.execute("DROP TRIGGER fail_network_handoff")
        .await
        .unwrap();
    sync_plex_networks(&pool, &PlexService::new(), &config, 1)
        .await
        .unwrap();
    assert_eq!(matching_network_episodes(&pool).await, 0);
    assert_eq!(
        provider_collection_refresh::resolve_pending(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        scan_refresh::pending_refreshes(&pool).await.unwrap(),
        pending
    );
    let generation: i64 = sqlx::query_scalar(
        r#"SELECT "Generation" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 881"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        generation, 2,
        "removal must preserve the now-absent old search match"
    );
}
