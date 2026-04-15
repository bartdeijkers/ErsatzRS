use async_trait::async_trait;
use ersatzrs_core::repositories::{DecoRepository, Result};
use ersatzrs_domain::{Deco, EntityId};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, load_ids, not_found, replace_ids};

#[derive(Clone)]
pub struct SqliteDecoRepository(pub Pool<Sqlite>);

#[async_trait]
impl DecoRepository for SqliteDecoRepository {
    async fn get_with_watermarks(&self, id: EntityId) -> Result<Deco> {
        let row = sqlx::query("SELECT * FROM decos WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("deco", id, error))?;
        let mut deco = map_deco(&row)?;
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
        let rows = sqlx::query("SELECT * FROM decos WHERE deco_group_id = ? ORDER BY name, id")
            .bind(group_id)
            .fetch_all(&self.0)
            .await?;
        rows.iter()
            .map(|row| {
                let mut deco = map_deco(row)?;
                deco.watermark_ids.clear();
                deco.graphics_element_ids.clear();
                Ok(deco)
            })
            .collect()
    }

    async fn upsert(&self, deco: &Deco) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO decos (id, deco_group_id, name, mode, break_content_id)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(deco.id)
        .bind(deco.deco_group_id)
        .bind(&deco.name)
        .bind(format!("{:?}", deco.mode))
        .bind(deco.break_content_id)
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

fn map_deco(row: &SqliteRow) -> Result<Deco> {
    Ok(Deco {
        id: row.try_get("id")?,
        deco_group_id: row.try_get("deco_group_id")?,
        name: row.try_get("name")?,
        mode: decode_enum("deco mode", &row.try_get::<String, _>("mode")?)?,
        break_content_id: row.try_get("break_content_id")?,
        watermark_ids: Vec::new(),
        graphics_element_ids: Vec::new(),
    })
}
