use anyhow::Context;
use clap::{Parser, ValueEnum};
use ersatzrs_core::repositories::{EmbyConnectionConfig, JellyfinConnectionConfig};
use ersatzrs_infra::db;
use ersatzrs_infra::emby::{sync_emby_movie_library, sync_emby_show_library, EmbyService};
use ersatzrs_infra::jellyfin::{
    sync_jellyfin_movie_library, sync_jellyfin_show_library, JellyfinService,
};
use ersatzrs_infra::plex::{
    sync_plex_movie_library, sync_plex_show_library, PlexConnectionConfig, PlexService,
};
use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::{Row, SqlitePool};
use tracing::info;

#[derive(Debug, Parser)]
#[command(about = "Run one ErsatzRS media library scan")]
struct Args {
    #[arg(long, value_enum)]
    provider: Provider,
    #[arg(long)]
    library_id: i64,
    #[arg(long, default_value = "ffprobe")]
    ffprobe_path: String,
    #[arg(long)]
    dry_run: bool,
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
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let pool = db::init(&database_url)
        .await
        .context("initialize sqlite database")?;

    match args.provider {
        Provider::Local => scan_local(&pool, &args).await?,
        Provider::Jellyfin => scan_jellyfin(&pool, &args).await?,
        Provider::Plex => scan_plex(&pool, &args).await?,
        Provider::Emby => scan_emby(&pool, &args).await?,
    }
    Ok(())
}

async fn scan_local(pool: &SqlitePool, args: &Args) -> anyhow::Result<()> {
    if args.dry_run {
        info!(library_id = args.library_id, provider = "local", "dry run");
        return Ok(());
    }
    let report = scan_local_library(
        pool,
        &LocalScanConfig {
            library_id: args.library_id,
            ffprobe_path: args.ffprobe_path.clone(),
        },
    )
    .await?;
    info!(scanned_files = report.scanned_files, "local scan complete");
    Ok(())
}

async fn scan_jellyfin(pool: &SqlitePool, args: &Args) -> anyhow::Result<()> {
    let config = load_jellyfin_config(pool, args.library_id).await?;
    let media_source_id = load_library_media_source_id(pool, args.library_id).await?;
    let parent_id =
        load_provider_library_key(pool, "JellyfinLibrary", "ItemId", args.library_id).await?;
    let kind = load_library_media_kind(pool, args.library_id).await?;
    if args.dry_run {
        info!(
            library_id = args.library_id,
            provider = "jellyfin",
            parent_id,
            "dry run"
        );
        return Ok(());
    }
    let service = JellyfinService::new();
    let count = if kind == 1 {
        sync_jellyfin_show_library(
            pool,
            &service,
            &config,
            media_source_id,
            args.library_id,
            &parent_id,
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
        )
        .await?
    };
    info!(count, provider = "jellyfin", "scan complete");
    Ok(())
}

async fn scan_plex(pool: &SqlitePool, args: &Args) -> anyhow::Result<()> {
    let config = load_plex_config(pool, args.library_id).await?;
    let media_source_id = load_library_media_source_id(pool, args.library_id).await?;
    let library_key =
        load_provider_library_key(pool, "PlexLibrary", "Key", args.library_id).await?;
    let kind = load_library_media_kind(pool, args.library_id).await?;
    if args.dry_run {
        info!(
            library_id = args.library_id,
            provider = "plex",
            library_key,
            "dry run"
        );
        return Ok(());
    }
    let service = PlexService::new();
    let count = if kind == 1 {
        sync_plex_show_library(
            pool,
            &service,
            &config,
            media_source_id,
            args.library_id,
            &library_key,
        )
        .await?
    } else {
        sync_plex_movie_library(
            pool,
            &service,
            &config,
            media_source_id,
            args.library_id,
            &library_key,
        )
        .await?
    };
    info!(count, provider = "plex", "scan complete");
    Ok(())
}

async fn scan_emby(pool: &SqlitePool, args: &Args) -> anyhow::Result<()> {
    let config = load_emby_config(pool, args.library_id).await?;
    let media_source_id = load_library_media_source_id(pool, args.library_id).await?;
    let parent_id =
        load_provider_library_key(pool, "EmbyLibrary", "ItemId", args.library_id).await?;
    let kind = load_library_media_kind(pool, args.library_id).await?;
    if args.dry_run {
        info!(
            library_id = args.library_id,
            provider = "emby",
            parent_id,
            "dry run"
        );
        return Ok(());
    }
    let service = EmbyService::new();
    let count = if kind == 1 {
        sync_emby_show_library(
            pool,
            &service,
            &config,
            media_source_id,
            args.library_id,
            &parent_id,
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
        )
        .await?
    };
    info!(count, provider = "emby", "scan complete");
    Ok(())
}

async fn load_library_media_source_id(pool: &SqlitePool, library_id: i64) -> anyhow::Result<i64> {
    sqlx::query(r#"SELECT "MediaSourceId" FROM "Library" WHERE "Id" = ?"#)
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .map(|row| row.get("MediaSourceId"))
        .context("library not found")
}

async fn load_library_media_kind(pool: &SqlitePool, library_id: i64) -> anyhow::Result<i64> {
    sqlx::query(r#"SELECT "MediaKind" FROM "Library" WHERE "Id" = ?"#)
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .map(|row| row.get("MediaKind"))
        .context("library not found")
}

async fn load_provider_library_key(
    pool: &SqlitePool,
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
    pool: &SqlitePool,
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
    let authorization_header = read_env("JELLYFIN_AUTHORIZATION_HEADER")
        .context("JELLYFIN_AUTHORIZATION_HEADER is required")?;
    Ok(JellyfinConnectionConfig {
        address,
        authorization_header,
    })
}

async fn load_emby_config(
    pool: &SqlitePool,
    library_id: i64,
) -> anyhow::Result<EmbyConnectionConfig> {
    let media_source_id = load_library_media_source_id(pool, library_id).await?;
    let address =
        load_connection_address(pool, "EmbyConnection", "EmbyMediaSourceId", media_source_id)
            .await?;
    let api_key = read_env("EMBY_API_KEY").context("EMBY_API_KEY is required")?;
    Ok(EmbyConnectionConfig { address, api_key })
}

async fn load_plex_config(
    pool: &SqlitePool,
    library_id: i64,
) -> anyhow::Result<PlexConnectionConfig> {
    let media_source_id = load_library_media_source_id(pool, library_id).await?;
    let uri = sqlx::query(
        r#"SELECT "Uri"
           FROM "PlexConnection"
           WHERE "PlexMediaSourceId" = ? AND "IsActive" = 1
           ORDER BY "Id"
           LIMIT 1"#,
    )
    .bind(media_source_id)
    .fetch_optional(pool)
    .await?
    .and_then(|row| row.get::<Option<String>, _>("Uri"))
    .context("active Plex connection not found")?;
    let token = read_env("PLEX_TOKEN").context("PLEX_TOKEN is required")?;
    Ok(PlexConnectionConfig { uri, token })
}

async fn load_connection_address(
    pool: &SqlitePool,
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
fn read_env(name: &str) -> Result<String, std::env::VarError> {
    std::env::var(name)
}
