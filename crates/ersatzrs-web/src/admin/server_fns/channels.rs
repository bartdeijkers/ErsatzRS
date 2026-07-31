use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Lightweight row type for the channel list table.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelListRow {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub preferred_audio_language_code: String,
    pub preferred_audio_language: String,
    pub is_enabled: bool,
    pub show_in_epg: bool,
    pub streaming_mode: i32,
    pub streaming_mode_label: String,
    pub ffmpeg_profile_name: String,
    pub playout_count: i64,
    pub logo_url: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelWatchAvailability {
    Ready,
    Disabled,
    NoPlayout,
    NotCurrentlyAiring,
    NotFound,
    #[default]
    Failed,
}

impl ChannelWatchAvailability {
    #[must_use]
    pub const fn can_play(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelWatchProgramme {
    pub id: i64,
    pub title: String,
    pub start: String,
    pub finish: String,
    pub filler_kind: i32,
    pub is_gap: bool,
    pub is_current: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelWatchData {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub logo_url: String,
    pub ffmpeg_profile_name: String,
    pub streaming_mode: i32,
    pub generated_at: String,
    pub window_start: String,
    pub window_finish: String,
    pub availability: ChannelWatchAvailability,
    pub programmes: Vec<ChannelWatchProgramme>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EpgPreviewChannelData {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub logo_url: String,
    pub ffmpeg_profile_name: String,
    pub streaming_mode: i32,
    pub is_active: bool,
    pub programmes: Vec<ChannelWatchProgramme>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EpgPreviewData {
    pub generated_at: String,
    pub window_start: String,
    pub window_finish: String,
    pub channels: Vec<EpgPreviewChannelData>,
}

#[cfg(feature = "ssr")]
impl ChannelWatchData {
    fn from_core(
        state: ersatzrs_core::repositories::ChannelWatchState,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let has_current = state
            .programmes
            .iter()
            .any(|programme| !programme.is_gap && programme.start <= now && programme.finish > now);
        let availability = if !state.is_enabled {
            ChannelWatchAvailability::Disabled
        } else if state.playout_id.is_none() {
            ChannelWatchAvailability::NoPlayout
        } else if !has_current {
            ChannelWatchAvailability::NotCurrentlyAiring
        } else {
            ChannelWatchAvailability::Ready
        };
        let logo_url = channel_logo_url(&state.name, state.logo_path.as_deref());
        Self {
            id: state.id,
            number: state.number,
            name: state.name,
            logo_url,
            ffmpeg_profile_name: state.ffmpeg_profile_name,
            streaming_mode: state.streaming_mode,
            generated_at: now.to_rfc3339(),
            window_start: (now - chrono::Duration::hours(2)).to_rfc3339(),
            window_finish: (now + chrono::Duration::hours(24)).to_rfc3339(),
            availability,
            programmes: state
                .programmes
                .into_iter()
                .map(|programme| ChannelWatchProgramme {
                    id: programme.id,
                    title: programme.title,
                    start: programme.start.to_rfc3339(),
                    finish: programme.finish.to_rfc3339(),
                    filler_kind: programme.filler_kind,
                    is_gap: programme.is_gap,
                    is_current: !programme.is_gap
                        && programme.start <= now
                        && programme.finish > now,
                })
                .collect(),
        }
    }

    fn not_found(id: i64, now: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            id,
            generated_at: now.to_rfc3339(),
            window_start: (now - chrono::Duration::hours(2)).to_rfc3339(),
            window_finish: (now + chrono::Duration::hours(24)).to_rfc3339(),
            availability: ChannelWatchAvailability::NotFound,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelNumberEditorRow {
    pub id: i64,
    pub number: String,
    pub original_number: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelNumberFormUpdate {
    pub id: i64,
    pub number: String,
}

/// Form data for channel create / edit.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelForm {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub group: String,
    pub categories: String,
    pub ffmpeg_profile_id: i64,
    pub slug_seconds: Option<f64>,
    pub logo_path: String,
    pub logo_content_type: String,
    pub external_logo_url: String,
    pub stream_selector_mode: i32,
    pub stream_selector: String,
    pub preferred_audio_language_code: String,
    pub preferred_audio_title: String,
    pub playout_source: i32,
    pub playout_mode: i32,
    pub mirror_source_channel_id: i64,
    pub playout_offset_hours: i32,
    /// DB integer value: 1=TS, 2=HLSDirect, 4=HLSSegmenter, 5=TSHybrid.
    pub streaming_mode: i32,
    pub watermark_id: i64,
    pub is_enabled: bool,
    pub show_in_epg: bool,
    /// 0 = no fallback filler.
    pub fallback_filler_id: i64,
    pub preferred_subtitle_language_code: String,
    pub subtitle_mode: i32,
    pub music_video_credits_mode: i32,
    pub music_video_credits_template: String,
    pub song_video_mode: i32,
    pub transcode_mode: i32,
    pub idle_behavior: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelSelectOption {
    pub id: i64,
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelMirrorOption {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub playout_source: i32,
    pub playout_count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelEditorData {
    pub form: ChannelForm,
    pub ffmpeg_profiles: Vec<ChannelSelectOption>,
    pub watermarks: Vec<ChannelSelectOption>,
    pub fallback_fillers: Vec<ChannelSelectOption>,
    pub mirror_channels: Vec<ChannelMirrorOption>,
    pub language_codes: Vec<ChannelSelectOption>,
    pub stream_selectors: Vec<String>,
    pub music_video_credit_templates: Vec<String>,
    pub playout_count: i64,
}

impl ChannelForm {
    #[must_use]
    pub fn new_defaults() -> Self {
        Self {
            is_enabled: true,
            show_in_epg: true,
            streaming_mode: 5,
            name: "New Channel".to_string(),
            group: "ErsatzRS".to_string(),
            ..Default::default()
        }
    }

    #[cfg(feature = "ssr")]
    fn from_core(form: ersatzrs_core::repositories::ChannelEditorForm) -> Self {
        Self {
            id: form.id,
            number: form.number,
            name: form.name,
            group: form.group,
            categories: form.categories.unwrap_or_default(),
            ffmpeg_profile_id: form.ffmpeg_profile_id,
            slug_seconds: form.slug_seconds,
            logo_path: form.logo.path.unwrap_or_default(),
            logo_content_type: form.logo.content_type.unwrap_or_default(),
            external_logo_url: form.external_logo_url.unwrap_or_default(),
            stream_selector_mode: form.stream_selector_mode,
            stream_selector: form.stream_selector.unwrap_or_default(),
            preferred_audio_language_code: form.preferred_audio_language_code.unwrap_or_default(),
            preferred_audio_title: form.preferred_audio_title.unwrap_or_default(),
            playout_source: form.playout_source,
            playout_mode: form.playout_mode,
            mirror_source_channel_id: form.mirror_source_channel_id.unwrap_or_default(),
            playout_offset_hours: form
                .playout_offset
                .map(|duration| i32::try_from(duration.num_hours()).unwrap_or(0))
                .unwrap_or_default(),
            streaming_mode: form.streaming_mode,
            watermark_id: form.watermark_id.unwrap_or_default(),
            is_enabled: form.is_enabled,
            show_in_epg: form.show_in_epg,
            fallback_filler_id: form.fallback_filler_id.unwrap_or_default(),
            preferred_subtitle_language_code: form
                .preferred_subtitle_language_code
                .unwrap_or_default(),
            subtitle_mode: form.subtitle_mode,
            music_video_credits_mode: form.music_video_credits_mode,
            music_video_credits_template: form.music_video_credits_template.unwrap_or_default(),
            song_video_mode: form.song_video_mode,
            transcode_mode: form.transcode_mode,
            idle_behavior: form.idle_behavior,
        }
    }

    #[cfg(feature = "ssr")]
    fn into_core(self) -> ersatzrs_core::repositories::ChannelEditorForm {
        let optional_id = |id| if id == 0 { None } else { Some(id) };
        let optional_string = |value: String| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        ersatzrs_core::repositories::ChannelEditorForm {
            id: self.id,
            number: self.number,
            name: self.name,
            group: self.group,
            categories: optional_string(self.categories),
            ffmpeg_profile_id: self.ffmpeg_profile_id,
            slug_seconds: self.slug_seconds,
            logo: ersatzrs_core::repositories::ChannelLogoForm {
                path: optional_string(self.logo_path),
                content_type: optional_string(self.logo_content_type),
            },
            external_logo_url: optional_string(self.external_logo_url),
            stream_selector_mode: self.stream_selector_mode,
            stream_selector: optional_string(self.stream_selector),
            preferred_audio_language_code: optional_string(self.preferred_audio_language_code),
            preferred_audio_title: optional_string(self.preferred_audio_title),
            playout_source: self.playout_source,
            playout_mode: self.playout_mode,
            mirror_source_channel_id: optional_id(self.mirror_source_channel_id),
            playout_offset: if self.playout_offset_hours == 0 {
                None
            } else {
                Some(chrono::Duration::hours(i64::from(
                    self.playout_offset_hours,
                )))
            },
            streaming_mode: self.streaming_mode,
            watermark_id: optional_id(self.watermark_id),
            fallback_filler_id: optional_id(self.fallback_filler_id),
            preferred_subtitle_language_code: optional_string(
                self.preferred_subtitle_language_code,
            ),
            subtitle_mode: self.subtitle_mode,
            music_video_credits_mode: self.music_video_credits_mode,
            music_video_credits_template: optional_string(self.music_video_credits_template),
            song_video_mode: self.song_video_mode,
            transcode_mode: self.transcode_mode,
            idle_behavior: self.idle_behavior,
            is_enabled: self.is_enabled,
            show_in_epg: self.show_in_epg,
        }
    }
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_core::repositories::ChannelEditorState> for ChannelEditorData {
    fn from(state: ersatzrs_core::repositories::ChannelEditorState) -> Self {
        Self {
            form: ChannelForm::from_core(state.form),
            ffmpeg_profiles: select_options_from_core(state.options.ffmpeg_profiles),
            watermarks: select_options_from_core(state.options.watermarks),
            fallback_fillers: select_options_from_core(state.options.fallback_fillers),
            mirror_channels: state
                .options
                .mirror_channels
                .into_iter()
                .map(|option| ChannelMirrorOption {
                    id: option.id,
                    number: option.number,
                    name: option.name,
                    playout_source: option.playout_source,
                    playout_count: option.playout_count,
                })
                .collect(),
            language_codes: select_options_from_core(state.options.language_codes),
            stream_selectors: state.options.stream_selectors,
            music_video_credit_templates: state.options.music_video_credit_templates,
            playout_count: state.playout_count,
        }
    }
}

#[cfg(feature = "ssr")]
fn select_options_from_core(
    options: Vec<ersatzrs_core::repositories::ChannelSelectOption>,
) -> Vec<ChannelSelectOption> {
    options
        .into_iter()
        .map(|option| ChannelSelectOption {
            id: option.id,
            value: option.value.unwrap_or_else(|| option.id.to_string()),
            label: option.label,
        })
        .collect()
}

/// Returns paginated channel rows (excludes disabled when `show_disabled = false`).
#[server]
pub async fn list_channel_rows(
    show_disabled: bool,
    page: u32,
    page_size: u32,
) -> Result<Vec<ChannelListRow>, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::queries::ListChannels;

    let state = expect_context::<AppState>();
    state
        .application
        .list_channels(ListChannels {
            show_disabled,
            page,
            page_size,
        })
        .await
        .map_err(ServerFnError::new)
        .map(|rows| {
            rows.into_iter()
                .map(|row| {
                    let mode = row.streaming_mode;
                    let preferred_audio_language_code =
                        row.preferred_audio_language_code.unwrap_or_default();
                    ChannelListRow {
                        id: row.id,
                        number: row.number,
                        name: row.name.clone(),
                        preferred_audio_language: language_label(Some(
                            &preferred_audio_language_code,
                        )),
                        preferred_audio_language_code,
                        is_enabled: row.is_enabled,
                        show_in_epg: row.show_in_epg,
                        streaming_mode: mode,
                        streaming_mode_label: streaming_mode_label(mode),
                        ffmpeg_profile_name: row.ffmpeg_profile_name,
                        playout_count: row.playout_count,
                        logo_url: channel_logo_url(&row.name, row.logo_path.as_deref()),
                    }
                })
                .collect()
        })
}

/// Returns the total channel count (optionally including disabled).
#[server]
pub async fn count_channels(show_disabled: bool) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .count_channels(show_disabled)
        .await
        .map_err(ServerFnError::new)
}

/// Loads all channels for the channel-number editor.
#[server]
pub async fn list_channel_number_rows() -> Result<Vec<ChannelNumberEditorRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    let rows = state
        .application
        .list_channel_numbers()
        .await
        .map_err(ServerFnError::new)?;
    Ok(rows
        .into_iter()
        .map(|row| ChannelNumberEditorRow {
            id: row.id,
            original_number: row.number.clone(),
            number: row.number,
            name: row.name,
        })
        .collect())
}

/// Loads a channel into `ChannelForm` for editing.
#[server]
pub async fn get_channel_form(id: i64) -> Result<ChannelForm, ServerFnError> {
    Ok(get_channel_editor_data(id).await?.form)
}

/// Loads the complete channel editor state for create/edit.
#[server]
pub async fn get_channel_editor_data(id: i64) -> Result<ChannelEditorData, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::queries::GetChannelEditor;

    let state = expect_context::<AppState>();
    let state = state
        .application
        .get_channel_editor(GetChannelEditor { channel_id: id })
        .await
        .map_err(ServerFnError::new)?
        .ok_or_else(|| ServerFnError::new("Channel not found"))?;
    Ok(ChannelEditorData::from(state))
}

/// Loads channel identity plus two hours of guide history and 24 hours ahead.
#[server]
pub async fn get_channel_watch_data(id: i64) -> Result<ChannelWatchData, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::queries::GetChannelWatch;

    let state = expect_context::<AppState>();
    let now = chrono::Utc::now();
    let data = state
        .application
        .get_channel_watch(GetChannelWatch {
            channel_id: id,
            now,
            horizon_minutes: 1_440,
            history_minutes: 120,
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok(data.map_or_else(
        || ChannelWatchData::not_found(id, now),
        |watch| ChannelWatchData::from_core(watch, now),
    ))
}

/// Loads the enabled, EPG-visible channel lanes for the global preview route.
#[server]
pub async fn get_epg_preview_data() -> Result<EpgPreviewData, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_app::queries::GetEpgPreview;

    let state = expect_context::<AppState>();
    let data = state
        .application
        .get_epg_preview(GetEpgPreview {
            now: chrono::Utc::now(),
        })
        .await
        .map_err(ServerFnError::new)?;
    Ok(EpgPreviewData {
        generated_at: data.generated_at.to_rfc3339(),
        window_start: data.window_start.to_rfc3339(),
        window_finish: data.window_finish.to_rfc3339(),
        channels: data
            .channels
            .into_iter()
            .map(|channel| {
                let state = channel.state;
                EpgPreviewChannelData {
                    id: state.id,
                    number: state.number,
                    logo_url: channel_logo_url(&state.name, state.logo_path.as_deref()),
                    name: state.name,
                    ffmpeg_profile_name: state.ffmpeg_profile_name,
                    streaming_mode: state.streaming_mode,
                    is_active: channel.is_active,
                    programmes: state
                        .programmes
                        .into_iter()
                        .map(|programme| ChannelWatchProgramme {
                            id: programme.id,
                            title: programme.title,
                            start: programme.start.to_rfc3339(),
                            finish: programme.finish.to_rfc3339(),
                            filler_kind: programme.filler_kind,
                            is_gap: programme.is_gap,
                            is_current: !programme.is_gap
                                && programme.start <= data.generated_at
                                && programme.finish > data.generated_at,
                        })
                        .collect(),
                }
            })
            .collect(),
    })
}

/// Returns an on-demand segmenter URL for a streamable channel.
#[server]
pub async fn get_channel_preview_url(id: i64) -> Result<String, ServerFnError> {
    use crate::host::state::AppState;
    use crate::transport::iptv::access_token::AccessToken;

    let watch = get_channel_watch_data(id).await?;
    if !watch.availability.can_play() {
        return Err(ServerFnError::new("Channel is not currently streamable"));
    }

    let state = expect_context::<AppState>();
    let token = state
        .iptv_access_token_verifier
        .issue_preview_token(&watch.number)
        .map_err(|_| ServerFnError::new("Unable to authorize channel preview"))?;
    let access_token = AccessToken::new(token);
    Ok(format!(
        "/iptv/preview/{}/master.m3u8{}",
        urlencoding::encode(&watch.number),
        access_token.query()
    ))
}

/// Creates or updates a channel. Returns the channel id.
#[server]
pub async fn save_channel(form: ChannelForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_channel_with_state(&state, form)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[cfg(feature = "ssr")]
pub async fn save_channel_with_state(
    state: &crate::host::state::AppState,
    form: ChannelForm,
) -> ersatzrs_app::Result<i64> {
    use ersatzrs_core::repositories::ChannelRepository as _;

    let requested_number = form.number.clone();
    let previous_number = if form.id > 0 {
        Some(
            state
                .repositories
                .channel
                .get_with_profile(form.id)
                .await?
                .number,
        )
    } else {
        None
    };
    let id = state
        .application
        .save_channel(ersatzrs_app::commands::SaveChannel::new(form.into_core()))
        .await?;
    if let Some(previous_number) = previous_number.as_deref() {
        state.hls_sessions.playout_updated(previous_number).await;
    }
    if previous_number.as_deref() != Some(requested_number.as_str()) {
        state.hls_sessions.playout_updated(&requested_number).await;
    }
    Ok(id)
}

#[cfg(feature = "ssr")]
pub async fn save_channel_numbers_with_state(
    state: &crate::host::state::AppState,
    updates: Vec<ChannelNumberFormUpdate>,
) -> ersatzrs_app::Result<Vec<i64>> {
    state
        .application
        .save_channel_numbers(ersatzrs_app::commands::SaveChannelNumbers::new(
            updates
                .into_iter()
                .map(|update| ersatzrs_core::repositories::ChannelNumberUpdate {
                    id: update.id,
                    number: update.number,
                })
                .collect(),
        ))
        .await
}

/// Returns the channel numbers that currently have a live transcode session.
///
/// Ports the `IFFmpegSegmenterService.Workers`/`IsActive` lookup that
/// `ErsatzTV`'s `Channels.razor` uses to decide which rows show a stop button.
#[server]
pub async fn list_active_channel_numbers() -> Result<Vec<String>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_active_channel_numbers()
        .await
        .map_err(ServerFnError::new)
}

/// Stops the active transcode session for `number`, returning whether one was
/// running. Ports `Channels.StopChannel` → `IFFmpegSegmenterService.StopChannel`.
#[server]
pub async fn stop_channel_session(number: String) -> Result<bool, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .stop_channel_session(&number)
        .await
        .map_err(ServerFnError::new)
}

/// Deletes a channel by id.
#[server]
pub async fn delete_channel(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_channel(ersatzrs_app::commands::DeleteChannel::new(id))
        .await
        .map_err(ServerFnError::new)
}

/// Returns `(id, name)` pairs for FFmpeg profiles — used to populate selects.
#[server]
pub async fn list_ffmpeg_profiles_for_select() -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_channel_ffmpeg_profiles()
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|option| (option.id, option.label))
        .collect())
}

/// Returns `(id, name)` pairs for filler presets — used to populate selects.
#[server]
pub async fn list_filler_presets_for_select() -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_channel_filler_presets()
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|option| (option.id, option.label))
        .collect())
}

fn streaming_mode_label(mode: i32) -> String {
    match mode {
        1 => "MPEG-TS (Legacy)".to_string(),
        2 => "HLS Direct".to_string(),
        4 => "HLS Segmenter".to_string(),
        5 => "MPEG-TS".to_string(),
        _ => format!("Unknown ({mode})"),
    }
}

fn language_label(code: Option<&str>) -> String {
    match code
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" => String::new(),
        "eng" | "en" => "English".to_string(),
        "nld" | "dut" | "nl" => "Dutch".to_string(),
        "deu" | "ger" | "de" => "German".to_string(),
        "fra" | "fre" | "fr" => "French".to_string(),
        "spa" | "es" => "Spanish".to_string(),
        "ita" | "it" => "Italian".to_string(),
        "jpn" | "ja" => "Japanese".to_string(),
        "kor" | "ko" => "Korean".to_string(),
        "zho" | "chi" | "zh" => "Chinese".to_string(),
        value => value.to_string(),
    }
}

