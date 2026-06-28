use sqlx::migrate::Migrator;
#[cfg(test)]
use std::ffi::OsString;
use std::path::{Path, PathBuf};
#[cfg(not(feature = "mysql"))]
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

use ersatzrs_domain::{
    HardwareAccelerationKind, CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
    CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES, CONFIG_KEY_FFMPEG_HLS_DIRECT_OUTPUT_FORMAT,
    CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT, CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE,
    CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES, CONFIG_KEY_FFMPEG_SAVE_REPORTS,
    CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT, CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
    CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS, CONFIG_KEY_LIBRARY_REFRESH_INTERVAL,
    CONFIG_KEY_MINIMUM_LOG_LEVEL, CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP,
    CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING, CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING,
    CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING, CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING,
    CONFIG_KEY_PAGES_IS_DARK_MODE, CONFIG_KEY_PAGES_LANGUAGE, CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD,
    CONFIG_KEY_PLAYOUT_SCRIPTED_SCHEDULE_TIMEOUT_SECONDS, CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS,
    CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR, CONFIG_KEY_XMLTV_DAYS_TO_BUILD, CONFIG_KEY_XMLTV_TIME_ZONE,
};

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

pub fn assert_sql_safe(sql: impl AsRef<str>) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(sql.as_ref().to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    Sqlite,
    #[cfg(feature = "mysql")]
    MySql,
}

pub const DATABASE_URL_ENV_VAR: &str = "DATABASE_URL";
pub const DEFAULT_SQLITE_DIR_NAME: &str = ".ersatzrs";
pub const DEFAULT_SQLITE_FILE_NAME: &str = "ErsatzRS.db";
#[cfg(not(feature = "mysql"))]
const SQLITE_BUSY_TIMEOUT_MS: u64 = 30_000;

const DEFAULT_SYSTEM_PLAYLIST_GROUP_NAME: &str = "Trakt Lists";
const DEFAULT_FFMPEG_PROFILE_NAME: &str = "1920x1080 x264 ac3 6ch";
const DEFAULT_CHANNEL_NUMBER: &str = "1";
const DEFAULT_CHANNEL_NAME: &str = "ErsatzRS";
const CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID: &str = "ffmpeg.default_profile_id";
const CONFIG_KEY_FFMPEG_DEFAULT_RESOLUTION_ID: &str = "ffmpeg.default_resolution_id";
const CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT: &str = "ffmpeg.default_mpegts_script";

const ADR0017_DEFAULT_PROFILE_NAMES: [&str; 5] = [
    "1280x720 x264 ac3 6ch",
    DEFAULT_FFMPEG_PROFILE_NAME,
    "1920x1080 x265 10-bit hdr ac3 6ch",
    "3840x2160 x265 ac3 6ch",
    "3840x2160 x265 10-bit hdr ac3 6ch",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DefaultFfmpegProfileSeed {
    name: &'static str,
    width: i32,
    height: i32,
    video_format: i32,
    video_profile: &'static str,
    bit_depth: i32,
    video_bitrate: i32,
    video_buffer_size: i32,
}

const ADR0017_DEFAULT_PROFILES: [DefaultFfmpegProfileSeed; 5] = [
    DefaultFfmpegProfileSeed {
        name: "1280x720 x264 ac3 6ch",
        width: 1280,
        height: 720,
        video_format: 1,
        video_profile: "high",
        bit_depth: 0,
        video_bitrate: 8000,
        video_buffer_size: 8000,
    },
    DefaultFfmpegProfileSeed {
        name: DEFAULT_FFMPEG_PROFILE_NAME,
        width: 1920,
        height: 1080,
        video_format: 1,
        video_profile: "high",
        bit_depth: 0,
        video_bitrate: 16000,
        video_buffer_size: 16000,
    },
    DefaultFfmpegProfileSeed {
        name: "1920x1080 x265 10-bit hdr ac3 6ch",
        width: 1920,
        height: 1080,
        video_format: 2,
        video_profile: "main10",
        bit_depth: 1,
        video_bitrate: 20000,
        video_buffer_size: 20000,
    },
    DefaultFfmpegProfileSeed {
        name: "3840x2160 x265 ac3 6ch",
        width: 3840,
        height: 2160,
        video_format: 2,
        video_profile: "main",
        bit_depth: 0,
        video_bitrate: 45000,
        video_buffer_size: 45000,
    },
    DefaultFfmpegProfileSeed {
        name: "3840x2160 x265 10-bit hdr ac3 6ch",
        width: 3840,
        height: 2160,
        video_format: 2,
        video_profile: "main10",
        bit_depth: 1,
        video_bitrate: 60000,
        video_buffer_size: 60000,
    },
];

static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("../../migrations");
#[cfg(feature = "mysql")]
static MYSQL_MIGRATOR: Migrator = sqlx::migrate!("../../migrations/mysql");

#[derive(Debug, Error)]
pub enum DatabaseUrlError {
    #[error("DATABASE_URL must be valid UTF-8")]
    NonUtf8EnvVar,
    #[error("unable to determine the current user's home directory")]
    HomeDirectoryUnavailable,
    #[error("failed to create sqlite data directory at {path}")]
    CreateSqliteDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug)]
pub struct StartupDatabase {
    pub database_url: String,
    pub database_backend: DatabaseBackend,
    pub pool: DbPool,
}

#[derive(Debug)]
pub struct MediaServerStartupDatabase {
    pub database_url: String,
    pub database_backend: DatabaseBackend,
    pub pool: DbPool,
    pub schema_status: DatabaseSchemaStatus,
    pub seeded_default_ffmpeg_profiles: bool,
}

