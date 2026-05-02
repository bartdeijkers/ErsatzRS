use anyhow::Context;
use clap::{Parser, ValueEnum};
use ersatzrs_core::repositories::{EmbyConnectionConfig, JellyfinConnectionConfig};
use ersatzrs_infra::db;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::emby::{sync_emby_movie_library, sync_emby_show_library, EmbyService};
use ersatzrs_infra::jellyfin::{
    sync_jellyfin_movie_library, sync_jellyfin_show_library, JellyfinService,
};
use ersatzrs_infra::plex::{
    sync_plex_collection_library, sync_plex_movie_library, sync_plex_show_library,
    PlexConnectionConfig, PlexService,
};
use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::Row;
use tracing::info;

const PLEX_SERVER_AUTH_TOKEN_PREFIX: &str = "plex.server_auth_token.";
const PLEX_CURRENT_SERVER_AUTH_TOKEN_KEY: &str = "plex.current_server_auth_token";
const JELLYFIN_AUTHORIZATION_HEADER_PREFIX: &str = "jellyfin.authorization_header.";
const JELLYFIN_API_KEY_PREFIX: &str = "jellyfin.api_key.";
const JELLYFIN_CURRENT_AUTHORIZATION_HEADER_KEY: &str = "jellyfin.current_authorization_header";
const JELLYFIN_CURRENT_SERVER_API_KEY_KEY: &str = "jellyfin.current_server_api_key";
const EMBY_API_KEY_PREFIX: &str = "emby.api_key.";
const EMBY_CURRENT_SERVER_API_KEY_KEY: &str = "emby.current_server_api_key";

#[derive(Debug, Parser)]
#[command(about = "Run one ErsatzRS media library scan")]
struct Args {
    #[arg(long, value_enum)]
    provider: Provider,
    #[arg(long)]
    library_id: i64,
    #[arg(long)]
    ffprobe_path: Option<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    deep_scan: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Provider {
    Local,
    Jellyfin,
    Plex,
    Emby,
}

#[tokio::main]
#[allow(clippy::disallowed_methods)]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let startup = db::open_startup_database()
        .await
        .context("initialize database")?;
    let pool = startup.pool;

    match args.provider {
        Provider::Local => scan_local(&pool, &args).await?,
        Provider::Jellyfin => scan_jellyfin(&pool, &args).await?,
        Provider::Plex => scan_plex(&pool, &args).await?,
        Provider::Emby => scan_emby(&pool, &args).await?,
    }
    Ok(())
}

async fn scan_local(pool: &DbPool, args: &Args) -> anyhow::Result<()> {
    if args.dry_run {
        info!(library_id = args.library_id, provider = "local", "dry run");
        return Ok(());
    }
    let report = scan_local_library(
        pool,
        &LocalScanConfig {
            library_id: args.library_id,
            ffprobe_path: args
                .ffprobe_path
                .clone()
                .unwrap_or_else(ffprobe_path_from_env),
        },
    )
    .await?;
    info!(scanned_files = report.scanned_files, "local scan complete");
    Ok(())
}

async fn scan_jellyfin(pool: &DbPool, args: &Args) -> anyhow::Result<()> {
    if args.dry_run {
        info!(
            library_id = args.library_id,
            provider = "jellyfin",
            "dry run"
        );
        return Ok(());
    }
    let config = load_jellyfin_config(pool, args.library_id).await?;
    let media_source_id = load_library_media_source_id(pool, args.library_id).await?;
    let parent_id =
        load_provider_library_key(pool, "JellyfinLibrary", "ItemId", args.library_id).await?;
    let kind = load_library_media_kind(pool, args.library_id).await?;
    let service = JellyfinService::new();
    let count = if kind == 1 {
        sync_jellyfin_show_library(
            pool,
            &service,
            &config,
            media_source_id,
            args.library_id,
            &parent_id,
            args.deep_scan,
        )
        .await?
    } else {
        sync_jellyfin_movie_library(
            pool,
            &service,
            &config,
            media_source_id,
            args.library_id,
            &parent_id,
            args.deep_scan,
        )
        .await?
    };
    info!(count, provider = "jellyfin", "scan complete");
    Ok(())
}

