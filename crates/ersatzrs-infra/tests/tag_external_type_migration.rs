use ersatzrs_infra::db::assert_sql_safe;
use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};

const VERSION: i64 = 202_609_271_400;
// Fixed fixture identifiers; parents precede their dependent media kinds.
const KINDS: [&str; 10] = [
    "Show",
    "Season",
    "Episode",
    "Artist",
    "MusicVideo",
    "Movie",
    "OtherVideo",
    "Image",
    "Song",
    "RemoteStream",
];

async fn previous_database(url: &str) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .unwrap();
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
    pool
}

async fn seed_source(pool: &SqlitePool, source: i64, provider: &str) {
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
    // Provider is chosen only by the fixed test cases below.
    sqlx::query(assert_sql_safe(format!(
        r#"INSERT INTO "{provider}MediaSource" ("Id") VALUES (?)"#
    )))
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind") VALUES (?, ?, 1)"#)
        .bind(source)
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId") VALUES (?, ?)"#)
        .bind(source)
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
    for (index, kind) in KINDS.iter().enumerate() {
        let id = source * 100 + i64::try_from(index).unwrap();
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (?, ?)"#)
            .bind(id)
            .bind(source)
            .execute(pool)
            .await
            .unwrap();
        let (extra, parent) = match *kind {
            "Season" => (", \"ShowId\"", Some(source * 100)),
            "Episode" => (", \"SeasonId\"", Some(source * 100 + 1)),
            "MusicVideo" => (", \"ArtistId\"", Some(source * 100 + 3)),
            _ => ("", None),
        };
        let placeholders = if parent.is_some() { "?, ?" } else { "?" };
        let mut query = sqlx::query(assert_sql_safe(format!(
            r#"INSERT INTO "{kind}" ("Id"{extra}) VALUES ({placeholders})"#
        )))
        .bind(id);
        if let Some(parent) = parent {
            query = query.bind(parent);
        }
        query.execute(pool).await.unwrap();
        sqlx::query(assert_sql_safe(format!(r#"INSERT INTO "{kind}Metadata" ("Id", "{kind}Id", "MetadataKind", "DateAdded", "DateUpdated", "Title") VALUES (?, ?, 1, '2026-09-27T00:00:00Z', '2026-09-27T00:00:00Z', 'Fixture')"#)))
            .bind(id).bind(id).execute(pool).await.unwrap();
    }
}

async fn seed_collection(pool: &SqlitePool, provider: &str, source: Option<i64>, key: &str) {
    let id: i64 = sqlx::query_scalar(
        r#"INSERT INTO "Collection" ("UseCustomPlaybackOrder") VALUES (0) RETURNING "Id""#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let column = if provider == "Plex" { "Key" } else { "ItemId" };
    sqlx::query(assert_sql_safe(format!(r#"INSERT INTO "{provider}Collection" ("Id", "MediaSourceId", "{column}", "Name") VALUES (?, ?, ?, 'Collection')"#)))
        .bind(id).bind(source).bind(key).execute(pool).await.unwrap();
}

async fn seed_tag(
    pool: &SqlitePool,
    kind: &str,
    metadata: i64,
    external: Option<&str>,
    tag_type: Option<&str>,
) -> i64 {
    sqlx::query_scalar(assert_sql_safe(format!(r#"INSERT INTO "Tag" ("{kind}MetadataId", "Name", "ExternalCollectionId", "ExternalTypeId") VALUES (?, 'Retained name', ?, ?) RETURNING "Id""#)))
        .bind(metadata).bind(external).bind(tag_type).fetch_one(pool).await.unwrap()
}

async fn tag_values(pool: &SqlitePool) -> Vec<(i64, Option<String>)> {
    sqlx::query_as(r#"SELECT "Id", "ExternalTypeId" FROM "Tag" ORDER BY "Id""#)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn retained_columns(pool: &SqlitePool) -> Vec<String> {
    let owners = KINDS
        .iter()
        .map(|kind| format!("'{kind}', \"{kind}MetadataId\""))
        .collect::<Vec<_>>()
        .join(", ");
    sqlx::query_scalar(assert_sql_safe(format!(r#"SELECT json_object('Id', "Id", 'Name', "Name", 'ExternalCollectionId', "ExternalCollectionId", {owners}) FROM "Tag" ORDER BY "Id""#)))
        .fetch_all(pool).await.unwrap()
}

#[tokio::test]
async fn tag_external_type_migration_scopes_all_metadata_owners_and_preserves_restart_rows() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("tags.db").display()
    );
    let pool = previous_database(&url).await;
    let mut expected = Vec::new();
    for (source, provider, collection_type) in [
        (1, "Plex", Some("plex/collection")),
        (2, "Plex", Some("plex/collection")),
        (3, "Emby", Some("emby/collection")),
        (4, "Jellyfin", Some("jellyfin/collection")),
        (5, "Local", None),
    ] {
        seed_source(&pool, source, provider).await;
        if provider != "Local" {
            seed_collection(&pool, provider, Some(source), "shared").await;
        }
        for (index, kind) in KINDS.iter().enumerate() {
            let metadata = source * 100 + i64::try_from(index).unwrap();
            for (external, before, after) in [
                (Some("shared"), None, collection_type),
                (
                    Some("label-only"),
                    None,
                    (provider == "Plex").then_some("plex/label"),
                ),
                (None, None, None),
                (
                    Some("shared"),
                    Some("third-party/owner"),
                    Some("third-party/owner"),
                ),
                (None, Some("319"), Some("319")),
                (None, Some("nfo/country"), Some("nfo/country")),
            ] {
                let id = seed_tag(&pool, kind, metadata, external, before).await;
                expected.push((id, after.map(str::to_owned)));
            }
        }
    }
    seed_collection(&pool, "Plex", None, "ambiguous").await;
    seed_collection(&pool, "Plex", Some(2), "foreign-key").await;
    for (external, after) in [
        ("ambiguous", None),
        ("foreign-key", Some("plex/label")),
        ("", None),
    ] {
        let id = seed_tag(&pool, "Movie", 105, Some(external), None).await;
        expected.push((id, after.map(str::to_owned)));
    }
    let orphan: i64 = sqlx::query_scalar(r#"INSERT INTO "Tag" ("Name", "ExternalCollectionId") VALUES ('Orphan', 'shared') RETURNING "Id""#)
        .fetch_one(&pool).await.unwrap();
    expected.push((orphan, None));
    let multiple = seed_tag(&pool, "Movie", 105, Some("shared"), None).await;
    sqlx::query(r#"UPDATE "Tag" SET "ShowMetadataId" = 400 WHERE "Id" = ?"#)
        .bind(multiple)
        .execute(&pool)
        .await
        .unwrap();
    expected.push((multiple, None));
    let before = retained_columns(&pool).await;
    let migrations = sqlx::migrate!("../../migrations");
    migrations.run(&pool).await.unwrap();
    assert_eq!(tag_values(&pool).await, expected);
    assert_eq!(retained_columns(&pool).await, before);
    pool.close().await;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    assert_eq!(tag_values(&pool).await, expected);
    assert_eq!(retained_columns(&pool).await, before);
}

#[tokio::test]
async fn tag_external_type_migration_rolls_back_failed_update() {
    let pool = previous_database("sqlite::memory:").await;
    seed_source(&pool, 1, "Plex").await;
    let id = seed_tag(&pool, "Movie", 105, Some("label"), None).await;
    let second = seed_tag(&pool, "Movie", 105, Some("failure-label"), None).await;
    pool.execute(r#"CREATE TRIGGER "FailTagUpgrade" BEFORE UPDATE ON "Tag" WHEN OLD."ExternalCollectionId" = 'failure-label' BEGIN SELECT RAISE(ABORT, 'synthetic migration failure'); END"#).await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    assert!(migrations.run(&pool).await.is_err());
    assert_eq!(tag_values(&pool).await, vec![(id, None), (second, None)]);
    pool.execute(r#"DROP TRIGGER "FailTagUpgrade""#)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    assert_eq!(
        tag_values(&pool).await,
        vec![
            (id, Some("plex/label".into())),
            (second, Some("plex/label".into()))
        ]
    );
}

#[tokio::test]
async fn tag_external_type_migration_does_not_guess_ambiguous_provider() {
    let pool = previous_database("sqlite::memory:").await;
    let mut expected = Vec::new();
    for (source, other) in [(6, "Jellyfin"), (7, "Local")] {
        seed_source(&pool, source, "Plex").await;
        sqlx::query(assert_sql_safe(format!(
            r#"INSERT INTO "{other}MediaSource" ("Id") VALUES (?)"#
        )))
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
        let id = seed_tag(&pool, "Movie", source * 100 + 5, Some("label"), None).await;
        expected.push((id, None));
    }
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    assert_eq!(tag_values(&pool).await, expected);
}
