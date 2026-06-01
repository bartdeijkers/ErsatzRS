use std::collections::BTreeSet;

use crate::db::{Db, DbRow};
use async_trait::async_trait;
use ersatzrs_core::repositories::{BlockRepository, Result};
use ersatzrs_domain::{Block, BlockItem, EntityId};
use sqlx::{Pool, Row, Transaction};

use super::common::{decode_enum_column, encode_enum, load_ids, not_found, replace_ids};

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
                .await?;
        rows.iter()
            .map(|row| {
                let mut block = map_block(row)?;
                block.item_ids.clear();
                Ok(block)
            })
            .collect()
    }

    async fn upsert(&self, block: &Block) -> Result<()> {
        let mut tx = self.0.begin().await?;
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
        .await?;
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
            .await?;
        }
        sync_block_item_ids(&mut tx, block.id, &block.item_ids).await?;
        tx.commit().await?;
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
        .await?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(map_block_item(&self.0, &row).await?);
        }
        Ok(items)
    }

    async fn upsert_item(&self, item: &BlockItem) -> Result<()> {
        let mut tx = self.0.begin().await?;
        upsert_block_item(&mut tx, item).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn delete_item(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "BlockItem" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
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
            .await?;
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
            .await?;
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
    .await?;

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
        .await?;
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
        .await?;
    rows.iter()
        .map(|row| row.try_get("Id").map_err(Into::into))
        .collect()
}

fn map_block(row: &DbRow) -> Result<Block> {
    let date_updated: String = row.try_get("DateUpdated")?;
    let updated_ticks: i64 = date_updated.parse().unwrap_or(0);
    let minutes: i64 = row.try_get("Minutes")?;
    Ok(Block {
        id: row.try_get("Id")?,
        block_group_id: row.try_get("BlockGroupId")?,
        name: row.try_get("Name")?,
        updated_ticks,
        duration: chrono::Duration::minutes(minutes),
        stop_scheduling: decode_enum_column("block stop_scheduling", row, "StopScheduling")?,
        item_ids: Vec::new(),
    })
}

async fn map_block_item(pool: &Pool<crate::db::Db>, row: &DbRow) -> Result<BlockItem> {
    let id = row.try_get::<EntityId, _>("Id")?;
    Ok(BlockItem {
        id,
        block_id: row.try_get("BlockId")?,
        collection_type: decode_enum_column("block item collection_type", row, "CollectionType")?,
        collection_id: row.try_get("CollectionId")?,
        media_item_id: row.try_get("MediaItemId")?,
        multi_collection_id: row.try_get("MultiCollectionId")?,
        smart_collection_id: row.try_get("SmartCollectionId")?,
        playlist_id: None,
        search_query: row.try_get("SearchQuery")?,
        playback_order: decode_enum_column("block item playback_order", row, "PlaybackOrder")?,
        sort_order: row.try_get("Index")?,
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
