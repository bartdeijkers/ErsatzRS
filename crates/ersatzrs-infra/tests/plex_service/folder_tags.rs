use super::*;

const VERSION: i64 = 202_609_300_600;

async fn roots<'a>(server: &'a MockServer, paths: &[&str]) -> httpmock::Mock<'a> {
    server.mock_async(|when, then| {
        when.method(GET).path("/library/sections").header("X-Plex-Token", "plex-token");
        then.status(200).json_body(json!({"MediaContainer": {"Directory": [
            {"Key": "99", "Title": "Unrelated", "type": "other", "Location": [{"path": "/wrong"}]},
            {"Key": "8", "Title": "Other Videos", "type": "other", "Location": paths.iter().map(|path| json!({"path": path})).collect::<Vec<_>>()}
        ]}}));
    }).await
}

async fn tags(pool: &SqlitePool) -> Vec<(i64, String, Option<String>, Option<String>)> {
    sqlx::query_as(
        r#"SELECT "Id", "Name", "ExternalTypeId", "ExternalCollectionId" FROM "Tag" ORDER BY "Id""#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn etag(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar(r#"SELECT "Etag" FROM "PlexOtherVideo" LIMIT 1"#)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "One persisted scan/migration/restart scenario verifies shared row identities."
)]
async fn folder_tags_refresh_after_migration_preserving_owners_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("folders.db").display()
    );
    let mut pool = SqlitePool::connect(&url).await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|m| m.version < VERSION)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    let server = MockServer::start_async().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".into(),
    };
    let mut locations = roots(&server, &[r"C:\media\other", r"C:\media\other\Travel"]).await;
    let mut item = media_json_with_subtitles(
        "/library/metadata/800",
        "/library/parts/80",
        r"C:\media\other\Travel\clip.mkv",
        800,
    );
    item["Label"] = json!([{ "id": 5, "tag": "Travel" }]);
    let page = server
        .mock_async(|when, then| {
            when.method(GET).path("/library/sections/8/all");
            then.status(200)
                .json_body(json!({"MediaContainer": {"totalSize": 1, "Metadata": [item]}}));
        })
        .await;
    sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", false)
        .await
        .unwrap();
    let item_id: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "PlexOtherVideo""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    let subtitles = ersatzrs_infra::playback::load_subtitles_for_media_item(&pool, item_id)
        .await
        .unwrap();
    assert_eq!(subtitles.len(), 2);
    for kind in [
        "plex/collection",
        "319",
        "nfo/country",
        "jellyfin/collection",
    ] {
        sqlx::query(r#"INSERT INTO "Tag" ("Name", "ExternalTypeId", "ExternalCollectionId", "OtherVideoMetadataId") SELECT 'Travel', ?, '5', "Id" FROM "OtherVideoMetadata""#)
            .bind(kind).execute(&pool).await.unwrap();
    }
    // Simulate an installation that has cached the item without folder tags.
    sqlx::query(r#"DELETE FROM "Tag" WHERE "ExternalTypeId" = 'plex/folder'"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(r#"
        UPDATE "Library" SET "MediaKind"=4, "LastScan"='2026-09-29 10:00:00' WHERE "Id"=8;
        INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","LastScan") VALUES (9,1,1,'2026-09-29 10:00:00'), (10,4,1,'2026-09-29 10:00:00');
        INSERT INTO "PlexLibrary" ("Id","Key","ShouldSyncItems") VALUES (9,'9',1);
    "#).execute(&pool).await.unwrap();
    let protected = tags(&pool).await;
    migrations.run(&pool).await.unwrap();
    assert!(ersatzrs_infra::db::inspect_database_schema(
        &pool,
        ersatzrs_infra::db::DatabaseBackend::Sqlite
    )
    .await
    .unwrap()
    .is_ready());
    assert_eq!(etag(&pool).await, None);
    assert_eq!(tags(&pool).await, protected, "migration preserves all tags");
    let scans: Vec<(i64, String)> =
        sqlx::query_as(r#"SELECT "Id", "LastScan" FROM "Library" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        scans,
        [
            (8, "1970-01-01 00:00:00".into()),
            (9, "2026-09-29 10:00:00".into()),
            (10, "2026-09-29 10:00:00".into())
        ]
    );
    sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", false)
        .await
        .unwrap();
    let restored = tags(&pool).await;
    assert_eq!(
        restored
            .iter()
            .filter(|row| row.2.as_deref() != Some("plex/folder"))
            .cloned()
            .collect::<Vec<_>>(),
        protected
    );
    sqlx::query(r#"INSERT INTO "Tag" ("Name", "ExternalTypeId", "OtherVideoMetadataId") SELECT "Name", "ExternalTypeId", "OtherVideoMetadataId" FROM "Tag" WHERE "ExternalTypeId"='plex/folder' AND "Name"='Travel'"#).execute(&pool).await.unwrap();
    sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", true)
        .await
        .unwrap();
    assert_eq!(
        tags(&pool).await,
        restored,
        "duplicate derived tags converge without changing the retained identity"
    );

    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let (_, matches, _) = repositories
        .scheduling
        .search_documents_page("tag:other", 0, 0)
        .await
        .unwrap();
    assert_eq!(
        matches, 1,
        "folder tags are visible to media/scheduling searches"
    );
    drop(repositories);

    assert_eq!(
        restored
            .iter()
            .filter(|row| row.2.as_deref() == Some("plex/folder"))
            .map(|row| row.1.as_str())
            .collect::<Vec<_>>(),
        ["Travel", "other"]
    );
    for deep in [false, true] {
        sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", deep)
            .await
            .unwrap();
        assert_eq!(tags(&pool).await, restored);
    }
    let saved_etag = etag(&pool).await;
    pool.close().await;
    pool = SqlitePool::connect(&url).await.unwrap();
    migrations.run(&pool).await.unwrap();
    assert_eq!(
        etag(&pool).await,
        saved_etag,
        "restart does not repeat migration"
    );
    sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", true)
        .await
        .unwrap();
    assert_eq!(tags(&pool).await, restored);
    assert_eq!(
        ersatzrs_infra::playback::load_subtitles_for_media_item(&pool, item_id)
            .await
            .unwrap(),
        subtitles
    );
    assert_eq!(count(&pool, "MediaStream").await, 4);
    // A changed library root must update tags even if the item timestamp is unchanged.
    locations.delete_async().await;
    locations = roots(&server, &[r"C:\media\other\Travel"]).await;
    sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", false)
        .await
        .unwrap();
    let expected: Vec<_> = restored
        .into_iter()
        .filter(|row| !(row.1 == "other" && row.2.as_deref() == Some("plex/folder")))
        .collect();
    assert_eq!(tags(&pool).await, expected);
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let (_, matches, _) = repositories
        .scheduling
        .search_documents_page("tag:other", 0, 0)
        .await
        .unwrap();
    assert_eq!(matches, 0, "removed folder tags stop matching searches");
    drop(repositories);
    let before_etag = etag(&pool).await;
    locations.delete_async().await;
    let failure = server
        .mock_async(|when, then| {
            when.method(GET).path("/library/sections");
            then.status(503);
        })
        .await;
    assert!(
        sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", false)
            .await
            .is_err()
    );
    assert_eq!(tags(&pool).await, expected);
    assert_eq!(etag(&pool).await, before_etag);
    failure.assert_calls_async(1).await;
    assert!(page.calls_async().await >= 5);
}

#[tokio::test]
async fn folder_tag_write_failure_rolls_back_item_and_tag_changes() {
    let pool = common::test_db().await;
    let server = MockServer::start_async().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".into(),
    };
    let locations = roots(&server, &["/media/other"]).await;
    for (phase, folder) in ["Travel", "Summer"].into_iter().enumerate() {
        let mut item = media_json(
            "/library/metadata/800",
            "/library/parts/80",
            &format!("/media/other/{folder}/clip.mkv"),
            800,
        );
        item["title"] = json!(folder);
        let page = server
            .mock_async(|when, then| {
                when.method(GET).path("/library/sections/8/all");
                then.status(200)
                    .json_body(json!({"MediaContainer": {"totalSize": 1, "Metadata": [item]}}));
            })
            .await;
        if phase == 0 {
            sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", false)
                .await
                .unwrap();
            sqlx::raw_sql(r#"CREATE TRIGGER "FailFolderTag" BEFORE INSERT ON "Tag" WHEN NEW."Name"='Summer' BEGIN SELECT RAISE(ABORT, 'synthetic folder write failure'); END;"#).execute(&pool).await.unwrap();
        } else {
            let before = tags(&pool).await;
            let old_etag = etag(&pool).await;
            assert!(
                sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", false)
                    .await
                    .is_err()
            );
            assert_eq!(tags(&pool).await, before);
            assert_eq!(etag(&pool).await, old_etag);
            let title: String = sqlx::query_scalar(r#"SELECT "Title" FROM "OtherVideoMetadata""#)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(title, "Travel");
        }
        page.delete_async().await;
    }
    locations.assert_calls_async(2).await;
}

#[tokio::test]
async fn folder_locations_are_reused_across_all_pages() {
    let pool = common::test_db().await;
    let server = MockServer::start_async().await;
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".into(),
    };
    let locations = roots(&server, &["/media/other"]).await;
    // The first page also contains metadata without playable media. Projection
    // skips those entries, but pagination must still reach the next 100 offset.
    let mut first = vec![json!({"ratingKey": "unsupported"}); 99];
    first.push(media_json(
        "/library/metadata/1",
        "/library/parts/1",
        "/media/other/Travel/first.mkv",
        1,
    ));
    let first_page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/8/all")
                .query_param("X-Plex-Container-Start", "0");
            then.status(200)
                .json_body(json!({"MediaContainer": {"totalSize": 101, "Metadata": first}}));
        })
        .await;
    let last_page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/8/all")
                .query_param("X-Plex-Container-Start", "100");
            then.status(200).json_body(json!({"MediaContainer": {"totalSize": 101, "Metadata": [
            media_json("/library/metadata/2", "/library/parts/2", "/media/other/Summer/last.mkv", 2)
        ]}}));
        })
        .await;
    assert_eq!(
        sync_plex_other_video_library(&pool, &PlexService::new(), &config, 1, 8, "8", false)
            .await
            .unwrap(),
        2
    );
    let derived: Vec<(String, String)> = sqlx::query_as(r#"SELECT p."Key", t."Name" FROM "PlexOtherVideo" p JOIN "OtherVideoMetadata" m ON m."OtherVideoId"=p."Id" JOIN "Tag" t ON t."OtherVideoMetadataId"=m."Id" WHERE t."ExternalTypeId"='plex/folder' ORDER BY p."Key", t."Name""#).fetch_all(&pool).await.unwrap();
    assert_eq!(
        derived,
        [
            ("/library/metadata/1".into(), "Travel".into()),
            ("/library/metadata/1".into(), "other".into()),
            ("/library/metadata/2".into(), "Summer".into()),
            ("/library/metadata/2".into(), "other".into()),
        ]
    );
    locations.assert_calls_async(1).await;
    first_page.assert_calls_async(1).await;
    last_page.assert_calls_async(1).await;
}
