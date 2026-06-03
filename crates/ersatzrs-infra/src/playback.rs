//! Shared helpers for building `FFmpeg` playback pipelines from persisted media
//! metadata, used by both the MPEG-TS and HLS transcode paths.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use ersatzrs_core::language_codes;
use ersatzrs_core::repositories::{GraphicsRepository as _, RepositoryError, Result};
use ersatzrs_domain::{
    Channel, ChannelSubtitleMode, ChannelWatermark, ChannelWatermarkImageSource,
    ChannelWatermarkMode, DecoMode, FFmpegProfile, FillerKind, GraphicsElement,
    GraphicsElementConfig, MotionEndBehavior, Resolution, StreamingMode, VideoScanKind,
    CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
};
use ersatzrs_ffmpeg::{
    ffprobe_path_from_env, FrameRate, FrameSize, GraphicsPipelineOverlay, GraphicsRenderContext,
    GraphicsTemplateRenderer, InputFile, OutputFormat, PixelFormat, SourceColorParams,
    SourceVideoCodec, SourceVideoStream, SubtitleMethod, WatermarkFadeKind, WatermarkFadePoint,
    WatermarkOverlay,
};
use sqlx::Row;

use crate::db::DbPool;
use crate::ffmpeg_config::use_embedded_subtitles;
use crate::ffprobe::FfprobeExtractor;
use crate::file_system_layout::{data_path, fonts_cache_folder, subtitle_cache_folder};
use crate::graphics_template::{
    graphics_template_variables, render_graphics_template, scalar_template_variables,
    GraphicsTemplateContext,
};
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

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPlaybackWatermark {
    pub watermark: ChannelWatermark,
    pub image_path: PathBuf,
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

    let item_ids = load_playout_item_watermark_ids(pool, playout_item_id).await?;
    if !item_ids.is_empty() {
        return resolve_playback_watermarks(pool, channel.id, item_ids).await;
    }

    let fallback_id = match channel.watermark_id {
        Some(id) => Some(id),
        None => load_global_watermark_id(pool).await?,
    };
    resolve_playback_watermarks(pool, channel.id, fallback_id.into_iter().collect()).await
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

    graphics.extend(load_playout_item_graphics(pool, playout_item_id).await?);
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
) -> Result<Vec<GraphicsPipelineOverlay>> {
    let renderer = GraphicsTemplateRenderer::new(output_dir.as_ref());
    let mut overlays = Vec::new();
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
        )
        .await?;
        let element = render_graphics_element_templates(&resolved.element, &variables)?;
        let scalar_variables = scalar_template_variables(&variables);
        let render_context = graphics_render_context_with_motion_probe(context, &element).await;
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
            overlays.push(rendered.into_pipeline_overlay());
        }
    }
    overlays.sort_by_key(graphics_overlay_z_index);
    Ok(overlays)
}

async fn graphics_render_context_with_motion_probe(
    context: &PlaybackGraphicsRenderContext,
    element: &GraphicsElement,
) -> GraphicsRenderContext {
    let render_context =
        GraphicsRenderContext::new(context.frame_size, context.square_pixel_frame_size);
    let GraphicsElementConfig::Motion(config) = &element.config else {
        return render_context;
    };
    if config.end_behavior != MotionEndBehavior::Hold {
        return render_context;
    }
    let Some(video_path) = config
        .video_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return render_context;
    };

    match FfprobeExtractor::new(ffprobe_path_from_env())
        .extract(video_path)
        .await
    {
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
    }
}

