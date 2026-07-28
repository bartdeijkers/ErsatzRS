use chrono::Timelike;
use leptos::html;
use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::channels::ChannelWatchProgramme;

use super::preview::{programme_time, programme_title};

stylance::import_crate_style!(style, "src/admin/pages/channels/guide.module.css");

const BASE_PIXELS_PER_MINUTE: i64 = 6;
const DEFAULT_VIEWPORT_MINUTES: i64 = 3 * 60;
const DEFAULT_ZOOM_PERCENT: i64 = 100;
const CHANNEL_COLUMN_PIXELS: i64 = 352;

#[derive(Clone, Debug, Default)]
pub struct GuideChannel {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub logo_url: String,
    pub is_active: bool,
    pub programmes: Vec<ChannelWatchProgramme>,
}

#[derive(Clone, Debug, Default)]
pub struct GuideSnapshot {
    pub generated_at: String,
    pub window_start: String,
    pub window_finish: String,
    pub channels: Vec<GuideChannel>,
}

#[component]
#[allow(clippy::too_many_lines)]
pub fn EpgGuideGrid(snapshot: RwSignal<GuideSnapshot>) -> impl IntoView {
    let scroll_ref = NodeRef::<html::Div>::new();
    let visible_start_minutes = RwSignal::new(0_i64);
    let zoom_percent = RwSignal::new(DEFAULT_ZOOM_PERCENT);
    let positioned = RwSignal::new(false);

    Effect::new(move |_| {
        let default = snapshot.with(default_visible_start_minutes);
        if !positioned.get() && snapshot.with(|data| !data.window_start.is_empty()) {
            positioned.set(true);
            visible_start_minutes.set(default);
            scroll_to_minutes(scroll_ref, default, zoom_percent.get_untracked());
        }
    });

    let move_viewport = move |delta: i64| {
        let max = snapshot.with(max_visible_start_minutes);
        let next = (visible_start_minutes.get_untracked() + delta).clamp(0, max);
        visible_start_minutes.set(next);
        scroll_to_minutes(scroll_ref, next, zoom_percent.get_untracked());
    };
    let move_now = move || {
        let next = snapshot.with(default_visible_start_minutes);
        visible_start_minutes.set(next);
        scroll_to_minutes(scroll_ref, next, zoom_percent.get_untracked());
    };

    view! {
        <section class=style::guide data-epg-grid>
            <div class=style::controls>
                <span class=style::visible_label>{tr(TextKey::EpgZoom)}</span>
                <input
                    class=style::scrubber
                    type="range"
                    min="50"
                    max="200"
                    step="10"
                    prop:value=move || zoom_percent.get()
                    aria-label=tr(TextKey::EpgZoom)
                    on:input=move |event| {
                        if let Ok(value) = event_target_value(&event).parse::<i64>() {
                            update_zoom(scroll_ref, zoom_percent, value.clamp(50, 200));
                        }
                    }
                />
                <span class=style::visible_time data-zoom-value>
                    {move || format!("{}%", zoom_percent.get())}
                </span>
                <div class=style::step_controls>
                    <button type="button" on:click=move |_| move_viewport(-120)>"-2h"</button>
                    <button type="button" on:click=move |_| move_viewport(-60)>"-1h"</button>
                    <button class=style::now_button type="button" on:click=move |_| move_now()>
                        {tr(TextKey::EpgNow)}
                    </button>
                    <button type="button" on:click=move |_| move_viewport(120)>"+2h"</button>
                    <button type="button" on:click=move |_| move_viewport(240)>"+4h"</button>
                </div>
            </div>

            <div
                node_ref=scroll_ref
                class=style::scroll
                role="grid"
                tabindex="0"
                aria-label=tr(TextKey::ProgrammeGuide)
                on:scroll=move |_| {
                    update_visible_start(
                        scroll_ref,
                        visible_start_minutes,
                        snapshot,
                        zoom_percent.get_untracked(),
                    );
                }
                on:keydown=move |event| {
                    match event.key().as_str() {
                        "ArrowLeft" => move_viewport(-15),
                        "ArrowRight" => move_viewport(15),
                        "PageUp" => move_viewport(-DEFAULT_VIEWPORT_MINUTES),
                        "PageDown" => move_viewport(DEFAULT_VIEWPORT_MINUTES),
                        "Home" => move_viewport(-i64::MAX),
                        "End" => move_viewport(i64::MAX),
                        _ => {}
                    }
                }
            >
                <div
                    class=style::track
                    style=move || format!(
                        "width:{}px",
                        CHANNEL_COLUMN_PIXELS
                            + scaled_minutes(snapshot.with(window_minutes), zoom_percent.get())
                    )
                >
                    <div class=style::ruler role="row">
                        <div class=style::corner role="columnheader">{tr(TextKey::Channels)}</div>
                        <div
                            class=style::time_track
                            data-epg-time-track
                            style=move || format!(
                                "--quarter-hour-width:{}px;width:{}px",
                                scaled_minutes(15, zoom_percent.get()),
                                scaled_minutes(snapshot.with(window_minutes), zoom_percent.get())
                            )
                        >
                            {move || snapshot.with(|data| hour_ticks(data, zoom_percent.get()))
                                .into_iter().map(|(left, label)| {
                                view! {
                                    <span class=style::hour_tick style=format!("left:{left}px")>
                                        {label}
                                    </span>
                                }
                            }).collect_view()}
                            <span
                                class=style::now_line
                                style=move || format!(
                                    "left:{}px",
                                    snapshot.with(|data| {
                                        now_line_left_pixels(data, zoom_percent.get())
                                    })
                                )
                                aria-hidden="true"
                            ></span>
                        </div>
                    </div>

                    <div
                        class=style::empty
                        style:display=move || {
                            if snapshot.with(|data| data.channels.is_empty()) { "" } else { "none" }
                        }
                    >
                        {tr(TextKey::NoEpgChannels)}
                    </div>

                    {move || snapshot.with(|data| data.channels.clone()).into_iter().enumerate().map(
                        |(palette, channel)| {
                            let channel_href = format!("/channels/{}/watch", channel.id);
                            let channel_label = format!("{} · {}", channel.number, channel.name);
                            let programmes = channel.programmes.clone();
                            view! {
                                <div class=style::lane role="row" data-channel-id=channel.id>
                                    <a
                                        class=style::channel
                                        href=channel_href.clone()
                                        role="rowheader"
                                        aria-label=channel_label.clone()
                                    >
                                        <img src=channel.logo_url alt=""/>
                                        <span class=style::channel_identity>
                                            <strong>{channel_label.clone()}</strong>
                                            <small
                                                class=style::active
                                                style:display={if channel.is_active { "" } else { "none" }}
                                            >
                                                {tr(TextKey::ContentVariantLive)}
                                            </small>
                                        </span>
                                    </a>
                                    <div
                                        class=style::lane_track
                                        style=move || {
                                            let zoom = zoom_percent.get();
                                            format!(
                                                "--quarter-hour-width:{}px;width:{}px",
                                                scaled_minutes(15, zoom),
                                                scaled_minutes(snapshot.with(window_minutes), zoom),
                                            )
                                        }
                                    >
                                        {programmes.into_iter().map(|programme| {
                                            let geometry_programme =
                                                StoredValue::new(programme.clone());
                                            let title = programme_title(&programme);
                                            let time = format!(
                                                "{}–{}",
                                                programme_time(&programme.start),
                                                programme_time(&programme.finish)
                                            );
                                            let label = filler_label(programme.filler_kind);
                                            let programme_href = channel_href.clone();
                                            view! {
                                                <a
                                                    class=style::programme
                                                    href=programme_href
                                                    role="gridcell"
                                                    data-gap=programme.is_gap
                                                    data-filler-kind=programme.filler_kind
                                                    data-palette=palette % 6
                                                    style=move || {
                                                        let geometry = snapshot.with(|data| {
                                                            geometry_programme.with_value(|programme| {
                                                                programme_geometry(
                                                                    data,
                                                                    programme,
                                                                    zoom_percent.get(),
                                                                )
                                                            })
                                                        });
                                                        format!(
                                                            "left:{}px;width:{}px",
                                                            geometry.0,
                                                            geometry.1
                                                        )
                                                    }
                                                    title=format!("{title} · {time}")
                                                >
                                                    <span
                                                        class=style::filler_label
                                                        style:display=move || {
                                                            let width = snapshot.with(|data| {
                                                                geometry_programme.with_value(|programme| {
                                                                    programme_geometry(
                                                                        data,
                                                                        programme,
                                                                        zoom_percent.get(),
                                                                    ).1
                                                                })
                                                            });
                                                            if !label.is_empty() && width >= 48 {
                                                                ""
                                                            } else {
                                                                "none"
                                                            }
                                                        }
                                                    >
                                                        {label}
                                                    </span>
                                                    <strong>{title.clone()}</strong>
                                                    <small>{time.clone()}</small>
                                                </a>
                                            }
                                        }).collect_view()}
                                        <span
                                            class=style::now_line
                                            style=move || format!(
                                                "left:{}px",
                                                snapshot.with(|data| {
                                                    now_line_left_pixels(data, zoom_percent.get())
                                                })
                                            )
                                            aria-hidden="true"
                                        ></span>
                                    </div>
                                </div>
                            }
                        }
                    ).collect_view()}
                </div>
            </div>
        </section>
    }
}

