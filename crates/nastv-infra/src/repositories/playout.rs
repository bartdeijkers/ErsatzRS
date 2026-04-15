use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nastv_core::repositories::{PlayoutRepository, Result};
use nastv_domain::{EntityId, Playout, PlayoutBuildStatus, PlayoutItem};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{
    decode_datetime, decode_duration, decode_enum, decode_optional_datetime,
    decode_optional_duration, encode_datetime, encode_optional_datetime, load_ids, not_found,
    replace_ids,
};

#[derive(Clone)]
pub struct SqlitePlayoutRepository(pub Pool<Sqlite>);

#[async_trait]
impl PlayoutRepository for SqlitePlayoutRepository {
    async fn get_with_anchor(&self, id: EntityId) -> Result<Playout> {
        let row = sqlx::query("SELECT * FROM playouts WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("playout", id, error))?;
        let mut playout = map_playout(&row)?;
        playout.item_ids = load_ids(
            &self.0,
            "SELECT id FROM playout_items WHERE playout_id = ? ORDER BY start, id",
            id,
        )
        .await?;
        playout.gap_ids = load_ids(
            &self.0,
            "SELECT id FROM playout_gaps WHERE playout_id = ? ORDER BY start, id",
            id,
        )
        .await?;
        playout.program_schedule_anchor_ids = load_ids(
            &self.0,
            "SELECT id FROM playout_program_schedule_anchors WHERE playout_id = ? ORDER BY id",
            id,
        )
        .await?;
        playout.fill_group_index_ids = load_ids(
            &self.0,
            "SELECT id FROM playout_schedule_item_fill_group_indices WHERE playout_id = ? ORDER BY id",
            id,
        )
        .await?;
        playout.playout_history_ids = load_ids(
            &self.0,
            "SELECT id FROM playout_history WHERE playout_id = ? ORDER BY history_when, id",
            id,
        )
        .await?;
        let template_ids = load_ids(
            &self.0,
            "SELECT template_id FROM playout_templates WHERE playout_id = ? ORDER BY template_id",
            id,
        )
        .await?;
        playout.templates = template_ids
            .into_iter()
            .map(|template_id| nastv_domain::PlayoutTemplate {
                playout_id: id,
                template_id,
            })
            .collect();

        let anchor = sqlx::query("SELECT * FROM playout_anchors WHERE playout_id = ?")
            .bind(id)
            .fetch_optional(&self.0)
            .await?;
        playout.anchor = anchor.as_ref().map(map_anchor).transpose()?;

        let status = sqlx::query("SELECT * FROM playout_build_statuses WHERE playout_id = ?")
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
            "SELECT *
             FROM playout_items
             WHERE playout_id = ? AND start < ? AND finish > ?
             ORDER BY start, id",
        )
        .bind(playout_id)
        .bind(finish.to_rfc3339())
        .bind(start.to_rfc3339())
        .fetch_all(&self.0)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            let item_id: EntityId = row.try_get("id")?;
            let mut item = map_playout_item(row)?;
            item.watermark_ids = load_ids(
                &self.0,
                "SELECT watermark_id FROM playout_item_watermarks WHERE playout_item_id = ? ORDER BY watermark_id",
                item_id,
            )
            .await?;
            item.graphics_element_ids = load_ids(
                &self.0,
                "SELECT graphics_element_id FROM playout_item_graphics_elements WHERE playout_item_id = ? ORDER BY graphics_element_id",
                item_id,
            )
            .await?;
            items.push(item);
        }
        Ok(items)
    }

    async fn upsert_item(&self, item: &PlayoutItem) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO playout_items (
                id, playout_id, media_item_id, start, finish, guide_start, guide_finish,
                custom_title, guide_group, filler_kind, in_point_ms, out_point_ms,
                chapter_title, disable_watermarks, preferred_audio_language_code,
                preferred_audio_title, preferred_subtitle_language_code, subtitle_mode,
                block_key, collection_key, collection_etag, scheduling_context
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(format!("{:?}", item.filler_kind))
        .bind(item.in_point.num_milliseconds())
        .bind(item.out_point.num_milliseconds())
        .bind(&item.chapter_title)
        .bind(item.disable_watermarks)
        .bind(&item.preferred_audio_language_code)
        .bind(&item.preferred_audio_title)
        .bind(&item.preferred_subtitle_language_code)
        .bind(item.subtitle_mode.map(|mode| format!("{mode:?}")))
        .bind(&item.block_key)
        .bind(&item.collection_key)
        .bind(&item.collection_etag)
        .bind(&item.scheduling_context)
        .execute(&mut *tx)
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM playout_item_watermarks WHERE playout_item_id = ?",
            "INSERT OR REPLACE INTO playout_item_watermarks (playout_item_id, watermark_id) VALUES (?, ?)",
            item.id,
            &item.watermark_ids,
        )
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM playout_item_graphics_elements WHERE playout_item_id = ?",
            "INSERT OR REPLACE INTO playout_item_graphics_elements (playout_item_id, graphics_element_id) VALUES (?, ?)",
            item.id,
            &item.graphics_element_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn delete_items_before(&self, playout_id: EntityId, cutoff: DateTime<Utc>) -> Result<()> {
        sqlx::query("DELETE FROM playout_items WHERE playout_id = ? AND finish < ?")
            .bind(playout_id)
            .bind(encode_datetime(cutoff))
            .execute(&self.0)
            .await?;
        Ok(())
    }

    async fn upsert_build_status(&self, status: &PlayoutBuildStatus) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO playout_build_statuses (playout_id, last_build, success, message)
             VALUES (?, ?, ?, ?)",
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

