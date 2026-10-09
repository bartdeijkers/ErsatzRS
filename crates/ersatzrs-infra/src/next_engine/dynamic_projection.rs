//! Short locked snapshots and stable identities for demand-time preparation.

use chrono::{DateTime, Utc};
use ersatzrs_core::{
    locking::EntityLocker,
    path_replacement::PathReplacement,
    repositories::{ChannelRepository, RepositoryError, Result},
};
use ersatzrs_domain::StreamingEngine;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::fmt::Write as _;

use crate::{db::DbPool, playback, repositories::make_repositories};

use super::{media::MediaRelay, protocol::Source, schedule, selection, source};

pub(crate) struct Identity {
    pub descriptor: String,
    pub cache_key: String,
}

/// Owns values only; no scheduler lock survives snapshot preparation.
pub(crate) struct Snapshot {
    pub projection: selection::DeferredProjection,
    pub identity: Identity,
    pub prepared_remote: Option<super::remote_projection::PreparedContext>,
    pub producer_remote: Option<super::remote_projection::ProducerContext>,
    pub retained_remote: Option<super::remote_projection::RetainedContext>,
}

/// Re-read the original persisted interval, even when the requested range starts
/// midway through it. Seeking to callback time belongs after identity validation.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn project_snapshot(
    pool: &DbPool,
    locker: &EntityLocker,
    relay: &MediaRelay,
    channel_id: i64,
    item_id: i64,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    replacements: &[PathReplacement],
    published_audio: Option<usize>,
) -> Result<Snapshot> {
    let channel = make_repositories(pool.clone())
        .channel
        .get_with_profile(channel_id)
        .await?;
    if channel.streaming_engine != StreamingEngine::Next {
        return Err(unavailable());
    }
    let schedule_snapshot = schedule::read_locked(pool, locker, &channel, start, finish)
        .await?
        .ok_or_else(unavailable)?;
    let mut matches = schedule_snapshot
        .items
        .iter()
        .filter(|media| media.item.id == item_id);
    let media = matches.next().ok_or_else(unavailable)?;
    if matches.next().is_some() {
        return Err(unavailable());
    }
    let profile = playback::load_channel_playback_profile(pool, channel_id)
        .await?
        .ok_or_else(unavailable)?;
    if media.remote_stream_id.is_some() {
        let result = super::remote_projection::project_publishable(
            pool,
            &channel,
            media,
            &profile.resolution,
            selection::AudioDirective::Published(published_audio),
        )
        .await;
        drop(schedule_snapshot);
        return Box::pin(super::remote_projection::materialize_prepared(
            pool,
            locker,
            result?,
            published_audio,
            &crate::file_system_layout::addon_processed_media_folder(),
        ))
        .await;
    }
    let selection_time = schedule_snapshot.context.source_time(media.item.start)?;
    let (watermarks, graphics) = if super::graphics::enabled(&profile) {
        let watermarks = playback::select_playback_watermarks(
            pool,
            &channel,
            media.item.id,
            media.item.disable_watermarks,
            selection_time,
        )
        .await?;
        let graphics =
            playback::select_playback_graphics(pool, &channel, media.item.id, selection_time)
                .await?;
        (watermarks, graphics)
    } else {
        (Vec::new(), Vec::new())
    };
    let source = source::resolve_published(pool, media, replacements, Some(relay)).await?;
    let mut projection = selection::project_deferred_item_with_audio(
        pool,
        &channel,
        &media.item,
        media.media_version_id,
        source,
        &profile.resolution,
        selection::AudioDirective::Published(published_audio),
    )
    .await?;
    super::credits::attach(pool, relay, &channel, &media.item, &mut projection).await?;
    projection.canvas = super::graphics::project(
        pool,
        &channel,
        media,
        &profile,
        &mut projection.item,
        &watermarks,
        &graphics,
    )
    .await?;
    let identity = fingerprint(pool, media, &projection, &channel).await?;
    drop(schedule_snapshot);
    Ok(Snapshot {
        projection,
        identity,
        prepared_remote: None,
        retained_remote: None,
        producer_remote: None,
    })
}

