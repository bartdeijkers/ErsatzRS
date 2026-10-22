//! Fresh retained-prefix metadata for one-shot Next inputs; never use stored pins.

use std::collections::HashSet;

use ersatzrs_core::repositories::{RepositoryError, Result as RepositoryResult};
use ersatzrs_domain::{MediaStream, MediaStreamKind, Resolution};
use sqlx::Row;

use crate::db::DbPool;
use crate::ffprobe::FfprobeMediaVersion;
use crate::repositories::SqlxResultExt as _;

use super::protocol::{AudioProbeHint, ProbeHint, SubtitleProbeHint, VideoProbeHint};

/// Invalid metadata is reported without source URLs, paths or provider content.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("Next producer input metadata is incomplete or unsupported")]
pub struct ProbeHintError;

/// Finite, output-sized video for an audio-only programme. Neither a cover-art
/// stream nor the original audio seek describes this generated video input.
pub(super) fn generated_video(
    resolution: &Resolution,
    duration_ms: u64,
    codec: &str,
    format_name: Option<&str>,
) -> RepositoryResult<ProbeHint> {
    let dimension = |value| u32::try_from(value).ok().filter(|value| *value > 0);
    let invalid =
        || RepositoryError::unavailable("Next generated video", "invalid geometry or duration");
    let width = dimension(resolution.width).ok_or_else(invalid)?;
    let height = dimension(resolution.height).ok_or_else(invalid)?;
    if duration_ms == 0 {
        return Err(invalid());
    }
    Ok(ProbeHint {
        video: vec![VideoProbeHint {
            source_frame_cut: None,
            codec: codec.to_owned(),
            width,
            height,
            pix_fmt: "yuv420p".into(),
            stream_index: 0,
            frame_rate: None,
            profile: None,
            field_order: None,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            color_range: None,
            color_space: None,
            color_transfer: None,
            color_primaries: None,
            dv_profile: None,
            has_hdr10_metadata: None,
            rotation: None,
        }],
        audio: Vec::new(),
        subtitle: Vec::new(),
        format_name: format_name.map(str::to_owned),
        duration_ms: Some(duration_ms),
    })
}

