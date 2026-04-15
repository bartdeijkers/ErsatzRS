use async_trait::async_trait;
use nastv_core::repositories::{GraphicsRepository, Result};
use nastv_domain::{EntityId, GraphicsElement};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, not_found};

#[derive(Clone)]
pub struct SqliteGraphicsRepository(pub Pool<Sqlite>);

#[async_trait]
impl GraphicsRepository for SqliteGraphicsRepository {
    async fn get_element(&self, id: EntityId) -> Result<GraphicsElement> {
        let row = sqlx::query("SELECT * FROM graphics_elements WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("graphics element", id, error))?;
        map_element(&row)
    }

    async fn list_for_channel(&self, channel_id: EntityId) -> Result<Vec<GraphicsElement>> {
        let rows = sqlx::query(
            "SELECT ge.*
             FROM graphics_elements ge
             JOIN playout_item_graphics_elements pige ON pige.graphics_element_id = ge.id
             JOIN playout_items pi ON pi.id = pige.playout_item_id
             JOIN playouts p ON p.id = pi.playout_id
             WHERE p.channel_id = ?
             UNION
             SELECT ge.*
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
        rows.iter().map(map_element).collect()
    }

    async fn upsert_element(&self, element: &GraphicsElement) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO graphics_elements (
                id, name, kind, x, y, width, height, z_index, path, text, script, variables
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(element.id)
        .bind(&element.name)
        .bind(format!("{:?}", element.kind))
        .bind(element.x)
        .bind(element.y)
        .bind(element.width)
        .bind(element.height)
        .bind(element.z_index)
        .bind(&element.path)
        .bind(&element.text)
        .bind(&element.script)
        .bind(&element.variables)
        .execute(&self.0)
        .await?;
        Ok(())
    }
}

fn map_element(row: &SqliteRow) -> Result<GraphicsElement> {
    Ok(GraphicsElement {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        kind: decode_enum(
            "graphics element kind",
            &row.try_get::<String, _>("kind")?,
        )?,
        x: row.try_get("x")?,
        y: row.try_get("y")?,
        width: row.try_get("width")?,
        height: row.try_get("height")?,
        z_index: row.try_get("z_index")?,
        path: row.try_get("path")?,
        text: row.try_get("text")?,
        script: row.try_get("script")?,
        variables: row.try_get("variables")?,
    })
}
