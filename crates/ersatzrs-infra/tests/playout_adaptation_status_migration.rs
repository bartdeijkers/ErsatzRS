use sqlx::{sqlite::SqlitePoolOptions, Executor};

type BuildStatusRow = (i64, String, bool, Option<String>, Option<String>);

#[tokio::test]
async fn adaptation_status_migration_preserves_known_success_and_unknown_history() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_610_022_200)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    pool.execute("PRAGMA foreign_keys = OFF").await.unwrap();
    pool.execute(
        r#"INSERT INTO "PlayoutBuildStatus" ("PlayoutId", "LastBuild", "Success", "Message")
        VALUES (1, '2026-10-02 12:00:00', 1, NULL), (2, '2026-10-02 13:00:00', 0, 'refused')"#,
    )
    .await
    .unwrap();
    migrations.run(&pool).await.unwrap();
    let rows: Vec<BuildStatusRow> = sqlx::query_as(
        r#"SELECT "PlayoutId", "LastBuild", "Success", "Message", "LastSuccessfulBuild" FROM "PlayoutBuildStatus" ORDER BY "PlayoutId""#
    ).fetch_all(&pool).await.unwrap();
    assert_eq!(
        rows,
        vec![
            (
                1,
                "2026-10-02 12:00:00".into(),
                true,
                None,
                Some("2026-10-02 12:00:00".into())
            ),
            (
                2,
                "2026-10-02 13:00:00".into(),
                false,
                Some("refused".into()),
                None
            ),
        ]
    );
}
