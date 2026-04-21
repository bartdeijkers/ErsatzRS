use crate::db::DbRow;
use async_trait::async_trait;
use ersatzrs_core::repositories::{DecoRepository, Result};
use ersatzrs_domain::{Deco, EntityId};
use sqlx::{Pool, Row};

use super::common::{
    decode_enum_column, decode_optional_enum_column, encode_enum, load_ids, not_found, replace_ids,
};

#[derive(Clone)]
pub struct SqliteDecoRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl DecoRepository for SqliteDecoRepository {
    async fn get_with_watermarks(&self, id: EntityId) -> Result<Deco> {
        let row = sqlx::query(r#"SELECT * FROM "Deco" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("deco", id, error))?;
        let mut deco = map_deco(&row)?;
        deco.watermark_ids = load_ids(
            &self.0,
            r#"SELECT "WatermarkId" FROM "DecoWatermark" WHERE "DecoId" = ? ORDER BY "WatermarkId""#,
            id,
        )
        .await?;
        deco.graphics_element_ids =
            load_ids(&self.0, r#"SELECT "GraphicsElementId" FROM "DecoGraphicsElement" WHERE "DecoId" = ? ORDER BY "GraphicsElementId""#, id)
                .await?;
        Ok(deco)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Deco>> {
        let rows =
            sqlx::query(r#"SELECT * FROM "Deco" WHERE "DecoGroupId" = ? ORDER BY "Name", "Id""#)
                .bind(group_id)
                .fetch_all(&self.0)
                .await?;
        rows.iter()
            .map(|row| {
                let mut deco = map_deco(row)?;
                deco.watermark_ids.clear();
                deco.graphics_element_ids.clear();
                Ok(deco)
            })
            .collect()
    }

    async fn upsert(&self, deco: &Deco) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            r#"REPLACE INTO "Deco" (
                "Id", "DecoGroupId", "Name", "BreakContentMode", "DefaultFillerMode",
                "DefaultFillerCollectionType", "DefaultFillerCollectionId",
                "DefaultFillerMediaItemId", "DefaultFillerMultiCollectionId",
                "DefaultFillerSmartCollectionId", "DefaultFillerTrimToFit",
                "DeadAirFallbackMode", "DeadAirFallbackCollectionType",
                "GraphicsElementsMode", "UseGraphicsElementsDuringFiller",
                "UseWatermarkDuringFiller", "WatermarkMode"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0, 0)"#,
        )
        .bind(deco.id)
        .bind(deco.deco_group_id)
        .bind(&deco.name)
        .bind(encode_enum(deco.mode))
        .bind(encode_enum(deco.default_filler_mode))
        .bind(deco.default_filler_collection_type.map(encode_enum))
        .bind(deco.default_filler_collection_id)
        .bind(deco.default_filler_media_item_id)
        .bind(deco.default_filler_multi_collection_id)
        .bind(deco.default_filler_smart_collection_id)
        .bind(deco.default_filler_trim_to_fit)
        .execute(&mut *tx)
        .await?;
        replace_ids(
            &mut tx,
            r#"DELETE FROM "DecoWatermark" WHERE "DecoId" = ?"#,
            r#"REPLACE INTO "DecoWatermark" ("DecoId", "WatermarkId") VALUES (?, ?)"#,
            deco.id,
            &deco.watermark_ids,
        )
        .await?;
        replace_ids(
            &mut tx,
            r#"DELETE FROM "DecoGraphicsElement" WHERE "DecoId" = ?"#,
            r#"REPLACE INTO "DecoGraphicsElement" ("DecoId", "GraphicsElementId") VALUES (?, ?)"#,
            deco.id,
            &deco.graphics_element_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn map_deco(row: &DbRow) -> Result<Deco> {
    Ok(Deco {
        id: row.try_get("Id")?,
        deco_group_id: row.try_get("DecoGroupId")?,
        name: row.try_get("Name")?,
        mode: decode_enum_column("deco mode", row, "BreakContentMode")?,
        break_content_id: None,
        default_filler_mode: decode_optional_enum_column(
            "deco default_filler_mode",
            row,
            "DefaultFillerMode",
        )?
        .unwrap_or_default(),
        default_filler_collection_type: decode_optional_enum_column(
            "deco default_filler_collection_type",
            row,
            "DefaultFillerCollectionType",
        )?,
        default_filler_collection_id: row.try_get("DefaultFillerCollectionId")?,
        default_filler_media_item_id: row.try_get("DefaultFillerMediaItemId")?,
        default_filler_multi_collection_id: row.try_get("DefaultFillerMultiCollectionId")?,
        default_filler_smart_collection_id: row.try_get("DefaultFillerSmartCollectionId")?,
        default_filler_trim_to_fit: row.try_get("DefaultFillerTrimToFit")?,
        watermark_ids: Vec::new(),
        graphics_element_ids: Vec::new(),
    })
}
