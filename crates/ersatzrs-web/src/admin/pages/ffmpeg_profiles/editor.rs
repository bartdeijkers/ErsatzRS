//! `FFmpegProfile` editor — ports `Pages/FFmpegEditor.razor`.
//!
//! - [`FfmpegProfileEditorPage`] — SSR wrapper; loads form data and resolution
//!   list server-side, then mounts the island (or shows "not found").
//! - [`FfmpegProfileEditorIsland`] — fully hydrated editor. Hardware
//!   acceleration section is visible when `normalize_video` is enabled;
//!   VAAPI device/driver/display/pad-mode and QSV extra-frames sub-rows appear
//!   only when the matching accelerator is selected.  "Copy Profile" opens a
//!   `Modal` wired to `copy_ffmpeg_profile`; on success it navigates to the
//!   newly-created profile's editor at `/ffmpeg/{id}`.
#![allow(clippy::too_many_lines)]
use std::fmt::Write;

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{
    ffmpeg_loudness_mode_label, ffmpeg_profile_error_message, ffmpeg_tonemap_algorithm_label,
    hardware_acceleration_option_text, save_failed, tr, use_i18n, TextKey,
};
use crate::admin::server_fns::ffmpeg_profiles::{
    copy_ffmpeg_profile, get_ffmpeg_profile_form, list_resolutions_for_select,
    list_supported_hw_accelerations, presets_for_profile, save_ffmpeg_profile,
    video_format_validation_error, video_profile_options, FfmpegProfileForm,
    HardwareAccelerationOption, VIDEO_PROFILE_HIGH10, VIDEO_PROFILE_HIGH444P,
};
use crate::admin::ui::primitives::dialog::Modal;
use crate::admin::ui::primitives::form::{CheckBox, NumberField, SelectField, TextField};
use crate::admin::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/ffmpeg_profiles/editor.module.css");

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

/// SSR page at `/ffmpeg/add` and `/ffmpeg/:id`.
///
/// Fetches the profile form and the resolution list in parallel, then delegates
/// to the fully-hydrated island.
#[component]
pub fn FfmpegProfileEditorPage() -> impl IntoView {
    let params = use_params_map();
    let profile_id = params
        .with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(0);
    let none_label = tr(TextKey::NoneOption).to_string();
    let language = use_i18n();
    let no_script_html =
        ffmpeg_profile_no_script_html(load_ffmpeg_profile_no_script_data(profile_id));

    #[allow(clippy::type_complexity)]
    let data: Resource<Option<(FfmpegProfileForm, Vec<(i64, String)>, Vec<(i32, String)>)>> =
        Resource::new_blocking(
            move || profile_id,
            move |id| {
                let none_label = none_label.clone();
                async move {
                    let (resolutions, form) =
                        tokio::join!(list_resolutions_for_select(), async move {
                            if id == 0 {
                                Some(FfmpegProfileForm::new_defaults())
                            } else {
                                get_ffmpeg_profile_form(id).await.unwrap_or(None)
                            }
                        });
                    let resolutions = resolutions.unwrap_or_default();
                    let hw_accels = list_supported_hw_accelerations().await;
                    let hw_accels = hw_accels.map_or_else(
                        |_| vec![(0, none_label)],
                        |options| localized_hw_accel_items(language, options),
                    );
                    form.map(|f| (f, resolutions, hw_accels))
                }
            },
        );

    view! {
        <style>".ffmpeg-profile-noscript { display: none; } .ffmpeg-profile-js { display: block; }"</style>
        <noscript>
            <style>".ffmpeg-profile-noscript { display: block; } .ffmpeg-profile-js { display: none; }"</style>
            <div class="ffmpeg-profile-noscript" inner_html=no_script_html></div>
        </noscript>
        <div class="ffmpeg-profile-js">
            <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                {move || {
                    data.get().map(|maybe| match maybe {
                        Some((form, resolutions, hw_accels)) => {
                            view! {
                                <FfmpegProfileEditorIsland
                                    initial_form=form
                                    initial_resolutions=resolutions
                                    initial_hw_accels=hw_accels
                                />
                            }
                            .into_any()
                        }
                        None => view! {
                            <p class=style::empty>{tr(TextKey::ProfileNotFound)}</p>
                        }
                        .into_any(),
                    })
                }}
            </Suspense>
        </div>
    }
}

