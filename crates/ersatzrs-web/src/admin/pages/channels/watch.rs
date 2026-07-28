use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::channels::{
    get_channel_watch_data, stop_channel_session, ChannelWatchData,
};
use crate::admin::ui::primitives::dialog::Modal;

use super::guide::{EpgGuideGrid, GuideChannel, GuideSnapshot};
use super::preview::{programme_time, programme_title, ChannelPreviewPlayer};

stylance::import_crate_style!(style, "src/admin/pages/channels/watch.module.css");

#[component]
pub fn ChannelWatchPage() -> impl IntoView {
    let params = use_params_map();
    let channel_id = params.with(|params| {
        params
            .get("id")
            .and_then(|id| id.parse::<i64>().ok())
            .unwrap_or(0)
    });
    let no_script_html =
        channel_watch_no_script_html(load_channel_watch_no_script_data(channel_id));

    view! {
        <noscript>
            <div inner_html=no_script_html></div>
        </noscript>
        <div class="channel-watch-js">
            <ChannelWatchIsland channel_id=channel_id/>
        </div>
    }
}

#[island]
fn ChannelWatchIsland(channel_id: i64) -> impl IntoView {
    let data = RwSignal::new(ChannelWatchData {
        id: channel_id,
        number: String::new(),
        name: String::new(),
        logo_url: String::new(),
        ffmpeg_profile_name: String::new(),
        streaming_mode: 0,
        generated_at: String::new(),
        window_start: String::new(),
        window_finish: String::new(),
        availability: crate::admin::server_fns::channels::ChannelWatchAvailability::Failed,
        programmes: Vec::new(),
    });
    let initial = LocalResource::new(move || get_channel_watch_data(channel_id));
    Effect::new(move |_| match initial.get() {
        Some(Ok(snapshot)) => data.set(snapshot),
        Some(Err(_)) => data.update(|watch| {
            watch.availability =
                crate::admin::server_fns::channels::ChannelWatchAvailability::Failed;
        }),
        None => {}
    });

    view! { <ChannelWatchContent data=data/> }
}

