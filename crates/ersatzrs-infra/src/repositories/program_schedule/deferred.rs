//! Save branding and track edits without invalidating Classic enumerators.

use super::*;
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, ScheduleDeferredChange};
use ersatzrs_domain::PlayoutScheduleKind;

use crate::repositories::common::decode_datetime;

pub(super) async fn save(
    pool: &Pool<Db>,
    schedule_id: EntityId,
    items: &[ScheduleItemAdmin],
    locked_playouts: &[EntityId],
) -> Result<Vec<ScheduleDeferredChange>> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ProgramSchedule" WHERE "Id"=?"#)
        .bind(schedule_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
    if exists == 0 {
        return Err(RepositoryError::NotFound {
            kind: "program schedule",
            id: schedule_id,
        });
    }
    let ids = crate::classic_deferred::playout_ids(&mut *tx, schedule_id).await?;
    if ids != locked_playouts {
        return Err(RepositoryError::Conflict(
            "Classic deferred save targets changed; reload and retry".into(),
        ));
    }
    let old = list_schedule_items_admin_on_connection(&mut tx, schedule_id).await?;
    let normalized = compatible_items(&old, items)?;
    let mut horizons = Vec::new();
    for id in &ids {
        horizons.push((*id, horizon(&mut tx, *id).await?));
    }
    for (previous, next) in old.iter().zip(&normalized) {
        if previous != next {
            update_item(&mut tx, schedule_id, next).await?;
        }
    }
    if old != normalized {
        for (id, boundary) in horizons {
            crate::classic_deferred::queue(&mut tx, id, schedule_id, boundary).await?;
        }
    }
    tx.commit().await.repository()?;
    crate::classic_deferred::list(pool, schedule_id).await
}

fn compatible_items(
    old: &[ScheduleItemAdmin],
    items: &[ScheduleItemAdmin],
) -> Result<Vec<ScheduleItemAdmin>> {
    let invalid = || {
        RepositoryError::Conflict(
        "Classic deferred save cannot change source, playback order or item structure; use a programme-preserving Refresh instead".into(),
    )
    };
    if old.len() != items.len() {
        return Err(invalid());
    }
    let mut result = Vec::with_capacity(items.len());
    for (previous, next) in old.iter().zip(items) {
        let mut expected = previous.clone();
        copy_deferred_fields(next, &mut expected);
        // Display-only projections do not change the source identity.
        expected.collection_name.clone_from(&next.collection_name);
        expected.inherited_legacy_smart_shuffle = next.inherited_legacy_smart_shuffle;
        // Playlist/rerun children own playback order; the editor normalizes
        // this unused parent field without changing their enumerators.
        if matches!(previous.collection_type, 6..=8) && next.playback_order == 0 {
            expected.playback_order = 0;
        }
        if expected != *next {
            return Err(invalid());
        }
        let mut normalized = previous.clone();
        copy_deferred_fields(next, &mut normalized);
        normalized.watermark_ids.sort_unstable();
        normalized.watermark_ids.dedup();
        normalized.graphics_element_ids.sort_unstable();
        normalized.graphics_element_ids.dedup();
        result.push(normalized);
    }
    Ok(result)
}

fn copy_deferred_fields(source: &ScheduleItemAdmin, target: &mut ScheduleItemAdmin) {
    target.custom_title.clone_from(&source.custom_title);
    target.guide_mode = source.guide_mode;
    target.pre_roll_filler_id = source.pre_roll_filler_id;
    target.mid_roll_filler_id = source.mid_roll_filler_id;
    target.post_roll_filler_id = source.post_roll_filler_id;
    target.tail_filler_id = source.tail_filler_id;
    target.fallback_filler_id = source.fallback_filler_id;
    target.watermark_ids.clone_from(&source.watermark_ids);
    target
        .graphics_element_ids
        .clone_from(&source.graphics_element_ids);
    target
        .preferred_audio_language_code
        .clone_from(&source.preferred_audio_language_code);
    target
        .preferred_audio_title
        .clone_from(&source.preferred_audio_title);
    target
        .preferred_subtitle_language_code
        .clone_from(&source.preferred_subtitle_language_code);
    target.subtitle_mode = source.subtitle_mode;
    target.pinned_stream_selection = source.pinned_stream_selection;
}

