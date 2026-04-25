use std::collections::BTreeSet;

use crate::db::{Db, DbRow};
use async_trait::async_trait;
use ersatzrs_core::repositories::{ProgramScheduleRepository, Result};
use ersatzrs_domain::{EntityId, ProgramSchedule};
use sqlx::{Pool, Row, Transaction};

use super::common::{decode_enum_column, encode_enum, load_ids, not_found};

#[derive(Clone)]
pub struct SqliteProgramScheduleRepository(pub Pool<crate::db::Db>);

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
        let result = sqlx::query(
            r#"UPDATE "ProgramSchedule"
               SET "Name" = ?,
                   "KeepMultiPartEpisodesTogether" = ?,
                   "TreatCollectionsAsShows" = ?,
                   "ShuffleScheduleItems" = ?,
                   "RandomStartPoint" = ?,
                   "FixedStartTimeBehavior" = ?
               WHERE "Id" = ?"#,
        )
        .bind(&schedule.name)
        .bind(schedule.keep_multi_part_episodes_together)
        .bind(schedule.treat_collections_as_shows)
        .bind(schedule.shuffle_schedule_items)
        .bind(schedule.random_start_point)
        .bind(encode_enum(schedule.fixed_start_time_behavior))
        .bind(schedule.id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            sqlx::query(
                r#"INSERT INTO "ProgramSchedule" (
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
        }
        sync_program_schedule_item_ids(&mut tx, schedule.id, &schedule.item_ids).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn sync_program_schedule_item_ids(
    tx: &mut Transaction<'_, Db>,
    schedule_id: EntityId,
    item_ids: &[EntityId],
) -> Result<()> {
    let existing = load_ids_in_tx(
        tx,
        r#"SELECT "Id" FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ? ORDER BY "Index", "Id""#,
        schedule_id,
    )
    .await?;
    let desired = item_ids.iter().copied().collect::<BTreeSet<_>>();

    for id in existing.iter().copied().filter(|id| !desired.contains(id)) {
        sqlx::query(r#"DELETE FROM "ProgramScheduleItem" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }

    let existing = existing.into_iter().collect::<BTreeSet<_>>();
    for (index, id) in item_ids.iter().copied().enumerate() {
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        if existing.contains(&id) {
            sqlx::query(
                r#"UPDATE "ProgramScheduleItem"
                   SET "ProgramScheduleId" = ?, "Index" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(schedule_id)
            .bind(index)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        } else {
            sqlx::query(
                r#"INSERT INTO "ProgramScheduleItem" (
                    "ProgramScheduleId", "Id", "CollectionType", "PlaybackOrder",
                    "FillWithGroupMode", "GuideMode", "MarathonGroupBy",
                    "MarathonShuffleGroups", "MarathonShuffleItems", "Index"
                 ) VALUES (?, ?, 0, 0, 0, 0, 0, 0, 0, ?)"#,
            )
            .bind(schedule_id)
            .bind(id)
            .bind(index)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}

async fn load_ids_in_tx(
    tx: &mut Transaction<'_, Db>,
    sql: &str,
    id: EntityId,
) -> Result<Vec<EntityId>> {
    let rows = sqlx::query(sql).bind(id).fetch_all(&mut **tx).await?;
    rows.iter()
        .map(|row| row.try_get("Id").map_err(Into::into))
        .collect()
}

fn map_program_schedule(row: &DbRow) -> Result<ProgramSchedule> {
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
