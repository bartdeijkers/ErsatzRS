//! Remote-stream browser parity for `$ERSATZTV_SRC/ErsatzTV/Pages/RemoteStreamList.razor`.
//!
//! The upstream route is a `MultiSelectBase` media-card browser: 64px pager /
//! selection action strip, 100-card pages, `type:remote_stream` search-index
//! query, `LetterBar` page jumps, thumbnail cards, and add-to-collection /
//! add-to-playlist side effects. The pinned ErsatzTV source has no
//! route-local create/edit/delete editor for remote streams.
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{item_count, range_of, selected_items_count, tr, TextKey};
use crate::admin::pages::media::multi_select_browser::{AddToCollectionModal, AddToPlaylistModal};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::layout::media_state_revision;
use crate::admin::server_fns::media_browser::{
    list_remote_stream_tags, list_remote_streams, remote_stream_page_map, MediaBrowserKind,
    MediaCardRow,
};
use crate::admin::ui::primitives::card::first_letter_placeholder;
use crate::admin::ui::primitives::facet_bar::{FacetBar, FacetLink};
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::letter_bar::{
    letter_bucket, FragmentLetterAnchor, LetterBar, LetterJump,
};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
#[cfg(target_arch = "wasm32")]
use crate::admin::ui::primitives::toast::ToastSeverity;
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast};

stylance::import_crate_style!(style, "src/admin/pages/media/remote_streams.module.css");
mod card_style {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/ui/primitives/card.module.css");
    pub use s::*;
}

const PAGE_SIZE: u32 = 100;
const BASE_PATH: &str = "/media/remote/streams";

#[component]
pub fn RemoteStreamsPage() -> impl IntoView {
    let query_map = use_query_map();
    let params = use_params_map();

    let query = query_map.with_untracked(|p| p.get("query").unwrap_or_default());
    let page = params.with_untracked(|p| {
        p.get("page")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1)
            .saturating_sub(1)
    });
    let initial_query = query.clone();
    let initial = Resource::new_blocking(
        move || (initial_query.clone(), page),
        move |(query, page)| async move {
            tokio::try_join!(
                list_remote_streams(query.clone(), page, PAGE_SIZE),
                remote_stream_page_map(query, PAGE_SIZE),
            )
        },
    );

    view! {
        <RemoteStreamTagFacets query=query.clone()/>
        <RemoteStreamsNoScriptFallback query=query.clone() page=page/>
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let query = query.clone();
                initial.get().map(move |result| admin_result_view(
                    result,
                    |((initial_rows, initial_total), initial_page_map)| view! {
                        <RemoteStreamsIsland
                            query=query.clone()
                            page=page
                            initial_rows=initial_rows
                            initial_total=initial_total
                            initial_page_map=initial_page_map
                        />
                    }.into_any(),
                ))
            }}
        </Suspense>
    }
}

