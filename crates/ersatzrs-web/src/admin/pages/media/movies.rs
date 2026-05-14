//! Movie browser — port of `Pages/MovieList.razor` (delegates to the shared
//! [`MultiSelectBrowserPage`]) plus the per-item `Pages/Movie.razor` details
//! page (`/media/movies/:id`).
//!
//! The list shell, multi-select toolbar, letter-bar, pager, and Add-to-
//! Collection / Add-to-Playlist / Show-Media-Info modals are shared across
//! movies/shows/artists/other-videos via
//! [`crate::admin::pages::media::multi_select_browser`].
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};
use crate::admin::server_fns::media_browser::{get_movie_details, MovieDetailRow};

stylance::import_crate_style!(
    detail_style,
    "src/admin/pages/media/movie_detail.module.css"
);

#[component]
pub fn MoviesPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::MOVIES/> }
}

// ---------------------------------------------------------------------------
// `/media/movies/:id` — details page port of `Pages/Movie.razor`.
// ---------------------------------------------------------------------------

#[component]
pub fn MoviePage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });

    let data: Resource<Option<MovieDetailRow>> = Resource::new_blocking(
        move || id,
        |id| async move { get_movie_details(id).await.ok().flatten() },
    );

    view! {
        <Suspense fallback=move || view! { <p class=detail_style::loading>"Loading…"</p> }>
            {move || {
                match data.get() {
                    Some(Some(movie)) => view! { <MovieDetailView movie/> }.into_any(),
                    Some(None) => view! {
                        <div class=detail_style::not_found>
                            <h1>"Movie not found"</h1>
                            <a href="/media/movies">"Back to movies"</a>
                        </div>
                    }
                    .into_any(),
                    None => view! { <p class=detail_style::loading>"Loading…"</p> }.into_any(),
                }
            }}
        </Suspense>
    }
}

#[component]
fn MovieDetailView(movie: MovieDetailRow) -> impl IntoView {
    let MovieDetailRow {
        id: _,
        title,
        year,
        plot,
        tagline: _,
        content_rating,
        release_date: _,
        poster_url,
        fanart_url,
        file_path,
        media_item_state,
        genres,
        tags,
        studios,
        directors,
        writers,
    } = movie;

    view! {
        <div class=detail_style::page>
            <div class=detail_style::fanart_container>
                <div class=detail_style::fanart_tint/>
                {fanart_url.map(|src| view! {
                    <img class=detail_style::fanart src=src alt="fan art"/>
                })}
            </div>

            <div class=detail_style::content>
                <div class=detail_style::hero>
                    {poster_url.map({
                        let title = title.clone();
                        move |src| view! {
                            <div class=detail_style::poster_wrap>
                                <img class=detail_style::poster src=src alt=title/>
                            </div>
                        }
                    })}
                    <div class=detail_style::header>
                        <h1 class=detail_style::title>{title}</h1>
                        {year.map(|y| view! { <p class=detail_style::year>{y.to_string()}</p> })}
                        {plot.filter(|p| !p.trim().is_empty()).map(|p| {
                            view! { <p class=detail_style::plot>{p}</p> }
                        })}
                        {match media_item_state {
                            1 => Some(view! {
                                <div class=detail_style::warning>
                                    "File not found"
                                    {file_path.clone().map(|p| view! { ": " <code>{p}</code> })}
                                </div>
                            }
                            .into_any()),
                            2 => Some(view! {
                                <div class=detail_style::warning>
                                    "Source unavailable"
                                    {file_path.clone().map(|p| view! { ": " <code>{p}</code> })}
                                </div>
                            }
                            .into_any()),
                            _ => None,
                        }}
                        <div class=detail_style::actions>
                            <a class=detail_style::btn_back href="/media/movies">"Back"</a>
                        </div>
                    </div>
                </div>

                <dl class=detail_style::facts>
                    {content_rating.filter(|s| !s.trim().is_empty()).map(|cr| {
                        view! {
                            <div class=detail_style::fact_row>
                                <dt>"Content Rating"</dt>
                                <dd>{cr}</dd>
                            </div>
                        }
                    })}
                    {fact_row("Studios", &studios)}
                    {fact_row("Directors", &directors)}
                    {fact_row("Writers", &writers)}
                    {fact_row("Genres", &genres)}
                    {fact_row("Tags", &tags)}
                    {file_path.filter(|s| !s.trim().is_empty()).map(|fp| {
                        view! {
                            <div class=detail_style::fact_row>
                                <dt>"Path"</dt>
                                <dd><code>{fp}</code></dd>
                            </div>
                        }
                    })}
                </dl>
            </div>
        </div>
    }
}

fn fact_row(label: &'static str, values: &[String]) -> Option<AnyView> {
    if values.iter().all(|s| s.trim().is_empty()) {
        return None;
    }
    let joined = values
        .iter()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    Some(
        view! {
            <div class=detail_style::fact_row>
                <dt>{label}</dt>
                <dd>{joined}</dd>
            </div>
        }
        .into_any(),
    )
}
