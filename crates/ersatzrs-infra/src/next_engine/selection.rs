//! Resolve persisted stream choices through the native playback selectors.

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{Channel, ChannelStreamSelectorMode, PlayoutItem, Resolution, StreamingMode};

use crate::db::DbPool;
use crate::ffmpeg_config::{playback_language_preferences, PlaybackLanguagePreferences};
use crate::playback::{
    load_source_audio_streams, load_source_video_stream_by_index,
    select_custom_playback_streams_with_subtitle_candidates, select_next_pinned_playback_subtitle,
    select_next_playback_subtitle_for_version, select_playback_audio_stream_index,
    subtitle_input_for_stream, CustomStreamSelection, PlaybackSubtitle,
};
use crate::repositories::SqlxResultExt as _;
use ersatzrs_ffmpeg::{OutputFormat, SourceAudioStream, SourceVideoStream};
use sqlx::Row as _;

use super::protocol as wire;

#[path = "subtitle_availability.rs"]
mod availability;

/// Projection requiring sidecar preparation before it can be published.
/// Deliberately no Debug: source paths and subtitle metadata may be private.
pub(crate) struct DeferredProjection {
    pub item: wire::PlayoutItem,
    pub original_source: wire::Source,
    pub canvas: Option<Box<super::graphics::CanvasProjection>>,
    pub song: Option<Box<super::song_background::SongProjection>>,
    pub pending_burn: Option<PendingBurn>,
    pub selected_subtitle: Option<PlaybackSubtitle>,
}

impl DeferredProjection {
    pub(super) fn publish_graphics(
        &mut self,
        relay: &super::media::MediaRelay,
        session: Option<uuid::Uuid>,
        start: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        if let Some(song) = &self.song {
            let source = song.publish(relay, session, start)?;
            self.item
                .tracks
                .as_mut()
                .and_then(|tracks| tracks.video.as_mut())
                .ok_or_else(|| invalid("missing song video track"))?
                .source = Some(source);
        }
        if let Some(canvas) = &self.canvas {
            self.item.graphics = vec![canvas.publish(relay, session, start)?];
        }
        Ok(())
    }
}

/// Exact selected embedded/provider track and its original, sought media source.
pub(crate) struct PendingBurn {
    pub subtitle: PlaybackSubtitle,
    pub source: wire::Source,
    pub source_text: Option<Box<super::source_teletext::PendingText>>,
}

pub(crate) struct TroubleshootingSelection<'a> {
    pub channel: &'a Channel,
    pub item: &'a PlayoutItem,
    pub media_version_id: i64,
    pub source: wire::Source,
    pub resolution: &'a Resolution,
    /// None keeps channel preferences; zero means explicitly disabled.
    pub subtitle_id: Option<i64>,
    pub diagnostics: Option<super::diagnostics::SelectionLog>,
    /// Prepared media is not materialized yet; never publish this projection directly.
    pub source_pending: bool,
}

/// Restrict troubleshooting subtitle candidates before normal track selection.
pub(crate) async fn project_troubleshooting_item(
    pool: &DbPool,
    request: TroubleshootingSelection<'_>,
) -> Result<DeferredProjection> {
    project_troubleshooting_item_with_audio(pool, request, AudioDirective::Select).await
}

pub(crate) async fn project_troubleshooting_item_with_audio(
    pool: &DbPool,
    request: TroubleshootingSelection<'_>,
    audio: AudioDirective,
) -> Result<DeferredProjection> {
    project_deferred_item_selected(
        pool,
        request.channel,
        request.item,
        request.media_version_id,
        request.source,
        request.resolution,
        SelectionDirectives {
            audio,
            subtitle_id: request.subtitle_id,
            diagnostics: request.diagnostics,
            source_pending: request.source_pending,
            source_text: None,
        },
    )
    .await
}

