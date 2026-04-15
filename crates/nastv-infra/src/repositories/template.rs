use async_trait::async_trait;
use nastv_core::repositories::{Result, TemplateRepository};
use nastv_domain::{EntityId, Template};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, load_ids, replace_ids, to_json};

#[derive(Clone)]
pub struct SqliteTemplateRepository(pub Pool<Sqlite>);

#[async_trait]
impl TemplateRepository for SqliteTemplateRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Template> {
        let mut template: Template = from_json(
            "template",
            id,
            fetch_data(&self.0, "templates", "template", id).await?,
        )?;
        template.item_ids = load_ids(
            &self.0,
            "SELECT id FROM template_items WHERE template_id = ? ORDER BY sort_order, id",
            id,
        )
        .await?;
        Ok(template)
    }

    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Template>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM templates WHERE template_group_id = ? ORDER BY name, id",
        )
        .bind(group_id)
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| {
                let mut template: Template = from_json("template", id, data)?;
                template.item_ids.clear();
                Ok(template)
            })
            .collect()
    }

    async fn upsert(&self, template: &Template) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query("INSERT OR REPLACE INTO templates (id, template_group_id, name, data) VALUES (?, ?, ?, ?)")
            .bind(template.id)
            .bind(template.template_group_id)
            .bind(&template.name)
            .bind(to_json(template)?)
            .execute(&mut *tx)
            .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM template_items WHERE template_id = ?",
            "INSERT OR REPLACE INTO template_items (template_id, id, block_id, sort_order, data) VALUES (?, ?, 0, 0, '{}')",
            template.id,
            &template.item_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
