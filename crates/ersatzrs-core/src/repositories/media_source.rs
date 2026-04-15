use async_trait::async_trait;
use ersatzrs_domain::{
    EmbyMediaSource, EntityId, JellyfinMediaSource, MediaSource, PlexMediaSource,
};

use super::Result;

#[async_trait]
pub trait MediaSourceRepository: Send + Sync {
    async fn get_plex(&self, id: EntityId) -> Result<PlexMediaSource>;
    async fn get_jellyfin(&self, id: EntityId) -> Result<JellyfinMediaSource>;
    async fn get_emby(&self, id: EntityId) -> Result<EmbyMediaSource>;
    async fn list_all(&self) -> Result<Vec<MediaSource>>;
    async fn upsert_plex(&self, source: &PlexMediaSource) -> Result<()>;
    async fn upsert_jellyfin(&self, source: &JellyfinMediaSource) -> Result<()>;
    async fn upsert_emby(&self, source: &EmbyMediaSource) -> Result<()>;
}
