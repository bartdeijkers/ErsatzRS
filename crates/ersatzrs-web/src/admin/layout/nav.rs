use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::admin::layout::badges::{HealthBadge, PlayoutWarningBadge};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

use super::style;

#[component]
pub(super) fn NavDrawer() -> impl IntoView {
    view! {
        <aside class=style::drawer>
            <nav class=style::drawer_nav>
                <NavLink href="/channels" label="Channels" icon=IconName::Play/>
                <NavLink href="/ffmpeg" label="FFmpeg Profiles" icon=IconName::Tune/>
                <NavLink href="/watermarks" label="Watermarks" icon=IconName::Visibility/>

                <NavGroup storage_key="media-sources" label="Media Sources">
                    <NavLink href="/media/sources/local" label="Local" icon=IconName::Download/>
                    <NavLink href="/media/sources/emby" label="Emby" icon=IconName::Download/>
                    <NavLink href="/media/sources/jellyfin" label="Jellyfin" icon=IconName::Download/>
                    <NavLink href="/media/sources/plex" label="Plex" icon=IconName::Download/>
                </NavGroup>

                <NavGroup storage_key="media" label="Media">
                    <NavLink href="/media/libraries" label="Libraries" icon=IconName::Copy/>
                    <NavLink href="/media/trash" label="Trash" icon=IconName::Delete/>
                    <NavLink href="/media/tv/shows" label="TV Shows" icon=IconName::Play/>
                    <NavLink href="/media/movies" label="Movies" icon=IconName::Play/>
                    <NavLink href="/media/music/artists" label="Music" icon=IconName::Play/>
                    <NavLink href="/media/other/videos" label="Other Videos" icon=IconName::Play/>
                    <NavLink href="/media/music/songs" label="Songs" icon=IconName::Play/>
                    <NavLink href="/media/browser/images" label="Images" icon=IconName::Visibility/>
                    <NavLink href="/media/remote/streams" label="Remote Streams" icon=IconName::Download/>
                </NavGroup>

                <NavGroup storage_key="lists" label="Lists">
                    <NavLink href="/media/collections" label="Collections" icon=IconName::Copy/>
                    <NavLink href="/media/smart-collections" label="Smart Collections" icon=IconName::Search/>
                    <NavLink href="/media/multi-collections" label="Multi-Collections" icon=IconName::Copy/>
                    <NavLink href="/media/rerun-collections" label="Rerun Collections" icon=IconName::Refresh/>
                    <NavLink href="/media/playlists" label="Playlists" icon=IconName::Play/>
                    <NavLink href="/media/trakt/lists" label="Trakt Lists" icon=IconName::Download/>
                    <NavLink href="/media/filler/presets" label="Filler Presets" icon=IconName::Settings/>
                </NavGroup>

                <SchedulingNavGroup>
                    <NavLink href="/schedules" label="Schedules" icon=IconName::Settings/>
                    <NavLink href="/blocks" label="Blocks" icon=IconName::Copy/>
                    <NavLink href="/templates" label="Templates" icon=IconName::Copy/>
                    <NavLink href="/decos" label="Decorations" icon=IconName::Edit/>
                    <NavLink href="/deco-templates" label="Deco Templates" icon=IconName::Copy/>
                    <NavLink href="/playouts" label="Playouts" icon=IconName::Play/>
                </SchedulingNavGroup>

                <NavGroup storage_key="settings" label="Settings">
                    <NavLink href="/settings/ffmpeg" label="FFmpeg" icon=IconName::Settings/>
                    <NavLink href="/settings/logging" label="Logging" icon=IconName::Settings/>
                    <NavLink href="/settings/hdhr" label="HDHomeRun" icon=IconName::Settings/>
                    <NavLink href="/settings/scanner" label="Scanner" icon=IconName::Settings/>
                    <NavLink href="/settings/playout" label="Playout" icon=IconName::Settings/>
                    <NavLink href="/settings/ui" label="UI" icon=IconName::Settings/>
                    <NavLink href="/settings/xmltv" label="XMLTV" icon=IconName::Settings/>
                </NavGroup>

                <SupportNavGroup>
                    <NavLink href="/system/health" label="Health Checks" icon=IconName::Check/>
                    <NavLink href="/system/logs" label="Logs" icon=IconName::Info/>
                    <NavLink href="/system/troubleshooting" label="Troubleshooting" icon=IconName::Warning/>
                </SupportNavGroup>
            </nav>
        </aside>
    }
}

#[component]
fn SchedulingNavGroup(children: ChildrenFn) -> impl IntoView {
    view! {
        <details class=style::nav_group data-nav-group="scheduling">
            <summary class=style::nav_group_header>
                <div class=style::nav_group_header_left>
                    <span>"Scheduling"</span>
                    <PlayoutWarningBadge/>
                </div>
                <span class=style::nav_group_chevron>
                    <span style=icon_style(IconName::ChevronDown, "0.75rem") aria-hidden="true"></span>
                </span>
            </summary>
            <div class=style::nav_group_children>
                {children()}
            </div>
        </details>
    }
}

#[component]
fn SupportNavGroup(children: ChildrenFn) -> impl IntoView {
    view! {
        <details class=style::nav_group open data-nav-group="support">
            <summary class=style::nav_group_header>
                <div class=style::nav_group_header_left>
                    <span>"Support"</span>
                    <HealthBadge/>
                </div>
                <span class=style::nav_group_chevron>
                    <span style=icon_style(IconName::ChevronDown, "0.75rem") aria-hidden="true"></span>
                </span>
            </summary>
            <div class=style::nav_group_children>
                {children()}
            </div>
        </details>
    }
}

#[component]
fn NavGroup(storage_key: &'static str, label: &'static str, children: ChildrenFn) -> impl IntoView {
    view! {
        <details class=style::nav_group data-nav-group=storage_key>
            <summary class=style::nav_group_header>
                <span>{label}</span>
                <span class=style::nav_group_chevron>
                    <span style=icon_style(IconName::ChevronDown, "0.75rem") aria-hidden="true"></span>
                </span>
            </summary>
            <div class=style::nav_group_children>
                {children()}
            </div>
        </details>
    }
}

#[component]
fn NavLink(href: &'static str, label: &'static str, icon: IconName) -> impl IntoView {
    let location = use_location();
    let is_active = move || location.pathname.get() == href;

    let cls = move || {
        if is_active() {
            format!("{} {}", style::nav_link, style::nav_link_active)
        } else {
            style::nav_link.to_string()
        }
    };

    view! {
        <a href=href class=cls>
            <span style=icon_style(icon, "1rem") aria-hidden="true"></span>
            <span class=style::nav_link_text>{label}</span>
        </a>
    }
}
