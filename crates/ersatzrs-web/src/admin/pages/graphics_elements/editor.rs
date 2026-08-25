use std::fmt::Write as _;

use leptos::prelude::*;
use leptos::server_fn::ServerFn;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{graphics_element_error_message, tr, TextKey};
use crate::admin::server_fns::graphics_elements::{
    save_graphics_element, GraphicsElementForm, SaveGraphicsElement,
    MAX_REFRESH_INTERVAL_MILLISECONDS, MIN_REFRESH_INTERVAL_MILLISECONDS,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

#[cfg(feature = "ssr")]
use crate::admin::server_fns::graphics_elements::get_graphics_element_form;

stylance::import_crate_style!(style, "src/admin/pages/graphics_elements/editor.module.css");

const MAX_NAME_CHARS: usize = 50;
const MAX_TEMPLATE_BYTES: usize = 64 * 1024;

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[component]
pub fn GraphicsElementEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let element_id = params
        .with_untracked(|params| params.get("id").and_then(|value| value.parse::<i64>().ok()))
        .unwrap_or_default();
    let error_code = query.with_untracked(|query| query.get("error").unwrap_or_default());
    let saved = query.with_untracked(|query| query.get("saved").as_deref() == Some("1"));
    let data = load_graphics_element_no_script_data(element_id);
    let no_script_html = graphics_element_no_script_html(data.clone());
    let editor = match data {
        Ok(Some(initial_form)) => view! {
            <GraphicsElementEditorIsland initial_form error_code saved/>
        }
        .into_any(),
        Ok(None) => view! {
            <p class=style::message role="alert" data-graphics-element-not-found>
                {tr(TextKey::GraphicsElementNotFound)}
            </p>
        }
        .into_any(),
        Err(error) => view! {
            <p class=style::message role="alert" data-graphics-element-load-error>
                {error}
            </p>
        }
        .into_any(),
    };

    view! {
        <style>".graphics-element-noscript { display: none; } .graphics-element-js { display: block; }"</style>
        <noscript>
            <style>".graphics-element-noscript { display: block; } .graphics-element-js { display: none; }"</style>
            <div
                class="graphics-element-noscript"
                data-graphics-element-noscript
                inner_html=no_script_html
            ></div>
        </noscript>
        <div class="graphics-element-js">
            {editor}
        </div>
    }
}

