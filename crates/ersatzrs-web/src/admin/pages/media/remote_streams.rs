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
use crate::admin::server_fns::media_browser::{
    list_remote_streams, remote_stream_page_map, MediaBrowserKind, MediaCardRow,
};
use crate::admin::ui::primitives::card::first_letter_placeholder;
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::letter_bar::{
    letter_bucket, FragmentLetterAnchor, LetterBar, LetterJump,
};

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

    view! {
        <RemoteStreamsNoScriptFallback query=query.clone() page=page/>
        <RemoteStreamsIsland query=query page=page/>
    }
}

#[component]
fn RemoteStreamsNoScriptFallback(query: String, page: u32) -> impl IntoView {
    let (items, total) = remote_stream_rows_for_noscript(query.clone(), page);

    view! {
        <noscript>
            <style>"leptos-island { display: none !important; }"</style>
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
        </noscript>
    }
}

#[cfg(feature = "ssr")]
fn remote_stream_rows_for_noscript(query: String, page: u32) -> (Vec<MediaCardRow>, u64) {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(list_remote_streams(query, page, PAGE_SIZE))
            .unwrap_or_default()
    })
}

#[cfg(not(feature = "ssr"))]
fn remote_stream_rows_for_noscript(_query: String, _page: u32) -> (Vec<MediaCardRow>, u64) {
    (Vec::new(), 0)
}

#[island]
fn RemoteStreamsIsland(query: String, page: u32) -> impl IntoView {
    let q_for_rows = query.clone();
    let rows: Resource<(Vec<MediaCardRow>, u64)> = Resource::new_blocking(
        move || (q_for_rows.clone(), page),
        move |(q, p)| async move {
            list_remote_streams(q, p, PAGE_SIZE)
                .await
                .unwrap_or_default()
        },
    );

    let q_for_map = query.clone();
    let page_map = Resource::new_blocking(
        move || q_for_map.clone(),
        move |q| async move {
            remote_stream_page_map(q, PAGE_SIZE)
                .await
                .unwrap_or_default()
        },
    );

    view! {
        <Suspense fallback=move || view! {
            <div class=style::page>
                <div class=style::action_bar>
                    <div class=style::action_row>
                        <span class=style::query_label>{tr(TextKey::Loading)}</span>
                    </div>
                </div>
                <div class=style::content>
                    <h1 class=style::title>{tr(TextKey::RemoteStreams)}</h1>
                    <div class=style::divider></div>
                    <p class=style::empty>{tr(TextKey::Loading)}</p>
                </div>
            </div>
        }>
            {move || {
                let query_for_view = query.clone();
                rows.get().map(|(items, total)| {
                    let present: Signal<Option<HashSet<char>>> = Signal::derive({
                        let items = items.clone();
                        move || Some(items.iter().map(|r| letter_bucket(&r.sort_title)).collect())
                    });
                    let jump = page_map.get().map(|map| LetterJump {
                        base_uri: BASE_PATH.to_owned(),
                        query: query_for_view.clone(),
                        page_map: map,
                    });

                    view! {
                        {match jump {
                            Some(jump) => view! { <LetterBar present=present jump=jump/> }.into_any(),
                            None => view! { <LetterBar present=present/> }.into_any(),
                        }}
                        <RemoteStreamsContent query=query_for_view page=page rows=items total=total/>
                    }
                    .into_any()
                })
            }}
        </Suspense>
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
                        {move || if selected_count.get() == 0 {
                            view! {
                                <button
                                    type="button"
                                    class=style::btn_primary
                                    disabled=select_all_disabled
                                    on:click=move |e| select_all.run(e)
                                >
                                    <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                                    <span>{tr(TextKey::SelectAll)}</span>
                                </button>
                            }.into_any()
                        } else {
                            view! {
                                <button type="button" class=style::btn_primary on:click=move |_| collection_open.set(true)>
                                    <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                                    <span>{tr(TextKey::AddToCollection)}</span>
                                </button>
                                <button type="button" class=style::btn_primary on:click=move |_| playlist_open.set(true)>
                                    <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                                    <span>{tr(TextKey::AddToPlaylist)}</span>
                                </button>
                                <button type="button" class=style::btn_secondary on:click=move |_| {
                                    selected.update(HashSet::clear);
                                    last_selected.set(None);
                                }>
                                    <span style=icon_style(IconName::Check, "1rem") aria-hidden="true"></span>
                                    <span>{tr(TextKey::ClearSelection)}</span>
                                </button>
                            }.into_any()
                        }}
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
                    {if rows.is_empty() {
                        view! { <p class=style::empty>{tr(TextKey::NoItemsToDisplay)}</p> }.into_any()
                    } else {
                        let sorted_ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
                        view! {
                            <div class=style::grid>
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
                                            />
                                        }
                                        .into_any()
                                    }
                                />
                            </div>
                        }.into_any()
                    }}
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
        {move || if selected_count.get() == 0 {
            let label = format_remote_stream_count(&query, total);
            view! { <span class=style::query_label title=label.clone()>{label.clone()}</span> }.into_any()
        } else {
            view! {
                <span class=style::selection_label>
                    {selected_items_count(selected_count.get())}
                </span>
            }.into_any()
        }}
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
            {if prev_disabled {
                view! { <span class=disabled_class.clone()> {chevron(IconName::ChevronLeft)} </span> }.into_any()
            } else {
                view! { <a class=style::pager_btn href=prev_href>{chevron(IconName::ChevronLeft)}</a> }.into_any()
            }}
            <span>{summary}</span>
            {if next_disabled {
                view! { <span class=disabled_class>{chevron(IconName::ChevronRight)}</span> }.into_any()
            } else {
                view! { <a class=style::pager_btn href=next_href>{chevron(IconName::ChevronRight)}</a> }.into_any()
            }}
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
                {move || if selected_count.get() == 0 {
                    view! {
                        <button
                            type="button"
                            role="menuitem"
                            class=style::mobile_menu_entry
                            disabled=select_all_disabled
                            on:click=move |e| {
                                open.set(false);
                                select_all.run(e);
                            }
                        >
                            <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::SelectAll)}</span>
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <button type="button" role="menuitem" class=style::mobile_menu_entry on:click=move |_| {
                            open.set(false);
                            open_collection.set(true);
                        }>
                            <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::AddToCollection)}</span>
                        </button>
                        <button type="button" role="menuitem" class=style::mobile_menu_entry on:click=move |_| {
                            open.set(false);
                            open_playlist.set(true);
                        }>
                            <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::AddToPlaylist)}</span>
                        </button>
                        <button type="button" role="menuitem" class=style::mobile_menu_entry on:click=move |_| {
                            open.set(false);
                            selected.update(HashSet::clear);
                            last_selected.set(None);
                        }>
                            <span style=icon_style(IconName::Check, "1rem") aria-hidden="true"></span>
                            <span>{tr(TextKey::ClearSelection)}</span>
                        </button>
                    }.into_any()
                }}
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
) -> impl IntoView {
    let item_id = row.id;
    let title = row.title.clone();
    let subtitle = row.subtitle.clone();
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
                        </div>
                    </details>
                </div>
                <div class=card_style::caption>
                    <p class=card_style::caption_title title=title.clone()>{title.clone()}</p>
                    {subtitle.map(|value| view! {
                        <p class=card_style::caption_sub title=value.clone()>{value.clone()}</p>
                    })}
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::page_href;

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
