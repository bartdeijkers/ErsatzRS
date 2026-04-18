//! Television show browser — port of `Pages/TelevisionShowList.razor`.
use leptos::prelude::*;

use crate::pages::media::browser::MediaBrowserPage;
use crate::server::media_browser::list_shows;

#[component]
pub fn ShowsPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title="TV Shows"
            loader=move |q, p, ps| list_shows(q, p, ps)
        />
    }
}