/// Project one persisted item's timing and tracks using its resolved media version/source.
///
/// The caller resolves source access and applies mirror timestamps before this
/// conversion. The source must use the original media timeline; this projection
/// applies the item's in/out points. The coordinator must attach graphics and
/// watermarks separately before claiming complete item parity. Stream preferences
/// use the native item/channel/global precedence.
/// Exact pins fail when stale; they never fall back to language preferences.
/// Explicit track sources and no general source prevent Next from re-enabling
/// subtitles. Missing source audio and pinned silence use a finite probe hint.
///
/// # Errors
/// Returns selection/database failures, invalid timing or stale pinned indexes.
pub async fn project_item(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
    media_version_id: i64,
    source: wire::Source,
    resolution: &Resolution,
) -> Result<wire::PlayoutItem> {
    let mut projected =
        project_deferred_item(pool, channel, item, media_version_id, source, resolution).await?;
    if projected.song.is_some() {
        return Err(invalid(
            "song background requires authenticated deferred publication",
        ));
    }
    if let Some(pending) = projected.pending_burn {
        if pending.subtitle.subtitle_kind == crate::metadata::subtitle_kind::SIDECAR {
            return Err(invalid("provider subtitles require deferred preparation"));
        }
        let track = subtitle_track(
            &pending.subtitle,
            &pending.source,
            channel.next_engine_text_subtitle_mode,
        )?;
        projected
            .item
            .tracks
            .as_mut()
            .ok_or_else(|| invalid("missing tracks"))?
            .subtitle = Some(track);
    }
    Ok(projected.item)
}

/// Select tracks without running extraction. Pending projections must not be
/// published until their exact embedded/provider text track has a prepared sidecar.
/// Legacy extraction metadata does not establish version-scoped cache identity;
/// the preparation caller must validate that separately.
pub(crate) async fn project_deferred_item(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
    media_version_id: i64,
    source: wire::Source,
    resolution: &Resolution,
) -> Result<DeferredProjection> {
    project_deferred_item_with_audio(
        pool,
        channel,
        item,
        media_version_id,
        source,
        resolution,
        AudioDirective::Select,
    )
    .await
}

/// Whether this projection selects audio or preserves an authenticated choice.
pub(crate) enum AudioDirective {
    Select,
    Published(Option<usize>),
}

/// Reuse an authenticated publication's audio choice without reexecuting JavaScript.
/// `Published(None)` preserves silence; `Select` performs ordinary selection.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn project_deferred_item_with_audio(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
    media_version_id: i64,
    source: wire::Source,
    resolution: &Resolution,
    audio: AudioDirective,
) -> Result<DeferredProjection> {
    project_deferred_item_selected(
        pool,
        channel,
        item,
        media_version_id,
        source,
        resolution,
        SelectionDirectives {
            audio,
            subtitle_id: None,
            diagnostics: None,
            source_pending: false,
            source_text: None,
        },
    )
    .await
}

struct SelectionDirectives {
    audio: AudioDirective,
    subtitle_id: Option<i64>,
    diagnostics: Option<super::diagnostics::SelectionLog>,
    source_pending: bool,
    source_text: Option<super::source_teletext::Selection>,
}

/// Exact scheduled file identity, retained before any local path replacement.
pub(super) async fn project_scheduled_item(
    pool: &DbPool,
    channel: &Channel,
    media: &super::schedule::ScheduledMedia,
    source: wire::Source,
    resolution: &Resolution,
    audio: AudioDirective,
) -> Result<DeferredProjection> {
    project_scheduled_item_with_source_text(pool, channel, media, source, resolution, audio, true)
        .await
}

/// Reuse ordinary selection only after the preferred page was proven absent.
pub(super) async fn project_scheduled_item_with_source_text(
    pool: &DbPool,
    channel: &Channel,
    media: &super::schedule::ScheduledMedia,
    source: wire::Source,
    resolution: &Resolution,
    audio: AudioDirective,
    include_source_text: bool,
) -> Result<DeferredProjection> {
    let source_text = if include_source_text {
        Box::pin(super::source_teletext::pending(
            pool, channel, media, &source,
        ))
        .await?
    } else {
        None
    };
    Box::pin(project_deferred_item_selected(
        pool,
        channel,
        &media.item,
        media.media_version_id,
        source,
        resolution,
        SelectionDirectives {
            audio,
            subtitle_id: None,
            diagnostics: None,
            source_pending: false,
            source_text,
        },
    ))
    .await
}

