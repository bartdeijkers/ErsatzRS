//! Shared multi-select browser shell used by `/media/movies`,
//! `/media/tv/shows`, `/media/music/artists`, and `/media/other/videos`.
//!
//! Ports the common shape of `ErsatzTV`'s `MovieList.razor`, `TelevisionShowList.razor`,
//! `ArtistList.razor`, and `OtherVideoList.razor` — all four `MultiSelectBase<T>` pages
//! share the same layout: search input, letter-bar, card grid with hover-checkboxes,
//! a "Select All" button, and a sticky selection toolbar with Add-to-Collection /
//! Add-to-Playlist actions.
//!
//! Hydration notes:
//!
//! * The page wrapper is a plain `#[component]`. Search and pagination are
//!   URL-driven (`?query=…&page=N`), mirroring `ErsatzTV`'s
//!   `NavigationManager.NavigateTo("…/page/N")` pattern.
//! * The interactive multi-select toolbar lives in [`MultiSelectIsland`].
//!   The island owns its own `Resource::new_blocking` for the card grid and
//!   renders `<Suspense>` internally, so it sits **above** the suspense
//!   boundary as the rules in `TEST.md` require. Selection state is
//!   per-page and ephemeral (matches `ErsatzTV`'s `MultiSelectBase`).
//! * The island only takes plain scalar props (`String`, `u32`, `i32` kind
//!   discriminant) — the "no prop-bearing island across Suspense" pattern.

#![allow(clippy::too_many_lines)]
use std::collections::HashSet;

use leptos::prelude::*;

