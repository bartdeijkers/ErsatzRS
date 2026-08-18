use crate::repositories::SqlxResultExt as _;
use std::collections::BTreeSet;

use crate::db::{Db, DbRow};
use async_trait::async_trait;
use ersatzrs_core::repositories::{
    BlockAdminRow, BlockEditorOptionAdmin, BlockEditorOptionsAdmin, BlockGroupAdminRow,
    BlockItemAdmin, BlockRepository, RepositoryError, Result,
};
use ersatzrs_core::scheduling::{
    block_item_smart_shuffle_config_key, SchedulingSmartShufflePolicy,
};
use ersatzrs_domain::{Block, BlockItem, CollectionType, EntityId};
use sqlx::{Pool, Row, Transaction};

use super::common::{
    decode_bool_column, decode_enum_column, decode_optional_string_column, decode_string_column,
    encode_enum, load_ids, not_found, replace_ids,
};

#[derive(Clone)]
pub struct SqliteBlockRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl BlockRepository for SqliteBlockRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Block> {
        let row = sqlx::query(r#"SELECT * FROM "Block" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("block", id, error))?;
        let mut block = map_block(&row)?;
        block.item_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = ? ORDER BY "Index", "Id""#,
            id,
        )
        .await?;
        Ok(block)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Block>> {
        let rows =
            sqlx::query(r#"SELECT * FROM "Block" WHERE "BlockGroupId" = ? ORDER BY "Name", "Id""#)
                .bind(group_id)
                .fetch_all(&self.0)
                .await
                .repository()?;
        rows.iter()
            .map(|row| {
                let mut block = map_block(row)?;
                block.item_ids.clear();
                Ok(block)
            })
            .collect()
    }

    async fn upsert(&self, block: &Block) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"UPDATE "Block"
               SET "BlockGroupId" = ?,
                   "Name" = ?,
                   "DateUpdated" = ?,
                   "Minutes" = ?,
                   "StopScheduling" = ?
               WHERE "Id" = ?"#,
        )
        .bind(block.block_group_id)
        .bind(&block.name)
        .bind(block.updated_ticks.to_string())
        .bind(block.duration.num_minutes().max(0))
        .bind(encode_enum(block.stop_scheduling))
        .bind(block.id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            sqlx::query(
                r#"INSERT INTO "Block" (
                    "Id", "BlockGroupId", "Name", "DateUpdated", "Minutes", "StopScheduling"
                 ) VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(block.id)
            .bind(block.block_group_id)
            .bind(&block.name)
            .bind(block.updated_ticks.to_string())
            .bind(block.duration.num_minutes().max(0))
            .bind(encode_enum(block.stop_scheduling))
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        sync_block_item_ids(&mut tx, block.id, &block.item_ids).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn get_item(&self, id: EntityId) -> Result<BlockItem> {
        let row = sqlx::query(r#"SELECT * FROM "BlockItem" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("block item", id, error))?;
        map_block_item(&self.0, &row).await
    }

    async fn list_items(&self, block_id: EntityId) -> Result<Vec<BlockItem>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "BlockItem"
             WHERE "BlockId" = ?
             ORDER BY "Index", "Id""#,
        )
        .bind(block_id)
        .fetch_all(&self.0)
        .await
        .repository()?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(map_block_item(&self.0, &row).await?);
        }
        Ok(items)
    }

    async fn upsert_item(&self, item: &BlockItem) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        upsert_block_item(&mut tx, item).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn delete_item(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "BlockItem" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        Ok(())
    }

    async fn list_groups_admin(&self) -> Result<Vec<BlockGroupAdminRow>> {
        let rows = sqlx::query(r#"SELECT "Id", "Name" FROM "BlockGroup" ORDER BY "Name", "Id""#)
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter()
            .map(|row| {
                Ok(BlockGroupAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: decode_string_column(row, "Name")?,
                })
            })
            .collect()
    }

    async fn list_admin(&self, search: &str) -> Result<Vec<BlockAdminRow>> {
        let search = search.trim();
        let like = format!("%{search}%");
        let rows = sqlx::query(
            r#"SELECT b."Id", b."BlockGroupId", bg."Name" AS "GroupName", b."Name",
                      b."Minutes", b."StopScheduling"
               FROM "Block" b
               JOIN "BlockGroup" bg ON bg."Id" = b."BlockGroupId"
               WHERE (? = '' OR b."Name" LIKE ?)
               UNION ALL
               SELECT -1 AS "Id", bg."Id" AS "BlockGroupId", bg."Name" AS "GroupName",
                      '' AS "Name", 0 AS "Minutes", 0 AS "StopScheduling"
               FROM "BlockGroup" bg
               WHERE NOT EXISTS (SELECT 1 FROM "Block" b WHERE b."BlockGroupId" = bg."Id")
                 AND ? = ''
               ORDER BY "GroupName", "Name""#,
        )
        .bind(search)
        .bind(like)
        .bind(search)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter().map(map_block_admin).collect()
    }

    async fn group_exists_admin(&self, id: EntityId) -> Result<bool> {
        entity_exists(
            &self.0,
            r#"SELECT COUNT(*) FROM "BlockGroup" WHERE "Id" = ?"#,
            id,
        )
        .await
    }

    async fn block_exists_admin(&self, id: EntityId) -> Result<bool> {
        entity_exists(
            &self.0,
            r#"SELECT COUNT(*) FROM "Block" WHERE "Id" = ?"#,
            id,
        )
        .await
    }

    async fn group_name_exists_admin(&self, name: &str) -> Result<bool> {
        let count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "BlockGroup" WHERE "Name" = ?"#)
                .bind(name)
                .fetch_one(&self.0)
                .await
                .repository()?;
        Ok(count > 0)
    }

    async fn block_name_exists_admin(
        &self,
        group_id: EntityId,
        block_id: Option<EntityId>,
        name: &str,
    ) -> Result<bool> {
        let count: i64 = match block_id {
            Some(id) => sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Block"
                   WHERE "BlockGroupId" = ? AND "Name" = ? AND "Id" <> ?"#,
            )
            .bind(group_id)
            .bind(name)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .repository()?,
            None => sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Block"
                   WHERE "BlockGroupId" = ? AND "Name" = ?"#,
            )
            .bind(group_id)
            .bind(name)
            .fetch_one(&self.0)
            .await
            .repository()?,
        };
        Ok(count > 0)
    }

    async fn create_group_admin(&self, name: &str) -> Result<EntityId> {
        let result = sqlx::query(r#"INSERT INTO "BlockGroup" ("Name") VALUES (?)"#)
            .bind(name)
            .execute(&self.0)
            .await
            .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn delete_group_admin(&self, id: EntityId) -> Result<()> {
        checked_delete(
            &self.0,
            r#"DELETE FROM "BlockGroup" WHERE "Id" = ?"#,
            "block group",
            id,
        )
        .await
    }

    async fn create_admin(&self, group_id: EntityId, name: &str) -> Result<EntityId> {
        let result = sqlx::query(
            r#"INSERT INTO "Block"
               ("BlockGroupId", "Name", "Minutes", "StopScheduling", "DateUpdated")
               VALUES (?, ?, 30, 0, CURRENT_TIMESTAMP)"#,
        )
        .bind(group_id)
        .bind(name)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn copy_admin(
        &self,
        block_id: EntityId,
        new_group_id: EntityId,
        name: &str,
    ) -> Result<EntityId> {
        let mut transaction = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "Block"
               ("BlockGroupId", "Name", "Minutes", "StopScheduling", "DateUpdated")
               SELECT ?, ?, "Minutes", "StopScheduling", CURRENT_TIMESTAMP
               FROM "Block" WHERE "Id" = ?"#,
        )
        .bind(new_group_id)
        .bind(name)
        .bind(block_id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "block",
                id: block_id,
            });
        }
        let new_block_id = crate::db::last_insert_id(result);
        copy_admin_items(&mut transaction, block_id, new_block_id).await?;
        transaction.commit().await.repository()?;
        Ok(new_block_id)
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        let mut transaction = crate::db::begin_write(&self.0).await.repository()?;
        let item_ids = load_ids_in_tx(
            &mut transaction,
            r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = ?"#,
            id,
        )
        .await?;
        for item_id in item_ids {
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(block_item_smart_shuffle_config_key(item_id))
                .execute(&mut *transaction)
                .await
                .repository()?;
        }
        let result = sqlx::query(r#"DELETE FROM "Block" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound { kind: "block", id });
        }
        transaction.commit().await.repository()?;
        Ok(())
    }

    async fn get_admin(&self, id: EntityId) -> Result<Option<BlockAdminRow>> {
        sqlx::query(
            r#"SELECT b."Id", b."BlockGroupId", bg."Name" AS "GroupName", b."Name",
                      b."Minutes", b."StopScheduling"
               FROM "Block" b
               JOIN "BlockGroup" bg ON bg."Id" = b."BlockGroupId"
               WHERE b."Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .as_ref()
        .map(map_block_admin)
        .transpose()
    }

    async fn list_items_admin(&self, block_id: EntityId) -> Result<Vec<BlockItemAdmin>> {
        let rows = sqlx::query(BLOCK_ITEM_ADMIN_SQL)
            .bind(block_id)
            .fetch_all(&self.0)
            .await
            .repository()?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(map_block_item_admin(&self.0, &row).await?);
        }
        Ok(items)
    }

    async fn save_admin(&self, block: &BlockAdminRow, items: &[BlockItemAdmin]) -> Result<()> {
        let mut transaction = crate::db::begin_write(&self.0).await.repository()?;
        let old_item_ids = load_ids_in_tx(
            &mut transaction,
            r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = ?"#,
            block.id,
        )
        .await?;
        for item_id in old_item_ids {
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(block_item_smart_shuffle_config_key(item_id))
                .execute(&mut *transaction)
                .await
                .repository()?;
        }
        let result = sqlx::query(
            r#"UPDATE "Block"
               SET "Name" = ?, "Minutes" = ?, "StopScheduling" = ?,
                   "DateUpdated" = CURRENT_TIMESTAMP
               WHERE "Id" = ?"#,
        )
        .bind(&block.name)
        .bind(block.minutes)
        .bind(encode_enum(block.stop_scheduling))
        .bind(block.id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "block",
                id: block.id,
            });
        }
        sqlx::query(r#"DELETE FROM "BlockItem" WHERE "BlockId" = ?"#)
            .bind(block.id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        for item in items {
            insert_admin_item(&mut transaction, block.id, item).await?;
        }
        transaction.commit().await.repository()?;
        Ok(())
    }

    async fn search_collections_admin(&self, prefix: &str) -> Result<Vec<BlockEditorOptionAdmin>> {
        search_named_options(
            &self.0,
            r#"SELECT "Id", "Name" FROM "Collection"
               WHERE "Name" LIKE ? ORDER BY "Name", "Id" LIMIT 10"#,
            prefix,
        )
        .await
    }

    async fn search_smart_collections_admin(
        &self,
        prefix: &str,
    ) -> Result<Vec<BlockEditorOptionAdmin>> {
        search_named_options(
            &self.0,
            r#"SELECT "Id", "Name" FROM "SmartCollection"
               WHERE "Name" LIKE ? ORDER BY "Name", "Id" LIMIT 10"#,
            prefix,
        )
        .await
    }

    async fn search_multi_collections_admin(
        &self,
        prefix: &str,
    ) -> Result<Vec<BlockEditorOptionAdmin>> {
        search_named_options(
            &self.0,
            r#"SELECT "Id", "Name" FROM "MultiCollection"
               WHERE "Name" LIKE ? ORDER BY "Name", "Id" LIMIT 10"#,
            prefix,
        )
        .await
    }

    async fn search_shows_admin(&self, prefix: &str) -> Result<Vec<BlockEditorOptionAdmin>> {
        search_named_options(
            &self.0,
            r#"SELECT s."Id", sm."Title" AS "Name"
               FROM "Show" s
               JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
               WHERE sm."Title" LIKE ? ORDER BY sm."Title", s."Id" LIMIT 10"#,
            prefix,
        )
        .await
    }

    async fn search_seasons_admin(&self, prefix: &str) -> Result<Vec<BlockEditorOptionAdmin>> {
        let like = format!("{}%", prefix.trim());
        let rows = sqlx::query(
            r#"SELECT se."Id", sm."Title", se."SeasonNumber"
               FROM "Season" se
               JOIN "Show" sh ON sh."Id" = se."ShowId"
               JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
               WHERE sm."Title" LIKE ?
               ORDER BY sm."Title", se."SeasonNumber", se."Id" LIMIT 20"#,
        )
        .bind(like)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let title = decode_string_column(row, "Title")?;
                let season: i64 = row.try_get("SeasonNumber").repository()?;
                Ok(BlockEditorOptionAdmin {
                    id: row.try_get("Id").repository()?,
                    name: format!("{title} S{season:02}"),
                })
            })
            .collect()
    }

    async fn editor_options_admin(&self) -> Result<BlockEditorOptionsAdmin> {
        Ok(BlockEditorOptionsAdmin {
            collections: list_named_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "Collection" ORDER BY "Name", "Id""#,
            )
            .await?,
            shows: list_named_options(
                &self.0,
                r#"SELECT sh."Id", sm."Title" AS "Name"
                   FROM "Show" sh
                   LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
                   ORDER BY sm."Title", sh."Id""#,
            )
            .await?,
            seasons: list_season_options(&self.0).await?,
            multi_collections: list_named_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "MultiCollection" ORDER BY "Name", "Id""#,
            )
            .await?,
            smart_collections: list_named_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "SmartCollection" ORDER BY "Name", "Id""#,
            )
            .await?,
            watermarks: list_named_options(
                &self.0,
                r#"SELECT "Id", "Name" FROM "ChannelWatermark" ORDER BY "Name", "Id""#,
            )
            .await?,
            graphics_elements: list_named_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", "Path", '') AS "Name"
                   FROM "GraphicsElement" ORDER BY "Name", "Id""#,
            )
            .await?,
        })
    }
}