async fn project_deferred_item_selected(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
    media_version_id: i64,
    mut source: wire::Source,
    resolution: &Resolution,
    directives: SelectionDirectives,
) -> Result<DeferredProjection> {
    let duration_ms = u64::try_from((item.finish - item.start).num_milliseconds())
        .ok()
        .filter(|duration| *duration > 0)
        .ok_or_else(|| invalid("invalid item duration"))?;
    apply_item_seek(&mut source, item)?;
    let source_pending = directives.source_pending;
    let explicit_subtitle = directives.subtitle_id.is_some_and(|id| id > 0)
        || item
            .pinned_stream_selection
            .as_ref()
            .is_some_and(|pin| pin.subtitle_stream_index.is_some());
    let selected =
        select_projection_tracks(pool, channel, item, media_version_id, directives).await?;
    let song = if selected.generated_video && is_song(pool, item.media_item_id).await? {
        Some(Box::new(
            super::song_background::SongProjection::new(
                pool,
                channel,
                item,
                media_version_id,
                resolution,
            )
            .await?,
        ))
    } else {
        None
    };
    let video = if selected.generated_video {
        wire::Track {
            source: if song.is_some() {
                None
            } else {
                Some(black_video(resolution, duration_ms)?)
            },
            stream_index: Some(0),
        }
    } else {
        wire::Track {
            source: Some(source.clone()),
            stream_index: selected.video.map(index_u32).transpose()?,
        }
    };
    let audio = match selected.audio {
        Some(index) => wire::Track {
            source: Some(source.clone()),
            stream_index: Some(index_u32(index)?),
        },
        None => silent_audio(duration_ms),
    };
    let mut selected_subtitle = selected.subtitle;
    let provider = availability::retain(
        pool,
        &mut selected_subtitle,
        &source,
        explicit_subtitle,
        item.media_item_id,
        source_pending,
    )
    .await?;
    let (subtitle, pending_burn) = if let Some(text) = selected.source_text {
        project_source_text_subtitle(text, selected_subtitle.as_ref(), &source)?
    } else {
        project_deferred_subtitle(
            provider,
            selected_subtitle.as_ref(),
            &source,
            channel.next_engine_text_subtitle_mode,
            item,
        )?
    };
    Ok(DeferredProjection {
        original_source: source,
        canvas: None,
        song,
        pending_burn,
        selected_subtitle,
        item: wire::PlayoutItem {
            watermark: None,
            graphics: Vec::new(),
            id: item.id.to_string(),
            start: item.start,
            finish: item.finish,
            source: None,
            tracks: Some(wire::Tracks {
                video: Some(video),
                audio: Some(audio),
                subtitle,
            }),
        },
    })
}

fn project_source_text_subtitle(
    text: super::source_teletext::PendingText,
    subtitle: Option<&PlaybackSubtitle>,
    source: &wire::Source,
) -> Result<(Option<wire::Track>, Option<PendingBurn>)> {
    Ok((
        Some(wire::Track {
            source: Some(source.clone()),
            stream_index: Some(0),
        }),
        Some(PendingBurn {
            subtitle: subtitle
                .cloned()
                .ok_or_else(|| invalid("source caption changed"))?,
            source: source.clone(),
            source_text: Some(Box::new(text)),
        }),
    ))
}

struct SelectedProjectionTracks {
    video: Option<usize>,
    generated_video: bool,
    audio: Option<usize>,
    subtitle: Option<PlaybackSubtitle>,
    source_text: Option<super::source_teletext::PendingText>,
}

