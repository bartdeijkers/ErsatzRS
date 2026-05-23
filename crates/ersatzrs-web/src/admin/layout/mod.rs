use leptos::prelude::*;

use crate::admin::layout::nav::NavDrawer;
use crate::admin::ui::primitives::icon::{icon_style, IconName};

mod badges;
mod nav;

stylance::import_crate_style!(style, "src/admin/layout/layout.module.css");

#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    view! {
        <div class=style::shell>
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
        </div>
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