const BLOCK_ITEM_ADMIN_SQL: &str = r#"
    SELECT bi."Id", bi."Index", bi."CollectionType", bi."CollectionId",
           bi."MultiCollectionId", bi."SmartCollectionId", bi."MediaItemId",
           bi."SearchTitle", bi."SearchQuery", bi."PlaybackOrder",
           bi."IncludeInProgramGuide", bi."DisableWatermarks", s."SeasonNumber",
           CASE bi."CollectionType"
               WHEN 0 THEN COALESCE(c."Name", '')
               WHEN 1 THEN COALESCE(sm."Title", '')
               WHEN 2 THEN COALESCE(sh."Title", '')
               WHEN 3 THEN COALESCE(am."Title", '')
               WHEN 4 THEN COALESCE(mc."Name", '')
               WHEN 5 THEN COALESCE(sc."Name", '')
               WHEN 9 THEN COALESCE(bi."SearchTitle", bi."SearchQuery", '')
               WHEN 10 THEN COALESCE(mm."Title", '')
               WHEN 20 THEN COALESCE(em."Title", '')
               WHEN 30 THEN COALESCE(mvm."Title", '')
               WHEN 40 THEN COALESCE(ovm."Title", '')
               WHEN 50 THEN COALESCE(som."Title", '')
               WHEN 60 THEN COALESCE(im."Title", '')
               WHEN 70 THEN COALESCE(rsm."Title", '')
               ELSE ''
           END AS "CollectionName"
    FROM "BlockItem" bi
    LEFT JOIN "Collection" c ON c."Id" = bi."CollectionId"
    LEFT JOIN "MultiCollection" mc ON mc."Id" = bi."MultiCollectionId"
    LEFT JOIN "SmartCollection" sc ON sc."Id" = bi."SmartCollectionId"
    LEFT JOIN "Season" s ON s."Id" = bi."MediaItemId" AND bi."CollectionType" = 2
    LEFT JOIN "Show" sh_row ON sh_row."Id" = s."ShowId"
    LEFT JOIN "ShowMetadata" sh ON sh."ShowId" = sh_row."Id"
    LEFT JOIN "Show" show_tv
        ON show_tv."Id" = bi."MediaItemId" AND bi."CollectionType" = 1
    LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = show_tv."Id"
    LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = bi."MediaItemId"
    LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = bi."MediaItemId"
    LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = bi."MediaItemId"
    LEFT JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = bi."MediaItemId"
    LEFT JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = bi."MediaItemId"
    LEFT JOIN "SongMetadata" som ON som."SongId" = bi."MediaItemId"
    LEFT JOIN "ImageMetadata" im ON im."ImageId" = bi."MediaItemId"
    LEFT JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = bi."MediaItemId"
    WHERE bi."BlockId" = ?
    ORDER BY bi."Index", bi."Id"
