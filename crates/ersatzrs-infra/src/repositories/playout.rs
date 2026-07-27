use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{
    PlayoutAlternateScheduleItemAdmin, PlayoutAlternateSchedulesAdmin, PlayoutCreateAdmin,
    PlayoutCreateChannelAdmin, PlayoutCreateScheduleAdmin, PlayoutDateFilterAdmin,
    PlayoutDecoAdmin, PlayoutDecoGroupAdmin, PlayoutDetailAdminItem, PlayoutKindEditorAdmin,
    PlayoutListAdminRow, PlayoutNestedChildOptionAdmin, PlayoutNestedOptionAdmin,
    PlayoutRepository, PlayoutTemplateItemAdmin, PlayoutTemplatesAdmin, RepositoryError, Result,
    SequentialPlayoutAdmin,
};
use ersatzrs_core::scheduling::{next_playout_seed, SchedulingResult};
use ersatzrs_domain::{
    EntityId, Playout, PlayoutAnchor, PlayoutBuildStatus, PlayoutGap, PlayoutHistory, PlayoutItem,
    PlayoutItemGraphicsElement, PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex,
};
use sqlx::{Pool, Row, Transaction};

use super::common::{
    decode_datetime, decode_duration_text, decode_enum_column, decode_message,
    decode_optional_datetime, decode_optional_duration_text, decode_optional_enum_column,
    encode_datetime, encode_duration, encode_enum, encode_optional_datetime, load_ids, not_found,
    replace_ids,
};

#[derive(Clone)]
pub struct SqlitePlayoutRepository(pub Pool<crate::db::Db>);