async fn scan_plex(pool: &DbPool, args: &Args) -> anyhow::Result<()> {
    if args.dry_run {
        info!(library_id = args.library_id, provider = "plex", "dry run");
        return Ok(());
    }
    let config = load_plex_config(pool, args.library_id).await?;
    let media_source_id = load_library_media_source_id(pool, args.library_id).await?;
    let library_key =
        load_provider_library_key(pool, "PlexLibrary", "Key", args.library_id).await?;
    let kind = load_library_media_kind(pool, args.library_id).await?;
    let service = PlexService::new();
    let count = match kind {
        1 => {
            sync_plex_show_library(
                pool,
                &service,
                &config,
                media_source_id,
                args.library_id,
                &library_key,
                args.deep_scan,
            )
            .await?
        }
        0 => {
            sync_plex_movie_library(
                pool,
                &service,
                &config,
                media_source_id,
                args.library_id,
                &library_key,
                args.deep_scan,
            )
            .await?
        }
        _ => {
            sync_plex_collection_library(pool, &service, &config, media_source_id, &library_key)
                .await?
        }
    };
    info!(count, provider = "plex", "scan complete");
    Ok(())
}

async fn scan_emby(pool: &DbPool, args: &Args) -> anyhow::Result<()> {
    if args.dry_run {
        info!(library_id = args.library_id, provider = "emby", "dry run");
        return Ok(());
    }
    let config = load_emby_config(pool, args.library_id).await?;
    let media_source_id = load_library_media_source_id(pool, args.library_id).await?;
    let parent_id =
        load_provider_library_key(pool, "EmbyLibrary", "ItemId", args.library_id).await?;
    let kind = load_library_media_kind(pool, args.library_id).await?;
    let service = EmbyService::new();
    let count = if kind == 1 {
        sync_emby_show_library(
            pool,
            &service,
            &config,
            media_source_id,
            args.library_id,
            &parent_id,
            args.deep_scan,
        )
        .await?
    } else {
        sync_emby_movie_library(
            pool,
            &service,
            &config,
            media_source_id,
            args.library_id,
            &parent_id,
            args.deep_scan,
        )
        .await?
    };
    info!(count, provider = "emby", "scan complete");
    Ok(())
}

