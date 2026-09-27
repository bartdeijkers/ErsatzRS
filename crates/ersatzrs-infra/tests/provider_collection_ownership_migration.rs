use ersatzrs_infra::db::assert_sql_safe;
use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};

const VERSION: i64 = 202_609_271_330;
// Dynamic SQL identifiers below come exclusively from this fixed fixture list.
const PROVIDERS: [(&str, &str); 3] = [("Plex", "Key"), ("Jellyfin", "ItemId"), ("Emby", "ItemId")];

async fn legacy_database(url: &str) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
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

async fn seed_legacy(pool: &SqlitePool) {
    for (index, (provider, key)) in PROVIDERS.iter().enumerate() {
        let base = i64::try_from(index + 1).unwrap() * 100;
        for offset in 1..=3 {
            let id = base + offset;
            sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (?)"#)
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
            if offset != 3 {
                sqlx::query(assert_sql_safe(format!(
                    r#"INSERT INTO "{provider}MediaSource" ("Id") VALUES (?)"#
                )))
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
            }
            sqlx::query(
                r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind") VALUES (?, ?, 1)"#,
            )
            .bind(id)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId") VALUES (?, ?)"#)
                .bind(id)
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (?, ?)"#)
                .bind(id)
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
        }
        for (offset, external_id, members) in [
            (1, Some("shared-key"), vec![1, 1]),
            (2, Some("empty"), vec![]),
            (3, Some("mixed"), vec![1, 2]),
            (4, Some("duplicate"), vec![1]),
            (5, Some("duplicate"), vec![1]),
            (6, Some("shared-key"), vec![2]),
            (7, Some("wrong-provider"), vec![3]),
            (8, None, vec![1]),
            (9, Some(""), vec![1]),
        ] {
            let id = base + offset;
            sqlx::query(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (?, ?, 1)"#)
                .bind(id).bind(format!("Retained {id}")).execute(pool).await.unwrap();
            sqlx::query(assert_sql_safe(format!(r#"INSERT INTO "{provider}Collection" ("Id", "{key}", "Name") VALUES (?, ?, 'Retained')"#)))
                .bind(id).bind(external_id).execute(pool).await.unwrap();
            for member in members {
                sqlx::query(r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode") VALUES (?, ?, 'fra', 'nld')"#)
                    .bind(id).bind(base + member).execute(pool).await.unwrap();
            }
        }
    }
}

async fn owners(pool: &SqlitePool, provider: &str) -> Vec<(i64, Option<i64>)> {
    sqlx::query_as(assert_sql_safe(format!(
        r#"SELECT "Id", "MediaSourceId" FROM "{provider}Collection" ORDER BY "Id""#
    )))
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn seed_tags_before_ownership(pool: &SqlitePool) -> Vec<(i64, Option<String>)> {
    let mut expected = Vec::new();
    for (index, (provider, _)) in PROVIDERS.iter().enumerate() {
        let base = i64::try_from(index + 1).unwrap() * 100;
        for offset in 1..=2 {
            let id = base + offset;
            sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
                .bind(id)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query(r#"INSERT INTO "MovieMetadata" ("Id", "MovieId", "MetadataKind", "DateAdded", "DateUpdated") VALUES (?, ?, 1, '2026-09-27T00:00:00Z', '2026-09-27T00:00:00Z')"#)
                .bind(id).bind(id).execute(pool).await.unwrap();
            sqlx::query(r#"INSERT INTO "Tag" ("Id", "MovieMetadataId", "Name", "ExternalCollectionId") VALUES (?, ?, 'Retained', 'shared-key')"#)
                .bind(id).bind(id).execute(pool).await.unwrap();
            expected.push((id, Some(format!("{}/collection", provider.to_lowercase()))));
        }
        // Duplicate legacy collection candidates must not turn into owned tags.
        sqlx::query(r#"INSERT INTO "Tag" ("Id", "MovieMetadataId", "Name", "ExternalCollectionId") VALUES (?, ?, 'Ambiguous', 'duplicate')"#)
            .bind(base + 3).bind(base + 1).execute(pool).await.unwrap();
        expected.push((base + 3, None));
    }
    expected
}

#[tokio::test]
async fn provider_collection_ownership_migration_preserves_ambiguous_rows_and_restart_identity() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("upgrade.db").display()
    );
    let pool = legacy_database(&url).await;
    seed_legacy(&pool).await;
    let expected_tags = seed_tags_before_ownership(&pool).await;
    let before: Vec<(i64, i64, i64, String, String)> = sqlx::query_as(r#"SELECT "Id", "CollectionId", "MediaItemId", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode" FROM "CollectionItem" ORDER BY "Id""#)
        .fetch_all(&pool).await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    migrations.run(&pool).await.unwrap();
    let tags: Vec<(i64, Option<String>)> =
        sqlx::query_as(r#"SELECT "Id", "ExternalTypeId" FROM "Tag" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(tags, expected_tags);
    for (index, (provider, key)) in PROVIDERS.iter().enumerate() {
        let base = i64::try_from(index + 1).unwrap() * 100;
        let expected: Vec<_> = (1..=9)
            .map(|n| {
                (
                    base + n,
                    match n {
                        1 => Some(base + 1),
                        6 => Some(base + 2),
                        _ => None,
                    },
                )
            })
            .collect();
        assert_eq!(owners(&pool, provider).await, expected, "{provider}");
        let duplicate = sqlx::query(assert_sql_safe(format!(r#"INSERT INTO "{provider}Collection" ("Id", "{key}", "MediaSourceId") VALUES (?, 'shared-key', ?)"#)))
            .bind(base + 90).bind(base + 1).execute(&pool).await;
        assert!(duplicate
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .is_unique_violation());
        let wrong_provider = sqlx::query(assert_sql_safe(format!(
            r#"UPDATE "{provider}Collection" SET "MediaSourceId" = ? WHERE "Id" = ?"#
        )))
        .bind(base + 3)
        .bind(base + 2)
        .execute(&pool)
        .await;
        assert!(wrong_provider
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .is_foreign_key_violation());
    }
    let after = sqlx::query_as(r#"SELECT "Id", "CollectionId", "MediaItemId", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode" FROM "CollectionItem" ORDER BY "Id""#)
        .fetch_all(&pool).await.unwrap();
    assert_eq!(before, after);
    pool.close().await;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    assert_eq!(owners(&pool, "Plex").await[0], (101, Some(101)));
    // Source deletion releases ownership without deleting the collection identity.
    pool.execute(r#"DELETE FROM "MediaSource" WHERE "Id" = 101"#)
        .await
        .unwrap();
    assert_eq!(owners(&pool, "Plex").await[0], (101, None));
    let retained: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Collection" WHERE "Id" = 101 AND "Name" = 'Retained 101' AND "UseCustomPlaybackOrder" = 1"#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(retained, 1);
}

#[tokio::test]
async fn provider_collection_ownership_migration_rolls_back_failed_upgrade() {
    let pool = legacy_database("sqlite::memory:").await;
    seed_legacy(&pool).await;
    pool.execute(r#"CREATE TRIGGER "FailOwnershipUpgrade" BEFORE UPDATE ON "JellyfinCollection" BEGIN SELECT RAISE(ABORT, 'synthetic upgrade failure'); END"#).await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    assert!(migrations.run(&pool).await.is_err());
    for (provider, _) in PROVIDERS {
        let columns: Vec<String> = sqlx::query_scalar(assert_sql_safe(format!(
            r#"SELECT "name" FROM pragma_table_info('{provider}Collection')"#
        )))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(!columns.iter().any(|name| name == "MediaSourceId"));
    }
    pool.execute(r#"DROP TRIGGER "FailOwnershipUpgrade""#)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    assert_eq!(owners(&pool, "Jellyfin").await[0], (201, Some(201)));
}

#[tokio::test]
async fn provider_collection_ownership_migration_accepts_fresh_database() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    for (provider, _) in PROVIDERS {
        let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(format!("{provider}Collection"))
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(columns.iter().any(|name| name == "MediaSourceId"));
        assert!(owners(&pool, provider).await.is_empty());
    }
}
