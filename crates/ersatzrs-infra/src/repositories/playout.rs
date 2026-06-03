use crate::db::DbRow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{PlayoutRepository, RepositoryError, Result};
use ersatzrs_core::scheduling::SchedulingResult;
use ersatzrs_domain::{
    EntityId, Playout, PlayoutBuildStatus, PlayoutGap, PlayoutHistory, PlayoutItem,
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
        .await?;
        playout.templates = template_rows
            .iter()
            .map(map_playout_template)
            .collect::<Result<Vec<_>>>()?;

        let anchor = sqlx::query(r#"SELECT * FROM "PlayoutAnchor" WHERE "PlayoutId" = ?"#)
            .bind(id)
            .fetch_optional(&self.0)
            .await?;
        if let Some(ref anchor_row) = anchor {
            let state_row = sqlx::query(
                r#"SELECT "Seed", "Index" FROM "ScheduleItemsEnumeratorState" WHERE "PlayoutAnchorPlayoutId" = ?"#,
            )
            .bind(id)
            .fetch_optional(&self.0)
            .await?;
            playout.anchor = Some(map_anchor(anchor_row, state_row.as_ref())?);
        }

        let status = sqlx::query(r#"SELECT * FROM "PlayoutBuildStatus" WHERE "PlayoutId" = ?"#)
            .bind(id)
            .fetch_optional(&self.0)
            .await?;
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
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            let item_id: EntityId = row.try_get("Id")?;
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
        let mut tx = self.0.begin().await?;

        update_playout_checkpoint(&mut tx, playout).await?;
        replace_anchor_state(&mut tx, playout).await?;
        replace_gaps(&mut tx, playout.id, gaps).await?;
        replace_history(&mut tx, playout.id, history).await?;
        replace_program_schedule_anchors(&mut tx, playout.id, program_schedule_anchors).await?;
        replace_fill_group_indices(&mut tx, playout.id, fill_group_indices).await?;
        replace_playout_templates(&mut tx, playout).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn upsert_item(&self, item: &PlayoutItem) -> Result<()> {
        let mut tx = self.0.begin().await?;
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
        .await?;
        replace_ids(
            &mut tx,
            r#"DELETE FROM "PlayoutItemWatermark" WHERE "PlayoutItemId" = ?"#,
            r#"REPLACE INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId") VALUES (?, ?)"#,
            item.id,
            &item.watermark_ids,
        )
        .await?;
        replace_playout_item_graphics_elements(&mut tx, item.id, item).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn delete_items_before(&self, playout_id: EntityId, cutoff: DateTime<Utc>) -> Result<()> {
        sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ? AND "Finish" < ?"#)
            .bind(playout_id)
            .bind(encode_datetime(cutoff))
            .execute(&self.0)
            .await?;
        Ok(())
    }

    async fn apply_build_result(&self, result: &SchedulingResult, playout: &Playout) -> Result<()> {
        let mut tx = self.0.begin().await?;

        // ── items ─────────────────────────────────────────────────────────────
        if result.clear_items {
            sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ?"#)
                .bind(playout.id)
                .execute(&mut *tx)
                .await?;
        } else {
            for &item_id in &result.items_to_remove {
                sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "Id" = ?"#)
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await?;
            }
            if let Some(cutoff) = result.remove_before {
                sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId" = ? AND "Start" < ?"#)
                    .bind(playout.id)
                    .bind(encode_datetime(cutoff))
                    .execute(&mut *tx)
                    .await?;
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
            .await?;
            let inserted_id = crate::db::last_insert_id(inserted);

            for &wm_id in &item.watermark_ids {
                sqlx::query(
                    r#"REPLACE INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId") VALUES (?, ?)"#,
                )
                .bind(inserted_id)
                .bind(wm_id)
                .execute(&mut *tx)
                .await?;
            }
            replace_playout_item_graphics_elements(&mut tx, inserted_id, item).await?;
        }

        // ── gaps ──────────────────────────────────────────────────────────────
        if result.clear_items {
            sqlx::query(r#"DELETE FROM "PlayoutGap" WHERE "PlayoutId" = ?"#)
                .bind(playout.id)
                .execute(&mut *tx)
                .await?;
        }
        for gap in &result.added_gaps {
            sqlx::query(
                r#"INSERT INTO "PlayoutGap" ("PlayoutId", "Start", "Finish") VALUES (?, ?, ?)"#,
            )
            .bind(playout.id)
            .bind(encode_datetime(gap.start))
            .bind(encode_datetime(gap.finish))
            .execute(&mut *tx)
            .await?;
        }

        // ── history ───────────────────────────────────────────────────────────
        if result.clear_items {
            sqlx::query(r#"DELETE FROM "PlayoutHistory" WHERE "PlayoutId" = ?"#)
                .bind(playout.id)
                .execute(&mut *tx)
                .await?;
        } else {
            for &history_id in &result.history_to_remove {
                sqlx::query(r#"DELETE FROM "PlayoutHistory" WHERE "Id" = ?"#)
                    .bind(history_id)
                    .execute(&mut *tx)
                    .await?;
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
            .await?;
        }

        // ── anchor, program-schedule anchors, fill-group indices, templates ───
        replace_anchor_state(&mut tx, playout).await?;
        replace_program_schedule_anchors(&mut tx, playout.id, &result.program_schedule_anchors)
            .await?;
        replace_fill_group_indices(&mut tx, playout.id, &result.fill_group_indices).await?;
        replace_playout_templates(&mut tx, playout).await?;

        tx.commit().await?;
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
        .await?;
        Ok(())
    }
}

fn map_playout(row: &DbRow) -> Result<Playout> {
    let daily_rebuild_time = decode_optional_duration_text(
        row.try_get::<Option<String>, _>("DailyRebuildTime")?
            .as_deref(),
    );
    Ok(Playout {
        id: row.try_get("Id")?,
        channel_id: row.try_get("ChannelId")?,
        program_schedule_id: row.try_get("ProgramScheduleId")?,
        schedule_file: row.try_get("ScheduleFile")?,
        program_schedule_alternate_ids: Vec::new(),
        schedule_kind: decode_enum_column("playout schedule_kind", row, "ScheduleKind")?,
        item_ids: Vec::new(),
        gap_ids: Vec::new(),
        anchor: None,
        program_schedule_anchor_ids: Vec::new(),
        fill_group_index_ids: Vec::new(),
        templates: Vec::new(),
        playout_history_ids: Vec::new(),
        seed: row.try_get("Seed")?,
        daily_rebuild_time,
        deco_id: row.try_get("DecoId")?,
        on_demand_checkpoint: decode_optional_datetime(
            "playout on_demand_checkpoint",
            row.try_get("OnDemandCheckpoint")?,
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
            &row.try_get::<String, _>("NextStart")?,
        )?,
        multiple_remaining: row.try_get("MultipleRemaining")?,
        duration_finish: decode_optional_datetime(
            "playout anchor duration_finish",
            row.try_get("DurationFinish")?,
        )?,
        in_flood: row.try_get("InFlood")?,
        in_duration_filler: row.try_get("InDurationFiller")?,
        next_guide_group: row.try_get("NextGuideGroup")?,
        next_instruction_index: row.try_get("NextInstructionIndex")?,
        context: row.try_get("Context")?,
    })
}

fn map_build_status(row: &DbRow) -> Result<PlayoutBuildStatus> {
    Ok(PlayoutBuildStatus {
        playout_id: row.try_get("PlayoutId")?,
        last_build: decode_datetime(
            "playout build status last_build",
            &row.try_get::<String, _>("LastBuild")?,
        )?,
        success: row.try_get("Success")?,
        message: row.try_get("Message")?,
    })
}

fn map_playout_item(row: &DbRow) -> Result<PlayoutItem> {
    Ok(PlayoutItem {
        id: row.try_get("Id")?,
        media_item_id: row.try_get("MediaItemId")?,
        start: decode_datetime("playout item start", &row.try_get::<String, _>("Start")?)?,
        finish: decode_datetime("playout item finish", &row.try_get::<String, _>("Finish")?)?,
        guide_start: decode_optional_datetime(
            "playout item guide_start",
            row.try_get("GuideStart")?,
        )?,
        guide_finish: decode_optional_datetime(
            "playout item guide_finish",
            row.try_get("GuideFinish")?,
        )?,
        custom_title: row.try_get("CustomTitle")?,
        guide_group: row.try_get("GuideGroup")?,
        filler_kind: decode_enum_column("playout item filler_kind", row, "FillerKind")?,
        playout_id: row.try_get("PlayoutId")?,
        in_point: decode_duration_text(&row.try_get::<String, _>("InPoint")?),
        out_point: decode_duration_text(&row.try_get::<String, _>("OutPoint")?),
        chapter_title: row.try_get("ChapterTitle")?,
        watermark_ids: Vec::new(),
        disable_watermarks: row.try_get("DisableWatermarks")?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode")?,
        preferred_audio_title: row.try_get("PreferredAudioTitle")?,
        preferred_subtitle_language_code: row.try_get("PreferredSubtitleLanguageCode")?,
        subtitle_mode: decode_optional_enum_column(
            "playout item subtitle_mode",
            row,
            "SubtitleMode",
        )?,
        block_key: row.try_get("BlockKey")?,
        collection_key: row.try_get("CollectionKey")?,
        collection_etag: row.try_get("CollectionEtag")?,
        graphics_element_ids: Vec::new(),
        graphics_elements: Vec::new(),
        scheduling_context: row.try_get("SchedulingContext")?,
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
    .await?;

    rows.iter()
        .map(|row| {
            Ok(PlayoutItemGraphicsElement {
                playout_item_id,
                graphics_element_id: row.try_get("GraphicsElementId")?,
                variables: row.try_get("Variables")?,
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
        .await?;

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
    .await?;
    Ok(())
}

fn map_playout_template(row: &DbRow) -> Result<ersatzrs_domain::PlayoutTemplate> {
    Ok(ersatzrs_domain::PlayoutTemplate {
        playout_id: row.try_get("PlayoutId")?,
        template_id: row.try_get("TemplateId")?,
        // DateUpdated is a TEXT datetime in the ErsatzTV schema; we store updated_ticks as i64.
        // Parse the stored string back as i64, or fall back to 0.
        updated_ticks: row
            .try_get::<String, _>("DateUpdated")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0),
        deco_template_id: row.try_get("DecoTemplateId")?,
        index: row.try_get("Index")?,
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
        limit_to_date_range: row.try_get("LimitToDateRange")?,
        start_month: row.try_get("StartMonth")?,
        start_day: row.try_get("StartDay")?,
        start_year: row.try_get("StartYear")?,
        end_month: row.try_get("EndMonth")?,
        end_day: row.try_get("EndDay")?,
        end_year: row.try_get("EndYear")?,
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
        .await?;
    Ok(())
}

async fn replace_anchor_state(
    tx: &mut Transaction<'_, crate::db::Db>,
    playout: &Playout,
) -> Result<()> {
    if let Some(anchor) = &playout.anchor {
        sqlx::query(
            r#"REPLACE INTO "PlayoutAnchor" (
                "PlayoutId", "DurationFinish", "InDurationFiller", "InFlood",
                "MultipleRemaining", "NextGuideGroup", "NextStart",
                "NextInstructionIndex", "Context"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(playout.id)
        .bind(encode_optional_datetime(anchor.duration_finish))
        .bind(anchor.in_duration_filler)
        .bind(anchor.in_flood)
        .bind(anchor.multiple_remaining)
        .bind(anchor.next_guide_group)
        .bind(encode_datetime(anchor.next_start))
        .bind(anchor.next_instruction_index)
        .bind(&anchor.context)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"REPLACE INTO "ScheduleItemsEnumeratorState" (
                "PlayoutAnchorPlayoutId", "Seed", "Index"
             ) VALUES (?, ?, ?)"#,
        )
        .bind(playout.id)
        .bind(anchor.schedule_items_enumerator_state.seed)
        .bind(anchor.schedule_items_enumerator_state.index)
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query(r#"DELETE FROM "PlayoutAnchor" WHERE "PlayoutId" = ?"#)
            .bind(playout.id)
            .execute(&mut **tx)
            .await?;
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
        .await?;
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
        .await?;
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
        .await?;
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
        .await?;
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
    .await?;
    sqlx::query(r#"DELETE FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .execute(&mut **tx)
        .await?;
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
        .await?;
        sqlx::query(
            r#"REPLACE INTO "CollectionEnumeratorState" (
                "PlayoutProgramScheduleAnchorId", "Seed", "Index"
             ) VALUES (?, ?, ?)"#,
        )
        .bind(anchor.id)
        .bind(anchor.enumerator_state.seed)
        .bind(anchor.enumerator_state.index)
        .execute(&mut **tx)
        .await?;
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
    .await?;
    sqlx::query(r#"DELETE FROM "PlayoutScheduleItemFillGroupIndex" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .execute(&mut **tx)
        .await?;
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
        .await?;
        sqlx::query(
            r#"REPLACE INTO "FillGroupEnumeratorState" (
                "PlayoutScheduleItemFillGroupIndexId", "Seed", "Index"
             ) VALUES (?, ?, ?)"#,
        )
        .bind(index.id)
        .bind(index.enumerator_state.seed)
        .bind(index.enumerator_state.index)
        .execute(&mut **tx)
        .await?;
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
        .await?;
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
        .await?;
    }
    Ok(())
}
