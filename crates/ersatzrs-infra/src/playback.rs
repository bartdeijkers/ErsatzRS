//! Shared helpers for building `FFmpeg` playback pipelines from persisted media
//! metadata, used by both the MPEG-TS and HLS transcode paths.

use std::path::{Path, PathBuf};

use ersatzrs_core::language_codes;
use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{
    ChannelSubtitleMode, FFmpegProfile, Resolution, StreamingMode, VideoScanKind,
};
use ersatzrs_ffmpeg::{
    FrameRate, FrameSize, InputFile, OutputFormat, PixelFormat, SourceColorParams,
    SourceVideoCodec, SourceVideoStream, SubtitleMethod,
};
use sqlx::Row;

use crate::db::DbPool;
use crate::ffmpeg_config::use_embedded_subtitles;
use crate::file_system_layout::{fonts_cache_folder, subtitle_cache_folder};
use crate::metadata::subtitle_kind;
use crate::repositories::common::{
    decode_bool_column, decode_enum_column, decode_optional_bool_column,
    decode_optional_string_column, decode_string_column,
};

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

#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackProfile {
    pub profile: FFmpegProfile,
    pub resolution: Resolution,
}

/// Load the source video-stream metadata the `FFmpeg` pipeline needs to make
/// ErsatzTV-parity decisions (decoder selection, deinterlace, tonemap, and the
/// 10-bit → 8-bit pixel-format conversion handled by `SetPixelFormat`).
///
/// Returns `None` when the media version has no recorded video stream, in
/// which case the pipeline falls back to its format-agnostic defaults.
pub async fn load_source_video_stream(
    pool: &DbPool,
    media_version_id: i64,
) -> Option<SourceVideoStream> {
    let row = sqlx::query(
        r#"SELECT ms."Codec" AS "Codec",
                  ms."PixelFormat" AS "PixelFormat",
                  ms."ColorTransfer" AS "ColorTransfer",
                  ms."ColorPrimaries" AS "ColorPrimaries",
                  ms."ColorSpace" AS "ColorSpace",
                  mv."VideoScanKind" AS "VideoScanKind",
                  mv."Width" AS "Width",
                  mv."Height" AS "Height",
                  mv."RFrameRate" AS "RFrameRate",
                  mv."SampleAspectRatio" AS "SampleAspectRatio",
                  mv."DisplayAspectRatio" AS "DisplayAspectRatio"
           FROM "MediaVersion" mv
           JOIN "MediaStream" ms
               ON ms."MediaVersionId" = mv."Id" AND ms."MediaStreamKind" = 1
           WHERE mv."Id" = ?
           ORDER BY ms."Index"
           LIMIT 1"#,
    )
    .bind(media_version_id)
    .fetch_optional(pool)
    .await
    .ok()??;

    let codec: String = row.try_get("Codec").ok()?;
    let pixel_format: Option<String> = row.try_get("PixelFormat").ok().flatten();
    let color_transfer: Option<String> = row.try_get("ColorTransfer").ok().flatten();
    let color_primaries: Option<String> = row.try_get("ColorPrimaries").ok().flatten();
    let color_space: Option<String> = row.try_get("ColorSpace").ok().flatten();
    let width: Option<i32> = row.try_get("Width").ok().flatten();
    let height: Option<i32> = row.try_get("Height").ok().flatten();
    let frame_rate: Option<String> = row.try_get("RFrameRate").ok().flatten();
    let scan_kind = row
        .try_get::<i64, _>("VideoScanKind")
        .ok()
        .map_or(VideoScanKind::Unknown, scan_kind_from_db);

    let mut stream = SourceVideoStream::new(SourceVideoCodec::from_ffmpeg_name(&codec));
    stream.pixel_format = pixel_format
        .as_deref()
        .and_then(PixelFormat::from_ffmpeg_name);
    stream.scan_kind = scan_kind;
    stream.frame_size = width
        .zip(height)
        .and_then(|(width, height)| frame_size_from_db(width, height));
    stream.frame_rate = frame_rate.as_deref().and_then(parse_frame_rate);
    stream.sample_aspect_ratio = row.try_get("SampleAspectRatio").ok().flatten();
    stream.display_aspect_ratio = row.try_get("DisplayAspectRatio").ok().flatten();
    stream.color_params = derive_color_params(
        pixel_format.as_deref(),
        color_transfer.as_deref(),
        color_primaries.as_deref(),
        color_space.as_deref(),
    );

    Some(stream)
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
                  fp."NormalizeAudio" AS "NormalizeAudio",
                  fp."NormalizeVideo" AS "NormalizeVideo",
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
                  fp."AllowBFrames" AS "AllowBFrames",
                  fp."BitDepth" AS "BitDepth",
                  fp."VideoBitrate" AS "VideoBitrate",
                  fp."VideoBufferSize" AS "VideoBufferSize",
                  fp."TonemapAlgorithm" AS "TonemapAlgorithm",
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
    .await?;

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
) -> std::result::Result<Option<FrameRate>, sqlx::Error> {
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
    .await?;

    let mut rates = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("RFrameRate").ok())
        .filter_map(|value| parse_frame_rate(&value))
        .collect::<Vec<_>>();
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
            id: row.try_get("ProfileId")?,
            name: decode_string_column(row, "ProfileName")?,
            thread_count: row.try_get("ThreadCount")?,
            normalize_audio: decode_bool_column(row, "NormalizeAudio")?,
            normalize_video: decode_bool_column(row, "NormalizeVideo")?,
            hardware_acceleration: decode_enum_column(
                "ffmpeg profile hardware_acceleration",
                row,
                "HardwareAcceleration",
            )?,
            vaapi_display: decode_optional_string_column(row, "VaapiDisplay")?,
            vaapi_driver: decode_enum_column("ffmpeg profile vaapi_driver", row, "VaapiDriver")?,
            vaapi_device: decode_optional_string_column(row, "VaapiDevice")?,
            qsv_extra_hardware_frames: row.try_get("QsvExtraHardwareFrames")?,
            resolution_id: row.try_get("ResolutionId")?,
            scaling_behavior: decode_enum_column(
                "ffmpeg profile scaling_behavior",
                row,
                "ScalingBehavior",
            )?,
            pad_mode: decode_enum_column("ffmpeg profile pad_mode", row, "PadMode")?,
            video_format: decode_enum_column("ffmpeg profile video_format", row, "VideoFormat")?,
            video_profile: decode_optional_string_column(row, "VideoProfile")?,
            video_preset: decode_optional_string_column(row, "VideoPreset")?,
            allow_b_frames: decode_bool_column(row, "AllowBFrames")?,
            bit_depth: decode_enum_column("ffmpeg profile bit_depth", row, "BitDepth")?,
            video_bitrate: row.try_get("VideoBitrate")?,
            video_buffer_size: row.try_get("VideoBufferSize")?,
            tonemap_algorithm: decode_enum_column(
                "ffmpeg profile tonemap_algorithm",
                row,
                "TonemapAlgorithm",
            )?,
            audio_format: decode_enum_column("ffmpeg profile audio_format", row, "AudioFormat")?,
            audio_bitrate: row.try_get("AudioBitrate")?,
            audio_buffer_size: row.try_get("AudioBufferSize")?,
            normalize_loudness_mode: decode_enum_column(
                "ffmpeg profile normalize_loudness_mode",
                row,
                "NormalizeLoudnessMode",
            )?,
            target_loudness: row.try_get("TargetLoudness")?,
            audio_channels: row.try_get("AudioChannels")?,
            audio_sample_rate: row.try_get("AudioSampleRate")?,
            normalize_framerate: decode_bool_column(row, "NormalizeFramerate")?,
            normalize_colors: decode_bool_column(row, "NormalizeColors")?,
            deinterlace_video: decode_optional_bool_column(row, "DeinterlaceVideo")?,
        },
        resolution: Resolution {
            id: row.try_get("ResolutionRowId")?,
            name: decode_optional_string_column(row, "ResolutionName")?.unwrap_or_default(),
            is_custom: decode_bool_column(row, "ResolutionIsCustom")?,
            width: row.try_get("ResolutionWidth")?,
            height: row.try_get("ResolutionHeight")?,
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
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(PlaybackSubtitle {
                id: row.try_get("Id")?,
                subtitle_kind: row.try_get("SubtitleKind")?,
                title: row.try_get("Title")?,
                stream_index: row.try_get("StreamIndex")?,
                codec: row.try_get("Codec")?,
                default: row.try_get::<i64, _>("Default")? != 0,
                forced: row.try_get::<i64, _>("Forced")? != 0,
                sdh: row.try_get::<i64, _>("SDH")? != 0,
                language: row.try_get("Language")?,
                is_extracted: row.try_get::<i64, _>("IsExtracted")? != 0,
                path: row.try_get("Path")?,
            })
        })
        .collect()
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
    preferred_language: Option<&str>,
    subtitle_mode: ChannelSubtitleMode,
    streaming_mode: StreamingMode,
) -> Result<Option<PlaybackSubtitle>> {
    if subtitle_mode == ChannelSubtitleMode::None {
        return Ok(None);
    }

    let mut candidates = load_subtitles_for_media_item(pool, media_item_id).await?;
    if !use_embedded_subtitles(pool).await? {
        candidates.retain(|subtitle| subtitle.subtitle_kind != subtitle_kind::EMBEDDED);
    }

    if streaming_mode != StreamingMode::HttpLiveStreamingDirect {
        candidates.retain(|subtitle| {
            if subtitle.subtitle_kind != subtitle_kind::EMBEDDED || subtitle.is_image() {
                return true;
            }
            subtitle.is_extracted
                && subtitle
                    .path
                    .as_deref()
                    .is_some_and(|path| !path.trim().is_empty())
        });
    }

    if let Some(language) = preferred_language.filter(|value| !value.trim().is_empty()) {
        let codes = language_codes::all_language_codes([language]);
        candidates.retain(|subtitle| {
            subtitle.language.as_deref().is_some_and(|candidate| {
                codes
                    .iter()
                    .any(|code| candidate.eq_ignore_ascii_case(code.as_str()))
            })
        });
    }

    let selected = match subtitle_mode {
        ChannelSubtitleMode::Forced => candidates
            .into_iter()
            .filter(|subtitle| subtitle.forced)
            .min_by_key(|subtitle| subtitle.stream_index),
        ChannelSubtitleMode::Default => candidates
            .into_iter()
            .min_by_key(|subtitle| (!subtitle.default, subtitle.stream_index)),
        ChannelSubtitleMode::Any => candidates
            .into_iter()
            .min_by_key(|subtitle| subtitle.stream_index),
        ChannelSubtitleMode::None => None,
    };
    Ok(selected)
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
        let input = if subtitle.is_image() {
            InputFile::image_subtitle(media_path.to_owned(), method)
        } else {
            InputFile::text_subtitle(media_path.to_owned(), method)
        };
        return Some(input.with_stream_index(stream_index));
    }

    if subtitle.is_image() {
        return Some(
            InputFile::image_subtitle(media_path.to_owned(), SubtitleMethod::Burn)
                .with_stream_index(stream_index),
        );
    }

    let path = subtitle_text_path(subtitle, media_path)?;
    Some(InputFile::text_subtitle(path, SubtitleMethod::Burn))
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

const fn scan_kind_from_db(value: i64) -> VideoScanKind {
    match value {
        1 => VideoScanKind::Progressive,
        2 => VideoScanKind::Interlaced,
        _ => VideoScanKind::Unknown,
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

/// Derive HDR color metadata from ffprobe color tags, mirroring ErsatzTV's
/// HDR/BT.2020 detection (PQ/HLG transfer => HDR; BT.2020 primaries on a 10-bit
/// stream => the BT.2020 ten-bit colorspace path).
fn derive_color_params(
    pixel_format: Option<&str>,
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

    SourceColorParams {
        is_hdr,
        is_bt2020_ten: is_bt2020 && is_ten_bit,
    }
}
