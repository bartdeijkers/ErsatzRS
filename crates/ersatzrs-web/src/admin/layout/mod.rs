use leptos::prelude::*;

use crate::admin::layout::nav::NavDrawer;
use crate::admin::ui::primitives::icon::{icon_style, IconName};

mod badges;
mod nav;

stylance::import_crate_style!(style, "src/admin/layout/layout.module.css");

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
        </div>
    }
}

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
        return if is_dark_mode { "dark" } else { "light" }.to_owned();
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
            <label for="nav-toggle" class=style::hamburger title="Toggle navigation">
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
                    placeholder="Search"
                    aria-label="Search media"
                    autocomplete="off"
                />
            </form>

            <nav class=style::topbar_nav>
                <a href="/iptv/channels.m3u" class=style::topbar_link target="_blank">"M3U"</a>
                <a href="/iptv/xmltv.xml" class=style::topbar_link target="_blank">"XMLTV"</a>
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
