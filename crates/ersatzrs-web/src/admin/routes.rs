#![allow(clippy::too_many_lines)]

use leptos::prelude::*;
use leptos_router::{
    components::{Route, Routes},
    ParamSegment, StaticSegment,
};

use crate::admin::home::HomePage;
use crate::admin::pages::channels::{editor::ChannelEditorPage, list::ChannelsListPage};
use crate::admin::pages::dev::ComponentsPage;
use crate::admin::pages::ffmpeg_profiles::{
    editor::FfmpegProfileEditorPage, list::FfmpegProfilesListPage,
};
use crate::admin::pages::libraries::LibrariesPage;
use crate::admin::pages::media::artists::{ArtistPage, ArtistsPage};
use crate::admin::pages::media::collection_detail::CollectionDetailPage;
use crate::admin::pages::media::collections::{CollectionEditorPage, CollectionsPage};
use crate::admin::pages::media::filler_presets::{FillerPresetEditorPage, FillerPresetsPage};
use crate::admin::pages::media::images::ImagesPage;
use crate::admin::pages::media::movies::{MoviePage, MoviesPage};
use crate::admin::pages::media::multi_collections::{
    MultiCollectionEditorPage, MultiCollectionsPage,
};
use crate::admin::pages::media::other_videos::{OtherVideoPage, OtherVideosPage};
use crate::admin::pages::media::playlists::{PlaylistEditorPage, PlaylistsPage};
use crate::admin::pages::media::rerun_collections::{
    RerunCollectionEditorPage, RerunCollectionsPage,
};
use crate::admin::pages::media::season_detail::SeasonPage;
use crate::admin::pages::media::see_all_lists::{
    EpisodesListPage, MusicVideosListPage, RemoteStreamsListPage, SeasonsListPage,
};
use crate::admin::pages::media::shows::{ShowPage, ShowsPage};
use crate::admin::pages::media::smart_collections::{
    SmartCollectionEditorPage, SmartCollectionsPage,
};
use crate::admin::pages::media::songs::SongsPage;
use crate::admin::pages::media::trakt_lists::{TraktListEditorPage, TraktListsPage};
use crate::admin::pages::media::trash::TrashPage;
use crate::admin::pages::media_sources::{
    emby::{EmbyConnectionPage, EmbyLibrariesPage, EmbyPathsPage, EmbySourcesPage},
    jellyfin::{
        JellyfinConnectionPage, JellyfinLibrariesPage, JellyfinPathsPage, JellyfinSourcesPage,
    },
    local::{LocalLibrariesPage, LocalLibraryEditorPage},
    plex::{PlexLibrariesPage, PlexPathsPage, PlexSourcesPage},
};
use crate::admin::pages::scheduling::block_editor::BlockEditorPage;
use crate::admin::pages::scheduling::blocks::BlocksPage;
use crate::admin::pages::scheduling::playouts::PlayoutsPage;
use crate::admin::pages::scheduling::yaml_editor::YamlEditorPage;
use crate::admin::pages::search::SearchPage;
use crate::admin::pages::system::{
    health::HealthPage,
    logs::LogsPage,
    troubleshooting::{PlaybackTroubleshootingPage, TroubleshootingPage},
};
use crate::admin::pages::watermarks::{editor::WatermarkEditorPage, list::WatermarksListPage};

stylance::import_crate_style!(style, "src/admin/app.module.css");

