use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use serde::Serialize;

use crate::admin::i18n::{settings_error_text, text, TextKey, UiLanguage};
use crate::admin::server_fns::settings::{
    create_custom_resolution_with_state, delete_custom_resolution_with_state,
    save_ffmpeg_settings_with_state, save_hdhr_settings_with_state,
    save_logging_settings_with_state, save_playout_settings_with_state,
    save_scanner_settings_with_state, save_ui_settings_with_state, save_xmltv_settings_with_state,
    set_windows_startup_with_state, FfmpegSettingsForm, HdhrSettingsForm, LoggingSettingsForm,
    PlayoutSettingsForm, ScannerSettingsForm, UiSettingsForm, XmltvSettingsForm,
};
use crate::host::state::AppState;

#[derive(Serialize)]
pub struct UiThemeResponse {
    theme: &'static str,
}

pub async fn ui_theme(State(state): State<AppState>) -> Response {
    let is_dark_mode = match crate::admin::server_fns::settings::load_bool_for_api(
        state.application.ports().admin_settings(),
        ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
        true,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "failed to load admin UI theme");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    Json(UiThemeResponse {
        theme: if is_dark_mode { "dark" } else { "light" },
    })
    .into_response()
}

pub async fn save_ffmpeg_settings_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    let form = FfmpegSettingsForm {
        ffmpeg_path: field(&fields, "ffmpeg_path"),
        ffprobe_path: field(&fields, "ffprobe_path"),
        default_ffmpeg_profile_id: parse_i64(&fields, "default_ffmpeg_profile_id"),
        preferred_audio_language_code: field(&fields, "preferred_audio_language_code"),
        preferred_subtitle_language_code: field(&fields, "preferred_subtitle_language_code"),
        fallback_subtitle_language_code: field(&fields, "fallback_subtitle_language_code"),
        use_embedded_subtitles: checked(&fields, "use_embedded_subtitles"),
        extract_embedded_subtitles: checked(&fields, "extract_embedded_subtitles"),
        probe_for_interlaced_frames: checked(&fields, "probe_for_interlaced_frames"),
        save_reports: checked(&fields, "save_reports"),
        global_watermark_id: parse_optional_i64(&fields, "global_watermark_id"),
        global_fallback_filler_id: parse_optional_i64(&fields, "global_fallback_filler_id"),
        hls_segmenter_idle_timeout: parse_i32(&fields, "hls_segmenter_idle_timeout"),
        work_ahead_segmenter_limit: parse_i32(&fields, "work_ahead_segmenter_limit"),
        active_transcode_limit: parse_i32(&fields, "active_transcode_limit"),
        initial_segment_count: parse_i32(&fields, "initial_segment_count"),
        hls_direct_output_format: parse_i32(&fields, "hls_direct_output_format"),
        default_mpeg_ts_script: field(&fields, "default_mpeg_ts_script"),
    };
    redirect_or_error(
        save_ffmpeg_settings_with_state(&state, form).await,
        "/settings/ffmpeg",
        &state,
    )
    .await
}

pub async fn save_logging_settings_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    let form = LoggingSettingsForm {
        default_minimum_log_level: field(&fields, "default_minimum_log_level"),
        scanning_minimum_log_level: field(&fields, "scanning_minimum_log_level"),
        scheduling_minimum_log_level: field(&fields, "scheduling_minimum_log_level"),
        searching_minimum_log_level: field(&fields, "searching_minimum_log_level"),
        streaming_minimum_log_level: field(&fields, "streaming_minimum_log_level"),
        http_minimum_log_level: field(&fields, "http_minimum_log_level"),
    };
    redirect_or_error(
        save_logging_settings_with_state(&state, form).await,
        "/settings/logging",
        &state,
    )
    .await
}

pub async fn save_hdhr_settings_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    let form = HdhrSettingsForm {
        uuid: field(&fields, "uuid"),
        tuner_count: parse_i32(&fields, "tuner_count"),
    };
    redirect_or_error(
        save_hdhr_settings_with_state(&state, form).await,
        "/settings/hdhr",
        &state,
    )
    .await
}

pub async fn save_scanner_settings_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    let form = ScannerSettingsForm {
        library_refresh_interval: parse_i32(&fields, "library_refresh_interval"),
    };
    redirect_or_error(
        save_scanner_settings_with_state(&state, form).await,
        "/settings/scanner",
        &state,
    )
    .await
}

