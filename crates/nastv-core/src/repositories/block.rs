use async_trait::async_trait;
use nastv_domain::{Block, EntityId};

use super::Result;

#[async_trait]
pub trait BlockRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<Block>;
    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Block>>;
    async fn upsert(&self, block: &Block) -> Result<()>;
}
