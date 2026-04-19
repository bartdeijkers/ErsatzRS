//! Watermark editor — ports `Pages/WatermarkEditor.razor`.
//!
//! - [`WatermarkEditorPage`] — SSR wrapper; loads form data server-side.
//! - [`WatermarkEditorIsland`] — fully hydrated editor. Conditional sections:
//!   - Image upload/preview shown when `image_source == 0` (Custom).
//!   - Frequency/duration fields shown when `mode == 2` (Intermittent).
//!   - Opacity expression field shown when `mode == 3` (`OpacityExpression`).
//!   - Width/margin/size/location/opacity/placement fields hidden when mode == 0 (None).
//!   - drawvg preview refreshes (debounced 400 ms) whenever placement params change.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::watermarks::{
    get_watermark_form, preview_watermark_placement, save_watermark, WatermarkForm,
    WatermarkPreviewParams,
};
use crate::ui::primitives::form::{CheckBox, NumberField, SelectField, TextField};
use crate::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/pages/watermarks/editor.module.css");

// ---------------------------------------------------------------------------
// SSR wrapper
// ---------------------------------------------------------------------------

#[component]
pub fn WatermarkEditorPage() -> impl IntoView {
    let params = use_params_map();
    let watermark_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    let data: Resource<Option<WatermarkForm>> =
        Resource::new(watermark_id, |maybe_id| async move {
            match maybe_id {
                None => Some(WatermarkForm::default()),
                Some(id) => get_watermark_form(id).await.ok(),
            }
        });

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>"Loading\u{2026}"</p> }>
            {move || {
                data.get().map(|maybe| match maybe {
                    Some(form) => view! {
                        <WatermarkEditorIsland initial_form=form/>
                    }
                    .into_any(),
                    None => view! {
                        <p class=style::empty>"Watermark not found."</p>
                    }
                    .into_any(),
                })
            }}
        </Suspense>
    }
}

// ---------------------------------------------------------------------------
// Island — fully hydrated editor
// ---------------------------------------------------------------------------

