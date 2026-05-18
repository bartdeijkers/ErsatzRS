use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::{
    admin::app::{shell, App},
    api,
    host::assets::resolve_pkg_dir,
    host::state::AppState,
    transport::{dev_stream, hdhr, iptv},
};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};

const WEB_PUBLIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/public");

pub fn build_app(app_state: AppState) -> Router {
    let conf = super::config::leptos_configuration();
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);
    let pkg_dir = resolve_pkg_dir(&leptos_options);
    let output_name = leptos_options.output_name.clone();

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
        let bg_wasm_name = format!("{output_name}_bg.wasm");
        let cargo_leptos_wasm_name = format!("{output_name}.wasm");
        let bg_wasm_path = pkg_dir.join(&bg_wasm_name);
        let cargo_leptos_wasm_path = pkg_dir.join(&cargo_leptos_wasm_name);
        if !bg_wasm_path.is_file() && cargo_leptos_wasm_path.is_file() {
            tracing::info!(
                from = %cargo_leptos_wasm_path.display(),
                to = %format!("/pkg/{bg_wasm_name}"),
                "serving cargo-leptos wasm output under wasm-bindgen filename"
            );
            leptos_router = leptos_router.route_service(
                &format!("/pkg/{bg_wasm_name}"),
                ServeFile::new(cargo_leptos_wasm_path),
            );
        }
        leptos_router = leptos_router.nest_service("/pkg", ServeDir::new(pkg_dir));
    } else {
        tracing::warn!(
            "Leptos pkg assets not found; admin actions that require browser hydration are disabled. Build assets with `(cd crates/ersatzrs-web && cargo leptos build)` before running ersatzrs-web."
        );
    }

    let api_router: Router = Router::new()
        .route("/healthz", get(super::bootstrap::healthz))
        .route("/api/channels/save", post(api::channels::save_channel_form))
        .route(
            "/api/ffmpeg/save",
            post(api::ffmpeg_profiles::save_ffmpeg_profile_form),
        )
        .route(
            "/api/media-sources/local/save",
            post(api::media_sources::save_local_library_form),
        )
        .route(
            "/api/watermarks/save",
            post(api::watermarks::save_watermark_form)
                .layer(DefaultBodyLimit::max(api::watermarks::max_upload_bytes())),
        )
        .route(
            "/api/watermarks/image",
            post(api::watermarks::upload_watermark_image)
                .layer(DefaultBodyLimit::max(api::watermarks::max_upload_bytes())),
        )
        .route(
            "/artwork/watermarks/{filename}",
            get(api::watermarks::serve_watermark_image),
        )
        .route("/artwork/posters/{*path}", get(api::artwork::serve_poster))
        .route("/artwork/fanart/{*path}", get(api::artwork::serve_fanart))
        .merge(build_iptv_router(&app_state))
        .route("/discover.json", get(hdhr::get_discover_json))
        .route("/device.xml", get(hdhr::get_device_xml))
        .route("/lineup_status.json", get(hdhr::get_lineup_status_json))
        .route("/lineup.json", get(hdhr::get_lineup_json))
        .with_state(app_state);

    Router::new()
        .route(
            "/dev/stream/bbb.ts",
            get(|| async { dev_stream::stream_big_buck_bunny() }),
        )
        .merge(api_router)
        .merge(leptos_router)
        .route_service(
            "/favicon.ico",
            ServeFile::new(format!("{WEB_PUBLIC_DIR}/images/ersatzrs.png")),
        )
        .layer(TraceLayer::new_for_http())
}

fn build_iptv_router(app_state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/iptv/channels.m3u", get(iptv::playlist::get_channels_m3u))
        .route("/iptv/xmltv.xml", get(iptv::guide::get_xmltv_xml))
        .route("/iptv/logos/gen", get(iptv::logos::get_generated_logo))
        .route("/iptv/channel/{file}", get(iptv::dispatch_channel_request))
        .route(
            "/iptv/session/{channel_number}/hls.m3u8",
            get(iptv::hls::get_session_playlist),
        )
        .route(
            "/iptv/session/{channel_number}/{filename}",
            get(iptv::hls::get_session_segment),
        )
        .route(
            "/iptv/hls-direct/{file}",
            get(iptv::hls::get_hls_direct_stream),
        )
        .route_layer(middleware::from_fn_with_state(
            app_state.clone(),
            iptv::access_token::require_iptv_access,
        ))
}