async fn load_library_media_source_id(pool: &DbPool, library_id: i64) -> anyhow::Result<i64> {
    sqlx::query(r#"SELECT "MediaSourceId" FROM "Library" WHERE "Id" = ?"#)
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .map(|row| row.get("MediaSourceId"))
        .context("library not found")
}

async fn load_library_media_kind(pool: &DbPool, library_id: i64) -> anyhow::Result<i64> {
    sqlx::query(r#"SELECT "MediaKind" FROM "Library" WHERE "Id" = ?"#)
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .map(|row| row.get("MediaKind"))
        .context("library not found")
}

async fn load_provider_library_key(
    pool: &DbPool,
    table: &str,
    column: &str,
    library_id: i64,
) -> anyhow::Result<String> {
    let sql = format!(r#"SELECT "{column}" AS "Value" FROM "{table}" WHERE "Id" = ?"#);
    sqlx::query(&sql)
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .and_then(|row| row.get::<Option<String>, _>("Value"))
        .context("provider library row not found")
}

async fn load_jellyfin_config(
    pool: &DbPool,
    library_id: i64,
) -> anyhow::Result<JellyfinConnectionConfig> {
    let media_source_id = load_library_media_source_id(pool, library_id).await?;
    let address = load_connection_address(
        pool,
        "JellyfinConnection",
        "JellyfinMediaSourceId",
        media_source_id,
    )
    .await?;
    let authorization_header = format_jellyfin_authorization_header(
        load_first_config_value(
            pool,
            &[
                format!("{JELLYFIN_AUTHORIZATION_HEADER_PREFIX}{address}"),
                JELLYFIN_CURRENT_AUTHORIZATION_HEADER_KEY.to_owned(),
                format!("{JELLYFIN_API_KEY_PREFIX}{address}"),
                JELLYFIN_CURRENT_SERVER_API_KEY_KEY.to_owned(),
            ],
            "jellyfin credential",
        )
        .await?,
    );
    Ok(JellyfinConnectionConfig {
        address,
        authorization_header,
    })
}

async fn load_emby_config(pool: &DbPool, library_id: i64) -> anyhow::Result<EmbyConnectionConfig> {
    let media_source_id = load_library_media_source_id(pool, library_id).await?;
    let address =
        load_connection_address(pool, "EmbyConnection", "EmbyMediaSourceId", media_source_id)
            .await?;
    let api_key = load_first_config_value(
        pool,
        &[
            format!("{EMBY_API_KEY_PREFIX}{address}"),
            EMBY_CURRENT_SERVER_API_KEY_KEY.to_owned(),
        ],
        "emby api key",
    )
    .await?;
    Ok(EmbyConnectionConfig { address, api_key })
}

async fn load_plex_config(pool: &DbPool, library_id: i64) -> anyhow::Result<PlexConnectionConfig> {
    let media_source_id = load_library_media_source_id(pool, library_id).await?;
    let row = sqlx::query(
        r#"SELECT "PlexConnection"."Uri" AS "Uri",
                  "PlexMediaSource"."ClientIdentifier" AS "ClientIdentifier"
           FROM "PlexConnection"
           JOIN "PlexMediaSource" ON "PlexMediaSource"."Id" = "PlexConnection"."PlexMediaSourceId"
           WHERE "PlexConnection"."PlexMediaSourceId" = ?
             AND "PlexConnection"."IsActive" = 1
           ORDER BY "PlexConnection"."Id"
           LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await?
    .context("active Plex connection not found")?;
    let uri = row
        .get::<Option<String>, _>("Uri")
        .context("active Plex connection uri not found")?;
    let client_identifier = row
        .get::<Option<String>, _>("ClientIdentifier")
        .context("plex media source client identifier not found")?;
    let token = load_first_config_value(
        pool,
        &[
            format!("{PLEX_SERVER_AUTH_TOKEN_PREFIX}{client_identifier}"),
            PLEX_CURRENT_SERVER_AUTH_TOKEN_KEY.to_owned(),
        ],
        "plex server auth token",
    )
    .await?;
    Ok(PlexConnectionConfig { uri, token })
}

async fn load_first_config_value(
    pool: &DbPool,
    keys: &[String],
    description: &str,
) -> anyhow::Result<String> {
    for key in keys {
        let value = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(key)
        .fetch_optional(pool)
        .await?;

        if let Some(value) = value.filter(|candidate| !candidate.is_empty()) {
            return Ok(value);
        }
    }

    Err(anyhow::anyhow!("{description} not found in ConfigElement"))
}

fn format_jellyfin_authorization_header(value: String) -> String {
    if value.contains("Token=") || value.starts_with("MediaBrowser ") {
        value
    } else {
        format!("MediaBrowser Token=\"{value}\"")
    }
}

async fn load_connection_address(
    pool: &DbPool,
    table: &str,
    media_source_column: &str,
    media_source_id: i64,
) -> anyhow::Result<String> {
    let sql = format!(
        r#"SELECT "Address" AS "Address"
           FROM "{table}"
           WHERE "{media_source_column}" = ?
           ORDER BY "Id"
           LIMIT 1"#
    );
    sqlx::query(&sql)
        .bind(media_source_id)
        .fetch_optional(pool)
        .await?
        .and_then(|row| row.get::<Option<String>, _>("Address"))
        .context("provider connection not found")
}

#[allow(clippy::disallowed_methods)]
fn ffprobe_path_from_env() -> String {
    [
        "ErsatzRS_FFPROBE__PATH",
        "ERSATZRS_FFPROBE__PATH",
        "FFPROBE_PATH",
        "ERSATZRS_FFPROBE",
    ]
    .iter()
    .find_map(|key| std::env::var(key).ok())
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "ffprobe".to_owned())
}

#[cfg(all(test, not(feature = "mysql")))]
mod tests {
    use super::{
        scan_emby, scan_jellyfin, scan_plex, Args, Provider, PLEX_SERVER_AUTH_TOKEN_PREFIX,
    };
    use httpmock::{prelude::*, Mock};
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::{Executor, Row, SqlitePool};

    async fn test_db() -> SqlitePool {
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

        pool.execute(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height")
               VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .await
        .expect("seed resolution");

        pool
    }

    async fn mock_plex_scan_page(server: &MockServer) -> Mock<'_> {
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/library/sections/library-key/all")
                    .header("Accept", "application/json")
                    .header("X-Plex-Token", "db-token")
                    .query_param("X-Plex-Container-Start", "0")
                    .query_param("X-Plex-Container-Size", "100");
                then.status(200).json_body(json!({
                    "MediaContainer": {
                        "totalSize": 1,
                        "Metadata": [
                            {
                                "key": "/library/metadata/1",
                                "title": "Movie",
                                "addedAt": 1_776_406_400,
                                "updatedAt": 1_776_406_500,
                                "index": 1,
                                "media": [
                                    {
                                        "id": 1,
                                        "duration": 120_000,
                                        "width": 1920,
                                        "height": 1080,
                                        "aspectRatio": 1.777,
                                        "part": [
                                            {
                                                "id": 2,
                                                "key": "/library/parts/1",
                                                "file": "/media/movie.mkv",
                                                "stream": [
                                                    {
                                                        "id": 1,
                                                        "index": 0,
                                                        "streamType": 1,
                                                        "codec": "h264",
                                                        "width": 1920,
                                                        "height": 1080,
                                                        "scanType": "progressive",
                                                        "frameRate": "24.000"
                                                    }
                                                ]
                                            }
                                        ]
                                    }
                                ]
                            }
                        ]
                    }
                }));
            })
            .await
    }

    async fn seed_plex_scan_db(pool: &SqlitePool, base_url: &str) {
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
            .await
            .expect("seed media source");
        pool.execute(
            r#"INSERT INTO "PlexMediaSource" ("Id", "ClientIdentifier") VALUES (10, 'client-1')"#,
        )
        .await
        .expect("seed plex media source");
        sqlx::query(
            r#"INSERT INTO "PlexConnection" ("Id", "IsActive", "Uri", "PlexMediaSourceId")
               VALUES (?, 1, ?, 10)"#,
        )
        .bind(1_i64)
        .bind(base_url)
        .execute(pool)
        .await
        .expect("seed plex connection");
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (5, 'Plex Movies', 0, 10)"#,
        )
        .await
        .expect("seed library");
        pool.execute(
            r#"INSERT INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems")
               VALUES (5, 'library-key', 1)"#,
        )
        .await
        .expect("seed plex library");
        sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(format!("{PLEX_SERVER_AUTH_TOKEN_PREFIX}client-1"))
            .bind("db-token")
            .execute(pool)
            .await
            .expect("seed config element");
    }

    fn plex_scan_args() -> Args {
        Args {
            provider: Provider::Plex,
            library_id: 5,
            ffprobe_path: Some("ffprobe".to_owned()),
            dry_run: false,
        }
    }

    fn dry_run_args(provider: Provider) -> Args {
        Args {
            provider,
            library_id: 999,
            ffprobe_path: Some("ffprobe".to_owned()),
            dry_run: true,
        }
    }

    #[tokio::test]
    async fn jellyfin_dry_run_returns_before_loading_credentials() {
        let pool = test_db().await;

        scan_jellyfin(&pool, &dry_run_args(Provider::Jellyfin))
            .await
            .expect("jellyfin dry run should short-circuit");
    }

    #[tokio::test]
    async fn plex_dry_run_returns_before_loading_credentials() {
        let pool = test_db().await;

        scan_plex(&pool, &dry_run_args(Provider::Plex))
            .await
            .expect("plex dry run should short-circuit");
    }

    #[tokio::test]
    async fn emby_dry_run_returns_before_loading_credentials() {
        let pool = test_db().await;

        scan_emby(&pool, &dry_run_args(Provider::Emby))
            .await
            .expect("emby dry run should short-circuit");
    }

    #[tokio::test]
    async fn plex_scan_reads_token_from_database_config() {
        let server = MockServer::start_async().await;
        let plex_page = mock_plex_scan_page(&server).await;

        let pool = test_db().await;
        seed_plex_scan_db(&pool, &server.base_url()).await;

        std::env::set_var("PLEX_TOKEN", "env-token-should-not-be-used");

        scan_plex(&pool, &plex_scan_args())
            .await
            .expect("scan plex library");

        plex_page.assert_async().await;

        let library_key: Option<String> =
            sqlx::query_scalar(r#"SELECT "Key" FROM "PlexLibrary" WHERE "Id" = 5"#)
                .fetch_one(&pool)
                .await
                .expect("load plex library key");
        assert_eq!(library_key.as_deref(), Some("library-key"));

        let file_count: i64 = sqlx::query(r#"SELECT COUNT(*) AS "Count" FROM "MediaFile""#)
            .fetch_one(&pool)
            .await
            .expect("count media files")
            .get("Count");
        assert_eq!(file_count, 1);
    }
}