/// Hash precisely the concrete projection used for publication. This performs
/// only database reads and local metadata access, never extraction or HTTP work.
/// The cache identity excludes occurrence times and seeks, allowing repeated
/// schedule items to reuse one full-timeline extracted subtitle.
pub(crate) async fn fingerprint(
    pool: &DbPool,
    media: &schedule::ScheduledMedia,
    projection: &selection::DeferredProjection,
    channel: &ersatzrs_domain::Channel,
) -> Result<Identity> {
    let row = sqlx::query(
        r#"SELECT v."DateUpdated", v."Duration", f."PathHash", f."Path"
        FROM "MediaVersion" v JOIN "MediaFile" f ON f."MediaVersionId"=v."Id"
        WHERE v."Id"=? AND f."Id"=? AND f."Path"=?
        AND (v."MovieId"=? OR v."EpisodeId"=? OR v."MusicVideoId"=? OR v."OtherVideoId"=?
        OR v."RemoteStreamId"=? OR v."SongId"=? OR v."ImageId"=?)"#,
    )
    .bind(media.media_version_id)
    .bind(media.media_file_id)
    .bind(&media.path)
    .bind(media.item.media_item_id)
    .bind(media.item.media_item_id)
    .bind(media.item.media_item_id)
    .bind(media.item.media_item_id)
    .bind(media.item.media_item_id)
    .bind(media.item.media_item_id)
    .bind(media.item.media_item_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(unavailable)?;
    let updated: String = row.try_get("DateUpdated").map_err(|_| unavailable())?;
    let duration: String = row.try_get("Duration").map_err(|_| unavailable())?;
    let path_hash: String = row.try_get("PathHash").map_err(|_| unavailable())?;
    let path: String = row.try_get("Path").map_err(|_| unavailable())?;
    let pending = projection.pending_burn.as_ref();
    // Generated song/black video and explicitly silent audio do not identify
    // the underlying programme. Keep source revisions and provider identity
    // tied to the original selected media, including its path replacements.
    let input = pending.map_or(&projection.original_source, |pending| &pending.source);
    let local_revision = if let Source::Local { path, .. } = input {
        let metadata = tokio::fs::metadata(path).await.map_err(|_| unavailable())?;
        if !metadata.is_file() {
            return Err(unavailable());
        }
        let modified = metadata
            .modified()
            .map_err(|_| unavailable())?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| unavailable())?;
        Some(json!({
            "resolved_path": path,
            "size": metadata.len(),
            "modified": (modified.as_secs(), modified.subsec_nanos()),
        }))
    } else {
        None
    };
    let subtitle = pending.map(|pending| {
        let subtitle = &pending.subtitle;
        json!({
            "id": subtitle.id, "kind": subtitle.subtitle_kind,
            "index": subtitle.stream_index, "codec": subtitle.codec,
            "language": subtitle.language, "title": subtitle.title,
            "default": subtitle.default, "forced": subtitle.forced, "sdh": subtitle.sdh,
        })
    });
    let binding = json!({
        "schema": 1, "media": media.item.media_item_id,
        "version": media.media_version_id, "file": media.media_file_id,
        "updated": updated, "duration": duration, "path_hash": path_hash,
        "path": path, "local": local_revision, "subtitle": subtitle,
        "provider": super::dynamic_provider_identity::identity(pool, media, input).await?,
    });
    let preferences = crate::ffmpeg_config::playback_language_preferences(pool).await?;
    let script = crate::audio_script::script_identity(pool, media.item.media_item_id).await?;
    Ok(Identity {
        descriptor: digest(&json!({"binding": binding, "item": projection.item,
            "canvas": projection.canvas, "song": projection.song,
            "persisted_item": media.item, "channel": channel, "audio_script": script,
            "audio_language": preferences.default_audio_language,
            "subtitle_language": preferences.default_subtitle_language,
            "fallback_subtitle_language": preferences.fallback_subtitle_language,
        }))?,
        cache_key: digest(&binding)?,
    })
}

pub(super) fn digest(value: &serde_json::Value) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| unavailable())?;
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(digest, "{byte:02x}").map_err(|_| unavailable())?;
    }
    Ok(digest)
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable("Next dynamic projection", "binding unavailable")
}

#[cfg(test)]
#[path = "dynamic_projection_tests.rs"]
mod tests;
