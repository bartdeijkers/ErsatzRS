use sqlx::{sqlite::SqlitePoolOptions, Executor};

#[tokio::test]
async fn playout_history_seed_migration_preserves_legacy_rows() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_609_271_200)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    pool.execute("PRAGMA foreign_keys = OFF").await.unwrap();
    pool.execute(r#"INSERT INTO "PlayoutHistory" ("Id", "PlayoutId", "Index", "PlaybackOrder", "When", "Details")
        VALUES (1, 1, 7, 0, '2026-09-27T00:00:00Z', '{}')"#).await.unwrap();
    migrations.run(&pool).await.unwrap();
    let row: (i32, Option<i32>, String) =
        sqlx::query_as(r#"SELECT "Index", "Seed", "Details" FROM "PlayoutHistory" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, (7, None, "{}".to_owned()));
    pool.execute(r#"UPDATE "PlayoutHistory" SET "Seed" = 12345 WHERE "Id" = 1"#)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    let seed: Option<i32> =
        sqlx::query_scalar(r#"SELECT "Seed" FROM "PlayoutHistory" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(seed, Some(12345));
}
