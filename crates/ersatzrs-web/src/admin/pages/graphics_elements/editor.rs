use super::context_preview::{load_channels, saved_preview_html, GraphicsContextPreview};
#[cfg(feature = "ssr")]
use crate::admin::server_fns::graphics_elements::get_graphics_element_form;
use crate::admin::{
    i18n::{tr, use_i18n, TextKey},
    server_fns::{
        channels::ChannelNumberEditorRow,
        graphics_authoring::{default_element, Fields},
        graphics_elements::GraphicsElementForm,
    },
};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

stylance::import_crate_style!(pub(super) style, "src/admin/pages/graphics_elements/editor.module.css");

#[component]
pub fn GraphicsElementEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = params
        .with_untracked(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or_default();
    let data = load(id);
    let channels = load_channels();
    let error = query.with_untracked(|q| q.get("error").unwrap_or_default());
    let saved = query.with_untracked(|q| q.get("saved").as_deref() == Some("1"));
    match data {
        Ok(Some(form)) => {
            let element=form.definition.clone().unwrap_or_else(default_element);
            let preview_fields: Fields = query.with_untracked(|q| ["preview_channel", "preview_time", "preview_width", "preview_height"].into_iter().filter_map(|key| q.get(key).map(|value| (key.into(), value))).collect());
            let Ok(html)=super::authoring::html(&element,&channels,use_i18n(),&error,saved,&preview_fields) else {
                return view! {<p role="alert" data-graphics-element-load-error>{tr(TextKey::ErrorOccurred)}</p>}.into_any();
            };
            let native=html.clone();
            let preview=query.with_untracked(|q|saved_preview_html(Some(&form),&channels,q.get("preview_channel").as_deref(),q.get("preview_time"),q.get("preview_width").as_deref(),q.get("preview_height").as_deref()));
            view! {
                <style>".graphics-element-noscript {display:none}.graphics-element-js {display:block}"</style>
                <noscript><style>".graphics-element-noscript {display:block}.graphics-element-js {display:none}"</style>
                    <div class="graphics-element-noscript" data-graphics-element-noscript inner_html=native></div>
                    <div class="graphics-element-noscript" inner_html=preview></div>
                </noscript>
                <div class="graphics-element-js"><GraphicsElementEditorIsland html form channels/></div>
            }.into_any()
        }
        Ok(None)=>view! {<p role="alert" data-graphics-element-not-found>{tr(TextKey::GraphicsElementNotFound)}</p>}.into_any(),
        Err(_)=>view! {<p role="alert" data-graphics-element-load-error>{tr(TextKey::ErrorOccurred)}</p>}.into_any(),
    }
}

fn load(id: i64) -> Result<Option<GraphicsElementForm>, String> {
    if id == 0 {
        return Ok(Some(GraphicsElementForm::default()));
    }
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_graphics_element_form(id))
        })
        .map_err(|error| error.to_string())
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = id;
        Ok(None)
    }
}

pub(crate) fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[island]
fn GraphicsElementEditorIsland(
    html: String,
    form: GraphicsElementForm,
    channels: Vec<ChannelNumberEditorRow>,
) -> impl IntoView {
    let text = RwSignal::new(form.text);
    let renderer = RwSignal::new(
        match form.renderer {
            ersatzrs_domain::TextGraphicsRenderer::Drawvg => "drawvg",
            ersatzrs_domain::TextGraphicsRenderer::Drawtext => "drawtext",
        }
        .to_owned(),
    );
    // Native submission is identical with and without hydration. Delegation only
    // opens the selected kind and synchronizes the existing text-context preview.
    view! {
        <div data-graphics-authoring on:click=move |event| {
            #[cfg(target_arch="wasm32")]
            {
                use wasm_bindgen::JsCast as _;
                if let Some(target)=event.target().and_then(|target|target.dyn_into::<web_sys::Element>().ok()) {
                    if target.closest("[data-graphics-preview-submit]").ok().flatten().is_some() { copy_text_preview_context(&target); }
                }
            }
            #[cfg(not(target_arch="wasm32"))]
            { let _=event; }
        } on:input=move |event| {
            #[cfg(target_arch="wasm32")]
            {
                use wasm_bindgen::JsCast as _;
                if let Some(target)=event.target().and_then(|target|target.dyn_into::<web_sys::Element>().ok()) {
                    if ["channel", "time", "width", "height"].into_iter().any(|field| target.has_attribute(&format!("data-graphics-preview-{field}"))) { copy_text_preview_context(&target); }
                    match target.get_attribute("name").as_deref() {
                        Some("text")=>text.set(event_target_value(&event)),
                        Some("renderer")=>renderer.set(event_target_value(&event)),
                        Some("kind")=>{
                            let value=event_target_value(&event);
                            if let Ok(Some(root))=target.closest("[data-graphics-element-editor]") {
                                if let Ok(groups)=root.query_selector_all("[data-kind-fields]") {
                                    for i in 0..groups.length() {if let Some(group)=groups.item(i).and_then(|node|node.dyn_into::<web_sys::Element>().ok()) {
                                        if group.get_attribute("data-kind-fields").as_deref()==Some(value.as_str()) {let _=group.set_attribute("open","");} else {let _=group.remove_attribute("open");}
                                    }}
                                }
                            }
                        }
                        _=>{}
                    }
                }
            }
            #[cfg(not(target_arch="wasm32"))]
            {let _=event;}
        }>
        <div inner_html=html></div>
        <GraphicsContextPreview element_id=form.id text renderer channels/>
        </div>
    }
}

#[cfg(target_arch = "wasm32")]
fn copy_text_preview_context(target: &web_sys::Element) {
    let Some(root) = target.closest("[data-graphics-authoring]").ok().flatten() else {
        return;
    };
    for (field, suffix) in [
        ("preview_channel", "channel"),
        ("preview_time", "time"),
        ("preview_width", "width"),
        ("preview_height", "height"),
    ] {
        let source = root
            .query_selector(&format!(
                "[data-graphics-context-preview] [data-graphics-preview-{suffix}]"
            ))
            .ok()
            .flatten();
        let destination = root
            .query_selector(&format!("[data-graphics-element-editor] [name='{field}']"))
            .ok()
            .flatten();
        if let (Some(source), Some(destination)) = (source, destination) {
            if let Ok(value) = js_sys::Reflect::get(source.as_ref(), &"value".into()) {
                let _ = js_sys::Reflect::set(destination.as_ref(), &"value".into(), &value);
            }
        }
    }
}
