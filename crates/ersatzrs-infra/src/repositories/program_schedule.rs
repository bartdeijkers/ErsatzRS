mod deferred;

use crate::repositories::SqlxResultExt as _;
use std::collections::BTreeSet;

use crate::db::{Db, DbRow};
use async_trait::async_trait;
use ersatzrs_core::repositories::{
    ProgramScheduleRepository, Result, ScheduleAdminForm, ScheduleAdminRow, ScheduleFillerOption,
    ScheduleItemAdmin, ScheduleItemOption, ScheduleItemOptions, ScheduleLanguageOption,
    SchedulePlaylistOption,
};
use ersatzrs_core::scheduling::{
    legacy_program_schedule_cooldown_config_key, program_schedule_item_smart_shuffle_config_key,
    SchedulingSmartShufflePolicy,
};
use ersatzrs_domain::{
    EntityId, FixedStartTimeBehavior, PlayoutMode, ProgramSchedule, ProgramScheduleAlternate,
    ProgramScheduleItem,
};
use sqlx::{Pool, Row, Transaction};

use super::common::{
    decode_bool_column, decode_enum_column, decode_optional_duration_text,
    decode_optional_enum_column, encode_duration, encode_enum, load_ids, not_found, replace_ids,
};

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
        .await
        .repository()?;
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
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
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
        .await
        .repository()?;
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
            .await
            .repository()?;
        }
        sync_program_schedule_item_ids(&mut tx, schedule.id, &schedule.item_ids).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn get_item(&self, id: EntityId) -> Result<ProgramScheduleItem> {
        let row = sqlx::query(crate::db::assert_sql_safe(format!(
            "{ITEM_SELECT_SQL} WHERE psi.\"Id\" = ?"
        )))
        .bind(id)
        .fetch_one(&self.0)
        .await
        .map_err(|error| not_found("program schedule item", id, error))?;
        map_program_schedule_item(&self.0, &row).await
    }

    async fn list_items(&self, schedule_id: EntityId) -> Result<Vec<ProgramScheduleItem>> {
        let rows = sqlx::query(crate::db::assert_sql_safe(format!(
            "{ITEM_SELECT_SQL} WHERE psi.\"ProgramScheduleId\" = ? ORDER BY psi.\"Index\", psi.\"Id\""
        )))
        .bind(schedule_id)
        .fetch_all(&self.0)
        .await.repository()?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(map_program_schedule_item(&self.0, &row).await?);
        }
        Ok(items)
    }

    async fn upsert_item(&self, item: &ProgramScheduleItem) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        upsert_program_schedule_item(&mut tx, item).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn delete_item(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "ProgramScheduleItem" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        Ok(())
    }

    async fn list_alternates(
        &self,
        schedule_id: EntityId,
    ) -> Result<Vec<ProgramScheduleAlternate>> {
        let rows = sqlx::query(
            r#"SELECT *
             FROM "ProgramScheduleAlternate"
             WHERE "ProgramScheduleId" = ?
             ORDER BY "Index", "Id""#,
        )
        .bind(schedule_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter().map(map_program_schedule_alternate).collect()
    }

    async fn upsert_alternate(&self, alternate: &ProgramScheduleAlternate) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        upsert_program_schedule_alternate(&mut tx, alternate).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn delete_alternate(&self, id: EntityId) -> Result<()> {
        sqlx::query(r#"DELETE FROM "ProgramScheduleAlternate" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        Ok(())
    }

    async fn list_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<ScheduleAdminRow>, u64)> {
        let search = search.trim();
        let like = format!("%{search}%");
        let total: i64 = if search.is_empty() {
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ProgramSchedule""#)
                .fetch_one(&self.0)
                .await
                .repository()?
        } else {
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ProgramSchedule" WHERE "Name" LIKE ?"#)
                .bind(&like)
                .fetch_one(&self.0)
                .await
                .repository()?
        };
        let offset = i64::from(page) * i64::from(page_size);
        let rows = if search.is_empty() {
            sqlx::query(
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "ProgramSchedule" ORDER BY "Name", "Id" LIMIT ? OFFSET ?"#,
            )
            .bind(i64::from(page_size))
            .bind(offset)
            .fetch_all(&self.0)
            .await
            .repository()?
        } else {
            sqlx::query(
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "ProgramSchedule" WHERE "Name" LIKE ?
                   ORDER BY "Name", "Id" LIMIT ? OFFSET ?"#,
            )
            .bind(&like)
            .bind(i64::from(page_size))
            .bind(offset)
            .fetch_all(&self.0)
            .await
            .repository()?
        };
        let rows = rows
            .iter()
            .map(|row| {
                Ok(ScheduleAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((
            rows,
            u64::try_from(total).map_err(|_| {
                ersatzrs_core::repositories::RepositoryError::Decode {
                    kind: "schedule count",
                    value: total.to_string(),
                }
            })?,
        ))
    }

    async fn get_admin(&self, id: EntityId) -> Result<Option<ScheduleAdminForm>> {
        let Some(row) = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                      "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                      "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
               FROM "ProgramSchedule" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(None);
        };
        Ok(Some(ScheduleAdminForm {
            id: row.try_get("Id").repository()?,
            name: row.try_get("Name").repository()?,
            keep_multi_part_episodes_together: decode_bool_column(
                &row,
                "KeepMultiPartEpisodesTogether",
            )?,
            treat_collections_as_shows: decode_bool_column(&row, "TreatCollectionsAsShows")?,
            shuffle_schedule_items: decode_bool_column(&row, "ShuffleScheduleItems")?,
            random_start_point: decode_bool_column(&row, "RandomStartPoint")?,
            fixed_start_time_behavior: row.try_get("FixedStartTimeBehavior").repository()?,
        }))
    }

    async fn admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ProgramSchedule"
               WHERE LOWER("Name") = LOWER(?) AND "Id" <> ?"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn save_admin(&self, form: &ScheduleAdminForm) -> Result<EntityId> {
        if form.id == 0 {
            let result = sqlx::query(
                r#"INSERT INTO "ProgramSchedule" (
                       "Name", "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                       "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
                   ) VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(&form.name)
            .bind(form.keep_multi_part_episodes_together)
            .bind(form.treat_collections_as_shows)
            .bind(form.shuffle_schedule_items)
            .bind(form.random_start_point)
            .bind(form.fixed_start_time_behavior)
            .execute(&self.0)
            .await
            .repository()?;
            Ok(crate::db::last_insert_id(result))
        } else {
            let result = sqlx::query(
                r#"UPDATE "ProgramSchedule"
                   SET "Name" = ?, "KeepMultiPartEpisodesTogether" = ?,
                       "TreatCollectionsAsShows" = ?, "ShuffleScheduleItems" = ?,
                       "RandomStartPoint" = ?, "FixedStartTimeBehavior" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(&form.name)
            .bind(form.keep_multi_part_episodes_together)
            .bind(form.treat_collections_as_shows)
            .bind(form.shuffle_schedule_items)
            .bind(form.random_start_point)
            .bind(form.fixed_start_time_behavior)
            .bind(form.id)
            .execute(&self.0)
            .await
            .repository()?;
            if result.rows_affected() == 0 {
                return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                    kind: "program schedule",
                    id: form.id,
                });
            }
            Ok(form.id)
        }
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let item_ids: Vec<EntityId> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ?"#,
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        for item_id in item_ids {
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(program_schedule_item_smart_shuffle_config_key(item_id))
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(legacy_program_schedule_cooldown_config_key(id))
            .execute(&mut *tx)
            .await
            .repository()?;
        crate::classic_deferred::clear_schedule(&mut tx, id).await?;
        // Deleting the primary schedule cascades its playouts, including any
        // receipts belonging to their alternate schedules.
        let deleted_playouts: Vec<EntityId> =
            sqlx::query_scalar(r#"SELECT "Id" FROM "Playout" WHERE "ProgramScheduleId"=?"#)
                .bind(id)
                .fetch_all(&mut *tx)
                .await
                .repository()?;
        for playout_id in deleted_playouts {
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
                .bind(crate::classic_deferred::key(playout_id))
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        sqlx::query(r#"DELETE FROM "ProgramSchedule" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
        tx.commit().await.repository()
    }

    async fn copy_admin(&self, id: EntityId, name: &str) -> Result<EntityId> {
        copy_schedule_admin(&self.0, id, name).await
    }

    async fn playout_ids_using_admin(&self, id: EntityId) -> Result<Vec<EntityId>> {
        crate::classic_deferred::playout_ids(&self.0, id).await
    }

    async fn list_items_admin(&self, id: EntityId) -> Result<Vec<ScheduleItemAdmin>> {
        list_schedule_items_admin(&self.0, id).await
    }

    async fn get_item_options_admin(&self) -> Result<ScheduleItemOptions> {
        load_schedule_item_options_admin(&self.0).await
    }

    async fn replace_items_admin(
        &self,
        schedule_id: EntityId,
        items: &[ScheduleItemAdmin],
    ) -> Result<()> {
        replace_schedule_items_admin(&self.0, schedule_id, items).await
    }

    async fn defer_items_admin(
        &self,
        schedule_id: EntityId,
        items: &[ScheduleItemAdmin],
        locked_playouts: &[EntityId],
    ) -> Result<Vec<ersatzrs_core::repositories::ScheduleDeferredChange>> {
        deferred::save(&self.0, schedule_id, items, locked_playouts).await
    }

    async fn deferred_changes_admin(
        &self,
        schedule_id: EntityId,
    ) -> Result<Vec<ersatzrs_core::repositories::ScheduleDeferredChange>> {
        crate::classic_deferred::list(&self.0, schedule_id).await
    }

    async fn append_one_item_admin(
        &self,
        schedule_id: EntityId,
        collection_type: i32,
        media_item_id: EntityId,
        pinned_stream_selection: Option<ersatzrs_domain::PinnedStreamSelection>,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let next_index = sqlx::query_scalar::<_, i64>(
            r#"SELECT COALESCE(MAX("Index"), -1) + 1
               FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ?"#,
        )
        .bind(schedule_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "ProgramScheduleItem" (
                   "ProgramScheduleId", "CollectionType", "MediaItemId",
                   "PlaybackOrder", "FillWithGroupMode", "GuideMode",
                   "MarathonGroupBy", "MarathonShuffleGroups", "MarathonShuffleItems",
                   "Index", "MediaVersionId", "VideoStreamIndex",
                   "AudioStreamIndex", "SubtitleStreamIndex"
               ) VALUES (?, ?, ?, 3, 0, 0, 0, 0, 0, ?, ?, ?, ?, ?)"#,
        )
        .bind(schedule_id)
        .bind(collection_type)
        .bind(media_item_id)
        .bind(next_index)
        .bind(pinned_stream_selection.map(|selection| selection.media_version_id))
        .bind(pinned_stream_selection.map(|selection| selection.video_stream_index))
        .bind(pinned_stream_selection.and_then(|selection| selection.audio_stream_index))
        .bind(pinned_stream_selection.and_then(|selection| selection.subtitle_stream_index))
        .execute(&mut *tx)
        .await
        .repository()?;
        let item_id = crate::db::last_insert_id(result);
        sqlx::query(r#"INSERT INTO "ProgramScheduleOneItem" ("Id") VALUES (?)"#)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        tx.commit().await.repository()
    }
}

async fn copy_schedule_admin(pool: &Pool<Db>, id: EntityId, name: &str) -> Result<EntityId> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let result = sqlx::query(
        r#"INSERT INTO "ProgramSchedule" (
               "Name", "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
               "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
           )
           SELECT ?, "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                  "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
           FROM "ProgramSchedule" WHERE "Id" = ?"#,
    )
    .bind(name)
    .bind(id)
    .execute(&mut *tx)
    .await
    .repository()?;
    if result.rows_affected() == 0 {
        return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
            kind: "program schedule",
            id,
        });
    }
    let new_schedule_id = crate::db::last_insert_id(result);
    let legacy_value =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(legacy_program_schedule_cooldown_config_key(id))
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
    let old_item_ids: Vec<EntityId> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "ProgramScheduleItem"
           WHERE "ProgramScheduleId" = ? ORDER BY "Index", "Id""#,
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .repository()?;
    for old_item_id in old_item_ids {
        let item_result = sqlx::query(
            r#"INSERT INTO "ProgramScheduleItem" (
                   "CollectionId", "CollectionType", "CustomTitle", "FakeCollectionKey",
                   "FallbackFillerId", "FillWithGroupMode", "FixedStartTimeBehavior",
                   "GuideMode", "Index", "MarathonBatchSize", "MarathonGroupBy",
                   "MarathonShuffleGroups", "MarathonShuffleItems", "MediaItemId",
                   "MidRollFillerId", "MultiCollectionId", "PlaybackOrder", "PlaylistId",
                   "PostRollFillerId", "PreRollFillerId", "PreferredAudioLanguageCode",
                   "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "ProgramScheduleId",
                   "RerunCollectionId", "SmartCollectionId", "StartTime", "SubtitleMode",
                   "TailFillerId", "SearchQuery", "SearchTitle", "MediaVersionId",
                   "VideoStreamIndex", "AudioStreamIndex", "SubtitleStreamIndex"
               )
               SELECT "CollectionId", "CollectionType", "CustomTitle", "FakeCollectionKey",
                      "FallbackFillerId", "FillWithGroupMode", "FixedStartTimeBehavior",
                      "GuideMode", "Index", "MarathonBatchSize", "MarathonGroupBy",
                      "MarathonShuffleGroups", "MarathonShuffleItems", "MediaItemId",
                      "MidRollFillerId", "MultiCollectionId", "PlaybackOrder", "PlaylistId",
                      "PostRollFillerId", "PreRollFillerId", "PreferredAudioLanguageCode",
                      "PreferredAudioTitle", "PreferredSubtitleLanguageCode", ?,
                      "RerunCollectionId", "SmartCollectionId", "StartTime", "SubtitleMode",
                      "TailFillerId", "SearchQuery", "SearchTitle", "MediaVersionId",
                      "VideoStreamIndex", "AudioStreamIndex", "SubtitleStreamIndex"
               FROM "ProgramScheduleItem" WHERE "Id" = ?"#,
        )
        .bind(new_schedule_id)
        .bind(old_item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        let new_item_id = crate::db::last_insert_id(item_result);
        copy_schedule_item_relations(&mut tx, old_item_id, new_item_id).await?;
        let policy_value = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(program_schedule_item_smart_shuffle_config_key(old_item_id))
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        .or_else(|| legacy_value.clone());
        if let Some(value) = policy_value {
            sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
                .bind(program_schedule_item_smart_shuffle_config_key(new_item_id))
                .bind(value)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
    }
    tx.commit().await.repository()?;
    Ok(new_schedule_id)
}

async fn copy_schedule_item_relations(
    tx: &mut Transaction<'_, Db>,
    old_item_id: EntityId,
    new_item_id: EntityId,
) -> Result<()> {
    for sql in [
        r#"INSERT INTO "ProgramScheduleDurationItem"
               ("Id", "PlayoutDuration", "TailMode", "DiscardToFillAttempts")
           SELECT ?, "PlayoutDuration", "TailMode", "DiscardToFillAttempts"
           FROM "ProgramScheduleDurationItem" WHERE "Id" = ?"#,
        r#"INSERT INTO "ProgramScheduleMultipleItem" ("Id", "Count", "MultipleMode")
           SELECT ?, "Count", "MultipleMode"
           FROM "ProgramScheduleMultipleItem" WHERE "Id" = ?"#,
        r#"INSERT INTO "ProgramScheduleOneItem" ("Id")
           SELECT ? FROM "ProgramScheduleOneItem" WHERE "Id" = ?"#,
        r#"INSERT INTO "ProgramScheduleFloodItem" ("Id")
           SELECT ? FROM "ProgramScheduleFloodItem" WHERE "Id" = ?"#,
        r#"INSERT INTO "ProgramScheduleItemWatermark"
               ("ProgramScheduleItemId", "WatermarkId")
           SELECT ?, "WatermarkId" FROM "ProgramScheduleItemWatermark"
           WHERE "ProgramScheduleItemId" = ?"#,
        r#"INSERT INTO "ProgramScheduleItemGraphicsElement"
               ("ProgramScheduleItemId", "GraphicsElementId")
           SELECT ?, "GraphicsElementId" FROM "ProgramScheduleItemGraphicsElement"
           WHERE "ProgramScheduleItemId" = ?"#,
    ] {
        sqlx::query(sql)
            .bind(new_item_id)
            .bind(old_item_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}

async fn list_schedule_items_admin(
    pool: &Pool<Db>,
    schedule_id: EntityId,
) -> Result<Vec<ScheduleItemAdmin>> {
    let mut connection = pool.acquire().await.repository()?;
    list_schedule_items_admin_on_connection(&mut connection, schedule_id).await
}

#[allow(clippy::too_many_lines)]
async fn list_schedule_items_admin_on_connection(
    connection: &mut sqlx::SqliteConnection,
    schedule_id: EntityId,
) -> Result<Vec<ScheduleItemAdmin>> {
    let rows = sqlx::query(
        r#"SELECT "psi"."Id", "psi"."Index", "psi"."StartTime",
                  "psi"."FixedStartTimeBehavior", "psi"."CollectionType",
                  "psi"."PlaybackOrder", "psi"."MarathonGroupBy",
                  "psi"."MarathonShuffleGroups", "psi"."MarathonShuffleItems",
                  "psi"."MarathonBatchSize", "psi"."FillWithGroupMode",
                  "psi"."CustomTitle", "psi"."GuideMode", "psi"."PreRollFillerId",
                  "psi"."MidRollFillerId", "psi"."PostRollFillerId",
                  "psi"."TailFillerId", "psi"."FallbackFillerId",
                  "psi"."PreferredAudioLanguageCode", "psi"."PreferredAudioTitle",
                  "psi"."PreferredSubtitleLanguageCode", "psi"."SubtitleMode",
                  "psi"."MediaVersionId", "psi"."VideoStreamIndex",
                  "psi"."AudioStreamIndex", "psi"."SubtitleStreamIndex",
                  "psi"."CollectionId", "psi"."MediaItemId", "psi"."MultiCollectionId",
                  "psi"."SmartCollectionId", "psi"."RerunCollectionId",
                  "psi"."PlaylistId", "psi"."SearchQuery", "psi"."SearchTitle",
                  "c"."Name" AS "CollectionName", "mc"."Name" AS "MultiCollectionName",
                  "sc"."Name" AS "SmartCollectionName", "rc"."Name" AS "RerunCollectionName",
                  "p"."Name" AS "PlaylistName", "am"."Title" AS "ArtistName",
                  "showm"."Title" AS "ShowName", "seasonm"."Title" AS "SeasonName",
                  "season"."SeasonNumber", "psdi"."Id" AS "DurationItemId",
                  "psdi"."PlayoutDuration", "psdi"."TailMode",
                  "psdi"."DiscardToFillAttempts", "psmi"."Id" AS "MultipleItemId",
                  "psmi"."Count", "psmi"."MultipleMode", "psoi"."Id" AS "OneItemId",
                  "psfi"."Id" AS "FloodItemId",
                  (SELECT GROUP_CONCAT("psiw"."WatermarkId")
                   FROM "ProgramScheduleItemWatermark" AS "psiw"
                   WHERE "psiw"."ProgramScheduleItemId" = "psi"."Id") AS "WatermarkIds",
                  (SELECT GROUP_CONCAT("psige"."GraphicsElementId")
                   FROM "ProgramScheduleItemGraphicsElement" AS "psige"
                   WHERE "psige"."ProgramScheduleItemId" = "psi"."Id") AS "GraphicsElementIds"
           FROM "ProgramScheduleItem" AS "psi"
           LEFT JOIN "Collection" AS "c" ON "c"."Id" = "psi"."CollectionId"
           LEFT JOIN "MultiCollection" AS "mc" ON "mc"."Id" = "psi"."MultiCollectionId"
           LEFT JOIN "SmartCollection" AS "sc" ON "sc"."Id" = "psi"."SmartCollectionId"
           LEFT JOIN "RerunCollection" AS "rc" ON "rc"."Id" = "psi"."RerunCollectionId"
           LEFT JOIN "Playlist" AS "p" ON "p"."Id" = "psi"."PlaylistId"
           LEFT JOIN "Artist" AS "a"
             ON "a"."Id" = "psi"."MediaItemId" AND "psi"."CollectionType" = 3
           LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "a"."Id"
           LEFT JOIN "Show" AS "show_item"
             ON "show_item"."Id" = "psi"."MediaItemId" AND "psi"."CollectionType" = 1
           LEFT JOIN "ShowMetadata" AS "showm" ON "showm"."ShowId" = "show_item"."Id"
           LEFT JOIN "Season" AS "season"
             ON "season"."Id" = "psi"."MediaItemId" AND "psi"."CollectionType" = 2
           LEFT JOIN "SeasonMetadata" AS "seasonm" ON "seasonm"."SeasonId" = "season"."Id"
           LEFT JOIN "ProgramScheduleDurationItem" AS "psdi" ON "psdi"."Id" = "psi"."Id"
           LEFT JOIN "ProgramScheduleMultipleItem" AS "psmi" ON "psmi"."Id" = "psi"."Id"
           LEFT JOIN "ProgramScheduleOneItem" AS "psoi" ON "psoi"."Id" = "psi"."Id"
           LEFT JOIN "ProgramScheduleFloodItem" AS "psfi" ON "psfi"."Id" = "psi"."Id"
           WHERE "psi"."ProgramScheduleId" = ?
           ORDER BY "psi"."Index", "psi"."Id""#,
    )
    .bind(schedule_id)
    .fetch_all(&mut *connection)
    .await
    .repository()?;
    let legacy_policy = load_schedule_policy(
        &mut *connection,
        &legacy_program_schedule_cooldown_config_key(schedule_id),
    )
    .await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        let id: EntityId = row.try_get("Id").repository()?;
        let explicit_policy = load_schedule_policy(
            &mut *connection,
            &program_schedule_item_smart_shuffle_config_key(id),
        )
        .await?;
        let inherited_legacy_smart_shuffle = explicit_policy.is_none() && legacy_policy.is_some();
        let smart_shuffle = explicit_policy
            .or_else(|| legacy_policy.clone())
            .unwrap_or_default();
        items.push(map_schedule_item_admin(
            row,
            smart_shuffle,
            inherited_legacy_smart_shuffle,
        )?);
    }
    Ok(items)
}

#[allow(clippy::too_many_lines)]
fn map_schedule_item_admin(
    row: &DbRow,
    smart_shuffle: SchedulingSmartShufflePolicy,
    inherited_legacy_smart_shuffle: bool,
) -> Result<ScheduleItemAdmin> {
    let id = row.try_get("Id").repository()?;
    let start_time: Option<String> = row.try_get("StartTime").repository()?;
    let duration = row
        .try_get::<Option<String>, _>("PlayoutDuration")
        .repository()?;
    let (duration_hours, duration_minutes) = duration
        .as_deref()
        .map(parse_schedule_duration_parts)
        .transpose()?
        .unwrap_or((1, 0));
    let playout_mode = if row
        .try_get::<Option<EntityId>, _>("FloodItemId")
        .repository()?
        .is_some()
    {
        1
    } else if row
        .try_get::<Option<EntityId>, _>("OneItemId")
        .repository()?
        .is_some()
    {
        2
    } else if row
        .try_get::<Option<EntityId>, _>("MultipleItemId")
        .repository()?
        .is_some()
    {
        3
    } else {
        4
    };
    Ok(ScheduleItemAdmin {
        id,
        index: row.try_get("Index").repository()?,
        start_type: if start_time.is_some() { 2 } else { 1 },
        start_time,
        fixed_start_time_behavior: row.try_get("FixedStartTimeBehavior").repository()?,
        collection_type: row.try_get("CollectionType").repository()?,
        collection_id: row.try_get("CollectionId").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        multi_collection_id: row.try_get("MultiCollectionId").repository()?,
        smart_collection_id: row.try_get("SmartCollectionId").repository()?,
        rerun_collection_id: row.try_get("RerunCollectionId").repository()?,
        playlist_id: row.try_get("PlaylistId").repository()?,
        search_title: row.try_get("SearchTitle").repository()?,
        search_query: row.try_get("SearchQuery").repository()?,
        collection_name: schedule_item_collection_name(row, id)?,
        playback_order: row.try_get("PlaybackOrder").repository()?,
        smart_shuffle,
        inherited_legacy_smart_shuffle,
        marathon_group_by: row.try_get("MarathonGroupBy").repository()?,
        marathon_shuffle_groups: decode_bool_column(row, "MarathonShuffleGroups")?,
        marathon_shuffle_items: decode_bool_column(row, "MarathonShuffleItems")?,
        marathon_batch_size: row.try_get("MarathonBatchSize").repository()?,
        fill_with_group_mode: row.try_get("FillWithGroupMode").repository()?,
        playout_mode,
        multiple_mode: row
            .try_get::<Option<i32>, _>("MultipleMode")
            .repository()?
            .unwrap_or_default(),
        multiple_count: row
            .try_get::<Option<String>, _>("Count")
            .repository()?
            .unwrap_or_else(|| "1".to_owned()),
        duration_hours,
        duration_minutes,
        tail_mode: row
            .try_get::<Option<i32>, _>("TailMode")
            .repository()?
            .unwrap_or_default(),
        discard_to_fill_attempts: row.try_get("DiscardToFillAttempts").repository()?,
        custom_title: row
            .try_get::<Option<String>, _>("CustomTitle")
            .repository()?
            .unwrap_or_default(),
        guide_mode: row.try_get("GuideMode").repository()?,
        pre_roll_filler_id: row.try_get("PreRollFillerId").repository()?,
        mid_roll_filler_id: row.try_get("MidRollFillerId").repository()?,
        post_roll_filler_id: row.try_get("PostRollFillerId").repository()?,
        tail_filler_id: row.try_get("TailFillerId").repository()?,
        fallback_filler_id: row.try_get("FallbackFillerId").repository()?,
        watermark_ids: decode_schedule_id_list(
            row.try_get::<Option<String>, _>("WatermarkIds")
                .repository()?
                .as_deref(),
        )?,
        graphics_element_ids: decode_schedule_id_list(
            row.try_get::<Option<String>, _>("GraphicsElementIds")
                .repository()?
                .as_deref(),
        )?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode").repository()?,
        preferred_audio_title: row
            .try_get::<Option<String>, _>("PreferredAudioTitle")
            .repository()?
            .unwrap_or_default(),
        preferred_subtitle_language_code: row
            .try_get("PreferredSubtitleLanguageCode")
            .repository()?,
        subtitle_mode: row.try_get("SubtitleMode").repository()?,
        pinned_stream_selection: decode_schedule_pinned_stream_selection(row)?,
    })
}

fn decode_schedule_pinned_stream_selection(
    row: &DbRow,
) -> Result<Option<ersatzrs_domain::PinnedStreamSelection>> {
    let media_version_id: Option<EntityId> = row.try_get("MediaVersionId").repository()?;
    let video_stream_index: Option<i32> = row.try_get("VideoStreamIndex").repository()?;
    match (media_version_id, video_stream_index) {
        (None, None) => Ok(None),
        (Some(media_version_id), Some(video_stream_index)) => {
            Ok(Some(ersatzrs_domain::PinnedStreamSelection {
                media_version_id,
                video_stream_index,
                audio_stream_index: row.try_get("AudioStreamIndex").repository()?,
                subtitle_stream_index: row.try_get("SubtitleStreamIndex").repository()?,
            }))
        }
        _ => Err(ersatzrs_core::repositories::RepositoryError::Decode {
            kind: "program schedule pinned stream selection",
            value: "media version and video stream must both be set".to_owned(),
        }),
    }
}

async fn load_schedule_policy<'e>(
    pool: impl sqlx::Executor<'e, Database = Db>,
    key: &str,
) -> Result<Option<SchedulingSmartShufflePolicy>> {
    sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .fetch_optional(pool)
        .await
        .repository()?
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                ersatzrs_core::repositories::RepositoryError::Decode {
                    kind: "schedule smart shuffle policy",
                    value: error.to_string(),
                }
            })
        })
        .transpose()
}

