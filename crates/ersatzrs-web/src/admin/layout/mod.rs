use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::layout::nav::NavDrawer;
use crate::admin::server_fns::layout::{
    load_ffmpeg_runtime_notification, AdminFfmpegRuntimeNotification,
};
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::resource_state::AdminErrorState;
use crate::admin::ui::primitives::toast::{use_toast, ToastContainer, ToastSeverity};

mod badges;
mod nav;

stylance::import_crate_style!(style, "src/admin/layout/layout.module.css");

#[cfg(target_arch = "wasm32")]
const FFMPEG_RUNTIME_NOTIFICATION_SESSION_KEY: &str = "ersatzrs.ffmpeg-runtime-notification";

#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    let (theme, theme_error) = match load_shell_theme_initial() {
        Ok(theme) => (theme, None),
        Err(error) => ("dark".to_owned(), Some(error)),
    };

    view! {
        <div
            class=style::shell
            data-theme=theme
        >
            <TopBar/>
            <div class=style::body>
                <NavDrawer/>
                <NavGroupsStateScript/>
                <div class=style::content_col>
                    <main class=style::main>
                        {theme_error.map(|error| view! { <AdminErrorState error/> })}
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
        |()| async move { load_ffmpeg_runtime_notification().await },
    );

    Effect::new(move |_| {
        if shown.get_untracked() {
            return;
        }

        let Some(result) = notification.get() else {
            return;
        };
        let notification = match result {
            Ok(Some(notification)) => notification,
            Ok(None) => return,
            Err(error) => {
                shown.set(true);
                toast.push(
                    format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                    ToastSeverity::Error,
                );
                return;
            }
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

fn load_shell_theme_initial() -> Result<String, String> {
    #[cfg(feature = "ssr")]
    {
        use crate::host::state::AppState;

        let state = use_context::<AppState>()
            .ok_or_else(|| "application state is unavailable".to_owned())?;
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|error| format!("async runtime is unavailable: {error}"))?;
        let is_dark_mode = tokio::task::block_in_place(|| {
            handle.block_on(crate::admin::server_fns::settings::load_bool_for_api(
                state.application.ports().admin_settings(),
                ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
                true,
            ))
        })
        .map_err(|error| error.to_string())?;
        Ok(if is_dark_mode { "dark" } else { "light" }.to_owned())
    }

    #[cfg(all(not(feature = "ssr"), target_arch = "wasm32"))]
    {
        Ok(web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.query_selector("[data-theme]").ok().flatten())
            .and_then(|element| element.get_attribute("data-theme"))
            .filter(|theme| theme == "light")
            .unwrap_or_else(|| "dark".to_owned()))
    }

    #[cfg(all(not(feature = "ssr"), not(target_arch = "wasm32")))]
    {
        Ok("dark".to_owned())
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
      const response = await fetch("/admin-api/settings/ui/theme", { cache: "no-store" });
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
  const drawerKey = "ersatzrs.nav.drawer.open.v2";
  document.documentElement.dataset.jsEnabled = "true";

  try {
    const stored = window.sessionStorage.getItem(drawerKey);
    if (window.matchMedia("(max-width: 767px)").matches) {
      document.documentElement.dataset.navDrawerOpen = "false";
    } else if (stored === "true" || stored === "false") {
      document.documentElement.dataset.navDrawerOpen = stored;
    }
  } catch {}
})();
"#;

    const STYLE: &str = r#"
@media (min-width: 768px) {
  html[data-nav-drawer-open="false"] [data-nav-drawer] {
    display: none;
  }
}
@media (max-width: 767px) {
  html[data-nav-drawer-open="true"] [data-nav-drawer] {
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
            <a href="/" class=style::logo>
                <img src="/public/images/ersatzrs-mark.svg" alt="" width="38" height="38" class=style::logo_image/>
                <span class=style::logo_wordmark>"ErsatzRS"</span>
            </a>

            <button
                id="nav-toggle"
                type="button"
                class=style::hamburger
                title=tr(TextKey::ToggleNavigation)
                aria-label=tr(TextKey::ToggleNavigation)
                aria-expanded="true"
                aria-controls="admin-navigation-drawer"
                data-nav-drawer-toggle
            >
                <span style=icon_style(IconName::Menu, "1.25rem") aria-hidden="true"></span>
            </button>

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
  const drawerKey = "ersatzrs.nav.drawer.open.v2";
  const groupsKey = "ersatzrs.nav.groups.v1";
  const mobile = window.matchMedia("(max-width: 767px)");

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

  function syncDrawer(open, toggle, drawer) {
    document.documentElement.dataset.navDrawerOpen = open ? "true" : "false";
    toggle.setAttribute("aria-expanded", open ? "true" : "false");
    drawer.setAttribute("aria-hidden", open ? "false" : "true");
    if (open) drawer.removeAttribute("inert");
    else drawer.setAttribute("inert", "");
  }

  function initDrawer(store) {
    const toggle = document.querySelector("[data-nav-drawer-toggle]");
    const drawer = document.querySelector("[data-nav-drawer]");
    if (!toggle || !drawer) return;

    const stored = store?.getItem(drawerKey);
    let open = mobile.matches
      ? false
      : stored === "true" || stored === "false"
        ? stored === "true"
        : true;
    syncDrawer(open, toggle, drawer);

    function setOpen(next, { restoreFocus = false, persist = true } = {}) {
      open = next;
      syncDrawer(open, toggle, drawer);
      if (persist && !mobile.matches) {
        try { store?.setItem(drawerKey, open ? "true" : "false"); } catch {}
      }
      if (open && mobile.matches) {
        drawer.querySelector("a, summary, button, input, select, textarea")?.focus();
      } else if (restoreFocus) {
        toggle.focus();
      }
    }

    toggle.addEventListener("click", () => {
      setOpen(!open, { restoreFocus: open });
    });

    drawer.addEventListener("click", (event) => {
      if (mobile.matches && event.target.closest("a")) setOpen(false);
    });

    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && mobile.matches && open) {
        event.preventDefault();
        setOpen(false, { restoreFocus: true });
      }
    });

    mobile.addEventListener("change", (event) => {
      const stored = store?.getItem(drawerKey);
      const desktopOpen = stored === "false" ? false : true;
      setOpen(event.matches ? false : desktopOpen, { persist: false });
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