"#;

fn map_block_admin(row: &DbRow) -> Result<BlockAdminRow> {
    Ok(BlockAdminRow {
        id: row.try_get("Id").repository()?,
        group_id: row.try_get("BlockGroupId").repository()?,
        group_name: decode_string_column(row, "GroupName")?,
        name: decode_string_column(row, "Name")?,
        minutes: row.try_get("Minutes").repository()?,
        stop_scheduling: decode_enum_column("block stop_scheduling", row, "StopScheduling")?,
    })
}

async fn map_block_item_admin(pool: &Pool<Db>, row: &DbRow) -> Result<BlockItemAdmin> {
    let id: EntityId = row.try_get("Id").repository()?;
    let collection_type: CollectionType =
        decode_enum_column("block item collection_type", row, "CollectionType")?;
    let mut collection_name = decode_string_column(row, "CollectionName")?;
    if collection_type == CollectionType::TelevisionSeason {
        let season_number: Option<i64> = row.try_get("SeasonNumber").repository()?;
        collection_name = format!(
            "{collection_name} S{:02}",
            season_number.unwrap_or_default()
        );
    }
    let smart_shuffle =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(block_item_smart_shuffle_config_key(id))
            .fetch_optional(pool)
            .await
            .repository()?
            .map(|json| {
                serde_json::from_str(&json).map_err(|error| RepositoryError::Decode {
                    kind: "block item smart shuffle",
                    value: error.to_string(),
                })
            })
            .transpose()?;
    Ok(BlockItemAdmin {
        id,
        index: row.try_get("Index").repository()?,
        collection_type,
        collection_id: row.try_get("CollectionId").repository()?,
        multi_collection_id: row.try_get("MultiCollectionId").repository()?,
        smart_collection_id: row.try_get("SmartCollectionId").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        search_title: decode_optional_string_column(row, "SearchTitle")?,
        search_query: decode_optional_string_column(row, "SearchQuery")?,
        playback_order: decode_enum_column("block item playback_order", row, "PlaybackOrder")?,
        smart_shuffle,
        include_in_program_guide: decode_bool_column(row, "IncludeInProgramGuide")?,
        disable_watermarks: decode_bool_column(row, "DisableWatermarks")?,
        collection_name,
        watermark_ids: load_ids(
            pool,
            r#"SELECT "WatermarkId" FROM "BlockItemWatermark"
               WHERE "BlockItemId" = ? ORDER BY "WatermarkId""#,
            id,
        )
        .await?,
        graphics_element_ids: load_ids(
            pool,
            r#"SELECT "GraphicsElementId" FROM "BlockItemGraphicsElement"
               WHERE "BlockItemId" = ? ORDER BY "GraphicsElementId""#,
            id,
        )
        .await?,
    })
}

