use anyhow::Context;
use clap::{Parser, ValueEnum};
use ersatzrs_core::repositories::{
    EmbyConnectionConfig, JellyfinConnectionConfig, ManagedLibraryOperationRepository,
    ManagedLibraryScanSpec,
};
use ersatzrs_domain::{ManagedLibraryOperationKind, ManagedLibraryOperationState, MediaSourceKind};
use ersatzrs_infra::db;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::emby::{sync_emby_movie_library, sync_emby_show_library, EmbyService};
use ersatzrs_infra::ffprobe::LocalProbeCleanupScope;
use ersatzrs_infra::jellyfin::{
    sync_jellyfin_movie_library, sync_jellyfin_show_library, JellyfinService,
};
use ersatzrs_infra::plex::{
    sync_plex_collection_library, sync_plex_movie_library, sync_plex_other_video_library,
    sync_plex_show_library, PlexConnectionConfig, PlexService,
};
use ersatzrs_infra::repositories::SqliteManagedLibraryOperationRepository;
use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::Row;
use std::future::Future;
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
    let args = Args::parse();
    let startup = db::open_startup_database()
        .await
        .context("initialize database")?;
    init_logging(&startup.pool).await;

    let pool = startup.pool;

    if args.dry_run {
        match args.provider {
            Provider::Local => scan_local(&pool, &args).await?,
            Provider::Jellyfin => scan_jellyfin(&pool, &args).await?,
            Provider::Plex => scan_plex(&pool, &args).await?,
            Provider::Emby => scan_emby(&pool, &args).await?,
        }
        return Ok(());
    }

    let provider = match args.provider {
        Provider::Local => MediaSourceKind::Local,
        Provider::Jellyfin => MediaSourceKind::Jellyfin,
        Provider::Plex => MediaSourceKind::Plex,
        Provider::Emby => MediaSourceKind::Emby,
    };
    let operations = SqliteManagedLibraryOperationRepository(pool.clone());
    let operation = operations
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            args.library_id,
            ManagedLibraryScanSpec {
                provider,
                show_id: None,
                deep_scan: args.deep_scan,
            },
            false,
        )
        .await?
        .context("persist scanner operation")?;
    let claim = loop {
        if let Some(claim) = operations
            .claim_operation(operation.id, chrono::Utc::now())
            .await?
        {
            break claim;
        }
        match operations.operation(operation.id).await? {
            Some(current) if current.state == ManagedLibraryOperationState::Completed => {
                info!(
                    operation_id = operation.id,
                    "scan completed by the active host worker"
                );
                return Ok(());
            }
            Some(current) if current.state == ManagedLibraryOperationState::Failed => {
                anyhow::bail!("persisted scan operation failed in the active host worker");
            }
            None => anyhow::bail!("persisted scan operation disappeared while waiting"),
            Some(_) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
        }
    };
    let heartbeat_operations = operations.clone();
    let heartbeat_claim = claim.clone();
    let heartbeat = async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let current = heartbeat_operations
                .heartbeat(&heartbeat_claim, chrono::Utc::now())
                .await
                .context("refresh standalone scanner lease")?;
            if !current {
                anyhow::bail!("standalone scanner lease is no longer current");
            }
        }
    };

    let scan = async {
        match args.provider {
            Provider::Local => scan_local(&pool, &args).await,
            Provider::Jellyfin => scan_jellyfin(&pool, &args).await,
            Provider::Plex => scan_plex(&pool, &args).await,
            Provider::Emby => scan_emby(&pool, &args).await,
        }
    };
    let result = Box::pin(run_while_lease_current(scan, heartbeat)).await;
    let result = match result? {
        ForegroundScanOutcome::Finished(result) => result,
        ForegroundScanOutcome::LeaseLost(error) => return Err(error),
    };
    match result {
        Ok(()) => {
            if !operations.complete_operation(&claim).await? {
                anyhow::bail!("scanner lease expired before completion could be persisted");
            }
        }
        Err(error) => {
            let _ = operations
                .fail_operation(&claim, "scan_failed", "standalone scanner operation failed")
                .await;
            return Err(error);
        }
    }

    Ok(())
}

#[derive(Debug)]
enum ForegroundScanOutcome {
    Finished(anyhow::Result<()>),
    LeaseLost(anyhow::Error),
}

