//! Artist browser — port of `Pages/ArtistList.razor`.
use leptos::prelude::*;

use crate::pages::media::browser::MediaBrowserPage;
use crate::server::media_browser::list_artists;

#[component]
pub fn ArtistsPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title="Artists"
            loader=move |q, p, ps| list_artists(q, p, ps)
        />
    }
}