#[derive(Debug, Error)]
pub enum DatabaseBootstrapError {
    #[error(transparent)]
    DatabaseUrl(#[from] DatabaseUrlError),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatabaseSchemaStatus {
    pub dirty_version: Option<i64>,
    pub unknown_migrations: Vec<i64>,
    pub mismatched_migrations: Vec<i64>,
}

impl DatabaseSchemaStatus {
    #[must_use]
    pub fn failure_count(&self) -> u32 {
        u32::from(self.dirty_version.is_some())
            + u32::from(!self.unknown_migrations.is_empty())
            + u32::from(!self.mismatched_migrations.is_empty())
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.failure_count() == 0
    }
}

/// Resolve the startup database URL through startup configuration and otherwise
/// default `SQLite` to `~/.ersatzrs/ErsatzRS.db`.
///
/// # Errors
///
/// Returns an error when the configured database URL is not valid UTF-8, the
/// current user's home directory cannot be determined, or the default `SQLite`
/// data directory cannot be created.
pub fn resolve_database_url() -> Result<String, DatabaseUrlError> {
    let config = crate::startup_config::StartupConfig::load();
    resolve_database_url_from_config(config)
}

#[cfg(test)]
fn resolve_database_url_from_parts(
    database_url: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> Result<String, DatabaseUrlError> {
    resolve_database_url_from_config(crate::startup_config::StartupConfig {
        database_url,
        home_dir,
    })
}

fn resolve_database_url_from_config(
    config: crate::startup_config::StartupConfig,
) -> Result<String, DatabaseUrlError> {
    if let Some(database_url) = config.database_url {
        return database_url
            .into_string()
            .map_err(|_| DatabaseUrlError::NonUtf8EnvVar);
    }

    let home_dir = config
        .home_dir
        .ok_or(DatabaseUrlError::HomeDirectoryUnavailable)?;
    let sqlite_dir = default_sqlite_data_dir(&home_dir);
    std::fs::create_dir_all(&sqlite_dir).map_err(|source| {
        DatabaseUrlError::CreateSqliteDirectory {
            path: sqlite_dir.clone(),
            source,
        }
    })?;

    Ok(default_sqlite_database_url(
        &sqlite_dir.join(DEFAULT_SQLITE_FILE_NAME),
    ))
}

fn default_sqlite_data_dir(home_dir: &Path) -> PathBuf {
    home_dir.join(DEFAULT_SQLITE_DIR_NAME)
}

fn default_sqlite_database_url(path: &Path) -> String {
    format!("sqlite:{}", path.display())
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

    let database_backend = database_backend(database_url);
    let pool = connect_database(database_url, database_backend).await?;
    run_migrations(&pool, database_backend).await?;
    Ok(pool)
}

/// Resolve the startup database URL, detect the backend, and open the pooled
/// connection with migrations applied.
///
/// # Errors
///
/// Returns an error when the startup database URL cannot be resolved, the
/// backend is unsupported, or migrations fail.
pub async fn open_startup_database() -> Result<StartupDatabase, DatabaseBootstrapError> {
    let database_url = resolve_database_url()?;
    let database_backend = database_backend(&database_url);
    let pool = init(&database_url).await?;

    Ok(StartupDatabase {
        database_url,
        database_backend,
        pool,
    })
}

/// Open the mediaserver database and apply the upstream one-time defaults that
/// `ErsatzTV`'s `DbInitializer` seeds after migrations.
///
/// # Errors
///
/// Returns an error when startup database resolution, migrations, or default
/// data initialization fail.
pub async fn bootstrap_media_server_database(
) -> Result<MediaServerStartupDatabase, DatabaseBootstrapError> {
    bootstrap_media_server_database_from_url(resolve_database_url()?).await
}

async fn bootstrap_media_server_database_from_url(
    database_url: String,
) -> Result<MediaServerStartupDatabase, DatabaseBootstrapError> {
    let database_backend = database_backend(&database_url);
    let pool = connect_database(&database_url, database_backend).await?;
    let schema_status = inspect_database_schema(&pool, database_backend).await?;
    let mut seeded_default_ffmpeg_profiles = false;

    if schema_status.is_ready() {
        run_migrations(&pool, database_backend).await?;
        seeded_default_ffmpeg_profiles = initialize_media_server_database(&pool).await?;
    }

    Ok(MediaServerStartupDatabase {
        database_url,
        database_backend,
        pool,
        schema_status,
        seeded_default_ffmpeg_profiles,
    })
}

#[cfg(not(feature = "mysql"))]
async fn connect_sqlite(database_url: &str) -> Result<DbPool, sqlx::Error> {
    let options = database_url
        .parse::<SqliteConnectOptions>()?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS))
        .disable_statement_logging();

    SqlitePoolOptions::new().connect_with(options).await
}

#[cfg(feature = "mysql")]
async fn connect_sqlite(database_url: &str) -> Result<DbPool, sqlx::Error> {
    sqlx::any::install_default_drivers();
    let database_url = sqlite_url_with_create_mode(database_url);
    let options = database_url
        .parse::<sqlx::any::AnyConnectOptions>()?
        .disable_statement_logging();

    AnyPoolOptions::new()
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                connection.execute("PRAGMA foreign_keys = ON").await?;
                connection.execute("PRAGMA journal_mode = WAL").await?;
                connection.execute("PRAGMA busy_timeout = 30000").await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
}

