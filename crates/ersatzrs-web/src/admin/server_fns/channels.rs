use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Lightweight row type for the channel list table.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelListRow {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub preferred_audio_language: String,
    pub is_enabled: bool,
    pub show_in_epg: bool,
    pub streaming_mode_label: String,
    pub ffmpeg_profile_name: String,
    pub logo_url: String,
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
            group: "ErsatzTV".to_string(),
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

#[cfg(feature = "ssr")]
#[derive(Debug, thiserror::Error)]
pub enum ChannelSaveError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] ersatzrs_core::repositories::RepositoryError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Returns paginated channel rows (excludes disabled when `show_disabled = false`).
#[server]
pub async fn list_channel_rows(
    show_disabled: bool,
    page: u32,
    page_size: u32,
) -> Result<Vec<ChannelListRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page) * i64::from(page_size);

    let rows = sqlx::query(
        r#"SELECT c."Id", c."Number", c."Name", c."PreferredAudioLanguageCode",
                  c."IsEnabled", c."ShowInEpg",
                  c."StreamingMode", COALESCE(p."Name", '') AS "ProfileName",
                  (SELECT a."Path" FROM "Artwork" a
                   WHERE a."ChannelId" = c."Id" AND a."ArtworkKind" = 2
                   ORDER BY a."Id" LIMIT 1) AS "LogoPath"
           FROM "Channel" c
           LEFT JOIN "FFmpegProfile" p ON p."Id" = c."FFmpegProfileId"
           WHERE (? OR c."IsEnabled" = 1)
           ORDER BY c."SortNumber", c."Number"
           LIMIT ? OFFSET ?"#,
    )
    .bind(show_disabled)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let list = rows
        .iter()
        .map(|row| {
            let mode: i32 = row.try_get("StreamingMode").unwrap_or(1);
            ChannelListRow {
                id: row.try_get("Id").unwrap_or(0),
                number: row.try_get("Number").unwrap_or_default(),
                name: row.try_get("Name").unwrap_or_default(),
                preferred_audio_language: language_label(
                    row.try_get::<Option<String>, _>("PreferredAudioLanguageCode")
                        .unwrap_or_default()
                        .as_deref(),
                ),
                is_enabled: row.try_get("IsEnabled").unwrap_or(true),
                show_in_epg: row.try_get("ShowInEpg").unwrap_or(true),
                streaming_mode_label: streaming_mode_label(mode),
                ffmpeg_profile_name: row.try_get("ProfileName").unwrap_or_default(),
                logo_url: channel_logo_url(
                    &row.try_get::<String, _>("Name").unwrap_or_default(),
                    row.try_get::<Option<String>, _>("LogoPath")
                        .unwrap_or_default()
                        .as_deref(),
                ),
            }
        })
        .collect();

    Ok(list)
}

/// Returns the total channel count (optionally including disabled).
#[server]
pub async fn count_channels(show_disabled: bool) -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Channel" WHERE (? OR "IsEnabled" = 1)"#)
            .bind(show_disabled)
            .fetch_one(&state.pool)
            .await?;

    Ok(u64::try_from(count).unwrap_or(0))
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
    use ersatzrs_core::repositories::ChannelRepository;

    let state = expect_context::<AppState>();
    let repo = ersatzrs_infra::repositories::SqliteChannelRepository(state.pool.clone());
    let state = repo
        .get_editor_state(if id > 0 { Some(id) } else { None })
        .await?
        .ok_or_else(|| ServerFnError::new("Channel not found"))?;
    Ok(ChannelEditorData::from(state))
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
) -> Result<i64, ChannelSaveError> {
    let before = if form.id > 0 {
        load_channel_save_side_effect_state(&state.pool, form.id).await?
    } else {
        None
    };

    let id = save_channel_with_pool(&state.pool, form).await?;

    if let Some(after) = load_channel_save_side_effect_state(&state.pool, id).await? {
        dispatch_channel_save_side_effects(state, before.as_ref(), &after);
    }

    Ok(id)
}

