use leptos::prelude::*;

use crate::admin::i18n::{log_level_text, supported_ui_language_options, tr, use_i18n, TextKey};
use crate::admin::server_fns::settings::{
    load_ffmpeg_settings, load_hdhr_settings, load_logging_settings, load_playout_settings,
    load_scanner_settings, load_ui_settings, load_xmltv_settings, FfmpegSettingsPageData,
    HdhrSettingsForm, LoggingSettingsForm, PlayoutSettingsForm, ScannerSettingsForm, SelectOption,
    UiSettingsForm, XmltvSettingsForm,
};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/pages/settings.module.css");

#[component]
pub fn FfmpegSettingsPage() -> impl IntoView {
    let data = Resource::new_blocking(
        || (),
        |()| async { load_ffmpeg_settings().await.unwrap_or_default() },
    );
    view! {
        <Suspense fallback=move || render_ffmpeg(FfmpegSettingsPageData::default())>
            {move || data.get().map(render_ffmpeg)}
        </Suspense>
    }
}

#[component]
pub fn LoggingSettingsPage() -> impl IntoView {
    let data = Resource::new_blocking(
        || (),
        |()| async { load_logging_settings().await.unwrap_or_default() },
    );
    view! {
        <Suspense fallback=move || render_logging(LoggingSettingsForm::default())>
            {move || data.get().map(render_logging)}
        </Suspense>
    }
}

#[component]
pub fn HdhrSettingsPage() -> impl IntoView {
    let data = Resource::new_blocking(
        || (),
        |()| async { load_hdhr_settings().await.unwrap_or_default() },
    );
    view! {
        <Suspense fallback=move || render_hdhr(HdhrSettingsForm::default())>
            {move || data.get().map(render_hdhr)}
        </Suspense>
    }
}

#[component]
pub fn ScannerSettingsPage() -> impl IntoView {
    let data = Resource::new_blocking(
        || (),
        |()| async { load_scanner_settings().await.unwrap_or_default() },
    );
    view! {
        <Suspense fallback=move || render_scanner(ScannerSettingsForm::default())>
            {move || data.get().map(render_scanner)}
        </Suspense>
    }
}

#[component]
pub fn PlayoutSettingsPage() -> impl IntoView {
    let data = Resource::new_blocking(
        || (),
        |()| async { load_playout_settings().await.unwrap_or_default() },
    );
    view! {
        <Suspense fallback=move || render_playout(PlayoutSettingsForm::default())>
            {move || data.get().map(render_playout)}
        </Suspense>
    }
}

#[component]
pub fn UiSettingsPage() -> impl IntoView {
    let data = Resource::new_blocking(
        || (),
        |()| async { load_ui_settings().await.unwrap_or_default() },
    );
    view! {
        <Suspense fallback=move || render_ui(UiSettingsForm::default())>
            {move || data.get().map(render_ui)}
        </Suspense>
    }
}

#[component]
pub fn XmltvSettingsPage() -> impl IntoView {
    let data = Resource::new_blocking(
        || (),
        |()| async { load_xmltv_settings().await.unwrap_or_default() },
    );
    view! {
        <Suspense fallback=move || render_xmltv(XmltvSettingsForm::default())>
            {move || data.get().map(render_xmltv)}
        </Suspense>
    }
}

