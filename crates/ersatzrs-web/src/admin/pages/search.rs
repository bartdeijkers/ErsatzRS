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
//! * The island owns its own `Resource::new_blocking` and renders
//!   `<Suspense>` internally, so it sits **above** the suspense boundary as
//!   the rules require. Selection state is per-page and ephemeral, mirroring
//!   `MultiSelectBase`.
//! * Shift-range selection rebuilds the cross-section sorted card list and
//!   applies `MultiSelectBase.SelectClicked`'s range logic.

#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

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
    search_results, SearchAllIds, SearchCard, SearchCardKind, SearchResults, SearchSection,
};
use crate::admin::ui::dialogs::save_as_smart_collection::SaveAsSmartCollectionDialog;
use crate::admin::ui::primitives::card::MediaCard;
use crate::admin::ui::primitives::dialog::Modal;
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/search.module.css");
mod shell {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/pages/media/collections.module.css");
    pub use s::*;
}

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

/// One section's static metadata: anchor id, heading, See-All list route.
#[derive(Clone, Copy)]
struct SectionMeta {
    kind: SearchCardKind,
    anchor: &'static str,
    label: &'static str,
    list_path: &'static str,
}

const SECTIONS: [SectionMeta; 10] = [
    SectionMeta {
        kind: SearchCardKind::Movie,
        anchor: "movies",
        label: "Movies",
        list_path: "/media/movies",
    },
    SectionMeta {
        kind: SearchCardKind::Show,
        anchor: "shows",
        label: "Shows",
        list_path: "/media/tv/shows",
    },
    SectionMeta {
        kind: SearchCardKind::Season,
        anchor: "seasons",
        label: "Seasons",
        list_path: "/media/tv/seasons",
    },
    SectionMeta {
        kind: SearchCardKind::Episode,
        anchor: "episodes",
        label: "Episodes",
        list_path: "/media/tv/episodes",
    },
    SectionMeta {
        kind: SearchCardKind::Artist,
        anchor: "artists",
        label: "Artists",
        list_path: "/media/music/artists",
    },
    SectionMeta {
        kind: SearchCardKind::MusicVideo,
        anchor: "music_videos",
        label: "Music Videos",
        list_path: "/media/music/videos",
    },
    SectionMeta {
        kind: SearchCardKind::OtherVideo,
        anchor: "other_videos",
        label: "Other Videos",
        list_path: "/media/other/videos",
    },
    SectionMeta {
        kind: SearchCardKind::Song,
        anchor: "songs",
        label: "Songs",
        list_path: "/media/music/songs",
    },
    SectionMeta {
        kind: SearchCardKind::Image,
        anchor: "images",
        label: "Images",
        list_path: "/media/images",
    },
    SectionMeta {
        kind: SearchCardKind::RemoteStream,
        anchor: "remote_streams",
        label: "Remote Streams",
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

#[component]
pub fn SearchPage() -> impl IntoView {
    view! { <SearchIsland/> }
}

#[island]
fn SearchIsland() -> impl IntoView {
    // Read `?query=` reactively. The `/search` route is matched by a single
    // `StaticSegment`, so a top-bar submit that only changes the query string
    // does **not** remount this component — Leptos client routing keeps the
    // same route match. Driving the resource off the reactive query map (not a
    // `with_untracked` snapshot) is what makes a same-route navigation
    // actually re-run the search. Mirrors `Search.razor`, which re-reads
    // `?query=` on every navigation.
    let query_map = use_query_map();
    let current_query = move || query_map.with(|params| params.get("query").unwrap_or_default());

    let input = RwSignal::new(current_query());
    // Keep the in-page search box in sync when the URL query changes (e.g. a
    // top-bar submit while already on `/search`).
    Effect::new(move |_| input.set(current_query()));

    let data: Resource<(String, SearchResults)> =
        Resource::new_blocking(current_query, move |query| async move {
            let results = search_results(query.clone()).await.unwrap_or_default();
            (query, results)
        });

    let do_search = move || {
        let query = input.get_untracked();
        let encoded = urlencoding::encode(query.trim()).into_owned();
        set_browser_location(&format!("/search?query={encoded}"));
    };

    view! {
        <div class=shell::page>
            <form
                class=shell::toolbar
                on:submit=move |e| {
                    e.prevent_default();
                    do_search();
                }
            >
                <h1 class=shell::page_title>"Search"</h1>
                <input
                    class=shell::search
                    type="search"
                    placeholder="Search…"
                    prop:value=move || input.get()
                    on:input=move |event| input.set(event_target_value(&event))
                />
                <button
                    type="submit"
                    class=shell::btn_add
                    disabled=move || input.with(|value| value.trim().is_empty())
                >
                    "Search"
                </button>
            </form>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
                {move || {
                    data.get().map(|(query, results)| {
                        if query.trim().is_empty() {
                            return view! {
                                <p class=style::empty>"Enter a search query."</p>
                            }
                            .into_any();
                        }
                        let any = SECTIONS
                            .iter()
                            .any(|m| !section_for(&results, m.kind).cards.is_empty());
                        if !any {
                            return view! {
                                <p class=style::empty>"No media matched."</p>
                            }
                            .into_any();
                        }
                        view! { <SearchResultsView query=query results=results/> }.into_any()
                    })
                }}
            </Suspense>
        </div>
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
    let save_as_open = RwSignal::new(false);
    // When true, the bulk dialogs operate on the full result set ("Add All");
    // otherwise on the current selection.
    let bulk_all = RwSignal::new(false);

    let query_for_dialogs = query.clone();
    let query_signal = Signal::derive({
        let q = query.clone();
        move || q.clone()
    });

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
            on_save_as=Callback::new(move |()| save_as_open.set(true))
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
                                    {meta.label}
                                </h2>
                                {(total > 50).then(|| {
                                    let href = see_all_href(meta.list_path, &q_for_link);
                                    view! {
                                        <a class=style::see_all href=href>"See All >>"</a>
                                    }
                                })}
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
        <SaveAsSmartCollectionDialog
            open=save_as_open
            query=query_signal
            on_saved=Some(Callback::new(move |_id: i64| clear_selection.run(())))
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
    on_save_as: Callback<()>,
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

    view! {
        <div class=shell::toolbar>
            <Show
                when=move || select_mode.get()
                fallback=move || {
                    let counts = counts.clone();
                    let query = query.clone();
                    view! {
                        <div class=style::count_links>
                            <span class=style::query_echo>{query}</span>
                            {counts
                                .into_iter()
                                .map(|(meta, total)| view! {
                                    <a
                                        class=style::count_link
                                        href=format!("#{}", meta.anchor)
                                    >
                                        {format!("{total} {}", meta.label)}
                                    </a>
                                })
                                .collect_view()}
                        </div>
                        <div class=style::actions>
                            <button
                                class=style::action_btn
                                title="Add All To Collection"
                                on:click=move |_| on_add_all_collection.run(())
                            >
                                "Add All"
                            </button>
                            <button
                                class=style::action_btn
                                title="Add All To Playlist"
                                on:click=move |_| on_add_all_playlist.run(())
                            >
                                "Add All"
                            </button>
                            <button
                                class=style::action_btn_secondary
                                title="Save As Smart Collection"
                                on:click=move |_| on_save_as.run(())
                            >
                                "Save As"
                            </button>
                        </div>
                    }
                }
            >
                <div class=style::count_links>
                    <span class=style::query_echo>
                        {move || {
                            let n = select_count.get();
                            if n == 1 {
                                "1 Item Selected".to_string()
                            } else {
                                format!("{n} Items Selected")
                            }
                        }}
                    </span>
                </div>
                <div class=style::actions>
                    <button
                        class=style::action_btn
                        on:click=move |_| on_sel_collection.run(())
                    >
                        "Add To Collection"
                    </button>
                    <button
                        class=style::action_btn
                        on:click=move |_| on_sel_playlist.run(())
                    >
                        "Add To Playlist"
                    </button>
                    <button
                        class=style::action_btn_secondary
                        on:click=move |_| on_clear.run(())
                    >
                        "Clear Selection"
                    </button>
                </div>
            </Show>
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

    view! {
        <div class=wrap_class>
            <MediaCard
                title=card.title
                subtitle=card.subtitle
                href=href
                artwork_url=card.artwork_url
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
            />
        </div>
    }
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

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        leptos::task::spawn_local(async move {
            options.set(list_all_collections().await.unwrap_or_default());
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
        async move {
            let ids = resolve_ids(q, sel, all).await?;
            let flat = ids.flatten();
            if flat.is_empty() {
                return Err(ServerFnError::new("No items to add."));
            }
            let target = if chosen == NEW_COLLECTION_ID {
                let name = name.trim();
                if name.is_empty() {
                    return Err(ServerFnError::new("Collection name is required"));
                }
                crate::admin::server_fns::media_browser::create_collection(name.to_string())
                    .await?
                    .id
            } else {
                chosen
            };
            let added = add_media_items_to_collection(target, flat.clone()).await?;
            Ok::<(usize, u64), ServerFnError>((flat.len(), added))
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _)) => {
                    toast.push(
                        format!("Added {requested} items to collection"),
                        ToastSeverity::Success,
                    );
                    selected.update(HashSet::clear);
                    open.set(false);
                    collection_id.set(NEW_COLLECTION_ID);
                    new_name.set(String::new());
                }
                Err(e) => toast.push(
                    format!("Add to collection failed: {e}"),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    view! {
        <Modal
            open=open
            title="Add To Collection".to_string()
            on_close=Callback::new(move |confirmed: bool| {
                if confirmed {
                    submit_action.dispatch(());
                }
            })
            confirm_label="Add To Collection"
            cancel_label="Cancel"
        >
            <p>"Select the collection to add the search results to."</p>
            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.75rem">
                <label>"Collection"</label>
                <select
                    prop:value=move || collection_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            collection_id.set(v);
                        }
                    }
                >
                    <option value=NEW_COLLECTION_ID.to_string()>"(New Collection)"</option>
                    {move || {
                        options.get().into_iter().map(|c| view! {
                            <option value=c.id.to_string()>{c.name}</option>
                        }).collect_view()
                    }}
                </select>
                <label>"New Collection Name"</label>
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

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        leptos::task::spawn_local(async move {
            groups.set(list_all_playlist_groups().await.unwrap_or_default());
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
            let loaded = list_playlists_by_group(gid).await.unwrap_or_default();
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
        async move {
            if pid == 0 {
                return Err(ServerFnError::new("Please choose a playlist"));
            }
            let ids = resolve_ids(q, sel, all).await?;
            if ids.is_empty() {
                return Err(ServerFnError::new("No items to add."));
            }
            let count = ids.flatten().len();
            let added = add_media_items_to_playlist_mixed(pid, ids).await?;
            Ok::<(usize, u64), ServerFnError>((count, added))
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _)) => {
                    toast.push(
                        format!("Added {requested} items to playlist"),
                        ToastSeverity::Success,
                    );
                    selected.update(HashSet::clear);
                    open.set(false);
                }
                Err(e) => {
                    toast.push(format!("Add to playlist failed: {e}"), ToastSeverity::Error);
                }
            }
        }
    });

    view! {
        <Modal
            open=open
            title="Add To Playlist".to_string()
            on_close=Callback::new(move |confirmed: bool| {
                if confirmed {
                    submit_action.dispatch(());
                }
            })
            confirm_label="Add To Playlist"
            cancel_label="Cancel"
        >
            <p>"Select the playlist to add the search results to."</p>
            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.75rem">
                <label>"Playlist Group"</label>
                <select
                    prop:value=move || group_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            group_id.set(v);
                            playlist_id.set(0);
                        }
                    }
                >
                    <option value="0">"(Select a group)"</option>
                    {move || {
                        groups.get().into_iter().map(|g| view! {
                            <option value=g.id.to_string()>{g.name}</option>
                        }).collect_view()
                    }}
                </select>
                <label>"Playlist"</label>
                <select
                    prop:value=move || playlist_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            playlist_id.set(v);
                        }
                    }
                    disabled=move || group_id.get() == 0
                >
                    <option value="0">"(Select a playlist)"</option>
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
