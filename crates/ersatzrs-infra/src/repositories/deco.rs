use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{
    DecoAdminRecord, DecoAdminRow, DecoBreakContentAdmin, DecoEditorOptionsAdmin,
    DecoGroupAdminRow, DecoOptionAdmin, DecoPlaylistOptionAdmin, DecoRepository,
    DecoTemplateAdminRow, DecoTemplateGroupAdminRow, DecoTemplateItemAdmin, RepositoryError,
    Result,
};
use ersatzrs_domain::{CollectionType, Deco, DecoMode, EntityId};
use sqlx::{Pool, Row};

use super::common::upsert_sql;
use super::common::{
    decode_bool_column, decode_enum_column, decode_optional_enum_column,
    decode_optional_string_column, decode_string_column, encode_enum, load_ids, not_found,
    replace_ids,
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
                .await
                .repository()?;
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
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        sqlx::query(upsert_sql!(
            r#"INSERT INTO "Deco" (
                "Id", "DecoGroupId", "Name", "BreakContentMode", "DefaultFillerMode",
                "DefaultFillerCollectionType", "DefaultFillerCollectionId",
                "DefaultFillerMediaItemId", "DefaultFillerMultiCollectionId",
                "DefaultFillerSmartCollectionId", "DefaultFillerTrimToFit",
                "DeadAirFallbackMode", "DeadAirFallbackCollectionType",
                "GraphicsElementsMode", "UseGraphicsElementsDuringFiller",
                "UseWatermarkDuringFiller", "WatermarkMode"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0, 0)"#,
            &[&["Id"], &["DecoGroupId", "Name"]],
        ))
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
        .await
        .repository()?;
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
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn list_groups_admin(&self) -> Result<Vec<DecoGroupAdminRow>> {
        let rows = sqlx::query(
            r#"SELECT dg."Id", dg."Name", COUNT(d."Id") AS "DecoCount"
               FROM "DecoGroup" dg
               LEFT JOIN "Deco" d ON d."DecoGroupId" = dg."Id"
               GROUP BY dg."Id", dg."Name"
               ORDER BY dg."Name", dg."Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(DecoGroupAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: decode_string_column(row, "Name")?,
                    deco_count: row.try_get("DecoCount").repository()?,
                })
            })
            .collect()
    }

    async fn list_admin(&self, search: &str) -> Result<Vec<DecoAdminRow>> {
        let search = search.trim();
        let like = format!("%{search}%");
        let rows = sqlx::query(
            r#"SELECT d."Id", d."DecoGroupId", dg."Name" AS "GroupName", d."Name"
               FROM "Deco" d
               JOIN "DecoGroup" dg ON dg."Id" = d."DecoGroupId"
               WHERE (? = '' OR d."Name" LIKE ?)
               UNION ALL
               SELECT -dg."Id" AS "Id", dg."Id" AS "DecoGroupId",
                      dg."Name" AS "GroupName", '(none)' AS "Name"
               FROM "DecoGroup" dg
               WHERE ? = ''
                 AND NOT EXISTS (SELECT 1 FROM "Deco" d WHERE d."DecoGroupId" = dg."Id")
               ORDER BY "GroupName", "Name""#,
        )
        .bind(search)
        .bind(like)
        .bind(search)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter().map(map_deco_admin_row).collect()
    }

    async fn group_exists_admin(&self, id: EntityId) -> Result<bool> {
        exists(
            &self.0,
            r#"SELECT COUNT(*) FROM "DecoGroup" WHERE "Id" = ?"#,
            id,
        )
        .await
    }

    async fn deco_exists_admin(&self, id: EntityId) -> Result<bool> {
        exists(&self.0, r#"SELECT COUNT(*) FROM "Deco" WHERE "Id" = ?"#, id).await
    }

    async fn group_name_exists_admin(&self, name: &str) -> Result<bool> {
        name_exists(
            &self.0,
            r#"SELECT COUNT(*) FROM "DecoGroup" WHERE "Name" = ?"#,
            name,
        )
        .await
    }

    async fn deco_name_exists_admin(
        &self,
        group_id: EntityId,
        deco_id: Option<EntityId>,
        name: &str,
    ) -> Result<bool> {
        scoped_name_exists(&self.0, "Deco", "DecoGroupId", group_id, deco_id, name).await
    }

    async fn create_group_admin(&self, name: &str) -> Result<EntityId> {
        insert_name(
            &self.0,
            r#"INSERT INTO "DecoGroup" ("Name") VALUES (?)"#,
            name,
        )
        .await
    }

    async fn create_admin(&self, group_id: EntityId, name: &str) -> Result<EntityId> {
        let result = sqlx::query(
            r#"INSERT INTO "Deco" (
                   "DecoGroupId", "Name", "BreakContentMode",
                   "DefaultFillerMode", "DefaultFillerCollectionType", "DefaultFillerTrimToFit",
                   "DeadAirFallbackMode", "DeadAirFallbackCollectionType",
                   "GraphicsElementsMode", "UseGraphicsElementsDuringFiller",
                   "UseWatermarkDuringFiller", "WatermarkMode"
               ) VALUES (?, ?, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)"#,
        )
        .bind(group_id)
        .bind(name)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn delete_group_admin(&self, id: EntityId) -> Result<()> {
        checked_delete(
            &self.0,
            r#"DELETE FROM "DecoGroup" WHERE "Id" = ?"#,
            "deco group",
            id,
        )
        .await
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        checked_delete(&self.0, r#"DELETE FROM "Deco" WHERE "Id" = ?"#, "deco", id).await
    }

    async fn get_admin(&self, id: EntityId) -> Result<Option<DecoAdminRecord>> {
        let row = sqlx::query(DECO_ADMIN_SQL)
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        map_deco_admin_record(&self.0, &row).await.map(Some)
    }

    async fn editor_options_admin(&self) -> Result<DecoEditorOptionsAdmin> {
        Ok(DecoEditorOptionsAdmin {
            watermarks: list_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "ChannelWatermark" ORDER BY "Name", "Id""#,
            )
            .await?,
            graphics_elements: list_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", "Path", '') AS "Name"
                   FROM "GraphicsElement" ORDER BY "Name", "Id""#,
            )
            .await?,
            collections: list_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "Collection" ORDER BY "Name", "Id""#,
            )
            .await?,
            shows: list_options(
                &self.0,
                r#"SELECT sh."Id", COALESCE(sm."Title", '') AS "Name"
                   FROM "Show" sh LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
                   ORDER BY sm."Title", sh."Id""#,
            )
            .await?,
            seasons: list_deco_seasons(&self.0).await?,
            artists: list_options(
                &self.0,
                r#"SELECT a."Id", COALESCE(am."Title", '') AS "Name"
                   FROM "Artist" a LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
                   ORDER BY am."Title", a."Id""#,
            )
            .await?,
            multi_collections: list_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "MultiCollection" ORDER BY "Name", "Id""#,
            )
            .await?,
            smart_collections: list_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "SmartCollection" ORDER BY "Name", "Id""#,
            )
            .await?,
            playlist_groups: list_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "PlaylistGroup"
                   ORDER BY COALESCE("IsSystem", 0) DESC, "Name", "Id""#,
            )
            .await?,
            playlists: list_deco_playlists(&self.0).await?,
        })
    }

    async fn save_admin(&self, deco: &DecoAdminRecord) -> Result<()> {
        let mut transaction = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"UPDATE "Deco" SET
                   "Name" = ?, "WatermarkMode" = ?, "UseWatermarkDuringFiller" = ?,
                   "GraphicsElementsMode" = ?, "UseGraphicsElementsDuringFiller" = ?,
                   "BreakContentMode" = ?, "DefaultFillerMode" = ?,
                   "DefaultFillerCollectionType" = ?, "DefaultFillerCollectionId" = ?,
                   "DefaultFillerMediaItemId" = ?, "DefaultFillerMultiCollectionId" = ?,
                   "DefaultFillerSmartCollectionId" = ?, "DefaultFillerTrimToFit" = ?,
                   "DeadAirFallbackMode" = ?, "DeadAirFallbackCollectionType" = ?,
                   "DeadAirFallbackCollectionId" = ?, "DeadAirFallbackMediaItemId" = ?,
                   "DeadAirFallbackMultiCollectionId" = ?,
                   "DeadAirFallbackSmartCollectionId" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&deco.name)
        .bind(encode_enum(deco.watermark_mode))
        .bind(deco.use_watermark_during_filler)
        .bind(encode_enum(deco.graphics_elements_mode))
        .bind(deco.use_graphics_elements_during_filler)
        .bind(encode_enum(deco.break_content_mode))
        .bind(encode_enum(deco.default_filler_mode))
        .bind(encode_enum(deco.default_filler_collection_type))
        .bind(deco.default_filler_collection_id)
        .bind(deco.default_filler_media_item_id)
        .bind(deco.default_filler_multi_collection_id)
        .bind(deco.default_filler_smart_collection_id)
        .bind(deco.default_filler_trim_to_fit)
        .bind(encode_enum(deco.dead_air_fallback_mode))
        .bind(encode_enum(deco.dead_air_fallback_collection_type))
        .bind(deco.dead_air_fallback_collection_id)
        .bind(deco.dead_air_fallback_media_item_id)
        .bind(deco.dead_air_fallback_multi_collection_id)
        .bind(deco.dead_air_fallback_smart_collection_id)
        .bind(deco.id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "deco",
                id: deco.id,
            });
        }
        replace_ids(
            &mut transaction,
            r#"DELETE FROM "DecoWatermark" WHERE "DecoId" = ?"#,
            r#"INSERT INTO "DecoWatermark" ("DecoId", "WatermarkId") VALUES (?, ?)"#,
            deco.id,
            &deco.watermark_ids,
        )
        .await?;
        replace_ids(
            &mut transaction,
            r#"DELETE FROM "DecoGraphicsElement" WHERE "DecoId" = ?"#,
            r#"INSERT INTO "DecoGraphicsElement" ("DecoId", "GraphicsElementId") VALUES (?, ?)"#,
            deco.id,
            &deco.graphics_element_ids,
        )
        .await?;
        sqlx::query(r#"DELETE FROM "DecoBreakContent" WHERE "DecoId" = ?"#)
            .bind(deco.id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        if deco.break_content_mode == DecoMode::Override {
            for item in &deco.break_content {
                sqlx::query(
                    r#"INSERT INTO "DecoBreakContent" (
                           "DecoId", "CollectionType", "CollectionId", "MediaItemId",
                           "MultiCollectionId", "SmartCollectionId", "PlaylistId", "Placement"
                       ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
                )
                .bind(deco.id)
                .bind(encode_enum(item.collection_type))
                .bind(item.collection_id)
                .bind(item.media_item_id)
                .bind(item.multi_collection_id)
                .bind(item.smart_collection_id)
                .bind(item.playlist_id)
                .bind(encode_enum(item.placement))
                .execute(&mut *transaction)
                .await
                .repository()?;
            }
        }
        transaction.commit().await.repository()?;
        Ok(())
    }

    async fn list_template_groups_admin(&self) -> Result<Vec<DecoTemplateGroupAdminRow>> {
        let rows = sqlx::query(
            r#"SELECT dtg."Id", dtg."Name", COUNT(dt."Id") AS "TemplateCount"
               FROM "DecoTemplateGroup" dtg
               LEFT JOIN "DecoTemplate" dt ON dt."DecoTemplateGroupId" = dtg."Id"
               GROUP BY dtg."Id", dtg."Name"
               ORDER BY dtg."Name", dtg."Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(DecoTemplateGroupAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: decode_string_column(row, "Name")?,
                    template_count: row.try_get("TemplateCount").repository()?,
                })
            })
            .collect()
    }

    async fn list_templates_admin(&self, search: &str) -> Result<Vec<DecoTemplateAdminRow>> {
        let search = search.trim();
        let like = format!("%{search}%");
        let rows = sqlx::query(
            r#"SELECT dt."Id", dt."DecoTemplateGroupId", dtg."Name" AS "GroupName", dt."Name"
               FROM "DecoTemplate" dt
               JOIN "DecoTemplateGroup" dtg ON dtg."Id" = dt."DecoTemplateGroupId"
               WHERE (? = '' OR dt."Name" LIKE ?)
               UNION ALL
               SELECT -dtg."Id" AS "Id", dtg."Id" AS "DecoTemplateGroupId",
                      dtg."Name" AS "GroupName", '(none)' AS "Name"
               FROM "DecoTemplateGroup" dtg
               WHERE ? = ''
                 AND NOT EXISTS (
                     SELECT 1 FROM "DecoTemplate" dt
                     WHERE dt."DecoTemplateGroupId" = dtg."Id"
                 )
               ORDER BY "GroupName", "Name""#,
        )
        .bind(search)
        .bind(like)
        .bind(search)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter().map(map_deco_template_row).collect()
    }

    async fn template_group_exists_admin(&self, id: EntityId) -> Result<bool> {
        exists(
            &self.0,
            r#"SELECT COUNT(*) FROM "DecoTemplateGroup" WHERE "Id" = ?"#,
            id,
        )
        .await
    }

    async fn template_exists_admin(&self, id: EntityId) -> Result<bool> {
        exists(
            &self.0,
            r#"SELECT COUNT(*) FROM "DecoTemplate" WHERE "Id" = ?"#,
            id,
        )
        .await
    }

    async fn template_group_name_exists_admin(&self, name: &str) -> Result<bool> {
        name_exists(
            &self.0,
            r#"SELECT COUNT(*) FROM "DecoTemplateGroup" WHERE "Name" = ?"#,
            name,
        )
        .await
    }

    async fn template_name_exists_admin(
        &self,
        group_id: EntityId,
        template_id: Option<EntityId>,
        name: &str,
    ) -> Result<bool> {
        scoped_name_exists(
            &self.0,
            "DecoTemplate",
            "DecoTemplateGroupId",
            group_id,
            template_id,
            name,
        )
        .await
    }

    async fn create_template_group_admin(&self, name: &str) -> Result<EntityId> {
        insert_name(
            &self.0,
            r#"INSERT INTO "DecoTemplateGroup" ("Name") VALUES (?)"#,
            name,
        )
        .await
    }

    async fn create_template_admin(&self, group_id: EntityId, name: &str) -> Result<EntityId> {
        let result = sqlx::query(
            r#"INSERT INTO "DecoTemplate" ("DecoTemplateGroupId", "Name", "DateUpdated")
               VALUES (?, ?, CURRENT_TIMESTAMP)"#,
        )
        .bind(group_id)
        .bind(name)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn delete_template_group_admin(&self, id: EntityId) -> Result<()> {
        checked_delete(
            &self.0,
            r#"DELETE FROM "DecoTemplateGroup" WHERE "Id" = ?"#,
            "deco template group",
            id,
        )
        .await
    }

    async fn delete_template_admin(&self, id: EntityId) -> Result<()> {
        checked_delete(
            &self.0,
            r#"DELETE FROM "DecoTemplate" WHERE "Id" = ?"#,
            "deco template",
            id,
        )
        .await
    }

    async fn get_template_admin(&self, id: EntityId) -> Result<Option<DecoTemplateAdminRow>> {
        sqlx::query(
            r#"SELECT dt."Id", dt."DecoTemplateGroupId", dtg."Name" AS "GroupName", dt."Name"
               FROM "DecoTemplate" dt
               JOIN "DecoTemplateGroup" dtg ON dtg."Id" = dt."DecoTemplateGroupId"
               WHERE dt."Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .as_ref()
        .map(map_deco_template_row)
        .transpose()
    }

    async fn list_template_items_admin(
        &self,
        template_id: EntityId,
    ) -> Result<Vec<DecoTemplateItemAdmin>> {
        let rows = sqlx::query(
            r#"SELECT dti."DecoId", dti."StartTime", dti."EndTime", d."Name" AS "DecoName"
               FROM "DecoTemplateItem" dti
               JOIN "Deco" d ON d."Id" = dti."DecoId"
               WHERE dti."DecoTemplateId" = ?
               ORDER BY dti."StartTime", dti."Id""#,
        )
        .bind(template_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let start_text = decode_string_column(row, "StartTime")?;
                let end_text = decode_string_column(row, "EndTime")?;
                let start = parse_minutes(&start_text).ok_or(RepositoryError::Decode {
                    kind: "deco template start time",
                    value: start_text,
                })?;
                let end = parse_minutes(&end_text).ok_or(RepositoryError::Decode {
                    kind: "deco template end time",
                    value: end_text,
                })?;
                Ok(DecoTemplateItemAdmin {
                    deco_id: row.try_get("DecoId").repository()?,
                    deco_name: decode_string_column(row, "DecoName")?,
                    start_minutes: start,
                    end_minutes: end,
                })
            })
            .collect()
    }

    async fn save_template_admin(
        &self,
        id: EntityId,
        name: &str,
        items: &[DecoTemplateItemAdmin],
    ) -> Result<()> {
        let mut transaction = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"UPDATE "DecoTemplate"
               SET "Name" = ?, "DateUpdated" = CURRENT_TIMESTAMP WHERE "Id" = ?"#,
        )
        .bind(name)
        .bind(id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "deco template",
                id,
            });
        }
        sqlx::query(r#"DELETE FROM "DecoTemplateItem" WHERE "DecoTemplateId" = ?"#)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        for item in items {
            let end = if item.end_minutes >= 24 * 60 {
                0
            } else {
                item.end_minutes
            };
            sqlx::query(
                r#"INSERT INTO "DecoTemplateItem"
                   ("DecoTemplateId", "DecoId", "StartTime", "EndTime")
                   VALUES (?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(item.deco_id)
            .bind(format_minutes(item.start_minutes))
            .bind(format_minutes(end))
            .execute(&mut *transaction)
            .await
            .repository()?;
        }
        transaction.commit().await.repository()?;
        Ok(())
    }
}

