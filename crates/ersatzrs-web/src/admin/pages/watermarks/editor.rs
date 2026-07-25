//! Watermark editor — ports `Pages/WatermarkEditor.razor`.
//!
//! - [`WatermarkEditorPage`] — SSR wrapper; loads form data server-side.
//! - [`WatermarkEditorIsland`] — fully hydrated editor. Conditional sections:
//!   - Mode-dependent fields stay visible and are disabled to match ErsatzTV.
//!   - The selected asset is composited into a guide frame by `FFmpeg`.
//!   - Previous/next navigation is deterministic and protects dirty edits.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{save_failed, tr, watermark_error_message, TextKey};
use crate::admin::server_fns::watermarks::{
    get_watermark_editor_data, preview_watermark_asset_placement, save_watermark,
    WatermarkAssetPreview, WatermarkAssetPreviewParams, WatermarkEditorData, WatermarkForm,
    WatermarkNavigationItem, WatermarkPreviewParams,
};
use crate::admin::ui::primitives::dialog::Modal;
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
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

    let data = Resource::new_blocking(
        move || watermark_id,
        |id| async move {
            if id == 0 {
                Ok(WatermarkEditorData {
                    form: WatermarkForm::default(),
                    previous: None,
                    next: None,
                })
            } else {
                get_watermark_editor_data(id).await
            }
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                data.get().map(|result| admin_result_view(result, |editor| {
                    view! {
                        <WatermarkEditorIsland initial_data=editor/>
                    }.into_any()
                }))
            }}
        </Suspense>
    }
}

// ---------------------------------------------------------------------------
// Island — owns blocking resource; inner component renders the editor body
// ---------------------------------------------------------------------------

#[island]
fn WatermarkEditorIsland(initial_data: WatermarkEditorData) -> impl IntoView {
    view! {
        <WatermarkEditorForm initial_data=initial_data/>
    }
}

