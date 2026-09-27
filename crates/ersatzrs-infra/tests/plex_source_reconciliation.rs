mod common;

use ersatzrs_core::repositories::{LibraryRepository, MediaSourceRepository};
use ersatzrs_infra::plex::{PlexServerResource, PlexService};
use sqlx::SqlitePool;

fn server(identifier: &str, name: &str) -> PlexServerResource {
    PlexServerResource {
        name: name.to_owned(),
        client_identifier: identifier.to_owned(),
        server_auth_token: "synthetic-server-token".to_owned(),
        uris: vec!["http://plex.example.test:32400".to_owned()],
    }
}

async fn missing_since(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar(r#"SELECT "MissingSince" FROM "PlexMediaSource" WHERE "Id" = 1"#)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn library_refresh_uses_current_server_credentials_and_rejects_missing_servers() {
    use httpmock::prelude::*;
    let endpoint = MockServer::start_async().await;
    let request = endpoint
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections")
                .header("X-Plex-Token", "synthetic-server-token");
            then.status(200)
                .json_body(serde_json::json!({"MediaContainer":{"Directory":[{
                    "Key":"8", "Title":"Refreshed library", "type":"movie"
                }]}}));
        })
        .await;
    let pool = common::test_db().await;
    let mut resource = server("first", "First");
    resource.uris = vec![endpoint.base_url()];
    PlexService::upsert_sources(&pool, &[resource])
        .await
        .unwrap();
    ersatzrs_infra::plex::refresh_plex_libraries(&pool, 1)
        .await
        .unwrap();
    request.assert_calls_async(1).await;
    let name: String =
        sqlx::query_scalar(r#"SELECT "Name" FROM "Library" WHERE "MediaSourceId" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(name, "Refreshed library");
    PlexService::upsert_sources(&pool, &[]).await.unwrap();
    assert!(ersatzrs_infra::plex::refresh_plex_libraries(&pool, 1)
        .await
        .is_err());
    request.assert_calls_async(1).await;
}

