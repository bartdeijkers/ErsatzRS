use async_trait::async_trait;
use nastv_core::repositories::{ProgramScheduleRepository, Result};
use nastv_domain::{EntityId, ProgramSchedule};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, load_ids, replace_ids, to_json};

#[derive(Clone)]
pub struct SqliteProgramScheduleRepository(pub Pool<Sqlite>);

#[async_trait]
impl ProgramScheduleRepository for SqliteProgramScheduleRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<ProgramSchedule> {
        let mut schedule: ProgramSchedule = from_json(
            "program schedule",
            id,
            fetch_data(&self.0, "program_schedules", "program schedule", id).await?,
        )?;
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
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT ps.id, ps.data
             FROM program_schedules ps
             JOIN playouts p ON p.program_schedule_id = ps.id
             WHERE p.id = ?
             ORDER BY ps.name, ps.id",
        )
        .bind(playout_id)
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| {
                let mut schedule: ProgramSchedule = from_json("program schedule", id, data)?;
                schedule.item_ids.clear();
                schedule.playout_ids.clear();
                schedule.alternate_ids.clear();
                Ok(schedule)
            })
            .collect()
    }

    async fn upsert(&self, schedule: &ProgramSchedule) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query("INSERT OR REPLACE INTO program_schedules (id, name, data) VALUES (?, ?, ?)")
            .bind(schedule.id)
            .bind(&schedule.name)
            .bind(to_json(schedule)?)
            .execute(&mut *tx)
            .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM program_schedule_items WHERE program_schedule_id = ?",
            "INSERT OR REPLACE INTO program_schedule_items (program_schedule_id, id, sort_order, data) VALUES (?, ?, 0, '{}')",
            schedule.id,
            &schedule.item_ids,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