async fn copy_admin_items(
    transaction: &mut Transaction<'_, Db>,
    old_block_id: EntityId,
    new_block_id: EntityId,
) -> Result<()> {
    let rows = sqlx::query(
        r#"SELECT "Id", "Index", "CollectionType", "CollectionId", "MultiCollectionId",
                  "SmartCollectionId", "MediaItemId", "SearchTitle", "SearchQuery",
                  "PlaybackOrder", "IncludeInProgramGuide", "DisableWatermarks"
           FROM "BlockItem" WHERE "BlockId" = ? ORDER BY "Index", "Id""#,
    )
    .bind(old_block_id)
    .fetch_all(&mut **transaction)
    .await
    .repository()?;
    for row in rows {
        let old_item_id: EntityId = row.try_get("Id").repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "BlockItem"
               ("BlockId", "Index", "CollectionType", "CollectionId", "MultiCollectionId",
                "SmartCollectionId", "MediaItemId", "SearchTitle", "SearchQuery",
                "PlaybackOrder", "IncludeInProgramGuide", "DisableWatermarks")
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(new_block_id)
        .bind(row.try_get::<i32, _>("Index").repository()?)
        .bind(row.try_get::<i32, _>("CollectionType").repository()?)
        .bind(
            row.try_get::<Option<EntityId>, _>("CollectionId")
                .repository()?,
        )
        .bind(
            row.try_get::<Option<EntityId>, _>("MultiCollectionId")
                .repository()?,
        )
        .bind(
            row.try_get::<Option<EntityId>, _>("SmartCollectionId")
                .repository()?,
        )
        .bind(
            row.try_get::<Option<EntityId>, _>("MediaItemId")
                .repository()?,
        )
        .bind(
            row.try_get::<Option<String>, _>("SearchTitle")
                .repository()?,
        )
        .bind(
            row.try_get::<Option<String>, _>("SearchQuery")
                .repository()?,
        )
        .bind(row.try_get::<i32, _>("PlaybackOrder").repository()?)
        .bind(
            row.try_get::<bool, _>("IncludeInProgramGuide")
                .repository()?,
        )
        .bind(row.try_get::<bool, _>("DisableWatermarks").repository()?)
        .execute(&mut **transaction)
        .await
        .repository()?;
        let new_item_id = crate::db::last_insert_id(result);
        copy_item_ids(
            transaction,
            r#""BlockItemWatermark""#,
            r#""WatermarkId""#,
            old_item_id,
            new_item_id,
        )
        .await?;
        copy_item_ids(
            transaction,
            r#""BlockItemGraphicsElement""#,
            r#""GraphicsElementId""#,
            old_item_id,
            new_item_id,
        )
        .await?;
        let json = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(block_item_smart_shuffle_config_key(old_item_id))
        .fetch_optional(&mut **transaction)
        .await
        .repository()?;
        if let Some(json) = json {
            let policy: SchedulingSmartShufflePolicy =
                serde_json::from_str(&json).map_err(|error| RepositoryError::Decode {
                    kind: "block item smart shuffle",
                    value: error.to_string(),
                })?;
            persist_smart_shuffle(transaction, new_item_id, &policy).await?;
        }
    }
    Ok(())
}

