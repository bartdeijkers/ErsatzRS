use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::admin::server_fns::media_browser::get_media_item_info_json;

const GENERAL_REPORT: &str = r#"{
  "Version": "ErsatzRS",
  "Environment": {},
  "Cpus": [],
  "VideoControllers": [],
  "Health": [],
  "FFmpegSettings": {},
  "AviSynth": {
    "Demuxer": null,
    "Installed": false
  },
  "Channels": [],
  "FFmpegProfiles": []
}"#;

#[component]
pub fn TroubleshootingPage() -> impl IntoView {
    view! {
        <div style="max-width:72rem;margin:0 auto;padding:2rem 0;display:flex;flex-direction:column;gap:1.5rem">
            <header style="display:flex;flex-direction:column;gap:0.75rem">
                <h1 style="font-size:1.5rem;font-weight:700;color:var(--color-gray-100);margin:0">
                    "Troubleshooting"
                </h1>
                <div style="height:1px;background:var(--color-border)"></div>
            </header>

            <div role="tablist" aria-label="Troubleshooting reports" style="display:flex;gap:0.5rem;flex-wrap:wrap">
                {tab_link("Tools", "tools-panel")}
                {tab_link("General", "general-panel")}
                {tab_link("NVIDIA", "nvidia-panel")}
                {tab_link("QSV", "qsv-panel")}
            </div>

            <section
                id="tools-panel"
                role="tabpanel"
                aria-label="Tools"
                style=panel_style()
            >
                <div style="display:flex;flex-direction:column;align-items:flex-start;gap:0.75rem">
                    <a href="/system/troubleshooting/sequential-schedule" style=tool_link_style()>
                        "Sequential Schedule Validation"
                    </a>
                    <a href="/system/troubleshooting/block-playout" style=tool_link_style()>
                        "Block Playout Troubleshooting"
                    </a>
                </div>
            </section>

            <section
                id="general-panel"
                role="tabpanel"
                aria-label="General"
                style=panel_style()
            >
                {report_block(GENERAL_REPORT, true)}
            </section>

            <section
                id="nvidia-panel"
                role="tabpanel"
                aria-label="NVIDIA"
                style=panel_style()
            >
                {report_block("NVIDIA capabilities have not been collected yet.", false)}
            </section>

            <section
                id="qsv-panel"
                role="tabpanel"
                aria-label="QSV"
                style=panel_style()
            >
                {report_block("QSV capabilities have not been collected yet.", false)}
            </section>
        </div>
    }
}

#[component]
pub fn PlaybackTroubleshootingPage() -> impl IntoView {
    let query = use_query_map();
    let media_item_id = query.with_untracked(|params| {
        params
            .get("mediaItem")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });

    let info: Resource<Result<String, ServerFnError>> = Resource::new_blocking(
        move || media_item_id,
        |id| async move {
            if id <= 0 {
                Err(ServerFnError::new("Missing mediaItem query parameter"))
            } else {
                get_media_item_info_json(id).await
            }
        },
    );

    view! {
        <div style="max-width:72rem;margin:0 auto;padding:2rem 0;display:flex;flex-direction:column;gap:1.5rem">
            <header style="display:flex;flex-direction:column;gap:0.75rem">
                <h1 style="font-size:1.5rem;font-weight:700;color:var(--color-gray-100);margin:0">
                    "Playback Troubleshooting"
                </h1>
                <div style="height:1px;background:var(--color-border)"></div>
            </header>

            <section style=panel_style()>
                <Suspense fallback=move || view! { <p>"Loading media info…"</p> }>
                    {move || match info.get() {
                        Some(Ok(json)) => view! {
                            <div style="display:flex;flex-direction:column;gap:1rem">
                                <p style="margin:0;color:var(--color-gray-300)">
                                    "Media item " {media_item_id.to_string()}
                                </p>
                                <pre style="max-height:31.25rem;overflow:auto;white-space:pre-wrap;margin:0;color:var(--color-gray-200);font-size:0.8125rem;line-height:1.5">
                                    <code>{json}</code>
                                </pre>
                            </div>
                        }.into_any(),
                        Some(Err(error)) => view! {
                            <p style="margin:0;color:var(--color-error,#f87171)">
                                {format!("Unable to load playback troubleshooting context: {error}")}
                            </p>
                        }.into_any(),
                        None => view! { <p>"Loading media info…"</p> }.into_any(),
                    }}
                </Suspense>
            </section>
        </div>
    }
}

fn tab_link(label: &'static str, panel_id: &'static str) -> impl IntoView {
    let href = format!("#{panel_id}");

    view! {
        <a href=href role="tab" style=tab_style()>
            {label}
        </a>
    }
}

fn report_block(content: &'static str, include_copy: bool) -> impl IntoView {
    view! {
        <div style="display:flex;flex-direction:column;gap:1rem">
            <pre style="max-height:31.25rem;overflow:auto;white-space:pre-wrap;margin:0;color:var(--color-gray-200);font-size:0.8125rem;line-height:1.5">
                <code>{content}</code>
            </pre>
            {include_copy.then(|| view! {
                <button type="button" style=copy_button_style()>"Copy"</button>
            })}
        </div>
    }
}

fn panel_style() -> &'static str {
    "border:1px solid var(--color-border);border-radius:var(--radius);padding:1.5rem;background:var(--color-surface)"
}

fn tab_style() -> &'static str {
    "border:1px solid var(--color-border);border-radius:var(--radius);background:var(--color-surface);color:var(--color-gray-100);display:inline-flex;align-items:center;min-height:2.25rem;padding:0 1rem;text-decoration:none"
}

fn tool_link_style() -> &'static str {
    "border:1px solid var(--color-border);border-radius:var(--radius);background:var(--color-primary);color:var(--color-on-primary,#fff);display:inline-flex;align-items:center;min-height:2.25rem;padding:0 1rem;text-decoration:none"
}

fn copy_button_style() -> &'static str {
    "border:1px solid var(--color-border);border-radius:var(--radius);background:var(--color-primary);color:var(--color-on-primary,#fff);height:2.25rem;padding:0 1rem;align-self:flex-start"
}
