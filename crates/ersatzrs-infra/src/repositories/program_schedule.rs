use async_trait::async_trait;
use ersatzrs_core::repositories::{ProgramScheduleRepository, Result};
use ersatzrs_domain::{EntityId, ProgramSchedule};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum_column, encode_enum, load_ids, not_found, replace_ids};

#[derive(Clone)]
pub struct SqliteProgramScheduleRepository(pub Pool<Sqlite>);

#[async_trait]
impl ProgramScheduleRepository for SqliteProgramScheduleRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<ProgramSchedule> {
        let row = sqlx::query(r#"SELECT * FROM "ProgramSchedule" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("program schedule", id, error))?;
        let mut schedule = map_program_schedule(&row)?;
        schedule.item_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ? ORDER BY "Index", "Id""#,
            id,
        )
        .await?;
        schedule.alternate_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "ProgramScheduleAlternate" WHERE "ProgramScheduleId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        schedule.playout_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "Playout" WHERE "ProgramScheduleId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        Ok(schedule)
    }

    async fn list_for_playout(&self, playout_id: EntityId) -> Result<Vec<ProgramSchedule>> {
        let rows = sqlx::query(
            r#"SELECT "ps".*
             FROM "ProgramSchedule" AS "ps"
             JOIN "Playout" AS "p" ON "p"."ProgramScheduleId" = "ps"."Id"
             WHERE "p"."Id" = ?
             ORDER BY "ps"."Name", "ps"."Id""#,
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
            r#"INSERT OR REPLACE INTO "ProgramSchedule" (
                "Id", "Name", "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
             ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(schedule.id)
        .bind(&schedule.name)
        .bind(schedule.keep_multi_part_episodes_together)
        .bind(schedule.treat_collections_as_shows)
        .bind(schedule.shuffle_schedule_items)
        .bind(schedule.random_start_point)
        .bind(encode_enum(schedule.fixed_start_time_behavior))
        .execute(&mut *tx)
        .await?;
        replace_ids(
            &mut tx,
            r#"DELETE FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ?"#,
            r#"INSERT OR REPLACE INTO "ProgramScheduleItem" (
                "ProgramScheduleId", "Id", "CollectionType", "PlaybackOrder",
                "FillWithGroupMode", "GuideMode", "MarathonGroupBy", "MarathonShuffleGroups",
                "MarathonShuffleItems", "Index"
             ) VALUES (?, ?, 0, 0, 0, 0, 0, 0, 0, 0)"#,
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
        id: row.try_get("Id")?,
        name: row.try_get("Name")?,
        keep_multi_part_episodes_together: row.try_get("KeepMultiPartEpisodesTogether")?,
        treat_collections_as_shows: row.try_get("TreatCollectionsAsShows")?,
        shuffle_schedule_items: row.try_get("ShuffleScheduleItems")?,
        random_start_point: row.try_get("RandomStartPoint")?,
        fixed_start_time_behavior: decode_enum_column(
            "program schedule fixed_start_time_behavior",
            row,
            "FixedStartTimeBehavior",
        )?,
        item_ids: Vec::new(),
        playout_ids: Vec::new(),
        alternate_ids: Vec::new(),
    })
}
