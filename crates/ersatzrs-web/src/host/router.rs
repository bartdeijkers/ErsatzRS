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

#[cfg(feature = "development-routes")]
use crate::transport::dev_stream;
use crate::{
    admin::app::{shell, App},
    api,
    host::assets::{resolve_pkg_dir, resolve_public_dir},
    host::state::AppState,
    transport::{hdhr, iptv},
};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};

#[allow(clippy::too_many_lines)]
pub fn build_app(app_state: AppState) -> Router {
    let conf = super::config::leptos_configuration();
    let leptos_options = conf.leptos_options;
    let routes = web_routes(super::config::development_routes_enabled());
    let pkg_dir = resolve_pkg_dir(&leptos_options);
    let public_dir = resolve_public_dir(&leptos_options);
    let output_name = leptos_options.output_name.clone();

    let leptos_options_clone = leptos_options.clone();
    let app_state_for_shell = app_state.clone();
    let app_state_for_leptos = app_state.clone();
    let mut leptos_router: Router = Router::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            move || provide_context(app_state_for_leptos.clone()),
            move || shell(leptos_options_clone.clone(), app_state_for_shell.clone()),
        )
        .with_state(leptos_options);

    if let Some(public_dir) = public_dir.clone() {
        tracing::info!(public_dir = %public_dir.display(), "serving web public assets");
        leptos_router = leptos_router.nest_service("/public", ServeDir::new(public_dir));
    } else {
        tracing::warn!(
            "web public assets not found; admin styling and static images are unavailable"
        );
    }

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

    let api_router: Router =
        Router::new()
            .route("/healthz", get(super::bootstrap::healthz))
            .route(
                "/api/addons/package",
                post(api::addons::upload_addon_package).layer(DefaultBodyLimit::max(
                    api::addons::MAX_ADDON_UPLOAD_BYTES + 1024 * 1024,
                )),
            )
            .route(
                "/api/addon-media-lists/artwork",
                post(api::addon_media_lists::upload_artwork).layer(DefaultBodyLimit::max(
                    api::addon_media_lists::MAX_ARTWORK_REQUEST_BYTES,
                )),
            )
            .route(
                "/api/addon-media-lists/artwork/{upload_id}",
                get(api::addon_media_lists::uploaded_artwork),
            )
            .route(
                "/api/addons/repository-bundle",
                post(api::addons::upload_repository_bundle)
                    .layer(DefaultBodyLimit::max(512 * 1024 * 1024 + 1024 * 1024)),
            )
            .route(
                "/api/addons/repository-bundle/remove",
                post(api::addons::remove_repository_bundle),
            )
            .route(
                "/api/addons/installed-icon/{addon_id}",
                get(api::addons::installed_icon),
            )
            .route(
                "/api/addons/source-icon/{source_id}/{addon_id}",
                get(api::addons::source_icon),
            )
            .route(
                "/api/blocks/items/save",
                post(api::blocks::save_block_items_form),
            )
            .route(
                "/api/collections/save",
                post(api::collections::save_collection_form),
            )
            .route(
                "/api/search/quick-filters",
                get(api::search::apply_quick_filters),
            )
            .route(
                "/api/saved-searches/load",
                post(api::search::load_saved_search_form),
            )
            .route(
                "/api/saved-searches/save",
                post(api::search::save_saved_search_form),
            )
            .route(
                "/api/smart-collections/save",
                post(api::collections::save_smart_collection_form),
            )
            .route(
                "/api/multi-collections/save",
                post(api::collections::save_multi_collection_form),
            )
            .route(
                "/api/multi-collections/delete",
                post(api::collections::delete_multi_collection_form),
            )
            .route("/api/channels/save", post(api::channels::save_channel_form))
            .route(
                "/api/channels/numbers/save",
                post(api::channels::save_channel_numbers_form),
            )
            .route(
                "/api/channels/save-multipart",
                post(api::channels::save_channel_multipart_form)
                    .layer(DefaultBodyLimit::max(api::watermarks::max_upload_bytes())),
            )
            .route(
                "/api/channels/logo",
                post(api::channels::upload_channel_logo)
                    .layer(DefaultBodyLimit::max(api::watermarks::max_upload_bytes())),
            )
            .route(
                "/api/ffmpeg/save",
                post(api::ffmpeg_profiles::save_ffmpeg_profile_form),
            )
            .route(
                "/api/settings/ffmpeg/save",
                post(api::settings::save_ffmpeg_settings_form),
            )
            .route(
                "/api/settings/ffmpeg/resolutions/add",
                post(api::settings::add_custom_resolution_form),
            )
            .route(
                "/api/settings/ffmpeg/resolutions/delete",
                post(api::settings::delete_custom_resolution_form),
            )
            .route(
                "/api/settings/logging/save",
                post(api::settings::save_logging_settings_form),
            )
            .route(
                "/api/settings/hdhr/save",
                post(api::settings::save_hdhr_settings_form),
            )
            .route(
                "/api/settings/scanner/save",
                post(api::settings::save_scanner_settings_form),
            )
            .route(
                "/api/settings/playout/save",
                post(api::settings::save_playout_settings_form),
            )
            .route(
                "/api/settings/ui/save",
                post(api::settings::save_ui_settings_form),
            )
            .route(
                "/api/settings/ui/windows-startup",
                post(api::settings::save_windows_startup_form),
            )
            .route("/api/settings/ui/theme", get(api::settings::ui_theme))
            .route(
                "/api/settings/xmltv/save",
                post(api::settings::save_xmltv_settings_form),
            )
            .route(
                "/api/filler-presets/save",
                post(api::filler::save_filler_preset_form),
            )
            .route(
                "/api/filler-presets/delete",
                post(api::filler::delete_filler_preset_form),
            )
            .route(
                "/api/playlists/groups/create",
                post(api::playlists::create_playlist_group_form),
            )
            .route(
                "/api/playlists/create",
                post(api::playlists::create_playlist_form),
            )
            .route(
                "/api/playlists/groups/delete",
                post(api::playlists::delete_playlist_group_form),
            )
            .route(
                "/api/playlists/delete",
                post(api::playlists::delete_playlist_form),
            )
            .route(
                "/api/playlists/save",
                post(api::playlists::save_playlist_form),
            )
            .route(
                "/api/playouts/create",
                post(api::playouts::create_playout_form),
            )
            .route(
                "/api/rerun-collections/save",
                post(api::rerun_collections::save_rerun_collection_form),
            )
            .route(
                "/api/rerun-collections/delete",
                post(api::rerun_collections::delete_rerun_collection_form),
            )
            .route(
                "/api/schedules/save",
                post(api::schedules::save_schedule_form),
            )
            .route(
                "/api/schedules/delete",
                post(api::schedules::delete_schedule_form),
            )
            .route(
                "/api/schedules/copy",
                post(api::schedules::copy_schedule_form),
            )
            .route(
                "/api/schedules/items/save",
                post(api::schedules::save_schedule_items_form),
            )
            .route(
                "/api/templates/groups/create",
                post(api::templates::create_template_group_form),
            )
            .route(
                "/api/templates/create",
                post(api::templates::create_template_form),
            )
            .route(
                "/api/templates/groups/delete",
                post(api::templates::delete_template_group_form),
            )
            .route(
                "/api/templates/delete",
                post(api::templates::delete_template_form),
            )
            .route(
                "/api/templates/copy",
                post(api::templates::copy_template_form),
            )
            .route(
                "/api/templates/items/save",
                post(api::templates::save_template_items_form),
            )
            .route(
                "/api/decos/groups/create",
                post(api::decos::create_deco_group_form),
            )
            .route("/api/decos/create", post(api::decos::create_deco_form))
            .route("/api/decos/save", post(api::decos::save_deco_form))
            .route(
                "/api/decos/groups/delete",
                post(api::decos::delete_deco_group_form),
            )
            .route("/api/decos/delete", post(api::decos::delete_deco_form))
            .route(
                "/api/deco-templates/groups/create",
                post(api::decos::create_deco_template_group_form),
            )
            .route(
                "/api/deco-templates/create",
                post(api::decos::create_deco_template_form),
            )
            .route(
                "/api/deco-templates/groups/delete",
                post(api::decos::delete_deco_template_group_form),
            )
            .route(
                "/api/deco-templates/delete",
                post(api::decos::delete_deco_template_form),
            )
            .route(
                "/api/deco-templates/items/save",
                post(api::decos::save_deco_template_items_form),
            )
            .route(
                "/media/addon-lists/{id}",
                post(crate::admin::pages::media::addon_lists::workspace::actions::post)
                    .layer(DefaultBodyLimit::max(
                    crate::admin::pages::media::addon_lists::workspace::actions::MAX_REQUEST_BYTES,
                )),
            )
            .route(
                "/media/addon-lists/{id}/settings",
                post(crate::admin::pages::media::addon_lists::workspace::actions::post)
                    .layer(DefaultBodyLimit::max(
                    crate::admin::pages::media::addon_lists::workspace::actions::MAX_REQUEST_BYTES,
                )),
            )
            .route(
                "/media/addon-lists/{id}/playback",
                post(crate::admin::pages::media::addon_lists::workspace::actions::post)
                    .layer(DefaultBodyLimit::max(
                    crate::admin::pages::media::addon_lists::workspace::actions::MAX_REQUEST_BYTES,
                )),
            )
            .route(
                "/media/addon-lists/{id}/activity",
                post(crate::admin::pages::media::addon_lists::workspace::actions::post)
                    .layer(DefaultBodyLimit::max(
                    crate::admin::pages::media::addon_lists::workspace::actions::MAX_REQUEST_BYTES,
                )),
            )
            .route(
                "/media/addon-lists/{id}/artwork",
                post(crate::admin::pages::media::addon_lists::workspace::actions::post)
                    .layer(DefaultBodyLimit::max(
                    crate::admin::pages::media::addon_lists::workspace::actions::MAX_REQUEST_BYTES,
                )),
            )
            .route(
                "/media/addon-lists/{id}/items/{item_id}",
                post(crate::admin::pages::media::addon_lists::workspace::actions::post)
                    .layer(DefaultBodyLimit::max(
                    crate::admin::pages::media::addon_lists::workspace::actions::MAX_REQUEST_BYTES,
                )),
            )
            .route(
                "/api/addon-lists/add",
                post(api::addon_media_lists::add_addon_media_list_form),
            )
            .route(
                "/api/addon-lists/disk-warning",
                post(api::addon_media_lists::save_addon_media_list_disk_warning_form)
                    .layer(DefaultBodyLimit::max(4096)),
            )
            .route(
                "/api/addon-lists/options/test-access",
                post(api::addon_list_options::test_access).layer(DefaultBodyLimit::max(4096)),
            )
            .route(
                "/api/addon-lists/options/cancel-access",
                post(api::addon_list_options::cancel_access).layer(DefaultBodyLimit::max(4096)),
            )
            .route(
                "/api/addon-lists/options/edit",
                post(api::addon_list_options::edit).layer(DefaultBodyLimit::max(
                    crate::admin::server_fns::addon_list_options::MAX_OPTION_FORM_BYTES,
                )),
            )
            .route(
                "/api/addon-lists/options/change",
                post(api::addon_list_options::change).layer(DefaultBodyLimit::max(
                    crate::admin::server_fns::addon_list_options::MAX_OPTION_FORM_BYTES,
                )),
            )
            .route(
                "/api/addon-lists/options/apply",
                post(api::addon_list_options::apply).layer(DefaultBodyLimit::max(
                    crate::admin::server_fns::addon_list_options::MAX_OPTION_FORM_BYTES,
                )),
            )
            .route(
                "/api/addon-lists/options/undo",
                post(api::addon_list_options::undo).layer(DefaultBodyLimit::max(
                    crate::admin::server_fns::addon_list_options::MAX_OPTION_FORM_BYTES,
                )),
            )
            .route(
                "/api/addon-lists/match",
                post(api::addon_media_lists::match_addon_media_list_form),
            )
            .route(
                "/api/addon-lists/save",
                post(api::addon_media_lists::save_addon_media_list_form),
            )
            .route(
                "/api/addon-lists/publish",
                post(api::addon_media_lists::publish_addon_media_list_form),
            )
            .route(
                "/api/addon-lists/recover",
                post(api::addon_media_lists::recover_addon_media_list_publication_form),
            )
            .route(
                "/api/addon-lists/reset",
                post(api::addon_media_lists::reset_addon_media_list_form),
            )
            .route(
                "/api/addon-lists/reconcile",
                post(api::addon_media_lists::reconcile_addon_media_list_form),
            )
            .route(
                "/api/addon-lists/confirm-removal",
                post(api::addon_media_lists::confirm_addon_media_list_removal_form),
            )
            .route(
                "/api/addon-lists/test-source",
                post(api::addon_media_lists::test_addon_media_list_source_form),
            )
            .route(
                "/api/addon-lists/restore-revision",
                post(api::addon_media_lists::restore_addon_media_list_revision_form),
            )
            .route(
                "/api/addon-lists/{id}/publication-diagnostics",
                get(api::addon_media_lists::download_addon_media_list_publication_diagnostics),
            )
            .route(
                "/api/addon-lists/items/review",
                post(api::addon_media_lists::save_addon_media_list_item_review_form),
            )
            .route(
                "/api/addon-lists/delete",
                post(api::addon_media_lists::delete_addon_media_list_form),
            )
            .route(
                "/api/media-sources/local/save",
                post(api::media_sources::save_local_library_form),
            )
            .route(
                "/api/media/actions/collections",
                get(api::media_actions::list_collections),
            )
            .route(
                "/api/media/actions/collections/add",
                post(api::media_actions::add_to_collection),
            )
            .route(
                "/api/media/actions/playlist-groups",
                get(api::media_actions::list_playlist_groups),
            )
            .route(
                "/api/media/actions/playlists",
                get(api::media_actions::list_playlists),
            )
            .route(
                "/api/media/actions/playlists/add",
                post(api::media_actions::add_to_playlist),
            )
            .route(
                "/api/media/actions/media-info/{id}",
                get(api::media_actions::media_info),
            )
            .route(
                "/api/troubleshoot/playback/archive",
                get(api::troubleshooting::playback_archive),
            )
            .route(
                "/api/troubleshoot/playback.m3u8",
                get(api::troubleshooting::playback_preview),
            )
            .route(
                "/api/troubleshoot/playback/sample/{media_item_id}",
                get(api::troubleshooting::playback_sample),
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
            .route(
                "/artwork/thumbnails/{*path}",
                get(api::artwork::serve_thumbnail),
            )
            .merge(build_iptv_router(&app_state))
            .route("/discover.json", get(hdhr::get_discover_json))
            .route("/device.xml", get(hdhr::get_device_xml))
            .route("/lineup_status.json", get(hdhr::get_lineup_status_json))
            .route("/lineup.json", get(hdhr::get_lineup_json))
            .with_state(app_state);

    let mut root = Router::new()
        .merge(api_router)
        .merge(leptos_router)
        .layer(TraceLayer::new_for_http());

    #[cfg(feature = "development-routes")]
    if super::config::development_routes_enabled() {
        tracing::warn!(
            "development-only routes are enabled; do not expose this server to untrusted clients"
        );
        root = root.route("/dev/stream/bbb.ts", get(dev_stream::stream_big_buck_bunny));
    }

    if let Some(public_dir) = public_dir {
        root = root.route_service(
            "/favicon.ico",
            ServeFile::new(public_dir.join("images").join("ersatzrs.png")),
        );
    }

    root
}

