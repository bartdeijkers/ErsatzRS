use async_trait::async_trait;
use ersatzrs_domain::{EntityId, FFmpegProfile};

use super::Result;

#[async_trait]
pub trait FfmpegProfileRepository: Send + Sync {
    async fn get(&self, id: EntityId) -> Result<FFmpegProfile>;
    async fn list(&self) -> Result<Vec<FFmpegProfile>>;
    async fn upsert(&self, profile: &FFmpegProfile) -> Result<()>;
    async fn delete(&self, id: EntityId) -> Result<()>;
}
