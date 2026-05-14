//! Movie browser — port of `Pages/MovieList.razor` (the list grid and
//! multi-select toolbar) plus the per-item `Pages/Movie.razor` details page
//! (`/media/movies/:id`).
//!
//! Hydration notes:
//!
//! * The page wrapper is a plain `#[component]`. Search and pagination are
//!   URL-driven (`?query=…&page=N`), mirroring `ErsatzTV`'s
//!   `NavigationManager.NavigateTo("media/movies/page/N")` pattern.
//!   * Pressing Enter on the search box submits a GET form (no hydration).
//!   * Pager arrows are plain `<a>` anchors (no hydration).
//!   * `LetterBar` renders as a fixed right-edge column of plain `<a>` links
//!     pointing at `/media/movies?query=…&page=N#letter-X`, where `N` comes
//!     from [`movies_page_map`] — the port of `ErsatzTV`'s `SearchPageMap`
//!     algorithm in `LuceneSearchIndex.GetSearchPageMap`.
//! * The interactive multi-select toolbar (Select All button, selection
//!   header, hover-checkboxes on cards, Add-to-Collection / Add-to-Playlist
//!   modals) lives in [`MovieMultiSelectIsland`]. The island owns its own
//!   `Resource::new_blocking` for the card grid and renders `<Suspense>`
//!   internally, so it sits **above** the suspense boundary as the rules in
//!   `TEST.md` require. Selection state is per-page and ephemeral (matches
//!   `ErsatzTV`'s `MultiSelectBase`).
//! * The details page is also a plain `#[component]` that owns a blocking
//!   resource and renders inside a single `<Suspense>` boundary — no
//!   prop-bearing island.
#![allow(clippy::too_many_lines)]
use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::server_fns::media_browser::{
    add_movies_to_collection, add_movies_to_playlist, create_collection, get_media_item_info_json,
    get_movie_details, list_movies, movies_page_map, CollectionOption, LetterPageMap,
    MediaCardRow, MovieDetailRow, PlaylistGroupOption, PlaylistOption,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::media_browser::{
    list_all_collections, list_all_playlist_groups, list_playlists_by_group,
};
use crate::admin::ui::primitives::dialog::Modal;
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::letter_bar::{
    letter_bucket, FragmentLetterAnchor, LetterBar, LetterJump,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

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

#[derive(Clone, Debug, Default)]
struct MovieMenuTarget {
    id: i64,
    title: String,
}

#[derive(Clone, Debug)]
enum MovieMenuCommand {
    AddToCollection(MovieMenuTarget),
    ShowMediaInfo(MovieMenuTarget),
}

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

            <MovieMultiSelectIsland query=initial_query page=initial_page/>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Island: data load + multi-select toolbar + grid with hover-checkboxes.
//
// Note: the island receives only plain scalar props (`String`, `u32`) and
// owns the `Resource::new_blocking` internally — this is the
// "no prop-bearing island across Suspense" pattern documented in `TEST.md`
// (rule 3) and used by `ChannelEditorIsland`, `FfmpegProfileEditorIsland`,
// and `YamlEditorIsland`.
// ---------------------------------------------------------------------------

#[island]
fn MovieMultiSelectIsland(query: String, page: u32) -> impl IntoView {
    let q_for_resource = query.clone();
    let data: Resource<(Vec<MediaCardRow>, u64)> = Resource::new_blocking(
        move || (q_for_resource.clone(), page),
        |(q, p)| async move { list_movies(q, p, PAGE_SIZE).await.unwrap_or_default() },
    );

    // Letter→page map is search-scoped (page-independent), so its resource
    // only depends on the query. Ports the `SearchPageMap` lookup that
    // backs `LetterBar.razor`'s `GetLinkForLetter`.
    let q_for_map = query.clone();
    let page_map: Resource<LetterPageMap> = Resource::new_blocking(
        move || q_for_map.clone(),
        |q| async move { movies_page_map(q, PAGE_SIZE).await.unwrap_or_default() },
    );

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>"Loading…"</p> }>
            {move || {
                let query_for_links = query.clone();
                let query_for_jump = query.clone();
                data.get().map(|(rows, total)| {
                    let present: Signal<Option<HashSet<char>>> = Signal::derive({
                        let rows = rows.clone();
                        move || Some(rows.iter().map(|r| letter_bucket(&r.sort_title)).collect())
                    });
                    let letter_bar_view = match page_map.get() {
                        Some(pm) => {
                            let jump = LetterJump {
                                base_uri: "/media/movies".to_string(),
                                query: query_for_jump,
                                page_map: pm,
                            };
                            view! { <LetterBar present=present jump=jump/> }.into_any()
                        }
                        None => view! { <LetterBar present=present/> }.into_any(),
                    };
                    view! {
                        {letter_bar_view}
                        <MovieGridWithSelection
                            rows=rows
                            total=total
                            page=page
                            query=query_for_links
                        />
                    }
                    .into_any()
                })
            }}
        </Suspense>
    }
}