fn web_routes(development_routes_enabled: bool) -> Vec<leptos_axum::AxumRouteListing> {
    let mut routes = generate_route_list(App);
    if !development_routes_enabled {
        routes.retain(|route| route.path() != "/dev/components");
    }
    routes
}

fn build_iptv_router(app_state: &AppState) -> Router<AppState> {
    let iptv_routes = Router::new()
        .route("/iptv/channels.m3u", get(iptv::playlist::get_channels_m3u))
        .route("/iptv/xmltv.xml", get(iptv::guide::get_xmltv_xml))
        .route("/iptv/logos/gen", get(iptv::logos::get_generated_logo))
        .route(
            "/iptv/logos/{filename}",
            get(api::channels::serve_channel_logo),
        )
        .route(
            "/iptv/preview/{channel_number}/master.m3u8",
            get(iptv::hls::get_preview_master_playlist),
        )
        .route(
            "/iptv/preview/{channel_number}/hls.m3u8",
            get(iptv::hls::get_preview_session_playlist),
        )
        .route(
            "/iptv/preview/{channel_number}/{filename}",
            get(iptv::hls::get_preview_session_segment),
        )
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
        ));

    let internal_ffmpeg_routes = Router::new()
        .route(
            "/ffmpeg/next-media/{item}/{version}/{file}",
            get(iptv::next_media::get_media),
        )
        .route(
            "/ffmpeg/concat/{channel_number}",
            get(iptv::get_concat_playlist),
        )
        .route(
            "/ffmpeg/stream/{channel_number}",
            get(iptv::get_internal_stream),
        );

    iptv_routes.merge(internal_ffmpeg_routes)
}

#[cfg(test)]
mod tests {
    use super::web_routes;

    #[tokio::test(flavor = "current_thread")]
    async fn component_gallery_registration_follows_development_opt_in() {
        tokio::task::LocalSet::new()
            .run_until(async {
                assert!(!web_routes(false)
                    .iter()
                    .any(|route| route.path() == "/dev/components"));
                assert!(web_routes(true)
                    .iter()
                    .any(|route| route.path() == "/dev/components"));
            })
            .await;
    }
}
