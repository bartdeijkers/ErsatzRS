#[cfg(feature = "mysql")]
use sqlx::any::{AnyPoolOptions, AnyRow};
#[cfg(not(feature = "mysql"))]
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteQueryResult, SqliteRow,
};
#[cfg(feature = "mysql")]
use sqlx::{Any, AnyPool};
#[cfg(not(feature = "mysql"))]
use sqlx::{ConnectOptions, Sqlite, SqlitePool};

#[cfg(feature = "mysql")]
use sqlx::{ConnectOptions, Executor};

#[cfg(feature = "mysql")]
pub type Db = Any;
#[cfg(not(feature = "mysql"))]
pub type Db = Sqlite;

#[cfg(feature = "mysql")]
pub type DbPool = AnyPool;
#[cfg(not(feature = "mysql"))]
pub type DbPool = SqlitePool;

#[cfg(feature = "mysql")]
pub type DbRow = AnyRow;
#[cfg(not(feature = "mysql"))]
pub type DbRow = SqliteRow;

#[cfg(feature = "mysql")]
pub type DbQueryResult = sqlx::any::AnyQueryResult;
#[cfg(not(feature = "mysql"))]
pub type DbQueryResult = SqliteQueryResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    Sqlite,
    #[cfg(feature = "mysql")]
    MySql,
}

#[must_use]
pub fn database_backend(database_url: &str) -> DatabaseBackend {
    if database_url.starts_with("mysql://") || database_url.starts_with("mariadb://") {
        #[cfg(feature = "mysql")]
        {
            return DatabaseBackend::MySql;
        }
    }

    DatabaseBackend::Sqlite
}

/// Open a database pool and run all pending migrations.
///
/// # Errors
///
/// Returns an error if the database cannot be opened, the selected backend is
/// not compiled in, or migrations fail.
pub async fn init(database_url: &str) -> Result<DbPool, sqlx::Error> {
    #[cfg(not(feature = "mysql"))]
    if database_url.starts_with("mysql://") || database_url.starts_with("mariadb://") {
        return Err(sqlx::Error::Configuration(
            "MySQL database URLs require building ersatzrs-infra with the `mysql` feature".into(),
        ));
    }

    match database_backend(database_url) {
        DatabaseBackend::Sqlite => init_sqlite(database_url).await,
        #[cfg(feature = "mysql")]
        DatabaseBackend::MySql => init_mysql(database_url).await,
    }
}

#[cfg(not(feature = "mysql"))]
async fn init_sqlite(database_url: &str) -> Result<DbPool, sqlx::Error> {
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

#[cfg(feature = "mysql")]
async fn init_sqlite(database_url: &str) -> Result<DbPool, sqlx::Error> {
    sqlx::any::install_default_drivers();
    let database_url = sqlite_url_with_create_mode(database_url);
    let options = database_url
        .parse::<sqlx::any::AnyConnectOptions>()?
        .disable_statement_logging();

    let pool = AnyPoolOptions::new()
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                connection.execute("PRAGMA foreign_keys = ON").await?;
                connection.execute("PRAGMA journal_mode = WAL").await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}

/// Open a `MySQL` pool, enable ANSI identifier quotes for each session, and run
/// all pending `MySQL` migrations.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or migrations fail.
#[cfg(feature = "mysql")]
async fn init_mysql(database_url: &str) -> Result<DbPool, sqlx::Error> {
    sqlx::any::install_default_drivers();
    let options = database_url
        .parse::<sqlx::any::AnyConnectOptions>()?
        .disable_statement_logging();

    let pool = AnyPoolOptions::new()
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                connection
                    .execute("SET SESSION sql_mode = CONCAT_WS(',', @@sql_mode, 'ANSI_QUOTES')")
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await?;
    sqlx::migrate!("../../migrations/mysql").run(&pool).await?;
    Ok(pool)
}

#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn last_insert_id(result: DbQueryResult) -> i64 {
    #[cfg(feature = "mysql")]
    {
        result.last_insert_id().unwrap_or_default()
    }
    #[cfg(not(feature = "mysql"))]
    {
        result.last_insert_rowid()
    }
}

#[must_use]
pub fn random_order_expression(database_backend: DatabaseBackend) -> &'static str {
    match database_backend {
        #[cfg(feature = "mysql")]
        DatabaseBackend::MySql => "RAND()",
        DatabaseBackend::Sqlite => "RANDOM()",
    }
}

/// Lazily create a `SQLite` pool for tests and server-function contexts that do
/// not need migrations.
///
/// # Errors
///
/// Returns an error when the `SQLite` URL cannot be parsed by the selected
/// database driver.
pub fn lazy_sqlite_pool(database_url: &str) -> Result<DbPool, sqlx::Error> {
    #[cfg(feature = "mysql")]
    {
        sqlx::any::install_default_drivers();
        let database_url = sqlite_url_with_create_mode(database_url);
        AnyPoolOptions::new().connect_lazy(&database_url)
    }
    #[cfg(not(feature = "mysql"))]
    {
        SqlitePoolOptions::new().connect_lazy(database_url)
    }
}

#[cfg(feature = "mysql")]
fn sqlite_url_with_create_mode(database_url: &str) -> String {
    if database_url == "sqlite::memory:" || database_url.contains("mode=") {
        database_url.to_string()
    } else if database_url.contains('?') {
        format!("{database_url}&mode=rwc")
    } else {
        format!("{database_url}?mode=rwc")
    }
}

#[cfg(test)]
mod tests {
    use super::{database_backend, DatabaseBackend};

    #[test]
    fn sqlite_urls_map_to_sqlite_backend() {
        assert_eq!(
            database_backend("sqlite://ersatzrs.db"),
            DatabaseBackend::Sqlite
        );
    }

    #[cfg(feature = "mysql")]
    #[test]
    fn mysql_urls_map_to_mysql_backend() {
        assert_eq!(
            database_backend("mysql://root@localhost:3306/ersatzrs"),
            DatabaseBackend::MySql
        );
    }

    #[cfg(feature = "mysql")]
    #[test]
    fn mariadb_urls_map_to_mysql_backend() {
        assert_eq!(
            database_backend("mariadb://root@localhost:3306/ersatzrs"),
            DatabaseBackend::MySql
        );
    }
}