fn map_playout(row: &SqliteRow) -> Result<Playout> {
    Ok(Playout {
        id: row.try_get("id")?,
        channel_id: row.try_get("channel_id")?,
        program_schedule_id: row.try_get("program_schedule_id")?,
        schedule_file: row.try_get("schedule_file")?,
        program_schedule_alternate_ids: Vec::new(),
        schedule_kind: decode_enum(
            "playout schedule_kind",
            &row.try_get::<String, _>("schedule_kind")?,
        )?,
        item_ids: Vec::new(),
        gap_ids: Vec::new(),
        anchor: None,
        program_schedule_anchor_ids: Vec::new(),
        fill_group_index_ids: Vec::new(),
        templates: Vec::new(),
        playout_history_ids: Vec::new(),
        seed: row.try_get("seed")?,
        daily_rebuild_time: decode_optional_duration(row.try_get("daily_rebuild_time_ms")?),
        deco_id: row.try_get("deco_id")?,
        on_demand_checkpoint: decode_optional_datetime(
            "playout on_demand_checkpoint",
            row.try_get("on_demand_checkpoint")?,
        )?,
        build_status: None,
    })
}

fn map_anchor(row: &SqliteRow) -> Result<nastv_domain::PlayoutAnchor> {
    Ok(nastv_domain::PlayoutAnchor {
        schedule_items_enumerator_state: nastv_domain::CollectionEnumeratorState {
            seed: row.try_get("schedule_items_seed")?,
            index: row.try_get("schedule_items_index")?,
        },
        next_start: decode_datetime(
            "playout anchor next_start",
            &row.try_get::<String, _>("next_start")?,
        )?,
        multiple_remaining: row.try_get("multiple_remaining")?,
        duration_finish: decode_optional_datetime(
            "playout anchor duration_finish",
            row.try_get("duration_finish")?,
        )?,
        in_flood: row.try_get("in_flood")?,
        in_duration_filler: row.try_get("in_duration_filler")?,
        next_guide_group: row.try_get("next_guide_group")?,
        next_instruction_index: row.try_get("next_instruction_index")?,
        context: row.try_get("context")?,
    })
}

fn map_build_status(row: &SqliteRow) -> Result<PlayoutBuildStatus> {
    Ok(PlayoutBuildStatus {
        playout_id: row.try_get("playout_id")?,
        last_build: decode_datetime(
            "playout build status last_build",
            &row.try_get::<String, _>("last_build")?,
        )?,
        success: row.try_get("success")?,
        message: row.try_get("message")?,
    })
}

fn map_playout_item(row: &SqliteRow) -> Result<PlayoutItem> {
    Ok(PlayoutItem {
        id: row.try_get("id")?,
        media_item_id: row.try_get("media_item_id")?,
        start: decode_datetime("playout item start", &row.try_get::<String, _>("start")?)?,
        finish: decode_datetime("playout item finish", &row.try_get::<String, _>("finish")?)?,
        guide_start: decode_optional_datetime(
            "playout item guide_start",
            row.try_get("guide_start")?,
        )?,
        guide_finish: decode_optional_datetime(
            "playout item guide_finish",
            row.try_get("guide_finish")?,
        )?,
        custom_title: row.try_get("custom_title")?,
        guide_group: row.try_get("guide_group")?,
        filler_kind: decode_enum(
            "playout item filler_kind",
            &row.try_get::<String, _>("filler_kind")?,
        )?,
        playout_id: row.try_get("playout_id")?,
        in_point: decode_duration(row.try_get("in_point_ms")?),
        out_point: decode_duration(row.try_get("out_point_ms")?),
        chapter_title: row.try_get("chapter_title")?,
        watermark_ids: Vec::new(),
        disable_watermarks: row.try_get("disable_watermarks")?,
        preferred_audio_language_code: row.try_get("preferred_audio_language_code")?,
        preferred_audio_title: row.try_get("preferred_audio_title")?,
        preferred_subtitle_language_code: row.try_get("preferred_subtitle_language_code")?,
        subtitle_mode: row
            .try_get::<Option<String>, _>("subtitle_mode")?
            .as_deref()
            .map(|value| decode_enum("playout item subtitle_mode", value))
            .transpose()?,
        block_key: row.try_get("block_key")?,
        collection_key: row.try_get("collection_key")?,
        collection_etag: row.try_get("collection_etag")?,
        graphics_element_ids: Vec::new(),
        scheduling_context: row.try_get("scheduling_context")?,
    })
}
