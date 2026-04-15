use async_trait::async_trait;
use ersatzrs_domain::{Channel, EntityId};

use super::Result;

#[async_trait]
pub trait ChannelRepository: Send + Sync {
    async fn get_with_profile(&self, id: EntityId) -> Result<Channel>;
    async fn get_by_number(&self, number: &str) -> Result<Channel>;
    async fn list_enabled(&self) -> Result<Vec<Channel>>;
    async fn upsert(&self, channel: &Channel) -> Result<()>;
    async fn delete(&self, id: EntityId) -> Result<()>;
}