async fn insert_admin_item(
    transaction: &mut Transaction<'_, Db>,
    block_id: EntityId,
    item: &BlockItemAdmin,
) -> Result<EntityId> {
    let result = sqlx::query(
        r#"INSERT INTO "BlockItem"
           ("BlockId", "Index", "CollectionType", "CollectionId", "MultiCollectionId",
            "SmartCollectionId", "MediaItemId", "SearchTitle", "SearchQuery",
            "PlaybackOrder", "IncludeInProgramGuide", "DisableWatermarks")
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(block_id)
    .bind(item.index)
    .bind(encode_enum(item.collection_type))
    .bind(item.collection_id)
    .bind(item.multi_collection_id)
    .bind(item.smart_collection_id)
    .bind(item.media_item_id)
    .bind(&item.search_title)
    .bind(&item.search_query)
    .bind(encode_enum(item.playback_order))
    .bind(item.include_in_program_guide)
    .bind(item.disable_watermarks)
    .execute(&mut **transaction)
    .await
    .repository()?;
    let id = crate::db::last_insert_id(result);
    for watermark_id in &item.watermark_ids {
        sqlx::query(
            r#"INSERT INTO "BlockItemWatermark" ("BlockItemId", "WatermarkId") VALUES (?, ?)"#,
        )
        .bind(id)
        .bind(watermark_id)
        .execute(&mut **transaction)
        .await
        .repository()?;
    }
    for graphics_id in &item.graphics_element_ids {
        sqlx::query(
            r#"INSERT INTO "BlockItemGraphicsElement"
               ("BlockItemId", "GraphicsElementId") VALUES (?, ?)"#,
        )
        .bind(id)
        .bind(graphics_id)
        .execute(&mut **transaction)
        .await
        .repository()?;
    }
    if let Some(policy) = &item.smart_shuffle {
        persist_smart_shuffle(transaction, id, policy).await?;
    }
    Ok(id)
}

async fn persist_smart_shuffle(
    transaction: &mut Transaction<'_, Db>,
    item_id: EntityId,
    policy: &SchedulingSmartShufflePolicy,
) -> Result<()> {
    let json = serde_json::to_string(policy).map_err(|error| RepositoryError::Decode {
        kind: "block item smart shuffle",
        value: error.to_string(),
    })?;
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(block_item_smart_shuffle_config_key(item_id))
        .bind(json)
        .execute(&mut **transaction)
        .await
        .repository()?;
    Ok(())
}

async fn copy_item_ids(
    transaction: &mut Transaction<'_, Db>,
    table: &str,
    column: &str,
    old_item_id: EntityId,
    new_item_id: EntityId,
) -> Result<()> {
    let sql = format!(
        r#"INSERT INTO {table} ("BlockItemId", {column})
           SELECT ?, {column} FROM {table} WHERE "BlockItemId" = ?"#
    );
    sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(new_item_id)
        .bind(old_item_id)
        .execute(&mut **transaction)
        .await
        .repository()?;
    Ok(())
}

async fn entity_exists(pool: &Pool<Db>, query: &str, id: EntityId) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(query))
        .bind(id)
        .fetch_one(pool)
        .await
        .repository()?;
    Ok(count > 0)
}

