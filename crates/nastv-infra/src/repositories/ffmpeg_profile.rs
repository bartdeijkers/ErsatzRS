use async_trait::async_trait;
use nastv_core::repositories::{FfmpegProfileRepository, Result};
use nastv_domain::{EntityId, FFmpegProfile};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, to_json};

#[derive(Clone)]
pub struct SqliteFfmpegProfileRepository(pub Pool<Sqlite>);

#[async_trait]
impl FfmpegProfileRepository for SqliteFfmpegProfileRepository {
    async fn get(&self, id: EntityId) -> Result<FFmpegProfile> {
        from_json(
            "ffmpeg profile",
            id,
            fetch_data(&self.0, "ffmpeg_profiles", "ffmpeg profile", id).await?,
        )
    }

    async fn list(&self) -> Result<Vec<FFmpegProfile>> {
        let rows = sqlx::query!("SELECT data FROM ffmpeg_profiles ORDER BY name, id")
            .fetch_all(&self.0)
            .await?;
        rows.into_iter()
            .map(|row| from_json("ffmpeg profile", 0, row.data))
            .collect()
    }

    async fn upsert(&self, profile: &FFmpegProfile) -> Result<()> {
        let data = to_json(profile)?;
        sqlx::query!(
            "INSERT OR REPLACE INTO ffmpeg_profiles (id, name, data) VALUES (?, ?, ?)",
            profile.id,
            profile.name,
            data
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query("DELETE FROM ffmpeg_profiles WHERE id = ?")
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}