/// Inner body rendered once the resource resolves. Owns the selection state
/// signals and renders pager, selection header, grid, and the two modals.
///
/// All `RwSignal`s are created here so they are tied to the island's
/// reactive owner; the children are plain components (no nested islands).
#[component]
fn MovieGridWithSelection(
    rows: Vec<MediaCardRow>,
    total: u64,
    page: u32,
    query: String,
) -> impl IntoView {
    // ── Selection state ────────────────────────────────────────────────
    // HashSet<i64> of selected movie ids. ErsatzTV's `MultiSelectBase` uses
    // a `HashSet<MediaCardViewModel>` keyed on identity, but our card rows
    // are plain structs so we key on the stable `id` (movie id).
    let selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let selected_count = Signal::derive(move || selected.with(HashSet::len));

    // Page ids for "Select All".
    let page_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let page_ids_for_select_all = page_ids.clone();

    // ── Modal open signals ─────────────────────────────────────────────
    let collection_open = RwSignal::new(false);
    let playlist_open = RwSignal::new(false);
    let media_info_open = RwSignal::new(false);
    let media_info_title = RwSignal::new(String::new());
    let media_info_body = RwSignal::new(String::new());
    let item_menu_command: RwSignal<Option<MovieMenuCommand>> = RwSignal::new(None);

    let toast = use_toast();
    let media_info_action = Action::new(move |target: &MovieMenuTarget| {
        let target = target.clone();
        async move {
            get_media_item_info_json(target.id)
                .await
                .map(|json| (target.title, json))
        }
    });

    Effect::new(move |_| {
        if let Some(result) = media_info_action.value().get() {
            match result {
                Ok((title, json)) => {
                    media_info_title.set(title);
                    media_info_body.set(json);
                }
                Err(error) => {
                    media_info_body.set(format!("Unable to load media info: {error}"));
                    toast.push("Show media info failed".to_string(), ToastSeverity::Error);
                }
            }
        }
    });

    Effect::new(move |_| {
        if let Some(command) = item_menu_command.get() {
            item_menu_command.set(None);
            match command {
                MovieMenuCommand::AddToCollection(target) => {
                    selected.update(|items| {
                        items.clear();
                        items.insert(target.id);
                    });
                    collection_open.set(true);
                }
                MovieMenuCommand::ShowMediaInfo(target) => {
                    media_info_title.set(target.title.clone());
                    media_info_body.set("Loading media info…".to_string());
                    media_info_open.set(true);
                    media_info_action.dispatch(target);
                }
            }
        }
    });

    // Stable closure: are all current-page items selected?
    let all_selected = {
        let page_ids = page_ids.clone();
        Signal::derive(move || {
            !page_ids.is_empty() && selected.with(|s| page_ids.iter().all(|id| s.contains(id)))
        })
    };

    let select_all_disabled = {
        let ids = page_ids_for_select_all.clone();
        Signal::derive(move || ids.is_empty() || all_selected.get())
    };
    let select_all_click = {
        let page_ids = page_ids_for_select_all.clone();
        move |_| {
            let ids = page_ids.clone();
            selected.update(|s| {
                for id in &ids {
                    s.insert(*id);
                }
            });
        }
    };

    view! {
        // ── Pager (centered summary + chevrons) and Select All on the
        //    right — mirrors ErsatzTV's `MediaCardPager.razor` row layout.
        <div class=card_style::pager>
            <div class=card_style::pager_left/>
            <PagerCenter
                query=query.clone()
                page=page
                total=total
            />
            <div style="flex: 1; display: flex; justify-content: flex-end;">
                <button
                    class=style::select_all_btn
                    disabled=move || select_all_disabled.get()
                    on:click=select_all_click
                >
                    "☑ Select All"
                </button>
            </div>
        </div>

        // ── Selection header — always rendered; visibility via `style:display`
        //    to keep the DOM shape stable (TEST.md rule 2). ──────────────
        <div
            class=style::selection_header
            style:display=move || if selected_count.get() == 0 { "none" } else { "" }
        >
            <span class=style::selection_count>
                {move || {
                    let n = selected_count.get();
                    if n == 1 { "1 Item Selected".to_string() } else { format!("{n} Items Selected") }
                }}
            </span>
            <button
                class=style::selection_btn
                on:click=move |_| collection_open.set(true)
            >
                "+ Add To Collection"
            </button>
            <button
                class=style::selection_btn
                on:click=move |_| playlist_open.set(true)
            >
                "+ Add To Playlist"
            </button>
            <button
                class=style::selection_btn_secondary
                on:click=move |_| selected.update(HashSet::clear)
            >
                "✓ Clear Selection"
            </button>
        </div>

        // ── Grid ───────────────────────────────────────────────────────
        {if rows.is_empty() {
            view! { <p class=style::empty>"No items to display."</p> }.into_any()
        } else {
            let rows_for_grid = rows;
            view! {
                <div class=style::grid>
                    <FragmentLetterAnchor
                        cards=rows_for_grid
                        sort_title=|r: &MediaCardRow| r.sort_title.as_str()
                        render=move |r: MediaCardRow| {
                            view! {
                                <SelectableMovieCard
                                    row=r
                                    selected=selected
                                    item_menu_command=item_menu_command
                                />
                            }
                            .into_any()
                        }
                    />
                </div>
            }
            .into_any()
        }}

        // ── Modals: rendered unconditionally so SSR/hydrate DOM matches.
        //    Visibility is controlled via `style:display` inside `Modal`. ─
        <AddToCollectionModal
            open=collection_open
            selected=selected
        />
        <AddToPlaylistModal
            open=playlist_open
            selected=selected
        />
        <MediaInfoModal
            open=media_info_open
            title=media_info_title
            body=media_info_body
        />
    }
}

