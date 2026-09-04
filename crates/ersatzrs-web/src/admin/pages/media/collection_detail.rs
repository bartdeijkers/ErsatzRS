//! Collection detail page — port of `ErsatzTV/Pages/CollectionItems.razor`
//! (`/media/collections/:id`). Shows the manual collection's items as a
//! poster grid grouped by media kind, with edit-page navigation, multi-select
//! removal, per-item trashcan, and the "Use Custom Playback Order" toggle.
//!
//! Hydration shape (mirrors the proven movies-page pattern in
//! `crates/ersatzrs-web/src/admin/pages/media/movies.rs`):
//!
//! * The page wrapper is a plain `#[component]` (`CollectionDetailPage`).
//! * The page wrapper resolves the initial detail payload before rendering
//!   [`CollectionDetailIsland`]. The island receives that stable payload as a
//!   prop outside any `<Suspense>` boundary and uses a local resource only for
//!   post-mutation refreshes, so hydration never swaps a loading/not-found
//!   branch under the island.
//! * All sub-components (`Header`, `SelectionBar`, `CardGrid`,
//!   `SelectableCollectionCard`) are plain `#[component]`s reading
//!   `RwSignal`s created inside the island, so nothing nested is itself
//!   an island.
//!
//! The upstream page wires drag ordering through `SortableJS`; this port uses
//! native browser drag/drop for the same movie-only custom-order surface and
//! keeps explicit Move Up / Move Down buttons as an accessible fallback.
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{media_browser_error_message, text, tr, use_i18n, TextKey, UiLanguage};
use crate::admin::server_fns::media_browser::{
    get_collection_detail, localize_episode_display_text, remove_items_from_collection,
    set_collection_use_custom_order, update_collection_custom_order, CollectionDetail,
    CollectionItemCard,
};
use crate::admin::ui::primitives::{
    icon::{icon_style, IconName},
    resource_state::AdminErrorState,
    toast::{load_or_notify, use_toast, ToastSeverity},
};

// Card grid styles (poster, caption) come from the shared primitive.
#[allow(dead_code)]
mod card_style {
    stylance::import_crate_style!(s, "src/admin/ui/primitives/card.module.css");
    pub use s::*;
}

stylance::import_crate_style!(style, "src/admin/pages/media/collection_detail.module.css");

/// Page wrapper — plain `#[component]`, reads the `:id` route param and
/// renders the interactive island with that scalar prop.
#[component]
pub fn CollectionDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });

    if id == 0 {
        return view! {
            <div class=style::not_found>
                <h1>{tr(TextKey::CollectionNotFound)}</h1>
                <a href="/media/collections">{tr(TextKey::BackToCollections)}</a>
            </div>
        }
        .into_any();
    }

    let detail = load_collection_detail_initial(id);

    match detail {
        Ok(Some(detail)) => view! { <CollectionDetailIsland initial_detail=detail/> }.into_any(),
        Ok(None) => view! {
            <div class=style::not_found>
                <h1>{tr(TextKey::CollectionNotFound)}</h1>
                <a href="/media/collections">{tr(TextKey::BackToCollections)}</a>
            </div>
        }
        .into_any(),
        Err(error) => view! { <AdminErrorState error/> }.into_any(),
    }
}

fn load_collection_detail_initial(id: i64) -> Result<Option<CollectionDetail>, String> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { get_collection_detail(id).await })
                .map_err(|error| error.to_string())
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = id;
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Island: owns the resource, the rename state, the toggle state, the
// selection set, and dispatches all server-fn actions. Children are plain
// components reading island-owned signals.
// ---------------------------------------------------------------------------

#[island]
fn CollectionDetailIsland(initial_detail: CollectionDetail) -> impl IntoView {
    let id = initial_detail.id;
    // Refresh tick — bumped after every successful mutation so the resource
    // re-runs and the grid reflects the new state.
    let refresh: RwSignal<u32> = RwSignal::new(0);
    let current_detail = RwSignal::new(initial_detail);
    let load_toast = use_toast();
    let data: LocalResource<Option<CollectionDetail>> = LocalResource::new(move || {
        let _ = refresh.get();
        async move { load_or_notify(get_collection_detail(id).await, load_toast) }
    });

    Effect::new(move |_| {
        if let Some(Some(detail)) = data.get() {
            current_detail.set(detail);
        }
    });

    view! {
        <div class=style::page>
            {move || view! {
                <CollectionDetailView
                    id=id
                    detail=current_detail.get()
                    refresh=refresh
                />
            }}
        </div>
    }
}