use crate::admin::server_fns::media_browser::{
    add_media_items_to_collection, add_media_items_to_playlist, create_collection,
    get_media_item_info_json, list_artists, list_episodes, list_movies, list_music_videos,
    list_other_videos, list_remote_streams, list_seasons, list_shows, media_page_map,
    CollectionOption, LetterPageMap, MediaBrowserKind, MediaCardRow, PlaylistGroupOption,
    PlaylistOption,
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
mod card_style {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/ui/primitives/card.module.css");
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

pub const PAGE_SIZE: u32 = 50;

/// Encodes per-kind configuration that the page wrapper supplies. The fields
/// are all scalars so they can be passed across the SSR → island boundary
/// without violating the "no prop-bearing islands across Suspense" rule.
#[derive(Clone, Copy, Debug)]
pub struct BrowserKindConfig {
    pub kind: MediaBrowserKind,
    /// Page title shown in the toolbar (e.g. "Movies", "TV Shows").
    pub title: &'static str,
    /// Base URL for search/pager links and per-card detail navigation,
    /// without a trailing slash (e.g. "/media/movies").
    pub base_path: &'static str,
    /// Whether per-card "Show Media Info" should be shown. Maps to
    /// `MediaCardViewModel.HasMediaInfo` on the C# side.
    pub has_media_info: bool,
    /// Whether the per-card thumbnail is a wide poster ("🎬") or a circular
    /// chip ("👤") — affects the placeholder icon when artwork is missing.
    pub placeholder_icon: &'static str,
}

impl BrowserKindConfig {
    pub const MOVIES: Self = Self {
        kind: MediaBrowserKind::Movie,
        title: "Movies",
        base_path: "/media/movies",
        has_media_info: true,
        placeholder_icon: "🎬",
    };
    pub const SHOWS: Self = Self {
        kind: MediaBrowserKind::Show,
        title: "TV Shows",
        base_path: "/media/tv/shows",
        has_media_info: false,
        placeholder_icon: "📺",
    };
    pub const ARTISTS: Self = Self {
        kind: MediaBrowserKind::Artist,
        title: "Artists",
        base_path: "/media/music/artists",
        has_media_info: false,
        placeholder_icon: "🎤",
    };
    pub const OTHER_VIDEOS: Self = Self {
        kind: MediaBrowserKind::OtherVideo,
        title: "Other Videos",
        base_path: "/media/other/videos",
        has_media_info: true,
        placeholder_icon: "🎞️",
    };
    pub const SEASONS: Self = Self {
        kind: MediaBrowserKind::Season,
        title: "Seasons",
        base_path: "/media/tv/seasons",
        has_media_info: false,
        placeholder_icon: "📺",
    };
    pub const EPISODES: Self = Self {
        kind: MediaBrowserKind::Episode,
        title: "Episodes",
        base_path: "/media/tv/episodes",
        has_media_info: true,
        placeholder_icon: "📺",
    };
    pub const MUSIC_VIDEOS: Self = Self {
        kind: MediaBrowserKind::MusicVideo,
        title: "Music Videos",
        base_path: "/media/music/videos",
        has_media_info: true,
        placeholder_icon: "🎵",
    };
    pub const REMOTE_STREAMS: Self = Self {
        kind: MediaBrowserKind::RemoteStream,
        title: "Remote Streams",
        base_path: "/media/remote/streams",
        has_media_info: false,
        placeholder_icon: "📡",
    };

    fn kind_code(self) -> i32 {
        match self.kind {
            MediaBrowserKind::Movie => 0,
            MediaBrowserKind::Show => 1,
            MediaBrowserKind::Artist => 2,
            MediaBrowserKind::OtherVideo => 3,
            MediaBrowserKind::Season => 4,
            MediaBrowserKind::Episode => 5,
            MediaBrowserKind::MusicVideo => 6,
            MediaBrowserKind::RemoteStream => 7,
        }
    }

    fn from_kind_code(code: i32) -> Self {
        match code {
            1 => Self::SHOWS,
            2 => Self::ARTISTS,
            3 => Self::OTHER_VIDEOS,
            4 => Self::SEASONS,
            5 => Self::EPISODES,
            6 => Self::MUSIC_VIDEOS,
            7 => Self::REMOTE_STREAMS,
            _ => Self::MOVIES,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct MenuTarget {
    id: i64,
    title: String,
}

#[derive(Clone, Debug)]
enum MenuCommand {
    AddToCollection(MenuTarget),
    ShowMediaInfo(MenuTarget),
}

/// The page wrapper for any multi-select media browser. Reads the URL
/// query/page params and renders the toolbar + the [`MultiSelectIsland`].
#[component]
pub fn MultiSelectBrowserPage(config: BrowserKindConfig) -> impl IntoView {
    use leptos_router::hooks::use_query_map;

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
    let title = config.title;
    let base_path = config.base_path;

    view! {
        <div class=style::page>
            <form method="get" action=base_path class=style::toolbar>
                <h1 class=style::page_title>{title}</h1>
                <input
                    class=style::search
                    type="search"
                    name="query"
                    placeholder="Search…"
                    value=toolbar_query
                />
            </form>

            <MultiSelectIsland
                query=initial_query
                page=initial_page
                kind_code=config.kind_code()
            />
        </div>
    }
}

#[island]
fn MultiSelectIsland(query: String, page: u32, kind_code: i32) -> impl IntoView {
    let config = BrowserKindConfig::from_kind_code(kind_code);
    let kind = config.kind;

    let q_for_resource = query.clone();
    let data: Resource<(Vec<MediaCardRow>, u64)> = Resource::new_blocking(
        move || (q_for_resource.clone(), page),
        move |(q, p)| async move {
            match kind {
                MediaBrowserKind::Movie => list_movies(q, p, PAGE_SIZE).await,
                MediaBrowserKind::Show => list_shows(q, p, PAGE_SIZE).await,
                MediaBrowserKind::Artist => list_artists(q, p, PAGE_SIZE).await,
                MediaBrowserKind::OtherVideo => list_other_videos(q, p, PAGE_SIZE).await,
                MediaBrowserKind::Season => list_seasons(q, p, PAGE_SIZE).await,
                MediaBrowserKind::Episode => list_episodes(q, p, PAGE_SIZE).await,
                MediaBrowserKind::MusicVideo => list_music_videos(q, p, PAGE_SIZE).await,
                MediaBrowserKind::RemoteStream => list_remote_streams(q, p, PAGE_SIZE).await,
            }
            .unwrap_or_default()
        },
    );

    let q_for_map = query.clone();
    let page_map: Resource<LetterPageMap> = Resource::new_blocking(
        move || q_for_map.clone(),
        move |q| async move { media_page_map(kind, q, PAGE_SIZE).await.unwrap_or_default() },
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
                                base_uri: config.base_path.to_string(),
                                query: query_for_jump,
                                page_map: pm,
                            };
                            view! { <LetterBar present=present jump=jump/> }.into_any()
                        }
                        None => view! { <LetterBar present=present/> }.into_any(),
                    };
                    view! {
                        {letter_bar_view}
                        <GridWithSelection
                            rows=rows
                            total=total
                            page=page
                            query=query_for_links
                            config=config
                        />
                    }
                    .into_any()
                })
            }}
        </Suspense>
    }
}

