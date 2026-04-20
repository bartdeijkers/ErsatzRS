#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::{
    components::{Route, Router, Routes},
    ParamSegment, StaticSegment,
};
use std::path::{Path, PathBuf};

use crate::pages::channels::{editor::ChannelEditorPage, list::ChannelsListPage};
use crate::pages::dev::ComponentsPage;
use crate::pages::ffmpeg_profiles::{
    editor::FfmpegProfileEditorPage, list::FfmpegProfilesListPage,
};
use crate::pages::libraries::LibrariesPage;
use crate::pages::media::artists::ArtistsPage;
use crate::pages::media::collections::{CollectionEditorPage, CollectionsPage};
use crate::pages::media::images::ImagesPage;
use crate::pages::media::movies::MoviesPage;
use crate::pages::media::other_videos::OtherVideosPage;
use crate::pages::media::shows::ShowsPage;
use crate::pages::media::smart_collections::{SmartCollectionEditorPage, SmartCollectionsPage};
use crate::pages::media::songs::SongsPage;
use crate::pages::media::trash::TrashPage;
use crate::pages::media_sources::{
    emby::{EmbyConnectionPage, EmbyLibrariesPage, EmbyPathsPage, EmbySourcesPage},
    jellyfin::{
        JellyfinConnectionPage, JellyfinLibrariesPage, JellyfinPathsPage, JellyfinSourcesPage,
    },
    local::{LocalLibrariesPage, LocalLibraryEditorPage},
    plex::{PlexLibrariesPage, PlexPathsPage, PlexSourcesPage},
};
use crate::pages::scheduling::block_editor::BlockEditorPage;
use crate::pages::scheduling::blocks::BlocksPage;
use crate::pages::scheduling::playouts::PlayoutsPage;
use crate::pages::scheduling::yaml_editor::YamlEditorPage;
use crate::pages::watermarks::{editor::WatermarkEditorPage, list::WatermarksListPage};
use crate::ui::layout::AppShell;
use crate::ui::primitives::toast::provide_toast_store;

