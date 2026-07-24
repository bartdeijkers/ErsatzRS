use async_trait::async_trait;
use ersatzrs_domain::{ChannelWatermark, EntityId};

use super::Result;

#[async_trait]
pub trait WatermarkRepository: Send + Sync {
    async fn list_admin(&self) -> Result<Vec<ChannelWatermark>>;
    async fn get_admin(&self, id: EntityId) -> Result<Option<ChannelWatermark>>;
    async fn name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn save_admin(&self, watermark: &ChannelWatermark) -> Result<EntityId>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
    async fn copy_admin(&self, id: EntityId, name: &str) -> Result<EntityId>;
}