/// One movie card with selection overlay. The whole card is wrapped in a
/// stable `<div>` so the selection border and hover-checkbox have a stable
/// element to attach to. The poster + caption are still wrapped in a link
/// to `/media/movies/{id}` (matches the C# `<a href>` overlay).
#[component]
fn SelectableMovieCard(
    row: MediaCardRow,
    selected: RwSignal<HashSet<i64>>,
    item_menu_command: RwSignal<Option<MovieMenuCommand>>,
) -> impl IntoView {
    let movie_id = row.id;
    let title = row.title.clone();
    let title_for_link = row.title.clone();
    let title_for_collection = row.title.clone();
    let title_for_info = row.title.clone();
    let subtitle = row.subtitle.clone();
    let artwork = row.artwork_path.clone();
    let href = format!("/media/movies/{movie_id}");

    let is_selected = Signal::derive(move || selected.with(|s| s.contains(&movie_id)));

    let wrap_class = move || {
        if is_selected.get() {
            format!("{} {}", style::card_wrap, style::selected)
        } else {
            style::card_wrap.to_string()
        }
    };

    let toggle = move || {
        selected.update(|s| {
            if !s.remove(&movie_id) {
                s.insert(movie_id);
            }
        });
    };

    view! {
        <div class=wrap_class>
            // Checkbox overlay sits ABOVE the link so clicks on it do not
            // navigate. `pointer-events` aren't needed because the label
            // stops propagation at the change event level.
            <label
                class=style::card_checkbox
                on:click=move |e| {
                    // Prevent the wrapping <a> from navigating when the
                    // user clicks the checkbox area.
                    e.stop_propagation();
                    e.prevent_default();
                    toggle();
                }
                aria-label=move || format!(
                    "Select {}",
                    if is_selected.get() { "off" } else { "on" }
                )
            >
                <input
                    type="checkbox"
                    prop:checked=move || is_selected.get()
                    on:click=move |e| e.stop_propagation()
                    on:change=move |_| toggle()
                />
            </label>

            <a href=href>
                <div class=card_style::card>
                    <div class=card_style::poster>
                        {match artwork {
                            Some(url) => view! {
                                <img src=url loading="lazy" class=card_style::poster_img alt=title_for_link/>
                            }.into_any(),
                            None => view! {
                                <div class=card_style::poster_placeholder>"🎬"</div>
                            }.into_any(),
                        }}
                    </div>
                    <div class=card_style::caption>
                        <p class=card_style::caption_title>{title}</p>
                        {subtitle.map(|s| view! {
                            <p class=card_style::caption_sub>{s}</p>
                        })}
                    </div>
                </div>
            </a>
            <MovieItemMenu
                movie_id=movie_id
                title_for_collection=title_for_collection
                title_for_info=title_for_info
                item_menu_command=item_menu_command
            />
        </div>
    }
}

