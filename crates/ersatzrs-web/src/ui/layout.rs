use leptos::prelude::*;
use leptos_router::hooks::use_location;

use super::primitives::icon::{Icon, IconName};

stylance::import_crate_style!(style, "src/ui/layout.module.css");

// ---------------------------------------------------------------------------
// AppShell
// ---------------------------------------------------------------------------

/// Root layout shell wrapping the nav drawer, top bar, and page content.
/// The drawer open/close state is managed via a hidden CSS checkbox so it
/// works without JavaScript. On desktop the drawer is always visible; on
/// mobile (< 768 px) it becomes a fixed overlay toggled by the hamburger.
#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    view! {
        <div class=style::shell>
            // Hidden checkbox: unchecked = desktop-open / mobile-closed (default).
            // Checking it = desktop-close / mobile-open. The hamburger <label>
            // toggles this via the browser's native label→checkbox association.
            <input type="checkbox" id="nav-toggle" class=style::drawer_toggle/>
            <NavDrawer/>
            <div class=style::content_col>
                <TopBar/>
                <main class=style::main>
                    {children()}
                </main>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// TopBar
// ---------------------------------------------------------------------------

#[component]
fn TopBar() -> impl IntoView {
    view! {
        <header class=style::topbar>
            // Hamburger is a <label> so no JavaScript is needed to toggle the
            // drawer; the browser links it to #nav-toggle natively.
            <label for="nav-toggle" class=style::hamburger title="Toggle navigation">
                <Icon name=IconName::Menu size="1.25rem"/>
            </label>

            <a href="/" class=style::logo>
                <span class=style::logo_text>"ErsatzRS"</span>
            </a>

            <div class=style::flex_1/>

            <nav class=style::topbar_nav>
                <a href="/iptv/channels.m3u" class=style::topbar_link target="_blank">"M3U"</a>
                <a href="/iptv/xmltv.xml" class=style::topbar_link target="_blank">"XMLTV"</a>
            </nav>
        </header>
    }
}

// ---------------------------------------------------------------------------
// NavDrawer
// ---------------------------------------------------------------------------

#[component]
fn NavDrawer() -> impl IntoView {
    view! {
        <aside class=style::drawer>
            <nav class=style::drawer_nav>
                <NavLink href="/channels"   label="Channels"        icon=IconName::Play/>
                <NavLink href="/ffmpeg" label="FFmpeg Profiles" icon=IconName::Tune/>
                <NavLink href="/watermarks" label="Watermarks"      icon=IconName::Visibility/>

                <NavGroup label="Media Sources">
                    <NavLink href="/media/sources/local"     label="Local"     icon=IconName::Download/>
                    <NavLink href="/media/sources/emby"      label="Emby"      icon=IconName::Download/>
                    <NavLink href="/media/sources/jellyfin"  label="Jellyfin"  icon=IconName::Download/>
                    <NavLink href="/media/sources/plex"      label="Plex"      icon=IconName::Download/>
                </NavGroup>

                <NavGroup label="Media">
                    <NavLink href="/media/libraries"        label="Libraries"      icon=IconName::Copy/>
                    <NavLink href="/media/trash"            label="Trash"          icon=IconName::Delete/>
                    <NavLink href="/media/tv/shows"         label="TV Shows"       icon=IconName::Play/>
                    <NavLink href="/media/movies"           label="Movies"         icon=IconName::Play/>
                    <NavLink href="/media/music/artists"    label="Music"          icon=IconName::Play/>
                    <NavLink href="/media/other/videos"     label="Other Videos"   icon=IconName::Play/>
                    <NavLink href="/media/music/songs"      label="Songs"          icon=IconName::Play/>
                    <NavLink href="/media/browser/images"   label="Images"         icon=IconName::Visibility/>
                    <NavLink href="/media/remote/streams"   label="Remote Streams" icon=IconName::Download/>
                </NavGroup>

                <NavGroup label="Lists">
                    <NavLink href="/media/collections"        label="Collections"        icon=IconName::Copy/>
                    <NavLink href="/media/smart-collections"  label="Smart Collections"  icon=IconName::Search/>
                    <NavLink href="/media/multi-collections"  label="Multi-Collections"  icon=IconName::Copy/>
                    <NavLink href="/media/rerun-collections"  label="Rerun Collections"  icon=IconName::Refresh/>
                    <NavLink href="/media/playlists"          label="Playlists"          icon=IconName::Play/>
                    <NavLink href="/media/trakt/lists"        label="Trakt Lists"        icon=IconName::Download/>
                    <NavLink href="/media/filler/presets"     label="Filler Presets"     icon=IconName::Settings/>
                </NavGroup>

                <SchedulingNavGroup>
                    <NavLink href="/schedules"     label="Schedules"        icon=IconName::Settings/>
                    <NavLink href="/blocks" label="Blocks"        icon=IconName::Copy/>
                    <NavLink href="/templates"     label="Templates"        icon=IconName::Copy/>
                    <NavLink href="/decos"         label="Decorations"      icon=IconName::Edit/>
                    <NavLink href="/deco-templates" label="Deco Templates"  icon=IconName::Copy/>
                    <NavLink href="/playouts"      label="Playouts"         icon=IconName::Play/>
                </SchedulingNavGroup>

                <NavGroup label="Settings">
                    <NavLink href="/settings/ffmpeg"   label="FFmpeg"      icon=IconName::Settings/>
                    <NavLink href="/settings/logging"  label="Logging"     icon=IconName::Settings/>
                    <NavLink href="/settings/hdhr"     label="HDHomeRun"   icon=IconName::Settings/>
                    <NavLink href="/settings/scanner"  label="Scanner"     icon=IconName::Settings/>
                    <NavLink href="/settings/playout"  label="Playout"     icon=IconName::Settings/>
                    <NavLink href="/settings/ui"       label="UI"          icon=IconName::Settings/>
                    <NavLink href="/settings/xmltv"    label="XMLTV"       icon=IconName::Settings/>
                </NavGroup>

                <SupportNavGroup>
                    <NavLink href="/system/health"          label="Health Checks"    icon=IconName::Check/>
                    <NavLink href="/system/logs"            label="Logs"             icon=IconName::Info/>
                    <NavLink href="/system/troubleshooting" label="Troubleshooting"  icon=IconName::Warning/>
                </SupportNavGroup>
            </nav>
        </aside>
    }
}