fn ffmpeg_profile_no_script_html(data: Option<FfmpegProfileForm>) -> String {
    let Some(form) = data else {
        return format!(
            r#"<p class="{}">{}</p>"#,
            style::empty,
            escape_html(tr(TextKey::ProfileNotFound)),
        );
    };
    let profile_items = video_profile_options(form.hardware_acceleration, form.video_format);
    let save_label = if form.id == 0 {
        tr(TextKey::AddProfile)
    } else {
        tr(TextKey::SaveProfile)
    };
    let mut profile_options = format!(
        r#"<option value=""{}>{}</option>"#,
        selected_attr(form.video_profile.is_empty()),
        escape_html(tr(TextKey::NoneOption)),
    );
    for profile in profile_items {
        let _ = write!(
            profile_options,
            r#"<option value="{value}"{selected}>{label}</option>"#,
            value = escape_html(profile),
            selected = selected_attr(form.video_profile == profile),
            label = escape_html(profile),
        );
    }

    format!(
        r#"<form method="post" action="/api/ffmpeg/save" class="{page}"><input type="hidden" name="id" value="{id}"><div class="{breadcrumb}"><a href="/ffmpeg" class="{back_link}">&#8592; {profiles}</a><h1 class="{page_title}">{title}</h1></div><div class="{action_bar}"><button class="{btn_primary}" type="submit">{save_label}</button><a href="/ffmpeg" class="{cancel_link}">{cancel}</a></div><div class="{section}"><h2 class="{section_title}">{video}</h2><div class="{form_row}"><div class="{fallback_field}"><label>{name_label}</label><input name="name" value="{name}"></div><div class="{fallback_field}"><label>{accel_label}</label><select name="hardware_acceleration">{hardware_options}</select></div><div class="{fallback_field}"><label>{format_label}</label><select name="video_format">{format_options}</select></div><div class="{fallback_field}"><label>{profile_label}</label><select name="video_profile">{profile_options}</select></div><div class="{fallback_field}"><label>{bit_depth_label}</label><select name="bit_depth">{bit_depth_options}</select></div></div></div></form>"#,
        page = style::page,
        id = form.id,
        breadcrumb = style::breadcrumb,
        back_link = style::back_link,
        profiles = escape_html(tr(TextKey::FfmpegProfiles)),
        page_title = style::page_title,
        title = escape_html(tr(TextKey::EditFfmpegProfile)),
        action_bar = style::action_bar,
        btn_primary = style::btn_primary,
        save_label = escape_html(save_label),
        cancel_link = style::cancel_link,
        cancel = escape_html(tr(TextKey::Cancel)),
        section = style::section,
        section_title = style::section_title,
        video = escape_html(tr(TextKey::Video)),
        form_row = style::form_row,
        fallback_field = style::fallback_field,
        name_label = escape_html(tr(TextKey::Name)),
        name = escape_html(&form.name),
        accel_label = escape_html(tr(TextKey::AccelerationKind)),
        hardware_options = hardware_options_html(form.hardware_acceleration),
        format_label = escape_html(tr(TextKey::VideoFormat)),
        format_options = video_format_options_html(form.video_format),
        profile_label = escape_html(tr(TextKey::VideoProfile)),
        profile_options = profile_options,
        bit_depth_label = escape_html(tr(TextKey::BitDepth)),
        bit_depth_options = bit_depth_options_html(form.bit_depth),
    )
}

fn load_ffmpeg_profile_no_script_data(profile_id: i64) -> Option<FfmpegProfileForm> {
    if profile_id == 0 {
        return Some(FfmpegProfileForm::new_defaults());
    }

    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { get_ffmpeg_profile_form(profile_id).await.ok().flatten() })
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = profile_id;
        None
    }
}

