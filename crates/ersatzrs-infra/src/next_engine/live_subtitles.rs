//! Rebase independently readable live sidecars before Next clears input seeks.
//! Official Next `channel_session::input_timing` uses zero for every live input,
//! including a finite subtitle file. Never read the multiplexed live producer here.

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_ffmpeg::SubtitleTextFormat;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use tokio::{io::AsyncReadExt as _, time::Instant};

use super::{burn_cache::BurnCache, dynamic_item, dynamic_projection::Snapshot, protocol::Source};

const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Preparation is bounded by the callback deadline and existing cache admission.
/// The caller must revalidate publication and session identity after this await.
pub(super) async fn prepare(
    cache: &BurnCache,
    snapshot: &mut Snapshot,
    now: DateTime<Utc>,
    deadline: Instant,
) -> Result<bool> {
    if Instant::now() >= deadline {
        return Err(unavailable());
    }
    let projection = &snapshot.projection;
    let Some(selected) = projection.selected_subtitle.as_ref().filter(|selected| {
        is_live(&projection.item)
            && selected.subtitle_kind != crate::metadata::subtitle_kind::EMBEDDED
    }) else {
        return Ok(false);
    };
    let (format, extension) = match selected.codec.as_deref() {
        Some("ass" | "ssa") => (SubtitleTextFormat::Ass, "ass"),
        Some("webvtt" | "vtt") => (SubtitleTextFormat::WebVtt, "vtt"),
        Some("subrip" | "srt") => (SubtitleTextFormat::Srt, "srt"),
        _ => return Err(unavailable()),
    };
    let mut resolved =
        dynamic_item::resolve_at(&projection.item, now).map_err(|_| unavailable())?;
    let track = resolved
        .tracks
        .as_mut()
        .and_then(|tracks| tracks.subtitle.as_mut())
        .ok_or_else(unavailable)?;
    // Installed sidecars are based beside their media manifest, not the provider URL.
    if let Some(context) = snapshot.producer_remote.as_ref() {
        let path = crate::playback::subtitle_input_for_stream(
            selected,
            &context.media.path,
            ersatzrs_ffmpeg::OutputFormat::MpegTs,
            ersatzrs_domain::StreamingMode::HttpLiveStreamingSegmenter,
        )
        .ok_or_else(unavailable)?
        .path;
        let seek = source_path(track)?.1;
        track.source = Some(Source::Local {
            path,
            in_point_ms: Some(seek),
            out_point_ms: None,
            probe_hint: None,
        });
    }
    let prepared = rebase_track(
        cache,
        track,
        format,
        extension,
        &snapshot.identity.descriptor,
        resolved.finish,
        deadline,
    )
    .await?;
    if let Some(prepared) = prepared {
        let path = source_path(&prepared)?.0;
        *track = prepared;
        let selected = snapshot
            .projection
            .selected_subtitle
            .as_mut()
            .ok_or_else(unavailable)?;
        selected.path = Some(path);
        selected.stream_index = 0;
    } else {
        resolved.tracks.as_mut().ok_or_else(unavailable)?.subtitle = None;
        snapshot.projection.selected_subtitle = None;
    }
    snapshot.projection.item = resolved;
    Ok(true)
}

/// Rebase only the immutable owned generated ASS selected by the preview Context.
/// Actual live input timing supplies zero seek to every independent Next input.
pub(super) async fn prepare_generated(
    cache: &BurnCache,
    track: &super::protocol::Track,
    publication: &str,
    finish: DateTime<Utc>,
    deadline: Instant,
) -> Result<Option<super::protocol::Track>> {
    rebase_track(
        cache,
        track,
        SubtitleTextFormat::Ass,
        "ass",
        publication,
        finish,
        deadline,
    )
    .await
}

/// The installed producer's actual liveness, rather than persisted provider metadata,
/// determines whether Next will discard the independent caption's elapsed seek.
pub(super) async fn prepare_generated_for_source(
    cache: &BurnCache,
    track: &super::protocol::Track,
    source: &Source,
    publication: &str,
    finish: DateTime<Utc>,
    deadline: Instant,
) -> Result<Option<super::protocol::Track>> {
    if matches!(
        source,
        Source::Rtsp { .. }
            | Source::Http {
                is_live: Some(true),
                ..
            }
    ) {
        prepare_generated(cache, track, publication, finish, deadline).await
    } else {
        Ok(Some(track.clone()))
    }
}

