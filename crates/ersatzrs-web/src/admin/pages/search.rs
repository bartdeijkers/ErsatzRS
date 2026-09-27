//! Global `/search` page — ports `$ERSATZTV_SRC/ErsatzTV/Pages/Search.razor`.
//!
//! Renders ten typed card grids (Movies, Shows, Seasons, Episodes, Artists,
//! Music Videos, Other Videos, Songs, Images, Remote Streams) with per-kind
//! counts, toolbar anchor links, a "See All" overflow link at 50, the
//! Add All / Add All To Playlist / Save As bulk actions, a select-mode toolbar
//! swap, and per-card Add-To-Collection / Add-To-Playlist menus.
//!
//! Hydration notes (see `TEST.md`):
//!
//! * The page wrapper is a plain `#[component]` that reads `?query=` from the
//!   URL and renders a single [`SearchIsland`] (scalar `String` prop only).
//! * The search form is plain SSR. The results grid is a scalar-prop island
//!   that owns its blocking resource, so per-card menu actions and modals
//!   hydrate together with the cards.
//! * Shift-range selection rebuilds the cross-section sorted card list and
//!   applies `MultiSelectBase.SelectClicked`'s range logic.

#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::admin::i18n::{
    add_to_collection_failed, add_to_playlist_failed, added_items_to_collection,
    added_items_to_playlist, content_variant_badge_label, episode_display_subtitle, search_count,
    season_display_name, selected_items_count, tr, TextKey,
};
use crate::admin::search_query::QuickFilterProjection;
use crate::admin::server_fns::media_browser::{
    add_media_items_to_collection, add_media_items_to_playlist_mixed, CollectionOption,
    PlaylistGroupOption, PlaylistOption,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::media_browser::{
    list_all_collections, list_all_playlist_groups, list_playlists_by_group,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::search::search_all_ids;
use crate::admin::server_fns::search::{
    project_quick_filters, search_results, SearchAllIds, SearchCard, SearchCardKind, SearchResults,
    SearchSection,
};
use crate::admin::ui::primitives::card::MediaCard;
use crate::admin::ui::primitives::dialog::Modal;
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastAction, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/search.module.css");
mod shell {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/pages/media/collections.module.css");
    pub use s::*;
}

/// One section's static metadata: anchor id, heading, See-All list route.
#[derive(Clone, Copy)]
struct SectionMeta {
    kind: SearchCardKind,
    anchor: &'static str,
    label: TextKey,
    list_path: &'static str,
}

const SECTIONS: [SectionMeta; 10] = [
    SectionMeta {
        kind: SearchCardKind::Movie,
        anchor: "movies",
        label: TextKey::Movies,
        list_path: "/media/movies",
    },
    SectionMeta {
        kind: SearchCardKind::Show,
        anchor: "shows",
        label: TextKey::TvShows,
        list_path: "/media/tv/shows",
    },
    SectionMeta {
        kind: SearchCardKind::Season,
        anchor: "seasons",
        label: TextKey::Seasons,
        list_path: "/media/tv/seasons",
    },
    SectionMeta {
        kind: SearchCardKind::Episode,
        anchor: "episodes",
        label: TextKey::Episodes,
        list_path: "/media/tv/episodes",
    },
    SectionMeta {
        kind: SearchCardKind::Artist,
        anchor: "artists",
        label: TextKey::Artists,
        list_path: "/media/music/artists",
    },
    SectionMeta {
        kind: SearchCardKind::MusicVideo,
        anchor: "music_videos",
        label: TextKey::MusicVideos,
        list_path: "/media/music/videos",
    },
    SectionMeta {
        kind: SearchCardKind::OtherVideo,
        anchor: "other_videos",
        label: TextKey::OtherVideos,
        list_path: "/media/other/videos",
    },
    SectionMeta {
        kind: SearchCardKind::Song,
        anchor: "songs",
        label: TextKey::Songs,
        list_path: "/media/music/songs",
    },
    SectionMeta {
        kind: SearchCardKind::Image,
        anchor: "images",
        label: TextKey::Images,
        list_path: "/media/images",
    },
    SectionMeta {
        kind: SearchCardKind::RemoteStream,
        anchor: "remote_streams",
        label: TextKey::RemoteStreams,
        list_path: "/media/remote/streams",
    },
];

fn section_for(results: &SearchResults, kind: SearchCardKind) -> &SearchSection {
    match kind {
        SearchCardKind::Movie => &results.movies,
        SearchCardKind::Show => &results.shows,
        SearchCardKind::Season => &results.seasons,
        SearchCardKind::Episode => &results.episodes,
        SearchCardKind::Artist => &results.artists,
        SearchCardKind::MusicVideo => &results.music_videos,
        SearchCardKind::OtherVideo => &results.other_videos,
        SearchCardKind::Song => &results.songs,
        SearchCardKind::Image => &results.images,
        SearchCardKind::RemoteStream => &results.remote_streams,
    }
}

/// Cross-section sorted card list used by shift-range select. Sections in
/// `SECTIONS` order; cards within a section are already sorted by
/// `sort_title` server-side. Mirrors `Search.razor`'s `GetSortedItems`.
fn sorted_cards(results: &SearchResults) -> Vec<(SearchCardKind, i64)> {
    let mut out = Vec::new();
    for meta in SECTIONS {
        for card in &section_for(results, meta.kind).cards {
            out.push((meta.kind, card.media_item_id));
        }
    }
    out
}

fn see_all_href(list_path: &str, query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        format!("{list_path}?page=1")
    } else {
        format!("{list_path}?query={}&page=1", urlencoding::encode(trimmed))
    }
}

fn search_query_href(query: &str) -> String {
    let query = query.trim();
    if query.is_empty() {
        "/search".to_owned()
    } else {
        format!("/search?query={}", urlencoding::encode(query))
    }
}

fn create_smart_collection_href(query: &str) -> String {
    let return_to = search_query_href(query);
    format!(
        "/media/smart-collections/add?query={}&return_to={}",
        urlencoding::encode(query),
        urlencoding::encode(&return_to)
    )
}

#[cfg(test)]
mod tests {
    use super::{create_smart_collection_href, see_all_href};

    #[test]
    fn see_all_links_preserve_encoded_query_and_first_page() {
        assert_eq!(
            see_all_href("/media/tv/episodes", "title:\"A & B\""),
            "/media/tv/episodes?query=title%3A%22A%20%26%20B%22&page=1"
        );
        assert_eq!(
            see_all_href("/media/music/videos", "   "),
            "/media/music/videos?page=1"
        );
    }

    #[test]
    fn smart_collection_handoff_preserves_query_and_exact_search_return() {
        assert_eq!(
            create_smart_collection_href("genre:\"Science Fiction\" AND year:[1980 TO 1999]"),
            "/media/smart-collections/add?query=genre%3A%22Science%20Fiction%22%20AND%20year%3A%5B1980%20TO%201999%5D&return_to=%2Fsearch%3Fquery%3Dgenre%253A%2522Science%2520Fiction%2522%2520AND%2520year%253A%255B1980%2520TO%25201999%255D"
        );
    }
}

#[component]
pub fn SearchPage() -> impl IntoView {
    let query_map = use_query_map();
    let initial_query = query_map.with(|params| params.get("query").unwrap_or_default());
    let input_value = initial_query.clone();
    let query_for_resource = initial_query.clone();
    let query_for_filters = initial_query.clone();
    let data = Resource::new_blocking(
        move || query_for_resource.clone(),
        move |query| async move {
            if query.trim().is_empty() {
                Ok(SearchResults::default())
            } else {
                search_results(query).await
            }
        },
    );
    let quick_filters =
        Resource::new_blocking(move || query_for_filters.clone(), project_quick_filters);

    view! {
        <div class=shell::page>
            <form method="get" action="/search" class=shell::toolbar>
                <h1 class=shell::page_title>{tr(TextKey::Search)}</h1>
                <label class=style::visually_hidden for="global-search-query">
                    {tr(TextKey::SearchMedia)}
                </label>
                <div class=shell::search_field>
                    <input
                        id="global-search-query"
                        class=shell::search
                        type="search"
                        name="query"
                        placeholder=tr(TextKey::Search)
                        value=input_value
                    />
                </div>
                <button
                    type="submit"
                    class=format!("{} {}", shell::btn_add, style::search_primary_action)
                >
                    {tr(TextKey::Search)}
                </button>
            </form>
            <Suspense fallback=admin_loading_resource_view>
                {move || {
                    quick_filters.get().map(|result| admin_result_view(result, |projection| {
                        view! { <QuickFilterDrawer projection/> }.into_any()
                    }))
                }}
            </Suspense>

            <Suspense fallback=admin_loading_resource_view>
                {move || {
                    data.get().map(|result| admin_result_view(result, |results| {
                        view! {
                            <SearchIsland query=initial_query.clone() results=results/>
                        }
                        .into_any()
                    }))
                }}
            </Suspense>
            <SearchCardActionFallback/>
        </div>
    }
}

#[component]
fn QuickFilterDrawer(projection: QuickFilterProjection) -> impl IntoView {
    let active_count = projection.active_count;
    let media_kind = projection.media_kind.clone();
    let genre = projection.genre.clone();
    let year_from = projection.year_from.clone();
    let year_to = projection.year_to.clone();

    view! {
        <section class=style::filter_surface aria-label=tr(TextKey::Filters)>
            <details class=style::filter_disclosure>
                <summary class=style::filter_summary>
                    <span>{tr(TextKey::Filters)}</span>
                    <span class=style::filter_count>
                        {format!("{active_count} {}", tr(TextKey::ActiveFilters))}
                    </span>
                </summary>
                <form method="get" action="/admin-api/search/quick-filters" class=style::filter_form>
                    <input type="hidden" name="base_query" value=projection.base_query.clone()/>
                    <label class=style::filter_field>
                        <span>{tr(TextKey::Type)}</span>
                        <select name="media_kind">
                            <option value="" selected=media_kind.is_empty()>{tr(TextKey::All)}</option>
                            {SECTIONS.into_iter().map(|meta| {
                                let value = meta.kind.media_kind();
                                view! {
                                    <option value=value selected=media_kind == value>{tr(meta.label)}</option>
                                }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class=style::filter_field>
                        <span>{tr(TextKey::Genres)}</span>
                        <input type="text" name="genre" value=genre.clone()/>
                    </label>
                    <fieldset class=style::year_fields>
                        <legend>{tr(TextKey::Year)}</legend>
                        <label class=style::filter_field>
                            <span>{tr(TextKey::FromYear)}</span>
                            <input type="number" name="year_from" min="1" max="9999" value=year_from.clone()/>
                        </label>
                        <label class=style::filter_field>
                            <span>{tr(TextKey::ToYear)}</span>
                            <input type="number" name="year_to" min="1" max="9999" value=year_to.clone()/>
                        </label>
                    </fieldset>
                    <div class=style::filter_actions>
                        <p>{tr(TextKey::SearchFiltersHelp)}</p>
                        <button type="submit" class=style::action_btn>{tr(TextKey::Apply)}</button>
                    </div>
                </form>
            </details>

            <div
                class=style::active_filters
                style:display=if active_count == 0 { "none" } else { "" }
                aria-label=tr(TextKey::ActiveFilters)
            >
                <a
                    class=style::filter_chip
                    href=search_query_href(&projection.clear_media_kind_query)
                    style:display=if media_kind.is_empty() { "none" } else { "" }
                >
                    {format!("{}: {} ×", tr(TextKey::Type), media_kind)}
                </a>
                <a
                    class=style::filter_chip
                    href=search_query_href(&projection.clear_genre_query)
                    style:display=if genre.is_empty() { "none" } else { "" }
                >
                    {format!("{}: {} ×", tr(TextKey::Genres), genre)}
                </a>
                <a
                    class=style::filter_chip
                    href=search_query_href(&projection.clear_year_query)
                    style:display=if year_from.is_empty() && year_to.is_empty() { "none" } else { "" }
                >
                    {format!("{}: {}–{} ×", tr(TextKey::Year), year_from, year_to)}
                </a>
                <a class=style::clear_filters href=search_query_href(&projection.base_query)>
                    {tr(TextKey::ClearFilters)}
                </a>
            </div>
        </section>
    }
}

#[island]
fn SearchIsland(query: String, results: SearchResults) -> impl IntoView {
    view! {
        {if query.trim().is_empty() {
            view! {
                <p class=style::empty>{tr(TextKey::EnterSearchQuery)}</p>
            }
            .into_any()
        } else {
            let any = SECTIONS
                .iter()
                .any(|m| !section_for(&results, m.kind).cards.is_empty());
            if any {
                view! { <SearchResultsView query=query results=results/> }.into_any()
            } else {
                let create_collection_href = create_smart_collection_href(&query);
                view! {
                    <div class=style::empty_state>
                        <p class=style::empty>{tr(TextKey::NoMediaMatched)}</p>
                        <a class=style::action_link_secondary href=create_collection_href>
                            {tr(TextKey::CreateSmartCollection)}
                        </a>
                    </div>
                }
                .into_any()
            }
        }}
    }
}

#[component]
fn SearchResultsView(query: String, results: SearchResults) -> impl IntoView {
    // Selection keyed by (kind, media_item_id) — search results are mixed-kind.
    let selected: RwSignal<HashSet<(SearchCardKind, i64)>> = RwSignal::new(HashSet::new());
    let recently_selected: RwSignal<Option<(SearchCardKind, i64)>> = RwSignal::new(None);
    let select_count = Signal::derive(move || selected.with(HashSet::len));
    let select_mode = Signal::derive(move || select_count.get() > 0);

    let collection_open = RwSignal::new(false);
    let playlist_open = RwSignal::new(false);
    // When true, the bulk dialogs operate on the full result set ("Add All");
    // otherwise on the current selection.
    let bulk_all = RwSignal::new(false);

    let query_for_dialogs = query.clone();
    let sorted = StoredValue::new(sorted_cards(&results));

    // Toggle / shift-range select. Mirrors `MultiSelectBase.SelectClicked`:
    // a hit that is already selected toggles off; otherwise a shift-click with
    // a prior anchor selects the inclusive range, and the click sets the new
    // anchor.
    let on_select = move |kind: SearchCardKind, id: i64, shift: bool| {
        let key = (kind, id);
        let was_selected = selected.with_untracked(|s| s.contains(&key));
        if was_selected {
            selected.update(|s| {
                s.remove(&key);
            });
            return;
        }
        if shift {
            if let Some(anchor) = recently_selected.get_untracked() {
                let list = sorted.get_value();
                if let (Some(mut start), Some(mut finish)) = (
                    list.iter().position(|k| *k == anchor),
                    list.iter().position(|k| *k == key),
                ) {
                    if start > finish {
                        std::mem::swap(&mut start, &mut finish);
                    }
                    selected.update(|s| {
                        for k in &list[start..finish] {
                            s.insert(*k);
                        }
                    });
                }
            }
        }
        recently_selected.set(Some(key));
        selected.update(|s| {
            s.insert(key);
        });
    };
    let on_select = Callback::new(move |(kind, id, shift): (SearchCardKind, i64, bool)| {
        on_select(kind, id, shift);
    });

    let clear_selection = Callback::new(move |()| {
        selected.update(HashSet::clear);
        recently_selected.set(None);
    });

    let results_for_sections = StoredValue::new(results);

    view! {
        <SearchToolbar
            query=query.clone()
            results_store=results_for_sections
            select_mode=select_mode
            select_count=select_count
            on_add_all_collection=Callback::new(move |()| {
                bulk_all.set(true);
                collection_open.set(true);
            })
            on_add_all_playlist=Callback::new(move |()| {
                bulk_all.set(true);
                playlist_open.set(true);
            })
            on_sel_collection=Callback::new(move |()| {
                bulk_all.set(false);
                collection_open.set(true);
            })
            on_sel_playlist=Callback::new(move |()| {
                bulk_all.set(false);
                playlist_open.set(true);
            })
            on_clear=clear_selection
        />

        <div class=style::results>
            {SECTIONS
                .iter()
                .filter(|m| {
                    results_for_sections
                        .with_value(|r| !section_for(r, m.kind).cards.is_empty())
                })
                .map(|meta| {
                    let meta = *meta;
                    let (cards, total) = results_for_sections.with_value(|r| {
                        let s = section_for(r, meta.kind);
                        (s.cards.clone(), s.total_count)
                    });
                    let q_for_link = query.clone();
                    view! {
                            <section class=style::section>
                                <div class=style::section_head>
                                    <h2 id=meta.anchor class=style::section_title>
                                        {tr(meta.label)}
                                    </h2>
                                    <a
                                        class=style::see_all
                                        href=see_all_href(meta.list_path, &q_for_link)
                                        style:display=move || { if total > 50 { "" } else { "none" } }
                                    >
                                        {tr(TextKey::SeeAll)}
                                    </a>
                                </div>
                            <div class=style::grid>
                                {cards
                                    .into_iter()
                                    .map(|card| view! {
                                        <SearchResultCard
                                            card=card
                                            selected=selected
                                            select_mode=select_mode
                                            on_select=on_select
                                            on_add_to_collection=Callback::new(move |key: (SearchCardKind, i64)| {
                                                selected.update(|s| {
                                                    s.clear();
                                                    s.insert(key);
                                                });
                                                bulk_all.set(false);
                                                collection_open.set(true);
                                            })
                                            on_add_to_playlist=Callback::new(move |key: (SearchCardKind, i64)| {
                                                selected.update(|s| {
                                                    s.clear();
                                                    s.insert(key);
                                                });
                                                bulk_all.set(false);
                                                playlist_open.set(true);
                                            })
                                        />
                                    })
                                    .collect_view()}
                            </div>
                        </section>
                    }
                })
                .collect_view()}
        </div>

        <AddToCollectionModal
            open=collection_open
            query=query_for_dialogs.clone()
            selected=selected
            bulk_all=bulk_all
        />
        <AddToPlaylistModal
            open=playlist_open
            query=query_for_dialogs.clone()
            selected=selected
            bulk_all=bulk_all
        />
    }
}

#[component]
fn SearchToolbar(
    query: String,
    results_store: StoredValue<SearchResults>,
    select_mode: Signal<bool>,
    select_count: Signal<usize>,
    on_add_all_collection: Callback<()>,
    on_add_all_playlist: Callback<()>,
    on_sel_collection: Callback<()>,
    on_sel_playlist: Callback<()>,
    on_clear: Callback<()>,
) -> impl IntoView {
    let counts: Vec<(SectionMeta, u32)> = SECTIONS
        .iter()
        .filter_map(|m| {
            let total = results_store.with_value(|r| section_for(r, m.kind).total_count);
            (total > 0).then_some((*m, total))
        })
        .collect();

    let create_collection_href = create_smart_collection_href(&query);

    view! {
        <div class=format!("{} {}", shell::toolbar, style::results_toolbar)>
            <div
                class=style::toolbar_mode
                style:display=move || if select_mode.get() { "none" } else { "" }
            >
                <div class=style::count_links>
                    {counts
                        .into_iter()
                        .map(|(meta, total)| view! {
                            <a
                                class=style::count_link
                                href=format!("#{}", meta.anchor)
                            >
                                {search_count(total, tr(meta.label))}
                            </a>
                        })
                        .collect_view()}
                </div>
                <div class=style::actions>
                    <button
                        class=style::action_btn
                        on:click=move |_| on_add_all_collection.run(())
                    >
                        {tr(TextKey::AddAllToCollection)}
                    </button>
                    <button
                        class=style::action_btn
                        on:click=move |_| on_add_all_playlist.run(())
                    >
                        {tr(TextKey::AddAllToPlaylist)}
                    </button>
                    <a
                        class=style::action_link_secondary
                        href=create_collection_href
                    >
                        {tr(TextKey::CreateSmartCollection)}
                    </a>
                </div>
            </div>
            <div
                class=style::toolbar_mode
                style:display=move || if select_mode.get() { "" } else { "none" }
            >
                <div class=style::count_links>
                    <span class=style::query_echo>
                        {move || {
                            selected_items_count(select_count.get())
                        }}
                    </span>
                </div>
                <div class=style::actions>
                    <button
                        class=style::action_btn
                        on:click=move |_| on_sel_collection.run(())
                    >
                        {tr(TextKey::AddToCollection)}
                    </button>
                    <button
                        class=style::action_btn
                        on:click=move |_| on_sel_playlist.run(())
                    >
                        {tr(TextKey::AddToPlaylist)}
                    </button>
                    <button
                        class=style::action_btn_secondary
                        on:click=move |_| on_clear.run(())
                    >
                        {tr(TextKey::ClearSelection)}
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
fn SearchResultCard(
    card: SearchCard,
    selected: RwSignal<HashSet<(SearchCardKind, i64)>>,
    select_mode: Signal<bool>,
    on_select: Callback<(SearchCardKind, i64, bool)>,
    on_add_to_collection: Callback<(SearchCardKind, i64)>,
    on_add_to_playlist: Callback<(SearchCardKind, i64)>,
) -> impl IntoView {
    let kind = card.kind;
    let id = card.media_item_id;
    let key = (kind, id);
    let is_selected = Signal::derive(move || selected.with(|s| s.contains(&key)));
    // `MediaCard` drives its checkbox off an `RwSignal<bool>`; mirror the
    // selection set into it so the card surface reflects multi-select state.
    let card_selected = RwSignal::new(is_selected.get_untracked());
    Effect::new(move |_| card_selected.set(is_selected.get()));

    let wrap_class = move || {
        if is_selected.get() {
            format!("{} {}", style::card_wrap, style::selected)
        } else {
            style::card_wrap.to_string()
        }
    };

    let href = (!card.href.is_empty()).then(|| card.href.clone());
    let subtitle = if kind == SearchCardKind::Season {
        card.season_number
            .map(|number| season_display_name(i64::from(number)))
            .or(card.subtitle)
    } else if kind == SearchCardKind::Episode {
        episode_display_subtitle(
            card.show_title.as_deref(),
            card.season_number.map(i64::from),
            card.episode_number.map(i64::from),
        )
        .or(card.subtitle)
    } else {
        card.subtitle
    };

    view! {
        <div class=wrap_class>
            <MediaCard
                title=card.title
                subtitle=subtitle
                href=href
                artwork_url=card.artwork_url
                badge=content_variant_badge_label(
                    card.variant.content_kind,
                    card.variant.sub_kind.as_deref(),
                )
                placeholder=card.placeholder
                state=card.state
                select_mode=select_mode
                selected=Some(card_selected)
                on_select=Some(Callback::new(move |e: MouseEvent| {
                    on_select.run((kind, id, e.shift_key()));
                }))
                on_add_to_collection=Some(Callback::new(move |()| {
                    on_add_to_collection.run((kind, id));
                }))
                on_add_to_playlist=Some(Callback::new(move |()| {
                    on_add_to_playlist.run((kind, id));
                }))
                menu_media_item_id=Some(id)
            />
        </div>
    }
}

#[component]
fn SearchCardActionFallback() -> impl IntoView {
    view! {
        <div
            id="search-card-action-modal"
            class=style::action_modal_backdrop
            style="display:none"
            role="dialog"
            aria-modal="true"
            aria-labelledby="search-card-action-title"
        >
            <div class=style::action_modal_panel>
                <h2 id="search-card-action-title" class=style::action_modal_title>
                    {tr(TextKey::AddToCollection)}
                </h2>
                <div
                    id="search-card-collection-fields"
                    class=style::action_modal_fields
                >
                    <label for="search-card-collection-select">{tr(TextKey::Collection)}</label>
                    <select id="search-card-collection-select"></select>
                    <label for="search-card-collection-name">{tr(TextKey::NewCollectionName)}</label>
                    <input
                        id="search-card-collection-name"
                        type="text"
                        autocomplete="off"
                    />
                </div>
                <div
                    id="search-card-playlist-fields"
                    class=style::action_modal_fields
                    style="display:none"
                >
                    <label for="search-card-playlist-group">{tr(TextKey::PlaylistGroup)}</label>
                    <select id="search-card-playlist-group"></select>
                    <label for="search-card-playlist-select">{tr(TextKey::Playlist)}</label>
                    <select id="search-card-playlist-select"></select>
                </div>
                <p id="search-card-action-message" class=style::action_modal_message></p>
                <div class=style::action_modal_buttons>
                    <button
                        id="search-card-action-cancel"
                        type="button"
                        class=style::action_btn_secondary
                    >
                        {tr(TextKey::Cancel)}
                    </button>
                    <button
                        id="search-card-action-submit"
                        type="button"
                        class=style::action_btn
                    >
                        {tr(TextKey::AddToCollection)}
                    </button>
                </div>
            </div>
        </div>
        <script inner_html=search_card_action_script()></script>
    }
}

fn search_card_action_script() -> String {
    let labels = serde_json::json!({
        "addToCollection": tr(TextKey::AddToCollection),
        "addToPlaylist": tr(TextKey::AddToPlaylist),
        "requestFailed": tr(TextKey::RequestFailed),
        "loadingPlaylists": tr(TextKey::LoadingPlaylists),
        "noPlaylistsInGroup": tr(TextKey::NoPlaylistsInGroup),
        "loadingCollections": tr(TextKey::LoadingCollections),
        "loadingPlaylistGroups": tr(TextKey::LoadingPlaylistGroups),
        "newCollection": tr(TextKey::NewCollection),
        "addingItem": tr(TextKey::AddingItem),
        "addingItemToPlaylist": tr(TextKey::AddingItemToPlaylist),
        "pleaseChoosePlaylist": tr(TextKey::PleaseChoosePlaylist),
        "addedItemToPlaylist": tr(TextKey::AddedItemToPlaylist),
    });
    r#"
(() => {
  if (window.__ersatzrsSearchCardActions) return;
  window.__ersatzrsSearchCardActions = true;
  const labels = __ERSATZRS_SEARCH_LABELS__;

  const state = { mode: "", mediaItemId: 0 };
  const byId = (id) => document.getElementById(id);
  const modal = () => byId("search-card-action-modal");
  const message = (value) => {
    const el = byId("search-card-action-message");
    if (el) el.textContent = value;
  };
  const show = (id, visible) => {
    const el = byId(id);
    if (el) el.style.display = visible ? "" : "none";
  };
  const json = async (url, options) => {
    const response = await fetch(url, options);
    if (!response.ok) {
      let detail = labels.requestFailed;
      try {
        const body = await response.json();
        detail = body.error || detail;
      } catch (_) {}
      throw new Error(detail);
    }
    return response.json();
  };
  const fillSelect = (id, rows, first) => {
    const select = byId(id);
    if (!select) return;
    select.innerHTML = "";
    const all = first ? [first, ...rows] : rows;
    for (const row of all) {
      const option = document.createElement("option");
      option.value = String(row.id);
      option.textContent = row.name;
      select.appendChild(option);
    }
  };
  const close = () => {
    const el = modal();
    if (el) el.style.display = "none";
  };
  const openModal = (mode, mediaItemId) => {
    state.mode = mode;
    state.mediaItemId = mediaItemId;
    show("search-card-collection-fields", mode === "collection");
    show("search-card-playlist-fields", mode === "playlist");
    const title = byId("search-card-action-title");
    const submit = byId("search-card-action-submit");
    if (title) title.textContent = mode === "collection" ? labels.addToCollection : labels.addToPlaylist;
    if (submit) submit.textContent = mode === "collection" ? labels.addToCollection : labels.addToPlaylist;
    const el = modal();
    if (el) el.style.display = "";
  };

  byId("search-card-collection-select")?.addEventListener("change", () => {
    const selected = Number(byId("search-card-collection-select")?.value || -1);
    const name = byId("search-card-collection-name");
    if (name) name.disabled = selected > 0;
  });

  byId("search-card-playlist-group")?.addEventListener("change", async () => {
    const groupId = Number(byId("search-card-playlist-group")?.value || 0);
    message(labels.loadingPlaylists);
    try {
      const playlists = groupId > 0
        ? await json(`/admin-api/media/actions/playlists?group_id=${groupId}`)
        : [];
      fillSelect("search-card-playlist-select", playlists);
      message(playlists.length ? "" : labels.noPlaylistsInGroup);
    } catch (error) {
      message(error.message);
    }
  });

  document.addEventListener("click", async (event) => {
    const button = event.target.closest("[data-search-card-action]");
    if (!button) return;
    const mediaItemId = Number(button.dataset.mediaItemId || 0);
    if (!mediaItemId) return;

    event.preventDefault();
    event.stopPropagation();
    button.closest("details")?.removeAttribute("open");

    const mode = button.dataset.searchCardAction;
    openModal(mode, mediaItemId);
    message(mode === "collection" ? labels.loadingCollections : labels.loadingPlaylistGroups);

    try {
      if (mode === "collection") {
        const name = byId("search-card-collection-name");
        if (name) {
          name.value = "";
          name.disabled = false;
        }
        const collections = await json("/admin-api/media/actions/collections");
        fillSelect("search-card-collection-select", collections, { id: -1, name: labels.newCollection });
        message("");
      } else if (mode === "playlist") {
        const groups = await json("/admin-api/media/actions/playlist-groups");
        fillSelect("search-card-playlist-group", groups);
        message("");
        byId("search-card-playlist-group")?.dispatchEvent(new Event("change"));
      }
    } catch (error) {
      message(error.message);
    }
  });

  byId("search-card-action-cancel")?.addEventListener("click", close);
  modal()?.addEventListener("click", (event) => {
    if (event.target === modal()) close();
  });
  byId("search-card-action-submit")?.addEventListener("click", async () => {
    if (!state.mediaItemId) return;
    message(state.mode === "collection" ? labels.addingItem : labels.addingItemToPlaylist);
    try {
      if (state.mode === "collection") {
        const selected = Number(byId("search-card-collection-select")?.value || -1);
        const name = byId("search-card-collection-name")?.value || "";
        const result = await json("/admin-api/media/actions/collections/add", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            media_item_id: state.mediaItemId,
            collection_id: selected > 0 ? selected : null,
            new_name: selected > 0 ? null : name
          })
        });
        window.location.href = `/media/collections/${result.id}`;
      } else if (state.mode === "playlist") {
        const playlistId = Number(byId("search-card-playlist-select")?.value || 0);
        if (!playlistId) {
          message(labels.pleaseChoosePlaylist);
          return;
        }
        await json("/admin-api/media/actions/playlists/add", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ media_item_id: state.mediaItemId, playlist_id: playlistId })
        });
        message(labels.addedItemToPlaylist);
        setTimeout(close, 600);
      }
    } catch (error) {
      message(error.message);
    }
  });
})();
"#
    .replace("__ERSATZRS_SEARCH_LABELS__", &labels.to_string())
}

