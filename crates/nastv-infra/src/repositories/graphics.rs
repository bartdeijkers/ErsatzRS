use async_trait::async_trait;
use nastv_core::repositories::{GraphicsRepository, Result};
use nastv_domain::{EntityId, GraphicsElement};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, to_json};

#[derive(Clone)]
pub struct SqliteGraphicsRepository(pub Pool<Sqlite>);

#[async_trait]
impl GraphicsRepository for SqliteGraphicsRepository {
    async fn get_element(&self, id: EntityId) -> Result<GraphicsElement> {
        from_json(
            "graphics element",
            id,
            fetch_data(&self.0, "graphics_elements", "graphics element", id).await?,
        )
    }

    async fn list_for_channel(&self, channel_id: EntityId) -> Result<Vec<GraphicsElement>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT ge.id, ge.data
             FROM graphics_elements ge
             JOIN playout_item_graphics_elements pige ON pige.graphics_element_id = ge.id
             JOIN playout_items pi ON pi.id = pige.playout_item_id
             JOIN playouts p ON p.id = pi.playout_id
             WHERE p.channel_id = ?
             UNION
             SELECT ge.id, ge.data
             FROM graphics_elements ge
             JOIN deco_graphics_elements dge ON dge.graphics_element_id = ge.id
             JOIN decos d ON d.id = dge.deco_id
             JOIN playouts p ON p.deco_id = d.id
             WHERE p.channel_id = ?
             ORDER BY 1",
        )
        .bind(channel_id)
        .bind(channel_id)
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| from_json("graphics element", id, data))
            .collect()
    }

    async fn upsert_element(&self, element: &GraphicsElement) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO graphics_elements (id, name, kind, data) VALUES (?, ?, ?, ?)",
        )
        .bind(element.id)
        .bind(&element.name)
        .bind(format!("{:?}", element.kind))
        .bind(to_json(element)?)
        .execute(&self.0)
        .await?;
        Ok(())
    }
}
