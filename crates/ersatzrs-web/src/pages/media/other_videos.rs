//! Other video browser — port of `Pages/OtherVideoList.razor`.
use leptos::prelude::*;

use crate::pages::media::browser::MediaBrowserPage;
use crate::server::media_browser::list_other_videos;

#[component]
pub fn OtherVideosPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title="Other Videos"
            loader=move |q, p, ps| list_other_videos(q, p, ps)
        />
    }
}