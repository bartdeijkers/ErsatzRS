use leptos::prelude::*;

stylance::import_crate_style!(style, "src/ui/primitives/card.module.css");

/// A media item card for browser grids (maps to ErsatzTV's `MediaCard` component).
#[allow(dead_code)]
#[component]
pub fn MediaCard(
    title: String,
    #[prop(default = None)] subtitle: Option<String>,
    #[prop(default = None)] artwork_url: Option<String>,
    #[prop(default = None)] href: Option<String>,
    #[prop(default = None)] selected: Option<RwSignal<bool>>,
) -> impl IntoView {
    let inner = view! {
        <div class=style::card>
            <div class=style::poster>
                {match artwork_url {
                    Some(url) => view! {
                        <img src=url loading="lazy" class=style::poster_img/>
                    }.into_any(),
                    None => view! {
                        <div class=style::poster_placeholder>"🎬"</div>
                    }.into_any(),
                }}
            </div>
            <div class=style::caption>
                <p class=style::caption_title>{title}</p>
                {subtitle.map(|s| view! {
                    <p class=style::caption_sub>{s}</p>
                })}
            </div>
            {selected.map(|sel| view! {
                <div class=style::selection on:click=|e| e.stop_propagation()>
                    <input
                        type="checkbox"
                        prop:checked=move || sel.get()
                        on:change=move |e| sel.set(event_target_checked(&e))
                    />
                </div>
            })}
        </div>
    };

    match href {
        Some(url) => view! { <a href=url>{inner}</a> }.into_any(),
        None => inner.into_any(),
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
