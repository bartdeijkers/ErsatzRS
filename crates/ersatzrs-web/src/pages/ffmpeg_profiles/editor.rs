//! `FFmpegProfile` editor — ports `Pages/FFmpegEditor.razor`.
//!
//! - [`FfmpegProfileEditorPage`] — SSR wrapper; loads form data and resolution
//!   list server-side, then mounts the island (or shows "not found").
//! - [`FfmpegProfileEditorIsland`] — fully hydrated editor. Hardware
//!   acceleration section is visible when `normalize_video` is enabled;
//!   VAAPI device/driver/display/pad-mode and QSV extra-frames sub-rows appear
//!   only when the matching accelerator is selected.  "Copy Profile" opens a
//!   `Modal` wired to `copy_ffmpeg_profile`; on success it navigates to the
//!   newly-created profile's editor at `/ffmpeg/profiles/{id}`.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::ffmpeg_profiles::{
    copy_ffmpeg_profile, get_ffmpeg_profile_form, list_resolutions_for_select,
    list_supported_hw_accelerations, presets_for_profile, save_ffmpeg_profile, FfmpegProfileForm,
};
use crate::ui::primitives::dialog::Modal;
use crate::ui::primitives::form::{CheckBox, NumberField, SelectField, TextField};
use crate::ui::primitives::toast::{use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/pages/ffmpeg_profiles/editor.module.css");

// ---------------------------------------------------------------------------
// SSR wrapper
// ---------------------------------------------------------------------------

/// SSR page at `/ffmpeg/profiles/add` and `/ffmpeg/profiles/:id`.
///
/// Fetches the profile form and the resolution list in parallel, then delegates
/// to the fully-hydrated island.
#[component]
pub fn FfmpegProfileEditorPage() -> impl IntoView {
    let params = use_params_map();
    let profile_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    #[allow(clippy::type_complexity)]
    let data: Resource<Option<(FfmpegProfileForm, Vec<(i64, String)>, Vec<(i32, String)>)>> =
        Resource::new(profile_id, |maybe_id| async move {
            let (resolutions, hw_accels) = tokio::join!(
                list_resolutions_for_select(),
                list_supported_hw_accelerations(),
            );
            let resolutions = resolutions.unwrap_or_default();
            let hw_accels = hw_accels.unwrap_or_else(|_| vec![(0, "None".to_string())]);
            let form = match maybe_id {
                None => Some(FfmpegProfileForm::new_defaults()),
                Some(id) => get_ffmpeg_profile_form(id).await.unwrap_or(None),
            };
            form.map(|f| (f, resolutions, hw_accels))
        });

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>"Loading\u{2026}"</p> }>
            {move || {
                data.get().map(|maybe| match maybe {
                    Some((form, resolutions, hw_accels)) => view! {
                        <FfmpegProfileEditorIsland
                            initial_form=form
                            initial_resolutions=resolutions
                            initial_hw_accels=hw_accels
                        />
                    }
                    .into_any(),
                    None => view! {
                        <p class=style::empty>"Profile not found."</p>
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

/// Fully hydrated `FFmpegProfile` editor.
///
/// Hardware accordion visibility is driven by `normalize_video`, not by the
/// selected video format.  Within the accordion, VAAPI-specific fields
/// (device, display, driver, pad mode) appear when `HardwareAcceleration == 3`
/// and QSV extra-hardware-frames appear when `HardwareAcceleration == 1`.
#[island]
fn FfmpegProfileEditorIsland(
    initial_form: FfmpegProfileForm,
    initial_resolutions: Vec<(i64, String)>,
    initial_hw_accels: Vec<(i32, String)>,
) -> impl IntoView {
    let profile_id = initial_form.id;

    // ── Signals ──────────────────────────────────────────────────────────
    let name = RwSignal::new(initial_form.name.clone());
    let thread_count = RwSignal::new(i64::from(initial_form.thread_count));
    let normalize_video = RwSignal::new(initial_form.normalize_video);
    let normalize_audio = RwSignal::new(initial_form.normalize_audio);
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

    let navigate = use_navigate();
    let navigate2 = navigate.clone();
    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(res) = save_action.value().get() {
            match res {
                Ok(_) => navigate("/ffmpeg/profiles", NavigateOptions::default()),
                Err(e) => toast.push(format!("Save failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(res) = copy_action.value().get() {
            match res {
                Ok(new_id) => {
                    toast.push("Profile copied.", ToastSeverity::Success);
                    copy_open.set(false);
                    navigate2(
                        &format!("/ffmpeg/profiles/{new_id}"),
                        NavigateOptions::default(),
                    );
                }
                Err(e) => toast.push(format!("Copy failed: {e}"), ToastSeverity::Error),
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

    // ── Build dispatched form ─────────────────────────────────────────────
    let build_form = move || FfmpegProfileForm {
        id: profile_id,
        name: name.get_untracked(),
        thread_count: i32::try_from(thread_count.get_untracked()).unwrap_or(0),
        normalize_video: normalize_video.get_untracked(),
        normalize_audio: normalize_audio.get_untracked(),
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
            (0, "Hardware If Possible".to_string()),
            (1, "Software".to_string()),
        ]
    };
    let mk_video_format_items = || -> Vec<(i32, String)> {
        vec![
            (0i32, "None".to_string()),
            (1i32, "H264".to_string()),
            (2i32, "H265 (HEVC)".to_string()),
            (3i32, "MPEG-2".to_string()),
            (4i32, "AV1".to_string()),
            (99i32, "Copy".to_string()),
        ]
    };
    let mk_audio_format_items = || -> Vec<(i32, String)> {
        vec![
            (0i32, "None".to_string()),
            (1i32, "AAC".to_string()),
            (2i32, "AC3".to_string()),
            (3i32, "AAC (LATM)".to_string()),
            (99i32, "Copy".to_string()),
        ]
    };

    let page_title = if profile_id == 0 {
        "Add FFmpeg Profile"
    } else {
        "Edit FFmpeg Profile"
    };
    let save_label = if profile_id == 0 {
        "Add Profile"
    } else {
        "Save Profile"
    };

    // ── View ─────────────────────────────────────────────────────────────
    view! {
        <div class=style::page>
            // ── Breadcrumb ────────────────────────────────────────────
            <div class=style::breadcrumb>
                <a href="/ffmpeg/profiles" class=style::back_link>"← FFmpeg Profiles"</a>
                <h1 class=style::page_title>{page_title}</h1>
            </div>

            // ── Action bar ────────────────────────────────────────────
            <div class=style::action_bar>
                <button
                    class=style::btn_primary
                    disabled=move || save_action.pending().get()
                    on:click=move |_| {
                        save_action.dispatch(build_form());
                    }
                >
                    {save_label}
                </button>

                {if profile_id != 0 {
                    view! {
                        <button
                            class=style::btn_secondary
                            on:click=move |_| {
                                copy_name.set(format!("{} (Copy)", name.get_untracked()));
                                copy_open.set(true);
                            }
                        >
                            "Copy Profile"
                        </button>
                        <a href="/ffmpeg/profiles" class=style::cancel_link>"Cancel"</a>
                    }
                    .into_any()
                } else {
                    view! {
                        <a href="/ffmpeg/profiles" class=style::cancel_link>"Cancel"</a>
                    }
                    .into_any()
                }}
            </div>

            // ── General section ───────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>"General"</h2>
                <div class=style::form_grid>
                    <TextField label="Name" value=name/>
                    <NumberField label="Thread Count (0 = auto)" value=thread_count min=Some(0)/>
                </div>
                <SelectField<i64>
                    label="Resolution"
                    value=resolution_id
                    items=initial_resolutions
                />
                <SelectField<i32>
                    label="Scaling Behavior"
                    value=scaling_behavior
                    items=vec![
                        (0i32, "Scale and Pad".to_string()),
                        (1i32, "Stretch".to_string()),
                        (2i32, "Crop".to_string()),
                    ]
                />
                <CheckBox label="Deinterlace Video" value=deinterlace_video/>
                <CheckBox label="Normalize Framerate" value=normalize_framerate/>
                <CheckBox label="Normalize Colors" value=normalize_colors/>
            </div>

            // ── Video section ─────────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>"Video"</h2>
                <CheckBox label="Normalize Video" value=normalize_video/>

                <Show when=move || show_hw_section.get()>
                    {move || view! {
                        <SelectField<i32>
                            label="Video Format"
                            value=video_format
                            items=mk_video_format_items()
                        />

                        // Hardware acceleration sub-panel (always shown when
                        // normalize_video is on; VAAPI/QSV rows conditional within).
                        <div class=style::hw_section>
                            <h3 class=style::subsection_title>"Hardware Acceleration"</h3>
                            <SelectField<i32>
                                label="Acceleration Kind"
                                value=hardware_acceleration
                                items=mk_hw_accel_items()
                            />
                            <Show when=move || show_qsv_frames.get()>
                                {move || view! {
                                    <NumberField
                                        label="QSV Extra Hardware Frames"
                                        value=qsv_extra_hardware_frames
                                        min=Some(0)
                                    />
                                }}
                            </Show>
                            <Show when=move || show_vaapi_fields.get()>
                                {move || view! {
                                    <TextField label="VAAPI Device Path" value=vaapi_device/>
                                    <TextField label="VAAPI Display" value=vaapi_display/>
                                    <SelectField<i32>
                                        label="VAAPI Driver"
                                        value=vaapi_driver
                                        items=mk_vaapi_driver_items()
                                    />
                                    <SelectField<i32>
                                        label="Pad Mode"
                                        value=pad_mode
                                        items=mk_pad_mode_items()
                                    />
                                }}
                            </Show>
                        </div>

                        <div class=style::form_row>
                            <NumberField label="Video Bitrate (kBit/s)" value=video_bitrate min=Some(0)/>
                            <NumberField label="Video Buffer Size (kBit)" value=video_buffer_size min=Some(0)/>
                        </div>
                        <div class=style::form_row>
                            <TextField label="Video Profile" value=video_profile/>
                            // Preset options vary by hw/format/depth (AvailablePresets.ForAccelAndFormat).
                            // Re-rendered reactively; the Effect above clears a stale preset value.
                            {move || {
                                let hw = hardware_acceleration.get();
                                let vf = video_format.get();
                                let bd = bit_depth.get();
                                let mut items: Vec<(String, String)> =
                                    vec![(String::new(), "(None)".to_string())];
                                for p in presets_for_profile(hw, vf, bd) {
                                    items.push((p.to_string(), p.to_string()));
                                }
                                view! {
                                    <SelectField<String>
                                        label="Video Preset"
                                        value=video_preset
                                        items=items
                                    />
                                }
                            }}
                        </div>
                        <div class=style::form_row>
                            <SelectField<i32>
                                label="Bit Depth"
                                value=bit_depth
                                items=vec![
                                    (0i32, "8-bit".to_string()),
                                    (1i32, "10-bit".to_string()),
                                ]
                            />
                            <SelectField<i32>
                                label="Tonemap Algorithm"
                                value=tonemap_algorithm
                                items=vec![
                                    (0i32, "Linear".to_string()),
                                    (1i32, "Clip".to_string()),
                                    (2i32, "Gamma".to_string()),
                                    (3i32, "Reinhard".to_string()),
                                    (4i32, "Mobius".to_string()),
                                    (5i32, "Hable".to_string()),
                                ]
                            />
                        </div>
                        <CheckBox label="Allow B-Frames" value=allow_b_frames/>
                    }}
                </Show>
            </div>

            // ── Audio section ─────────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>"Audio"</h2>
                <CheckBox label="Normalize Audio" value=normalize_audio/>

                <Show when=move || normalize_audio.get()>
                    {move || view! {
                        <SelectField<i32>
                            label="Audio Format"
                            value=audio_format
                            items=mk_audio_format_items()
                        />
                        <div class=style::form_row>
                            <NumberField label="Audio Bitrate (kBit/s)" value=audio_bitrate min=Some(0)/>
                            <NumberField label="Audio Buffer Size (kBit)" value=audio_buffer_size min=Some(0)/>
                        </div>
                        <div class=style::form_row>
                            <NumberField label="Channels" value=audio_channels min=Some(1)/>
                            <NumberField label="Sample Rate (kHz)" value=audio_sample_rate min=Some(0)/>
                        </div>
                        <SelectField<i32>
                            label="Normalize Loudness"
                            value=normalize_loudness_mode
                            items=vec![
                                (0i32, "Off".to_string()),
                                (1i32, "LoudNorm".to_string()),
                            ]
                        />
                        <Show when=move || show_loudness_target.get()>
                            {move || view! {
                                <TextField label="Target Loudness (LUFS)" value=target_loudness/>
                            }}
                        </Show>
                    }}
                </Show>
            </div>

            // ── Copy Profile modal ────────────────────────────────────
            <Modal
                open=copy_open
                title="Copy Profile".to_string()
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        copy_action.dispatch((profile_id, copy_name.get_untracked()));
                    } else {
                        copy_open.set(false);
                    }
                })
                confirm_label="Copy"
            >
                <TextField label="New Profile Name" value=copy_name/>
            </Modal>
        </div>
    }
}