fn graphics_element_no_script_html(data: Result<Option<GraphicsElementForm>, String>) -> String {
    let form = match data {
        Ok(Some(form)) => form,
        Ok(None) => {
            return format!(
                r#"<p class="{}" role="alert">{}</p>"#,
                style::message,
                escape_html(tr(TextKey::GraphicsElementNotFound)),
            );
        }
        Err(error) => {
            return format!(
                r#"<p class="{}" role="alert">{}: {}</p>"#,
                style::message,
                escape_html(tr(TextKey::ErrorOccurred)),
                escape_html(&error),
            );
        }
    };
    let title = if form.id == 0 {
        tr(TextKey::AddGraphicsElement)
    } else {
        tr(TextKey::EditGraphicsElement)
    };
    let save_label = if form.id == 0 {
        tr(TextKey::AddGraphicsElement)
    } else {
        tr(TextKey::SaveGraphicsElement)
    };
    let drawvg_selected =
        selected_attr(form.renderer == ersatzrs_domain::TextGraphicsRenderer::Drawvg);
    let drawtext_selected =
        selected_attr(form.renderer == ersatzrs_domain::TextGraphicsRenderer::Drawtext);
    let mut html = String::new();
    let _ = write!(
        html,
        r#"<form method="post" action="{action}" class="{page}" data-graphics-element-editor><input type="hidden" name="id" value="{id}"><div class="{breadcrumb}"><a href="/graphics-elements" class="{back_link}">&#8592; {elements}</a><h1 class="{page_title}">{title}</h1></div><div class="{action_bar}"><button type="submit" class="{primary}" data-save-graphics-element>{save_label}</button><a href="/graphics-elements" class="{secondary}">{cancel}</a></div><section class="{section}"><h2 class="{section_title}">{element}</h2><label class="{field}"><span class="{label}">{name_label}</span><input class="{input}" type="text" name="name" required maxlength="{max_name}" value="{name}" data-graphics-element-name></label><label class="{field}"><span class="{label}">{renderer_label}</span><select class="{input}" name="renderer" data-graphics-element-renderer-select><option value="drawvg"{drawvg_selected}>{drawvg}</option><option value="drawtext"{drawtext_selected}>{drawtext}</option></select></label><label class="{field}"><span class="{label}">{template_label}</span><textarea class="{textarea}" name="text" required maxlength="{max_template}" data-graphics-element-text>{text}</textarea></label></section><section class="{section}" data-drawtext-controls><h2 class="{section_title}">{settings}</h2><p class="{help}">{font_help}</p><label class="{field}"><span class="{label}">{font_label}</span><input class="{input}" type="text" name="font_file" value="{font_file}" data-drawtext-font-file></label><div class="{field_grid}"><label class="{field}"><span class="{label}">{refresh_label}</span><input class="{input}" type="number" name="refresh_interval_milliseconds" min="{min_refresh}" max="{max_refresh}" required value="{refresh}" data-drawtext-refresh-period></label><label class="{field}"><span class="{label}">{reload_label}</span><input class="{input}" type="number" name="reload_interval_frames" min="1" required value="{reload}" data-drawtext-reload-frames></label></div></section></form>"#,
        action = SaveGraphicsElement::PATH,
        page = style::page,
        id = form.id,
        breadcrumb = style::breadcrumb,
        back_link = style::back_link,
        elements = escape_html(tr(TextKey::GraphicsElements)),
        page_title = style::page_title,
        title = escape_html(title),
        action_bar = style::action_bar,
        primary = style::btn_primary,
        save_label = escape_html(save_label),
        secondary = style::btn_secondary,
        cancel = escape_html(tr(TextKey::Cancel)),
        section = style::section,
        section_title = style::section_title,
        element = escape_html(tr(TextKey::GraphicsElement)),
        field = style::field,
        label = style::label,
        name_label = escape_html(tr(TextKey::Name)),
        input = style::input,
        max_name = MAX_NAME_CHARS,
        name = escape_html(&form.name),
        renderer_label = escape_html(tr(TextKey::Renderer)),
        drawvg_selected = drawvg_selected,
        drawvg = escape_html(tr(TextKey::DrawvgRendererOption)),
        drawtext_selected = drawtext_selected,
        drawtext = escape_html(tr(TextKey::DrawtextRendererOption)),
        template_label = escape_html(tr(TextKey::TextTemplate)),
        textarea = style::textarea,
        max_template = MAX_TEMPLATE_BYTES,
        text = escape_html(&form.text),
        settings = escape_html(tr(TextKey::DrawtextSettings)),
        help = style::help,
        font_help = escape_html(tr(TextKey::FontFileOptionalHelp)),
        font_label = escape_html(tr(TextKey::FontFile)),
        font_file = escape_html(&form.font_file),
        field_grid = style::field_grid,
        refresh_label = escape_html(tr(TextKey::RefreshPeriodMilliseconds)),
        min_refresh = MIN_REFRESH_INTERVAL_MILLISECONDS,
        max_refresh = MAX_REFRESH_INTERVAL_MILLISECONDS,
        refresh = form.refresh_interval_milliseconds,
        reload_label = escape_html(tr(TextKey::ReloadFrameInterval)),
        reload = form.reload_interval_frames,
    );
    html
}

fn load_graphics_element_no_script_data(
    element_id: i64,
) -> Result<Option<GraphicsElementForm>, String> {
    if element_id == 0 {
        return Ok(Some(GraphicsElementForm::default()));
    }

    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { get_graphics_element_form(element_id).await })
                .map_err(|error| error.to_string())
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = element_id;
        Ok(None)
    }
}