async fn run_while_lease_current<Scan, Heartbeat>(
    scan: Scan,
    heartbeat: Heartbeat,
) -> anyhow::Result<ForegroundScanOutcome>
where
    Scan: Future<Output = anyhow::Result<()>>,
    Heartbeat: Future<Output = anyhow::Result<()>>,
{
    let scope = LocalProbeCleanupScope::new();
    tokio::pin!(heartbeat);
    let mut heartbeat_owned = true;
    let mut outcome = {
        let scan = scope.scope(scan);
        tokio::pin!(scan);
        let outcome = tokio::select! {
            result = &mut scan => ForegroundScanOutcome::Finished(result),
            result = &mut heartbeat => {
                heartbeat_owned = false;
                ForegroundScanOutcome::LeaseLost(heartbeat_error(result))
            },
        };
        scope.close();
        outcome
    };
    let cleanup = scope.wait_cleanup();
    tokio::pin!(cleanup);
    loop {
        tokio::select! {
            biased;
            result = &mut cleanup => {
                result.map_err(|_| anyhow::anyhow!("Local probe cleanup was not confirmed"))?;
                return Ok(outcome);
            }
            result = &mut heartbeat, if heartbeat_owned => {
                heartbeat_owned = false;
                outcome = ForegroundScanOutcome::LeaseLost(heartbeat_error(result));
            }
        }
    }
}

fn heartbeat_error(result: anyhow::Result<()>) -> anyhow::Error {
    result
        .err()
        .unwrap_or_else(|| anyhow::anyhow!("standalone scanner heartbeat stopped unexpectedly"))
}

#[allow(clippy::disallowed_methods)]
async fn init_logging(pool: &DbPool) {
    let base_filter = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| ersatzrs_infra::logging::default_filter().to_owned());
    let filter = ersatzrs_infra::logging::logging_filter_from_database(pool, base_filter)
        .await
        .unwrap_or_else(|_| ersatzrs_infra::logging::default_filter().to_owned());

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(filter).unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(ersatzrs_infra::logging::default_filter())
            }),
        )
        .init();
}

