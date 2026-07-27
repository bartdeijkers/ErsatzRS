use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{
    ChannelAdminListRow, ChannelEditorForm, ChannelEditorOptions, ChannelEditorState,
    ChannelLogoForm, ChannelMirrorOption, ChannelNumberRow, ChannelNumberSaveResult,
    ChannelNumberUpdate, ChannelRepository, ChannelSaveResult, ChannelSaveSideEffects,
    ChannelSelectOption, ChannelValidationError, ChannelValidationRow, ChannelValidationState,
    ChannelWatchProgramme, ChannelWatchState, EpgPreviewChannelState, Result,
};
use ersatzrs_domain::{Channel, EntityId};
use sqlx::{Pool, Row};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::music_video_credits::list_music_video_credit_template_names;

use super::common::{
    decode_bool_column, decode_datetime, decode_duration_text, decode_enum_column, decode_message,
    encode_datetime, encode_duration, encode_enum, load_ids, not_found,
};

#[derive(Clone)]
pub struct SqliteChannelRepository(pub Pool<crate::db::Db>);

#[async_trait]
#[allow(clippy::too_many_lines)]
impl ChannelRepository for SqliteChannelRepository {
    async fn get_with_profile(&self, id: EntityId) -> Result<Channel> {
        let row = sqlx::query(r#"SELECT * FROM "Channel" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("channel", id, error))?;
        let mut channel = map_channel(&row)?;
        channel.playout_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "Playout" WHERE "ChannelId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        channel.artwork_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "Artwork" WHERE "ChannelId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        Ok(channel)
    }

    async fn get_by_number(&self, number: &str) -> Result<Channel> {
        let row = sqlx::query(r#"SELECT * FROM "Channel" WHERE "Number" = ?"#)
            .bind(number)
            .fetch_one(&self.0)
            .await
            .repository()?;
        map_channel(&row)
    }

    async fn list_enabled(&self) -> Result<Vec<Channel>> {
        let rows = sqlx::query(
            r#"SELECT * FROM "Channel" WHERE "IsEnabled" = 1 ORDER BY "SortNumber", "Number""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let mut channel = map_channel(row)?;
                channel.playout_ids.clear();
                channel.artwork_ids.clear();
                Ok(channel)
            })
            .collect()
    }

    async fn upsert(&self, channel: &Channel) -> Result<()> {
        let mut tx = self.0.begin().await.repository()?;
        sqlx::query(
            r#"REPLACE INTO "Channel" (
                "Id", "UniqueId", "Number", "SortNumber", "Name", "Group", "Categories",
                "FFmpegProfileId", "SlugSeconds", "WatermarkId", "FallbackFillerId",
                "StreamingMode", "StreamSelectorMode", "StreamSelector",
                "PreferredAudioLanguageCode", "PreferredAudioTitle",
                "PreferredSubtitleLanguageCode", "SubtitleMode",
                "MusicVideoCreditsMode", "MusicVideoCreditsTemplate",
                "SongVideoMode", "PlayoutSource", "PlayoutMode", "MirrorSourceChannelId",
                "PlayoutOffset", "TranscodeMode", "IdleBehavior", "IsEnabled", "ShowInEpg"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(channel.id)
        .bind(channel.unique_id.to_string())
        .bind(&channel.number)
        .bind(channel.sort_number)
        .bind(&channel.name)
        .bind(&channel.group)
        .bind(&channel.categories)
        .bind(channel.ffmpeg_profile_id)
        .bind(channel.slug_seconds)
        .bind(channel.watermark_id)
        .bind(channel.fallback_filler_id)
        .bind(encode_enum(channel.streaming_mode))
        .bind(encode_enum(channel.stream_selector_mode))
        .bind(&channel.stream_selector)
        .bind(&channel.preferred_audio_language_code)
        .bind(&channel.preferred_audio_title)
        .bind(&channel.preferred_subtitle_language_code)
        .bind(encode_enum(channel.subtitle_mode))
        .bind(encode_enum(channel.music_video_credits_mode))
        .bind(&channel.music_video_credits_template)
        .bind(encode_enum(channel.song_video_mode))
        .bind(encode_enum(channel.playout_source))
        .bind(encode_enum(channel.playout_mode))
        .bind(channel.mirror_source_channel_id)
        .bind(channel.playout_offset.map(encode_duration))
        .bind(encode_enum(channel.transcode_mode))
        .bind(encode_enum(channel.idle_behavior))
        .bind(channel.is_enabled)
        .bind(channel.show_in_epg)
        .execute(&mut *tx)
        .await.repository()?;

        // Update artwork rows that belong to this channel: clear old ones, set new ones.
        // In ErsatzTV, Artwork.ChannelId is a direct FK - we don't use a junction table.
        // We clear the ChannelId on previously linked artwork and set it on the new ones.
        sqlx::query(r#"UPDATE "Artwork" SET "ChannelId" = NULL WHERE "ChannelId" = ?"#)
            .bind(channel.id)
            .execute(&mut *tx)
            .await
            .repository()?;
        for artwork_id in &channel.artwork_ids {
            sqlx::query(r#"UPDATE "Artwork" SET "ChannelId" = ? WHERE "Id" = ?"#)
                .bind(channel.id)
                .bind(artwork_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }

        tx.commit().await.repository()?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        let result = sqlx::query(r#"DELETE FROM "Channel" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "channel",
                id,
            });
        }
        Ok(())
    }

    async fn get_editor_state(&self, id: Option<EntityId>) -> Result<Option<ChannelEditorState>> {
        let options = load_editor_options(&self.0).await?;
        let Some(id) = id.filter(|value| *value > 0) else {
            let number = next_channel_number(&self.0).await?;
            return Ok(Some(ChannelEditorState {
                form: ChannelEditorForm::new_defaults(options.default_ffmpeg_profile_id, number),
                options,
                playout_count: 0,
            }));
        };

        let Some((form, playout_count)) = load_editor_form(&self.0, id).await? else {
            return Ok(None);
        };

        Ok(Some(ChannelEditorState {
            form,
            options,
            playout_count,
        }))
    }

    async fn save_editor_form(&self, form: ChannelEditorForm) -> Result<ChannelSaveResult> {
        let validation_errors = validate_editor_form(&self.0, &form).await?;
        if !validation_errors.is_empty() {
            return Ok(ChannelSaveResult::validation(validation_errors));
        }

        let mut tx = self.0.begin().await.repository()?;
        let now = encode_datetime(Utc::now());
        let normalized = normalize_editor_form(form, &mut tx).await?;
        let group = normalized.group.trim();
        let show_in_epg = normalized.is_enabled && normalized.show_in_epg;
        let sort_number = normalized.number.parse::<f64>().unwrap_or(0.0);
        let playout_offset = normalized.playout_offset.map(encode_signed_duration);

        let id = if normalized.id == 0 {
            let result = sqlx::query(
                r#"INSERT INTO "Channel" (
                    "UniqueId", "Number", "SortNumber", "Name", "Group", "Categories",
                    "FFmpegProfileId", "SlugSeconds", "WatermarkId", "FallbackFillerId",
                    "StreamingMode", "StreamSelectorMode", "StreamSelector",
                    "PreferredAudioLanguageCode", "PreferredAudioTitle",
                    "PreferredSubtitleLanguageCode", "SubtitleMode",
                    "MusicVideoCreditsMode", "MusicVideoCreditsTemplate",
                    "SongVideoMode", "PlayoutSource", "PlayoutMode", "MirrorSourceChannelId",
                    "PlayoutOffset", "TranscodeMode", "IdleBehavior", "IsEnabled", "ShowInEpg"
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&normalized.number)
            .bind(sort_number)
            .bind(&normalized.name)
            .bind(group)
            .bind(&normalized.categories)
            .bind(normalized.ffmpeg_profile_id)
            .bind(normalized.slug_seconds)
            .bind(normalized.watermark_id)
            .bind(normalized.fallback_filler_id)
            .bind(normalized.streaming_mode)
            .bind(normalized.stream_selector_mode)
            .bind(&normalized.stream_selector)
            .bind(&normalized.preferred_audio_language_code)
            .bind(&normalized.preferred_audio_title)
            .bind(&normalized.preferred_subtitle_language_code)
            .bind(normalized.subtitle_mode)
            .bind(normalized.music_video_credits_mode)
            .bind(&normalized.music_video_credits_template)
            .bind(normalized.song_video_mode)
            .bind(normalized.playout_source)
            .bind(normalized.playout_mode)
            .bind(normalized.mirror_source_channel_id)
            .bind(playout_offset)
            .bind(normalized.transcode_mode)
            .bind(normalized.idle_behavior)
            .bind(normalized.is_enabled)
            .bind(show_in_epg)
            .execute(&mut *tx)
            .await.repository()?;
            crate::db::last_insert_id(result)
        } else {
            sqlx::query(
                r#"UPDATE "Channel" SET
                    "Number" = ?, "SortNumber" = ?, "Name" = ?, "Group" = ?,
                    "Categories" = ?, "FFmpegProfileId" = ?, "SlugSeconds" = ?,
                    "WatermarkId" = ?, "FallbackFillerId" = ?, "StreamingMode" = ?,
                    "StreamSelectorMode" = ?, "StreamSelector" = ?,
                    "PreferredAudioLanguageCode" = ?, "PreferredAudioTitle" = ?,
                    "PreferredSubtitleLanguageCode" = ?, "SubtitleMode" = ?,
                    "MusicVideoCreditsMode" = ?, "MusicVideoCreditsTemplate" = ?,
                    "SongVideoMode" = ?, "PlayoutSource" = ?, "PlayoutMode" = ?,
                    "MirrorSourceChannelId" = ?, "PlayoutOffset" = ?,
                    "TranscodeMode" = ?, "IdleBehavior" = ?,
                    "IsEnabled" = ?, "ShowInEpg" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(&normalized.number)
            .bind(sort_number)
            .bind(&normalized.name)
            .bind(group)
            .bind(&normalized.categories)
            .bind(normalized.ffmpeg_profile_id)
            .bind(normalized.slug_seconds)
            .bind(normalized.watermark_id)
            .bind(normalized.fallback_filler_id)
            .bind(normalized.streaming_mode)
            .bind(normalized.stream_selector_mode)
            .bind(&normalized.stream_selector)
            .bind(&normalized.preferred_audio_language_code)
            .bind(&normalized.preferred_audio_title)
            .bind(&normalized.preferred_subtitle_language_code)
            .bind(normalized.subtitle_mode)
            .bind(normalized.music_video_credits_mode)
            .bind(&normalized.music_video_credits_template)
            .bind(normalized.song_video_mode)
            .bind(normalized.playout_source)
            .bind(normalized.playout_mode)
            .bind(normalized.mirror_source_channel_id)
            .bind(playout_offset)
            .bind(normalized.transcode_mode)
            .bind(normalized.idle_behavior)
            .bind(normalized.is_enabled)
            .bind(show_in_epg)
            .bind(normalized.id)
            .execute(&mut *tx)
            .await
            .repository()?;
            normalized.id
        };

        replace_logo_artwork(&mut tx, id, &normalized, &now).await?;
        tx.commit().await.repository()?;
        Ok(ChannelSaveResult::success(id))
    }

    async fn list_number_rows(&self) -> Result<Vec<ChannelNumberRow>> {
        let rows = sqlx::query(
            r#"SELECT "Id", "Number", "Name"
               FROM "Channel"
               ORDER BY "SortNumber", "Number", "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                Ok(ChannelNumberRow {
                    id: row.try_get("Id").repository()?,
                    number: row.try_get("Number").repository()?,
                    name: row.try_get("Name").repository()?,
                })
            })
            .collect()
    }

    async fn save_number_updates(
        &self,
        updates: Vec<ChannelNumberUpdate>,
    ) -> Result<ChannelNumberSaveResult> {
        save_number_updates(&self.0, updates).await
    }

    async fn list_admin_rows(
        &self,
        show_disabled: bool,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<ChannelAdminListRow>> {
        let offset = i64::from(page) * i64::from(page_size);
        let rows = sqlx::query(
            r#"SELECT c."Id", c."Number", c."Name", c."PreferredAudioLanguageCode",
                      c."IsEnabled", c."ShowInEpg", c."StreamingMode",
                      COALESCE(p."Name", '') AS "ProfileName",
                      (SELECT COUNT(*) FROM "Playout" pl WHERE pl."ChannelId" = c."Id") AS "PlayoutCount",
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
        .fetch_all(&self.0)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                Ok(ChannelAdminListRow {
                    id: row.try_get("Id").repository()?,
                    number: row.try_get("Number").repository()?,
                    name: row.try_get("Name").repository()?,
                    preferred_audio_language_code: row
                        .try_get("PreferredAudioLanguageCode")
                        .repository()?,
                    is_enabled: decode_bool_column(row, "IsEnabled")?,
                    show_in_epg: decode_bool_column(row, "ShowInEpg")?,
                    streaming_mode: row.try_get("StreamingMode").repository()?,
                    ffmpeg_profile_name: row.try_get("ProfileName").repository()?,
                    playout_count: row.try_get("PlayoutCount").repository()?,
                    logo_path: row.try_get("LogoPath").repository()?,
                })
            })
            .collect()
    }

    async fn count_admin_rows(&self, show_disabled: bool) -> Result<u64> {
        let count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Channel" WHERE (? OR "IsEnabled" = 1)"#)
                .bind(show_disabled)
                .fetch_one(&self.0)
                .await
                .repository()?;
        u64::try_from(count).map_err(|_| ersatzrs_core::repositories::RepositoryError::Decode {
            kind: "channel count",
            value: count.to_string(),
        })
    }

    async fn get_watch_state(
        &self,
        id: EntityId,
        now: DateTime<Utc>,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Option<ChannelWatchState>> {
        load_watch_state(&self.0, id, now, start, finish).await
    }

    async fn list_epg_preview_states(
        &self,
        now: DateTime<Utc>,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Vec<EpgPreviewChannelState>> {
        load_epg_preview_states(&self.0, now, start, finish).await
    }

    async fn list_ffmpeg_profile_options(&self) -> Result<Vec<ChannelSelectOption>> {
        select_options(
            &self.0,
            r#"SELECT "Id", "Name" FROM "FFmpegProfile" ORDER BY "Name", "Id""#,
        )
        .await
    }

    async fn list_filler_preset_options(&self) -> Result<Vec<ChannelSelectOption>> {
        select_options(
            &self.0,
            r#"SELECT "Id", "Name" FROM "FillerPreset" ORDER BY "Name", "Id""#,
        )
        .await
    }

    async fn validation_state(&self) -> Result<ChannelValidationState> {
        let channel_rows = sqlx::query(
            r#"SELECT c."Id", c."Number", c."Name", c."PlayoutSource",
                      (SELECT COUNT(*) FROM "Playout" p WHERE p."ChannelId" = c."Id") AS "PlayoutCount"
               FROM "Channel" c
               ORDER BY c."Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        let channels = channel_rows
            .iter()
            .map(|row| {
                Ok(ChannelValidationRow {
                    id: row.try_get("Id").repository()?,
                    number: row.try_get("Number").repository()?,
                    name: row.try_get("Name").repository()?,
                    playout_source: row.try_get("PlayoutSource").repository()?,
                    playout_count: row.try_get("PlayoutCount").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ChannelValidationState {
            channels,
            ffmpeg_profile_ids: load_reference_ids(
                &self.0,
                r#"SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id""#,
            )
            .await?,
            watermark_ids: load_reference_ids(
                &self.0,
                r#"SELECT "Id" FROM "ChannelWatermark" ORDER BY "Id""#,
            )
            .await?,
            fallback_filler_ids: load_reference_ids(
                &self.0,
                r#"SELECT "Id" FROM "FillerPreset" WHERE "FillerKind" = 5 ORDER BY "Id""#,
            )
            .await?,
        })
    }

    async fn save_side_effects(&self, id: EntityId) -> Result<Option<ChannelSaveSideEffects>> {
        let Some(row) = sqlx::query(r#"SELECT "SubtitleMode" FROM "Channel" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?
        else {
            return Ok(None);
        };
        let playout_ids =
            sqlx::query_scalar(r#"SELECT "Id" FROM "Playout" WHERE "ChannelId" = ? ORDER BY "Id""#)
                .bind(id)
                .fetch_all(&self.0)
                .await
                .repository()?;
        Ok(Some(ChannelSaveSideEffects {
            subtitle_mode: row.try_get("SubtitleMode").repository()?,
            playout_ids,
        }))
    }
}

async fn load_watch_state(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
    now: DateTime<Utc>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Option<ChannelWatchState>> {
    let Some(channel) = sqlx::query(
        r#"SELECT c."Id", c."Number", c."Name", c."StreamingMode", c."IsEnabled",
                  COALESCE(fp."Name", '') AS "ProfileName",
                  (SELECT a."Path" FROM "Artwork" a
                   WHERE a."ChannelId" = c."Id" AND a."ArtworkKind" = 2
                   ORDER BY a."Id" LIMIT 1) AS "LogoPath"
           FROM "Channel" c
           LEFT JOIN "FFmpegProfile" fp ON fp."Id" = c."FFmpegProfileId"
           WHERE c."Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?
    else {
        return Ok(None);
    };

    let playout_id = select_watch_playout(pool, id, now).await?;
    let programmes = match playout_id {
        Some(playout_id) => load_watch_programmes(pool, playout_id, start, finish).await?,
        None => Vec::new(),
    };

    Ok(Some(ChannelWatchState {
        id: channel.try_get("Id").repository()?,
        number: channel.try_get("Number").repository()?,
        name: channel.try_get("Name").repository()?,
        logo_path: channel.try_get("LogoPath").repository()?,
        ffmpeg_profile_name: channel.try_get("ProfileName").repository()?,
        streaming_mode: channel.try_get("StreamingMode").repository()?,
        is_enabled: decode_bool_column(&channel, "IsEnabled")?,
        playout_id,
        programmes,
    }))
}

async fn select_watch_playout(
    pool: &Pool<crate::db::Db>,
    channel_id: EntityId,
    now: DateTime<Utc>,
) -> Result<Option<EntityId>> {
    let encoded_now = encode_datetime(now);
    let current = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT p."Id"
           FROM "Playout" p
           JOIN "PlayoutItem" pi ON pi."PlayoutId" = p."Id"
           WHERE p."ChannelId" = ? AND pi."Start" <= ? AND pi."Finish" > ?
           ORDER BY pi."Start" DESC, pi."Id" DESC
           LIMIT 1"#,
    )
    .bind(channel_id)
    .bind(&encoded_now)
    .bind(&encoded_now)
    .fetch_optional(pool)
    .await
    .repository()?;

    if current.is_some() {
        return Ok(current);
    }
    let upcoming = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT p."Id"
           FROM "Playout" p
           JOIN "PlayoutItem" pi ON pi."PlayoutId" = p."Id"
           WHERE p."ChannelId" = ? AND pi."Start" > ?
           ORDER BY pi."Start", pi."Id"
           LIMIT 1"#,
    )
    .bind(channel_id)
    .bind(&encoded_now)
    .fetch_optional(pool)
    .await
    .repository()?;
    if upcoming.is_some() {
        return Ok(upcoming);
    }
    sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "Id" FROM "Playout" WHERE "ChannelId" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await
    .repository()
}

async fn load_watch_programmes(
    pool: &Pool<crate::db::Db>,
    playout_id: EntityId,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Vec<ChannelWatchProgramme>> {
    let encoded_start = encode_datetime(start);
    let rows = sqlx::query(
        r#"SELECT * FROM (
                   SELECT 'item' AS "Kind", pi."Id" AS "Id", pi."Start" AS "Start",
                          pi."Finish" AS "Finish", pi."FillerKind" AS "FillerKind"
                   FROM "PlayoutItem" pi
                   WHERE pi."PlayoutId" = ? AND pi."Finish" > ? AND pi."Start" < ?
                   UNION ALL
                   SELECT 'gap' AS "Kind", pg."Id" AS "Id", pg."Start" AS "Start",
                          pg."Finish" AS "Finish", -1 AS "FillerKind"
                   FROM "PlayoutGap" pg
                   WHERE pg."PlayoutId" = ? AND pg."Finish" > ? AND pg."Start" < ?
               ) AS combined
               ORDER BY "Start", "Kind", "Id""#,
    )
    .bind(playout_id)
    .bind(&encoded_start)
    .bind(encode_datetime(finish))
    .bind(playout_id)
    .bind(&encoded_start)
    .bind(encode_datetime(finish))
    .fetch_all(pool)
    .await
    .repository()?;
    let item_ids = rows
        .iter()
        .filter_map(|row| {
            let kind = row.try_get::<String, _>("Kind").ok()?;
            (kind == "item").then(|| row.try_get::<EntityId, _>("Id").ok())?
        })
        .collect::<Vec<_>>();
    let titles = super::playout::load_item_titles_admin(pool, &item_ids).await?;
    rows.iter()
        .map(|row| {
            let kind: String = row.try_get("Kind").repository()?;
            let programme_id: EntityId = row.try_get("Id").repository()?;
            let start_value: String = row.try_get("Start").repository()?;
            let finish_value: String = row.try_get("Finish").repository()?;
            Ok(ChannelWatchProgramme {
                id: programme_id,
                title: if kind == "gap" {
                    String::new()
                } else {
                    titles.get(&programme_id).cloned().unwrap_or_default()
                },
                start: decode_datetime("channel watch programme start", &start_value)?,
                finish: decode_datetime("channel watch programme finish", &finish_value)?,
                filler_kind: row.try_get("FillerKind").repository()?,
                is_gap: kind == "gap",
            })
        })
        .collect()
}

#[derive(Debug)]
struct EpgChannelRow {
    id: EntityId,
    number: String,
    sort_number: f64,
    name: String,
    logo_path: Option<String>,
    ffmpeg_profile_name: String,
    streaming_mode: i32,
}

async fn load_epg_preview_states(
    pool: &Pool<crate::db::Db>,
    now: DateTime<Utc>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Vec<EpgPreviewChannelState>> {
    let channel_rows = sqlx::query(
        r#"SELECT c."Id", c."Number", c."SortNumber", c."Name", c."StreamingMode",
                  COALESCE(fp."Name", '') AS "ProfileName",
                  (SELECT a."Path" FROM "Artwork" a
                   WHERE a."ChannelId" = c."Id" AND a."ArtworkKind" = 2
                   ORDER BY a."Id" LIMIT 1) AS "LogoPath"
           FROM "Channel" c
           LEFT JOIN "FFmpegProfile" fp ON fp."Id" = c."FFmpegProfileId"
           WHERE c."IsEnabled" = 1 AND c."ShowInEpg" = 1
           ORDER BY c."SortNumber", c."Number", c."Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    let channels = channel_rows
        .iter()
        .map(|row| {
            Ok(EpgChannelRow {
                id: row.try_get("Id").repository()?,
                number: row.try_get("Number").repository()?,
                sort_number: row.try_get("SortNumber").repository()?,
                name: row.try_get("Name").repository()?,
                logo_path: row.try_get("LogoPath").repository()?,
                ffmpeg_profile_name: row.try_get("ProfileName").repository()?,
                streaming_mode: row.try_get("StreamingMode").repository()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if channels.is_empty() {
        return Ok(Vec::new());
    }

    let channel_ids = channels
        .iter()
        .map(|channel| channel.id)
        .collect::<HashSet<_>>();
    let playout_rows = sqlx::query(
        r#"SELECT p."Id", p."ChannelId"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE c."IsEnabled" = 1 AND c."ShowInEpg" = 1
           ORDER BY p."ChannelId", p."Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    let mut playouts_by_channel = HashMap::<EntityId, Vec<EntityId>>::new();
    for row in &playout_rows {
        let channel_id: EntityId = row.try_get("ChannelId").repository()?;
        if channel_ids.contains(&channel_id) {
            playouts_by_channel
                .entry(channel_id)
                .or_default()
                .push(row.try_get("Id").repository()?);
        }
    }

    let mut rows_by_playout = load_epg_programmes(pool, start, finish).await?;

    let mut output = Vec::with_capacity(channels.len());
    for channel in channels {
        let playout_ids = playouts_by_channel
            .get(&channel.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let playout_id = select_epg_playout(playout_ids, &rows_by_playout, now);
        let programmes = playout_id
            .and_then(|id| rows_by_playout.remove(&id))
            .unwrap_or_default();
        output.push(EpgPreviewChannelState {
            id: channel.id,
            number: channel.number,
            sort_number: channel.sort_number,
            name: channel.name,
            logo_path: channel.logo_path,
            ffmpeg_profile_name: channel.ffmpeg_profile_name,
            streaming_mode: channel.streaming_mode,
            playout_id,
            programmes,
        });
    }
    Ok(output)
}

async fn load_epg_programmes(
    pool: &Pool<crate::db::Db>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<HashMap<EntityId, Vec<ChannelWatchProgramme>>> {
    let encoded_start = encode_datetime(start);
    let encoded_finish = encode_datetime(finish);
    let programme_rows = sqlx::query(
        r#"SELECT * FROM (
                   SELECT p."ChannelId" AS "ChannelId", pi."PlayoutId" AS "PlayoutId",
                          'item' AS "Kind", pi."Id" AS "Id", pi."Start" AS "Start",
                          pi."Finish" AS "Finish", pi."FillerKind" AS "FillerKind"
                   FROM "PlayoutItem" pi
                   JOIN "Playout" p ON p."Id" = pi."PlayoutId"
                   JOIN "Channel" c ON c."Id" = p."ChannelId"
                   WHERE c."IsEnabled" = 1 AND c."ShowInEpg" = 1
                     AND pi."Finish" > ? AND pi."Start" < ?
                   UNION ALL
                   SELECT p."ChannelId" AS "ChannelId", pg."PlayoutId" AS "PlayoutId",
                          'gap' AS "Kind", pg."Id" AS "Id", pg."Start" AS "Start",
                          pg."Finish" AS "Finish", -1 AS "FillerKind"
                   FROM "PlayoutGap" pg
                   JOIN "Playout" p ON p."Id" = pg."PlayoutId"
                   JOIN "Channel" c ON c."Id" = p."ChannelId"
                   WHERE c."IsEnabled" = 1 AND c."ShowInEpg" = 1
                     AND pg."Finish" > ? AND pg."Start" < ?
               ) AS combined
               ORDER BY "ChannelId", "PlayoutId", "Start", "Kind", "Id""#,
    )
    .bind(&encoded_start)
    .bind(&encoded_finish)
    .bind(&encoded_start)
    .bind(&encoded_finish)
    .fetch_all(pool)
    .await
    .repository()?;

    let mut rows_by_playout = HashMap::<EntityId, Vec<ChannelWatchProgramme>>::new();
    let item_ids = programme_rows
        .iter()
        .filter_map(|row| {
            (row.try_get::<String, _>("Kind").ok()?.as_str() == "item")
                .then(|| row.try_get::<EntityId, _>("Id").ok())?
        })
        .collect::<Vec<_>>();
    let titles = super::playout::load_item_titles_admin(pool, &item_ids).await?;
    for row in &programme_rows {
        let kind: String = row.try_get("Kind").repository()?;
        let programme_id: EntityId = row.try_get("Id").repository()?;
        let start_value: String = row.try_get("Start").repository()?;
        let finish_value: String = row.try_get("Finish").repository()?;
        rows_by_playout
            .entry(row.try_get("PlayoutId").repository()?)
            .or_default()
            .push(ChannelWatchProgramme {
                id: programme_id,
                title: if kind == "gap" {
                    String::new()
                } else {
                    titles.get(&programme_id).cloned().unwrap_or_default()
                },
                start: decode_datetime("EPG preview programme start", &start_value)?,
                finish: decode_datetime("EPG preview programme finish", &finish_value)?,
                filler_kind: row.try_get("FillerKind").repository()?,
                is_gap: kind == "gap",
            });
    }
    Ok(rows_by_playout)
}

fn select_epg_playout(
    playout_ids: &[EntityId],
    rows_by_playout: &HashMap<EntityId, Vec<ChannelWatchProgramme>>,
    now: DateTime<Utc>,
) -> Option<EntityId> {
    playout_ids
        .iter()
        .filter_map(|id| {
            rows_by_playout
                .get(id)?
                .iter()
                .filter(|row| !row.is_gap && row.start <= now && row.finish > now)
                .max_by_key(|row| (row.start, row.id))
                .map(|row| (*id, row.start, row.id))
        })
        .max_by_key(|(_, start, item_id)| (*start, *item_id))
        .map(|(id, _, _)| id)
        .or_else(|| {
            playout_ids
                .iter()
                .filter_map(|id| {
                    rows_by_playout
                        .get(id)?
                        .iter()
                        .filter(|row| !row.is_gap && row.start > now)
                        .min_by_key(|row| (row.start, row.id))
                        .map(|row| (*id, row.start, row.id))
                })
                .min_by_key(|(_, start, item_id)| (*start, *item_id))
                .map(|(id, _, _)| id)
        })
        .or_else(|| playout_ids.first().copied())
}

async fn save_number_updates(
    pool: &Pool<crate::db::Db>,
    updates: Vec<ChannelNumberUpdate>,
) -> Result<ChannelNumberSaveResult> {
    let normalized = normalize_number_updates(updates);
    let validation_errors = validate_number_updates(pool, &normalized).await?;
    if !validation_errors.is_empty() {
        return Ok(ChannelNumberSaveResult::validation(validation_errors));
    }

    let current = load_channel_numbers(pool).await?;
    let changed = normalized
        .into_iter()
        .filter(|update| {
            current
                .get(&update.id)
                .is_some_and(|current_number| current_number != &update.number)
        })
        .collect::<Vec<_>>();

    if changed.is_empty() {
        return Ok(ChannelNumberSaveResult::success(Vec::new()));
    }

    let mut tx = pool.begin().await.repository()?;

    for update in &changed {
        sqlx::query(r#"UPDATE "Channel" SET "Number" = ? WHERE "Id" = ?"#)
            .bind(format!("-{}", update.id))
            .bind(update.id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }

    for update in &changed {
        let sort_number = update.number.parse::<f64>().map_err(|source| {
            ersatzrs_core::repositories::RepositoryError::Decode {
                kind: "channel number",
                value: format!("{} ({source})", update.number),
            }
        })?;
        sqlx::query(r#"UPDATE "Channel" SET "Number" = ?, "SortNumber" = ? WHERE "Id" = ?"#)
            .bind(&update.number)
            .bind(sort_number)
            .bind(update.id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }

    tx.commit().await.repository()?;
    Ok(ChannelNumberSaveResult::success(
        changed.iter().map(|update| update.id).collect(),
    ))
}

fn normalize_number_updates(updates: Vec<ChannelNumberUpdate>) -> Vec<ChannelNumberUpdate> {
    updates
        .into_iter()
        .map(|update| ChannelNumberUpdate {
            id: update.id,
            number: update.number.trim().to_owned(),
        })
        .collect()
}

async fn validate_number_updates(
    pool: &Pool<crate::db::Db>,
    updates: &[ChannelNumberUpdate],
) -> Result<Vec<ChannelValidationError>> {
    let mut errors = Vec::new();
    let mut numbers = HashMap::<String, EntityId>::new();
    let current = load_channel_numbers(pool).await?;
    let submitted_ids = updates
        .iter()
        .map(|update| update.id)
        .collect::<HashSet<_>>();

    for update in updates {
        if update.id <= 0 || !current.contains_key(&update.id) {
            errors.push(ChannelValidationError::new(
                format!("number_{}", update.id),
                "Channel does not exist.",
            ));
            continue;
        }

        if !is_valid_channel_number(&update.number) {
            errors.push(ChannelValidationError::new(
                format!("number_{}", update.id),
                "Invalid channel number; two decimals are allowed for subchannels",
            ));
            continue;
        }

        if let Some(existing_id) = numbers.insert(update.number.clone(), update.id) {
            if existing_id != update.id {
                errors.push(ChannelValidationError::new(
                    format!("number_{}", update.id),
                    "Channel number must be unique",
                ));
            }
        }

        if current.iter().any(|(id, number)| {
            *id != update.id && !submitted_ids.contains(id) && number == &update.number
        }) {
            errors.push(ChannelValidationError::new(
                format!("number_{}", update.id),
                "Channel number must be unique",
            ));
        }
    }

    Ok(errors)
}

async fn load_channel_numbers(pool: &Pool<crate::db::Db>) -> Result<HashMap<EntityId, String>> {
    let rows = sqlx::query(r#"SELECT "Id", "Number" FROM "Channel""#)
        .fetch_all(pool)
        .await
        .repository()?;

    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("Id").repository()?,
                row.try_get("Number").repository()?,
            ))
        })
        .collect()
}

async fn load_editor_form(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
) -> Result<Option<(ChannelEditorForm, i64)>> {
    let Some(row) = sqlx::query(
        r#"SELECT c.*,
                  (SELECT COUNT(*) FROM "Playout" p WHERE p."ChannelId" = c."Id") AS "PlayoutCount",
                  a."Path" AS "LogoPath",
                  a."OriginalContentType" AS "LogoContentType"
           FROM "Channel" c
           LEFT JOIN "Artwork" a ON a."ChannelId" = c."Id" AND a."ArtworkKind" = 2
           WHERE c."Id" = ?
           ORDER BY a."Id"
           LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?
    else {
        return Ok(None);
    };

    let logo_path: Option<String> = row.try_get("LogoPath").repository()?;
    let logo_content_type: Option<String> = row.try_get("LogoContentType").repository()?;
    let is_external_logo = logo_path.as_deref().is_some_and(is_external_logo_url);
    let playout_count = row.try_get("PlayoutCount").repository()?;
    let playout_offset = row
        .try_get::<Option<String>, _>("PlayoutOffset")
        .repository()?
        .as_deref()
        .map(decode_duration_text);

    Ok(Some((
        ChannelEditorForm {
            id: row.try_get("Id").repository()?,
            number: row.try_get("Number").repository()?,
            name: row.try_get("Name").repository()?,
            group: row.try_get("Group").repository()?,
            categories: row.try_get("Categories").repository()?,
            ffmpeg_profile_id: row.try_get("FFmpegProfileId").repository()?,
            slug_seconds: row.try_get("SlugSeconds").repository()?,
            logo: ChannelLogoForm {
                path: if is_external_logo {
                    None
                } else {
                    logo_path.as_ref().map(|path| format!("iptv/logos/{path}"))
                },
                content_type: logo_content_type,
            },
            external_logo_url: if is_external_logo { logo_path } else { None },
            stream_selector_mode: row.try_get("StreamSelectorMode").repository()?,
            stream_selector: row.try_get("StreamSelector").repository()?,
            preferred_audio_language_code: row
                .try_get("PreferredAudioLanguageCode")
                .repository()?,
            preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
            playout_source: row.try_get("PlayoutSource").repository()?,
            playout_mode: row.try_get("PlayoutMode").repository()?,
            mirror_source_channel_id: row.try_get("MirrorSourceChannelId").repository()?,
            playout_offset,
            streaming_mode: row.try_get("StreamingMode").repository()?,
            watermark_id: row.try_get("WatermarkId").repository()?,
            fallback_filler_id: row.try_get("FallbackFillerId").repository()?,
            preferred_subtitle_language_code: row
                .try_get("PreferredSubtitleLanguageCode")
                .repository()?,
            subtitle_mode: row.try_get("SubtitleMode").repository()?,
            music_video_credits_mode: row.try_get("MusicVideoCreditsMode").repository()?,
            music_video_credits_template: row.try_get("MusicVideoCreditsTemplate").repository()?,
            song_video_mode: row.try_get("SongVideoMode").repository()?,
            transcode_mode: row.try_get("TranscodeMode").repository()?,
            idle_behavior: row.try_get("IdleBehavior").repository()?,
            is_enabled: decode_bool_column(&row, "IsEnabled")?,
            show_in_epg: decode_bool_column(&row, "ShowInEpg")?,
        },
        playout_count,
    )))
}

async fn load_editor_options(pool: &Pool<crate::db::Db>) -> Result<ChannelEditorOptions> {
    Ok(ChannelEditorOptions {
        ffmpeg_profiles: select_options(pool, r#"SELECT "Id", "Name" FROM "FFmpegProfile" ORDER BY "Name", "Id""#).await?,
        watermarks: select_options(pool, r#"SELECT "Id", "Name" FROM "ChannelWatermark" ORDER BY "Name", "Id""#).await?,
        fallback_fillers: select_options(
            pool,
            r#"SELECT "Id", "Name" FROM "FillerPreset" WHERE "FillerKind" = 5 ORDER BY "Name", "Id""#,
        )
        .await?,
        mirror_channels: load_mirror_channels(pool).await?,
        language_codes: load_language_codes(pool).await?,
        stream_selectors: crate::file_system_layout::list_channel_stream_selectors().await,
        music_video_credit_templates: list_music_video_credit_template_names().await?,
        default_ffmpeg_profile_id: load_default_ffmpeg_profile_id(pool).await?,
    })
}

async fn select_options(
    pool: &Pool<crate::db::Db>,
    query: &'static str,
) -> Result<Vec<ChannelSelectOption>> {
    let rows = sqlx::query(crate::db::assert_sql_safe(query))
        .fetch_all(pool)
        .await
        .repository()?;
    rows.iter()
        .map(|row| {
            Ok(ChannelSelectOption {
                id: row.try_get("Id").repository()?,
                value: None,
                label: row.try_get("Name").repository()?,
            })
        })
        .collect()
}

async fn load_reference_ids(
    pool: &Pool<crate::db::Db>,
    query: &'static str,
) -> Result<Vec<EntityId>> {
    sqlx::query_scalar(crate::db::assert_sql_safe(query))
        .fetch_all(pool)
        .await
        .repository()
}

async fn load_mirror_channels(pool: &Pool<crate::db::Db>) -> Result<Vec<ChannelMirrorOption>> {
    let rows = sqlx::query(
        r#"SELECT c."Id", c."Number", c."Name", c."PlayoutSource",
                  (SELECT COUNT(*) FROM "Playout" p WHERE p."ChannelId" = c."Id") AS "PlayoutCount"
           FROM "Channel" c
           ORDER BY c."SortNumber", c."Number""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;

    rows.iter()
        .map(|row| {
            Ok(ChannelMirrorOption {
                id: row.try_get("Id").repository()?,
                number: row.try_get("Number").repository()?,
                name: row.try_get("Name").repository()?,
                playout_source: row.try_get("PlayoutSource").repository()?,
                playout_count: row.try_get("PlayoutCount").repository()?,
            })
        })
        .collect()
}

async fn load_language_codes(pool: &Pool<crate::db::Db>) -> Result<Vec<ChannelSelectOption>> {
    let rows = sqlx::query(
        r#"SELECT "LanguageCode"
           FROM (
               SELECT "Language" AS "LanguageCode" FROM "MediaStream" WHERE "Language" IS NOT NULL
               UNION ALL
               SELECT "Language" AS "LanguageCode" FROM "Subtitle" WHERE "Language" IS NOT NULL
               UNION ALL
               SELECT "PreferredAudioLanguageCode" AS "LanguageCode"
               FROM "Channel" WHERE "PreferredAudioLanguageCode" IS NOT NULL
           ) codes
           GROUP BY "LanguageCode"
           ORDER BY COUNT("LanguageCode") DESC, "LanguageCode""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;

    let mut seen = HashSet::new();
    let mut options = Vec::new();
    for row in rows {
        let media_code = row
            .try_get::<String, _>("LanguageCode")
            .repository()?
            .trim()
            .to_string();
        if media_code.is_empty() {
            continue;
        }

        let expanded = ersatzrs_core::language_codes::all_language_codes([media_code.as_str()]);
        let resolved = expanded
            .into_iter()
            .filter(|code| code.len() == 3)
            .find_map(|code| {
                ersatzrs_core::language_codes::english_name(&code).map(|name| (code, name))
            });
        let (code, label) = resolved.unwrap_or_else(|| (media_code.clone(), media_code));
        if seen.insert(code.clone()) {
            options.push(ChannelSelectOption {
                id: i64::try_from(options.len()).unwrap_or_default(),
                value: Some(code),
                label,
            });
        }
    }

    Ok(options)
}

async fn load_default_ffmpeg_profile_id(pool: &Pool<crate::db::Db>) -> Result<EntityId> {
    let configured = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = 'ffmpeg.default_profile_id'"#,
    )
    .fetch_optional(pool)
    .await
    .repository()?
    .flatten()
    .and_then(|value| value.parse::<EntityId>().ok())
    .unwrap_or_default();

    if configured > 0 {
        return Ok(configured);
    }

    Ok(sqlx::query_scalar::<_, Option<EntityId>>(
        r#"SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .repository()?
    .flatten()
    .unwrap_or_default())
}

async fn next_channel_number(pool: &Pool<crate::db::Db>) -> Result<String> {
    let rows = sqlx::query(r#"SELECT "Number" FROM "Channel""#)
        .fetch_all(pool)
        .await
        .repository()?;
    let max_number = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("Number").ok())
        .filter_map(|number| {
            number
                .split('.')
                .next()
                .unwrap_or_default()
                .parse::<i64>()
                .ok()
        })
        .max()
        .unwrap_or_default();
    Ok((max_number + 1).to_string())
}

async fn validate_editor_form(
    pool: &Pool<crate::db::Db>,
    form: &ChannelEditorForm,
) -> Result<Vec<ChannelValidationError>> {
    let mut errors = Vec::new();

    if form.id > 0 {
        let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Channel" WHERE "Id" = ?"#)
            .bind(form.id)
            .fetch_one(pool)
            .await
            .repository()?;
        if exists == 0 {
            errors.push(ChannelValidationError::new("id", "Channel does not exist."));
        }
    }

    if !is_valid_channel_number(&form.number) {
        errors.push(ChannelValidationError::new(
            "number",
            "Invalid channel number; two decimals are allowed for subchannels",
        ));
    }

    let existing_id = sqlx::query_scalar::<_, Option<EntityId>>(
        r#"SELECT "Id" FROM "Channel" WHERE "Number" = ? ORDER BY "Id" LIMIT 1"#,
    )
    .bind(&form.number)
    .fetch_optional(pool)
    .await
    .repository()?
    .flatten();
    if existing_id.is_some_and(|id| id != form.id) {
        errors.push(ChannelValidationError::new(
            "number",
            "Channel number must be unique",
        ));
    }

    let name = form.name.trim();
    if name.is_empty() {
        errors.push(ChannelValidationError::new("name", "Name is required"));
    } else if name.chars().count() > 50 {
        errors.push(ChannelValidationError::new(
            "name",
            "Name must be 50 characters or fewer",
        ));
    }

    if form.group.trim().is_empty() {
        errors.push(ChannelValidationError::new("group", "Group is required"));
    }

    if count_id(pool, "FFmpegProfile", form.ffmpeg_profile_id).await? == 0 {
        errors.push(ChannelValidationError::new(
            "ffmpeg_profile_id",
            format!("FFmpegProfile {} does not exist.", form.ffmpeg_profile_id),
        ));
    }

    if let Some(id) = form.watermark_id {
        if count_id(pool, "ChannelWatermark", id).await? == 0 {
            errors.push(ChannelValidationError::new(
                "watermark_id",
                format!("Watermark {id} does not exist."),
            ));
        }
    }

    if let Some(id) = form.fallback_filler_id {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "FillerPreset" WHERE "Id" = ? AND "FillerKind" = 5"#,
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .repository()?;
        if count == 0 {
            errors.push(ChannelValidationError::new(
                "fallback_filler_id",
                format!("Fallback filler {id} does not exist."),
            ));
        }
    }

    if let Some(url) = form
        .external_logo_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
    {
        if !is_external_logo_url(url) {
            errors.push(ChannelValidationError::new(
                "external_logo_url",
                "External logo url is invalid",
            ));
        }
    }

    if !form.is_enabled && form.show_in_epg {
        errors.push(ChannelValidationError::new(
            "show_in_epg",
            "Disabled channels cannot be shown in EPG",
        ));
    }

    if form.playout_source == 1 {
        validate_mirror_source(pool, form, &mut errors).await?;
    }

    Ok(errors)
}

async fn validate_mirror_source(
    pool: &Pool<crate::db::Db>,
    form: &ChannelEditorForm,
    errors: &mut Vec<ChannelValidationError>,
) -> Result<()> {
    let Some(source_id) = form.mirror_source_channel_id else {
        errors.push(ChannelValidationError::new(
            "mirror_source_channel_id",
            "Mirror source channel does not exist.",
        ));
        return Ok(());
    };

    if source_id == form.id {
        errors.push(ChannelValidationError::new(
            "mirror_source_channel_id",
            "Mirror source channel does not exist.",
        ));
        return Ok(());
    }

    let maybe_source =
        sqlx::query(r#"SELECT "Name", "PlayoutSource" FROM "Channel" WHERE "Id" = ?"#)
            .bind(source_id)
            .fetch_optional(pool)
            .await
            .repository()?;
    match maybe_source {
        None => errors.push(ChannelValidationError::new(
            "mirror_source_channel_id",
            "Mirror source channel does not exist.",
        )),
        Some(row) => {
            let playout_source: i32 = row.try_get("PlayoutSource").repository()?;
            if playout_source != 0 {
                let name: String = row.try_get("Name").repository()?;
                errors.push(ChannelValidationError::new(
                    "mirror_source_channel_id",
                    format!("Mirror source channel {name} must use generated playout source"),
                ));
            }
        }
    }

    if let Some(offset) = form.playout_offset {
        if offset < Duration::hours(-12) || offset > Duration::hours(12) {
            errors.push(ChannelValidationError::new(
                "playout_offset",
                "Playout offset must not be greater than 12 hours",
            ));
        }
    }

    Ok(())
}

async fn count_id(pool: &Pool<crate::db::Db>, table: &str, id: EntityId) -> Result<i64> {
    let query = match table {
        "FFmpegProfile" => r#"SELECT COUNT(*) FROM "FFmpegProfile" WHERE "Id" = ?"#,
        "ChannelWatermark" => r#"SELECT COUNT(*) FROM "ChannelWatermark" WHERE "Id" = ?"#,
        _ => unreachable!("validated table name"),
    };
    sqlx::query_scalar(crate::db::assert_sql_safe(query))
        .bind(id)
        .fetch_one(pool)
        .await
        .repository()
}

async fn normalize_editor_form(
    mut form: ChannelEditorForm,
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
) -> Result<ChannelEditorForm> {
    form.name = form.name.trim().to_owned();
    form.number = form.number.trim().to_owned();
    form.group = form.group.trim().to_owned();
    form.categories = clean_optional(form.categories);
    form.preferred_audio_language_code = clean_optional(form.preferred_audio_language_code);
    form.preferred_audio_title = clean_optional(form.preferred_audio_title);
    form.preferred_subtitle_language_code = clean_optional(form.preferred_subtitle_language_code);
    form.stream_selector = clean_optional(form.stream_selector);

    if form.music_video_credits_mode == 1 {
        form.music_video_credits_template = clean_optional(form.music_video_credits_template);
    } else {
        form.music_video_credits_template = None;
    }

    if form.playout_source == 1 {
        form.playout_mode = 0;
    } else {
        form.mirror_source_channel_id = None;
        form.playout_offset = None;
    }

    if form.id > 0 {
        let playout_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playout" WHERE "ChannelId" = ?"#)
                .bind(form.id)
                .fetch_one(&mut **tx)
                .await
                .repository()?;
        if playout_count > 0 {
            form.playout_source = 0;
            form.mirror_source_channel_id = None;
            form.playout_offset = None;
        }
    }

    Ok(form)
}

async fn replace_logo_artwork(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    channel_id: EntityId,
    form: &ChannelEditorForm,
    now: &str,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "Artwork" WHERE "ChannelId" = ? AND "ArtworkKind" = 2"#)
        .bind(channel_id)
        .execute(&mut **tx)
        .await
        .repository()?;

    let logo_path = form
        .external_logo_url
        .as_ref()
        .and_then(|path| non_empty(path))
        .or_else(|| {
            form.logo
                .path
                .as_ref()
                .and_then(|path| non_empty(path))
                .map(|path| normalize_logo_path(&path))
        });

    let Some(path) = logo_path else {
        return Ok(());
    };

    let original_content_type = if form
        .external_logo_url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty())
    {
        None
    } else {
        clean_optional(form.logo.content_type.clone())
    };

    sqlx::query(
        r#"INSERT INTO "Artwork" (
            "ArtworkKind", "ChannelId", "Path", "OriginalContentType",
            "DateAdded", "DateUpdated"
           ) VALUES (2, ?, ?, ?, ?, ?)"#,
    )
    .bind(channel_id)
    .bind(path)
    .bind(original_content_type)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await
    .repository()?;

    Ok(())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|inner| non_empty(&inner))
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn normalize_logo_path(path: &str) -> String {
    path.strip_prefix("iptv/logos/")
        .unwrap_or(path)
        .trim_start_matches('/')
        .to_owned()
}

fn is_external_logo_url(value: &str) -> bool {
    let trimmed = value.trim();
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"));
    rest.is_some_and(|rest| {
        !rest.is_empty() && !rest.starts_with('/') && !rest.contains(char::is_whitespace)
    })
}

fn is_valid_channel_number(value: &str) -> bool {
    let value = value.trim();
    let Some((whole, fractional)) = value.split_once('.') else {
        return !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit());
    };

    !whole.is_empty()
        && whole.chars().all(|ch| ch.is_ascii_digit())
        && (1..=2).contains(&fractional.len())
        && fractional.chars().all(|ch| ch.is_ascii_digit())
        && !fractional.contains('.')
}

fn encode_signed_duration(value: Duration) -> String {
    let total_seconds = value.num_seconds();
    let sign = if total_seconds < 0 { "-" } else { "" };
    let total_seconds = total_seconds.abs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{sign}{hours:02}:{minutes:02}:{seconds:02}")
}

fn map_channel(row: &DbRow) -> Result<Channel> {
    let unique_id = row.try_get::<String, _>("UniqueId").repository()?;
    Ok(Channel {
        id: row.try_get("Id").repository()?,
        unique_id: Uuid::parse_str(&unique_id)
            .map_err(|source| decode_message("channel unique_id", &source.to_string()))?,
        number: row.try_get("Number").repository()?,
        sort_number: row.try_get("SortNumber").repository()?,
        name: row.try_get("Name").repository()?,
        group: row.try_get("Group").repository()?,
        categories: row.try_get("Categories").repository()?,
        ffmpeg_profile_id: row.try_get("FFmpegProfileId").repository()?,
        ffmpeg_profile: None,
        slug_seconds: row.try_get("SlugSeconds").repository()?,
        watermark_id: row.try_get("WatermarkId").repository()?,
        fallback_filler_id: row.try_get("FallbackFillerId").repository()?,
        fallback_filler: None,
        streaming_mode: decode_enum_column("channel streaming_mode", row, "StreamingMode")?,
        playout_ids: Vec::new(),
        artwork_ids: Vec::new(),
        stream_selector_mode: decode_enum_column(
            "channel stream_selector_mode",
            row,
            "StreamSelectorMode",
        )?,
        stream_selector: row.try_get("StreamSelector").repository()?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode").repository()?,
        preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
        preferred_subtitle_language_code: row
            .try_get("PreferredSubtitleLanguageCode")
            .repository()?,
        subtitle_mode: decode_enum_column("channel subtitle_mode", row, "SubtitleMode")?,
        music_video_credits_mode: decode_enum_column(
            "channel music_video_credits_mode",
            row,
            "MusicVideoCreditsMode",
        )?,
        music_video_credits_template: row.try_get("MusicVideoCreditsTemplate").repository()?,
        song_video_mode: decode_enum_column("channel song_video_mode", row, "SongVideoMode")?,
        playout_source: decode_enum_column("channel playout_source", row, "PlayoutSource")?,
        playout_mode: decode_enum_column("channel playout_mode", row, "PlayoutMode")?,
        mirror_source_channel_id: row.try_get("MirrorSourceChannelId").repository()?,
        playout_offset: row
            .try_get::<Option<String>, _>("PlayoutOffset")
            .repository()?
            .as_deref()
            .map(decode_duration_text),
        transcode_mode: decode_enum_column("channel transcode_mode", row, "TranscodeMode")?,
        idle_behavior: decode_enum_column("channel idle_behavior", row, "IdleBehavior")?,
        is_enabled: decode_bool_column(row, "IsEnabled")?,
        show_in_epg: decode_bool_column(row, "ShowInEpg")?,
    })
}
