#![cfg(not(feature = "mysql"))]

use sqlx::{sqlite::SqlitePoolOptions, Executor, Row};

/// `MySQL` rejects any identifier longer than 64 characters, and a migration
/// that names one aborts, taking the whole `MySQL` backend down from that point
/// on. `SQLite` has no such limit, so an over-long name is invisible until the
/// `MySQL` job runs. Check every `MySQL` migration, not one of them: an earlier
/// version of this test pinned a single file, and the next migration to exceed
/// the limit went straight to CI.
#[test]
fn mysql_migration_identifiers_fit_the_identifier_limit() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations/mysql")
        .canonicalize()
        .expect("locate the MySQL migration directory");

    let mut files: Vec<_> = std::fs::read_dir(&directory)
        .expect("read the MySQL migration directory")
        .map(|entry| entry.expect("read a MySQL migration entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sql"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no MySQL migrations found in {}",
        directory.display()
    );

    for path in files {
        let sql = std::fs::read_to_string(&path).expect("read a MySQL migration");
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_owned();
        for identifier in mysql_declared_identifiers(&sql) {
            assert!(
                identifier.len() <= 64,
                "MySQL identifier in {name} exceeds 64 characters ({} chars): {identifier}. \
                 Truncate it the way the rest of the MySQL schema does, to 63 characters \
                 plus `~`, and leave the SQLite spelling untouched.",
                identifier.len()
            );
        }
    }
}

/// Collect the backtick-quoted names a `MySQL` migration declares: tables,
/// constraints (inline and `ADD CONSTRAINT`), and indexes.
fn mysql_declared_identifiers(sql: &str) -> Vec<String> {
    const KEYWORDS: [&str; 5] = [
        "CONSTRAINT ",
        "CREATE TABLE ",
        "CREATE INDEX ",
        "CREATE UNIQUE INDEX ",
        "ADD INDEX ",
    ];
    let upper = sql.to_uppercase();
    let mut identifiers = Vec::new();
    for keyword in KEYWORDS {
        let mut from = 0;
        while let Some(offset) = upper[from..].find(keyword) {
            let after = from + offset + keyword.len();
            from = after;
            // Only a backtick-quoted name is being declared here; anything else
            // (`CREATE TABLE IF NOT EXISTS`, a bare word) is skipped.
            let rest = &sql[after..];
            let Some(rest) = rest.strip_prefix('`') else {
                continue;
            };
            if let Some(end) = rest.find('`') {
                identifiers.push(rest[..end].to_owned());
            }
        }
    }
    identifiers
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