const DECO_ADMIN_SQL: &str = r#"
    SELECT d."Id", d."DecoGroupId", dg."Name" AS "GroupName", d."Name",
           d."WatermarkMode", d."UseWatermarkDuringFiller",
           d."GraphicsElementsMode", d."UseGraphicsElementsDuringFiller",
           d."BreakContentMode", d."DefaultFillerMode",
           d."DefaultFillerCollectionType", d."DefaultFillerCollectionId",
           d."DefaultFillerMediaItemId", d."DefaultFillerMultiCollectionId",
           d."DefaultFillerSmartCollectionId", d."DefaultFillerTrimToFit",
           d."DeadAirFallbackMode", d."DeadAirFallbackCollectionType",
           d."DeadAirFallbackCollectionId", d."DeadAirFallbackMediaItemId",
           d."DeadAirFallbackMultiCollectionId", d."DeadAirFallbackSmartCollectionId"
    FROM "Deco" d
    JOIN "DecoGroup" dg ON dg."Id" = d."DecoGroupId"
    WHERE d."Id" = ?
"#;

fn map_deco_admin_row(row: &DbRow) -> Result<DecoAdminRow> {
    Ok(DecoAdminRow {
        id: row.try_get("Id").repository()?,
        group_id: row.try_get("DecoGroupId").repository()?,
        group_name: decode_string_column(row, "GroupName")?,
        name: decode_string_column(row, "Name")?,
    })
}