#[tokio::test]
async fn explicit_missing_server_removal_cascades_content_and_search_but_preserves_other_sources() {
    let pool = common::test_db().await;
    let first = server("first", "First");
    let second = server("second", "Second");
    PlexService::upsert_sources(&pool, &[first.clone(), second.clone()])
        .await
        .unwrap();
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    assert!(repos
        .media_source
        .delete_missing_plex_source_admin(1)
        .await
        .is_err());
    sqlx::raw_sql(r#"
        INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name") VALUES (1, 1, 1, 'Synthetic library');
        INSERT INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems") VALUES (1, '1', 1);
        INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, 'plex://synthetic');
        INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (1, 1, 0);
        INSERT INTO "Movie" ("Id") VALUES (1);
        INSERT INTO "MovieMetadata" ("Id", "MovieId", "MetadataKind", "Title", "DateAdded", "DateUpdated")
        VALUES (1, 1, 0, 'Synthetic Removal Movie', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
    "#).execute(&pool).await.unwrap();
    let (_, before, _) = repos
        .scheduling
        .search_documents_page("", 0, 100)
        .await
        .unwrap();
    assert_eq!(before, 1);
    PlexService::upsert_sources(&pool, std::slice::from_ref(&second))
        .await
        .unwrap();
    // A returned source invalidates an already-open removal confirmation.
    PlexService::upsert_sources(&pool, &[first, second.clone()])
        .await
        .unwrap();
    assert!(repos
        .media_source
        .delete_missing_plex_source_admin(1)
        .await
        .is_err());
    PlexService::upsert_sources(&pool, &[second]).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "ManagedLibraryOperation"
        ("LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt")
        VALUES (1, 'synthetic-owner', 0, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(repos
        .media_source
        .delete_missing_plex_source_admin(1)
        .await
        .is_err());
    sqlx::query(r#"UPDATE "ManagedLibraryOperation" SET "State" = 3 WHERE "LibraryId" = 1"#)
        .execute(&pool)
        .await
        .unwrap();
    repos
        .media_source
        .delete_missing_plex_source_admin(1)
        .await
        .unwrap();
    let (_, after, _) = repos
        .scheduling
        .search_documents_page("", 0, 100)
        .await
        .unwrap();
    assert_eq!(after, 0);
    let remaining: (i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT COUNT(*) FROM "Library"), (SELECT COUNT(*) FROM "LibraryPath"),
           (SELECT COUNT(*) FROM "MediaItem"), (SELECT COUNT(*) FROM "MovieMetadata")"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, (0, 0, 0, 0));
    assert_eq!(
        repos
            .media_source
            .get_plex(2)
            .await
            .unwrap()
            .server_name
            .as_deref(),
        Some("Second")
    );
    let first_token: Option<String> = sqlx::query_scalar(
        r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = 'plex.server_auth_token.first'"#,
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(first_token.is_none());
}

#[tokio::test]
async fn plex_missing_since_migration_preserves_existing_servers_and_recorded_absence() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_609_270_800)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1);
           INSERT INTO "PlexMediaSource" ("Id", "ClientIdentifier", "ServerName")
           VALUES (1, 'synthetic-existing', 'Existing Plex');"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    migrations.run(&pool).await.unwrap();
    let row: (String, String, Option<String>) = sqlx::query_as(
        r#"SELECT "ClientIdentifier", "ServerName", "MissingSince" FROM "PlexMediaSource" WHERE "Id" = 1"#,
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(
        row,
        (
            "synthetic-existing".to_owned(),
            "Existing Plex".to_owned(),
            None
        )
    );
    sqlx::query(
        r#"UPDATE "PlexMediaSource" SET "MissingSince" = '2026-01-02T03:04:05Z' WHERE "Id" = 1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    migrations.run(&pool).await.unwrap();
    assert_eq!(
        missing_since(&pool).await.as_deref(),
        Some("2026-01-02T03:04:05Z")
    );
}

#[tokio::test]
async fn plex_inventory_absence_and_return_preserve_content_and_first_missing_time() {
    let pool = common::test_db().await;
    let resource = server("synthetic-client", "Synthetic Plex");
    PlexService::upsert_sources(&pool, std::slice::from_ref(&resource))
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
           VALUES (1, 1, 1, 'Synthetic library');
           INSERT INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems") VALUES (1, '1', 1);
           INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, 'plex://synthetic');
           INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (1, 1, 0);
           INSERT INTO "PlexPathReplacement" ("Id", "PlexPath", "LocalPath", "PlexMediaSourceId")
           VALUES (1, '/synthetic/remote', '/synthetic/local', 1);"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(missing_since(&pool).await.is_none());

    PlexService::upsert_sources(&pool, &[]).await.unwrap();
    assert!(
        missing_since(&pool).await.is_some(),
        "a successful empty inventory must mark the retained server"
    );
    sqlx::query(
        r#"UPDATE "PlexMediaSource" SET "MissingSince" = '2026-01-02T03:04:05Z' WHERE "Id" = 1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    PlexService::upsert_sources(&pool, &[]).await.unwrap();
    assert_eq!(
        missing_since(&pool).await.as_deref(),
        Some("2026-01-02T03:04:05Z")
    );

    // A missing source must not contact its old address or refresh retained libraries.
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    assert!(
        !repositories
            .library
            .get_scan_target(1)
            .await
            .unwrap()
            .should_sync_items
    );
    assert_eq!(
        ersatzrs_infra::plex::sync_library_by_id(&pool, 1)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ersatzrs_infra::plex::sync_collections_for_media_source(&pool, 1, false)
            .await
            .unwrap(),
        0
    );
    ersatzrs_infra::plex::sync_plex_libraries(&pool, 1, &[])
        .await
        .unwrap();
    let retained_library: String =
        sqlx::query_scalar(r#"SELECT "Name" FROM "Library" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(retained_library, "Synthetic library");

    PlexService::upsert_sources(&pool, &[resource])
        .await
        .unwrap();
    assert!(missing_since(&pool).await.is_none());
    assert!(
        repositories
            .library
            .get_scan_target(1)
            .await
            .unwrap()
            .should_sync_items
    );
    let retained: (i64, i64, i64, String) = sqlx::query_as(
        r#"SELECT source."Id", library."Id", item."Id", replacement."LocalPath"
           FROM "PlexMediaSource" source
           JOIN "Library" library ON library."MediaSourceId" = source."Id"
           JOIN "LibraryPath" path ON path."LibraryId" = library."Id"
           JOIN "MediaItem" item ON item."LibraryPathId" = path."Id"
           JOIN "PlexPathReplacement" replacement ON replacement."PlexMediaSourceId" = source."Id""#,
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(retained, (1, 1, 1, "/synthetic/local".to_owned()));
}

#[tokio::test]
async fn plex_inventory_write_failure_rolls_back_source_and_missing_state() {
    let pool = common::test_db().await;
    PlexService::upsert_sources(&pool, &[server("first", "Original")])
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"CREATE TRIGGER "RejectSyntheticPlexSource" BEFORE INSERT ON "PlexMediaSource"
           WHEN NEW."ClientIdentifier" = 'rejected'
           BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END;"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(PlexService::upsert_sources(
        &pool,
        &[server("first", "Changed"), server("rejected", "Rejected")]
    )
    .await
    .is_err());
    let name: String =
        sqlx::query_scalar(r#"SELECT "ServerName" FROM "PlexMediaSource" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(name, "Original");
    assert!(missing_since(&pool).await.is_none());
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaSource""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
