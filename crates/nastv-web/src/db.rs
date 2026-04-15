//! Database pool initialisation and migration runner.

use sqlx::SqlitePool;

/// Open (or create) the `SQLite` database at `database_url` and run all pending migrations.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or migrations fail.
pub async fn init(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePool::connect(database_url).await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}