async fn select_projection_tracks(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
    media_version_id: i64,
    directives: SelectionDirectives,
) -> Result<SelectedProjectionTracks> {
    let pin = item.pinned_stream_selection;
    if pin.is_some_and(|pin| pin.media_version_id != media_version_id) {
        return Err(invalid("resolved version differs from pinned version"));
    }
    let generated_video = has_generated_video(pool, media_version_id).await?;
    let video = if generated_video {
        if pin.is_some_and(|pin| pin.video_stream_index != 0) {
            return Err(invalid("generated video has only stream zero"));
        }
        None
    } else {
        source_video(
            pool,
            media_version_id,
            pin.map(|pin| pin.video_stream_index),
        )
        .await?
    };
    let audio_streams = load_source_audio_streams(pool, media_version_id).await?;
    let preferences = playback_language_preferences(pool).await?;
    if let Some(log) = &directives.diagnostics {
        log.debug(&format!("Media {} version {media_version_id}: selector={:?}, pinned={}, audio language={:?}, subtitle language={:?}, explicit subtitle={:?}",
            item.media_item_id, channel.stream_selector_mode, pin.is_some(),
            channel.preferred_audio_language_code.as_ref().or(preferences.default_audio_language.as_ref()),
            channel.preferred_subtitle_language_code.as_ref().or(preferences.default_subtitle_language.as_ref()), directives.subtitle_id));
    }
    let explicit = troubleshooting_subtitles(
        pool,
        item.media_item_id,
        media_version_id,
        directives.subtitle_id,
    )
    .await?;
    let custom =
        if pin.is_none() && channel.stream_selector_mode == ChannelStreamSelectorMode::Custom {
            select_custom_playback_streams_with_subtitle_candidates(
                pool,
                media_version_id,
                item.media_item_id,
                channel,
                item.start,
                StreamingMode::HttpLiveStreamingSegmenter,
                crate::playback::CustomSelectionContext {
                    subtitles: explicit.as_deref(),
                    diagnostics: directives.diagnostics.as_ref(),
                },
            )
            .await?
        } else {
            None
        };
    let audio_index = if let AudioDirective::Published(audio) = directives.audio {
        validated_published_audio(item, audio, &audio_streams)?
    } else {
        selected_audio_with_script(
            pool,
            channel,
            item,
            media_version_id,
            &preferences,
            custom.as_ref(),
            &audio_streams,
        )
        .await?
    };
    let source_text = directives.source_text;
    let subtitle = if let Some(source) = &source_text {
        Some(source.subtitle.clone())
    } else if let Some(explicit) = explicit {
        custom.as_ref().map_or_else(
            || explicit.into_iter().next(),
            |selection| selection.subtitle.clone(),
        )
    } else {
        selected_subtitle(
            pool,
            channel,
            item,
            media_version_id,
            custom.as_ref(),
            &preferences,
        )
        .await?
    };
    if let Some(log) = &directives.diagnostics {
        log.debug(&format!("Selected video stream={:?}, audio stream={audio_index:?}, subtitle id={:?}; custom rule matched={}",
            video.as_ref().map(|stream| stream.index), subtitle.as_ref().map(|subtitle| subtitle.id), custom.is_some()));
    }
    Ok(SelectedProjectionTracks {
        video: video.map(|stream| stream.index),
        generated_video,
        audio: audio_index,
        subtitle,
        source_text: source_text.map(|source| source.text),
    })
}

fn validated_published_audio(
    item: &PlayoutItem,
    audio: Option<usize>,
    streams: &[SourceAudioStream],
) -> Result<Option<usize>> {
    let pinned_audio = audio
        .map(i32::try_from)
        .transpose()
        .map_err(|_| invalid("published audio index is invalid"))?;
    if item
        .pinned_stream_selection
        .is_some_and(|pin| pin.audio_stream_index != pinned_audio)
        || audio.is_some_and(|index| !streams.iter().any(|stream| stream.index == index))
    {
        return Err(invalid("published audio binding changed"));
    }
    Ok(audio)
}

/// Upstream `PlayoutItemConverter` classifies raw stream presence before selection.
/// Pinned silence and attached cover art cannot turn an audio-only programme
/// back into a source-video item. Unknown streams retain ordinary probing.
pub(super) async fn has_generated_video(pool: &DbPool, version_id: i64) -> Result<bool> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "MediaStream" WHERE "MediaVersionId"=? AND "MediaStreamKind"=2)
             AND NOT EXISTS(SELECT 1 FROM "MediaStream" WHERE "MediaVersionId"=?
                            AND "MediaStreamKind"=1 AND "AttachedPic"=0)"#,
    )
    .bind(version_id).bind(version_id).fetch_one(pool).await.repository()
}

async fn is_song(pool: &DbPool, media_id: i64) -> Result<bool> {
    sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "Song" WHERE "Id"=?)"#)
        .bind(media_id)
        .fetch_one(pool)
        .await
        .repository()
}

async fn eligible_video_index(
    pool: &DbPool,
    version_id: i64,
    pinned: Option<i32>,
) -> Result<Option<(i32, Option<String>)>> {
    let row = sqlx::query(
        r#"SELECT "Index", "Codec" FROM "MediaStream"
           WHERE "MediaVersionId"=? AND "MediaStreamKind"=1 AND "AttachedPic"=0
             AND (? IS NULL OR "Index"=?) ORDER BY "Index", "Id" LIMIT 1"#,
    )
    .bind(version_id)
    .bind(pinned)
    .bind(pinned)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        if pinned.is_some() {
            return Err(invalid(
                "pinned source video stream is unavailable or attached artwork",
            ));
        }
        return Ok(None);
    };
    let index = i32::try_from(row.try_get::<i64, _>("Index").repository()?)
        .map_err(|_| invalid("source video index is invalid"))?;
    if index < 0 {
        return Err(invalid("source video index is invalid"));
    }
    Ok(Some((index, row.try_get("Codec").repository()?)))
}