async fn map_deco_admin_record(pool: &Pool<crate::db::Db>, row: &DbRow) -> Result<DecoAdminRecord> {
    let id: EntityId = row.try_get("Id").repository()?;
    Ok(DecoAdminRecord {
        id,
        group_id: row.try_get("DecoGroupId").repository()?,
        group_name: decode_string_column(row, "GroupName")?,
        name: decode_string_column(row, "Name")?,
        watermark_mode: decode_enum_column("deco watermark mode", row, "WatermarkMode")?,
        watermark_ids: load_ids(
            pool,
            r#"SELECT "WatermarkId" FROM "DecoWatermark"
               WHERE "DecoId" = ? ORDER BY "WatermarkId""#,
            id,
        )
        .await?,
        use_watermark_during_filler: decode_bool_column(row, "UseWatermarkDuringFiller")?,
        graphics_elements_mode: decode_enum_column(
            "deco graphics elements mode",
            row,
            "GraphicsElementsMode",
        )?,
        graphics_element_ids: load_ids(
            pool,
            r#"SELECT "GraphicsElementId" FROM "DecoGraphicsElement"
               WHERE "DecoId" = ? ORDER BY "GraphicsElementId""#,
            id,
        )
        .await?,
        use_graphics_elements_during_filler: decode_bool_column(
            row,
            "UseGraphicsElementsDuringFiller",
        )?,
        break_content_mode: decode_enum_column("deco break content mode", row, "BreakContentMode")?,
        break_content: load_break_content(pool, id).await?,
        default_filler_mode: decode_enum_column(
            "deco default filler mode",
            row,
            "DefaultFillerMode",
        )?,
        default_filler_collection_type: decode_enum_column(
            "deco default filler collection type",
            row,
            "DefaultFillerCollectionType",
        )?,
        default_filler_collection_id: row.try_get("DefaultFillerCollectionId").repository()?,
        default_filler_media_item_id: row.try_get("DefaultFillerMediaItemId").repository()?,
        default_filler_multi_collection_id: row
            .try_get("DefaultFillerMultiCollectionId")
            .repository()?,
        default_filler_smart_collection_id: row
            .try_get("DefaultFillerSmartCollectionId")
            .repository()?,
        default_filler_trim_to_fit: decode_bool_column(row, "DefaultFillerTrimToFit")?,
        dead_air_fallback_mode: decode_enum_column(
            "deco dead air fallback mode",
            row,
            "DeadAirFallbackMode",
        )?,
        dead_air_fallback_collection_type: decode_enum_column(
            "deco dead air fallback collection type",
            row,
            "DeadAirFallbackCollectionType",
        )?,
        dead_air_fallback_collection_id: row.try_get("DeadAirFallbackCollectionId").repository()?,
        dead_air_fallback_media_item_id: row.try_get("DeadAirFallbackMediaItemId").repository()?,
        dead_air_fallback_multi_collection_id: row
            .try_get("DeadAirFallbackMultiCollectionId")
            .repository()?,
        dead_air_fallback_smart_collection_id: row
            .try_get("DeadAirFallbackSmartCollectionId")
            .repository()?,
    })
}