fn render_graphics_element_templates(
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
            config.text = render_optional_graphics_template(config.text.as_deref(), variables)?;
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
                image_path,
            });
        }
    }
    Ok(resolved)
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
    .await?;

    rows.into_iter()
        .map(|row| row.try_get("WatermarkId").map_err(Into::into))
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
    .await?;
    Ok(PlaybackWatermarkContext {
        playout_id: row.try_get("PlayoutId")?,
        filler_kind: decode_enum_column("playout item filler kind", &row, "FillerKind")?,
        playout_deco_id: row.try_get("DecoId")?,
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
    .await?;
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
    .await?;
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
    .await?;
    rows.into_iter()
        .map(|row| row.try_get("WatermarkId").map_err(Into::into))
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
    .await?;
    rows.into_iter()
        .map(|row| row.try_get("GraphicsElementId").map_err(Into::into))
        .collect()
}

async fn load_active_template_deco(
    pool: &DbPool,
    playout_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<PlaybackDeco>> {
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
    .await?;

    for row in rows {
        let start = parse_time_of_day_seconds(&row.try_get::<String, _>("StartTime")?);
        let finish = parse_time_of_day_seconds(&row.try_get::<String, _>("EndTime")?);
        if start <= time_seconds && (finish == 0 || finish > time_seconds) {
            let deco_id = row.try_get("DecoId")?;
            let deco = load_playback_deco(pool, deco_id).await?;
            return Ok(Some(deco));
        }
    }

    Ok(None)
}

async fn load_active_template_graphics_deco(
    pool: &DbPool,
    playout_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<PlaybackGraphicsDeco>> {
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
    .await?;

    for row in rows {
        let start = parse_time_of_day_seconds(&row.try_get::<String, _>("StartTime")?);
        let finish = parse_time_of_day_seconds(&row.try_get::<String, _>("EndTime")?);
        if start <= time_seconds && (finish == 0 || finish > time_seconds) {
            let deco_id = row.try_get("DecoId")?;
            let deco = load_playback_graphics_deco(pool, deco_id).await?;
            return Ok(Some(deco));
        }
    }

    Ok(None)
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
    .await?;

    for row in rows {
        if playout_template_matches(&row, now) {
            return row.try_get("DecoTemplateId").map_err(Into::into);
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
    .await?;

    rows.into_iter()
        .map(|row| Ok((row.try_get("GraphicsElementId")?, row.try_get("Variables")?)))
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
    .await?;
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
    .await?;
    map_channel_watermark(&row)
}

async fn resolve_watermark_image_path(
    pool: &DbPool,
    channel_id: i64,
    watermark: &ChannelWatermark,
) -> Result<Option<PathBuf>> {
    match watermark.image_source {
        ChannelWatermarkImageSource::Custom => Ok(watermark
            .image
            .as_deref()
            .map(str::trim)
            .filter(|image| !image.is_empty())
            .map(|image| data_path(Path::new("watermarks").join(image)))
            .filter(|path| path.is_file())),
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
            .await?;
            Ok(path
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .filter(|path| !path.starts_with("http://") && !path.starts_with("https://"))
                .map(resolve_channel_logo_path)
                .filter(|path| path.is_file()))
        }
        ChannelWatermarkImageSource::Resource => Ok(None),
    }
}

fn resolve_channel_logo_path(path: &str) -> PathBuf {
    let path = path.strip_prefix("iptv/logos/").unwrap_or(path);
    data_path(Path::new("iptv").join("logos").join(path))
}

fn map_channel_watermark(row: &crate::db::DbRow) -> Result<ChannelWatermark> {
    Ok(ChannelWatermark {
        id: row.try_get("Id")?,
        name: decode_string_column(row, "Name")?,
        mode: decode_enum_column("channel watermark mode", row, "Mode")?,
        image_source: decode_enum_column("channel watermark image source", row, "ImageSource")?,
        image: decode_optional_string_column(row, "Image")?,
        original_content_type: decode_optional_string_column(row, "OriginalContentType")?,
        location: decode_enum_column("watermark location", row, "Location")?,
        size: decode_enum_column("watermark size", row, "Size")?,
        width_percent: row.try_get("WidthPercent")?,
        horizontal_margin_percent: row.try_get("HorizontalMarginPercent")?,
        vertical_margin_percent: row.try_get("VerticalMarginPercent")?,
        frequency_minutes: row.try_get("FrequencyMinutes")?,
        duration_seconds: row.try_get("DurationSeconds")?,
        opacity: row.try_get("Opacity")?,
        place_within_source_content: decode_bool_column(row, "PlaceWithinSourceContent")?,
        opacity_expression: decode_optional_string_column(row, "OpacityExpression")?,
        z_index: row.try_get("ZIndex")?,
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