async fn horizon(tx: &mut Transaction<'_, Db>, id: EntityId) -> Result<Option<DateTime<Utc>>> {
    let row = sqlx::query(
        r#"SELECT "p"."ScheduleKind", "a"."NextStart"
        FROM "Playout" AS "p" LEFT JOIN "PlayoutAnchor" AS "a" ON "a"."PlayoutId"="p"."Id"
        WHERE "p"."Id"=?"#,
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if decode_enum_column::<PlayoutScheduleKind>("playout schedule kind", &row, "ScheduleKind")?
        != PlayoutScheduleKind::Classic
    {
        return Err(RepositoryError::Conflict(
            "Classic deferred save requires Classic playouts".into(),
        ));
    }
    let anchor = row
        .try_get::<Option<String>, _>("NextStart")
        .repository()?
        .as_deref()
        .map(|value| decode_datetime("Classic deferred boundary", value))
        .transpose()?;
    let rows = sqlx::query(
        r#"SELECT "Start", "Finish" FROM "PlayoutItem" WHERE "PlayoutId"=? ORDER BY "Start", "Id""#,
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let intervals = rows
        .iter()
        .map(|row| {
            Ok((
                decode_datetime(
                    "playout start",
                    row.try_get::<&str, _>("Start").repository()?,
                )?,
                decode_datetime(
                    "playout finish",
                    row.try_get::<&str, _>("Finish").repository()?,
                )?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    ersatzrs_core::scheduling::validate_classic_timeline(intervals.iter().copied(), None)
        .map_err(|_| RepositoryError::Conflict("Classic deferred save requires a valid published timeline; repair the overlap before saving".into()))?;
    Ok(anchor
        .into_iter()
        .chain(intervals.last().map(|(_, finish)| *finish))
        .max())
}

async fn update_item(
    tx: &mut Transaction<'_, Db>,
    schedule_id: EntityId,
    item: &ScheduleItemAdmin,
) -> Result<()> {
    sqlx::query(r#"UPDATE "ProgramScheduleItem" SET
        "CustomTitle"=?, "GuideMode"=?, "PreRollFillerId"=?, "MidRollFillerId"=?,
        "PostRollFillerId"=?, "TailFillerId"=?, "FallbackFillerId"=?,
        "PreferredAudioLanguageCode"=?, "PreferredAudioTitle"=?, "PreferredSubtitleLanguageCode"=?,
        "SubtitleMode"=?, "MediaVersionId"=?, "VideoStreamIndex"=?, "AudioStreamIndex"=?, "SubtitleStreamIndex"=?
        WHERE "Id"=? AND "ProgramScheduleId"=?"#)
        .bind(schedule_optional_text(&item.custom_title)).bind(item.guide_mode)
        .bind(item.pre_roll_filler_id).bind(item.mid_roll_filler_id).bind(item.post_roll_filler_id)
        .bind(item.tail_filler_id).bind(item.fallback_filler_id)
        .bind(item.preferred_audio_language_code.as_deref()).bind(schedule_optional_text(&item.preferred_audio_title))
        .bind(item.preferred_subtitle_language_code.as_deref()).bind(item.subtitle_mode)
        .bind(item.pinned_stream_selection.map(|value| value.media_version_id))
        .bind(item.pinned_stream_selection.map(|value| value.video_stream_index))
        .bind(item.pinned_stream_selection.and_then(|value| value.audio_stream_index))
        .bind(item.pinned_stream_selection.and_then(|value| value.subtitle_stream_index))
        .bind(item.id).bind(schedule_id).execute(&mut **tx).await.repository()?;
    replace_ids(tx,
        r#"DELETE FROM "ProgramScheduleItemWatermark" WHERE "ProgramScheduleItemId"=?"#,
        r#"INSERT INTO "ProgramScheduleItemWatermark" ("ProgramScheduleItemId", "WatermarkId") VALUES (?, ?)"#,
        item.id, &item.watermark_ids).await?;
    replace_ids(tx,
        r#"DELETE FROM "ProgramScheduleItemGraphicsElement" WHERE "ProgramScheduleItemId"=?"#,
        r#"INSERT INTO "ProgramScheduleItemGraphicsElement" ("ProgramScheduleItemId", "GraphicsElementId") VALUES (?, ?)"#,
        item.id, &item.graphics_element_ids).await?;
    Ok(())
}
