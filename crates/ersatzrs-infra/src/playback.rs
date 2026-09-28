//! Shared helpers for building `FFmpeg` playback pipelines from persisted and
//! transient media metadata, used by the MPEG-TS, HLS and Next paths.

#[path = "playback_transient.rs"]
pub mod transient;
use crate::repositories::SqlxResultExt as _;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::condition::{evaluate as evaluate_condition, ConditionContext};
use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use ersatzrs_core::language_codes;
use ersatzrs_core::repositories::{GraphicsRepository as _, RepositoryError, Result};
use ersatzrs_domain::{
    Channel, ChannelSubtitleMode, ChannelWatermark, ChannelWatermarkImageSource,
    ChannelWatermarkMode, DecoMode, FFmpegProfile, FillerKind, GraphicsElement,
    GraphicsElementConfig, HardwareAccelerationKind, MotionEndBehavior, Resolution, StreamingMode,
    TextGraphicsRenderer, VideoScanKind, CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
};
use ersatzrs_ffmpeg::{
    ffmpeg_path, ffprobe_path, graphics_element_drawtext_overlay, FrameRate, FrameSize,
    GraphicsPipelineOverlay, GraphicsRenderContext, GraphicsTemplateRenderer, InputFile,
    OutputFormat, PixelFormat, SourceAudioCodec, SourceAudioStream, SourceColorParams,
    SourceVideoCodec, SourceVideoStream, SubtitleMethod, WatermarkFadeKind, WatermarkFadePoint,
    WatermarkOverlay,
};
use sqlx::Row;

