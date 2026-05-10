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
}

/// Form data for channel create / edit.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelForm {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub group: String,
    pub ffmpeg_profile_id: i64,
    /// DB integer value: 1=TS, 2=HLSDirect, 4=HLSSegmenter, 5=TSHybrid.
    pub streaming_mode: i32,
    pub is_enabled: bool,
    pub show_in_epg: bool,
    /// 0 = no fallback filler.
    pub fallback_filler_id: i64,
}

impl ChannelForm {
    #[must_use]
    pub fn new_defaults() -> Self {
        Self {
            is_enabled: true,
            show_in_epg: true,
            streaming_mode: 1,
            ..Default::default()
        }
    }
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
                  c."StreamingMode", COALESCE(p."Name", '') AS "ProfileName"
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
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let row = sqlx::query(r#"SELECT * FROM "Channel" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_one(&state.pool)
        .await?;

    Ok(ChannelForm {
        id: row.try_get("Id").unwrap_or(0),
        number: row.try_get("Number").unwrap_or_default(),
        name: row.try_get("Name").unwrap_or_default(),
        group: row
            .try_get::<Option<String>, _>("Group")
            .unwrap_or_default()
            .unwrap_or_default(),
        ffmpeg_profile_id: row.try_get("FFmpegProfileId").unwrap_or(0),
        streaming_mode: row.try_get("StreamingMode").unwrap_or(1),
        is_enabled: row.try_get("IsEnabled").unwrap_or(true),
        show_in_epg: row.try_get("ShowInEpg").unwrap_or(true),
        fallback_filler_id: row
            .try_get::<Option<i64>, _>("FallbackFillerId")
            .unwrap_or_default()
            .unwrap_or(0),
    })
}

/// Creates or updates a channel. Returns the channel id.
#[server]
pub async fn save_channel(form: ChannelForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_channel_with_pool(&state.pool, form)
        .await
        .map_err(Into::into)
}

#[cfg(feature = "ssr")]
pub async fn save_channel_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: ChannelForm,
) -> Result<i64, sqlx::Error> {
    let sort_number: f64 = form.number.parse().unwrap_or(0.0);
    let group_trimmed = form.group.trim();
    let group = if group_trimmed.is_empty() {
        "ErsatzTV"
    } else {
        group_trimmed
    };
    let filler_id: Option<i64> = if form.fallback_filler_id == 0 {
        None
    } else {
        Some(form.fallback_filler_id)
    };

    if form.id == 0 {
        let result = sqlx::query(
            r#"INSERT INTO "Channel" (
                "UniqueId", "Number", "SortNumber", "Name", "Group",
                "FFmpegProfileId", "StreamingMode", "IsEnabled", "ShowInEpg",
                "FallbackFillerId", "StreamSelectorMode", "SubtitleMode",
                "MusicVideoCreditsMode", "SongVideoMode", "PlayoutSource",
                "PlayoutMode", "TranscodeMode", "IdleBehavior"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0, 0, 0, 0)"#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&form.number)
        .bind(sort_number)
        .bind(&form.name)
        .bind(group)
        .bind(form.ffmpeg_profile_id)
        .bind(form.streaming_mode)
        .bind(form.is_enabled)
        .bind(form.show_in_epg)
        .bind(filler_id)
        .execute(pool)
        .await?;

        Ok(ersatzrs_infra::db::last_insert_id(result))
    } else {
        sqlx::query(
            r#"UPDATE "Channel" SET
                "Number" = ?, "SortNumber" = ?, "Name" = ?, "Group" = ?,
                "FFmpegProfileId" = ?, "StreamingMode" = ?,
                "IsEnabled" = ?, "ShowInEpg" = ?, "FallbackFillerId" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&form.number)
        .bind(sort_number)
        .bind(&form.name)
        .bind(group)
        .bind(form.ffmpeg_profile_id)
        .bind(form.streaming_mode)
        .bind(form.is_enabled)
        .bind(form.show_in_epg)
        .bind(filler_id)
        .bind(form.id)
        .execute(pool)
        .await?;

        Ok(form.id)
    }
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
