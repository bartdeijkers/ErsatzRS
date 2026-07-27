use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::channels::{get_epg_preview_data, EpgPreviewData};

use super::guide::{EpgGuideGrid, GuideChannel, GuideSnapshot};

stylance::import_crate_style!(style, "src/admin/pages/channels/epg.module.css");

#[component]
pub fn EpgPreviewPage() -> impl IntoView {
    view! {
        <noscript>
            <main class=style::page>
                <h1>{tr(TextKey::EpgPreview)}</h1>
                <p>{tr(TextKey::JavascriptRequiredPlayback)}</p>
            </main>
        </noscript>
        <div class="epg-preview-js">
            <EpgPreviewIsland/>
        </div>
    }
}

#[island]
fn EpgPreviewIsland() -> impl IntoView {
    let snapshot = RwSignal::new(GuideSnapshot::default());
    let error = RwSignal::new(String::new());
    let initial = LocalResource::new(get_epg_preview_data);
    Effect::new(move |_| match initial.get() {
        Some(Ok(data)) => {
            error.set(String::new());
            snapshot.set(guide_snapshot(data));
        }
        Some(Err(problem)) => error.set(problem.to_string()),
        None => {}
    });

    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        let running = Arc::new(AtomicBool::new(true));
        let loop_running = Arc::clone(&running);
        leptos::task::spawn_local(async move {
            while loop_running.load(Ordering::Relaxed) {
                gloo_timers::future::TimeoutFuture::new(30_000).await;
                if !loop_running.load(Ordering::Relaxed) {
                    break;
                }
                match get_epg_preview_data().await {
                    Ok(data) => {
                        error.set(String::new());
                        snapshot.set(guide_snapshot(data));
                    }
                    Err(problem) => error.set(problem.to_string()),
                }
            }
        });
        on_cleanup(move || running.store(false, Ordering::Relaxed));
    }

    view! {
        <main class=style::page data-epg-preview-route>
            <header class=style::header>
                <div>
                    <h1>{tr(TextKey::EpgPreview)}</h1>
                    <p>{tr(TextKey::EpgPreviewDescription)}</p>
                </div>
                <span class=style::updated>
                    {move || snapshot.with(|data| {
                        if data.generated_at.is_empty() {
                            tr(TextKey::Loading).to_owned()
                        } else {
                            super::preview::programme_time(&data.generated_at)
                        }
                    })}
                </span>
            </header>
            <p
                class=style::error
                role="alert"
                style:display=move || if error.get().is_empty() { "none" } else { "" }
            >
                {move || format!("{}: {}", tr(TextKey::ErrorOccurred), error.get())}
            </p>
            <EpgGuideGrid snapshot=snapshot/>
        </main>
    }
}

fn guide_snapshot(data: EpgPreviewData) -> GuideSnapshot {
    GuideSnapshot {
        generated_at: data.generated_at,
        window_start: data.window_start,
        window_finish: data.window_finish,
        channels: data
            .channels
            .into_iter()
            .map(|channel| GuideChannel {
                id: channel.id,
                number: channel.number,
                name: channel.name,
                logo_url: channel.logo_url,
                is_active: channel.is_active,
                programmes: channel.programmes,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::channels::EpgPreviewChannelData;

    #[test]
    fn web_projection_keeps_channel_links_and_programmes() {
        let snapshot = guide_snapshot(EpgPreviewData {
            generated_at: "now".to_owned(),
            window_start: "start".to_owned(),
            window_finish: "finish".to_owned(),
            channels: vec![EpgPreviewChannelData {
                id: 42,
                number: "42".to_owned(),
                name: "Channel".to_owned(),
                is_active: true,
                programmes: vec![
                    crate::admin::server_fns::channels::ChannelWatchProgramme::default(),
                ],
                ..EpgPreviewChannelData::default()
            }],
        });
        assert_eq!(snapshot.channels[0].id, 42);
        assert!(snapshot.channels[0].is_active);
        assert_eq!(snapshot.channels[0].programmes.len(), 1);
    }
}