async fn load_break_content(
    pool: &Pool<crate::db::Db>,
    deco_id: EntityId,
) -> Result<Vec<DecoBreakContentAdmin>> {
    let rows = sqlx::query(
        r#"SELECT dbc."Id", dbc."CollectionType", dbc."CollectionId", dbc."MediaItemId",
                  dbc."MultiCollectionId", dbc."SmartCollectionId", dbc."PlaylistId",
                  dbc."Placement", c."Name" AS "CollectionName",
                  mc."Name" AS "MultiCollectionName", sc."Name" AS "SmartCollectionName",
                  p."Name" AS "PlaylistName", shm."Title" AS "ShowTitle",
                  sem."Title" AS "SeasonTitle", s."SeasonNumber",
                  am."Title" AS "ArtistTitle"
           FROM "DecoBreakContent" dbc
           LEFT JOIN "Collection" c ON c."Id" = dbc."CollectionId"
           LEFT JOIN "MultiCollection" mc ON mc."Id" = dbc."MultiCollectionId"
           LEFT JOIN "SmartCollection" sc ON sc."Id" = dbc."SmartCollectionId"
           LEFT JOIN "Playlist" p ON p."Id" = dbc."PlaylistId"
           LEFT JOIN "Show" sh ON sh."Id" = dbc."MediaItemId"
           LEFT JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           LEFT JOIN "Season" s ON s."Id" = dbc."MediaItemId"
           LEFT JOIN "SeasonMetadata" sem ON sem."SeasonId" = s."Id"
           LEFT JOIN "Artist" a ON a."Id" = dbc."MediaItemId"
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           WHERE dbc."DecoId" = ? ORDER BY dbc."Placement", dbc."Id""#,
    )
    .bind(deco_id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            let collection_type: CollectionType =
                decode_enum_column("deco break collection type", row, "CollectionType")?;
            let collection_name = match collection_type {
                CollectionType::Collection => decode_optional_string_column(row, "CollectionName")?,
                CollectionType::MultiCollection => {
                    decode_optional_string_column(row, "MultiCollectionName")?
                }
                CollectionType::SmartCollection => {
                    decode_optional_string_column(row, "SmartCollectionName")?
                }
                CollectionType::Playlist => decode_optional_string_column(row, "PlaylistName")?,
                CollectionType::TelevisionShow => decode_optional_string_column(row, "ShowTitle")?,
                CollectionType::TelevisionSeason => {
                    let title =
                        decode_optional_string_column(row, "SeasonTitle")?.unwrap_or_default();
                    let season: Option<i64> = row.try_get("SeasonNumber").repository()?;
                    Some(format!("{title} S{:02}", season.unwrap_or_default()))
                }
                CollectionType::Artist => decode_optional_string_column(row, "ArtistTitle")?,
                _ => None,
            }
            .unwrap_or_default();
            Ok(DecoBreakContentAdmin {
                id: row.try_get("Id").repository()?,
                collection_type,
                collection_id: row.try_get("CollectionId").repository()?,
                media_item_id: row.try_get("MediaItemId").repository()?,
                multi_collection_id: row.try_get("MultiCollectionId").repository()?,
                smart_collection_id: row.try_get("SmartCollectionId").repository()?,
                playlist_id: row.try_get("PlaylistId").repository()?,
                collection_name,
                placement: decode_enum_column("deco break placement", row, "Placement")?,
            })
        })
        .collect()
}

