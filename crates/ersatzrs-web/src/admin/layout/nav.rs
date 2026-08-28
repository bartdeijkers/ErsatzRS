use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::layout::badges::{HealthBadge, PlayoutWarningBadge};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

use super::style;

#[component]
pub(super) fn NavDrawer() -> impl IntoView {
    view! {
        <aside id="admin-navigation-drawer" class=style::drawer data-nav-drawer>
            <nav class=style::drawer_nav>
                <NavLink href="/channels" label=tr(TextKey::Channels) icon=IconName::Play/>
                <NavLink href="/epg" label=tr(TextKey::EpgPreview) icon=IconName::Calendar/>
                <NavLink href="/ffmpeg" label=tr(TextKey::FfmpegProfiles) icon=IconName::Tune/>
                <NavLink href="/watermarks" label=tr(TextKey::Watermarks) icon=IconName::Visibility/>
                <NavLink href="/graphics-elements" label=tr(TextKey::GraphicsElements) icon=IconName::Visibility/>

                <NavGroup storage_key="media-sources" label=tr(TextKey::MediaSources)>
                    <NavLink href="/media/sources/local" label=tr(TextKey::Local) icon=IconName::Download/>
                    <NavLink href="/media/sources/emby" label="Emby" icon=IconName::Download/>
                    <NavLink href="/media/sources/jellyfin" label="Jellyfin" icon=IconName::Download/>
                    <NavLink href="/media/sources/plex" label="Plex" icon=IconName::Download/>
                </NavGroup>

                <NavGroup storage_key="media" label=tr(TextKey::Media)>
                    <NavLink href="/media/libraries" label=tr(TextKey::Libraries) icon=IconName::Copy/>
                    <NavLink href="/media/trash" label=tr(TextKey::Trash) icon=IconName::Delete/>
                    <NavLink href="/media/tv/shows" label=tr(TextKey::TvShows) icon=IconName::Play/>
                    <NavLink href="/media/movies" label=tr(TextKey::Movies) icon=IconName::Play/>
                    <NavLink href="/media/music/artists" label=tr(TextKey::Music) icon=IconName::Play/>
                    <NavLink href="/media/other/videos" label=tr(TextKey::OtherVideos) icon=IconName::Play/>
                    <NavLink href="/media/music/songs" label=tr(TextKey::Songs) icon=IconName::Play/>
                    <NavLink href="/media/browser/images" label=tr(TextKey::Images) icon=IconName::Visibility/>
                    <NavLink href="/media/remote/streams" label=tr(TextKey::RemoteStreams) icon=IconName::Download/>
                </NavGroup>

                <NavGroup storage_key="lists" label=tr(TextKey::Lists)>
                    <NavLink href="/media/addon-lists" label=tr(TextKey::AddonMediaLists) icon=IconName::Download/>
                    <NavLink href="/media/collections" label=tr(TextKey::Collections) icon=IconName::Copy/>
                    <NavLink href="/media/smart-collections" label=tr(TextKey::SmartCollections) icon=IconName::Search/>
                    <NavLink href="/media/multi-collections" label=tr(TextKey::MultiCollections) icon=IconName::Copy/>
                    <NavLink href="/media/rerun-collections" label=tr(TextKey::RerunCollections) icon=IconName::Refresh/>
                    <NavLink href="/media/playlists" label=tr(TextKey::Playlists) icon=IconName::Play/>
                    <NavLink href="/media/filler/presets" label=tr(TextKey::FillerPresets) icon=IconName::Settings/>
                </NavGroup>

                <SchedulingNavGroup>
                    <NavLink href="/schedules" label=tr(TextKey::Schedules) icon=IconName::Settings/>
                    <NavLink href="/blocks" label=tr(TextKey::Blocks) icon=IconName::Copy/>
                    <NavLink href="/templates" label=tr(TextKey::Templates) icon=IconName::Copy/>
                    <NavLink href="/decos" label=tr(TextKey::Decorations) icon=IconName::Edit/>
                    <NavLink href="/deco-templates" label=tr(TextKey::DecoTemplates) icon=IconName::Copy/>
                    <NavLink href="/playouts" label=tr(TextKey::Playouts) icon=IconName::Play/>
                </SchedulingNavGroup>

                <NavLink
                    href="/settings/ffmpeg"
                    label=tr(TextKey::Settings)
                    icon=IconName::Settings
                    match_prefix="/settings/"
                />

                <SupportNavGroup>
                    <NavLink href="/system/health" label=tr(TextKey::HealthChecks) icon=IconName::Check/>
                    <NavLink href="/system/logs" label=tr(TextKey::Logging) icon=IconName::Info/>
                    <NavLink href="/system/troubleshooting" label=tr(TextKey::Troubleshooting) icon=IconName::Warning/>
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
                    <span>{tr(TextKey::Scheduling)}</span>
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
                    <span>{tr(TextKey::Support)}</span>
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
fn NavLink(
    href: &'static str,
    label: &'static str,
    icon: IconName,
    #[prop(optional)] match_prefix: Option<&'static str>,
) -> impl IntoView {
    let location = use_location();
    let is_active = move || {
        let pathname = location.pathname.get();
        pathname == href || match_prefix.is_some_and(|prefix| pathname.starts_with(prefix))
    };

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

#[cfg(test)]
mod tests {
    #[test]
    fn settings_prefix_does_not_match_system_routes() {
        let prefix = "/settings/";
        assert!("/settings/logging".starts_with(prefix));
        assert!(!"/system/logs".starts_with(prefix));
    }
}
