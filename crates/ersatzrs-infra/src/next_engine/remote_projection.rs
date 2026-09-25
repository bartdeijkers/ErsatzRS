//! DB-only Remote Stream publication snapshots. Concrete sources stay private to the host.

use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use ersatzrs_core::{
    locking::EntityLocker,
    repositories::{
        AddonMediaListLifecycleRepository as _, ChannelRepository as _, RepositoryError, Result,
    },
};
use ersatzrs_domain::{Channel, RemoteStreamLiveness, Resolution, StreamingEngine};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

use super::{
    dynamic_projection::Identity,
    protocol::Source,
    schedule::{self, ScheduledMedia},
    selection,
};
use crate::{
    db::DbPool,
    playback,
    repositories::{make_repositories, SqliteAddonMediaListRepository, SqlxResultExt as _},
};

/// Owned values only. Never publish the concrete projection or log this owner:
/// callback publication must replace its sources with the authenticated Dynamic URI.
pub(crate) struct Snapshot {
    pub media: ScheduledMedia,
    pub projection: selection::DeferredProjection,
    pub identity: Identity,
    pub liveness: RemoteStreamLiveness,
    pub prepared: bool,
    pub retained: bool,
}

/// Distinguishes persisted programmes from session-owned filler without fake SQL rows.
#[derive(Clone, Copy)]
pub(super) enum Origin {
    Persisted,
    Fallback(super::fallback::Gap),
}

impl Origin {
    pub(super) async fn snapshot(
        self,
        pool: &DbPool,
        locker: &EntityLocker,
        channel_id: i64,
        item: &ersatzrs_domain::PlayoutItem,
        audio: Option<usize>,
    ) -> Result<Snapshot> {
        match self {
            Self::Persisted => {
                snapshot(
                    pool,
                    locker,
                    channel_id,
                    item.id,
                    item.start,
                    item.finish,
                    audio,
                )
                .await
            }
            Self::Fallback(gap) => {
                super::fallback::remote_snapshot(pool, locker, channel_id, item, gap, audio).await
            }
        }
    }
}

pub(crate) struct RetainedContext {
    pub(super) origin: Origin,
    pub media: ScheduledMedia,
    pub previous_audio: Vec<ersatzrs_ffmpeg::SourceAudioStream>,
    pub max_height: Option<u32>,
}

pub(crate) type ProducerContext = RetainedContext;

pub(crate) struct PreparedContext {
    origin: Origin,
    media: ScheduledMedia,
    channel: Channel,
    resolution: Resolution,
}

/// Open only after schedule guards have been released. Re-project against the
/// actual artifact so relative sidecars and embedded Burn use the correct input.
pub(crate) async fn materialize_prepared(
    pool: &DbPool,
    locker: &EntityLocker,
    mut projected: super::dynamic_projection::Snapshot,
    audio: Option<usize>,
    root: &std::path::Path,
) -> Result<super::dynamic_projection::Snapshot> {
    let Some(context) = projected.prepared_remote.take() else {
        return Ok(projected);
    };
    let resolved = Box::pin(
        crate::addon_media_list_playback::resolve_persisted_remote_stream(
            pool,
            context.media.item.media_item_id,
            &crate::addons::AddonRuntimeContext::default(),
            root,
        ),
    )
    .await?;
    let crate::remote_stream::RemoteStreamSource::Prepared(path) = resolved.source else {
        return Err(unavailable());
    };
    let file_identity = artifact_file_identity(&path).await?;
    let mut concrete = selection::project_deferred_item_with_audio(
        pool,
        &context.channel,
        &context.media.item,
        context.media.media_version_id,
        Source::Local {
            path: path.to_str().ok_or_else(unavailable)?.to_owned(),
            in_point_ms: None,
            out_point_ms: None,

            probe_hint: None,
        },
        &context.resolution,
        selection::AudioDirective::Published(audio),
    )
    .await?;
    concrete.item.watermark = projected.projection.item.watermark.take();
    let current = Box::pin(context.origin.snapshot(
        pool,
        locker,
        context.channel.id,
        &context.media.item,
        audio,
    ))
    .await?;
    if current.identity.descriptor != projected.identity.descriptor
        || !current.prepared
        || artifact_file_identity(&path).await? != file_identity
    {
        return Err(unavailable());
    }
    projected.identity.cache_key =
        digest(&json!({"publication":projected.identity.cache_key,"artifact":file_identity}))?;
    projected.projection = concrete;
    Ok(projected)
}

