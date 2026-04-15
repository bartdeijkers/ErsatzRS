use async_trait::async_trait;
use nastv_domain::{EntityId, EpisodeMetadata, MovieMetadata, MusicVideoMetadata};

use super::Result;

#[async_trait]
pub trait MetadataRepository: Send + Sync {
    async fn get_movie_metadata(&self, item_id: EntityId) -> Result<MovieMetadata>;
    async fn get_episode_metadata(&self, item_id: EntityId) -> Result<EpisodeMetadata>;
    async fn get_music_video_metadata(&self, item_id: EntityId) -> Result<MusicVideoMetadata>;
    async fn upsert_movie_metadata(&self, metadata: &MovieMetadata) -> Result<()>;
    async fn upsert_episode_metadata(&self, metadata: &EpisodeMetadata) -> Result<()>;
}
