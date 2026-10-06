use leptos::prelude::*;

use crate::admin::i18n::{log_level_text, supported_ui_language_options, tr, use_i18n, TextKey};
use crate::admin::server_fns::settings::{
    load_ffmpeg_settings, load_hdhr_settings, load_logging_settings, load_playout_settings,
    load_scanner_settings, load_ui_settings, load_xmltv_settings, ActiveTranscodeCapacityStatus,
    ActiveTranscodeSuggestionReason, FfmpegSettingsPageData, HdhrSettingsPageData,
    LoggingSettingsForm, PlayoutSettingsForm, ScannerSettingsForm, SelectOption,
    UiSettingsPageData, WindowsStartupStatusForm, XmltvSettingsForm,
};
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::resource_state::{
    AdminErrorState, AdminLoadState, AdminLoadingState, AdminResourceState,
};
use crate::admin::ui::primitives::settings_navigation::SettingsPageLayout;

stylance::import_crate_style!(style, "src/admin/pages/settings.module.css");

#[component]
pub fn FfmpegSettingsPage() -> impl IntoView {
    let data: Resource<Result<FfmpegSettingsPageData, ServerFnError>> =
        Resource::new_blocking(|| (), |()| load_ffmpeg_settings());
    view! {
        <SettingsPageLayout>
            <Suspense fallback=loading_resource_view>
                {move || data.get().map(|result| settings_result_view(result, |value| render_ffmpeg(value).into_any()))}
            </Suspense>
        </SettingsPageLayout>
    }
}

#[component]
pub fn LoggingSettingsPage() -> impl IntoView {
    let data: Resource<Result<LoggingSettingsForm, ServerFnError>> =
        Resource::new_blocking(|| (), |()| load_logging_settings());
    view! {
        <SettingsPageLayout>
            <Suspense fallback=loading_resource_view>
                {move || data.get().map(|result| settings_result_view(result, |value| render_logging(value).into_any()))}
            </Suspense>
        </SettingsPageLayout>
    }
}

#[component]
pub fn HdhrSettingsPage() -> impl IntoView {
    let data: Resource<Result<HdhrSettingsPageData, ServerFnError>> =
        Resource::new_blocking(|| (), |()| load_hdhr_settings());
    view! {
        <SettingsPageLayout>
            <Suspense fallback=loading_resource_view>
                {move || data.get().map(|result| settings_result_view(result, |value| render_hdhr(value).into_any()))}
            </Suspense>
        </SettingsPageLayout>
    }
}

#[component]
pub fn ScannerSettingsPage() -> impl IntoView {
    let data: Resource<Result<ScannerSettingsForm, ServerFnError>> =
        Resource::new_blocking(|| (), |()| load_scanner_settings());
    view! {
        <SettingsPageLayout>
            <Suspense fallback=loading_resource_view>
                {move || data.get().map(|result| settings_result_view(result, |value| render_scanner(value).into_any()))}
            </Suspense>
        </SettingsPageLayout>
    }
}

#[component]
pub fn PlayoutSettingsPage() -> impl IntoView {
    let data: Resource<Result<PlayoutSettingsForm, ServerFnError>> =
        Resource::new_blocking(|| (), |()| load_playout_settings());
    view! {
        <SettingsPageLayout>
            <Suspense fallback=loading_resource_view>
                {move || data.get().map(|result| settings_result_view(result, |value| render_playout(value).into_any()))}
            </Suspense>
        </SettingsPageLayout>
    }
}

#[component]
pub fn UiSettingsPage() -> impl IntoView {
    let data: Resource<Result<UiSettingsPageData, ServerFnError>> =
        Resource::new_blocking(|| (), |()| load_ui_settings());
    view! {
        <SettingsPageLayout>
            <Suspense fallback=loading_resource_view>
                {move || data.get().map(|result| settings_result_view(result, |value| render_ui(value).into_any()))}
            </Suspense>
        </SettingsPageLayout>
    }
}

#[component]
pub fn XmltvSettingsPage() -> impl IntoView {
    let data: Resource<Result<XmltvSettingsForm, ServerFnError>> =
        Resource::new_blocking(|| (), |()| load_xmltv_settings());
    view! {
        <SettingsPageLayout>
            <Suspense fallback=loading_resource_view>
                {move || data.get().map(|result| settings_result_view(result, |value| render_xmltv(value).into_any()))}
            </Suspense>
        </SettingsPageLayout>
    }
}

fn loading_resource_view() -> AnyView {
    view! {
        <AdminResourceState state=Signal::derive(|| AdminLoadState::Loading)>
            <AdminLoadingState/>
        </AdminResourceState>
    }
    .into_any()
}

fn settings_result_view<T>(
    result: Result<T, ServerFnError>,
    ready: impl FnOnce(T) -> AnyView,
) -> AnyView {
    match result {
        Ok(value) => {
            let content = ready(value);
            view! {
                <AdminResourceState state=Signal::derive(|| AdminLoadState::Ready)>
                    {content}
                </AdminResourceState>
            }
            .into_any()
        }
        Err(error) => view! {
            <AdminResourceState state=Signal::derive(|| AdminLoadState::Failed)>
                <AdminErrorState error=error.to_string()/>
            </AdminResourceState>
        }
        .into_any(),
    }
}