stylance::import_crate_style!(style, "src/app.module.css");

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_toast_store();

    view! {
        <Router>
            <AppShell>
                <Routes fallback=|| {
                    view! {
                        <div class=style::not_found>
                            <p class=style::not_found_code>"404"</p>
                            <p class=style::not_found_msg>"Page not found."</p>
                            <a href="/" class=style::not_found_link>"Go home"</a>
                        </div>
                    }
                    .into_view()
                }>
                    <Route path=StaticSegment("") view=HomePage/>
                    <Route path=StaticSegment("dev/components") view=ComponentsPage/>
                    <Route path=StaticSegment("channels") view=ChannelsListPage/>
                    <Route
                        path=(StaticSegment("channels"), StaticSegment("add"))
                        view=ChannelEditorPage
                    />
                    <Route
                        path=(StaticSegment("channels"), ParamSegment("id"))
                        view=ChannelEditorPage
                    />

                    // ── FFmpeg Profiles ───────────────────────────────────────
                    <Route path=StaticSegment("ffmpeg") view=FfmpegProfilesListPage/>
                    <Route
                        path=(StaticSegment("ffmpeg"), StaticSegment("add"))
                        view=FfmpegProfileEditorPage
                    />
                    <Route
                        path=(StaticSegment("ffmpeg"), ParamSegment("id"))
                        view=FfmpegProfileEditorPage
                    />
                    <Route
                        path=(StaticSegment("ffmpeg"), StaticSegment("profiles"))
                        view=FfmpegProfilesListPage
                    />
                    <Route
                        path=(StaticSegment("ffmpeg"), StaticSegment("profiles"), StaticSegment("add"))
                        view=FfmpegProfileEditorPage
                    />
                    <Route
                        path=(StaticSegment("ffmpeg"), StaticSegment("profiles"), ParamSegment("id"))
                        view=FfmpegProfileEditorPage
                    />

                    // ── Watermarks ────────────────────────────────────────────
                    <Route path=StaticSegment("watermarks") view=WatermarksListPage/>
                    <Route
                        path=(StaticSegment("watermarks"), StaticSegment("add"))
                        view=WatermarkEditorPage
                    />
                    <Route
                        path=(StaticSegment("watermarks"), ParamSegment("id"))
                        view=WatermarkEditorPage
                    />

                    // ── Scheduling ────────────────────────────────────────────
                    <Route path=StaticSegment("playouts") view=PlayoutsPage/>
                    <Route
                        path=(StaticSegment("playouts"), StaticSegment("sequential"), ParamSegment("id"))
                        view=YamlEditorPage
                    />
                    <Route path=StaticSegment("blocks") view=BlocksPage/>
                    <Route
                        path=(StaticSegment("blocks"), ParamSegment("id"))
                        view=BlockEditorPage
                    />
                    <Route
                        path=(StaticSegment("scheduling"), StaticSegment("blocks"))
                        view=BlocksPage
                    />
                    <Route
                        path=(StaticSegment("scheduling"), StaticSegment("blocks"), ParamSegment("id"))
                        view=BlockEditorPage
                    />
                    // ── Libraries ────────────────────────────────────────────
                    <Route
                        path=(StaticSegment("media"), StaticSegment("libraries"))
                        view=LibrariesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("trash"))
                        view=TrashPage
                    />

                    // ── Media browsers ───────────────────────────────────────
                    <Route
                        path=(StaticSegment("media"), StaticSegment("movies"))
                        view=MoviesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("tv"), StaticSegment("shows"))
                        view=ShowsPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("shows"))
                        view=ShowsPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("images"))
                        view=ImagesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("browser"), StaticSegment("images"))
                        view=ImagesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("music"), StaticSegment("artists"))
                        view=ArtistsPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("other"), StaticSegment("videos"))
                        view=OtherVideosPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("music"), StaticSegment("songs"))
                        view=SongsPage
                    />

                    // ── Collections ──────────────────────────────────────────
                    <Route
                        path=(StaticSegment("media"), StaticSegment("collections"))
                        view=CollectionsPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("collections"), StaticSegment("add"))
                        view=CollectionEditorPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("collections"), ParamSegment("id"), StaticSegment("edit"))
                        view=CollectionEditorPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("collections"), ParamSegment("id"))
                        view=CollectionEditorPage
                    />

                    // ── Smart Collections ────────────────────────────────────
                    <Route
                        path=(StaticSegment("media"), StaticSegment("smart-collections"))
                        view=SmartCollectionsPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("smart-collections"), StaticSegment("add"))
                        view=SmartCollectionEditorPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("smart-collections"), ParamSegment("id"), StaticSegment("edit"))
                        view=SmartCollectionEditorPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("smart-collections"), ParamSegment("id"))
                        view=SmartCollectionEditorPage
                    />

                    // ── Local libraries ──────────────────────────────────────
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("local"))
                        view=LocalLibrariesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("local"), StaticSegment("add"))
                        view=LocalLibraryEditorPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("local"), ParamSegment("id"), StaticSegment("edit"))
                        view=LocalLibraryEditorPage
                    />

                    // ── Jellyfin ─────────────────────────────────────────────
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("jellyfin"))
                        view=JellyfinSourcesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("jellyfin"), StaticSegment("edit"))
                        view=JellyfinConnectionPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("jellyfin"), ParamSegment("id"), StaticSegment("libraries"))
                        view=JellyfinLibrariesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("jellyfin"), ParamSegment("id"), StaticSegment("paths"))
                        view=JellyfinPathsPage
                    />

                    // ── Emby ─────────────────────────────────────────────────
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("emby"))
                        view=EmbySourcesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("emby"), StaticSegment("edit"))
                        view=EmbyConnectionPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("emby"), ParamSegment("id"), StaticSegment("libraries"))
                        view=EmbyLibrariesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("emby"), ParamSegment("id"), StaticSegment("paths"))
                        view=EmbyPathsPage
                    />

                    // ── Plex ─────────────────────────────────────────────────
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("plex"))
                        view=PlexSourcesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("plex"), ParamSegment("id"), StaticSegment("libraries"))
                        view=PlexLibrariesPage
                    />
                    <Route
                        path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("plex"), ParamSegment("id"), StaticSegment("paths"))
                        view=PlexPathsPage
                    />
                </Routes>
            </AppShell>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <div class=style::page_home>
            <h1 class=style::page_home_title>"ErsatzRS"</h1>
            <p class=style::page_home_subtitle>"IPTV server is running."</p>
            <p>
                <a href="/channels" class=style::page_home_link>"→ Channels"</a>
            </p>
        </div>
    }
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    let hydration_assets_available = resolve_hydration_assets(&options).is_some();

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="UTF-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
                <meta name="color-scheme" content="dark"/>
                <title>"ErsatzRS"</title>
                <link rel="stylesheet" href="/public/bundle.css"/>
                {hydration_assets_available.then(|| {
                    view! { <HydrationScripts options=options.clone() islands=true/> }
                })}
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

fn resolve_hydration_assets(options: &LeptosOptions) -> Option<PathBuf> {
    let js_name = format!("{}.js", options.output_name);
    let wasm_name = format!("{}_bg.wasm", options.output_name);
    let site_root = options.site_root.to_string();
    let site_pkg_dir = options.site_pkg_dir.to_string();

    candidate_site_roots(site_root)
        .into_iter()
        .map(|root| root.join(&site_pkg_dir))
        .find(|pkg_dir| pkg_dir.join(&js_name).is_file() && pkg_dir.join(&wasm_name).is_file())
}

fn candidate_site_roots(site_root: impl AsRef<Path>) -> Vec<PathBuf> {
    let site_root = site_root.as_ref();
    let configured = PathBuf::from(site_root);
    if configured.is_absolute() {
        return vec![configured];
    }

    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        crate_dir.join(site_root),
        crate_dir.join("..").join("..").join(site_root),
        configured,
    ]
}
