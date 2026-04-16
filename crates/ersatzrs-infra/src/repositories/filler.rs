use async_trait::async_trait;
use ersatzrs_core::repositories::{FillerRepository, Result};
use ersatzrs_domain::{EntityId, FillerPreset};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{
    decode_duration_text, decode_enum, decode_optional_duration_text, encode_duration,
    encode_optional_duration, not_found,
};

#[derive(Clone)]
pub struct SqliteFillerRepository(pub Pool<Sqlite>);

#[async_trait]
impl FillerRepository for SqliteFillerRepository {
    async fn get(&self, id: EntityId) -> Result<FillerPreset> {
        let row = sqlx::query(r#"SELECT * FROM "FillerPreset" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("filler preset", id, error))?;
        map_filler(&row)
    }

    async fn list(&self) -> Result<Vec<FillerPreset>> {
        let rows = sqlx::query(r#"SELECT * FROM "FillerPreset" ORDER BY "Name", "Id""#)
            .fetch_all(&self.0)
            .await?;
        rows.iter().map(map_filler).collect()
    }

    async fn upsert(&self, preset: &FillerPreset) -> Result<()> {
        sqlx::query(
            r#"INSERT OR REPLACE INTO "FillerPreset" (
                "Id", "Name", "FillerMode", "FillerKind", "CollectionType", "CollectionId",
                "MediaItemId", "MultiCollectionId", "SmartCollectionId", "PlaylistId",
                "Duration", "Count", "PadToNearestMinute", "AllowWatermarks",
                "UseChaptersAsMediaItems"
             ) VALUES (?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)"#,
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
        .bind(preset.duration.map(encode_duration))
        .bind(preset.count)
        .bind(preset.pad_to_next_hour)
        .bind(preset.allow_watermarks)
        .execute(&self.0)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "FillerPreset" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}

fn map_filler(row: &SqliteRow) -> Result<FillerPreset> {
    Ok(FillerPreset {
        id: row.try_get("Id")?,
        name: row.try_get("Name")?,
        filler_mode: decode_enum(
            "filler preset filler_mode",
            &row.try_get::<String, _>("FillerMode")?,
        )?,
        collection_type: decode_enum(
            "filler preset collection_type",
            &row.try_get::<String, _>("CollectionType")?,
        )?,
        collection_id: row.try_get("CollectionId")?,
        media_item_id: row.try_get("MediaItemId")?,
        multi_collection_id: row.try_get("MultiCollectionId")?,
        smart_collection_id: row.try_get("SmartCollectionId")?,
        playlist_id: row.try_get("PlaylistId")?,
        duration: decode_optional_duration_text(row.try_get("Duration")?),
        count: row.try_get("Count")?,
        pad_to_next_hour: row.try_get::<Option<bool>, _>("PadToNearestMinute")?.unwrap_or(false),
        allow_watermarks: row.try_get("AllowWatermarks")?,
        // ErsatzTV FillerPreset has no PlaybackOrder column; use default
        playback_order: ersatzrs_domain::PlaybackOrder::default(),
    })
}