#[component]
fn MovieItemMenu(
    movie_id: i64,
    title_for_collection: String,
    title_for_info: String,
    item_menu_command: RwSignal<Option<MovieMenuCommand>>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let target_for_collection = MovieMenuTarget {
        id: movie_id,
        title: title_for_collection,
    };
    let target_for_info = MovieMenuTarget {
        id: movie_id,
        title: title_for_info,
    };
    let troubleshoot_href = format!("/system/troubleshooting/playback?mediaItem={movie_id}");

    view! {
        <div class=style::item_menu on:click=|e| e.stop_propagation()>
            <button
                type="button"
                class=style::item_menu_trigger
                title="Movie actions"
                aria-label="Movie actions"
                aria-expanded=move || open.get().to_string()
                on:click=move |e| {
                    e.prevent_default();
                    e.stop_propagation();
                    open.update(|value| *value = !*value);
                }
            >
                <span style=icon_style(IconName::MoreVert, "1.25rem") aria-hidden="true"></span>
            </button>
            <div
                class=style::item_menu_popover
                role="menu"
                style:display=move || if open.get() { "" } else { "none" }
            >
                <button
                    type="button"
                    class=style::item_menu_entry
                    role="menuitem"
                    on:click={
                        let target = target_for_collection.clone();
                        move |e| {
                            e.prevent_default();
                            e.stop_propagation();
                            open.set(false);
                            item_menu_command.set(Some(MovieMenuCommand::AddToCollection(target.clone())));
                        }
                    }
                >
                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                    <span>"Add To Collection"</span>
                </button>
                <button
                    type="button"
                    class=style::item_menu_entry
                    role="menuitem"
                    on:click={
                        let target = target_for_info.clone();
                        move |e| {
                            e.prevent_default();
                            e.stop_propagation();
                            open.set(false);
                            item_menu_command.set(Some(MovieMenuCommand::ShowMediaInfo(target.clone())));
                        }
                    }
                >
                    <span style=icon_style(IconName::Info, "1.125rem") aria-hidden="true"></span>
                    <span>"Show Media Info"</span>
                </button>
                <a
                    class=style::item_menu_entry
                    role="menuitem"
                    href=troubleshoot_href
                    on:click=move |e| {
                        e.stop_propagation();
                        open.set(false);
                    }
                >
                    <span style=icon_style(IconName::FindReplace, "1.125rem") aria-hidden="true"></span>
                    <span>"Troubleshoot Playback"</span>
                </a>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Add To Collection modal — ports `Shared/AddToCollectionDialog.razor`.
// ---------------------------------------------------------------------------

/// Sentinel id for the "(New Collection)" pseudo-option. `ErsatzTV` uses a
/// distinct `MediaCollectionViewModel` reference; we use `-1` since
/// `Collection.Id` is `AUTOINCREMENT` and always positive.
const NEW_COLLECTION_ID: i64 = -1;

#[component]
fn AddToCollectionModal(open: RwSignal<bool>, selected: RwSignal<HashSet<i64>>) -> impl IntoView {
    let refresh: RwSignal<u32> = RwSignal::new(0);
    let options: RwSignal<Vec<CollectionOption>> = RwSignal::new(Vec::new());

    // Selected collection id. `-1` means "(New Collection)".
    let collection_id: RwSignal<i64> = RwSignal::new(NEW_COLLECTION_ID);
    let new_name: RwSignal<String> = RwSignal::new(String::new());

    let toast = use_toast();

    // Load option rows only after hydration. Rendering server-resolved
    // `<option>` nodes inside an island `<select>` can abort hydration before
    // the modal buttons receive handlers.
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let _ = refresh.get();
        leptos::task::spawn_local(async move {
            let loaded = list_all_collections().await.unwrap_or_default();
            let first_id = loaded.first().map(|c| c.id);
            options.set(loaded);
            if let Some(id) = first_id {
                if collection_id.get_untracked() == NEW_COLLECTION_ID
                    && new_name.with_untracked(String::is_empty)
                {
                    collection_id.set(id);
                }
            }
        });
    });

    // Submit action — runs the create-then-add or add-only branches.
    let submit_action = Action::new(move |(): &()| {
        let ids: Vec<i64> = selected.with_untracked(|s| s.iter().copied().collect());
        let chosen_id = collection_id.get_untracked();
        let new_collection_name = new_name.get_untracked();
        async move {
            let target_id = if chosen_id == NEW_COLLECTION_ID {
                let name = new_collection_name.trim();
                if name.is_empty() {
                    return Err(ServerFnError::new("Collection name is required"));
                }
                create_collection(name.to_string()).await?.id
            } else {
                chosen_id
            };
            let added = add_movies_to_collection(target_id, ids.clone()).await?;
            Ok::<(usize, u64, i64), ServerFnError>((ids.len(), added, target_id))
        }
    });

    // Effect: on success, toast, close, clear selection, and refresh dropdown.
    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _added, _id)) => {
                    toast.push(
                        format!("Added {requested} items to collection"),
                        ToastSeverity::Success,
                    );
                    selected.update(HashSet::clear);
                    open.set(false);
                    refresh.update(|n| *n += 1);
                    // Reset modal state.
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

    let count_label = Signal::derive(move || selected.with(HashSet::len));

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
            <p>
                {move || format!(
                    "Select the collection to add the {} selected items",
                    count_label.get()
                )}
            </p>

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
fn MediaInfoModal(
    open: RwSignal<bool>,
    title: RwSignal<String>,
    body: RwSignal<String>,
) -> impl IntoView {
    view! {
        <div
            class=style::modal_backdrop
            style:display=move || if open.get() { "" } else { "none" }
        />
        <div
            class=style::modal_overlay
            style:display=move || if open.get() { "" } else { "none" }
            on:click=move |_| open.set(false)
        >
            <div class=style::media_info_panel on:click=|e| e.stop_propagation()>
                <h2 class=style::media_info_title>{move || title.get()}</h2>
                <div class=style::media_info_dialog>
                    <pre class=style::media_info_pre><code>{move || body.get()}</code></pre>
                </div>
                <div class=style::media_info_footer>
                    <button type="button" class=style::selection_btn on:click=move |_| open.set(false)>
                        "Close"
                    </button>
                </div>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Add To Playlist modal — ports `Shared/AddToPlaylistDialog.razor`.
// ---------------------------------------------------------------------------

#[component]
fn AddToPlaylistModal(open: RwSignal<bool>, selected: RwSignal<HashSet<i64>>) -> impl IntoView {
    let group_id: RwSignal<i64> = RwSignal::new(0);
    let playlist_id: RwSignal<i64> = RwSignal::new(0);
    let groups: RwSignal<Vec<PlaylistGroupOption>> = RwSignal::new(Vec::new());
    let playlists: RwSignal<Vec<PlaylistOption>> = RwSignal::new(Vec::new());

    // Load select options only after hydration, for the same reason as the
    // collection modal above.
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
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
            if gid == 0 {
                return;
            }
            if group_id.get_untracked() == gid {
                playlists.set(loaded);
            }
        });
    });

    let toast = use_toast();

    let submit_action = Action::new(move |(): &()| {
        let ids: Vec<i64> = selected.with_untracked(|s| s.iter().copied().collect());
        let pid = playlist_id.get_untracked();
        async move {
            if pid == 0 {
                return Err(ServerFnError::new("Please choose a playlist"));
            }
            let added = add_movies_to_playlist(pid, ids.clone()).await?;
            Ok::<(usize, u64), ServerFnError>((ids.len(), added))
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _added)) => {
                    toast.push(
                        format!("Added {requested} items to playlist"),
                        ToastSeverity::Success,
                    );
                    selected.update(HashSet::clear);
                    open.set(false);
                }
                Err(e) => toast.push(format!("Add to playlist failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    let count_label = Signal::derive(move || selected.with(HashSet::len));

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
            <p>
                {move || format!(
                    "Select the playlist to add the {} selected items",
                    count_label.get()
                )}
            </p>

            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.75rem">
                <label>"Playlist Group"</label>
                <select
                    prop:value=move || group_id.get().to_string()
                    on:change=move |e| {
                        if let Ok(v) = event_target_value(&e).parse::<i64>() {
                            group_id.set(v);
                            // Reset playlist choice on group change.
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

// ---------------------------------------------------------------------------
// URL-driven pager. Identical to the previous implementation; lifted up so
// it can be reused inside the island. Renders summary + prev/next anchors
// targeting `/media/movies?query=…&page=N`. Disabled state collapses the
// anchor into a styled `<span>` so the DOM shape stays stable.
// ---------------------------------------------------------------------------

#[component]
fn PagerCenter(query: String, page: u32, total: u64) -> impl IntoView {
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
        <div class=card_style::pager_right>
            {if prev_disabled {
                view! {
                    <span class=format!("{} {}", card_style::pager_btn, card_style::pager_btn_disabled)>"‹"</span>
                }
                .into_any()
            } else {
                view! { <a class=card_style::pager_btn href=prev_href>"‹"</a> }.into_any()
            }}
            <span>{summary}</span>
            {if next_disabled {
                view! {
                    <span class=format!("{} {}", card_style::pager_btn, card_style::pager_btn_disabled)>"›"</span>
                }
                .into_any()
            } else {
                view! { <a class=card_style::pager_btn href=next_href>"›"</a> }.into_any()
            }}
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
