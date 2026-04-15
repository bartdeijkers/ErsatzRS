use async_trait::async_trait;
use ersatzrs_domain::{EntityId, Template};

use super::Result;

#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<Template>;
    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Template>>;
    async fn upsert(&self, template: &Template) -> Result<()>;
}