#[allow(clippy::too_many_lines)]
fn render_ffmpeg(data: FfmpegSettingsPageData) -> impl IntoView {
    let form = data.form;
    view! {
        <SettingsForm title=tr(TextKey::Ffmpeg) action="/api/settings/ffmpeg/save">
            <Field label=tr(TextKey::FfmpegPath)>
                <input class=style::input name="ffmpeg_path" required value=form.ffmpeg_path/>
                <p class=style::help>{tr(TextKey::FfmpegPathHelp)}</p>
            </Field>
            <Field label=tr(TextKey::FfprobePath)>
                <input class=style::input name="ffprobe_path" required value=form.ffprobe_path/>
                <p class=style::help>{tr(TextKey::FfprobePathHelp)}</p>
            </Field>
            <Field label=tr(TextKey::DefaultFfmpegProfile)>
                <Select
                    name="default_ffmpeg_profile_id"
                    selected=form.default_ffmpeg_profile_id.to_string()
                    options=data.ffmpeg_profiles
                    include_none=false
                />
                <p class=style::help>{tr(TextKey::DefaultFfmpegProfileHelp)}</p>
            </Field>
            <Field label=tr(TextKey::PreferredAudioLanguage)>
                <Select
                    name="preferred_audio_language_code"
                    selected=form.preferred_audio_language_code
                    options=data.languages
                    include_none=false
                />
            </Field>
            <CheckboxField
                label=tr(TextKey::UseEmbeddedSubtitles)
                name="use_embedded_subtitles"
                checked=form.use_embedded_subtitles
                caption=None
            />
            <CheckboxField
                label=tr(TextKey::ExtractEmbeddedSubtitles)
                name="extract_embedded_subtitles"
                checked=form.extract_embedded_subtitles
                caption=None
            />
            <CheckboxField
                label=tr(TextKey::ProbeForInterlacedFrames)
                name="probe_for_interlaced_frames"
                checked=form.probe_for_interlaced_frames
                caption=None
            />
            <CheckboxField
                label=tr(TextKey::SaveTroubleshootingReportsToDisk)
                name="save_reports"
                checked=form.save_reports
                caption=None
            />
            <Field label=tr(TextKey::GlobalWatermark)>
                <Select
                    name="global_watermark_id"
                    selected=form.global_watermark_id.map(|v| v.to_string()).unwrap_or_default()
                    options=data.watermarks
                    include_none=true
                />
            </Field>
            <Field label=tr(TextKey::GlobalFallbackFiller)>
                <Select
                    name="global_fallback_filler_id"
                    selected=form.global_fallback_filler_id.map(|v| v.to_string()).unwrap_or_default()
                    options=data.fallback_fillers
                    include_none=true
                />
            </Field>
            <Field label=tr(TextKey::HlsSegmenterIdleTimeout)>
                <input class=style::input name="hls_segmenter_idle_timeout" type="number" min="30" required value=form.hls_segmenter_idle_timeout/>
                <p class=style::help>{tr(TextKey::HlsSegmenterIdleTimeoutHelp)}</p>
            </Field>
            <Field label=tr(TextKey::WorkAheadSegmenterLimit)>
                <input class=style::input name="work_ahead_segmenter_limit" type="number" min="0" required value=form.work_ahead_segmenter_limit/>
                <p class=style::help>{tr(TextKey::WorkAheadSegmenterLimitHelp)}</p>
            </Field>
            <Field label=tr(TextKey::HlsSegmenterInitialSegmentCount)>
                <input class=style::input name="initial_segment_count" type="number" min="1" required value=form.initial_segment_count/>
                <p class=style::help>{tr(TextKey::HlsSegmenterInitialSegmentCountHelp)}</p>
            </Field>
            <Field label=tr(TextKey::HlsDirectOutputFormat)>
                <Select
                    name="hls_direct_output_format"
                    selected=form.hls_direct_output_format.to_string()
                    options=vec![
                        SelectOption { value: "2".to_owned(), label: "MPEG-TS".to_owned() },
                        SelectOption { value: "3".to_owned(), label: "MP4".to_owned() },
                        SelectOption { value: "1".to_owned(), label: "MKV".to_owned() },
                    ]
                    include_none=false
                />
            </Field>
            <Field label=tr(TextKey::DefaultMpegTsScript)>
                <Select
                    name="default_mpeg_ts_script"
                    selected=form.default_mpeg_ts_script
                    options=data.mpeg_ts_scripts
                    include_none=false
                />
                <p class=style::help>{tr(TextKey::DefaultMpegTsScriptHelp)}</p>
            </Field>
        </SettingsForm>
        <section class=style::content>
            <h2 class=style::section_title>{tr(TextKey::CustomResolutions)}</h2>
            <div class=style::divider></div>
            <form method="post" action="/api/settings/ffmpeg/resolutions/add" class=style::inline_form>
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
                                        <form method="post" action="/api/settings/ffmpeg/resolutions/delete">
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

fn render_logging(form: LoggingSettingsForm) -> impl IntoView {
    let levels = log_level_options();
    view! {
        <SettingsForm title=tr(TextKey::Logging) action="/api/settings/logging/save">
            <SelectField label=tr(TextKey::DefaultMinimumLogLevel) name="default_minimum_log_level" selected=form.default_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label=tr(TextKey::ScanningMinimumLogLevel) name="scanning_minimum_log_level" selected=form.scanning_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label=tr(TextKey::SchedulingMinimumLogLevel) name="scheduling_minimum_log_level" selected=form.scheduling_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label=tr(TextKey::SearchingMinimumLogLevel) name="searching_minimum_log_level" selected=form.searching_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label=tr(TextKey::StreamingMinimumLogLevel) name="streaming_minimum_log_level" selected=form.streaming_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label=tr(TextKey::HttpMinimumLogLevel) name="http_minimum_log_level" selected=form.http_minimum_log_level options=levels include_none=false/>
        </SettingsForm>
    }
}

fn render_hdhr(form: HdhrSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title=tr(TextKey::HdHomeRun) action="/api/settings/hdhr/save">
            <Field label=tr(TextKey::Uuid)>
                <input class=style::input name="uuid" disabled value=form.uuid/>
            </Field>
            <Field label=tr(TextKey::TunerCount)>
                <input class=style::input name="tuner_count" type="number" min="1" required value=form.tuner_count/>
            </Field>
        </SettingsForm>
    }
}

fn render_scanner(form: ScannerSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title=tr(TextKey::Scanner) action="/api/settings/scanner/save">
            <Field label=tr(TextKey::LibraryRefreshInterval)>
                <input class=style::input name="library_refresh_interval" type="number" min="0" max="999999" required value=form.library_refresh_interval/>
            </Field>
        </SettingsForm>
    }
}

