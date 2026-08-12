use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{GraphicsRepository, RepositoryError, Result};
use ersatzrs_domain::{
    EntityId, GraphicsElement, GraphicsElementConfig, GraphicsElementKind, ImageGraphicsConfig,
    MotionGraphicsConfig, ScriptGraphicsConfig, SubtitleGraphicsConfig, TextGraphicsConfig,
};
use sqlx::{Pool, Row};

use super::common::{decode_bool_column, decode_enum_column, encode_enum, not_found};

use super::common::upsert_sql;
#[derive(Clone)]
pub struct SqliteGraphicsRepository(pub Pool<crate::db::Db>);

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
        .await
        .repository()?;
        rows.iter().map(map_element).collect()
    }

    async fn upsert_element(&self, element: &GraphicsElement) -> Result<()> {
        sqlx::query(upsert_sql!(
            r#"INSERT INTO "GraphicsElement" (
                "Id", "Name", "Kind", "Path", "ZIndex",
                "OpacityPercent", "OpacityExpression", "Location",
                "HorizontalMarginPercent", "VerticalMarginPercent",
                "LocationX", "LocationY", "Scale", "ScaleWidthPercent",
                "PlaceWithinSourceContent", "EpgEntries", "Config"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            &[&["Id"]],
        ))
        .bind(element.id)
        .bind(&element.name)
        .bind(encode_enum(element.kind))
        .bind(&element.path)
        .bind(element.z_index)
        .bind(element.opacity_percent)
        .bind(&element.opacity_expression)
        .bind(encode_enum(element.location))
        .bind(element.horizontal_margin_percent)
        .bind(element.vertical_margin_percent)
        .bind(element.location_x)
        .bind(element.location_y)
        .bind(element.scale)
        .bind(element.scale_width_percent)
        .bind(element.place_within_source_content)
        .bind(element.epg_entries)
        .bind(
            serde_json::to_string(&element.config)
                .expect("graphics element config should serialize"),
        )
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }
}

fn map_element(row: &DbRow) -> Result<GraphicsElement> {
    let kind = decode_enum_column("graphics element kind", row, "Kind")?;
    let config = decode_config(kind, row.try_get("Config").ok().flatten())?;

    Ok(GraphicsElement {
        id: row.try_get("Id").repository()?,
        name: row
            .try_get::<Option<String>, _>("Name")
            .map(Option::unwrap_or_default)
            .or_else(|_| row.try_get("Name"))
            .repository()?,
        kind,
        path: row.try_get("Path").repository()?,
        z_index: row.try_get("ZIndex").unwrap_or(0),
        opacity_percent: row.try_get("OpacityPercent").unwrap_or(None),
        opacity_expression: row.try_get("OpacityExpression").unwrap_or(None),
        location: decode_enum_column("graphics element location", row, "Location")?,
        horizontal_margin_percent: row.try_get("HorizontalMarginPercent").unwrap_or(None),
        vertical_margin_percent: row.try_get("VerticalMarginPercent").unwrap_or(None),
        location_x: row.try_get("LocationX").unwrap_or(None),
        location_y: row.try_get("LocationY").unwrap_or(None),
        scale: decode_bool_column(row, "Scale").unwrap_or(false),
        scale_width_percent: row.try_get("ScaleWidthPercent").unwrap_or(None),
        place_within_source_content: decode_bool_column(row, "PlaceWithinSourceContent")
            .unwrap_or(false),
        epg_entries: row.try_get("EpgEntries").unwrap_or(0),
        config,
    })
}

fn decode_config(
    kind: GraphicsElementKind,
    value: Option<String>,
) -> Result<GraphicsElementConfig> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(default_config_for_kind(kind));
    };

    let config: GraphicsElementConfig =
        serde_json::from_str(&value).map_err(|error| RepositoryError::Decode {
            kind: "graphics element config",
            value: error.to_string(),
        })?;

    if config.kind() != kind {
        return Err(RepositoryError::Decode {
            kind: "graphics element config",
            value: format!(
                "config kind {:?} does not match row kind {:?}",
                config.kind(),
                kind
            ),
        });
    }

    Ok(config)
}

pub(crate) fn default_config_for_kind(kind: GraphicsElementKind) -> GraphicsElementConfig {
    match kind {
        GraphicsElementKind::Image => GraphicsElementConfig::Image(ImageGraphicsConfig::default()),
        GraphicsElementKind::Text => GraphicsElementConfig::Text(TextGraphicsConfig::default()),
        GraphicsElementKind::Subtitle => {
            GraphicsElementConfig::Subtitle(SubtitleGraphicsConfig::default())
        }
        GraphicsElementKind::Motion => {
            GraphicsElementConfig::Motion(MotionGraphicsConfig::default())
        }
        GraphicsElementKind::Script => {
            GraphicsElementConfig::Script(ScriptGraphicsConfig::default())
        }
    }
}