// ---------------------------------------------------------------------------
// Bulk-action dialogs. Both resolve ids from either the current selection
// (mixed-kind) or the full search result set ("Add All"), then call the
// kind-agnostic collection insert / the mixed playlist insert.
// ---------------------------------------------------------------------------

const NEW_COLLECTION_ID: i64 = -1;

#[cfg(feature = "ssr")]
#[allow(clippy::unused_async)]
async fn resolve_ids(
    _query: String,
    _selected: Vec<(SearchCardKind, i64)>,
    _bulk_all: bool,
) -> Result<SearchAllIds, ServerFnError> {
    // Server-side rendering never dispatches these actions; the island runs
    // on wasm. Provided only so the module type-checks under `ssr`.
    Ok(SearchAllIds::default())
}

#[cfg(target_arch = "wasm32")]
async fn resolve_ids(
    query: String,
    selected: Vec<(SearchCardKind, i64)>,
    bulk_all: bool,
) -> Result<SearchAllIds, ServerFnError> {
    if bulk_all {
        return search_all_ids(query).await;
    }
    let mut ids = SearchAllIds::default();
    for (kind, id) in selected {
        match kind {
            SearchCardKind::Movie => ids.movie_ids.push(id),
            SearchCardKind::Show => ids.show_ids.push(id),
            SearchCardKind::Season => ids.season_ids.push(id),
            SearchCardKind::Episode => ids.episode_ids.push(id),
            SearchCardKind::Artist => ids.artist_ids.push(id),
            SearchCardKind::MusicVideo => ids.music_video_ids.push(id),
            SearchCardKind::OtherVideo => ids.other_video_ids.push(id),
            SearchCardKind::Song => ids.song_ids.push(id),
            SearchCardKind::Image => ids.image_ids.push(id),
            SearchCardKind::RemoteStream => ids.remote_stream_ids.push(id),
        }
    }
    Ok(ids)
}

