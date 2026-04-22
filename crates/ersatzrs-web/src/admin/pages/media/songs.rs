//! Song browser — port of `Pages/SongList.razor`.
use leptos::prelude::*;

use crate::admin::pages::media::browser::MediaBrowserPage;
use crate::admin::server_fns::media_browser::list_songs;

#[component]
pub fn SongsPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title="Songs"
            loader=move |q, p, ps| list_songs(q, p, ps)
        />
    }
}