#[component]
fn RemoteStreamTagFacets(query: String) -> impl IntoView {
    let tags = Resource::new(|| (), |()| list_remote_stream_tags());
    let active = active_tag(&query);
    view! {
        <Suspense fallback=|| ()>
            {move || {
                let active = active.clone();
                tags.get().map(|result| admin_result_view(result, move |tags| {
                    let items: Vec<FacetLink> = tags
                        .into_iter()
                        .map(|tag| FacetLink {
                            href: tag_query_href(&tag),
                            label: tag,
                        })
                        .collect();
                    view! {
                        <FacetBar
                            id="tag"
                            label=tr(TextKey::Tags)
                            items=items
                            active=active
                            reset_href=BASE_PATH
                            reset_label=tr(TextKey::All)
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

/// Browser link that filters the list down to one tag.
fn tag_query_href(tag: &str) -> String {
    format!(
        "{BASE_PATH}?query={}",
        urlencoding::encode(&format!("tag:\"{tag}\""))
    )
}

/// Reads back the tag a `tag:"…"` browser query filters on, so the rail can
/// mark that chip active and offer a way back to the unfiltered list.
fn active_tag(query: &str) -> Option<String> {
    let value = query.trim().strip_prefix("tag:\"")?.strip_suffix('"')?;
    (!value.is_empty()).then(|| value.to_owned())
}

#[component]
fn RemoteStreamsNoScriptFallback(query: String, page: u32) -> impl IntoView {
    let rows = remote_stream_rows_for_noscript(query.clone(), page);

    view! {
        <noscript>
            <style>"leptos-island { display: none !important; }"</style>
            {admin_result_view(rows, move |(items, total)| view! {
                <div class=style::page>
                    <div class=style::action_bar>
                        <div class=style::action_row>
                            <span class=style::query_label>
                                {format_remote_stream_count(&query, total)}
                            </span>
                            <Pager query=query.clone() page=page total=total/>
                            <div class=style::actions></div>
                        </div>
                    </div>
                    <div class=style::scroll>
                        <section class=style::content>
                            <h1 class=style::title>{tr(TextKey::RemoteStreams)}</h1>
                            <div class=style::divider></div>
                            {if items.is_empty() {
                                view! { <p class=style::empty>{tr(TextKey::NoItemsToDisplay)}</p> }.into_any()
                            } else {
                                view! {
                                    <div class=style::grid>
                                        {items.into_iter().map(|row| {
                                            let title = row.title;
                                            let subtitle = row.subtitle;
                                            let placeholder = first_letter_placeholder(&row.sort_title);
                                            view! {
                                                <div class=card_style::card>
                                                    <div class=card_style::poster>
                                                        <div class=card_style::poster_placeholder>{placeholder}</div>
                                                    </div>
                                                    <div class=card_style::caption>
                                                        <p class=card_style::caption_title title=title.clone()>{title.clone()}</p>
                                                        {subtitle.map(|value| view! {
                                                            <p class=card_style::caption_sub title=value.clone()>{value.clone()}</p>
                                                        })}
                                                    </div>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </section>
                    </div>
                </div>
            }.into_any())}
        </noscript>
    }
}

#[cfg(feature = "ssr")]
fn remote_stream_rows_for_noscript(
    query: String,
    page: u32,
) -> Result<(Vec<MediaCardRow>, u64), String> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(list_remote_streams(query, page, PAGE_SIZE))
            .map_err(|error| error.to_string())
    })
}

#[cfg(not(feature = "ssr"))]
fn remote_stream_rows_for_noscript(
    _query: String,
    _page: u32,
) -> Result<(Vec<MediaCardRow>, u64), String> {
    Ok((Vec::new(), 0))
}

#[island]
fn RemoteStreamsIsland(
    query: String,
    page: u32,
    initial_rows: Vec<MediaCardRow>,
    initial_total: u64,
    initial_page_map: crate::admin::server_fns::media_browser::LetterPageMap,
) -> impl IntoView {
    let media_revision = RwSignal::new(String::new());
    let current_rows = RwSignal::new((initial_rows, initial_total));
    let current_page_map = RwSignal::new(initial_page_map);
    let load_toast = use_toast();
    let q_for_rows = query.clone();
    let rows: LocalResource<(Vec<MediaCardRow>, u64)> = LocalResource::new(move || {
        let q = q_for_rows.clone();
        let p = page;
        let _ = media_revision.get();
        async move { load_or_notify(list_remote_streams(q, p, PAGE_SIZE).await, load_toast) }
    });
    Effect::new(move |_| {
        if let Some(next_rows) = rows.get() {
            current_rows.maybe_update(|current| {
                if *current == next_rows {
                    false
                } else {
                    *current = next_rows;
                    true
                }
            });
        }
    });

    let q_for_map = query.clone();
    let page_map = LocalResource::new(move || {
        let q = q_for_map.clone();
        let _ = media_revision.get();
        async move { load_or_notify(remote_stream_page_map(q, PAGE_SIZE).await, load_toast) }
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
            let query_for_view = query.clone();
            let (items, total) = current_rows.get();
            let present: Signal<Option<HashSet<char>>> = Signal::derive({
                let items = items.clone();
                move || Some(items.iter().map(|r| letter_bucket(&r.sort_title)).collect())
            });
            let jump = LetterJump {
                base_uri: BASE_PATH.to_owned(),
                query: query_for_view.clone(),
                page_map: current_page_map.get(),
            };

            view! {
                <LetterBar present=present jump=jump/>
                <RemoteStreamsContent query=query_for_view page=page rows=items total=total/>
            }
        }}
    }
}

#[component]
fn RemoteStreamsContent(
    query: String,
    page: u32,
    rows: Vec<MediaCardRow>,
    total: u64,
) -> impl IntoView {
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| {
        hydrated.set(true);
    });

    let selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let last_selected: RwSignal<Option<i64>> = RwSignal::new(None);
    let selected_count = Signal::derive(move || selected.with(HashSet::len));

    let collection_open = RwSignal::new(false);
    let playlist_open = RwSignal::new(false);

    let page_ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let page_ids_for_select_all = page_ids.clone();
    let select_all_disabled = page_ids.is_empty();

    let select_all = Callback::new(move |_: MouseEvent| {
        let ids = page_ids_for_select_all.clone();
        selected.update(|items| {
            items.clear();
            items.extend(ids.iter().copied());
        });
        last_selected.set(ids.last().copied());
    });

    view! {
        <div
            class=style::page
            data-remote-streams-hydrated=move || if hydrated.get() { "true" } else { "false" }
        >
            <div class=style::action_bar>
                <div class=style::action_row>
                    <ActionLabel query=query.clone() total=total selected_count=selected_count/>
                    <Pager query=query.clone() page=page total=total/>
                    <div class=style::actions>
                        <button
                            type="button"
                            class=style::btn_primary
                            disabled=select_all_disabled
                            style:display=move || if selected_count.get() == 0 { "" } else { "none" }
                            on:click=move |e| select_all.run(e)
                        >
                            <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::SelectAll)}</span>
                        </button>
                        <button
                            type="button"
                            class=style::btn_primary
                            style:display=move || if selected_count.get() == 0 { "none" } else { "" }
                            on:click=move |_| collection_open.set(true)
                        >
                            <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::AddToCollection)}</span>
                        </button>
                        <button
                            type="button"
                            class=style::btn_primary
                            style:display=move || if selected_count.get() == 0 { "none" } else { "" }
                            on:click=move |_| playlist_open.set(true)
                        >
                            <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::AddToPlaylist)}</span>
                        </button>
                        <button
                            type="button"
                            class=style::btn_secondary
                            style:display=move || if selected_count.get() == 0 { "none" } else { "" }
                            on:click=move |_| {
                                selected.update(HashSet::clear);
                                last_selected.set(None);
                            }
                        >
                            <span style=icon_style(IconName::Check, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::ClearSelection)}</span>
                        </button>
                    </div>
                    <MobileActionMenu
                        selected_count=selected_count
                        select_all_disabled=select_all_disabled
                        select_all=select_all
                        open_collection=collection_open
                        open_playlist=playlist_open
                        selected=selected
                        last_selected=last_selected
                    />
                </div>
            </div>

            <div class=style::scroll>
                <section class=style::content>
                    <h1 class=style::title>{tr(TextKey::RemoteStreams)}</h1>
                    <div class=style::divider></div>
                    {
                        let rows_empty = rows.is_empty();
                        let sorted_ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
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
                                    sort_title=|row: &MediaCardRow| row.sort_title.as_str()
                                    render=move |row: MediaCardRow| {
                                        view! {
                                            <RemoteStreamCard
                                                row=row
                                                sorted_ids=sorted_ids.clone()
                                                selected=selected
                                                last_selected=last_selected
                                                open_collection=collection_open
                                                open_playlist=playlist_open
                                            />
                                        }
                                        .into_any()
                                    }
                                />
                            </div>
                        }
                    }
                </section>
            </div>

            <AddToCollectionModal open=collection_open selected=selected/>
            <AddToPlaylistModal
                open=playlist_open
                selected=selected
                kind=MediaBrowserKind::RemoteStream
            />
        </div>
    }
}

#[component]
fn ActionLabel(query: String, total: u64, selected_count: Signal<usize>) -> impl IntoView {
    view! {
        {
            let label = format_remote_stream_count(&query, total);
            view! {
                <span
                    class=style::query_label
                    title=label.clone()
                    style:display=move || if selected_count.get() == 0 { "" } else { "none" }
                >
                    {label.clone()}
                </span>
                <span
                    class=style::selection_label
                    style:display=move || if selected_count.get() == 0 { "none" } else { "" }
                >
                    {move || selected_items_count(selected_count.get())}
                </span>
            }
        }
    }
}

fn format_remote_stream_count(query: &str, total: u64) -> String {
    let query = query.trim();
    let count = item_count(total);
    if query.is_empty() {
        count
    } else {
        format!("{query}  {count}")
    }
}

#[component]
fn Pager(query: String, page: u32, total: u64) -> impl IntoView {
    let max_page = if total == 0 {
        0
    } else {
        (total - 1) / u64::from(PAGE_SIZE)
    };
    let current_page = u64::from(page);
    let prev_disabled = page == 0;
    let next_disabled = current_page >= max_page;
    let start = page * PAGE_SIZE + 1;
    let end = ((page + 1) * PAGE_SIZE).min(u32::try_from(total).unwrap_or(u32::MAX));
    let summary = if total == 0 {
        range_of(0, 0, 0)
    } else {
        range_of(start, end, total)
    };

    let prev_href = page_href(&query, page.saturating_sub(1));
    let next_href = page_href(&query, page + 1);
    let disabled_class = format!("{} {}", style::pager_btn, style::pager_btn_disabled);

    view! {
        <div class=style::pager>
            <span
                class=disabled_class.clone()
                style:display=move || if prev_disabled { "" } else { "none" }
            >
                {chevron(IconName::ChevronLeft)}
            </span>
            <a
                class=style::pager_btn
                href=prev_href
                style:display=move || if prev_disabled { "none" } else { "" }
            >
                {chevron(IconName::ChevronLeft)}
            </a>
            <span>{summary}</span>
            <span
                class=disabled_class
                style:display=move || if next_disabled { "" } else { "none" }
            >
                {chevron(IconName::ChevronRight)}
            </span>
            <a
                class=style::pager_btn
                href=next_href
                style:display=move || if next_disabled { "none" } else { "" }
            >
                {chevron(IconName::ChevronRight)}
            </a>
        </div>
    }
}

fn chevron(icon: IconName) -> AnyView {
    view! { <span style=icon_style(icon, "1.25rem") aria-hidden="true"></span> }.into_any()
}

fn page_href(query: &str, page: u32) -> String {
    let mut url = if page == 0 {
        BASE_PATH.to_owned()
    } else {
        format!("{BASE_PATH}/page/{}", page + 1)
    };
    let trimmed = query.trim();
    if !trimmed.is_empty() {
        url.push_str("?query=");
        url.push_str(&urlencoding::encode(trimmed));
    }
    url
}

#[component]
fn MobileActionMenu(
    selected_count: Signal<usize>,
    select_all_disabled: bool,
    select_all: Callback<MouseEvent>,
    open_collection: RwSignal<bool>,
    open_playlist: RwSignal<bool>,
    selected: RwSignal<HashSet<i64>>,
    last_selected: RwSignal<Option<i64>>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <div class=style::mobile_menu>
            <button
                type="button"
                class=style::pager_btn
                aria-label=tr(TextKey::ItemActions)
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|value| *value = !*value)
            >
                <span style=icon_style(IconName::MoreVert, "1.25rem") aria-hidden="true"></span>
            </button>
            <div
                class=style::mobile_menu_popover
                role="menu"
                style:display=move || if open.get() { "" } else { "none" }
            >
                <button
                    type="button"
                    role="menuitem"
                    class=style::mobile_menu_entry
                    disabled=select_all_disabled
                    style:display=move || if selected_count.get() == 0 { "" } else { "none" }
                    on:click=move |e| {
                        open.set(false);
                        select_all.run(e);
                    }
                >
                    <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::SelectAll)}</span>
                </button>
                <button
                    type="button"
                    role="menuitem"
                    class=style::mobile_menu_entry
                    style:display=move || if selected_count.get() == 0 { "none" } else { "" }
                    on:click=move |_| {
                        open.set(false);
                        open_collection.set(true);
                    }
                >
                    <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::AddToCollection)}</span>
                </button>
                <button
                    type="button"
                    role="menuitem"
                    class=style::mobile_menu_entry
                    style:display=move || if selected_count.get() == 0 { "none" } else { "" }
                    on:click=move |_| {
                        open.set(false);
                        open_playlist.set(true);
                    }
                >
                    <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::AddToPlaylist)}</span>
                </button>
                <button
                    type="button"
                    role="menuitem"
                    class=style::mobile_menu_entry
                    style:display=move || if selected_count.get() == 0 { "none" } else { "" }
                    on:click=move |_| {
                        open.set(false);
                        selected.update(HashSet::clear);
                        last_selected.set(None);
                    }
                >
                    <span style=icon_style(IconName::Check, "1rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::ClearSelection)}</span>
                </button>
            </div>
        </div>
    }
}

