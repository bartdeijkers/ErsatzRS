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
            <input type="checkbox" id="nav-toggle" class=style::drawer_toggle/>
            <TopBar/>
            <div class=style::body>
                <NavDrawer/>
                <div class=style::content_col>
                    <main class=style::main>
                        {children()}
                    </main>
                </div>
            </div>
        </div>
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

            <nav class=style::topbar_nav>
                <a href="/iptv/channels.m3u" class=style::topbar_link target="_blank">"M3U"</a>
                <a href="/iptv/xmltv.xml" class=style::topbar_link target="_blank">"XMLTV"</a>
            </nav>
        </header>
    }
}
