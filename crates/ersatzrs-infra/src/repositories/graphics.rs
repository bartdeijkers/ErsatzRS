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
            r#"SELECT ge.*
             FROM "GraphicsElement" ge
             JOIN "PlayoutItemGraphicsElement" pige ON pige."GraphicsElementId" = ge."Id"
             JOIN "PlayoutItem" pi ON pi."Id" = pige."PlayoutItemId"
             JOIN "Playout" p ON p."Id" = pi."PlayoutId"
             WHERE p."ChannelId" = ?
             UNION
             SELECT ge.*
             FROM "GraphicsElement" ge
             JOIN "DecoGraphicsElement" dge ON dge."GraphicsElementId" = ge."Id"
             JOIN "Deco" d ON d."Id" = dge."DecoId"
             JOIN "Playout" p ON p."DecoId" = d."Id"
             WHERE p."ChannelId" = ?
             ORDER BY 1"#,
        )
        .bind(channel_id)
        .bind(channel_id)
        .fetch_all(&self.0)
        .await?;
        rows.iter().map(map_element).collect()
    }

    async fn upsert_element(&self, element: &GraphicsElement) -> Result<()> {
        // ErsatzTV GraphicsElement only has Id, Path, Kind, Name.
        // The extra fields (x, y, width, height, z_index, text, script, variables) are
        // ErsatzRS-only extensions and are not stored in the ErsatzTV schema.
        sqlx::query(
            r#"INSERT OR REPLACE INTO "GraphicsElement" (
                "Id", "Name", "Kind", "Path"
             ) VALUES (?, ?, ?, ?)"#,
        )
        .bind(element.id)
        .bind(&element.name)
        .bind(encode_enum(element.kind))
        .bind(&element.path)
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
        // These fields do not exist in the ErsatzTV schema; default to zero/None.
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        z_index: 0,
        path: row.try_get("Path")?,
        text: None,
        script: None,
        variables: None,
    })
}
