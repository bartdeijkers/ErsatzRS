use async_trait::async_trait;
use ersatzrs_domain::{Collection, EntityId, Playlist, SmartCollection};

use super::Result;

#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<Collection>;
    async fn list_for_media_item(&self, item_id: EntityId) -> Result<Vec<Collection>>;
    async fn get_smart(&self, id: EntityId) -> Result<SmartCollection>;
    async fn get_playlist_with_items(&self, id: EntityId) -> Result<Playlist>;
    async fn upsert(&self, collection: &Collection) -> Result<()>;
}
