//! Read the persisted channel timeline without multiplying items by media files.

use chrono::{DateTime, Utc};
use ersatzrs_core::locking::{EntityLocker, PlayoutReadGuard};
use ersatzrs_core::repositories::{PlayoutRepository, RepositoryError, Result};
use ersatzrs_domain::{Channel, PlayoutItem};
use sqlx::Row;

use crate::channel_playout::ChannelPlayoutContext;
use crate::db::DbPool;
use crate::repositories::{PendingPlayoutBuild, SqlitePlayoutRepository, SqlxResultExt as _};

/// One logical scheduled item and its deterministic persisted source binding.
/// Remote bindings require the existing provider resolver before wire publication.
/// Deliberately no `Debug`: a media path can contain a private provider URL.
#[derive(Clone)]
pub struct ScheduledMedia {
    pub item: PlayoutItem,
    pub media_version_id: i64,
    /// Exact file selected with the version; provider keys belong to this file.
    pub media_file_id: i64,
    pub path: String,
    pub remote_stream_id: Option<i64>,
    pub(crate) candidate_overlays: Option<crate::playback::CandidateOverlayContext>,
}

/// Keeps the source playouts locked while their derived projection is prepared.
///
/// Keep this owner alive until publication completes. Locks cover the shared
/// scheduler's builds, not arbitrary metadata edits or external database writers.
/// Use the host's existing singleton `EntityLocker`, never a new local instance.
pub struct LockedSchedule {
    pub context: ChannelPlayoutContext,
    pub items: Vec<ScheduledMedia>,
    _guards: Vec<PlayoutReadGuard>,
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
    read_with_candidate(pool, locker, channel, start, finish, None).await
}

/// Read one unpublished build alongside the other accepted source playouts.
/// The caller retains the candidate's exclusive build lock; acquire read locks
/// for every other source before projecting this channel's complete timeline.
pub(super) async fn read_pending(
    pool: &DbPool,
    locker: &EntityLocker,
    channel: &Channel,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    pending: &mut PendingPlayoutBuild,
) -> Result<Option<LockedSchedule>> {
    read_with_candidate(pool, locker, channel, start, finish, Some(pending)).await
}

