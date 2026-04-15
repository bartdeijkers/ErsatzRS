use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Executor, SqlitePool};

/// Create a migrated in-memory database for repository tests.
///
/// # Panics
///
/// Panics when the in-memory database cannot be created, foreign keys cannot be
/// enabled, or the test migration cannot run.
pub async fn test_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("create in-memory sqlite pool");

    pool.execute("PRAGMA foreign_keys = ON")
        .await
        .expect("enable sqlite foreign keys");

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    pool
}
