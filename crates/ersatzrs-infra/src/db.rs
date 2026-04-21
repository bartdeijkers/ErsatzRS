use directories::BaseDirs;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

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

pub const DATABASE_URL_ENV_VAR: &str = "DATABASE_URL";
pub const DEFAULT_SQLITE_DIR_NAME: &str = ".ersatzrs";
pub const DEFAULT_SQLITE_FILE_NAME: &str = "ErsatzRS.db";

const DEFAULT_SYSTEM_PLAYLIST_GROUP_NAME: &str = "Trakt Lists";
const DEFAULT_FFMPEG_PROFILE_NAME: &str = "1920x1080 x264 aac";
const DEFAULT_CHANNEL_NUMBER: &str = "1";
const DEFAULT_CHANNEL_NAME: &str = "ErsatzTV";
const CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID: &str = "ffmpeg.default_profile_id";
const CONFIG_KEY_FFMPEG_DEFAULT_RESOLUTION_ID: &str = "ffmpeg.default_resolution_id";

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

#[derive(Debug, Error)]
pub enum DatabaseBootstrapError {
    #[error(transparent)]
    DatabaseUrl(#[from] DatabaseUrlError),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Resolve the startup database URL, preferring `DATABASE_URL` and otherwise
/// defaulting SQLite to `~/.ersatzrs/ErsatzRS.db`.
///
/// # Errors
///
/// Returns an error when the configured `DATABASE_URL` is not valid UTF-8, the
/// current user's home directory cannot be determined, or the default SQLite
/// data directory cannot be created.
pub fn resolve_database_url() -> Result<String, DatabaseUrlError> {
    resolve_database_url_from_parts(
        std::env::var_os(DATABASE_URL_ENV_VAR),
        BaseDirs::new().map(|base_dirs| base_dirs.home_dir().to_path_buf()),
    )
}

fn resolve_database_url_from_parts(
    database_url: Option<OsString>,
    home_dir: Option<PathBuf>,
) -> Result<String, DatabaseUrlError> {
    if let Some(database_url) = database_url {
        return database_url
            .into_string()
            .map_err(|_| DatabaseUrlError::NonUtf8EnvVar);
    }

    let home_dir = home_dir.ok_or(DatabaseUrlError::HomeDirectoryUnavailable)?;
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

    match database_backend(database_url) {
        DatabaseBackend::Sqlite => init_sqlite(database_url).await,
        #[cfg(feature = "mysql")]
        DatabaseBackend::MySql => init_mysql(database_url).await,
    }
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
/// ErsatzTV's `DbInitializer` seeds after migrations.
///
/// # Errors
///
/// Returns an error when startup database resolution, migrations, or default
/// data initialization fail.
pub async fn bootstrap_media_server_database() -> Result<StartupDatabase, DatabaseBootstrapError> {
    let startup = open_startup_database().await?;
    initialize_media_server_database(&startup.pool).await?;
    Ok(startup)
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

async fn initialize_media_server_database(pool: &DbPool) -> Result<(), sqlx::Error> {
    ensure_system_playlist_group(pool).await?;

    let default_resolution_id = ensure_default_resolution(pool).await?;
    ensure_config_value(
        pool,
        CONFIG_KEY_FFMPEG_DEFAULT_RESOLUTION_ID,
        &default_resolution_id.to_string(),
    )
    .await?;

    let default_profile_id = ensure_default_ffmpeg_profile(pool, default_resolution_id).await?;
    ensure_config_value(
        pool,
        CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID,
        &default_profile_id.to_string(),
    )
    .await?;

    ensure_default_channel(pool, default_profile_id).await?;

    Ok(())
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

    let result = sqlx::query(r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem") VALUES (?, 1)"#)
        .bind(DEFAULT_SYSTEM_PLAYLIST_GROUP_NAME)
        .execute(pool)
        .await?;
    Ok(last_insert_id(result))
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

async fn ensure_default_ffmpeg_profile(
    pool: &DbPool,
    resolution_id: i64,
) -> Result<i64, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "FFmpegProfile" WHERE "Name" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(DEFAULT_FFMPEG_PROFILE_NAME)
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }

    let result = sqlx::query(
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
    .bind(192_i32)
    .bind(384_i32)
    .bind(2_i32)
    .bind(1_i32)
    .bind(48_i32)
    .bind(0_i32)
    .bind(1_i32)
    .bind(0_i32)
    .bind(DEFAULT_FFMPEG_PROFILE_NAME)
    .bind(0_i32)
    .bind(0_i32)
    .bind(64_i32)
    .bind(resolution_id)
    .bind(0_i32)
    .bind(0_i32)
    .bind(Option::<&str>::None)
    .bind(0_i32)
    .bind(2000_i32)
    .bind(4000_i32)
    .bind(1_i32)
    .bind("high")
    .bind(Option::<&str>::None)
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

    Ok(last_insert_id(result))
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
            "UniqueId", "Number", "SortNumber", "Name", "FFmpegProfileId",
            "StreamingMode", "StreamSelectorMode", "SubtitleMode",
            "MusicVideoCreditsMode", "SongVideoMode", "PlayoutSource",
            "PlayoutMode", "TranscodeMode", "IdleBehavior", "IsEnabled", "ShowInEpg"
           ) VALUES (?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1)"#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(DEFAULT_CHANNEL_NUMBER)
    .bind(1.0_f64)
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
        database_backend, init, initialize_media_server_database, resolve_database_url_from_parts,
        DatabaseBackend, DatabaseUrlError, DEFAULT_SQLITE_DIR_NAME, DEFAULT_SQLITE_FILE_NAME,
    };
    use sqlx::Row;
    use std::ffi::OsString;

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

        initialize_media_server_database(&pool)
            .await
            .expect("seed mediaserver defaults");

        let resolution_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Resolution""#)
            .fetch_one(&pool)
            .await
            .expect("load resolution count");
        let profile_name: String =
            sqlx::query_scalar(r#"SELECT "Name" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1"#)
                .fetch_one(&pool)
                .await
                .expect("load default profile");
        let channel_name: String =
            sqlx::query_scalar(r#"SELECT "Name" FROM "Channel" ORDER BY "Id" LIMIT 1"#)
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
               WHERE "Key" IN ('ffmpeg.default_profile_id', 'ffmpeg.default_resolution_id')
               ORDER BY "Key""#,
        )
        .fetch_all(&pool)
        .await
        .expect("load config rows");

        assert_eq!(resolution_count, 4);
        assert_eq!(profile_name, "1920x1080 x264 aac");
        assert_eq!(channel_name, "ErsatzTV");
        assert_eq!(group_name, "Trakt Lists");
        assert_eq!(config_rows.len(), 2);
        for row in config_rows {
            let value: Option<String> = row.try_get("Value").expect("config value");
            assert!(value.is_some());
        }
    }

    #[tokio::test]
    async fn media_server_initializer_is_idempotent() {
        let database_url = temp_sqlite_database_url();
        let pool = init(&database_url).await.expect("initialize database");

        initialize_media_server_database(&pool)
            .await
            .expect("seed mediaserver defaults");
        initialize_media_server_database(&pool)
            .await
            .expect("repeat mediaserver defaults");

        let resolution_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Resolution""#)
            .fetch_one(&pool)
            .await
            .expect("load resolution count");
        let profile_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "FFmpegProfile" WHERE "Name" = '1920x1080 x264 aac'"#,
        )
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
        assert_eq!(profile_count, 1);
        assert_eq!(channel_count, 1);
        assert_eq!(system_group_count, 1);
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
