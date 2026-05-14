//! Artist browser — port of `Pages/ArtistList.razor`.
//!
//! Delegates to the shared [`MultiSelectBrowserPage`].
//! `ArtistCardViewModel.HasMediaInfo = false`, so the per-card "Show Media
//! Info" entry is suppressed (artists are containers; their music videos
//! carry the media-info metadata).
use leptos::prelude::*;

use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};

#[component]
pub fn ArtistsPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::ARTISTS/> }
}
