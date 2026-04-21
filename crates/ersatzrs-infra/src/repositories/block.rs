use crate::db::DbRow;
use async_trait::async_trait;
use ersatzrs_core::repositories::{BlockRepository, Result};
use ersatzrs_domain::{Block, EntityId};
use sqlx::{Pool, Row};

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
        sqlx::query(
            r#"REPLACE INTO "Block" (
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
        replace_ids(
            &mut tx,
            r#"DELETE FROM "BlockItem" WHERE "BlockId" = ?"#,
            r#"REPLACE INTO "BlockItem" (
                "BlockId", "Id", "CollectionType", "PlaybackOrder", "Index"
             ) VALUES (?, ?, 0, 0, 0)"#,
            block.id,
            &block.item_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
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
