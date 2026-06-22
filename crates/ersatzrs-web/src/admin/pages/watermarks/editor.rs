//! Watermark editor — ports `Pages/WatermarkEditor.razor`.
//!
//! - [`WatermarkEditorPage`] — SSR wrapper; loads form data server-side.
//! - [`WatermarkEditorIsland`] — fully hydrated editor. Conditional sections:
//!   - Mode-dependent fields stay visible and are disabled to match ErsatzTV.
//!   - drawvg preview refreshes (debounced 400 ms) whenever placement params change.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{save_failed, tr, watermark_error_message, TextKey};
use crate::admin::server_fns::watermarks::{
    get_watermark_form, preview_watermark_placement, save_watermark, WatermarkForm,
    WatermarkPreviewParams,
};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/watermarks/editor.module.css");

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

// ---------------------------------------------------------------------------
// SSR wrapper
// ---------------------------------------------------------------------------

#[component]
pub fn WatermarkEditorPage() -> impl IntoView {
    let params = use_params_map();
    let watermark_id = params
        .with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(0);

    let data: Resource<Option<WatermarkForm>> = Resource::new_blocking(
        move || watermark_id,
        |id| async move {
            if id == 0 {
                Some(WatermarkForm::default())
            } else {
                get_watermark_form(id).await.ok()
            }
        },
    );

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
            {move || {
                data.get().map(|maybe| match maybe {
                    Some(form) => view! {
                        <WatermarkEditorIsland initial_form=form/>
                    }
                    .into_any(),
                    None => view! {
                        <p class=style::empty>{tr(TextKey::WatermarkNotFound)}</p>
                    }
                    .into_any(),
                })
            }}
        </Suspense>
    }
}

// ---------------------------------------------------------------------------
// Island — owns blocking resource; inner component renders the editor body
// ---------------------------------------------------------------------------

#[island]
fn WatermarkEditorIsland(initial_form: WatermarkForm) -> impl IntoView {
    view! {
        <WatermarkEditorForm initial_form=initial_form/>
    }
}