fn render_playout(form: PlayoutSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title=tr(TextKey::Playout) action="/api/settings/playout/save">
            <Field label=tr(TextKey::DaysToBuild)>
                <input class=style::input name="days_to_build" type="number" min="1" required value=form.days_to_build/>
            </Field>
            <CheckboxField
                label=tr(TextKey::SkipMissingItems)
                name="skip_missing_items"
                checked=form.skip_missing_items
                caption=Some(tr(TextKey::SkipMissingItemsHelp))
            />
            <Field label=tr(TextKey::ScriptedScheduleTimeout)>
                <input class=style::input name="scripted_schedule_timeout_seconds" type="number" min="10" required value=form.scripted_schedule_timeout_seconds/>
            </Field>
        </SettingsForm>
    }
}

fn render_ui(form: UiSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title=tr(TextKey::Ui) action="/api/settings/ui/save">
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
            <SelectField
                label=tr(TextKey::Language)
                name="language"
                selected=form.language
                options=supported_ui_language_options()
                include_none=false
            />
        </SettingsForm>
    }
}

fn render_xmltv(form: XmltvSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title=tr(TextKey::Xmltv) action="/api/settings/xmltv/save">
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
        </SettingsForm>
    }
}

#[component]
fn SettingsForm(title: &'static str, action: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class=style::page>
            <form method="post" action=action class=style::form>
                <div class=style::action_bar>
                    <button class=style::button type="submit">
                        <span style=icon_style(IconName::Save, "1rem") aria-hidden="true"></span>
                        {tr(TextKey::SaveSettings)}
                    </button>
                </div>
                <div class=style::content_scroll>
                    <section class=style::content>
                        <h1 class=style::title>{title}</h1>
                        <div class=style::divider></div>
                        {children()}
                    </section>
                </div>
            </form>
        </div>
    }
}

#[component]
fn Field(label: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class=style::field_row>
            <div class=style::label>{label}</div>
            <div>{children()}</div>
        </div>
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

#[component]
fn CheckboxField(
    label: &'static str,
    name: &'static str,
    checked: bool,
    caption: Option<&'static str>,
) -> impl IntoView {
    view! {
        <Field label=label>
            <div class=style::checkbox_wrap>
                <input class=style::checkbox type="checkbox" name=name value="true" checked=checked/>
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

    #[test]
    fn ui_language_options_match_ersatztv_supported_languages() {
        let values: Vec<String> = supported_ui_language_options()
            .into_iter()
            .map(|option| option.value)
            .collect();

        assert_eq!(values, vec!["en-us", "pl", "pt-br", "nl"]);
    }
}