/// Inner view rendered once the blocking resource resolves. All island-owned
/// `RwSignal`s are created here so that they live in the island's reactive
/// owner; sub-views read those signals as plain components.
#[component]
fn CollectionDetailView(
    id: i64,
    detail: CollectionDetail,
    refresh: RwSignal<u32>,
) -> impl IntoView {
    // ── Custom-order toggle ────────────────────────────────────────────
    let use_custom_order = RwSignal::new(detail.use_custom_playback_order);

    // ── Selection state (HashSet<collection_item_id>) — ADR 0048 rows are
    //    identified by the surrogate CollectionItem.Id so duplicate
    //    same-movie rows select/remove independently. ────────────────────
    let selected: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let selected_count = Signal::derive(move || selected.with(HashSet::len));

    // ── Items, in display order ────────────────────────────────────────
    // ErsatzTV sorts movies by `CustomIndex` when `UseCustomPlaybackOrder`
    // is on and the collection is movies-only; otherwise alphabetical per
    // section (Movies → Shows → ... → RemoteStreams). We keep the SQL-side
    // grouping by kind+sort_title; the toggle only takes effect when the
    // single "movies" section is present (matches `SupportsCustomOrdering`).
    let items_signal: RwSignal<Vec<CollectionItemCard>> = RwSignal::new(detail.items.clone());

    let supports_custom_order = {
        let items = detail.items.clone();
        items.iter().all(|i| i.kind == "movie") && !items.is_empty()
    };

    // ── Toast ───────────────────────────────────────────────────────────
    let toast = use_toast();

    // ── Actions ─────────────────────────────────────────────────────────
    let toggle_action = Action::new(move |on: &bool| {
        let on = *on;
        async move { set_collection_use_custom_order(id, on).await }
    });

    Effect::new(move |_| {
        if let Some(res) = toggle_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(tr(TextKey::PlaybackOrderUpdated), ToastSeverity::Success);
                    refresh.update(|n| *n += 1);
                }
                Err(e) => {
                    // Revert on failure.
                    use_custom_order.update(|v| *v = !*v);
                    toast.push(
                        format!(
                            "{}: {}",
                            tr(TextKey::PlaybackOrderUpdateFailed),
                            media_browser_error_message(e)
                        ),
                        ToastSeverity::Error,
                    );
                }
            }
        }
    });

    let remove_action = Action::new(move |ids: &Vec<i64>| {
        let ids = ids.clone();
        async move { remove_items_from_collection(id, ids).await }
    });

    Effect::new(move |_| {
        if let Some(res) = remove_action.value().get() {
            match res {
                Ok(()) => {
                    toast.push(tr(TextKey::RemovedFromCollection), ToastSeverity::Success);
                    selected.update(HashSet::clear);
                    refresh.update(|n| *n += 1);
                }
                Err(e) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::RemoveFailed),
                        media_browser_error_message(e)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let reorder_action = Action::new(move |order: &Vec<i64>| {
        let order = order.clone();
        async move { update_collection_custom_order(id, order).await }
    });

    Effect::new(move |_| {
        if let Some(Err(e)) = reorder_action.value().get() {
            toast.push(
                format!(
                    "{}: {}",
                    tr(TextKey::ReorderFailed),
                    media_browser_error_message(e)
                ),
                ToastSeverity::Error,
            );
            refresh.update(|n| *n += 1);
        }
    });

    // ── Callbacks ───────────────────────────────────────────────────────
    let on_toggle = Callback::new(move |new_value: bool| {
        use_custom_order.set(new_value);
        toggle_action.dispatch(new_value);
    });

    let on_remove_one = Callback::new(move |item_id: i64| {
        remove_action.dispatch(vec![item_id]);
    });

    let on_remove_selected = Callback::new(move |()| {
        let ids: Vec<i64> = selected.with_untracked(|s| s.iter().copied().collect());
        if !ids.is_empty() {
            remove_action.dispatch(ids);
        }
    });

    let on_clear_selection = Callback::new(move |()| {
        selected.update(HashSet::clear);
    });

    // Move an item up or down within `items_signal`. We persist the new
    // ordering across ALL items; ErsatzTV's `UpdateCollectionCustomOrder`
    // only writes the indices that changed, but writing all of them is
    // idempotent and keeps the index sequence dense.
    let on_move = Callback::new(move |(row_id, direction): (i64, i32)| {
        items_signal.update(|items| {
            let Some(pos) = items.iter().position(|i| i.collection_item_id == row_id) else {
                return;
            };
            let new_pos = if direction < 0 {
                pos.saturating_sub(1)
            } else {
                (pos + 1).min(items.len().saturating_sub(1))
            };
            if new_pos != pos {
                items.swap(pos, new_pos);
            }
        });
        let order: Vec<i64> = items_signal.with_untracked(|items| {
            items
                .iter()
                .filter(|i| i.kind == "movie")
                .map(|i| i.collection_item_id)
                .collect()
        });
        reorder_action.dispatch(order);
    });

    let on_reorder = Callback::new(move |order: Vec<i64>| {
        items_signal.update(|items| {
            for (idx, row_id) in order.iter().enumerate() {
                if let Some(item) = items
                    .iter_mut()
                    .find(|item| item.kind == "movie" && item.collection_item_id == *row_id)
                {
                    item.custom_index = i64::try_from(idx).unwrap_or(i64::MAX);
                }
            }
        });
        reorder_action.dispatch(order);
    });

    let section_links = section_links(&detail.items);

    view! {
        // ── Header (rename + summary + custom-order toggle) ────────────
        <Header
            id=id
            title=detail.name.clone()
            section_links=section_links
            selected_count=selected_count
            supports_custom_order=supports_custom_order
            use_custom_order=use_custom_order
            on_toggle=on_toggle
        />

        // ── Selection bar (visible when ≥1 selected) — always rendered
        //    in the DOM; visibility via `style:display` to keep the
        //    SSR / hydrate DOM shapes identical. ───────────────────────
        <SelectionBar
            count=selected_count
            on_remove=on_remove_selected
            on_clear=on_clear_selection
        />

        // ── Card grid grouped by kind. ─────────────────────────────────
        <div class=style::content>
            <CardGrid
                items=items_signal
                selected=selected
                use_custom_order=use_custom_order
                supports_custom_order=supports_custom_order
                on_remove_one=on_remove_one
                on_move=on_move
                on_reorder=on_reorder
            />
        </div>
    }
}

fn section_links(items: &[CollectionItemCard]) -> Vec<(TextKey, &'static str, usize)> {
    SECTION_ORDER
        .iter()
        .filter_map(|(kind, label)| {
            let count = items.iter().filter(|item| item.kind == *kind).count();
            (count > 0).then_some((*label, *kind, count))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Header (rename pencil + counts + custom-order switch).
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
#[component]
fn Header(
    id: i64,
    title: String,
    section_links: Vec<(TextKey, &'static str, usize)>,
    selected_count: Signal<usize>,
    supports_custom_order: bool,
    use_custom_order: RwSignal<bool>,
    on_toggle: Callback<bool>,
) -> impl IntoView {
    view! {
        <div
            class=style::action_bar
            style:display={move || if selected_count.get() > 0 { "none" } else { "" }}
        >
            <div class=style::normal_bar>
                <span class=style::title>{title}</span>
                <a
                    class=style::icon_btn
                    href=format!("/media/collections/{id}/edit")
                    aria-label=tr(TextKey::EditCollection)
                    title=tr(TextKey::EditCollection)
                >
                    <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                </a>

                <nav class=style::summary_links aria-label=tr(TextKey::Collection)>
                    {section_links.into_iter().map(|(label, anchor, count)| {
                        view! {
                            <a href=format!("#{anchor}")>
                                {format!("{count} {}", tr(label))}
                            </a>
                        }
                    }).collect_view()}
                </nav>

                <span class=style::spacer/>

                <label
                    class=style::toggle
                    style:display=move || if supports_custom_order { "" } else { "none" }
                >
                    <input
                        type="checkbox"
                        prop:checked=move || use_custom_order.get()
                        on:change=move |e| on_toggle.run(event_target_checked(&e))
                    />
                    {tr(TextKey::UseCustomPlaybackOrder)}
                </label>

                <a
                    class=style::mobile_edit
                    href=format!("/media/collections/{id}/edit")
                    aria-label=tr(TextKey::EditCollection)
                    title=tr(TextKey::EditCollection)
                >
                    <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                </a>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Multi-select bar (mirrors movies.rs selection header — but tuned for
// "Remove From Collection" and "Clear Selection" rather than collection/
// playlist adds).
// ---------------------------------------------------------------------------

#[component]
fn SelectionBar(
    count: Signal<usize>,
    on_remove: Callback<()>,
    on_clear: Callback<()>,
) -> impl IntoView {
    view! {
        <div
            class=style::selection_bar
            style:display=move || if count.get() == 0 { "none" } else { "" }
        >
            <span class=style::selection_count>
                {move || {
                    let n = count.get();
                    if n == 1 {
                        format!("1 {} {}", tr(TextKey::Item), tr(TextKey::Selected))
                    } else {
                        format!("{n} {} {}", tr(TextKey::Items), tr(TextKey::Selected))
                    }
                }}
            </span>
            <button
                class=style::danger_btn
                on:click=move |_| on_remove.run(())
                aria-label=tr(TextKey::RemoveFromCollection)
            >
                <span style=icon_style(IconName::Delete, "1.125rem") aria-hidden="true"></span>
                {tr(TextKey::RemoveFromCollection)}
            </button>
            <button
                class=style::secondary_btn
                on:click=move |_| on_clear.run(())
                aria-label=tr(TextKey::ClearSelection)
            >
                <span style=icon_style(IconName::Check, "1.125rem") aria-hidden="true"></span>
                {tr(TextKey::ClearSelection)}
            </button>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Card grid — grouped by kind, alphabetical inside each group; or
// custom-index ordered when the toggle is on and the collection is
// movies-only.
// ---------------------------------------------------------------------------

const SECTION_ORDER: &[(&str, TextKey)] = &[
    ("movie", TextKey::Movies),
    ("show", TextKey::Shows),
    ("season", TextKey::Seasons),
    ("episode", TextKey::Episodes),
    ("artist", TextKey::Artists),
    ("music_video", TextKey::MusicVideos),
    ("other_video", TextKey::OtherVideos),
    ("song", TextKey::Songs),
    ("image", TextKey::Images),
    ("remote_stream", TextKey::RemoteStreams),
];

#[component]
fn CardGrid(
    items: RwSignal<Vec<CollectionItemCard>>,
    selected: RwSignal<HashSet<i64>>,
    use_custom_order: RwSignal<bool>,
    supports_custom_order: bool,
    on_remove_one: Callback<i64>,
    on_move: Callback<(i64, i32)>,
    on_reorder: Callback<Vec<i64>>,
) -> impl IntoView {
    let dragged_item = RwSignal::new(None::<i64>);

    view! {
        {move || {
            let snapshot = items.get();
            if snapshot.is_empty() {
                return view! {
                    <p class=style::empty>{tr(TextKey::ThisCollectionHasNoItemsYet)}</p>
                }.into_any();
            }

            let use_custom = use_custom_order.get();
            SECTION_ORDER.iter().filter_map(|(kind, label)| {
                let mut section: Vec<CollectionItemCard> = snapshot
                    .iter()
                    .filter(|i| i.kind == *kind)
                    .cloned()
                    .collect();
                if section.is_empty() { return None; }

                if *kind == "movie" && use_custom && supports_custom_order {
                    section.sort_by_key(|i| i.custom_index);
                } else {
                    section.sort_by(|a, b| {
                        a.sort_title
                            .to_lowercase()
                            .cmp(&b.sort_title.to_lowercase())
                    });
                }

                let len = section.len();
                let section_order: Vec<i64> = section.iter().map(|item| item.collection_item_id).collect();
                let kind_is_movie = *kind == "movie";
                let show_move_for_section = supports_custom_order && use_custom && kind_is_movie;
                Some(view! {
                    <h2 class=style::section_title id=*kind>{tr(*label)}</h2>
                    <div class=style::divider/>
                    <div class=style::grid>
                        {section.into_iter().enumerate().map(|(idx, card)| {
                            let can_move_up = idx > 0;
                            let can_move_down = idx + 1 < len;
                            view! {
                                <SelectableCollectionCard
                                    card=card
                                    selected=selected
                                    show_move=show_move_for_section
                                    can_move_up=can_move_up
                                    can_move_down=can_move_down
                                    on_remove_one=on_remove_one
                                    on_move=on_move
                                    on_reorder=on_reorder
                                    dragged_item=dragged_item
                                    section_order=section_order.clone()
                                />
                            }
                        }).collect_view()}
                    </div>
                }.into_any())
            }).collect_view().into_any()
        }}
    }
}

#[component]
fn SelectableCollectionCard(
    card: CollectionItemCard,
    selected: RwSignal<HashSet<i64>>,
    show_move: bool,
    can_move_up: bool,
    can_move_down: bool,
    on_remove_one: Callback<i64>,
    on_move: Callback<(i64, i32)>,
    on_reorder: Callback<Vec<i64>>,
    dragged_item: RwSignal<Option<i64>>,
    section_order: Vec<i64>,
) -> impl IntoView {
    // Row identity: the surrogate CollectionItem.Id. Duplicate same-movie
    // rows (ADR 0048) must select, remove, drag, and reorder independently.
    let item_id = card.collection_item_id;
    let title = card.title.clone();
    let title_for_alt = card.title.clone();
    let subtitle = collection_item_subtitle(use_i18n(), &card.kind, card.subtitle.as_deref());
    let artwork = card.artwork_path.clone();
    let href = card.href.clone();
    let playback_summary = collection_item_playback_summary(&card);

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

    let drop_order = section_order.clone();
    let on_card_drop = move || {
        let Some(source_id) = dragged_item.get_untracked() else {
            return;
        };
        dragged_item.set(None);
        if source_id == item_id {
            return;
        }

        let mut order = drop_order.clone();
        let Some(source_index) = order.iter().position(|id| *id == source_id) else {
            return;
        };
        let Some(target_index) = order.iter().position(|id| *id == item_id) else {
            return;
        };
        let moved = order.remove(source_index);
        order.insert(target_index, moved);
        on_reorder.run(order);
    };

    let card_inner = view! {
        <div class=card_style::card>
            <div class=card_style::poster>
                {match artwork {
                    Some(url) => view! {
                        <img src=url loading="lazy" class=card_style::poster_img alt=title_for_alt/>
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
                {playback_summary.map(|s| view! {
                    <p class=card_style::caption_sub data-playback-override="true">{s}</p>
                })}
            </div>
        </div>
    };

    view! {
        <div
            class=wrap_class
            draggable=if show_move { "true" } else { "false" }
            on:dragstart=move |_| {
                if show_move {
                    dragged_item.set(Some(item_id));
                }
            }
            on:dragend=move |_| dragged_item.set(None)
            on:dragover=move |event| {
                if show_move {
                    event.prevent_default();
                }
            }
            on:drop=move |event| {
                if show_move {
                    event.prevent_default();
                    on_card_drop();
                }
            }
        >
            // Top-left: hover-revealed selection checkbox (red).
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

            // Bottom-right: trashcan (single-item remove).
            <div class=style::card_actions>
                <button
                    class=format!("{} {}", style::card_btn, style::card_btn_danger)
                    aria-label=tr(TextKey::RemoveFromCollection)
                    title=tr(TextKey::RemoveFromCollection)
                    on:click=move |e| {
                        e.stop_propagation();
                        e.prevent_default();
                        on_remove_one.run(item_id);
                    }
                >
                    <span style=icon_style(IconName::Delete, "1rem") aria-hidden="true"></span>
                </button>
            </div>

            // Bottom-left: Move Up / Move Down (only when custom-order is on).
            <div
                class=style::card_actions_left
                style:display=move || if show_move { "" } else { "none" }
            >
                <button
                    class=style::card_btn
                    aria-label=tr(TextKey::MoveUp)
                    title=tr(TextKey::MoveUp)
                    disabled=!can_move_up
                    on:click=move |e| {
                        e.stop_propagation();
                        e.prevent_default();
                        on_move.run((item_id, -1));
                    }
                >
                    <span style=icon_style(IconName::ArrowUp, "1rem") aria-hidden="true"></span>
                </button>
                <button
                    class=style::card_btn
                    aria-label=tr(TextKey::MoveDown)
                    title=tr(TextKey::MoveDown)
                    disabled=!can_move_down
                    on:click=move |e| {
                        e.stop_propagation();
                        e.prevent_default();
                        on_move.run((item_id, 1));
                    }
                >
                    <span style=icon_style(IconName::ArrowDown, "1rem") aria-hidden="true"></span>
                </button>
            </div>

            {match href {
                Some(url) => view! { <a href=url>{card_inner}</a> }.into_any(),
                None => card_inner.into_any(),
            }}
        </div>
    }
}

fn collection_item_subtitle(
    language: UiLanguage,
    kind: &str,
    subtitle: Option<&str>,
) -> Option<String> {
    if kind != "episode" {
        return subtitle.map(str::to_owned);
    }

    subtitle
        .and_then(|value| {
            localize_episode_display_text(language, value).or_else(|| Some(value.to_owned()))
        })
        .or_else(|| Some(text(language, TextKey::Unnumbered).to_owned()))
}

fn collection_item_playback_summary(card: &CollectionItemCard) -> Option<String> {
    let mut parts = Vec::new();
    let audio = card
        .preferred_audio_title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            card.preferred_audio_language_code
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        });
    if let Some(audio) = audio {
        parts.push(format!("{}: {audio}", tr(TextKey::Audio)));
    }

    let subtitle = card
        .preferred_subtitle_language_code
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    if let Some(subtitle) = subtitle {
        if let Some(mode) = card.subtitle_mode.and_then(subtitle_mode_label) {
            parts.push(format!("{}: {subtitle} ({mode})", tr(TextKey::Subtitle)));
        } else {
            parts.push(format!("{}: {subtitle}", tr(TextKey::Subtitle)));
        }
    } else if let Some(mode) = card.subtitle_mode.and_then(subtitle_mode_label) {
        parts.push(format!("{}: {mode}", tr(TextKey::SubtitleMode)));
    }

    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn subtitle_mode_label(mode: i32) -> Option<&'static str> {
    match mode {
        1 => Some(tr(TextKey::Forced)),
        2 => Some(tr(TextKey::DefaultOption)),
        3 => Some(tr(TextKey::Any)),
        _ => None,
    }
}

#[cfg(test)]
mod episode_subtitle_tests {
    use super::collection_item_subtitle;
    use crate::admin::i18n::UiLanguage;

    #[test]
    fn collection_episode_zero_sentinel_uses_the_active_language() {
        assert_eq!(
            collection_item_subtitle(UiLanguage::Dutch, "episode", Some("Serie - S0 E0"),)
                .as_deref(),
            Some("Serie - Ongenummerd")
        );
        assert_eq!(
            collection_item_subtitle(UiLanguage::English, "episode", None).as_deref(),
            Some("Unnumbered")
        );
    }

    #[test]
    fn collection_episode_special_and_non_episode_subtitles_are_preserved() {
        assert_eq!(
            collection_item_subtitle(UiLanguage::English, "episode", Some("Show - S0 E3"),)
                .as_deref(),
            Some("Show - S0 E3")
        );
        assert_eq!(
            collection_item_subtitle(UiLanguage::English, "movie", Some("2026")).as_deref(),
            Some("2026")
        );
    }
}
