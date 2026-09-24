//! Read the persisted channel timeline without multiplying items by media files.

use chrono::{DateTime, Utc};
use ersatzrs_core::locking::{EntityLocker, PlayoutLockGuard};
use ersatzrs_core::repositories::{PlayoutRepository, RepositoryError, Result};
use ersatzrs_domain::{Channel, PlayoutItem};
use sqlx::Row;

use crate::channel_playout::ChannelPlayoutContext;
use crate::db::DbPool;
use crate::repositories::{SqlitePlayoutRepository, SqlxResultExt as _};

/// One logical scheduled item and its deterministic persisted source binding.
/// Remote bindings require the existing provider resolver before wire publication.
/// Deliberately no `Debug`: a media path can contain a private provider URL.
pub struct ScheduledMedia {
    pub item: PlayoutItem,
    pub media_version_id: i64,
    /// Exact file selected with the version; provider keys belong to this file.
    pub media_file_id: i64,
    pub path: String,
    pub remote_stream_id: Option<i64>,
}

/// Keeps the source playouts locked while their derived projection is prepared.
///
/// Keep this owner alive until publication completes. Locks cover the shared
/// scheduler's builds, not arbitrary metadata edits or external database writers.
/// Use the host's existing singleton `EntityLocker`, never a new local instance.
pub struct LockedSchedule {
    pub context: ChannelPlayoutContext,
    pub items: Vec<ScheduledMedia>,
    _guards: Vec<PlayoutLockGuard>,
}

/// Read all items overlapping a target-channel time range, in start/id order.
///
/// Mirror offsets are applied to the query range and returned timeline. Multiple
/// persisted playouts are retained, matching upstream's channel-wide exporter.
/// Missing media fails the whole read rather than silently dropping a programme.
/// An empty successful read is distinct from a busy read and can withdraw an old
/// publication. No provider/network work occurs while loading these bindings.
///
/// # Errors
/// Returns database/decode failures or invalid/overflowing time ranges. `Ok(None)`
/// means a build owns a source lock or the playout membership changed while locks
/// were being acquired; retain the previous publication and retry later.
pub async fn read_locked(
    pool: &DbPool,
    locker: &EntityLocker,
    channel: &Channel,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<Option<LockedSchedule>> {
    if finish <= start {
        return Err(invalid("invalid requested time range"));
    }
    let context = ChannelPlayoutContext::for_channel(channel);
    let source_start = context.source_time(start)?;
    let source_finish = context.source_time(finish)?;
    let ids = playout_ids(pool, context.source_channel_id).await?;
    let mut guards = Vec::with_capacity(ids.len());
    for id in &ids {
        let Some(guard) = PlayoutLockGuard::try_acquire(locker, *id) else {
            return Ok(None);
        };
        guards.push(guard);
    }
    if playout_ids(pool, context.source_channel_id).await? != ids {
        return Ok(None);
    }
    let repository = SqlitePlayoutRepository(pool.clone());
    let mut items = Vec::new();
    for id in &ids {
        for mut item in repository
            .items_in_range(*id, source_start, source_finish)
            .await?
        {
            item.start = context.target_time(item.start)?;
            item.finish = context.target_time(item.finish)?;
            item.guide_start = item
                .guide_start
                .map(|time| context.target_time(time))
                .transpose()?;
            item.guide_finish = item
                .guide_finish
                .map(|time| context.target_time(time))
                .transpose()?;
            items.push(bind_media(pool, item).await?);
        }
    }
    // A source playout may be created or deleted by administration independently
    // of build locks. Never publish a knowingly incomplete membership read.
    if playout_ids(pool, context.source_channel_id).await? != ids {
        return Ok(None);
    }
    items.sort_by_key(|media| (media.item.start, media.item.id));
    Ok(Some(LockedSchedule {
        context,
        items,
        _guards: guards,
    }))
}

async fn playout_ids(pool: &DbPool, channel_id: i64) -> Result<Vec<i64>> {
    sqlx::query_scalar(r#"SELECT "Id" FROM "Playout" WHERE "ChannelId" = ? ORDER BY "Id""#)
        .bind(channel_id)
        .fetch_all(pool)
        .await
        .repository()
}

const MEDIA_QUERY: &str = r#"
    SELECT mv."Id" AS "VersionId", mf."Id" AS "FileId", mf."Path", mv."RemoteStreamId"
    FROM "MediaVersion" mv
    JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
    WHERE (mv."MovieId" = ? OR mv."EpisodeId" = ? OR mv."MusicVideoId" = ?
        OR mv."OtherVideoId" = ? OR mv."RemoteStreamId" = ? OR mv."SongId" = ?
        OR mv."ImageId" = ?)
      AND (? IS NULL OR mv."Id" = ?)
      AND mf."Path" IS NOT NULL
    ORDER BY mv."Id", mf."Id"
    LIMIT 1
"#;

pub(crate) async fn bind_media(pool: &DbPool, item: PlayoutItem) -> Result<ScheduledMedia> {
    let mut query = sqlx::query(MEDIA_QUERY);
    for _ in 0..7 {
        query = query.bind(item.media_item_id);
    }
    let pinned = item.pinned_stream_selection.map(|pin| pin.media_version_id);
    let row = query
        .bind(pinned)
        .bind(pinned)
        .fetch_optional(pool)
        .await
        .repository()?
        .ok_or_else(|| invalid("scheduled media version or file is unavailable"))?;
    let path: String = row.try_get("Path").repository()?;
    if path.trim().is_empty() {
        return Err(invalid("scheduled media path is empty"));
    }
    Ok(ScheduledMedia {
        item,
        path,
        media_version_id: row.try_get("VersionId").repository()?,
        media_file_id: row.try_get("FileId").repository()?,
        remote_stream_id: row.try_get("RemoteStreamId").repository()?,
    })
}

fn invalid(message: &str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "Next channel schedule",
        value: message.to_owned(),
    }
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
pub(crate) mod tests;
