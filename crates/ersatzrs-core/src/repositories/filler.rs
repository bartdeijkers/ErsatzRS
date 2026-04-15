use async_trait::async_trait;
use ersatzrs_domain::{EntityId, FillerPreset};

use super::Result;

#[async_trait]
pub trait FillerRepository: Send + Sync {
    async fn get(&self, id: EntityId) -> Result<FillerPreset>;
    async fn list(&self) -> Result<Vec<FillerPreset>>;
    async fn upsert(&self, preset: &FillerPreset) -> Result<()>;
    async fn delete(&self, id: EntityId) -> Result<()>;
}
