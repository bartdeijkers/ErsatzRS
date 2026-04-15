use async_trait::async_trait;
use nastv_domain::{EntityId, GraphicsElement};

use super::Result;

#[async_trait]
pub trait GraphicsRepository: Send + Sync {
    async fn get_element(&self, id: EntityId) -> Result<GraphicsElement>;
    async fn list_for_channel(&self, channel_id: EntityId) -> Result<Vec<GraphicsElement>>;
    async fn upsert_element(&self, element: &GraphicsElement) -> Result<()>;
}
