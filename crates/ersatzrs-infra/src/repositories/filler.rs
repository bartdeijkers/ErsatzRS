use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{FillerRepository, Result};
use ersatzrs_domain::{EntityId, FillerPreset};
use sqlx::{Pool, Row};

use super::common::{
    decode_enum_column, decode_optional_duration_text, encode_duration, encode_enum, not_found,
};

#[derive(Clone)]
pub struct SqliteFillerRepository(pub Pool<crate::db::Db>);

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

    async fn get_optional_admin(&self, id: EntityId) -> Result<Option<FillerPreset>> {
        sqlx::query(r#"SELECT * FROM "FillerPreset" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?
            .as_ref()
            .map(map_filler)
            .transpose()
    }

    async fn list(&self) -> Result<Vec<FillerPreset>> {
        let rows = sqlx::query(r#"SELECT * FROM "FillerPreset" ORDER BY "Name", "Id""#)
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter().map(map_filler).collect()
    }

    async fn upsert(&self, preset: &FillerPreset) -> Result<()> {
        sqlx::query(
            r#"REPLACE INTO "FillerPreset" (
                "Id", "Name", "FillerMode", "FillerKind", "CollectionType", "CollectionId",
                "MediaItemId", "MultiCollectionId", "SmartCollectionId", "PlaylistId",
                "Duration", "Count", "PadToNearestMinute", "AllowWatermarks",
                "Expression", "UseChaptersAsMediaItems"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(preset.id)
        .bind(&preset.name)
        .bind(encode_enum(preset.filler_mode))
        .bind(encode_enum(preset.filler_kind))
        .bind(encode_enum(preset.collection_type))
        .bind(preset.collection_id)
        .bind(preset.media_item_id)
        .bind(preset.multi_collection_id)
        .bind(preset.smart_collection_id)
        .bind(preset.playlist_id)
        .bind(preset.duration.map(encode_duration))
        .bind(preset.count)
        .bind(preset.pad_to_nearest_minute)
        .bind(preset.allow_watermarks)
        .bind(&preset.expression)
        .bind(preset.use_chapters_as_media_items)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "FillerPreset" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        Ok(())
    }

    async fn name_exists(&self, id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "FillerPreset" WHERE "Name" = ? AND "Id" <> ?"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn save_admin(&self, preset: &FillerPreset) -> Result<EntityId> {
        if preset.id > 0 {
            self.upsert(preset).await?;
            return Ok(preset.id);
        }
        let result = sqlx::query(
            r#"INSERT INTO "FillerPreset" (
                "Name", "FillerMode", "FillerKind", "CollectionType", "CollectionId",
                "MediaItemId", "MultiCollectionId", "SmartCollectionId", "PlaylistId",
                "Duration", "Count", "PadToNearestMinute", "AllowWatermarks",
                "Expression", "UseChaptersAsMediaItems"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&preset.name)
        .bind(encode_enum(preset.filler_mode))
        .bind(encode_enum(preset.filler_kind))
        .bind(encode_enum(preset.collection_type))
        .bind(preset.collection_id)
        .bind(preset.media_item_id)
        .bind(preset.multi_collection_id)
        .bind(preset.smart_collection_id)
        .bind(preset.playlist_id)
        .bind(preset.duration.map(encode_duration))
        .bind(preset.count)
        .bind(preset.pad_to_nearest_minute)
        .bind(preset.allow_watermarks)
        .bind(&preset.expression)
        .bind(preset.use_chapters_as_media_items)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        let result = sqlx::query(r#"DELETE FROM "FillerPreset" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "filler preset",
                id,
            });
        }
        Ok(())
    }
}

fn map_filler(row: &DbRow) -> Result<FillerPreset> {
    Ok(FillerPreset {
        id: row.try_get("Id").repository()?,
        name: row.try_get("Name").repository()?,
        filler_kind: decode_enum_column("filler preset filler_kind", row, "FillerKind")?,
        filler_mode: decode_enum_column("filler preset filler_mode", row, "FillerMode")?,
        collection_type: decode_enum_column(
            "filler preset collection_type",
            row,
            "CollectionType",
        )?,
        collection_id: row.try_get("CollectionId").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        multi_collection_id: row.try_get("MultiCollectionId").repository()?,
        smart_collection_id: row.try_get("SmartCollectionId").repository()?,
        playlist_id: row.try_get("PlaylistId").repository()?,
        duration: decode_optional_duration_text(
            row.try_get::<Option<String>, _>("Duration")
                .repository()?
                .as_deref(),
        ),
        count: row.try_get("Count").repository()?,
        pad_to_nearest_minute: row.try_get("PadToNearestMinute").repository()?,
        allow_watermarks: row.try_get("AllowWatermarks").repository()?,
        expression: row.try_get("Expression").repository()?,
        use_chapters_as_media_items: row.try_get("UseChaptersAsMediaItems").repository()?,
        // ErsatzTV FillerPreset has no PlaybackOrder column; use default
        playback_order: ersatzrs_domain::PlaybackOrder::default(),
    })
}
