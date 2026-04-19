//! Watermark editor — ports `Pages/WatermarkEditor.razor`.
//!
//! - [`WatermarkEditorPage`] — SSR wrapper; loads form data server-side.
//! - [`WatermarkEditorIsland`] — fully hydrated editor. Conditional sections:
//!   - Image upload/preview shown when `image_source == 0` (Custom).
//!   - Frequency/duration fields shown when `mode == 2` (Intermittent).
//!   - Opacity expression field shown when `mode == 3` (`OpacityExpression`).
//!   - Width/margin/size/location/opacity/placement fields hidden when mode == 0 (None).
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::watermarks::{get_watermark_form, save_watermark, WatermarkForm};
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
    let location = RwSignal::new(initial_form.location);
    let size = RwSignal::new(initial_form.size);
    let width_percent = RwSignal::new(initial_form.width_percent.to_string());
    let horizontal_margin_percent =
        RwSignal::new(initial_form.horizontal_margin_percent.to_string());
    let vertical_margin_percent = RwSignal::new(initial_form.vertical_margin_percent.to_string());
    let frequency_minutes = RwSignal::new(i64::from(initial_form.frequency_minutes));
    let duration_seconds = RwSignal::new(i64::from(initial_form.duration_seconds));
    let opacity = RwSignal::new(i64::from(initial_form.opacity));
    let place_within_source_content = RwSignal::new(initial_form.place_within_source_content);
    let opacity_expression = RwSignal::new(initial_form.opacity_expression.clone());
    let z_index = RwSignal::new(i64::from(initial_form.z_index));

    // ── Derived visibility ────────────────────────────────────────────────
    let show_placement_fields = Signal::derive(move || mode.get() != 0);
    let show_intermittent = Signal::derive(move || mode.get() == 2);
    let show_opacity_expr = Signal::derive(move || mode.get() == 3);

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
        let width = width_percent.get_untracked().parse::<f64>().unwrap_or(10.0);
        let h_margin = horizontal_margin_percent
            .get_untracked()
            .parse::<f64>()
            .unwrap_or(2.0);
        let v_margin = vertical_margin_percent
            .get_untracked()
            .parse::<f64>()
            .unwrap_or(2.0);
        let form = WatermarkForm {
            id: watermark_id,
            name: name.get_untracked(),
            mode: mode.get_untracked(),
            image_source: image_source.get_untracked(),
            image: None,
            original_content_type: None,
            location: location.get_untracked(),
            size: size.get_untracked(),
            width_percent: width,
            horizontal_margin_percent: h_margin,
            vertical_margin_percent: v_margin,
            frequency_minutes: i32::try_from(frequency_minutes.get_untracked()).unwrap_or(5),
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
                    <p class=style::hint>
                        "Image upload is available after saving (edit mode). "
                        "Channel Logo uses the channel's configured logo."
                    </p>
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
                        <NumberField label="Opacity (0–100)" value=opacity min=Some(0) max=Some(100)/>
                        <CheckBox label="Place Within Source Content" value=place_within_source_content/>
                        <NumberField label="Z-Index" value=z_index min=Some(0)/>
                    </div>
                </div>
            </Show>

            <Show when=move || show_intermittent.get()>
                <div class=style::section>
                    <h2 class=style::section_title>"Intermittent Timing"</h2>
                    <div class=style::fields>
                        <NumberField label="Frequency (minutes)" value=frequency_minutes min=Some(1) max=Some(3600)/>
                        <NumberField label="Duration (seconds)" value=duration_seconds min=Some(1) max=Some(3600)/>
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