fn parse_time(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(value).ok()
}

fn window_minutes(snapshot: &GuideSnapshot) -> i64 {
    let (Some(start), Some(finish)) = (
        parse_time(&snapshot.window_start),
        parse_time(&snapshot.window_finish),
    ) else {
        return 26 * 60;
    };
    (finish - start).num_minutes().max(DEFAULT_VIEWPORT_MINUTES)
}

fn max_visible_start_minutes(snapshot: &GuideSnapshot) -> i64 {
    window_minutes(snapshot).saturating_sub(DEFAULT_VIEWPORT_MINUTES)
}

fn default_visible_start_minutes(snapshot: &GuideSnapshot) -> i64 {
    let (Some(start), Some(now)) = (
        parse_time(&snapshot.window_start),
        parse_time(&snapshot.generated_at),
    ) else {
        return 60;
    };
    ((now - start).num_minutes() - 60).clamp(0, max_visible_start_minutes(snapshot))
}

#[cfg(test)]
fn visible_time_in_timezone<Tz>(
    snapshot: &GuideSnapshot,
    offset_minutes: i64,
    timezone: &Tz,
) -> String
where
    Tz: chrono::TimeZone,
    Tz::Offset: std::fmt::Display,
{
    parse_time(&snapshot.window_start).map_or_else(String::new, |start| {
        (start + chrono::Duration::minutes(offset_minutes))
            .with_timezone(timezone)
            .format("%H:%M")
            .to_string()
    })
}

