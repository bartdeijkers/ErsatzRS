//! Movie browser — port of `Pages/MovieList.razor`.
use leptos::prelude::*;

use crate::admin::pages::media::browser::MediaBrowserPage;
use crate::admin::server_fns::media_browser::list_movies;

#[component]
pub fn MoviesPage() -> impl IntoView {
    view! {
        <MediaBrowserPage
            title="Movies"
            loader=move |q, p, ps| list_movies(q, p, ps)
        />
    }
}
