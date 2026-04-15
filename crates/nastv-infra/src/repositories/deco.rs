use async_trait::async_trait;
use nastv_core::repositories::{DecoRepository, Result};
use nastv_domain::{Deco, EntityId};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, load_ids, replace_ids, to_json};

#[derive(Clone)]
pub struct SqliteDecoRepository(pub Pool<Sqlite>);

#[async_trait]
impl DecoRepository for SqliteDecoRepository {
    async fn get_with_watermarks(&self, id: EntityId) -> Result<Deco> {
        let mut deco: Deco =
            from_json("deco", id, fetch_data(&self.0, "decos", "deco", id).await?)?;
        deco.watermark_ids = load_ids(
            &self.0,
            "SELECT watermark_id FROM deco_watermarks WHERE deco_id = ? ORDER BY watermark_id",
            id,
        )
        .await?;
        deco.graphics_element_ids =
            load_ids(&self.0, "SELECT graphics_element_id FROM deco_graphics_elements WHERE deco_id = ? ORDER BY graphics_element_id", id)
                .await?;
        Ok(deco)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Deco>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM decos WHERE deco_group_id = ? ORDER BY name, id",
        )
        .bind(group_id)
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| {
                let mut deco: Deco = from_json("deco", id, data)?;
                deco.watermark_ids.clear();
                deco.graphics_element_ids.clear();
                Ok(deco)
            })
            .collect()
    }

    async fn upsert(&self, deco: &Deco) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO decos (id, deco_group_id, name, data) VALUES (?, ?, ?, ?)",
        )
        .bind(deco.id)
        .bind(deco.deco_group_id)
        .bind(&deco.name)
        .bind(to_json(deco)?)
        .execute(&mut *tx)
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM deco_watermarks WHERE deco_id = ?",
            "INSERT OR REPLACE INTO deco_watermarks (deco_id, watermark_id) VALUES (?, ?)",
            deco.id,
            &deco.watermark_ids,
        )
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM deco_graphics_elements WHERE deco_id = ?",
            "INSERT OR REPLACE INTO deco_graphics_elements (deco_id, graphics_element_id) VALUES (?, ?)",
            deco.id,
            &deco.graphics_element_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