fn hour_ticks(snapshot: &GuideSnapshot, zoom_percent: i64) -> Vec<(i64, String)> {
    hour_ticks_in_timezone(snapshot, zoom_percent, &chrono::Local)
}

fn hour_ticks_in_timezone<Tz>(
    snapshot: &GuideSnapshot,
    zoom_percent: i64,
    timezone: &Tz,
) -> Vec<(i64, String)>
where
    Tz: chrono::TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let Some(start) = parse_time(&snapshot.window_start) else {
        return Vec::new();
    };
    let total = window_minutes(snapshot);
    let first = 60 - i64::from(start.with_timezone(timezone).time().minute());
    (first..=total)
        .step_by(60)
        .map(|minute| {
            (
                scaled_minutes(minute, zoom_percent),
                (start + chrono::Duration::minutes(minute))
                    .with_timezone(timezone)
                    .format("%H:%M")
                    .to_string(),
            )
        })
        .collect()
}

fn now_line_left_pixels(snapshot: &GuideSnapshot, zoom_percent: i64) -> i64 {
    let (Some(start), Some(now)) = (
        parse_time(&snapshot.window_start),
        parse_time(&snapshot.generated_at),
    ) else {
        return 0;
    };
    scaled_minutes((now - start).num_minutes().max(0), zoom_percent)
}

fn programme_geometry(
    snapshot: &GuideSnapshot,
    programme: &ChannelWatchProgramme,
    zoom_percent: i64,
) -> (i64, i64) {
    let (Some(window_start), Some(window_finish), Some(start), Some(finish)) = (
        parse_time(&snapshot.window_start),
        parse_time(&snapshot.window_finish),
        parse_time(&programme.start),
        parse_time(&programme.finish),
    ) else {
        return (0, pixels_per_minute(zoom_percent));
    };
    let clipped_start = start.max(window_start);
    let clipped_finish = finish.min(window_finish);
    let left = scaled_minutes(
        (clipped_start - window_start).num_minutes().max(0),
        zoom_percent,
    );
    let width = (clipped_finish - clipped_start).num_seconds().max(20)
        * BASE_PIXELS_PER_MINUTE
        * zoom_percent
        / 6_000;
    (left, width.max(1))
}

const fn pixels_per_minute(zoom_percent: i64) -> i64 {
    BASE_PIXELS_PER_MINUTE * zoom_percent / 100
}

const fn scaled_minutes(minutes: i64, zoom_percent: i64) -> i64 {
    minutes * BASE_PIXELS_PER_MINUTE * zoom_percent / 100
}

const fn filler_label(kind: i32) -> &'static str {
    match kind {
        1 => "PRE",
        2 => "MID",
        3 => "POST",
        4 => "TAIL",
        5 => "FALLBACK",
        99 => "GUIDE",
        100 => "DECO",
        _ => "",
    }
}

