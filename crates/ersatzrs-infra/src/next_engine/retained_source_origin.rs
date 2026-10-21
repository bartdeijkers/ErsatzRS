//! Native observations bound to one retained provider response, never SQL readiness.
use super::{
    dynamic_provider_identity, protocol::Source, schedule::ScheduledMedia, source_teletext,
};
use crate::{
    db::DbPool,
    ffprobe::{
        teletext_probe::{TeletextProbeSnapshot, MAX_TELETEXT_PROBE_STREAMS},
        FfprobeMediaVersion, LocalMediaProbe, RetainedProviderSource,
    },
    teletext_source::{
        acquired_observation, descriptor_candidates_from_acquired, SourceTeletextCaptionCandidate,
        SourceTeletextParents,
    },
};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{Channel, MediaSourceKind, MediaStreamKind};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
struct Stream {
    id: i64,
    index: i32,
    kind: i32,
    codec: Option<String>,
    language: Option<String>,
    title: Option<String>,
    default: bool,
    forced: bool,
    channels: Option<i32>,
    attached_pic: bool,
}

/// Exact database/URL intent before network work, independent from source readiness.
/// Private identities can contain provider locators: hash them, never publish/log.
#[derive(Clone)]
pub(super) struct RetainedSourceIntent {
    item: i64,
    version: i64,
    file: i64,
    path: String,
    source_id: i64,
    provider: MediaSourceKind,
    provider_identity: Value,
    streams: Vec<Stream>,
}
impl RetainedSourceIntent {
    /// Optional caption refusal never prevents an otherwise valid AV publication.
    pub(super) async fn discover(
        pool: &DbPool,
        channel: &Channel,
        media: &ScheduledMedia,
        source: &Source,
    ) -> Option<Self> {
        if let Ok(intent) = Box::pin(Self::load(pool, channel, media, source)).await {
            intent
        } else {
            tracing::warn!(
                channel_id = channel.id,
                item_id = media.item.id,
                "Finite provider caption intent refused; ordinary AV remains available"
            );
            None
        }
    }
    pub(super) async fn load(
        pool: &DbPool,
        channel: &Channel,
        media: &ScheduledMedia,
        source: &Source,
    ) -> Result<Option<Self>> {
        if !source_teletext::selection_enabled(channel, media)
            || !matches!(
                source,
                Source::Http {
                    is_live: None | Some(false),
                    ..
                }
            )
        {
            return Ok(None);
        }
        if !crate::ffmpeg_config::use_embedded_subtitles(pool).await? {
            return Ok(None);
        }
        Box::pin(Self::for_binding(pool, media, source)).await
    }

    /// Recheck already published source identity without changing its caption policy.
    pub(super) async fn for_binding(
        pool: &DbPool,
        media: &ScheduledMedia,
        source: &Source,
    ) -> Result<Option<Self>> {
        if !matches!(
            source,
            Source::Http {
                is_live: None | Some(false),
                ..
            }
        ) {
            return Ok(None);
        }
        let declared: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "MediaStream" WHERE "MediaVersionId"=?
                AND "MediaStreamKind"=3 AND "Codec"='dvb_teletext')"#,
        )
        .bind(media.media_version_id)
        .fetch_one(pool)
        .await
        .map_err(|_| unavailable())?;
        if !declared {
            return Ok(None);
        }
        let identity = dynamic_provider_identity::identity(pool, media, source).await?;
        let provider = match identity["kind"].as_str() {
            Some("jellyfin") => MediaSourceKind::Jellyfin,
            Some("emby") => MediaSourceKind::Emby,
            Some("plex") => MediaSourceKind::Plex,
            _ => return Err(unavailable()),
        };
        if provider != MediaSourceKind::Plex
            && identity["provider_media_source_id"]
                .as_str()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Ok(None);
        }
        let source_id = identity["source_id"]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or_else(unavailable)?;
        let streams = load_layout(pool, media).await?;
        if !streams
            .iter()
            .any(|stream| stream.kind == 3 && stream.codec.as_deref() == Some("dvb_teletext"))
        {
            return Ok(None);
        }
        if dynamic_provider_identity::identity(pool, media, source).await? != identity {
            return Err(unavailable());
        }
        Ok(Some(Self {
            item: media.item.media_item_id,
            version: media.media_version_id,
            file: media.media_file_id,
            path: media.path.clone(),
            source_id,
            provider,
            provider_identity: identity,
            streams,
        }))
    }
    pub(super) fn identity(&self) -> Value {
        json!({"item":self.item,"version":self.version,"file":self.file,"path":self.path,
            "provider":self.provider_identity,"streams":self.streams})
    }
    fn matches_media(&self, media: &ScheduledMedia) -> bool {
        self.item == media.item.media_item_id
            && self.version == media.media_version_id
            && self.file == media.media_file_id
            && self.path == media.path
    }
}