fn hardware_options_html(selected: i32) -> String {
    [
        (0, tr(TextKey::NoneOption)),
        (1, "QSV"),
        (2, "NVENC"),
        (3, "VAAPI"),
        (4, "VideoToolbox"),
        (5, "AMF"),
        (7, "RKMPP"),
    ]
    .into_iter()
    .fold(String::new(), |mut html, (value, label)| {
        let _ = write!(
            html,
            r#"<option value="{value}"{}>{}</option>"#,
            selected_attr(selected == value),
            escape_html(label),
        );
        html
    })
}

fn video_format_options_html(selected: i32) -> String {
    [
        (1, "H264"),
        (2, "H265 (HEVC)"),
        (3, "MPEG-2"),
        (4, "AV1"),
        (99, tr(TextKey::Copy)),
    ]
    .into_iter()
    .fold(String::new(), |mut html, (value, label)| {
        let _ = write!(
            html,
            r#"<option value="{value}"{}>{}</option>"#,
            selected_attr(selected == value),
            escape_html(label),
        );
        html
    })
}

fn bit_depth_options_html(selected: i32) -> String {
    [(0, "8-bit"), (1, "10-bit")]
        .into_iter()
        .fold(String::new(), |mut html, (value, label)| {
            let _ = write!(
                html,
                r#"<option value="{value}"{}>{}</option>"#,
                selected_attr(selected == value),
                escape_html(label),
            );
            html
        })
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
}

// ---------------------------------------------------------------------------
// Island — renders concrete SSR data and owns only client interactivity
// ---------------------------------------------------------------------------

#[island]
fn FfmpegProfileEditorIsland(
    initial_form: FfmpegProfileForm,
    initial_resolutions: Vec<(i64, String)>,
    initial_hw_accels: Vec<(i32, String)>,
) -> impl IntoView {
    view! {
        <FfmpegProfileEditorForm
            initial_form=initial_form
            initial_resolutions=initial_resolutions
            initial_hw_accels=initial_hw_accels
        />
    }
}

fn localized_hw_accel_items(
    language: crate::admin::i18n::UiLanguage,
    options: Vec<HardwareAccelerationOption>,
) -> Vec<(i32, String)> {
    options
        .into_iter()
        .map(|option| {
            let label =
                hardware_acceleration_option_text(language, &option.label, option.supported);
            (option.value, label)
        })
        .collect()
}

