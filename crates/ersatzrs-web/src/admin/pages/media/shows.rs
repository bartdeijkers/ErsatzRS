//! Television show browser — port of `Pages/TelevisionShowList.razor`.
//!
//! Delegates to the shared [`MultiSelectBrowserPage`], which mirrors the
//! `ErsatzTV` `MultiSelectBase<TelevisionShowList>` page: letter-bar, search,
//! pager, hover-checkbox cards, Add-to-Collection / Add-to-Playlist actions.
//! `ShowCardViewModel.HasMediaInfo = false`, so the per-card "Show Media
//! Info" entry is suppressed.
use leptos::prelude::*;

use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};

#[component]
pub fn ShowsPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::SHOWS/> }
}