#[cfg(feature = "ssr")]
pub async fn save_channel_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: ChannelForm,
) -> Result<i64, ChannelSaveError> {
    use ersatzrs_core::repositories::ChannelRepository;

    let repo = ersatzrs_infra::repositories::SqliteChannelRepository(pool.clone());
    let result = repo.save_editor_form(form.into_core()).await?;
    if result.is_success() {
        Ok(result.id)
    } else {
        Err(ChannelSaveError::Validation(
            result
                .validation_errors
                .into_iter()
                .map(|error| format!("{}: {}", error.field, error.message))
                .collect::<Vec<_>>()
                .join("; "),
        ))
    }
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug)]
struct ChannelSaveSideEffectState {
    number: String,
    playout_source: i32,
    mirror_source_channel_id: Option<i64>,
    playout_offset: Option<String>,
    subtitle_mode: i32,
    show_in_epg: bool,
    playout_id: Option<i64>,
}

#[cfg(feature = "ssr")]
impl ChannelSaveSideEffectState {
    fn epg_signature(&self) -> (&str, i32, bool, Option<i64>, Option<&str>) {
        (
            &self.number,
            self.playout_source,
            self.show_in_epg,
            self.mirror_source_channel_id,
            self.playout_offset.as_deref(),
        )
    }
}

#[cfg(feature = "ssr")]
async fn load_channel_save_side_effect_state(
    pool: &ersatzrs_infra::db::DbPool,
    channel_id: i64,
) -> Result<Option<ChannelSaveSideEffectState>, sqlx::Error> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT c."Number", c."PlayoutSource", c."MirrorSourceChannelId",
                  c."PlayoutOffset", c."SubtitleMode", c."ShowInEpg",
                  (SELECT p."Id" FROM "Playout" p WHERE p."ChannelId" = c."Id" ORDER BY p."Id" LIMIT 1) AS "PlayoutId"
           FROM "Channel" c
           WHERE c."Id" = ?"#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| ChannelSaveSideEffectState {
        number: row.try_get("Number").unwrap_or_default(),
        playout_source: row.try_get("PlayoutSource").unwrap_or_default(),
        mirror_source_channel_id: row.try_get("MirrorSourceChannelId").unwrap_or_default(),
        playout_offset: row.try_get("PlayoutOffset").unwrap_or_default(),
        subtitle_mode: row.try_get("SubtitleMode").unwrap_or_default(),
        show_in_epg: row.try_get("ShowInEpg").unwrap_or(false),
        playout_id: row.try_get("PlayoutId").unwrap_or_default(),
    }))
}

#[cfg(feature = "ssr")]
fn dispatch_channel_save_side_effects(
    state: &crate::host::state::AppState,
    before: Option<&ChannelSaveSideEffectState>,
    after: &ChannelSaveSideEffectState,
) {
    if before.is_some_and(|before| before.epg_signature() != after.epg_signature()) {
        tracing::debug!(
            channel_number = %after.number,
            "channel guide data changed; ErsatzRS XMLTV and channel list responses are SQL-backed"
        );
    }

    if after.subtitle_mode == 0 {
        return;
    }

    let Some(playout_id) = after.playout_id else {
        return;
    };

    let pool = state.pool.clone();
    tokio::spawn(async move {
        if let Err(error) =
            ersatzrs_infra::subtitles::extract_embedded_playout_subtitles(&pool, playout_id).await
        {
            tracing::warn!(
                playout_id,
                %error,
                "embedded subtitle extraction failed after channel save"
            );
        }
    });
}

/// Deletes a channel by id.
#[server]
pub async fn delete_channel(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "Channel" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// Returns `(id, name)` pairs for FFmpeg profiles — used to populate selects.
#[server]
pub async fn list_ffmpeg_profiles_for_select() -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(r#"SELECT "Id", "Name" FROM "FFmpegProfile" ORDER BY "Name", "Id""#)
        .fetch_all(&state.pool)
        .await?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get("Id").unwrap_or(0),
                r.try_get("Name").unwrap_or_default(),
            )
        })
        .collect())
}

/// Returns `(id, name)` pairs for filler presets — used to populate selects.
#[server]
pub async fn list_filler_presets_for_select() -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(r#"SELECT "Id", "Name" FROM "FillerPreset" ORDER BY "Name", "Id""#)
        .fetch_all(&state.pool)
        .await?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get("Id").unwrap_or(0),
                r.try_get("Name").unwrap_or_default(),
            )
        })
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