fn map_deco_template_row(row: &DbRow) -> Result<DecoTemplateAdminRow> {
    Ok(DecoTemplateAdminRow {
        id: row.try_get("Id").repository()?,
        group_id: row.try_get("DecoTemplateGroupId").repository()?,
        group_name: decode_string_column(row, "GroupName")?,
        name: decode_string_column(row, "Name")?,
    })
}

async fn exists(pool: &Pool<crate::db::Db>, query: &str, id: EntityId) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(query))
        .bind(id)
        .fetch_one(pool)
        .await
        .repository()?;
    Ok(count > 0)
}

async fn name_exists(pool: &Pool<crate::db::Db>, query: &str, name: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(query))
        .bind(name)
        .fetch_one(pool)
        .await
        .repository()?;
    Ok(count > 0)
}

async fn scoped_name_exists(
    pool: &Pool<crate::db::Db>,
    table: &str,
    group_column: &str,
    group_id: EntityId,
    entity_id: Option<EntityId>,
    name: &str,
) -> Result<bool> {
    let query = if entity_id.is_some() {
        format!(
            r#"SELECT COUNT(*) FROM "{table}"
               WHERE "{group_column}" = ? AND "Name" = ? AND "Id" <> ?"#
        )
    } else {
        format!(
            r#"SELECT COUNT(*) FROM "{table}"
               WHERE "{group_column}" = ? AND "Name" = ?"#
        )
    };
    let mut query = sqlx::query_scalar(crate::db::assert_sql_safe(&query))
        .bind(group_id)
        .bind(name);
    if let Some(id) = entity_id {
        query = query.bind(id);
    }
    let count: i64 = query.fetch_one(pool).await.repository()?;
    Ok(count > 0)
}