async fn scan_local(pool: &DbPool, args: &Args) -> anyhow::Result<()> {
    if args.dry_run {
        info!(library_id = args.library_id, provider = "local", "dry run");
        return Ok(());
    }
    let report = scan_local_library(
        pool,
        &LocalScanConfig {
            deep_scan: args.deep_scan,
            library_id: args.library_id,
            ffprobe_path: resolve_ffprobe_path(pool, args).await?,
        },
    )
    .await?;
    if report.failed_files > 0 {
        anyhow::bail!("Local scan reported {} failed file(s)", report.failed_files);
    }
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
    let count = if kind == 2 {
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
    let media_source_id = load_library_media_source_id(pool, args.library_id).await?;
    let missing: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "PlexMediaSource"
           WHERE "Id" = ? AND "MissingSince" IS NOT NULL)"#,
    )
    .bind(media_source_id)
    .fetch_one(pool)
    .await?;
    if missing {
        info!(
            media_source_id,
            "Plex scan skipped because the server is missing"
        );
        return Ok(());
    }
    let config = load_plex_config(pool, args.library_id).await?;
    let library_key =
        load_provider_library_key(pool, "PlexLibrary", "Key", args.library_id).await?;
    let kind = load_library_media_kind(pool, args.library_id).await?;
    let service = PlexService::new();
    let count = match kind {
        1 => {
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
        2 => {
            let count = sync_plex_show_library(
                pool,
                &service,
                &config,
                media_source_id,
                args.library_id,
                &library_key,
                args.deep_scan,
            )
            .await?;
            ersatzrs_infra::plex::sync_plex_networks(pool, &service, &config, args.library_id)
                .await?;
            count
        }
        4 => {
            sync_plex_other_video_library(
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
    let count = if kind == 2 {
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
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
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
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(media_source_id)
        .fetch_optional(pool)
        .await?
        .and_then(|row| row.get::<Option<String>, _>("Address"))
        .context("provider connection not found")
}

/// Resolve the `ffprobe` path for a scan using the ADR 0008 runtime manager.
/// An explicit `--ffprobe-path` remains a one-shot override; otherwise scanner
/// shares the same managed runtime as the web host.
async fn resolve_ffprobe_path(pool: &DbPool, args: &Args) -> anyhow::Result<String> {
    if let Some(explicit) = args
        .ffprobe_path
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(explicit.to_owned());
    }

    let ffmpeg_config = load_first_config_value(
        pool,
        &[ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH.to_owned()],
        "ffmpeg path",
    )
    .await
    .ok();
    let ffprobe_config = load_first_config_value(
        pool,
        &[ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH.to_owned()],
        "ffprobe path",
    )
    .await
    .ok();

    let manager = ersatzrs_ffmpeg::FfmpegRuntimeManagerConfig::from_env(
        ersatzrs_infra::file_system_layout::ffmpeg_runtime_folder(),
    );
    let report = ersatzrs_ffmpeg::resolve_or_acquire_ffmpeg_runtime(
        &manager,
        ffmpeg_config.as_deref(),
        ffprobe_config.as_deref(),
    )
    .await
    .context("resolve FFmpeg runtime for scanner")?;
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(
        report.ffmpeg.path.clone(),
        report.ffprobe.path.clone(),
    );
    Ok(report.ffprobe.path)
}

#[cfg(test)]
mod tests {
    use super::{
        run_while_lease_current, scan_emby, scan_jellyfin, scan_plex, Args, ForegroundScanOutcome,
        Provider, PLEX_SERVER_AUTH_TOKEN_PREFIX,
    };
    use httpmock::{prelude::*, Mock};
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::{Executor, Row, SqlitePool};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

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
               VALUES (5, 'Plex Movies', 1, 10)"#,
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
            deep_scan: false,
        }
    }

    fn dry_run_args(provider: Provider) -> Args {
        Args {
            provider,
            library_id: 999,
            ffprobe_path: Some("ffprobe".to_owned()),
            dry_run: true,
            deep_scan: false,
        }
    }

    #[tokio::test]
    async fn heartbeat_failure_cancels_the_foreground_scan() {
        struct DropFlag(Arc<AtomicBool>);

        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let started = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let scan_started = started.clone();
        let scan_cancelled = cancelled.clone();
        let scan = async move {
            let _drop_flag = DropFlag(scan_cancelled);
            scan_started.store(true, Ordering::SeqCst);
            std::future::pending::<anyhow::Result<()>>().await
        };
        let heartbeat = async move {
            while !started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            anyhow::bail!("lease heartbeat failed")
        };

        let outcome = run_while_lease_current(scan, heartbeat)
            .await
            .expect("cancelled scan cleanup is confirmed");
        let ForegroundScanOutcome::LeaseLost(error) = outcome else {
            panic!("heartbeat loss must not become a finished scan");
        };

        assert!(error.to_string().contains("lease heartbeat failed"));
        assert!(
            cancelled.load(Ordering::SeqCst),
            "the foreground scan future must be dropped before returning"
        );
    }

    #[tokio::test]
    async fn confirmed_scan_failure_remains_a_finished_failure() {
        let outcome = run_while_lease_current(
            async { anyhow::bail!("ordinary scan failure") },
            std::future::pending(),
        )
        .await
        .expect("no local owners remain");
        let ForegroundScanOutcome::Finished(Err(error)) = outcome else {
            panic!("confirmed scan error must remain eligible for failure persistence");
        };
        assert_eq!(error.to_string(), "ordinary scan failure");
    }

    // Unix-only fixture exercises process groups; native Windows acceptance
    // remains responsible for the corresponding JobObject runtime proof.
    #[cfg(unix)]
    #[tokio::test]
    async fn foreground_lease_loss_reaps_actual_local_probe_before_returning() {
        use ersatzrs_infra::ffprobe::{FfprobeExtractor, LocalProbeCleanupScope};
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir().expect("temporary probe fixture");
        let program = directory.path().join("probe.sh");
        let ready = directory.path().join("probe.sh.ready");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s' \"$$\" > \"$0.ready\"\nexec sleep 60\n",
        )
        .expect("write original probe fixture");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
            .expect("make probe executable");
        let extractor = FfprobeExtractor::new(program.to_string_lossy());
        let completed = Arc::new(AtomicBool::new(false));
        let (scope_sender, scope_receiver) = tokio::sync::oneshot::channel();
        let scan = async {
            scope_sender
                .send(LocalProbeCleanupScope::current().expect("managed scan scope"))
                .expect("retain cleanup scope");
            let _ = extractor.extract("synthetic.mkv").await;
            completed.store(true, Ordering::SeqCst);
            Ok(())
        };
        let heartbeat = async {
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while std::fs::read_to_string(&ready)
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok())
                    .is_none()
                {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("probe became active");
            anyhow::bail!("lease lost while probing")
        };
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(9),
            run_while_lease_current(scan, heartbeat),
        )
        .await
        .expect("owned cleanup finishes")
        .expect("cleanup confirmed");
        assert!(
            matches!(outcome, ForegroundScanOutcome::LeaseLost(_)),
            "lease loss cannot enter terminal success/failure persistence"
        );
        assert!(!completed.load(Ordering::SeqCst));
        assert_eq!(
            scope_receiver
                .await
                .expect("cleanup scope")
                .cleanup_status(),
            Ok(true)
        );
        let pid = std::fs::read_to_string(&ready).expect("probe process identity");
        let alive = tokio::process::Command::new("/bin/sh")
            .args(["-c", "kill -0 \"$1\"", "probe-check", pid.as_str()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .expect("inspect probe process")
            .success();
        assert!(!alive, "probe process is reaped before lease-loss return");
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
    async fn plex_scan_skips_missing_server_without_contacting_old_address() {
        let server = MockServer::start_async().await;
        let plex_page = mock_plex_scan_page(&server).await;
        let pool = test_db().await;
        seed_plex_scan_db(&pool, &server.base_url()).await;
        sqlx::query(r#"UPDATE "PlexMediaSource" SET "MissingSince" = '2026-01-02T03:04:05Z'"#)
            .execute(&pool)
            .await
            .unwrap();
        scan_plex(&pool, &plex_scan_args()).await.unwrap();
        plex_page.assert_calls_async(0).await;
        let preference: i64 =
            sqlx::query_scalar(r#"SELECT "ShouldSyncItems" FROM "PlexLibrary" WHERE "Id" = 5"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(preference, 1);
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