pub(super) async fn source_video(
    pool: &DbPool,
    version_id: i64,
    pinned: Option<i32>,
) -> Result<Option<SourceVideoStream>> {
    match eligible_video_index(pool, version_id, pinned).await? {
        Some((index, codec)) if codec.is_some() || pinned.is_some() => {
            load_source_video_stream_by_index(pool, version_id, Some(index)).await
        }
        _ => Ok(None),
    }
}

fn black_video(resolution: &Resolution, duration_ms: u64) -> Result<wire::Source> {
    let hint = super::probe_hint::generated_video(resolution, duration_ms, "rawvideo", None)?;
    Ok(wire::Source::Lavfi {
        params: format!("color=c=black:s={}x{}", resolution.width, resolution.height),
        probe_hint: Some(hint),
    })
}

async fn troubleshooting_subtitles(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    subtitle_id: Option<i64>,
) -> Result<Option<Vec<PlaybackSubtitle>>> {
    match subtitle_id {
        None => Ok(None),
        Some(0) => Ok(Some(Vec::new())),
        Some(id) if id > 0 => Ok(Some(vec![
            crate::playback::select_next_troubleshooting_subtitle(
                pool,
                media_item_id,
                media_version_id,
                id,
            )
            .await?,
        ])),
        Some(_) => Err(invalid("troubleshooting subtitle identity is invalid")),
    }
}

fn project_deferred_subtitle(
    provider: bool,
    selected_subtitle: Option<&PlaybackSubtitle>,
    source: &wire::Source,
    mode: ersatzrs_domain::NextEngineTextSubtitleMode,
    item: &PlayoutItem,
) -> Result<(Option<wire::Track>, Option<PendingBurn>)> {
    Ok(if provider {
        let selected = selected_subtitle.ok_or_else(|| invalid("provider subtitle changed"))?;
        // A placeholder is never published directly: the existing deferred
        // callback fetches and validates the exact provider subtitle first.
        (
            Some(wire::Track {
                source: Some(source.clone()),
                stream_index: Some(0),
            }),
            Some(PendingBurn {
                subtitle: selected.clone(),
                source: source.clone(),
                source_text: None,
            }),
        )
    } else {
        project_subtitle(selected_subtitle, source, mode, item)?
    })
}

fn project_subtitle(
    subtitle: Option<&PlaybackSubtitle>,
    source: &wire::Source,
    mode: ersatzrs_domain::NextEngineTextSubtitleMode,
    item: &PlayoutItem,
) -> Result<(Option<wire::Track>, Option<PendingBurn>)> {
    let standalone = subtitle
        .is_some_and(|selected| selected.subtitle_kind != crate::metadata::subtitle_kind::EMBEDDED);
    let pending_burn = subtitle
        .filter(|subtitle| {
            subtitle.subtitle_kind == crate::metadata::subtitle_kind::EMBEDDED
                && !subtitle.is_image()
                && mode == ersatzrs_domain::NextEngineTextSubtitleMode::Burn
        })
        .map(|subtitle| PendingBurn {
            subtitle: subtitle.clone(),
            source: source.clone(),
            source_text: None,
        });
    let mut subtitle = if let Some(pending) = &pending_burn {
        Some(wire::Track {
            source: Some(pending.source.clone()),
            stream_index: Some(
                u32::try_from(pending.subtitle.stream_index)
                    .map_err(|_| invalid("invalid embedded subtitle index"))?,
            ),
        })
    } else {
        subtitle
            .map(|subtitle| subtitle_track(subtitle, source, mode))
            .transpose()?
    };
    // A standalone sidecar follows the programme's source clock even when the
    // video is live. Next clears live input seeks; the callback rebases these
    // independently readable cues before handing them to the engine.
    if standalone {
        if let Some(source) = subtitle.as_mut().and_then(|track| track.source.as_mut()) {
            apply_item_seek(source, item)?;
        }
    }
    Ok((subtitle, pending_burn))
}

pub(crate) fn silent_audio(duration_ms: u64) -> wire::Track {
    wire::Track {
        source: Some(wire::Source::Lavfi {
            params: "anullsrc=channel_layout=stereo:sample_rate=48000".to_owned(),
            probe_hint: Some(wire::ProbeHint {
                video: Vec::new(),
                subtitle: Vec::new(),
                format_name: Some("mpegts".to_owned()),
                duration_ms: Some(duration_ms),
                audio: vec![wire::AudioProbeHint {
                    stream_index: 0,
                    codec: "pcm_s16le".to_owned(),
                    channels: 2,
                }],
            }),
        }),
        stream_index: Some(0),
    }
}

