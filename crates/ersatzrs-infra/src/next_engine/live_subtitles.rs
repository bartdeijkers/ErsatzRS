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
    let live = is_live(&projection.item);
    let Some(selected) = projection.selected_subtitle.as_ref().filter(|selected| {
        live && selected.subtitle_kind != crate::metadata::subtitle_kind::EMBEDDED
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
    let (path, seek) = match track.source.as_ref() {
        Some(Source::Local {
            path, in_point_ms, ..
        }) => (path.clone(), in_point_ms.unwrap_or(0)),
        Some(Source::Http {
            uri,
            in_point_ms,
            is_live,
            ..
        }) if *is_live != Some(true) => (uri.clone(), in_point_ms.unwrap_or(0)),
        _ => return Err(unavailable()),
    };
    // Installed producers store their sidecar base beside the media manifest,
    // whereas their primary source is a provider URL during SQL projection.
    let path = if let Some(context) = snapshot.producer_remote.as_ref() {
        crate::playback::subtitle_input_for_stream(
            selected,
            &context.media.path,
            ersatzrs_ffmpeg::OutputFormat::MpegTs,
            ersatzrs_domain::StreamingMode::HttpLiveStreamingSegmenter,
        )
        .ok_or_else(unavailable)?
        .path
    } else {
        path
    };
    let bytes = tokio::time::timeout_at(deadline, read(&path))
        .await
        .map_err(|_| unavailable())??;
    let content = crate::subtitles::rebase_text_cues_milliseconds(bytes, format, seek, MAX_BYTES)?;
    let Some(content) = content else {
        // All successfully parsed cues have ended before this callback time.
        resolved.tracks.as_mut().ok_or_else(unavailable)?.subtitle = None;
        snapshot.projection.selected_subtitle = None;
        snapshot.projection.item = resolved;
        return Ok(true);
    };
    let mut digest = Sha256::new();
    digest.update(snapshot.identity.descriptor.as_bytes());
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
            resolved.finish,
            deadline,
            || async { true },
        )
        .await
        .map_err(|_| unavailable())?;
    let path = artifact.to_str().ok_or_else(unavailable)?.to_owned();
    track.source = Some(Source::Local {
        path: path.clone(),
        in_point_ms: Some(0),
        out_point_ms: None,

        probe_hint: None,
    });
    track.stream_index = Some(0);
    let selected = snapshot
        .projection
        .selected_subtitle
        .as_mut()
        .ok_or_else(unavailable)?;
    selected.path = Some(path);
    selected.stream_index = 0;
    snapshot.projection.item = resolved;
    Ok(true)
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