#[allow(clippy::too_many_lines)]
fn render_ffmpeg(data: FfmpegSettingsPageData) -> impl IntoView {
    let active_transcode_capacity = data.active_transcode_capacity;
    let mut form = data.form;
    let mpeg_ts_scripts = localize_mpeg_ts_script_options(data.mpeg_ts_scripts);
    let audio_languages = playback_language_options(
        &data.languages,
        &form.preferred_audio_language_code,
        TextKey::OriginalLanguageOption,
    );
    let default_subtitle_languages = playback_language_options(
        &data.languages,
        &form.preferred_subtitle_language_code,
        TextKey::UndefinedLanguageOption,
    );
    let fallback_subtitle_languages = playback_language_options(
        &data.languages,
        &form.fallback_subtitle_language_code,
        TextKey::UndefinedLanguageOption,
    );
    form.preferred_audio_language_code = canonical_playback_language_selection(
        &audio_languages,
        &form.preferred_audio_language_code,
    );
    form.preferred_subtitle_language_code = canonical_playback_language_selection(
        &default_subtitle_languages,
        &form.preferred_subtitle_language_code,
    );
    form.fallback_subtitle_language_code = canonical_playback_language_selection(
        &fallback_subtitle_languages,
        &form.fallback_subtitle_language_code,
    );
    // The path inputs are intentionally not `required`: an empty value saves
    // and means auto-detect, matching the FFmpegLocator behavior the startup
    // path and save server fn already implement. The locator-resolved path is
    // surfaced as placeholder + help so an empty field still shows the
    // detected tool.
    let ffmpeg_autodetect_help = auto_detect_help(&data.ffmpeg_resolved);
    let ffprobe_autodetect_help = auto_detect_help(&data.ffprobe_resolved);
    view! {
        <SettingsForm title=tr(TextKey::Ffmpeg) action="/admin-api/settings/ffmpeg/save">
            <SettingsSection title=tr(TextKey::FfmpegRuntime)>
                <Field label=tr(TextKey::FfmpegPath)>
                    <input class=style::input name="ffmpeg_path" value=form.ffmpeg_path placeholder=data.ffmpeg_resolved/>
                    <p class=style::help>{tr(TextKey::FfmpegPathHelp)}</p>
                    <p class=style::help>{ffmpeg_autodetect_help}</p>
                </Field>
                <Field label=tr(TextKey::FfprobePath)>
                    <input class=style::input name="ffprobe_path" value=form.ffprobe_path placeholder=data.ffprobe_resolved/>
                    <p class=style::help>{tr(TextKey::FfprobePathHelp)}</p>
                    <p class=style::help>{ffprobe_autodetect_help}</p>
                </Field>
                <Field label=tr(TextKey::DefaultFfmpegProfile)>
                    <Select name="default_ffmpeg_profile_id" selected=form.default_ffmpeg_profile_id.to_string() options=data.ffmpeg_profiles include_none=false/>
                    <p class=style::help>{tr(TextKey::DefaultFfmpegProfileHelp)}</p>
                </Field>
            </SettingsSection>
            <SettingsSection title=tr(TextKey::Language)>
                <Field label=tr(TextKey::PreferredAudioLanguage)>
                    <Select name="preferred_audio_language_code" selected=form.preferred_audio_language_code options=audio_languages include_none=false/>
                </Field>
                <Field label=tr(TextKey::DefaultSubtitleLanguage)>
                    <Select name="preferred_subtitle_language_code" selected=form.preferred_subtitle_language_code options=default_subtitle_languages include_none=true/>
                </Field>
                <Field label=tr(TextKey::FallbackSubtitleLanguage)>
                    <Select name="fallback_subtitle_language_code" selected=form.fallback_subtitle_language_code options=fallback_subtitle_languages include_none=true/>
                </Field>
                <CheckboxField label=tr(TextKey::UseEmbeddedSubtitles) name="use_embedded_subtitles" checked=form.use_embedded_subtitles caption=None/>
                <CheckboxField label=tr(TextKey::ExtractEmbeddedSubtitles) name="extract_embedded_subtitles" checked=form.extract_embedded_subtitles caption=None disabled=!form.use_embedded_subtitles/>
            </SettingsSection>
            <SettingsSection title=tr(TextKey::Playback)>
                <Field label=tr(TextKey::HlsSegmenterIdleTimeout)>
                    <input class=style::input name="hls_segmenter_idle_timeout" type="number" min="30" required value=form.hls_segmenter_idle_timeout/>
                    <p class=style::help>{tr(TextKey::HlsSegmenterIdleTimeoutHelp)}</p>
                </Field>
                <Field label=tr(TextKey::WorkAheadSegmenterLimit)>
                    <input class=style::input name="work_ahead_segmenter_limit" type="number" min="0" required value=form.work_ahead_segmenter_limit/>
                    <p class=style::help>{tr(TextKey::WorkAheadSegmenterLimitHelp)}</p>
                </Field>
                <Field label=tr(TextKey::ActiveTranscodeLimit)>
                    <input class=style::input name="active_transcode_limit" type="number" min="1" required value=form.active_transcode_limit/>
                    <p class=style::help>{tr(TextKey::ActiveTranscodeLimitHelp)}</p>
                    <p class=style::help>{active_transcode_readout(active_transcode_capacity)}</p>
                    <p class=style::help>{active_transcode_suggestion(active_transcode_capacity)}</p>
                </Field>
                <Field label=tr(TextKey::HlsSegmenterInitialSegmentCount)>
                    <input class=style::input name="initial_segment_count" type="number" min="1" required value=form.initial_segment_count/>
                    <p class=style::help>{tr(TextKey::HlsSegmenterInitialSegmentCountHelp)}</p>
                </Field>
            </SettingsSection>
            <SettingsSection title=tr(TextKey::General)>
                <CheckboxField label=tr(TextKey::ProbeForInterlacedFrames) name="probe_for_interlaced_frames" checked=form.probe_for_interlaced_frames caption=None/>
                <CheckboxField label=tr(TextKey::SaveTroubleshootingReportsToDisk) name="save_reports" checked=form.save_reports caption=None/>
                <Field label=tr(TextKey::GlobalWatermark)>
                    <Select name="global_watermark_id" selected=form.global_watermark_id.map(|v| v.to_string()).unwrap_or_default() options=data.watermarks include_none=true/>
                </Field>
                <Field label=tr(TextKey::GlobalFallbackFiller)>
                    <Select name="global_fallback_filler_id" selected=form.global_fallback_filler_id.map(|v| v.to_string()).unwrap_or_default() options=data.fallback_fillers include_none=true/>
                </Field>
                <Field label=tr(TextKey::HlsDirectOutputFormat)>
                    <Select name="hls_direct_output_format" selected=form.hls_direct_output_format.to_string() options=vec![
                        SelectOption { value: "2".to_owned(), label: "MPEG-TS".to_owned() },
                        SelectOption { value: "3".to_owned(), label: "MP4".to_owned() },
                        SelectOption { value: "1".to_owned(), label: "MKV".to_owned() },
                    ] include_none=false/>
                </Field>
                <Field label=tr(TextKey::DefaultMpegTsScript)>
                    <Select name="default_mpeg_ts_script" selected=form.default_mpeg_ts_script options=mpeg_ts_scripts include_none=false/>
                    <p class=style::help>{tr(TextKey::DefaultMpegTsScriptHelp)}</p>
                </Field>
            </SettingsSection>
        </SettingsForm>
        <script inner_html=embedded_subtitle_dependency_script()></script>
        <section class=style::content>
            <h2 class=style::section_title>{tr(TextKey::CustomResolutions)}</h2>
            <div class=style::divider></div>
            <form method="post" action="/admin-api/settings/ffmpeg/resolutions/add" class=style::inline_form>
                <input class=format!("{} {}", style::input, style::inline_input) name="width" type="number" min="1" required placeholder=tr(TextKey::Width) aria-label=tr(TextKey::CustomResolutionWidth)/>
                <input class=format!("{} {}", style::input, style::inline_input) name="height" type="number" min="1" required placeholder=tr(TextKey::Height) aria-label=tr(TextKey::CustomResolutionHeight)/>
                <button class=style::button type="submit">
                    <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                    {tr(TextKey::AddCustomResolution)}
                </button>
            </form>
            <div class=style::table_wrap>
                <table class=style::table>
                    <tbody>
                        {data.custom_resolutions.into_iter().map(|option| {
                            view! {
                                <tr>
                                    <td>{option.label}</td>
                                    <td>
                                        <form method="post" action="/admin-api/settings/ffmpeg/resolutions/delete">
                                            <input type="hidden" name="id" value=option.value/>
                                            <button class=style::icon_button type="submit" aria-label=tr(TextKey::DeleteCustomResolution)>
                                                <span style=icon_style(IconName::Delete, "1.1rem") aria-hidden="true"></span>
                                            </button>
                                        </form>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

fn embedded_subtitle_dependency_script() -> &'static str {
    r#"
(() => {
  const sync = () => {
    const form = document.querySelector('form[action="/admin-api/settings/ffmpeg/save"]');
    if (!form) return;

    const useEmbedded = form.elements.namedItem("use_embedded_subtitles");
    const extract = form.elements.namedItem("extract_embedded_subtitles");
    if (!(useEmbedded instanceof HTMLInputElement) || !(extract instanceof HTMLInputElement)) return;

    extract.disabled = !useEmbedded.checked;
  };

  sync();
  document.addEventListener("change", (event) => {
    if (event.target instanceof HTMLInputElement && event.target.name === "use_embedded_subtitles") {
      sync();
    }
  });
  new MutationObserver(sync).observe(document.body, { childList: true, subtree: true });
})();
"#
}

fn auto_detect_help(resolved: &str) -> String {
    let label = tr(TextKey::LeaveEmptyToAutoDetect);
    if resolved.trim().is_empty() {
        label.to_owned()
    } else {
        format!("{label} ({resolved})")
    }
}

fn localize_mpeg_ts_script_options(options: Vec<SelectOption>) -> Vec<SelectOption> {
    options
        .into_iter()
        .map(|mut option| {
            if option.value.eq_ignore_ascii_case("default") && option.label == "Default" {
                tr(TextKey::DefaultOption).clone_into(&mut option.label);
            }
            option
        })
        .collect()
}

fn active_transcode_readout(capacity: ActiveTranscodeCapacityStatus) -> String {
    format!(
        "{}: {} / {} ({} {})",
        tr(TextKey::StreamsInUse),
        capacity.active,
        capacity.limit,
        tr(TextKey::Peak),
        capacity.peak
    )
}

fn active_transcode_suggestion(capacity: ActiveTranscodeCapacityStatus) -> String {
    let reason = match capacity.suggestion_reason {
        ActiveTranscodeSuggestionReason::Hardware => tr(TextKey::ActiveTranscodeSuggestionHardware),
        ActiveTranscodeSuggestionReason::Software => tr(TextKey::ActiveTranscodeSuggestionSoftware),
    };
    format!(
        "{}: {} - {}",
        tr(TextKey::SuggestedTranscodeLimit),
        capacity.suggested_limit,
        reason
    )
}

fn render_logging(form: LoggingSettingsForm) -> impl IntoView {
    let levels = log_level_options();
    let default_levels = levels.clone();
    let http_levels = levels.clone();
    let scanning_levels = levels.clone();
    let scheduling_levels = levels.clone();
    let searching_levels = levels.clone();
    view! {
        <SettingsForm title=tr(TextKey::Logging) action="/admin-api/settings/logging/save">
            <SettingsSection title=tr(TextKey::General)>
                <SelectField label=tr(TextKey::DefaultMinimumLogLevel) name="default_minimum_log_level" selected=form.default_minimum_log_level options=default_levels include_none=false/>
                <SelectField label=tr(TextKey::HttpMinimumLogLevel) name="http_minimum_log_level" selected=form.http_minimum_log_level options=http_levels include_none=false/>
            </SettingsSection>
            <SettingsSection title=tr(TextKey::Logging)>
                <SelectField label=tr(TextKey::ScanningMinimumLogLevel) name="scanning_minimum_log_level" selected=form.scanning_minimum_log_level options=scanning_levels include_none=false/>
                <SelectField label=tr(TextKey::SchedulingMinimumLogLevel) name="scheduling_minimum_log_level" selected=form.scheduling_minimum_log_level options=scheduling_levels include_none=false/>
                <SelectField label=tr(TextKey::SearchingMinimumLogLevel) name="searching_minimum_log_level" selected=form.searching_minimum_log_level options=searching_levels include_none=false/>
                <SelectField label=tr(TextKey::StreamingMinimumLogLevel) name="streaming_minimum_log_level" selected=form.streaming_minimum_log_level options=levels include_none=false/>
            </SettingsSection>
        </SettingsForm>
    }
}

fn render_hdhr(data: HdhrSettingsPageData) -> impl IntoView {
    let form = data.form;
    let disabled = data.disabled_by_jwt;
    view! {
        <SettingsForm title=tr(TextKey::HdHomeRun) action="/admin-api/settings/hdhr/save" disabled=disabled>
            <SettingsSection title=tr(TextKey::General)>
                <p class=style::warning role="status" hidden=!disabled>{tr(TextKey::HdhrDisabledByJwt)}</p>
                <Field label=tr(TextKey::Uuid)>
                    <input class=style::input name="uuid" disabled value=form.uuid/>
                </Field>
                <Field label=tr(TextKey::TunerCount)>
                    <input class=style::input name="tuner_count" type="number" min="1" required disabled=disabled value=form.tuner_count/>
                </Field>
            </SettingsSection>
        </SettingsForm>
    }
}

fn render_scanner(form: ScannerSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title=tr(TextKey::Scanner) action="/admin-api/settings/scanner/save">
            <SettingsSection title=tr(TextKey::General)>
                <Field label=tr(TextKey::LibraryRefreshInterval)>
                    <div class=style::input_with_adornment>
                        <input class=style::input name="library_refresh_interval" type="number" min="0" max="999999" required value=form.library_refresh_interval/>
                        <span class=style::adornment>{tr(TextKey::Hours)}</span>
                    </div>
                </Field>
            </SettingsSection>
        </SettingsForm>
    }
}

fn render_playout(form: PlayoutSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title=tr(TextKey::Playout) action="/admin-api/settings/playout/save">
            <SettingsSection title=tr(TextKey::General)>
                <Field label=tr(TextKey::DaysToBuild)>
                    <input class=style::input name="days_to_build" type="number" min="1" required value=form.days_to_build/>
                </Field>
                <CheckboxField label=tr(TextKey::SkipMissingItems) name="skip_missing_items" checked=form.skip_missing_items caption=Some(tr(TextKey::SkipMissingItemsHelp))/>
                <Field label=tr(TextKey::ScriptedScheduleTimeout)>
                    <input class=style::input name="scripted_schedule_timeout_seconds" type="number" min="10" required value=form.scripted_schedule_timeout_seconds/>
                </Field>
            </SettingsSection>
        </SettingsForm>
    }
}

fn render_ui(data: UiSettingsPageData) -> impl IntoView {
    let form = data.form;
    let windows_startup = windows_startup_controls(data.windows_startup);
    view! {
        <SettingsForm title=tr(TextKey::Ui) action="/admin-api/settings/ui/save">
            <SettingsSection title=tr(TextKey::General)>
                <SelectField
                    label=tr(TextKey::Theme)
                    name="is_dark_mode"
                    selected=if form.is_dark_mode { "true".to_owned() } else { "false".to_owned() }
                    options=vec![
                        SelectOption { value: "true".to_owned(), label: tr(TextKey::Dark).to_owned() },
                        SelectOption { value: "false".to_owned(), label: tr(TextKey::Light).to_owned() },
                    ]
                    include_none=false
                />
                <SelectField label=tr(TextKey::Language) name="language" selected=form.language options=supported_ui_language_options() include_none=false/>
            </SettingsSection>
            {windows_startup}
        </SettingsForm>
    }
}

fn windows_startup_controls(status: WindowsStartupStatusForm) -> Option<AnyView> {
    let (state, checked, disabled, caption) = match status {
        WindowsStartupStatusForm::Unsupported => return None,
        WindowsStartupStatusForm::Disabled => {
            ("disabled", false, false, tr(TextKey::StartWithWindowsHelp))
        }
        WindowsStartupStatusForm::Enabled => {
            ("enabled", true, false, tr(TextKey::StartWithWindowsHelp))
        }
        WindowsStartupStatusForm::NeedsRepair => (
            "needs-repair",
            true,
            false,
            tr(TextKey::WindowsStartupNeedsRepair),
        ),
        WindowsStartupStatusForm::Unavailable => (
            "unavailable",
            false,
            true,
            tr(TextKey::WindowsStartupUnavailable),
        ),
    };

    Some(
        view! {
            <SettingsSection title=tr(TextKey::WindowsStartup)>
                <div data-windows-startup-state=state>
                <CheckboxField
                    label=tr(TextKey::StartWithWindows)
                    name="windows_startup_enabled"
                    checked=checked
                    caption=Some(caption)
                    disabled=disabled
                />
                {(!disabled).then(|| view! {
                    <input type="hidden" name="windows_startup_present" value="true"/>
                })}
                </div>
            </SettingsSection>
        }
        .into_any(),
    )
}

fn render_xmltv(form: XmltvSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title=tr(TextKey::Xmltv) action="/admin-api/settings/xmltv/save">
            <SettingsSection title=tr(TextKey::General)>
                <Field label=tr(TextKey::DaysToBuild)>
                    <input class=style::input name="days_to_build" type="number" min="1" required value=form.days_to_build/>
                </Field>
                <SelectField
                    label=tr(TextKey::XmltvTimeZone)
                    name="time_zone"
                    selected=form.time_zone
                    options=vec![
                        SelectOption { value: "Local".to_owned(), label: tr(TextKey::Local).to_owned() },
                        SelectOption { value: "Utc".to_owned(), label: "UTC".to_owned() },
                    ]
                    include_none=false
                />
                <SelectField
                    label=tr(TextKey::XmltvBlockBehavior)
                    name="block_behavior"
                    selected=form.block_behavior
                    options=vec![
                        SelectOption { value: "SplitTimeEvenly".to_owned(), label: tr(TextKey::SplitTimeEvenly).to_owned() },
                        SelectOption { value: "UseActualTimes".to_owned(), label: tr(TextKey::UseActualTimes).to_owned() },
                    ]
                    include_none=false
                />
            </SettingsSection>
        </SettingsForm>
    }
}

#[component]
fn SettingsForm(
    title: &'static str,
    action: &'static str,
    #[prop(default = false)] disabled: bool,
    children: Children,
) -> impl IntoView {
    view! {
        <form method="post" action=action class=style::form>
            <div class=style::action_bar>
                <strong class=style::action_title>{title}</strong>
                <button class=style::button type="submit" disabled=disabled>
                    <span style=icon_style(IconName::Save, "1rem") aria-hidden="true"></span>
                    {tr(TextKey::SaveSettings)}
                </button>
            </div>
            <div class=style::content_scroll>
                <header class=style::page_header>
                    <h1 class=style::title>{title}</h1>
                </header>
                <div class=style::section_grid>{children()}</div>
            </div>
        </form>
    }
}

#[component]
fn SettingsSection(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <section class=style::settings_section>
            <h2 class=style::section_title>{title}</h2>
            <div class=style::section_fields>{children()}</div>
        </section>
    }
}

#[component]
fn Field(label: &'static str, children: Children) -> impl IntoView {
    view! {
        <label class=style::field_row>
            <span class=style::label>{label}</span>
            <div>{children()}</div>
        </label>
    }
}

#[component]
fn SelectField(
    label: &'static str,
    name: &'static str,
    selected: String,
    options: Vec<SelectOption>,
    include_none: bool,
) -> impl IntoView {
    view! {
        <Field label=label>
            <Select name=name selected=selected options=options include_none=include_none/>
        </Field>
    }
}

#[component]
fn Select(
    name: &'static str,
    selected: String,
    options: Vec<SelectOption>,
    include_none: bool,
) -> impl IntoView {
    view! {
        <select class=style::select name=name>
            {include_none.then(|| view! {
                <option value="" selected=selected.is_empty()>{tr(TextKey::NoneOption)}</option>
            })}
            {options.into_iter().map(|option| {
                let is_selected = option.value == selected;
                view! {
                    <option value=option.value selected=is_selected>{option.label}</option>
                }
            }).collect_view()}
        </select>
    }
}

fn playback_language_options(
    languages: &[SelectOption],
    selected: &str,
    und_label: TextKey,
) -> Vec<SelectOption> {
    let mut options = vec![SelectOption {
        value: "und".to_owned(),
        label: tr(und_label).to_owned(),
    }];
    options.extend(
        languages
            .iter()
            .filter(|option| option.value != "und")
            .cloned(),
    );

    let selected = selected.trim();
    if !selected.is_empty()
        && !options
            .iter()
            .any(|option| option.value.eq_ignore_ascii_case(selected))
    {
        options.push(SelectOption {
            value: selected.to_owned(),
            label: selected.to_owned(),
        });
    }

    options
}

fn canonical_playback_language_selection(options: &[SelectOption], selected: &str) -> String {
    let selected = selected.trim();
    options
        .iter()
        .find(|option| option.value.eq_ignore_ascii_case(selected))
        .map_or_else(|| selected.to_owned(), |option| option.value.clone())
}

#[component]
fn CheckboxField(
    label: &'static str,
    name: &'static str,
    checked: bool,
    caption: Option<&'static str>,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    view! {
        <Field label=label>
            <div class=style::checkbox_wrap>
                <input
                    class=style::checkbox
                    type="checkbox"
                    name=name
                    value="true"
                    checked=checked
                    disabled=disabled
                />
                {caption.map(|text| view! { <span class=style::caption>{text}</span> })}
            </div>
        </Field>
    }
}