/// One actual native report and descriptor set from the same retained bytes.
/// Construct only from the report acquired by the host on this exact source.
pub(super) struct RetainedOrigin {
    // Close native metadata handles before releasing retained bytes and quota.
    probe: LocalMediaProbe,
    source: Arc<RetainedProviderSource>,
    intent: RetainedSourceIntent,
    candidates: Vec<SourceTeletextCaptionCandidate>,
}
impl RetainedOrigin {
    pub(super) fn qualify(
        intent: RetainedSourceIntent,
        source: Arc<RetainedProviderSource>,
        probe: LocalMediaProbe,
    ) -> Result<Arc<Self>> {
        validate_layout(&intent.streams, &probe.media)?;
        let acquired = probe.teletext.as_ref().ok_or_else(unavailable)?;
        let TeletextProbeSnapshot::Available(streams) = acquired.snapshot() else {
            return Err(unavailable());
        };
        let mut observations = Vec::new();
        for stream in streams {
            let stored = intent
                .streams
                .iter()
                .find(|stored| {
                    stored.index == stream.stream_index
                        && stored.kind == 3
                        && stored.codec.as_deref() == Some("dvb_teletext")
                })
                .ok_or_else(unavailable)?;
            let parents = SourceTeletextParents {
                media_version_id: intent.version,
                media_file_id: intent.file,
                media_stream_id: stored.id,
            };
            let observation =
                acquired_observation(intent.source_id, intent.provider, parents, acquired, stream)?;
            observations.push((parents, observation));
        }
        let candidates = descriptor_candidates_from_acquired(&observations)?;
        Ok(Arc::new(Self {
            probe,
            source,
            intent,
            candidates,
        }))
    }
    pub(super) fn source(&self) -> Arc<RetainedProviderSource> {
        self.source.clone()
    }
    pub(super) fn media(&self) -> &FfprobeMediaVersion {
        &self.probe.media
    }
    pub(super) fn candidates(&self) -> &[SourceTeletextCaptionCandidate] {
        &self.candidates
    }
    pub(super) fn matches_media(&self, media: &ScheduledMedia) -> bool {
        self.intent.matches_media(media)
    }
    pub(super) fn identity(&self) -> Value {
        json!({"intent":self.intent.identity(),"source_bytes":self.source.length,"source_sha256":self.source.digest})
    }
}
async fn load_layout(pool: &DbPool, media: &ScheduledMedia) -> Result<Vec<Stream>> {
    let rows:Vec<Stream>=sqlx::query_as(r#"SELECT s."Id" AS id,s."Index" AS "index",s."MediaStreamKind" AS kind,s."Codec" AS codec,
        s."Language" AS language,s."Title" AS title,s."Default" AS "default",s."Forced" AS forced,
        s."Channels" AS channels,s."AttachedPic" AS attached_pic
        FROM "MediaStream" s JOIN "MediaVersion" v ON v."Id"=s."MediaVersionId"
        JOIN "MediaFile" f ON f."MediaVersionId"=v."Id"
        WHERE v."Id"=? AND f."Id"=? AND f."Path"=? COLLATE BINARY AND (v."MovieId"=? OR v."EpisodeId"=?)
          AND ((v."MovieId" IS NOT NULL)+(v."EpisodeId" IS NOT NULL)+(v."MusicVideoId" IS NOT NULL)
             +(v."OtherVideoId" IS NOT NULL)+(v."SongId" IS NOT NULL)+(v."ImageId" IS NOT NULL)+(v."RemoteStreamId" IS NOT NULL))=1
          AND s."MediaStreamKind"!=5 ORDER BY s."Index",s."Id" LIMIT ?"#)
        .bind(media.media_version_id).bind(media.media_file_id).bind(&media.path)
        .bind(media.item.media_item_id).bind(media.item.media_item_id)
        .bind(i64::try_from(MAX_TELETEXT_PROBE_STREAMS+1).map_err(|_|unavailable())?)
        .fetch_all(pool).await.map_err(|_|unavailable())?;
    let mut indexes = BTreeSet::new();
    if rows.is_empty()
        || rows.len() > MAX_TELETEXT_PROBE_STREAMS
        || rows.iter().any(|row| {
            row.id <= 0
                || row.index < 0
                || !(1..=4).contains(&row.kind)
                || !indexes.insert(row.index)
        })
    {
        return Err(unavailable());
    }
    Ok(rows)
}
fn kind_code(kind: MediaStreamKind) -> i32 {
    match kind {
        MediaStreamKind::Video => 1,
        MediaStreamKind::Audio => 2,
        MediaStreamKind::Subtitle => 3,
        MediaStreamKind::Attachment => 4,
        MediaStreamKind::ExternalSubtitle => 5,
    }
}
fn validate_layout(stored: &[Stream], probe: &FfprobeMediaVersion) -> Result<()> {
    let mut indexes = BTreeSet::new();
    if probe.streams.len() != stored.len()
        || probe.streams.iter().any(|stream| {
            !indexes.insert(stream.index)
                || !stored.iter().any(|old| {
                    old.index == stream.index
                        && old.kind == kind_code(stream.kind)
                        && old.codec == stream.codec
                        && selection_semantics_match(old, stream)
                })
        })
    {
        return Err(unavailable());
    }
    Ok(())
}
fn selection_semantics_match(stored: &Stream, native: &ersatzrs_domain::MediaStream) -> bool {
    match native.kind {
        MediaStreamKind::Video => stored.attached_pic == native.attached_pic,
        MediaStreamKind::Audio => {
            let language = |value: Option<&str>| {
                value.and_then(ersatzrs_core::language_codes::canonical_language_code)
            };
            language(stored.language.as_deref()) == language(native.language.as_deref())
                && stored.title == native.title
                && stored.default == native.default
                && stored.forced == native.forced
                && stored.channels == native.channels
        }
        _ => true,
    }
}
fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "retained provider source",
        "native source association is unavailable",
    )
}

#[cfg(test)]
#[path = "retained_source_origin/tests.rs"]
mod tests;
