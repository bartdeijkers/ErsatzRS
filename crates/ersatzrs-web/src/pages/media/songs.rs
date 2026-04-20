//! Song browser — port of `Pages/SongList.razor`.
use leptos::prelude::*;

use crate::pages::media::browser::MediaBrowserPage;
use crate::server::media_browser::list_songs;

#[component]
pub fn SongsPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title="Songs"
            loader=move |q, p, ps| list_songs(q, p, ps)
        />
    }
}