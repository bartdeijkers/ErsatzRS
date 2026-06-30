use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::layout::nav::NavDrawer;
use crate::admin::server_fns::layout::{
    load_ffmpeg_runtime_notification, AdminFfmpegRuntimeNotification,
};
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::toast::{use_toast, ToastContainer, ToastSeverity};

mod badges;
mod nav;

stylance::import_crate_style!(style, "src/admin/layout/layout.module.css");

#[cfg(target_arch = "wasm32")]
const FFMPEG_RUNTIME_NOTIFICATION_SESSION_KEY: &str = "ersatzrs.ffmpeg-runtime-notification";

#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    let theme = load_shell_theme_initial();

    view! {
        <div
            class=style::shell
            data-theme=theme
        >
            <input
                type="checkbox"
                id="nav-toggle"
                class=style::drawer_toggle
                data-nav-drawer-toggle
            />
            <TopBar/>
            <div class=style::body>
                <NavDrawer/>
                <NavGroupsStateScript/>
                <div class=style::content_col>
                    <main class=style::main>
                        {children()}
                    </main>
                </div>
            </div>
            <NavStateScript/>
            <ThemeStateScript/>
            <StartupNotifications/>
        </div>
    }
}

#[island]
fn StartupNotifications() -> impl IntoView {
    let toast = use_toast();
    let shown = RwSignal::new(false);
    let notification = Resource::new(
        || (),
        |()| async move { load_ffmpeg_runtime_notification().await.ok().flatten() },
    );

    Effect::new(move |_| {
        if shown.get_untracked() {
            return;
        }

        let Some(Some(notification)) = notification.get() else {
            return;
        };

        let event_key = ffmpeg_runtime_notification_event_key(&notification);
        if ffmpeg_runtime_notification_was_shown(&event_key) {
            shown.set(true);
            return;
        }

        shown.set(true);
        remember_ffmpeg_runtime_notification(&event_key);

        if notification.update_failed {
            let message = notification.error.as_deref().map_or_else(
                || tr(TextKey::FfmpegRuntimeUpdateFailed).to_owned(),
                |error| format!("{}: {error}", tr(TextKey::FfmpegRuntimeUpdateFailed)),
            );
            toast.push(message, ToastSeverity::Warning);
        } else {
            toast.push(
                ffmpeg_runtime_notification_message(&notification),
                ToastSeverity::Info,
            );
        }
    });

    view! { <ToastContainer/> }
}

fn ffmpeg_runtime_notification_message(notification: &AdminFfmpegRuntimeNotification) -> String {
    match (
        notification.previous_tag.as_deref(),
        notification.installed_tag.as_deref(),
    ) {
        (Some(previous), Some(installed)) => {
            format!(
                "{}: {previous} -> {installed}",
                tr(TextKey::FfmpegRuntimeUpdated)
            )
        }
        (None, Some(installed)) => {
            format!("{}: {installed}", tr(TextKey::FfmpegRuntimeDownloaded))
        }
        _ => tr(TextKey::FfmpegRuntimeDownloaded).to_owned(),
    }
}

fn ffmpeg_runtime_notification_event_key(notification: &AdminFfmpegRuntimeNotification) -> String {
    format!(
        "failed={};previous={};installed={};asset={};error={}",
        notification.update_failed,
        notification.previous_tag.as_deref().unwrap_or_default(),
        notification.installed_tag.as_deref().unwrap_or_default(),
        notification.asset_name.as_deref().unwrap_or_default(),
        notification.error.as_deref().unwrap_or_default()
    )
}

#[cfg(target_arch = "wasm32")]
fn ffmpeg_runtime_notification_was_shown(event_key: &str) -> bool {
    web_sys::window()
        .and_then(|window| window.session_storage().ok().flatten())
        .and_then(|storage| {
            storage
                .get_item(FFMPEG_RUNTIME_NOTIFICATION_SESSION_KEY)
                .ok()
                .flatten()
        })
        .as_deref()
        == Some(event_key)
}