fn parse_schedule_duration_parts(value: &str) -> Result<(i32, i32)> {
    let duration = super::common::try_decode_duration_text(value)?;
    let hours = i32::try_from(duration.num_hours()).map_err(|_| {
        ersatzrs_core::repositories::RepositoryError::Decode {
            kind: "schedule duration",
            value: value.to_owned(),
        }
    })?;
    let minutes = i32::try_from(duration.num_minutes() % 60).map_err(|_| {
        ersatzrs_core::repositories::RepositoryError::Decode {
            kind: "schedule duration",
            value: value.to_owned(),
        }
    })?;
    Ok((hours, minutes))
}

fn decode_schedule_id_list(value: Option<&str>) -> Result<Vec<EntityId>> {
    value
        .unwrap_or_default()
        .split(',')
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<EntityId>().map_err(|error| {
                ersatzrs_core::repositories::RepositoryError::Decode {
                    kind: "schedule relation id",
                    value: error.to_string(),
                }
            })
        })
        .collect()
}

fn schedule_item_collection_name(row: &DbRow, id: EntityId) -> Result<String> {
    for column in [
        "CollectionName",
        "MultiCollectionName",
        "SmartCollectionName",
        "RerunCollectionName",
        "PlaylistName",
        "ArtistName",
        "ShowName",
        "SeasonName",
    ] {
        if let Some(value) = row.try_get::<Option<String>, _>(column).repository()? {
            if !value.trim().is_empty() {
                return Ok(value);
            }
        }
    }
    for column in ["SearchTitle", "SearchQuery"] {
        if let Some(value) = row.try_get::<Option<String>, _>(column).repository()? {
            if !value.trim().is_empty() {
                return Ok(value);
            }
        }
    }
    if let Some(media_item_id) = row
        .try_get::<Option<EntityId>, _>("MediaItemId")
        .repository()?
    {
        return Ok(format!("Media Item #{media_item_id}"));
    }
    Ok(format!("Schedule Item #{id}"))
}

