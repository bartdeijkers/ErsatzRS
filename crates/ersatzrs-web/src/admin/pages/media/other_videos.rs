//! Other video browser — port of `Pages/OtherVideoList.razor`.
//!
//! Delegates to the shared [`MultiSelectBrowserPage`].
//! `OtherVideoCardViewModel.HasMediaInfo = true`, so the per-card "Show Media
//! Info" entry is enabled.
use leptos::prelude::*;

use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};

#[component]
pub fn OtherVideosPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::OTHER_VIDEOS/> }
}
