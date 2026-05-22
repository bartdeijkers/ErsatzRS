use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::admin::ui::primitives::icon::{icon_style, IconName};

pub use ersatzrs_domain::MediaItemState;

/// Returns the placeholder string used by `MediaCard` when no artwork is
/// available. Mirrors `MediaCard.razor`'s `GetPlaceholder`: uppercase first
/// letter of `sort_title` for ASCII letters; `#` for digits / other characters
/// / empty.
#[must_use]
pub fn first_letter_placeholder(sort_title: &str) -> String {
    let Some(first) = sort_title.chars().next() else {
        return "#".to_string();
    };
    if first.is_ascii_alphabetic() {
        first.to_ascii_uppercase().to_string()
    } else {
        "#".to_string()
    }
}

// `pager_btn_disabled` is used by external consumers (e.g. the movies page
// re-imports this stylesheet), not by `MediaCardPager` itself.
#[allow(dead_code)]
mod style_mod {
    stylance::import_crate_style!(s, "src/admin/ui/primitives/card.module.css");
    pub use s::*;
}
use style_mod as style;

/// A media item card for browser grids (maps to ErsatzTV's `MediaCard` component).
///
/// `placeholder` is the string shown when no artwork is available (first letter
/// of `sort_title`, `#`, or season number — see `MediaCard.razor`'s
/// `GetPlaceholder`). `state` adds a corner warning icon for
/// `FileNotFound` (red) and `Unavailable` (amber).
///
/// When `select_mode` is true, the whole card surface acts as a select toggle
/// (the `on_select` callback receives the click event so callers can read
/// `event.shift_key()` for range select). When false and `href` is set, the
/// card navigates. The `MoreVert` per-card menu renders only when at least one
/// of the `on_add_to_collection` / `on_add_to_playlist` / `on_show_media_info`
/// callbacks is set and `select_mode` is false.
#[allow(
    dead_code,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::needless_pass_by_value
)]
#[component]
pub fn MediaCard(
    title: String,
    #[prop(default = None)] subtitle: Option<String>,
    #[prop(default = None)] artwork_url: Option<String>,
    #[prop(default = None)] href: Option<String>,
    #[prop(into, default = String::new())] placeholder: String,
    #[prop(default = MediaItemState::Normal)] state: MediaItemState,
    #[prop(default = Signal::derive(|| false))] select_mode: Signal<bool>,
    #[prop(default = None)] selected: Option<RwSignal<bool>>,
    #[prop(default = None)] on_select: Option<Callback<MouseEvent>>,
    #[prop(default = None)] on_add_to_collection: Option<Callback<()>>,
    #[prop(default = None)] on_add_to_playlist: Option<Callback<()>>,
    #[prop(default = None)] on_show_media_info: Option<Callback<()>>,
    #[prop(default = None)] menu_media_item_id: Option<i64>,
) -> impl IntoView {
    let has_menu = on_add_to_collection.is_some()
        || on_add_to_playlist.is_some()
        || on_show_media_info.is_some();

    // Match `GetPlaceholder` in MediaCard.razor — letter count drives typography.
    let placeholder_size_class = match placeholder.chars().count() {
        n if n > 12 => style::placeholder_h3,
        n if n > 2 => style::placeholder_h2,
        _ => style::placeholder_h1,
    };

    let poster_view = {
        let placeholder = placeholder.clone();
        let title_alt = title.clone();
        move || match artwork_url.clone() {
            Some(url) => view! {
                <img src=url loading="lazy" class=style::poster_img alt=title_alt.clone()/>
            }
            .into_any(),
            None => view! {
                <div class=format!("{} {}", style::poster_placeholder, placeholder_size_class)>
                    {placeholder.clone()}
                </div>
            }
            .into_any(),
        }
    };

    let state_overlay = match state {
        MediaItemState::FileNotFound => Some(
            view! {
                <div class=format!("{} {}", style::state_overlay, style::state_overlay_error)
                     aria-label="File not found">
                    <span style=icon_style(IconName::Warning, "1.25rem") aria-hidden="true"></span>
                </div>
            }
            .into_any(),
        ),
        MediaItemState::Unavailable => Some(
            view! {
                <div class=format!("{} {}", style::state_overlay, style::state_overlay_warning)
                     aria-label="Unavailable">
                    <span style=icon_style(IconName::Warning, "1.25rem") aria-hidden="true"></span>
                </div>
            }
            .into_any(),
        ),
        MediaItemState::Normal | MediaItemState::RemoteOnly => None,
    };

    let selection_check = selected.map(|sel| {
        view! {
            <div class=style::selection on:click=|e: MouseEvent| e.stop_propagation()>
                <input
                    type="checkbox"
                    prop:checked=move || sel.get()
                    on:change=move |e| sel.set(event_target_checked(&e))
                />
            </div>
        }
    });

    // Card surface click: select-mode → on_select; otherwise the transparent
    // navigation overlay owns href clicks. When in select-mode but the caller
    // provided no on_select, the click is a no-op.
    let on_card_click = move |e: MouseEvent| {
        if select_mode.get() {
            e.prevent_default();
            e.stop_propagation();
            if let Some(cb) = on_select {
                cb.run(e);
            }
        }
    };

    let nav_overlay = href.map(|url| {
        let title = title.clone();
        view! {
            <a
                class=style::card_link_overlay
                href=url
                aria-label=title
                on:click=move |e: MouseEvent| {
                    if select_mode.get() {
                        e.prevent_default();
                    }
                }
            />
        }
    });
    let menu_media_item_id = menu_media_item_id
        .map(|id| id.to_string())
        .unwrap_or_default();

    view! {
        <div class=style::card on:click=on_card_click>
            {nav_overlay}
            <div class=style::poster>
                {poster_view()}
                {has_menu.then(|| view! {
                    <details
                        class=style::item_menu
                        on:click=|e: MouseEvent| e.stop_propagation()
                        style:display=move || if select_mode.get() { "none" } else { "" }
                    >
                        <summary
                            class=style::item_menu_trigger
                            title="Item actions"
                            aria-label="Item actions"
                            on:click=move |e: MouseEvent| {
                                e.stop_propagation();
                            }
                        >
                            <span style=icon_style(IconName::MoreVert, "1.25rem") aria-hidden="true"></span>
                        </summary>
                        <div
                            class=style::item_menu_popover
                            role="menu"
                        >
                            {on_add_to_collection.map(|cb| view! {
                                <button
                                    type="button"
                                    class=style::item_menu_entry
                                    role="menuitem"
                                    data-search-card-action="collection"
                                    data-media-item-id=menu_media_item_id.clone()
                                    on:click=move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        cb.run(());
                                    }
                                >
                                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                                    <span>"Add To Collection"</span>
                                </button>
                            })}
                            {on_add_to_playlist.map(|cb| view! {
                                <button
                                    type="button"
                                    class=style::item_menu_entry
                                    role="menuitem"
                                    data-search-card-action="playlist"
                                    data-media-item-id=menu_media_item_id.clone()
                                    on:click=move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        cb.run(());
                                    }
                                >
                                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                                    <span>"Add To Playlist"</span>
                                </button>
                            })}
                            {on_show_media_info.map(|cb| view! {
                                <button
                                    type="button"
                                    class=style::item_menu_entry
                                    role="menuitem"
                                    on:click=move |e: MouseEvent| {
                                        e.prevent_default();
                                        e.stop_propagation();
                                        cb.run(());
                                    }
                                >
                                    <span style=icon_style(IconName::Info, "1.125rem") aria-hidden="true"></span>
                                    <span>"Show Media Info"</span>
                                </button>
                            })}
                        </div>
                    </details>
                })}
                {state_overlay}
            </div>
            <div class=style::caption>
                <p class=style::caption_title>{title}</p>
                {subtitle.map(|s| view! { <p class=style::caption_sub>{s}</p> })}
            </div>
            {selection_check}
        </div>
    }
}

