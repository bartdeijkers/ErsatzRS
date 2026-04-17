mod app;
mod db;
mod hdhr;
mod iptv;
mod state;
mod streaming;

use app::{shell, App};
use axum::{http::StatusCode, routing::get, Router};
use ersatzrs_core::repositories::{ConfigRepository, RepositoryError};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use sqlx::SqlitePool;
use state::AppState;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const LISTEN_ADDR: &str = "0.0.0.0:8080";
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

    let conf = get_configuration(None).expect("valid leptos config");
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let leptos_options_clone = leptos_options.clone();
    let leptos_router: Router = Router::new()
        .leptos_routes(&leptos_options, routes, move || {
            shell(leptos_options_clone.clone())
        })
        .with_state(leptos_options);

    let api_router: Router = Router::new()
        .route("/iptv/channels.m3u", get(iptv::playlist::get_channels_m3u))
        .route("/iptv/xmltv.xml", get(iptv::guide::get_xmltv_xml))
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

    Ok(AppState::new(pool, device_uuid, tuner_count))
}

fn sqlx_repo_error(error: &RepositoryError) -> sqlx::Error {
    sqlx::Error::Protocol(error.to_string())
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}
