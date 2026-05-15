#![allow(dead_code)]

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::admin::server_fns::search::{search_results, SearchCard, SearchCardKind, SearchResults};

stylance::import_crate_style!(style, "src/admin/pages/media/collections.module.css");

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[component]
pub fn SearchPage() -> impl IntoView {
    let query_map = use_query_map();
    let initial_query = query_map.with_untracked(|params| params.get("query").unwrap_or_default());

    view! { <SearchIsland initial_query/> }
}

#[island]
fn SearchIsland(initial_query: String) -> impl IntoView {
    let input = RwSignal::new(initial_query.clone());

    let data: Resource<(String, SearchResults)> = Resource::new(
        move || initial_query.clone(),
        move |query| async move {
            let results = search_results(query.clone()).await.unwrap_or_default();
            (query, results)
        },
    );

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Search"</h1>
                <input
                    class=style::search
                    type="text"
                    prop:value=move || input.get()
                    on:input=move |event| input.set(event_target_value(&event))
                    on:keydown=move |event| {
                        if event.key() == "Enter" {
                            let query = input.get_untracked();
                            let encoded = urlencoding::encode(query.trim()).into_owned();
                            set_browser_location(&format!("/search?query={encoded}"));
                        }
                    }
                />
                <button
                    class=style::btn_add
                    disabled=move || input.with(|value| value.trim().is_empty())
                    on:click=move |_| {
                        let query = input.get_untracked();
                        let encoded = urlencoding::encode(query.trim()).into_owned();
                        set_browser_location(&format!("/search?query={encoded}"));
                    }
                >
                    "Search"
                </button>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|(query, results)| {
                        if query.trim().is_empty() {
                            return view! {
                                <p class=style::empty>"Enter a search query."</p>
                            }
                            .into_any();
                        }

                        let rows = flatten_search_cards(results);
                        if rows.is_empty() {
                            return view! {
                                <p class=style::empty>"No media matched."</p>
                            }
                            .into_any();
                        }

                        view! {
                            <div class=style::table_wrap>
                                <table>
                                    <thead>
                                        <tr>
                                            <th>"Title"</th>
                                            <th>"Kind"</th>
                                            <th>"Subtitle"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|row| {
                                            view! {
                                                <tr>
                                                    <td>{row.title}</td>
                                                    <td>{card_kind_label(row.kind)}</td>
                                                    <td>{row.subtitle.unwrap_or_default()}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }
                        .into_any()
                    })
                }}
            </Suspense>
        </div>
    }
}

fn flatten_search_cards(results: SearchResults) -> Vec<SearchCard> {
    let mut rows = Vec::new();
    rows.extend(results.movies.cards);
    rows.extend(results.shows.cards);
    rows.extend(results.seasons.cards);
    rows.extend(results.episodes.cards);
    rows.extend(results.artists.cards);
    rows.extend(results.music_videos.cards);
    rows.extend(results.other_videos.cards);
    rows.extend(results.songs.cards);
    rows.extend(results.images.cards);
    rows.extend(results.remote_streams.cards);
    rows
}

fn card_kind_label(kind: SearchCardKind) -> &'static str {
    match kind {
        SearchCardKind::Movie => "Movie",
        SearchCardKind::Show => "Show",
        SearchCardKind::Season => "Season",
        SearchCardKind::Episode => "Episode",
        SearchCardKind::Artist => "Artist",
        SearchCardKind::MusicVideo => "Music Video",
        SearchCardKind::OtherVideo => "Other Video",
        SearchCardKind::Song => "Song",
        SearchCardKind::Image => "Image",
        SearchCardKind::RemoteStream => "Remote Stream",
    }
}
