use async_trait::async_trait;
use nastv_core::repositories::{MetadataRepository, Result};
use nastv_domain::{EntityId, EpisodeMetadata, MovieMetadata, MusicVideoMetadata};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, to_json};

#[derive(Clone)]
pub struct SqliteMetadataRepository(pub Pool<Sqlite>);

#[async_trait]
impl MetadataRepository for SqliteMetadataRepository {
    async fn get_movie_metadata(&self, item_id: EntityId) -> Result<MovieMetadata> {
        let row = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM movie_metadata WHERE item_id = ?",
        )
        .bind(item_id)
        .fetch_one(&self.0)
        .await?;
        from_json("movie metadata", row.0, row.1)
    }

    async fn get_episode_metadata(&self, item_id: EntityId) -> Result<EpisodeMetadata> {
        let row = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM episode_metadata WHERE item_id = ?",
        )
        .bind(item_id)
        .fetch_one(&self.0)
        .await?;
        from_json("episode metadata", row.0, row.1)
    }

    async fn get_music_video_metadata(&self, item_id: EntityId) -> Result<MusicVideoMetadata> {
        let row = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM music_video_metadata WHERE item_id = ?",
        )
        .bind(item_id)
        .fetch_one(&self.0)
        .await?;
        from_json("music video metadata", row.0, row.1)
    }

    async fn upsert_movie_metadata(&self, metadata: &MovieMetadata) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO movie_metadata (id, item_id, title, data) VALUES (?, ?, ?, ?)",
        )
        .bind(metadata.metadata.id)
        .bind(metadata.metadata.id)
        .bind(&metadata.metadata.title)
        .bind(to_json(metadata)?)
        .execute(&self.0)
        .await?;
        Ok(())
    }

    async fn upsert_episode_metadata(&self, metadata: &EpisodeMetadata) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO episode_metadata (id, item_id, title, data) VALUES (?, ?, ?, ?)")
            .bind(metadata.metadata.id)
            .bind(metadata.metadata.id)
            .bind(&metadata.metadata.title)
            .bind(to_json(metadata)?)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}

#[allow(dead_code)]
async fn get_by_id(
    pool: &Pool<Sqlite>,
    table: &str,
    kind: &'static str,
    id: EntityId,
) -> Result<String> {
    fetch_data(pool, table, kind, id).await
}
