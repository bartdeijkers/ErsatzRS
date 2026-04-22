//! Read-only playout timeline visualiser.
#![allow(clippy::too_many_lines)]

#[cfg(not(target_arch = "wasm32"))]
use chrono::{Local, TimeZone, Timelike};
use leptos::prelude::*;

use crate::server::playouts::{list_playout_items, PlayoutTimelineItem};

stylance::import_crate_style!(style, "src/pages/scheduling/timeline.module.css");

const HOUR_MS: i64 = 3_600_000;
const WINDOW_MS: i64 = 86_400_000;
const SVG_WIDTH: f64 = 1000.0;
const SVG_HEIGHT: f64 = 60.0;
const BAR_Y: f64 = 10.0;
const BAR_HEIGHT: f64 = 40.0;

#[derive(Clone, Debug)]
struct TimelineBar {
    item: PlayoutTimelineItem,
    x: f64,
    width: f64,
}

#[component]
fn TimelineInfo(selected_item: RwSignal<Option<PlayoutTimelineItem>>) -> impl IntoView {
    view! {
        <div class=style::info_box>
            {move || {
                selected_item.get().map_or_else(
                    || {
                        view! {
                            <p class=style::info_empty>
                                "Click a playout item to inspect its title, times, and scheduling context."
                            </p>
                        }
                            .into_any()
                    },
                    |item| {
                        let title = if item.title.trim().is_empty() {
                            "Untitled item".to_string()
                        } else {
                            item.title.clone()
                        };
                        let start = format_local_datetime(item.start_ms);
                        let finish = format_local_datetime(item.finish_ms);
                        let context = item
                            .scheduling_context
                            .clone()
                            .filter(|value| !value.trim().is_empty())
                            .unwrap_or_else(|| "No scheduling context".to_string());

                        view! {
                            <>
                                <div class=style::info_row>
                                    <span class=style::info_label>"Title"</span>
                                    <span class=style::info_value>{title}</span>
                                </div>
                                <div class=style::info_row>
                                    <span class=style::info_label>"Start"</span>
                                    <span class=style::info_value>{start}</span>
                                </div>
                                <div class=style::info_row>
                                    <span class=style::info_label>"Finish"</span>
                                    <span class=style::info_value>{finish}</span>
                                </div>
                                <div class=style::info_row>
                                    <span class=style::info_label>"Scheduling Context"</span>
                                    <span class=style::info_context>{context}</span>
                                </div>
                            </>
                        }
                            .into_any()
                    },
                )
            }}
        </div>
    }
}

