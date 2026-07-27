use leptos::html;
use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::channels::{
    get_channel_preview_url, ChannelWatchAvailability, ChannelWatchData,
};

stylance::import_crate_style!(style, "src/admin/pages/channels/preview.module.css");

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ErsatzRSChannelPreview, js_name = start)]
    fn preview_start(video: &web_sys::HtmlVideoElement, url: &str);
    #[wasm_bindgen(js_namespace = ErsatzRSChannelPreview, js_name = destroy)]
    fn preview_destroy(video: &web_sys::HtmlVideoElement);
}

#[cfg(target_arch = "wasm32")]
fn start_browser_player(video_ref: NodeRef<html::Video>, url: &str) {
    if let Some(video) = video_ref.get() {
        preview_start(&video, url);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn start_browser_player(_video_ref: NodeRef<html::Video>, _url: &str) {}

#[cfg(target_arch = "wasm32")]
fn destroy_browser_player(video_ref: NodeRef<html::Video>) {
    if let Some(video) = video_ref.get() {
        preview_destroy(&video);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn destroy_browser_player(_video_ref: NodeRef<html::Video>) {}

#[allow(clippy::too_many_lines)]
#[component]
pub fn ChannelPreviewPlayer(
    data: RwSignal<ChannelWatchData>,
    #[prop(default = false)] compact: bool,
    #[prop(optional)] active: Option<Signal<bool>>,
) -> impl IntoView {
    let video_ref = NodeRef::<html::Video>::new();
    let request_error = RwSignal::new(String::new());
    let request_generation = RwSignal::new(0_u64);
    let autoplayed_channel = RwSignal::new(None::<i64>);
    let start_action = Action::new(move |request: &(i64, u64)| {
        let (channel_id, generation) = *request;
        async move {
            (
                channel_id,
                generation,
                get_channel_preview_url(channel_id).await,
            )
        }
    });

    Effect::new(move |_| {
        if let Some((channel_id, generation, result)) = start_action.value().get() {
            let is_active = active.is_none_or(|active| active.get_untracked());
            let current_channel_id = data.with_untracked(|watch| watch.id);
            if !preview_request_is_current(
                is_active,
                channel_id,
                current_channel_id,
                generation,
                request_generation.get_untracked(),
            ) {
                return;
            }
            match result {
                Ok(url) => {
                    request_error.set(String::new());
                    start_browser_player(video_ref, &url);
                }
                Err(_) => request_error.set(tr(TextKey::PreviewAuthorizationFailed).to_owned()),
            }
        }
    });

    if let Some(active) = active {
        Effect::new(move |_| {
            if !active.get() {
                autoplayed_channel.set(None);
                request_generation.set(request_generation.get_untracked().wrapping_add(1));
                destroy_browser_player(video_ref);
            }
        });
    }

    Effect::new(move |_| {
        let is_active = active.is_none_or(|active| active.get());
        let (channel_id, can_play) = data.with(|watch| (watch.id, watch.availability.can_play()));
        if !is_active {
            autoplayed_channel.set(None);
            return;
        }
        if !can_play || autoplayed_channel.get_untracked() == Some(channel_id) {
            return;
        }

        autoplayed_channel.set(Some(channel_id));
        request_error.set(String::new());
        let generation = request_generation.get_untracked().wrapping_add(1);
        request_generation.set(generation);
        start_action.dispatch((channel_id, generation));
    });

    on_cleanup(move || destroy_browser_player(video_ref));

    let can_play = move || data.with(|watch| watch.availability.can_play());
    let availability_message =
        move || data.with(|watch| availability_text(watch.availability).to_owned());
    let current_title = move || {
        data.with(|watch| {
            watch
                .programmes
                .iter()
                .find(|programme| programme.is_current)
                .map_or_else(|| tr(TextKey::NoProgramming).to_owned(), programme_title)
        })
    };

    view! {
        <section
            class=if compact { style::preview_compact } else { style::preview }
            data-channel-preview
            data-preview-state="idle"
            data-message-idle=tr(TextKey::PreviewIdle)
            data-message-connecting=tr(TextKey::PreviewConnecting)
            data-message-live=tr(TextKey::PreviewLive)
            data-message-paused=tr(TextKey::PreviewPaused)
            data-message-recovering=tr(TextKey::PreviewRecovering)
            data-message-stopped=tr(TextKey::PreviewStopped)
            data-message-unsupported=tr(TextKey::PreviewUnsupported)
            data-message-authorization=tr(TextKey::PreviewAuthorizationFailed)
            data-message-unavailable=tr(TextKey::PreviewUnavailable)
            data-message-capacity=tr(TextKey::PreviewCapacityReached)
            data-message-network=tr(TextKey::PreviewNetworkError)
            data-message-media=tr(TextKey::PreviewMediaError)
        >
            <div class=style::video_slot>
                <div class=style::video_frame>
                    <video
                        node_ref=video_ref
                        class=style::video
                        autoplay
                        controls
                        muted
                        playsinline
                        preload="none"
                    ></video>
                </div>
            </div>

            <p
                class=style::availability
                style:display=move || if can_play() { "none" } else { "" }
            >
                {availability_message}
            </p>
            <p class=style::status data-preview-status aria-live="polite">
                {move || {
                    let error = request_error.get();
                    if error.is_empty() { tr(TextKey::PreviewIdle).to_owned() } else { error }
                }}
            </p>
            <p class=style::derivative_note>{tr(TextKey::PreviewDerivativeNote)}</p>

            <div class=style::controls>
                <button
                    type="button"
                    class=style::control_button
                    disabled=move || !can_play() || start_action.pending().get()
                    on:click=move |_| {
                        let generation = request_generation.get_untracked().wrapping_add(1);
                        request_generation.set(generation);
                        let channel_id = data.with_untracked(|watch| watch.id);
                        start_action.dispatch((channel_id, generation));
                    }
                >
                    {tr(TextKey::ReloadStream)}
                </button>
                <button
                    type="button"
                    class=style::control_button
                    on:click=move |_| {
                        request_generation
                            .set(request_generation.get_untracked().wrapping_add(1));
                        destroy_browser_player(video_ref);
                    }
                >
                    {tr(TextKey::StopPreview)}
                </button>
            </div>

            <div class=style::current_programme>
                <span class=style::eyebrow>{tr(TextKey::NowPlaying)}</span>
                <strong>{current_title}</strong>
            </div>

            <noscript>
                <p class=style::availability>{tr(TextKey::JavascriptRequiredPlayback)}</p>
            </noscript>
        </section>
    }
}

const fn preview_request_is_current(
    is_active: bool,
    requested_channel_id: i64,
    current_channel_id: i64,
    request_generation: u64,
    current_generation: u64,
) -> bool {
    is_active
        && requested_channel_id == current_channel_id
        && request_generation == current_generation
}

fn availability_text(availability: ChannelWatchAvailability) -> &'static str {
    match availability {
        ChannelWatchAvailability::Ready => tr(TextKey::PreviewIdle),
        ChannelWatchAvailability::Disabled => tr(TextKey::ChannelDisabledPreview),
        ChannelWatchAvailability::NoPlayout => tr(TextKey::ChannelNoPlayout),
        ChannelWatchAvailability::NotCurrentlyAiring => tr(TextKey::ChannelNotAiring),
        ChannelWatchAvailability::NotFound => tr(TextKey::NotFound),
        ChannelWatchAvailability::Failed => tr(TextKey::PreviewUnavailable),
    }
}

pub fn programme_title(
    programme: &crate::admin::server_fns::channels::ChannelWatchProgramme,
) -> String {
    if programme.is_gap {
        tr(TextKey::ScheduleGap).to_owned()
    } else if programme.title.trim().is_empty() {
        if programme.filler_kind > 0 {
            tr(TextKey::Filler).to_owned()
        } else {
            tr(TextKey::NoProgramming).to_owned()
        }
    } else if programme.filler_kind > 0 {
        format!("{} · {}", programme.title, tr(TextKey::Filler))
    } else {
        programme.title.clone()
    }
}

pub fn programme_time(value: &str) -> String {
    programme_time_in_timezone(value, &chrono::Local)
}

fn programme_time_in_timezone<Tz>(value: &str, timezone: &Tz) -> String
where
    Tz: chrono::TimeZone,
    Tz::Offset: std::fmt::Display,
{
    chrono::DateTime::parse_from_rfc3339(value).map_or_else(
        |_| value.to_owned(),
        |parsed| parsed.with_timezone(timezone).format("%H:%M").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::channels::ChannelWatchProgramme;

    #[test]
    fn programme_labels_distinguish_gap_and_filler() {
        assert_eq!(
            programme_title(&ChannelWatchProgramme {
                is_gap: true,
                ..ChannelWatchProgramme::default()
            }),
            tr(TextKey::ScheduleGap)
        );
        assert!(programme_title(&ChannelWatchProgramme {
            title: "Intermission".to_owned(),
            filler_kind: 1,
            ..ChannelWatchProgramme::default()
        })
        .contains(tr(TextKey::Filler)));
    }

    #[test]
    fn programme_times_are_rendered_in_the_selected_local_offset() {
        let timezone = chrono::FixedOffset::east_opt(2 * 60 * 60).unwrap();

        assert_eq!(
            programme_time_in_timezone("2026-07-26T10:15:00+00:00", &timezone),
            "12:15"
        );
    }

    #[test]
    fn preview_start_results_are_ignored_after_close_or_request_replacement() {
        assert!(preview_request_is_current(true, 42, 42, 7, 7));
        assert!(!preview_request_is_current(false, 42, 42, 7, 7));
        assert!(!preview_request_is_current(true, 41, 42, 7, 7));
        assert!(!preview_request_is_current(true, 42, 42, 6, 7));
    }
}