#[component]
fn AddToCollectionModal(
    open: RwSignal<bool>,
    query: String,
    selected: RwSignal<HashSet<(SearchCardKind, i64)>>,
    bulk_all: RwSignal<bool>,
) -> impl IntoView {
    let options: RwSignal<Vec<CollectionOption>> = RwSignal::new(Vec::new());
    let collection_id: RwSignal<i64> = RwSignal::new(NEW_COLLECTION_ID);
    let new_name: RwSignal<String> = RwSignal::new(String::new());
    let toast = use_toast();
    let no_items_to_add = tr(TextKey::NoItemsToAdd).to_owned();
    let collection_name_required = tr(TextKey::CollectionNameRequired).to_owned();

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        leptos::task::spawn_local(async move {
            match list_all_collections().await {
                Ok(loaded) => options.set(loaded),
                Err(error) => toast.push(
                    format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                    ToastSeverity::Error,
                ),
            }
        });
    });

    let query_for_action = query.clone();
    let submit_action = Action::new(move |(): &()| {
        let q = query_for_action.clone();
        let sel: Vec<(SearchCardKind, i64)> =
            selected.with_untracked(|s| s.iter().copied().collect());
        let all = bulk_all.get_untracked();
        let chosen = collection_id.get_untracked();
        let name = new_name.get_untracked();
        let no_items_to_add = no_items_to_add.clone();
        let collection_name_required = collection_name_required.clone();
        async move {
            let ids = resolve_ids(q, sel, all).await?;
            let flat = ids.flatten();
            if flat.is_empty() {
                return Err(ServerFnError::new(no_items_to_add));
            }
            let target = if chosen == NEW_COLLECTION_ID {
                let name = name.trim();
                if name.is_empty() {
                    return Err(ServerFnError::new(collection_name_required));
                }
                crate::admin::server_fns::media_browser::create_collection(name.to_string())
                    .await?
                    .id
            } else {
                chosen
            };
            let added = add_media_items_to_collection(target, flat.clone()).await?;
            Ok::<(usize, u64, i64), ServerFnError>((flat.len(), added, target))
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _, target_id)) => {
                    // Stay on the search results; the toast offers the
                    // explicit navigation path instead.
                    toast.push_with_action(
                        added_items_to_collection(requested),
                        ToastSeverity::Success,
                        ToastAction {
                            label: tr(TextKey::ViewCollection).to_string(),
                            href: format!("/media/collections/{target_id}"),
                        },
                    );
                    selected.update(HashSet::clear);
                    open.set(false);
                    collection_id.set(NEW_COLLECTION_ID);
                    new_name.set(String::new());
                }
                Err(e) => toast.push(add_to_collection_failed(e), ToastSeverity::Error),
            }
        }
    });

    view! {
        <Modal
            dialog_id="search-add-to-collection-dialog"
            open=open
            title=tr(TextKey::AddToCollection).to_string()
            on_close=Callback::new(move |confirmed: bool| {
                if confirmed {
                    submit_action.dispatch(());
                }
            })
            confirm_label=Some(tr(TextKey::AddToCollection))
            cancel_label=Some(tr(TextKey::Cancel))
        >
            <p>{tr(TextKey::SelectSearchCollectionHelp)}</p>
            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.75rem">
                <label>{tr(TextKey::Collection)}</label>
                <select
                    prop:value=move || collection_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            collection_id.set(v);
                        }
                    }
                >
                    <option value=NEW_COLLECTION_ID.to_string()>{tr(TextKey::NewCollection)}</option>
                    {move || {
                        options.get().into_iter().map(|c| view! {
                            <option value=c.id.to_string()>{c.name}</option>
                        }).collect_view()
                    }}
                </select>
                <label>{tr(TextKey::NewCollectionName)}</label>
                <input
                    type="text"
                    prop:value=move || new_name.get()
                    on:input=move |e| new_name.set(event_target_value(&e))
                    disabled=move || collection_id.get() != NEW_COLLECTION_ID
                />
            </div>
        </Modal>
    }
}

