use async_trait::async_trait;
use ersatzrs_core::repositories::{GraphicsRepository, Result};
use ersatzrs_domain::{EntityId, GraphicsElement};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum_column, encode_enum, not_found};

#[derive(Clone)]
pub struct SqliteGraphicsRepository(pub Pool<Sqlite>);

#[async_trait]
impl GraphicsRepository for SqliteGraphicsRepository {
    async fn get_element(&self, id: EntityId) -> Result<GraphicsElement> {
        let row = sqlx::query(r#"SELECT * FROM "GraphicsElement" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("graphics element", id, error))?;
        map_element(&row)
    }

    async fn list_for_channel(&self, channel_id: EntityId) -> Result<Vec<GraphicsElement>> {
        let rows = sqlx::query(
            r#"SELECT "ge".*
             FROM "GraphicsElement" AS "ge"
             JOIN "PlayoutItemGraphicsElement" AS "pige" ON "pige"."GraphicsElementId" = "ge"."Id"
             JOIN "PlayoutItem" AS "pi" ON "pi"."Id" = "pige"."PlayoutItemId"
             JOIN "Playout" AS "p" ON "p"."Id" = "pi"."PlayoutId"
             WHERE "p"."ChannelId" = ?
             UNION
             SELECT "ge".*
             FROM "GraphicsElement" AS "ge"
             JOIN "DecoGraphicsElement" AS "dge" ON "dge"."GraphicsElementId" = "ge"."Id"
             JOIN "Deco" AS "d" ON "d"."Id" = "dge"."DecoId"
             JOIN "Playout" AS "p" ON "p"."DecoId" = "d"."Id"
             WHERE "p"."ChannelId" = ?
             ORDER BY 1"#,
        )
        .bind(channel_id)
        .bind(channel_id)
        .fetch_all(&self.0)
        .await?;
        rows.iter().map(map_element).collect()
    }

    async fn upsert_element(&self, element: &GraphicsElement) -> Result<()> {
        sqlx::query(
            r#"INSERT OR REPLACE INTO "GraphicsElement" (
                "Id", "Name", "Kind", "Path",
                "X", "Y", "Width", "Height", "ZIndex", "EnableExpression", "Text"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(element.id)
        .bind(&element.name)
        .bind(encode_enum(element.kind))
        .bind(&element.path)
        .bind(element.x)
        .bind(element.y)
        .bind(element.width)
        .bind(element.height)
        .bind(element.z_index)
        .bind(&element.enable_expression)
        .bind(&element.text)
        .execute(&self.0)
        .await?;
        Ok(())
    }
}

fn map_element(row: &SqliteRow) -> Result<GraphicsElement> {
    Ok(GraphicsElement {
        id: row.try_get("Id")?,
        name: row.try_get("Name")?,
        kind: decode_enum_column("graphics element kind", row, "Kind")?,
        path: row.try_get("Path")?,
        x: row.try_get("X").unwrap_or(0),
        y: row.try_get("Y").unwrap_or(0),
        width: row.try_get("Width").unwrap_or(0),
        height: row.try_get("Height").unwrap_or(0),
        z_index: row.try_get("ZIndex").unwrap_or(0),
        enable_expression: row.try_get("EnableExpression").unwrap_or(None),
        text: row.try_get("Text").unwrap_or(None),
        script: None,
        variables: None,
    })
}