pub async fn save_playout_settings_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    let form = PlayoutSettingsForm {
        days_to_build: parse_i32(&fields, "days_to_build"),
        skip_missing_items: checked(&fields, "skip_missing_items"),
        scripted_schedule_timeout_seconds: parse_i32(&fields, "scripted_schedule_timeout_seconds"),
    };
    redirect_or_error(
        save_playout_settings_with_state(&state, form).await,
        "/settings/playout",
        &state,
    )
    .await
}

pub async fn save_ui_settings_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    let form = UiSettingsForm {
        is_dark_mode: field(&fields, "is_dark_mode") == "true",
        language: field(&fields, "language"),
        date_format: field(&fields, "date_format"),
        time_format: field(&fields, "time_format"),
    };
    let result = save_ui_settings_with_state(&state, form).await;
    let result = match (
        result,
        optional_checked(
            &fields,
            "windows_startup_present",
            "windows_startup_enabled",
        ),
    ) {
        (Ok(()), Some(enabled)) => set_windows_startup_with_state(&state, enabled),
        (result, _) => result,
    };
    redirect_or_error(result, "/settings/ui", &state).await
}

pub async fn save_windows_startup_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    redirect_or_error(
        set_windows_startup_with_state(&state, checked(&fields, "windows_startup_enabled")),
        "/settings/ui",
        &state,
    )
    .await
}

pub async fn save_xmltv_settings_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    let form = XmltvSettingsForm {
        days_to_build: parse_i32(&fields, "days_to_build"),
        time_zone: field(&fields, "time_zone"),
        block_behavior: field(&fields, "block_behavior"),
    };
    redirect_or_error(
        save_xmltv_settings_with_state(&state, form).await,
        "/settings/xmltv",
        &state,
    )
    .await
}

pub async fn add_custom_resolution_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    redirect_or_error(
        create_custom_resolution_with_state(
            &state,
            parse_i32(&fields, "width"),
            parse_i32(&fields, "height"),
        )
        .await,
        "/settings/ffmpeg",
        &state,
    )
    .await
}

pub async fn delete_custom_resolution_form(State(state): State<AppState>, body: Bytes) -> Response {
    let fields = parse_form(&body);
    redirect_or_error(
        delete_custom_resolution_with_state(&state, parse_i64(&fields, "id")).await,
        "/settings/ffmpeg",
        &state,
    )
    .await
}

async fn redirect_or_error(
    result: Result<(), leptos::server_fn::ServerFnError>,
    success_path: &str,
    state: &AppState,
) -> Response {
    match result {
        Ok(()) => Redirect::to(success_path).into_response(),
        Err(error) => {
            let language = load_error_language(state).await;
            settings_error_response(success_path, &error.to_string(), language)
        }
    }
}

async fn load_error_language(state: &AppState) -> UiLanguage {
    match crate::admin::server_fns::settings::load_ui_language_for_api(
        state.application.ports().admin_settings(),
    )
    .await
    {
        Ok(language) => language,
        Err(error) => {
            tracing::warn!(%error, "failed to load UI language for settings API error");
            UiLanguage::English
        }
    }
}

fn settings_error_response(back_path: &str, message: &str, language: UiLanguage) -> Response {
    let title = text(language, TextKey::SettingsSaveFailed);
    let back = text(language, TextKey::BackToSettings);
    let message = settings_error_text(language, message);
    (
        StatusCode::BAD_REQUEST,
        Html(format!(
            r#"<!doctype html>
<html lang="{}">
<head><meta charset="utf-8"><title>{}</title></head>
<body>
<main>
<h1>{}</h1>
<p role="alert">{}</p>
<p><a href="{}">{}</a></p>
</main>
</body>
</html>"#,
            language.code(),
            escape_html(title),
            escape_html(title),
            escape_html(&message),
            escape_html(back_path),
            escape_html(back),
        )),
    )
        .into_response()
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn parse_form(body: &[u8]) -> Vec<(String, String)> {
    String::from_utf8_lossy(body)
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (decode_form_part(key), decode_form_part(value))
        })
        .collect()
}

