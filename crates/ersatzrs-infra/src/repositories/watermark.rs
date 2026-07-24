use async_trait::async_trait;
use ersatzrs_core::repositories::{Result, WatermarkRepository};
use ersatzrs_domain::{ChannelWatermark, EntityId};
use sqlx::{Pool, Row};

use crate::db::{Db, DbRow};
use crate::repositories::common::{
    decode_bool_column, decode_enum_column, decode_optional_string_column, decode_string_column,
    encode_enum,
};
use crate::repositories::SqlxResultExt as _;

#[derive(Clone)]
pub struct SqliteWatermarkRepository(pub Pool<Db>);

#[async_trait]
impl WatermarkRepository for SqliteWatermarkRepository {
    async fn list_admin(&self) -> Result<Vec<ChannelWatermark>> {
        let rows = sqlx::query(r#"SELECT * FROM "ChannelWatermark" ORDER BY "Name", "Id""#)
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter().map(map_watermark).collect()
    }

    async fn get_admin(&self, id: EntityId) -> Result<Option<ChannelWatermark>> {
        sqlx::query(r#"SELECT * FROM "ChannelWatermark" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?
            .as_ref()
            .map(map_watermark)
            .transpose()
    }

    async fn name_exists(&self, id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ChannelWatermark" WHERE "Name" = ? AND "Id" <> ?"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn save_admin(&self, watermark: &ChannelWatermark) -> Result<EntityId> {
        if watermark.id == 0 {
            let result = sqlx::query(
                r#"INSERT INTO "ChannelWatermark" (
                    "Name", "Mode", "ImageSource", "Image", "OriginalContentType",
                    "Location", "Size", "WidthPercent", "HorizontalMarginPercent",
                    "VerticalMarginPercent", "FrequencyMinutes", "DurationSeconds",
                    "Opacity", "PlaceWithinSourceContent", "OpacityExpression", "ZIndex"
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(&watermark.name)
            .bind(encode_enum(watermark.mode))
            .bind(encode_enum(watermark.image_source))
            .bind(&watermark.image)
            .bind(&watermark.original_content_type)
            .bind(encode_enum(watermark.location))
            .bind(encode_enum(watermark.size))
            .bind(watermark.width_percent)
            .bind(watermark.horizontal_margin_percent)
            .bind(watermark.vertical_margin_percent)
            .bind(watermark.frequency_minutes)
            .bind(watermark.duration_seconds)
            .bind(watermark.opacity)
            .bind(watermark.place_within_source_content)
            .bind(&watermark.opacity_expression)
            .bind(watermark.z_index)
            .execute(&self.0)
            .await
            .repository()?;
            return Ok(crate::db::last_insert_id(result));
        }

        let result = sqlx::query(
            r#"UPDATE "ChannelWatermark" SET
                "Name" = ?, "Mode" = ?, "ImageSource" = ?, "Image" = ?,
                "OriginalContentType" = ?, "Location" = ?, "Size" = ?,
                "WidthPercent" = ?, "HorizontalMarginPercent" = ?,
                "VerticalMarginPercent" = ?, "FrequencyMinutes" = ?,
                "DurationSeconds" = ?, "Opacity" = ?, "PlaceWithinSourceContent" = ?,
                "OpacityExpression" = ?, "ZIndex" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&watermark.name)
        .bind(encode_enum(watermark.mode))
        .bind(encode_enum(watermark.image_source))
        .bind(&watermark.image)
        .bind(&watermark.original_content_type)
        .bind(encode_enum(watermark.location))
        .bind(encode_enum(watermark.size))
        .bind(watermark.width_percent)
        .bind(watermark.horizontal_margin_percent)
        .bind(watermark.vertical_margin_percent)
        .bind(watermark.frequency_minutes)
        .bind(watermark.duration_seconds)
        .bind(watermark.opacity)
        .bind(watermark.place_within_source_content)
        .bind(&watermark.opacity_expression)
        .bind(watermark.z_index)
        .bind(watermark.id)
        .execute(&self.0)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "watermark",
                id: watermark.id,
            });
        }
        Ok(watermark.id)
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        let result = sqlx::query(r#"DELETE FROM "ChannelWatermark" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "watermark",
                id,
            });
        }
        Ok(())
    }

    async fn copy_admin(&self, id: EntityId, name: &str) -> Result<EntityId> {
        let result = sqlx::query(
            r#"INSERT INTO "ChannelWatermark" (
                "Name", "Mode", "ImageSource", "Image", "OriginalContentType",
                "Location", "Size", "WidthPercent", "HorizontalMarginPercent",
                "VerticalMarginPercent", "FrequencyMinutes", "DurationSeconds",
                "Opacity", "PlaceWithinSourceContent", "OpacityExpression", "ZIndex"
            )
            SELECT ?, "Mode", "ImageSource", "Image", "OriginalContentType",
                "Location", "Size", "WidthPercent", "HorizontalMarginPercent",
                "VerticalMarginPercent", "FrequencyMinutes", "DurationSeconds",
                "Opacity", "PlaceWithinSourceContent", "OpacityExpression", "ZIndex"
            FROM "ChannelWatermark" WHERE "Id" = ?"#,
        )
        .bind(name)
        .bind(id)
        .execute(&self.0)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "watermark",
                id,
            });
        }
        Ok(crate::db::last_insert_id(result))
    }
}

fn map_watermark(row: &DbRow) -> Result<ChannelWatermark> {
    Ok(ChannelWatermark {
        id: row.try_get("Id").repository()?,
        name: decode_string_column(row, "Name")?,
        mode: decode_enum_column("watermark mode", row, "Mode")?,
        image_source: decode_enum_column("watermark image_source", row, "ImageSource")?,
        image: decode_optional_string_column(row, "Image")?,
        original_content_type: decode_optional_string_column(row, "OriginalContentType")?,
        location: decode_enum_column("watermark location", row, "Location")?,
        size: decode_enum_column("watermark size", row, "Size")?,
        width_percent: row.try_get("WidthPercent").repository()?,
        horizontal_margin_percent: row.try_get("HorizontalMarginPercent").repository()?,
        vertical_margin_percent: row.try_get("VerticalMarginPercent").repository()?,
        frequency_minutes: row.try_get("FrequencyMinutes").repository()?,
        duration_seconds: row.try_get("DurationSeconds").repository()?,
        opacity: row.try_get("Opacity").repository()?,
        place_within_source_content: decode_bool_column(row, "PlaceWithinSourceContent")?,
        opacity_expression: decode_optional_string_column(row, "OpacityExpression")?,
        z_index: row.try_get("ZIndex").repository()?,
    })
}