fn needs_burn_preparation(
    subtitle: &PlaybackSubtitle,
    mode: ersatzrs_domain::NextEngineTextSubtitleMode,
) -> bool {
    subtitle.subtitle_kind == crate::metadata::subtitle_kind::EMBEDDED
        && !subtitle.is_image()
        && mode == ersatzrs_domain::NextEngineTextSubtitleMode::Burn
        && (!subtitle.is_extracted
            || subtitle
                .path
                .as_deref()
                .is_none_or(|path| path.trim().is_empty()))
}

fn subtitle_track(
    subtitle: &PlaybackSubtitle,
    source: &wire::Source,
    mode: ersatzrs_domain::NextEngineTextSubtitleMode,
) -> Result<wire::Track> {
    if subtitle.codec.as_deref() == Some("dvb_teletext") {
        return Err(invalid(
            "source teletext requires finite page-qualified preparation",
        ));
    }
    let embedded = subtitle.subtitle_kind == crate::metadata::subtitle_kind::EMBEDDED;
    let burn_embedded_text = embedded
        && !subtitle.is_image()
        && mode == ersatzrs_domain::NextEngineTextSubtitleMode::Burn;
    if needs_burn_preparation(subtitle, mode) {
        return Err(invalid(
            "embedded text subtitles require an extracted sidecar for Next burn mode",
        ));
    }
    if embedded && !burn_embedded_text {
        return Ok(wire::Track {
            source: Some(source.clone()),
            stream_index: Some(
                u32::try_from(subtitle.stream_index)
                    .map_err(|_| invalid("invalid embedded subtitle index"))?,
            ),
        });
    }
    if !embedded && subtitle.is_image() {
        return Err(invalid("external bitmap subtitles are unsupported by Next"));
    }
    let (media_path, in_point_ms, out_point_ms) = match source {
        wire::Source::Local {
            path,
            in_point_ms,
            out_point_ms,
            ..
        } => (path, *in_point_ms, *out_point_ms),
        wire::Source::Http {
            uri,
            in_point_ms,
            out_point_ms,
            ..
        } => (uri, *in_point_ms, *out_point_ms),
        wire::Source::Rtsp { uri, .. } => (uri, None, None),
        wire::Source::Lavfi { .. } | wire::Source::Dynamic { .. } => {
            return Err(invalid("subtitle requires a media source"));
        }
    };
    let input = subtitle_input_for_stream(
        subtitle,
        media_path,
        OutputFormat::MpegTs,
        StreamingMode::HttpLiveStreamingSegmenter,
    )
    .ok_or_else(|| invalid("selected subtitle is unavailable"))?;
    let subtitle_source = if &input.path == media_path {
        source.clone()
    } else if crate::playback::subtitle_is_http(&input.path) {
        wire::Source::Http {
            probe_hint: None,
            is_live: None,
            uri: input.path,
            headers: Vec::new(),
            user_agent: None,
            keep_alive: Some(false),
            reconnect: None,
            reconnect_delay_max: None,
            timeout_us: None,
            in_point_ms,
            out_point_ms,
        }
    } else {
        wire::Source::Local {
            path: input.path,
            in_point_ms,
            out_point_ms,
            probe_hint: None,
        }
    };
    Ok(wire::Track {
        source: Some(subtitle_source),
        stream_index: input
            .streams
            .first()
            .map(|stream| index_u32(stream.index))
            .transpose()?,
    })
}

fn index_u32(index: usize) -> Result<u32> {
    u32::try_from(index).map_err(|_| invalid("stream index exceeds Next protocol range"))
}

fn invalid(message: &str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "Next item stream selection",
        value: message.to_owned(),
    }
}