#[component]
fn WatermarkEditorForm(initial_data: WatermarkEditorData) -> impl IntoView {
    let initial_form = initial_data.form;
    let original_form = initial_form.clone();
    let previous_watermark = initial_data.previous;
    let next_watermark = initial_data.next;
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
    let discard_open = RwSignal::new(false);
    let pending_navigation = RwSignal::new(None::<String>);
    let preview_result = RwSignal::new(PreviewLoadState::Waiting);

    // ── real-asset drawvg preview — debounced 400 ms ─────────────────────
    let preview_key: RwSignal<Option<WatermarkAssetPreviewParams>> = RwSignal::new(None);
    let preview_generation = RwSignal::new(0_u64);

    Effect::new(move |_| {
        preview_result.set(PreviewLoadState::Waiting);
        preview_generation.update(|generation| *generation = generation.saturating_add(1));
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
        let source = image_source.get();
        let Some(selected_image) = image_path.get() else {
            preview_key.set(None);
            return;
        };
        if m == 0 || source != 0 {
            preview_key.set(None);
            return;
        }
        let params = WatermarkAssetPreviewParams {
            placement: WatermarkPreviewParams {
                location: loc,
                width_pct: w,
                h_margin: h,
                v_margin: v,
                mode: m,
                size: s,
                opacity: op,
            },
            image_path: selected_image,
        };

        // Only the latest edit may publish a debounced request; older timers
        // self-cancel so a slow stale preview cannot replace the current one.
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            use leptos::task::spawn_local;
            let generation = preview_generation.get_untracked();
            spawn_local(async move {
                TimeoutFuture::new(400).await;
                if preview_generation.get_untracked() == generation {
                    preview_key.set(Some(params));
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        preview_key.set(Some(params));
    });

    let toast = use_toast();
    // The preview is client-requested and its typed result is rendered in the
    // stable preview card rather than being converted into a toast or empty box.
    let preview_resource: LocalResource<Result<Option<WatermarkAssetPreview>, ServerFnError>> =
        LocalResource::new(move || {
            let maybe_params = preview_key.get();
            async move {
                let Some(params) = maybe_params else {
                    return Ok(None);
                };
                preview_watermark_asset_placement(params).await.map(Some)
            }
        });
    Effect::new(move |_| match preview_resource.get() {
        Some(Ok(Some(preview))) => preview_result.set(PreviewLoadState::Ready(preview)),
        Some(Err(error)) => preview_result.set(PreviewLoadState::Failed(
            watermark_error_message(error).into_owned(),
        )),
        Some(Ok(None)) | None => preview_result.set(PreviewLoadState::Waiting),
    });

    // ── Save action ──────────────────────────────────────────────────────
    let save_action = Action::new(move |form: &WatermarkForm| {
        let form = form.clone();
        async move { save_watermark(form).await }
    });

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

    let is_dirty = Signal::derive(move || {
        let width = width_percent.get().parse::<f64>().unwrap_or(15.0);
        let h_margin = horizontal_margin_percent
            .get()
            .parse::<f64>()
            .unwrap_or(5.0);
        let v_margin = vertical_margin_percent.get().parse::<f64>().unwrap_or(5.0);
        let current = WatermarkForm {
            id: watermark_id,
            name: name.get(),
            mode: mode.get(),
            image_source: image_source.get(),
            image: image_path.get(),
            original_content_type: image_content_type.get(),
            location: location.get(),
            size: size.get(),
            width_percent: width,
            horizontal_margin_percent: h_margin,
            vertical_margin_percent: v_margin,
            frequency_minutes: frequency_minutes.get(),
            duration_seconds: i32::try_from(duration_seconds.get()).unwrap_or(15),
            opacity: i32::try_from(opacity.get()).unwrap_or(100),
            place_within_source_content: place_within_source_content.get(),
            opacity_expression: opacity_expression.get(),
            z_index: i32::try_from(z_index.get()).unwrap_or(0),
        };
        watermark_form_is_dirty(&current, &original_form)
    });

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
    let previous_target = previous_watermark
        .as_ref()
        .map(|item| format!("/watermarks/{}", item.id));
    let next_target = next_watermark
        .as_ref()
        .map(|item| format!("/watermarks/{}", item.id));
    let previous_title = previous_watermark.as_ref().map_or_else(
        || tr(TextKey::PreviousWatermark).to_owned(),
        navigation_title,
    );
    let next_title = next_watermark
        .as_ref()
        .map_or_else(|| tr(TextKey::NextWatermark).to_owned(), navigation_title);

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
                <div class=style::navigation_group aria-label=tr(TextKey::WatermarkNavigation)>
                    <button
                        type="button"
                        class=style::btn_secondary
                        disabled=previous_target.is_none()
                        title=previous_title
                        on:click={
                            let target = previous_target.clone();
                            move |_| {
                                if let Some(target) = target.clone() {
                                    if is_dirty.get_untracked() {
                                        pending_navigation.set(Some(target));
                                        discard_open.set(true);
                                    } else {
                                        set_browser_location(&target);
                                    }
                                }
                            }
                        }
                    >
                        {"← "}{tr(TextKey::PreviousWatermark)}
                    </button>
                    <button
                        type="button"
                        class=style::btn_secondary
                        disabled=next_target.is_none()
                        title=next_title
                        on:click={
                            let target = next_target.clone();
                            move |_| {
                                if let Some(target) = target.clone() {
                                    if is_dirty.get_untracked() {
                                        pending_navigation.set(Some(target));
                                        discard_open.set(true);
                                    } else {
                                        set_browser_location(&target);
                                    }
                                }
                            }
                        }
                    >
                        {tr(TextKey::NextWatermark)}{" →"}
                    </button>
                </div>
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
                                            Ok((path, ct)) => {
                                                image_path.set(Some(path));
                                                image_content_type.set(Some(ct));
                                            }
                                            Err(error) => {
                                                toast.push(
                                                    format!("{}: {error}", tr(TextKey::UploadFailed)),
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

            <div class=format!("{} {}", style::section, style::preview_section)>
                <div class=style::preview_heading>
                    <div>
                        <h2 class=style::section_title>{tr(TextKey::PlacementPreview)}</h2>
                        <p class=style::preview_reference>{tr(TextKey::PreviewReferenceFullHd)}</p>
                    </div>
                    <div class=style::guide_legend aria-label=tr(TextKey::AspectRatioGuides)>
                        <span class=style::guide_item>
                            <span class=format!("{} {}", style::guide_swatch, style::guide_four_three)></span>
                            {tr(TextKey::GuideFourThree)}
                        </span>
                    </div>
                </div>
                <div
                    class=style::preview_shell
                    data-state=move || preview_state(
                        mode.get(),
                        image_source.get(),
                        image_path.get().is_some(),
                        preview_key.get().is_some(),
                        &preview_result.get(),
                    )
                >
                    <img
                        src=move || {
                            preview_data(preview_result.get())
                                .map(|preview| preview.data_url)
                                .unwrap_or_default()
                        }
                        class=style::preview_img
                        style:display=move || {
                            let ready = matches!(preview_result.get(), PreviewLoadState::Ready(_));
                            if ready { "" } else { "none" }
                        }
                        data-preview-kind="asset"
                        data-asset-width=move || preview_dimension(preview_result.get(), true)
                        data-asset-height=move || preview_dimension(preview_result.get(), false)
                        data-rendered-width=move || {
                            preview_rendered_dimension(preview_result.get(), true)
                        }
                        data-rendered-height=move || {
                            preview_rendered_dimension(preview_result.get(), false)
                        }
                        alt=tr(TextKey::WatermarkPlacementPreview)
                    />
                    <div
                        class=style::preview_status
                        style:display=move || {
                            let state = preview_state(
                                mode.get(),
                                image_source.get(),
                                image_path.get().is_some(),
                                preview_key.get().is_some(),
                                &preview_result.get(),
                            );
                            if state == "ready" { "none" } else { "" }
                        }
                    >
                        {move || preview_status_text(
                            mode.get(),
                            image_source.get(),
                            image_path.get().is_some(),
                            preview_key.get().is_some(),
                            preview_result.get(),
                        )}
                    </div>
                </div>
                <p
                    class=style::preview_dimensions
                    style:display=move || {
                        let ready = matches!(preview_result.get(), PreviewLoadState::Ready(_));
                        if ready { "" } else { "none" }
                    }
                >
                    {move || {
                        preview_data(preview_result.get())
                            .map(|preview| {
                                format!(
                                    "{}: {}×{} px · {}×{} px",
                                    tr(TextKey::NativeAssetDimensions),
                                    preview.asset_width,
                                    preview.asset_height,
                                    preview.rendered_width,
                                    preview.rendered_height,
                                )
                            })
                            .unwrap_or_default()
                    }}
                </p>
            </div>

            <Modal
                open=discard_open
                title=tr(TextKey::UnsavedChanges).to_owned()
                on_close=Callback::new(move |confirmed| {
                    if confirmed {
                        if let Some(target) = pending_navigation.get_untracked() {
                            set_browser_location(&target);
                        }
                    } else {
                        pending_navigation.set(None);
                    }
                })
                confirm_label=Some(tr(TextKey::DiscardChanges))
                destructive=true
            >
                <p>{tr(TextKey::UnsavedWatermarkChangesMessage)}</p>
            </Modal>
        </form>
    }
}

fn navigation_title(item: &WatermarkNavigationItem) -> String {
    format!("{} — {}", item.name, item.id)
}

fn watermark_form_is_dirty(current: &WatermarkForm, original: &WatermarkForm) -> bool {
    current != original
}

#[derive(Clone, Debug, PartialEq)]
enum PreviewLoadState {
    Waiting,
    Ready(WatermarkAssetPreview),
    Failed(String),
}

fn preview_state(
    mode: i32,
    image_source: i32,
    has_image: bool,
    request_ready: bool,
    result: &PreviewLoadState,
) -> &'static str {
    if mode == 0 || image_source != 0 || !has_image {
        return "unavailable";
    }
    if !request_ready {
        return "loading";
    }
    match result {
        PreviewLoadState::Ready(_) => "ready",
        PreviewLoadState::Failed(_) => "error",
        PreviewLoadState::Waiting => "loading",
    }
}

fn preview_status_text(
    mode: i32,
    image_source: i32,
    has_image: bool,
    request_ready: bool,
    result: PreviewLoadState,
) -> String {
    if mode == 0 {
        return tr(TextKey::EnableWatermarkToPreview).to_owned();
    }
    if image_source != 0 {
        return tr(TextKey::ChannelLogoPreviewUnavailable).to_owned();
    }
    if !has_image {
        return tr(TextKey::SelectCustomImageToPreview).to_owned();
    }
    if !request_ready {
        return tr(TextKey::GeneratingPreview).to_owned();
    }
    match result {
        PreviewLoadState::Failed(error) => {
            format!("{}: {error}", tr(TextKey::PreviewFailed))
        }
        _ => tr(TextKey::GeneratingPreview).to_owned(),
    }
}

fn preview_data(result: PreviewLoadState) -> Option<WatermarkAssetPreview> {
    match result {
        PreviewLoadState::Ready(preview) => Some(preview),
        PreviewLoadState::Waiting | PreviewLoadState::Failed(_) => None,
    }
}

fn preview_dimension(result: PreviewLoadState, width: bool) -> String {
    preview_data(result)
        .map(|preview| {
            if width {
                preview.asset_width
            } else {
                preview.asset_height
            }
            .to_string()
        })
        .unwrap_or_default()
}

fn preview_rendered_dimension(result: PreviewLoadState, width: bool) -> String {
    preview_data(result)
        .map(|preview| {
            if width {
                preview.rendered_width
            } else {
                preview.rendered_height
            }
            .to_string()
        })
        .unwrap_or_default()
}

/// Uploads a file from an HTML input element to `/api/watermarks/image`.
/// Returns `(path, content_type)` on success.
#[cfg(target_arch = "wasm32")]
async fn upload_file(input: web_sys::HtmlInputElement) -> Result<(String, String), String> {
    use gloo_net::http::Request;
    use web_sys::FormData;

    let files = input
        .files()
        .ok_or_else(|| "file input is unavailable".to_owned())?;
    let file = files
        .get(0)
        .ok_or_else(|| "no file was selected".to_owned())?;
    let content_type = file.type_();

    let form_data = FormData::new().map_err(|error| format!("{error:?}"))?;
    let blob: &web_sys::Blob = file.as_ref();
    form_data
        .append_with_blob_and_filename("file", blob, &file.name())
        .map_err(|error| format!("{error:?}"))?;

    let resp = Request::post("/api/watermarks/image")
        .body(form_data)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;

    let json: serde_json::Value = resp.json().await.map_err(|error| error.to_string())?;
    let path = json["path"]
        .as_str()
        .ok_or_else(|| "upload response has no path".to_owned())?
        .to_string();
    let ct = json["content_type"]
        .as_str()
        .unwrap_or(&content_type)
        .to_string();
    Ok((path, ct))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_preview() -> WatermarkAssetPreview {
        WatermarkAssetPreview {
            data_url: "data:image/png;base64,test".to_owned(),
            asset_width: 600,
            asset_height: 300,
            rendered_width: 100,
            rendered_height: 50,
        }
    }

    #[test]
    fn dirty_form_protection_compares_every_serialized_field() {
        let original = WatermarkForm::default();
        assert!(!watermark_form_is_dirty(&original, &original));

        let mut changed = original.clone();
        changed.vertical_margin_percent = 12.5;
        assert!(watermark_form_is_dirty(&changed, &original));

        changed = original.clone();
        changed.image = Some("replacement.png".to_owned());
        assert!(watermark_form_is_dirty(&changed, &original));
    }

    #[test]
    fn preview_states_distinguish_unavailable_loading_ready_and_failure() {
        assert_eq!(
            preview_state(0, 0, true, true, &PreviewLoadState::Waiting),
            "unavailable"
        );
        assert_eq!(
            preview_state(1, 1, true, true, &PreviewLoadState::Waiting),
            "unavailable"
        );
        assert_eq!(
            preview_state(1, 0, false, true, &PreviewLoadState::Waiting),
            "unavailable"
        );
        assert_eq!(
            preview_state(1, 0, true, false, &PreviewLoadState::Waiting),
            "loading"
        );
        assert_eq!(
            preview_state(1, 0, true, true, &PreviewLoadState::Ready(ready_preview())),
            "ready"
        );
        assert_eq!(
            preview_state(
                1,
                0,
                true,
                true,
                &PreviewLoadState::Failed("missing asset".to_owned())
            ),
            "error"
        );
    }
}