use crate::db::DbPool;
use crate::dynamic_text::{
    shutdown_dynamic_text_controllers, DynamicTextController, DynamicTextControllerRequest,
};
use crate::ffmpeg_config::use_embedded_subtitles;
use crate::ffprobe::{FfprobeExtractor, FfprobeMediaVersion};
use crate::file_system_layout::{
    data_path, fonts_cache_folder, resources_cache_folder, subtitle_cache_folder,
};
use crate::graphics_template::{
    graphics_template_variables, render_graphics_template, scalar_template_variables,
    GraphicsTemplateContext,
};
use crate::metadata::subtitle_kind;
use crate::music_video_credits::generate_music_video_credits_subtitle;
use crate::repositories::common::{
    decode_bool_column, decode_enum_column, decode_optional_bool_column,
    decode_optional_string_column, decode_string_column, map_data_access_error,
};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const IMAGE_SUBTITLE_CODECS: &[&str] = &[
    "hdmv_pgs_subtitle",
    "dvd_subtitle",
    "dvdsub",
    "vobsub",
    "pgssub",
    "pgs",
];

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct PlaybackSubtitle {
    pub id: i64,
    pub subtitle_kind: i32,
    pub title: Option<String>,
    pub stream_index: i32,
    pub codec: Option<String>,
    pub default: bool,
    pub forced: bool,
    pub sdh: bool,
    pub language: Option<String>,
    pub is_extracted: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackAudioStream {
    pub stream_index: i32,
    pub title: Option<String>,
    pub language: Option<String>,
    pub default: bool,
    pub forced: bool,
    pub codec: Option<String>,
    pub channels: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomStreamSelection {
    pub audio_stream_index: Option<usize>,
    pub subtitle: Option<PlaybackSubtitle>,
}

/// Runtime audio-selection state after applying an optional pinned tuple.
///
/// `Automatic` preserves normal channel and profile behavior, while
/// `Disabled` represents the explicit silence encoded by a pinned tuple whose
/// audio stream index is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackAudioSelection {
    Automatic,
    Exact(usize),
    Disabled,
}

impl PlaybackAudioSelection {
    #[must_use]
    pub const fn stream_index(self) -> Option<usize> {
        match self {
            Self::Exact(index) => Some(index),
            Self::Automatic | Self::Disabled => None,
        }
    }

    #[must_use]
    pub const fn is_disabled(self) -> bool {
        matches!(self, Self::Disabled)
    }
}

impl From<Option<usize>> for PlaybackAudioSelection {
    fn from(stream_index: Option<usize>) -> Self {
        stream_index.map_or(Self::Automatic, Self::Exact)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackProfile {
    pub profile: FFmpegProfile,
    pub resolution: Resolution,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPlaybackWatermark {
    pub watermark: ChannelWatermark,
    pub image: PlaybackWatermarkImage,
}

/// A watermark input retains whether it is a filesystem path or an HTTP URL.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackWatermarkImage {
    Local(PathBuf),
    Http(String),
}

impl PlaybackWatermarkImage {
    /// Input spelling passed to native `FFmpeg` without filesystem URL conversion.
    #[must_use]
    pub fn input_path(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Local(path) => path.to_string_lossy(),
            Self::Http(uri) => std::borrow::Cow::Borrowed(uri),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPlaybackGraphicsElement {
    pub element: GraphicsElement,
    pub variables: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackGraphicsRenderContext {
    pub channel_id: i64,
    pub channel_number: String,
    pub media_item_id: i64,
    pub media_path: String,
    pub channel_start_time: DateTime<Utc>,
    pub content_start_time: DateTime<Utc>,
    pub stream_seek_seconds: f64,
    pub duration_seconds: f64,
    pub frame_size: FrameSize,
    pub square_pixel_frame_size: FrameSize,
    pub frame_rate: Option<FrameRate>,
}

/// Rendered `FFmpeg` overlays and the live text controllers they depend on.
#[derive(Default)]
pub struct PlaybackGraphicsArtifacts {
    pub overlays: Vec<GraphicsPipelineOverlay>,
    pub dynamic_text_controllers: Vec<DynamicTextController>,
}

impl PlaybackGraphicsArtifacts {
    /// Stop every live controller. Call this before deleting the graphics directory.
    pub async fn shutdown(self) {
        shutdown_dynamic_text_controllers(self.dynamic_text_controllers).await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlaybackWatermarkContext {
    playout_id: i64,
    filler_kind: FillerKind,
    playout_deco_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlaybackDeco {
    mode: DecoMode,
    use_watermark_during_filler: bool,
    watermark_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlaybackGraphicsDeco {
    mode: DecoMode,
    use_graphics_during_filler: bool,
    graphics_element_ids: Vec<i64>,
}

/// Load the source video-stream metadata the `FFmpeg` pipeline needs to make
/// ErsatzTV-parity decisions (decoder selection, deinterlace, tonemap, and the
/// 10-bit → 8-bit pixel-format conversion handled by `SetPixelFormat`).
///
/// Returns `Ok(None)` when the media version has no recorded video stream, in
/// which case the pipeline falls back to its format-agnostic defaults.
///
/// # Errors
///
/// Returns a database error when the query fails and a decode error when a
/// persisted row cannot be projected without changing playback behavior.
pub async fn load_source_video_stream(
    pool: &DbPool,
    media_version_id: i64,
) -> Result<Option<SourceVideoStream>> {
    load_source_video_stream_by_index(pool, media_version_id, None).await
}

/// Load one exact persisted source video stream when `stream_index` is set.
///
/// A pinned stream that no longer exists is reported as a decode error so
/// playback cannot silently drift to another stream.
///
/// # Errors
///
/// Returns a database error when the query fails, or a decode error when the
/// requested pinned stream is missing or cannot be projected safely.
pub async fn load_source_video_stream_by_index(
    pool: &DbPool,
    media_version_id: i64,
    stream_index: Option<i32>,
) -> Result<Option<SourceVideoStream>> {
    let row = sqlx::query(SOURCE_VIDEO_STREAM_QUERY)
        .bind(media_version_id)
        .bind(stream_index)
        .bind(stream_index)
        .fetch_optional(pool)
        .await
        .repository()?;
    let Some(row) = row else {
        if let Some(stream_index) = stream_index {
            return Err(RepositoryError::Decode {
                kind: "pinned source video stream",
                value: format!(
                    "media version {media_version_id} no longer has video stream {stream_index}"
                ),
            });
        }
        return Ok(None);
    };

    let Some(codec) = decode_playback_column::<Option<String>>(&row, "Codec")? else {
        if let Some(stream_index) = stream_index {
            return Err(RepositoryError::Decode {
                kind: "pinned source video stream",
                value: format!(
                    "video stream {stream_index} on media version {media_version_id} has no codec"
                ),
            });
        }
        return Ok(None);
    };
    let index = decode_playback_column::<i64>(&row, "Index")?;
    let index = usize::try_from(index).map_err(|error| RepositoryError::Decode {
        kind: "source video stream",
        value: format!("Index: {error}"),
    })?;
    let pixel_format: Option<String> = decode_playback_column(&row, "PixelFormat")?;
    let color_range: Option<String> = decode_playback_column(&row, "ColorRange")?;
    let color_transfer: Option<String> = decode_playback_column(&row, "ColorTransfer")?;
    let color_primaries: Option<String> = decode_playback_column(&row, "ColorPrimaries")?;
    let color_space: Option<String> = decode_playback_column(&row, "ColorSpace")?;
    let width: i32 = decode_playback_column(&row, "Width")?;
    let height: i32 = decode_playback_column(&row, "Height")?;
    let frame_rate: Option<String> = decode_playback_column(&row, "RFrameRate")?;
    let scan_kind_value = decode_playback_column::<i64>(&row, "VideoScanKind")?;
    let scan_kind = scan_kind_from_db(scan_kind_value).ok_or_else(|| RepositoryError::Decode {
        kind: "source video stream",
        value: format!("VideoScanKind: unknown value {scan_kind_value}"),
    })?;

    let mut stream =
        SourceVideoStream::new(SourceVideoCodec::from_ffmpeg_name(&codec)).with_index(index);
    stream.pixel_format = pixel_format
        .as_deref()
        .and_then(PixelFormat::from_ffmpeg_name);
    stream.scan_kind = scan_kind;
    stream.frame_size = frame_size_from_db(width, height);
    stream.frame_rate = frame_rate.as_deref().and_then(parse_frame_rate);
    stream.sample_aspect_ratio = decode_playback_column(&row, "SampleAspectRatio")?;
    stream.display_aspect_ratio = decode_playback_column(&row, "DisplayAspectRatio")?;
    decode_source_display_metadata(&row, &mut stream)?;
    stream.is_still_image = decode_playback_bool(&row, "IsStillImage")?;
    stream.color_params = derive_color_params(
        pixel_format.as_deref(),
        color_range.as_deref(),
        color_transfer.as_deref(),
        color_primaries.as_deref(),
        color_space.as_deref(),
    );

    Ok(Some(stream))
}

const SOURCE_VIDEO_STREAM_QUERY: &str = r#"SELECT ms."Index" AS "Index",
              ms."Codec" AS "Codec",
              ms."PixelFormat" AS "PixelFormat",
              ms."ColorRange" AS "ColorRange",
              ms."ColorTransfer" AS "ColorTransfer",
              ms."ColorPrimaries" AS "ColorPrimaries",
              ms."ColorSpace" AS "ColorSpace",
              ms."Rotation" AS "Rotation",
              ms."MasteringDisplay" AS "MasteringDisplay",
              ms."MaxCll" AS "MaxCll",
              ms."MaxFall" AS "MaxFall",
              ms."DolbyVisionProfile" AS "DolbyVisionProfile",
              ms."DolbyVisionBlCompatibilityId" AS "DolbyVisionBlCompatibilityId",
              ms."DolbyVisionBlPresent" AS "DolbyVisionBlPresent",
              ms."DolbyVisionElPresent" AS "DolbyVisionElPresent",
              ms."DolbyVisionRpuPresent" AS "DolbyVisionRpuPresent",
              ms."DolbyVisionMetadataState" AS "DolbyVisionMetadataState",
              ms."DolbyVisionMetadataHint" AS "DolbyVisionMetadataHint",
              mv."VideoScanKind" AS "VideoScanKind",
              mv."Width" AS "Width",
              mv."Height" AS "Height",
              mv."RFrameRate" AS "RFrameRate",
              mv."SampleAspectRatio" AS "SampleAspectRatio",
              mv."DisplayAspectRatio" AS "DisplayAspectRatio",
              mv."ImageId" IS NOT NULL AS "IsStillImage"
       FROM "MediaVersion" mv
       JOIN "MediaStream" ms
           ON ms."MediaVersionId" = mv."Id" AND ms."MediaStreamKind" = 1
       WHERE mv."Id" = ?
         AND (? IS NULL OR ms."Index" = ?)
       ORDER BY ms."Index"
       LIMIT 1"#;

/// Load every persisted source audio stream for a media version.
///
/// # Errors
///
/// Returns a database error when the query fails and a decode error when any
/// row is malformed. A malformed row is never dropped because doing so could
/// silently select a different audio stream.
pub async fn load_source_audio_streams(
    pool: &DbPool,
    media_version_id: i64,
) -> Result<Vec<SourceAudioStream>> {
    let rows = sqlx::query(
        r#"SELECT "Index", "Codec", "Channels", "Default", "Language", "Title"
           FROM "MediaStream"
           WHERE "MediaVersionId" = ? AND "MediaStreamKind" = 2
           ORDER BY "Index", "Id""#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await
    .repository()?;

    rows.into_iter()
        .map(|row| {
            let index = decode_playback_column::<i64>(&row, "Index")?;
            let index = usize::try_from(index).map_err(|error| RepositoryError::Decode {
                kind: "source audio stream",
                value: format!("Index: {error}"),
            })?;
            let codec: Option<String> = decode_playback_column(&row, "Codec")?;
            let channels = decode_playback_column::<i32>(&row, "Channels")?;
            let channels = u32::try_from(channels).map_err(|error| RepositoryError::Decode {
                kind: "source audio stream",
                value: format!("Channels: {error}"),
            })?;
            let default = decode_playback_bool(&row, "Default")?;
            let language: Option<String> = decode_playback_column(&row, "Language")?;
            let title: Option<String> = decode_playback_column(&row, "Title")?;

            Ok(source_audio_stream(
                index,
                codec.as_deref(),
                Some(channels),
                default,
                language.as_deref(),
                title.as_deref(),
            ))
        })
        .collect()
}

/// Convert fresh probe metadata without persisting it or replacing programme duration.
///
/// # Errors
/// Rejects ambiguous video geometry, invalid indexes and missing video codecs.
/// The probe projection has version-level aspect/rate metadata, so multiple
/// video streams cannot safely be paired with an arbitrary selected stream.
pub fn source_video_stream_from_probe(
    probe: &FfprobeMediaVersion,
) -> Result<Option<SourceVideoStream>> {
    let mut videos = probe
        .streams
        .iter()
        .filter(|stream| stream.kind == ersatzrs_domain::MediaStreamKind::Video);
    let Some(video) = videos.next() else {
        return Ok(None);
    };
    if videos.next().is_some() {
        return Err(probe_metadata_error(
            "multiple video streams have ambiguous geometry",
        ));
    }
    let index = usize::try_from(video.index)
        .map_err(|_| probe_metadata_error("invalid video stream index"))?;
    let codec = video
        .codec
        .as_deref()
        .filter(|codec| !codec.trim().is_empty())
        .ok_or_else(|| probe_metadata_error("video codec is missing"))?;
    let mut stream =
        SourceVideoStream::new(SourceVideoCodec::from_ffmpeg_name(codec)).with_index(index);
    stream.profile.clone_from(&video.profile);
    stream.pixel_format = video
        .pixel_format
        .as_deref()
        .and_then(PixelFormat::from_ffmpeg_name);
    stream.scan_kind = probe.version.video_scan_kind;
    stream.frame_size = frame_size_from_db(
        video.width.unwrap_or(probe.version.width),
        video.height.unwrap_or(probe.version.height),
    );
    stream.frame_rate = probe
        .version
        .r_frame_rate
        .as_deref()
        .and_then(parse_frame_rate);
    stream
        .sample_aspect_ratio
        .clone_from(&probe.version.sample_aspect_ratio);
    stream
        .display_aspect_ratio
        .clone_from(&probe.version.display_aspect_ratio);
    stream.is_still_image = video.attached_pic;
    stream.rotation_degrees = video.rotation;
    stream
        .mastering_display
        .clone_from(&video.mastering_display);
    stream.max_cll = video.max_cll;
    stream.max_fall = video.max_fall;
    stream.color_params = derive_color_params(
        video.pixel_format.as_deref(),
        video.color_range.as_deref(),
        video.color_transfer.as_deref(),
        video.color_primaries.as_deref(),
        video.color_space.as_deref(),
    );
    let configuration = (|| {
        Some(ersatzrs_ffmpeg::DolbyVisionConfiguration {
            profile: u8::try_from(video.dolby_vision_profile?).ok()?,
            bl_signal_compatibility_id: u8::try_from(video.dolby_vision_bl_compatibility_id?)
                .ok()?,
            rpu_present: video.dolby_vision_rpu_present?,
            el_present: video.dolby_vision_el_present?,
            bl_present: video.dolby_vision_bl_present?,
        })
    })();
    apply_source_dolby_vision_metadata(
        &mut stream,
        video.dolby_vision_metadata_state,
        video.dolby_vision_metadata_hint.clone(),
        configuration,
    );
    Ok(Some(stream))
}

/// Convert every fresh audio stream, preserving transport indexes and languages.
/// No database write or historical audio metadata is involved.
///
/// # Errors
/// Rejects invalid or duplicate audio indexes and negative channel counts.
pub fn source_audio_streams_from_probe(
    probe: &FfprobeMediaVersion,
) -> Result<Vec<SourceAudioStream>> {
    let mut indexes = HashSet::new();
    let mut streams = probe
        .streams
        .iter()
        .filter(|stream| stream.kind == ersatzrs_domain::MediaStreamKind::Audio)
        .map(|audio| {
            let index = usize::try_from(audio.index)
                .map_err(|_| probe_metadata_error("invalid audio stream index"))?;
            if !indexes.insert(index) {
                return Err(probe_metadata_error("duplicate audio stream index"));
            }
            let channels = audio
                .channels
                .map(u32::try_from)
                .transpose()
                .map_err(|_| probe_metadata_error("invalid audio channel count"))?;
            Ok(source_audio_stream(
                index,
                audio.codec.as_deref(),
                channels,
                audio.default,
                audio.language.as_deref(),
                audio.title.as_deref(),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    streams.sort_by_key(|stream| stream.index);
    Ok(streams)
}

fn probe_metadata_error(value: &'static str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "fresh playback metadata",
        value: value.to_owned(),
    }
}

fn source_audio_stream(
    index: usize,
    codec: Option<&str>,
    channels: Option<u32>,
    default: bool,
    language: Option<&str>,
    title: Option<&str>,
) -> SourceAudioStream {
    let mut stream = SourceAudioStream::new(SourceAudioCodec::from_ffmpeg_name(
        codec.unwrap_or_default(),
    ))
    .with_index(index);
    stream.channels = channels;
    stream.default = default;
    stream.language = language.and_then(non_empty_string);
    stream.title = title.and_then(non_empty_string);
    stream
}

fn decode_playback_column<T>(row: &crate::db::DbRow, column: &'static str) -> Result<T>
where
    for<'row> T: sqlx::Decode<'row, crate::db::Db> + sqlx::Type<crate::db::Db>,
{
    row.try_get(column)
        .map_err(|error| RepositoryError::Decode {
            kind: "playback metadata row",
            value: format!("{column}: {error}"),
        })
}

fn decode_playback_bool(row: &crate::db::DbRow, column: &'static str) -> Result<bool> {
    let value = decode_playback_column::<i64>(row, column)?;
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RepositoryError::Decode {
            kind: "playback metadata row",
            value: format!("{column}: expected 0 or 1, found {value}"),
        }),
    }
}

#[must_use]
pub fn select_playback_audio_stream_index(
    source_audio_streams: &[SourceAudioStream],
    preferred_language: Option<&str>,
    preferred_title: Option<&str>,
) -> Option<usize> {
    let language = preferred_language.and_then(non_empty_string);
    let title = preferred_title.and_then(non_empty_string);
    if language.is_none() && title.is_none() {
        return None;
    }

    let candidates = language
        .as_deref()
        .and_then(|language| {
            let codes = language_codes::all_language_codes([language]);
            let matching = source_audio_streams
                .iter()
                .filter(|stream| language_matches_preference(stream.language.as_deref(), &codes))
                .collect::<Vec<_>>();
            (!matching.is_empty()).then_some(matching)
        })
        .unwrap_or_else(|| source_audio_streams.iter().collect());

    let candidates = title
        .as_deref()
        .and_then(|title| {
            let matching = candidates
                .iter()
                .copied()
                .filter(|stream| {
                    stream
                        .title
                        .as_deref()
                        .is_some_and(|candidate| contains_case_insensitive(candidate, title))
                })
                .collect::<Vec<_>>();
            (!matching.is_empty()).then_some(matching)
        })
        .unwrap_or(candidates);

    prioritize_audio_stream(&candidates).map(|stream| stream.index)
}

/// Preserve an explicit audio choice when fresh formats use different transport indexes.
/// Ordinary language/title preferences should instead be evaluated on fresh metadata.
/// Codec, channel count and index are format properties, not stable track identity.
///
/// # Errors
/// Rejects missing or ambiguous track identity rather than silently changing a pin.
pub fn remap_pinned_audio_selection(
    selection: PlaybackAudioSelection,
    previous: &[SourceAudioStream],
    fresh: &[SourceAudioStream],
) -> Result<PlaybackAudioSelection> {
    let PlaybackAudioSelection::Exact(index) = selection else {
        return Ok(selection);
    };
    let unavailable = || RepositoryError::Decode {
        kind: "published interval audio selection",
        value: "the pinned audio track cannot be uniquely mapped to the resolved media".to_owned(),
    };
    let selected = previous
        .iter()
        .find(|stream| stream.index == index)
        .ok_or_else(unavailable)?;
    let language = selected
        .language
        .as_deref()
        .map(str::trim)
        .filter(|language| !language.is_empty() && !language.eq_ignore_ascii_case("und"));
    let title = selected
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty());
    if language.is_none() && title.is_none() {
        return if previous.len() == 1 && fresh.len() == 1 {
            Ok(PlaybackAudioSelection::Exact(fresh[0].index))
        } else {
            Err(unavailable())
        };
    }
    let codes = language.map(|language| language_codes::all_language_codes([language]));
    let matches = |stream: &&SourceAudioStream| {
        codes
            .as_ref()
            .is_none_or(|codes| language_matches_preference(stream.language.as_deref(), codes))
            && title.is_none_or(|title| {
                stream
                    .title
                    .as_deref()
                    .is_some_and(|candidate| candidate.trim().eq_ignore_ascii_case(title))
            })
    };
    if previous.iter().filter(matches).count() != 1 {
        return Err(unavailable());
    }
    let mut candidates = fresh.iter().filter(matches);
    let selected = candidates.next().ok_or_else(unavailable)?;
    if candidates.next().is_some() {
        return Err(unavailable());
    }
    Ok(PlaybackAudioSelection::Exact(selected.index))
}

fn contains_case_insensitive(value: &str, needle: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn language_matches_preference(candidate: Option<&str>, codes: &[String]) -> bool {
    let candidate = candidate
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .unwrap_or("und");
    codes
        .iter()
        .any(|code| candidate.eq_ignore_ascii_case(code))
}

fn prioritize_audio_stream<'a>(streams: &[&'a SourceAudioStream]) -> Option<&'a SourceAudioStream> {
    streams
        .iter()
        .copied()
        .filter(|stream| stream.default)
        .max_by_key(|stream| stream.channels.unwrap_or_default())
        .or_else(|| streams.first().copied())
}

fn non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// Populate runtime-only playback flags that ErsatzTV probes immediately before
/// command generation instead of storing in the media schema.
pub async fn enrich_source_video_stream_for_playback(
    pool: &DbPool,
    media_version_id: i64,
    deinterlace: bool,
    source_stream: Option<SourceVideoStream>,
    acceleration: HardwareAccelerationKind,
    media_path: &str,
) -> Option<SourceVideoStream> {
    let mut stream = source_stream?;
    crate::interlace_probe::enrich(pool, media_version_id, media_path, deinterlace, &mut stream)
        .await;

    if should_probe_h264_profile_count(&stream, acceleration) {
        match trace_headers_profile_count(&ffmpeg_path(), media_path).await {
            Ok(count) => {
                stream.has_multiple_profiles = count > 1;
            }
            Err(error) => {
                tracing::info!(
                    %error,
                    media_path,
                    "failed to probe H.264 profile count for QSV decode workaround"
                );
            }
        }
    }

    Some(stream)
}

fn should_probe_h264_profile_count(
    stream: &SourceVideoStream,
    acceleration: HardwareAccelerationKind,
) -> bool {
    acceleration == HardwareAccelerationKind::Qsv
        && stream.codec == SourceVideoCodec::H264
        && !stream.has_multiple_profiles
}

#[derive(Debug, thiserror::Error)]
enum ProfileCountProbeError {
    #[error("failed to spawn ffmpeg profile-count probe for {path}: {source}")]
    Spawn {
        path: String,
        source: std::io::Error,
    },
    #[error("ffmpeg profile-count probe for {path} exited with status {status}")]
    Process {
        path: String,
        status: std::process::ExitStatus,
    },
}

fn trace_headers_profile_count_args(path: &str) -> Vec<String> {
    [
        "-hide_banner",
        "-t",
        "30",
        "-i",
        path,
        "-c",
        "copy",
        "-bsf:v",
        "trace_headers",
        "-f",
        "null",
        "-",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

async fn trace_headers_profile_count(
    ffmpeg_path: &str,
    media_path: &str,
) -> std::result::Result<usize, ProfileCountProbeError> {
    let mut child = Command::new(ffmpeg_path)
        .args(trace_headers_profile_count_args(media_path))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| ProfileCountProbeError::Spawn {
            path: media_path.to_owned(),
            source,
        })?;

    let Some(stderr) = child.stderr.take() else {
        let status = child
            .wait()
            .await
            .map_err(|source| ProfileCountProbeError::Spawn {
                path: media_path.to_owned(),
                source,
            })?;
        return if status.success() {
            Ok(0)
        } else {
            Err(ProfileCountProbeError::Process {
                path: media_path.to_owned(),
                status,
            })
        };
    };

    let mut profiles = HashSet::new();
    let mut lines = BufReader::new(stderr).lines();
    while let Some(line) =
        lines
            .next_line()
            .await
            .map_err(|source| ProfileCountProbeError::Spawn {
                path: media_path.to_owned(),
                source,
            })?
    {
        collect_trace_headers_profile_idc(&line, &mut profiles);
        if profiles.len() > 1 {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Ok(profiles.len());
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|source| ProfileCountProbeError::Spawn {
            path: media_path.to_owned(),
            source,
        })?;
    if status.success() {
        Ok(profiles.len())
    } else {
        Err(ProfileCountProbeError::Process {
            path: media_path.to_owned(),
            status,
        })
    }
}

fn collect_trace_headers_profile_idc(line: &str, profiles: &mut HashSet<String>) {
    if let Some(idx) = line.find("profile_idc") {
        profiles.insert(line[idx..].to_owned());
    }
}

/// Load the persisted `FFmpeg` profile and resolution assigned to a channel.
///
/// # Errors
///
/// Returns an error when the profile or resolution row cannot be queried or
/// decoded from the database.
pub async fn load_channel_playback_profile(
    pool: &DbPool,
    channel_id: i64,
) -> Result<Option<PlaybackProfile>> {
    let row = sqlx::query(
        r#"SELECT fp."Id" AS "ProfileId",
                  fp."Name" AS "ProfileName",
                  fp."ThreadCount" AS "ThreadCount",
                  fp."TranscodeAudio" AS "TranscodeAudio",
                  fp."TranscodeVideo" AS "TranscodeVideo",
                  fp."HardwareAcceleration" AS "HardwareAcceleration",
                  fp."VaapiDisplay" AS "VaapiDisplay",
                  fp."VaapiDriver" AS "VaapiDriver",
                  fp."VaapiDevice" AS "VaapiDevice",
                  fp."QsvExtraHardwareFrames" AS "QsvExtraHardwareFrames",
                  fp."ResolutionId" AS "ResolutionId",
                  fp."ScalingBehavior" AS "ScalingBehavior",
                  fp."PadMode" AS "PadMode",
                  fp."VideoFormat" AS "VideoFormat",
                  fp."VideoProfile" AS "VideoProfile",
                  fp."VideoPreset" AS "VideoPreset",
                  fp."VideoTune" AS "VideoTune",
                  fp."AllowBFrames" AS "AllowBFrames",
                  fp."BitDepth" AS "BitDepth",
                  fp."VideoBitrate" AS "VideoBitrate",
                  fp."VideoBufferSize" AS "VideoBufferSize",
                  fp."TonemapAlgorithm" AS "TonemapAlgorithm",
                  fp."OutputDynamicRange" AS "OutputDynamicRange",
                  fp."HdrMasteringDisplay" AS "HdrMasteringDisplay",
                  fp."HdrMaxCll" AS "HdrMaxCll",
                  fp."HdrMaxFall" AS "HdrMaxFall",
                  fp."AudioFormat" AS "AudioFormat",
                  fp."AudioBitrate" AS "AudioBitrate",
                  fp."AudioBufferSize" AS "AudioBufferSize",
                  fp."NormalizeLoudnessMode" AS "NormalizeLoudnessMode",
                  fp."TargetLoudness" AS "TargetLoudness",
                  fp."AudioChannels" AS "AudioChannels",
                  fp."AudioSampleRate" AS "AudioSampleRate",
                  fp."NormalizeFramerate" AS "NormalizeFramerate",
                  fp."NormalizeColors" AS "NormalizeColors",
                  fp."DeinterlaceVideo" AS "DeinterlaceVideo",
                  r."Id" AS "ResolutionRowId",
                  r."Name" AS "ResolutionName",
                  r."IsCustom" AS "ResolutionIsCustom",
                  r."Width" AS "ResolutionWidth",
                  r."Height" AS "ResolutionHeight"
           FROM "Channel" c
           JOIN "FFmpegProfile" fp ON fp."Id" = c."FFmpegProfileId"
           JOIN "Resolution" r ON r."Id" = fp."ResolutionId"
           WHERE c."Id" = ?"#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await
    .repository()?;

    row.as_ref().map(map_playback_profile).transpose()
}

/// Load a persisted `FFmpeg` profile and resolution by profile id.
///
/// Troubleshooting playback uses an ad hoc `.troubleshooting` channel in
/// ErsatzTV, so it cannot rely on a real `Channel.FFmpegProfileId` row.
///
/// # Errors
///
/// Returns an error when the profile or resolution row cannot be queried or
/// decoded from the database.
pub async fn load_playback_profile(
    pool: &DbPool,
    ffmpeg_profile_id: i64,
) -> Result<Option<PlaybackProfile>> {
    let row = sqlx::query(
        r#"SELECT fp."Id" AS "ProfileId",
                  fp."Name" AS "ProfileName",
                  fp."ThreadCount" AS "ThreadCount",
                  fp."TranscodeAudio" AS "TranscodeAudio",
                  fp."TranscodeVideo" AS "TranscodeVideo",
                  fp."HardwareAcceleration" AS "HardwareAcceleration",
                  fp."VaapiDisplay" AS "VaapiDisplay",
                  fp."VaapiDriver" AS "VaapiDriver",
                  fp."VaapiDevice" AS "VaapiDevice",
                  fp."QsvExtraHardwareFrames" AS "QsvExtraHardwareFrames",
                  fp."ResolutionId" AS "ResolutionId",
                  fp."ScalingBehavior" AS "ScalingBehavior",
                  fp."PadMode" AS "PadMode",
                  fp."VideoFormat" AS "VideoFormat",
                  fp."VideoProfile" AS "VideoProfile",
                  fp."VideoPreset" AS "VideoPreset",
                  fp."VideoTune" AS "VideoTune",
                  fp."AllowBFrames" AS "AllowBFrames",
                  fp."BitDepth" AS "BitDepth",
                  fp."VideoBitrate" AS "VideoBitrate",
                  fp."VideoBufferSize" AS "VideoBufferSize",
                  fp."TonemapAlgorithm" AS "TonemapAlgorithm",
                  fp."OutputDynamicRange" AS "OutputDynamicRange",
                  fp."HdrMasteringDisplay" AS "HdrMasteringDisplay",
                  fp."HdrMaxCll" AS "HdrMaxCll",
                  fp."HdrMaxFall" AS "HdrMaxFall",
                  fp."AudioFormat" AS "AudioFormat",
                  fp."AudioBitrate" AS "AudioBitrate",
                  fp."AudioBufferSize" AS "AudioBufferSize",
                  fp."NormalizeLoudnessMode" AS "NormalizeLoudnessMode",
                  fp."TargetLoudness" AS "TargetLoudness",
                  fp."AudioChannels" AS "AudioChannels",
                  fp."AudioSampleRate" AS "AudioSampleRate",
                  fp."NormalizeFramerate" AS "NormalizeFramerate",
                  fp."NormalizeColors" AS "NormalizeColors",
                  fp."DeinterlaceVideo" AS "DeinterlaceVideo",
                  r."Id" AS "ResolutionRowId",
                  r."Name" AS "ResolutionName",
                  r."IsCustom" AS "ResolutionIsCustom",
                  r."Width" AS "ResolutionWidth",
                  r."Height" AS "ResolutionHeight"
           FROM "FFmpegProfile" fp
           JOIN "Resolution" r ON r."Id" = fp."ResolutionId"
           WHERE fp."Id" = ?"#,
    )
    .bind(ffmpeg_profile_id)
    .fetch_optional(pool)
    .await
    .repository()?;

    row.as_ref().map(map_playback_profile).transpose()
}

/// Resolve the channel-level target frame rate used for profile normalization.
///
/// # Errors
///
/// Returns an error when scheduled playout media versions cannot be queried.
pub async fn load_channel_target_frame_rate(
    pool: &DbPool,
    channel_id: i64,
) -> Result<Option<FrameRate>> {
    let rows = sqlx::query(
        r#"SELECT mv."RFrameRate" AS "RFrameRate"
           FROM "Playout" p
           JOIN "PlayoutItem" pi ON pi."PlayoutId" = p."Id"
           JOIN "MediaVersion" mv
               ON mv."MovieId" = pi."MediaItemId"
               OR mv."EpisodeId" = pi."MediaItemId"
               OR mv."MusicVideoId" = pi."MediaItemId"
               OR mv."OtherVideoId" = pi."MediaItemId"
               OR mv."RemoteStreamId" = pi."MediaItemId"
               OR mv."SongId" = pi."MediaItemId"
               OR mv."ImageId" = pi."MediaItemId"
           WHERE p."ChannelId" = ?
             AND mv."RFrameRate" IS NOT NULL"#,
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await
    .map_err(|error| map_data_access_error("channel target frame rate", &error))?;

    let mut rates = Vec::with_capacity(rows.len());
    for row in &rows {
        let value = decode_playback_column::<String>(row, "RFrameRate")?;
        let frame_rate = parse_frame_rate(&value).ok_or(RepositoryError::Decode {
            kind: "channel target frame rate",
            value,
        })?;
        rates.push(frame_rate);
    }
    rates.sort_by(|a, b| a.parsed().total_cmp(&b.parsed()));
    rates.dedup();

    if rates.len() <= 1 {
        return Ok(None);
    }

    Ok(rates
        .iter()
        .copied()
        .filter(|rate| rate.parsed() > 23.0)
        .min_by(|a, b| a.parsed().total_cmp(&b.parsed()))
        .or(Some(FrameRate::DEFAULT)))
}

fn map_playback_profile(row: &crate::db::DbRow) -> Result<PlaybackProfile> {
    Ok(PlaybackProfile {
        profile: FFmpegProfile {
            id: row.try_get("ProfileId").repository()?,
            name: decode_string_column(row, "ProfileName")?,
            thread_count: row.try_get("ThreadCount").repository()?,
            transcode_audio: decode_bool_column(row, "TranscodeAudio")?,
            transcode_video: decode_bool_column(row, "TranscodeVideo")?,
            hardware_acceleration: decode_enum_column(
                "ffmpeg profile hardware_acceleration",
                row,
                "HardwareAcceleration",
            )?,
            vaapi_display: decode_optional_string_column(row, "VaapiDisplay")?,
            vaapi_driver: decode_enum_column("ffmpeg profile vaapi_driver", row, "VaapiDriver")?,
            vaapi_device: decode_optional_string_column(row, "VaapiDevice")?,
            qsv_extra_hardware_frames: row.try_get("QsvExtraHardwareFrames").repository()?,
            resolution_id: row.try_get("ResolutionId").repository()?,
            scaling_behavior: decode_enum_column(
                "ffmpeg profile scaling_behavior",
                row,
                "ScalingBehavior",
            )?,
            pad_mode: decode_enum_column("ffmpeg profile pad_mode", row, "PadMode")?,
            video_format: decode_enum_column("ffmpeg profile video_format", row, "VideoFormat")?,
            video_profile: decode_optional_string_column(row, "VideoProfile")?,
            video_preset: decode_optional_string_column(row, "VideoPreset")?,
            video_tune: decode_optional_string_column(row, "VideoTune")?,
            allow_b_frames: decode_bool_column(row, "AllowBFrames")?,
            bit_depth: decode_enum_column("ffmpeg profile bit_depth", row, "BitDepth")?,
            video_bitrate: row.try_get("VideoBitrate").repository()?,
            video_buffer_size: row.try_get("VideoBufferSize").repository()?,
            tonemap_algorithm: decode_enum_column(
                "ffmpeg profile tonemap_algorithm",
                row,
                "TonemapAlgorithm",
            )?,
            output_dynamic_range: decode_enum_column(
                "ffmpeg profile output_dynamic_range",
                row,
                "OutputDynamicRange",
            )?,
            hdr_mastering_display: crate::repositories::decode_hdr_mastering_display(row)?,
            hdr_content_light: crate::repositories::decode_hdr_content_light(row)?,
            audio_format: decode_enum_column("ffmpeg profile audio_format", row, "AudioFormat")?,
            audio_bitrate: row.try_get("AudioBitrate").repository()?,
            audio_buffer_size: row.try_get("AudioBufferSize").repository()?,
            normalize_loudness_mode: decode_enum_column(
                "ffmpeg profile normalize_loudness_mode",
                row,
                "NormalizeLoudnessMode",
            )?,
            target_loudness: row.try_get("TargetLoudness").repository()?,
            audio_channels: row.try_get("AudioChannels").repository()?,
            audio_sample_rate: row.try_get("AudioSampleRate").repository()?,
            normalize_framerate: decode_bool_column(row, "NormalizeFramerate")?,
            normalize_colors: decode_bool_column(row, "NormalizeColors")?,
            deinterlace_video: decode_optional_bool_column(row, "DeinterlaceVideo")?,
        },
        resolution: Resolution {
            id: row.try_get("ResolutionRowId").repository()?,
            name: decode_optional_string_column(row, "ResolutionName")?.unwrap_or_default(),
            is_custom: decode_bool_column(row, "ResolutionIsCustom")?,
            width: row.try_get("ResolutionWidth").repository()?,
            height: row.try_get("ResolutionHeight").repository()?,
        },
    })
}

/// Load the ErsatzTV `Subtitle` rows attached to the active media item.
///
/// The `Subtitle` table hangs from metadata rows instead of `MediaVersion`, so
/// this joins each video metadata type by the current `PlayoutItem.MediaItemId`.
///
/// # Errors
///
/// Returns the database error when the subtitle query fails.
pub async fn load_subtitles_for_media_item(
    pool: &DbPool,
    media_item_id: i64,
) -> Result<Vec<PlaybackSubtitle>> {
    let rows = sqlx::query(
        r#"SELECT sub."Id" AS "Id",
                  sub."SubtitleKind" AS "SubtitleKind",
                  sub."Title" AS "Title",
                  sub."StreamIndex" AS "StreamIndex",
                  sub."Codec" AS "Codec",
                  sub."Default" AS "Default",
                  sub."Forced" AS "Forced",
                  sub."SDH" AS "SDH",
                  sub."Language" AS "Language",
                  sub."IsExtracted" AS "IsExtracted",
                  sub."Path" AS "Path"
           FROM "Subtitle" sub
           LEFT JOIN "MovieMetadata" mm ON mm."Id" = sub."MovieMetadataId"
           LEFT JOIN "EpisodeMetadata" em ON em."Id" = sub."EpisodeMetadataId"
           LEFT JOIN "MusicVideoMetadata" mvm ON mvm."Id" = sub."MusicVideoMetadataId"
           LEFT JOIN "OtherVideoMetadata" ovm ON ovm."Id" = sub."OtherVideoMetadataId"
           LEFT JOIN "RemoteStreamMetadata" rsm ON rsm."Id" = sub."RemoteStreamMetadataId"
           WHERE mm."MovieId" = ?
              OR em."EpisodeId" = ?
              OR mvm."MusicVideoId" = ?
              OR ovm."OtherVideoId" = ?
              OR rsm."RemoteStreamId" = ?
           ORDER BY sub."StreamIndex", sub."Id""#,
    )
    .bind(media_item_id)
    .bind(media_item_id)
    .bind(media_item_id)
    .bind(media_item_id)
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .repository()?;

    rows.into_iter()
        .map(|row| {
            Ok(PlaybackSubtitle {
                id: row.try_get("Id").repository()?,
                subtitle_kind: row.try_get("SubtitleKind").repository()?,
                title: row.try_get("Title").repository()?,
                stream_index: row.try_get("StreamIndex").repository()?,
                codec: row.try_get("Codec").repository()?,
                default: row.try_get::<i64, _>("Default").repository()? != 0,
                forced: row.try_get::<i64, _>("Forced").repository()? != 0,
                sdh: row.try_get::<i64, _>("SDH").repository()? != 0,
                language: row.try_get("Language").repository()?,
                is_extracted: row.try_get::<i64, _>("IsExtracted").repository()? != 0,
                path: row.try_get("Path").repository()?,
            })
        })
        .collect()
}

/// Filter embedded metadata against the resolved file before preference ranking.
/// External/generated subtitles are separate sources and remain available.
async fn load_subtitles_for_media_version(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
) -> Result<Vec<PlaybackSubtitle>> {
    let mut candidates = load_subtitles_for_media_item(pool, media_item_id).await?;
    let rows = sqlx::query(
        r#"SELECT "Index", "Title", "Language", "Codec", "Default", "Forced"
        FROM "MediaStream" WHERE "MediaVersionId" = ? AND "MediaStreamKind" = 3"#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let streams = rows
        .iter()
        .map(|row| {
            Ok((
                row.try_get::<i32, _>("Index").repository()?,
                row.try_get::<Option<String>, _>("Title").repository()?,
                row.try_get::<Option<String>, _>("Language").repository()?,
                row.try_get::<Option<String>, _>("Codec").repository()?,
                row.try_get::<i64, _>("Default").repository()? != 0,
                row.try_get::<i64, _>("Forced").repository()? != 0,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    candidates.retain(|subtitle| {
        subtitle.subtitle_kind != subtitle_kind::EMBEDDED
            || streams
                .iter()
                .any(|(index, title, language, codec, default, forced)| {
                    subtitle.stream_index == *index
                        && subtitle.default == *default
                        && subtitle.forced == *forced
                        && optional_stream_text_matches(title.as_deref(), subtitle.title.as_deref())
                        && optional_stream_text_matches(
                            language.as_deref(),
                            subtitle.language.as_deref(),
                        )
                        && optional_stream_text_matches(codec.as_deref(), subtitle.codec.as_deref())
                })
    });
    Ok(candidates)
}

/// Load the persisted audio streams for a media version.
///
/// ErsatzTV's custom channel stream selector evaluates these `MediaStream`
/// fields before falling back to the built-in audio selector.
///
/// # Errors
///
/// Returns database errors from the audio stream query.
pub async fn load_audio_streams_for_media_version(
    pool: &DbPool,
    media_version_id: i64,
) -> Result<Vec<PlaybackAudioStream>> {
    let rows = sqlx::query(
        r#"SELECT "Index", "Title", "Language", "Default", "Forced", "Codec", "Channels"
           FROM "MediaStream"
           WHERE "MediaVersionId" = ? AND "MediaStreamKind" = 2
           ORDER BY "Index", "Id""#,
    )
    .bind(media_version_id)
    .fetch_all(pool)
    .await
    .repository()?;

    rows.into_iter()
        .map(|row| {
            Ok(PlaybackAudioStream {
                stream_index: row.try_get("Index").repository()?,
                title: row.try_get("Title").repository()?,
                language: row.try_get("Language").repository()?,
                default: row.try_get::<i64, _>("Default").repository()? != 0,
                forced: row.try_get::<i64, _>("Forced").repository()? != 0,
                codec: row.try_get("Codec").repository()?,
                channels: row.try_get("Channels").repository()?,
            })
        })
        .collect()
}

/// Execute ErsatzTV's custom channel stream-selector YAML against the current
/// content item.
///
/// This ports `CustomStreamSelector.SelectStreams`: selector items are tried in
/// order; each item may match content, audio, and subtitles; audio/subtitle
/// language list order decides priority; and embedded text subtitles that have
/// not been extracted are ignored outside direct HLS.
///
/// # Errors
///
/// Returns database errors from stream/subtitle reads or decode errors when the
/// YAML file cannot be read or parsed.
pub async fn select_custom_playback_streams(
    pool: &DbPool,
    media_version_id: i64,
    media_item_id: i64,
    channel: &Channel,
    content_start_time: DateTime<Utc>,
    streaming_mode: StreamingMode,
) -> Result<Option<CustomStreamSelection>> {
    select_custom_playback_streams_inner(
        pool,
        media_version_id,
        media_item_id,
        channel,
        content_start_time,
        streaming_mode,
        SubtitleSelectionEngine::Native,
        CustomSelectionContext::default(),
    )
    .await
}

/// Select Next custom tracks while excluding embedded subtitles from other versions.
/// Next reads embedded text directly and excludes external bitmap subtitles.
///
/// # Errors
/// Returns selector, database and subtitle metadata errors.
pub async fn select_custom_playback_streams_for_version(
    pool: &DbPool,
    media_version_id: i64,
    media_item_id: i64,
    channel: &Channel,
    content_start_time: DateTime<Utc>,
    streaming_mode: StreamingMode,
) -> Result<Option<CustomStreamSelection>> {
    select_custom_playback_streams_with_subtitle_candidates(
        pool,
        media_version_id,
        media_item_id,
        channel,
        content_start_time,
        streaming_mode,
        CustomSelectionContext::default(),
    )
    .await
}

/// Troubleshooting restricts candidates before evaluating custom selector rules.
/// The selected record retains its stored metadata for later provenance checks.
#[derive(Default)]
pub(crate) struct CustomSelectionContext<'a> {
    pub subtitles: Option<&'a [PlaybackSubtitle]>,
    pub diagnostics: Option<&'a crate::next_engine::diagnostics::SelectionLog>,
}

pub(crate) async fn select_custom_playback_streams_with_subtitle_candidates(
    pool: &DbPool,
    media_version_id: i64,
    media_item_id: i64,
    channel: &Channel,
    content_start_time: DateTime<Utc>,
    streaming_mode: StreamingMode,
    context: CustomSelectionContext<'_>,
) -> Result<Option<CustomStreamSelection>> {
    select_custom_playback_streams_inner(
        pool,
        media_version_id,
        media_item_id,
        channel,
        content_start_time,
        streaming_mode,
        SubtitleSelectionEngine::Next,
        context,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn select_custom_playback_streams_inner(
    pool: &DbPool,
    media_version_id: i64,
    media_item_id: i64,
    channel: &Channel,
    content_start_time: DateTime<Utc>,
    streaming_mode: StreamingMode,
    engine: SubtitleSelectionEngine,
    context: CustomSelectionContext<'_>,
) -> Result<Option<CustomStreamSelection>> {
    let Some(selector) = load_custom_stream_selector(channel, context.diagnostics).await? else {
        return Ok(None);
    };

    let audio_streams = load_audio_streams_for_media_version(pool, media_version_id).await?;
    let mut subtitles = if let Some(candidates) = context.subtitles {
        candidates.to_vec()
    } else if engine == SubtitleSelectionEngine::Next {
        load_subtitles_for_media_version(pool, media_item_id, media_version_id).await?
    } else {
        load_subtitles_for_media_item(pool, media_item_id).await?
    };
    let media_server = engine == SubtitleSelectionEngine::Next
        && is_media_server_subtitle_item(pool, media_item_id).await?;
    transient::retain_playable_subtitles(&mut subtitles, streaming_mode, engine, media_server);

    Ok(select_custom_stream_candidates(
        selector,
        &audio_streams,
        &mut subtitles,
        context.subtitles.is_some(),
        channel,
        content_start_time,
        context.diagnostics,
    ))
}

async fn load_custom_stream_selector(
    channel: &Channel,
    diagnostics: Option<&crate::next_engine::diagnostics::SelectionLog>,
) -> Result<Option<StreamSelector>> {
    let Some(selector) = channel
        .stream_selector
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if selector.contains("..") || selector.contains('/') || selector.contains('\\') {
        return Ok(None);
    }

    let selector_path = crate::file_system_layout::channel_stream_selectors_folder().join(selector);
    let yaml = match tokio::fs::read_to_string(&selector_path).await {
        Ok(yaml) => yaml,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(log) = diagnostics {
                log.debug("Custom selector file is absent; using normal stream preferences");
            }
            return Ok(None);
        }
        Err(error) => {
            return Err(RepositoryError::Decode {
                kind: "channel stream selector",
                value: error.to_string(),
            });
        }
    };
    serde_yaml::from_str(&yaml)
        .map(Some)
        .map_err(|error| RepositoryError::Decode {
            kind: "channel stream selector",
            value: error.to_string(),
        })
}

fn select_custom_stream_candidates(
    selector: StreamSelector,
    audio_streams: &[PlaybackAudioStream],
    subtitles: &mut [PlaybackSubtitle],
    explicit: bool,
    channel: &Channel,
    content_start_time: DateTime<Utc>,
    diagnostics: Option<&crate::next_engine::diagnostics::SelectionLog>,
) -> Option<CustomStreamSelection> {
    // ErsatzTV treats an explicitly supplied troubleshooting track as forced
    // during selection. Do not mutate the record returned to artifact validation.
    let originals = explicit.then(|| subtitles.to_vec());
    if originals.is_some() {
        for subtitle in &mut *subtitles {
            subtitle.forced = true;
        }
    }

    for (index, item) in selector.items.into_iter().enumerate() {
        if !item.content_condition.trim().is_empty()
            && !content_matches_condition(channel, content_start_time, &item.content_condition)
        {
            if let Some(log) = diagnostics {
                log.debug(&format!(
                    "Custom selector rule {}: content condition did not match",
                    index + 1
                ));
            }
            continue;
        }

        let rule_log = diagnostics.map(|log| (log, index + 1));
        let audio = select_custom_audio(audio_streams, &item, rule_log);
        let subtitles = select_custom_subtitle(subtitles, &item, rule_log);
        if let Some(log) = diagnostics {
            log.debug(&format!("Custom selector rule {}: audio matched={}, subtitle matched={}, subtitles disabled={}", index + 1, audio.passed, subtitles.passed, item.disable_subtitles));
        }
        if audio.passed && subtitles.passed {
            return Some(CustomStreamSelection {
                audio_stream_index: audio
                    .selected
                    .and_then(|stream| usize::try_from(stream.stream_index).ok()),
                subtitle: subtitles
                    .selected
                    .and_then(|selected| {
                        originals.as_ref().map_or(Some(selected), |rows| {
                            rows.iter().find(|row| row.id == selected.id)
                        })
                    })
                    .cloned(),
            });
        }
    }

    if let Some(log) = diagnostics {
        log.debug("No custom selector rule matched; using normal stream preferences");
    }
    None
}

#[derive(Debug, Deserialize, Default)]
struct StreamSelector {
    #[serde(default)]
    items: Vec<StreamSelectorItem>,
}

#[derive(Debug, Deserialize, Default)]
struct StreamSelectorItem {
    #[serde(default, rename = "audio_language")]
    audio_languages: Vec<String>,
    #[serde(default, rename = "audio_title_allowlist")]
    audio_title_allowlist: Vec<String>,
    #[serde(default, rename = "audio_title_blocklist")]
    audio_title_blocklist: Vec<String>,
    #[serde(default, rename = "audio_condition")]
    audio_condition: String,
    #[serde(default, rename = "disable_subtitles")]
    disable_subtitles: bool,
    #[serde(default, rename = "subtitle_language")]
    subtitle_languages: Vec<String>,
    #[serde(default, rename = "subtitle_title_allowlist")]
    subtitle_title_allowlist: Vec<String>,
    #[serde(default, rename = "subtitle_title_blocklist")]
    subtitle_title_blocklist: Vec<String>,
    #[serde(default, rename = "subtitle_condition")]
    subtitle_condition: String,
    #[serde(default, rename = "content_condition")]
    content_condition: String,
}

#[derive(Debug, Clone, Copy)]
struct RankedSelection<'a, T> {
    passed: bool,
    selected: Option<&'a T>,
}

fn select_custom_audio<'a>(
    audio_streams: &'a [PlaybackAudioStream],
    item: &StreamSelectorItem,
    diagnostics: Option<(&crate::next_engine::diagnostics::SelectionLog, usize)>,
) -> RankedSelection<'a, PlaybackAudioStream> {
    let mut candidates = Vec::new();
    let mut passed = false;
    for stream in audio_streams {
        let safe_title = stream
            .title
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let safe_language = stream
            .language
            .as_deref()
            .unwrap_or("und")
            .to_ascii_lowercase();
        let mut matches = item.audio_languages.is_empty();
        let mut rank = i32::MAX;
        for (index, language) in item.audio_languages.iter().enumerate() {
            if language == "*" || simple_expression_matches(language, &safe_language) {
                matches = true;
                rank = rank.min(i32::try_from(index).unwrap_or(i32::MAX));
            }
        }
        if item
            .audio_title_blocklist
            .iter()
            .any(|block| safe_title.contains(&block.to_ascii_lowercase()))
        {
            matches = false;
        }
        if !item.audio_title_allowlist.is_empty()
            && !item
                .audio_title_allowlist
                .iter()
                .any(|block| safe_title.contains(&block.to_ascii_lowercase()))
        {
            matches = false;
        }
        if !item.audio_condition.trim().is_empty()
            && !audio_matches_condition(stream, &item.audio_condition)
        {
            matches = false;
        }
        if let Some((log, rule)) = diagnostics {
            log.debug(&format!("Custom selector rule {rule}: audio stream {}, language={safe_language}, matched={matches}", stream.stream_index));
        }
        if matches {
            passed = true;
            candidates.push((rank, stream.stream_index, stream));
        }
    }
    candidates.sort_by_key(|(rank, stream_index, _)| (*rank, *stream_index));
    RankedSelection {
        passed,
        selected: candidates.first().map(|(_, _, stream)| *stream),
    }
}

fn select_custom_subtitle<'a>(
    subtitles: &'a [PlaybackSubtitle],
    item: &StreamSelectorItem,
    diagnostics: Option<(&crate::next_engine::diagnostics::SelectionLog, usize)>,
) -> RankedSelection<'a, PlaybackSubtitle> {
    if item.disable_subtitles {
        return RankedSelection {
            passed: true,
            selected: None,
        };
    }

    let mut candidates = Vec::new();
    let mut passed = false;
    for subtitle in subtitles {
        let safe_title = subtitle
            .title
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let safe_language = subtitle
            .language
            .as_deref()
            .unwrap_or("und")
            .to_ascii_lowercase();
        let mut matches = item.subtitle_languages.is_empty();
        let mut rank = i32::MAX;
        for (index, language) in item.subtitle_languages.iter().enumerate() {
            if language == "*" || simple_expression_matches(language, &safe_language) {
                matches = true;
                rank = rank.min(i32::try_from(index).unwrap_or(i32::MAX));
            }
        }
        if item
            .subtitle_title_blocklist
            .iter()
            .any(|block| safe_title.contains(&block.to_ascii_lowercase()))
        {
            matches = false;
        }
        if !item.subtitle_title_allowlist.is_empty()
            && !item
                .subtitle_title_allowlist
                .iter()
                .any(|block| safe_title.contains(&block.to_ascii_lowercase()))
        {
            matches = false;
        }
        if !item.subtitle_condition.trim().is_empty()
            && !subtitle_matches_condition(subtitle, &item.subtitle_condition)
        {
            matches = false;
        }
        if let Some((log, rule)) = diagnostics {
            log.debug(&format!("Custom selector rule {rule}: subtitle {}, language={safe_language}, matched={matches}", subtitle.id));
        }
        if matches {
            passed = true;
            candidates.push((rank, subtitle.stream_index, subtitle));
        }
    }
    candidates.sort_by_key(|(rank, stream_index, _)| (*rank, *stream_index));
    RankedSelection {
        passed,
        selected: candidates.first().map(|(_, _, subtitle)| *subtitle),
    }
}

fn simple_expression_matches(pattern: &str, value: &str) -> bool {
    fn matches(pattern: &[u8], value: &[u8]) -> bool {
        if pattern.is_empty() {
            return value.is_empty();
        }
        match pattern[0] {
            b'*' => {
                matches(&pattern[1..], value) || !value.is_empty() && matches(pattern, &value[1..])
            }
            b'?' => !value.is_empty() && matches(&pattern[1..], &value[1..]),
            current => {
                value.first().is_some_and(|candidate| *candidate == current)
                    && matches(&pattern[1..], &value[1..])
            }
        }
    }

    matches(
        pattern.to_ascii_lowercase().as_bytes(),
        value.to_ascii_lowercase().as_bytes(),
    )
}

fn audio_matches_condition(stream: &PlaybackAudioStream, condition: &str) -> bool {
    let mut context = ConditionContext::new();
    context.set("id", i64::from(stream.stream_index));
    context.set(
        "title",
        stream
            .title
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    );
    context.set(
        "lang",
        stream
            .language
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    );
    context.set("default", stream.default);
    context.set("forced", stream.forced);
    context.set(
        "codec",
        stream
            .codec
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    );
    context.set("channels", i64::from(stream.channels.unwrap_or_default()));
    evaluate_condition(condition, &context)
}

fn subtitle_matches_condition(subtitle: &PlaybackSubtitle, condition: &str) -> bool {
    let mut context = ConditionContext::new();
    context.set("id", i64::from(subtitle.stream_index));
    context.set(
        "title",
        subtitle
            .title
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    );
    context.set(
        "lang",
        subtitle
            .language
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    );
    context.set("default", subtitle.default);
    context.set("forced", subtitle.forced);
    context.set("sdh", subtitle.sdh);
    context.set(
        "codec",
        subtitle
            .codec
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase(),
    );
    context.set("external", subtitle.subtitle_kind == subtitle_kind::SIDECAR);
    evaluate_condition(condition, &context)
}

fn content_matches_condition(
    channel: &Channel,
    content_start_time: DateTime<Utc>,
    condition: &str,
) -> bool {
    let mut context = ConditionContext::new();
    context.set("channel_number", channel.number.to_ascii_lowercase());
    context.set("channel_name", channel.name.to_ascii_lowercase());
    context.set(
        "time_of_day_seconds",
        i64::from(content_start_time.time().num_seconds_from_midnight()),
    );
    context.set(
        "day_of_week",
        i64::from(content_start_time.weekday().num_days_from_monday()),
    );
    evaluate_condition(condition, &context)
}

/// Engine-specific subtitle eligibility; native extraction restrictions stay local.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SubtitleSelectionEngine {
    /// Native `FFmpeg` playback, including direct and transcoded HLS.
    Native,
    /// The optional Next runtime, which reads embedded text directly.
    Next,
}

/// Select the subtitle row for channel playback, porting the built-in branch of
/// `FFmpegStreamSelector.SelectSubtitleStream`.
///
/// # Errors
///
/// Returns database errors from subtitle/config reads.
pub async fn select_playback_subtitle(
    pool: &DbPool,
    media_item_id: i64,
    preferred_languages: &[String],
    subtitle_mode: ChannelSubtitleMode,
    streaming_mode: StreamingMode,
) -> Result<Option<PlaybackSubtitle>> {
    select_playback_subtitle_inner(
        pool,
        media_item_id,
        None,
        preferred_languages,
        subtitle_mode,
        streaming_mode,
        SubtitleSelectionEngine::Native,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn select_playback_subtitle_inner(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: Option<i64>,
    preferred_languages: &[String],
    subtitle_mode: ChannelSubtitleMode,
    streaming_mode: StreamingMode,
    engine: SubtitleSelectionEngine,
) -> Result<Option<PlaybackSubtitle>> {
    if subtitle_mode == ChannelSubtitleMode::None {
        return Ok(None);
    }

    let mut candidates = if let Some(version) = media_version_id {
        load_subtitles_for_media_version(pool, media_item_id, version).await?
    } else {
        load_subtitles_for_media_item(pool, media_item_id).await?
    };
    if engine == SubtitleSelectionEngine::Native && !use_embedded_subtitles(pool).await? {
        candidates.retain(|subtitle| subtitle.subtitle_kind != subtitle_kind::EMBEDDED);
    }

    let media_server = engine == SubtitleSelectionEngine::Next
        && is_media_server_subtitle_item(pool, media_item_id).await?;
    transient::retain_playable_subtitles(&mut candidates, streaming_mode, engine, media_server);

    Ok(select_subtitle_preferences(
        &candidates,
        preferred_languages,
        subtitle_mode,
    ))
}

fn select_subtitle_preferences(
    candidates: &[PlaybackSubtitle],
    preferred_languages: &[String],
    subtitle_mode: ChannelSubtitleMode,
) -> Option<PlaybackSubtitle> {
    if preferred_languages.is_empty() {
        return select_subtitle_by_mode(candidates.iter(), subtitle_mode).cloned();
    }

    for language in preferred_languages {
        let codes = language_codes::all_language_codes([language]);
        let selected = select_subtitle_by_mode(
            candidates.iter().filter(|subtitle| {
                language_matches_preference(subtitle.language.as_deref(), &codes)
            }),
            subtitle_mode,
        );
        if let Some(selected) = selected {
            return Some(selected.clone());
        }
    }

    None
}

/// Resolve an exact persisted subtitle stream without preference fallback.
///
/// # Errors
///
/// Returns a decode error if the pinned row is stale or unusable for the
/// requested streaming mode.
pub async fn select_pinned_playback_subtitle(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    stream_index: i32,
    streaming_mode: StreamingMode,
) -> Result<PlaybackSubtitle> {
    select_pinned_playback_subtitle_inner(
        pool,
        media_item_id,
        media_version_id,
        stream_index,
        streaming_mode,
        SubtitleSelectionEngine::Native,
    )
    .await
}

/// Resolve a Next pin without requiring native embedded-text extraction.
///
/// # Errors
/// Returns an error for stale identities, external bitmap sources or provider captions.
pub(crate) async fn select_next_pinned_playback_subtitle(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    stream_index: i32,
) -> Result<PlaybackSubtitle> {
    select_pinned_playback_subtitle_inner(
        pool,
        media_item_id,
        media_version_id,
        stream_index,
        StreamingMode::HttpLiveStreamingSegmenter,
        SubtitleSelectionEngine::Next,
    )
    .await
}

async fn select_pinned_playback_subtitle_inner(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    stream_index: i32,
    streaming_mode: StreamingMode,
    engine: SubtitleSelectionEngine,
) -> Result<PlaybackSubtitle> {
    let candidates = load_subtitles_for_media_item(pool, media_item_id).await?;
    let source_stream = sqlx::query(
        r#"SELECT "Title", "Language", "Codec", "Default", "Forced"
           FROM "MediaStream"
           WHERE "MediaVersionId" = ?
             AND "MediaStreamKind" = 3
             AND "Index" = ?
           ORDER BY "Id"
           LIMIT 1"#,
    )
    .bind(media_version_id)
    .bind(stream_index)
    .fetch_optional(pool)
    .await
    .repository()?;

    let mut matching = if let Some(source_stream) = source_stream {
        let title = source_stream
            .try_get::<Option<String>, _>("Title")
            .repository()?;
        let language = source_stream
            .try_get::<Option<String>, _>("Language")
            .repository()?;
        let codec = source_stream
            .try_get::<Option<String>, _>("Codec")
            .repository()?;
        let is_default = source_stream.try_get::<i64, _>("Default").repository()? != 0;
        let is_forced = source_stream.try_get::<i64, _>("Forced").repository()? != 0;
        candidates
            .into_iter()
            .filter(|subtitle| {
                subtitle.stream_index == stream_index
                    && subtitle.subtitle_kind == subtitle_kind::EMBEDDED
                    && subtitle.default == is_default
                    && subtitle.forced == is_forced
                    && optional_stream_text_matches(title.as_deref(), subtitle.title.as_deref())
                    && optional_stream_text_matches(
                        language.as_deref(),
                        subtitle.language.as_deref(),
                    )
                    && optional_stream_text_matches(codec.as_deref(), subtitle.codec.as_deref())
            })
            .collect::<Vec<_>>()
    } else {
        candidates
            .into_iter()
            .filter(|subtitle| {
                subtitle.stream_index == stream_index
                    && subtitle.subtitle_kind != subtitle_kind::EMBEDDED
            })
            .collect::<Vec<_>>()
    };
    if matching.len() != 1 {
        return Err(RepositoryError::Decode {
            kind: "pinned subtitle stream",
            value: format!(
                "media version {media_version_id} has {} compatible subtitle rows for stream {stream_index}",
                matching.len()
            ),
        });
    }
    let subtitle = matching.pop().ok_or_else(|| RepositoryError::Decode {
        kind: "pinned subtitle stream",
        value: format!(
            "media version {media_version_id} has no compatible subtitle row for stream {stream_index}"
        ),
    })?;
    if engine == SubtitleSelectionEngine::Native
        && streaming_mode != StreamingMode::HttpLiveStreamingDirect
        && subtitle.subtitle_kind == subtitle_kind::EMBEDDED
        && !subtitle.is_image()
        && (!subtitle.is_extracted
            || subtitle
                .path
                .as_deref()
                .is_none_or(|path| path.trim().is_empty()))
    {
        return Err(RepositoryError::Decode {
            kind: "pinned subtitle stream",
            value: format!(
                "embedded text subtitle stream {stream_index} is not extracted for transcoding"
            ),
        });
    }
    if engine == SubtitleSelectionEngine::Next
        && !next_subtitle_supported(
            &subtitle,
            is_media_server_subtitle_item(pool, media_item_id).await?,
        )
    {
        return Err(RepositoryError::Decode {
            kind: "pinned subtitle stream",
            value: "selected subtitle is unsupported by Next".to_owned(),
        });
    }
    Ok(subtitle)
}

fn next_subtitle_supported(subtitle: &PlaybackSubtitle, media_server: bool) -> bool {
    (!subtitle.is_image() || subtitle.subtitle_kind == subtitle_kind::EMBEDDED)
        && !(media_server && subtitle.codec.as_deref() == Some("eia_608"))
}

/// Resolve a troubleshooting selection by row identity against the exact media version.
pub(crate) async fn select_next_troubleshooting_subtitle(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    subtitle_id: i64,
) -> Result<PlaybackSubtitle> {
    let subtitle = load_subtitles_for_media_version(pool, media_item_id, media_version_id)
        .await?
        .into_iter()
        .find(|subtitle| subtitle.id == subtitle_id)
        .ok_or_else(|| {
            RepositoryError::unavailable(
                "Next troubleshooting subtitle",
                "selection is unavailable for this media version",
            )
        })?;
    if !next_subtitle_supported(
        &subtitle,
        is_media_server_subtitle_item(pool, media_item_id).await?,
    ) {
        return Err(RepositoryError::unavailable(
            "Next troubleshooting subtitle",
            "selected subtitle is unsupported by Next",
        ));
    }
    // Selection is already explicit. Preserve the persisted metadata used by
    // retained-caption provenance checks; Next needs no native forced override.
    Ok(subtitle)
}

/// Upstream classifies the persisted provider item, not its resolved transport:
/// path replacement can make a media-server item locally accessible.
async fn is_media_server_subtitle_item(pool: &DbPool, media_item_id: i64) -> Result<bool> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "PlexMovie" WHERE "Id"=?1)
            OR EXISTS(SELECT 1 FROM "PlexEpisode" WHERE "Id"=?1)
            OR EXISTS(SELECT 1 FROM "JellyfinMovie" WHERE "Id"=?1)
            OR EXISTS(SELECT 1 FROM "JellyfinEpisode" WHERE "Id"=?1)
            OR EXISTS(SELECT 1 FROM "EmbyMovie" WHERE "Id"=?1)
            OR EXISTS(SELECT 1 FROM "EmbyEpisode" WHERE "Id"=?1)"#,
    )
    .bind(media_item_id)
    .fetch_one(pool)
    .await
    .repository()
}

fn optional_stream_text_matches(expected: Option<&str>, actual: Option<&str>) -> bool {
    match expected.map(str::trim).filter(|value| !value.is_empty()) {
        Some(expected) => actual
            .map(str::trim)
            .is_some_and(|actual| actual.eq_ignore_ascii_case(expected)),
        None => true,
    }
}

fn select_subtitle_by_mode<'a>(
    candidates: impl Iterator<Item = &'a PlaybackSubtitle>,
    subtitle_mode: ChannelSubtitleMode,
) -> Option<&'a PlaybackSubtitle> {
    match subtitle_mode {
        ChannelSubtitleMode::Forced => candidates
            .filter(|subtitle| subtitle.forced)
            .min_by_key(|subtitle| subtitle.stream_index),
        ChannelSubtitleMode::Default => {
            candidates.min_by_key(|subtitle| (!subtitle.default, subtitle.stream_index))
        }
        ChannelSubtitleMode::Any => candidates.min_by_key(|subtitle| subtitle.stream_index),
        ChannelSubtitleMode::None => None,
    }
}

/// Select the channel playback subtitle, including generated music-video
/// credits when the channel requests them.
///
/// This ports ErsatzTV's streaming path, where `GenerateSubtitles` creates a
/// temporary forced/default ASS subtitle for music videos before `FFmpeg` command
/// generation instead of requiring a persisted `Subtitle` row.
///
/// # Errors
///
/// Returns database or generated-subtitle file errors from music-video credits
/// generation and persisted subtitle lookup.
#[allow(clippy::too_many_arguments)]
pub async fn select_channel_playback_subtitle(
    pool: &DbPool,
    media_item_id: i64,
    preferred_languages: &[String],
    subtitle_mode: ChannelSubtitleMode,
    streaming_mode: StreamingMode,
    channel: &Channel,
    resolution: &Resolution,
    stream_seek_seconds: i64,
) -> Result<Option<PlaybackSubtitle>> {
    if let Some(subtitle) = generate_music_video_credits_subtitle(
        pool,
        media_item_id,
        channel,
        resolution,
        stream_seek_seconds,
    )
    .await?
    {
        return Ok(Some(subtitle));
    }
    select_playback_subtitle(
        pool,
        media_item_id,
        preferred_languages,
        subtitle_mode,
        streaming_mode,
    )
    .await
}

/// Select persisted Next subtitles against the exact resolved version before
/// ranking. Generated credits are attached as a session-bound HTTP source by
/// the Next host, after this selection and only for `MusicVideo` items.
///
/// # Errors
/// Returns database errors from persisted subtitle lookup.
pub async fn select_next_playback_subtitle_for_version(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    preferred_languages: &[String],
    subtitle_mode: ChannelSubtitleMode,
    streaming_mode: StreamingMode,
) -> Result<Option<PlaybackSubtitle>> {
    select_playback_subtitle_inner(
        pool,
        media_item_id,
        Some(media_version_id),
        preferred_languages,
        subtitle_mode,
        streaming_mode,
        SubtitleSelectionEngine::Next,
    )
    .await
}

/// Select static `FFmpeg` watermarks for channel playback, porting the non-deco
/// branches of ErsatzTV's `WatermarkSelector`.
///
/// # Errors
///
/// Returns database errors from watermark/config/artwork reads.
pub async fn select_playback_watermarks(
    pool: &DbPool,
    channel: &Channel,
    playout_item_id: i64,
    disable_watermarks: bool,
    now: DateTime<Utc>,
) -> Result<Vec<ResolvedPlaybackWatermark>> {
    if channel.streaming_mode == StreamingMode::HttpLiveStreamingDirect || disable_watermarks {
        return Ok(Vec::new());
    }

    let context = load_playback_watermark_context(pool, playout_item_id).await?;
    select_context_watermarks(pool, channel, context, Some(playout_item_id), now).await
}

/// Select decoration and channel watermarks for an ephemeral fallback item.
pub(crate) async fn select_fallback_watermarks(
    pool: &DbPool,
    channel: &Channel,
    item: &ersatzrs_domain::PlayoutItem,
    now: DateTime<Utc>,
) -> Result<Vec<ResolvedPlaybackWatermark>> {
    if item.disable_watermarks {
        return Ok(Vec::new());
    }
    let playout_deco_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "DecoId" FROM "Playout" WHERE "Id"=?"#)
            .bind(item.playout_id)
            .fetch_optional(pool)
            .await
            .repository()?
            .flatten();
    let context = PlaybackWatermarkContext {
        playout_id: item.playout_id,
        filler_kind: item.filler_kind,
        playout_deco_id,
    };
    select_context_watermarks(pool, channel, context, None, now).await
}

async fn select_context_watermarks(
    pool: &DbPool,
    channel: &Channel,
    context: PlaybackWatermarkContext,
    playout_item_id: Option<i64>,
    now: DateTime<Utc>,
) -> Result<Vec<ResolvedPlaybackWatermark>> {
    let mut watermark_ids = Vec::new();

    if let Some(template_deco) = load_active_template_deco(pool, context.playout_id, now).await? {
        if apply_deco_watermarks(&template_deco, context.filler_kind, &mut watermark_ids) {
            return resolve_playback_watermarks(pool, channel.id, watermark_ids).await;
        }
    }

    if let Some(playout_deco_id) = context.playout_deco_id {
        let playout_deco = load_playback_deco(pool, playout_deco_id).await?;
        if apply_deco_watermarks(&playout_deco, context.filler_kind, &mut watermark_ids) {
            return resolve_playback_watermarks(pool, channel.id, watermark_ids).await;
        }
    }

    let item_ids = match playout_item_id {
        Some(id) => load_playout_item_watermark_ids(pool, id).await?,
        None => Vec::new(),
    };
    if !item_ids.is_empty() {
        watermark_ids.extend(item_ids);
        return resolve_playback_watermarks(pool, channel.id, watermark_ids).await;
    }

    let fallback_id = match channel.watermark_id {
        Some(id) => Some(id),
        None => load_global_watermark_id(pool).await?,
    };
    watermark_ids.extend(fallback_id);
    resolve_playback_watermarks(pool, channel.id, watermark_ids).await
}

/// Select graphics elements for channel playback, porting ErsatzTV's
/// `GraphicsElementSelector` precedence.
///
/// # Errors
///
/// Returns database errors from playout/deco/graphics reads.
pub async fn select_playback_graphics(
    pool: &DbPool,
    _channel: &Channel,
    playout_item_id: i64,
    now: DateTime<Utc>,
) -> Result<Vec<ResolvedPlaybackGraphicsElement>> {
    let context = load_playback_watermark_context(pool, playout_item_id).await?;
    select_context_graphics(pool, context, Some(playout_item_id), now).await
}

pub(crate) async fn select_fallback_graphics(
    pool: &DbPool,
    item: &ersatzrs_domain::PlayoutItem,
    now: DateTime<Utc>,
) -> Result<Vec<ResolvedPlaybackGraphicsElement>> {
    let playout_deco_id: Option<i64> =
        sqlx::query_scalar(r#"SELECT "DecoId" FROM "Playout" WHERE "Id"=?"#)
            .bind(item.playout_id)
            .fetch_optional(pool)
            .await
            .repository()?
            .flatten();
    select_context_graphics(
        pool,
        PlaybackWatermarkContext {
            playout_id: item.playout_id,
            filler_kind: item.filler_kind,
            playout_deco_id,
        },
        None,
        now,
    )
    .await
}

async fn select_context_graphics(
    pool: &DbPool,
    context: PlaybackWatermarkContext,
    playout_item_id: Option<i64>,
    now: DateTime<Utc>,
) -> Result<Vec<ResolvedPlaybackGraphicsElement>> {
    let mut graphics = Vec::new();

    if let Some(template_deco) =
        load_active_template_graphics_deco(pool, context.playout_id, now).await?
    {
        if apply_deco_graphics(&template_deco, context.filler_kind, &mut graphics) {
            return resolve_playback_graphics(pool, graphics).await;
        }
    }

    if let Some(playout_deco_id) = context.playout_deco_id {
        let playout_deco = load_playback_graphics_deco(pool, playout_deco_id).await?;
        if apply_deco_graphics(&playout_deco, context.filler_kind, &mut graphics) {
            return resolve_playback_graphics(pool, graphics).await;
        }
    }

    if let Some(id) = playout_item_id {
        graphics.extend(load_playout_item_graphics(pool, id).await?);
    }
    resolve_playback_graphics(pool, graphics).await
}

/// Render selected graphics elements into `FFmpeg` pipeline overlays.
///
/// Each graphics element gets its own variable map because
/// `PlayoutItemGraphicsElement.Variables` is stored per selected element.
///
/// # Errors
///
/// Returns database/template errors from variable assembly or rendering errors
/// from generated SVG/ASS artifacts.
pub async fn render_playback_graphics_overlays(
    pool: &DbPool,
    graphics: &[ResolvedPlaybackGraphicsElement],
    context: &PlaybackGraphicsRenderContext,
    output_dir: impl AsRef<Path>,
) -> Result<PlaybackGraphicsArtifacts> {
    render_playback_graphics_overlays_inner(pool, graphics, context, output_dir.as_ref(), None)
        .await
}

/// Render overlays within the caller's shared playback startup deadline.
///
/// Signal cancellation and await this future before releasing the graphics
/// directory. Motion/Hold probes and already started text controllers are
/// stopped and awaited before returning a startup failure.
///
/// # Errors
/// Returns rendering/database errors, or a static startup cancellation/timeout error.
pub async fn render_playback_graphics_overlays_cancellable(
    pool: &DbPool,
    graphics: &[ResolvedPlaybackGraphicsElement],
    context: &PlaybackGraphicsRenderContext,
    output_dir: impl AsRef<Path>,
    deadline: tokio::time::Instant,
    cancellation: tokio::sync::oneshot::Receiver<()>,
) -> Result<PlaybackGraphicsArtifacts> {
    let mut startup = GraphicsStartupBudget {
        deadline,
        cancellation,
    };
    render_playback_graphics_overlays_inner(
        pool,
        graphics,
        context,
        output_dir.as_ref(),
        Some(&mut startup),
    )
    .await
}

struct GraphicsStartupBudget {
    deadline: tokio::time::Instant,
    cancellation: tokio::sync::oneshot::Receiver<()>,
}

impl GraphicsStartupBudget {
    fn check(&mut self) -> Result<()> {
        if !matches!(
            self.cancellation.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ) {
            return Err(graphics_startup_error("cancelled"));
        }
        if tokio::time::Instant::now() >= self.deadline {
            return Err(graphics_startup_error("deadline exceeded"));
        }
        Ok(())
    }

    // Only child-free futures may be dropped here. DynamicTextController::start
    // spawns its refresh task after its last await and returns it in the same poll.
    async fn child_free<T>(&mut self, work: impl std::future::Future<Output = T>) -> Result<T> {
        self.check()?;
        tokio::select! {
            biased;
            _ = &mut self.cancellation => Err(graphics_startup_error("cancelled")),
            () = tokio::time::sleep_until(self.deadline) => {
                Err(graphics_startup_error("deadline exceeded"))
            }
            result = work => Ok(result),
        }
    }
}

fn graphics_startup_error(value: &'static str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "graphics startup",
        value: value.to_owned(),
    }
}

#[allow(clippy::too_many_lines)]
async fn render_playback_graphics_overlays_inner(
    pool: &DbPool,
    graphics: &[ResolvedPlaybackGraphicsElement],
    context: &PlaybackGraphicsRenderContext,
    output_dir: &Path,
    mut startup: Option<&mut GraphicsStartupBudget>,
) -> Result<PlaybackGraphicsArtifacts> {
    let output_dir = output_dir.to_path_buf();
    let renderer = GraphicsTemplateRenderer::new(&output_dir);
    let mut artifacts = PlaybackGraphicsArtifacts::default();
    let render_result: Result<()> = async {
        if let Some(startup) = startup.as_deref_mut() {
            startup.check()?;
        }
        for resolved in graphics {
            let variable_context = GraphicsTemplateContext {
                channel_id: context.channel_id,
                channel_number: context.channel_number.clone(),
                media_item_id: context.media_item_id,
                media_path: context.media_path.clone(),
                channel_start_time: context.channel_start_time,
                content_start_time: context.content_start_time,
                stream_seek_seconds: context.stream_seek_seconds,
                duration_seconds: context.duration_seconds,
                frame_width: context.frame_size.width,
                frame_height: context.frame_size.height,
                scaled_frame_width: context.square_pixel_frame_size.width,
                scaled_frame_height: context.square_pixel_frame_size.height,
                r_frame_rate: context.frame_rate.map(FrameRate::r_frame_rate),
                frame_rate: context.frame_rate.map(FrameRate::parsed),
            };
            let variables = graphics_template_variables(
                pool,
                &variable_context,
                usize::try_from(resolved.element.epg_entries.max(0)).unwrap_or_default(),
                resolved.variables.as_deref(),
            );
            let variables = match startup.as_deref_mut() {
                Some(startup) => startup.child_free(variables).await??,
                None => variables.await?,
            };
            let element = render_graphics_element_templates(&resolved.element, &variables)?;
            let render_context = graphics_render_context_with_motion_probe(
                context,
                &element,
                startup.as_deref_mut(),
            )
            .await?;
            let drawtext_config = match &element.config {
                GraphicsElementConfig::Text(config)
                    if config.renderer == TextGraphicsRenderer::Drawtext =>
                {
                    Some(config)
                }
                _ => None,
            };
            if let Some(config) = drawtext_config {
                let Some(template) = config.text.clone() else {
                    tracing::warn!(
                        graphics_element_id = element.id,
                        "skipping dynamic text graphics element without a text template"
                    );
                    continue;
                };
                let text_file = output_dir.join(format!("{}-dynamic-text.txt", element.id));
                let controller = DynamicTextController::start(DynamicTextControllerRequest {
                    element_id: element.id,
                    template,
                    variables,
                    text_file: text_file.clone(),
                    refresh_interval_milliseconds: config.drawtext.refresh_interval_milliseconds,
                    playback_offset_seconds: context.stream_seek_seconds,
                    playback_duration_seconds: context.duration_seconds,
                });
                let controller = match startup.as_deref_mut() {
                    Some(startup) => startup.child_free(controller).await?,
                    None => controller.await,
                };
                let controller = match controller {
                    Ok(controller) => controller,
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            graphics_element_id = element.id,
                            "skipping invalid dynamic text graphics element"
                        );
                        continue;
                    }
                };
                let overlay_result = graphics_element_drawtext_overlay(
                    &element,
                    text_file,
                    fonts_cache_folder().join("Roboto-Regular.ttf"),
                    &render_context,
                )
                .map_err(|error| error.to_string());
                let overlay = match overlay_result {
                    Ok(overlay) => overlay,
                    Err(error) => {
                        controller.shutdown().await;
                        tracing::warn!(
                            %error,
                            graphics_element_id = element.id,
                            "skipping dynamic text graphics element with an invalid overlay"
                        );
                        continue;
                    }
                };
                artifacts.overlays.push(overlay);
                artifacts.dynamic_text_controllers.push(controller);
            } else {
                let scalar_variables = scalar_template_variables(&variables);
                for rendered in renderer
                    .render_graphics_elements(
                        std::slice::from_ref(&element),
                        &scalar_variables,
                        &render_context,
                    )
                    .map_err(|error| RepositoryError::Decode {
                        kind: "graphics render",
                        value: error.to_string(),
                    })?
                {
                    artifacts.overlays.push(rendered.into_pipeline_overlay());
                }
            }
        }
        if let Some(startup) = startup.as_deref_mut() {
            startup.check()?;
        }
        Ok(())
    }
    .await;
    if let Err(error) = render_result {
        shutdown_dynamic_text_controllers(std::mem::take(&mut artifacts.dynamic_text_controllers))
            .await;
        return Err(error);
    }
    artifacts.overlays.sort_by_key(graphics_overlay_z_index);
    Ok(artifacts)
}

async fn graphics_render_context_with_motion_probe(
    context: &PlaybackGraphicsRenderContext,
    element: &GraphicsElement,
    startup: Option<&mut GraphicsStartupBudget>,
) -> Result<GraphicsRenderContext> {
    let render_context =
        GraphicsRenderContext::new(context.frame_size, context.square_pixel_frame_size);
    let GraphicsElementConfig::Motion(config) = &element.config else {
        return Ok(render_context);
    };
    if config.end_behavior != MotionEndBehavior::Hold {
        return Ok(render_context);
    }
    let Some(video_path) = config
        .video_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(render_context);
    };

    let extractor = FfprobeExtractor::new(ffprobe_path());
    let probe = match startup {
        Some(startup) => graphics_motion_probe_cancellable(&extractor, video_path, startup).await?,
        None => extractor.extract(video_path).await,
    };
    Ok(match probe {
        Ok(probe) => {
            if let Ok(duration) = probe.version.duration.to_std() {
                render_context.with_motion_source_duration(element.id, duration.as_secs_f64())
            } else {
                render_context
            }
        }
        Err(error) => {
            tracing::debug!(%error, element_id = element.id, video_path, "failed to probe graphics motion duration; using zero-duration hold fallback");
            render_context.with_motion_source_duration(element.id, 0.0)
        }
    })
}

async fn graphics_motion_probe_cancellable(
    extractor: &FfprobeExtractor,
    video_path: &str,
    startup: &mut GraphicsStartupBudget,
) -> Result<std::result::Result<FfprobeMediaVersion, crate::ffprobe::FfprobeError>> {
    startup.check()?;
    let remaining = startup
        .deadline
        .saturating_duration_since(tokio::time::Instant::now())
        .min(Duration::from_secs(15));
    let input = InputFile::video(video_path);
    let (cancel, cancellation) = tokio::sync::oneshot::channel();
    // Reuse the bounded startup-input owner for a local Motion/Hold asset.
    // It owns bounded pipe readers and awaits process cleanup on cancellation.
    let probe = extractor.extract_input_cancellable(&input, remaining, cancellation);
    tokio::pin!(probe);
    let result = tokio::select! {
        biased;
        _ = &mut startup.cancellation => {
            let _ = cancel.send(());
            let _ = probe.await;
            return Err(graphics_startup_error("cancelled"));
        }
        () = tokio::time::sleep_until(startup.deadline) => {
            let _ = cancel.send(());
            let _ = probe.await;
            return Err(graphics_startup_error("deadline exceeded"));
        }
        result = &mut probe => result,
    };
    startup.check()?;
    match result {
        Err(crate::ffprobe::FfprobeError::Timeout { .. }) => {
            Err(graphics_startup_error("deadline exceeded"))
        }
        Err(crate::ffprobe::FfprobeError::Cancelled { .. }) => {
            Err(graphics_startup_error("cancelled"))
        }
        result => Ok(result),
    }
}

pub(crate) fn render_graphics_element_templates(
    element: &GraphicsElement,
    variables: &serde_json::Value,
) -> Result<GraphicsElement> {
    let mut element = element.clone();
    element.opacity_expression =
        render_optional_graphics_template(element.opacity_expression.as_deref(), variables)?;
    element.config = match element.config {
        GraphicsElementConfig::Image(mut config) => {
            config.image = render_optional_graphics_template(config.image.as_deref(), variables)?;
            GraphicsElementConfig::Image(config)
        }
        GraphicsElementConfig::Text(mut config) => {
            if config.renderer == TextGraphicsRenderer::Drawvg {
                config.text = render_optional_graphics_template(config.text.as_deref(), variables)?;
            }
            config.base_style =
                render_optional_graphics_template(config.base_style.as_deref(), variables)?;
            config.include_fonts_from =
                render_optional_graphics_template(config.include_fonts_from.as_deref(), variables)?;
            GraphicsElementConfig::Text(config)
        }
        GraphicsElementConfig::Subtitle(mut config) => {
            config.template =
                render_optional_graphics_template(config.template.as_deref(), variables)?;
            GraphicsElementConfig::Subtitle(config)
        }
        GraphicsElementConfig::Motion(mut config) => {
            config.video_path =
                render_optional_graphics_template(config.video_path.as_deref(), variables)?;
            GraphicsElementConfig::Motion(config)
        }
        GraphicsElementConfig::Script(mut config) => {
            config.command =
                render_optional_graphics_template(config.command.as_deref(), variables)?;
            config.args = config
                .args
                .iter()
                .map(|arg| render_graphics_template(arg, variables))
                .collect::<Result<Vec<_>>>()?;
            GraphicsElementConfig::Script(config)
        }
    };
    Ok(element)
}

fn render_optional_graphics_template(
    value: Option<&str>,
    variables: &serde_json::Value,
) -> Result<Option<String>> {
    value
        .map(|value| render_graphics_template(value, variables))
        .transpose()
}

fn graphics_overlay_z_index(overlay: &GraphicsPipelineOverlay) -> i32 {
    match overlay {
        GraphicsPipelineOverlay::Drawvg { z_index, .. }
        | GraphicsPipelineOverlay::Drawtext { z_index, .. }
        | GraphicsPipelineOverlay::NativeImage { z_index, .. } => *z_index,
    }
}

async fn resolve_playback_graphics(
    pool: &DbPool,
    graphics: Vec<(i64, Option<String>)>,
) -> Result<Vec<ResolvedPlaybackGraphicsElement>> {
    let repository = crate::repositories::SqliteGraphicsRepository(pool.clone());
    let mut resolved = Vec::with_capacity(graphics.len());
    for (id, variables) in graphics {
        resolved.push(ResolvedPlaybackGraphicsElement {
            element: repository.get_element(id).await?,
            variables,
        });
    }
    resolved.sort_by_key(|graphics| graphics.element.z_index);
    Ok(resolved)
}

/// Resolve explicit graphics element ids selected on the troubleshooting page.
///
/// # Errors
///
/// Returns repository errors when any selected graphics element cannot be read
/// or decoded.
pub async fn resolve_playback_graphics_by_ids(
    pool: &DbPool,
    graphics_element_ids: Vec<i64>,
) -> Result<Vec<ResolvedPlaybackGraphicsElement>> {
    resolve_playback_graphics(
        pool,
        graphics_element_ids
            .into_iter()
            .map(|id| (id, None))
            .collect(),
    )
    .await
}

async fn resolve_playback_watermarks(
    pool: &DbPool,
    channel_id: i64,
    watermark_ids: Vec<i64>,
) -> Result<Vec<ResolvedPlaybackWatermark>> {
    let mut watermarks = Vec::with_capacity(watermark_ids.len());
    for id in watermark_ids {
        watermarks.push(load_channel_watermark(pool, id).await?);
    }
    watermarks.sort_by_key(|watermark| watermark.z_index);
    let mut resolved = Vec::new();
    for watermark in watermarks {
        if let Some(image_path) = resolve_watermark_image_path(pool, channel_id, &watermark).await?
        {
            resolved.push(ResolvedPlaybackWatermark {
                watermark,
                image: image_path,
            });
        }
    }
    Ok(resolved)
}

/// Resolve explicit watermark ids selected on the troubleshooting page.
///
/// # Errors
///
/// Returns database errors from watermark/artwork reads.
pub async fn resolve_playback_watermarks_by_ids(
    pool: &DbPool,
    channel_id: i64,
    watermark_ids: Vec<i64>,
) -> Result<Vec<ResolvedPlaybackWatermark>> {
    resolve_playback_watermarks(pool, channel_id, watermark_ids).await
}

/// Map resolved playback watermarks to `(input, overlay)` pairs for the
/// stream builders, applying the `SetWatermark` loop-option decision per
/// image.
pub async fn watermark_inputs_for_stream(
    watermarks: Vec<ResolvedPlaybackWatermark>,
    program_duration: Option<Duration>,
) -> Vec<(InputFile, WatermarkOverlay)> {
    let mut inputs = Vec::with_capacity(watermarks.len());
    for resolved in watermarks {
        if let Some(overlay) = watermark_overlay_for_stream(&resolved.watermark, program_duration) {
            let path = resolved.image.input_path().into_owned();
            let input = watermark_input_for_stream(&path, &overlay).await;
            inputs.push((input, overlay));
        }
    }
    inputs
}

/// Build the `FFmpeg` input for a resolved watermark image, mirroring `ErsatzTV`'s
/// `SetWatermark` loop-option decision: an animated image (probed once via
/// ffprobe like `IsWatermarkAnimated`) gets `-ignore_loop 0`; a still image
/// gets `-loop 1` when fade points or an opacity expression require the image
/// timeline to advance; a plain still image gets no loop option.
pub async fn watermark_input_for_stream(
    image_path: &str,
    overlay: &ersatzrs_ffmpeg::WatermarkOverlay,
) -> InputFile {
    watermark_input_with_animation(image_path, overlay, is_watermark_animated(image_path).await)
}

fn watermark_input_with_animation(
    image_path: &str,
    overlay: &ersatzrs_ffmpeg::WatermarkOverlay,
    animated: bool,
) -> InputFile {
    if animated {
        InputFile::watermark_animated(image_path)
    } else if !overlay.fade_points.is_empty()
        || overlay
            .opacity_expression
            .as_deref()
            .is_some_and(|expression| !expression.trim().is_empty())
    {
        InputFile::watermark_looped(image_path)
    } else {
        InputFile::watermark(image_path)
    }
}

/// Resolve watermark inputs within the shared playback startup deadline.
/// Signal cancellation and await this future before releasing startup ownership.
///
/// # Errors
/// Returns a static startup error on cancellation or deadline expiry. Ordinary
/// frame-count failures preserve the historical still-image fallback.
pub async fn watermark_inputs_for_stream_cancellable(
    watermarks: Vec<ResolvedPlaybackWatermark>,
    program_duration: Option<Duration>,
    deadline: tokio::time::Instant,
    cancellation: tokio::sync::oneshot::Receiver<()>,
) -> Result<Vec<(InputFile, WatermarkOverlay)>> {
    let mut startup = GraphicsStartupBudget {
        deadline,
        cancellation,
    };
    startup.check()?;
    let mut inputs = Vec::with_capacity(watermarks.len());
    for resolved in watermarks {
        if let Some(overlay) = watermark_overlay_for_stream(&resolved.watermark, program_duration) {
            let path = resolved.image.input_path().into_owned();
            let animated = is_watermark_animated_inner(&path, Some(&mut startup)).await?;
            inputs.push((
                watermark_input_with_animation(&path, &overlay, animated),
                overlay,
            ));
        }
    }
    startup.check()?;
    Ok(inputs)
}

/// Port of `FFmpegLibraryProcessService.IsWatermarkAnimated`: count decoded
/// frames with ffprobe, treat more than one frame as animated, cache the
/// answer per file name for a day, and fall back to "still" on any error.
async fn is_watermark_animated(path: &str) -> bool {
    is_watermark_animated_inner(path, None)
        .await
        .unwrap_or(false)
}

async fn is_watermark_animated_inner(
    path: &str,
    mut startup: Option<&mut GraphicsStartupBudget>,
) -> Result<bool> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    static CACHE: OnceLock<Mutex<HashMap<String, (bool, Instant)>>> = OnceLock::new();
    const CACHE_TTL: std::time::Duration = std::time::Duration::from_hours(24);

    if let Some(startup) = startup.as_deref_mut() {
        startup.check()?;
    }

    let cache_key = Path::new(path).file_name().map_or_else(
        || path.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((animated, stored)) = guard.get(&cache_key) {
            if stored.elapsed() < CACHE_TTL {
                return Ok(*animated);
            }
        }
    }

    if let Some(startup) = startup {
        let extractor = FfprobeExtractor::new(ffprobe_path());
        let Some(animated) =
            probe_watermark_animation_cancellable(&extractor, path, startup).await?
        else {
            return Ok(false);
        };
        if let Ok(mut guard) = cache.lock() {
            guard.insert(cache_key, (animated, Instant::now()));
        }
        return Ok(animated);
    }

    let output = tokio::process::Command::new(ffprobe_path())
        .args([
            "-loglevel",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "stream=nb_read_frames",
            "-print_format",
            "csv",
            path,
        ])
        .output()
        .await;

    Ok(match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let Ok(frame_count) = stdout.trim().replace("stream,", "").parse::<i64>() else {
                return Ok(false);
            };
            let animated = frame_count > 1;
            if let Ok(mut guard) = cache.lock() {
                guard.insert(cache_key, (animated, Instant::now()));
            }
            animated
        }
        Ok(output) => {
            tracing::warn!(
                path,
                exit_code = output.status.code(),
                "error checking watermark frame count"
            );
            false
        }
        Err(error) => {
            tracing::warn!(%error, path, "error checking watermark frame count");
            false
        }
    })
}

async fn probe_watermark_animation_cancellable(
    extractor: &FfprobeExtractor,
    path: &str,
    startup: &mut GraphicsStartupBudget,
) -> Result<Option<bool>> {
    startup.check()?;
    let (cancel, receiver) = tokio::sync::oneshot::channel();
    let probe = extractor.count_watermark_frames_cancellable(path, startup.deadline, receiver);
    tokio::pin!(probe);
    let result = tokio::select! {
        biased;
        _ = &mut startup.cancellation => {
            let _ = cancel.send(());
            let _ = probe.await;
            return Err(graphics_startup_error("cancelled"));
        }
        result = &mut probe => result,
    };
    startup.check()?;
    match result {
        Ok(frames) => Ok(Some(frames > 1)),
        Err(crate::ffprobe::FfprobeError::Timeout { .. }) => {
            Err(graphics_startup_error("deadline exceeded"))
        }
        Err(crate::ffprobe::FfprobeError::Cancelled { .. }) => {
            Err(graphics_startup_error("cancelled"))
        }
        Err(_) => {
            tracing::warn!("error checking watermark frame count; using still-image fallback");
            Ok(None)
        }
    }
}

#[must_use]
pub fn watermark_overlay_for_stream(
    watermark: &ChannelWatermark,
    program_duration: Option<Duration>,
) -> Option<WatermarkOverlay> {
    if watermark.mode == ChannelWatermarkMode::None {
        return None;
    }
    let opacity = u8::try_from(watermark.opacity.clamp(0, 100)).unwrap_or(100);
    let mut overlay = WatermarkOverlay {
        location: watermark.location,
        size: watermark.size,
        width_percent: watermark.width_percent,
        horizontal_margin_percent: watermark.horizontal_margin_percent,
        vertical_margin_percent: watermark.vertical_margin_percent,
        opacity,
        place_within_source_content: watermark.place_within_source_content,
        fade_points: Vec::new(),
        opacity_expression: None,
        enable: None,
        eof_action_pass: false,
        z_index: watermark.z_index,
    };

    match watermark.mode {
        ChannelWatermarkMode::None => None,
        ChannelWatermarkMode::Permanent => Some(overlay),
        ChannelWatermarkMode::Intermittent => {
            overlay.fade_points = watermark_fade_points_for_program(
                program_duration?,
                watermark.frequency_minutes,
                watermark.duration_seconds,
            );
            Some(overlay)
        }
        ChannelWatermarkMode::OpacityExpression => {
            overlay
                .opacity_expression
                .clone_from(&watermark.opacity_expression);
            Some(overlay)
        }
    }
}

#[must_use]
pub fn watermark_fade_points_for_program(
    program_duration: Duration,
    frequency_minutes: i32,
    duration_seconds: i32,
) -> Vec<WatermarkFadePoint> {
    let Ok(frequency_minutes) = u64::try_from(frequency_minutes) else {
        return Vec::new();
    };
    let Ok(duration_seconds) = u64::try_from(duration_seconds) else {
        return Vec::new();
    };
    if frequency_minutes == 0 || duration_seconds == 0 {
        return Vec::new();
    }

    let frequency = Duration::from_secs(frequency_minutes.saturating_mul(60));
    let visible = Duration::from_secs(duration_seconds);
    let fade = Duration::from_secs(1);
    let mut points = Vec::new();
    let mut start = Duration::ZERO;
    while start < program_duration {
        let hold_finish = start.saturating_add(fade).saturating_add(visible);
        let fade_finish = hold_finish.saturating_add(fade);
        points.push(WatermarkFadePoint {
            kind: WatermarkFadeKind::In,
            time: start,
            enable_start: start,
            enable_finish: start.saturating_add(fade),
        });
        points.push(WatermarkFadePoint {
            kind: WatermarkFadeKind::Out,
            time: hold_finish,
            enable_start: hold_finish,
            enable_finish: fade_finish,
        });
        start = start.saturating_add(frequency);
    }
    points
}

async fn load_playout_item_watermark_ids(pool: &DbPool, playout_item_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query(
        r#"SELECT "WatermarkId"
           FROM "PlayoutItemWatermark"
           WHERE "PlayoutItemId" = ?
           ORDER BY "WatermarkId""#,
    )
    .bind(playout_item_id)
    .fetch_all(pool)
    .await
    .repository()?;

    rows.into_iter()
        .map(|row| row.try_get("WatermarkId").repository())
        .collect()
}

async fn load_playback_watermark_context(
    pool: &DbPool,
    playout_item_id: i64,
) -> Result<PlaybackWatermarkContext> {
    let row = sqlx::query(
        r#"SELECT "pi"."PlayoutId", "pi"."FillerKind", "p"."DecoId"
           FROM "PlayoutItem" AS "pi"
           JOIN "Playout" AS "p" ON "p"."Id" = "pi"."PlayoutId"
           WHERE "pi"."Id" = ?"#,
    )
    .bind(playout_item_id)
    .fetch_one(pool)
    .await
    .repository()?;
    Ok(PlaybackWatermarkContext {
        playout_id: row.try_get("PlayoutId").repository()?,
        filler_kind: decode_enum_column("playout item filler kind", &row, "FillerKind")?,
        playout_deco_id: row.try_get("DecoId").repository()?,
    })
}

async fn load_playback_deco(pool: &DbPool, deco_id: i64) -> Result<PlaybackDeco> {
    let row = sqlx::query(
        r#"SELECT "WatermarkMode", "UseWatermarkDuringFiller"
           FROM "Deco"
           WHERE "Id" = ?"#,
    )
    .bind(deco_id)
    .fetch_one(pool)
    .await
    .repository()?;
    Ok(PlaybackDeco {
        mode: decode_enum_column("deco watermark mode", &row, "WatermarkMode")?,
        use_watermark_during_filler: decode_bool_column(&row, "UseWatermarkDuringFiller")?,
        watermark_ids: load_deco_watermark_ids(pool, deco_id).await?,
    })
}

async fn load_playback_graphics_deco(pool: &DbPool, deco_id: i64) -> Result<PlaybackGraphicsDeco> {
    let row = sqlx::query(
        r#"SELECT "GraphicsElementsMode", "UseGraphicsElementsDuringFiller"
           FROM "Deco"
           WHERE "Id" = ?"#,
    )
    .bind(deco_id)
    .fetch_one(pool)
    .await
    .repository()?;
    Ok(PlaybackGraphicsDeco {
        mode: decode_enum_column("deco graphics elements mode", &row, "GraphicsElementsMode")?,
        use_graphics_during_filler: decode_bool_column(&row, "UseGraphicsElementsDuringFiller")?,
        graphics_element_ids: load_deco_graphics_element_ids(pool, deco_id).await?,
    })
}

async fn load_deco_watermark_ids(pool: &DbPool, deco_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query(
        r#"SELECT "WatermarkId"
           FROM "DecoWatermark"
           WHERE "DecoId" = ?
           ORDER BY "WatermarkId""#,
    )
    .bind(deco_id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.into_iter()
        .map(|row| row.try_get("WatermarkId").repository())
        .collect()
}

async fn load_deco_graphics_element_ids(pool: &DbPool, deco_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query(
        r#"SELECT "GraphicsElementId"
           FROM "DecoGraphicsElement"
           WHERE "DecoId" = ?
           ORDER BY "GraphicsElementId""#,
    )
    .bind(deco_id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.into_iter()
        .map(|row| row.try_get("GraphicsElementId").repository())
        .collect()
}

async fn load_active_template_deco(
    pool: &DbPool,
    playout_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<PlaybackDeco>> {
    match active_template_deco_id(pool, playout_id, now).await? {
        Some(id) => load_playback_deco(pool, id).await.map(Some),
        None => Ok(None),
    }
}

pub(crate) async fn active_template_deco_id(
    pool: &DbPool,
    playout_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<i64>> {
    let Some(deco_template_id) = active_playout_template_deco_id(pool, playout_id, now).await?
    else {
        return Ok(None);
    };

    let time_seconds = i64::from(now.time().num_seconds_from_midnight());
    let rows = sqlx::query(
        r#"SELECT "dti"."DecoId", "dti"."StartTime", "dti"."EndTime"
           FROM "DecoTemplateItem" AS "dti"
           WHERE "dti"."DecoTemplateId" = ?
           ORDER BY "dti"."StartTime", "dti"."Id""#,
    )
    .bind(deco_template_id)
    .fetch_all(pool)
    .await
    .repository()?;

    for row in rows {
        let start = parse_time_of_day_seconds(&row.try_get::<String, _>("StartTime").repository()?);
        let finish = parse_time_of_day_seconds(&row.try_get::<String, _>("EndTime").repository()?);
        if start <= time_seconds && (finish == 0 || finish > time_seconds) {
            return row.try_get("DecoId").repository().map(Some);
        }
    }

    Ok(None)
}

async fn load_active_template_graphics_deco(
    pool: &DbPool,
    playout_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<PlaybackGraphicsDeco>> {
    match active_template_deco_id(pool, playout_id, now).await? {
        Some(id) => load_playback_graphics_deco(pool, id).await.map(Some),
        None => Ok(None),
    }
}

async fn active_playout_template_deco_id(
    pool: &DbPool,
    playout_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<i64>> {
    let rows = sqlx::query(
        r#"SELECT "DecoTemplateId", "DaysOfWeek", "DaysOfMonth", "MonthsOfYear",
                  "LimitToDateRange", "StartMonth", "StartDay", "StartYear",
                  "EndMonth", "EndDay", "EndYear"
           FROM "PlayoutTemplate"
           WHERE "PlayoutId" = ? AND "DecoTemplateId" IS NOT NULL
           ORDER BY "Index", "TemplateId""#,
    )
    .bind(playout_id)
    .fetch_all(pool)
    .await
    .repository()?;

    for row in rows {
        if playout_template_matches(&row, now) {
            return row.try_get("DecoTemplateId").repository();
        }
    }
    Ok(None)
}

async fn load_playout_item_graphics(
    pool: &DbPool,
    playout_item_id: i64,
) -> Result<Vec<(i64, Option<String>)>> {
    let rows = sqlx::query(
        r#"SELECT "GraphicsElementId", "Variables"
           FROM "PlayoutItemGraphicsElement"
           WHERE "PlayoutItemId" = ?
           ORDER BY "GraphicsElementId""#,
    )
    .bind(playout_item_id)
    .fetch_all(pool)
    .await
    .repository()?;

    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("GraphicsElementId").repository()?,
                row.try_get("Variables").repository()?,
            ))
        })
        .collect()
}

fn apply_deco_watermarks(deco: &PlaybackDeco, filler_kind: FillerKind, out: &mut Vec<i64>) -> bool {
    let allowed = filler_kind == FillerKind::None || deco.use_watermark_during_filler;
    match deco.mode {
        DecoMode::Merge if allowed => {
            out.extend(deco.watermark_ids.iter().copied());
            false
        }
        DecoMode::Merge | DecoMode::Override | DecoMode::Disable if !allowed => {
            out.clear();
            true
        }
        DecoMode::Merge | DecoMode::Inherit => false,
        DecoMode::Override => {
            out.extend(deco.watermark_ids.iter().copied());
            true
        }
        DecoMode::Disable => true,
    }
}

fn apply_deco_graphics(
    deco: &PlaybackGraphicsDeco,
    filler_kind: FillerKind,
    out: &mut Vec<(i64, Option<String>)>,
) -> bool {
    let allowed = filler_kind == FillerKind::None || deco.use_graphics_during_filler;
    match deco.mode {
        DecoMode::Merge if allowed => {
            out.extend(
                deco.graphics_element_ids
                    .iter()
                    .copied()
                    .map(|id| (id, None)),
            );
            false
        }
        DecoMode::Merge | DecoMode::Override | DecoMode::Disable if !allowed => {
            out.clear();
            true
        }
        DecoMode::Merge | DecoMode::Inherit => false,
        DecoMode::Override => {
            out.extend(
                deco.graphics_element_ids
                    .iter()
                    .copied()
                    .map(|id| (id, None)),
            );
            true
        }
        DecoMode::Disable => true,
    }
}

fn playout_template_matches(row: &crate::db::DbRow, now: DateTime<Utc>) -> bool {
    let days_of_week = row
        .try_get::<String, _>("DaysOfWeek")
        .ok()
        .and_then(|value| parse_integer_list(&value))
        .unwrap_or_default();
    let days_of_month = row
        .try_get::<String, _>("DaysOfMonth")
        .ok()
        .and_then(|value| parse_integer_list(&value))
        .unwrap_or_default();
    let months_of_year = row
        .try_get::<String, _>("MonthsOfYear")
        .ok()
        .and_then(|value| parse_integer_list(&value))
        .unwrap_or_default();

    if !matches_integer_filter(&days_of_week, csharp_day_of_week(now.weekday()))
        || !matches_integer_filter(&days_of_month, i32::try_from(now.day()).unwrap_or_default())
        || !matches_integer_filter(
            &months_of_year,
            i32::try_from(now.month()).unwrap_or_default(),
        )
    {
        return false;
    }

    let limit = row
        .try_get::<i64, _>("LimitToDateRange")
        .is_ok_and(|value| value != 0);
    if !limit {
        return true;
    }

    let date = now.date_naive();
    let Some(range_start) = NaiveDate::from_ymd_opt(
        row.try_get::<Option<i32>, _>("StartYear")
            .ok()
            .flatten()
            .unwrap_or_else(|| date.year()),
        u32::try_from(row.try_get::<i32, _>("StartMonth").unwrap_or(1)).unwrap_or_default(),
        u32::try_from(row.try_get::<i32, _>("StartDay").unwrap_or(1)).unwrap_or_default(),
    ) else {
        return false;
    };
    let Some(range_end) = NaiveDate::from_ymd_opt(
        row.try_get::<Option<i32>, _>("EndYear")
            .ok()
            .flatten()
            .unwrap_or_else(|| date.year()),
        u32::try_from(row.try_get::<i32, _>("EndMonth").unwrap_or(12)).unwrap_or_default(),
        u32::try_from(row.try_get::<i32, _>("EndDay").unwrap_or(31)).unwrap_or_default(),
    ) else {
        return false;
    };

    if range_start <= range_end {
        date >= range_start && date <= range_end
    } else {
        date >= range_start || date <= range_end
    }
}

fn parse_integer_list(value: &str) -> Option<Vec<i32>> {
    let values = serde_json::from_str::<Vec<i32>>(value).ok()?;
    (!values.is_empty()).then_some(values)
}

fn matches_integer_filter(values: &[i32], needle: i32) -> bool {
    values.is_empty() || values.contains(&needle)
}

fn csharp_day_of_week(weekday: chrono::Weekday) -> i32 {
    match weekday {
        chrono::Weekday::Mon => 1,
        chrono::Weekday::Tue => 2,
        chrono::Weekday::Wed => 3,
        chrono::Weekday::Thu => 4,
        chrono::Weekday::Fri => 5,
        chrono::Weekday::Sat => 6,
        chrono::Weekday::Sun => 0,
    }
}

fn parse_time_of_day_seconds(value: &str) -> i64 {
    let mut parts = value.split(':');
    let hours = parts
        .next()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let minutes = parts
        .next()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let seconds = parts
        .next()
        .and_then(|value| value.split('.').next())
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    hours
        .saturating_mul(3600)
        .saturating_add(minutes.saturating_mul(60))
        .saturating_add(seconds)
}

async fn load_global_watermark_id(pool: &DbPool) -> Result<Option<i64>> {
    let value: Option<String> = sqlx::query_scalar(
        r#"SELECT "Value"
           FROM "ConfigElement"
           WHERE "Key" = ?"#,
    )
    .bind(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID)
    .fetch_optional(pool)
    .await
    .repository()?;
    Ok(value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<i64>().ok()))
}

async fn load_channel_watermark(pool: &DbPool, id: i64) -> Result<ChannelWatermark> {
    let row = sqlx::query(
        r#"SELECT "Id","Name","Mode","ImageSource","Image","OriginalContentType",
                  "Location","Size","WidthPercent","HorizontalMarginPercent",
                  "VerticalMarginPercent","FrequencyMinutes","DurationSeconds",
                  "Opacity","PlaceWithinSourceContent","OpacityExpression","ZIndex"
           FROM "ChannelWatermark"
           WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .repository()?;
    map_channel_watermark(&row)
}

async fn resolve_watermark_image_path(
    pool: &DbPool,
    channel_id: i64,
    watermark: &ChannelWatermark,
) -> Result<Option<PlaybackWatermarkImage>> {
    match watermark.image_source {
        ChannelWatermarkImageSource::Custom => Ok(watermark
            .image
            .as_deref()
            .map(str::trim)
            .filter(|image| !image.is_empty())
            .map(|image| data_path(Path::new("watermarks").join(image)))
            .filter(|path| path.is_file())
            .map(PlaybackWatermarkImage::Local)),
        ChannelWatermarkImageSource::ChannelLogo => {
            let path: Option<String> = sqlx::query_scalar(
                r#"SELECT "Path"
                   FROM "Artwork"
                   WHERE "ChannelId" = ? AND "ArtworkKind" = 2
                   ORDER BY "Id"
                   LIMIT 1"#,
            )
            .bind(channel_id)
            .fetch_optional(pool)
            .await
            .repository()?;
            if let Some(path) = path {
                return Ok(resolve_channel_logo_image(&path));
            }
            let name: String = sqlx::query_scalar(r#"SELECT "Name" FROM "Channel" WHERE "Id"=?"#)
                .bind(channel_id)
                .fetch_one(pool)
                .await
                .repository()?;
            Ok(Some(PlaybackWatermarkImage::Local(
                crate::channel_logo::generated_image(&name).await?,
            )))
        }
        // "used for song progress overlay" — `WatermarkSelector.GetWatermarkOptions`
        // resolves the image inside the resources cache folder and ignores the
        // watermark with a warning when the resource no longer exists.
        ChannelWatermarkImageSource::Resource => Ok(resolve_resource_watermark_path(
            &resources_cache_folder(),
            watermark.image.as_deref(),
        )
        .map(PlaybackWatermarkImage::Local)),
    }
}

fn resolve_channel_logo_image(path: &str) -> Option<PlaybackWatermarkImage> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    if url::Url::parse(path)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
    {
        return Some(PlaybackWatermarkImage::Http(path.to_owned()));
    }
    let path = resolve_channel_logo_path(path);
    path.is_file()
        .then_some(PlaybackWatermarkImage::Local(path))
}

fn resolve_resource_watermark_path(
    resources_folder: &Path,
    image: Option<&str>,
) -> Option<PathBuf> {
    let image = image.map(str::trim).filter(|image| !image.is_empty())?;
    let path = resources_folder.join(image);
    if path.is_file() {
        Some(path)
    } else {
        tracing::warn!(
            path = %path.display(),
            "Watermark resource no longer exists and will be ignored"
        );
        None
    }
}

fn resolve_channel_logo_path(path: &str) -> PathBuf {
    let path = path.strip_prefix("iptv/logos/").unwrap_or(path);
    data_path(Path::new("iptv").join("logos").join(path))
}

fn map_channel_watermark(row: &crate::db::DbRow) -> Result<ChannelWatermark> {
    Ok(ChannelWatermark {
        id: row.try_get("Id").repository()?,
        name: decode_string_column(row, "Name")?,
        mode: decode_enum_column("channel watermark mode", row, "Mode")?,
        image_source: decode_enum_column("channel watermark image source", row, "ImageSource")?,
        image: decode_optional_string_column(row, "Image")?,
        original_content_type: decode_optional_string_column(row, "OriginalContentType")?,
        location: decode_enum_column("watermark location", row, "Location")?,
        size: decode_enum_column("watermark size", row, "Size")?,
        width_percent: row.try_get("WidthPercent").repository()?,
        horizontal_margin_percent: row.try_get("HorizontalMarginPercent").repository()?,
        vertical_margin_percent: row.try_get("VerticalMarginPercent").repository()?,
        frequency_minutes: row.try_get("FrequencyMinutes").repository()?,
        duration_seconds: row.try_get("DurationSeconds").repository()?,
        opacity: row.try_get("Opacity").repository()?,
        place_within_source_content: decode_bool_column(row, "PlaceWithinSourceContent")?,
        opacity_expression: decode_optional_string_column(row, "OpacityExpression")?,
        z_index: row.try_get("ZIndex").repository()?,
    })
}

#[must_use]
pub fn subtitle_fonts_dir() -> String {
    fonts_cache_folder().to_string_lossy().into_owned()
}

#[must_use]
pub fn subtitle_input_for_stream(
    subtitle: &PlaybackSubtitle,
    media_path: &str,
    output_format: OutputFormat,
    streaming_mode: StreamingMode,
) -> Option<InputFile> {
    let stream_index = usize::try_from(subtitle.stream_index).ok()?;
    if streaming_mode == StreamingMode::HttpLiveStreamingDirect {
        let method = match (
            output_format,
            subtitle.subtitle_kind,
            subtitle.codec.as_deref().unwrap_or_default(),
        ) {
            (OutputFormat::Mkv, subtitle_kind::EMBEDDED, _) => SubtitleMethod::Copy,
            (OutputFormat::Mp4, subtitle_kind::EMBEDDED, "dvdsub" | "dvd_subtitle" | "vobsub") => {
                SubtitleMethod::Copy
            }
            _ => return None,
        };
        let mut input = if subtitle.is_image() {
            InputFile::image_subtitle(media_path.to_owned(), method)
        } else {
            InputFile::text_subtitle(media_path.to_owned(), method)
        };
        if let Some(codec) = &subtitle.codec {
            input = input.with_subtitle_codec(codec);
        }
        return Some(input.with_stream_index(stream_index));
    }

    if subtitle.is_image() {
        let mut input = InputFile::image_subtitle(media_path.to_owned(), SubtitleMethod::Burn)
            .with_stream_index(stream_index);
        if let Some(codec) = &subtitle.codec {
            input = input.with_subtitle_codec(codec);
        }
        return Some(input);
    }

    let path = subtitle_text_path(subtitle, media_path)?;
    let mut input = InputFile::text_subtitle(path, SubtitleMethod::Burn);
    if let Some(codec) = &subtitle.codec {
        input = input.with_subtitle_codec(codec);
    }
    Some(input)
}

fn subtitle_text_path(subtitle: &PlaybackSubtitle, media_path: &str) -> Option<String> {
    let path = subtitle.path.as_deref()?.trim();
    if path.is_empty() {
        return None;
    }
    if subtitle.subtitle_kind == subtitle_kind::EMBEDDED && subtitle.is_extracted {
        return Some(
            subtitle_cache_folder()
                .join(path)
                .to_string_lossy()
                .into_owned(),
        );
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        return Some(path.to_owned());
    }

    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return Some(path.to_owned());
    }

    let parent = Path::new(media_path).parent()?;
    Some(parent.join(candidate).to_string_lossy().into_owned())
}

impl PlaybackSubtitle {
    #[must_use]
    pub fn is_image(&self) -> bool {
        self.codec.as_deref().is_some_and(|codec| {
            IMAGE_SUBTITLE_CODECS
                .iter()
                .any(|known| codec.eq_ignore_ascii_case(known))
        })
    }
}

const fn scan_kind_from_db(value: i64) -> Option<VideoScanKind> {
    match value {
        0 => Some(VideoScanKind::Unknown),
        1 => Some(VideoScanKind::Progressive),
        2 => Some(VideoScanKind::Interlaced),
        _ => None,
    }
}

fn frame_size_from_db(width: i32, height: i32) -> Option<FrameSize> {
    Some(FrameSize::new(
        u32::try_from(width).ok().filter(|value| *value > 0)?,
        u32::try_from(height).ok().filter(|value| *value > 0)?,
    ))
}

fn parse_frame_rate(value: &str) -> Option<FrameRate> {
    let trimmed = value.trim();
    let (numerator, denominator) = trimmed.split_once('/').map_or_else(
        || trimmed.parse::<u32>().ok().map(|num| (num, 1)),
        |(num, den)| Some((num.parse::<u32>().ok()?, den.parse::<u32>().ok()?)),
    )?;
    if numerator == 0 || denominator == 0 {
        return None;
    }
    Some(FrameRate::new(numerator, denominator))
}

/// Apply the persisted orientation and HDR/Dolby Vision metadata to a source
/// stream (ADR 0003 and ADR 0064).
///
/// Grouped so the loader stays readable: these six columns are all
/// "how should the picture be interpreted", as distinct from the codec and
/// geometry fields around them.
fn decode_source_display_metadata(
    row: &crate::db::DbRow,
    stream: &mut SourceVideoStream,
) -> Result<()> {
    stream.rotation_degrees = decode_playback_column(row, "Rotation")?;
    stream.mastering_display = decode_playback_column(row, "MasteringDisplay")?;
    stream.max_cll = decode_playback_column(row, "MaxCll")?;
    stream.max_fall = decode_playback_column(row, "MaxFall")?;
    let metadata_state = decode_playback_column::<Option<i32>>(row, "DolbyVisionMetadataState")
        .ok()
        .flatten()
        .map(ersatzrs_domain::DolbyVisionMetadataState::from_i32)
        .unwrap_or_default();
    let metadata_hint = decode_playback_column::<Option<String>>(row, "DolbyVisionMetadataHint")
        .ok()
        .flatten();
    apply_source_dolby_vision_metadata(
        stream,
        metadata_state,
        metadata_hint,
        decode_dolby_vision_configuration(row),
    );
    Ok(())
}

fn apply_source_dolby_vision_metadata(
    stream: &mut SourceVideoStream,
    metadata_state: ersatzrs_domain::DolbyVisionMetadataState,
    metadata_hint: Option<String>,
    configuration: Option<ersatzrs_ffmpeg::DolbyVisionConfiguration>,
) {
    if metadata_state == ersatzrs_domain::DolbyVisionMetadataState::Incomplete {
        stream.incomplete_dolby_vision = Some(ersatzrs_ffmpeg::IncompleteDolbyVisionMetadata {
            reason: metadata_hint.unwrap_or_else(|| {
                "the media provider omitted required Dolby Vision fields".to_owned()
            }),
        });
    } else if let Some(configuration) = configuration {
        stream.dolby_vision = Some(configuration);
    } else if metadata_state == ersatzrs_domain::DolbyVisionMetadataState::Complete {
        stream.incomplete_dolby_vision = Some(
            ersatzrs_ffmpeg::IncompleteDolbyVisionMetadata {
                reason: "the persisted Dolby Vision record is marked complete but required fields are missing"
                    .to_owned(),
            },
        );
    }
}

/// Rebuild the persisted Dolby Vision configuration record (ADR 0064).
///
/// Returns `None` when the stream is not Dolby Vision or any required
/// companion field is absent. The caller turns a positively detected partial
/// record into an explicit incomplete-metadata rejection.
fn decode_dolby_vision_configuration(
    row: &crate::db::DbRow,
) -> Option<ersatzrs_ffmpeg::DolbyVisionConfiguration> {
    let profile = decode_playback_column::<Option<i32>>(row, "DolbyVisionProfile")
        .ok()
        .flatten()
        .and_then(|profile| u8::try_from(profile).ok())?;

    let flag = |column| {
        decode_playback_column::<Option<bool>>(row, column)
            .ok()
            .flatten()
    };

    Some(ersatzrs_ffmpeg::DolbyVisionConfiguration {
        profile,
        bl_signal_compatibility_id: decode_playback_column::<Option<i32>>(
            row,
            "DolbyVisionBlCompatibilityId",
        )
        .ok()
        .flatten()
        .and_then(|value| u8::try_from(value).ok())?,
        rpu_present: flag("DolbyVisionRpuPresent")?,
        el_present: flag("DolbyVisionElPresent")?,
        bl_present: flag("DolbyVisionBlPresent")?,
    })
}

/// Derive HDR color metadata from ffprobe color tags, mirroring ErsatzTV's
/// HDR/BT.2020 detection (PQ/HLG transfer => HDR; BT.2020 primaries on a 10-bit
/// stream => the BT.2020 ten-bit colorspace path).
fn derive_color_params(
    pixel_format: Option<&str>,
    color_range: Option<&str>,
    color_transfer: Option<&str>,
    color_primaries: Option<&str>,
    color_space: Option<&str>,
) -> SourceColorParams {
    let transfer = color_transfer.unwrap_or_default().to_ascii_lowercase();
    let is_hdr = transfer.contains("smpte2084") || transfer.contains("arib-std-b67");

    let is_ten_bit = pixel_format
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("10");
    let is_bt2020 = color_primaries
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("bt2020")
        || color_space
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("bt2020");

    // Carry the raw ffprobe color tags through to the pipeline so the ported
    // `colorspace`/`colormatrix` normalization filter can classify the source
    // (bt470bg / mixed / unknown) exactly like ErsatzTV's `ColorParams`.
    SourceColorParams {
        color_range: color_range.and_then(non_empty_string),
        color_space: color_space.and_then(non_empty_string),
        color_transfer: color_transfer.and_then(non_empty_string),
        color_primaries: color_primaries.and_then(non_empty_string),
        is_hdr,
        is_bt2020_ten: is_bt2020 && is_ten_bit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graphics_startup_context() -> PlaybackGraphicsRenderContext {
        PlaybackGraphicsRenderContext {
            channel_id: 1,
            channel_number: "1".to_owned(),
            media_item_id: 1,
            media_path: "synthetic-media.mkv".to_owned(),
            channel_start_time: Utc::now(),
            content_start_time: Utc::now(),
            stream_seek_seconds: 0.0,
            duration_seconds: 60.0,
            frame_size: FrameSize::new(1920, 1080),
            square_pixel_frame_size: FrameSize::new(1920, 1080),
            frame_rate: None,
        }
    }

    #[tokio::test]
    async fn graphics_startup_cancellation_interrupts_held_database_pool() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let _held = pool.acquire().await.unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("not-created");
        let graphics = [ResolvedPlaybackGraphicsElement {
            element: GraphicsElement::default(),
            variables: None,
        }];
        let context = graphics_startup_context();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let render = render_playback_graphics_overlays_cancellable(
            &pool,
            &graphics,
            &context,
            &output,
            tokio::time::Instant::now() + Duration::from_secs(15),
            receiver,
        );
        tokio::pin!(render);
        tokio::select! {
            biased;
            _ = &mut render => panic!("graphics should wait for the held database connection"),
            () = tokio::task::yield_now() => {}
        }
        sender.send(()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), render)
            .await
            .unwrap();
        assert!(matches!(result, Err(RepositoryError::Decode {
            kind: "graphics startup", value,
        }) if value == "cancelled"));
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn graphics_startup_expired_deadline_does_not_read_database_or_create_artifacts() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("not-created");
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        let result = render_playback_graphics_overlays_cancellable(
            &pool,
            &[ResolvedPlaybackGraphicsElement {
                element: GraphicsElement::default(),
                variables: None,
            }],
            &graphics_startup_context(),
            &output,
            tokio::time::Instant::now(),
            receiver,
        )
        .await;
        assert!(matches!(result, Err(RepositoryError::Decode {
            kind: "graphics startup", value,
        }) if value == "deadline exceeded"));
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn graphics_startup_motion_probe_cancellation_never_uses_hold_fallback() {
        let extractor = FfprobeExtractor::new("nonexistent-graphics-probe");
        let (sender, cancellation) = tokio::sync::oneshot::channel();
        sender.send(()).unwrap();
        let mut startup = GraphicsStartupBudget {
            deadline: tokio::time::Instant::now() + Duration::from_secs(15),
            cancellation,
        };
        let result =
            graphics_motion_probe_cancellable(&extractor, "synthetic-motion.mkv", &mut startup)
                .await;
        assert!(matches!(result, Err(RepositoryError::Decode {
            kind: "graphics startup", value,
        }) if value == "cancelled"));
    }

    #[tokio::test]
    async fn watermark_startup_cancellation_never_uses_still_image_fallback() {
        let extractor = FfprobeExtractor::new("nonexistent-watermark-probe");
        let (sender, cancellation) = tokio::sync::oneshot::channel();
        sender.send(()).unwrap();
        let mut startup = GraphicsStartupBudget {
            deadline: tokio::time::Instant::now() + Duration::from_secs(15),
            cancellation,
        };
        assert!(matches!(
            probe_watermark_animation_cancellable(&extractor, "synthetic-logo.gif", &mut startup).await,
            Err(RepositoryError::Decode { kind: "graphics startup", value }) if value == "cancelled"
        ));
        let (_sender, cancellation) = tokio::sync::oneshot::channel();
        let mut startup = GraphicsStartupBudget {
            deadline: tokio::time::Instant::now(),
            cancellation,
        };
        assert!(matches!(
            probe_watermark_animation_cancellable(&extractor, "synthetic-logo.gif", &mut startup).await,
            Err(RepositoryError::Decode { kind: "graphics startup", value }) if value == "deadline exceeded"
        ));
    }

    // Fake executable permissions and PID liveness assertions are Unix-specific;
    // preflight/deadline and database cancellation tests above run on all targets.
    #[cfg(unix)]
    #[tokio::test]
    async fn graphics_startup_motion_probe_cancellation_awaits_child_cleanup() {
        assert_startup_probe_cancellation_awaits_child_cleanup(false).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn watermark_startup_probe_cancellation_awaits_child_cleanup() {
        assert_startup_probe_cancellation_awaits_child_cleanup(true).await;
    }

    #[cfg(unix)]
    async fn assert_startup_probe_cancellation_awaits_child_cleanup(watermark: bool) {
        use std::os::unix::fs::PermissionsExt as _;

        let temporary = tempfile::tempdir().unwrap();
        let program = temporary.path().join("motion-probe.sh");
        let pid_path = temporary.path().join("motion-probe.sh.pid");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s' $$ > \"$0.pid\"\nwhile :; do :; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let extractor = FfprobeExtractor::new(program.to_str().unwrap());
        let (sender, cancellation) = tokio::sync::oneshot::channel();
        let mut startup = GraphicsStartupBudget {
            deadline: tokio::time::Instant::now() + Duration::from_secs(15),
            cancellation,
        };
        let probe = async {
            if watermark {
                probe_watermark_animation_cancellable(
                    &extractor,
                    "synthetic-logo.gif",
                    &mut startup,
                )
                .await
                .map(|_| ())
            } else {
                graphics_motion_probe_cancellable(&extractor, "synthetic-motion.mkv", &mut startup)
                    .await
                    .map(|_| ())
            }
        };
        tokio::pin!(probe);
        let wait_started = async {
            while !pid_path.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        };
        tokio::select! {
            result = &mut probe => panic!("motion probe ended before cancellation: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(2), wait_started) => result.unwrap(),
        }
        sender.send(()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), probe)
            .await
            .unwrap();
        assert!(matches!(result, Err(RepositoryError::Decode {
            kind: "graphics startup", value,
        }) if value == "cancelled"));
        let pid: i32 = std::fs::read_to_string(pid_path).unwrap().parse().unwrap();
        assert_eq!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH),
        );
    }

    fn fresh_playback_probe() -> FfprobeMediaVersion {
        crate::ffprobe::parse_ffprobe_json(
            Path::new("synthetic-media"),
            br#"{
          "format":{"duration":"120"},
          "streams":[
            {"index":7,"codec_type":"audio","codec_name":"aac","channels":6,
             "disposition":{"default":1},"tags":{"language":"jpn","title":"Main audio"}},
            {"index":3,"codec_type":"video","codec_name":"hevc","profile":"Main 10",
             "width":1920,"height":1080,"pix_fmt":"yuv420p10le",
             "sample_aspect_ratio":"4:3","display_aspect_ratio":"64:27",
             "r_frame_rate":"24000/1001","field_order":"progressive",
             "color_transfer":"smpte2084","color_primaries":"bt2020",
             "color_space":"bt2020nc","color_range":"tv"}
          ]
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn fresh_playback_metadata_preserves_geometry_color_and_audio_selection() {
        let mut probe = fresh_playback_probe();
        let video = probe
            .streams
            .iter_mut()
            .find(|stream| stream.index == 3)
            .unwrap();
        video.rotation = Some(90);
        video.mastering_display = Some("synthetic mastering display".into());
        video.max_cll = Some(1000);
        video.max_fall = Some(400);
        let source = source_video_stream_from_probe(&probe).unwrap().unwrap();
        assert_eq!(source.index, 3);
        assert_eq!(source.codec, SourceVideoCodec::Hevc);
        assert_eq!(source.profile.as_deref(), Some("main 10"));
        assert_eq!(source.frame_size, Some(FrameSize::new(1920, 1080)));
        assert_eq!(source.frame_rate, Some(FrameRate::new(24000, 1001)));
        assert_eq!(source.sample_aspect_ratio.as_deref(), Some("4:3"));
        assert_eq!(source.display_aspect_ratio.as_deref(), Some("64:27"));
        assert_eq!(source.rotation_degrees, Some(90));
        assert_eq!(
            source.mastering_display.as_deref(),
            Some("synthetic mastering display")
        );
        assert_eq!(source.max_cll, Some(1000));
        assert_eq!(source.max_fall, Some(400));
        assert!(source.color_params.is_hdr);
        assert!(source.color_params.is_bt2020_ten);
        let audio = source_audio_streams_from_probe(&probe).unwrap();
        assert_eq!(audio.len(), 1);
        assert_eq!(audio[0].index, 7);
        assert_eq!(audio[0].channels, Some(6));
        assert_eq!(audio[0].language.as_deref(), Some("jpn"));
        assert_eq!(audio[0].title.as_deref(), Some("Main audio"));
        assert!(audio[0].default);
        assert_eq!(probe.version.duration, chrono::Duration::seconds(120));
    }

    #[test]
    fn fresh_playback_metadata_preserves_complete_and_incomplete_dolby_vision() {
        let mut probe = fresh_playback_probe();
        let video = probe
            .streams
            .iter_mut()
            .find(|stream| stream.index == 3)
            .unwrap();
        video.dolby_vision_metadata_state = ersatzrs_domain::DolbyVisionMetadataState::Complete;
        video.dolby_vision_profile = Some(8);
        video.dolby_vision_bl_compatibility_id = Some(1);
        video.dolby_vision_bl_present = Some(true);
        video.dolby_vision_el_present = Some(false);
        video.dolby_vision_rpu_present = Some(true);
        assert_eq!(
            source_video_stream_from_probe(&probe)
                .unwrap()
                .unwrap()
                .dolby_vision
                .unwrap()
                .profile,
            8
        );
        let video = probe
            .streams
            .iter_mut()
            .find(|stream| stream.index == 3)
            .unwrap();
        video.dolby_vision_bl_present = None;
        let source = source_video_stream_from_probe(&probe).unwrap().unwrap();
        assert!(source.dolby_vision.is_none());
        assert!(source.incomplete_dolby_vision.is_some());
        let video = probe
            .streams
            .iter_mut()
            .find(|stream| stream.index == 3)
            .unwrap();
        video.dolby_vision_metadata_state = ersatzrs_domain::DolbyVisionMetadataState::Incomplete;
        video.dolby_vision_metadata_hint = Some("missing base layer flag".into());
        assert_eq!(
            source_video_stream_from_probe(&probe)
                .unwrap()
                .unwrap()
                .incomplete_dolby_vision
                .unwrap()
                .reason,
            "missing base layer flag"
        );
    }

    #[test]
    fn fresh_playback_metadata_rejects_ambiguous_or_invalid_streams() {
        let mut probe = fresh_playback_probe();
        let video = probe
            .streams
            .iter()
            .find(|stream| stream.index == 3)
            .unwrap()
            .clone();
        probe.streams.push(video);
        assert!(source_video_stream_from_probe(&probe).is_err());
        probe
            .streams
            .retain(|stream| stream.kind == ersatzrs_domain::MediaStreamKind::Audio);
        assert!(source_video_stream_from_probe(&probe).unwrap().is_none());
        probe.streams[0].index = 0;
        assert_eq!(source_audio_streams_from_probe(&probe).unwrap()[0].index, 0);
        probe.streams[0].channels = Some(-1);
        assert!(source_audio_streams_from_probe(&probe).is_err());
        probe.streams[0].channels = None;
        probe.streams.push(probe.streams[0].clone());
        assert!(source_audio_streams_from_probe(&probe).is_err());
        probe.streams.pop();
        probe.streams[0].index = -1;
        assert!(source_audio_streams_from_probe(&probe).is_err());
    }
    use chrono::TimeZone;

    async fn source_stream_test_pool() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open playback metadata test database");
        for statement in [
            r#"CREATE TABLE "MediaVersion" (
                "Id" INTEGER PRIMARY KEY,
                "VideoScanKind" INTEGER NOT NULL,
                "Width" INTEGER NOT NULL,
                "Height" INTEGER NOT NULL,
                "RFrameRate" TEXT NULL,
                "SampleAspectRatio" TEXT NULL,
                "DisplayAspectRatio" TEXT NULL,
                "MovieId" INTEGER NULL,
                "EpisodeId" INTEGER NULL,
                "MusicVideoId" INTEGER NULL,
                "OtherVideoId" INTEGER NULL,
                "RemoteStreamId" INTEGER NULL,
                "SongId" INTEGER NULL,
                "ImageId" INTEGER NULL
            )"#,
            r#"CREATE TABLE "MediaStream" (
                "Id" INTEGER PRIMARY KEY,
                "Index" INTEGER NOT NULL,
                "Codec" TEXT NULL,
                "PixelFormat" TEXT NULL,
                "ColorRange" TEXT NULL,
                "ColorTransfer" TEXT NULL,
                "ColorPrimaries" TEXT NULL,
                "ColorSpace" TEXT NULL,
                "Rotation" INTEGER NULL,
                "MasteringDisplay" TEXT NULL,
                "MaxCll" INTEGER NULL,
                "MaxFall" INTEGER NULL,
                "DolbyVisionProfile" INTEGER NULL,
                "DolbyVisionBlCompatibilityId" INTEGER NULL,
                "DolbyVisionBlPresent" INTEGER NULL,
                "DolbyVisionElPresent" INTEGER NULL,
                "DolbyVisionRpuPresent" INTEGER NULL,
                "DolbyVisionMetadataState" INTEGER NOT NULL DEFAULT 0,
                "DolbyVisionMetadataHint" TEXT NULL,
                "MediaVersionId" INTEGER NOT NULL,
                "MediaStreamKind" INTEGER NOT NULL,
                "Channels" INTEGER NOT NULL,
                "Default" INTEGER NOT NULL,
                "Language" TEXT NULL,
                "Title" TEXT NULL
            )"#,
            r#"CREATE TABLE "Playout" (
                "Id" INTEGER PRIMARY KEY,
                "ChannelId" INTEGER NOT NULL
            )"#,
            r#"CREATE TABLE "PlayoutItem" (
                "Id" INTEGER PRIMARY KEY,
                "PlayoutId" INTEGER NOT NULL,
                "MediaItemId" INTEGER NOT NULL
            )"#,
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("create playback metadata test table");
        }
        pool
    }

    #[tokio::test]
    async fn source_video_loader_distinguishes_absence_decode_and_database_failure() {
        let pool = source_stream_test_pool().await;
        assert_eq!(
            load_source_video_stream(&pool, 10)
                .await
                .expect("load absent source video"),
            None
        );
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                "Id", "VideoScanKind", "Width", "Height", "RFrameRate"
            ) VALUES (10, 1, 1920, 1080, '30000/1001')"#,
        )
        .execute(&pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                "Id", "Index", "Codec", "MediaVersionId", "MediaStreamKind",
                "Channels", "Default"
            ) VALUES (1, -1, 'h264', 10, 1, 0, 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed malformed video stream");
        assert!(matches!(
            load_source_video_stream(&pool, 10).await,
            Err(RepositoryError::Decode { .. })
        ));

        pool.close().await;
        assert!(matches!(
            load_source_video_stream(&pool, 10).await,
            Err(RepositoryError::Unavailable { .. })
        ));
    }

    #[tokio::test]
    async fn source_video_loader_preserves_null_codec_as_absent_metadata() {
        let pool = source_stream_test_pool().await;
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                "Id", "VideoScanKind", "Width", "Height", "RFrameRate"
            ) VALUES (12, 1, 1920, 1080, '30000/1001')"#,
        )
        .execute(&pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                "Id", "Index", "Codec", "MediaVersionId", "MediaStreamKind",
                "Channels", "Default"
            ) VALUES (12, 0, NULL, 12, 1, 0, 0)"#,
        )
        .execute(&pool)
        .await
        .expect("seed video stream without codec metadata");

        assert_eq!(
            load_source_video_stream(&pool, 12)
                .await
                .expect("load source video without codec metadata"),
            None
        );
        assert!(matches!(
            load_source_video_stream_by_index(&pool, 12, Some(0)).await,
            Err(RepositoryError::Decode { .. })
        ));
        assert!(matches!(
            load_source_video_stream_by_index(&pool, 12, Some(9)).await,
            Err(RepositoryError::Decode { .. })
        ));
    }

    #[tokio::test]
    async fn source_video_loader_requires_the_persisted_dolby_vision_base_layer_flag() {
        let pool = source_stream_test_pool().await;
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                "Id", "VideoScanKind", "Width", "Height", "RFrameRate"
            ) VALUES (13, 1, 3840, 2160, '24000/1001')"#,
        )
        .execute(&pool)
        .await
        .expect("seed media version");
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                "Id", "Index", "Codec", "MediaVersionId", "MediaStreamKind",
                "Channels", "Default", "DolbyVisionProfile",
                "DolbyVisionBlCompatibilityId", "DolbyVisionBlPresent",
                "DolbyVisionElPresent", "DolbyVisionRpuPresent",
                "DolbyVisionMetadataState"
            ) VALUES (13, 0, 'hevc', 13, 1, 0, 0, 7, 6, 1, 1, 1, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("seed profile 7 stream");

        let source = load_source_video_stream(&pool, 13)
            .await
            .expect("load profile 7 source")
            .expect("source video");
        let configuration = source.dolby_vision.expect("Dolby Vision record");
        assert!(configuration.bl_present);
        assert!(configuration.el_present);
        assert!(configuration.rpu_present);
        assert!(configuration.is_playable());

        sqlx::query(r#"UPDATE "MediaStream" SET "DolbyVisionBlPresent" = NULL WHERE "Id" = 13"#)
            .execute(&pool)
            .await
            .expect("clear profile 7 base-layer evidence");
        let source = load_source_video_stream(&pool, 13)
            .await
            .expect("load incomplete profile 7 source")
            .expect("source video");
        assert!(source.dolby_vision.is_none());
        let incomplete = source
            .incomplete_dolby_vision
            .expect("missing base-layer flag must be a visible rejection");
        assert!(incomplete.reason.contains("required fields are missing"));
    }

    #[tokio::test]
    async fn source_audio_loader_rejects_malformed_rows_instead_of_dropping_them() {
        let pool = source_stream_test_pool().await;
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                "Id", "Index", "Codec", "MediaVersionId", "MediaStreamKind",
                "Channels", "Default", "Language", "Title"
            ) VALUES (2, 1, 'aac', 10, 2, 2, 2, 'eng', 'Main')"#,
        )
        .execute(&pool)
        .await
        .expect("seed malformed audio stream");

        assert!(matches!(
            load_source_audio_streams(&pool, 10).await,
            Err(RepositoryError::Decode { .. })
        ));
    }

    #[tokio::test]
    async fn target_frame_rate_loader_distinguishes_absence_decode_and_database_failure() {
        let pool = source_stream_test_pool().await;
        assert_eq!(
            load_channel_target_frame_rate(&pool, 5)
                .await
                .expect("load absent target frame rate"),
            None
        );
        sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId") VALUES (1, 5)"#)
            .execute(&pool)
            .await
            .expect("seed playout");
        sqlx::query(
            r#"INSERT INTO "PlayoutItem" ("Id", "PlayoutId", "MediaItemId")
               VALUES (1, 1, 77)"#,
        )
        .execute(&pool)
        .await
        .expect("seed playout item");
        sqlx::query(
            r#"INSERT INTO "MediaVersion" (
                "Id", "VideoScanKind", "Width", "Height", "RFrameRate", "MovieId"
            ) VALUES (11, 1, 1920, 1080, 'invalid', 77)"#,
        )
        .execute(&pool)
        .await
        .expect("seed malformed frame rate");
        assert!(matches!(
            load_channel_target_frame_rate(&pool, 5).await,
            Err(RepositoryError::Decode { .. })
        ));

        pool.close().await;
        assert!(matches!(
            load_channel_target_frame_rate(&pool, 5).await,
            Err(RepositoryError::Unavailable { .. })
        ));
    }

    #[test]
    fn qsv_h264_profile_count_probe_uses_ersatztv_trace_headers_args() {
        assert_eq!(
            trace_headers_profile_count_args("/media/movie.mkv"),
            vec![
                "-hide_banner",
                "-t",
                "30",
                "-i",
                "/media/movie.mkv",
                "-c",
                "copy",
                "-bsf:v",
                "trace_headers",
                "-f",
                "null",
                "-",
            ]
        );
    }

    #[test]
    fn qsv_h264_profile_count_probe_counts_distinct_profile_idc_lines() {
        let mut profiles = HashSet::new();

        collect_trace_headers_profile_idc(
            "[trace_headers @ 0x1] 42          profile_idc 00111100 = 60",
            &mut profiles,
        );
        collect_trace_headers_profile_idc(
            "[trace_headers @ 0x1] 42          profile_idc 00111100 = 60",
            &mut profiles,
        );
        collect_trace_headers_profile_idc(
            "[trace_headers @ 0x1] 42          profile_idc 01100100 = 100",
            &mut profiles,
        );

        assert_eq!(profiles.len(), 2);
    }

    #[test]
    fn qsv_h264_profile_count_probe_is_limited_to_qsv_h264() {
        let h264 = SourceVideoStream::new(SourceVideoCodec::H264);
        let hevc = SourceVideoStream::new(SourceVideoCodec::Hevc);

        assert!(should_probe_h264_profile_count(
            &h264,
            HardwareAccelerationKind::Qsv
        ));
        assert!(!should_probe_h264_profile_count(
            &h264,
            HardwareAccelerationKind::Nvenc
        ));
        assert!(!should_probe_h264_profile_count(
            &hevc,
            HardwareAccelerationKind::Qsv
        ));
    }

    #[test]
    fn custom_stream_selector_prioritizes_audio_language_and_conditions() {
        let item = StreamSelectorItem {
            audio_languages: vec!["jpn".to_owned(), "eng".to_owned()],
            audio_title_blocklist: vec!["commentary".to_owned()],
            audio_condition: "channels >= 6 and default = false".to_owned(),
            ..StreamSelectorItem::default()
        };
        let streams = vec![
            PlaybackAudioStream {
                stream_index: 1,
                title: Some("English commentary".to_owned()),
                language: Some("eng".to_owned()),
                default: false,
                forced: false,
                codec: Some("aac".to_owned()),
                channels: Some(6),
            },
            PlaybackAudioStream {
                stream_index: 2,
                title: Some("English 5.1".to_owned()),
                language: Some("eng".to_owned()),
                default: false,
                forced: false,
                codec: Some("aac".to_owned()),
                channels: Some(6),
            },
            PlaybackAudioStream {
                stream_index: 3,
                title: Some("Japanese stereo".to_owned()),
                language: Some("jpn".to_owned()),
                default: false,
                forced: false,
                codec: Some("aac".to_owned()),
                channels: Some(2),
            },
        ];

        let selected = select_custom_audio(&streams, &item, None);

        assert!(selected.passed);
        assert_eq!(selected.selected.map(|stream| stream.stream_index), Some(2));
    }

    #[test]
    fn retained_audio_pin_maps_identity_not_codec_or_transport_index() {
        let mut original = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        original.language = Some("eng".into());
        original.title = Some("Main audio".into());
        let mut commentary = original.clone().with_index(2);
        commentary.title = Some("Commentary".into());
        let previous = [original.clone(), commentary];
        let mut fresh =
            SourceAudioStream::new(SourceAudioCodec::from_ffmpeg_name("opus")).with_index(0);
        fresh.language = Some("en".into());
        fresh.title = Some("main audio".into());
        let mut wrong = fresh.clone().with_index(1);
        wrong.title = Some("Commentary".into());
        assert_eq!(
            remap_pinned_audio_selection(
                PlaybackAudioSelection::Exact(1),
                &previous,
                &[wrong, fresh.clone()]
            )
            .unwrap(),
            PlaybackAudioSelection::Exact(0)
        );
        assert!(remap_pinned_audio_selection(
            PlaybackAudioSelection::Exact(99),
            &previous,
            std::slice::from_ref(&fresh)
        )
        .is_err());
        assert!(remap_pinned_audio_selection(
            PlaybackAudioSelection::Exact(1),
            &previous,
            &[fresh.clone(), fresh.with_index(3)]
        )
        .is_err());
    }

    #[test]
    fn retained_audio_pin_does_not_guess_missing_or_ambiguous_identity() {
        let mut original = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        original.language = Some("jpn".into());
        let mut fresh = original.clone().with_index(0);
        fresh.language = Some("eng".into());
        assert!(remap_pinned_audio_selection(
            PlaybackAudioSelection::Exact(1),
            std::slice::from_ref(&original),
            std::slice::from_ref(&fresh)
        )
        .is_err());
        fresh.language = None;
        assert!(remap_pinned_audio_selection(
            PlaybackAudioSelection::Exact(1),
            std::slice::from_ref(&original),
            &[fresh]
        )
        .is_err());
        assert!(remap_pinned_audio_selection(
            PlaybackAudioSelection::Exact(1),
            &[original.clone(), original.clone().with_index(2)],
            &[original.with_index(0)]
        )
        .is_err());
    }

    #[test]
    fn retained_audio_pin_preserves_silence_and_unlabelled_single_audio() {
        for selection in [
            PlaybackAudioSelection::Disabled,
            PlaybackAudioSelection::Automatic,
        ] {
            assert_eq!(
                remap_pinned_audio_selection(selection, &[], &[]).unwrap(),
                selection
            );
        }
        let original = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        let fresh =
            SourceAudioStream::new(SourceAudioCodec::from_ffmpeg_name("opus")).with_index(0);
        assert_eq!(
            remap_pinned_audio_selection(
                PlaybackAudioSelection::Exact(1),
                std::slice::from_ref(&original),
                std::slice::from_ref(&fresh)
            )
            .unwrap(),
            PlaybackAudioSelection::Exact(0)
        );
        assert!(remap_pinned_audio_selection(
            PlaybackAudioSelection::Exact(1),
            &[original],
            &[fresh.clone(), fresh.with_index(2)]
        )
        .is_err());
    }

    #[test]
    fn playback_audio_selector_prefers_title_within_language_candidates() {
        let mut default_eng = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        default_eng.language = Some("eng".to_owned());
        default_eng.title = Some("English 5.1".to_owned());
        default_eng.default = true;
        default_eng.channels = Some(6);
        let mut commentary = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3);
        commentary.language = Some("eng".to_owned());
        commentary.title = Some("Director Commentary".to_owned());
        commentary.channels = Some(2);
        let mut japanese = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(4);
        japanese.language = Some("jpn".to_owned());
        japanese.title = Some("Japanese Commentary".to_owned());

        let selected = select_playback_audio_stream_index(
            &[default_eng, commentary, japanese],
            Some("en"),
            Some("commentary"),
        );

        assert_eq!(selected, Some(3));
    }

    #[test]
    fn playback_audio_selector_falls_back_when_title_is_missing() {
        let mut default_eng = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        default_eng.language = Some("eng".to_owned());
        default_eng.title = Some("English 5.1".to_owned());
        default_eng.default = true;
        default_eng.channels = Some(6);
        let mut stereo_eng = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(2);
        stereo_eng.language = Some("eng".to_owned());
        stereo_eng.title = Some("English stereo".to_owned());

        let selected = select_playback_audio_stream_index(
            &[stereo_eng, default_eng],
            Some("eng"),
            Some("commentary"),
        );

        assert_eq!(selected, Some(1));
    }

    #[test]
    fn playback_audio_selector_treats_missing_language_as_und() {
        let mut literal_und = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        literal_und.language = Some("und".to_owned());
        literal_und.channels = Some(2);
        let mut missing_language = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(2);
        missing_language.default = true;
        missing_language.channels = Some(6);
        let mut english = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3);
        english.language = Some("eng".to_owned());
        english.default = true;
        english.channels = Some(8);

        let selected = select_playback_audio_stream_index(
            &[literal_und, missing_language, english],
            Some("und"),
            None,
        );

        assert_eq!(selected, Some(2));
    }

    #[test]
    fn playback_audio_selector_und_falls_back_to_overall_default() {
        let mut english = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        english.language = Some("eng".to_owned());
        english.default = true;
        let mut japanese = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(2);
        japanese.language = Some("jpn".to_owned());

        let selected = select_playback_audio_stream_index(&[japanese, english], Some("und"), None);

        assert_eq!(selected, Some(1));
    }

    #[test]
    fn playback_audio_selector_leaves_unconfigured_channels_on_pipeline_default() {
        let mut default_eng = SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1);
        default_eng.language = Some("eng".to_owned());

        let selected = select_playback_audio_stream_index(&[default_eng], None, None);

        assert_eq!(selected, None);
    }

    #[test]
    fn custom_stream_selector_matches_commentary_allowlists() {
        let item = StreamSelectorItem {
            audio_title_allowlist: vec!["commentary".to_owned()],
            subtitle_title_allowlist: vec!["commentary".to_owned()],
            ..StreamSelectorItem::default()
        };
        let streams = vec![
            PlaybackAudioStream {
                stream_index: 1,
                title: Some("English 5.1".to_owned()),
                language: Some("eng".to_owned()),
                default: true,
                forced: false,
                codec: Some("aac".to_owned()),
                channels: Some(6),
            },
            PlaybackAudioStream {
                stream_index: 2,
                title: Some("Director Commentary".to_owned()),
                language: Some("eng".to_owned()),
                default: false,
                forced: false,
                codec: Some("aac".to_owned()),
                channels: Some(2),
            },
        ];
        let subtitles = [PlaybackSubtitle {
            id: 1,
            subtitle_kind: subtitle_kind::SIDECAR,
            title: Some("Commentary subtitles".to_owned()),
            stream_index: 4,
            codec: Some("srt".to_owned()),
            default: false,
            forced: false,
            sdh: false,
            language: Some("eng".to_owned()),
            is_extracted: false,
            path: Some("commentary.srt".to_owned()),
        }];

        let audio = select_custom_audio(&streams, &item, None);
        let subtitle = select_custom_subtitle(&subtitles, &item, None);

        assert!(audio.passed);
        assert_eq!(audio.selected.map(|stream| stream.stream_index), Some(2));
        assert!(subtitle.passed);
        assert_eq!(
            subtitle.selected.map(|subtitle| subtitle.stream_index),
            Some(4)
        );
    }

    #[test]
    fn custom_stream_selector_matches_content_and_can_disable_subtitles() {
        let channel = Channel {
            number: "42".to_owned(),
            name: "Movies".to_owned(),
            ..Channel::default()
        };
        let start = Utc.with_ymd_and_hms(2026, 6, 1, 20, 0, 0).unwrap();
        let item = StreamSelectorItem {
            disable_subtitles: true,
            content_condition:
                "channel_number = '42' and channel_name = 'movies' and time_of_day_seconds >= 72000"
                    .to_owned(),
            ..StreamSelectorItem::default()
        };

        let subtitles = [PlaybackSubtitle {
            id: 1,
            subtitle_kind: subtitle_kind::SIDECAR,
            title: Some("English".to_owned()),
            stream_index: 4,
            codec: Some("srt".to_owned()),
            default: false,
            forced: false,
            sdh: false,
            language: Some("eng".to_owned()),
            is_extracted: false,
            path: Some("movie.srt".to_owned()),
        }];
        let selected = select_custom_subtitle(&subtitles, &item, None);

        assert!(content_matches_condition(
            &channel,
            start,
            &item.content_condition
        ));
        assert!(selected.passed);
        assert!(selected.selected.is_none());
    }

    #[test]
    fn simple_expression_matches_ersatztv_wildcards_case_insensitively() {
        assert!(simple_expression_matches("en*", "ENG"));
        assert!(simple_expression_matches("p?r", "por"));
        assert!(!simple_expression_matches("jpn", "eng"));
    }

    #[test]
    fn channel_logo_urls_are_preserved_and_local_logos_still_require_files() {
        for uri in [
            "http://example.test/logo.png",
            "https://example.test/logo%20image.gif?revision=2",
            "HTTPS://example.test/logo.png",
        ] {
            let image = resolve_channel_logo_image(uri).unwrap();
            assert_eq!(image, PlaybackWatermarkImage::Http(uri.to_owned()));
            assert_eq!(image.input_path(), uri);
            let input = watermark_input_with_animation(
                &image.input_path(),
                &WatermarkOverlay::default(),
                false,
            );
            assert_eq!(input.path, uri);
        }
        let root = tempfile::tempdir().unwrap();
        let logo = root.path().join("logo.png");
        assert!(resolve_channel_logo_image(logo.to_str().unwrap()).is_none());
        std::fs::write(&logo, b"fixture").unwrap();
        assert_eq!(
            resolve_channel_logo_image(logo.to_str().unwrap()),
            Some(PlaybackWatermarkImage::Local(logo))
        );
        assert!(resolve_channel_logo_image("  ").is_none());
    }

    #[test]
    fn resource_watermark_resolves_inside_resources_cache_folder() {
        let root = tempfile::tempdir().expect("temp resources root");
        let path = root.path().join("song_progress_overlay.png");
        std::fs::write(&path, b"png").expect("write resource");

        let resolved =
            resolve_resource_watermark_path(root.path(), Some("song_progress_overlay.png"));
        assert_eq!(resolved, Some(path));
    }

    #[test]
    fn missing_or_empty_resource_watermark_is_ignored() {
        let root = tempfile::tempdir().expect("temp resources root");

        assert_eq!(
            resolve_resource_watermark_path(root.path(), Some("song_progress_overlay.png")),
            None
        );
        assert_eq!(
            resolve_resource_watermark_path(root.path(), Some("  ")),
            None
        );
        assert_eq!(resolve_resource_watermark_path(root.path(), None), None);
    }

    #[test]
    fn subtitle_input_preserves_extensionless_external_ass_codec() {
        let subtitle = PlaybackSubtitle {
            id: 1,
            subtitle_kind: subtitle_kind::SIDECAR,
            title: None,
            stream_index: 0,
            codec: Some("ass".to_owned()),
            default: false,
            forced: false,
            sdh: false,
            language: Some("eng".to_owned()),
            is_extracted: false,
            path: Some("https://example.test/library/streams/123?X-Plex-Token=redacted".to_owned()),
        };

        let input = subtitle_input_for_stream(
            &subtitle,
            "/media/video.mkv",
            OutputFormat::MpegTs,
            StreamingMode::HttpLiveStreamingSegmenter,
        )
        .expect("external ASS subtitle should map to an input");

        assert_eq!(
            input.subtitle_text_format,
            Some(ersatzrs_ffmpeg::SubtitleTextFormat::Ass)
        );
    }

    #[tokio::test]
    async fn opacity_expression_still_watermark_uses_looped_input() {
        let input = watermark_input_for_stream(
            "/media/logo.png",
            &WatermarkOverlay {
                opacity_expression: Some("content_seconds < 5".to_owned()),
                ..WatermarkOverlay::default()
            },
        )
        .await;

        assert_eq!(input.input_options, vec!["-loop", "1"]);
    }

    #[tokio::test]
    async fn plain_still_watermark_keeps_non_looped_input() {
        let input =
            watermark_input_for_stream("/media/logo.png", &WatermarkOverlay::default()).await;

        assert!(input.input_options.is_empty());
    }
}
