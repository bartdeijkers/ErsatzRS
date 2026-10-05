use leptos::prelude::*;

use super::editor::{escape_html, style};
use crate::admin::server_fns::graphics_elements::GraphicsElementForm;
use crate::admin::{
    i18n::{tr, TextKey},
    server_fns::{
        channels::ChannelNumberEditorRow,
        graphics_elements::{preview_graphics_text, GraphicsTextPreviewForm},
    },
};

pub(super) fn saved_preview_html(
    form: Option<&GraphicsElementForm>,
    channels: &[ChannelNumberEditorRow],
    channel: Option<&str>,
    timestamp: Option<String>,
    width: Option<&str>,
    height: Option<&str>,
) -> String {
    use std::fmt::Write as _;
    let Some(form) = form.filter(|form| form.id > 0) else {
        return String::new();
    };
    let submitted = channel.is_some();
    let selected = channel
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_default();
    let timestamp = timestamp.unwrap_or_default();
    let width = width
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1920);
    let height = height
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1080);
    let mut options = String::new();
    for channel in channels {
        let _ = write!(
            options,
            "<option value=\"{}\"{}>{}</option>",
            channel.id,
            if channel.id == selected {
                " selected"
            } else {
                ""
            },
            escape_html(&format!("{} · {}", channel.number, channel.name))
        );
    }
    let mut html = format!(
        r#"<form method="get" class="{}" data-graphics-context-preview><h2>{}</h2><p>{}</p><label class="{}">{}<select class="{}" name="preview_channel" data-graphics-preview-channel>{}</select></label><label class="{}">{}<input class="{}" name="preview_time" value="{}" required data-graphics-preview-time></label><p>{}</p><label class="{}">{}<input class="{}" name="preview_width" type="number" min="16" max="8192" value="{}" required></label><label class="{}">{}<input class="{}" name="preview_height" type="number" min="16" max="8192" value="{}" required></label><button class="{}" type="submit" data-graphics-preview-submit>{}</button>"#,
        style::section,
        escape_html(tr(TextKey::GraphicsContextPreview)),
        escape_html(tr(TextKey::GraphicsPreviewSavedHelp)),
        style::field,
        escape_html(tr(TextKey::Channel)),
        style::input,
        options,
        style::field,
        escape_html(tr(TextKey::PreviewTimestamp)),
        style::input,
        escape_html(&timestamp),
        escape_html(tr(TextKey::GraphicsPreviewTimeHelp)),
        style::field,
        escape_html(tr(TextKey::Width)),
        style::input,
        width,
        style::field,
        escape_html(tr(TextKey::Height)),
        style::input,
        height,
        style::btn_secondary,
        escape_html(tr(TextKey::Preview)),
    );
    if submitted {
        let request = GraphicsTextPreviewForm {
            channel_id: selected,
            element_id: form.id,
            render_time: timestamp,
            template: form.text.clone(),
            renderer: form.renderer,
            width,
            height,
        };
        #[cfg(feature = "ssr")]
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(preview_graphics_text(request))
        });
        #[cfg(not(feature = "ssr"))]
        let result: Result<
            crate::admin::server_fns::graphics_elements::GraphicsTextPreviewResult,
            ServerFnError,
        > = {
            let _ = request;
            Err(ServerFnError::new("preview unavailable"))
        };
        match result {
            Ok(preview) => {
                let _ = write!(html, "<pre class=\"{}\" role=\"status\" data-graphics-preview-text>{}</pre><details><summary>{}</summary><pre class=\"{}\" data-graphics-preview-context>{}\n{}</pre></details>",
                style::preview_output, escape_html(&preview.text), escape_html(tr(TextKey::ProgrammeGuide)), style::preview_output,
                escape_html(&preview.local_time), escape_html(&preview.programme_context));
            }
            Err(error) => {
                let _ = write!(
                    html,
                    "<p class=\"{}\" role=\"alert\" data-graphics-preview-error>{}</p>",
                    style::error,
                    escape_html(&preview_error(error))
                );
            }
        }
    }
    html.push_str("</form>");
    html
}

pub(super) fn load_channels() -> Vec<ChannelNumberEditorRow> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(crate::admin::server_fns::channels::list_channel_number_rows())
        })
        .unwrap_or_default()
    }
    #[cfg(not(feature = "ssr"))]
    {
        Vec::new()
    }
}

pub(super) fn preview_error(error: impl std::fmt::Display) -> String {
    let error = error.to_string();
    if error.contains("graphics-preview-no-item") {
        tr(TextKey::ChannelNotAiring).to_owned()
    } else if error.contains("graphics-preview-ambiguous") {
        tr(TextKey::GraphicsPreviewAmbiguous).to_owned()
    } else if error.contains("graphics-preview-invalid-time") {
        tr(TextKey::GraphicsPreviewTimeHelp).to_owned()
    } else if error.contains("graphics-preview-no-media") {
        tr(TextKey::PreviewUnavailable).to_owned()
    } else if error.contains("graphics-preview-timeout") {
        tr(TextKey::PreviewStartupFailed).to_owned()
    } else {
        format!("{}: {error}", tr(TextKey::PreviewConfigurationInvalid))
    }
}

