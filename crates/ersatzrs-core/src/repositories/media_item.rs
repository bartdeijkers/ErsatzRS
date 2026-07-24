use async_trait::async_trait;
use ersatzrs_domain::{EntityId, MediaItem};

use super::Result;

#[async_trait]
pub trait MediaItemRepository: Send + Sync {
    async fn get_with_version(&self, id: EntityId) -> Result<MediaItem>;
    async fn list_for_library_path(&self, path_id: EntityId) -> Result<Vec<MediaItem>>;
    async fn find_by_path(&self, path: &str) -> Result<MediaItem>;
    async fn upsert(&self, item: &MediaItem) -> Result<()>;
    async fn delete_admin(&self, ids: &[EntityId]) -> Result<u64>;
    async fn empty_trash_admin(&self) -> Result<u64>;
}
