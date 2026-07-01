//! Song browser — port of `Pages/SongList.razor`.
use leptos::prelude::*;

use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};

#[component]
pub fn SongsPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::SONGS/> }
}