#[component]
pub(super) fn GraphicsContextPreview(
    element_id: i64,
    text: RwSignal<String>,
    renderer: RwSignal<String>,
    channels: Vec<ChannelNumberEditorRow>,
) -> impl IntoView {
    let channel = RwSignal::new(
        channels
            .first()
            .map(|row| row.id.to_string())
            .unwrap_or_default(),
    );
    let timestamp = RwSignal::new(String::new());
    let width = RwSignal::new("1920".to_owned());
    let height = RwSignal::new("1080".to_owned());
    let result = RwSignal::new(String::new());
    let context = RwSignal::new(String::new());
    let error = RwSignal::new(String::new());
    let action = Action::new(|request: &GraphicsTextPreviewForm| {
        let request = request.clone();
        async move { preview_graphics_text(request).await }
    });
    Effect::new(move |_| {
        if let Some(value) = action.value().get() {
            match value {
                Ok(value) => {
                    error.set(String::new());
                    result.set(value.text);
                    context.set(format!("{}\n{}", value.local_time, value.programme_context));
                }
                Err(value) => {
                    result.set(String::new());
                    context.set(String::new());
                    error.set(preview_error(value));
                }
            }
        }
    });
    view! {
        <section class=style::section data-graphics-context-preview>
            <h2 class=style::section_title>{tr(TextKey::GraphicsContextPreview)}</h2>
            <p class=style::help>{tr(TextKey::GraphicsContextPreviewHelp)}</p>
            <label class=style::field><span class=style::label>{tr(TextKey::Channel)}</span>
                <select class=style::input prop:value=move || channel.get()
                    on:input=move |event| channel.set(event_target_value(&event)) data-graphics-preview-channel>
                    {channels.into_iter().map(|row| view! {
                        <option value=row.id.to_string()>{format!("{} · {}", row.number, row.name)}</option>
                    }).collect_view()}
                </select>
            </label>
            <label class=style::field><span class=style::label>{tr(TextKey::PreviewTimestamp)}</span>
                <input class=style::input type="text" prop:value=move || timestamp.get()
                    on:input=move |event| timestamp.set(event_target_value(&event))
                    aria-describedby="graphics-preview-time-help" data-graphics-preview-time/>
            </label>
            <p class=style::help id="graphics-preview-time-help">{tr(TextKey::GraphicsPreviewTimeHelp)}</p>
            <div class=style::field_grid>
                <label class=style::field><span class=style::label>{tr(TextKey::Width)}</span>
                    <input class=style::input type="number" min="16" max="8192" prop:value=move || width.get()
                        on:input=move |event| width.set(event_target_value(&event)) data-graphics-preview-width/>
                </label>
                <label class=style::field><span class=style::label>{tr(TextKey::Height)}</span>
                    <input class=style::input type="number" min="16" max="8192" prop:value=move || height.get()
                        on:input=move |event| height.set(event_target_value(&event)) data-graphics-preview-height/>
                </label>
            </div>
            <button type="button" class=style::btn_secondary disabled=move || action.pending().get()
                on:click=move |_| {
                    let dimensions = width.get_untracked().parse::<u32>().ok().zip(height.get_untracked().parse::<u32>().ok());
                    let selected = channel.get_untracked().parse::<i64>().ok();
                    if let (Some(channel_id), Some((width, height))) = (selected, dimensions) {
                        result.set(String::new()); context.set(String::new()); error.set(String::new());
                        action.dispatch(GraphicsTextPreviewForm { channel_id, element_id, render_time: timestamp.get_untracked(),
                            template: text.get_untracked(), renderer: if renderer.get_untracked() == "drawtext" { ersatzrs_domain::TextGraphicsRenderer::Drawtext } else { ersatzrs_domain::TextGraphicsRenderer::Drawvg }, width, height });
                    } else { error.set(tr(TextKey::PreviewConfigurationInvalid).to_owned()); }
                } data-graphics-preview-submit>{tr(TextKey::Preview)}</button>
            <p class=style::error role="alert" style:display=move || if error.get().is_empty() {"none"} else {"block"}
                data-graphics-preview-error>{move || error.get()}</p>
            <pre class=style::preview_output role="status" aria-busy=move || action.pending().get().to_string()
                data-graphics-preview-text>{move || result.get()}</pre>
            <details><summary>{tr(TextKey::ProgrammeGuide)}</summary>
                <pre class=style::preview_output data-graphics-preview-context>{move || context.get()}</pre>
            </details>
        </section>
    }
}
