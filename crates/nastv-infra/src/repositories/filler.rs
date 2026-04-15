use async_trait::async_trait;
use nastv_core::repositories::{FillerRepository, Result};
use nastv_domain::{EntityId, FillerPreset};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, to_json};

#[derive(Clone)]
pub struct SqliteFillerRepository(pub Pool<Sqlite>);

#[async_trait]
impl FillerRepository for SqliteFillerRepository {
    async fn get(&self, id: EntityId) -> Result<FillerPreset> {
        from_json(
            "filler preset",
            id,
            fetch_data(&self.0, "filler_presets", "filler preset", id).await?,
        )
    }

    async fn list(&self) -> Result<Vec<FillerPreset>> {
        let rows = sqlx::query!("SELECT data FROM filler_presets ORDER BY name, id")
            .fetch_all(&self.0)
            .await?;
        rows.into_iter()
            .map(|row| from_json("filler preset", 0, row.data))
            .collect()
    }

    async fn upsert(&self, preset: &FillerPreset) -> Result<()> {
        let data = to_json(preset)?;
        sqlx::query!(
            "INSERT OR REPLACE INTO filler_presets (id, name, data) VALUES (?, ?, ?)",
            preset.id,
            preset.name,
            data
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        sqlx::query("DELETE FROM filler_presets WHERE id = ?")
            .bind(id)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}
