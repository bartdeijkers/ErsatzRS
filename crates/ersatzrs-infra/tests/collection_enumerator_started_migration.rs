use sqlx::{sqlite::SqlitePoolOptions, Executor};

#[tokio::test]
async fn existing_enumerator_states_upgrade_with_not_started_default() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_609_251_430)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();

    // These standalone legacy rows exercise all three old state tables. The
    // owning rows are irrelevant to the additive column migration.
    pool.execute("PRAGMA foreign_keys = OFF").await.unwrap();
    pool.execute(
        r#"INSERT INTO "ScheduleItemsEnumeratorState" ("PlayoutAnchorPlayoutId", "Seed", "Index")
           VALUES (101, 5, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "FillGroupEnumeratorState" ("PlayoutScheduleItemFillGroupIndexId", "Seed", "Index")
           VALUES (102, 6, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "CollectionEnumeratorState" ("PlayoutProgramScheduleAnchorId", "Seed", "Index")
           VALUES (103, 7, 0)"#,
    )
    .await
    .unwrap();

    migrations.run(&pool).await.unwrap();
    for (table, query) in [
        (
            "ScheduleItemsEnumeratorState",
            r#"SELECT "Started" FROM "ScheduleItemsEnumeratorState""#,
        ),
        (
            "FillGroupEnumeratorState",
            r#"SELECT "Started" FROM "FillGroupEnumeratorState""#,
        ),
        (
            "CollectionEnumeratorState",
            r#"SELECT "Started" FROM "CollectionEnumeratorState""#,
        ),
    ] {
        let started: bool = sqlx::query_scalar(query).fetch_one(&pool).await.unwrap();
        assert!(!started, "legacy {table} state should start false");
    }
    migrations.run(&pool).await.unwrap();
}
