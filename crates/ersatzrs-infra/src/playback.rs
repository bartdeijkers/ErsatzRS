//! Shared helpers for building `FFmpeg` playback pipelines from persisted media
//! metadata, used by both the MPEG-TS and HLS transcode paths.

use std::path::{Path, PathBuf};

use ersatzrs_core::language_codes;
use ersatzrs_core::repositories::Result;
use ersatzrs_domain::VideoScanKind;
use ersatzrs_domain::{ChannelSubtitleMode, StreamingMode};
use ersatzrs_ffmpeg::{
    InputFile, OutputFormat, PixelFormat, SourceColorParams, SourceVideoCodec, SourceVideoStream,
    SubtitleMethod,
};
use sqlx::Row;

use crate::db::DbPool;
use crate::ffmpeg_config::use_embedded_subtitles;
use crate::file_system_layout::{fonts_cache_folder, subtitle_cache_folder};
use crate::metadata::subtitle_kind;

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
                  mv."VideoScanKind" AS "VideoScanKind"
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
    let scan_kind = row
        .try_get::<i64, _>("VideoScanKind")
        .ok()
        .map_or(VideoScanKind::Unknown, scan_kind_from_db);

    let mut stream = SourceVideoStream::new(SourceVideoCodec::from_ffmpeg_name(&codec));
    stream.pixel_format = pixel_format
        .as_deref()
        .and_then(PixelFormat::from_ffmpeg_name);
    stream.scan_kind = scan_kind;
    stream.color_params = derive_color_params(
        pixel_format.as_deref(),
        color_transfer.as_deref(),
        color_primaries.as_deref(),
        color_space.as_deref(),
    );

    Some(stream)
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
