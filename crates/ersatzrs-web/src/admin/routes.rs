#![allow(clippy::too_many_lines)]

use crate::admin::pages::media::addon_lists::workspace::AddonListWorkspacePage;

use leptos::prelude::*;
use leptos_router::{
    components::{Route, Routes},
    ParamSegment, SsrMode, StaticSegment,
};

use crate::admin::home::HomePage;
use crate::admin::i18n::{tr, TextKey};
use crate::admin::pages::addons::{AddonDetailPage, AddonsSettingsPage};
use crate::admin::pages::channels::{
    editor::ChannelEditorPage, epg::EpgPreviewPage, list::ChannelsListPage,
    numbers::ChannelNumbersPage, watch::ChannelWatchPage,
};
use crate::admin::pages::dev::ComponentsPage;
use crate::admin::pages::ffmpeg_profiles::{
    editor::FfmpegProfileEditorPage, list::FfmpegProfilesListPage,
};
use crate::admin::pages::graphics_elements::{
    editor::GraphicsElementEditorPage, list::GraphicsElementsListPage,
};
use crate::admin::pages::libraries::LibrariesPage;
use crate::admin::pages::media::addon_lists::{AddAddonMediaListPage, AddonMediaListsPage};
use crate::admin::pages::media::artists::{ArtistPage, ArtistsPage};
use crate::admin::pages::media::collection_detail::CollectionDetailPage;
use crate::admin::pages::media::collections::{CollectionEditorPage, CollectionsPage};
use crate::admin::pages::media::faceted_detail::{ImagePage, MusicVideoPage, SongPage};
use crate::admin::pages::media::filler_presets::{FillerPresetEditorPage, FillerPresetsPage};
use crate::admin::pages::media::images::ImagesPage;
use crate::admin::pages::media::movies::{MoviePage, MoviesPage};
use crate::admin::pages::media::multi_collections::{
    MultiCollectionEditorPage, MultiCollectionsPage,
};
use crate::admin::pages::media::other_videos::{OtherVideoPage, OtherVideosPage};
use crate::admin::pages::media::playlists::{PlaylistEditorPage, PlaylistsPage};
use crate::admin::pages::media::remote_stream_detail::RemoteStreamDetailPage;
use crate::admin::pages::media::remote_stream_playlists::RemoteStreamPlaylistsPage;
use crate::admin::pages::media::remote_streams::RemoteStreamsPage;
use crate::admin::pages::media::rerun_collections::{
    RerunCollectionEditorPage, RerunCollectionsPage,
};
use crate::admin::pages::media::season_detail::SeasonPage;
use crate::admin::pages::media::see_all_lists::{
    EpisodesListPage, MusicVideosListPage, SeasonsListPage,
};
use crate::admin::pages::media::shows::{ShowPage, ShowsPage};
use crate::admin::pages::media::smart_collections::{
    SmartCollectionEditorPage, SmartCollectionsPage,
};
use crate::admin::pages::media::songs::SongsPage;
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
use crate::admin::pages::scheduling::decos::{
    DecoEditorPage, DecoTemplateEditorPage, DecoTemplatesPage, DecosPage,
};
use crate::admin::pages::scheduling::playout_editor::PlayoutEditorPage;
use crate::admin::pages::scheduling::playout_kind_editor::{
    BlockPlayoutEditorPage, ClassicPlayoutEditorPage, ScriptedPlayoutEditorPage,
};
use crate::admin::pages::scheduling::playout_nested_editor::{
    PlayoutAlternateSchedulesEditorPage, PlayoutTemplatesEditorPage,
};
use crate::admin::pages::scheduling::playouts::PlayoutsPage;
use crate::admin::pages::scheduling::schedules::{
    ScheduleEditorPage, ScheduleItemsPage, SchedulesPage,
};
use crate::admin::pages::scheduling::templates::{TemplateEditorPage, TemplatesPage};
use crate::admin::pages::scheduling::yaml_editor::YamlEditorPage;
use crate::admin::pages::search::SearchPage;
use crate::admin::pages::settings::{
    FfmpegSettingsPage, HdhrSettingsPage, LoggingSettingsPage, PlayoutSettingsPage,
    ScannerSettingsPage, UiSettingsPage, XmltvSettingsPage,
};
use crate::admin::pages::system::{
    health::HealthPage,
    logs::LogsPage,
    troubleshooting::{
        BlockPlayoutHistoryPage, BlockPlayoutTroubleshootingPage, MpegTroubleshootingPage,
        PlaybackTroubleshootingPage, SequentialScheduleValidationPage, TroubleshootingPage,
    },
};
use crate::admin::pages::watermarks::{editor::WatermarkEditorPage, list::WatermarksListPage};

stylance::import_crate_style!(style, "src/admin/app.module.css");

