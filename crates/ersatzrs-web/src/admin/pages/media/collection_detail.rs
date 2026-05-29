//! Collection detail page — port of `ErsatzTV/Pages/CollectionItems.razor`
//! (`/media/collections/:id`). Shows the manual collection's items as a
//! poster grid grouped by media kind, with inline rename, multi-select
//! removal, per-item trashcan, and the "Use Custom Playback Order" toggle.
//!
//! Hydration shape (mirrors the proven movies-page pattern in
//! `crates/ersatzrs-web/src/admin/pages/media/movies.rs`):
//!
//! * The page wrapper is a plain `#[component]` (`CollectionDetailPage`).
//! * The interactive surface lives in [`CollectionDetailIsland`], which
//!   takes only a scalar `id: i64` prop, owns a `Resource::new_blocking`
//!   for the detail payload, and renders `<Suspense>` internally. This
//!   honours the `TEST.md` rule "no prop-bearing islands across Suspense".
//! * All sub-components (`Header`, `SelectionBar`, `CardGrid`,
//!   `SelectableCollectionCard`) are plain `#[component]`s reading
//!   `RwSignal`s created inside the island, so nothing nested is itself
//!   an island.
//!
//! Deferred from the C# behaviour:
//!
//! * **Drag-and-drop reordering** — `ErsatzTV` uses a `SortableJS` JS
//!   interop (`sortableCollection` / `enableSorting`). Replaced for now by
//!   **Move Up / Move Down** buttons exposed on every card when the
//!   "Use Custom Playback Order" toggle is ON; both call
//!   `update_collection_custom_order`. Drag-and-drop is tracked as TODO.
//! * **Per-card overflow menu** — `ErsatzTV`'s `MediaCard` exposes a menu
//!   per item kind; the only manual-collection-specific entry is the
//!   trashcan (already covered) and an "Add To Schedule" item that the
//!   `ErsatzRS` UI does not yet provide. Tracked as TODO.
#![allow(clippy::too_many_lines)]

use std::collections::HashSet;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::media_browser::{
    get_collection_detail, remove_items_from_collection, save_collection,
    set_collection_use_custom_order, update_collection_custom_order, CollectionDetail,
    CollectionForm, CollectionItemCard,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

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

    view! { <CollectionDetailIsland id/> }.into_any()
}

// ---------------------------------------------------------------------------
// Island: owns the resource, the rename state, the toggle state, the
// selection set, and dispatches all server-fn actions. Children are plain
// components reading island-owned signals.
// ---------------------------------------------------------------------------

#[island]
fn CollectionDetailIsland(id: i64) -> impl IntoView {
    // Refresh tick — bumped after every successful mutation so the resource
    // re-runs and the grid reflects the new state.
    let refresh: RwSignal<u32> = RwSignal::new(0);
    let data: Resource<Option<CollectionDetail>> = Resource::new_blocking(
        move || (id, refresh.get()),
        |(id, _)| async move { get_collection_detail(id).await.ok().flatten() },
    );

    view! {
        <div class=style::page>
            <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                {move || match data.get() {
                    Some(Some(detail)) => {
                        view! { <CollectionDetailView id detail refresh=refresh/> }.into_any()
                    }
                    Some(None) => view! {
                        <div class=style::not_found>
                            <h1>{tr(TextKey::CollectionNotFound)}</h1>
                            <a href="/media/collections">{tr(TextKey::BackToCollections)}</a>
                        </div>
                    }
                    .into_any(),
                    None => view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }.into_any(),
                }}
            </Suspense>
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
    // ── Header rename state ────────────────────────────────────────────
    let editing_name = RwSignal::new(false);
    let name = RwSignal::new(detail.name.clone());
    let header_name = RwSignal::new(detail.name.clone());
    let name_error: RwSignal<Option<String>> = RwSignal::new(None);

    // ── Custom-order toggle ────────────────────────────────────────────
    let use_custom_order = RwSignal::new(detail.use_custom_playback_order);

    // ── Selection state (HashSet<media_item_id>) ───────────────────────
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
    let rename_action = Action::new(move |new_name: &String| {
        let new_name = new_name.clone();
        async move {
            save_collection(CollectionForm {
                id,
                name: new_name,
                query: String::new(),
            })
            .await
        }
    });

    Effect::new(move |_| {
        if let Some(res) = rename_action.value().get() {
            match res {
                Ok(_) => {
                    let new_name = name.get_untracked();
                    header_name.set(new_name);
                    editing_name.set(false);
                    name_error.set(None);
                    toast.push(tr(TextKey::CollectionRenamed), ToastSeverity::Success);
                    refresh.update(|n| *n += 1);
                }
                Err(e) => {
                    name_error.set(Some(format!("{e}")));
                    toast.push(
                        format!("{}: {e}", tr(TextKey::RenameFailed)),
                        ToastSeverity::Error,
                    );
                }
            }
        }
    });

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
                        format!("{}: {e}", tr(TextKey::PlaybackOrderUpdateFailed)),
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
                    format!("{}: {e}", tr(TextKey::RemoveFailed)),
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
                format!("{}: {e}", tr(TextKey::ReorderFailed)),
                ToastSeverity::Error,
            );
            refresh.update(|n| *n += 1);
        }
    });

    // ── Callbacks ───────────────────────────────────────────────────────
    let on_save_name = Callback::new(move |()| {
        let trimmed = name.get_untracked().trim().to_string();
        if trimmed.is_empty() {
            name_error.set(Some(tr(TextKey::CollectionNameRequired).into()));
            return;
        }
        name_error.set(None);
        rename_action.dispatch(trimmed);
    });

    let on_cancel_rename = Callback::new(move |()| {
        name.set(header_name.get_untracked());
        editing_name.set(false);
        name_error.set(None);
    });

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
    let on_move = Callback::new(move |(item_id, direction): (i64, i32)| {
        items_signal.update(|items| {
            let Some(pos) = items.iter().position(|i| i.media_item_id == item_id) else {
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
                .map(|i| i.media_item_id)
                .collect()
        });
        reorder_action.dispatch(order);
    });

    let item_count = detail.items.len();
    let movie_count = detail.items.iter().filter(|i| i.kind == "movie").count();

    view! {
        // ── Header (rename + summary + custom-order toggle) ────────────
        <Header
            id=id
            name=name
            header_name=header_name
            editing=editing_name
            name_error=Signal::from(name_error)
            on_save=on_save_name
            on_cancel=on_cancel_rename
            item_count=item_count
            movie_count=movie_count
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
        <CardGrid
            items=items_signal
            selected=selected
            use_custom_order=use_custom_order
            supports_custom_order=supports_custom_order
            on_remove_one=on_remove_one
            on_move=on_move
        />
    }
}