fn selected_attr(selected: bool) -> &'static str {
    if selected {
        " selected"
    } else {
        ""
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

#[island]
fn GraphicsElementEditorIsland(
    initial_form: GraphicsElementForm,
    error_code: String,
    saved: bool,
) -> impl IntoView {
    view! {
        <GraphicsElementEditorForm
            initial_form
            initial_error=error_code
            saved
        />
    }
}

#[component]
fn GraphicsElementEditorForm(
    initial_form: GraphicsElementForm,
    initial_error: String,
    saved: bool,
) -> impl IntoView {
    let toast = use_toast();
    let element_id = initial_form.id;
    let initial_renderer = initial_form.renderer;
    let name = RwSignal::new(initial_form.name);
    let text = RwSignal::new(initial_form.text);
    let renderer = RwSignal::new(renderer_value(initial_renderer).to_owned());
    let font_file = RwSignal::new(initial_form.font_file);
    let refresh_interval = RwSignal::new(initial_form.refresh_interval_milliseconds.to_string());
    let reload_interval = RwSignal::new(initial_form.reload_interval_frames.to_string());
    let error_message = RwSignal::new(if initial_error.is_empty() {
        String::new()
    } else {
        graphics_element_error_message(&initial_error).clone()
    });
    let drawtext_selected = Signal::derive(move || renderer.get() == "drawtext");

    let save_action = Action::new(
        |(id, name, text, renderer, font_file, refresh, reload): &(
            i64,
            String,
            String,
            String,
            String,
            u64,
            u32,
        )| {
            let (id, name, text, renderer, font_file, refresh, reload) = (
                *id,
                name.clone(),
                text.clone(),
                renderer.clone(),
                font_file.clone(),
                *refresh,
                *reload,
            );
            async move {
                save_graphics_element(id, name, text, renderer, font_file, refresh, reload).await
            }
        },
    );

    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(saved_id) => {
                    toast.push(
                        tr(TextKey::GraphicsElementSaved).to_owned(),
                        ToastSeverity::Success,
                    );
                    set_browser_location(&format!("/graphics-elements/{saved_id}?saved=1"));
                }
                Err(error) => {
                    let message = graphics_element_error_message(&error).clone();
                    error_message.set(message.clone());
                    toast.push(message, ToastSeverity::Error);
                }
            }
        }
    });

    let submit = move || {
        let current_name = name.get_untracked();
        let current_text = text.get_untracked();
        let refresh = refresh_interval.get_untracked().parse::<u64>().ok();
        let reload = reload_interval.get_untracked().parse::<u32>().ok();
        let validation_key = if current_name.trim().is_empty() {
            Some(TextKey::NameRequired)
        } else if current_name.chars().count() > MAX_NAME_CHARS {
            Some(TextKey::GraphicsElementNameTooLong)
        } else if current_text.trim().is_empty() {
            Some(TextKey::TextTemplateRequired)
        } else if current_text.len() > MAX_TEMPLATE_BYTES || current_text.contains('\0') {
            Some(TextKey::TextTemplateTooLong)
        } else if !refresh.is_some_and(|value| {
            (MIN_REFRESH_INTERVAL_MILLISECONDS..=MAX_REFRESH_INTERVAL_MILLISECONDS).contains(&value)
        }) {
            Some(TextKey::RefreshPeriodBounds)
        } else if reload.is_none_or(|value| value == 0) {
            Some(TextKey::ReloadFrameIntervalBounds)
        } else {
            None
        };
        if let Some(key) = validation_key {
            error_message.set(tr(key).to_owned());
            return;
        }
        let (Some(refresh), Some(reload)) = (refresh, reload) else {
            error_message.set(tr(TextKey::GraphicsElementSaveFailed).to_owned());
            return;
        };
        error_message.set(String::new());
        save_action.dispatch((
            element_id,
            current_name,
            current_text,
            renderer.get_untracked(),
            font_file.get_untracked(),
            refresh,
            reload,
        ));
    };

    let title = if element_id == 0 {
        tr(TextKey::AddGraphicsElement)
    } else {
        tr(TextKey::EditGraphicsElement)
    };
    let save_label = if element_id == 0 {
        tr(TextKey::AddGraphicsElement)
    } else {
        tr(TextKey::SaveGraphicsElement)
    };
    let drawtext_initial = initial_renderer == ersatzrs_domain::TextGraphicsRenderer::Drawtext;

    view! {
        <form
            method="post"
            action=SaveGraphicsElement::PATH
            class=style::page
            data-graphics-element-editor
            data-graphics-element-id=element_id.to_string()
            on:submit=move |event| {
                event.prevent_default();
                submit();
            }
        >
            <input type="hidden" name="id" value=element_id.to_string()/>
            <div class=style::breadcrumb>
                <a href="/graphics-elements" class=style::back_link>
                    {format!("← {}", tr(TextKey::GraphicsElements))}
                </a>
                <h1 class=style::page_title>{title}</h1>
            </div>

            <div class=style::action_bar>
                <button
                    type="submit"
                    class=style::btn_primary
                    disabled=move || save_action.pending().get()
                    data-save-graphics-element
                >
                    {save_label}
                </button>
                <a href="/graphics-elements" class=style::btn_secondary>{tr(TextKey::Cancel)}</a>
            </div>

            <p
                class=style::success
                style:display=if saved { "block" } else { "none" }
                role="status"
                data-graphics-element-saved=saved.then_some("true")
            >
                {tr(TextKey::GraphicsElementSaved)}
            </p>
            <p
                class=style::error
                style:display=move || if error_message.get().is_empty() { "none" } else { "block" }
                role="alert"
                data-graphics-element-error
            >
                {move || error_message.get()}
            </p>

            <section class=style::section>
                <h2 class=style::section_title>{tr(TextKey::GraphicsElement)}</h2>
                <label class=style::field>
                    <span class=style::label>{tr(TextKey::Name)}</span>
                    <input
                        class=style::input
                        type="text"
                        name="name"
                        required
                        maxlength=MAX_NAME_CHARS.to_string()
                        prop:value=move || name.get()
                        on:input=move |event| name.set(event_target_value(&event))
                        data-graphics-element-name
                    />
                </label>
                <label class=style::field>
                    <span class=style::label>{tr(TextKey::Renderer)}</span>
                    <select
                        class=style::input
                        name="renderer"
                        prop:value=move || renderer.get()
                        on:input=move |event| renderer.set(event_target_value(&event))
                        data-graphics-element-renderer-select
                    >
                        <option value="drawvg" selected=initial_renderer == ersatzrs_domain::TextGraphicsRenderer::Drawvg>
                            {tr(TextKey::DrawvgRendererOption)}
                        </option>
                        <option value="drawtext" selected=drawtext_initial>
                            {tr(TextKey::DrawtextRendererOption)}
                        </option>
                    </select>
                </label>
                <label class=style::field>
                    <span class=style::label>{tr(TextKey::TextTemplate)}</span>
                    <textarea
                        class=style::textarea
                        name="text"
                        required
                        maxlength=MAX_TEMPLATE_BYTES.to_string()
                        prop:value=move || text.get()
                        on:input=move |event| text.set(event_target_value(&event))
                        data-graphics-element-text
                    >
                        {text.get_untracked()}
                    </textarea>
                </label>
            </section>

            <section
                class=style::section
                style:display=move || if drawtext_selected.get() { "flex" } else { "none" }
                aria-hidden=move || (!drawtext_selected.get()).to_string()
                data-drawtext-controls
            >
                <h2 class=style::section_title>{tr(TextKey::DrawtextSettings)}</h2>
                <p class=style::help>{tr(TextKey::FontFileOptionalHelp)}</p>
                {(!drawtext_initial).then(|| view! {
                    <input type="hidden" name="font_file" value=move || font_file.get()/>
                    <input type="hidden" name="refresh_interval_milliseconds" value=move || refresh_interval.get()/>
                    <input type="hidden" name="reload_interval_frames" value=move || reload_interval.get()/>
                })}
                <label class=style::field>
                    <span class=style::label>{tr(TextKey::FontFile)}</span>
                    <input
                        class=style::input
                        type="text"
                        name=drawtext_initial.then_some("font_file")
                        disabled=move || !drawtext_selected.get()
                        prop:value=move || font_file.get()
                        on:input=move |event| font_file.set(event_target_value(&event))
                        data-drawtext-font-file
                    />
                </label>
                <div class=style::field_grid>
                    <label class=style::field>
                        <span class=style::label>{tr(TextKey::RefreshPeriodMilliseconds)}</span>
                        <input
                            class=style::input
                            type="number"
                            name=drawtext_initial.then_some("refresh_interval_milliseconds")
                            min=MIN_REFRESH_INTERVAL_MILLISECONDS.to_string()
                            max=MAX_REFRESH_INTERVAL_MILLISECONDS.to_string()
                            required
                            disabled=move || !drawtext_selected.get()
                            prop:value=move || refresh_interval.get()
                            on:input=move |event| refresh_interval.set(event_target_value(&event))
                            data-drawtext-refresh-period
                        />
                    </label>
                    <label class=style::field>
                        <span class=style::label>{tr(TextKey::ReloadFrameInterval)}</span>
                        <input
                            class=style::input
                            type="number"
                            name=drawtext_initial.then_some("reload_interval_frames")
                            min="1"
                            required
                            disabled=move || !drawtext_selected.get()
                            prop:value=move || reload_interval.get()
                            on:input=move |event| reload_interval.set(event_target_value(&event))
                            data-drawtext-reload-frames
                        />
                    </label>
                </div>
            </section>

            <section class=format!("{} {}", style::section, style::information)>
                <h2 class=style::section_title>{tr(TextKey::Information)}</h2>
                <p class=style::help>{tr(TextKey::GraphicsExistingDrawvgHelp)}</p>
                <p class=style::help>{tr(TextKey::GraphicsStreamRestartHelp)}</p>
            </section>
        </form>
    }
}

const fn renderer_value(renderer: ersatzrs_domain::TextGraphicsRenderer) -> &'static str {
    match renderer {
        ersatzrs_domain::TextGraphicsRenderer::Drawvg => "drawvg",
        ersatzrs_domain::TextGraphicsRenderer::Drawtext => "drawtext",
    }
}