fn scroll_to_minutes(node_ref: NodeRef<html::Div>, minutes: i64, zoom_percent: i64) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = node_ref.get() {
        element.set_scroll_left(
            i32::try_from(scaled_minutes(minutes, zoom_percent)).unwrap_or(i32::MAX),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (node_ref, minutes, zoom_percent);
}

fn update_zoom(node_ref: NodeRef<html::Div>, zoom: RwSignal<i64>, next_zoom: i64) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = node_ref.get() {
        let old_zoom = zoom.get_untracked();
        let timeline_viewport =
            i64::from(element.client_width()).saturating_sub(CHANNEL_COLUMN_PIXELS);
        let next_scroll = zoomed_scroll_left(
            i64::from(element.scroll_left()),
            timeline_viewport,
            old_zoom,
            next_zoom,
        );
        zoom.set(next_zoom);
        leptos::leptos_dom::helpers::request_animation_frame(move || {
            let max_scroll =
                i64::from(element.scroll_width()).saturating_sub(i64::from(element.client_width()));
            let next_scroll = next_scroll.clamp(0, max_scroll.max(0));
            element.set_scroll_left(i32::try_from(next_scroll).unwrap_or(i32::MAX));
        });
    } else {
        zoom.set(next_zoom);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = node_ref;
        zoom.set(next_zoom);
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn zoomed_scroll_left(
    scroll_left: i64,
    timeline_viewport: i64,
    old_zoom: i64,
    new_zoom: i64,
) -> i64 {
    let viewport = timeline_viewport.max(0);
    let old_zoom = old_zoom.max(1);
    let center_pixels = scroll_left.max(0).saturating_add(viewport / 2);
    center_pixels
        .saturating_mul(new_zoom.max(1))
        .saturating_div(old_zoom)
        .saturating_sub(viewport / 2)
        .max(0)
}

fn update_visible_start(
    node_ref: NodeRef<html::Div>,
    visible_start: RwSignal<i64>,
    snapshot: RwSignal<GuideSnapshot>,
    zoom_percent: i64,
) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = node_ref.get() {
        let minutes = i64::from(element.scroll_left()).max(0) * 100
            / (BASE_PIXELS_PER_MINUTE * zoom_percent).max(1);
        visible_start.set(minutes.clamp(0, snapshot.with(max_visible_start_minutes)));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (node_ref, visible_start, snapshot, zoom_percent);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> GuideSnapshot {
        GuideSnapshot {
            generated_at: "2026-07-26T10:00:00+00:00".to_owned(),
            window_start: "2026-07-26T08:00:00+00:00".to_owned(),
            window_finish: "2026-07-27T10:00:00+00:00".to_owned(),
            channels: Vec::new(),
        }
    }

    #[test]
    fn three_hour_default_viewport_starts_one_hour_before_now_and_clamps() {
        let data = snapshot();
        assert_eq!(window_minutes(&data), 1_560);
        assert_eq!(default_visible_start_minutes(&data), 60);
        assert_eq!(max_visible_start_minutes(&data), 1_380);
    }

    #[test]
    fn programme_geometry_scales_with_zoom() {
        let programme = ChannelWatchProgramme {
            start: "2026-07-26T07:30:00+00:00".to_owned(),
            finish: "2026-07-26T09:00:00+00:00".to_owned(),
            ..ChannelWatchProgramme::default()
        };
        assert_eq!(
            programme_geometry(&snapshot(), &programme, DEFAULT_ZOOM_PERCENT),
            (0, 360)
        );
        assert_eq!(programme_geometry(&snapshot(), &programme, 50), (0, 180));
    }

    #[test]
    fn zoom_preserves_the_temporal_viewport_center() {
        assert_eq!(zoomed_scroll_left(360, 720, 100, 150), 720);
        assert_eq!(zoomed_scroll_left(720, 720, 150, 50), 0);
        assert_eq!(zoomed_scroll_left(1_800, 720, 200, 50), 180);
    }

    #[test]
    fn zoom_at_the_start_clamps_negative_scroll_to_zero() {
        assert_eq!(zoomed_scroll_left(0, 720, 100, 50), 0);
    }

    #[test]
    fn hour_ticks_and_now_line_use_loaded_window() {
        let data = snapshot();
        assert_eq!(
            hour_ticks_in_timezone(&data, DEFAULT_ZOOM_PERCENT, &chrono::Utc)[0],
            (360, "09:00".to_owned())
        );
        assert_eq!(now_line_left_pixels(&data, DEFAULT_ZOOM_PERCENT), 720);
    }

    #[test]
    fn visible_window_and_ruler_use_local_time_including_fractional_offsets() {
        let data = snapshot();
        let timezone = chrono::FixedOffset::east_opt(5 * 60 * 60 + 30 * 60).unwrap();

        assert_eq!(visible_time_in_timezone(&data, 60, &timezone), "14:30");
        assert_eq!(
            hour_ticks_in_timezone(&data, DEFAULT_ZOOM_PERCENT, &timezone)[0],
            (180, "14:00".to_owned())
        );
    }

    #[test]
    fn every_filler_kind_has_a_semantic_label() {
        assert_eq!(
            [1, 2, 3, 4, 5, 99, 100]
                .into_iter()
                .map(filler_label)
                .collect::<Vec<_>>(),
            ["PRE", "MID", "POST", "TAIL", "FALLBACK", "GUIDE", "DECO"]
        );
    }
}