fn channel_logo_url(name: &str, path: Option<&str>) -> String {
    let trimmed = path.unwrap_or_default().trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if let Some(filename) = trimmed.strip_prefix("iptv/logos/") {
        format!("/iptv/logos/{filename}")
    } else if !trimmed.is_empty() {
        format!("/iptv/logos/{trimmed}")
    } else {
        format!(
            "/iptv/logos/gen?text={}",
            urlencoding::encode(name).into_owned()
        )
    }
}

#[cfg(all(test, feature = "ssr", not(feature = "mysql")))]
mod tests {
    use tokio::sync::mpsc;

    use super::*;

    fn channel_state_with_pool_and_playout_rx(
        pool: ersatzrs_infra::db::DbPool,
    ) -> (
        crate::host::state::AppState,
        mpsc::UnboundedReceiver<ersatzrs_core::scheduling::PlayoutBuildRequest>,
    ) {
        let (playout_build_tx, playout_build_rx) = mpsc::unbounded_channel();
        let state = crate::host::state::AppState::new(
            pool,
            ersatzrs_infra::db::DatabaseBackend::Sqlite,
            crate::host::state::SystemStartup {
                database_ready: true,
                schema_status: ersatzrs_infra::db::DatabaseSchemaStatus::default(),
            },
            ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
            uuid::Uuid::nil(),
            1,
            crate::host::state::IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );
        (state, playout_build_rx)
    }