async fn artifact_file_identity(path: &std::path::Path) -> Result<Value> {
    let metadata = tokio::fs::metadata(path).await.map_err(|_| unavailable())?;
    if !metadata.is_file() {
        return Err(unavailable());
    }
    let modified = metadata
        .modified()
        .map_err(|_| unavailable())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| unavailable())?;
    Ok(
        json!({"path":path,"length":metadata.len(),"modified":(modified.as_secs(),modified.subsec_nanos())}),
    )
}

/// Publish direct URLs and prepared artifacts through authenticated callbacks.
/// Producer-backed sources must use their session owner, never the direct URL.
pub(crate) async fn project_publishable(
    pool: &DbPool,
    channel: &Channel,
    media: &ScheduledMedia,
    resolution: &Resolution,
    audio: selection::AudioDirective,
) -> Result<super::dynamic_projection::Snapshot> {
    project_publishable_for_origin(pool, channel, media, resolution, audio, Origin::Persisted).await
}

pub(super) async fn project_publishable_for_origin(
    pool: &DbPool,
    channel: &Channel,
    media: &ScheduledMedia,
    resolution: &Resolution,
    audio: selection::AudioDirective,
    origin: Origin,
) -> Result<super::dynamic_projection::Snapshot> {
    let binding = load_binding(pool, media.remote_stream_id.ok_or_else(unavailable)?).await?;
    if binding.script.is_some() {
        return Err(unavailable());
    }
    let managed = binding.addon_id.is_some() || binding.list_id.is_some();
    if !managed {
        crate::remote_stream::resolve_remote_stream(
            &crate::remote_stream::RemoteStreamDefinition {
                url: binding.url,
                duration: binding.duration,
                liveness: Some(
                    crate::remote_stream::decode_persisted_liveness(
                        binding.liveness,
                        binding.is_live != 0,
                    )
                    .map_err(|_| unavailable())?,
                ),
                is_live: binding.is_live != 0,
                ..Default::default()
            },
            std::path::Path::new("."),
        )
        .map_err(|_| unavailable())?;
    }
    let snapshot = project_for_origin(pool, channel, media, resolution, audio, origin).await?;
    validate_live_subtitles(&snapshot)?;
    validate_producer_subtitles(&snapshot, managed)?;
    let producer = managed && !snapshot.prepared && !snapshot.retained;
    let mut owned_remote = if snapshot.retained || producer {
        Some(RetainedContext {
            origin,
            previous_audio: playback::load_source_audio_streams(pool, media.media_version_id)
                .await?,
            media: ScheduledMedia {
                item: media.item.clone(),
                media_version_id: media.media_version_id,
                media_file_id: media.media_file_id,
                path: media.path.clone(),
                remote_stream_id: media.remote_stream_id,
            },
            max_height: u32::try_from(resolution.height).ok(),
        })
    } else {
        None
    };
    Ok(super::dynamic_projection::Snapshot {
        projection: snapshot.projection,
        identity: snapshot.identity,
        retained_remote: if producer { None } else { owned_remote.take() },
        producer_remote: if producer { owned_remote } else { None },
        prepared_remote: snapshot.prepared.then(|| PreparedContext {
            origin,
            media: snapshot.media,
            channel: channel.clone(),
            resolution: resolution.clone(),
        }),
    })
}