#[allow(clippy::too_many_lines)]
#[async_trait]
impl PlayoutRepository for SqlitePlayoutRepository {
    async fn get_with_anchor(&self, id: EntityId) -> Result<Playout> {
        let row = sqlx::query(r#"SELECT * FROM "Playout" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("playout", id, error))?;
        let mut playout = map_playout(&row)?;
        playout.item_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "PlayoutItem" WHERE "PlayoutId" = ? ORDER BY "Start", "Id""#,
            id,
        )
        .await?;
        playout.gap_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "PlayoutGap" WHERE "PlayoutId" = ? ORDER BY "Start", "Id""#,
            id,
        )
        .await?;
        playout.program_schedule_anchor_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        playout.fill_group_index_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "PlayoutScheduleItemFillGroupIndex" WHERE "PlayoutId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        playout.playout_history_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "PlayoutHistory" WHERE "PlayoutId" = ? ORDER BY "When", "Id""#,
            id,
        )
        .await?;
        let template_rows = sqlx::query(
            r#"SELECT *
             FROM "PlayoutTemplate"
             WHERE "PlayoutId" = ?
             ORDER BY "Index", "TemplateId""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        playout.templates = template_rows
            .iter()
            .map(map_playout_template)
            .collect::<Result<Vec<_>>>()?;

        let anchor = sqlx::query(r#"SELECT * FROM "PlayoutAnchor" WHERE "PlayoutId" = ?"#)
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?;
        if let Some(ref anchor_row) = anchor {
            let state_row = sqlx::query(
                r#"SELECT "Seed", "Index" FROM "ScheduleItemsEnumeratorState" WHERE "PlayoutAnchorPlayoutId" = ?"#,
            )
            .bind(id)
            .fetch_optional(&self.0)
            .await.repository()?;
            playout.anchor = Some(map_anchor(anchor_row, state_row.as_ref())?);
        }

        let status = sqlx::query(r#"SELECT * FROM "PlayoutBuildStatus" WHERE "PlayoutId" = ?"#)
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?;
        playout.build_status = status.as_ref().map(map_build_status).transpose()?;
        Ok(playout)
    }

    async fn items_in_range(
        &self,
        playout_id: EntityId,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Vec<PlayoutItem>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "PlayoutItem"
             WHERE "PlayoutId" = ? AND "Start" < ? AND "Finish" > ?
             ORDER BY "Start", "Id""#,
        )
        .bind(playout_id)
        .bind(encode_datetime(finish))
        .bind(encode_datetime(start))
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            let item_id: EntityId = row.try_get("Id").repository()?;
            let mut item = map_playout_item(row)?;
            item.watermark_ids = load_ids(
                &self.0,
                r#"SELECT "WatermarkId" FROM "PlayoutItemWatermark" WHERE "PlayoutItemId" = ? ORDER BY "WatermarkId""#,
                item_id,
            )
            .await?;
            item.graphics_element_ids = load_ids(
                &self.0,
                r#"SELECT "GraphicsElementId" FROM "PlayoutItemGraphicsElement" WHERE "PlayoutItemId" = ? ORDER BY "GraphicsElementId""#,
                item_id,
            )
            .await?;
            item.graphics_elements = load_playout_item_graphics_elements(&self.0, item_id).await?;
            items.push(item);
        }
        Ok(items)
    }

    async fn replace_runtime_state(
        &self,
        playout: &Playout,
        gaps: &[PlayoutGap],
        history: &[PlayoutHistory],
        program_schedule_anchors: &[PlayoutProgramScheduleAnchor],
        fill_group_indices: &[PlayoutScheduleItemFillGroupIndex],
    ) -> Result<()> {
        let mut tx = self.0.begin().await.repository()?;

        update_playout_checkpoint(&mut tx, playout).await?;
        replace_anchor_state(&mut tx, playout.id, playout.anchor.as_ref()).await?;
        replace_gaps(&mut tx, playout.id, gaps).await?;
        replace_history(&mut tx, playout.id, history).await?;
        replace_program_schedule_anchors(&mut tx, playout.id, program_schedule_anchors).await?;
        replace_fill_group_indices(&mut tx, playout.id, fill_group_indices).await?;
        replace_playout_templates(&mut tx, playout).await?;

        tx.commit().await.repository()?;
        Ok(())
    }

    async fn upsert_item(&self, item: &PlayoutItem) -> Result<()> {
        let mut tx = self.0.begin().await.repository()?;
        sqlx::query(
            r#"REPLACE INTO "PlayoutItem" (
                "Id", "PlayoutId", "MediaItemId", "Start", "Finish", "GuideStart", "GuideFinish",
                "CustomTitle", "GuideGroup", "FillerKind", "InPoint", "OutPoint",
                "ChapterTitle", "DisableWatermarks", "PreferredAudioLanguageCode",
                "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "SubtitleMode",
                "BlockKey", "CollectionKey", "CollectionEtag", "SchedulingContext"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item.id)
        .bind(item.playout_id)
        .bind(item.media_item_id)
        .bind(encode_datetime(item.start))
        .bind(encode_datetime(item.finish))
        .bind(encode_optional_datetime(item.guide_start))
        .bind(encode_optional_datetime(item.guide_finish))
        .bind(&item.custom_title)
        .bind(item.guide_group)
        .bind(encode_enum(item.filler_kind))
        .bind(encode_duration(item.in_point))
        .bind(encode_duration(item.out_point))
        .bind(&item.chapter_title)
        .bind(item.disable_watermarks)
        .bind(&item.preferred_audio_language_code)
        .bind(&item.preferred_audio_title)
        .bind(&item.preferred_subtitle_language_code)
        .bind(item.subtitle_mode.map(encode_enum))
        .bind(&item.block_key)
        .bind(&item.collection_key)
        .bind(&item.collection_etag)
        .bind(&item.scheduling_context)
        .execute(&mut *tx)
        .await
        .repository()?;
        replace_ids(
            &mut tx,
            r#"DELETE FROM "PlayoutItemWatermark" WHERE "PlayoutItemId" = ?"#,
            r#"REPLACE INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId") VALUES (?, ?)"#,
            item.id,
            &item.watermark_ids,
        )
        .await?;
        replace_playout_item_graphics_elements(&mut tx, item.id, item).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn delete_items_before(&self, playout_id: EntityId, cutoff: DateTime<Utc>) -> Result<()> {
        sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ? AND "Finish" < ?"#)
            .bind(playout_id)
            .bind(encode_datetime(cutoff))
            .execute(&self.0)
            .await
            .repository()?;
        Ok(())
    }

    async fn apply_build_result(&self, result: &SchedulingResult, playout: &Playout) -> Result<()> {
        let mut tx = self.0.begin().await.repository()?;

        // ── items ─────────────────────────────────────────────────────────────
        if result.clear_items {
            sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ?"#)
                .bind(playout.id)
                .execute(&mut *tx)
                .await
                .repository()?;
        } else {
            for &item_id in &result.items_to_remove {
                sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "Id" = ?"#)
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            if let Some(cutoff) = result.remove_before {
                sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ? AND "Start" < ?"#)
                    .bind(playout.id)
                    .bind(encode_datetime(cutoff))
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
        }

        for item in &result.added_items {
            // Insert without an explicit Id so SQLite auto-assigns one.
            let inserted = sqlx::query(
                r#"INSERT INTO "PlayoutItem" (
                    "PlayoutId", "MediaItemId", "Start", "Finish",
                    "GuideStart", "GuideFinish", "CustomTitle", "GuideGroup",
                    "FillerKind", "InPoint", "OutPoint", "ChapterTitle",
                    "DisableWatermarks", "PreferredAudioLanguageCode",
                    "PreferredAudioTitle", "PreferredSubtitleLanguageCode",
                    "SubtitleMode", "BlockKey", "CollectionKey",
                    "CollectionEtag", "SchedulingContext"
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(item.playout_id)
            .bind(item.media_item_id)
            .bind(encode_datetime(item.start))
            .bind(encode_datetime(item.finish))
            .bind(encode_optional_datetime(item.guide_start))
            .bind(encode_optional_datetime(item.guide_finish))
            .bind(&item.custom_title)
            .bind(item.guide_group)
            .bind(encode_enum(item.filler_kind))
            .bind(encode_duration(item.in_point))
            .bind(encode_duration(item.out_point))
            .bind(&item.chapter_title)
            .bind(item.disable_watermarks)
            .bind(&item.preferred_audio_language_code)
            .bind(&item.preferred_audio_title)
            .bind(&item.preferred_subtitle_language_code)
            .bind(item.subtitle_mode.map(encode_enum))
            .bind(&item.block_key)
            .bind(&item.collection_key)
            .bind(&item.collection_etag)
            .bind(&item.scheduling_context)
            .execute(&mut *tx)
            .await
            .repository()?;
            let inserted_id = crate::db::last_insert_id(inserted);

            for &wm_id in &item.watermark_ids {
                sqlx::query(
                    r#"REPLACE INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId") VALUES (?, ?)"#,
                )
                .bind(inserted_id)
                .bind(wm_id)
                .execute(&mut *tx)
                .await.repository()?;
            }
            replace_playout_item_graphics_elements(&mut tx, inserted_id, item).await?;
        }

        // ── gaps ──────────────────────────────────────────────────────────────
        if result.clear_items {
            sqlx::query(r#"DELETE FROM "PlayoutGap" WHERE "PlayoutId" = ?"#)
                .bind(playout.id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        for gap in &result.added_gaps {
            sqlx::query(
                r#"INSERT INTO "PlayoutGap" ("PlayoutId", "Start", "Finish") VALUES (?, ?, ?)"#,
            )
            .bind(playout.id)
            .bind(encode_datetime(gap.start))
            .bind(encode_datetime(gap.finish))
            .execute(&mut *tx)
            .await
            .repository()?;
        }

        // ── history ───────────────────────────────────────────────────────────
        if result.clear_items {
            sqlx::query(r#"DELETE FROM "PlayoutHistory" WHERE "PlayoutId" = ?"#)
                .bind(playout.id)
                .execute(&mut *tx)
                .await
                .repository()?;
        } else {
            for &history_id in &result.history_to_remove {
                sqlx::query(r#"DELETE FROM "PlayoutHistory" WHERE "Id" = ?"#)
                    .bind(history_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
        }
        for entry in &result.added_history {
            sqlx::query(
                r#"INSERT INTO "PlayoutHistory" (
                    "BlockId", "Details", "Index", "Key", "PlaybackOrder",
                    "PlayoutId", "When", "Finish", "ChildKey", "IsCurrentChild"
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(entry.block_id)
            .bind(&entry.details)
            .bind(entry.index)
            .bind(&entry.key)
            .bind(encode_enum(entry.playback_order))
            .bind(entry.playout_id)
            .bind(encode_datetime(entry.when))
            .bind(encode_optional_datetime(entry.finish))
            .bind(&entry.child_key)
            .bind(entry.is_current_child)
            .execute(&mut *tx)
            .await
            .repository()?;
        }

        // Persist `playout.seed` unconditionally. It only actually changes on a
        // Reset build (see `build_playout` in `maintenance.rs`, which mirrors
        // ErsatzTV's `ResetPlayout(): playout.Seed = new Random().Next()`), but
        // writing it every time keeps this function mode-agnostic and is a
        // no-op for Continue/Refresh, where `playout.seed` is unchanged.
        sqlx::query(r#"UPDATE "Playout" SET "Seed" = ? WHERE "Id" = ?"#)
            .bind(playout.seed)
            .bind(playout.id)
            .execute(&mut *tx)
            .await
            .repository()?;

        // ── anchor, program-schedule anchors, fill-group indices, templates ───
        // Persist the anchor produced by *this* build (`result.anchor`), not the
        // stale anchor carried on the input `playout`. The input anchor is the
        // pre-build state; for a fresh playout it is `None`, and writing it back
        // would delete the row and force every Continue build to restart from
        // `now` — producing overlapping, stacked playout items. ErsatzTV's
        // `PlayoutBuilder` mutates `playout.Anchor` in place before saving; the
        // result anchor is our equivalent of that post-build value.
        replace_anchor_state(&mut tx, playout.id, result.anchor.as_ref()).await?;
        replace_program_schedule_anchors(&mut tx, playout.id, &result.program_schedule_anchors)
            .await?;
        replace_fill_group_indices(&mut tx, playout.id, &result.fill_group_indices).await?;
        replace_playout_templates(&mut tx, playout).await?;

        tx.commit().await.repository()?;
        Ok(())
    }

    async fn upsert_build_status(&self, status: &PlayoutBuildStatus) -> Result<()> {
        sqlx::query(
            r#"REPLACE INTO "PlayoutBuildStatus" ("PlayoutId", "LastBuild", "Success", "Message")
             VALUES (?, ?, ?, ?)"#,
        )
        .bind(status.playout_id)
        .bind(encode_datetime(status.last_build))
        .bind(status.success)
        .bind(&status.message)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn list_admin(
        &self,
        query: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PlayoutListAdminRow>> {
        list_admin(&self.0, query, offset, limit).await
    }

    async fn count_admin(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playout""#)
            .fetch_one(&self.0)
            .await
            .repository()?;
        u64::try_from(count).map_err(|error| invalid_admin("playout count", error))
    }

    async fn count_failed_builds_admin(&self) -> Result<u32> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "PlayoutBuildStatus" WHERE "Success" = 0"#,
        )
        .fetch_one(&self.0)
        .await
        .repository()?;
        u32::try_from(count).map_err(|error| RepositoryError::Decode {
            kind: "playout failed-build count",
            value: error.to_string(),
        })
    }

    async fn list_detail_admin(
        &self,
        playout_id: EntityId,
        show_filler: bool,
        now: DateTime<Utc>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PlayoutDetailAdminItem>> {
        list_detail_admin(&self.0, playout_id, show_filler, now, offset, limit).await
    }

    async fn count_detail_admin(
        &self,
        playout_id: EntityId,
        show_filler: bool,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        count_detail_admin(&self.0, playout_id, show_filler, now).await
    }

    async fn list_create_channels_admin(&self) -> Result<Vec<PlayoutCreateChannelAdmin>> {
        list_create_channels_admin(&self.0).await
    }

    async fn list_create_schedules_admin(&self) -> Result<Vec<PlayoutCreateScheduleAdmin>> {
        list_create_schedules_admin(&self.0).await
    }

    async fn create_admin(&self, playout: &PlayoutCreateAdmin) -> Result<EntityId> {
        let result = sqlx::query(
            r#"INSERT INTO "Playout" (
                   "ChannelId", "ScheduleKind", "ProgramScheduleId", "ScheduleFile",
                   "Seed"
               ) VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(playout.channel_id)
        .bind(playout.schedule_kind)
        .bind(playout.program_schedule_id)
        .bind(&playout.schedule_file)
        .bind(playout.seed)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        checked_delete(&self.0, "Playout", id).await
    }

    async fn list_id_kinds_admin(&self) -> Result<Vec<(EntityId, i32)>> {
        let rows = sqlx::query(r#"SELECT "Id", "ScheduleKind" FROM "Playout""#)
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get("Id").repository()?,
                    row.try_get("ScheduleKind").repository()?,
                ))
            })
            .collect()
    }

    async fn save_daily_reset_admin(
        &self,
        id: EntityId,
        daily_rebuild_minutes: Option<i32>,
    ) -> Result<()> {
        let value = daily_rebuild_minutes.map(format_daily_rebuild_time);
        checked_update(
            sqlx::query(r#"UPDATE "Playout" SET "DailyRebuildTime" = ? WHERE "Id" = ?"#)
                .bind(value)
                .bind(id)
                .execute(&self.0)
                .await
                .repository()?,
            "playout",
            id,
        )
    }

    async fn save_schedule_file_admin(
        &self,
        id: EntityId,
        schedule_kind: i32,
        schedule_file: &str,
    ) -> Result<()> {
        checked_update(
            sqlx::query(
                r#"UPDATE "Playout" SET "ScheduleFile" = ? WHERE "Id" = ? AND "ScheduleKind" = ?"#,
            )
            .bind(schedule_file)
            .bind(id)
            .bind(schedule_kind)
            .execute(&self.0)
            .await
            .repository()?,
            "playout",
            id,
        )
    }

    async fn get_kind_editor_admin(
        &self,
        id: EntityId,
        schedule_kind: i32,
    ) -> Result<Option<PlayoutKindEditorAdmin>> {
        get_kind_editor_admin(&self.0, id, schedule_kind).await
    }

    async fn deco_exists_admin(&self, id: EntityId) -> Result<bool> {
        exists(&self.0, "Deco", id).await
    }

    async fn save_default_deco_admin(&self, id: EntityId, deco_id: Option<EntityId>) -> Result<()> {
        checked_update(
            sqlx::query(
                r#"UPDATE "Playout" SET "DecoId" = ? WHERE "Id" = ? AND "ScheduleKind" = 2"#,
            )
            .bind(deco_id)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?,
            "playout",
            id,
        )
    }

    async fn get_alternate_schedules_admin(
        &self,
        id: EntityId,
    ) -> Result<Option<PlayoutAlternateSchedulesAdmin>> {
        get_alternate_schedules_admin(&self.0, id).await
    }

    async fn save_alternate_schedules_admin(
        &self,
        id: EntityId,
        items: &[PlayoutAlternateScheduleItemAdmin],
    ) -> Result<()> {
        save_alternate_schedules_admin(&self.0, id, items).await
    }

    async fn has_items_admin(&self, id: EntityId) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"SELECT 1 FROM "PlayoutItem" WHERE "PlayoutId" = ? LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .is_some())
    }

    async fn get_templates_admin(&self, id: EntityId) -> Result<Option<PlayoutTemplatesAdmin>> {
        get_templates_admin(&self.0, id).await
    }

    async fn save_templates_admin(
        &self,
        id: EntityId,
        items: &[PlayoutTemplateItemAdmin],
    ) -> Result<()> {
        save_templates_admin(&self.0, id, items).await
    }

    async fn erase_items_admin(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        Ok(())
    }

    async fn get_sequential_admin(&self, id: EntityId) -> Result<Option<SequentialPlayoutAdmin>> {
        get_sequential_admin(&self.0, id).await
    }

    async fn erase_items_and_history_admin(&self, id: EntityId) -> Result<()> {
        erase_items_and_history_admin(&self.0, id).await
    }

    async fn enrich_scheduling_context_admin(&self, serialized: &str) -> Result<String> {
        enrich_scheduling_context_admin(&self.0, serialized).await
    }
}

async fn list_admin(
    pool: &crate::db::DbPool,
    query: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PlayoutListAdminRow>> {
    let base = r#"SELECT p."Id",
                         COALESCE(c."Number", '') AS "ChannelNumber",
                         COALESCE(c."Name", '') AS "ChannelName",
                         p."ScheduleKind",
                         COALESCE(ps."Name", '') AS "ScheduleName",
                         COALESCE(p."ScheduleFile", '') AS "ScheduleFile",
                         p."DailyRebuildTime",
                         pbs."Success" AS "BuildSuccess",
                         pbs."Message" AS "BuildMessage"
                  FROM "Playout" p
                  JOIN "Channel" c ON c."Id" = p."ChannelId"
                  LEFT JOIN "ProgramSchedule" ps ON ps."Id" = p."ProgramScheduleId"
                  LEFT JOIN "PlayoutBuildStatus" pbs ON pbs."PlayoutId" = p."Id""#;
    let query = query.trim();
    let rows = if query.is_empty() {
        sqlx::query(crate::db::assert_sql_safe(format!(
            r#"{base} ORDER BY c."SortNumber", p."Id" LIMIT ? OFFSET ?"#
        )))
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .repository()?
    } else {
        sqlx::query(crate::db::assert_sql_safe(format!(
            r#"{base} WHERE c."Name" LIKE ? ORDER BY c."SortNumber", p."Id" LIMIT ? OFFSET ?"#
        )))
        .bind(format!("%{query}%"))
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .repository()?
    };
    rows.iter()
        .map(|row| {
            let daily_rebuild = row
                .try_get::<Option<String>, _>("DailyRebuildTime")
                .repository()?
                .map(|value| parse_daily_rebuild_minutes(&value))
                .transpose()?;
            Ok(PlayoutListAdminRow {
                id: row.try_get("Id").repository()?,
                channel_number: row.try_get("ChannelNumber").repository()?,
                channel_name: row.try_get("ChannelName").repository()?,
                schedule_kind: row.try_get("ScheduleKind").repository()?,
                schedule_name: row.try_get("ScheduleName").repository()?,
                schedule_file: row.try_get("ScheduleFile").repository()?,
                daily_rebuild_minutes: daily_rebuild,
                build_success: row
                    .try_get::<Option<i64>, _>("BuildSuccess")
                    .repository()?
                    .map(|value| value != 0),
                build_message: row.try_get("BuildMessage").repository()?,
            })
        })
        .collect()
}

async fn list_detail_admin(
    pool: &crate::db::DbPool,
    playout_id: EntityId,
    show_filler: bool,
    now: DateTime<Utc>,
    offset: i64,
    limit: i64,
) -> Result<Vec<PlayoutDetailAdminItem>> {
    let filler_clause = if show_filler {
        ""
    } else {
        r#" AND pi."FillerKind" = 0"#
    };
    let sql = format!(
        r#"SELECT * FROM (
               SELECT 'item' AS "Kind", pi."Id" AS "Id", pi."Start" AS "Start",
                      pi."Finish" AS "Finish", pi."FillerKind" AS "FillerKind",
                      pi."SchedulingContext" AS "SchedulingContext"
               FROM "PlayoutItem" pi
               WHERE pi."PlayoutId" = ? AND pi."Finish" >= ?{filler_clause}
               UNION ALL
               SELECT 'gap' AS "Kind", pg."Id" AS "Id", pg."Start" AS "Start",
                      pg."Finish" AS "Finish", -1 AS "FillerKind",
                      NULL AS "SchedulingContext"
               FROM "PlayoutGap" pg
               WHERE pg."PlayoutId" = ? AND pg."Finish" >= ?
           ) AS combined
           ORDER BY "Start"
           LIMIT ? OFFSET ?"#
    );
    let rows = sqlx::query(crate::db::assert_sql_safe(sql))
        .bind(playout_id)
        .bind(encode_datetime(now))
        .bind(playout_id)
        .bind(encode_datetime(now))
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .repository()?;
    let ids = rows
        .iter()
        .filter_map(|row| {
            let kind = row.try_get::<String, _>("Kind").ok()?;
            (kind == "item").then(|| row.try_get::<EntityId, _>("Id").ok())?
        })
        .collect::<Vec<_>>();
    let titles = load_item_titles_admin(pool, &ids).await?;
    rows.iter()
        .map(|row| {
            let kind: String = row.try_get("Kind").repository()?;
            let id: EntityId = row.try_get("Id").repository()?;
            let scheduling_context = row
                .try_get::<Option<String>, _>("SchedulingContext")
                .repository()?
                .filter(|value| !value.trim().is_empty());
            Ok(PlayoutDetailAdminItem {
                title: if kind == "gap" {
                    String::new()
                } else {
                    titles.get(&id).cloned().unwrap_or_default()
                },
                start: row.try_get("Start").repository()?,
                finish: row.try_get("Finish").repository()?,
                scheduling_context,
                filler_kind: if kind == "gap" {
                    -1
                } else {
                    row.try_get("FillerKind").repository()?
                },
            })
        })
        .collect()
}

async fn count_detail_admin(
    pool: &crate::db::DbPool,
    playout_id: EntityId,
    show_filler: bool,
    now: DateTime<Utc>,
) -> Result<u64> {
    let filler_clause = if show_filler {
        ""
    } else {
        r#" AND "FillerKind" = 0"#
    };
    let sql = format!(
        r#"SELECT
              (SELECT COUNT(*) FROM "PlayoutItem"
               WHERE "PlayoutId" = ? AND "Finish" >= ?{filler_clause})
            + (SELECT COUNT(*) FROM "PlayoutGap"
               WHERE "PlayoutId" = ? AND "Finish" >= ?)"#
    );
    let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(sql))
        .bind(playout_id)
        .bind(encode_datetime(now))
        .bind(playout_id)
        .bind(encode_datetime(now))
        .fetch_one(pool)
        .await
        .repository()?;
    u64::try_from(count).map_err(|error| invalid_admin("playout detail count", error))
}

async fn list_create_channels_admin(
    pool: &crate::db::DbPool,
) -> Result<Vec<PlayoutCreateChannelAdmin>> {
    let rows = sqlx::query(
        r#"SELECT c."Id", COALESCE(c."Number", '') AS "Number",
                  COALESCE(c."Name", '') AS "Name", COUNT(p."Id") AS "PlayoutCount"
           FROM "Channel" c
           LEFT JOIN "Playout" p ON p."ChannelId" = c."Id"
           GROUP BY c."Id", c."Number", c."Name", c."SortNumber"
           ORDER BY c."SortNumber", c."Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(PlayoutCreateChannelAdmin {
                id: row.try_get("Id").repository()?,
                number: row.try_get("Number").repository()?,
                name: row.try_get("Name").repository()?,
                playout_count: row.try_get("PlayoutCount").repository()?,
            })
        })
        .collect()
}

async fn list_create_schedules_admin(
    pool: &crate::db::DbPool,
) -> Result<Vec<PlayoutCreateScheduleAdmin>> {
    let rows = sqlx::query(
        r#"SELECT ps."Id", COALESCE(ps."Name", '') AS "Name",
                  COUNT(psi."Id") AS "ItemCount"
           FROM "ProgramSchedule" ps
           LEFT JOIN "ProgramScheduleItem" psi ON psi."ProgramScheduleId" = ps."Id"
           GROUP BY ps."Id", ps."Name"
           ORDER BY ps."Name", ps."Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(PlayoutCreateScheduleAdmin {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
                item_count: row.try_get("ItemCount").repository()?,
            })
        })
        .collect()
}

async fn get_kind_editor_admin(
    pool: &crate::db::DbPool,
    id: EntityId,
    schedule_kind: i32,
) -> Result<Option<PlayoutKindEditorAdmin>> {
    let row = sqlx::query(
        r#"SELECT p."Id", p."ScheduleKind",
                  COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name", '') AS "ChannelName",
                  COALESCE(c."PlayoutMode", 0) AS "ChannelPlayoutMode",
                  COALESCE(p."ScheduleFile", '') AS "ScheduleFile", p."DecoId"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."Id" = ? AND p."ScheduleKind" = ?"#,
    )
    .bind(id)
    .bind(schedule_kind)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let (deco_groups, decos) = if schedule_kind == 2 {
        load_playout_deco_options_admin(pool).await?
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(Some(PlayoutKindEditorAdmin {
        id: row.try_get("Id").repository()?,
        schedule_kind: row.try_get("ScheduleKind").repository()?,
        channel_number: row.try_get("ChannelNumber").repository()?,
        channel_name: row.try_get("ChannelName").repository()?,
        channel_playout_mode: row.try_get("ChannelPlayoutMode").repository()?,
        schedule_file: row.try_get("ScheduleFile").repository()?,
        default_deco_id: row.try_get("DecoId").repository()?,
        deco_groups,
        decos,
    }))
}

async fn load_playout_deco_options_admin(
    pool: &crate::db::DbPool,
) -> Result<(Vec<PlayoutDecoGroupAdmin>, Vec<PlayoutDecoAdmin>)> {
    let group_rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "DecoGroup" ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    let deco_rows = sqlx::query(
        r#"SELECT "Id", "DecoGroupId", COALESCE("Name", '') AS "Name"
           FROM "Deco" ORDER BY "DecoGroupId", "Name", "Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    let groups = group_rows
        .iter()
        .map(|row| {
            Ok(PlayoutDecoGroupAdmin {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let decos = deco_rows
        .iter()
        .map(|row| {
            Ok(PlayoutDecoAdmin {
                id: row.try_get("Id").repository()?,
                group_id: row.try_get("DecoGroupId").repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((groups, decos))
}

async fn get_sequential_admin(
    pool: &crate::db::DbPool,
    id: EntityId,
) -> Result<Option<SequentialPlayoutAdmin>> {
    let row = sqlx::query(
        r#"SELECT p."Id", COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name", '') AS "ChannelName",
                  COALESCE(p."ScheduleFile", '') AS "ScheduleFile"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."Id" = ? AND p."ScheduleKind" = 3"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    row.as_ref()
        .map(|row| {
            Ok(SequentialPlayoutAdmin {
                id: row.try_get("Id").repository()?,
                channel_number: row.try_get("ChannelNumber").repository()?,
                channel_name: row.try_get("ChannelName").repository()?,
                yaml_content: row.try_get("ScheduleFile").repository()?,
            })
        })
        .transpose()
}

async fn checked_delete(pool: &crate::db::DbPool, table: &'static str, id: EntityId) -> Result<()> {
    let result = sqlx::query(crate::db::assert_sql_safe(format!(
        r#"DELETE FROM "{table}" WHERE "Id" = ?"#
    )))
    .bind(id)
    .execute(pool)
    .await
    .repository()?;
    checked_update(result, table, id)
}

#[allow(clippy::needless_pass_by_value)]
fn invalid_admin(kind: &'static str, value: impl ToString) -> RepositoryError {
    RepositoryError::Decode {
        kind,
        value: value.to_string(),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn checked_update(
    result: crate::db::DbQueryResult,
    kind: &'static str,
    id: EntityId,
) -> Result<()> {
    if result.rows_affected() == 0 {
        Err(RepositoryError::NotFound { kind, id })
    } else {
        Ok(())
    }
}

async fn exists(pool: &crate::db::DbPool, table: &'static str, id: EntityId) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(format!(
        r#"SELECT COUNT(*) FROM "{table}" WHERE "Id" = ?"#
    )))
    .bind(id)
    .fetch_one(pool)
    .await
    .repository()?;
    Ok(count > 0)
}

fn parse_daily_rebuild_minutes(value: &str) -> Result<i32> {
    let mut parts = value.split(':');
    let hours = parts
        .next()
        .ok_or_else(|| invalid_admin("daily rebuild time", "hour missing"))?
        .parse::<i32>()
        .map_err(|error| invalid_admin("daily rebuild hour", error))?;
    let minutes = parts
        .next()
        .ok_or_else(|| invalid_admin("daily rebuild time", "minute missing"))?
        .parse::<i32>()
        .map_err(|error| invalid_admin("daily rebuild minute", error))?;
    if !(0..24).contains(&hours) || !(0..60).contains(&minutes) {
        return Err(invalid_admin(
            "daily rebuild time",
            format!("invalid value {value}"),
        ));
    }
    Ok(hours * 60 + minutes)
}

fn format_daily_rebuild_time(minutes: i32) -> String {
    let minutes = minutes.clamp(0, 24 * 60 - 1);
    format!("{:02}:{:02}:00", minutes / 60, minutes % 60)
}

async fn get_alternate_schedules_admin(
    pool: &crate::db::DbPool,
    id: EntityId,
) -> Result<Option<PlayoutAlternateSchedulesAdmin>> {
    let row = sqlx::query(
        r#"SELECT p."ProgramScheduleId",
                  COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name", '') AS "ChannelName"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."Id" = ? AND p."ScheduleKind" = 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let schedules = load_nested_options_admin(pool, "ProgramSchedule").await?;
    let names = schedules
        .iter()
        .map(|item| (item.id, item.name.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let rows = sqlx::query(
        r#"SELECT "Id", "ProgramScheduleId", "Index", "DaysOfWeek", "DaysOfMonth",
                  "MonthsOfYear", "LimitToDateRange", "StartMonth", "StartDay",
                  "StartYear", "EndMonth", "EndDay", "EndYear"
           FROM "ProgramScheduleAlternate"
           WHERE "PlayoutId" = ?
           ORDER BY "Index", "Id""#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut items = rows
        .iter()
        .map(|row| alternate_item_admin(row, &names))
        .collect::<Result<Vec<_>>>()?;
    if let Some(default_id) = row
        .try_get::<Option<EntityId>, _>("ProgramScheduleId")
        .repository()?
        .filter(|id| *id > 0)
    {
        let index = items.iter().map(|item| item.index).max().unwrap_or(0) + 1;
        items.push(PlayoutAlternateScheduleItemAdmin {
            id: -1,
            index,
            program_schedule_id: default_id,
            program_schedule_name: names.get(&default_id).cloned().unwrap_or_default(),
            date_filter: default_date_filter_admin(),
        });
    }
    Ok(Some(PlayoutAlternateSchedulesAdmin {
        playout_id: id,
        channel_number: row.try_get("ChannelNumber").repository()?,
        channel_name: row.try_get("ChannelName").repository()?,
        schedules,
        items,
    }))
}

async fn save_alternate_schedules_admin(
    pool: &crate::db::DbPool,
    id: EntityId,
    items: &[PlayoutAlternateScheduleItemAdmin],
) -> Result<()> {
    let mut tx = pool.begin().await.repository()?;
    sqlx::query(r#"DELETE FROM "ProgramScheduleAlternate" WHERE "PlayoutId" = ?"#)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
    let default = items
        .last()
        .ok_or_else(|| invalid_admin("alternate schedules", "must include a default schedule"))?;
    for (position, item) in items.iter().take(items.len().saturating_sub(1)).enumerate() {
        sqlx::query(
            r#"INSERT INTO "ProgramScheduleAlternate"
               ("PlayoutId", "ProgramScheduleId", "Index", "DaysOfWeek", "DaysOfMonth",
                "MonthsOfYear", "LimitToDateRange", "StartMonth", "StartDay",
                "StartYear", "EndMonth", "EndDay", "EndYear")
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(id)
        .bind(item.program_schedule_id)
        .bind(i32::try_from(position + 1).unwrap_or(i32::MAX))
        .bind(encode_day_names_admin(&item.date_filter.days_of_week))
        .bind(encode_int_csv_admin(&item.date_filter.days_of_month))
        .bind(encode_int_csv_admin(&item.date_filter.months_of_year))
        .bind(item.date_filter.limit_to_date_range)
        .bind(item.date_filter.start_month)
        .bind(item.date_filter.start_day)
        .bind(item.date_filter.start_year)
        .bind(item.date_filter.end_month)
        .bind(item.date_filter.end_day)
        .bind(item.date_filter.end_year)
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    checked_update(
        sqlx::query(r#"UPDATE "Playout" SET "ProgramScheduleId" = ? WHERE "Id" = ?"#)
            .bind(default.program_schedule_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?,
        "playout",
        id,
    )?;
    tx.commit().await.repository()?;
    Ok(())
}

async fn get_templates_admin(
    pool: &crate::db::DbPool,
    id: EntityId,
) -> Result<Option<PlayoutTemplatesAdmin>> {
    let row = sqlx::query(
        r#"SELECT COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name", '') AS "ChannelName"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."Id" = ? AND p."ScheduleKind" = 2"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let template_groups = load_nested_options_admin(pool, "TemplateGroup").await?;
    let templates = load_nested_child_options_admin(pool, "Template", "TemplateGroupId").await?;
    let deco_template_groups = load_nested_options_admin(pool, "DecoTemplateGroup").await?;
    let deco_templates =
        load_nested_child_options_admin(pool, "DecoTemplate", "DecoTemplateGroupId").await?;
    let rows = sqlx::query(
        r#"SELECT pt."Id", pt."Index", pt."TemplateId", t."TemplateGroupId",
                  COALESCE(t."Name", '') AS "TemplateName",
                  pt."DecoTemplateId", dt."DecoTemplateGroupId",
                  dt."Name" AS "DecoTemplateName",
                  pt."DaysOfWeek", pt."DaysOfMonth", pt."MonthsOfYear",
                  pt."LimitToDateRange", pt."StartMonth", pt."StartDay", pt."StartYear",
                  pt."EndMonth", pt."EndDay", pt."EndYear"
           FROM "PlayoutTemplate" pt
           JOIN "Template" t ON t."Id" = pt."TemplateId"
           LEFT JOIN "DecoTemplate" dt ON dt."Id" = pt."DecoTemplateId"
           WHERE pt."PlayoutId" = ?
           ORDER BY pt."Index", pt."Id""#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .repository()?;
    let items = rows
        .iter()
        .map(|item| {
            Ok(PlayoutTemplateItemAdmin {
                id: item.try_get("Id").repository()?,
                index: item.try_get("Index").repository()?,
                template_group_id: item.try_get("TemplateGroupId").repository()?,
                template_id: item.try_get("TemplateId").repository()?,
                template_name: item.try_get("TemplateName").repository()?,
                deco_template_group_id: item.try_get("DecoTemplateGroupId").repository()?,
                deco_template_id: item.try_get("DecoTemplateId").repository()?,
                deco_template_name: item.try_get("DecoTemplateName").repository()?,
                date_filter: date_filter_admin(item, false)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(PlayoutTemplatesAdmin {
        playout_id: id,
        channel_number: row.try_get("ChannelNumber").repository()?,
        channel_name: row.try_get("ChannelName").repository()?,
        template_groups,
        templates,
        deco_template_groups,
        deco_templates,
        items,
    }))
}

async fn save_templates_admin(
    pool: &crate::db::DbPool,
    id: EntityId,
    items: &[PlayoutTemplateItemAdmin],
) -> Result<()> {
    let mut tx = pool.begin().await.repository()?;
    sqlx::query(r#"DELETE FROM "PlayoutTemplate" WHERE "PlayoutId" = ?"#)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
    for (position, item) in items.iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO "PlayoutTemplate"
               ("DateUpdated", "TemplateId", "DecoTemplateId", "Index", "DaysOfWeek",
                "DaysOfMonth", "MonthsOfYear", "LimitToDateRange", "StartMonth",
                "StartDay", "StartYear", "EndMonth", "EndDay", "EndYear", "PlayoutId")
               VALUES (CURRENT_TIMESTAMP, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item.template_id)
        .bind(item.deco_template_id.filter(|value| *value > 0))
        .bind(i32::try_from(position + 1).unwrap_or(i32::MAX))
        .bind(encode_int_json_admin(&item.date_filter.days_of_week)?)
        .bind(encode_int_json_admin(&item.date_filter.days_of_month)?)
        .bind(encode_int_json_admin(&item.date_filter.months_of_year)?)
        .bind(item.date_filter.limit_to_date_range)
        .bind(item.date_filter.start_month)
        .bind(item.date_filter.start_day)
        .bind(item.date_filter.start_year)
        .bind(item.date_filter.end_month)
        .bind(item.date_filter.end_day)
        .bind(item.date_filter.end_year)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    tx.commit().await.repository()?;
    Ok(())
}

async fn load_nested_options_admin(
    pool: &crate::db::DbPool,
    table: &'static str,
) -> Result<Vec<PlayoutNestedOptionAdmin>> {
    let rows = sqlx::query(crate::db::assert_sql_safe(format!(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "{table}" ORDER BY "Name", "Id""#
    )))
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(PlayoutNestedOptionAdmin {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .collect()
}

async fn load_nested_child_options_admin(
    pool: &crate::db::DbPool,
    table: &'static str,
    group_column: &'static str,
) -> Result<Vec<PlayoutNestedChildOptionAdmin>> {
    let rows = sqlx::query(crate::db::assert_sql_safe(format!(
        r#"SELECT "Id", "{group_column}", COALESCE("Name", '') AS "Name"
           FROM "{table}" ORDER BY "{group_column}", "Name", "Id""#
    )))
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(PlayoutNestedChildOptionAdmin {
                id: row.try_get("Id").repository()?,
                group_id: row.try_get(group_column).repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .collect()
}

fn alternate_item_admin(
    row: &DbRow,
    names: &std::collections::HashMap<EntityId, String>,
) -> Result<PlayoutAlternateScheduleItemAdmin> {
    let schedule_id = row.try_get("ProgramScheduleId").repository()?;
    Ok(PlayoutAlternateScheduleItemAdmin {
        id: row.try_get("Id").repository()?,
        index: row.try_get("Index").repository()?,
        program_schedule_id: schedule_id,
        program_schedule_name: names.get(&schedule_id).cloned().unwrap_or_default(),
        date_filter: date_filter_admin(row, true)?,
    })
}

fn date_filter_admin(row: &DbRow, named_days: bool) -> Result<PlayoutDateFilterAdmin> {
    Ok(PlayoutDateFilterAdmin {
        limit_to_date_range: row.try_get("LimitToDateRange").repository()?,
        start_month: row.try_get("StartMonth").repository()?,
        start_day: row.try_get("StartDay").repository()?,
        start_year: row.try_get("StartYear").repository()?,
        end_month: row.try_get("EndMonth").repository()?,
        end_day: row.try_get("EndDay").repository()?,
        end_year: row.try_get("EndYear").repository()?,
        days_of_week: if named_days {
            decode_day_names_admin(row.try_get("DaysOfWeek").repository()?)?
        } else {
            decode_int_json_admin(row.try_get("DaysOfWeek").repository()?)?
        },
        days_of_month: if named_days {
            decode_int_csv_admin(row.try_get("DaysOfMonth").repository()?)?
        } else {
            decode_int_json_admin(row.try_get("DaysOfMonth").repository()?)?
        },
        months_of_year: if named_days {
            decode_int_csv_admin(row.try_get("MonthsOfYear").repository()?)?
        } else {
            decode_int_json_admin(row.try_get("MonthsOfYear").repository()?)?
        },
    })
}

fn default_date_filter_admin() -> PlayoutDateFilterAdmin {
    PlayoutDateFilterAdmin {
        limit_to_date_range: false,
        start_month: 1,
        start_day: 1,
        start_year: None,
        end_month: 12,
        end_day: 31,
        end_year: None,
        days_of_week: (0..=6).collect(),
        days_of_month: (1..=31).collect(),
        months_of_year: (1..=12).collect(),
    }
}

fn encode_day_names_admin(values: &[i32]) -> String {
    values
        .iter()
        .filter_map(|value| {
            [
                "Sunday",
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
            ]
            .get(usize::try_from(*value).ok()?)
            .copied()
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_day_names_admin(value: Option<String>) -> Result<Vec<i32>> {
    let mut values = Vec::new();
    for item in value.unwrap_or_default().split(',') {
        let value = match item.trim().to_ascii_lowercase().as_str() {
            "sunday" | "0" => 0,
            "monday" | "1" => 1,
            "tuesday" | "2" => 2,
            "wednesday" | "3" => 3,
            "thursday" | "4" => 4,
            "friday" | "5" => 5,
            "saturday" | "6" => 6,
            "" => continue,
            other => {
                return Err(invalid_admin(
                    "day of week",
                    format!("invalid value {other}"),
                ));
            }
        };
        values.push(value);
    }
    values.sort_unstable();
    values.dedup();
    Ok(values)
}

fn encode_int_csv_admin(values: &[i32]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_int_csv_admin(value: Option<String>) -> Result<Vec<i32>> {
    let mut values = Vec::new();
    for item in value.unwrap_or_default().split(',') {
        if item.trim().is_empty() {
            continue;
        }
        values.push(
            item.trim()
                .parse()
                .map_err(|error: std::num::ParseIntError| invalid_admin("integer list", error))?,
        );
    }
    Ok(values)
}

fn encode_int_json_admin(values: &[i32]) -> Result<String> {
    serde_json::to_string(values).map_err(|error| invalid_admin("integer list", error))
}

fn decode_int_json_admin(value: Option<String>) -> Result<Vec<i32>> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&value).map_err(|error| invalid_admin("integer list", error))
}

async fn erase_items_and_history_admin(pool: &crate::db::DbPool, id: EntityId) -> Result<()> {
    let mut tx = pool.begin().await.repository()?;
    checked_update(
        sqlx::query(
            r#"UPDATE "Playout"
               SET "Seed" = ?, "OnDemandCheckpoint" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(next_playout_seed())
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?,
        "playout",
        id,
    )?;
    sqlx::query(r#"DELETE FROM "PlayoutAnchor" WHERE "PlayoutId" = ?"#)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
    for table in [
        "PlayoutItem",
        "PlayoutHistory",
        "PlayoutProgramScheduleAnchor",
        "RerunHistory",
        "PlayoutGap",
        "PlayoutBuildStatus",
    ] {
        sqlx::query(crate::db::assert_sql_safe(format!(
            r#"DELETE FROM "{table}" WHERE "PlayoutId" = ?"#
        )))
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    tx.commit().await.repository()?;
    Ok(())
}

const ITEM_TITLE_QUERY_BATCH_SIZE: usize = 500;

pub(super) async fn load_item_titles_admin(
    pool: &crate::db::DbPool,
    item_ids: &[EntityId],
) -> Result<std::collections::HashMap<EntityId, String>> {
    use std::collections::HashMap;

    let mut unique_ids = item_ids.to_vec();
    unique_ids.sort_unstable();
    unique_ids.dedup();

    let mut titles = HashMap::new();
    for batch in unique_ids.chunks(ITEM_TITLE_QUERY_BATCH_SIZE) {
        titles.extend(load_item_title_batch_admin(pool, batch).await?);
    }
    Ok(titles)
}

#[allow(clippy::too_many_lines)]
async fn load_item_title_batch_admin(
    pool: &crate::db::DbPool,
    item_ids: &[EntityId],
) -> Result<std::collections::HashMap<EntityId, String>> {
    use std::collections::HashMap;

    let placeholders = vec!["?"; item_ids.len()].join(", ");
    let sql = format!(
        r#"SELECT pi."Id" AS "Id", pi."ChapterTitle" AS "ChapterTitle",
                  EXISTS(SELECT 1 FROM "Episode" e WHERE e."Id" = pi."MediaItemId") AS "IsEpisode",
                  EXISTS(SELECT 1 FROM "Movie" m WHERE m."Id" = pi."MediaItemId") AS "IsMovie",
                  EXISTS(SELECT 1 FROM "MusicVideo" mv WHERE mv."Id" = pi."MediaItemId") AS "IsMusicVideo",
                  EXISTS(SELECT 1 FROM "OtherVideo" ov WHERE ov."Id" = pi."MediaItemId") AS "IsOtherVideo",
                  EXISTS(SELECT 1 FROM "Song" s WHERE s."Id" = pi."MediaItemId") AS "IsSong",
                  EXISTS(SELECT 1 FROM "Image" i WHERE i."Id" = pi."MediaItemId") AS "IsImage",
                  EXISTS(SELECT 1 FROM "RemoteStream" rs WHERE rs."Id" = pi."MediaItemId") AS "IsRemoteStream",
                  (SELECT sm."Title" FROM "Episode" e
                     JOIN "Season" se ON se."Id" = e."SeasonId"
                     JOIN "ShowMetadata" sm ON sm."ShowId" = se."ShowId"
                   WHERE e."Id" = pi."MediaItemId" LIMIT 1) AS "ShowTitle",
                  (SELECT se."SeasonNumber" FROM "Episode" e
                     JOIN "Season" se ON se."Id" = e."SeasonId"
                   WHERE e."Id" = pi."MediaItemId" LIMIT 1) AS "SeasonNumber",
                  (SELECT em."EpisodeNumber" FROM "EpisodeMetadata" em
                   WHERE em."EpisodeId" = pi."MediaItemId" LIMIT 1) AS "EpisodeNumber",
                  (SELECT em."Title" FROM "EpisodeMetadata" em
                   WHERE em."EpisodeId" = pi."MediaItemId" LIMIT 1) AS "EpisodeTitle",
                  (SELECT mm."Title" FROM "MovieMetadata" mm
                   WHERE mm."MovieId" = pi."MediaItemId" LIMIT 1) AS "MovieTitle",
                  (SELECT am."Title" FROM "MusicVideo" mv
                     JOIN "ArtistMetadata" am ON am."ArtistId" = mv."ArtistId"
                   WHERE mv."Id" = pi."MediaItemId" LIMIT 1) AS "MvArtist",
                  (SELECT mvm."Title" FROM "MusicVideoMetadata" mvm
                   WHERE mvm."MusicVideoId" = pi."MediaItemId" LIMIT 1) AS "MvTitle",
                  (SELECT smd."Title" FROM "SongMetadata" smd
                   WHERE smd."SongId" = pi."MediaItemId" LIMIT 1) AS "SongTitle",
                  (SELECT smd."Artists" FROM "SongMetadata" smd
                   WHERE smd."SongId" = pi."MediaItemId" LIMIT 1) AS "SongArtists",
                  (SELECT ovm."Title" FROM "OtherVideoMetadata" ovm
                   WHERE ovm."OtherVideoId" = pi."MediaItemId" LIMIT 1) AS "OtherTitle",
                  (SELECT im."Title" FROM "ImageMetadata" im
                   WHERE im."ImageId" = pi."MediaItemId" LIMIT 1) AS "ImageTitle",
                  (SELECT rsm."Title" FROM "RemoteStreamMetadata" rsm
                   WHERE rsm."RemoteStreamId" = pi."MediaItemId" LIMIT 1) AS "RemoteTitle"
           FROM "PlayoutItem" pi
           WHERE pi."Id" IN ({placeholders})"#
    );
    let mut query = sqlx::query(crate::db::assert_sql_safe(sql));
    for id in item_ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await.repository()?;
    let mut titles = HashMap::new();
    for row in &rows {
        let id = row.try_get("Id").repository()?;
        let chapter = row
            .try_get::<Option<String>, _>("ChapterTitle")
            .repository()?
            .filter(|value| !value.trim().is_empty());
        let flag =
            |column| -> Result<bool> { Ok(row.try_get::<i64, _>(column).repository()? != 0) };
        let title = if flag("IsEpisode")? {
            let episode: Option<i64> = row.try_get("EpisodeNumber").repository()?;
            let episode_title: Option<String> = row.try_get("EpisodeTitle").repository()?;
            match (episode, episode_title) {
                (Some(episode), Some(episode_title)) => {
                    let show = row
                        .try_get::<Option<String>, _>("ShowTitle")
                        .repository()?
                        .map(|value| format!("{value} - "))
                        .unwrap_or_default();
                    let season = row
                        .try_get::<Option<i64>, _>("SeasonNumber")
                        .repository()?
                        .unwrap_or_default();
                    with_chapter_admin(
                        format!("{show}s{season:02}e{episode:02} - {episode_title}"),
                        chapter.as_deref(),
                    )
                }
                _ => "[unknown episode]".to_owned(),
            }
        } else if flag("IsMovie")? {
            row.try_get::<Option<String>, _>("MovieTitle")
                .repository()?
                .unwrap_or_else(|| "[unknown movie]".to_owned())
        } else if flag("IsMusicVideo")? {
            match row.try_get::<Option<String>, _>("MvTitle").repository()? {
                Some(title) => {
                    let artist = row
                        .try_get::<Option<String>, _>("MvArtist")
                        .repository()?
                        .map(|value| format!("{value} - "))
                        .unwrap_or_default();
                    with_chapter_admin(format!("{artist}{title}"), chapter.as_deref())
                }
                None => "[unknown music video]".to_owned(),
            }
        } else if flag("IsOtherVideo")? {
            with_chapter_admin(
                row.try_get::<Option<String>, _>("OtherTitle")
                    .repository()?
                    .unwrap_or_else(|| "[unknown video]".to_owned()),
                chapter.as_deref(),
            )
        } else if flag("IsSong")? {
            match row.try_get::<Option<String>, _>("SongTitle").repository()? {
                Some(title) => {
                    let artists = row
                        .try_get::<Option<String>, _>("SongArtists")
                        .repository()?
                        .filter(|value| !value.trim().is_empty())
                        .map(|value| format!("{} - ", normalize_song_artists_admin(&value)))
                        .unwrap_or_default();
                    with_chapter_admin(format!("{artists}{title}"), chapter.as_deref())
                }
                None => "[unknown song]".to_owned(),
            }
        } else if flag("IsImage")? {
            row.try_get::<Option<String>, _>("ImageTitle")
                .repository()?
                .unwrap_or_else(|| "[unknown image]".to_owned())
        } else if flag("IsRemoteStream")? {
            row.try_get::<Option<String>, _>("RemoteTitle")
                .repository()?
                .unwrap_or_else(|| "[unknown remote stream]".to_owned())
        } else {
            String::new()
        };
        titles.insert(id, title);
    }
    Ok(titles)
}

fn normalize_song_artists_admin(value: &str) -> String {
    let value = value.trim();
    if value.starts_with('[') {
        serde_json::from_str::<Vec<String>>(value)
            .map_or_else(|_| value.to_owned(), |artists| artists.join(", "))
    } else {
        value.to_owned()
    }
}

fn with_chapter_admin(title: String, chapter: Option<&str>) -> String {
    match chapter {
        Some(chapter) if !chapter.trim().is_empty() => format!("{title} ({chapter})"),
        _ => title,
    }
}

async fn enrich_scheduling_context_admin(
    pool: &crate::db::DbPool,
    serialized: &str,
) -> Result<String> {
    use serde_json::Value;

    let Ok(value) = serde_json::from_str::<Value>(serialized) else {
        return Ok(serialized.to_owned());
    };
    let Some(object) = value.as_object() else {
        return pretty_json_admin(&value, serialized);
    };
    if let Some(id) = object.get("ScheduleId").and_then(Value::as_i64) {
        if id > 0 {
            return enrich_classic_admin(pool, object, id).await;
        }
    } else if let Some(id) = object.get("BlockId").and_then(Value::as_i64) {
        if id > 0 {
            return enrich_block_admin(pool, object, id).await;
        }
    }
    pretty_json_admin(&value, serialized)
}

fn pretty_json_admin(value: &serde_json::Value, original: &str) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| invalid_admin("scheduling context", error))
        .or_else(|_| Ok(original.to_owned()))
}

async fn enrich_classic_admin(
    pool: &crate::db::DbPool,
    object: &serde_json::Map<String, serde_json::Value>,
    schedule_id: EntityId,
) -> Result<String> {
    use serde_json::{json, Value};

    let item_id = object
        .get("ItemId")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let schedule_name: Option<String> =
        sqlx::query_scalar(r#"SELECT "Name" FROM "ProgramSchedule" WHERE "Id" = ?"#)
            .bind(schedule_id)
            .fetch_optional(pool)
            .await
            .repository()?;
    let item = collection_ref_schedule_admin(pool, item_id).await?;
    let filler_id = object.get("FillerPresetId").and_then(Value::as_i64);
    let (scheduler, filler) = match filler_id.filter(|id| *id > 0) {
        Some(id) => ("Classic - Filler", filler_preset_admin(pool, id).await?),
        None => ("Classic", Value::Null),
    };
    let context = json!({
        "Scheduler": { "Type": scheduler, "Mode": object.get("Scheduler") },
        "Schedule": { "Id": schedule_id, "Name": schedule_name.unwrap_or_default() },
        "ScheduleItem": item.into_value(item_id),
        "Filler": filler,
        "Enumerator": enumerator_admin(object),
    });
    serde_json::to_string_pretty(&context)
        .map_err(|error| invalid_admin("scheduling context", error))
}

async fn enrich_block_admin(
    pool: &crate::db::DbPool,
    object: &serde_json::Map<String, serde_json::Value>,
    block_id: EntityId,
) -> Result<String> {
    use serde_json::{json, Value};

    let item_id = object
        .get("BlockItemId")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let block = sqlx::query(
        r#"SELECT COALESCE(b."Name", '') AS "Name",
                  COALESCE(bg."Name", '') AS "GroupName"
           FROM "Block" b
           JOIN "BlockGroup" bg ON bg."Id" = b."BlockGroupId"
           WHERE b."Id" = ?"#,
    )
    .bind(block_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let (group, name) = match block {
        Some(row) => (
            row.try_get("GroupName").repository()?,
            row.try_get("Name").repository()?,
        ),
        None => (String::new(), String::new()),
    };
    let item = collection_ref_block_admin(pool, item_id).await?;
    let context = json!({
        "Scheduler": { "Type": "Block", "Mode": Value::Null },
        "Block": { "Id": block_id, "Group": group, "Name": name },
        "BlockItem": item.into_value(item_id),
        "Enumerator": enumerator_admin(object),
    });
    serde_json::to_string_pretty(&context)
        .map_err(|error| invalid_admin("scheduling context", error))
}

fn enumerator_admin(object: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    use serde_json::{json, Value};
    json!({
        "Name": object.get("Enumerator").cloned().unwrap_or(Value::Null),
        "Seed": object.get("Seed").cloned().unwrap_or(Value::Null),
        "Index": object.get("Index").cloned().unwrap_or(Value::Null),
    })
}

struct CollectionRefAdmin {
    collection_type: Option<i32>,
    collection_name: Option<String>,
}

impl CollectionRefAdmin {
    fn into_value(self, id: EntityId) -> serde_json::Value {
        serde_json::json!({
            "Id": id,
            "CollectionType": self.collection_type.map(collection_type_name_admin),
            "CollectionName": self.collection_name,
        })
    }
}

async fn collection_ref_schedule_admin(
    pool: &crate::db::DbPool,
    id: EntityId,
) -> Result<CollectionRefAdmin> {
    let row = sqlx::query(
        r#"SELECT "CollectionType", "CollectionId", "MultiCollectionId", "PlaylistId",
                  "RerunCollectionId", "SmartCollectionId"
           FROM "ProgramScheduleItem" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(CollectionRefAdmin {
            collection_type: None,
            collection_name: None,
        });
    };
    let kind = row.try_get("CollectionType").repository()?;
    Ok(CollectionRefAdmin {
        collection_type: Some(kind),
        collection_name: resolve_collection_name_admin(
            pool,
            kind,
            row.try_get("CollectionId").repository()?,
            row.try_get("MultiCollectionId").repository()?,
            row.try_get("PlaylistId").repository()?,
            row.try_get("RerunCollectionId").repository()?,
            row.try_get("SmartCollectionId").repository()?,
        )
        .await?,
    })
}

async fn collection_ref_block_admin(
    pool: &crate::db::DbPool,
    id: EntityId,
) -> Result<CollectionRefAdmin> {
    let row = sqlx::query(
        r#"SELECT "CollectionType", "CollectionId", "MultiCollectionId", "SmartCollectionId"
           FROM "BlockItem" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(CollectionRefAdmin {
            collection_type: None,
            collection_name: None,
        });
    };
    let kind = row.try_get("CollectionType").repository()?;
    Ok(CollectionRefAdmin {
        collection_type: Some(kind),
        collection_name: resolve_collection_name_admin(
            pool,
            kind,
            row.try_get("CollectionId").repository()?,
            row.try_get("MultiCollectionId").repository()?,
            None,
            None,
            row.try_get("SmartCollectionId").repository()?,
        )
        .await?,
    })
}

async fn filler_preset_admin(pool: &crate::db::DbPool, id: EntityId) -> Result<serde_json::Value> {
    use serde_json::{json, Value};
    let row = sqlx::query(
        r#"SELECT "Name", "FillerKind", "FillerMode", "CollectionType",
                  "CollectionId", "MultiCollectionId", "PlaylistId", "SmartCollectionId"
           FROM "FillerPreset" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(Value::Null);
    };
    let kind = row.try_get("CollectionType").repository()?;
    let collection_name = resolve_collection_name_admin(
        pool,
        kind,
        row.try_get("CollectionId").repository()?,
        row.try_get("MultiCollectionId").repository()?,
        row.try_get("PlaylistId").repository()?,
        None,
        row.try_get("SmartCollectionId").repository()?,
    )
    .await?;
    Ok(json!({
        "Id": id,
        "Name": row.try_get::<Option<String>, _>("Name").repository()?,
        "Kind": filler_kind_name_admin(row.try_get("FillerKind").repository()?),
        "Mode": filler_mode_name_admin(row.try_get("FillerMode").repository()?),
        "CollectionType": collection_type_name_admin(kind),
        "CollectionName": collection_name,
    }))
}

#[allow(clippy::too_many_arguments)]
async fn resolve_collection_name_admin(
    pool: &crate::db::DbPool,
    kind: i32,
    collection_id: Option<EntityId>,
    multi_id: Option<EntityId>,
    playlist_id: Option<EntityId>,
    rerun_id: Option<EntityId>,
    smart_id: Option<EntityId>,
) -> Result<Option<String>> {
    let (table, id) = match kind {
        0 => ("Collection", collection_id),
        4 => ("MultiCollection", multi_id),
        5 => ("SmartCollection", smart_id),
        6 => ("Playlist", playlist_id),
        7 | 8 => ("RerunCollection", rerun_id),
        _ => return Ok(None),
    };
    let Some(id) = id else {
        return Ok(None);
    };
    sqlx::query_scalar(crate::db::assert_sql_safe(format!(
        r#"SELECT "Name" FROM "{table}" WHERE "Id" = ?"#
    )))
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()
}

fn collection_type_name_admin(value: i32) -> &'static str {
    match value {
        0 => "Collection",
        1 => "TelevisionShow",
        2 => "TelevisionSeason",
        3 => "Artist",
        4 => "MultiCollection",
        5 => "SmartCollection",
        6 => "Playlist",
        7 => "RerunFirstRun",
        8 => "RerunRerun",
        9 => "SearchQuery",
        10 => "Movie",
        20 => "Episode",
        _ => "Unknown",
    }
}

fn filler_kind_name_admin(value: i32) -> &'static str {
    match value {
        0 => "None",
        1 => "PreRoll",
        2 => "MidRoll",
        3 => "PostRoll",
        4 => "Tail",
        5 => "Fallback",
        99 => "GuideMode",
        100 => "DecoDefault",
        _ => "Unknown",
    }
}

fn filler_mode_name_admin(value: i32) -> &'static str {
    match value {
        0 => "None",
        1 => "Duration",
        2 => "Count",
        3 => "Pad",
        4 => "RandomCount",
        _ => "Unknown",
    }
}

fn map_playout(row: &DbRow) -> Result<Playout> {
    let daily_rebuild_time = decode_optional_duration_text(
        row.try_get::<Option<String>, _>("DailyRebuildTime")
            .repository()?
            .as_deref(),
    );
    Ok(Playout {
        id: row.try_get("Id").repository()?,
        channel_id: row.try_get("ChannelId").repository()?,
        program_schedule_id: row.try_get("ProgramScheduleId").repository()?,
        schedule_file: row.try_get("ScheduleFile").repository()?,
        program_schedule_alternate_ids: Vec::new(),
        schedule_kind: decode_enum_column("playout schedule_kind", row, "ScheduleKind")?,
        item_ids: Vec::new(),
        gap_ids: Vec::new(),
        anchor: None,
        program_schedule_anchor_ids: Vec::new(),
        fill_group_index_ids: Vec::new(),
        templates: Vec::new(),
        playout_history_ids: Vec::new(),
        seed: row.try_get("Seed").repository()?,
        daily_rebuild_time,
        deco_id: row.try_get("DecoId").repository()?,
        on_demand_checkpoint: decode_optional_datetime(
            "playout on_demand_checkpoint",
            row.try_get("OnDemandCheckpoint").repository()?,
        )?,
        build_status: None,
    })
}

fn map_anchor(row: &DbRow, state_row: Option<&DbRow>) -> Result<ersatzrs_domain::PlayoutAnchor> {
    let (seed, index) = if let Some(state) = state_row {
        (
            state.try_get("Seed").unwrap_or(0),
            state.try_get("Index").unwrap_or(0),
        )
    } else {
        (0, 0)
    };
    Ok(ersatzrs_domain::PlayoutAnchor {
        schedule_items_enumerator_state: ersatzrs_domain::CollectionEnumeratorState { seed, index },
        next_start: decode_datetime(
            "playout anchor next_start",
            &row.try_get::<String, _>("NextStart").repository()?,
        )?,
        multiple_remaining: row.try_get("MultipleRemaining").repository()?,
        duration_finish: decode_optional_datetime(
            "playout anchor duration_finish",
            row.try_get("DurationFinish").repository()?,
        )?,
        in_flood: row.try_get("InFlood").repository()?,
        in_duration_filler: row.try_get("InDurationFiller").repository()?,
        next_guide_group: row.try_get("NextGuideGroup").repository()?,
        next_instruction_index: row.try_get("NextInstructionIndex").repository()?,
        context: row.try_get("Context").repository()?,
    })
}

fn map_build_status(row: &DbRow) -> Result<PlayoutBuildStatus> {
    Ok(PlayoutBuildStatus {
        playout_id: row.try_get("PlayoutId").repository()?,
        last_build: decode_datetime(
            "playout build status last_build",
            &row.try_get::<String, _>("LastBuild").repository()?,
        )?,
        success: row.try_get("Success").repository()?,
        message: row.try_get("Message").repository()?,
    })
}

fn map_playout_item(row: &DbRow) -> Result<PlayoutItem> {
    Ok(PlayoutItem {
        id: row.try_get("Id").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        start: decode_datetime(
            "playout item start",
            &row.try_get::<String, _>("Start").repository()?,
        )?,
        finish: decode_datetime(
            "playout item finish",
            &row.try_get::<String, _>("Finish").repository()?,
        )?,
        guide_start: decode_optional_datetime(
            "playout item guide_start",
            row.try_get("GuideStart").repository()?,
        )?,
        guide_finish: decode_optional_datetime(
            "playout item guide_finish",
            row.try_get("GuideFinish").repository()?,
        )?,
        custom_title: row.try_get("CustomTitle").repository()?,
        guide_group: row.try_get("GuideGroup").repository()?,
        filler_kind: decode_enum_column("playout item filler_kind", row, "FillerKind")?,
        playout_id: row.try_get("PlayoutId").repository()?,
        in_point: decode_duration_text(&row.try_get::<String, _>("InPoint").repository()?),
        out_point: decode_duration_text(&row.try_get::<String, _>("OutPoint").repository()?),
        chapter_title: row.try_get("ChapterTitle").repository()?,
        watermark_ids: Vec::new(),
        disable_watermarks: row.try_get("DisableWatermarks").repository()?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode").repository()?,
        preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
        preferred_subtitle_language_code: row
            .try_get("PreferredSubtitleLanguageCode")
            .repository()?,
        subtitle_mode: decode_optional_enum_column(
            "playout item subtitle_mode",
            row,
            "SubtitleMode",
        )?,
        block_key: row.try_get("BlockKey").repository()?,
        collection_key: row.try_get("CollectionKey").repository()?,
        collection_etag: row.try_get("CollectionEtag").repository()?,
        graphics_element_ids: Vec::new(),
        graphics_elements: Vec::new(),
        scheduling_context: row.try_get("SchedulingContext").repository()?,
    })
}

async fn load_playout_item_graphics_elements(
    pool: &Pool<crate::db::Db>,
    playout_item_id: EntityId,
) -> Result<Vec<PlayoutItemGraphicsElement>> {
    let rows = sqlx::query(
        r#"SELECT "GraphicsElementId", "Variables"
           FROM "PlayoutItemGraphicsElement"
           WHERE "PlayoutItemId" = ?
           ORDER BY "GraphicsElementId""#,
    )
    .bind(playout_item_id)
    .fetch_all(pool)
    .await
    .repository()?;

    rows.iter()
        .map(|row| {
            Ok(PlayoutItemGraphicsElement {
                playout_item_id,
                graphics_element_id: row.try_get("GraphicsElementId").repository()?,
                variables: row.try_get("Variables").repository()?,
            })
        })
        .collect()
}

async fn replace_playout_item_graphics_elements(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_item_id: EntityId,
    item: &PlayoutItem,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "PlayoutItemGraphicsElement" WHERE "PlayoutItemId" = ?"#)
        .bind(playout_item_id)
        .execute(&mut **tx)
        .await
        .repository()?;

    if item.graphics_elements.is_empty() {
        for &graphics_element_id in &item.graphics_element_ids {
            insert_playout_item_graphics_element(tx, playout_item_id, graphics_element_id, None)
                .await?;
        }
    } else {
        for graphics in &item.graphics_elements {
            if let Some(graphics_element_id) = graphics.graphics_element_id {
                insert_playout_item_graphics_element(
                    tx,
                    playout_item_id,
                    graphics_element_id,
                    graphics.variables.as_deref(),
                )
                .await?;
            }
        }
    }

    Ok(())
}

async fn insert_playout_item_graphics_element(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_item_id: EntityId,
    graphics_element_id: EntityId,
    variables: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"REPLACE INTO "PlayoutItemGraphicsElement" (
            "PlayoutItemId", "GraphicsElementId", "Variables"
         ) VALUES (?, ?, ?)"#,
    )
    .bind(playout_item_id)
    .bind(graphics_element_id)
    .bind(variables)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

fn map_playout_template(row: &DbRow) -> Result<ersatzrs_domain::PlayoutTemplate> {
    Ok(ersatzrs_domain::PlayoutTemplate {
        playout_id: row.try_get("PlayoutId").repository()?,
        template_id: row.try_get("TemplateId").repository()?,
        // DateUpdated is a TEXT datetime in the ErsatzTV schema; we store updated_ticks as i64.
        // Parse the stored string back as i64, or fall back to 0.
        updated_ticks: row
            .try_get::<String, _>("DateUpdated")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0),
        deco_template_id: row.try_get("DecoTemplateId").repository()?,
        index: row.try_get("Index").repository()?,
        days_of_week: decode_integer_list(
            "playout template DaysOfWeek",
            &row.try_get::<String, _>("DaysOfWeek").unwrap_or_default(),
        )?,
        days_of_month: decode_integer_list(
            "playout template DaysOfMonth",
            &row.try_get::<String, _>("DaysOfMonth").unwrap_or_default(),
        )?,
        months_of_year: decode_integer_list(
            "playout template MonthsOfYear",
            &row.try_get::<String, _>("MonthsOfYear").unwrap_or_default(),
        )?,
        limit_to_date_range: row.try_get("LimitToDateRange").repository()?,
        start_month: row.try_get("StartMonth").repository()?,
        start_day: row.try_get("StartDay").repository()?,
        start_year: row.try_get("StartYear").repository()?,
        end_month: row.try_get("EndMonth").repository()?,
        end_day: row.try_get("EndDay").repository()?,
        end_year: row.try_get("EndYear").repository()?,
    })
}

fn decode_integer_list(kind: &'static str, value: &str) -> Result<Vec<i32>> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(value).map_err(|source| decode_message(kind, &source.to_string()))
}

fn encode_integer_list(value: &[i32]) -> Result<String> {
    serde_json::to_string(value).map_err(|source| RepositoryError::Conflict(source.to_string()))
}

async fn update_playout_checkpoint(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout: &Playout,
) -> Result<()> {
    sqlx::query(r#"UPDATE "Playout" SET "OnDemandCheckpoint" = ? WHERE "Id" = ?"#)
        .bind(encode_optional_datetime(playout.on_demand_checkpoint))
        .bind(playout.id)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(())
}

async fn replace_anchor_state(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_id: EntityId,
    anchor: Option<&PlayoutAnchor>,
) -> Result<()> {
    if let Some(anchor) = anchor {
        sqlx::query(
            r#"REPLACE INTO "PlayoutAnchor" (
                "PlayoutId", "DurationFinish", "InDurationFiller", "InFlood",
                "MultipleRemaining", "NextGuideGroup", "NextStart",
                "NextInstructionIndex", "Context"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(playout_id)
        .bind(encode_optional_datetime(anchor.duration_finish))
        .bind(anchor.in_duration_filler)
        .bind(anchor.in_flood)
        .bind(anchor.multiple_remaining)
        .bind(anchor.next_guide_group)
        .bind(encode_datetime(anchor.next_start))
        .bind(anchor.next_instruction_index)
        .bind(&anchor.context)
        .execute(&mut **tx)
        .await
        .repository()?;
        sqlx::query(
            r#"REPLACE INTO "ScheduleItemsEnumeratorState" (
                "PlayoutAnchorPlayoutId", "Seed", "Index"
             ) VALUES (?, ?, ?)"#,
        )
        .bind(playout_id)
        .bind(anchor.schedule_items_enumerator_state.seed)
        .bind(anchor.schedule_items_enumerator_state.index)
        .execute(&mut **tx)
        .await
        .repository()?;
    } else {
        sqlx::query(r#"DELETE FROM "PlayoutAnchor" WHERE "PlayoutId" = ?"#)
            .bind(playout_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}

async fn replace_gaps(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_id: EntityId,
    gaps: &[PlayoutGap],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "PlayoutGap" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for gap in gaps {
        sqlx::query(
            r#"REPLACE INTO "PlayoutGap" ("Id", "PlayoutId", "Start", "Finish")
             VALUES (?, ?, ?, ?)"#,
        )
        .bind(gap.id)
        .bind(gap.playout_id)
        .bind(encode_datetime(gap.start))
        .bind(encode_datetime(gap.finish))
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

async fn replace_history(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_id: EntityId,
    history: &[PlayoutHistory],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "PlayoutHistory" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for entry in history {
        sqlx::query(
            r#"REPLACE INTO "PlayoutHistory" (
                "Id", "BlockId", "Details", "Index", "Key", "PlaybackOrder",
                "PlayoutId", "When", "Finish", "ChildKey", "IsCurrentChild"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(entry.id)
        .bind(entry.block_id)
        .bind(&entry.details)
        .bind(entry.index)
        .bind(&entry.key)
        .bind(encode_enum(entry.playback_order))
        .bind(entry.playout_id)
        .bind(encode_datetime(entry.when))
        .bind(encode_optional_datetime(entry.finish))
        .bind(&entry.child_key)
        .bind(entry.is_current_child)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

async fn replace_program_schedule_anchors(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_id: EntityId,
    anchors: &[PlayoutProgramScheduleAnchor],
) -> Result<()> {
    sqlx::query(
        r#"DELETE FROM "CollectionEnumeratorState"
           WHERE "PlayoutProgramScheduleAnchorId" IN (
               SELECT "Id" FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId" = ?
           )"#,
    )
    .bind(playout_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(r#"DELETE FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for anchor in anchors {
        sqlx::query(
            r#"REPLACE INTO "PlayoutProgramScheduleAnchor" (
                "Id", "AnchorDate", "CollectionId", "CollectionType", "FakeCollectionKey",
                "MediaItemId", "MultiCollectionId", "PlaylistId", "PlayoutId",
                "RerunCollectionId", "SmartCollectionId", "SearchQuery"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(anchor.id)
        .bind(encode_optional_datetime(anchor.anchor_date))
        .bind(anchor.collection_id)
        .bind(encode_enum(anchor.collection_type))
        .bind(&anchor.fake_collection_key)
        .bind(anchor.media_item_id)
        .bind(anchor.multi_collection_id)
        .bind(anchor.playlist_id)
        .bind(anchor.playout_id)
        .bind(anchor.rerun_collection_id)
        .bind(anchor.smart_collection_id)
        .bind(&anchor.search_query)
        .execute(&mut **tx)
        .await
        .repository()?;
        sqlx::query(
            r#"REPLACE INTO "CollectionEnumeratorState" (
                "PlayoutProgramScheduleAnchorId", "Seed", "Index"
             ) VALUES (?, ?, ?)"#,
        )
        .bind(anchor.id)
        .bind(anchor.enumerator_state.seed)
        .bind(anchor.enumerator_state.index)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

async fn replace_fill_group_indices(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout_id: EntityId,
    indices: &[PlayoutScheduleItemFillGroupIndex],
) -> Result<()> {
    sqlx::query(
        r#"DELETE FROM "FillGroupEnumeratorState"
           WHERE "PlayoutScheduleItemFillGroupIndexId" IN (
               SELECT "Id" FROM "PlayoutScheduleItemFillGroupIndex" WHERE "PlayoutId" = ?
           )"#,
    )
    .bind(playout_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    sqlx::query(r#"DELETE FROM "PlayoutScheduleItemFillGroupIndex" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for index in indices {
        sqlx::query(
            r#"REPLACE INTO "PlayoutScheduleItemFillGroupIndex" (
                "Id", "PlayoutId", "ProgramScheduleItemId"
             ) VALUES (?, ?, ?)"#,
        )
        .bind(index.id)
        .bind(index.playout_id)
        .bind(index.program_schedule_item_id)
        .execute(&mut **tx)
        .await
        .repository()?;
        sqlx::query(
            r#"REPLACE INTO "FillGroupEnumeratorState" (
                "PlayoutScheduleItemFillGroupIndexId", "Seed", "Index"
             ) VALUES (?, ?, ?)"#,
        )
        .bind(index.id)
        .bind(index.enumerator_state.seed)
        .bind(index.enumerator_state.index)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

async fn replace_playout_templates(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout: &Playout,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "PlayoutTemplate" WHERE "PlayoutId" = ?"#)
        .bind(playout.id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for template in &playout.templates {
        sqlx::query(
            r#"REPLACE INTO "PlayoutTemplate" (
                "PlayoutId", "TemplateId", "DateUpdated", "DecoTemplateId", "Index",
                "DaysOfWeek", "DaysOfMonth", "MonthsOfYear", "LimitToDateRange",
                "StartMonth", "StartDay", "StartYear", "EndMonth", "EndDay", "EndYear"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(template.playout_id)
        .bind(template.template_id)
        .bind(template.updated_ticks.to_string())
        .bind(template.deco_template_id)
        .bind(template.index)
        .bind(encode_integer_list(&template.days_of_week)?)
        .bind(encode_integer_list(&template.days_of_month)?)
        .bind(encode_integer_list(&template.months_of_year)?)
        .bind(template.limit_to_date_range)
        .bind(template.start_month)
        .bind(template.start_day)
        .bind(template.start_year)
        .bind(template.end_month)
        .bind(template.end_day)
        .bind(template.end_year)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}