    async fn migrated_channel_state_with_playout_rx() -> (
        crate::host::state::AppState,
        mpsc::UnboundedReceiver<ersatzrs_core::scheduling::PlayoutBuildRequest>,
    ) {
        use sqlx::Executor;

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create sqlite pool");
        pool.execute("PRAGMA foreign_keys = ON")
            .await
            .expect("enable foreign keys");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        pool.execute(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height")
               VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .await
        .expect("seed resolution");

        channel_state_with_pool_and_playout_rx(pool)
    }

    #[tokio::test]
    async fn save_channel_does_not_enqueue_refresh_for_channel_playouts() {
        use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
        use sqlx::Executor;

        let (state, mut playout_build_rx) = migrated_channel_state_with_playout_rx().await;
        let repos = ersatzrs_infra::repositories::make_repositories(state.pool.clone());
        repos
            .ffmpeg_profile
            .upsert(&ersatzrs_domain::FFmpegProfile {
                id: 1,
                name: "default".to_owned(),
                resolution_id: 1,
                ..ersatzrs_domain::FFmpegProfile::default()
            })
            .await
            .unwrap();
        repos
            .channel
            .upsert(&ersatzrs_domain::Channel {
                id: 1,
                unique_id: uuid::Uuid::from_u128(1),
                number: "1".to_owned(),
                sort_number: 1.0,
                name: "Channel 1".to_owned(),
                group: Some("ErsatzRS".to_owned()),
                ffmpeg_profile_id: 1,
                ..ersatzrs_domain::Channel::default()
            })
            .await
            .unwrap();
        state
            .pool
            .execute(
                r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
                   VALUES (10, 1, 'Classic', 0), (11, 1, 'Classic', 1)"#,
            )
            .await
            .unwrap();

        let saved_id = save_channel_with_state(
            &state,
            ChannelForm {
                id: 1,
                number: "1".to_owned(),
                name: "Channel 1".to_owned(),
                group: "ErsatzRS".to_owned(),
                ffmpeg_profile_id: 1,
                streaming_mode: 5,
                subtitle_mode: 3,
                is_enabled: true,
                show_in_epg: true,
                ..ChannelForm::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(saved_id, 1);
        assert!(playout_build_rx.try_recv().is_err());
    }
}
