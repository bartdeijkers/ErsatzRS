use leptos::prelude::*;

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
        <SettingsForm title="FFmpeg" action="/api/settings/ffmpeg/save">
            <Field label="FFmpeg Path">
                <input class=style::input name="ffmpeg_path" required value=form.ffmpeg_path/>
                <p class=style::help>"The full path to the ffmpeg executable file"</p>
            </Field>
            <Field label="FFprobe Path">
                <input class=style::input name="ffprobe_path" required value=form.ffprobe_path/>
                <p class=style::help>"The full path to the ffprobe executable file"</p>
            </Field>
            <Field label="Default FFmpeg Profile">
                <Select
                    name="default_ffmpeg_profile_id"
                    selected=form.default_ffmpeg_profile_id.to_string()
                    options=data.ffmpeg_profiles
                    include_none=false
                />
                <p class=style::help>"The FFmpeg Profile to use when creating new channels"</p>
            </Field>
            <Field label="Preferred Audio Language">
                <Select
                    name="preferred_audio_language_code"
                    selected=form.preferred_audio_language_code
                    options=data.languages
                    include_none=false
                />
            </Field>
            <CheckboxField
                label="Use Embedded Subtitles"
                name="use_embedded_subtitles"
                checked=form.use_embedded_subtitles
                caption=None
            />
            <CheckboxField
                label="Extract Embedded (Text) Subtitles"
                name="extract_embedded_subtitles"
                checked=form.extract_embedded_subtitles
                caption=None
            />
            <CheckboxField
                label="Probe For Interlaced Frames"
                name="probe_for_interlaced_frames"
                checked=form.probe_for_interlaced_frames
                caption=None
            />
            <CheckboxField
                label="Save Troubleshooting Reports To Disk"
                name="save_reports"
                checked=form.save_reports
                caption=None
            />
            <Field label="Global Watermark">
                <Select
                    name="global_watermark_id"
                    selected=form.global_watermark_id.map(|v| v.to_string()).unwrap_or_default()
                    options=data.watermarks
                    include_none=true
                />
            </Field>
            <Field label="Global Fallback Filler">
                <Select
                    name="global_fallback_filler_id"
                    selected=form.global_fallback_filler_id.map(|v| v.to_string()).unwrap_or_default()
                    options=data.fallback_fillers
                    include_none=true
                />
            </Field>
            <Field label="HLS Segmenter Idle Timeout">
                <input class=style::input name="hls_segmenter_idle_timeout" type="number" min="30" required value=form.hls_segmenter_idle_timeout/>
                <p class=style::help>"The number of seconds to continue transcoding a channel while no requests have been received from any client"</p>
            </Field>
            <Field label="Work-Ahead HLS Segmenter Limit">
                <input class=style::input name="work_ahead_segmenter_limit" type="number" min="0" required value=form.work_ahead_segmenter_limit/>
                <p class=style::help>"The number of segmenters (channels) that will work-ahead (transcode at maximum speed) simultaneously, if multiple channels are being watched"</p>
            </Field>
            <Field label="HLS Segmenter Initial Segment Count">
                <input class=style::input name="initial_segment_count" type="number" min="1" required value=form.initial_segment_count/>
                <p class=style::help>"Delays stream start until the specified number of (4-second) segments have been transcoded; a larger number will mean slower initial playback, but potentially less buffering"</p>
            </Field>
            <Field label="HLS Direct Output Format">
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
            <Field label="Default MPEG-TS Script">
                <Select
                    name="default_mpeg_ts_script"
                    selected=form.default_mpeg_ts_script
                    options=data.mpeg_ts_scripts
                    include_none=false
                />
                <p class=style::help>"The MPEG-TS script to use when streaming"</p>
            </Field>
        </SettingsForm>
        <section class=style::content>
            <h2 class=style::section_title>"Custom Resolutions"</h2>
            <div class=style::divider></div>
            <form method="post" action="/api/settings/ffmpeg/resolutions/add" class=style::inline_form>
                <input class=format!("{} {}", style::input, style::inline_input) name="width" type="number" min="1" required placeholder="Width" aria-label="Custom resolution width"/>
                <input class=format!("{} {}", style::input, style::inline_input) name="height" type="number" min="1" required placeholder="Height" aria-label="Custom resolution height"/>
                <button class=style::button type="submit">
                    <span style=icon_style(IconName::Add, "1rem") aria-hidden="true"></span>
                    "Add Custom Resolution"
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
                                            <button class=style::icon_button type="submit" aria-label="Delete custom resolution">
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
        <SettingsForm title="Logging" action="/api/settings/logging/save">
            <SelectField label="Default Minimum Log Level" name="default_minimum_log_level" selected=form.default_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label="Scanning Minimum Log Level" name="scanning_minimum_log_level" selected=form.scanning_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label="Scheduling Minimum Log Level" name="scheduling_minimum_log_level" selected=form.scheduling_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label="Searching Minimum Log Level" name="searching_minimum_log_level" selected=form.searching_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label="Streaming Minimum Log Level" name="streaming_minimum_log_level" selected=form.streaming_minimum_log_level options=levels.clone() include_none=false/>
            <SelectField label="Request Logging Minimum Log Level" name="http_minimum_log_level" selected=form.http_minimum_log_level options=levels include_none=false/>
        </SettingsForm>
    }
}