async fn load_schedule_item_options_admin(pool: &Pool<Db>) -> Result<ScheduleItemOptions> {
    let seasons_sql = r#"SELECT "s"."Id",
                                COALESCE("sm"."Title", 'Season ' || "s"."SeasonNumber") AS "Name"
                         FROM "Season" AS "s"
                         LEFT JOIN "SeasonMetadata" AS "sm" ON "sm"."SeasonId" = "s"."Id"
                         ORDER BY "sm"."Title", "s"."SeasonNumber", "s"."Id""#;
    Ok(ScheduleItemOptions {
        collections: load_schedule_simple_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "Collection" ORDER BY "Name""#,
        )
        .await?,
        shows: load_schedule_simple_options(
            pool,
            r#"SELECT "sh"."Id", COALESCE("sm"."Title", '') AS "Name"
               FROM "Show" AS "sh"
               LEFT JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "sh"."Id"
               ORDER BY "sm"."Title", "sh"."Id""#,
        )
        .await?,
        seasons: load_schedule_simple_options(pool, seasons_sql).await?,
        artists: load_schedule_simple_options(
            pool,
            r#"SELECT "a"."Id", COALESCE("am"."Title", '') AS "Name"
               FROM "Artist" AS "a"
               LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "a"."Id"
               ORDER BY "am"."Title", "a"."Id""#,
        )
        .await?,
        multi_collections: load_schedule_simple_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "MultiCollection" ORDER BY "Name""#,
        )
        .await?,
        smart_collections: load_schedule_simple_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "SmartCollection" ORDER BY "Name""#,
        )
        .await?,
        rerun_collections: load_schedule_simple_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "RerunCollection" ORDER BY "Name""#,
        )
        .await?,
        playlist_groups: load_schedule_simple_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "PlaylistGroup" ORDER BY "Name""#,
        )
        .await?,
        playlists: load_schedule_playlist_options(pool).await?,
        fillers: load_schedule_filler_options(pool).await?,
        watermarks: load_schedule_simple_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "ChannelWatermark" ORDER BY "Name""#,
        )
        .await?,
        graphics_elements: load_schedule_simple_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", "Path", '') AS "Name"
               FROM "GraphicsElement" ORDER BY "Name", "Id""#,
        )
        .await?,
        languages: load_schedule_language_options(pool).await?,
    })
}