fn validate_producer_subtitles(snapshot: &Snapshot, managed: bool) -> Result<()> {
    if managed
        && !snapshot.prepared
        && !snapshot.retained
        && snapshot
            .projection
            .selected_subtitle
            .as_ref()
            .is_some_and(|subtitle| {
                subtitle.subtitle_kind == crate::metadata::subtitle_kind::EMBEDDED
                    && !subtitle.is_image()
            })
    {
        return Err(RepositoryError::unavailable(
            "Next producer subtitles",
            "selected embedded text subtitles require an independent source/version-bound artifact",
        ));
    }
    Ok(())
}

fn validate_live_subtitles(snapshot: &Snapshot) -> Result<()> {
    // ADR 0100: never extract an entire live multiplexed source for captions.
    let unsupported = snapshot
        .projection
        .selected_subtitle
        .as_ref()
        .is_some_and(|subtitle| {
            !subtitle.is_image()
                && subtitle.subtitle_kind == crate::metadata::subtitle_kind::EMBEDDED
        });
    if (snapshot.liveness == RemoteStreamLiveness::Live
        || super::live_subtitles::is_live(&snapshot.projection.item))
        && unsupported
    {
        return Err(RepositoryError::unavailable(
            "Next live subtitles",
            "selected live embedded text subtitles cannot be streamed by the official Next engine",
        ));
    }
    Ok(())
}

#[derive(sqlx::FromRow, Serialize)]
#[sqlx(rename_all = "PascalCase")]
struct Binding {
    id: i64,
    url: Option<String>,
    addon_id: Option<String>,
    provider_id: Option<String>,
    script: Option<String>,
    liveness: i64,
    is_live: i64,
    duration: Option<String>,
    fallback_query: Option<String>,
    list_id: Option<i64>,
    list_addon_id: Option<String>,
    published_revision: Option<i32>,
}

/// Load one original occurrence and release all scheduler guards before returning.
#[allow(
    clippy::too_many_arguments,
    reason = "Snapshot loading needs the channel, schedule, and published source identity."
)]
pub(crate) async fn snapshot(
    pool: &DbPool,
    locker: &EntityLocker,
    channel_id: i64,
    item_id: i64,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    published_audio: Option<usize>,
) -> Result<Snapshot> {
    let channel = make_repositories(pool.clone())
        .channel
        .get_with_profile(channel_id)
        .await?;
    if channel.streaming_engine != StreamingEngine::Next {
        return Err(unavailable());
    }
    let schedule_read = schedule::read_locked(pool, locker, &channel, start, finish)
        .await?
        .ok_or_else(unavailable)?;
    let mut matches = schedule_read
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
    let result = project(
        pool,
        &channel,
        media,
        &profile.resolution,
        selection::AudioDirective::Published(published_audio),
    )
    .await;
    drop(schedule_read);
    result
}

/// Select once during publication, or validate the frozen choice during callbacks.
/// Only SQL and bounded pure projection occur here; package/filesystem/provider
/// resolution belongs after the caller has dropped its schedule guards.
pub(crate) async fn project(
    pool: &DbPool,
    channel: &Channel,
    media: &ScheduledMedia,
    resolution: &Resolution,
    audio: selection::AudioDirective,
) -> Result<Snapshot> {
    project_for_origin(pool, channel, media, resolution, audio, Origin::Persisted).await
}

