//! Movie browser — port of `Pages/MovieList.razor` plus the per-item
//! `Pages/Movie.razor` details page (`/media/movies/:id`).
//!
//! Hydration notes:
//!
//! * The list page is a plain `#[component]`, not an island. Search and
//!   pagination are URL-driven (`?query=…&page=N`), mirroring `ErsatzTV`'s
//!   `NavigationManager.NavigateTo("media/movies/page/N")` pattern. This
//!   sidesteps hydration entirely for the toolbar/pager: pressing Enter on
//!   the search box submits a GET form; clicking the pager arrows is a plain
//!   `<a>` navigation. The grid re-renders from server data on each request,
//!   so SSR and client always agree.
//! * The details page is also a plain `#[component]` that owns a blocking
//!   resource and renders inside a single `<Suspense>` boundary — no
//!   prop-bearing island.
#![allow(clippy::too_many_lines)]
use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::server_fns::media_browser::{
    get_movie_details, list_movies, MediaCardRow, MovieDetailRow,
};
use crate::admin::ui::primitives::card::MediaCard;
use crate::admin::ui::primitives::letter_bar::{letter_bucket, FragmentLetterAnchor, LetterBar};

stylance::import_crate_style!(style, "src/admin/pages/media/browser.module.css");
// Re-imported so the pager can be styled from this file rather than via the
// shared `MediaCardPager` primitive. Most generated constants here are unused.
mod card_style {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/ui/primitives/card.module.css");
    pub use s::*;
}
stylance::import_crate_style!(
    detail_style,
    "src/admin/pages/media/movie_detail.module.css"
);

const PAGE_SIZE: u32 = 50;

#[component]
pub fn MoviesPage() -> impl IntoView {
    let query_map = use_query_map();
    let (initial_query, initial_page) = query_map.with_untracked(|p| {
        let q = p.get("query").unwrap_or_default();
        let pg = p
            .get("page")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1)
            .saturating_sub(1);
        (q, pg)
    });

    // Blocking so SSR resolves before HTML streams — same DOM on hydration.
    let query_for_resource = initial_query.clone();
    let data: Resource<(Vec<MediaCardRow>, u64)> = Resource::new_blocking(
        move || (query_for_resource.clone(), initial_page),
        |(q, p)| async move { list_movies(q, p, PAGE_SIZE).await.unwrap_or_default() },
    );

    let toolbar_query = initial_query.clone();

    view! {
        <div class=style::page>
            // Search is a plain GET form — same UX as ErsatzTV's
            // `NavigationManager.NavigateTo` redirect after a query change.
            <form method="get" action="/media/movies" class=style::toolbar>
                <h1 class=style::page_title>"Movies"</h1>
                <input
                    class=style::search
                    type="search"
                    name="query"
                    placeholder="Search…"
                    value=toolbar_query
                />
            </form>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|(rows, total)| {
                        let present: Signal<Option<HashSet<char>>> = Signal::derive({
                            let rows = rows.clone();
                            move || Some(rows.iter().map(|r| letter_bucket(&r.sort_title)).collect())
                        });
                        let query_for_links = initial_query.clone();
                        view! {
                            <LetterBar present=present/>
                            <Pager
                                query=query_for_links
                                page=initial_page
                                total=total
                            />
                            {if rows.is_empty() {
                                view! { <p class=style::empty>"No items to display."</p> }
                                    .into_any()
                            } else {
                                view! {
                                    <div class=style::grid>
                                        <FragmentLetterAnchor
                                            cards=rows
                                            sort_title=|r: &MediaCardRow| r.sort_title.as_str()
                                            render=move |r: MediaCardRow| {
                                                let href = format!("/media/movies/{}", r.id);
                                                view! {
                                                    <MediaCard
                                                        title=r.title
                                                        subtitle=r.subtitle
                                                        artwork_url=r.artwork_path
                                                        href=Some(href)
                                                    />
                                                }
                                                .into_any()
                                            }
                                        />
                                    </div>
                                }
                                .into_any()
                            }}
                        }
                        .into_any()
                    })
                }}
            </Suspense>
        </div>
    }
}

/// URL-driven pager. Renders summary + prev/next anchors that target the
/// same `/media/movies?query=…&page=N` route. Disabled state collapses the
/// anchor into a styled `<span>` so the DOM shape stays stable while still
/// being non-interactive.
#[component]
fn Pager(query: String, page: u32, total: u64) -> impl IntoView {
    let max_page = if total == 0 {
        0
    } else {
        (total - 1) / u64::from(PAGE_SIZE)
    };
    let start = page * PAGE_SIZE + 1;
    let end = ((page + 1) * PAGE_SIZE).min(u32::try_from(total).unwrap_or(u32::MAX));
    let summary = if total == 0 {
        "0 items".to_string()
    } else {
        format!("{start}–{end} of {total}")
    };

    let prev_disabled = page == 0;
    let next_disabled = u64::from(page) >= max_page;

    let build_link = |q: &str, p: u32| {
        let mut url = String::from("/media/movies");
        let mut params: Vec<String> = Vec::new();
        let trimmed = q.trim();
        if !trimmed.is_empty() {
            params.push(format!("query={}", urlencoding::encode(trimmed)));
        }
        if p > 0 {
            // ErsatzTV uses 1-based page numbers in the URL.
            params.push(format!("page={}", p + 1));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        url
    };

    let prev_href = build_link(&query, page.saturating_sub(1));
    let next_href = build_link(&query, page + 1);

    view! {
        <div class=card_style::pager>
            <div class=card_style::pager_left/>
            <div class=card_style::pager_right>
                <span>{summary}</span>
                {if prev_disabled {
                    view! {
                        <span class=format!("{} {}", card_style::pager_btn, card_style::pager_btn_disabled)>"‹"</span>
                    }
                    .into_any()
                } else {
                    view! { <a class=card_style::pager_btn href=prev_href>"‹"</a> }.into_any()
                }}
                {if next_disabled {
                    view! {
                        <span class=format!("{} {}", card_style::pager_btn, card_style::pager_btn_disabled)>"›"</span>
                    }
                    .into_any()
                } else {
                    view! { <a class=card_style::pager_btn href=next_href>"›"</a> }.into_any()
                }}
            </div>
        </div>
    }
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
