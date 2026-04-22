use leptos::prelude::*;

stylance::import_crate_style!(style, "src/admin/home.module.css");

#[component]
pub fn HomePage() -> impl IntoView {
    view! {
        <div class=style::page_home>
            <h1 class=style::page_home_title>"ErsatzRS"</h1>
            <p class=style::page_home_subtitle>"IPTV server is running."</p>
            <p>
                <a href="/channels" class=style::page_home_link>"→ Channels"</a>
            </p>
        </div>
    }
}
