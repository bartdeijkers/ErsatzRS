//! Image browser — port of `Pages/ImageList.razor`.
use leptos::prelude::*;

use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};

#[component]
pub fn ImagesPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::IMAGES/> }
}
