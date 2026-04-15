use async_trait::async_trait;
use nastv_core::repositories::{BlockRepository, Result};
use nastv_domain::{Block, EntityId};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, load_ids, replace_ids, to_json};

#[derive(Clone)]
pub struct SqliteBlockRepository(pub Pool<Sqlite>);

#[async_trait]
impl BlockRepository for SqliteBlockRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Block> {
        let mut block: Block = from_json(
            "block",
            id,
            fetch_data(&self.0, "blocks", "block", id).await?,
        )?;
        block.item_ids = load_ids(
            &self.0,
            "SELECT id FROM block_items WHERE block_id = ? ORDER BY sort_order, id",
            id,
        )
        .await?;
        Ok(block)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Block>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM blocks WHERE block_group_id = ? ORDER BY name, id",
        )
        .bind(group_id)
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| {
                let mut block: Block = from_json("block", id, data)?;
                block.item_ids.clear();
                Ok(block)
            })
            .collect()
    }

    async fn upsert(&self, block: &Block) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO blocks (id, block_group_id, name, duration_ms, data)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(block.id)
        .bind(block.block_group_id)
        .bind(&block.name)
        .bind(block.duration.num_milliseconds())
        .bind(to_json(block)?)
        .execute(&mut *tx)
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM block_items WHERE block_id = ?",
            "INSERT OR REPLACE INTO block_items (block_id, id, sort_order, data) VALUES (?, ?, 0, '{}')",
            block.id,
            &block.item_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
