use async_trait::async_trait;
use ersatzrs_core::repositories::{BlockRepository, Result};
use ersatzrs_domain::{Block, EntityId};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_duration, decode_enum, load_ids, not_found, replace_ids};

#[derive(Clone)]
pub struct SqliteBlockRepository(pub Pool<Sqlite>);

#[async_trait]
impl BlockRepository for SqliteBlockRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Block> {
        let row = sqlx::query("SELECT * FROM blocks WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("block", id, error))?;
        let mut block = map_block(&row)?;
        block.item_ids = load_ids(
            &self.0,
            "SELECT id FROM block_items WHERE block_id = ? ORDER BY sort_order, id",
            id,
        )
        .await?;
        Ok(block)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Block>> {
        let rows = sqlx::query("SELECT * FROM blocks WHERE block_group_id = ? ORDER BY name, id")
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
            "INSERT OR REPLACE INTO blocks (id, block_group_id, name, duration_ms, stop_scheduling)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(block.id)
        .bind(block.block_group_id)
        .bind(&block.name)
        .bind(block.duration.num_milliseconds())
        .bind(format!("{:?}", block.stop_scheduling))
        .execute(&mut *tx)
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM block_items WHERE block_id = ?",
            "INSERT OR REPLACE INTO block_items (
                block_id, id, collection_type, playback_order, sort_order
             ) VALUES (?, ?, 'Collection', 'None', 0)",
            block.id,
            &block.item_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn map_block(row: &SqliteRow) -> Result<Block> {
    Ok(Block {
        id: row.try_get("id")?,
        block_group_id: row.try_get("block_group_id")?,
        name: row.try_get("name")?,
        duration: decode_duration(row.try_get("duration_ms")?),
        stop_scheduling: decode_enum(
            "block stop_scheduling",
            &row.try_get::<String, _>("stop_scheduling")?,
        )?,
        item_ids: Vec::new(),
    })
}
