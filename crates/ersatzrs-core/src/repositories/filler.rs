use async_trait::async_trait;
use ersatzrs_domain::{EntityId, FillerPreset};

use super::Result;

#[async_trait]
pub trait FillerRepository: Send + Sync {
    async fn get(&self, id: EntityId) -> Result<FillerPreset>;
    async fn get_optional_admin(&self, id: EntityId) -> Result<Option<FillerPreset>>;
    async fn list(&self) -> Result<Vec<FillerPreset>>;
    async fn upsert(&self, preset: &FillerPreset) -> Result<()>;
    async fn delete(&self, id: EntityId) -> Result<()>;
    async fn name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn save_admin(&self, preset: &FillerPreset) -> Result<EntityId>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
}