// ---------------------------------------------------------------------------
// Header (rename pencil + counts + custom-order switch).
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
#[component]
fn Header(
    id: i64,
    name: RwSignal<String>,
    header_name: RwSignal<String>,
    editing: RwSignal<bool>,
    name_error: Signal<Option<String>>,
    on_save: Callback<()>,
    on_cancel: Callback<()>,
    item_count: usize,
    movie_count: usize,
    supports_custom_order: bool,
    use_custom_order: RwSignal<bool>,
    on_toggle: Callback<bool>,
) -> impl IntoView {
    let summary_label = if item_count == 1 {
        format!("1 {}", tr(TextKey::Item))
    } else if movie_count == item_count && item_count > 0 {
        if item_count == 1 {
            format!("1 {}", tr(TextKey::Movie))
        } else {
            format!("{movie_count} {}", tr(TextKey::Movies))
        }
    } else {
        format!("{item_count} {}", tr(TextKey::Items))
    };

    let back_href = "/media/collections".to_string();
    let _ = id; // referenced only for future deep-link variants

    view! {
        <div class=style::header>
            // Static name display (when not editing)
            <h1
                class=style::title
                style:display=move || if editing.get() { "none" } else { "" }
            >
                {move || header_name.get()}
            </h1>

            // Editable input (when editing) — always rendered, visibility toggled
            <input
                type="text"
                class=style::title_input
                prop:value=move || name.get()
                on:input=move |e| name.set(event_target_value(&e))
                on:keydown=move |e| {
                    if e.key() == "Enter" {
                        on_save.run(());
                    } else if e.key() == "Escape" {
                        on_cancel.run(());
                    }
                }
                style:display=move || if editing.get() { "" } else { "none" }
            />

            // Pencil / Save / Cancel buttons.
            <button
                class=style::icon_btn
                aria-label=tr(TextKey::Rename)
                title=tr(TextKey::Rename)
                style:display=move || if editing.get() { "none" } else { "" }
                on:click=move |_| {
                    name.set(header_name.get_untracked());
                    editing.set(true);
                }
            >
                "✎"
            </button>
            <button
                class=style::icon_btn
                aria-label=tr(TextKey::Save)
                title=tr(TextKey::Save)
                style:display=move || if editing.get() { "" } else { "none" }
                on:click=move |_| on_save.run(())
            >
                "✓"
            </button>
            <button
                class=style::icon_btn
                aria-label=tr(TextKey::Cancel)
                title=tr(TextKey::Cancel)
                style:display=move || if editing.get() { "" } else { "none" }
                on:click=move |_| on_cancel.run(())
            >
                "✕"
            </button>

            <span class=style::summary>{summary_label}</span>

            // Inline error — always rendered, visibility toggled.
            <span
                class=style::error
                style:display=move || if name_error.get().is_some() { "" } else { "none" }
            >
                {move || name_error.get().unwrap_or_default()}
            </span>

            <span class=style::spacer/>

            // Custom-order toggle (only visible when supported).
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

            <a class=style::summary href=back_href>{tr(TextKey::BackToCollections)}</a>
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
                "− " {tr(TextKey::RemoveFromCollection)}
            </button>
            <button
                class=style::secondary_btn
                on:click=move |_| on_clear.run(())
                aria-label=tr(TextKey::ClearSelection)
            >
                "✓ " {tr(TextKey::ClearSelection)}
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
) -> impl IntoView {
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
) -> impl IntoView {
    let item_id = card.media_item_id;
    let title = card.title.clone();
    let title_for_alt = card.title.clone();
    let subtitle = card.subtitle.clone();
    let artwork = card.artwork_path.clone();
    let href = card.href.clone();

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
            </div>
        </div>
    };

    view! {
        <div class=wrap_class>
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
                    "🗑"
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
                    "↑"
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
                    "↓"
                </button>
            </div>

            {match href {
                Some(url) => view! { <a href=url>{card_inner}</a> }.into_any(),
                None => card_inner.into_any(),
            }}
        </div>
    }
}
