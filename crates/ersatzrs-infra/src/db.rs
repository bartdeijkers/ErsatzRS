use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{ConnectOptions, SqlitePool};

/// Open a `SQLite` pool with foreign keys enabled and run all pending migrations.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or migrations fail.
pub async fn init(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = database_url
        .parse::<SqliteConnectOptions>()?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .disable_statement_logging();

    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}
