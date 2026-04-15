use async_trait::async_trait;
use nastv_core::repositories::{ProgramScheduleRepository, Result};
use nastv_domain::{EntityId, ProgramSchedule};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, load_ids, not_found, replace_ids};

#[derive(Clone)]
pub struct SqliteProgramScheduleRepository(pub Pool<Sqlite>);

#[async_trait]
impl ProgramScheduleRepository for SqliteProgramScheduleRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<ProgramSchedule> {
        let row = sqlx::query("SELECT * FROM program_schedules WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("program schedule", id, error))?;
        let mut schedule = map_program_schedule(&row)?;
        schedule.item_ids =
            load_ids(&self.0, "SELECT id FROM program_schedule_items WHERE program_schedule_id = ? ORDER BY sort_order, id", id)
                .await?;
        schedule.alternate_ids = load_ids(
            &self.0,
            "SELECT id FROM program_schedule_alternates WHERE program_schedule_id = ? ORDER BY id",
            id,
        )
        .await?;
        schedule.playout_ids = load_ids(
            &self.0,
            "SELECT id FROM playouts WHERE program_schedule_id = ? ORDER BY id",
            id,
        )
        .await?;
        Ok(schedule)
    }

    async fn list_for_playout(&self, playout_id: EntityId) -> Result<Vec<ProgramSchedule>> {
        let rows = sqlx::query(
            "SELECT ps.*
             FROM program_schedules ps
             JOIN playouts p ON p.program_schedule_id = ps.id
             WHERE p.id = ?
             ORDER BY ps.name, ps.id",
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await?;
        rows.iter()
            .map(|row| {
                let mut schedule = map_program_schedule(row)?;
                schedule.item_ids.clear();
                schedule.playout_ids.clear();
                schedule.alternate_ids.clear();
                Ok(schedule)
            })
            .collect()
    }

    async fn upsert(&self, schedule: &ProgramSchedule) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO program_schedules (
                id, name, keep_multi_part_episodes_together, treat_collections_as_shows,
                shuffle_schedule_items, random_start_point, fixed_start_time_behavior
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
            .bind(schedule.id)
            .bind(&schedule.name)
            .bind(schedule.keep_multi_part_episodes_together)
            .bind(schedule.treat_collections_as_shows)
            .bind(schedule.shuffle_schedule_items)
            .bind(schedule.random_start_point)
            .bind(format!("{:?}", schedule.fixed_start_time_behavior))
            .execute(&mut *tx)
            .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM program_schedule_items WHERE program_schedule_id = ?",
            "INSERT OR REPLACE INTO program_schedule_items (
                program_schedule_id, id, collection_type, playout_mode, playback_order,
                multiple_mode, start_type, guide_mode, tail_mode, fill_with_group_mode,
                marathon_group_by, marathon_shuffle_items, sort_order
             ) VALUES (?, ?, 'Collection', 'Duration', 'None', 'Count', 'Dynamic', 'Normal', 'None', 'None', 'None', 0, 0)",
            schedule.id,
            &schedule.item_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn map_program_schedule(row: &SqliteRow) -> Result<ProgramSchedule> {
    Ok(ProgramSchedule {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        keep_multi_part_episodes_together: row.try_get("keep_multi_part_episodes_together")?,
        treat_collections_as_shows: row.try_get("treat_collections_as_shows")?,
        shuffle_schedule_items: row.try_get("shuffle_schedule_items")?,
        random_start_point: row.try_get("random_start_point")?,
        fixed_start_time_behavior: decode_enum(
            "program schedule fixed_start_time_behavior",
            &row.try_get::<String, _>("fixed_start_time_behavior")?,
        )?,
        item_ids: Vec::new(),
        playout_ids: Vec::new(),
        alternate_ids: Vec::new(),
    })
}
