//! Song browser — port of `Pages/SongList.razor`.
use leptos::prelude::*;

use crate::admin::i18n::TextKey;
use crate::admin::pages::media::browser::MediaBrowserPage;
use crate::admin::server_fns::media_browser::list_songs;

#[component]
pub fn SongsPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title=TextKey::Songs
            loader=move |q, p, ps, show_variants| list_songs(q, p, ps, show_variants)
        />
    }
}