async fn checked_delete(
    pool: &Pool<Db>,
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

async fn search_named_options(
    pool: &Pool<Db>,
    query: &str,
    prefix: &str,
) -> Result<Vec<BlockEditorOptionAdmin>> {
    let like = format!("{}%", prefix.trim());
    let rows = sqlx::query(crate::db::assert_sql_safe(query))
        .bind(like)
        .fetch_all(pool)
        .await
        .repository()?;
    map_named_options(&rows)
}

async fn list_named_options(pool: &Pool<Db>, query: &str) -> Result<Vec<BlockEditorOptionAdmin>> {
    let rows = sqlx::query(crate::db::assert_sql_safe(query))
        .fetch_all(pool)
        .await
        .repository()?;
    map_named_options(&rows)
}

fn map_named_options(rows: &[DbRow]) -> Result<Vec<BlockEditorOptionAdmin>> {
    rows.iter()
        .map(|row| {
            Ok(BlockEditorOptionAdmin {
                id: row.try_get("Id").repository()?,
                name: decode_string_column(row, "Name")?,
            })
        })
        .collect()
}

async fn list_season_options(pool: &Pool<Db>) -> Result<Vec<BlockEditorOptionAdmin>> {
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
            Ok(BlockEditorOptionAdmin {
                id: row.try_get("Id").repository()?,
                name: format!("{title} S{season:02}"),
            })
        })
        .collect()
}