#[component]
fn GridWithSelection(
    rows: Vec<MediaCardRow>,
    total: u64,
    page: u32,
    query: String,
    config: BrowserKindConfig,
) -> impl IntoView {
    let kind = config.kind;
    let selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let selected_count = Signal::derive(move || selected.with(HashSet::len));

    let page_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let page_ids_for_select_all = page_ids.clone();

    let collection_open = RwSignal::new(false);
    let playlist_open = RwSignal::new(false);
    let media_info_open = RwSignal::new(false);
    let media_info_title = RwSignal::new(String::new());
    let media_info_body = RwSignal::new(String::new());
    let item_menu_command: RwSignal<Option<MenuCommand>> = RwSignal::new(None);

    let toast = use_toast();
    let media_info_action = Action::new(move |target: &MenuTarget| {
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
                MenuCommand::AddToCollection(target) => {
                    selected.update(|items| {
                        items.clear();
                        items.insert(target.id);
                    });
                    collection_open.set(true);
                }
                MenuCommand::ShowMediaInfo(target) => {
                    media_info_title.set(target.title.clone());
                    media_info_body.set("Loading media info…".to_string());
                    media_info_open.set(true);
                    media_info_action.dispatch(target);
                }
            }
        }
    });

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
        <div class=card_style::pager>
            <div class=card_style::pager_left/>
            <PagerCenter
                base_path=config.base_path
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
                                <SelectableCard
                                    row=r
                                    selected=selected
                                    item_menu_command=item_menu_command
                                    config=config
                                />
                            }
                            .into_any()
                        }
                    />
                </div>
            }
            .into_any()
        }}

        <AddToCollectionModal
            open=collection_open
            selected=selected
        />
        <AddToPlaylistModal
            open=playlist_open
            selected=selected
            kind=kind
        />
        <MediaInfoModal
            open=media_info_open
            title=media_info_title
            body=media_info_body
        />
    }
}