async fn load_schedule_simple_options(
    pool: &Pool<Db>,
    sql: &'static str,
) -> Result<Vec<ScheduleItemOption>> {
    let rows = sqlx::query(sql).fetch_all(pool).await.repository()?;
    rows.iter()
        .map(|row| {
            Ok(ScheduleItemOption {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .collect()
}

async fn load_schedule_playlist_options(pool: &Pool<Db>) -> Result<Vec<SchedulePlaylistOption>> {
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name", "PlaylistGroupId"
           FROM "Playlist" ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(SchedulePlaylistOption {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
                group_id: row.try_get("PlaylistGroupId").repository()?,
            })
        })
        .collect()
}

async fn load_schedule_filler_options(pool: &Pool<Db>) -> Result<Vec<ScheduleFillerOption>> {
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name", "FillerKind",
                  "PadToNearestMinute"
           FROM "FillerPreset" ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(ScheduleFillerOption {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
                filler_kind: row.try_get("FillerKind").repository()?,
                pad_to_nearest_minute: decode_bool_column(row, "PadToNearestMinute")?,
            })
        })
        .collect()
}

async fn load_schedule_language_options(pool: &Pool<Db>) -> Result<Vec<ScheduleLanguageOption>> {
    let rows = sqlx::query(
        r#"SELECT "LanguageCode"
           FROM (
               SELECT "Language" AS "LanguageCode" FROM "MediaStream"
               WHERE "Language" IS NOT NULL
               UNION ALL
               SELECT "Language" AS "LanguageCode" FROM "Subtitle"
               WHERE "Language" IS NOT NULL
               UNION ALL
               SELECT "PreferredAudioLanguageCode" AS "LanguageCode" FROM "Channel"
               WHERE "PreferredAudioLanguageCode" IS NOT NULL
           ) AS "codes"
           GROUP BY "LanguageCode"
           ORDER BY COUNT("LanguageCode") DESC, "LanguageCode""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;
    let mut seen = std::collections::BTreeSet::new();
    let mut options = Vec::new();
    for row in rows {
        let media_code: String = row.try_get("LanguageCode").repository()?;
        let media_code = media_code.trim();
        if media_code.is_empty() {
            continue;
        }
        let resolved = ersatzrs_core::language_codes::all_language_codes([media_code])
            .into_iter()
            .filter(|code| code.len() == 3)
            .find_map(|code| {
                ersatzrs_core::language_codes::english_name(&code).map(|name| (code, name))
            })
            .unwrap_or_else(|| (media_code.to_owned(), media_code.to_owned()));
        if seen.insert(resolved.0.clone()) {
            options.push(ScheduleLanguageOption {
                code: resolved.0,
                name: resolved.1,
            });
        }
    }
    Ok(options)
}

#[allow(clippy::too_many_lines)]
async fn replace_schedule_items_admin(
    pool: &Pool<Db>,
    schedule_id: EntityId,
    items: &[ScheduleItemAdmin],
) -> Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let old_item_ids: Vec<EntityId> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ?"#,
    )
    .bind(schedule_id)
    .fetch_all(&mut *tx)
    .await
    .repository()?;
    for old_item_id in old_item_ids {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(program_schedule_item_smart_shuffle_config_key(old_item_id))
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(r#"DELETE FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ?"#)
        .bind(schedule_id)
        .execute(&mut *tx)
        .await
        .repository()?;

    let mut ordered = items.to_vec();
    ordered.sort_by_key(|item| (item.index, item.id));
    for (index, item) in ordered.iter().enumerate() {
        let fixed = item.start_type == 2;
        let result = sqlx::query(
            r#"INSERT INTO "ProgramScheduleItem" (
                   "CollectionId", "CollectionType", "CustomTitle", "FakeCollectionKey",
                   "FallbackFillerId", "FillWithGroupMode", "FixedStartTimeBehavior",
                   "GuideMode", "Index", "MarathonBatchSize", "MarathonGroupBy",
                   "MarathonShuffleGroups", "MarathonShuffleItems", "MediaItemId",
                   "MidRollFillerId", "MultiCollectionId", "PlaybackOrder", "PlaylistId",
                   "PostRollFillerId", "PreRollFillerId", "PreferredAudioLanguageCode",
                   "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "ProgramScheduleId",
                   "RerunCollectionId", "SmartCollectionId", "StartTime", "SubtitleMode",
                   "TailFillerId", "SearchQuery", "SearchTitle", "MediaVersionId",
                   "VideoStreamIndex", "AudioStreamIndex", "SubtitleStreamIndex"
               ) VALUES (?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item.collection_id)
        .bind(item.collection_type)
        .bind(schedule_optional_text(&item.custom_title))
        .bind(item.fallback_filler_id)
        .bind(item.fill_with_group_mode)
        .bind(fixed.then_some(item.fixed_start_time_behavior).flatten())
        .bind(item.guide_mode)
        .bind(i32::try_from(index).map_err(|_| {
            ersatzrs_core::repositories::RepositoryError::Conflict(
                "schedule item count exceeds i32 range".to_owned(),
            )
        })?)
        .bind(item.marathon_batch_size)
        .bind(item.marathon_group_by)
        .bind(item.marathon_shuffle_groups)
        .bind(item.marathon_shuffle_items)
        .bind(item.media_item_id)
        .bind(item.mid_roll_filler_id)
        .bind(item.multi_collection_id)
        .bind(item.playback_order)
        .bind(item.playlist_id)
        .bind(item.post_roll_filler_id)
        .bind(item.pre_roll_filler_id)
        .bind(item.preferred_audio_language_code.as_deref())
        .bind(schedule_optional_text(&item.preferred_audio_title))
        .bind(item.preferred_subtitle_language_code.as_deref())
        .bind(schedule_id)
        .bind(item.rerun_collection_id)
        .bind(item.smart_collection_id)
        .bind(fixed.then_some(item.start_time.as_deref()).flatten())
        .bind(item.subtitle_mode)
        .bind(item.tail_filler_id)
        .bind(item.search_query.as_deref())
        .bind(item.search_title.as_deref())
        .bind(
            item.pinned_stream_selection
                .map(|selection| selection.media_version_id),
        )
        .bind(
            item.pinned_stream_selection
                .map(|selection| selection.video_stream_index),
        )
        .bind(
            item.pinned_stream_selection
                .and_then(|selection| selection.audio_stream_index),
        )
        .bind(
            item.pinned_stream_selection
                .and_then(|selection| selection.subtitle_stream_index),
        )
        .execute(&mut *tx)
        .await
        .repository()?;
        let new_item_id = crate::db::last_insert_id(result);
        persist_schedule_policy_tx(
            &mut tx,
            &program_schedule_item_smart_shuffle_config_key(new_item_id),
            &item.smart_shuffle,
        )
        .await?;
        match item.playout_mode {
            1 => {
                sqlx::query(r#"INSERT INTO "ProgramScheduleFloodItem" ("Id") VALUES (?)"#)
                    .bind(new_item_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            3 => {
                sqlx::query(
                    r#"INSERT INTO "ProgramScheduleMultipleItem" ("Id", "Count", "MultipleMode")
                       VALUES (?, ?, ?)"#,
                )
                .bind(new_item_id)
                .bind(&item.multiple_count)
                .bind(item.multiple_mode)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            4 => {
                sqlx::query(
                    r#"INSERT INTO "ProgramScheduleDurationItem"
                           ("Id", "PlayoutDuration", "TailMode", "DiscardToFillAttempts")
                       VALUES (?, ?, ?, ?)"#,
                )
                .bind(new_item_id)
                .bind(format!(
                    "{:02}:{:02}:00",
                    item.duration_hours.max(0),
                    item.duration_minutes.clamp(0, 59)
                ))
                .bind(item.tail_mode)
                .bind(item.discard_to_fill_attempts)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            _ => {
                sqlx::query(r#"INSERT INTO "ProgramScheduleOneItem" ("Id") VALUES (?)"#)
                    .bind(new_item_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
        }
        for watermark_id in &item.watermark_ids {
            sqlx::query(
                r#"INSERT INTO "ProgramScheduleItemWatermark"
                       ("ProgramScheduleItemId", "WatermarkId") VALUES (?, ?)"#,
            )
            .bind(new_item_id)
            .bind(watermark_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        for graphics_element_id in &item.graphics_element_ids {
            sqlx::query(
                r#"INSERT INTO "ProgramScheduleItemGraphicsElement"
                       ("ProgramScheduleItemId", "GraphicsElementId") VALUES (?, ?)"#,
            )
            .bind(new_item_id)
            .bind(graphics_element_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
    }
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(legacy_program_schedule_cooldown_config_key(schedule_id))
        .execute(&mut *tx)
        .await
        .repository()?;
    crate::classic_deferred::clear_schedule(&mut tx, schedule_id).await?;
    tx.commit().await.repository()
}

async fn persist_schedule_policy_tx(
    tx: &mut Transaction<'_, Db>,
    key: &str,
    policy: &SchedulingSmartShufflePolicy,
) -> Result<()> {
    if !policy.is_active() {
        return Ok(());
    }
    let value = serde_json::to_string(policy).map_err(|error| {
        ersatzrs_core::repositories::RepositoryError::Conflict(error.to_string())
    })?;
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(key)
        .bind(value)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(())
}

fn schedule_optional_text(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

const ITEM_SELECT_SQL: &str = r#"SELECT psi.*,
       psdi."Id" AS "DurationItemId",
       psdi."PlayoutDuration",
       psdi."TailMode",
       psmi."Id" AS "MultipleItemId",
       psmi."Count",
       psmi."MultipleMode",
       psoi."Id" AS "OneItemId",
       psfi."Id" AS "FloodItemId"
FROM "ProgramScheduleItem" AS psi
LEFT JOIN "ProgramScheduleDurationItem" AS psdi ON psdi."Id" = psi."Id"
LEFT JOIN "ProgramScheduleMultipleItem" AS psmi ON psmi."Id" = psi."Id"
LEFT JOIN "ProgramScheduleOneItem" AS psoi ON psoi."Id" = psi."Id"
LEFT JOIN "ProgramScheduleFloodItem" AS psfi ON psfi."Id" = psi."Id""#;

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
            .await
            .repository()?;
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
            .await
            .repository()?;
        } else {
            let item = ProgramScheduleItem {
                id,
                program_schedule_id: schedule_id,
                sort_order: index,
                ..ProgramScheduleItem::default()
            };
            upsert_program_schedule_item(tx, &item).await?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn upsert_program_schedule_item(
    tx: &mut Transaction<'_, Db>,
    item: &ProgramScheduleItem,
) -> Result<()> {
    let start_time = item.start_time.map(encode_duration);
    let result = sqlx::query(
        r#"UPDATE "ProgramScheduleItem"
           SET "ProgramScheduleId" = ?,
               "CollectionType" = ?,
               "CollectionId" = ?,
               "MediaItemId" = ?,
               "MultiCollectionId" = ?,
               "SmartCollectionId" = ?,
               "RerunCollectionId" = ?,
               "PlaylistId" = ?,
               "SearchQuery" = ?,
               "PlaybackOrder" = ?,
               "FillWithGroupMode" = ?,
               "GuideMode" = ?,
               "MarathonGroupBy" = ?,
               "MarathonShuffleGroups" = ?,
               "MarathonShuffleItems" = ?,
               "MarathonBatchSize" = ?,
               "Index" = ?,
               "StartTime" = ?,
               "FixedStartTimeBehavior" = ?,
               "CustomTitle" = ?,
               "PreferredAudioLanguageCode" = ?,
               "PreferredAudioTitle" = ?,
               "PreferredSubtitleLanguageCode" = ?,
               "SubtitleMode" = ?,
               "MediaVersionId" = ?,
               "VideoStreamIndex" = ?,
               "AudioStreamIndex" = ?,
               "SubtitleStreamIndex" = ?,
               "SearchTitle" = NULL,
               "FakeCollectionKey" = NULL,
               "PreRollFillerId" = ?,
               "MidRollFillerId" = ?,
               "PostRollFillerId" = ?,
               "TailFillerId" = ?,
               "FallbackFillerId" = ?
           WHERE "Id" = ?"#,
    )
    .bind(item.program_schedule_id)
    .bind(encode_enum(item.collection_type))
    .bind(item.collection_id)
    .bind(item.media_item_id)
    .bind(item.multi_collection_id)
    .bind(item.smart_collection_id)
    .bind(item.rerun_collection_id)
    .bind(item.playlist_id)
    .bind(item.search_query.as_deref())
    .bind(encode_enum(item.playback_order))
    .bind(encode_enum(item.fill_with_group_mode))
    .bind(encode_enum(item.guide_mode))
    .bind(encode_enum(item.marathon_group_by))
    .bind(item.marathon_shuffle_groups)
    .bind(item.marathon_shuffle_items)
    .bind(item.marathon_batch_size)
    .bind(item.sort_order)
    .bind(start_time.as_deref())
    .bind(item.fixed_start_time_behavior.map(encode_enum))
    .bind(item.custom_title.as_deref())
    .bind(item.preferred_audio_language_code.as_deref())
    .bind(item.preferred_audio_title.as_deref())
    .bind(item.preferred_subtitle_language_code.as_deref())
    .bind(item.subtitle_mode.map(encode_enum))
    .bind(
        item.pinned_stream_selection
            .map(|selection| selection.media_version_id),
    )
    .bind(
        item.pinned_stream_selection
            .map(|selection| selection.video_stream_index),
    )
    .bind(
        item.pinned_stream_selection
            .and_then(|selection| selection.audio_stream_index),
    )
    .bind(
        item.pinned_stream_selection
            .and_then(|selection| selection.subtitle_stream_index),
    )
    .bind(item.pre_roll_filler_id)
    .bind(item.mid_roll_filler_id)
    .bind(item.post_roll_filler_id)
    .bind(item.tail_filler_id)
    .bind(item.fallback_filler_id)
    .bind(item.id)
    .execute(&mut **tx)
    .await
    .repository()?;

    if result.rows_affected() == 0 {
        sqlx::query(
            r#"INSERT INTO "ProgramScheduleItem" (
                "Id", "ProgramScheduleId", "CollectionType", "CollectionId",
                "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                "RerunCollectionId", "PlaylistId", "SearchQuery",
                "PlaybackOrder", "FillWithGroupMode", "GuideMode",
                "MarathonGroupBy", "MarathonShuffleGroups", "MarathonShuffleItems", "MarathonBatchSize",
                "Index", "StartTime", "FixedStartTimeBehavior", "CustomTitle",
                "PreferredAudioLanguageCode", "PreferredAudioTitle",
                "PreferredSubtitleLanguageCode", "SubtitleMode", "PreRollFillerId",
                "MidRollFillerId", "PostRollFillerId", "TailFillerId", "FallbackFillerId",
                "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
                "SubtitleStreamIndex"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item.id)
        .bind(item.program_schedule_id)
        .bind(encode_enum(item.collection_type))
        .bind(item.collection_id)
        .bind(item.media_item_id)
        .bind(item.multi_collection_id)
        .bind(item.smart_collection_id)
        .bind(item.rerun_collection_id)
        .bind(item.playlist_id)
        .bind(item.search_query.as_deref())
        .bind(encode_enum(item.playback_order))
        .bind(encode_enum(item.fill_with_group_mode))
        .bind(encode_enum(item.guide_mode))
        .bind(encode_enum(item.marathon_group_by))
        .bind(item.marathon_shuffle_groups)
        .bind(item.marathon_shuffle_items)
        .bind(item.marathon_batch_size)
        .bind(item.sort_order)
        .bind(start_time.as_deref())
        .bind(item.fixed_start_time_behavior.map(encode_enum))
        .bind(item.custom_title.as_deref())
        .bind(item.preferred_audio_language_code.as_deref())
        .bind(item.preferred_audio_title.as_deref())
        .bind(item.preferred_subtitle_language_code.as_deref())
        .bind(item.subtitle_mode.map(encode_enum))
        .bind(item.pre_roll_filler_id)
        .bind(item.mid_roll_filler_id)
        .bind(item.post_roll_filler_id)
        .bind(item.tail_filler_id)
        .bind(item.fallback_filler_id)
        .bind(
            item.pinned_stream_selection
                .map(|selection| selection.media_version_id),
        )
        .bind(
            item.pinned_stream_selection
                .map(|selection| selection.video_stream_index),
        )
        .bind(
            item.pinned_stream_selection
                .and_then(|selection| selection.audio_stream_index),
        )
        .bind(
            item.pinned_stream_selection
                .and_then(|selection| selection.subtitle_stream_index),
        )
        .execute(&mut **tx)
        .await.repository()?;
    }

    sync_program_schedule_item_subtype(tx, item).await?;
    replace_ids(
        tx,
        r#"DELETE FROM "ProgramScheduleItemWatermark" WHERE "ProgramScheduleItemId" = ?"#,
        r#"REPLACE INTO "ProgramScheduleItemWatermark" ("ProgramScheduleItemId", "WatermarkId") VALUES (?, ?)"#,
        item.id,
        &item.watermark_ids,
    )
    .await?;
    replace_ids(
        tx,
        r#"DELETE FROM "ProgramScheduleItemGraphicsElement" WHERE "ProgramScheduleItemId" = ?"#,
        r#"REPLACE INTO "ProgramScheduleItemGraphicsElement" ("ProgramScheduleItemId", "GraphicsElementId") VALUES (?, ?)"#,
        item.id,
        &item.graphics_element_ids,
    )
    .await?;

    Ok(())
}

async fn sync_program_schedule_item_subtype(
    tx: &mut Transaction<'_, Db>,
    item: &ProgramScheduleItem,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "ProgramScheduleDurationItem" WHERE "Id" = ?"#)
        .bind(item.id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"DELETE FROM "ProgramScheduleMultipleItem" WHERE "Id" = ?"#)
        .bind(item.id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"DELETE FROM "ProgramScheduleOneItem" WHERE "Id" = ?"#)
        .bind(item.id)
        .execute(&mut **tx)
        .await
        .repository()?;
    sqlx::query(r#"DELETE FROM "ProgramScheduleFloodItem" WHERE "Id" = ?"#)
        .bind(item.id)
        .execute(&mut **tx)
        .await
        .repository()?;

    match item.playout_mode {
        PlayoutMode::Duration => {
            sqlx::query(
                r#"INSERT INTO "ProgramScheduleDurationItem" (
                    "Id", "PlayoutDuration", "TailMode"
                 ) VALUES (?, ?, ?)"#,
            )
            .bind(item.id)
            .bind(encode_duration(item.duration.unwrap_or_default()))
            .bind(encode_enum(item.tail_mode))
            .execute(&mut **tx)
            .await
            .repository()?;
        }
        PlayoutMode::Multiple => {
            sqlx::query(
                r#"INSERT INTO "ProgramScheduleMultipleItem" (
                    "Id", "Count", "MultipleMode"
                 ) VALUES (?, ?, ?)"#,
            )
            .bind(item.id)
            .bind(item.multiple_count.as_deref())
            .bind(encode_enum(item.multiple_mode))
            .execute(&mut **tx)
            .await
            .repository()?;
        }
        PlayoutMode::One => {
            sqlx::query(r#"INSERT INTO "ProgramScheduleOneItem" ("Id") VALUES (?)"#)
                .bind(item.id)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
        PlayoutMode::Flood => {
            sqlx::query(r#"INSERT INTO "ProgramScheduleFloodItem" ("Id") VALUES (?)"#)
                .bind(item.id)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
    }

    Ok(())
}

async fn load_ids_in_tx(
    tx: &mut Transaction<'_, Db>,
    sql: &str,
    id: EntityId,
) -> Result<Vec<EntityId>> {
    let rows = sqlx::query(crate::db::assert_sql_safe(sql))
        .bind(id)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
    rows.iter()
        .map(|row| row.try_get("Id").repository())
        .collect()
}

fn map_program_schedule(row: &DbRow) -> Result<ProgramSchedule> {
    Ok(ProgramSchedule {
        id: row.try_get("Id").repository()?,
        name: row.try_get("Name").repository()?,
        keep_multi_part_episodes_together: decode_bool_column(
            row,
            "KeepMultiPartEpisodesTogether",
        )?,
        treat_collections_as_shows: decode_bool_column(row, "TreatCollectionsAsShows")?,
        shuffle_schedule_items: decode_bool_column(row, "ShuffleScheduleItems")?,
        random_start_point: decode_bool_column(row, "RandomStartPoint")?,
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

#[allow(clippy::too_many_lines)]
async fn map_program_schedule_item(
    pool: &Pool<crate::db::Db>,
    row: &DbRow,
) -> Result<ProgramScheduleItem> {
    let id = row.try_get::<EntityId, _>("Id").repository()?;
    let duration_text = row
        .try_get::<Option<String>, _>("PlayoutDuration")
        .ok()
        .flatten();
    let duration = decode_optional_duration_text(duration_text.as_deref());
    let playout_mode = if row
        .try_get::<Option<EntityId>, _>("DurationItemId")
        .ok()
        .flatten()
        .is_some()
    {
        PlayoutMode::Duration
    } else if row
        .try_get::<Option<EntityId>, _>("MultipleItemId")
        .ok()
        .flatten()
        .is_some()
    {
        PlayoutMode::Multiple
    } else if row
        .try_get::<Option<EntityId>, _>("OneItemId")
        .ok()
        .flatten()
        .is_some()
    {
        PlayoutMode::One
    } else if row
        .try_get::<Option<EntityId>, _>("FloodItemId")
        .ok()
        .flatten()
        .is_some()
    {
        PlayoutMode::Flood
    } else {
        PlayoutMode::Duration
    };

    Ok(ProgramScheduleItem {
        id,
        program_schedule_id: row.try_get("ProgramScheduleId").repository()?,
        collection_type: decode_enum_column("program schedule item collection_type", row, "CollectionType")?,
        collection_id: row.try_get("CollectionId").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        multi_collection_id: row.try_get("MultiCollectionId").repository()?,
        smart_collection_id: row.try_get("SmartCollectionId").repository()?,
        rerun_collection_id: row.try_get("RerunCollectionId").repository()?,
        playlist_id: row.try_get("PlaylistId").repository()?,
        search_query: row.try_get("SearchQuery").repository()?,
        pre_roll_filler_id: row.try_get("PreRollFillerId").repository()?,
        mid_roll_filler_id: row.try_get("MidRollFillerId").repository()?,
        post_roll_filler_id: row.try_get("PostRollFillerId").repository()?,
        tail_filler_id: row.try_get("TailFillerId").repository()?,
        fallback_filler_id: row.try_get("FallbackFillerId").repository()?,
        playout_mode,
        playback_order: decode_enum_column("program schedule item playback_order", row, "PlaybackOrder")?,
        multiple_mode: decode_optional_enum_column("program schedule item multiple_mode", row, "MultipleMode")?
            .unwrap_or_default(),
        multiple_count: row.try_get::<Option<String>, _>("Count").ok().flatten(),
        duration,
        start_type: if row.try_get::<Option<String>, _>("StartTime").repository()?.is_some() {
            ersatzrs_domain::StartType::Fixed
        } else {
            ersatzrs_domain::StartType::Dynamic
        },
        start_time: decode_optional_duration_text(row.try_get::<Option<String>, _>("StartTime").repository()?.as_deref()),
        fixed_start_time_behavior: decode_optional_enum_column::<FixedStartTimeBehavior>(
            "program schedule item fixed_start_time_behavior",
            row,
            "FixedStartTimeBehavior",
        )?,
        guide_mode: decode_enum_column("program schedule item guide_mode", row, "GuideMode")?,
        tail_mode: decode_optional_enum_column("program schedule item tail_mode", row, "TailMode")?
            .unwrap_or_default(),
        fill_with_group_mode: decode_enum_column(
            "program schedule item fill_with_group_mode",
            row,
            "FillWithGroupMode",
        )?,
        marathon_group_by: decode_enum_column("program schedule item marathon_group_by", row, "MarathonGroupBy")?,
        marathon_shuffle_groups: decode_bool_column(row, "MarathonShuffleGroups")?,
        marathon_shuffle_items: decode_bool_column(row, "MarathonShuffleItems")?,
        marathon_batch_size: row.try_get("MarathonBatchSize").repository()?,
        custom_title: row.try_get("CustomTitle").repository()?,
        preferred_audio_language_code: row.try_get("PreferredAudioLanguageCode").repository()?,
        preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
        preferred_subtitle_language_code: row.try_get("PreferredSubtitleLanguageCode").repository()?,
        subtitle_mode: decode_optional_enum_column("program schedule item subtitle_mode", row, "SubtitleMode")?,
        pinned_stream_selection: decode_schedule_pinned_stream_selection(row)?,
        sort_order: row.try_get("Index").repository()?,
        watermark_ids: load_ids(
            pool,
            r#"SELECT "WatermarkId" FROM "ProgramScheduleItemWatermark" WHERE "ProgramScheduleItemId" = ? ORDER BY "WatermarkId""#,
            id,
        )
        .await?,
        graphics_element_ids: load_ids(
            pool,
            r#"SELECT "GraphicsElementId" FROM "ProgramScheduleItemGraphicsElement" WHERE "ProgramScheduleItemId" = ? ORDER BY "GraphicsElementId""#,
            id,
        )
        .await?,
    })
}

fn map_program_schedule_alternate(row: &DbRow) -> Result<ProgramScheduleAlternate> {
    Ok(ProgramScheduleAlternate {
        id: row.try_get("Id").repository()?,
        program_schedule_id: row.try_get("ProgramScheduleId").repository()?,
        playout_id: Some(row.try_get("PlayoutId").repository()?),
        sort_order: row.try_get("Index").repository()?,
        days_of_week: decode_string_list(
            row.try_get::<Option<String>, _>("DaysOfWeek")
                .repository()?,
        ),
        days_of_month: decode_int_list(
            row.try_get::<Option<String>, _>("DaysOfMonth")
                .repository()?,
        ),
        months_of_year: decode_int_list(
            row.try_get::<Option<String>, _>("MonthsOfYear")
                .repository()?,
        ),
        limit_to_date_range: decode_bool_column(row, "LimitToDateRange")?,
        start_month: row.try_get("StartMonth").repository()?,
        start_day: row.try_get("StartDay").repository()?,
        start_year: row.try_get("StartYear").repository()?,
        end_month: row.try_get("EndMonth").repository()?,
        end_day: row.try_get("EndDay").repository()?,
        end_year: row.try_get("EndYear").repository()?,
    })
}

async fn upsert_program_schedule_alternate(
    tx: &mut Transaction<'_, Db>,
    alternate: &ProgramScheduleAlternate,
) -> Result<()> {
    let result = sqlx::query(
        r#"UPDATE "ProgramScheduleAlternate"
           SET "PlayoutId" = ?,
               "ProgramScheduleId" = ?,
               "Index" = ?,
               "DaysOfWeek" = ?,
               "DaysOfMonth" = ?,
               "MonthsOfYear" = ?,
               "LimitToDateRange" = ?,
               "StartMonth" = ?,
               "StartDay" = ?,
               "StartYear" = ?,
               "EndMonth" = ?,
               "EndDay" = ?,
               "EndYear" = ?
           WHERE "Id" = ?"#,
    )
    .bind(alternate.playout_id)
    .bind(alternate.program_schedule_id)
    .bind(alternate.sort_order)
    .bind(encode_string_list(&alternate.days_of_week))
    .bind(encode_int_list(&alternate.days_of_month))
    .bind(encode_int_list(&alternate.months_of_year))
    .bind(alternate.limit_to_date_range)
    .bind(alternate.start_month)
    .bind(alternate.start_day)
    .bind(alternate.start_year)
    .bind(alternate.end_month)
    .bind(alternate.end_day)
    .bind(alternate.end_year)
    .bind(alternate.id)
    .execute(&mut **tx)
    .await
    .repository()?;

    if result.rows_affected() == 0 {
        sqlx::query(
            r#"INSERT INTO "ProgramScheduleAlternate" (
                "Id", "PlayoutId", "ProgramScheduleId", "Index", "DaysOfWeek",
                "DaysOfMonth", "MonthsOfYear", "LimitToDateRange", "StartMonth",
                "StartDay", "StartYear", "EndMonth", "EndDay", "EndYear"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(alternate.id)
        .bind(alternate.playout_id)
        .bind(alternate.program_schedule_id)
        .bind(alternate.sort_order)
        .bind(encode_string_list(&alternate.days_of_week))
        .bind(encode_int_list(&alternate.days_of_month))
        .bind(encode_int_list(&alternate.months_of_year))
        .bind(alternate.limit_to_date_range)
        .bind(alternate.start_month)
        .bind(alternate.start_day)
        .bind(alternate.start_year)
        .bind(alternate.end_month)
        .bind(alternate.end_day)
        .bind(alternate.end_year)
        .execute(&mut **tx)
        .await
        .repository()?;
    }

    Ok(())
}

fn decode_int_list(value: Option<String>) -> Vec<i32> {
    value
        .unwrap_or_default()
        .split(',')
        .filter_map(|part| part.parse::<i32>().ok())
        .collect()
}

fn encode_int_list(value: &[i32]) -> String {
    value
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_string_list(value: Option<String>) -> Vec<String> {
    value
        .and_then(|inner| serde_json::from_str(&inner).ok())
        .unwrap_or_default()
}

fn encode_string_list(value: &[String]) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[]".to_owned())
}