#[component]
fn AddToPlaylistModal(
    open: RwSignal<bool>,
    query: String,
    selected: RwSignal<HashSet<(SearchCardKind, i64)>>,
    bulk_all: RwSignal<bool>,
) -> impl IntoView {
    let group_id: RwSignal<i64> = RwSignal::new(0);
    let playlist_id: RwSignal<i64> = RwSignal::new(0);
    let groups: RwSignal<Vec<PlaylistGroupOption>> = RwSignal::new(Vec::new());
    let playlists: RwSignal<Vec<PlaylistOption>> = RwSignal::new(Vec::new());
    let toast = use_toast();
    let no_items_to_add = tr(TextKey::NoItemsToAdd).to_owned();
    let please_choose_playlist = tr(TextKey::PleaseChoosePlaylist).to_owned();

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        leptos::task::spawn_local(async move {
            match list_all_playlist_groups().await {
                Ok(loaded) => groups.set(loaded),
                Err(error) => toast.push(
                    format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                    ToastSeverity::Error,
                ),
            }
        });
    });

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let gid = group_id.get();
        playlists.set(Vec::new());
        if gid == 0 {
            return;
        }
        leptos::task::spawn_local(async move {
            let loaded = match list_playlists_by_group(gid).await {
                Ok(loaded) => loaded,
                Err(error) => {
                    toast.push(
                        format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                        ToastSeverity::Error,
                    );
                    return;
                }
            };
            if group_id.get_untracked() == gid {
                playlists.set(loaded);
            }
        });
    });

    let query_for_action = query.clone();
    let submit_action = Action::new(move |(): &()| {
        let q = query_for_action.clone();
        let sel: Vec<(SearchCardKind, i64)> =
            selected.with_untracked(|s| s.iter().copied().collect());
        let all = bulk_all.get_untracked();
        let pid = playlist_id.get_untracked();
        let no_items_to_add = no_items_to_add.clone();
        let please_choose_playlist = please_choose_playlist.clone();
        async move {
            if pid == 0 {
                return Err(ServerFnError::new(please_choose_playlist));
            }
            let ids = resolve_ids(q, sel, all).await?;
            if ids.is_empty() {
                return Err(ServerFnError::new(no_items_to_add));
            }
            let count = ids.flatten().len();
            let added = add_media_items_to_playlist_mixed(pid, ids).await?;
            Ok::<(usize, u64, i64), ServerFnError>((count, added, pid))
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _, target_id)) => {
                    // Stay on the search results; the toast offers the
                    // explicit navigation path instead.
                    toast.push_with_action(
                        added_items_to_playlist(requested),
                        ToastSeverity::Success,
                        ToastAction {
                            label: tr(TextKey::ViewPlaylist).to_string(),
                            href: format!("/media/playlists/{target_id}"),
                        },
                    );
                    selected.update(HashSet::clear);
                    open.set(false);
                }
                Err(e) => {
                    toast.push(add_to_playlist_failed(e), ToastSeverity::Error);
                }
            }
        }
    });

    view! {
        <Modal
            dialog_id="search-add-to-playlist-dialog"
            open=open
            title=tr(TextKey::AddToPlaylist).to_string()
            on_close=Callback::new(move |confirmed: bool| {
                if confirmed {
                    submit_action.dispatch(());
                }
            })
            confirm_label=Some(tr(TextKey::AddToPlaylist))
            cancel_label=Some(tr(TextKey::Cancel))
        >
            <p>{tr(TextKey::SelectSearchPlaylistHelp)}</p>
            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.75rem">
                <label>{tr(TextKey::PlaylistGroup)}</label>
                <select
                    prop:value=move || group_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            group_id.set(v);
                            playlist_id.set(0);
                        }
                    }
                >
                    <option value="0">{tr(TextKey::SelectGroup)}</option>
                    {move || {
                        groups.get().into_iter().map(|g| view! {
                            <option value=g.id.to_string()>{g.name}</option>
                        }).collect_view()
                    }}
                </select>
                <label>{tr(TextKey::Playlist)}</label>
                <select
                    prop:value=move || playlist_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            playlist_id.set(v);
                        }
                    }
                    disabled=move || group_id.get() == 0
                >
                    <option value="0">{tr(TextKey::SelectPlaylist)}</option>
                    {move || {
                        playlists.get().into_iter().map(|p| view! {
                            <option value=p.id.to_string()>{p.name}</option>
                        }).collect_view()
                    }}
                </select>
            </div>
        </Modal>
    }
}
