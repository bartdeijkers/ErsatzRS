//! Image browser — port of `Pages/ImageList.razor`.
use leptos::prelude::*;

use crate::admin::pages::media::browser::MediaBrowserPage;
use crate::admin::server_fns::media_browser::list_images;

#[component]
pub fn ImagesPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title="Images"
            loader=move |q, p, ps| list_images(q, p, ps)
        />
    }
}