#[island]
fn WatermarkEditorIsland(initial_form: WatermarkForm) -> impl IntoView {
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

    // ── Derived visibility ────────────────────────────────────────────────
    let show_placement_fields = Signal::derive(move || mode.get() != 0);
    let show_intermittent = Signal::derive(move || mode.get() == 2);
    let show_opacity_expr = Signal::derive(move || mode.get() == 3);
    let show_custom_image = Signal::derive(move || image_source.get() == 0);

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

    let navigate = use_navigate();
    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(_) => navigate("/watermarks", NavigateOptions::default()),
                Err(e) => toast.push(format!("Save failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    let on_save = move |_| {
        let width = width_percent.get_untracked().parse::<f64>().unwrap_or(15.0);
        let h_margin = horizontal_margin_percent
            .get_untracked()
            .parse::<f64>()
            .unwrap_or(5.0);
        let v_margin = vertical_margin_percent
            .get_untracked()
            .parse::<f64>()
            .unwrap_or(5.0);
        let form = WatermarkForm {
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
        };
        save_action.dispatch(form);
    };

    let is_saving = save_action.pending();
    let is_edit = watermark_id != 0;
    let title = if is_edit {
        "Edit Watermark"
    } else {
        "Add Watermark"
    };
    let btn_label = if is_edit {
        "Save Watermark"
    } else {
        "Add Watermark"
    };

    let mode_items: Vec<(i32, String)> = vec![
        (0, "None".to_string()),
        (1, "Permanent".to_string()),
        (2, "Intermittent".to_string()),
        (3, "Opacity Expression".to_string()),
    ];
    let image_source_items: Vec<(i32, String)> =
        vec![(0, "Custom".to_string()), (1, "Channel Logo".to_string())];
    // Wrap in StoredValue so these Vecs are Copy and can be captured by Fn closures inside Show.
    let location_items = StoredValue::new(vec![
        (0i32, "Bottom Right".to_string()),
        (1i32, "Bottom Left".to_string()),
        (2i32, "Top Right".to_string()),
        (3i32, "Top Left".to_string()),
        (4i32, "Top Middle".to_string()),
        (5i32, "Right Middle".to_string()),
        (6i32, "Bottom Middle".to_string()),
        (7i32, "Left Middle".to_string()),
        (8i32, "Middle Center".to_string()),
    ]);
    let size_items = StoredValue::new(vec![
        (0i32, "Scaled".to_string()),
        (1i32, "Actual Size".to_string()),
    ]);
    let freq_items = StoredValue::new(vec![
        (5i32, "5 minutes".to_string()),
        (10i32, "10 minutes".to_string()),
        (15i32, "15 minutes".to_string()),
        (20i32, "20 minutes".to_string()),
        (30i32, "30 minutes".to_string()),
        (60i32, "60 minutes".to_string()),
    ]);

    view! {
        <div class=style::page>
            <div class=style::breadcrumb>
                <a href="/watermarks" class=style::back_link>"← Watermarks"</a>
                <h1 class=style::page_title>{title}</h1>
            </div>

            <div class=style::action_bar>
                <button
                    class=style::btn_primary
                    disabled=move || is_saving.get()
                    on:click=on_save
                >
                    {btn_label}
                </button>
            </div>

            <div class=style::section>
                <h2 class=style::section_title>"Identity"</h2>
                <div class=style::fields>
                    <TextField label="Name" value=name/>
                    <SelectField<i32> label="Mode" value=mode items=mode_items/>
                </div>
            </div>

            <div class=style::section>
                <h2 class=style::section_title>"Image"</h2>
                <div class=style::fields>
                    <SelectField<i32>
                        label="Image Source"
                        value=image_source
                        items=image_source_items
                    />

                    <Show when=move || show_custom_image.get()>
                        // Image preview (when an image is already uploaded)
                        {move || {
                            image_path
                                .get()
                                .map(|p| {
                                    view! {
                                        <div class=style::img_preview_row>
                                            <span class=style::field_label>"Current image"</span>
                                            <img
                                                src=format!("/artwork/watermarks/{p}")
                                                class=style::img_preview
                                                alt="watermark preview"
                                            />
                                        </div>
                                    }
                                })
                        }}

                        // File upload input
                        <div class=style::upload_row>
                            <span class=style::field_label>"Upload image"</span>
                            <input
                                type="file"
                                accept="image/*"
                                class=style::file_input
                                on:change=move |ev| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        use wasm_bindgen::JsCast;
                                        let target = ev.target().unwrap();
                                        let input = target
                                            .dyn_into::<web_sys::HtmlInputElement>()
                                            .unwrap();
                                        spawn_local(async move {
                                            if let Some((path, ct)) = upload_file(input).await {
                                                image_path.set(Some(path));
                                                image_content_type.set(Some(ct));
                                            }
                                        });
                                    }
                                    let _ = ev;
                                }
                            />
                        </div>
                    </Show>
                </div>
            </div>

            <Show when=move || show_placement_fields.get()>
                <div class=style::section>
                    <h2 class=style::section_title>"Placement"</h2>
                    <div class=style::fields>
                        <SelectField<i32>
                            label="Location"
                            value=location
                            items=location_items.get_value()
                        />
                        <SelectField<i32>
                            label="Size"
                            value=size
                            items=size_items.get_value()
                        />
                        <TextField label="Width (%)" value=width_percent/>
                        <TextField label="Horizontal Margin (%)" value=horizontal_margin_percent/>
                        <TextField label="Vertical Margin (%)" value=vertical_margin_percent/>
                        <NumberField label="Opacity (1–100)" value=opacity min=Some(1) max=Some(100)/>
                        <CheckBox label="Place Within Source Content" value=place_within_source_content/>
                        <NumberField label="Z-Index" value=z_index min=Some(0)/>
                    </div>
                </div>

                // drawvg placement preview
                <div class=style::section>
                    <h2 class=style::section_title>"Placement Preview"</h2>
                    <Suspense fallback=move || {
                        view! { <p class=style::preview_loading>"Generating preview…"</p> }
                    }>
                        {move || {
                            preview_resource
                                .get()
                                .flatten()
                                .filter(|url| !url.is_empty())
                                .map(|url| {
                                    view! {
                                        <img
                                            src=url
                                            class=style::preview_img
                                            alt="watermark placement preview"
                                        />
                                    }
                                })
                        }}
                    </Suspense>
                </div>
            </Show>

            <Show when=move || show_intermittent.get()>
                <div class=style::section>
                    <h2 class=style::section_title>"Intermittent Timing"</h2>
                    <div class=style::fields>
                        <SelectField<i32>
                            label="Frequency"
                            value=frequency_minutes
                            items=freq_items.get_value()
                        />
                        <NumberField
                            label="Duration (seconds)"
                            value=duration_seconds
                            min=Some(1)
                        />
                    </div>
                </div>
            </Show>

            <Show when=move || show_opacity_expr.get()>
                <div class=style::section>
                    <h2 class=style::section_title>"Opacity Expression"</h2>
                    <div class=style::fields>
                        <TextField label="Expression" value=opacity_expression/>
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// Uploads a file from an HTML input element to `/api/watermarks/image`.
/// Returns `(path, content_type)` on success, `None` on failure.
#[cfg(target_arch = "wasm32")]
async fn upload_file(input: web_sys::HtmlInputElement) -> Option<(String, String)> {
    use gloo_net::http::Request;
    use wasm_bindgen::JsCast;
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