async fn read_with_candidate(
    pool: &DbPool,
    locker: &EntityLocker,
    channel: &Channel,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    mut pending: Option<&mut PendingPlayoutBuild>,
) -> Result<Option<LockedSchedule>> {
    if finish <= start {
        return Err(invalid("invalid requested time range"));
    }
    let context = ChannelPlayoutContext::for_channel(channel);
    let source_start = context.source_time(start)?;
    // Full publication uses the last four-digit year as its open-ended bound.
    // A leading mirror can shift it into +10000, whose encoded SQL text sorts
    // before ordinary programme dates and would silently select no rows.
    let maximum_query_time = DateTime::from_timestamp(253_402_300_799, 0)
        .ok_or_else(|| invalid("invalid publication horizon"))?;
    let source_finish = context.source_time(finish)?.min(maximum_query_time);
    if pending
        .as_ref()
        .is_some_and(|build| build.channel_id() != context.source_channel_id)
    {
        return Err(invalid("candidate belongs to another source channel"));
    }
    let ids = playout_ids(pool, context.source_channel_id).await?;
    if pending
        .as_ref()
        .is_some_and(|build| !ids.contains(&build.playout_id()))
    {
        return Err(invalid("candidate source playout is no longer attached"));
    }
    let mut guards = Vec::with_capacity(ids.len());
    for id in &ids {
        if pending
            .as_ref()
            .is_some_and(|build| build.playout_id() == *id)
        {
            continue;
        }
        let Some(guard) = PlayoutReadGuard::try_acquire(locker, *id) else {
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
        let source_items = match pending.as_mut() {
            Some(build) if build.playout_id() == *id => {
                build.items_in_range(source_start, source_finish).await?
            }
            _ => {
                repository
                    .items_in_range(*id, source_start, source_finish)
                    .await?
            }
        };
        for mut item in source_items {
            let candidate_overlays = match pending.as_mut() {
                Some(build) if build.playout_id() == *id => {
                    Some(build.overlay_context(item.id, item.start).await?)
                }
                _ => None,
            };
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
            let mut media = bind_media(pool, item).await?;
            media.candidate_overlays = candidate_overlays;
            items.push(media);
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

/// Sample only versions selected by the same binding as Next publication.
/// No preparation, provider calls, media probing or persisted mutations occur.
///
/// # Errors
/// Returns database/metadata errors or unavailable selected media bindings.
pub(super) async fn frame_rates(pool: &DbPool, source_channel_id: i64) -> Result<Vec<String>> {
    let rows = sqlx::query(r#"SELECT DISTINCT pi."MediaItemId", pi."MediaVersionId", pi."VideoStreamIndex", pi."AudioStreamIndex", pi."SubtitleStreamIndex"
        FROM "Playout" p JOIN "PlayoutItem" pi ON pi."PlayoutId"=p."Id"
        WHERE p."ChannelId"=?"#)
        .bind(source_channel_id).fetch_all(pool).await.repository()?;
    let mut versions = std::collections::BTreeSet::new();
    for row in rows {
        let pinned = match (
            row.try_get::<Option<i64>, _>("MediaVersionId")
                .repository()?,
            row.try_get::<Option<i32>, _>("VideoStreamIndex")
                .repository()?,
        ) {
            (Some(media_version_id), Some(video_stream_index)) => {
                Some(ersatzrs_domain::PinnedStreamSelection {
                    media_version_id,
                    video_stream_index,
                    audio_stream_index: row.try_get("AudioStreamIndex").repository()?,
                    subtitle_stream_index: row.try_get("SubtitleStreamIndex").repository()?,
                })
            }
            _ => None,
        };
        let selected = bind_media(
            pool,
            PlayoutItem {
                media_item_id: row.try_get("MediaItemId").repository()?,
                pinned_stream_selection: pinned,
                ..Default::default()
            },
        )
        .await?;
        versions.insert(selected.media_version_id);
    }
    let mut rates = Vec::with_capacity(versions.len());
    for version in versions {
        let rate: Option<String> =
            sqlx::query_scalar(r#"SELECT "RFrameRate" FROM "MediaVersion" WHERE "Id"=?"#)
                .bind(version)
                .fetch_one(pool)
                .await
                .repository()?;
        if let Some(rate) = rate {
            rates.push(rate);
        }
    }
    Ok(rates)
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

/// Resolve the deterministic version/file binding for a persisted or transient item.
/// Does not persist the supplied item or take scheduler locks. An explicit pinned
/// version must still belong to the media item and have a usable file.
///
/// # Errors
/// Returns read/decode failures or an unavailable version/file binding.
pub async fn bind_media(pool: &DbPool, item: PlayoutItem) -> Result<ScheduledMedia> {
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
        candidate_overlays: None,
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

impl ScheduledMedia {
    /// Resolve watermark and graphics precedence for this exact schedule read.
    /// Pending candidates use their captured item/template inputs; ordinary
    /// reads keep the existing persisted selection behavior.
    ///
    /// # Errors
    /// Returns selection, database or invalid source-clock errors.
    pub async fn overlays(
        &self,
        pool: &DbPool,
        channel: &Channel,
    ) -> Result<(
        Vec<crate::playback::ResolvedPlaybackWatermark>,
        Vec<crate::playback::ResolvedPlaybackGraphicsElement>,
    )> {
        let source_time =
            ChannelPlayoutContext::for_channel(channel).source_time(self.item.start)?;
        if let Some(inputs) = &self.candidate_overlays {
            return inputs
                .resolve(pool, channel, self.item.disable_watermarks, source_time)
                .await;
        }
        Ok((
            crate::playback::select_playback_watermarks(
                pool,
                channel,
                self.item.id,
                self.item.disable_watermarks,
                source_time,
            )
            .await?,
            crate::playback::select_playback_graphics(pool, channel, self.item.id, source_time)
                .await?,
        ))
    }
}