#[cfg(not(target_arch = "wasm32"))]
fn ffmpeg_runtime_notification_was_shown(_event_key: &str) -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn remember_ffmpeg_runtime_notification(event_key: &str) {
    if let Some(storage) =
        web_sys::window().and_then(|window| window.session_storage().ok().flatten())
    {
        let _ = storage.set_item(FFMPEG_RUNTIME_NOTIFICATION_SESSION_KEY, event_key);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn remember_ffmpeg_runtime_notification(_event_key: &str) {}

fn load_shell_theme_initial() -> String {
    #[cfg(feature = "ssr")]
    {
        use crate::host::state::AppState;

        let Some(state) = use_context::<AppState>() else {
            return "dark".to_owned();
        };
        let is_dark_mode = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| {
                handle.block_on(crate::admin::server_fns::settings::load_bool_for_api(
                    &state.pool,
                    ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
                    true,
                ))
            })
            .unwrap_or(true),
            Err(_) => true,
        };
        if is_dark_mode { "dark" } else { "light" }.to_owned()
    }

    #[cfg(all(not(feature = "ssr"), target_arch = "wasm32"))]
    {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.query_selector("[data-theme]").ok().flatten())
            .and_then(|element| element.get_attribute("data-theme"))
            .filter(|theme| theme == "light")
            .unwrap_or_else(|| "dark".to_owned())
    }

    #[cfg(all(not(feature = "ssr"), not(target_arch = "wasm32")))]
    {
        "dark".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{ffmpeg_runtime_notification_event_key, AdminFfmpegRuntimeNotification};

    fn downloaded_notification(installed_tag: &str) -> AdminFfmpegRuntimeNotification {
        AdminFfmpegRuntimeNotification {
            previous_tag: None,
            installed_tag: Some(installed_tag.to_owned()),
            asset_name: Some(format!("ersatzrs-ffmpeg-{installed_tag}-linux-x64.tar.gz")),
            update_failed: false,
            error: None,
        }
    }

    #[test]
    fn ffmpeg_runtime_notification_event_key_is_stable_for_same_event() {
        let first = downloaded_notification("v1.0.1");
        let second = downloaded_notification("v1.0.1");

        assert_eq!(
            ffmpeg_runtime_notification_event_key(&first),
            ffmpeg_runtime_notification_event_key(&second)
        );
    }

    #[test]
    fn ffmpeg_runtime_notification_event_key_changes_for_new_event() {
        let first = downloaded_notification("v1.0.1");
        let second = downloaded_notification("v1.0.2");

        assert_ne!(
            ffmpeg_runtime_notification_event_key(&first),
            ffmpeg_runtime_notification_event_key(&second)
        );
    }

    #[test]
    fn ffmpeg_runtime_notification_event_key_includes_update_error() {
        let first = AdminFfmpegRuntimeNotification {
            previous_tag: Some("v1.0.0".to_owned()),
            installed_tag: Some("v1.0.0".to_owned()),
            asset_name: None,
            update_failed: true,
            error: Some("checksum mismatch".to_owned()),
        };
        let second = AdminFfmpegRuntimeNotification {
            error: Some("download timeout".to_owned()),
            ..first.clone()
        };

        assert_ne!(
            ffmpeg_runtime_notification_event_key(&first),
            ffmpeg_runtime_notification_event_key(&second)
        );
    }
}

#[component]
fn ThemeStateScript() -> impl IntoView {
    const SCRIPT: &str = r#"
(() => {
  async function initTheme() {
    try {
      const response = await fetch("/api/settings/ui/theme", { cache: "no-store" });
      if (!response.ok) return;
      const payload = await response.json();
      const theme = payload && payload.theme === "light" ? "light" : "dark";
      document.querySelector("[data-theme]")?.setAttribute("data-theme", theme);
    } catch {}
  }

  initTheme();
})();
"#;

    view! {
        <script inner_html=SCRIPT></script>
    }
}

#[component]
pub fn NavStateHeadScript() -> impl IntoView {
    const SCRIPT: &str = r#"
(() => {
  const drawerKey = "ersatzrs.nav.drawer.checked.v1";

  try {
    const stored = window.sessionStorage.getItem(drawerKey);
    if (stored === "true" || stored === "false") {
      document.documentElement.dataset.navDrawerChecked = stored;
    }
  } catch {}
})();
"#;

    const STYLE: &str = r#"
@media (min-width: 768px) {
  html[data-nav-drawer-checked="true"] [data-nav-drawer] {
    display: none;
  }
}
@media (max-width: 767px) {
  html[data-nav-drawer-checked="true"] [data-nav-drawer] {
    transform: translateX(0);
  }
}
"#;

    view! {
        <script inner_html=SCRIPT></script>
        <style inner_html=STYLE></style>
    }
}

