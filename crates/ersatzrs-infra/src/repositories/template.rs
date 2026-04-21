use crate::db::DbRow;
use async_trait::async_trait;
use ersatzrs_core::repositories::{Result, TemplateRepository};
use ersatzrs_domain::{EntityId, Template};
use sqlx::{Pool, Row};

use super::common::{load_ids, not_found};

#[derive(Clone)]
pub struct SqliteTemplateRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl TemplateRepository for SqliteTemplateRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Template> {
        let row = sqlx::query(r#"SELECT * FROM "Template" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("template", id, error))?;
        let mut template = map_template(&row)?;
        template.item_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "TemplateItem" WHERE "TemplateId" = ? ORDER BY "StartTime", "Id""#,
            id,
        )
        .await?;
        Ok(template)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Template>> {
        let rows = sqlx::query(
            r#"SELECT * FROM "Template" WHERE "TemplateGroupId" = ? ORDER BY "Name", "Id""#,
        )
        .bind(group_id)
        .fetch_all(&self.0)
        .await?;
        rows.iter()
            .map(|row| {
                let mut template = map_template(row)?;
                template.item_ids.clear();
                Ok(template)
            })
            .collect()
    }

    async fn upsert(&self, template: &Template) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            r#"REPLACE INTO "Template" (
                "Id", "TemplateGroupId", "Name", "DateUpdated"
             ) VALUES (?, ?, ?, ?)"#,
        )
        .bind(template.id)
        .bind(template.template_group_id)
        .bind(&template.name)
        .bind(template.updated_ticks.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn map_template(row: &DbRow) -> Result<Template> {
    Ok(Template {
        id: row.try_get("Id")?,
        template_group_id: row.try_get("TemplateGroupId")?,
        name: row.try_get("Name")?,
        updated_ticks: row
            .try_get::<String, _>("DateUpdated")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0),
        item_ids: Vec::new(),
    })
}
