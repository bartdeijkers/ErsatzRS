#![cfg(not(feature = "mysql"))]

use sqlx::{sqlite::SqlitePoolOptions, Executor, Row};

#[test]
fn mysql_migration_constraint_names_fit_the_identifier_limit() {
    for name in include_str!("../../../migrations/mysql/202608131200_addon_media_lists.sql")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("CONSTRAINT `"))
        .filter_map(|line| line.split('`').next())
    {
        assert!(
            name.len() <= 64,
            "MySQL constraint name exceeds 64 characters: {name}"
        );
    }
}

#[tokio::test]
async fn destructive_migration_removes_legacy_trakt_schema_and_data() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    sqlx::raw_sql(
        r#"
        CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT);
        CREATE TABLE "AddonInstallation" ("AddonId" TEXT PRIMARY KEY);
        CREATE TABLE "MediaItem" ("Id" INTEGER PRIMARY KEY);
        CREATE TABLE "Playlist" ("Id" INTEGER PRIMARY KEY, "IsSystem" INTEGER NOT NULL);
        CREATE TABLE "TraktList" (
            "Id" INTEGER PRIMARY KEY,
            "PlaylistId" INTEGER NULL,
            FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE SET NULL
        );
        CREATE TABLE "TraktListItem" (
            "Id" INTEGER PRIMARY KEY,
            "TraktListId" INTEGER NOT NULL,
            FOREIGN KEY ("TraktListId") REFERENCES "TraktList" ("Id") ON DELETE CASCADE
        );
        CREATE TABLE "TraktListItemGuid" (
            "Id" INTEGER PRIMARY KEY,
            "TraktListItemId" INTEGER NOT NULL,
            FOREIGN KEY ("TraktListItemId") REFERENCES "TraktListItem" ("Id") ON DELETE CASCADE
        );
        INSERT INTO "Playlist" ("Id", "IsSystem") VALUES (9, 1);
        INSERT INTO "TraktList" ("Id", "PlaylistId") VALUES (1, 9);
        INSERT INTO "TraktListItem" ("Id", "TraktListId") VALUES (2, 1);
        INSERT INTO "TraktListItemGuid" ("Id", "TraktListItemId") VALUES (3, 2);
        INSERT INTO "ConfigElement" ("Key", "Value")
            VALUES ('pages.trakt.lists_page_size', '25');
        INSERT INTO "ConfigElement" ("Key", "Value")
            VALUES ('pages.addon_media_lists.page_size', '50');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!(
        "../../../migrations/202608131200_addon_media_lists.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let table_names =
        sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect::<Vec<_>>();
    assert!(!table_names.iter().any(|name| name.starts_with("Trakt")));
    assert!(table_names.iter().any(|name| name == "AddonMediaList"));
    assert!(table_names.iter().any(|name| name == "AddonMediaListItem"));
    assert!(table_names
        .iter()
        .any(|name| name == "AddonMediaListItemGuid"));

    let playlist_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"Playlist\"")
        .fetch_one(&pool)
        .await
        .unwrap();
    let legacy_config_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM \"ConfigElement\" WHERE \"Key\" LIKE '%trakt%' OR \"Key\" = 'pages.addon_media_lists.page_size'",
    )
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((playlist_count, legacy_config_count), (0, 0));
}