#[component]
fn TopBar() -> impl IntoView {
    view! {
        <header class=style::topbar>
            <label for="nav-toggle" class=style::hamburger title=tr(TextKey::ToggleNavigation)>
                <span style=icon_style(IconName::Menu, "1.25rem") aria-hidden="true"></span>
            </label>

            <a href="/" class=style::logo>
                <img src="/public/images/ersatzrs.png" alt="ErsatzRS" class=style::logo_image/>
            </a>

            <div class=style::flex_1/>

            // Server-rendered GET form: pressing Enter navigates to
            // /search?query=... with no hydration cost. Mirrors ErsatzTV's
            // MainLayout search-bar (Search.razor reads ?query= on load).
            <form method="get" action="/search" class=style::search_form role="search">
                <span
                    class=style::search_icon
                    style=icon_style(IconName::Search, "1rem")
                    aria-hidden="true"
                ></span>
                <input
                    type="search"
                    name="query"
                    class=style::search_input
                    placeholder=tr(TextKey::Search)
                    aria-label=tr(TextKey::SearchMedia)
                    autocomplete="off"
                />
            </form>

            <nav class=style::topbar_nav>
                <a href="/iptv/channels.m3u" class=style::topbar_link target="_blank">{tr(TextKey::M3u)}</a>
                <a href="/iptv/xmltv.xml" class=style::topbar_link target="_blank">{tr(TextKey::Xmltv)}</a>
            </nav>
        </header>
    }
}

#[component]
fn NavGroupsStateScript() -> impl IntoView {
    const SCRIPT: &str = r#"
(() => {
  const groupsKey = "ersatzrs.nav.groups.v1";

  function readGroups() {
    try {
      return JSON.parse(window.sessionStorage.getItem(groupsKey) || "{}") || {};
    } catch {
      return {};
    }
  }

  const state = readGroups();
  for (const group of document.querySelectorAll("[data-nav-group]")) {
    const key = group.getAttribute("data-nav-group");
    if (key && Object.prototype.hasOwnProperty.call(state, key)) {
      group.open = state[key] === true;
    }
  }
})();
"#;

    view! {
        <script inner_html=SCRIPT></script>
    }
}

#[component]
fn NavStateScript() -> impl IntoView {
    const SCRIPT: &str = r#"
(() => {
  const drawerKey = "ersatzrs.nav.drawer.checked.v1";
  const groupsKey = "ersatzrs.nav.groups.v1";

  function storage() {
    try {
      return window.sessionStorage;
    } catch {
      return null;
    }
  }

  function readGroups(store) {
    if (!store) return {};
    try {
      return JSON.parse(store.getItem(groupsKey) || "{}") || {};
    } catch {
      return {};
    }
  }

  function syncRootDrawerState(value) {
    document.documentElement.dataset.navDrawerChecked = value ? "true" : "false";
  }

  function initDrawer(store) {
    const toggle = document.querySelector("[data-nav-drawer-toggle]");
    if (!toggle) return;

    const stored = store?.getItem(drawerKey);
    if (stored === "true" || stored === "false") {
      toggle.checked = stored === "true";
    }
    syncRootDrawerState(toggle.checked);

    toggle.addEventListener("change", () => {
      syncRootDrawerState(toggle.checked);
      store?.setItem(drawerKey, toggle.checked ? "true" : "false");
    });
  }

  function initGroups(store) {
    const groups = Array.from(document.querySelectorAll("[data-nav-group]"));
    const state = readGroups(store);

    for (const group of groups) {
      const key = group.getAttribute("data-nav-group");
      if (!key) continue;

      if (Object.prototype.hasOwnProperty.call(state, key)) {
        group.open = state[key] === true;
      }

      group.addEventListener("toggle", () => {
        const next = readGroups(store);
        next[key] = group.open;
        try {
          store?.setItem(groupsKey, JSON.stringify(next));
        } catch {}
      });
    }
  }

  function init() {
    const store = storage();
    initDrawer(store);
    initGroups(store);
  }

  init();
})();
"#;

    view! {
        <script inner_html=SCRIPT></script>
    }
}
