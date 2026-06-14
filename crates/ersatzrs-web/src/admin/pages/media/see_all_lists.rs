//! "See All" list-route pages for the search page's per-kind sections.
//!
//! These three kinds (seasons, episodes, music videos) have no
//! dedicated `*List.razor` in `ErsatzTV` — the search page is their only entry
//! point. Each delegates to the shared [`MultiSelectBrowserPage`], which
//! supplies the letter-bar + paginated grid + query echo that satisfies the
//! "See All" contract ("land on a page that shows all hits of this kind for
//! this query"). Full per-kind detail pages and the library-filter combobox
//! are deferred to a separate plan.
use leptos::prelude::*;

use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};

#[component]
pub fn SeasonsListPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::SEASONS/> }
}

#[component]
pub fn EpisodesListPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::EPISODES/> }
}

#[component]
pub fn MusicVideosListPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::MUSIC_VIDEOS/> }
}
