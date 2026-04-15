use async_trait::async_trait;
use nastv_core::repositories::{Result, TemplateRepository};
use nastv_domain::{EntityId, Template};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{load_ids, not_found, replace_ids};

#[derive(Clone)]
pub struct SqliteTemplateRepository(pub Pool<Sqlite>);

#[async_trait]
impl TemplateRepository for SqliteTemplateRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Template> {
        let row = sqlx::query("SELECT * FROM templates WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("template", id, error))?;
        let mut template = map_template(&row)?;
        template.item_ids = load_ids(
            &self.0,
            "SELECT id FROM template_items WHERE template_id = ? ORDER BY sort_order, id",
            id,
        )
        .await?;
        Ok(template)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Template>> {
        let rows = sqlx::query("SELECT * FROM templates WHERE template_group_id = ? ORDER BY name, id")
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
        sqlx::query("INSERT OR REPLACE INTO templates (id, template_group_id, name) VALUES (?, ?, ?)")
            .bind(template.id)
            .bind(template.template_group_id)
            .bind(&template.name)
            .execute(&mut *tx)
            .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM template_items WHERE template_id = ?",
            "INSERT OR REPLACE INTO template_items (template_id, id, block_id, sort_order) VALUES (?, ?, 0, 0)",
            template.id,
            &template.item_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn map_template(row: &SqliteRow) -> Result<Template> {
    Ok(Template {
        id: row.try_get("id")?,
        template_group_id: row.try_get("template_group_id")?,
        name: row.try_get("name")?,
        item_ids: Vec::new(),
    })
}
