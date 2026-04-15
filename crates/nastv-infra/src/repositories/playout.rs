use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nastv_core::repositories::{PlayoutRepository, Result};
use nastv_domain::{EntityId, Playout, PlayoutBuildStatus, PlayoutItem};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, load_ids, replace_ids, to_json};

#[derive(Clone)]
pub struct SqlitePlayoutRepository(pub Pool<Sqlite>);

#[async_trait]
impl PlayoutRepository for SqlitePlayoutRepository {
    async fn get_with_anchor(&self, id: EntityId) -> Result<Playout> {
        let mut playout: Playout = from_json(
            "playout",
            id,
            fetch_data(&self.0, "playouts", "playout", id).await?,
        )?;
        playout.item_ids = load_ids(
            &self.0,
            "SELECT id FROM playout_items WHERE playout_id = ? ORDER BY start, id",
            id,
        )
        .await?;
        playout.gap_ids = load_ids(
            &self.0,
            "SELECT id FROM playout_gaps WHERE playout_id = ? ORDER BY start, id",
            id,
        )
        .await?;
        let status = sqlx::query_scalar::<_, String>(
            "SELECT data FROM playout_build_statuses WHERE playout_id = ?",
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await?;
        playout.build_status = status
            .map(|data| from_json("playout build status", id, data))
            .transpose()?;
        Ok(playout)
    }

    async fn items_in_range(
        &self,
        playout_id: EntityId,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Vec<PlayoutItem>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data
             FROM playout_items
             WHERE playout_id = ? AND start < ? AND finish > ?
             ORDER BY start, id",
        )
        .bind(playout_id)
        .bind(finish.to_rfc3339())
        .bind(start.to_rfc3339())
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| from_json("playout item", id, data))
            .collect()
    }

    async fn upsert_item(&self, item: &PlayoutItem) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO playout_items (id, playout_id, media_item_id, start, finish, data)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(item.id)
        .bind(item.playout_id)
        .bind(item.media_item_id)
        .bind(item.start.to_rfc3339())
        .bind(item.finish.to_rfc3339())
        .bind(to_json(item)?)
        .execute(&mut *tx)
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM playout_item_watermarks WHERE playout_item_id = ?",
            "INSERT OR REPLACE INTO playout_item_watermarks (playout_item_id, watermark_id) VALUES (?, ?)",
            item.id,
            &item.watermark_ids,
        )
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM playout_item_graphics_elements WHERE playout_item_id = ?",
            "INSERT OR REPLACE INTO playout_item_graphics_elements (playout_item_id, graphics_element_id) VALUES (?, ?)",
            item.id,
            &item.graphics_element_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn delete_items_before(&self, playout_id: EntityId, cutoff: DateTime<Utc>) -> Result<()> {
        sqlx::query("DELETE FROM playout_items WHERE playout_id = ? AND finish < ?")
            .bind(playout_id)
            .bind(cutoff.to_rfc3339())
            .execute(&self.0)
            .await?;
        Ok(())
    }

    async fn upsert_build_status(&self, status: &PlayoutBuildStatus) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO playout_build_statuses (playout_id, last_build, success, message, data)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(status.playout_id)
        .bind(status.last_build.to_rfc3339())
        .bind(status.success)
        .bind(&status.message)
        .bind(to_json(status)?)
        .execute(&self.0)
        .await?;
        Ok(())
    }
}