#[island]
pub fn PlayoutTimelineIsland(playout_id: i64) -> impl IntoView {
    let window_start = RwSignal::new(current_hour_boundary_ms());
    let selected_item: RwSignal<Option<PlayoutTimelineItem>> = RwSignal::new(None);

    let items: Resource<Vec<PlayoutTimelineItem>> = Resource::new(
        move || (playout_id, window_start.get()),
        |(id, from)| async move {
            list_playout_items(id, from, from + WINDOW_MS)
                .await
                .ok()
                .unwrap_or_default()
        },
    );

    Effect::new(move |_| {
        let _ = window_start.get();
        selected_item.set(None);
    });

    let prev_window = move |_| {
        window_start.update(|value| *value -= WINDOW_MS);
    };
    let next_window = move |_| {
        window_start.update(|value| *value += WINDOW_MS);
    };

    view! {
        <div class=style::timeline>
            <div class=style::toolbar>
                <div class=style::window_summary>
                    <span class=style::window_label>"Window"</span>
                    <strong class=style::window_value>
                        {move || format_window_label(window_start.get())}
                    </strong>
                </div>
                <div class=style::nav_buttons>
                    <button class=style::nav_button on:click=prev_window>
                        "Prev 24h"
                    </button>
                    <button class=style::nav_button on:click=next_window>
                        "Next 24h"
                    </button>
                </div>
            </div>

            <Suspense fallback=move || view! { <p class=style::empty>"Loading timeline…"</p> }>
                {move || {
                    items.get().map(|entries| {
                        let bars = timeline_bars(&entries, window_start.get());

                        view! {
                            <>
                                <div class=style::svg_shell>
                                    <svg
                                        class=style::timeline_svg
                                        viewBox="0 0 1000 60"
                                        preserveAspectRatio="none"
                                    >
                                        <rect
                                            x="0"
                                            y="0"
                                            width="1000"
                                            height="60"
                                            class=style::background
                                        />
                                        {hour_ticks(window_start.get())}
                                        {if bars.is_empty() {
                                            view! {
                                                <text x="500" y="34" text-anchor="middle" class=style::empty_label>
                                                    "No playout items in this 24-hour window"
                                                </text>
                                            }
                                                .into_any()
                                        } else {
                                            bars.into_iter().map(|bar| {
                                                let fill = if bar.item.filler_kind == 0 {
                                                    "var(--color-primary)"
                                                } else {
                                                    "var(--color-muted)"
                                                };
                                                let label = if bar.item.title.trim().is_empty() {
                                                    "Untitled item".to_string()
                                                } else {
                                                    bar.item.title.clone()
                                                };
                                                let item_for_click = bar.item.clone();

                                                view! {
                                                    <g>
                                                        <rect
                                                            x=format!("{:.2}", bar.x)
                                                            y=format!("{BAR_Y:.2}")
                                                            width=format!("{:.2}", bar.width)
                                                            height=format!("{BAR_HEIGHT:.2}")
                                                            rx="4"
                                                            ry="4"
                                                            fill=fill
                                                            class=style::bar
                                                            on:click=move |_| {
                                                                selected_item.set(Some(item_for_click.clone()));
                                                            }
                                                        />
                                                        <title>{label}</title>
                                                    </g>
                                                }
                                            }).collect_view().into_any()
                                        }}
                                    </svg>
                                </div>

                                <TimelineInfo selected_item/>
                            </>
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

fn timeline_bars(items: &[PlayoutTimelineItem], window_start: i64) -> Vec<TimelineBar> {
    let window_finish = window_start + WINDOW_MS;
    let window_ms = i32::try_from(WINDOW_MS).unwrap_or(86_400_000);

    items
        .iter()
        .filter_map(|item| {
            let visible_start = item.start_ms.max(window_start);
            let visible_finish = item.finish_ms.min(window_finish);
            if visible_finish <= visible_start {
                return None;
            }

            let start_offset_ms = i32::try_from(visible_start - window_start).ok()?;
            let finish_offset_ms = i32::try_from(visible_finish - window_start).ok()?;
            let x = (f64::from(start_offset_ms) / f64::from(window_ms)) * SVG_WIDTH;
            let finish_x = (f64::from(finish_offset_ms) / f64::from(window_ms)) * SVG_WIDTH;
            let clamped_x = x.clamp(0.0, SVG_WIDTH);
            let width = (finish_x - clamped_x).clamp(1.0, SVG_WIDTH - clamped_x);

            Some(TimelineBar {
                item: item.clone(),
                x: clamped_x,
                width,
            })
        })
        .collect()
}

fn hour_ticks(window_start: i64) -> impl IntoView {
    (0..24)
        .map(|offset| {
            let x = f64::from(offset) / 24.0 * SVG_WIDTH;
            let tick_ms = window_start + i64::from(offset) * HOUR_MS;
            let show_label = offset == 0 || offset % 6 == 0;

            view! {
                <g>
                    <line
                        x1=format!("{x:.2}")
                        y1="0"
                        x2=format!("{x:.2}")
                        y2=format!("{SVG_HEIGHT:.2}")
                        class=style::tick_line
                    />
                    {if show_label {
                        view! {
                            <text
                                x=format!("{:.2}", (x + 6.0).min(SVG_WIDTH - 30.0))
                                y="9"
                                class=style::tick_label
                            >
                                {format_hour_label(tick_ms)}
                            </text>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }}
                </g>
            }
        })
        .collect_view()
}

fn format_window_label(window_start: i64) -> String {
    let finish = window_start + WINDOW_MS;
    format!(
        "{} to {}",
        format_local_datetime(window_start),
        format_local_datetime(finish)
    )
}

fn format_hour_label(epoch_ms: i64) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(epoch_ms as f64));
        let hours = date.get_hours();
        let minutes = date.get_minutes();
        format!("{hours:02}:{minutes:02}")
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Local
            .timestamp_millis_opt(epoch_ms)
            .single()
            .map(|value| value.format("%H:%M").to_string())
            .unwrap_or_default()
    }
}

fn format_local_datetime(epoch_ms: i64) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(epoch_ms as f64));
        date.to_locale_string("default", &wasm_bindgen::JsValue::UNDEFINED)
            .into()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Local
            .timestamp_millis_opt(epoch_ms)
            .single()
            .map(|value| value.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default()
    }
}

fn current_hour_boundary_ms() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        let now = js_sys::Date::new_0();
        let millis = now.get_time() as i64;
        let minutes = i64::from(now.get_minutes());
        let seconds = i64::from(now.get_seconds());
        let ms = i64::from(now.get_milliseconds());
        millis - minutes * 60_000 - seconds * 1_000 - ms
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let now = Local::now();
        let floored = now
            .with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0));

        floored.map_or(0, |value| value.timestamp_millis())
    }
}