pub(super) async fn project_for_origin(
    pool: &DbPool,
    channel: &Channel,
    media: &ScheduledMedia,
    resolution: &Resolution,
    audio: selection::AudioDirective,
    origin: Origin,
) -> Result<Snapshot> {
    let remote_id = media
        .remote_stream_id
        .filter(|id| *id == media.item.media_item_id)
        .ok_or_else(unavailable)?;
    let binding = load_binding(pool, remote_id).await?;
    if binding.script.is_some() {
        return Err(unavailable());
    }
    let (publication, liveness, prepared, retained) = publication_binding(pool, &binding).await?;
    let source = selection_source(&binding, media, liveness, prepared)?;
    let mut projection = selection::project_deferred_item_with_audio(
        pool,
        channel,
        &media.item,
        media.media_version_id,
        source,
        resolution,
        audio,
    )
    .await?;
    let selection_time = crate::channel_playout::ChannelPlayoutContext::for_channel(channel)
        .source_time(media.item.start)?;
    let (graphics, watermarks) = match origin {
        Origin::Persisted => (
            playback::select_playback_graphics(pool, channel, media.item.id, selection_time)
                .await?,
            playback::select_playback_watermarks(
                pool,
                channel,
                media.item.id,
                media.item.disable_watermarks,
                selection_time,
            )
            .await?,
        ),
        Origin::Fallback(_) => (
            playback::select_fallback_graphics(pool, &media.item, selection_time).await?,
            playback::select_fallback_watermarks(pool, channel, &media.item, selection_time)
                .await?,
        ),
    };
    if !graphics.is_empty() {
        return Err(unavailable());
    }
    projection.item.watermark = super::watermark::project(&watermarks)?;
    let material = fingerprint_material(pool, media, &binding, publication).await?;
    let preferences = crate::ffmpeg_config::playback_language_preferences(pool).await?;
    let script = crate::audio_script::script_identity(pool, media.item.media_item_id).await?;
    let profile = playback::load_channel_playback_profile(pool, channel.id)
        .await?
        .ok_or_else(unavailable)?;
    let descriptor = digest(&json!({
        "binding":material, "item":media.item, "selection":projection.item,
        "channel":channel, "resolution":resolution, "profile":profile.profile,
        "audio_script":script, "audio_language":preferences.default_audio_language,
        "subtitle_language":preferences.default_subtitle_language,
        "fallback_subtitle_language":preferences.fallback_subtitle_language,
    }))?;
    Ok(Snapshot {
        media: ScheduledMedia {
            item: media.item.clone(),
            media_version_id: media.media_version_id,
            media_file_id: media.media_file_id,
            path: media.path.clone(),
            remote_stream_id: media.remote_stream_id,
        },
        projection,
        identity: Identity {
            descriptor,
            cache_key: digest(&material)?,
        },
        liveness,
        prepared,
        retained,
    })
}