fn decode_form_part(value: &str) -> String {
    urlencoding::decode(&value.replace('+', " "))
        .map(std::borrow::Cow::into_owned)
        .unwrap_or_default()
}

fn field(fields: &[(String, String)], name: &str) -> String {
    fields
        .iter()
        .rev()
        .find_map(|(key, value)| (key == name).then(|| value.clone()))
        .unwrap_or_default()
}

fn checked(fields: &[(String, String)], name: &str) -> bool {
    fields
        .iter()
        .any(|(key, value)| key == name && matches!(value.as_str(), "true" | "on" | "1" | "True"))
}

fn optional_checked(
    fields: &[(String, String)],
    present_name: &str,
    checked_name: &str,
) -> Option<bool> {
    fields
        .iter()
        .any(|(key, _)| key == present_name)
        .then(|| checked(fields, checked_name))
}

fn parse_i32(fields: &[(String, String)], name: &str) -> i32 {
    field(fields, name).parse().unwrap_or_default()
}

fn parse_i64(fields: &[(String, String)], name: &str) -> i64 {
    field(fields, name).parse().unwrap_or_default()
}

fn parse_optional_i64(fields: &[(String, String)], name: &str) -> Option<i64> {
    let value = field(fields, name);
    if value.trim().is_empty() {
        None
    } else {
        value.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkboxes_default_to_false_when_missing() {
        let fields = parse_form(b"days_to_build=2&scripted_schedule_timeout_seconds=30");
        assert!(!checked(&fields, "skip_missing_items"));
    }

    #[test]
    fn windows_startup_checkbox_accepts_browser_checkbox_values() {
        assert!(checked(
            &parse_form(b"windows_startup_enabled=on"),
            "windows_startup_enabled"
        ));
        assert!(!checked(&parse_form(b""), "windows_startup_enabled"));
    }

    #[test]
    fn optional_checkbox_distinguishes_unchecked_from_absent_controls() {
        assert_eq!(
            optional_checked(
                &parse_form(b"windows_startup_present=true&windows_startup_enabled=on"),
                "windows_startup_present",
                "windows_startup_enabled"
            ),
            Some(true)
        );
        assert_eq!(
            optional_checked(
                &parse_form(b"windows_startup_present=true"),
                "windows_startup_present",
                "windows_startup_enabled"
            ),
            Some(false)
        );
        assert_eq!(
            optional_checked(
                &parse_form(b"is_dark_mode=true&language=en-us"),
                "windows_startup_present",
                "windows_startup_enabled"
            ),
            None
        );
    }

    #[test]
    fn optional_ids_accept_empty_selects() {
        let fields = parse_form(b"global_watermark_id=&global_fallback_filler_id=42");
        assert_eq!(parse_optional_i64(&fields, "global_watermark_id"), None);
        assert_eq!(
            parse_optional_i64(&fields, "global_fallback_filler_id"),
            Some(42)
        );
    }

    #[test]
    fn settings_error_page_escapes_message_and_back_path() {
        let html = escape_html(r#"<bad attr="x">&'"#);
        assert_eq!(html, "&lt;bad attr=&quot;x&quot;&gt;&amp;&#39;");
    }

    #[tokio::test]
    async fn settings_error_page_uses_selected_language() {
        let response = settings_error_response(
            "/settings/hdhr",
            "Tuner count must be greater than zero",
            UiLanguage::Dutch,
        );
        let body = response.into_body();
        let bytes = axum::body::to_bytes(body, usize::MAX)
            .await
            .expect("body should collect");
        let html = String::from_utf8(bytes.to_vec()).expect("html should be utf8");

        assert!(html.contains(r#"<html lang="nl">"#));
        assert!(html.contains("Instellingen opslaan mislukt"));
        assert!(html.contains("Aantal tuners moet groter zijn dan nul."));
        assert!(html.contains("Terug naar instellingen"));
    }

    #[tokio::test]
    async fn windows_startup_error_uses_selected_language() {
        let response = settings_error_response(
            "/settings/ui",
            "update Windows startup registration is unavailable: Unable to update Windows startup registration",
            UiLanguage::Dutch,
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should collect");
        let html = String::from_utf8(bytes.to_vec()).expect("html should be utf8");

        assert!(html.contains("De Windows-opstartregistratie kon niet worden bijgewerkt."));
    }
}