fn render_hdhr(form: HdhrSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title="HDHomeRun" action="/api/settings/hdhr/save">
            <Field label="UUID">
                <input class=style::input name="uuid" disabled value=form.uuid/>
            </Field>
            <Field label="Tuner Count">
                <input class=style::input name="tuner_count" type="number" min="1" required value=form.tuner_count/>
            </Field>
        </SettingsForm>
    }
}

fn render_scanner(form: ScannerSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title="Scanner" action="/api/settings/scanner/save">
            <Field label="Library Refresh Interval">
                <input class=style::input name="library_refresh_interval" type="number" min="0" max="999999" required value=form.library_refresh_interval/>
            </Field>
        </SettingsForm>
    }
}

fn render_playout(form: PlayoutSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title="Playout" action="/api/settings/playout/save">
            <Field label="Days To Build">
                <input class=style::input name="days_to_build" type="number" min="1" required value=form.days_to_build/>
            </Field>
            <CheckboxField
                label="Skip Missing Items"
                name="skip_missing_items"
                checked=form.skip_missing_items
                caption=Some("Controls whether file-not-found or unavailable items should be included in playouts")
            />
            <Field label="Scripted Schedule Timeout">
                <input class=style::input name="scripted_schedule_timeout_seconds" type="number" min="10" required value=form.scripted_schedule_timeout_seconds/>
            </Field>
        </SettingsForm>
    }
}

fn render_ui(form: UiSettingsForm) -> impl IntoView {
    view! {
        <SettingsForm title="UI" action="/api/settings/ui/save">
            <SelectField
                label="Theme"
                name="is_dark_mode"
                selected=if form.is_dark_mode { "true".to_owned() } else { "false".to_owned() }
                options=vec![
                    SelectOption { value: "true".to_owned(), label: "Dark".to_owned() },
                    SelectOption { value: "false".to_owned(), label: "Light".to_owned() },
                ]
                include_none=false
            />
            <SelectField
                label="Language"
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
        <SettingsForm title="XMLTV" action="/api/settings/xmltv/save">
            <Field label="Days To Build">
                <input class=style::input name="days_to_build" type="number" min="1" required value=form.days_to_build/>
            </Field>
            <SelectField
                label="XMLTV Time Zone"
                name="time_zone"
                selected=form.time_zone
                options=vec![
                    SelectOption { value: "Local".to_owned(), label: "Local".to_owned() },
                    SelectOption { value: "Utc".to_owned(), label: "UTC".to_owned() },
                ]
                include_none=false
            />
            <SelectField
                label="XMLTV Block Behavior"
                name="block_behavior"
                selected=form.block_behavior
                options=vec![
                    SelectOption { value: "SplitTimeEvenly".to_owned(), label: "Split Time Evenly".to_owned() },
                    SelectOption { value: "UseActualTimes".to_owned(), label: "Use Actual Times".to_owned() },
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
                        "Save Settings"
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
                <option value="" selected=selected.is_empty()>"(none)"</option>
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
    ["Debug", "Information", "Warning", "Error"]
        .into_iter()
        .map(|level| SelectOption {
            value: level.to_owned(),
            label: level.to_owned(),
        })
        .collect()
}

pub fn supported_ui_language_options() -> Vec<SelectOption> {
    [
        ("en-us", "English"),
        ("pl", "Polski"),
        ("pt-br", "Português (Brasil)"),
    ]
    .into_iter()
    .map(|(value, label)| SelectOption {
        value: value.to_owned(),
        label: label.to_owned(),
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

        assert_eq!(values, vec!["en-us", "pl", "pt-br"]);
    }
}
