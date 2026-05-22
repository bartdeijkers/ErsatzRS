#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::server_fns::channels::{
    get_channel_editor_data, ChannelEditorData, ChannelMirrorOption, ChannelSelectOption,
};

stylance::import_crate_style!(style, "src/admin/pages/channels/editor.module.css");

// Render this editor as an SSR form. Other admin islands still have an app-wide
// Leptos marker mismatch; keeping this route server-rendered gives the channel
// save path full no-JS coverage while that broader hydration issue is handled.
#[component]
pub fn ChannelEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let channel_id =
        move || params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    let form_error = query.with_untracked(|params| {
        let raw = params.get("error").unwrap_or_default();
        match raw.as_str() {
            "" => String::new(),
            "save" => {
                "Channel could not be saved. Check the form values and try again.".to_string()
            }
            "upload" => "Channel logo upload failed. Check the file and try again.".to_string(),
            message => message.to_string(),
        }
    });

    let data: Resource<ChannelEditorData> = Resource::new_blocking(channel_id, |id| async move {
        get_channel_editor_data(id).await.unwrap_or_default()
    });

    view! {
        <Suspense fallback=move || view! { <p>"Loading..."</p> }>
            {move || {
                let form_error = form_error.clone();
                data.get()
                    .map(|initial_data| view! {
                        <ChannelEditorForm initial_data=initial_data form_error=form_error/>
                    })
            }}
        </Suspense>
    }
}

#[component]
fn ChannelEditorForm(initial_data: ChannelEditorData, form_error: String) -> impl IntoView {
    channel_editor_form(initial_data, form_error)
}

