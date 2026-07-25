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
//! * The page wrapper loads the first card grid and letter map during SSR,
//!   then hands that concrete payload to [`MultiSelectIsland`]. The island
//!   keeps the same interactive selection state and refreshes from client
//!   resources after hydration.

#![allow(clippy::too_many_lines)]
use std::collections::HashSet;

use leptos::prelude::*;

use crate::admin::i18n::{
    add_to_collection_failed, add_to_playlist_failed, added_items_to_collection,
    added_items_to_playlist, content_variant_badge_label, episode_display_subtitle, item_count,
    range_of, season_display_name, select_collection_to_add_selected_items,
    select_playlist_to_add_selected_items, selected_items_count, tr, TextKey,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::layout::media_state_revision;
use crate::admin::server_fns::media_browser::{
    add_media_items_to_collection_with_playback, add_media_items_to_playlist_with_playback,
    create_collection, get_media_item_info_json, list_artists, list_episodes, list_images,
    list_movies, list_music_videos, list_other_videos, list_remote_streams, list_seasons,
    list_shows, list_songs, media_page_map, CollectionOption, LetterPageMap, MediaBrowserKind,
    MediaCardRow, PlaybackOverrideForm, PlaylistGroupOption, PlaylistOption,
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
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast, ToastAction, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/media/browser.module.css");
mod card_style {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/ui/primitives/card.module.css");
    pub use s::*;
}

pub const PAGE_SIZE: u32 = 50;

/// Encodes per-kind configuration that the page wrapper supplies. The fields
/// are all scalars so they can be passed across the SSR → island boundary
/// without violating the "no prop-bearing islands across Suspense" rule.
#[derive(Clone, Copy, Debug)]
pub struct BrowserKindConfig {
    pub kind: MediaBrowserKind,
    /// Page title shown in the toolbar.
    pub title: TextKey,
    /// Base URL for search/pager links and per-card detail navigation,
    /// without a trailing slash (e.g. "/media/movies").
    pub base_path: &'static str,
    /// Base URL for per-card detail links. Empty when the upstream page has no
    /// card detail route, e.g. song and image lists use `Href=""`.
    pub card_base_path: &'static str,
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
        title: TextKey::Movies,
        base_path: "/media/movies",
        card_base_path: "/media/movies",
        has_media_info: true,
        placeholder_icon: "🎬",
    };
    pub const SHOWS: Self = Self {
        kind: MediaBrowserKind::Show,
        title: TextKey::TvShows,
        base_path: "/media/tv/shows",
        card_base_path: "/media/tv/shows",
        has_media_info: false,
        placeholder_icon: "📺",
    };
    pub const ARTISTS: Self = Self {
        kind: MediaBrowserKind::Artist,
        title: TextKey::Artists,
        base_path: "/media/music/artists",
        card_base_path: "/media/music/artists",
        has_media_info: false,
        placeholder_icon: "🎤",
    };
    pub const OTHER_VIDEOS: Self = Self {
        kind: MediaBrowserKind::OtherVideo,
        title: TextKey::OtherVideos,
        base_path: "/media/other/videos",
        card_base_path: "/media/other/videos",
        has_media_info: true,
        placeholder_icon: "🎞️",
    };
    pub const SEASONS: Self = Self {
        kind: MediaBrowserKind::Season,
        title: TextKey::Seasons,
        base_path: "/media/tv/seasons",
        card_base_path: "/media/tv/seasons",
        has_media_info: false,
        placeholder_icon: "📺",
    };
    pub const EPISODES: Self = Self {
        kind: MediaBrowserKind::Episode,
        title: TextKey::Episodes,
        base_path: "/media/tv/episodes",
        card_base_path: "",
        has_media_info: true,
        placeholder_icon: "📺",
    };
    pub const MUSIC_VIDEOS: Self = Self {
        kind: MediaBrowserKind::MusicVideo,
        title: TextKey::MusicVideos,
        base_path: "/media/music/videos",
        card_base_path: "",
        has_media_info: true,
        placeholder_icon: "🎵",
    };
    pub const REMOTE_STREAMS: Self = Self {
        kind: MediaBrowserKind::RemoteStream,
        title: TextKey::RemoteStreams,
        base_path: "/media/remote/streams",
        card_base_path: "/media/remote/streams",
        has_media_info: false,
        placeholder_icon: "📡",
    };
    pub const SONGS: Self = Self {
        kind: MediaBrowserKind::Song,
        title: TextKey::Songs,
        base_path: "/media/music/songs",
        card_base_path: "",
        has_media_info: true,
        placeholder_icon: "♪",
    };
    pub const IMAGES: Self = Self {
        kind: MediaBrowserKind::Image,
        title: TextKey::Images,
        base_path: "/media/browser/images",
        card_base_path: "",
        has_media_info: true,
        placeholder_icon: "▣",
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
            MediaBrowserKind::Song => 8,
            MediaBrowserKind::Image => 9,
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
            8 => Self::SONGS,
            9 => Self::IMAGES,
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
    let initial_query_for_resource = initial_query.clone();
    let initial = Resource::new_blocking(
        move || (initial_query_for_resource.clone(), initial_page),
        move |(q, p)| async move {
            tokio::try_join!(
                load_browser_rows(config.kind, q.clone(), p),
                load_browser_page_map(config.kind, q),
            )
        },
    );

    view! {
        <div class=style::page>
            <form method="get" action=base_path class=style::toolbar>
                <h1 class=style::page_title>{tr(title)}</h1>
                <input
                    class=style::search
                    type="search"
                    name="query"
                    placeholder=tr(TextKey::Search)
                    value=toolbar_query
                />
            </form>

            <Suspense fallback=admin_loading_resource_view>
                {move || {
                    let query = initial_query.clone();
                    initial.get().map(move |result| admin_result_view(result, |((initial_rows, initial_total), initial_page_map)| {
                        view! {
                            <MultiSelectIsland
                                query=query.clone()
                                page=initial_page
                                kind_code=config.kind_code()
                                initial_rows=initial_rows
                                initial_total=initial_total
                                initial_page_map=initial_page_map
                            />
                        }
                        .into_any()
                    }))
                }}
            </Suspense>
        </div>
    }
}

#[island]
fn MultiSelectIsland(
    query: String,
    page: u32,
    kind_code: i32,
    initial_rows: Vec<MediaCardRow>,
    initial_total: u64,
    initial_page_map: LetterPageMap,
) -> impl IntoView {
    let config = BrowserKindConfig::from_kind_code(kind_code);
    let kind = config.kind;
    let media_revision = RwSignal::new(String::new());
    let current_data = RwSignal::new((initial_rows, initial_total));
    let current_page_map = RwSignal::new(initial_page_map);
    let load_toast = use_toast();

    let q_for_resource = query.clone();
    let data: LocalResource<(Vec<MediaCardRow>, u64)> = LocalResource::new(move || {
        let q = q_for_resource.clone();
        let _ = media_revision.get();
        async move { load_or_notify(load_browser_rows(kind, q, page).await, load_toast) }
    });
    Effect::new(move |_| {
        if let Some(rows) = data.get() {
            current_data.maybe_update(|current| {
                if *current == rows {
                    false
                } else {
                    *current = rows;
                    true
                }
            });
        }
    });

    let q_for_map = query.clone();
    let page_map: LocalResource<LetterPageMap> = LocalResource::new(move || {
        let q = q_for_map.clone();
        let _ = media_revision.get();
        async move { load_or_notify(load_browser_page_map(kind, q).await, load_toast) }
    });
    #[cfg(target_arch = "wasm32")]
    let revision_load_failed = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(next_page_map) = page_map.get() {
            current_page_map.maybe_update(|current| {
                if *current == next_page_map {
                    false
                } else {
                    *current = next_page_map;
                    true
                }
            });
        }
    });

    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                loop {
                    match media_state_revision().await {
                        Ok(next) => {
                            if media_revision.get_untracked() != next {
                                media_revision.set(next);
                            }
                            revision_load_failed.set(false);
                        }
                        Err(error) if !revision_load_failed.get_untracked() => {
                            revision_load_failed.set(true);
                            load_toast.push(
                                format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                                ToastSeverity::Error,
                            );
                        }
                        Err(_) => {}
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        }
    });

    view! {
        {move || {
            let query_for_links = query.clone();
            let (rows, total) = current_data.get();
            let present: Signal<Option<HashSet<char>>> = Signal::derive({
                let rows = rows.clone();
                move || Some(rows.iter().map(|r| letter_bucket(&r.sort_title)).collect())
            });
            let jump = LetterJump {
                base_uri: config.base_path.to_string(),
                query: query.clone(),
                page_map: current_page_map.get(),
            };

            view! {
                <LetterBar present=present jump=jump/>
                <GridWithSelection
                    rows=rows
                    total=total
                    page=page
                    query=query_for_links
                    config=config
                />
            }
        }}
    }
}

