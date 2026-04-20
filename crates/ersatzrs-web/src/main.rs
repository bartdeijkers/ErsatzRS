#![recursion_limit = "512"]

mod app;
mod db;
mod hdhr;
mod iptv;
mod pages;
mod server;
mod state;
mod streaming;
mod ui;
mod watermark_upload;

use app::{shell, App};
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    middleware,
    routing::{get, post},
    Router,
};
use ersatzrs_core::repositories::{ConfigRepository, RepositoryError};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use sqlx::SqlitePool;
use state::AppState;
use state::IptvAccessTokenVerifier;
use std::path::{Path, PathBuf};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const WEB_CRATE_CARGO_TOML: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
const WEB_PUBLIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/public");
const WEB_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");
const LISTEN_ADDR: &str = "0.0.0.0:8409";
const LISTEN_PORT: u16 = 8409;
const CONFIG_KEY_HDHR_UUID: &str = "HDHR.DeviceUuid";
const CONFIG_KEY_HDHR_TUNER_COUNT: &str = "HDHR.TunerCount";
const DEFAULT_TUNER_COUNT: i32 = 1;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    match ersatzrs_ffmpeg::check_ffmpeg_version().await {
        Ok(version) => tracing::info!("FFmpeg 8.1+ detected (found: {version})"),
        Err(e) => {
            tracing::error!(error = %e, "FFmpeg version check failed");
            std::process::exit(1);
        }
    }

    #[allow(clippy::disallowed_methods)] // Phase 0: config crate not yet available
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://ErsatzRS.db".to_string());
    let pool = db::init(&db_url)
        .await
        .expect("database initialisation failed");
    tracing::info!(database_url = %db_url, "database initialised");

    let app_state = build_app_state(pool)
        .await
        .expect("failed to build application state");

    tokio::spawn(hdhr::run_udp_discovery(
        app_state.device_uuid,
        app_state.tuner_count,
        LISTEN_PORT,
    ));

    let conf = get_configuration(Some(WEB_CRATE_CARGO_TOML)).expect("valid leptos config");
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);
    let pkg_dir = resolve_pkg_dir(&leptos_options);

    let leptos_options_clone = leptos_options.clone();
    let app_state_for_leptos = app_state.clone();
    let mut leptos_router: Router = Router::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            move || provide_context(app_state_for_leptos.clone()),
            move || shell(leptos_options_clone.clone()),
        )
        .nest_service("/public", ServeDir::new(WEB_PUBLIC_DIR))
        .with_state(leptos_options);

    if let Some(pkg_dir) = pkg_dir {
        tracing::info!(pkg_dir = %pkg_dir.display(), "serving Leptos hydration assets");
        leptos_router = leptos_router.nest_service("/pkg", ServeDir::new(pkg_dir));
    } else {
        tracing::warn!(
            "Leptos pkg assets not found; serving SSR HTML only without browser hydration"
        );
    }

    let api_router: Router = Router::new()
        .route(
            "/api/watermarks/image",
            post(watermark_upload::upload_watermark_image)
                .layer(DefaultBodyLimit::max(watermark_upload::max_upload_bytes())),
        )
        .route(
            "/artwork/watermarks/:filename",
            get(watermark_upload::serve_watermark_image),
        )
        .merge(build_iptv_router(&app_state))
        .route("/discover.json", get(hdhr::get_discover_json))
        .route("/device.xml", get(hdhr::get_device_xml))
        .route("/lineup_status.json", get(hdhr::get_lineup_status_json))
        .route("/lineup.json", get(hdhr::get_lineup_json))
        .with_state(app_state);

    let app: Router = Router::new()
        .route("/healthz", get(healthz))
        .route(
            "/dev/stream/bbb.ts",
            get(|| async { streaming::stream_big_buck_bunny() }),
        )
        .merge(api_router)
        .merge(leptos_router)
        .layer(TraceLayer::new_for_http());

    tracing::info!("ersatzrs-web listening on {LISTEN_ADDR}");

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR)
        .await
        .unwrap_or_else(|e| panic!("failed to bind to {LISTEN_ADDR}: {e}"));

    axum::serve(listener, app.into_make_service())
        .await
        .expect("server error");
}

async fn build_app_state(pool: SqlitePool) -> Result<AppState, sqlx::Error> {
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let iptv_access_token_verifier =
        IptvAccessTokenVerifier::from_optional_secret(load_iptv_jwt_signing_key());

    let device_uuid = if let Some(value) = repositories
        .config
        .get(CONFIG_KEY_HDHR_UUID)
        .await
        .map_err(|error| sqlx_repo_error(&error))?
    {
        Uuid::parse_str(value.trim()).unwrap_or_else(|_| Uuid::new_v4())
    } else {
        let new_uuid = Uuid::new_v4();
        repositories
            .config
            .set(CONFIG_KEY_HDHR_UUID, &new_uuid.to_string())
            .await
            .map_err(|error| sqlx_repo_error(&error))?;
        tracing::info!(device_uuid = %new_uuid, "seeded HDHomeRun device UUID");
        new_uuid
    };

    let tuner_count = repositories
        .config
        .get(CONFIG_KEY_HDHR_TUNER_COUNT)
        .await
        .map_err(|error| sqlx_repo_error(&error))?
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(DEFAULT_TUNER_COUNT);

    Ok(AppState::new(
        pool,
        device_uuid,
        tuner_count,
        iptv_access_token_verifier,
    ))
}

fn load_iptv_jwt_signing_key() -> Option<String> {
    #[allow(clippy::disallowed_methods)]
    std::env::var("JWT__IssuerSigningKey")
        .ok()
        .or_else(|| std::env::var("JWT:IssuerSigningKey").ok())
}

fn build_iptv_router(app_state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/iptv/channels.m3u", get(iptv::playlist::get_channels_m3u))
        .route("/iptv/xmltv.xml", get(iptv::guide::get_xmltv_xml))
        .route("/iptv/channel/:file", get(iptv::dispatch_channel_request))
        .route(
            "/iptv/session/:channel_number/hls.m3u8",
            get(iptv::hls::get_session_playlist),
        )
        .route(
            "/iptv/session/:channel_number/:filename",
            get(iptv::hls::get_session_segment),
        )
        .route(
            "/iptv/hls-direct/:file",
            get(iptv::hls::get_hls_direct_stream),
        )
        .route_layer(middleware::from_fn_with_state(
            app_state.clone(),
            iptv::access_token::require_iptv_access,
        ))
}

fn sqlx_repo_error(error: &RepositoryError) -> sqlx::Error {
    sqlx::Error::Protocol(error.to_string())
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

fn resolve_pkg_dir(options: &LeptosOptions) -> Option<PathBuf> {
    let site_root = options.site_root.to_string();
    let pkg_dir = options.site_pkg_dir.to_string();
    candidate_site_roots(site_root)
        .into_iter()
        .map(|root| root.join(&pkg_dir))
        .find(|candidate| candidate.is_dir())
}

fn candidate_site_roots(site_root: impl AsRef<Path>) -> Vec<PathBuf> {
    let site_root = site_root.as_ref();
    let configured = PathBuf::from(site_root);
    if configured.is_absolute() {
        return vec![configured];
    }

    vec![
        Path::new(WEB_CRATE_DIR).join(site_root),
        Path::new(WEB_CRATE_DIR)
            .join("..")
            .join("..")
            .join(site_root),
        configured,
    ]
}