// ---------------------------------------------------------------------------
// SchedulingNavGroup — nav group with playout-warning badge (island)
// ---------------------------------------------------------------------------

#[component]
fn SchedulingNavGroup(children: ChildrenFn) -> impl IntoView {
    let expanded = RwSignal::new(true);

    view! {
        <div class=style::nav_group>
            <button
                class=style::nav_group_header
                on:click=move |_| expanded.update(|e| *e = !*e)
            >
                <div class=style::nav_group_header_left>
                    <span>"Scheduling"</span>
                    <PlayoutWarningBadge/>
                </div>
                {move || {
                    if expanded.get() {
                        view! { <Icon name=IconName::ChevronDown size="0.75rem"/> }.into_any()
                    } else {
                        view! { <Icon name=IconName::ChevronRight size="0.75rem"/> }.into_any()
                    }
                }}
            </button>
            <Show when=move || expanded.get()>
                <div class=style::nav_group_children>
                    {children()}
                </div>
            </Show>
        </div>
    }
}

// ---------------------------------------------------------------------------
// SupportNavGroup — nav group with health-issue badge (island)
// ---------------------------------------------------------------------------

#[component]
fn SupportNavGroup(children: ChildrenFn) -> impl IntoView {
    let expanded = RwSignal::new(true);

    view! {
        <div class=style::nav_group>
            <button
                class=style::nav_group_header
                on:click=move |_| expanded.update(|e| *e = !*e)
            >
                <div class=style::nav_group_header_left>
                    <span>"Support"</span>
                    <HealthBadge/>
                </div>
                {move || {
                    if expanded.get() {
                        view! { <Icon name=IconName::ChevronDown size="0.75rem"/> }.into_any()
                    } else {
                        view! { <Icon name=IconName::ChevronRight size="0.75rem"/> }.into_any()
                    }
                }}
            </button>
            <Show when=move || expanded.get()>
                <div class=style::nav_group_children>
                    {children()}
                </div>
            </Show>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Badge islands — independently hydrated; poll badge counts every 30 s
// ---------------------------------------------------------------------------

/// Displays a warning badge when one or more playouts have a failed build.
/// Island: re-fetches on every client-side navigation tick.
/// Full 30-second polling via leptos-use requires WASM build setup (Phase 6.x).
#[island]
pub fn PlayoutWarningBadge() -> impl IntoView {
    use crate::server::layout::count_playout_warnings;

    let count = Resource::new(
        || (),
        |()| async move { count_playout_warnings().await.unwrap_or(0) },
    );

    view! {
        <Suspense fallback=|| ()>
            {move || {
                count.get().map(|value| {
                    if value == 0 {
                        ().into_any()
                    } else {
                        let label = if value > 99 {
                            "99+".to_string()
                        } else {
                            value.to_string()
                        };

                        view! {
                            <span class=style::nav_badge_warning>{label}</span>
                        }
                            .into_any()
                    }
                })
            }}
        </Suspense>
    }
}

/// Displays an error or warning badge based on system health check counts.
/// Island: re-fetches on every client-side navigation tick.
/// Full 30-second polling via leptos-use requires WASM build setup (Phase 6.x).
#[island]
pub fn HealthBadge() -> impl IntoView {
    use crate::server::layout::count_health_issues;

    let issues = Resource::new(
        || (),
        |()| async move { count_health_issues().await.unwrap_or((0, 0)) },
    );

    view! {
        <Suspense fallback=|| ()>
            {move || {
                issues.get().map(|(errors, warnings)| {
                    if errors != 0 {
                        let label = if errors > 99 {
                            "99+".to_string()
                        } else {
                            errors.to_string()
                        };

                        view! {
                            <span class=style::nav_badge_error>{label}</span>
                        }
                            .into_any()
                    } else if warnings != 0 {
                        let label = if warnings > 99 {
                            "99+".to_string()
                        } else {
                            warnings.to_string()
                        };

                        view! {
                            <span class=style::nav_badge_warning>{label}</span>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }
                })
            }}
        </Suspense>
    }
}

// ---------------------------------------------------------------------------
// NavGroup / NavLink helpers
// ---------------------------------------------------------------------------

#[component]
fn NavGroup(label: &'static str, children: ChildrenFn) -> impl IntoView {
    let expanded = RwSignal::new(true);

    view! {
        <div class=style::nav_group>
            <button
                class=style::nav_group_header
                on:click=move |_| expanded.update(|e| *e = !*e)
            >
                <span>{label}</span>
                {move || {
                    if expanded.get() {
                        view! { <Icon name=IconName::ChevronDown size="0.75rem"/> }.into_any()
                    } else {
                        view! { <Icon name=IconName::ChevronRight size="0.75rem"/> }.into_any()
                    }
                }}
            </button>
            <Show when=move || expanded.get()>
                <div class=style::nav_group_children>
                    {children()}
                </div>
            </Show>
        </div>
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
            <Icon name=icon size="1rem"/>
            <span class=style::nav_link_text>{label}</span>
        </a>
    }
}
