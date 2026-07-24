use async_trait::async_trait;
use ersatzrs_domain::{EntityId, Template};

use super::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateGroupAdminRow {
    pub id: EntityId,
    pub name: String,
    pub template_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateAdminRow {
    pub id: EntityId,
    pub group_id: EntityId,
    pub group_name: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateItemAdmin {
    pub block_id: EntityId,
    pub block_name: String,
    pub start_minutes: i32,
    pub duration_minutes: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateBlockGroupAdmin {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateBlockAdmin {
    pub id: EntityId,
    pub group_id: EntityId,
    pub name: String,
    pub minutes: i32,
}

#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<Template>;
    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Template>>;
    async fn upsert(&self, template: &Template) -> Result<()>;
    async fn list_groups_admin(&self) -> Result<Vec<TemplateGroupAdminRow>>;
    async fn list_admin(&self, search: &str) -> Result<Vec<TemplateAdminRow>>;
    async fn group_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn template_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn group_name_exists_admin(&self, name: &str) -> Result<bool>;
    async fn template_name_exists_admin(
        &self,
        group_id: EntityId,
        template_id: Option<EntityId>,
        name: &str,
    ) -> Result<bool>;
    async fn create_group_admin(&self, name: &str) -> Result<EntityId>;
    async fn create_admin(&self, group_id: EntityId, name: &str) -> Result<EntityId>;
    async fn copy_admin(
        &self,
        template_id: EntityId,
        new_group_id: EntityId,
        name: &str,
    ) -> Result<EntityId>;
    async fn delete_group_admin(&self, id: EntityId) -> Result<()>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
    async fn get_admin(&self, id: EntityId) -> Result<Option<TemplateAdminRow>>;
    async fn list_items_admin(&self, template_id: EntityId) -> Result<Vec<TemplateItemAdmin>>;
    async fn list_block_groups_admin(&self) -> Result<Vec<TemplateBlockGroupAdmin>>;
    async fn list_blocks_admin(&self, group_id: EntityId) -> Result<Vec<TemplateBlockAdmin>>;
    async fn save_admin(&self, id: EntityId, name: &str, items: &[TemplateItemAdmin])
        -> Result<()>;
}
