use async_trait::async_trait;
use nastv_core::repositories::{FillerRepository, Result};
use nastv_domain::{EntityId, FillerPreset};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{
    decode_enum, decode_optional_duration, encode_optional_duration, not_found,
};

#[derive(Clone)]
pub struct SqliteFillerRepository(pub Pool<Sqlite>);

#[async_trait]
impl FillerRepository for SqliteFillerRepository {
    async fn get(&self, id: EntityId) -> Result<FillerPreset> {
        let row = sqlx::query("SELECT * FROM filler_presets WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("filler preset", id, error))?;
        map_filler(&row)
    }

    async fn list(&self) -> Result<Vec<FillerPreset>> {
        let rows = sqlx::query("SELECT * FROM filler_presets ORDER BY name, id")
            .fetch_all(&self.0)
            .await?;
        rows.iter().map(map_filler).collect()
    }

    async fn upsert(&self, preset: &FillerPreset) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO filler_presets (
                id, name, filler_mode, collection_type, collection_id, media_item_id,
                multi_collection_id, smart_collection_id, playlist_id, duration_ms, count,
                pad_to_next_hour, allow_watermarks, playback_order
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(preset.id)
        .bind(&preset.name)
        .bind(format!("{:?}", preset.filler_mode))
        .bind(format!("{:?}", preset.collection_type))
        .bind(preset.collection_id)
        .bind(preset.media_item_id)
        .bind(preset.multi_collection_id)
        .bind(preset.smart_collection_id)
        .bind(preset.playlist_id)
        .bind(encode_optional_duration(preset.duration))
        .bind(preset.count)
        .bind(preset.pad_to_next_hour)
        .bind(preset.allow_watermarks)
        .bind(format!("{:?}", preset.playback_order))
        .execute(&self.0)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query("DELETE FROM filler_presets WHERE id = ?")
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}

fn map_filler(row: &SqliteRow) -> Result<FillerPreset> {
    Ok(FillerPreset {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        filler_mode: decode_enum(
            "filler preset filler_mode",
            &row.try_get::<String, _>("filler_mode")?,
        )?,
        collection_type: decode_enum(
            "filler preset collection_type",
            &row.try_get::<String, _>("collection_type")?,
        )?,
        collection_id: row.try_get("collection_id")?,
        media_item_id: row.try_get("media_item_id")?,
        multi_collection_id: row.try_get("multi_collection_id")?,
        smart_collection_id: row.try_get("smart_collection_id")?,
        playlist_id: row.try_get("playlist_id")?,
        duration: decode_optional_duration(row.try_get("duration_ms")?),
        count: row.try_get("count")?,
        pad_to_next_hour: row.try_get("pad_to_next_hour")?,
        allow_watermarks: row.try_get("allow_watermarks")?,
        playback_order: decode_enum(
            "filler preset playback_order",
            &row.try_get::<String, _>("playback_order")?,
        )?,
    })
}