async fn insert_name(pool: &Pool<crate::db::Db>, query: &str, name: &str) -> Result<EntityId> {
    let result = sqlx::query(crate::db::assert_sql_safe(query))
        .bind(name)
        .execute(pool)
        .await
        .repository()?;
    Ok(crate::db::last_insert_id(result))
}

async fn checked_delete(
    pool: &Pool<crate::db::Db>,
    query: &str,
    kind: &'static str,
    id: EntityId,
) -> Result<()> {
    let result = sqlx::query(crate::db::assert_sql_safe(query))
        .bind(id)
        .execute(pool)
        .await
        .repository()?;
    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound { kind, id });
    }
    Ok(())
}

async fn list_options(pool: &Pool<crate::db::Db>, query: &str) -> Result<Vec<DecoOptionAdmin>> {
    let rows = sqlx::query(crate::db::assert_sql_safe(query))
        .fetch_all(pool)
        .await
        .repository()?;
    rows.iter()
        .map(|row| {
            Ok(DecoOptionAdmin {
                id: row.try_get("Id").repository()?,
                name: decode_string_column(row, "Name")?,
            })
        })
        .collect()
}

async fn list_deco_seasons(pool: &Pool<crate::db::Db>) -> Result<Vec<DecoOptionAdmin>> {
    let rows = sqlx::query(
        r#"SELECT s."Id", sm."Title", s."SeasonNumber"
           FROM "Season" s
           LEFT JOIN "Show" sh ON sh."Id" = s."ShowId"
           LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
           ORDER BY sm."Title", s."SeasonNumber", s."Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            let season: i64 = row.try_get("SeasonNumber").repository()?;
            let title = decode_optional_string_column(row, "Title")?
                .unwrap_or_else(|| format!("Season {season}"));
            Ok(DecoOptionAdmin {
                id: row.try_get("Id").repository()?,
                name: format!("{title} S{season:02}"),
            })
        })
        .collect()
}

