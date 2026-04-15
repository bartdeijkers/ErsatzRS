mod app;
mod ffmpeg;

use app::{shell, App};
use axum::{http::StatusCode, routing::get, Router};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

const LISTEN_ADDR: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    match ffmpeg::check_ffmpeg_version().await {
        Ok(version) => tracing::info!("FFmpeg 8.1+ detected (found: {version})"),
        Err(e) => {
            tracing::error!(error = %e, "FFmpeg version check failed");
            std::process::exit(1);
        }
    }

    let conf = get_configuration(None).expect("valid leptos config");
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let leptos_options_clone = leptos_options.clone();
    // LeptosRoutes requires `LeptosOptions: FromRef<S>`, which holds when S = LeptosOptions.
    // Build a Router<LeptosOptions> by calling with_state first on a stub, then merge.
    let leptos_router: Router<LeptosOptions> =
        Router::new().leptos_routes(&leptos_options, routes, move || {
            shell(leptos_options_clone.clone())
        });

    let app: Router = Router::new()
        .route("/healthz", get(healthz))
        .merge(leptos_router)
        .layer(TraceLayer::new_for_http())
        .with_state(leptos_options);

    tracing::info!("nastv-web listening on {LISTEN_ADDR}");

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