/// Open a `MySQL` pool, enable ANSI identifier quotes for each session, and run
/// all pending `MySQL` migrations.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or migrations fail.
#[cfg(feature = "mysql")]
async fn connect_mysql(database_url: &str) -> Result<DbPool, sqlx::Error> {
    sqlx::any::install_default_drivers();
    let options = database_url
        .parse::<sqlx::any::AnyConnectOptions>()?
        .disable_statement_logging();

    AnyPoolOptions::new()
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                connection
                    .execute("SET SESSION sql_mode = CONCAT_WS(',', @@sql_mode, 'ANSI_QUOTES')")
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
}

async fn connect_database(
    database_url: &str,
    database_backend: DatabaseBackend,
) -> Result<DbPool, sqlx::Error> {
    match database_backend {
        DatabaseBackend::Sqlite => connect_sqlite(database_url).await,
        #[cfg(feature = "mysql")]
        DatabaseBackend::MySql => connect_mysql(database_url).await,
    }
}

async fn run_migrations(
    pool: &DbPool,
    database_backend: DatabaseBackend,
) -> Result<(), sqlx::Error> {
    migrator_for_backend(database_backend).run(pool).await?;
    Ok(())
}

/// Inspect the migrations metadata table for downgrade or partially applied
/// schema problems.
///
/// # Errors
///
/// Returns an error when the migrations metadata cannot be read.
pub async fn inspect_database_schema(
    pool: &DbPool,
    database_backend: DatabaseBackend,
) -> Result<DatabaseSchemaStatus, sqlx::Error> {
    if !migrations_table_exists(pool, database_backend).await? {
        return Ok(DatabaseSchemaStatus::default());
    }

    let applied_rows = sqlx::query_as::<_, (i64, i64, Vec<u8>)>(
        r"SELECT version, CASE WHEN success THEN 1 ELSE 0 END AS success, checksum FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await?;

    let known_migrations = migrator_for_backend(database_backend)
        .iter()
        .map(|migration| (migration.version, migration.checksum.as_ref().to_vec()))
        .collect::<std::collections::HashMap<_, _>>();

    let dirty_version = applied_rows
        .iter()
        .find_map(|(version, success, _checksum)| (*success == 0).then_some(*version));
    let unknown_migrations = applied_rows
        .iter()
        .map(|(version, _success, _checksum)| *version)
        .filter(|version| !known_migrations.contains_key(version))
        .collect();
    let mismatched_migrations = applied_rows
        .into_iter()
        .filter_map(|(version, _success, checksum)| {
            known_migrations
                .get(&version)
                .filter(|known_checksum| known_checksum.as_slice() != checksum.as_slice())
                .map(|_known_checksum| version)
        })
        .collect();

    Ok(DatabaseSchemaStatus {
        dirty_version,
        unknown_migrations,
        mismatched_migrations,
    })
}

async fn migrations_table_exists(
    pool: &DbPool,
    database_backend: DatabaseBackend,
) -> Result<bool, sqlx::Error> {
    let exists: i64 = match database_backend {
        DatabaseBackend::Sqlite => sqlx::query_scalar(
            r"SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
        )
        .fetch_one(pool)
        .await?,
        #[cfg(feature = "mysql")]
        DatabaseBackend::MySql => sqlx::query_scalar(
            r"SELECT COUNT(*) FROM information_schema.tables
               WHERE table_schema = DATABASE() AND table_name = '_sqlx_migrations'",
        )
        .fetch_one(pool)
        .await?,
    };

    Ok(exists > 0)
}

fn migrator_for_backend(database_backend: DatabaseBackend) -> &'static Migrator {
    match database_backend {
        #[cfg(feature = "mysql")]
        DatabaseBackend::MySql => &MYSQL_MIGRATOR,
        DatabaseBackend::Sqlite => &SQLITE_MIGRATOR,
    }
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

async fn initialize_media_server_database(pool: &DbPool) -> Result<bool, sqlx::Error> {
    ensure_system_playlist_group(pool).await?;

    let default_resolution_id = ensure_default_resolution(pool).await?;
    ensure_config_value(
        pool,
        CONFIG_KEY_FFMPEG_DEFAULT_RESOLUTION_ID,
        &default_resolution_id.to_string(),
    )
    .await?;

    let (default_profile_id, seeded_default_ffmpeg_profiles) =
        ensure_default_ffmpeg_profile(pool).await?;
    ensure_config_value(
        pool,
        CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID,
        &default_profile_id.to_string(),
    )
    .await?;

    for (key, value) in [
        (CONFIG_KEY_FFMPEG_SAVE_REPORTS, "False"),
        (CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE, "eng"),
        (CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES, "True"),
        (CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES, "False"),
        (CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES, "False"),
        (CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT, "60"),
        (CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS, "1"),
        (CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT, "2"),
        (CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT, "1"),
        (CONFIG_KEY_FFMPEG_HLS_DIRECT_OUTPUT_FORMAT, "2"),
        (CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT, "default"),
        (CONFIG_KEY_PAGES_IS_DARK_MODE, "True"),
        (CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (CONFIG_KEY_LIBRARY_REFRESH_INTERVAL, "6"),
        (CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD, "2"),
        (CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS, "False"),
        (CONFIG_KEY_PLAYOUT_SCRIPTED_SCHEDULE_TIMEOUT_SECONDS, "30"),
        (CONFIG_KEY_XMLTV_DAYS_TO_BUILD, "2"),
        (CONFIG_KEY_XMLTV_TIME_ZONE, "Local"),
        (CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR, "SplitTimeEvenly"),
    ] {
        ensure_config_value(pool, key, value).await?;
    }

    for key in [
        CONFIG_KEY_MINIMUM_LOG_LEVEL,
        CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING,
        CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING,
        CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING,
        CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING,
        CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP,
    ] {
        ensure_config_value(pool, key, crate::logging::default_logging_level()).await?;
    }

    ensure_default_channel(pool, default_profile_id).await?;

    Ok(seeded_default_ffmpeg_profiles)
}

async fn ensure_system_playlist_group(pool: &DbPool) -> Result<i64, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "PlaylistGroup" WHERE "IsSystem" = 1 ORDER BY "Id" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }

    sqlx::query(r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem") VALUES (?, 1)"#)
        .bind(DEFAULT_SYSTEM_PLAYLIST_GROUP_NAME)
        .execute(pool)
        .await?;

    sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "PlaylistGroup" WHERE "IsSystem" = 1 ORDER BY "Id" LIMIT 1"#,
    )
    .fetch_one(pool)
    .await
}