fn channel_editor_form(initial_data: ChannelEditorData, form_error: String) -> impl IntoView {
    let initial_form = initial_data.form;
    let channel_id = initial_form.id;
    let playout_source_locked =
        channel_id != 0 && initial_data.playout_count > 0 && initial_form.playout_source == 0;

    let number = RwSignal::new(initial_form.number);
    let name = RwSignal::new(initial_form.name);
    let group = RwSignal::new(initial_form.group);
    let categories = RwSignal::new(initial_form.categories);
    let transcode_mode = RwSignal::new(initial_form.transcode_mode);
    let idle_behavior = RwSignal::new(initial_form.idle_behavior);
    let playout_source = RwSignal::new(initial_form.playout_source);
    let playout_mode = RwSignal::new(initial_form.playout_mode);
    let mirror_source_channel_id = RwSignal::new(initial_form.mirror_source_channel_id);
    let playout_offset_hours = RwSignal::new(initial_form.playout_offset_hours);
    let is_enabled = RwSignal::new(initial_form.is_enabled);
    let show_in_epg = RwSignal::new(initial_form.show_in_epg);
    let streaming_mode = RwSignal::new(initial_form.streaming_mode);
    let ffmpeg_profile_id = RwSignal::new(initial_form.ffmpeg_profile_id);
    let slug_seconds = RwSignal::new(
        initial_form
            .slug_seconds
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let stream_selector_mode = RwSignal::new(initial_form.stream_selector_mode);
    let stream_selector = RwSignal::new(initial_form.stream_selector);
    let preferred_audio_language_code = RwSignal::new(initial_form.preferred_audio_language_code);
    let preferred_audio_title = RwSignal::new(initial_form.preferred_audio_title);
    let preferred_subtitle_language_code =
        RwSignal::new(initial_form.preferred_subtitle_language_code);
    let subtitle_mode = RwSignal::new(initial_form.subtitle_mode);
    let music_video_credits_mode = RwSignal::new(initial_form.music_video_credits_mode);
    let music_video_credits_template = RwSignal::new(initial_form.music_video_credits_template);
    let song_video_mode = RwSignal::new(initial_form.song_video_mode);
    let logo_path = RwSignal::new(initial_form.logo_path);
    let logo_content_type = RwSignal::new(initial_form.logo_content_type);
    let external_logo_url = RwSignal::new(initial_form.external_logo_url);
    let watermark_id = RwSignal::new(initial_form.watermark_id);
    let fallback_filler_id = RwSignal::new(initial_form.fallback_filler_id);

    let page_title = if channel_id != 0 {
        "Edit Channel"
    } else {
        "Add Channel"
    };

    let save_label = if channel_id != 0 {
        "Save Channel"
    } else {
        "Add Channel"
    };
    let has_form_error = !form_error.is_empty();

    let mirror_channels = initial_data.mirror_channels;
    let mirror_options =
        Signal::derive(move || mirror_channel_options(&mirror_channels, channel_id));
    let profile_options: Vec<(i64, String)> = initial_data
        .ffmpeg_profiles
        .iter()
        .map(|option| (option.id, option.label.clone()))
        .collect();
    let has_profiles = !profile_options.is_empty();
    let profile_items = Signal::derive(move || profile_options.clone());
    let language_options = language_options(&initial_data.language_codes);
    let language_items = Signal::derive(move || language_options.clone());
    let watermark_options: Vec<(i64, String)> = initial_data
        .watermarks
        .iter()
        .map(|option| (option.id, option.label.clone()))
        .collect();
    let watermark_items = Signal::derive(move || watermark_options.clone());
    let filler_options: Vec<(i64, String)> = initial_data
        .fallback_fillers
        .iter()
        .map(|option| (option.id, option.label.clone()))
        .collect();
    let filler_items = Signal::derive(move || filler_options.clone());
    let stream_selector_options: Vec<(String, String)> = initial_data
        .stream_selectors
        .iter()
        .map(|value| (value.clone(), value.clone()))
        .collect();
    let stream_selector_items = Signal::derive(move || stream_selector_options.clone());
    let music_video_template_options: Vec<(String, String)> = initial_data
        .music_video_credit_templates
        .iter()
        .map(|value| (value.clone(), value.clone()))
        .collect();
    let music_video_template_items = Signal::derive(move || music_video_template_options.clone());

    let is_hls_direct = Signal::derive(move || streaming_mode.get() == 2);
    let logo_preview = Signal::derive(move || {
        let external = external_logo_url.get();
        if !external.trim().is_empty() {
            return external;
        }

        let path = logo_path.get();
        if let Some(path) = path.strip_prefix('/') {
            format!("/{path}")
        } else if let Some(path) = path.strip_prefix("iptv/logos/") {
            format!("/iptv/logos/{path}")
        } else {
            path
        }
    });

    view! {
        <div class=style::page>
            <div class=style::breadcrumb>
                <a href="/channels" class=style::back_link>"Channels"</a>
                <h1 class=style::page_title inner_html=page_title></h1>
            </div>

            <form method="post" action="/api/channels/save-multipart" enctype="multipart/form-data">
                <input type="hidden" name="id" value=channel_id.to_string()/>
                <input type="hidden" name="logo_path" value=logo_path.get_untracked()/>
                <input type="hidden" name="logo_content_type" value=logo_content_type.get_untracked()/>

                <div class=style::card>
                    <p class=style::form_error hidden=!has_form_error>
                        {form_error}
                    </p>

                    {text_field("Number", "number", number, "")}
                    {text_field("Name", "name", name, "")}
                    {text_field("Group", "group", group, "")}
                    {text_field(
                        "Categories",
                        "categories",
                        categories,
                        "Comma-separated list of categories",
                    )}

                    {select_i32_field(
                        "Transcode Mode",
                        "transcode_mode",
                        transcode_mode,
                        Signal::derive(|| vec![(0, "On Demand".to_string())]),
                        Some(Signal::derive(|| true)),
                        "Controls when the transcoding process is active",
                    )}
                    {select_i32_field(
                        "Idle Behavior",
                        "idle_behavior",
                        idle_behavior,
                        Signal::derive(|| vec![
                            (0, "Stop On Disconnect".to_string()),
                            (1, "Keep Running".to_string()),
                        ]),
                        None,
                        "Controls how the transcoding process behaves after all clients have disconnected",
                    )}

                    {select_i32_field(
                        "Playout Source",
                        "playout_source",
                        playout_source,
                        Signal::derive(|| vec![
                            (0, "Generated".to_string()),
                            (1, "Mirror".to_string()),
                        ]),
                        Some(Signal::derive(move || playout_source_locked)),
                        "Controls the source of the channel's content. This cannot be changed when the channel already has a playout.",
                    )}

                    <div>
                        {select_i32_field(
                            "Playout Mode",
                            "playout_mode",
                            playout_mode,
                            Signal::derive(|| vec![
                                (0, "Continuous".to_string()),
                                (1, "On Demand".to_string()),
                            ]),
                            None,
                            "Controls the progression of the channel's playout",
                        )}
                    </div>

                    <div>
                        {select_i64_field(
                            "Mirror Source Channel",
                            "mirror_source_channel_id",
                            mirror_source_channel_id,
                            mirror_options,
                            "(none)",
                            None,
                        )}
                        {number_field("Playout Offset", "playout_offset_hours", playout_offset_hours, "hours")}
                    </div>

                    <div class=style::checkboxes>
                        {checkbox_field("Is Enabled", "is_enabled", is_enabled)}
                        {checkbox_field("Show In EPG", "show_in_epg", show_in_epg)}
                    </div>

                    {select_i32_field(
                        "Streaming Mode",
                        "streaming_mode",
                        streaming_mode,
                        Signal::derive(|| vec![
                            (5, "MPEG-TS".to_string()),
                            (1, "MPEG-TS (Legacy)".to_string()),
                            (2, "HLS Direct".to_string()),
                            (4, "HLS Segmenter".to_string()),
                        ]),
                        None,
                        "",
                    )}

                    {select_i64_field(
                        "FFmpeg Profile",
                        "ffmpeg_profile_id",
                        ffmpeg_profile_id,
                        profile_items,
                        "- Select profile -",
                        Some(is_hls_direct),
                    )}
                    <p class=style::no_profiles hidden=has_profiles>
                        "No FFmpeg profiles found. Create one first."
                    </p>

                    {select_string_field(
                        "Slug Seconds",
                        "slug_seconds",
                        slug_seconds,
                        Signal::derive(|| vec![
                            (String::new(), "(none)".to_string()),
                            ("0.5".to_string(), "0.5 seconds".to_string()),
                            ("1".to_string(), "1 second".to_string()),
                            ("2".to_string(), "2 seconds".to_string()),
                            ("3".to_string(), "3 seconds".to_string()),
                            ("5".to_string(), "5 seconds".to_string()),
                        ]),
                        "",
                        None,
                        "Seconds of black video and silent audio to insert between every playout item",
                    )}

                    {select_i32_field(
                        "Stream Selector Mode",
                        "stream_selector_mode",
                        stream_selector_mode,
                        Signal::derive(|| vec![
                            (0, "Default".to_string()),
                            (1, "Custom".to_string()),
                        ]),
                        None,
                        "",
                    )}

                    <div>
                        {select_string_field(
                            "Preferred Audio Language",
                            "preferred_audio_language_code",
                            preferred_audio_language_code,
                            language_items,
                            "(none)",
                            None,
                            "",
                        )}
                        {text_field("Preferred Audio Title", "preferred_audio_title", preferred_audio_title, "")}
                        {select_string_field(
                            "Preferred Subtitle Language",
                            "preferred_subtitle_language_code",
                            preferred_subtitle_language_code,
                            language_items,
                            "(none)",
                            None,
                            "",
                        )}
                        {select_i32_field(
                            "Subtitle Mode",
                            "subtitle_mode",
                            subtitle_mode,
                            Signal::derive(|| vec![
                                (0, "None".to_string()),
                                (1, "Forced".to_string()),
                                (2, "Default".to_string()),
                                (3, "Any".to_string()),
                            ]),
                            None,
                            "",
                        )}
                    </div>

                    <div>
                        {select_string_field(
                            "Stream Selector",
                            "stream_selector",
                            stream_selector,
                            stream_selector_items,
                            "(none)",
                            None,
                            "",
                        )}
                    </div>

                    {select_i32_field(
                        "Music Video Credits Mode",
                        "music_video_credits_mode",
                        music_video_credits_mode,
                        Signal::derive(|| vec![
                            (0, "None".to_string()),
                            (1, "Generate Subtitles".to_string()),
                        ]),
                        None,
                        "",
                    )}
                    {select_string_field(
                        "Music Video Credits Template",
                        "music_video_credits_template",
                        music_video_credits_template,
                        music_video_template_items,
                        "(none)",
                        None,
                        "",
                    )}
                    {select_i32_field(
                        "Song Video Mode",
                        "song_video_mode",
                        song_video_mode,
                        Signal::derive(|| vec![
                            (0, "Default".to_string()),
                            (1, "With Progress".to_string()),
                        ]),
                        None,
                        "",
                    )}

                    <div class=style::field>
                        <label class=style::label for="channel-logo-file">"Upload Logo"</label>
                        <input
                            id="channel-logo-file"
                            name="logo_file"
                            type="file"
                            accept="image/*"
                            class=style::input
                        />
                        <span class=style::helper>
                            "Choose a local image to replace the stored channel logo on save"
                        </span>
                        <span class=style::error></span>
                    </div>

                    {text_field("External Logo URL", "external_logo_url", external_logo_url, "")}

                    <div class=style::field>
                        <label class=style::label>"Logo Preview"</label>
                        <img
                            class=style::logo_preview
                            src=move || logo_preview.get()
                            alt="Channel logo preview"
                            style:display=move || if logo_preview.get().trim().is_empty() { "none" } else { "" }
                        />
                    </div>

                    <div class=style::checkboxes>
                        {checkbox_field("Remove Stored Logo", "remove_logo", RwSignal::new(false))}
                    </div>

                    {select_i64_field(
                        "Watermark",
                        "watermark_id",
                        watermark_id,
                        watermark_items,
                        "(none)",
                        Some(is_hls_direct),
                    )}

                    {select_i64_field(
                        "Fallback Filler",
                        "fallback_filler_id",
                        fallback_filler_id,
                        filler_items,
                        "(none)",
                        None,
                    )}
                </div>

                <div class=style::form_actions>
                    <button
                        type="submit"
                        class=style::btn_primary
                    >
                        {save_label}
                    </button>
                    <a href="/channels" class=style::cancel_link>"Cancel"</a>
                </div>
            </form>
        </div>
    }
}

fn mirror_channel_options(options: &[ChannelMirrorOption], channel_id: i64) -> Vec<(i64, String)> {
    options
        .iter()
        .filter(|option| option.id != channel_id && option.playout_source == 0)
        .map(|option| (option.id, format!("({}) - {}", option.number, option.name)))
        .collect()
}

fn language_options(options: &[ChannelSelectOption]) -> Vec<(String, String)> {
    let mut result = vec![(String::new(), "(none)".to_string())];
    result.extend(
        options
            .iter()
            .map(|option| (option.value.clone(), option.label.clone())),
    );
    result
}

fn text_field(
    label: &'static str,
    name: &'static str,
    value: RwSignal<String>,
    helper: &'static str,
) -> impl IntoView {
    view! {
        <div class=style::field>
            <label class=style::label inner_html=label></label>
            <input
                name=name
                type="text"
                class=style::input
                value=value.get_untracked()
                on:input=move |e| value.set(event_target_value(&e))
            />
            <span class=style::helper inner_html=helper></span>
            <span class=style::error></span>
        </div>
    }
}

fn number_field(
    label: &'static str,
    name: &'static str,
    value: RwSignal<i32>,
    suffix: &'static str,
) -> impl IntoView {
    view! {
        <div class=style::field>
            <label class=style::label inner_html=label></label>
            <div class=style::number_row>
                <input
                    name=name
                    type="number"
                    class=style::input
                    value=value.get_untracked().to_string()
                    on:input=move |e| {
                        if let Ok(parsed) = event_target_value(&e).parse::<i32>() {
                            value.set(parsed);
                        }
                    }
                />
                <span class=style::suffix inner_html=suffix></span>
            </div>
            <span class=style::error></span>
        </div>
    }
}

fn checkbox_field(label: &'static str, name: &'static str, value: RwSignal<bool>) -> impl IntoView {
    view! {
        <label class=style::checkbox_label>
            <input
                name=name
                type="checkbox"
                value="true"
                class=style::checkbox
                checked=value.get_untracked()
                on:change=move |e| value.set(event_target_checked(&e))
            />
            <span class=style::checkbox_text inner_html=label></span>
        </label>
    }
}

fn select_i32_field(
    label: &'static str,
    name: &'static str,
    value: RwSignal<i32>,
    options: Signal<Vec<(i32, String)>>,
    disabled: Option<Signal<bool>>,
    helper: &'static str,
) -> impl IntoView {
    let select_class = format!("{} {}", style::input, style::select);
    let disabled = disabled.unwrap_or_else(|| Signal::derive(|| false));
    let disabled_initial = disabled.get_untracked();
    view! {
        <div class=style::field>
            <label class=style::label inner_html=label></label>
            {disabled_initial.then(|| {
                view! {
                    <input
                        type="hidden"
                        name=name
                        value=value.get_untracked().to_string()
                    />
                }
            })}
            <select
                name=name
                class=select_class
                disabled=move || disabled.get()
                on:change=move |e| {
                    if let Ok(parsed) = event_target_value(&e).parse::<i32>() {
                        value.set(parsed);
                    }
                }
            >
                {move || {
                    options
                        .get()
                        .into_iter()
                        .map(|(option_value, label)| {
                            view! {
                                <option
                                    value=option_value.to_string()
                                    selected=option_value == value.get_untracked()
                                >
                                    {label}
                                </option>
                            }
                        })
                        .collect_view()
                }}
            </select>
            <span class=style::helper inner_html=helper></span>
            <span class=style::error></span>
        </div>
    }
}

fn select_i64_field(
    label: &'static str,
    name: &'static str,
    value: RwSignal<i64>,
    options: Signal<Vec<(i64, String)>>,
    none_label: &'static str,
    disabled: Option<Signal<bool>>,
) -> impl IntoView {
    let select_class = format!("{} {}", style::input, style::select);
    let disabled = disabled.unwrap_or_else(|| Signal::derive(|| false));
    let disabled_initial = disabled.get_untracked();
    view! {
        <div class=style::field>
            <label class=style::label inner_html=label></label>
            {disabled_initial.then(|| {
                view! {
                    <input
                        type="hidden"
                        name=name
                        value=value.get_untracked().to_string()
                    />
                }
            })}
            <select
                name=name
                class=select_class
                disabled=move || disabled.get()
                on:change=move |e| {
                    if let Ok(parsed) = event_target_value(&e).parse::<i64>() {
                        value.set(parsed);
                    }
                }
            >
                <option value="0" selected=value.get_untracked() == 0 inner_html=none_label></option>
                {move || {
                    options
                        .get()
                        .into_iter()
                        .map(|(option_value, label)| {
                            view! {
                                <option
                                    value=option_value.to_string()
                                    selected=option_value == value.get_untracked()
                                >
                                    {label}
                                </option>
                            }
                        })
                        .collect_view()
                }}
            </select>
            <span class=style::error></span>
        </div>
    }
}

fn select_string_field(
    label: &'static str,
    name: &'static str,
    value: RwSignal<String>,
    options: Signal<Vec<(String, String)>>,
    none_label: &'static str,
    disabled: Option<Signal<bool>>,
    helper: &'static str,
) -> impl IntoView {
    let select_class = format!("{} {}", style::input, style::select);
    let disabled = disabled.unwrap_or_else(|| Signal::derive(|| false));
    let disabled_initial = disabled.get_untracked();
    view! {
        <div class=style::field>
            <label class=style::label inner_html=label></label>
            {disabled_initial.then(|| {
                view! {
                    <input
                        type="hidden"
                        name=name
                        value=value.get_untracked()
                    />
                }
            })}
            <select
                name=name
                class=select_class
                disabled=move || disabled.get()
                on:change=move |e| value.set(event_target_value(&e))
            >
                <option value="" selected=value.get_untracked().is_empty() inner_html=none_label></option>
                {move || {
                    options
                        .get()
                        .into_iter()
                        .filter(|(option_value, _)| !option_value.is_empty())
                        .map(|(option_value, label)| {
                            let selected = option_value == value.get_untracked();
                            view! {
                                <option value=option_value selected=selected>{label}</option>
                            }
                        })
                        .collect_view()
                }}
            </select>
            <span class=style::helper inner_html=helper></span>
            <span class=style::error></span>
        </div>
    }
}