/// Fully hydrated `FFmpegProfile` editor body.
///
/// Hardware accordion visibility is driven by `normalize_video`, not by the
/// selected video format.  Within the accordion, VAAPI-specific fields
/// (device, display, driver, pad mode) appear when `HardwareAcceleration == 3`
/// and QSV extra-hardware-frames appear when `HardwareAcceleration == 1`.
#[component]
fn FfmpegProfileEditorForm(
    initial_form: FfmpegProfileForm,
    initial_resolutions: Vec<(i64, String)>,
    initial_hw_accels: Vec<(i32, String)>,
) -> impl IntoView {
    let profile_id = initial_form.id;

    // ── Signals ──────────────────────────────────────────────────────────
    let name = RwSignal::new(initial_form.name.clone());
    let thread_count = RwSignal::new(i64::from(initial_form.thread_count));
    let normalize_video = RwSignal::new(initial_form.normalize_video);
    let transcode_audio = RwSignal::new(initial_form.transcode_audio);
    let hardware_acceleration = RwSignal::new(initial_form.hardware_acceleration);
    let vaapi_display = RwSignal::new(initial_form.vaapi_display.clone());
    let vaapi_driver = RwSignal::new(initial_form.vaapi_driver);
    let vaapi_device = RwSignal::new(initial_form.vaapi_device.clone());
    let qsv_extra_hardware_frames =
        RwSignal::new(i64::from(initial_form.qsv_extra_hardware_frames));
    let resolution_id = RwSignal::new(initial_form.resolution_id);
    let scaling_behavior = RwSignal::new(initial_form.scaling_behavior);
    let pad_mode = RwSignal::new(initial_form.pad_mode);
    let video_format = RwSignal::new(initial_form.video_format);
    let video_profile = RwSignal::new(initial_form.video_profile.clone());
    let video_preset = RwSignal::new(initial_form.video_preset.clone());
    let allow_b_frames = RwSignal::new(initial_form.allow_b_frames);
    let bit_depth = RwSignal::new(initial_form.bit_depth);
    let video_bitrate = RwSignal::new(i64::from(initial_form.video_bitrate));
    let video_buffer_size = RwSignal::new(i64::from(initial_form.video_buffer_size));
    let tonemap_algorithm = RwSignal::new(initial_form.tonemap_algorithm);
    let audio_format = RwSignal::new(initial_form.audio_format);
    let audio_bitrate = RwSignal::new(i64::from(initial_form.audio_bitrate));
    let audio_buffer_size = RwSignal::new(i64::from(initial_form.audio_buffer_size));
    let normalize_loudness_mode = RwSignal::new(initial_form.normalize_loudness_mode);
    let target_loudness = RwSignal::new(initial_form.target_loudness.clone());
    let audio_channels = RwSignal::new(i64::from(initial_form.audio_channels));
    let audio_sample_rate = RwSignal::new(i64::from(initial_form.audio_sample_rate));
    let normalize_framerate = RwSignal::new(initial_form.normalize_framerate);
    let normalize_colors = RwSignal::new(initial_form.normalize_colors);
    let deinterlace_video = RwSignal::new(initial_form.deinterlace_video);

    // Copy-profile modal state
    let copy_open = RwSignal::new(false);
    let copy_name = RwSignal::new(String::new());

    // ── Derived visibility ────────────────────────────────────────────────
    // HW accordion: visible when normalize_video is enabled.
    let show_hw_section = Signal::derive(move || normalize_video.get());
    // QSV-specific sub-row: hardware == 1 (QSV).
    let show_qsv_frames =
        Signal::derive(move || normalize_video.get() && hardware_acceleration.get() == 1);
    // VAAPI-specific sub-rows: hardware == 3 (VAAPI).
    let show_vaapi_fields =
        Signal::derive(move || normalize_video.get() && hardware_acceleration.get() == 3);
    // Target-loudness field: loudness mode == 1 (LoudNorm).
    let show_loudness_target = Signal::derive(move || normalize_loudness_mode.get() == 1);
    let video_format_error = Signal::derive(move || {
        normalize_video
            .get()
            .then(|| video_format_validation_error(hardware_acceleration.get(), video_format.get()))
            .flatten()
            .map(ffmpeg_profile_error_message)
    });

    // ── Actions ──────────────────────────────────────────────────────────
    let save_action = Action::new(move |form: &FfmpegProfileForm| {
        let form = form.clone();
        async move { save_ffmpeg_profile(form).await }
    });

    let copy_action = Action::new(|(id, new_name): &(i64, String)| {
        let id = *id;
        let new_name = new_name.clone();
        async move { copy_ffmpeg_profile(id, new_name).await }
    });

    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(_) => set_browser_location("/ffmpeg"),
                Err(e) => toast.push(
                    save_failed(ffmpeg_profile_error_message(e)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(res) = copy_action.value().get() {
            match res {
                Ok(new_id) => {
                    toast.push(
                        tr(TextKey::ProfileCopied).to_owned(),
                        ToastSeverity::Success,
                    );
                    copy_open.set(false);
                    set_browser_location(&format!("/ffmpeg/{new_id}"));
                }
                Err(e) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::CopyFailed),
                        ffmpeg_profile_error_message(e)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    // When NormalizeLoudnessMode changes, mirror the ViewModel setter from
    // FFmpegProfileEditViewModel.cs: enabling LoudNorm sets target to -16,
    // disabling clears it.
    Effect::new(move |prev_mode: Option<i32>| {
        let mode = normalize_loudness_mode.get();
        if let Some(prev) = prev_mode {
            if prev != 1 && mode == 1 {
                target_loudness.set("-16".to_string());
            } else if prev == 1 && mode != 1 {
                target_loudness.set(String::new());
            }
        }
        mode
    });

    // Clear VideoPreset when the hw/format/depth combination changes and the
    // current preset is no longer valid.  Mirrors the server-side clearing in
    // UpdateFFmpegProfileHandler.cs so the island stays in sync.
    Effect::new(move |_| {
        let hw = hardware_acceleration.get();
        let vf = video_format.get();
        let bd = bit_depth.get();
        let valid: Vec<String> = presets_for_profile(hw, vf, bd)
            .into_iter()
            .map(ToString::to_string)
            .collect();
        let current = video_preset.get_untracked();
        if !current.is_empty() && !valid.contains(&current) {
            video_preset.set(String::new());
        }
    });

    // FFmpegProfileEditViewModel.cs exposes a real select for h264 profiles and
    // derives the required 10-bit profile by hardware acceleration.
    Effect::new(move |_| {
        let hw = hardware_acceleration.get();
        let vf = video_format.get();
        let bd = bit_depth.get();
        let options = video_profile_options(hw, vf);
        let current = video_profile.get_untracked();

        if options.is_empty() {
            if !current.is_empty() {
                video_profile.set(String::new());
            }
            return;
        }

        if bd == 1 {
            let required = if hw == 2 {
                VIDEO_PROFILE_HIGH444P
            } else {
                VIDEO_PROFILE_HIGH10
            };
            if current != required {
                video_profile.set(required.to_string());
            }
        } else if !current.is_empty() && !options.contains(&current.as_str()) {
            video_profile.set(String::new());
        }
    });

    // ── Build dispatched form ─────────────────────────────────────────────
    let build_form = move || FfmpegProfileForm {
        id: profile_id,
        name: name.get_untracked(),
        thread_count: i32::try_from(thread_count.get_untracked()).unwrap_or(0),
        normalize_video: normalize_video.get_untracked(),
        transcode_audio: transcode_audio.get_untracked(),
        hardware_acceleration: hardware_acceleration.get_untracked(),
        vaapi_display: vaapi_display.get_untracked(),
        vaapi_driver: vaapi_driver.get_untracked(),
        vaapi_device: vaapi_device.get_untracked(),
        qsv_extra_hardware_frames: i32::try_from(qsv_extra_hardware_frames.get_untracked())
            .unwrap_or(64),
        resolution_id: resolution_id.get_untracked(),
        scaling_behavior: scaling_behavior.get_untracked(),
        pad_mode: pad_mode.get_untracked(),
        video_format: video_format.get_untracked(),
        video_profile: video_profile.get_untracked(),
        video_preset: video_preset.get_untracked(),
        allow_b_frames: allow_b_frames.get_untracked(),
        bit_depth: bit_depth.get_untracked(),
        video_bitrate: i32::try_from(video_bitrate.get_untracked()).unwrap_or(2000),
        video_buffer_size: i32::try_from(video_buffer_size.get_untracked()).unwrap_or(4000),
        tonemap_algorithm: tonemap_algorithm.get_untracked(),
        audio_format: audio_format.get_untracked(),
        audio_bitrate: i32::try_from(audio_bitrate.get_untracked()).unwrap_or(192),
        audio_buffer_size: i32::try_from(audio_buffer_size.get_untracked()).unwrap_or(384),
        normalize_loudness_mode: normalize_loudness_mode.get_untracked(),
        target_loudness: target_loudness.get_untracked(),
        audio_channels: i32::try_from(audio_channels.get_untracked()).unwrap_or(2),
        audio_sample_rate: i32::try_from(audio_sample_rate.get_untracked()).unwrap_or(48),
        normalize_framerate: normalize_framerate.get_untracked(),
        normalize_colors: normalize_colors.get_untracked(),
        deinterlace_video: deinterlace_video.get_untracked(),
    };

    // ── Item list constructors (closures so they can be called inside Show) ─
    // HW acceleration items come from the capability-filtered list loaded by the
    // SSR wrapper (mirrors GetSupportedHardwareAccelerationKindsHandler).
    // StoredValue is Copy, so the closure below is Fn and can be captured by
    // multiple reactive closures in the view.
    let hw_accel_stored = StoredValue::new(initial_hw_accels);
    let mk_hw_accel_items = move || hw_accel_stored.get_value();
    let mk_vaapi_driver_items = || -> Vec<(i32, String)> {
        vec![
            (0, "Auto".to_string()),
            (1, "IHD".to_string()),
            (2, "I965".to_string()),
            (3, "RadeonSI".to_string()),
            (4, "Nouveau".to_string()),
        ]
    };
    let mk_pad_mode_items = || -> Vec<(i32, String)> {
        vec![
            (0, tr(TextKey::HardwareIfPossible).to_string()),
            (1, tr(TextKey::Software).to_string()),
        ]
    };
    let mk_video_format_items = || -> Vec<(i32, String)> {
        vec![
            (0i32, tr(TextKey::NoneOption).to_string()),
            (1i32, "H264".to_string()),
            (2i32, "H265 (HEVC)".to_string()),
            (3i32, "MPEG-2".to_string()),
            (4i32, "AV1".to_string()),
            (99i32, tr(TextKey::Copy).to_string()),
        ]
    };
    let mk_audio_format_items = || -> Vec<(i32, String)> {
        vec![
            (0i32, tr(TextKey::NoneOption).to_string()),
            (1i32, "AAC".to_string()),
            (2i32, "AC3".to_string()),
            (3i32, "AAC (LATM)".to_string()),
            (99i32, tr(TextKey::Copy).to_string()),
        ]
    };

    let page_title = if profile_id == 0 {
        tr(TextKey::AddFfmpegProfile)
    } else {
        tr(TextKey::EditFfmpegProfile)
    };
    let save_label = if profile_id == 0 {
        tr(TextKey::AddProfile)
    } else {
        tr(TextKey::SaveProfile)
    };

    // ── View ─────────────────────────────────────────────────────────────
    view! {
        <form
            method="post"
            action="/api/ffmpeg/save"
            class=style::page
            data-ffmpeg-profile-editor-hydrated="true"
        >
            <input type="hidden" name="id" value=profile_id.to_string()/>
            // ── Breadcrumb ────────────────────────────────────────────
            <div class=style::breadcrumb>
                <a href="/ffmpeg" class=style::back_link>{format!("← {}", tr(TextKey::FfmpegProfiles))}</a>
                <h1 class=style::page_title>{page_title}</h1>
            </div>

            // ── Action bar ────────────────────────────────────────────
            <div class=style::action_bar>
                <button
                    class=style::btn_primary
                    type="submit"
                    disabled=move || save_action.pending().get()
                    on:click=move |_| {
                        save_action.dispatch(build_form());
                    }
                >
                    {save_label}
                </button>

                // "Copy Profile" button — always rendered; hidden for new profiles.
                <button
                    class=style::btn_secondary
                    type="button"
                    style:display=if profile_id != 0 { "" } else { "none" }
                    disabled=profile_id == 0
                    on:click=move |_| {
                        copy_name.set(format!(
                            "{} ({})",
                            name.get_untracked(),
                            tr(TextKey::CopyNameSuffix)
                        ));
                        copy_open.set(true);
                    }
                >
                    {tr(TextKey::CopyProfile)}
                </button>
                <a href="/ffmpeg" class=style::cancel_link>{tr(TextKey::Cancel)}</a>
            </div>

            // ── General section ───────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::General)}</h2>
                <div class=style::form_grid>
                    <TextField label=tr(TextKey::Name) value=name name=Some("name")/>
                    <NumberField label=tr(TextKey::ThreadCountAuto) value=thread_count min=Some(0)/>
                </div>
                <SelectField<i64>
                    label=tr(TextKey::Resolution)
                    value=resolution_id
                    items=initial_resolutions
                />
                <SelectField<i32>
                    label=tr(TextKey::ScalingBehavior)
                    value=scaling_behavior
                    items=vec![
                        (0i32, tr(TextKey::ScaleAndPad).to_string()),
                        (1i32, tr(TextKey::Stretch).to_string()),
                        (2i32, tr(TextKey::Crop).to_string()),
                    ]
                />
                <CheckBox label=tr(TextKey::DeinterlaceVideo) value=deinterlace_video/>
                <CheckBox label=tr(TextKey::NormalizeFramerate) value=normalize_framerate/>
                <CheckBox label=tr(TextKey::NormalizeColors) value=normalize_colors/>
            </div>

            // ── Video section ─────────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::Video)}</h2>
                <CheckBox label=tr(TextKey::NormalizeVideo) value=normalize_video/>

                // Video normalization fields — always rendered; hidden when normalize_video is off.
                // Stable DOM shape avoids hydration mismatches.
                <div
                    class=style::section_fields
                    style:display=move || if show_hw_section.get() { "" } else { "none" }
                >
                    <SelectField<i32>
                        label=tr(TextKey::VideoFormat)
                        value=video_format
                        items=mk_video_format_items()
                        name=Some("video_format")
                        error=Some(video_format_error)
                    />

                    // Hardware acceleration sub-panel.
                    <div class=style::hw_section>
                        <h3 class=style::subsection_title>{tr(TextKey::HardwareAcceleration)}</h3>
                        <SelectField<i32>
                            label=tr(TextKey::AccelerationKind)
                            value=hardware_acceleration
                            items=mk_hw_accel_items()
                            name=Some("hardware_acceleration")
                        />
                        // QSV extra frames — always rendered; hidden for non-QSV accelerators.
                        <div style:display=move || if show_qsv_frames.get() { "" } else { "none" }>
                            <NumberField
                                label=tr(TextKey::QsvExtraHardwareFrames)
                                value=qsv_extra_hardware_frames
                                min=Some(0)
                            />
                        </div>
                        // VAAPI fields — always rendered; hidden for non-VAAPI accelerators.
                        <div
                            class=style::section_fields
                            style:display=move || if show_vaapi_fields.get() { "" } else { "none" }
                        >
                            <TextField label=tr(TextKey::VaapiDevicePath) value=vaapi_device/>
                            <TextField label=tr(TextKey::VaapiDisplay) value=vaapi_display/>
                            <SelectField<i32>
                                label=tr(TextKey::VaapiDriver)
                                value=vaapi_driver
                                items=mk_vaapi_driver_items()
                            />
                            <SelectField<i32>
                                label=tr(TextKey::PadMode)
                                value=pad_mode
                                items=mk_pad_mode_items()
                            />
                        </div>
                    </div>

                    <div class=style::form_row>
                        <NumberField label=tr(TextKey::VideoBitrate) value=video_bitrate min=Some(0)/>
                        <NumberField label=tr(TextKey::VideoBufferSize) value=video_buffer_size min=Some(0)/>
                    </div>
                    <div class=style::form_row>
                        {move || {
                            let hw = hardware_acceleration.get();
                            let vf = video_format.get();
                            let options = video_profile_options(hw, vf);
                            let disabled = options.is_empty();
                            let mut items: Vec<(String, String)> =
                                vec![(String::new(), tr(TextKey::NoneOption).to_string())];
                            for profile in options {
                                items.push((profile.to_string(), profile.to_string()));
                            }
                            view! {
                                <SelectField<String>
                                    label=tr(TextKey::VideoProfile)
                                    value=video_profile
                                    items=items
                                    name=Some("video_profile")
                                    disabled=disabled
                                />
                            }
                        }}
                        // Preset options vary by hw/format/depth (AvailablePresets.ForAccelAndFormat).
                        // Re-rendered reactively; the Effect above clears a stale preset value.
                        {move || {
                            let hw = hardware_acceleration.get();
                            let vf = video_format.get();
                            let bd = bit_depth.get();
                            let mut items: Vec<(String, String)> =
                                vec![(String::new(), tr(TextKey::NoneOption).to_string())];
                            for p in presets_for_profile(hw, vf, bd) {
                                items.push((p.to_string(), p.to_string()));
                            }
                            view! {
                                <SelectField<String>
                                    label=tr(TextKey::VideoPreset)
                                    value=video_preset
                                    items=items
                                />
                            }
                        }}
                    </div>
                    <div class=style::form_row>
                        <SelectField<i32>
                            label=tr(TextKey::BitDepth)
                            value=bit_depth
                            name=Some("bit_depth")
                            items=vec![
                                (0i32, "8-bit".to_string()),
                                (1i32, "10-bit".to_string()),
                            ]
                        />
                        <SelectField<i32>
                            label=tr(TextKey::TonemapAlgorithm)
                            value=tonemap_algorithm
                            items=vec![
                                (0i32, ffmpeg_tonemap_algorithm_label(0)),
                                (1i32, ffmpeg_tonemap_algorithm_label(1)),
                                (2i32, ffmpeg_tonemap_algorithm_label(2)),
                                (3i32, ffmpeg_tonemap_algorithm_label(3)),
                                (4i32, ffmpeg_tonemap_algorithm_label(4)),
                                (5i32, ffmpeg_tonemap_algorithm_label(5)),
                            ]
                        />
                    </div>
                    <CheckBox label=tr(TextKey::AllowBFrames) value=allow_b_frames/>
                </div>
            </div>

            // ── Audio section ─────────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>{tr(TextKey::Audio)}</h2>
                <CheckBox label=tr(TextKey::TranscodeAudio) value=transcode_audio/>

                // Audio transcode fields — always rendered; hidden when transcoding is off.
                <div
                    class=style::section_fields
                    style:display=move || if transcode_audio.get() { "" } else { "none" }
                >
                    <SelectField<i32>
                        label=tr(TextKey::AudioFormat)
                        value=audio_format
                        items=mk_audio_format_items()
                    />
                    <div class=style::form_row>
                        <NumberField label=tr(TextKey::AudioBitrate) value=audio_bitrate min=Some(0)/>
                        <NumberField label=tr(TextKey::AudioBufferSize) value=audio_buffer_size min=Some(0)/>
                    </div>
                    <div class=style::form_row>
                        <NumberField label=tr(TextKey::AudioChannels) value=audio_channels min=Some(1)/>
                        <NumberField label=tr(TextKey::SampleRateKhz) value=audio_sample_rate min=Some(0)/>
                    </div>
                    <SelectField<i32>
                        label=tr(TextKey::NormalizeLoudness)
                        value=normalize_loudness_mode
                        items=vec![
                            (0i32, ffmpeg_loudness_mode_label(0)),
                            (1i32, ffmpeg_loudness_mode_label(1)),
                        ]
                    />
                    // Target loudness — always rendered; hidden when LoudNorm is not selected.
                    <div style:display=move || if show_loudness_target.get() { "" } else { "none" }>
                        <TextField label=tr(TextKey::TargetLoudnessLufs) value=target_loudness/>
                    </div>
                </div>
            </div>

            // ── Copy Profile modal ────────────────────────────────────
            <Modal
                open=copy_open
                title=tr(TextKey::CopyProfile).to_string()
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        copy_action.dispatch((profile_id, copy_name.get_untracked()));
                    } else {
                        copy_open.set(false);
                    }
                })
                confirm_label=Some(tr(TextKey::Copy))
            >
                <TextField label=tr(TextKey::NewProfileName) value=copy_name/>
            </Modal>
        </form>
    }
}