async fn ensure_default_resolution(pool: &DbPool) -> Result<i64, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "Resolution"
           WHERE "Width" = 1920 AND "Height" = 1080
           ORDER BY "Id"
           LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }

    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Resolution""#)
        .fetch_one(pool)
        .await?;

    if count == 0 {
        for (id, name, width, height) in [
            (1_i64, "720x480", 720_i32, 480_i32),
            (2_i64, "1280x720", 1280_i32, 720_i32),
            (3_i64, "1920x1080", 1920_i32, 1080_i32),
            (4_i64, "3840x2160", 3840_i32, 2160_i32),
        ] {
            sqlx::query(
                r#"INSERT INTO "Resolution" ("Id", "Name", "IsCustom", "Width", "Height")
                   VALUES (?, ?, 0, ?, ?)"#,
            )
            .bind(id)
            .bind(name)
            .bind(width)
            .bind(height)
            .execute(pool)
            .await?;
        }
    } else {
        sqlx::query(
            r#"INSERT INTO "Resolution" ("Name", "IsCustom", "Width", "Height")
               VALUES ('1920x1080', 0, 1920, 1080)"#,
        )
        .execute(pool)
        .await?;
    }

    sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "Resolution"
           WHERE "Width" = 1920 AND "Height" = 1080
           ORDER BY "Id"
           LIMIT 1"#,
    )
    .fetch_one(pool)
    .await
}

async fn ensure_resolution(
    pool: &DbPool,
    name: &str,
    width: i32,
    height: i32,
) -> Result<i64, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "Resolution"
           WHERE "Width" = ? AND "Height" = ?
           ORDER BY "Id"
           LIMIT 1"#,
    )
    .bind(width)
    .bind(height)
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }

    sqlx::query(
        r#"INSERT INTO "Resolution" ("Name", "IsCustom", "Width", "Height")
           VALUES (?, 0, ?, ?)"#,
    )
    .bind(name)
    .bind(width)
    .bind(height)
    .execute(pool)
    .await?;

    sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "Resolution"
           WHERE "Width" = ? AND "Height" = ?
           ORDER BY "Id"
           LIMIT 1"#,
    )
    .bind(width)
    .bind(height)
    .fetch_one(pool)
    .await
}

async fn ensure_default_ffmpeg_profile(pool: &DbPool) -> Result<(i64, bool), sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "FFmpegProfile" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(DEFAULT_FFMPEG_PROFILE_NAME)
    .fetch_optional(pool)
    .await?
    {
        return Ok((id, false));
    }

    let profile_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FFmpegProfile""#)
        .fetch_one(pool)
        .await?;

    if profile_count > 0 {
        let id = sqlx::query_scalar::<_, i64>(
            r#"SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1"#,
        )
        .fetch_one(pool)
        .await?;
        return Ok((id, false));
    }

    for profile in ADR0017_DEFAULT_PROFILES {
        let resolution_id =
            ensure_resolution(pool, profile.name, profile.width, profile.height).await?;
        insert_default_ffmpeg_profile(pool, profile, resolution_id, HardwareAccelerationKind::None)
            .await?;
    }

    let id = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "FFmpegProfile" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(DEFAULT_FFMPEG_PROFILE_NAME)
    .fetch_one(pool)
    .await?;

    Ok((id, true))
}

async fn insert_default_ffmpeg_profile(
    pool: &DbPool,
    profile: DefaultFfmpegProfileSeed,
    resolution_id: i64,
    hardware_acceleration: HardwareAccelerationKind,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO "FFmpegProfile" (
            "AudioBitrate", "AudioBufferSize", "AudioChannels", "AudioFormat",
            "AudioSampleRate", "BitDepth", "DeinterlaceVideo", "HardwareAcceleration",
            "Name", "NormalizeFramerate", "NormalizeLoudnessMode",
            "QsvExtraHardwareFrames", "ResolutionId", "ScalingBehavior", "ThreadCount",
            "VaapiDevice", "VaapiDriver", "VideoBitrate", "VideoBufferSize",
            "VideoFormat", "VideoProfile", "VideoPreset", "AllowBFrames",
            "VaapiDisplay", "TonemapAlgorithm", "TargetLoudness", "PadMode",
            "NormalizeAudio", "NormalizeColors", "NormalizeVideo"
           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(640_i32)
    .bind(640_i32)
    .bind(6_i32)
    .bind(2_i32)
    .bind(48_i32)
    .bind(profile.bit_depth)
    .bind(1_i32)
    .bind(hardware_acceleration as i32)
    .bind(profile.name)
    .bind(0_i32)
    .bind(0_i32)
    .bind(64_i32)
    .bind(resolution_id)
    .bind(0_i32)
    .bind(0_i32)
    .bind(Option::<&str>::None)
    .bind(0_i32)
    .bind(profile.video_bitrate)
    .bind(profile.video_buffer_size)
    .bind(profile.video_format)
    .bind(profile.video_profile)
    .bind("veryfast")
    .bind(0_i32)
    .bind(Option::<&str>::None)
    .bind(0_i32)
    .bind(Option::<f64>::None)
    .bind(1_i32)
    .bind(1_i32)
    .bind(1_i32)
    .bind(1_i32)
    .execute(pool)
    .await?;

    Ok(())
}

/// Apply the startup-detected encoder choice to freshly seeded ADR 0017
/// profiles.
///
/// The caller must only invoke this for the same startup that inserted the
/// default profiles. Existing databases are intentionally left alone.
///
/// # Errors
///
/// Returns an error when the update query fails.
pub async fn apply_default_ffmpeg_profile_hardware_acceleration(
    pool: &DbPool,
    acceleration: HardwareAccelerationKind,
) -> Result<(), sqlx::Error> {
    let acceleration = acceleration as i32;
    let video_preset = default_profile_video_preset_for_acceleration(acceleration);
    for name in ADR0017_DEFAULT_PROFILE_NAMES {
        sqlx::query(
            r#"UPDATE "FFmpegProfile"
               SET "HardwareAcceleration" = ?, "VideoPreset" = ?
               WHERE "Name" = ?"#,
        )
        .bind(acceleration)
        .bind(video_preset)
        .bind(name)
        .execute(pool)
        .await?;
    }

    Ok(())
}

const fn default_profile_video_preset_for_acceleration(acceleration: i32) -> Option<&'static str> {
    match acceleration {
        0 | 1 => Some("veryfast"),
        2 => Some("llhq"),
        _ => None,
    }
}