#[component]
pub fn AdminRoutes() -> impl IntoView {
    view! {
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
            <Route path=(StaticSegment("channels"), ParamSegment("id")) view=ChannelEditorPage/>

            <Route path=StaticSegment("ffmpeg") view=FfmpegProfilesListPage/>
            <Route
                path=(StaticSegment("ffmpeg"), StaticSegment("add"))
                view=FfmpegProfileEditorPage
            />
            <Route
                path=(StaticSegment("ffmpeg"), StaticSegment("profiles"))
                view=FfmpegProfilesListPage
            />
            <Route
                path=(
                    StaticSegment("ffmpeg"),
                    StaticSegment("profiles"),
                    StaticSegment("add"),
                )
                view=FfmpegProfileEditorPage
            />
            <Route
                path=(StaticSegment("ffmpeg"), StaticSegment("profiles"), ParamSegment("id"))
                view=FfmpegProfileEditorPage
            />
            <Route
                path=(StaticSegment("ffmpeg"), ParamSegment("id"))
                view=FfmpegProfileEditorPage
            />

            <Route path=StaticSegment("watermarks") view=WatermarksListPage/>
            <Route
                path=(StaticSegment("watermarks"), StaticSegment("add"))
                view=WatermarkEditorPage
            />
            <Route
                path=(StaticSegment("watermarks"), ParamSegment("id"))
                view=WatermarkEditorPage
            />

            <Route path=StaticSegment("playouts") view=PlayoutsPage/>
            <Route
                path=(StaticSegment("playouts"), StaticSegment("sequential"), ParamSegment("id"))
                view=YamlEditorPage
            />
            <Route path=StaticSegment("blocks") view=BlocksPage/>
            <Route path=StaticSegment("search") view=SearchPage/>
            <Route
                path=(StaticSegment("system"), StaticSegment("health"))
                view=HealthPage
            />
            <Route path=(StaticSegment("system"), StaticSegment("logs")) view=LogsPage/>
            <Route
                path=(StaticSegment("system"), StaticSegment("troubleshooting"))
                view=TroubleshootingPage
            />
            <Route
                path=(
                    StaticSegment("system"),
                    StaticSegment("troubleshooting"),
                    StaticSegment("playback"),
                )
                view=PlaybackTroubleshootingPage
            />
            <Route path=(StaticSegment("blocks"), ParamSegment("id")) view=BlockEditorPage/>
            <Route path=(StaticSegment("scheduling"), StaticSegment("blocks")) view=BlocksPage/>
            <Route
                path=(StaticSegment("scheduling"), StaticSegment("blocks"), ParamSegment("id"))
                view=BlockEditorPage
            />

            <Route path=(StaticSegment("media"), StaticSegment("libraries")) view=LibrariesPage/>
            <Route path=(StaticSegment("media"), StaticSegment("trash")) view=TrashPage/>
            <Route path=(StaticSegment("media"), StaticSegment("movies")) view=MoviesPage/>
            <Route
                path=(StaticSegment("media"), StaticSegment("movies"), ParamSegment("id"))
                view=MoviePage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("tv"), StaticSegment("shows"))
                view=ShowsPage
            />
            <Route path=(StaticSegment("media"), StaticSegment("shows")) view=ShowsPage/>
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("tv"),
                    StaticSegment("shows"),
                    ParamSegment("id"),
                )
                view=ShowPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("shows"), ParamSegment("id"))
                view=ShowPage
            />
            <Route path=(StaticSegment("media"), StaticSegment("images")) view=ImagesPage/>
            <Route
                path=(StaticSegment("media"), StaticSegment("browser"), StaticSegment("images"))
                view=ImagesPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("music"), StaticSegment("artists"))
                view=ArtistsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("music"),
                    StaticSegment("artists"),
                    ParamSegment("id"),
                )
                view=ArtistPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("other"), StaticSegment("videos"))
                view=OtherVideosPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("other"),
                    StaticSegment("videos"),
                    ParamSegment("id"),
                )
                view=OtherVideoPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("music"), StaticSegment("songs"))
                view=SongsPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("tv"), StaticSegment("seasons"))
                view=SeasonsListPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("tv"),
                    StaticSegment("seasons"),
                    ParamSegment("id"),
                )
                view=SeasonPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("seasons"), ParamSegment("id"))
                view=SeasonPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("tv"), StaticSegment("episodes"))
                view=EpisodesListPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("music"), StaticSegment("videos"))
                view=MusicVideosListPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("remote"), StaticSegment("streams"))
                view=RemoteStreamsListPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("collections"))
                view=CollectionsPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("collections"), StaticSegment("add"))
                view=CollectionEditorPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("collections"),
                    ParamSegment("id"),
                    StaticSegment("edit"),
                )
                view=CollectionEditorPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("collections"), ParamSegment("id"))
                view=CollectionDetailPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("smart-collections"))
                view=SmartCollectionsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("smart-collections"),
                    StaticSegment("add"),
                )
                view=SmartCollectionEditorPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("smart-collections"),
                    ParamSegment("id"),
                    StaticSegment("edit"),
                )
                view=SmartCollectionEditorPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("smart-collections"),
                    ParamSegment("id"),
                )
                view=SmartCollectionEditorPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("multi-collections"))
                view=MultiCollectionsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("multi-collections"),
                    StaticSegment("add"),
                )
                view=MultiCollectionEditorPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("multi-collections"),
                    ParamSegment("id"),
                    StaticSegment("edit"),
                )
                view=MultiCollectionEditorPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("rerun-collections"))
                view=RerunCollectionsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("rerun-collections"),
                    StaticSegment("add"),
                )
                view=RerunCollectionEditorPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("rerun-collections"),
                    ParamSegment("id"),
                    StaticSegment("edit"),
                )
                view=RerunCollectionEditorPage
            />

            <Route path=(StaticSegment("media"), StaticSegment("playlists")) view=PlaylistsPage/>
            <Route
                path=(StaticSegment("media"), StaticSegment("playlists"), ParamSegment("id"))
                view=PlaylistEditorPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("trakt"), StaticSegment("lists"))
                view=TraktListsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("trakt"),
                    StaticSegment("lists"),
                    ParamSegment("id"),
                )
                view=TraktListEditorPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("filler"), StaticSegment("presets"))
                view=FillerPresetsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("filler"),
                    StaticSegment("presets"),
                    StaticSegment("add"),
                )
                view=FillerPresetEditorPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("filler"),
                    StaticSegment("presets"),
                    ParamSegment("id"),
                    StaticSegment("edit"),
                )
                view=FillerPresetEditorPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("local"))
                view=LocalLibrariesPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("local"),
                    StaticSegment("add"),
                )
                view=LocalLibraryEditorPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("local"),
                    ParamSegment("id"),
                    StaticSegment("edit"),
                )
                view=LocalLibraryEditorPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("jellyfin"))
                view=JellyfinSourcesPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("jellyfin"),
                    StaticSegment("edit"),
                )
                view=JellyfinConnectionPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("jellyfin"),
                    ParamSegment("id"),
                    StaticSegment("libraries"),
                )
                view=JellyfinLibrariesPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("jellyfin"),
                    ParamSegment("id"),
                    StaticSegment("paths"),
                )
                view=JellyfinPathsPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("emby"))
                view=EmbySourcesPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("emby"),
                    StaticSegment("edit"),
                )
                view=EmbyConnectionPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("emby"),
                    ParamSegment("id"),
                    StaticSegment("libraries"),
                )
                view=EmbyLibrariesPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("emby"),
                    ParamSegment("id"),
                    StaticSegment("paths"),
                )
                view=EmbyPathsPage
            />

            <Route
                path=(StaticSegment("media"), StaticSegment("sources"), StaticSegment("plex"))
                view=PlexSourcesPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("plex"),
                    ParamSegment("id"),
                    StaticSegment("libraries"),
                )
                view=PlexLibrariesPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("sources"),
                    StaticSegment("plex"),
                    ParamSegment("id"),
                    StaticSegment("paths"),
                )
                view=PlexPathsPage
            />
        </Routes>
    }
}
