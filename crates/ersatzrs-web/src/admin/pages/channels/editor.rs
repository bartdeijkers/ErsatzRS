#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{channel_streaming_mode_label, tr, TextKey};
use crate::admin::server_fns::channels::{
    get_channel_editor_data, ChannelEditorData, ChannelMirrorOption, ChannelSelectOption,
};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
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
            "save" => tr(TextKey::ChannelCouldNotBeSaved).to_string(),
            "upload" => tr(TextKey::ChannelLogoUploadFailed).to_string(),
            message => message
                .replace(
                    "Invalid streaming engine",
                    tr(TextKey::InvalidStreamingEngine),
                )
                .replace(
                    "Invalid Next engine text subtitle mode",
                    tr(TextKey::InvalidNextEngineTextSubtitleMode),
                ),
        }
    });

    let data = Resource::new_blocking(channel_id, get_channel_editor_data);

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let form_error = form_error.clone();
                data.get().map(|result| admin_result_view(result, |initial_data| {
                    view! {
                        <ChannelEditorForm initial_data=initial_data form_error=form_error/>
                    }
                    .into_any()
                }))
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
    let mirror_options_disabled = Signal::derive(move || playout_source.get() != 1);
    let playout_mode = RwSignal::new(initial_form.playout_mode);
    let mirror_source_channel_id = RwSignal::new(initial_form.mirror_source_channel_id);
    let playout_offset_hours = RwSignal::new(initial_form.playout_offset_hours);
    let is_enabled = RwSignal::new(initial_form.is_enabled);
    let show_in_epg = RwSignal::new(initial_form.show_in_epg);
    let streaming_engine = RwSignal::new(initial_form.streaming_engine);
    let next_engine_text_subtitle_mode = RwSignal::new(initial_form.next_engine_text_subtitle_mode);
    let streaming_mode = RwSignal::new(
        if initial_form.streaming_engine == 1 && !matches!(initial_form.streaming_mode, 4 | 5) {
            4
        } else {
            initial_form.streaming_mode
        },
    );
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
        tr(TextKey::EditChannel)
    } else {
        tr(TextKey::AddChannel)
    };

    let save_label = if channel_id != 0 {
        tr(TextKey::SaveChannel)
    } else {
        tr(TextKey::AddChannel)
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
                <a href="/channels" class=style::back_link>{tr(TextKey::Channels)}</a>
                <h1 class=style::page_title inner_html=page_title></h1>
            </div>

            <form method="post" action="/admin-api/channels/save-multipart" enctype="multipart/form-data">
                <input type="hidden" name="id" value=channel_id.to_string()/>
                <input type="hidden" name="logo_path" value=logo_path.get_untracked()/>
                <input type="hidden" name="logo_content_type" value=logo_content_type.get_untracked()/>

                <div class=style::card>
                    <p class=style::form_error hidden=!has_form_error>
                        {form_error}
                    </p>

                    {text_field(tr(TextKey::Number), "number", number, "")}
                    {text_field(tr(TextKey::Name), "name", name, "")}
                    {text_field(tr(TextKey::Group), "group", group, "")}
                    {text_field(
                        tr(TextKey::Categories),
                        "categories",
                        categories,
                        tr(TextKey::CategoriesHelp),
                    )}

                    {select_i32_field(
                        tr(TextKey::TranscodeMode),
                        "transcode_mode",
                        transcode_mode,
                        Signal::derive(|| vec![(0, tr(TextKey::OnDemand).to_string())]),
                        Some(Signal::derive(|| true)),
                        tr(TextKey::TranscodeModeHelp),
                    )}
                    {select_i32_field(
                        tr(TextKey::IdleBehavior),
                        "idle_behavior",
                        idle_behavior,
                        Signal::derive(|| vec![
                            (0, tr(TextKey::StopOnDisconnect).to_string()),
                            (1, tr(TextKey::KeepRunning).to_string()),
                        ]),
                        None,
                        tr(TextKey::IdleBehaviorHelp),
                    )}

                    {select_i32_field(
                        tr(TextKey::PlayoutSource),
                        "playout_source",
                        playout_source,
                        Signal::derive(|| vec![
                            (0, tr(TextKey::Generated).to_string()),
                            (1, tr(TextKey::Mirror).to_string()),
                        ]),
                        Some(Signal::derive(move || playout_source_locked)),
                        tr(TextKey::PlayoutSourceHelp),
                    )}

                    <div>
                        {select_i32_field(
                            tr(TextKey::PlayoutMode),
                            "playout_mode",
                            playout_mode,
                            Signal::derive(|| vec![
                                (0, tr(TextKey::Continuous).to_string()),
                                (1, tr(TextKey::OnDemand).to_string()),
                            ]),
                            None,
                            tr(TextKey::PlayoutModeHelp),
                        )}
                    </div>

                    <div>
                        {select_i64_field(
                            tr(TextKey::MirrorSourceChannel),
                            "mirror_source_channel_id",
                            mirror_source_channel_id,
                            mirror_options,
                            tr(TextKey::NoneOption),
                            Some(mirror_options_disabled),
                            false,
                        )}
                        {number_field(
                            tr(TextKey::PlayoutOffset),
                            "playout_offset_hours",
                            playout_offset_hours,
                            tr(TextKey::Hours),
                            Some(mirror_options_disabled),
                        )}
                        <span class=style::helper>{tr(TextKey::MirrorOptionsHelp)}</span>
                    </div>
                    <script inner_html=playout_source_dependency_script()></script>

                    <div class=style::checkboxes>
                        {checkbox_field(tr(TextKey::IsEnabled), "is_enabled", is_enabled)}
                        {checkbox_field(tr(TextKey::ShowInEpg), "show_in_epg", show_in_epg)}
                    </div>

                    {select_i32_field(
                        tr(TextKey::StreamingEngine),
                        "streaming_engine",
                        streaming_engine,
                        Signal::derive(|| vec![
                            (0, "ErsatzRS".to_owned()),
                            (1, "ErsatzTV Next".to_owned()),
                        ]),
                        None,
                        "",
                    )}
                    <div data-next-subtitle-mode hidden=initial_form.streaming_engine != 1>
                        {select_i32_field(
                            tr(TextKey::NextEngineTextSubtitleMode),
                            "next_engine_text_subtitle_mode",
                            next_engine_text_subtitle_mode,
                            Signal::derive(|| vec![
                                (0, tr(TextKey::SubtitleBurn).to_owned()),
                                (1, tr(TextKey::SubtitleConvertIfPossible).to_owned()),
                            ]),
                            None,
                            tr(TextKey::NextEngineTextSubtitleModeHelp),
                        )}
                    </div>
                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::StreamingMode)}</label>
                        <select
                            name="streaming_mode"
                            class=format!("{} {}", style::input, style::select)
                        >
                            {[5, 1, 2, 4].into_iter().map(|mode| view! {
                                <option
                                    value=mode.to_string()
                                    selected=mode == streaming_mode.get_untracked()
                                    disabled=streaming_engine.get_untracked() == 1 && !matches!(mode, 4 | 5)
                                    hidden=streaming_engine.get_untracked() == 1 && !matches!(mode, 4 | 5)
                                >
                                    {channel_streaming_mode_label(mode)}
                                </option>
                            }).collect_view()}
                        </select>
                    </div>
                    <script inner_html=streaming_engine_dependency_script()></script>

                    {select_i64_field(
                        tr(TextKey::FfmpegProfile),
                        "ffmpeg_profile_id",
                        ffmpeg_profile_id,
                        profile_items,
                        tr(TextKey::SelectProfile),
                        Some(is_hls_direct),
                        true,
                    )}
                    <p class=style::no_profiles hidden=has_profiles>
                        {tr(TextKey::NoFfmpegProfiles)}
                    </p>

                    // Hidden controls stay populated and enabled so Next saves retain the native value.
                    <div data-native-slug-seconds hidden=initial_form.streaming_engine == 1>
                        {select_string_field(
                            tr(TextKey::SlugSeconds),
                            "slug_seconds",
                            slug_seconds,
                            Signal::derive(|| vec![
                                (String::new(), tr(TextKey::NoneOption).to_string()),
                                ("0.5".to_string(), format!("0.5 {}", tr(TextKey::Seconds))),
                                ("1".to_string(), format!("1 {}", tr(TextKey::Second))),
                                ("2".to_string(), format!("2 {}", tr(TextKey::Seconds))),
                                ("3".to_string(), format!("3 {}", tr(TextKey::Seconds))),
                                ("5".to_string(), format!("5 {}", tr(TextKey::Seconds))),
                            ]),
                            "",
                            None,
                            tr(TextKey::SlugSecondsHelp),
                        )}
                    </div>

                    {select_i32_field(
                        tr(TextKey::StreamSelectorMode),
                        "stream_selector_mode",
                        stream_selector_mode,
                        Signal::derive(|| vec![
                            (0, tr(TextKey::DefaultOption).to_string()),
                            (1, tr(TextKey::Custom).to_string()),
                        ]),
                        None,
                        "",
                    )}

                    <div>
                        {select_string_field(
                            tr(TextKey::PreferredAudioLanguage),
                            "preferred_audio_language_code",
                            preferred_audio_language_code,
                            language_items,
                            tr(TextKey::NoneOption),
                            None,
                            "",
                        )}
                        {text_field(tr(TextKey::PreferredAudioTitle), "preferred_audio_title", preferred_audio_title, "")}
                        {select_string_field(
                            tr(TextKey::PreferredSubtitleLanguage),
                            "preferred_subtitle_language_code",
                            preferred_subtitle_language_code,
                            language_items,
                            tr(TextKey::NoneOption),
                            None,
                            "",
                        )}
                        {select_i32_field(
                            tr(TextKey::SubtitleMode),
                            "subtitle_mode",
                            subtitle_mode,
                            Signal::derive(|| vec![
                                (0, tr(TextKey::NoneOption).to_string()),
                                (1, tr(TextKey::Forced).to_string()),
                                (2, tr(TextKey::DefaultOption).to_string()),
                                (3, tr(TextKey::Any).to_string()),
                            ]),
                            None,
                            tr(TextKey::SubtitleModeHelp),
                        )}
                    </div>

                    <div>
                        {select_string_field(
                            tr(TextKey::StreamSelector),
                            "stream_selector",
                            stream_selector,
                            stream_selector_items,
                            tr(TextKey::NoneOption),
                            None,
                            "",
                        )}
                    </div>

                    {select_i32_field(
                        tr(TextKey::MusicVideoCreditsMode),
                        "music_video_credits_mode",
                        music_video_credits_mode,
                        Signal::derive(|| vec![
                            (0, tr(TextKey::NoneOption).to_string()),
                            (1, tr(TextKey::GenerateSubtitles).to_string()),
                        ]),
                        None,
                        "",
                    )}
                    {select_string_field(
                        tr(TextKey::MusicVideoCreditsTemplate),
                        "music_video_credits_template",
                        music_video_credits_template,
                        music_video_template_items,
                        tr(TextKey::NoneOption),
                        None,
                        "",
                    )}
                    {select_i32_field(
                        tr(TextKey::SongVideoMode),
                        "song_video_mode",
                        song_video_mode,
                        Signal::derive(|| vec![
                            (0, tr(TextKey::DefaultOption).to_string()),
                            (1, tr(TextKey::WithProgress).to_string()),
                        ]),
                        None,
                        "",
                    )}
                    <script inner_html=music_video_credits_template_dependency_script()></script>

                    <div class=style::field>
                        <label class=style::label for="channel-logo-file">{tr(TextKey::UploadLogo)}</label>
                        <input
                            id="channel-logo-file"
                            name="logo_file"
                            type="file"
                            accept="image/*"
                            class=style::input
                        />
                        <span class=style::helper>
                            {tr(TextKey::UploadLogoHelp)}
                        </span>
                        <span class=style::error></span>
                    </div>

                    {text_field(tr(TextKey::ExternalLogoUrl), "external_logo_url", external_logo_url, "")}

                    <div class=style::field>
                        <label class=style::label>{tr(TextKey::ChannelLogoPreview)}</label>
                        <img
                            class=style::logo_preview
                            src=move || logo_preview.get()
                            alt=tr(TextKey::ChannelLogoPreview)
                            style:display=move || if logo_preview.get().trim().is_empty() { "none" } else { "" }
                        />
                    </div>

                    <div class=style::checkboxes>
                        {checkbox_field(tr(TextKey::RemoveStoredLogo), "remove_logo", RwSignal::new(false))}
                    </div>

                    {select_i64_field(
                        tr(TextKey::Watermark),
                        "watermark_id",
                        watermark_id,
                        watermark_items,
                        tr(TextKey::NoneOption),
                        Some(is_hls_direct),
                        true,
                    )}

                    {select_i64_field(
                        tr(TextKey::FallbackFiller),
                        "fallback_filler_id",
                        fallback_filler_id,
                        filler_items,
                        tr(TextKey::NoneOption),
                        None,
                        true,
                    )}
                </div>

                <div class=style::form_actions>
                    <button
                        type="submit"
                        class=style::btn_primary
                    >
                        {save_label}
                    </button>
                    <a href="/channels" class=style::cancel_link>{tr(TextKey::Cancel)}</a>
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
    let mut result = vec![(String::new(), tr(TextKey::NoneOption).to_string())];
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
    disabled: Option<Signal<bool>>,
) -> impl IntoView {
    let disabled = disabled.unwrap_or_else(|| Signal::derive(|| false));
    view! {
        <div class=style::field>
            <label class=style::label inner_html=label></label>
            <div class=style::number_row>
                <input
                    name=name
                    type="number"
                    class=style::input
                    value=value.get_untracked().to_string()
                    disabled=move || disabled.get()
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
    preserve_disabled_value: bool,
) -> impl IntoView {
    let select_class = format!("{} {}", style::input, style::select);
    let disabled = disabled.unwrap_or_else(|| Signal::derive(|| false));
    let disabled_initial = disabled.get_untracked();
    view! {
        <div class=style::field>
            <label class=style::label inner_html=label></label>
            {(disabled_initial && preserve_disabled_value).then(|| {
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

fn streaming_engine_dependency_script() -> &'static str {
    r#"
(() => {
  const form = document.querySelector('form[action="/admin-api/channels/save-multipart"]');
  if (!form) return;
  const engine = form.querySelector('select[name="streaming_engine"]');
  const mode = form.querySelector('select[name="streaming_mode"]');
  if (!engine || !mode) return;
  const sync = (engineChanged = false) => {
    const next = engine.value === "1";
    const subtitles = form.querySelector("[data-next-subtitle-mode]");
    if (subtitles) subtitles.hidden = !next;
    const slug = form.querySelector("[data-native-slug-seconds]");
    if (slug) slug.hidden = next;
    for (const option of mode.options) {
      option.disabled = next && !["4", "5"].includes(option.value);
      option.hidden = option.disabled;
    }
    if (next && (engineChanged || !["4", "5"].includes(mode.value))) mode.value = "4";
    mode.dispatchEvent(new Event("change", { bubbles: true }));
  };
  sync();
  engine.addEventListener("change", () => sync(true));
})();
"#
}

fn playout_source_dependency_script() -> &'static str {
    r#"
(() => {
  const form = document.querySelector('form[action="/admin-api/channels/save-multipart"]');
  if (!form) return;
  const source = form.querySelector('select[name="playout_source"]');
  const mirrorSource = form.querySelector('select[name="mirror_source_channel_id"]');
  const offset = form.querySelector('input[name="playout_offset_hours"]');
  if (!source || !mirrorSource || !offset) return;
  const sync = () => {
    const disabled = source.value !== "1";
    mirrorSource.disabled = disabled;
    offset.disabled = disabled;
  };
  sync();
  source.addEventListener("change", sync);
})();
"#
}

fn music_video_credits_template_dependency_script() -> &'static str {
    r#"
(() => {
  const form = document.querySelector('form[action="/admin-api/channels/save-multipart"]');
  if (!form) return;
  const mode = form.querySelector('[name="music_video_credits_mode"]');
  const template = form.querySelector('[name="music_video_credits_template"]');
  if (!mode || !template) return;
  const sync = () => {
    template.disabled = mode.value !== "1";
  };
  sync();
  mode.addEventListener("change", sync);
})();
"#
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::admin::server_fns::channels::ChannelForm;

    #[test]
    fn native_slug_setting_stays_in_ssr_form_when_next_hides_it() {
        for engine in [0, 1] {
            let owner = Owner::new();
            let html = owner.with(|| {
                channel_editor_form(
                    ChannelEditorData {
                        form: ChannelForm {
                            streaming_engine: engine,
                            slug_seconds: Some(2.0),
                            ..ChannelForm::new_defaults()
                        },
                        ..ChannelEditorData::default()
                    },
                    String::new(),
                )
                .into_view()
                .to_html()
            });
            // The dependency script mentions this selector before the actual field.
            let wrapper = html.split("<div data-native-slug-seconds").nth(1).unwrap();
            assert_eq!(
                wrapper.split('>').next().unwrap().contains("hidden"),
                engine == 1
            );
            let slug = wrapper.split("name=\"slug_seconds\"").nth(1).unwrap();
            assert!(!slug.split('>').next().unwrap().contains("disabled"));
            assert!(slug.contains("value=\"2\" selected"));
            assert!(html.contains("form.querySelector(\"[data-native-slug-seconds]\")"));
        }
    }
}