fn log_level_options() -> Vec<SelectOption> {
    let language = use_i18n();
    ["Debug", "Information", "Warning", "Error"]
        .into_iter()
        .map(|level| SelectOption {
            value: level.to_owned(),
            label: log_level_text(language, level),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::settings::FfmpegSettingsForm;

    #[test]
    fn ui_language_options_follow_the_generated_registry() {
        let values: Vec<String> = supported_ui_language_options()
            .into_iter()
            .map(|option| option.value)
            .collect();
        let expected: Vec<String> = crate::admin::languages::LANGUAGES
            .iter()
            .map(|language| language.code.to_owned())
            .collect();

        assert_eq!(values, expected);
    }

    #[test]
    fn ui_settings_omit_windows_startup_on_unsupported_platforms() {
        let html = render_ui(ui_settings_fixture(WindowsStartupStatusForm::Unsupported))
            .into_view()
            .to_html();

        assert!(!html.contains("data-windows-startup-state"));
        assert!(!html.contains("/admin-api/settings/ui/windows-startup"));
    }

    #[test]
    fn ui_settings_render_the_current_windows_startup_state() {
        let disabled = render_ui(ui_settings_fixture(WindowsStartupStatusForm::Disabled))
            .into_view()
            .to_html();
        let enabled = render_ui(ui_settings_fixture(WindowsStartupStatusForm::Enabled))
            .into_view()
            .to_html();
        let repair = render_ui(ui_settings_fixture(WindowsStartupStatusForm::NeedsRepair))
            .into_view()
            .to_html();
        let unavailable = render_ui(ui_settings_fixture(WindowsStartupStatusForm::Unavailable))
            .into_view()
            .to_html();

        assert!(!input_html(&disabled, r#"name="windows_startup_enabled""#).contains("checked"));
        assert!(input_html(&enabled, r#"name="windows_startup_enabled""#).contains("checked"));
        assert!(repair.contains(r#"data-windows-startup-state="needs-repair""#));
        assert!(input_html(&repair, r#"name="windows_startup_enabled""#).contains("checked"));
        assert!(unavailable.contains(r#"data-windows-startup-state="unavailable""#));
        assert!(input_html(&unavailable, r#"name="windows_startup_enabled""#).contains("disabled"));
        assert!(disabled.contains(r#"name="windows_startup_present""#));
        assert!(enabled.contains(r#"name="windows_startup_present""#));
        assert!(repair.contains(r#"name="windows_startup_present""#));
        assert!(!unavailable.contains("windows_startup_present"));
        assert!(!disabled.contains("formaction"));
        assert!(!enabled.contains("formaction"));
        assert_eq!(enabled.matches(r#"type="submit""#).count(), 1);
    }

    #[test]
    fn ffmpeg_path_inputs_allow_empty_for_auto_detect() {
        let mut data = ffmpeg_settings_fixture();
        data.ffmpeg_resolved = "/usr/bin/ffmpeg".to_owned();
        data.ffprobe_resolved = "/usr/bin/ffprobe".to_owned();

        let html = render_ffmpeg(data).into_view().to_html();
        let ffmpeg_input = input_html(&html, r#"name="ffmpeg_path""#);
        let ffprobe_input = input_html(&html, r#"name="ffprobe_path""#);

        // Empty means auto-detect, so the browser must not block the save.
        assert!(!ffmpeg_input.contains("required"));
        assert!(!ffprobe_input.contains("required"));
        // The locator-resolved path is shown so an empty field still
        // surfaces the auto-detected tool.
        assert!(ffmpeg_input.contains(r#"placeholder="/usr/bin/ffmpeg""#));
        assert!(ffprobe_input.contains(r#"placeholder="/usr/bin/ffprobe""#));
        assert!(html.contains("(/usr/bin/ffmpeg)"));
        assert!(html.contains("(/usr/bin/ffprobe)"));
    }

    #[test]
    fn ffmpeg_extract_embedded_subtitles_is_disabled_without_embedded_subtitles() {
        let mut data = ffmpeg_settings_fixture();
        data.form.use_embedded_subtitles = false;
        data.form.extract_embedded_subtitles = true;

        let html = render_ffmpeg(data).into_view().to_html();
        let input = input_html(&html, r#"name="extract_embedded_subtitles""#);

        assert!(input.contains("disabled"));
        assert!(input.contains("checked"));
    }

    #[test]
    fn ffmpeg_extract_embedded_subtitles_is_enabled_with_embedded_subtitles() {
        let mut data = ffmpeg_settings_fixture();
        data.form.use_embedded_subtitles = true;
        data.form.extract_embedded_subtitles = true;

        let html = render_ffmpeg(data).into_view().to_html();
        let input = input_html(&html, r#"name="extract_embedded_subtitles""#);

        assert!(!input.contains("disabled"));
        assert!(input.contains("checked"));
    }

    #[test]
    fn ffmpeg_active_transcode_limit_is_rendered() {
        let mut data = ffmpeg_settings_fixture();
        data.form.active_transcode_limit = 2;

        let html = render_ffmpeg(data).into_view().to_html();
        let input = input_html(&html, r#"name="active_transcode_limit""#);

        assert!(html.contains("Maximum Simultaneous Streaming Transcodes"));
        assert!(html.contains("all-streams-in-use"));
        assert!(html.contains("Streams in use: 0 / 2 (peak 0)"));
        assert!(html.contains("Suggested value: 2"));
        assert!(input.contains(r#"type="number""#));
        assert!(input.contains(r#"min="1""#));
        assert!(input.contains(r#"value="2""#));
    }

    #[test]
    fn ffmpeg_subtitle_language_preferences_are_rendered() {
        let mut data = ffmpeg_settings_fixture();
        data.form.preferred_subtitle_language_code = "nld".to_owned();
        data.form.fallback_subtitle_language_code = "eng".to_owned();
        data.languages = vec![
            SelectOption {
                value: "eng".to_owned(),
                label: "English".to_owned(),
            },
            SelectOption {
                value: "nld".to_owned(),
                label: "Dutch".to_owned(),
            },
        ];

        let html = render_ffmpeg(data).into_view().to_html();

        assert!(html.contains("Default Subtitle Language"));
        assert!(html.contains("Fallback Subtitle Language"));
        assert!(html.contains(r#"name="preferred_subtitle_language_code""#));
        assert!(html.contains(r#"name="fallback_subtitle_language_code""#));
        assert!(html.contains(r#"value="nld" selected"#));
        assert!(html.contains(r#"value="eng" selected"#));
    }

    #[test]
    fn ffmpeg_language_preferences_render_contextual_und_labels() {
        let mut data = ffmpeg_settings_fixture();
        data.form.preferred_audio_language_code = "und".to_owned();
        data.form.preferred_subtitle_language_code = "und".to_owned();
        data.form.fallback_subtitle_language_code = "und".to_owned();
        data.languages = vec![SelectOption {
            value: "eng".to_owned(),
            label: "English".to_owned(),
        }];

        let html = render_ffmpeg(data).into_view().to_html();

        assert_eq!(html.matches(">Original (und)</option>").count(), 1);
        assert_eq!(html.matches(">Undefined (und)</option>").count(), 2);
        assert_eq!(html.matches(r#"value="und" selected"#).count(), 3);
    }

    #[test]
    fn playback_language_options_preserve_only_the_selected_legacy_value() {
        let languages = vec![SelectOption {
            value: "eng".to_owned(),
            label: "English".to_owned(),
        }];

        let options =
            playback_language_options(&languages, "enm", TextKey::UndefinedLanguageOption);

        assert_eq!(options[0].value, "und");
        assert_eq!(options[0].label, "Undefined (und)");
        assert!(options
            .iter()
            .any(|option| option.value == "enm" && option.label == "enm"));
        assert!(
            !playback_language_options(&languages, "eng", TextKey::UndefinedLanguageOption,)
                .iter()
                .any(|option| option.value == "enm")
        );
    }

    #[test]
    fn playback_language_options_canonicalize_case_and_whitespace_variants() {
        let languages = vec![SelectOption {
            value: "eng".to_owned(),
            label: "English".to_owned(),
        }];

        for selected in ["ENG", " eng "] {
            let options =
                playback_language_options(&languages, selected, TextKey::UndefinedLanguageOption);

            assert_eq!(
                canonical_playback_language_selection(&options, selected),
                "eng"
            );
            assert!(!options.iter().any(|option| option.value == selected));
        }
    }

    #[test]
    fn ffmpeg_language_preferences_select_canonical_persisted_variants() {
        let mut data = ffmpeg_settings_fixture();
        data.form.preferred_audio_language_code = "ENG".to_owned();
        data.form.preferred_subtitle_language_code = " eng ".to_owned();
        data.form.fallback_subtitle_language_code = "UND".to_owned();
        data.languages = vec![SelectOption {
            value: "eng".to_owned(),
            label: "English".to_owned(),
        }];

        let html = render_ffmpeg(data).into_view().to_html();

        assert_eq!(html.matches(r#"value="eng" selected"#).count(), 2);
        assert_eq!(html.matches(r#"value="und" selected"#).count(), 1);
        assert!(!html.contains(r#"value="ENG""#));
        assert!(!html.contains(r#"value=" eng ""#));
        assert!(!html.contains(r#"value="UND""#));
    }

    #[test]
    fn scanner_refresh_interval_renders_hours_adornment() {
        let html = render_scanner(ScannerSettingsForm {
            library_refresh_interval: 6,
        })
        .into_view()
        .to_html();

        let input = input_html(&html, r#"name="library_refresh_interval""#);
        assert!(input.contains(r#"value="6""#));
        assert!(html.contains(r">hours</span>"));
    }

    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn closed_pool_renders_error_state_without_a_submittable_form() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("test pool");
        pool.close().await;

        let error = crate::admin::server_fns::settings::load_ffmpeg_settings_with_pool(&pool)
            .await
            .expect_err("closed pool should fail");
        let owner = Owner::new();
        let html = owner.with(|| {
            settings_result_view::<FfmpegSettingsPageData>(Err(error), |_| {
                unreachable!("error result cannot render the ready form")
            })
            .to_html()
        });

        assert!(html.contains(r#"data-state="error""#));
        assert!(!html.contains("<form"));
        assert!(!html.contains(r#"type="submit""#));
    }

    fn ffmpeg_settings_fixture() -> FfmpegSettingsPageData {
        FfmpegSettingsPageData {
            form: FfmpegSettingsForm {
                ffmpeg_path: String::new(),
                ffprobe_path: String::new(),
                default_ffmpeg_profile_id: 0,
                preferred_audio_language_code: "eng".to_owned(),
                preferred_subtitle_language_code: String::new(),
                fallback_subtitle_language_code: String::new(),
                use_embedded_subtitles: true,
                extract_embedded_subtitles: false,
                probe_for_interlaced_frames: false,
                save_reports: false,
                global_watermark_id: None,
                global_fallback_filler_id: None,
                hls_segmenter_idle_timeout: 60,
                work_ahead_segmenter_limit: 1,
                active_transcode_limit: 2,
                initial_segment_count: 1,
                hls_direct_output_format: 2,
                default_mpeg_ts_script: "default".to_owned(),
            },
            ffmpeg_resolved: String::new(),
            ffprobe_resolved: String::new(),
            active_transcode_capacity: ActiveTranscodeCapacityStatus::default(),
            ffmpeg_profiles: Vec::new(),
            languages: Vec::new(),
            watermarks: Vec::new(),
            fallback_fillers: Vec::new(),
            mpeg_ts_scripts: Vec::new(),
            custom_resolutions: Vec::new(),
        }
    }

    fn ui_settings_fixture(windows_startup: WindowsStartupStatusForm) -> UiSettingsPageData {
        UiSettingsPageData {
            form: crate::admin::server_fns::settings::UiSettingsForm {
                is_dark_mode: true,
                language: "en-us".to_owned(),
            },
            windows_startup,
        }
    }

    fn input_html<'a>(html: &'a str, needle: &str) -> &'a str {
        let name_index = html.find(needle).expect("input should be rendered");
        let start = html[..name_index].rfind("<input").expect("input start");
        let end = html[name_index..].find('>').expect("input end") + name_index + 1;
        &html[start..end]
    }
}