async fn load_browser_rows(
    kind: MediaBrowserKind,
    query: String,
    page: u32,
) -> Result<(Vec<MediaCardRow>, u64), ServerFnError> {
    match kind {
        MediaBrowserKind::Movie => list_movies(query, page, PAGE_SIZE, true).await,
        MediaBrowserKind::Show => list_shows(query, page, PAGE_SIZE, true).await,
        MediaBrowserKind::Artist => list_artists(query, page, PAGE_SIZE, true).await,
        MediaBrowserKind::OtherVideo => list_other_videos(query, page, PAGE_SIZE, true).await,
        MediaBrowserKind::Season => list_seasons(query, page, PAGE_SIZE, true).await,
        MediaBrowserKind::Episode => list_episodes(query, page, PAGE_SIZE, true).await,
        MediaBrowserKind::MusicVideo => list_music_videos(query, page, PAGE_SIZE, true).await,
        MediaBrowserKind::RemoteStream => list_remote_streams(query, page, PAGE_SIZE).await,
        MediaBrowserKind::Song => list_songs(query, page, PAGE_SIZE, true).await,
        MediaBrowserKind::Image => list_images(query, page, PAGE_SIZE, true).await,
    }
}

async fn load_browser_page_map(
    kind: MediaBrowserKind,
    query: String,
) -> Result<LetterPageMap, ServerFnError> {
    media_page_map(kind, query, PAGE_SIZE, true).await
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
                    media_info_body.set(format!("{}: {error}", tr(TextKey::UnableToLoadMediaInfo)));
                    toast.push(
                        tr(TextKey::UnableToLoadMediaInfo).to_string(),
                        ToastSeverity::Error,
                    );
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
                    media_info_body.set(tr(TextKey::LoadingMediaInfo).to_string());
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
                    {format!("☑ {}", tr(TextKey::SelectAll))}
                </button>
            </div>
        </div>

        <SelectionToolbar
            selected=selected
            collection_open=collection_open
            playlist_open=playlist_open
        />

        {
            let rows_empty = rows.is_empty();
            view! {
                <p
                    class=style::empty
                    style:display=move || if rows_empty { "" } else { "none" }
                >
                    {tr(TextKey::NoItemsToDisplay)}
                </p>
                <div
                    class=style::grid
                    style:display=move || if rows_empty { "none" } else { "" }
                >
                    <FragmentLetterAnchor
                        cards=rows
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
        }

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
    let title = row
        .season_number
        .map_or_else(|| row.title.clone(), season_display_name);
    let title_for_link = title.clone();
    let title_for_collection = title.clone();
    let title_for_info = title.clone();
    let variant_badge =
        content_variant_badge_label(row.variant.content_kind, row.variant.sub_kind.as_deref());
    let subtitle = if config.kind == MediaBrowserKind::Episode {
        row.subtitle
            .as_deref()
            .and_then(localize_episode_subtitle)
            .or_else(|| row.subtitle.clone())
    } else {
        row.subtitle.clone()
    };
    let subtitle_text = subtitle.unwrap_or_default();
    let has_subtitle = !subtitle_text.is_empty();
    let artwork = row.artwork_path.clone();
    let href = row.href.clone().or_else(|| {
        (!config.card_base_path.is_empty())
            .then(|| format!("{}/{}", config.card_base_path, item_id))
    });
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
                aria-label=tr(TextKey::Select)
            >
                <input
                    type="checkbox"
                    prop:checked=move || is_selected.get()
                    on:click=move |e| e.stop_propagation()
                    on:change=move |_| toggle()
                />
            </label>

            {
                let card_body = view! {
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
                            {variant_badge.map(|label| view! {
                                <span class=style::variant_badge>{label}</span>
                            })}
                        </div>
                        <div class=card_style::caption>
                            <p class=card_style::caption_title>{title}</p>
                            <p
                                class=card_style::caption_sub
                                style:display=move || if has_subtitle { "" } else { "none" }
                            >
                                {subtitle_text}
                            </p>
                        </div>
                    </div>
                };
                match href {
                    Some(href) => view! { <a href=href>{card_body}</a> }.into_any(),
                    None => view! { <div>{card_body}</div> }.into_any(),
                }
            }
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

fn localize_episode_subtitle(subtitle: &str) -> Option<String> {
    let (show, episode_part) = subtitle.rsplit_once(" - S")?;
    let (season, episode) = episode_part.split_once(" E")?;
    episode_display_subtitle(
        Some(show),
        season.trim().parse::<i64>().ok(),
        episode.trim().parse::<i64>().ok(),
    )
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
                title=tr(TextKey::ItemActions)
                aria-label=tr(TextKey::ItemActions)
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
                    <span>{tr(TextKey::AddToCollection)}</span>
                </button>
                <button
                    type="button"
                    class=style::item_menu_entry
                    role="menuitem"
                    disabled=!has_media_info
                    style:display=move || if has_media_info { "" } else { "none" }
                    on:click={
                        let target = target_for_info.clone();
                        move |e| {
                            e.prevent_default();
                            e.stop_propagation();
                            if !has_media_info {
                                return;
                            }
                            open.set(false);
                            item_menu_command.set(Some(MenuCommand::ShowMediaInfo(target.clone())));
                        }
                    }
                >
                    <span style=icon_style(IconName::Info, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::ShowMediaInfo)}</span>
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
                    <span>{tr(TextKey::TroubleshootPlayback)}</span>
                </a>
            </div>
        </div>
    }
}

