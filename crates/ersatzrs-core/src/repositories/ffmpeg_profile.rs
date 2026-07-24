use async_trait::async_trait;
use ersatzrs_domain::{EntityId, FFmpegProfile, Resolution};

use super::Result;

#[async_trait]
pub trait FfmpegProfileRepository: Send + Sync {
    async fn get(&self, id: EntityId) -> Result<FFmpegProfile>;
    async fn get_optional_admin(&self, id: EntityId) -> Result<Option<FFmpegProfile>>;
    async fn list(&self) -> Result<Vec<FFmpegProfile>>;
    async fn upsert(&self, profile: &FFmpegProfile) -> Result<()>;
    async fn delete(&self, id: EntityId) -> Result<()>;
    async fn list_resolutions(&self) -> Result<Vec<Resolution>>;
    async fn name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn save_admin(&self, profile: &FFmpegProfile) -> Result<EntityId>;
    async fn copy_admin(&self, id: EntityId, name: &str) -> Result<EntityId>;
}
