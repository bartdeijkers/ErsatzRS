use async_trait::async_trait;
use nastv_domain::{Deco, EntityId};

use super::Result;

#[async_trait]
pub trait DecoRepository: Send + Sync {
    async fn get_with_watermarks(&self, id: EntityId) -> Result<Deco>;
    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Deco>>;
    async fn upsert(&self, deco: &Deco) -> Result<()>;
}