/// Describe a persisted local or media-server version without opening the
/// source. Incomplete stream metadata leaves Next's ordinary probe in control.
/// One-shot producers use fresh metadata through `producer` instead.
#[allow(clippy::too_many_lines)] // Keep one version and its ordered stream projection together.
pub(crate) async fn persisted(
    pool: &DbPool,
    version_id: i64,
) -> RepositoryResult<Option<ProbeHint>> {
    let version = sqlx::query(
        r#"SELECT "Duration", "Width", "Height", "RFrameRate", "SampleAspectRatio",
                  "DisplayAspectRatio", "VideoScanKind", "ImageId",
                  (SELECT "DurationSeconds" FROM "ImageMetadata" WHERE "ImageId"=mv."ImageId"
                   ORDER BY "Id" LIMIT 1) AS "ImageDurationSeconds",
                  (SELECT "Duration" FROM "RemoteStream" WHERE "Id"=mv."RemoteStreamId") AS "RemoteDuration",
                  (SELECT "Path" FROM "MediaFile" WHERE "MediaVersionId"=mv."Id"
                   ORDER BY "Id" LIMIT 1) AS "Path"
           FROM "MediaVersion" mv WHERE "Id"=?"#,
    )
    .bind(version_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(version) = version else {
        return Ok(None);
    };
    let width: i32 = version.try_get("Width").repository()?;
    let height: i32 = version.try_get("Height").repository()?;
    let frame_rate: Option<String> = version.try_get("RFrameRate").repository()?;
    let duration_ms =
        u64::try_from(crate::playout_duration::from_row(&version)?.num_milliseconds())
            .ok()
            .filter(|duration| *duration > 0);
    let image: Option<i64> = version.try_get("ImageId").repository()?;
    let scan_kind: i64 = version.try_get("VideoScanKind").repository()?;
    let mut hint = ProbeHint {
        video: Vec::new(),
        audio: Vec::new(),
        subtitle: Vec::new(),
        format_name: version
            .try_get::<Option<String>, _>("Path")
            .repository()?
            .filter(|path| {
                path.rsplit_once('.')
                    .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("avi"))
            })
            .map(|_| "avi".into()),
        duration_ms,
    };
    let rows = sqlx::query(
        r#"SELECT "Index", "MediaStreamKind", "Codec", "Profile", "Channels",
                  "PixelFormat", "BitsPerRawSample", "AttachedPic", "ColorPrimaries",
                  "ColorRange", "ColorSpace", "ColorTransfer", "Rotation",
                  "DolbyVisionProfile", "MasteringDisplay", "MaxCll", "MaxFall"
           FROM "MediaStream" WHERE "MediaVersionId"=? ORDER BY "Index", "Id""#,
    )
    .bind(version_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut indexes = HashSet::new();
    for row in rows {
        let kind: i64 = row.try_get("MediaStreamKind").repository()?;
        if !matches!(kind, 1..=3) {
            continue;
        }
        let raw_index: i32 = row.try_get("Index").repository()?;
        let Some(index) = u32::try_from(raw_index)
            .ok()
            .filter(|index| indexes.insert(*index))
        else {
            return Ok(None);
        };
        let codec: Option<String> = row.try_get("Codec").repository()?;
        let Some(codec) = codec.filter(|value| !value.trim().is_empty()) else {
            return Ok(None);
        };
        match kind {
            1 => {
                let attached: i64 = row.try_get("AttachedPic").repository()?;
                if attached != 0 {
                    continue;
                }
                let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
                    return Ok(None);
                };
                if width == 0 || height == 0 {
                    return Ok(None);
                }
                let pixel_format: Option<String> = row.try_get("PixelFormat").repository()?;
                let bit_depth: i32 = row.try_get("BitsPerRawSample").repository()?;
                if image.is_some() && pixel_format.as_deref().is_none_or(str::is_empty) {
                    return Ok(None);
                }
                let pix_fmt = pixel_format
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| {
                        if bit_depth == 10 {
                            "yuv420p10le"
                        } else {
                            "yuv420p"
                        }
                        .into()
                    });
                if image.is_some() {
                    hint.format_name = Some(
                        match codec.as_str() {
                            "png" => "png_pipe",
                            "mjpeg" => "jpeg_pipe",
                            "bmp" => "bmp_pipe",
                            "tiff" => "tiff_pipe",
                            "webp" => "webp_pipe",
                            "webp_anim" => "webp_anim",
                            "gif" => "gif",
                            _ => return Ok(None),
                        }
                        .into(),
                    );
                }
                let dv_profile: Option<i32> = row.try_get("DolbyVisionProfile").repository()?;
                let mastering: Option<String> = row.try_get("MasteringDisplay").repository()?;
                let max_cll: Option<i32> = row.try_get("MaxCll").repository()?;
                let max_fall: Option<i32> = row.try_get("MaxFall").repository()?;
                hint.video.push(VideoProbeHint {
                    source_frame_cut: None,
                    codec,
                    width,
                    height,
                    pix_fmt,
                    stream_index: index,
                    frame_rate: frame_rate.clone(),
                    profile: row.try_get("Profile").repository()?,
                    field_order: Some(if scan_kind == 2 { "tt" } else { "progressive" }.into()),
                    sample_aspect_ratio: version.try_get("SampleAspectRatio").repository()?,
                    display_aspect_ratio: version.try_get("DisplayAspectRatio").repository()?,
                    color_range: row.try_get("ColorRange").repository()?,
                    color_space: row.try_get("ColorSpace").repository()?,
                    color_transfer: row.try_get("ColorTransfer").repository()?,
                    color_primaries: row.try_get("ColorPrimaries").repository()?,
                    dv_profile: dv_profile.and_then(|value| u32::try_from(value).ok()),
                    has_hdr10_metadata: Some(
                        mastering.is_some() || max_cll.is_some() || max_fall.is_some(),
                    ),
                    rotation: row.try_get("Rotation").repository()?,
                });
            }
            2 => {
                let channels: i32 = row.try_get("Channels").repository()?;
                let Ok(channels) = u32::try_from(channels) else {
                    return Ok(None);
                };
                if channels == 0 {
                    return Ok(None);
                }
                hint.audio.push(AudioProbeHint {
                    stream_index: index,
                    codec,
                    channels,
                });
            }
            3 => hint.subtitle.push(SubtitleProbeHint {
                stream_index: index,
                codec,
            }),
            _ => unreachable!(),
        }
    }
    if hint.video.is_empty() && hint.audio.is_empty() {
        return Ok(None);
    }
    Ok(Some(hint))
}

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
            // Album artwork stays in the producer bytes but cannot be selected
            // as programme video, just like persisted AttachedPic metadata.
            MediaStreamKind::Video if stream.attached_pic => {}
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
        source_frame_cut: None,
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