/// Install a prepared generated caption without changing its publication identity.
pub(super) fn install_generated(
    snapshot: &mut Snapshot,
    now: DateTime<Utc>,
    track: Option<super::protocol::Track>,
) -> Result<()> {
    let mut resolved =
        dynamic_item::resolve_at(&snapshot.projection.item, now).map_err(|_| unavailable())?;
    snapshot.projection.selected_subtitle = track
        .as_ref()
        .map(|track| -> Result<_> {
            Ok(crate::playback::PlaybackSubtitle {
                id: 0,
                subtitle_kind: crate::metadata::subtitle_kind::GENERATED,
                title: None,
                stream_index: 0,
                codec: Some("ass".into()),
                default: true,
                forced: true,
                sdh: false,
                language: None,
                is_extracted: false,
                path: Some(source_path(track)?.0),
            })
        })
        .transpose()?;
    resolved.tracks.as_mut().ok_or_else(unavailable)?.subtitle = track;
    snapshot.projection.item = resolved;
    Ok(())
}

fn source_path(track: &super::protocol::Track) -> Result<(String, u64)> {
    match track.source.as_ref() {
        Some(Source::Local {
            path, in_point_ms, ..
        }) => Ok((path.clone(), in_point_ms.unwrap_or(0))),
        Some(Source::Http {
            uri,
            in_point_ms,
            is_live,
            ..
        }) if *is_live != Some(true) => Ok((uri.clone(), in_point_ms.unwrap_or(0))),
        _ => Err(unavailable()),
    }
}

async fn rebase_track(
    cache: &BurnCache,
    track: &super::protocol::Track,
    format: SubtitleTextFormat,
    extension: &str,
    publication: &str,
    finish: DateTime<Utc>,
    deadline: Instant,
) -> Result<Option<super::protocol::Track>> {
    if Instant::now() >= deadline {
        return Err(unavailable());
    }
    let (path, seek) = source_path(track)?;
    let bytes = tokio::time::timeout_at(deadline, read(&path))
        .await
        .map_err(|_| unavailable())??;
    let Some(content) =
        crate::subtitles::rebase_text_cues_milliseconds(bytes, format, seek, MAX_BYTES)?
    else {
        return Ok(None);
    };
    let mut digest = Sha256::new();
    digest.update(publication.as_bytes());
    digest.update(seek.to_le_bytes());
    digest.update(extension.as_bytes());
    digest.update(content.as_bytes());
    let mut key = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(key, "{byte:02x}").map_err(|_| unavailable())?;
    }
    let artifact = cache
        .prepare_text(
            &key,
            extension,
            &content,
            finish.max(Utc::now()),
            deadline,
            || async { true },
        )
        .await
        .map_err(|_| unavailable())?;
    Ok(Some(super::protocol::Track {
        stream_index: Some(0),
        source: Some(Source::Local {
            path: artifact.to_str().ok_or_else(unavailable)?.to_owned(),
            in_point_ms: Some(0),
            out_point_ms: None,
            probe_hint: None,
        }),
    }))
}

pub(super) fn is_live(item: &super::protocol::PlayoutItem) -> bool {
    item.tracks.as_ref().is_some_and(|tracks| {
        [&tracks.video, &tracks.audio]
            .into_iter()
            .flatten()
            .any(|track| {
                matches!(
                    track.source,
                    Some(
                        Source::Rtsp { .. }
                            | Source::Http {
                                is_live: Some(true),
                                ..
                            }
                    )
                )
            })
    })
}

async fn read(path: &str) -> Result<Vec<u8>> {
    if path.starts_with("http://") || path.starts_with("https://") {
        let mut response = reqwest::get(path)
            .await
            .map_err(|_| unavailable())?
            .error_for_status()
            .map_err(|_| unavailable())?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_BYTES as u64)
        {
            return Err(unavailable());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if chunk.len() > MAX_BYTES.saturating_sub(bytes.len()) {
                return Err(unavailable());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    } else {
        let metadata = tokio::fs::metadata(path).await.map_err(|_| unavailable())?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
            return Err(unavailable());
        }
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|_| unavailable())?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| unavailable())?;
        if bytes.len() > MAX_BYTES {
            return Err(unavailable());
        }
        Ok(bytes)
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next live subtitles",
        "selected standalone captions could not be safely prepared",
    )
}

#[cfg(test)]
#[path = "live_subtitles_tests.rs"]
mod tests;