async fn ensure_default_channel(pool: &DbPool, ffmpeg_profile_id: i64) -> Result<(), sqlx::Error> {
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Channel""#)
        .fetch_one(pool)
        .await?;

    if count > 0 {
        return Ok(());
    }

    sqlx::query(
        r#"INSERT INTO "Channel" (
            "UniqueId", "Number", "SortNumber", "Name", "Group", "FFmpegProfileId",
            "StreamingMode", "StreamSelectorMode", "SubtitleMode",
            "MusicVideoCreditsMode", "SongVideoMode", "PlayoutSource",
            "PlayoutMode", "TranscodeMode", "IdleBehavior", "IsEnabled", "ShowInEpg"
           ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1)"#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(DEFAULT_CHANNEL_NUMBER)
    .bind(1.0_f64)
    .bind(DEFAULT_CHANNEL_NAME)
    .bind(DEFAULT_CHANNEL_NAME)
    .bind(ffmpeg_profile_id)
    .bind(5_i32)
    .execute(pool)
    .await?;

    Ok(())
}

async fn ensure_config_value(pool: &DbPool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    let exists =
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .fetch_one(pool)
            .await?;

    if exists == 0 {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(key)
            .bind(value)
            .execute(pool)
            .await?;
    }

    Ok(())
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
    use super::{
        apply_default_ffmpeg_profile_hardware_acceleration,
        bootstrap_media_server_database_from_url, connect_database, database_backend, init,
        initialize_media_server_database, inspect_database_schema, resolve_database_url_from_parts,
        DatabaseBackend, DatabaseUrlError, DbRow, DEFAULT_SQLITE_DIR_NAME,
        DEFAULT_SQLITE_FILE_NAME,
    };
    use ersatzrs_domain::HardwareAccelerationKind;
    use sqlx::Row;
    use std::ffi::OsString;

    #[derive(Clone, Copy)]
    struct ExpectedProfile<'a> {
        name: &'a str,
        width: i32,
        height: i32,
        video_format: i32,
        video_profile: &'a str,
        bit_depth: i32,
        bitrate: i32,
    }

    #[test]
    fn sqlite_urls_map_to_sqlite_backend() {
        assert_eq!(
            database_backend("sqlite://ersatzrs.db"),
            DatabaseBackend::Sqlite
        );
    }

    #[test]
    fn resolve_database_url_prefers_env_override() {
        let resolved = resolve_database_url_from_parts(
            Some(OsString::from("mysql://root@localhost:3306/ersatzrs")),
            None,
        )
        .expect("use env override");

        assert_eq!(resolved, "mysql://root@localhost:3306/ersatzrs");
    }

    #[test]
    fn resolve_database_url_preserves_docker_sqlite_override() {
        let resolved = resolve_database_url_from_parts(
            Some(OsString::from("sqlite:///data/.ersatzrs/ErsatzRS.db")),
            None,
        )
        .expect("use docker database url override");

        assert_eq!(resolved, "sqlite:///data/.ersatzrs/ErsatzRS.db");
        assert_eq!(database_backend(&resolved), DatabaseBackend::Sqlite);
    }

    #[test]
    fn resolve_database_url_defaults_to_hidden_home_dir() {
        let home_dir =
            std::env::temp_dir().join(format!("ersatzrs-db-home-{}", uuid::Uuid::new_v4()));
        let resolved = resolve_database_url_from_parts(None, Some(home_dir.clone()))
            .expect("resolve default sqlite url");

        assert_eq!(
            resolved,
            format!(
                "sqlite:{}",
                home_dir
                    .join(DEFAULT_SQLITE_DIR_NAME)
                    .join(DEFAULT_SQLITE_FILE_NAME)
                    .display()
            )
        );
        assert!(home_dir.join(DEFAULT_SQLITE_DIR_NAME).is_dir());

        std::fs::remove_dir_all(home_dir).expect("clean up test home dir");
    }

    #[test]
    fn resolve_database_url_requires_home_dir_without_override() {
        let error =
            resolve_database_url_from_parts(None, None).expect_err("missing home dir should fail");

        assert!(matches!(error, DatabaseUrlError::HomeDirectoryUnavailable));
    }

    #[test]
    fn resolve_database_url_rejects_non_utf8_env_override() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;

            let error = resolve_database_url_from_parts(Some(OsString::from_vec(vec![0xff])), None)
                .expect_err("non-utf8 env var should fail");

            assert!(matches!(error, DatabaseUrlError::NonUtf8EnvVar));
        }
    }

    #[tokio::test]
    async fn media_server_initializer_seeds_upstream_defaults() {
        let database_url = temp_sqlite_database_url();
        let pool = init(&database_url).await.expect("initialize database");

        let seeded = initialize_media_server_database(&pool)
            .await
            .expect("seed mediaserver defaults");
        assert!(seeded);

        let resolution_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Resolution""#)
            .fetch_one(&pool)
            .await
            .expect("load resolution count");
        let profiles = sqlx::query(
            r#"SELECT p."Name", r."Width", r."Height", p."VideoFormat", p."VideoProfile",
                      p."BitDepth", p."VideoBitrate", p."VideoBufferSize",
                      p."AudioFormat", p."AudioBitrate", p."AudioBufferSize",
                      p."AudioChannels", p."AudioSampleRate", p."VideoPreset",
                      p."AllowBFrames", p."DeinterlaceVideo", p."NormalizeFramerate",
                      p."HardwareAcceleration"
               FROM "FFmpegProfile" p
               JOIN "Resolution" r ON r."Id" = p."ResolutionId"
               ORDER BY p."Id""#,
        )
        .fetch_all(&pool)
        .await
        .expect("load default profiles");
        let default_profile_id: i64 = sqlx::query_scalar(
            r#"SELECT "Id" FROM "FFmpegProfile" WHERE "Name" = '1920x1080 x264 ac3 6ch'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load default profile id");
        let (channel_name, channel_group): (String, String) =
            sqlx::query_as(r#"SELECT "Name", "Group" FROM "Channel" ORDER BY "Id" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .expect("load default channel");
        let group_name: String = sqlx::query_scalar(
            r#"SELECT "Name" FROM "PlaylistGroup" WHERE "IsSystem" = 1 ORDER BY "Id" LIMIT 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load system playlist group");
        let config_rows = sqlx::query(
            r#"SELECT "Key", "Value" FROM "ConfigElement"
               WHERE "Key" IN (
                   'ffmpeg.default_profile_id',
                   'ffmpeg.default_resolution_id',
                   'log.minimum_level',
                   'log.minimum_level.scanning',
                   'log.minimum_level.scheduling',
                   'log.minimum_level.searching',
                   'log.minimum_level.streaming',
                   'log.minimum_level.http',
                   'ffmpeg.save_reports',
                   'ffmpeg.preferred_language_code',
                   'ffmpeg.use_embedded_subtitles',
                   'ffmpeg.extract_embedded_subtitles',
                   'ffmpeg.probe_for_interlaced_frames',
                   'ffmpeg.segmenter.timeout_seconds',
                   'ffmpeg.segmenter.work_ahead_limit',
                   'ffmpeg.segmenter.active_transcode_limit',
                   'ffmpeg.segmenter.initial_segment_count',
                   'ffmpeg.hls_direct.output_format',
                   'ffmpeg.default_mpegts_script',
                   'pages.is_dark_mode',
                   'pages.language',
                   'scanner.library_refresh_interval',
                   'playout.days_to_build',
                   'playout.skip_missing_items',
                   'playout.scripted_schedule_timeout_seconds',
                   'xmltv.days_to_build',
                   'xmltv.time_zone',
                   'xmltv.block_behavior'
               )
               ORDER BY "Key""#,
        )
        .fetch_all(&pool)
        .await
        .expect("load config rows");

        assert_eq!(resolution_count, 4);
        assert_default_ffmpeg_profiles(&profiles);
        assert_eq!(channel_name, "ErsatzRS");
        assert_eq!(channel_group, "ErsatzRS");
        assert_eq!(group_name, "Trakt Lists");
        let config_rows: std::collections::HashMap<String, String> = config_rows
            .into_iter()
            .map(|row| {
                (
                    row.try_get("Key").expect("config key"),
                    row.try_get::<Option<String>, _>("Value")
                        .expect("config value")
                        .expect("config value present"),
                )
            })
            .collect();

        assert_default_config_rows(&config_rows, default_profile_id);
    }

    fn assert_default_ffmpeg_profiles(profiles: &[DbRow]) {
        assert_eq!(profiles.len(), 5);
        assert_profile_row(
            &profiles[0],
            ExpectedProfile {
                name: "1280x720 x264 ac3 6ch",
                width: 1280,
                height: 720,
                video_format: 1,
                video_profile: "high",
                bit_depth: 0,
                bitrate: 8000,
            },
        );
        assert_profile_row(
            &profiles[1],
            ExpectedProfile {
                name: "1920x1080 x264 ac3 6ch",
                width: 1920,
                height: 1080,
                video_format: 1,
                video_profile: "high",
                bit_depth: 0,
                bitrate: 16000,
            },
        );
        assert_profile_row(
            &profiles[2],
            ExpectedProfile {
                name: "1920x1080 x265 10-bit hdr ac3 6ch",
                width: 1920,
                height: 1080,
                video_format: 2,
                video_profile: "main10",
                bit_depth: 1,
                bitrate: 20000,
            },
        );
        assert_profile_row(
            &profiles[3],
            ExpectedProfile {
                name: "3840x2160 x265 ac3 6ch",
                width: 3840,
                height: 2160,
                video_format: 2,
                video_profile: "main",
                bit_depth: 0,
                bitrate: 45000,
            },
        );
        assert_profile_row(
            &profiles[4],
            ExpectedProfile {
                name: "3840x2160 x265 10-bit hdr ac3 6ch",
                width: 3840,
                height: 2160,
                video_format: 2,
                video_profile: "main10",
                bit_depth: 1,
                bitrate: 60000,
            },
        );

        for row in profiles {
            assert_shared_profile_defaults(row);
        }
    }

    fn assert_profile_row(row: &DbRow, expected: ExpectedProfile<'_>) {
        assert_eq!(
            row.try_get::<String, _>("Name").expect("name"),
            expected.name
        );
        assert_eq!(
            row.try_get::<i32, _>("Width").expect("width"),
            expected.width
        );
        assert_eq!(
            row.try_get::<i32, _>("Height").expect("height"),
            expected.height
        );
        assert_eq!(
            row.try_get::<i32, _>("VideoFormat").expect("video format"),
            expected.video_format
        );
        assert_eq!(
            row.try_get::<String, _>("VideoProfile")
                .expect("video profile"),
            expected.video_profile
        );
        assert_eq!(
            row.try_get::<i32, _>("BitDepth").expect("bit depth"),
            expected.bit_depth
        );
        assert_eq!(
            row.try_get::<i32, _>("VideoBitrate")
                .expect("video bitrate"),
            expected.bitrate
        );
        assert_eq!(
            row.try_get::<i32, _>("VideoBufferSize")
                .expect("video buffer"),
            expected.bitrate
        );
    }

    fn assert_shared_profile_defaults(row: &DbRow) {
        assert_eq!(row.try_get::<i32, _>("AudioFormat").expect("audio"), 2);
        assert_eq!(
            row.try_get::<i32, _>("AudioBitrate")
                .expect("audio bitrate"),
            640
        );
        assert_eq!(
            row.try_get::<i32, _>("AudioBufferSize")
                .expect("audio buffer"),
            640
        );
        assert_eq!(
            row.try_get::<i32, _>("AudioChannels")
                .expect("audio channels"),
            6
        );
        assert_eq!(
            row.try_get::<i32, _>("AudioSampleRate")
                .expect("sample rate"),
            48
        );
        assert_eq!(
            row.try_get::<String, _>("VideoPreset")
                .expect("video preset"),
            "veryfast"
        );
        assert_eq!(row.try_get::<i32, _>("AllowBFrames").expect("bframes"), 0);
        assert_eq!(
            row.try_get::<i32, _>("DeinterlaceVideo")
                .expect("deinterlace"),
            1
        );
        assert_eq!(
            row.try_get::<i32, _>("NormalizeFramerate")
                .expect("framerate"),
            0
        );
        assert_eq!(
            row.try_get::<i32, _>("HardwareAcceleration")
                .expect("hardware acceleration"),
            0
        );
    }

    fn assert_default_config_rows(
        config_rows: &std::collections::HashMap<String, String>,
        default_profile_id: i64,
    ) {
        assert_eq!(config_rows.len(), 28);
        assert_eq!(
            config_rows["ffmpeg.default_profile_id"],
            default_profile_id.to_string()
        );
        assert_eq!(config_rows["ffmpeg.preferred_language_code"], "eng");
        assert_eq!(config_rows["ffmpeg.use_embedded_subtitles"], "True");
        assert_eq!(config_rows["ffmpeg.extract_embedded_subtitles"], "False");
        assert_eq!(config_rows["ffmpeg.probe_for_interlaced_frames"], "False");
        assert_eq!(config_rows["ffmpeg.segmenter.timeout_seconds"], "60");
        assert_eq!(config_rows["ffmpeg.segmenter.work_ahead_limit"], "1");
        assert_eq!(config_rows["ffmpeg.segmenter.active_transcode_limit"], "2");
        assert_eq!(config_rows["ffmpeg.segmenter.initial_segment_count"], "1");
        assert_eq!(config_rows["ffmpeg.hls_direct.output_format"], "2");
        assert_eq!(config_rows["ffmpeg.default_mpegts_script"], "default");
        assert_eq!(config_rows["pages.is_dark_mode"], "True");
        assert_eq!(config_rows["pages.language"], "en-us");
        assert_eq!(config_rows["scanner.library_refresh_interval"], "6");
        assert_eq!(config_rows["playout.days_to_build"], "2");
        assert_eq!(config_rows["playout.skip_missing_items"], "False");
        assert_eq!(
            config_rows["playout.scripted_schedule_timeout_seconds"],
            "30"
        );
        assert_eq!(config_rows["xmltv.days_to_build"], "2");
        assert_eq!(config_rows["xmltv.time_zone"], "Local");
        assert_eq!(config_rows["xmltv.block_behavior"], "SplitTimeEvenly");
    }

    #[tokio::test]
    async fn media_server_initializer_is_idempotent() {
        let database_url = temp_sqlite_database_url();
        let pool = init(&database_url).await.expect("initialize database");

        let first_seeded = initialize_media_server_database(&pool)
            .await
            .expect("seed mediaserver defaults");
        let second_seeded = initialize_media_server_database(&pool)
            .await
            .expect("repeat mediaserver defaults");
        assert!(first_seeded);
        assert!(!second_seeded);

        let resolution_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Resolution""#)
            .fetch_one(&pool)
            .await
            .expect("load resolution count");
        let profile_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FFmpegProfile""#)
            .fetch_one(&pool)
            .await
            .expect("load profile count");
        let channel_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Channel""#)
            .fetch_one(&pool)
            .await
            .expect("load channel count");
        let system_group_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlaylistGroup" WHERE "IsSystem" = 1"#)
                .fetch_one(&pool)
                .await
                .expect("load system group count");

        assert_eq!(resolution_count, 4);
        assert_eq!(profile_count, 5);
        assert_eq!(channel_count, 1);
        assert_eq!(system_group_count, 1);
    }

    #[tokio::test]
    async fn default_ffmpeg_profile_hardware_update_targets_seeded_profiles() {
        let database_url = temp_sqlite_database_url();
        let pool = init(&database_url).await.expect("initialize database");

        let seeded = initialize_media_server_database(&pool)
            .await
            .expect("seed mediaserver defaults");
        assert!(seeded);

        apply_default_ffmpeg_profile_hardware_acceleration(
            &pool,
            HardwareAccelerationKind::VideoToolbox,
        )
        .await
        .expect("apply hardware acceleration");

        let rows = sqlx::query(
            r#"SELECT "HardwareAcceleration", "VideoPreset"
               FROM "FFmpegProfile"
               ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .expect("load hardware rows");

        for row in rows {
            assert_eq!(
                row.try_get::<i32, _>("HardwareAcceleration")
                    .expect("hardware acceleration"),
                4
            );
            assert_eq!(
                row.try_get::<Option<String>, _>("VideoPreset")
                    .expect("video preset"),
                None
            );
        }
    }

    #[tokio::test]
    async fn schema_inspection_reports_unknown_migrations() {
        let database_url = temp_sqlite_database_url();
        let pool = init(&database_url).await.expect("initialize database");

        sqlx::query(
            r"INSERT INTO _sqlx_migrations
                (version, description, installed_on, success, checksum, execution_time)
               VALUES (9999, 'future migration', CURRENT_TIMESTAMP, 1, X'01', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert future migration row");

        let schema = inspect_database_schema(&pool, DatabaseBackend::Sqlite)
            .await
            .expect("inspect schema");

        assert_eq!(schema.dirty_version, None);
        assert_eq!(schema.unknown_migrations, vec![9999]);
        assert!(schema.mismatched_migrations.is_empty());
        assert!(!schema.is_ready());
    }

    #[tokio::test]
    async fn schema_inspection_reports_dirty_migrations() {
        let database_url = temp_sqlite_database_url();
        let pool = init(&database_url).await.expect("initialize database");

        sqlx::query(
            r"INSERT INTO _sqlx_migrations
                (version, description, installed_on, success, checksum, execution_time)
               VALUES (9998, 'dirty migration', CURRENT_TIMESTAMP, 0, X'02', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert dirty migration row");

        let schema = inspect_database_schema(&pool, DatabaseBackend::Sqlite)
            .await
            .expect("inspect schema");

        assert_eq!(schema.dirty_version, Some(9998));
        assert!(schema.unknown_migrations.contains(&9998));
        assert!(schema.mismatched_migrations.is_empty());
        assert!(!schema.is_ready());
    }

    #[tokio::test]
    async fn schema_inspection_reports_mismatched_migrations() {
        let database_url = temp_sqlite_database_url();
        let pool = init(&database_url).await.expect("initialize database");

        sqlx::query(r"UPDATE _sqlx_migrations SET checksum = X'03' WHERE version = 1")
            .execute(&pool)
            .await
            .expect("update checksum");

        let schema = inspect_database_schema(&pool, DatabaseBackend::Sqlite)
            .await
            .expect("inspect schema");

        assert_eq!(schema.dirty_version, None);
        assert!(schema.unknown_migrations.is_empty());
        assert_eq!(schema.mismatched_migrations, vec![1]);
        assert!(!schema.is_ready());
    }

    #[tokio::test]
    async fn schema_inspection_is_clean_when_migrations_table_is_missing() {
        let database_url = temp_sqlite_database_url();
        let pool = connect_database(&database_url, DatabaseBackend::Sqlite)
            .await
            .expect("connect database without migrations");

        let schema = inspect_database_schema(&pool, DatabaseBackend::Sqlite)
            .await
            .expect("inspect schema");

        assert_eq!(schema, super::DatabaseSchemaStatus::default());
    }

    #[tokio::test]
    async fn mediaserver_bootstrap_skips_migrations_when_schema_is_invalid() {
        let database_path = std::env::temp_dir().join(format!(
            "ersatzrs-bootstrap-invalid-{}.db",
            uuid::Uuid::new_v4()
        ));
        let database_url = format!("sqlite:{}", database_path.display());
        let pool = connect_database(&database_url, DatabaseBackend::Sqlite)
            .await
            .expect("connect database without migrations");

        sqlx::query(
            r"CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("create migrations table");
        sqlx::query(
            r"INSERT INTO _sqlx_migrations
                (version, description, installed_on, success, checksum, execution_time)
               VALUES (9999, 'future migration', CURRENT_TIMESTAMP, 1, X'01', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert future migration row");
        drop(pool);

        let startup = bootstrap_media_server_database_from_url(database_url)
            .await
            .expect("bootstrap mediaserver database");

        assert_eq!(startup.schema_status.unknown_migrations, vec![9999]);

        let resolution_table_count: i64 = sqlx::query_scalar(
            r"SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'Resolution'",
        )
        .fetch_one(&startup.pool)
        .await
        .expect("check for resolution table");

        assert_eq!(resolution_table_count, 0);
        assert!(!startup.schema_status.is_ready());
    }

    fn temp_sqlite_database_url() -> String {
        let temp_dir =
            std::env::temp_dir().join(format!("ersatzrs-bootstrap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        format!("sqlite:{}", temp_dir.join("ErsatzRS.db").display())
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