/// Pagination + multi-select toolbar above/below a media card grid.
#[allow(dead_code)]
#[component]
pub fn MediaCardPager(
    total: Signal<Option<u64>>,
    page: RwSignal<u32>,
    page_size: u32,
    #[prop(default = None)] selected_count: Option<Signal<usize>>,
    #[prop(default = None)] on_add_to_collection: Option<Callback<()>>,
    #[prop(default = None)] on_clear_selection: Option<Callback<()>>,
) -> impl IntoView {
    let go_prev = move |_| {
        page.update(|p| {
            if *p > 0 {
                *p -= 1;
            }
        });
    };
    let go_next = move |_| {
        page.update(|p| {
            let max_page = total.get().unwrap_or(0).saturating_sub(1) / u64::from(page_size);
            if u64::from(*p) < max_page {
                *p += 1;
            }
        });
    };

    view! {
        <div class=style::pager>
            <div class=style::pager_left>
                {selected_count.map(|cnt| view! {
                    <Show when=move || cnt.get() != 0>
                        <span class=style::selected_label>
                            {move || format!("{} selected", cnt.get())}
                        </span>
                        {on_add_to_collection.map(|cb| view! {
                            <button class=style::add_btn on:click=move |_| cb.run(())>
                                "Add to Collection"
                            </button>
                        })}
                        {on_clear_selection.map(|cb| view! {
                            <button class=style::clear_btn on:click=move |_| cb.run(())>
                                "Clear"
                            </button>
                        })}
                    </Show>
                })}
            </div>
            <div class=style::pager_right>
                <Suspense fallback=move || view! {
                    <span>"Loading..."</span>
                    <button class=style::pager_btn disabled=true>"‹"</button>
                    <button class=style::pager_btn disabled=true>"›"</button>
                }>
                    {move || {
                        total.get().map(|total_items| {
                            view! {
                                <span>
                                    {move || {
                                        let p = page.get();
                                        let start = p * page_size + 1;
                                        let end = ((p + 1) * page_size)
                                            .min(u32::try_from(total_items).unwrap_or(u32::MAX));
                                        if total_items == 0 {
                                            "0 items".to_string()
                                        } else {
                                            format!("{start}–{end} of {total_items}")
                                        }
                                    }}
                                </span>
                                <button
                                    class=style::pager_btn
                                    disabled=move || page.get() == 0
                                    on:click=go_prev
                                >
                                    "‹"
                                </button>
                                <button
                                    class=style::pager_btn
                                    disabled=move || {
                                        total_items == 0
                                            || u64::from((page.get() + 1) * page_size) >= total_items
                                    }
                                    on:click=go_next
                                >
                                    "›"
                                </button>
                            }
                        })
                    }}
                </Suspense>
            </div>
        </div>
    }
}
