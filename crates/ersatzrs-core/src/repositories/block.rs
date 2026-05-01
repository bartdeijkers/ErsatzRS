use async_trait::async_trait;
use ersatzrs_domain::{Block, BlockItem, EntityId};

use super::Result;

#[async_trait]
pub trait BlockRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<Block>;
    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Block>>;
    async fn upsert(&self, block: &Block) -> Result<()>;
    async fn get_item(&self, id: EntityId) -> Result<BlockItem>;
    async fn list_items(&self, block_id: EntityId) -> Result<Vec<BlockItem>>;
    async fn upsert_item(&self, item: &BlockItem) -> Result<()>;
    async fn delete_item(&self, id: EntityId) -> Result<()>;
}