async fn selected_audio_with_script(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
    media_version_id: i64,
    preferences: &PlaybackLanguagePreferences,
    custom: Option<&CustomStreamSelection>,
    audio_streams: &[SourceAudioStream],
) -> Result<Option<usize>> {
    let script_audio = if item.pinned_stream_selection.is_none()
        && custom
            .as_ref()
            .and_then(|selection| selection.audio_stream_index)
            .is_none()
    {
        let title = item
            .preferred_audio_title
            .as_deref()
            .or(channel.preferred_audio_title.as_deref());
        let language = preferences.audio_language(
            item.preferred_audio_language_code.as_deref(),
            channel.preferred_audio_language_code.as_deref(),
            title,
            StreamingMode::HttpLiveStreamingSegmenter,
        );
        crate::audio_script::select(
            pool,
            channel,
            item.media_item_id,
            media_version_id,
            crate::audio_script::AudioScriptPreferences {
                language: language.as_deref(),
                title,
                streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
            },
            audio_streams,
        )
        .await
    } else {
        None
    };
    if script_audio.is_some() {
        Ok(script_audio)
    } else {
        selected_audio(item, channel, preferences, custom, audio_streams)
    }
}

fn selected_audio(
    item: &PlayoutItem,
    channel: &Channel,
    preferences: &PlaybackLanguagePreferences,
    custom: Option<&CustomStreamSelection>,
    audio_streams: &[SourceAudioStream],
) -> Result<Option<usize>> {
    let pin = item.pinned_stream_selection;
    let title = item
        .preferred_audio_title
        .as_deref()
        .or(channel.preferred_audio_title.as_deref());
    let language = preferences.audio_language(
        item.preferred_audio_language_code.as_deref(),
        channel.preferred_audio_language_code.as_deref(),
        title,
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    let audio_index = if let Some(pin) = pin {
        pin.audio_stream_index
            .map(|index| usize::try_from(index).map_err(|_| invalid("invalid pinned audio index")))
            .transpose()?
    } else {
        custom
            .and_then(|selection| selection.audio_stream_index)
            .or_else(|| {
                select_playback_audio_stream_index(audio_streams, language.as_deref(), title)
            })
            .or_else(|| audio_streams.first().map(|stream| stream.index))
    };
    if audio_index.is_some_and(|index| !audio_streams.iter().any(|stream| stream.index == index)) {
        return Err(invalid("selected audio stream is unavailable"));
    }
    Ok(audio_index)
}

async fn selected_subtitle(
    pool: &DbPool,
    channel: &Channel,
    item: &PlayoutItem,
    media_version_id: i64,
    custom: Option<&CustomStreamSelection>,
    preferences: &PlaybackLanguagePreferences,
) -> Result<Option<PlaybackSubtitle>> {
    let subtitle = if let Some(pin) = item.pinned_stream_selection {
        match pin.subtitle_stream_index {
            Some(index) => Some(
                select_next_pinned_playback_subtitle(
                    pool,
                    item.media_item_id,
                    media_version_id,
                    index,
                )
                .await?,
            ),
            None => None,
        }
    } else if let Some(custom) = custom {
        custom.subtitle.clone()
    } else {
        let languages = preferences.subtitle_languages(
            item.preferred_subtitle_language_code.as_deref(),
            channel.preferred_subtitle_language_code.as_deref(),
        );
        select_next_playback_subtitle_for_version(
            pool,
            item.media_item_id,
            media_version_id,
            &languages,
            item.subtitle_mode.unwrap_or(channel.subtitle_mode),
            StreamingMode::HttpLiveStreamingSegmenter,
        )
        .await?
    };
    Ok(subtitle)
}

fn apply_item_seek(source: &mut wire::Source, item: &PlayoutItem) -> Result<()> {
    let start = u64::try_from(item.in_point.num_milliseconds())
        .map_err(|_| invalid("negative item in-point"))?;
    let finish = u64::try_from(item.out_point.num_milliseconds())
        .map_err(|_| invalid("negative item out-point"))?;
    if finish != 0 && finish <= start {
        return Err(invalid("item out-point does not follow in-point"));
    }
    match source {
        wire::Source::Http {
            is_live: Some(true),
            in_point_ms,
            out_point_ms,
            ..
        } => {
            *in_point_ms = None;
            *out_point_ms = None;
        }
        wire::Source::Local {
            in_point_ms,
            out_point_ms,
            ..
        }
        | wire::Source::Http {
            in_point_ms,
            out_point_ms,
            ..
        } => {
            *in_point_ms = Some(start);
            *out_point_ms = (finish != 0).then_some(finish);
        }
        wire::Source::Lavfi { .. } | wire::Source::Rtsp { .. } => {}
        wire::Source::Dynamic { .. } => {
            return Err(invalid("dynamic source requires concrete resolution"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "selection_tests.rs"]
pub(crate) mod tests;