#[component]
fn ChannelWatchContent(data: RwSignal<ChannelWatchData>) -> impl IntoView {
    let guide_snapshot = RwSignal::new(GuideSnapshot::default());
    Effect::new(move |_| {
        guide_snapshot.set(channel_guide_snapshot(data.get()));
    });
    let stop_confirm_open = RwSignal::new(false);
    let stop_action = Action::new(move |number: &String| {
        let number = number.clone();
        async move { stop_channel_session(number).await }
    });

    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        let running = Arc::new(AtomicBool::new(true));
        let loop_running = Arc::clone(&running);
        let channel_id = data.with_untracked(|watch| watch.id);
        leptos::task::spawn_local(async move {
            while loop_running.load(Ordering::Relaxed) {
                gloo_timers::future::TimeoutFuture::new(30_000).await;
                if !loop_running.load(Ordering::Relaxed) {
                    break;
                }
                match get_channel_watch_data(channel_id).await {
                    Ok(snapshot) => data.set(snapshot),
                    Err(_) => data.update(|watch| {
                        watch.availability = crate::admin::server_fns::channels::ChannelWatchAvailability::Failed;
                    }),
                }
            }
        });
        on_cleanup(move || running.store(false, Ordering::Relaxed));
    }

    let current = move || {
        data.with(|watch| {
            watch
                .programmes
                .iter()
                .find(|programme| programme.is_current)
                .cloned()
        })
    };
    let next = move || {
        data.with(|watch| {
            let current_finish = watch
                .programmes
                .iter()
                .find(|programme| programme.is_current)
                .map(|programme| programme.finish.as_str());
            watch
                .programmes
                .iter()
                .find(|programme| {
                    !programme.is_current
                        && !programme.is_gap
                        && current_finish.is_none_or(|finish| programme.start.as_str() >= finish)
                })
                .cloned()
        })
    };

    view! {
        <div class=style::page data-channel-watch-route>
            <header class=style::header>
                <a href="/channels" class=style::back_link>{tr(TextKey::BackToChannels)}</a>
                <div class=style::identity>
                    <img
                        class=style::logo
                        src=move || data.with(|watch| watch.logo_url.clone())
                        alt=move || data.with(|watch| format!("{} {}", watch.name, tr(TextKey::Logo)))
                    />
                    <div>
                        <span class=style::live_badge>{tr(TextKey::ContentVariantLive)}</span>
                        <h1>
                            {move || data.with(|watch| format!("{} · {}", watch.number, watch.name))}
                        </h1>
                        <p>
                            {move || data.with(|watch| {
                                format!(
                                    "{} · {}",
                                    watch.ffmpeg_profile_name,
                                    crate::admin::i18n::channel_streaming_mode_label(watch.streaming_mode)
                                )
                            })}
                        </p>
                    </div>
                </div>
                <div class=style::actions>
                    <a
                        class=style::action_link
                        href=move || data.with(|watch| format!("/channels/{}", watch.id))
                    >
                        {tr(TextKey::EditChannel)}
                    </a>
                    <a
                        class=style::action_link
                        href=move || data.with(|watch| {
                            format!("/system/troubleshooting/playback?channel={}", watch.id)
                        })
                    >
                        {tr(TextKey::TroubleshootChannel)}
                    </a>
                    <button
                        type="button"
                        class=style::danger_button
                        on:click=move |_| stop_confirm_open.set(true)
                    >
                        {tr(TextKey::StopTranscodeSession)}
                    </button>
                </div>
            </header>

            <main class=style::content>
                <div class=style::preview_region data-watch-preview-region>
                    <ChannelPreviewPlayer data=data align_start=true/>
                </div>

                <section class=style::now_next>
                    <article class=style::programme_card>
                        <span>{tr(TextKey::NowPlaying)}</span>
                        <strong>
                            {move || current().as_ref().map_or_else(
                                || tr(TextKey::NoProgramming).to_owned(),
                                programme_title,
                            )}
                        </strong>
                        <small>
                            {move || current().as_ref().map_or_else(String::new, |programme| {
                                format!(
                                    "{}–{}",
                                    programme_time(&programme.start),
                                    programme_time(&programme.finish)
                                )
                            })}
                        </small>
                    </article>
                    <article class=style::programme_card>
                        <span>{tr(TextKey::UpNext)}</span>
                        <strong>
                            {move || next().as_ref().map_or_else(
                                || tr(TextKey::NoProgramming).to_owned(),
                                programme_title,
                            )}
                        </strong>
                        <small>
                            {move || next().as_ref().map_or_else(String::new, |programme| {
                                format!(
                                    "{}–{}",
                                    programme_time(&programme.start),
                                    programme_time(&programme.finish)
                                )
                            })}
                        </small>
                    </article>
                </section>

                <section aria-label=tr(TextKey::ProgrammeGuide)>
                    <h2>{tr(TextKey::ProgrammeGuide)}</h2>
                    <EpgGuideGrid snapshot=guide_snapshot/>
                </section>
            </main>

            <Modal
                open=stop_confirm_open
                title=tr(TextKey::StopTranscodeSession).to_owned()
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        stop_action.dispatch(data.with_untracked(|watch| watch.number.clone()));
                    }
                })
                confirm_label=Some(tr(TextKey::StopTranscodeSession))
                destructive=true
            >
                <p>{tr(TextKey::ConfirmStopTranscodeSession)}</p>
            </Modal>
        </div>
    }
}

fn channel_watch_no_script_html(data: Result<ChannelWatchData, String>) -> String {
    match data {
        Ok(watch)
            if watch.availability
                == crate::admin::server_fns::channels::ChannelWatchAvailability::NotFound =>
        {
            format!(
                r#"<main class="{}"><p>{}</p><a href="/channels">{}</a></main>"#,
                style::content,
                escape_html(tr(TextKey::NotFound)),
                escape_html(tr(TextKey::BackToChannels)),
            )
        }
        Ok(watch) => format!(
            r#"<main class="{}"><h1>{} · {}</h1><p>{}</p><a href="/channels">{}</a></main>"#,
            style::content,
            escape_html(&watch.number),
            escape_html(&watch.name),
            escape_html(tr(TextKey::JavascriptRequiredPlayback)),
            escape_html(tr(TextKey::BackToChannels)),
        ),
        Err(error) => format!(
            r#"<main class="{}"><p role="alert">{}: {}</p><a href="/channels">{}</a></main>"#,
            style::content,
            escape_html(tr(TextKey::ErrorOccurred)),
            escape_html(&error),
            escape_html(tr(TextKey::BackToChannels)),
        ),
    }
}

fn load_channel_watch_no_script_data(channel_id: i64) -> Result<ChannelWatchData, String> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { get_channel_watch_data(channel_id).await })
                .map_err(|error| error.to_string())
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = channel_id;
        Err(tr(TextKey::JavascriptRequiredPlayback).to_owned())
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn channel_guide_snapshot(data: ChannelWatchData) -> GuideSnapshot {
    GuideSnapshot {
        generated_at: data.generated_at,
        window_start: data.window_start,
        window_finish: data.window_finish,
        channels: vec![GuideChannel {
            id: data.id,
            number: data.number,
            name: data.name,
            logo_url: data.logo_url,
            is_active: data.availability.can_play(),
            programmes: data.programmes,
        }],
    }
}
