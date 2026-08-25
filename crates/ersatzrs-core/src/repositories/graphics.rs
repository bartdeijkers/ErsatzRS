use async_trait::async_trait;
use ersatzrs_domain::{EntityId, GraphicsElement};

use super::Result;

#[async_trait]
pub trait GraphicsRepository: Send + Sync {
    async fn list_elements(&self) -> Result<Vec<GraphicsElement>>;
    async fn get_element(&self, id: EntityId) -> Result<GraphicsElement>;
    async fn list_for_channel(&self, channel_id: EntityId) -> Result<Vec<GraphicsElement>>;
    async fn create_element(&self, element: &GraphicsElement) -> Result<EntityId>;
    async fn upsert_element(&self, element: &GraphicsElement) -> Result<()>;
}
