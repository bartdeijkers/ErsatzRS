//! Fresh retained-prefix metadata for one-shot Next inputs; never use stored pins.

use std::collections::HashSet;

use ersatzrs_domain::{MediaStream, MediaStreamKind};

use crate::ffprobe::FfprobeMediaVersion;

use super::protocol::{AudioProbeHint, ProbeHint, SubtitleProbeHint, VideoProbeHint};

/// Invalid metadata is reported without source URLs, paths or provider content.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("Next producer input metadata is incomplete or unsupported")]
pub struct ProbeHintError;

/// Describe the fresh rawvideo/PCM stream produced by retained-interval playback.
///
/// The remaining duration comes from the validated requested interval, not the
/// prefix probe (which cannot know a one-shot stream's complete duration).
/// Indexes and format details are copied from that producer's fresh metadata.
///
/// # Errors
/// Rejects missing/ambiguous video, missing audio, invalid indexes/geometry/rate,
/// unsupported codecs. No guessed metadata may replace a consuming probe.
pub fn retained(
    probe: &FfprobeMediaVersion,
    duration_ms: u64,
) -> Result<ProbeHint, ProbeHintError> {
    if duration_ms == 0 {
        return Err(ProbeHintError);
    }
    let mut hint = ProbeHint {
        video: Vec::new(),
        audio: Vec::new(),
        subtitle: Vec::new(),
        format_name: Some("nut".into()),
        duration_ms: Some(duration_ms),
    };
    let mut indexes = HashSet::new();
    for stream in &probe.streams {
        let index = u32::try_from(stream.index).map_err(|_| ProbeHintError)?;
        if !indexes.insert(index) {
            return Err(ProbeHintError);
        }
        match stream.kind {
            MediaStreamKind::Video
                if stream.codec.as_deref() == Some("rawvideo") && !stream.attached_pic =>
            {
                hint.video.push(video(probe, stream, index)?);
            }
            MediaStreamKind::Audio if stream.codec.as_deref() == Some("pcm_f32le") => {
                hint.audio.push(AudioProbeHint {
                    stream_index: index,
                    codec: "pcm_f32le".into(),
                    channels: positive(stream.channels)?,
                });
            }
            _ => return Err(ProbeHintError),
        }
    }
    if hint.video.len() != 1 || hint.audio.is_empty() {
        return Err(ProbeHintError);
    }
    Ok(hint)
}

/// Describe fresh ordinary producer metadata without inventing a total duration.
///
/// The container must be known: official Next otherwise assumes MPEG-TS. Only
/// copied prefix metadata is used; this hint must not trigger a consuming probe.
///
/// # Errors
/// Rejects incomplete metadata, duplicate indexes, unsupported stream kinds and
/// multiple video tracks whose individual timing is not represented by the probe.
pub fn producer(probe: &FfprobeMediaVersion) -> Result<ProbeHint, ProbeHintError> {
    let format = probe
        .format_name
        .as_ref()
        .filter(|name| !name.trim().is_empty())
        .ok_or(ProbeHintError)?;
    let mut hint = ProbeHint {
        video: Vec::new(),
        audio: Vec::new(),
        subtitle: Vec::new(),
        format_name: Some(format.clone()),
        duration_ms: None,
    };
    let mut indexes = HashSet::new();
    for stream in &probe.streams {
        let index = u32::try_from(stream.index).map_err(|_| ProbeHintError)?;
        if !indexes.insert(index) {
            return Err(ProbeHintError);
        }
        match stream.kind {
            MediaStreamKind::Video if !stream.attached_pic => {
                hint.video.push(video(probe, stream, index)?);
            }
            MediaStreamKind::Audio => hint.audio.push(AudioProbeHint {
                stream_index: index,
                codec: codec(stream)?.into(),
                channels: positive(stream.channels)?,
            }),
            MediaStreamKind::Subtitle => hint.subtitle.push(SubtitleProbeHint {
                stream_index: index,
                codec: codec(stream)?.into(),
            }),
            // Font attachments remain in the original byte stream. The Next
            // hint contract only describes selectable video/audio/subtitle tracks.
            MediaStreamKind::Attachment => {}
            _ => return Err(ProbeHintError),
        }
    }
    if hint.video.len() > 1 || (hint.video.is_empty() && hint.audio.is_empty()) {
        return Err(ProbeHintError);
    }
    Ok(hint)
}

fn codec(stream: &MediaStream) -> Result<&str, ProbeHintError> {
    stream
        .codec
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ProbeHintError)
}

fn positive(value: Option<i32>) -> Result<u32, ProbeHintError> {
    value
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or(ProbeHintError)
}

fn video(
    probe: &FfprobeMediaVersion,
    stream: &MediaStream,
    index: u32,
) -> Result<VideoProbeHint, ProbeHintError> {
    let rate = probe
        .version
        .r_frame_rate
        .as_deref()
        .ok_or(ProbeHintError)?;
    let valid_rate = rate.split_once('/').is_some_and(|(n, d)| {
        n.parse::<u32>().is_ok_and(|n| n > 0) && d.parse::<u32>().is_ok_and(|d| d > 0)
    });
    if !valid_rate {
        return Err(ProbeHintError);
    }
    let pixel_format = stream
        .pixel_format
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ProbeHintError)?;
    Ok(VideoProbeHint {
        codec: codec(stream)?.into(),
        stream_index: index,
        width: positive(stream.width.or(Some(probe.version.width)))?,
        height: positive(stream.height.or(Some(probe.version.height)))?,
        pix_fmt: pixel_format.clone(),
        frame_rate: Some(rate.into()),
        profile: stream.profile.clone(),
        field_order: probe.field_orders.get(&stream.index).cloned(),
        sample_aspect_ratio: probe.version.sample_aspect_ratio.clone(),
        display_aspect_ratio: probe.version.display_aspect_ratio.clone(),
        color_range: stream.color_range.clone(),
        color_space: stream.color_space.clone(),
        color_transfer: stream.color_transfer.clone(),
        color_primaries: stream.color_primaries.clone(),
        dv_profile: stream
            .dolby_vision_profile
            .map(u32::try_from)
            .transpose()
            .map_err(|_| ProbeHintError)?,
        has_hdr10_metadata: Some(
            stream.mastering_display.is_some()
                || stream.max_cll.is_some()
                || stream.max_fall.is_some(),
        ),
        rotation: stream.rotation,
    })
}

#[cfg(test)]
#[path = "probe_hint_tests.rs"]
pub(crate) mod tests;