/// Sticky bulk-selection toolbar shared by the media-browser lists and detail
/// pages with selectable rows (e.g. artist music videos): selected count plus
/// bulk Add To Collection / Add To Playlist / Clear Selection actions.
/// Rendered in the DOM at all times and toggled via `style:display` to keep
/// the SSR and hydrated DOM shapes identical.
#[component]
pub(crate) fn SelectionToolbar(
    selected: RwSignal<HashSet<i64>>,
    collection_open: RwSignal<bool>,
    playlist_open: RwSignal<bool>,
) -> impl IntoView {
    let selected_count = Signal::derive(move || selected.with(HashSet::len));

    view! {
        <div
            class=style::selection_header
            style:display=move || if selected_count.get() == 0 { "none" } else { "" }
        >
            <span class=style::selection_count>
                {move || {
                    selected_items_count(selected_count.get())
                }}
            </span>
            <button
                class=style::selection_btn
                on:click=move |_| collection_open.set(true)
            >
                {format!("+ {}", tr(TextKey::AddToCollection))}
            </button>
            <button
                class=style::selection_btn
                on:click=move |_| playlist_open.set(true)
            >
                {format!("+ {}", tr(TextKey::AddToPlaylist))}
            </button>
            <button
                class=style::selection_btn_secondary
                on:click=move |_| selected.update(HashSet::clear)
            >
                {format!("✓ {}", tr(TextKey::ClearSelection))}
            </button>
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
    #[prop(optional)] playback: Option<RwSignal<Option<PlaybackOverrideForm>>>,
) -> impl IntoView {
    let refresh: RwSignal<u32> = RwSignal::new(0);
    let options: RwSignal<Vec<CollectionOption>> = RwSignal::new(Vec::new());

    let collection_id: RwSignal<i64> = RwSignal::new(NEW_COLLECTION_ID);
    let new_name: RwSignal<String> = RwSignal::new(String::new());

    let toast = use_toast();

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        let _ = refresh.get();
        leptos::task::spawn_local(async move {
            let loaded = match list_all_collections().await {
                Ok(loaded) => loaded,
                Err(error) => {
                    toast.push(
                        format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                        ToastSeverity::Error,
                    );
                    return;
                }
            };
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
        let playback = playback.and_then(|signal| signal.get_untracked());
        async move {
            let target_id = if chosen_id == NEW_COLLECTION_ID {
                let name = new_collection_name.trim();
                if name.is_empty() {
                    return Err(ServerFnError::new(tr(TextKey::CollectionNameRequired)));
                }
                create_collection(name.to_string()).await?.id
            } else {
                chosen_id
            };
            let added =
                add_media_items_to_collection_with_playback(target_id, ids.clone(), playback)
                    .await?;
            Ok::<(usize, u64, i64), ServerFnError>((ids.len(), added, target_id))
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _added, target_id)) => {
                    // Stay in the current browsing/detail context; the toast
                    // offers the explicit navigation path instead.
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
                    refresh.update(|n| *n += 1);
                    collection_id.set(NEW_COLLECTION_ID);
                    new_name.set(String::new());
                }
                Err(e) => toast.push(add_to_collection_failed(e), ToastSeverity::Error),
            }
        }
    });

    let count_label = Signal::derive(move || selected.with(HashSet::len));

    view! {
        <Modal
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
            <p>
                {move || select_collection_to_add_selected_items(count_label.get())}
            </p>

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
                        {tr(TextKey::Close)}
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
    #[prop(optional)] playback: Option<RwSignal<Option<PlaybackOverrideForm>>>,
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
        if !open.get() {
            return;
        }
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
            if gid == 0 {
                return;
            }
            if group_id.get_untracked() == gid {
                playlists.set(loaded);
            }
        });
    });

    let submit_action = Action::new(move |(): &()| {
        let ids: Vec<i64> = selected.with_untracked(|s| s.iter().copied().collect());
        let pid = playlist_id.get_untracked();
        let playback = playback.and_then(|signal| signal.get_untracked());
        async move {
            if pid == 0 {
                return Err(ServerFnError::new(tr(TextKey::PleaseChoosePlaylist)));
            }
            let added =
                add_media_items_to_playlist_with_playback(kind, pid, ids.clone(), playback).await?;
            Ok::<(usize, u64, i64), ServerFnError>((ids.len(), added, pid))
        }
    });

    Effect::new(move |_| {
        if let Some(res) = submit_action.value().get() {
            match res {
                Ok((requested, _added, target_id)) => {
                    // Stay in the current browsing/detail context; the toast
                    // offers the explicit navigation path instead.
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
                Err(e) => toast.push(add_to_playlist_failed(e), ToastSeverity::Error),
            }
        }
    });

    let count_label = Signal::derive(move || selected.with(HashSet::len));

    view! {
        <Modal
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
            <p>
                {move || select_playlist_to_add_selected_items(count_label.get())}
            </p>

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
        item_count(0)
    } else {
        range_of(start, end, total)
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
            <span
                class=format!("{} {}", card_style::pager_btn, card_style::pager_btn_disabled)
                style:display=move || if prev_disabled { "" } else { "none" }
            >
                "‹"
            </span>
            <a
                class=card_style::pager_btn
                href=prev_href
                style:display=move || if prev_disabled { "none" } else { "" }
            >
                "‹"
            </a>
            <span>{summary}</span>
            <span
                class=format!("{} {}", card_style::pager_btn, card_style::pager_btn_disabled)
                style:display=move || if next_disabled { "" } else { "none" }
            >
                "›"
            </span>
            <a
                class=card_style::pager_btn
                href=next_href
                style:display=move || if next_disabled { "none" } else { "" }
            >
                "›"
            </a>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localizes_episode_subtitle_from_ersatztv_shape() {
        assert_eq!(
            localize_episode_subtitle("Show - S2 E5").as_deref(),
            Some("Show - S2 E5")
        );
    }

    #[test]
    fn ignores_episode_subtitle_without_expected_shape() {
        assert_eq!(localize_episode_subtitle("Show"), None);
    }
}
