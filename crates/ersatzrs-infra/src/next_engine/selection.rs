//! Resolve persisted stream choices through the native playback selectors.

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{Channel, ChannelStreamSelectorMode, PlayoutItem, Resolution, StreamingMode};

use crate::db::DbPool;
use crate::ffmpeg_config::{playback_language_preferences, PlaybackLanguagePreferences};
use crate::playback::{
    load_source_audio_streams, load_source_video_stream_by_index,
    select_custom_playback_streams_for_version, select_next_pinned_playback_subtitle,
    select_next_playback_subtitle_for_version, select_playback_audio_stream_index,
    subtitle_input_for_stream, CustomStreamSelection, PlaybackSubtitle,
};
use ersatzrs_ffmpeg::{OutputFormat, SourceAudioStream};

use super::protocol as wire;

/// Projection requiring sidecar preparation before it can be published.
/// Deliberately no Debug: source paths and subtitle metadata may be private.
pub(crate) struct DeferredProjection {
    pub item: wire::PlayoutItem,
    pub pending_burn: Option<PendingBurn>,
    pub selected_subtitle: Option<PlaybackSubtitle>,
}

/// Exact selected embedded/provider track and its original, sought media source.
pub(crate) struct PendingBurn {
    pub subtitle: PlaybackSubtitle,
    pub source: wire::Source,
}

pub(crate) struct TroubleshootingSelection<'a> {
    pub channel: &'a Channel,
    pub item: &'a PlayoutItem,
    pub media_version_id: i64,
    pub source: wire::Source,
    pub resolution: &'a Resolution,
    /// None keeps channel preferences; zero means explicitly disabled.
    pub subtitle_id: Option<i64>,
}

/// Reuse normal track selection, then apply an explicit troubleshooting subtitle.
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
    let mut projected = project_deferred_item_with_audio(
        pool,
        request.channel,
        request.item,
        request.media_version_id,
        request.source,
        request.resolution,
        audio,
    )
    .await?;
    if let Some(id) = request.subtitle_id {
        if id < 0 {
            return Err(invalid("troubleshooting subtitle identity is invalid"));
        }
        let selected = if id > 0 {
            Some(
                crate::playback::select_next_troubleshooting_subtitle(
                    pool,
                    request.item.media_item_id,
                    request.media_version_id,
                    id,
                )
                .await?,
            )
        } else {
            None
        };
        let tracks = projected
            .item
            .tracks
            .as_mut()
            .ok_or_else(|| invalid("troubleshooting tracks are unavailable"))?;
        let source = tracks
            .video
            .as_ref()
            .and_then(|track| track.source.as_ref())
            .ok_or_else(|| invalid("troubleshooting source is unavailable"))?;
        let (subtitle, pending) = project_deferred_subtitle(
            pool,
            selected.as_ref(),
            source,
            request.channel.next_engine_text_subtitle_mode,
            request.item,
        )
        .await?;
        tracks.subtitle = subtitle;
        projected.pending_burn = pending;
        projected.selected_subtitle = selected;
    }
    Ok(projected)
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
    mut source: wire::Source,
    _resolution: &Resolution,
    audio_directive: AudioDirective,
) -> Result<DeferredProjection> {
    let duration_ms = u64::try_from((item.finish - item.start).num_milliseconds())
        .ok()
        .filter(|duration| *duration > 0)
        .ok_or_else(|| invalid("invalid item duration"))?;
    apply_item_seek(&mut source, item)?;
    let pin = item.pinned_stream_selection;
    if pin.is_some_and(|pin| pin.media_version_id != media_version_id) {
        return Err(invalid("resolved version differs from pinned version"));
    }
    let video = load_source_video_stream_by_index(
        pool,
        media_version_id,
        pin.map(|pin| pin.video_stream_index),
    )
    .await?;
    let audio_streams = load_source_audio_streams(pool, media_version_id).await?;
    let preferences = playback_language_preferences(pool).await?;
    let custom =
        if pin.is_none() && channel.stream_selector_mode == ChannelStreamSelectorMode::Custom {
            select_custom_playback_streams_for_version(
                pool,
                media_version_id,
                item.media_item_id,
                channel,
                item.start,
                StreamingMode::HttpLiveStreamingSegmenter,
            )
            .await?
        } else {
            None
        };
    let audio_index = if let AudioDirective::Published(audio) = audio_directive {
        let pinned_audio = audio
            .map(i32::try_from)
            .transpose()
            .map_err(|_| invalid("published audio index is invalid"))?;
        if pin.is_some_and(|pin| pin.audio_stream_index != pinned_audio)
            || audio.is_some_and(|index| !audio_streams.iter().any(|stream| stream.index == index))
        {
            return Err(invalid("published audio binding changed"));
        }
        audio
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
    let subtitle = selected_subtitle(
        pool,
        channel,
        item,
        media_version_id,
        custom.as_ref(),
        &preferences,
    )
    .await?;
    let audio = match audio_index {
        Some(index) => wire::Track {
            source: Some(source.clone()),
            stream_index: Some(index_u32(index)?),
        },
        None => silent_audio(duration_ms),
    };
    let selected_subtitle = subtitle;
    let (subtitle, pending_burn) = project_deferred_subtitle(
        pool,
        selected_subtitle.as_ref(),
        &source,
        channel.next_engine_text_subtitle_mode,
        item,
    )
    .await?;
    Ok(DeferredProjection {
        pending_burn,
        selected_subtitle,
        item: wire::PlayoutItem {
            watermark: None,
            id: item.id.to_string(),
            start: item.start,
            finish: item.finish,
            source: None,
            tracks: Some(wire::Tracks {
                video: Some(wire::Track {
                    source: Some(source),
                    stream_index: video.map(|stream| index_u32(stream.index)).transpose()?,
                }),
                audio: Some(audio),
                subtitle,
            }),
        },
    })
}

async fn project_deferred_subtitle(
    pool: &DbPool,
    selected_subtitle: Option<&PlaybackSubtitle>,
    source: &wire::Source,
    mode: ersatzrs_domain::NextEngineTextSubtitleMode,
    item: &PlayoutItem,
) -> Result<(Option<wire::Track>, Option<PendingBurn>)> {
    let provider = if let Some(selected) = selected_subtitle {
        crate::provider_subtitles::binding(pool, selected.id)
            .await?
            .is_some()
    } else {
        false
    };
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
            probe_hint: Some(wire::AudioOnlyProbeHint {
                video: [],
                subtitle: [],
                format_name: "mpegts".to_owned(),
                duration_ms,
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
    } else if input.path.starts_with("http://") || input.path.starts_with("https://") {
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