async fn list_deco_playlists(pool: &Pool<crate::db::Db>) -> Result<Vec<DecoPlaylistOptionAdmin>> {
    let rows = sqlx::query(
        r#"SELECT "Id", "PlaylistGroupId", "Name" FROM "Playlist" ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(DecoPlaylistOptionAdmin {
                id: row.try_get("Id").repository()?,
                group_id: row.try_get("PlaylistGroupId").repository()?,
                name: decode_string_column(row, "Name")?,
            })
        })
        .collect()
}

fn parse_minutes(value: &str) -> Option<i32> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<i32>().ok()?;
    let minutes = parts.next()?.parse::<i32>().ok()?;
    if !(0..=24).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    let total = hours * 60 + minutes;
    (total <= 24 * 60).then_some(total)
}

fn format_minutes(minutes: i32) -> String {
    let minutes = minutes.clamp(0, 24 * 60);
    format!("{:02}:{:02}:00", minutes / 60, minutes % 60)
}

fn map_deco(row: &DbRow) -> Result<Deco> {
    Ok(Deco {
        id: row.try_get("Id").repository()?,
        deco_group_id: row.try_get("DecoGroupId").repository()?,
        name: row.try_get("Name").repository()?,
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
        default_filler_collection_id: row.try_get("DefaultFillerCollectionId").repository()?,
        default_filler_media_item_id: row.try_get("DefaultFillerMediaItemId").repository()?,
        default_filler_multi_collection_id: row
            .try_get("DefaultFillerMultiCollectionId")
            .repository()?,
        default_filler_smart_collection_id: row
            .try_get("DefaultFillerSmartCollectionId")
            .repository()?,
        default_filler_trim_to_fit: row.try_get("DefaultFillerTrimToFit").repository()?,
        watermark_ids: Vec::new(),
        graphics_element_ids: Vec::new(),
    })
}
