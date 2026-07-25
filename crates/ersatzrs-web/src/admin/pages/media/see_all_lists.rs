//! "See All" list-route pages for the search page's per-kind sections.
//!
//! ErsatzTV has dedicated `TelevisionSeasonSearchResults.razor`,
//! `EpisodeList.razor`, and `MusicVideoList.razor` routes. Each delegates here
//! to the shared [`MultiSelectBrowserPage`], which preserves their filtered
//! pager, letter bar, selection/actions, and card-navigation behavior.
//! The pinned upstream pages expose no library-filter combobox, so none is
//! invented on these parity routes.
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
