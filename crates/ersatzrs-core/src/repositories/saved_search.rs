use async_trait::async_trait;
use ersatzrs_domain::EntityId;

use super::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedSearchAdminRow {
    pub id: EntityId,
    pub name: String,
    pub query: String,
}

#[async_trait]
pub trait SavedSearchRepository: Send + Sync {
    async fn list_admin(&self) -> Result<Vec<SavedSearchAdminRow>>;
    async fn get_admin(&self, id: EntityId) -> Result<Option<SavedSearchAdminRow>>;
    async fn name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn save_admin(&self, id: EntityId, name: &str, query: &str) -> Result<EntityId>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
}