#[component]
fn SelectableCard(
    row: MediaCardRow,
    selected: RwSignal<HashSet<i64>>,
    item_menu_command: RwSignal<Option<MenuCommand>>,
    config: BrowserKindConfig,
) -> impl IntoView {
    let item_id = row.id;
    let title = row.title.clone();
    let title_for_link = row.title.clone();
    let title_for_collection = row.title.clone();
    let title_for_info = row.title.clone();
    let subtitle = row.subtitle.clone();
    let artwork = row.artwork_path.clone();
    let href = format!("{}/{}", config.base_path, item_id);
    let placeholder = config.placeholder_icon;

    let is_selected = Signal::derive(move || selected.with(|s| s.contains(&item_id)));

    let wrap_class = move || {
        if is_selected.get() {
            format!("{} {}", style::card_wrap, style::selected)
        } else {
            style::card_wrap.to_string()
        }
    };

    let toggle = move || {
        selected.update(|s| {
            if !s.remove(&item_id) {
                s.insert(item_id);
            }
        });
    };

    view! {
        <div class=wrap_class>
            <label
                class=style::card_checkbox
                on:click=move |e| {
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
                                <div class=card_style::poster_placeholder>{placeholder}</div>
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
            <ItemMenu
                item_id=item_id
                title_for_collection=title_for_collection
                title_for_info=title_for_info
                item_menu_command=item_menu_command
                has_media_info=config.has_media_info
            />
        </div>
    }
}

#[component]
fn ItemMenu(
    item_id: i64,
    title_for_collection: String,
    title_for_info: String,
    item_menu_command: RwSignal<Option<MenuCommand>>,
    has_media_info: bool,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let target_for_collection = MenuTarget {
        id: item_id,
        title: title_for_collection,
    };
    let target_for_info = MenuTarget {
        id: item_id,
        title: title_for_info,
    };
    let troubleshoot_href = format!("/system/troubleshooting/playback?mediaItem={item_id}");

    view! {
        <div class=style::item_menu on:click=|e| e.stop_propagation()>
            <button
                type="button"
                class=style::item_menu_trigger
                title="Item actions"
                aria-label="Item actions"
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
                            item_menu_command.set(Some(MenuCommand::AddToCollection(target.clone())));
                        }
                    }
                >
                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                    <span>"Add To Collection"</span>
                </button>
                {has_media_info.then(|| view! {
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
                                item_menu_command.set(Some(MenuCommand::ShowMediaInfo(target.clone())));
                            }
                        }
                    >
                        <span style=icon_style(IconName::Info, "1.125rem") aria-hidden="true"></span>
                        <span>"Show Media Info"</span>
                    </button>
                })}
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

const NEW_COLLECTION_ID: i64 = -1;

#[component]
pub(crate) fn AddToCollectionModal(
    open: RwSignal<bool>,
    selected: RwSignal<HashSet<i64>>,
    #[prop(optional)] redirect_after_add: bool,
) -> impl IntoView {
    let refresh: RwSignal<u32> = RwSignal::new(0);
    let options: RwSignal<Vec<CollectionOption>> = RwSignal::new(Vec::new());

    let collection_id: RwSignal<i64> = RwSignal::new(NEW_COLLECTION_ID);
    let new_name: RwSignal<String> = RwSignal::new(String::new());

    let toast = use_toast();

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
            let added = add_media_items_to_collection(target_id, ids.clone()).await?;
            Ok::<(usize, u64, i64), ServerFnError>((ids.len(), added, target_id))
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _added, target_id)) => {
                    toast.push(
                        format!("Added {requested} items to collection"),
                        ToastSeverity::Success,
                    );
                    selected.update(HashSet::clear);
                    open.set(false);
                    if redirect_after_add {
                        set_browser_location(&format!("/media/collections/{target_id}"));
                    }
                    refresh.update(|n| *n += 1);
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
pub(crate) fn MediaInfoModal(
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
pub(crate) fn AddToPlaylistModal(
    open: RwSignal<bool>,
    selected: RwSignal<HashSet<i64>>,
    kind: MediaBrowserKind,
) -> impl IntoView {
    let group_id: RwSignal<i64> = RwSignal::new(0);
    let playlist_id: RwSignal<i64> = RwSignal::new(0);
    let groups: RwSignal<Vec<PlaylistGroupOption>> = RwSignal::new(Vec::new());
    let playlists: RwSignal<Vec<PlaylistOption>> = RwSignal::new(Vec::new());

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
            let added = add_media_items_to_playlist(kind, pid, ids.clone()).await?;
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

#[component]
fn PagerCenter(base_path: &'static str, query: String, page: u32, total: u64) -> impl IntoView {
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
        let mut url = String::from(base_path);
        let mut params: Vec<String> = Vec::new();
        let trimmed = q.trim();
        if !trimmed.is_empty() {
            params.push(format!("query={}", urlencoding::encode(trimmed)));
        }
        if p > 0 {
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