async fn sync_block_item_ids(
    tx: &mut Transaction<'_, Db>,
    block_id: EntityId,
    item_ids: &[EntityId],
) -> Result<()> {
    let existing = load_ids_in_tx(
        tx,
        r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = ? ORDER BY "Index", "Id""#,
        block_id,
    )
    .await?;
    let desired = item_ids.iter().copied().collect::<BTreeSet<_>>();

    for id in existing.iter().copied().filter(|id| !desired.contains(id)) {
        sqlx::query(r#"DELETE FROM "BlockItem" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }

    let existing = existing.into_iter().collect::<BTreeSet<_>>();
    for (index, id) in item_ids.iter().copied().enumerate() {
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        if existing.contains(&id) {
            sqlx::query(
                r#"UPDATE "BlockItem"
                   SET "BlockId" = ?, "Index" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(block_id)
            .bind(index)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        } else {
            let item = BlockItem {
                id,
                block_id,
                sort_order: index,
                ..BlockItem::default()
            };
            upsert_block_item(tx, &item).await?;
        }
    }

    Ok(())
}

async fn upsert_block_item(tx: &mut Transaction<'_, Db>, item: &BlockItem) -> Result<()> {
    let result = sqlx::query(
        r#"UPDATE "BlockItem"
           SET "BlockId" = ?,
               "Index" = ?,
               "CollectionType" = ?,
               "CollectionId" = ?,
               "MediaItemId" = ?,
               "MultiCollectionId" = ?,
               "SmartCollectionId" = ?,
               "SearchQuery" = ?,
               "PlaybackOrder" = ?,
               "SearchTitle" = NULL
           WHERE "Id" = ?"#,
    )
    .bind(item.block_id)
    .bind(item.sort_order)
    .bind(encode_enum(item.collection_type))
    .bind(item.collection_id)
    .bind(item.media_item_id)
    .bind(item.multi_collection_id)
    .bind(item.smart_collection_id)
    .bind(item.search_query.as_deref())
    .bind(encode_enum(item.playback_order))
    .bind(item.id)
    .execute(&mut **tx)
    .await
    .repository()?;

    if result.rows_affected() == 0 {
        sqlx::query(
            r#"INSERT INTO "BlockItem" (
                "Id", "BlockId", "Index", "CollectionType", "CollectionId",
                "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                "SearchQuery", "PlaybackOrder"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item.id)
        .bind(item.block_id)
        .bind(item.sort_order)
        .bind(encode_enum(item.collection_type))
        .bind(item.collection_id)
        .bind(item.media_item_id)
        .bind(item.multi_collection_id)
        .bind(item.smart_collection_id)
        .bind(item.search_query.as_deref())
        .bind(encode_enum(item.playback_order))
        .execute(&mut **tx)
        .await
        .repository()?;
    }

    replace_ids(
        tx,
        r#"DELETE FROM "BlockItemWatermark" WHERE "BlockItemId" = ?"#,
        r#"REPLACE INTO "BlockItemWatermark" ("BlockItemId", "WatermarkId") VALUES (?, ?)"#,
        item.id,
        &item.watermark_ids,
    )
    .await?;
    replace_ids(
        tx,
        r#"DELETE FROM "BlockItemGraphicsElement" WHERE "BlockItemId" = ?"#,
        r#"REPLACE INTO "BlockItemGraphicsElement" ("BlockItemId", "GraphicsElementId") VALUES (?, ?)"#,
        item.id,
        &item.graphics_element_ids,
    )
    .await?;

    Ok(())
}

async fn load_ids_in_tx(
    tx: &mut Transaction<'_, Db>,
    sql: &str,
    id: EntityId,
) -> Result<Vec<EntityId>> {
    let rows = sqlx::query(crate::db::assert_sql_safe(sql))
        .bind(id)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
    rows.iter()
        .map(|row| row.try_get("Id").repository())
        .collect()
}

fn map_block(row: &DbRow) -> Result<Block> {
    let date_updated: String = row.try_get("DateUpdated").repository()?;
    let updated_ticks: i64 = date_updated.parse().unwrap_or(0);
    let minutes: i64 = row.try_get("Minutes").repository()?;
    Ok(Block {
        id: row.try_get("Id").repository()?,
        block_group_id: row.try_get("BlockGroupId").repository()?,
        name: row.try_get("Name").repository()?,
        updated_ticks,
        duration: chrono::Duration::minutes(minutes),
        stop_scheduling: decode_enum_column("block stop_scheduling", row, "StopScheduling")?,
        item_ids: Vec::new(),
    })
}

async fn map_block_item(pool: &Pool<crate::db::Db>, row: &DbRow) -> Result<BlockItem> {
    let id = row.try_get::<EntityId, _>("Id").repository()?;
    Ok(BlockItem {
        id,
        block_id: row.try_get("BlockId").repository()?,
        collection_type: decode_enum_column("block item collection_type", row, "CollectionType")?,
        collection_id: row.try_get("CollectionId").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        multi_collection_id: row.try_get("MultiCollectionId").repository()?,
        smart_collection_id: row.try_get("SmartCollectionId").repository()?,
        playlist_id: None,
        search_query: row.try_get("SearchQuery").repository()?,
        playback_order: decode_enum_column("block item playback_order", row, "PlaybackOrder")?,
        sort_order: row.try_get("Index").repository()?,
        watermark_ids: load_ids(
            pool,
            r#"SELECT "WatermarkId" FROM "BlockItemWatermark" WHERE "BlockItemId" = ? ORDER BY "WatermarkId""#,
            id,
        )
        .await?,
        graphics_element_ids: load_ids(
            pool,
            r#"SELECT "GraphicsElementId" FROM "BlockItemGraphicsElement" WHERE "BlockItemId" = ? ORDER BY "GraphicsElementId""#,
            id,
        )
        .await?,
    })
}
