mod app;
mod db;
mod iptv;
mod state;
mod streaming;

use app::{shell, App};
use axum::{http::StatusCode, routing::get, Router};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use state::AppState;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

const LISTEN_ADDR: &str = "0.0.0.0:8080";

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

    let app_state = AppState::new(pool);

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

async fn healthz() -> StatusCode {
    StatusCode::OK
}