#[component]
pub fn AdminRoutes() -> impl IntoView {
    view! {
        <Routes fallback=|| {
            view! {
                <div class=style::not_found data-not-found="true">
                    <p class=style::not_found_code>"404"</p>
                    <p class=style::not_found_msg>{tr(TextKey::NotFound)}</p>
                    <a href="/" class=style::not_found_link>{tr(TextKey::GoHome)}</a>
                </div>
            }
            .into_view()
        }>
            <Route path=StaticSegment("") view=HomePage/>
            <Route path=StaticSegment("dev/components") view=ComponentsPage/>
            <Route path=StaticSegment("epg") view=EpgPreviewPage/>
            <Route path=StaticSegment("channels") view=ChannelsListPage/>
            <Route
                path=(StaticSegment("channels"), StaticSegment("add"))
                view=ChannelEditorPage
                ssr=SsrMode::InOrder
            />
            <Route
                path=(StaticSegment("channels"), StaticSegment("numbers"))
                view=ChannelNumbersPage
            />
            <Route
                path=(
                    StaticSegment("channels"),
                    ParamSegment("id"),
                    StaticSegment("watch"),
                )
                view=ChannelWatchPage
            />
            <Route path=(StaticSegment("channels"), ParamSegment("id")) view=ChannelEditorPage ssr=SsrMode::InOrder/>

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

            <Route path=StaticSegment("graphics-elements") view=GraphicsElementsListPage/>
            <Route
                path=(StaticSegment("graphics-elements"), StaticSegment("add"))
                view=GraphicsElementEditorPage
            />
            <Route
                path=(StaticSegment("graphics-elements"), ParamSegment("id"))
                view=GraphicsElementEditorPage
            />

            <Route path=StaticSegment("playouts") view=PlayoutsPage ssr=SsrMode::InOrder/>
            <Route
                path=(StaticSegment("playouts"), StaticSegment("add"))
                view=PlayoutEditorPage
            />
            <Route
                path=(StaticSegment("playouts"), StaticSegment("add"), ParamSegment("kind"))
                view=PlayoutEditorPage
            />
            <Route
                path=(StaticSegment("playouts"), StaticSegment("sequential"), ParamSegment("id"))
                view=YamlEditorPage
            />
            <Route
                path=(StaticSegment("playouts"), StaticSegment("classic"), ParamSegment("id"))
                view=ClassicPlayoutEditorPage
            />
            <Route
                path=(StaticSegment("playouts"), StaticSegment("block"), ParamSegment("id"))
                view=BlockPlayoutEditorPage
            />
            <Route
                path=(StaticSegment("playouts"), StaticSegment("scripted"), ParamSegment("id"))
                view=ScriptedPlayoutEditorPage
            />
            <Route
                path=(
                    StaticSegment("playouts"),
                    ParamSegment("id"),
                    StaticSegment("alternate-schedules"),
                )
                view=PlayoutAlternateSchedulesEditorPage
            />
            <Route
                path=(StaticSegment("playouts"), ParamSegment("id"), StaticSegment("templates"))
                view=PlayoutTemplatesEditorPage
                ssr=SsrMode::InOrder
            />
            <Route path=StaticSegment("blocks") view=BlocksPage/>
            <Route path=StaticSegment("decos") view=DecosPage/>
            <Route path=(StaticSegment("decos"), ParamSegment("id")) view=DecoEditorPage/>
            <Route path=StaticSegment("deco-templates") view=DecoTemplatesPage/>
            <Route
                path=(StaticSegment("deco-templates"), ParamSegment("id"))
                view=DecoTemplateEditorPage
            />
            <Route path=StaticSegment("templates") view=TemplatesPage/>
            <Route path=(StaticSegment("templates"), ParamSegment("id")) view=TemplateEditorPage/>
            <Route path=StaticSegment("schedules") view=SchedulesPage/>
            <Route
                path=(StaticSegment("schedules"), StaticSegment("add"))
                view=ScheduleEditorPage
            />
            <Route
                path=(StaticSegment("schedules"), ParamSegment("id"), StaticSegment("items"))
                view=ScheduleItemsPage
            />
            <Route path=(StaticSegment("schedules"), ParamSegment("id")) view=ScheduleEditorPage/>
            <Route path=StaticSegment("search") view=SearchPage ssr=SsrMode::InOrder/>
            <Route
                path=(StaticSegment("settings"), StaticSegment("addons"))
                view=AddonsSettingsPage
            />
            <Route
                path=(StaticSegment("settings"), StaticSegment("addons"), ParamSegment("addon_id"))
                view=AddonDetailPage
            />
            <Route
                path=(StaticSegment("settings"), StaticSegment("ffmpeg"))
                view=FfmpegSettingsPage
                ssr=SsrMode::InOrder
            />
            <Route
                path=(StaticSegment("settings"), StaticSegment("logging"))
                view=LoggingSettingsPage
            />
            <Route
                path=(StaticSegment("settings"), StaticSegment("hdhr"))
                view=HdhrSettingsPage
                ssr=SsrMode::InOrder
            />
            <Route
                path=(StaticSegment("settings"), StaticSegment("scanner"))
                view=ScannerSettingsPage
            />
            <Route
                path=(StaticSegment("settings"), StaticSegment("playout"))
                view=PlayoutSettingsPage
            />
            <Route path=(StaticSegment("settings"), StaticSegment("ui")) view=UiSettingsPage ssr=SsrMode::InOrder/>
            <Route
                path=(StaticSegment("settings"), StaticSegment("xmltv"))
                view=XmltvSettingsPage
            />
            <Route
                path=(StaticSegment("system"), StaticSegment("health"))
                view=HealthPage
            />
            <Route path=(StaticSegment("system"), StaticSegment("logs")) view=LogsPage/>
            <Route
                path=(StaticSegment("system"), StaticSegment("troubleshooting"))
                view=TroubleshootingPage
                ssr=SsrMode::InOrder
            />
            <Route
                path=(
                    StaticSegment("system"),
                    StaticSegment("troubleshooting"),
                    StaticSegment("mpeg"),
                )
                view=MpegTroubleshootingPage
            />
            <Route
                path=(
                    StaticSegment("system"),
                    StaticSegment("troubleshooting"),
                    StaticSegment("sequential-schedule"),
                )
                view=SequentialScheduleValidationPage
            />
            <Route
                path=(
                    StaticSegment("system"),
                    StaticSegment("troubleshooting"),
                    StaticSegment("block-playout"),
                )
                view=BlockPlayoutTroubleshootingPage
            />
            <Route
                path=(
                    StaticSegment("system"),
                    StaticSegment("troubleshooting"),
                    StaticSegment("block-playout"),
                    StaticSegment("history"),
                )
                view=BlockPlayoutHistoryPage
            />
            <Route
                path=(
                    StaticSegment("system"),
                    StaticSegment("troubleshooting"),
                    StaticSegment("playback"),
                )
                view=PlaybackTroubleshootingPage
                ssr=SsrMode::InOrder
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
                path=(StaticSegment("media"), StaticSegment("images"), ParamSegment("id"))
                view=ImagePage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("browser"), StaticSegment("images"))
                view=ImagesPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("browser"),
                    StaticSegment("images"),
                    ParamSegment("id"),
                )
                view=ImagePage
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
                path=(
                    StaticSegment("media"),
                    StaticSegment("music"),
                    StaticSegment("songs"),
                    ParamSegment("id"),
                )
                view=SongPage
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
                path=(
                    StaticSegment("media"),
                    StaticSegment("music"),
                    StaticSegment("videos"),
                    ParamSegment("id"),
                )
                view=MusicVideoPage
            />
            <Route
                path=(StaticSegment("media"), StaticSegment("remote"), StaticSegment("streams"))
                view=RemoteStreamsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("remote"),
                    StaticSegment("streams"),
                    StaticSegment("playlists"),
                    ParamSegment("library_id"),
                )
                view=RemoteStreamPlaylistsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("remote"),
                    StaticSegment("streams"),
                    StaticSegment("page"),
                    ParamSegment("page"),
                )
                view=RemoteStreamsPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("remote"),
                    StaticSegment("streams"),
                    ParamSegment("id"),
                )
                view=RemoteStreamDetailPage
                ssr=SsrMode::InOrder
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
                path=(StaticSegment("media"), StaticSegment("addon-lists"))
                view=AddonMediaListsPage
                ssr=SsrMode::InOrder
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("addon-lists"),
                    StaticSegment("add"),
                )
                view=AddAddonMediaListPage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("addon-lists"),
                    ParamSegment("id"),
                    StaticSegment("items"),
                    ParamSegment("item_id"),
                )
                view=AddonListWorkspacePage
            />
            <Route
                path=(
                    StaticSegment("media"),
                    StaticSegment("addon-lists"),
                    ParamSegment("id"),
                )
                view=AddonListWorkspacePage
            />

            <Route path=(StaticSegment("media"), StaticSegment("addon-lists"), ParamSegment("id"), StaticSegment("settings")) view=AddonListWorkspacePage ssr=SsrMode::InOrder/>
            <Route path=(StaticSegment("media"), StaticSegment("addon-lists"), ParamSegment("id"), StaticSegment("playback")) view=AddonListWorkspacePage ssr=SsrMode::InOrder/>
            <Route path=(StaticSegment("media"), StaticSegment("addon-lists"), ParamSegment("id"), StaticSegment("activity")) view=AddonListWorkspacePage ssr=SsrMode::InOrder/>
            <Route path=(StaticSegment("media"), StaticSegment("addon-lists"), ParamSegment("id"), StaticSegment("artwork")) view=AddonListWorkspacePage ssr=SsrMode::InOrder/>

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
                ssr=SsrMode::InOrder
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
                ssr=SsrMode::InOrder
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
