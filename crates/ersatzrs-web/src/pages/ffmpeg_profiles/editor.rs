//! `FFmpeg` profile editor — ports `Pages/FFmpegEditor.razor`.
//!
//! Two Leptos components:
//! - [`FfmpegProfileEditorPage`] — SSR wrapper that loads the form and mounts
//!   the island (or shows "not found" for unknown ids).
//! - [`FfmpegProfileEditorIsland`] — fully hydrated editor; a `RwSignal<i32>`
//!   for `video_format` drives the conditional hardware accordion; "Copy
//!   Profile" opens a `Modal` with a single name input wired to
//!   `copy_ffmpeg_profile`.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;

use crate::server::ffmpeg_profiles::{
    copy_ffmpeg_profile, get_ffmpeg_profile_form, save_ffmpeg_profile, FfmpegProfileForm,
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
/// Loads profile data server-side, then delegates to the island.
#[component]
pub fn FfmpegProfileEditorPage() -> impl IntoView {
    let params = use_params_map();
    let profile_id = move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()));

    let form_data: Resource<Option<FfmpegProfileForm>> =
        Resource::new(profile_id, |maybe_id| async move {
            match maybe_id {
                None => Some(FfmpegProfileForm::new_defaults()),
                Some(id) => get_ffmpeg_profile_form(id).await.unwrap_or(None),
            }
        });

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>"Loading\u{2026}"</p> }>
            {move || {
                form_data.get().map(|maybe| match maybe {
                    Some(form) => view! { <FfmpegProfileEditorIsland initial_form=form/> }.into_any(),
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

/// Fully hydrated FFmpeg profile editor.
///
/// `video_format` drives the hardware accordion: the section is visible only
/// when the format is H264 (1) or H265 (2). Within the accordion,
/// `hardware_acceleration != 0` reveals the device / driver fields.
#[island]
fn FfmpegProfileEditorIsland(initial_form: FfmpegProfileForm) -> impl IntoView {
    let profile_id = initial_form.id;
    let resolution_id = initial_form.resolution_id;

    // ── Signals ─────────────────────────────────────────────────────────
    let name = RwSignal::new(initial_form.name.clone());
    let thread_count = RwSignal::new(i64::from(initial_form.thread_count));
    let hardware_acceleration = RwSignal::new(initial_form.hardware_acceleration);
    let video_format = RwSignal::new(initial_form.video_format);
    let video_bitrate = RwSignal::new(i64::from(initial_form.video_bitrate));
    let video_buffer_size = RwSignal::new(i64::from(initial_form.video_buffer_size));
    let audio_format = RwSignal::new(initial_form.audio_format);
    let audio_bitrate = RwSignal::new(i64::from(initial_form.audio_bitrate));
    let audio_channels = RwSignal::new(i64::from(initial_form.audio_channels));
    let audio_sample_rate = RwSignal::new(i64::from(initial_form.audio_sample_rate));
    let normalize_loudness_mode = RwSignal::new(initial_form.normalize_loudness_mode);
    let deinterlace_video = RwSignal::new(initial_form.deinterlace_video);
    let vaapi_device = RwSignal::new(initial_form.vaapi_device.clone());
    let vaapi_driver = RwSignal::new(initial_form.vaapi_driver);

    // Copy-profile modal state
    let copy_open = RwSignal::new(false);
    let copy_name = RwSignal::new(String::new());

    // ── Derived visibility ────────────────────────────────────────────────
    // Hardware accordion is shown when VideoFormat is H264 (1) or H265 (2).
    let show_hw_section = Signal::derive(move || {
        let vf = video_format.get();
        vf == 1 || vf == 2
    });
    // Device / driver fields are shown when a hardware accelerator is selected.
    let show_hw_device = Signal::derive(move || hardware_acceleration.get() != 0);

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
                Ok(_) => {
                    toast.push("Profile copied.", ToastSeverity::Success);
                    copy_open.set(false);
                    navigate2("/ffmpeg/profiles", NavigateOptions::default());
                }
                Err(e) => toast.push(format!("Copy failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    // ── Helpers to build the dispatched form ─────────────────────────────
    let build_form = move || FfmpegProfileForm {
        id: profile_id,
        name: name.get_untracked(),
        thread_count: i32::try_from(thread_count.get_untracked()).unwrap_or(0),
        hardware_acceleration: hardware_acceleration.get_untracked(),
        video_format: video_format.get_untracked(),
        video_bitrate: i32::try_from(video_bitrate.get_untracked()).unwrap_or(0),
        video_buffer_size: i32::try_from(video_buffer_size.get_untracked()).unwrap_or(0),
        audio_format: audio_format.get_untracked(),
        audio_bitrate: i32::try_from(audio_bitrate.get_untracked()).unwrap_or(0),
        audio_channels: i32::try_from(audio_channels.get_untracked()).unwrap_or(2),
        audio_sample_rate: i32::try_from(audio_sample_rate.get_untracked()).unwrap_or(48),
        normalize_loudness_mode: normalize_loudness_mode.get_untracked(),
        deinterlace_video: deinterlace_video.get_untracked(),
        vaapi_device: vaapi_device.get_untracked(),
        vaapi_driver: vaapi_driver.get_untracked(),
        resolution_id,
    };

    // ── Item list constructors (called inside Show closures → must be Fn) ──
    // Items are defined as closures rather than Vecs so they can be called
    // inside `<Show>` children (which require `Fn`, not `FnOnce`).
    let mk_hw_accel_items = || -> Vec<(i32, String)> {
        vec![
            (0, "None".to_string()),
            (1, "QSV".to_string()),
            (2, "NVENC".to_string()),
            (3, "VAAPI".to_string()),
            (4, "VideoToolbox".to_string()),
            (5, "AMF".to_string()),
            (6, "V4L2M2M".to_string()),
            (7, "RKMPP".to_string()),
        ]
    };
    let mk_vaapi_driver_items = || -> Vec<(i32, String)> {
        vec![
            (0, "Auto".to_string()),
            (1, "IHD".to_string()),
            (2, "I965".to_string()),
            (3, "RadeonSI".to_string()),
            (4, "Nouveau".to_string()),
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
            // ── Breadcrumb ─────────────────────────────────────────────
            <div class=style::breadcrumb>
                <a href="/ffmpeg/profiles" class=style::back_link>"← FFmpeg Profiles"</a>
                <h1 class=style::page_title>{page_title}</h1>
            </div>

            // ── Action bar ─────────────────────────────────────────────
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

            // ── General section ────────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>"General"</h2>
                <div class=style::form_grid>
                    <TextField label="Name" value=name/>
                    <NumberField label="Thread Count (0 = auto)" value=thread_count min=Some(0)/>
                </div>
                <CheckBox label="Deinterlace Video" value=deinterlace_video/>
            </div>

            // ── Video section ──────────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>"Video"</h2>

                <SelectField<i32>
                    label="Video Format"
                    value=video_format
                    items=vec![
                        (0i32, "None".to_string()),
                        (1i32, "H264".to_string()),
                        (2i32, "H265 (HEVC)".to_string()),
                        (3i32, "MPEG-2".to_string()),
                        (4i32, "AV1".to_string()),
                        (5i32, "Copy".to_string()),
                    ]
                />

                // Hardware accordion — visible only for H264 / H265
                <Show when=move || show_hw_section.get()>
                    {move || view! {
                        <div class=style::hw_section>
                            <h3 class=style::subsection_title>"Hardware Acceleration"</h3>
                            <SelectField<i32>
                                label="Acceleration Kind"
                                value=hardware_acceleration
                                items=mk_hw_accel_items()
                            />
                            // Device + driver — visible when acceleration != None
                            <Show when=move || show_hw_device.get()>
                                {move || view! {
                                    <TextField label="Device Path (VAAPI / QSV)" value=vaapi_device/>
                                    <SelectField<i32>
                                        label="VAAPI Driver"
                                        value=vaapi_driver
                                        items=mk_vaapi_driver_items()
                                    />
                                }}
                            </Show>
                        </div>
                    }}
                </Show>

                <div class=style::form_row>
                    <NumberField label="Video Bitrate (kBit/s)" value=video_bitrate min=Some(0)/>
                    <NumberField label="Video Buffer Size (kBit)" value=video_buffer_size min=Some(0)/>
                </div>
            </div>

            // ── Audio section ──────────────────────────────────────────
            <div class=style::section>
                <h2 class=style::section_title>"Audio"</h2>

                <SelectField<i32>
                    label="Audio Format"
                    value=audio_format
                    items=vec![
                        (0i32, "None".to_string()),
                        (1i32, "AAC".to_string()),
                        (2i32, "AC3".to_string()),
                        (3i32, "AAC (LATM)".to_string()),
                        (4i32, "Copy".to_string()),
                    ]
                />
                <SelectField<i32>
                    label="Normalize Loudness"
                    value=normalize_loudness_mode
                    items=vec![
                        (0i32, "Off".to_string()),
                        (1i32, "LoudNorm".to_string()),
                    ]
                />
                <div class=style::form_row>
                    <NumberField label="Audio Bitrate (kBit/s)" value=audio_bitrate min=Some(0)/>
                    <NumberField label="Channels" value=audio_channels min=Some(1)/>
                </div>
                <NumberField label="Sample Rate (kHz)" value=audio_sample_rate min=Some(0)/>
            </div>

            // ── Copy Profile modal ─────────────────────────────────────
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