#[component]
fn WatermarkEditorForm(initial_form: WatermarkForm) -> impl IntoView {
    let watermark_id = initial_form.id;

    // ── Signals ──────────────────────────────────────────────────────────
    let name = RwSignal::new(initial_form.name.clone());
    let mode = RwSignal::new(initial_form.mode);
    let image_source = RwSignal::new(initial_form.image_source);
    let image_path = RwSignal::new(initial_form.image.clone());
    let image_content_type = RwSignal::new(initial_form.original_content_type.clone());
    let location = RwSignal::new(initial_form.location);
    let size = RwSignal::new(initial_form.size);
    let width_percent = RwSignal::new(initial_form.width_percent.to_string());
    let horizontal_margin_percent =
        RwSignal::new(initial_form.horizontal_margin_percent.to_string());
    let vertical_margin_percent = RwSignal::new(initial_form.vertical_margin_percent.to_string());
    let frequency_minutes = RwSignal::new(initial_form.frequency_minutes);
    let duration_seconds = RwSignal::new(i64::from(initial_form.duration_seconds));
    let opacity = RwSignal::new(i64::from(initial_form.opacity));
    let place_within_source_content = RwSignal::new(initial_form.place_within_source_content);
    let opacity_expression = RwSignal::new(initial_form.opacity_expression.clone());
    let z_index = RwSignal::new(i64::from(initial_form.z_index));

    let is_uploading = RwSignal::new(false);

    // ── drawvg preview — debounced 400 ms ────────────────────────────────
    let preview_key: RwSignal<Option<WatermarkPreviewParams>> = RwSignal::new(None);

    Effect::new(move |_| {
        let loc = location.get();
        let w = width_percent.get().parse::<f64>().unwrap_or(15.0);
        let h = horizontal_margin_percent
            .get()
            .parse::<f64>()
            .unwrap_or(5.0);
        let v = vertical_margin_percent.get().parse::<f64>().unwrap_or(5.0);
        let m = mode.get();
        let s = size.get();
        let op = i32::try_from(opacity.get()).unwrap_or(100);
        let params = WatermarkPreviewParams {
            location: loc,
            width_pct: w,
            h_margin: h,
            v_margin: v,
            mode: m,
            size: s,
            opacity: op,
        };

        // Debounce: on WASM schedule 400 ms before applying; on SSR apply immediately
        // (SSR Effects do not run, so the cfg(not(wasm32)) branch never executes).
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            use leptos::task::spawn_local;
            spawn_local(async move {
                TimeoutFuture::new(400).await;
                preview_key.set(Some(params));
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        preview_key.set(Some(params));
    });

    let preview_resource: Resource<Option<String>> = Resource::new(
        move || preview_key.get(),
        |maybe_params| async move {
            let params = maybe_params?;
            preview_watermark_placement(params).await.ok()
        },
    );

    // ── Save action ──────────────────────────────────────────────────────
    let save_action = Action::new(move |form: &WatermarkForm| {
        let form = form.clone();
        async move { save_watermark(form).await }
    });

    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(_) => set_browser_location("/watermarks"),
                Err(e) => toast.push(
                    save_failed(watermark_error_message(e)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let build_form = move || {
        let width = width_percent.get_untracked().parse::<f64>().unwrap_or(15.0);
        let h_margin = horizontal_margin_percent
            .get_untracked()
            .parse::<f64>()
            .unwrap_or(5.0);
        let v_margin = vertical_margin_percent
            .get_untracked()
            .parse::<f64>()
            .unwrap_or(5.0);
        WatermarkForm {
            id: watermark_id,
            name: name.get_untracked(),
            mode: mode.get_untracked(),
            image_source: image_source.get_untracked(),
            image: image_path.get_untracked(),
            original_content_type: image_content_type.get_untracked(),
            location: location.get_untracked(),
            size: size.get_untracked(),
            width_percent: width,
            horizontal_margin_percent: h_margin,
            vertical_margin_percent: v_margin,
            frequency_minutes: frequency_minutes.get_untracked(),
            duration_seconds: i32::try_from(duration_seconds.get_untracked()).unwrap_or(15),
            opacity: i32::try_from(opacity.get_untracked()).unwrap_or(100),
            place_within_source_content: place_within_source_content.get_untracked(),
            opacity_expression: opacity_expression.get_untracked(),
            z_index: i32::try_from(z_index.get_untracked()).unwrap_or(0),
        }
    };

    let on_save = move || {
        save_action.dispatch(build_form());
    };

    let is_saving = save_action.pending();
    let is_busy = Signal::derive(move || is_saving.get() || is_uploading.get());
    let is_edit = watermark_id != 0;
    let title = if is_edit {
        tr(TextKey::EditWatermark)
    } else {
        tr(TextKey::AddWatermark)
    };
    let btn_label = if is_edit {
        tr(TextKey::SaveWatermark)
    } else {
        tr(TextKey::AddWatermark)
    };

    let mode_items: Vec<(i32, String)> = vec![
        (0, tr(TextKey::NoneOption).to_string()),
        (1, tr(TextKey::Permanent).to_string()),
        (2, tr(TextKey::Intermittent).to_string()),
        (3, tr(TextKey::OpacityExpression).to_string()),
    ];
    let image_source_items: Vec<(i32, String)> = vec![
        (0, tr(TextKey::Custom).to_string()),
        (1, tr(TextKey::ChannelLogo).to_string()),
    ];
    let mut location_items = vec![
        (0i32, tr(TextKey::BottomRight).to_string()),
        (6i32, tr(TextKey::BottomMiddle).to_string()),
        (1i32, tr(TextKey::BottomLeft).to_string()),
        (7i32, tr(TextKey::LeftMiddle).to_string()),
        (3i32, tr(TextKey::TopLeft).to_string()),
        (4i32, tr(TextKey::TopMiddle).to_string()),
        (2i32, tr(TextKey::TopRight).to_string()),
        (5i32, tr(TextKey::RightMiddle).to_string()),
    ];
    if initial_form.location == 8 {
        location_items.push((8i32, tr(TextKey::MiddleCenter).to_string()));
    }
    let size_items = vec![
        (0i32, tr(TextKey::Scaled).to_string()),
        (1i32, tr(TextKey::ActualSize).to_string()),
    ];
    let freq_items = vec![
        (5i32, format!("5 {}", tr(TextKey::Minutes))),
        (10i32, format!("10 {}", tr(TextKey::Minutes))),
        (15i32, format!("15 {}", tr(TextKey::Minutes))),
        (20i32, format!("20 {}", tr(TextKey::Minutes))),
        (30i32, format!("30 {}", tr(TextKey::Minutes))),
        (60i32, format!("60 {}", tr(TextKey::Minutes))),
    ];
    let select_class = format!("{} {}", style::input, style::select);

    view! {
        <form
            method="post"
            action="/api/watermarks/save"
            enctype="multipart/form-data"
            class=style::page
            on:submit=move |ev| {
                ev.prevent_default();
                on_save();
            }
        >
            <input type="hidden" name="id" value=watermark_id.to_string()/>
            <div class=style::breadcrumb>
                <a href="/watermarks" class=style::back_link>
                    {format!("← {}", tr(TextKey::Watermarks))}
                </a>
                <h1 class=style::page_title>{title}</h1>
            </div>

            <div class=style::action_bar>
                <button
                    type="submit"
                    class=style::btn_primary
                    disabled=move || is_busy.get()
                >
                    {btn_label}
                </button>
            </div>

            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::Identity)}</h2>
                <div class=style::fields>
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::Name)}</label>
                        <input
                            type="text"
                            name="name"
                            class=style::input
                            value=name.get_untracked()
                            prop:value=move || name.get()
                            on:input=move |e| name.set(event_target_value(&e))
                        />
                    </div>
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::Mode)}</label>
                        <select
                            name="mode"
                            class=select_class.clone()
                            prop:value=move || mode.get().to_string()
                            on:change=move |e| {
                                if let Ok(parsed) = event_target_value(&e).parse::<i32>() {
                                    mode.set(parsed);
                                }
                            }
                        >
                            {mode_items
                                .into_iter()
                                .map(|(value, label)| {
                                    let selected = value == mode.get_untracked();
                                    view! {
                                        <option value=value.to_string() selected=selected>{label}</option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </div>
                </div>
            </div>

            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::Image)}</h2>
                <div class=style::fields>
                    <input
                        type="hidden"
                        name="image"
                        value=move || image_path.get().unwrap_or_default()
                    />
                    <input
                        type="hidden"
                        name="original_content_type"
                        value=move || image_content_type.get().unwrap_or_default()
                    />
                    <input
                        type="hidden"
                        name="image_source"
                        value=move || image_source.get().to_string()
                    />
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::ImageSource)}</label>
                        <select
                            name="image_source"
                            class=select_class.clone()
                            disabled=move || mode.get() == 0
                            prop:value=move || image_source.get().to_string()
                            on:change=move |e| {
                                if let Ok(parsed) = event_target_value(&e).parse::<i32>() {
                                    image_source.set(parsed);
                                }
                            }
                        >
                            {image_source_items
                                .into_iter()
                                .map(|(value, label)| {
                                    let selected = value == image_source.get_untracked();
                                    view! {
                                        <option value=value.to_string() selected=selected>{label}</option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </div>

                    <div
                        class=style::img_preview_row
                        style:display=move || if image_path.get().is_some() { "" } else { "none" }
                    >
                        <span class=style::field_label>{tr(TextKey::CurrentImage)}</span>
                        <img
                            src=move || {
                                image_path.get()
                                    .map(|p| format!("/artwork/watermarks/{p}"))
                                    .unwrap_or_default()
                            }
                            class=style::img_preview
                            alt=tr(TextKey::WatermarkPreview)
                        />
                    </div>

                    <div class=style::upload_row>
                        <span class=style::field_label>{tr(TextKey::UploadImage)}</span>
                        <input
                            type="file"
                            name="image_file"
                            accept="image/*"
                            class=style::file_input
                            disabled=move || image_source.get() != 0
                            on:change=move |ev| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    use leptos::task::spawn_local;
                                    use wasm_bindgen::JsCast;
                                    let target = ev.target().unwrap();
                                    let input = target
                                        .dyn_into::<web_sys::HtmlInputElement>()
                                        .unwrap();
                                    is_uploading.set(true);
                                    spawn_local(async move {
                                        match upload_file(input).await {
                                            Some((path, ct)) => {
                                                image_path.set(Some(path));
                                                image_content_type.set(Some(ct));
                                            }
                                            None => {
                                                toast.push(
                                                    tr(TextKey::UploadFailed).to_string(),
                                                    ToastSeverity::Error,
                                                );
                                            }
                                        }
                                        is_uploading.set(false);
                                    });
                                }
                                let _ = ev;
                            }
                        />
                    </div>
                </div>
            </div>

            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::Placement)}</h2>
                <div class=style::fields>
                    <input
                        type="hidden"
                        name="location"
                        value=move || location.get().to_string()
                    />
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::Location)}</label>
                        <select
                            name="location"
                            class=select_class.clone()
                            disabled=move || mode.get() == 0
                            prop:value=move || location.get().to_string()
                            on:change=move |e| {
                                if let Ok(parsed) = event_target_value(&e).parse::<i32>() {
                                    location.set(parsed);
                                }
                            }
                        >
                            {location_items
                                .into_iter()
                                .map(|(value, label)| {
                                    let selected = value == location.get_untracked();
                                    view! {
                                        <option value=value.to_string() selected=selected>{label}</option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </div>
                    <input type="hidden" name="size" value=move || size.get().to_string()/>
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::Size)}</label>
                        <select
                            name="size"
                            class=select_class.clone()
                            disabled=move || mode.get() == 0
                            prop:value=move || size.get().to_string()
                            on:change=move |e| {
                                if let Ok(parsed) = event_target_value(&e).parse::<i32>() {
                                    size.set(parsed);
                                }
                            }
                        >
                            {size_items
                                .into_iter()
                                .map(|(value, label)| {
                                    let selected = value == size.get_untracked();
                                    view! {
                                        <option value=value.to_string() selected=selected>{label}</option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </div>
                    <input
                        type="hidden"
                        name="width_percent"
                        value=move || width_percent.get()
                    />
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::WidthPercent)}</label>
                        <input
                            type="text"
                            name="width_percent"
                            class=style::input
                            disabled=move || mode.get() == 0 || size.get() == 1
                            value=width_percent.get_untracked()
                            prop:value=move || width_percent.get()
                            on:input=move |e| width_percent.set(event_target_value(&e))
                        />
                    </div>
                    <input
                        type="hidden"
                        name="horizontal_margin_percent"
                        value=move || horizontal_margin_percent.get()
                    />
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::HorizontalMarginPercent)}</label>
                        <input
                            type="text"
                            name="horizontal_margin_percent"
                            class=style::input
                            disabled=move || mode.get() == 0
                            value=horizontal_margin_percent.get_untracked()
                            prop:value=move || horizontal_margin_percent.get()
                            on:input=move |e| {
                                horizontal_margin_percent.set(event_target_value(&e));
                            }
                        />
                    </div>
                    <input
                        type="hidden"
                        name="vertical_margin_percent"
                        value=move || vertical_margin_percent.get()
                    />
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::VerticalMarginPercent)}</label>
                        <input
                            type="text"
                            name="vertical_margin_percent"
                            class=style::input
                            disabled=move || mode.get() == 0
                            value=vertical_margin_percent.get_untracked()
                            prop:value=move || vertical_margin_percent.get()
                            on:input=move |e| vertical_margin_percent.set(event_target_value(&e))
                        />
                    </div>
                    <input
                        type="hidden"
                        name="place_within_source_content"
                        value=move || place_within_source_content.get().to_string()
                    />
                    <label class=style::checkbox_label>
                        <input
                            type="checkbox"
                            name="place_within_source_content"
                            value="true"
                            class=style::checkbox
                            disabled=move || mode.get() == 0
                            checked=place_within_source_content.get_untracked()
                            prop:checked=move || place_within_source_content.get()
                            on:change=move |e| {
                                place_within_source_content.set(event_target_checked(&e));
                            }
                        />
                        <span class=style::checkbox_text>{tr(TextKey::PlaceWithinSourceContent)}</span>
                    </label>
                </div>
            </div>

            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::IntermittentTiming)}</h2>
                <div class=style::fields>
                    <input
                        type="hidden"
                        name="frequency_minutes"
                        value=move || frequency_minutes.get().to_string()
                    />
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::Frequency)}</label>
                        <select
                            name="frequency_minutes"
                            class=select_class.clone()
                            disabled=move || mode.get() != 2
                            prop:value=move || frequency_minutes.get().to_string()
                            on:change=move |e| {
                                if let Ok(parsed) = event_target_value(&e).parse::<i32>() {
                                    frequency_minutes.set(parsed);
                                }
                            }
                        >
                            {freq_items
                                .into_iter()
                                .map(|(value, label)| {
                                    let selected = value == frequency_minutes.get_untracked();
                                    view! {
                                        <option value=value.to_string() selected=selected>{label}</option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </div>
                    <input
                        type="hidden"
                        name="duration_seconds"
                        value=move || duration_seconds.get().to_string()
                    />
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::DurationSeconds)}</label>
                        <input
                            type="number"
                            name="duration_seconds"
                            min="1"
                            class=style::input
                            disabled=move || mode.get() != 2
                            value=duration_seconds.get_untracked().to_string()
                            prop:value=move || duration_seconds.get().to_string()
                            on:input=move |e| {
                                if let Ok(parsed) = event_target_value(&e).parse::<i64>() {
                                    duration_seconds.set(parsed);
                                }
                            }
                        />
                    </div>
                </div>
            </div>

            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::Opacity)}</h2>
                <div class=style::fields>
                    <input type="hidden" name="opacity" value=move || opacity.get().to_string()/>
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::OpacityOneToHundred)}</label>
                        <input
                            type="number"
                            name="opacity"
                            min="1"
                            max="100"
                            class=style::input
                            disabled=move || mode.get() == 0 || mode.get() == 3
                            value=opacity.get_untracked().to_string()
                            prop:value=move || opacity.get().to_string()
                            on:input=move |e| {
                                if let Ok(parsed) = event_target_value(&e).parse::<i64>() {
                                    opacity.set(parsed);
                                }
                            }
                        />
                    </div>
                    <input
                        type="hidden"
                        name="opacity_expression"
                        value=move || opacity_expression.get()
                    />
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::OpacityExpression)}</label>
                        <input
                            type="text"
                            name="opacity_expression"
                            class=style::input
                            disabled=move || mode.get() != 3
                            value=opacity_expression.get_untracked()
                            prop:value=move || opacity_expression.get()
                            on:input=move |e| opacity_expression.set(event_target_value(&e))
                        />
                    </div>
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::ZIndex)}</label>
                        <input
                            type="number"
                            name="z_index"
                            min="0"
                            class=style::input
                            value=z_index.get_untracked().to_string()
                            prop:value=move || z_index.get().to_string()
                            on:input=move |e| {
                                if let Ok(parsed) = event_target_value(&e).parse::<i64>() {
                                    z_index.set(parsed);
                                }
                            }
                        />
                    </div>
                </div>
            </div>

            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::PlacementPreview)}</h2>
                <Suspense fallback=move || {
                    view! { <p class=style::preview_loading>{tr(TextKey::GeneratingPreview)}</p> }
                }>
                    <img
                        src=move || {
                            preview_resource
                                .get()
                                .flatten()
                                .filter(|url| !url.is_empty())
                                .unwrap_or_default()
                        }
                        class=style::preview_img
                        style:display=move || {
                            let has_preview = preview_resource
                                .get()
                                .flatten()
                                .is_some_and(|url| !url.is_empty());
                            if has_preview { "" } else { "none" }
                        }
                        alt=tr(TextKey::WatermarkPlacementPreview)
                    />
                </Suspense>
            </div>
        </form>
    }
}

/// Uploads a file from an HTML input element to `/api/watermarks/image`.
/// Returns `(path, content_type)` on success, `None` on failure.
#[cfg(target_arch = "wasm32")]
async fn upload_file(input: web_sys::HtmlInputElement) -> Option<(String, String)> {
    use gloo_net::http::Request;
    use web_sys::FormData;

    let files = input.files()?;
    let file = files.get(0)?;
    let content_type = file.type_();

    let form_data = FormData::new().ok()?;
    let blob: &web_sys::Blob = file.as_ref();
    form_data
        .append_with_blob_and_filename("file", blob, &file.name())
        .ok()?;

    let resp = Request::post("/api/watermarks/image")
        .body(form_data)
        .ok()?
        .send()
        .await
        .ok()?;

    let json: serde_json::Value = resp.json().await.ok()?;
    let path = json["path"].as_str()?.to_string();
    let ct = json["content_type"]
        .as_str()
        .unwrap_or(&content_type)
        .to_string();
    Some((path, ct))
}