async fn load_binding(pool: &DbPool, id: i64) -> Result<Binding> {
    sqlx::query_as::<_, Binding>(r#"SELECT s."Id",s."Url",s."AddonId",s."ProviderId",s."Script",s."Liveness",s."IsLive",s."Duration",s."FallbackQuery",
        l."Id" AS "ListId",l."AddonId" AS "ListAddonId",l."PublishedRevision"
        FROM "RemoteStream" s JOIN "MediaItem" m ON m."Id"=s."Id"
        JOIN "LibraryPath" p ON p."Id"=m."LibraryPathId"
        LEFT JOIN "AddonMediaListLibrary" b ON b."LibraryId"=p."LibraryId"
        LEFT JOIN "AddonMediaList" l ON l."Id"=b."AddonMediaListId"
        WHERE s."Id"=?"#).bind(id).fetch_optional(pool).await.repository()?.ok_or_else(unavailable)
}

async fn publication_binding(
    pool: &DbPool,
    binding: &Binding,
) -> Result<(Value, RemoteStreamLiveness, bool, bool)> {
    let legacy_liveness = || {
        crate::remote_stream::decode_persisted_liveness(binding.liveness, binding.is_live != 0)
            .map_err(|_| unavailable())
    };
    let Some(list_id) = binding.list_id else {
        return Ok((Value::Null, legacy_liveness()?, false, false));
    };
    let revision = binding
        .published_revision
        .filter(|revision| *revision > 0)
        .ok_or_else(unavailable)?;
    if binding.addon_id.is_none() || binding.addon_id != binding.list_addon_id {
        return Err(unavailable());
    }
    let provider = binding.provider_id.as_deref().ok_or_else(unavailable)?;
    let url = binding.url.as_deref().ok_or_else(unavailable)?;
    let repository = SqliteAddonMediaListRepository(pool.clone());
    let Some(snapshot) = repository.revision_snapshot(list_id, revision).await? else {
        if !legacy_publication(pool, list_id, revision, provider, binding.id).await? {
            return Err(unavailable());
        }
        return Ok((
            json!({"list":list_id,"revision":revision,"legacy":true}),
            legacy_liveness()?,
            false,
            false,
        ));
    };
    let item = snapshot
        .items
        .iter()
        .find(|item| item.provider_id == provider && item.published)
        .ok_or_else(unavailable)?;
    if item.source_url.as_deref() != Some(url)
        || item.replacement_media_item_id.is_some()
        || item
            .published_media_item_id
            .is_some_and(|id| id != binding.id)
    {
        return Err(unavailable());
    }
    let (liveness, options) = crate::addon_media_list_options::load_published_item_options(
        pool, list_id, revision, provider, url,
    )
    .await?;
    if item.playback_timeline.is_some()
        && (liveness != RemoteStreamLiveness::Finite || item.prepared_artifact_id.is_some())
    {
        return Err(unavailable());
    }
    if let Some(artifact) = item.prepared_artifact_id {
        if liveness != RemoteStreamLiveness::Finite || !snapshot.artifact_ids.contains(&artifact) {
            return Err(unavailable());
        }
    }
    let artifact = if let Some(id) = item.prepared_artifact_id {
        // Match native prepared-file readiness without filesystem access while
        // holding the scheduler snapshot. Opening still uses the native resolver.
        Some(sqlx::query_scalar::<_, String>(r#"SELECT json_array(a."Id",a."RelativePath",a."ByteLength",a."DurationMilliseconds",a."UpdatedAt")
            FROM "AddonMediaListArtifact" a
            WHERE a."Id"=? AND a."AddonMediaListId"=? AND a."Kind"=1 AND a."State"=2
              AND a."RelativePath" IS NOT NULL AND a."ByteLength">0 AND a."DurationMilliseconds">0"#)
            .bind(id).bind(list_id).fetch_optional(pool).await.repository()?.ok_or_else(unavailable)?)
    } else {
        None
    };
    let option_binding = options.map(|options| json!({"schema":options.schema,"values":options.values,"definition":options.definition}));
    Ok((
        json!({"list":list_id,"revision":revision,"digest":snapshot.snapshot_digest,"item":item,"options":option_binding,"prepared_artifact":artifact}),
        liveness,
        item.prepared_artifact_id.is_some(),
        item.playback_timeline.is_some(),
    ))
}

async fn legacy_publication(
    pool: &DbPool,
    list: i64,
    revision: i32,
    provider: &str,
    remote: i64,
) -> Result<bool> {
    sqlx::query_scalar(r#"SELECT EXISTS (
        SELECT 1 FROM "AddonMediaListRevision" r JOIN "AddonMediaListItem" i ON i."AddonMediaListId"=r."AddonMediaListId"
        WHERE r."AddonMediaListId"=? AND r."Revision"=? AND r."OptionSchemaId" IS NULL AND r."OptionValuesJson"='{}'
        AND r."SnapshotVersion" IS NULL AND r."SnapshotDigest" IS NULL
        AND i."ProviderId"=? AND i."MediaItemId"=? AND i."Published"=1
        AND NOT EXISTS (SELECT 1 FROM "AddonMediaListRevisionItemOption" o WHERE o."AddonMediaListRevisionId"=r."Id")
        AND NOT EXISTS (SELECT 1 FROM "AddonMediaListRevisionArtifact" a WHERE a."AddonMediaListRevisionId"=r."Id"))"#)
        .bind(list).bind(revision).bind(provider).bind(remote).fetch_one(pool).await.repository()
}

fn selection_source(
    binding: &Binding,
    media: &ScheduledMedia,
    liveness: RemoteStreamLiveness,
    prepared: bool,
) -> Result<Source> {
    if liveness == RemoteStreamLiveness::Unknown {
        return Err(unavailable());
    }
    if prepared {
        return Ok(Source::Local {
            path: media.path.clone(),
            in_point_ms: None,
            out_point_ms: None,

            probe_hint: None,
        });
    }
    let uri = binding
        .url
        .as_deref()
        .filter(|uri| !uri.trim().is_empty())
        .unwrap_or(&media.path)
        .to_owned();
    if binding.addon_id.is_none()
        && uri
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rtsp://"))
    {
        return Ok(Source::Rtsp {
            uri,
            timeout_us: None,
            probe_hint: None,
        });
    }
    Ok(Source::Http {
        uri,
        is_live: Some(liveness == RemoteStreamLiveness::Live),
        probe_hint: None,
        headers: Vec::new(),
        user_agent: None,
        keep_alive: Some(true),
        reconnect: Some(true),
        reconnect_delay_max: None,
        timeout_us: None,
        in_point_ms: None,
        out_point_ms: None,
    })
}

async fn fingerprint_material(
    pool: &DbPool,
    media: &ScheduledMedia,
    binding: &Binding,
    publication: Value,
) -> Result<Value> {
    let version: String = sqlx::query_scalar(r#"SELECT json_array(v."Id",v."DateUpdated",v."Duration",v."Width",v."Height",v."VideoScanKind",f."Id",f."Path",f."PathHash")
        FROM "MediaVersion" v JOIN "MediaFile" f ON f."MediaVersionId"=v."Id"
        WHERE v."Id"=? AND f."Id"=? AND f."Path"=? AND v."RemoteStreamId"=?"#)
        .bind(media.media_version_id).bind(media.media_file_id).bind(&media.path).bind(binding.id)
        .fetch_optional(pool).await.repository()?.ok_or_else(unavailable)?;
    let streams: Vec<String> = sqlx::query_scalar(r#"SELECT json_array(s."Index",s."Codec",s."Profile",s."Language",s."Title",s."Default",s."Forced",s."Channels",s."MediaStreamKind",s."PixelFormat",s."ColorPrimaries",s."ColorRange",s."ColorSpace",s."ColorTransfer",s."Rotation",s."BitsPerRawSample",s."AttachedPic")
        FROM "MediaStream" s WHERE s."MediaVersionId"=? ORDER BY s."Index",s."Id""#)
        .bind(media.media_version_id).fetch_all(pool).await.repository()?;
    let subtitles: Vec<String> = sqlx::query_scalar(r#"SELECT json_array(s."Id",s."SubtitleKind",s."StreamIndex",s."Codec",s."Language",s."Title",s."Default",s."Forced",s."SDH",s."Path",s."IsExtracted",s."DateUpdated")
        FROM "Subtitle" s JOIN "RemoteStreamMetadata" m ON s."RemoteStreamMetadataId"=m."Id" WHERE m."RemoteStreamId"=? ORDER BY s."Id""#)
        .bind(binding.id).fetch_all(pool).await.repository()?;
    let package: Option<String> = sqlx::query_scalar(r#"SELECT json_array("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","UpdatedAt") FROM "AddonInstallation" WHERE "AddonId"=?"#)
        .bind(&binding.addon_id).fetch_optional(pool).await.repository()?;
    let settings: Vec<String> = sqlx::query_scalar(r#"SELECT json_array("Key","Value","ReferenceKind","Reference") FROM "AddonSetting" WHERE "AddonId"=? ORDER BY "Key""#)
        .bind(&binding.addon_id).fetch_all(pool).await.repository()?;
    Ok(
        json!({"schema":1,"source":binding,"publication":publication,"version":version,"streams":streams,"subtitles":subtitles,"package":package,"settings":settings}),
    )
}

fn digest(value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| unavailable())?;
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(output, "{byte:02x}").map_err(|_| unavailable())?;
    }
    Ok(output)
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next Remote Stream publication",
        "source binding unavailable",
    )
}

#[cfg(test)]
#[path = "remote_projection_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "remote_artifact_tests.rs"]
mod artifact_tests;
