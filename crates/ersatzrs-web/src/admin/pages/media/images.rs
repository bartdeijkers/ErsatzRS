//! Image browser — port of `Pages/ImageList.razor`.
use leptos::prelude::*;

use crate::admin::i18n::TextKey;
use crate::admin::pages::media::browser::MediaBrowserPage;
use crate::admin::server_fns::media_browser::list_images;

#[component]
pub fn ImagesPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title=TextKey::Images
            loader=move |q, p, ps, show_variants| list_images(q, p, ps, show_variants)
        />
    }
}