#[component]
fn RemoteStreamCard(
    row: MediaCardRow,
    sorted_ids: Vec<i64>,
    selected: RwSignal<HashSet<i64>>,
    last_selected: RwSignal<Option<i64>>,
    open_collection: RwSignal<bool>,
    open_playlist: RwSignal<bool>,
) -> impl IntoView {
    let item_id = row.id;
    let title = row.title.clone();
    let subtitle = row.subtitle.clone().unwrap_or_default();
    let has_subtitle = !subtitle.is_empty();
    let artwork = row.artwork_path.clone();
    let placeholder = first_letter_placeholder(&row.sort_title);
    let is_selected = Signal::derive(move || selected.with(|items| items.contains(&item_id)));
    let wrap_class = move || {
        if is_selected.get() {
            format!("{} {}", style::card_wrap, style::selected)
        } else {
            style::card_wrap.to_owned()
        }
    };

    let toggle = Callback::new(move |shift: bool| {
        selected.update(|items| {
            if shift {
                if let Some(previous) = last_selected.get_untracked() {
                    if let (Some(start), Some(end)) = (
                        sorted_ids.iter().position(|id| *id == previous),
                        sorted_ids.iter().position(|id| *id == item_id),
                    ) {
                        let (start, end) = if start <= end {
                            (start, end)
                        } else {
                            (end, start)
                        };
                        for id in &sorted_ids[start..=end] {
                            items.insert(*id);
                        }
                        last_selected.set(Some(item_id));
                        return;
                    }
                }
            }

            if items.remove(&item_id) {
                last_selected.set(None);
            } else {
                items.insert(item_id);
                last_selected.set(Some(item_id));
            }
        });
    });

    view! {
        <div class=wrap_class>
            <label
                class=style::checkbox
                aria-label=tr(TextKey::Select)
                on:click=move |e: MouseEvent| {
                    e.prevent_default();
                    e.stop_propagation();
                    toggle.run(e.shift_key());
                }
            >
                <input
                    type="checkbox"
                    prop:checked=move || is_selected.get()
                    on:click=move |e| e.stop_propagation()
                    on:change=move |_| toggle.run(false)
                />
            </label>
            <div class=card_style::card on:click=move |e: MouseEvent| toggle.run(e.shift_key())>
                <div class=card_style::poster>
                    {match artwork {
                        Some(url) => view! {
                            <img src=url loading="lazy" class=card_style::poster_img alt=title.clone()/>
                        }.into_any(),
                        None => view! {
                            <div class=card_style::poster_placeholder>{placeholder}</div>
                        }.into_any(),
                    }}
                    <details class=card_style::item_menu on:click=|e: MouseEvent| e.stop_propagation()>
                        <summary class=card_style::item_menu_trigger aria-label=tr(TextKey::ItemActions)>
                            <span style=icon_style(IconName::MoreVert, "1.25rem") aria-hidden="true"></span>
                        </summary>
                        <div class=card_style::item_menu_popover role="menu">
                            <button
                                type="button"
                                class=card_style::item_menu_entry
                                role="menuitem"
                                data-media-item-id=item_id.to_string()
                                on:click=move |e| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    selected.update(|items| {
                                        items.clear();
                                        items.insert(item_id);
                                    });
                                    last_selected.set(Some(item_id));
                                    open_collection.set(true);
                                }
                            >
                                <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::AddToCollection)}</span>
                            </button>
                            <button
                                type="button"
                                class=card_style::item_menu_entry
                                role="menuitem"
                                data-media-item-id=item_id.to_string()
                                on:click=move |e| {
                                    e.prevent_default();
                                    e.stop_propagation();
                                    selected.update(|items| {
                                        items.clear();
                                        items.insert(item_id);
                                    });
                                    last_selected.set(Some(item_id));
                                    open_playlist.set(true);
                                }
                            >
                                <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                                <span>{tr(TextKey::AddToPlaylist)}</span>
                            </button>
                        </div>
                    </details>
                </div>
                <div class=card_style::caption>
                    <p class=card_style::caption_title title=title.clone()>{title.clone()}</p>
                    <p
                        class=card_style::caption_sub
                        title=subtitle.clone()
                        style:display=move || if has_subtitle { "" } else { "none" }
                    >
                        {subtitle.clone()}
                    </p>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{active_tag, page_href, tag_query_href};

    #[test]
    fn active_tag_round_trips_the_facet_link_query() {
        let href = tag_query_href("Dutch TV");
        let query = href.split_once("?query=").expect("query string").1;
        let decoded = urlencoding::decode(query).expect("valid encoding");

        assert_eq!(active_tag(&decoded).as_deref(), Some("Dutch TV"));
    }

    #[test]
    fn active_tag_ignores_queries_that_are_not_a_single_tag_filter() {
        assert_eq!(active_tag(""), None);
        assert_eq!(active_tag("library_id:7"), None);
        assert_eq!(active_tag(r#"tag:"""#), None);
        assert_eq!(active_tag(r#"tag:"news" AND library_id:7"#), None);
    }

    #[test]
    fn remote_stream_page_links_match_ersatztv_route_shape() {
        assert_eq!(page_href("", 0), "/media/remote/streams");
        assert_eq!(page_href("", 1), "/media/remote/streams/page/2");
        assert_eq!(
            page_href("library_id:7", 1),
            "/media/remote/streams/page/2?query=library_id%3A7"
        );
    }
}
