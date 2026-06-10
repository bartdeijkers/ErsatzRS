//! Embedded text-subtitle and font extraction for deep show scans, playout
//! builds, and channel/settings save side effects.
//!
//! Ports `ErsatzTV.Application/Subtitles/Commands/ExtractEmbeddedShowSubtitlesHandler.cs`,
//! `ExtractEmbeddedSubtitlesHandler.cs`, and `ExtractEmbeddedSubtitlesHandlerBase.cs`.
//! ErsatzTV queues `ExtractEmbeddedShowSubtitles` after a successful deep show
//! scan (`QueueShowScanByLibraryIdHandler` lines 92-95) and
//! `ExtractEmbeddedSubtitles(playoutId)` after channel/settings/playout changes
//! can require subtitles.
//!
//! Two deliberate divergences from ErsatzTV, both consistent with existing
//! ErsatzRS patterns:
//!   * **Input path**: ErsatzTV's `GetMediaItemPath` builds internal
//!     streaming-proxy URLs for Plex/Jellyfin/Emby. ErsatzRS feeds ffmpeg the
//!     `MediaFile."Path"` with path-replacements applied, mirroring
//!     `crates/ersatzrs-web/src/transport/iptv/stream.rs` `load_path_replacements`.
//!   * **ffmpeg path**: resolved via `ersatzrs_ffmpeg::ffmpeg_path` (the shared
//!     locator: DB config / next-to-exe / `PATH`), mirroring ErsatzTV's
//!     `FFmpegLocator`.
//!   * **seek re-base delivery**: ErsatzTV serves re-based playback subtitles
//!     through a localhost HTTP pipe. ErsatzRS writes the same re-based stream
//!     to the subtitle cache and burns that temp file, matching the existing
//!     file-cache model used for extracted subtitles.

use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::Result;
use ersatzrs_ffmpeg::{ffmpeg_path, InputFile, InputKind, SubtitleMethod};
use md5::{Digest, Md5};
use sqlx::{Pool, Row};
use tokio::process::Command;

use crate::db::Db;
use crate::ffmpeg_config::{extract_embedded_subtitles, use_embedded_subtitles};
use crate::file_system_layout::{
    fonts_cache_folder, subtitle_cache_folder, subtitle_seek_cache_folder,
};
use crate::metadata::subtitle_kind;
use crate::repositories::common::encode_datetime;

/// Image-subtitle codecs that cannot be extracted as text. Mirrors
/// `Subtitle.IsImage` (`Subtitle.cs`) and the filters in
/// `ExtractEmbeddedSubtitlesHandlerBase`.
const IMAGE_CODECS: &[&str] = &[
    "hdmv_pgs_subtitle",
    "dvd_subtitle",
    "dvdsub",
    "vobsub",
    "pgssub",
    "pgs",
];

/// True when `codec` is an image-subtitle codec (not extractable as text).
fn is_image_codec(codec: &str) -> bool {
    IMAGE_CODECS.contains(&codec)
}

/// Port of `GetRelativeOutputPath` (`ExtractEmbeddedSubtitlesHandlerBase.cs`):
/// `MD5_hex_upper("{mediaItemId}_{streamIndex}_{codec}")` split into
/// `[..2]/[2..4]/<hash>.<ext>`. Returns `None` for codecs without a text
/// extension (the row is skipped).
fn relative_output_path(media_item_id: i64, stream_index: i32, codec: &str) -> Option<PathBuf> {
    let extension = match codec {
        "subrip" | "srt" | "mov_text" => "srt",
        "ass" => "ass",
        "webvtt" => "vtt",
        _ => return None,
    };

    let mut hasher = Md5::new();
    hasher.update(format!("{media_item_id}_{stream_index}_{codec}").as_bytes());
    let digest = hasher.finalize();
    let mut name = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02X}").expect("writing to String should not fail");
    }

    let subfolder = &name[..2];
    let subfolder2 = &name[2..4];
    Some(
        Path::new(subfolder)
            .join(subfolder2)
            .join(format!("{name}.{extension}")),
    )
}

/// `-c:s` value for a codec: `text` only for `mov_text`, else `copy`.
/// Mirrors `ExtractSubtitles`.
fn subtitle_codec_arg(codec: &str) -> &'static str {
    if codec == "mov_text" {
        "text"
    } else {
        "copy"
    }
}

fn seek_subtitle_output_format(input_path: &str) -> (&'static str, &'static str) {
    let extension_source = input_path.split(['?', '#']).next().unwrap_or(input_path);
    match Path::new(extension_source)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("ass" | "ssa") => ("ass", "ass"),
        Some("vtt") => ("webvtt", "vtt"),
        _ => ("srt", "srt"),
    }
}

fn md5_upper_hex(value: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut name = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02X}").expect("writing to String should not fail");
    }
    name
}

fn format_seek_seconds(seek_seconds: i64) -> String {
    let hours = seek_seconds / 3_600;
    let minutes = (seek_seconds % 3_600) / 60;
    let seconds = seek_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn seek_subtitle_output_path(input_path: &str, seek_seconds: i64) -> PathBuf {
    let (_, extension) = seek_subtitle_output_format(input_path);
    let name = md5_upper_hex(input_path);

    subtitle_seek_cache_folder().join(format!("{name}_{seek_seconds}.{extension}"))
}

/// Build the ffmpeg playback subtitle seek argv, mirroring
/// `PipelineBuilderBase.Seek`: read the text subtitle, output-seek to the
/// current item offset, copy subtitle packets, and write the selected text
/// container format.
fn build_seek_subtitle_args(input_path: &str, seek_seconds: i64, out_path: &Path) -> Vec<String> {
    let (format, _) = seek_subtitle_output_format(input_path);
    vec![
        "-nostdin".to_owned(),
        "-hide_banner".to_owned(),
        "-nostats".to_owned(),
        "-loglevel".to_owned(),
        "error".to_owned(),
        "-i".to_owned(),
        input_path.to_owned(),
        "-ss".to_owned(),
        format_seek_seconds(seek_seconds),
        "-c:s".to_owned(),
        "copy".to_owned(),
        "-f".to_owned(),
        format.to_owned(),
        out_path.to_string_lossy().into_owned(),
    ]
}

/// Re-base a text subtitle file to `seek_seconds` for burned playback.
///
/// Returns `input_path` unchanged for zero/negative seeks and falls back to the
/// original path on all ffmpeg/cache failures so playback remains no worse than
/// the pre-existing behavior. Returns `None` when the seek lands past the last
/// cue: ffmpeg then writes an empty file that libass cannot open, so the burn
/// must be skipped entirely instead of crashing the transcode.
pub async fn rebase_text_subtitle(input_path: &str, seek_seconds: i64) -> Option<String> {
    if seek_seconds <= 0 {
        return Some(input_path.to_owned());
    }

    let output_path = seek_subtitle_output_path(input_path, seek_seconds);
    if output_path.exists() {
        return non_empty_subtitle_path(&output_path).await;
    }

    if let Some(parent) = output_path.parent() {
        if let Err(error) = tokio::fs::create_dir_all(parent).await {
            tracing::warn!(
                %error,
                input_hash = %md5_upper_hex(input_path),
                seek_seconds,
                "failed to create subtitle seek cache directory"
            );
            return Some(input_path.to_owned());
        }
    }

    let ffmpeg_path = ffmpeg_path();
    let args = build_seek_subtitle_args(input_path, seek_seconds, &output_path);
    let output = match Command::new(&ffmpeg_path).args(&args).output().await {
        Ok(output) => output,
        Err(error) => {
            tracing::warn!(
                %error,
                input_hash = %md5_upper_hex(input_path),
                seek_seconds,
                "failed to spawn ffmpeg for subtitle seek rebase"
            );
            return Some(input_path.to_owned());
        }
    };

    if output.status.success() {
        non_empty_subtitle_path(&output_path).await
    } else {
        tracing::warn!(
            input_hash = %md5_upper_hex(input_path),
            seek_seconds,
            status = %output.status,
            stderr_bytes = output.stderr.len(),
            "failed to rebase subtitle for playback seek"
        );
        Some(input_path.to_owned())
    }
}

/// A successfully re-based subtitle with no remaining cues is written as an
/// empty file; report it as `None` so callers drop the burn instead of handing
/// libass a file it cannot open. The empty file stays in the cache as a marker
/// for subsequent requests at the same seek point.
async fn non_empty_subtitle_path(output_path: &Path) -> Option<String> {
    match tokio::fs::metadata(output_path).await {
        Ok(metadata) if metadata.len() == 0 => {
            tracing::debug!(
                path = %output_path.display(),
                "seeked subtitle has no remaining cues; skipping burn"
            );
            None
        }
        _ => Some(output_path.to_string_lossy().into_owned()),
    }
}

/// Re-base only burned text subtitle inputs. Image subtitles and copied
/// subtitle streams already ride the seeked media input and must not be
/// rewritten.
pub async fn rebase_burned_text_subtitle_input(
    input: Option<InputFile>,
    seek_seconds: i64,
) -> Option<InputFile> {
    let mut input = input?;
    if input.kind == InputKind::Subtitle
        && input.subtitle_method == Some(SubtitleMethod::Burn)
        && !input.is_image_based_subtitle
        && seek_seconds > 0
    {
        input.path = rebase_text_subtitle(&input.path, seek_seconds).await?;
    }
    Some(input)
}

/// One subtitle row scheduled for extraction.
struct SubtitleToExtract {
    id: i64,
    stream_index: i32,
    codec: String,
    relative_output: PathBuf,
}

#[derive(Clone, Copy)]
struct MetadataSubtitleJoin {
    metadata_table: &'static str,
    owner_column: &'static str,
    subtitle_column: &'static str,
}

const METADATA_SUBTITLE_JOINS: &[MetadataSubtitleJoin] = &[
    MetadataSubtitleJoin {
        metadata_table: "EpisodeMetadata",
        owner_column: "EpisodeId",
        subtitle_column: "EpisodeMetadataId",
    },
    MetadataSubtitleJoin {
        metadata_table: "MovieMetadata",
        owner_column: "MovieId",
        subtitle_column: "MovieMetadataId",
    },
    MetadataSubtitleJoin {
        metadata_table: "MusicVideoMetadata",
        owner_column: "MusicVideoId",
        subtitle_column: "MusicVideoMetadataId",
    },
    MetadataSubtitleJoin {
        metadata_table: "OtherVideoMetadata",
        owner_column: "OtherVideoId",
        subtitle_column: "OtherVideoMetadataId",
    },
];

/// Build the ffmpeg subtitle-extraction argv (port of the `ArgumentsBuilder`
/// block in `ExtractSubtitles`): `-nostdin -hide_banner -i <input>` then per
/// subtitle `-map 0:<idx> -c:s <copy|text> <fullOutputPath>`.
fn build_subtitle_args(input_path: &str, subtitles: &[SubtitleToExtract]) -> Vec<String> {
    let mut args = vec![
        "-nostdin".to_owned(),
        "-hide_banner".to_owned(),
        "-i".to_owned(),
        input_path.to_owned(),
    ];
    let cache = subtitle_cache_folder();
    for sub in subtitles {
        let full = cache.join(&sub.relative_output);
        args.push("-map".to_owned());
        args.push(format!("0:{}", sub.stream_index));
        args.push("-c:s".to_owned());
        args.push(subtitle_codec_arg(&sub.codec).to_owned());
        args.push(full.to_string_lossy().into_owned());
    }
    args
}

/// Build the ffmpeg font-dump argv (port of the `arguments` string in
/// `ExtractFonts`): `-nostdin -hide_banner -dump_attachment:t:<i> "" -i <input> -y`.
/// Run with the fonts cache as the working directory.
fn build_font_args(input_path: &str, attachment_index: usize) -> Vec<String> {
    vec![
        "-nostdin".to_owned(),
        "-hide_banner".to_owned(),
        format!("-dump_attachment:t:{attachment_index}"),
        String::new(),
        "-i".to_owned(),
        input_path.to_owned(),
        "-y".to_owned(),
    ]
}

/// Load all configured path-replacements across providers, mirroring
/// `stream.rs` `load_path_replacements` but querying directly (infra cannot
/// depend on `ersatzrs-web`). Each provider table stores `{Provider}Path`
/// (source) and `LocalPath` (replacement).
async fn load_path_replacements(pool: &Pool<Db>) -> Result<Vec<PathReplacement>> {
    let rows = sqlx::query(
        r#"SELECT "PlexPath" AS "Source", "LocalPath" AS "Local" FROM "PlexPathReplacement"
           UNION ALL
           SELECT "JellyfinPath" AS "Source", "LocalPath" AS "Local" FROM "JellyfinPathReplacement"
           UNION ALL
           SELECT "EmbyPath" AS "Source", "LocalPath" AS "Local" FROM "EmbyPathReplacement""#,
    )
    .fetch_all(pool)
    .await?;

    let mut replacements = Vec::new();
    for row in rows {
        let source: Option<String> = row.try_get("Source")?;
        let local: Option<String> = row.try_get("Local")?;
        if let (Some(source_path), Some(replacement_path)) = (source, local) {
            let source_platform = if source_path.contains('\\') || source_path.contains(':') {
                PathPlatform::Windows
            } else {
                PathPlatform::Unix
            };
            replacements.push(PathReplacement {
                source_platform,
                source_path,
                replacement_path,
            });
        }
    }
    Ok(replacements)
}

/// Resolve the media item's head-version `MediaFile."Path"`, with path-replacements
/// applied (target platform Unix, matching `stream.rs`).
async fn resolve_input_path(
    pool: &Pool<Db>,
    media_item_id: i64,
    replacements: &[PathReplacement],
) -> Result<Option<String>> {
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "Path"
           FROM "MediaVersion" mv
           JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE COALESCE(
                     mv."EpisodeId", mv."MovieId", mv."MusicVideoId", mv."OtherVideoId"
                 ) = ?
             AND mf."Path" IS NOT NULL
           ORDER BY mv."Id" ASC, mf."Id" ASC
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let path: String = row.try_get("Path")?;
    let resolved = if replacements.is_empty() {
        path
    } else {
        replace_path(replacements, &path, PathPlatform::Unix)
    };
    Ok(Some(resolved))
}

/// Episode IDs under `show_id` that have an embedded text `Subtitle` needing
/// extraction. Ports the SQL in `ExtractAll` (handler lines 75-85) AND the
/// text-codec filter from `GetMediaItemIdsWithTextSubtitles` (image codecs
/// excluded directly in SQL).
async fn episode_ids_with_text_subtitles(pool: &Pool<Db>, show_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query(
        r#"SELECT E."Id" AS "Id"
           FROM "Episode" E
           INNER JOIN "Season" S ON S."Id" = E."SeasonId"
           WHERE S."ShowId" = ?
           ORDER BY E."Id""#,
    )
    .bind(show_id)
    .fetch_all(pool)
    .await?;
    let episode_ids: Vec<i64> = rows
        .into_iter()
        .map(|row| row.try_get("Id"))
        .collect::<std::result::Result<_, _>>()?;
    media_item_ids_with_text_subtitles(pool, &episode_ids).await
}

async fn playout_media_item_ids_with_text_subtitles(
    pool: &Pool<Db>,
    playout_id: i64,
) -> Result<Vec<i64>> {
    let now = Utc::now();
    let until = now + Duration::hours(1);
    let rows = sqlx::query(
        r#"SELECT PI."MediaItemId" AS "MediaItemId", MIN(PI."Start") AS "SortStart"
           FROM "PlayoutItem" PI
           INNER JOIN "Playout" P ON P."Id" = PI."PlayoutId"
           INNER JOIN "Channel" C ON C."Id" = P."ChannelId"
           WHERE PI."PlayoutId" = ?
             AND PI."Finish" >= ?
             AND PI."Start" <= ?
             AND (
               C."SubtitleMode" != 0 OR EXISTS (
                 SELECT 1
                 FROM "ProgramScheduleItem" PSI
                 WHERE PSI."ProgramScheduleId" = P."ProgramScheduleId"
                   AND PSI."SubtitleMode" IS NOT NULL
                   AND PSI."SubtitleMode" != 0
               )
             )
           GROUP BY PI."MediaItemId"
           ORDER BY MIN(PI."Start")"#,
    )
    .bind(playout_id)
    .bind(encode_datetime(now))
    .bind(encode_datetime(until))
    .fetch_all(pool)
    .await?;
    let media_item_ids: Vec<i64> = rows
        .into_iter()
        .map(|row| row.try_get("MediaItemId"))
        .collect::<std::result::Result<_, _>>()?;
    media_item_ids_with_text_subtitles(pool, &media_item_ids).await
}

async fn media_item_ids_with_text_subtitles(
    pool: &Pool<Db>,
    media_item_ids: &[i64],
) -> Result<Vec<i64>> {
    if media_item_ids.is_empty() {
        return Ok(Vec::new());
    }

    let media_placeholders = media_item_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = IMAGE_CODECS
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let mut result = Vec::new();

    for join in METADATA_SUBTITLE_JOINS {
        let sql = format!(
            r#"SELECT DISTINCT M."{owner_column}" AS "Id"
               FROM "{metadata_table}" M
               INNER JOIN "Subtitle" SUB ON SUB."{subtitle_column}" = M."Id"
               WHERE M."{owner_column}" IN ({media_placeholders})
                 AND SUB."SubtitleKind" = {embedded}
                 AND SUB."Codec" IS NOT NULL
                 AND SUB."Codec" NOT IN ({placeholders})"#,
            metadata_table = join.metadata_table,
            owner_column = join.owner_column,
            subtitle_column = join.subtitle_column,
            embedded = subtitle_kind::EMBEDDED,
        );
        let mut query = sqlx::query(crate::db::assert_sql_safe(&sql));
        for id in media_item_ids {
            query = query.bind(*id);
        }
        for codec in IMAGE_CODECS {
            query = query.bind(*codec);
        }
        let rows = query.fetch_all(pool).await?;
        for row in rows {
            let id: i64 = row.try_get("Id")?;
            if !result.contains(&id) {
                result.push(id);
            }
        }
    }

    result.sort_by_key(|id| {
        media_item_ids
            .iter()
            .position(|candidate| candidate == id)
            .unwrap_or(usize::MAX)
    });
    Ok(result)
}

/// Extract embedded text subtitles + font attachments for every episode under
/// `show_id`. Self-gates on the ffmpeg config keys, so a default-configured
/// deployment (`extract_embedded_subtitles = false`) is a no-op. Per-episode
/// ffmpeg failures are logged and skipped (matching ErsatzTV's swallow).
///
/// # Errors
///
/// Returns an error only when a database query fails; ffmpeg spawn/exit
/// failures are logged and do not abort the run.
pub async fn extract_embedded_show_subtitles(pool: &Pool<Db>, show_id: i64) -> Result<()> {
    extract_embedded_episode_subtitles(pool, episode_ids_with_text_subtitles(pool, show_id).await?)
        .await
}

/// Extract embedded text subtitles + font attachments for every eligible media
/// item in one playout. This is the ErsatzRS equivalent of ErsatzTV's
/// `ExtractEmbeddedSubtitles(Some(playoutId))` background request.
///
/// # Errors
///
/// Returns an error only when a database query fails; ffmpeg spawn/exit
/// failures are logged and do not abort the run.
pub async fn extract_embedded_playout_subtitles(pool: &Pool<Db>, playout_id: i64) -> Result<()> {
    extract_embedded_media_item_subtitles(
        pool,
        playout_media_item_ids_with_text_subtitles(pool, playout_id).await?,
    )
    .await
}

async fn extract_embedded_episode_subtitles(pool: &Pool<Db>, episode_ids: Vec<i64>) -> Result<()> {
    extract_embedded_media_item_subtitles(pool, episode_ids).await
}

async fn extract_embedded_media_item_subtitles(
    pool: &Pool<Db>,
    media_item_ids: Vec<i64>,
) -> Result<()> {
    // ffmpeg path resolved via the shared locator (DB config / next-to-exe /
    // PATH), matching ErsatzTV's FFmpegLocator.
    let ffmpeg_path = ffmpeg_path();

    // Gate (mirror handler lines 48-64): both keys must be enabled.
    if !use_embedded_subtitles(pool).await? || !extract_embedded_subtitles(pool).await? {
        return Ok(());
    }

    if media_item_ids.is_empty() {
        return Ok(());
    }

    let replacements = load_path_replacements(pool).await?;
    for media_item_id in media_item_ids {
        let Some(input_path) = resolve_input_path(pool, media_item_id, &replacements).await? else {
            tracing::debug!(
                media_item_id,
                "no media path for media item; skipping extraction"
            );
            continue;
        };
        extract_subtitles(pool, media_item_id, &input_path, &ffmpeg_path).await?;
        extract_fonts(pool, media_item_id, &input_path, &ffmpeg_path).await?;
    }

    Ok(())
}

/// Extract this media item's embedded text subtitles. Ports `ExtractSubtitles`.
async fn extract_subtitles(
    pool: &Pool<Db>,
    media_item_id: i64,
    input_path: &str,
    ffmpeg_path: &str,
) -> Result<()> {
    // All embedded subtitles for this media item. Image codecs and "already
    // extracted" rows are filtered in-memory below, mirroring the `.Filter(...)`
    // chain in ErsatzTV's `ExtractSubtitles`.
    let mut rows = Vec::new();
    for join in METADATA_SUBTITLE_JOINS {
        let sql = format!(
            r#"SELECT SUB."Id" AS "Id", SUB."StreamIndex" AS "StreamIndex",
                      SUB."Codec" AS "Codec", SUB."IsExtracted" AS "IsExtracted",
                      SUB."Path" AS "Path"
               FROM "Subtitle" SUB
               INNER JOIN "{metadata_table}" M ON M."Id" = SUB."{subtitle_column}"
               WHERE M."{owner_column}" = ?
                 AND SUB."SubtitleKind" = {embedded}
                 AND SUB."Codec" IS NOT NULL"#,
            metadata_table = join.metadata_table,
            subtitle_column = join.subtitle_column,
            owner_column = join.owner_column,
            embedded = subtitle_kind::EMBEDDED,
        );
        rows.extend(
            sqlx::query(crate::db::assert_sql_safe(&sql))
                .bind(media_item_id)
                .fetch_all(pool)
                .await?,
        );
    }

    let mut to_extract = Vec::new();
    for row in rows {
        let codec: String = row.try_get("Codec")?;
        let is_extracted: i64 = row.try_get("IsExtracted")?;
        let path: Option<String> = row.try_get("Path")?;
        let stream_index: i32 = row.try_get("StreamIndex")?;
        let id: i64 = row.try_get("Id")?;

        if is_image_codec(&codec) {
            continue;
        }
        let Some(relative_output) = relative_output_path(media_item_id, stream_index, &codec)
        else {
            continue;
        };
        let full = subtitle_cache_folder().join(&relative_output);
        // Re-extract when not extracted, no path recorded, or the cache file is
        // missing (mirror the `FileDoesntExist` filter in ExtractSubtitles).
        let needs = is_extracted == 0 || path.is_none() || !full.exists();
        if !needs {
            continue;
        }
        to_extract.push(SubtitleToExtract {
            id,
            stream_index,
            codec,
            relative_output,
        });
    }

    if to_extract.is_empty() {
        return Ok(());
    }

    // Ensure output directories exist and delete any stale output.
    for sub in &to_extract {
        let full = subtitle_cache_folder().join(&sub.relative_output);
        if let Some(parent) = full.parent() {
            if let Err(error) = tokio::fs::create_dir_all(parent).await {
                tracing::error!(%error, "failed to create subtitle cache directory");
                return Ok(());
            }
        }
        if full.exists() {
            let _ = tokio::fs::remove_file(&full).await;
        }
    }

    let args = build_subtitle_args(input_path, &to_extract);
    let output = match Command::new(ffmpeg_path).args(&args).output().await {
        Ok(output) => output,
        Err(error) => {
            tracing::error!(%error, media_item_id, "failed to spawn ffmpeg for subtitle extraction");
            return Ok(());
        }
    };

    if output.status.success() {
        for sub in &to_extract {
            sqlx::query(r#"UPDATE "Subtitle" SET "IsExtracted" = 1, "Path" = ? WHERE "Id" = ?"#)
                .bind(sub.relative_output.to_string_lossy().into_owned())
                .bind(sub.id)
                .execute(pool)
                .await?;
        }
        tracing::debug!(
            media_item_id,
            count = to_extract.len(),
            "successfully extracted subtitles"
        );
    } else {
        tracing::error!(
            media_item_id,
            stderr = %String::from_utf8_lossy(&output.stderr),
            "failed to extract subtitles"
        );
    }

    Ok(())
}

/// Extract this media item's font attachments. Ports `ExtractFonts`. ffmpeg exits
/// non-zero even on a successful dump, so success is detected by the output
/// file existing.
async fn extract_fonts(
    pool: &Pool<Db>,
    media_item_id: i64,
    input_path: &str,
    ffmpeg_path: &str,
) -> Result<()> {
    // Attachment streams for the episode head version, ordered by Index. The
    // 0-based position in this list is the `-dump_attachment:t:<i>` index.
    let rows = sqlx::query(
        r#"SELECT ms."FileName" AS "FileName", ms."MimeType" AS "MimeType"
           FROM "MediaVersion" mv
           JOIN "MediaStream" ms ON ms."MediaVersionId" = mv."Id"
           WHERE COALESCE(
                     mv."EpisodeId", mv."MovieId", mv."MusicVideoId", mv."OtherVideoId"
                 ) = ?
             AND ms."MediaStreamKind" = 4
           ORDER BY mv."Id" ASC, ms."Index" ASC"#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await?;

    let fonts_dir = fonts_cache_folder();
    for (attachment_index, row) in rows.iter().enumerate() {
        let mime_type: Option<String> = row.try_get("MimeType")?;
        let file_name: Option<String> = row.try_get("FileName")?;

        let mime = mime_type.unwrap_or_default();
        if !mime.contains("font") && !mime.contains("opentype") {
            continue;
        }
        let Some(file_name) = file_name.filter(|name| !name.trim().is_empty()) else {
            continue;
        };

        let full = fonts_dir.join(&file_name);
        if full.exists() {
            // already extracted
            continue;
        }
        if let Err(error) = tokio::fs::create_dir_all(&fonts_dir).await {
            tracing::error!(%error, "failed to create fonts cache directory");
            return Ok(());
        }

        let args = build_font_args(input_path, attachment_index);
        let result = Command::new(ffmpeg_path)
            .args(&args)
            .current_dir(&fonts_dir)
            .output()
            .await;
        let stderr = match result {
            Ok(output) => String::from_utf8_lossy(&output.stderr).into_owned(),
            Err(error) => {
                tracing::error!(%error, media_item_id, "failed to spawn ffmpeg for font extraction");
                continue;
            }
        };

        // ffmpeg returns exit code 1 even on a successful dump; check the file.
        if full.exists() {
            tracing::debug!(media_item_id, font = %file_name, "successfully extracted font");
        } else {
            tracing::error!(media_item_id, font = %file_name, %stderr, "failed to extract attached font");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "mysql"))]
    use sqlx::Executor;

    #[cfg(not(feature = "mysql"))]
    async fn test_db() -> Pool<Db> {
        let temp_dir =
            std::env::temp_dir().join(format!("ersatzrs-subtitles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create subtitles test db dir");
        let database_url = format!("sqlite:{}", temp_dir.join("ErsatzRS.db").display());
        crate::db::init(&database_url)
            .await
            .expect("initialize subtitles test database")
    }

    #[test]
    fn image_codec_filter_matches_etv_list() {
        for codec in IMAGE_CODECS {
            assert!(is_image_codec(codec), "{codec} should be an image codec");
        }
        for codec in ["subrip", "ass", "webvtt", "mov_text", "srt"] {
            assert!(!is_image_codec(codec), "{codec} should be text");
        }
    }

    #[test]
    fn relative_output_path_uses_md5_upper_hex_and_extension_map() {
        // MD5("1_2_subrip") upper-hex.
        let path = relative_output_path(1, 2, "subrip").unwrap();
        let expected_hash = {
            let mut hasher = Md5::new();
            hasher.update(b"1_2_subrip");
            let digest = hasher.finalize();
            let mut s = String::new();
            for byte in digest {
                use std::fmt::Write as _;
                write!(&mut s, "{byte:02X}").unwrap();
            }
            s
        };
        let expected = Path::new(&expected_hash[..2])
            .join(&expected_hash[2..4])
            .join(format!("{expected_hash}.srt"));
        assert_eq!(path, expected);

        // Extension mapping.
        assert!(relative_output_path(1, 0, "srt")
            .unwrap()
            .to_string_lossy()
            .ends_with(".srt"));
        assert!(relative_output_path(1, 0, "ass")
            .unwrap()
            .to_string_lossy()
            .ends_with(".ass"));
        assert!(relative_output_path(1, 0, "webvtt")
            .unwrap()
            .to_string_lossy()
            .ends_with(".vtt"));
        assert!(relative_output_path(1, 0, "mov_text")
            .unwrap()
            .to_string_lossy()
            .ends_with(".srt"));
        // Unknown codec yields no output path.
        assert!(relative_output_path(1, 0, "pgssub").is_none());
    }

    #[test]
    fn subtitle_codec_arg_uses_text_only_for_mov_text() {
        assert_eq!(subtitle_codec_arg("mov_text"), "text");
        assert_eq!(subtitle_codec_arg("subrip"), "copy");
        assert_eq!(subtitle_codec_arg("ass"), "copy");
    }

    #[test]
    fn build_subtitle_args_snapshot() {
        let cache = subtitle_cache_folder();
        let subs = vec![
            SubtitleToExtract {
                id: 1,
                stream_index: 2,
                codec: "subrip".to_owned(),
                relative_output: Path::new("ab").join("cd").join("AABBCC.srt"),
            },
            SubtitleToExtract {
                id: 2,
                stream_index: 3,
                codec: "mov_text".to_owned(),
                relative_output: Path::new("ef").join("01").join("DDEEFF.srt"),
            },
        ];
        let args = build_subtitle_args("/srv/media/show/s01e01.mkv", &subs);

        let out1 = cache.join("ab").join("cd").join("AABBCC.srt");
        let out2 = cache.join("ef").join("01").join("DDEEFF.srt");
        assert_eq!(
            args,
            vec![
                "-nostdin".to_owned(),
                "-hide_banner".to_owned(),
                "-i".to_owned(),
                "/srv/media/show/s01e01.mkv".to_owned(),
                "-map".to_owned(),
                "0:2".to_owned(),
                "-c:s".to_owned(),
                "copy".to_owned(),
                out1.to_string_lossy().into_owned(),
                "-map".to_owned(),
                "0:3".to_owned(),
                "-c:s".to_owned(),
                "text".to_owned(),
                out2.to_string_lossy().into_owned(),
            ]
        );
    }

    #[test]
    fn build_seek_subtitle_args_matches_etv_rebase_pass() {
        let out = Path::new("/tmp/seeked.srt");
        let args = build_seek_subtitle_args("/srv/media/show/subtitle.srt", 900, out);
        assert_eq!(
            args,
            vec![
                "-nostdin".to_owned(),
                "-hide_banner".to_owned(),
                "-nostats".to_owned(),
                "-loglevel".to_owned(),
                "error".to_owned(),
                "-i".to_owned(),
                "/srv/media/show/subtitle.srt".to_owned(),
                "-ss".to_owned(),
                "00:15:00".to_owned(),
                "-c:s".to_owned(),
                "copy".to_owned(),
                "-f".to_owned(),
                "srt".to_owned(),
                "/tmp/seeked.srt".to_owned(),
            ]
        );
    }

    #[test]
    fn build_seek_subtitle_args_selects_text_output_format_from_extension() {
        assert_eq!(
            build_seek_subtitle_args("/tmp/subtitle.ass", 3_723, Path::new("/tmp/out.ass"))[12],
            "ass"
        );
        assert_eq!(
            build_seek_subtitle_args("/tmp/subtitle.ssa", 3_723, Path::new("/tmp/out.ass"))[12],
            "ass"
        );
        assert_eq!(
            build_seek_subtitle_args("/tmp/subtitle.vtt", 3_723, Path::new("/tmp/out.vtt"))[12],
            "webvtt"
        );
        assert_eq!(
            build_seek_subtitle_args(
                "https://example.test/subtitle.ass?X-Plex-Token=redacted",
                3_723,
                Path::new("/tmp/out.ass")
            )[12],
            "ass"
        );
        assert_eq!(
            build_seek_subtitle_args("/tmp/subtitle.subrip", 3_723, Path::new("/tmp/out.srt"))[8],
            "01:02:03"
        );
    }

    #[tokio::test]
    async fn rebase_text_subtitle_skips_non_positive_seek() {
        assert_eq!(
            rebase_text_subtitle("/srv/media/show/subtitle.srt", 0).await,
            Some("/srv/media/show/subtitle.srt".to_owned())
        );
        assert_eq!(
            rebase_text_subtitle("/srv/media/show/subtitle.srt", -1).await,
            Some("/srv/media/show/subtitle.srt".to_owned())
        );
    }

    #[tokio::test]
    async fn non_empty_subtitle_path_skips_empty_seek_output() {
        let dir = tempfile::tempdir().expect("temp dir");
        let empty = dir.path().join("empty.srt");
        tokio::fs::write(&empty, b"").await.expect("write empty");
        assert_eq!(non_empty_subtitle_path(&empty).await, None);

        let cues = dir.path().join("cues.srt");
        tokio::fs::write(&cues, b"1\n00:00:00,000 --> 00:00:02,000\nHallo\n")
            .await
            .expect("write cues");
        assert_eq!(
            non_empty_subtitle_path(&cues).await,
            Some(cues.to_string_lossy().into_owned())
        );
    }

    #[tokio::test]
    async fn rebase_burned_text_subtitle_input_skips_non_text_burn_inputs() {
        let image = InputFile::image_subtitle("/srv/media/show/video.mkv", SubtitleMethod::Burn);
        assert_eq!(
            rebase_burned_text_subtitle_input(Some(image.clone()), 900).await,
            Some(image)
        );

        let copied = InputFile::text_subtitle("/srv/media/show/video.mkv", SubtitleMethod::Copy);
        assert_eq!(
            rebase_burned_text_subtitle_input(Some(copied.clone()), 900).await,
            Some(copied)
        );

        let burned = InputFile::text_subtitle("/srv/media/show/subtitle.srt", SubtitleMethod::Burn);
        assert_eq!(
            rebase_burned_text_subtitle_input(Some(burned.clone()), 0).await,
            Some(burned)
        );
    }

    #[test]
    fn build_font_args_matches_etv_dump_attachment() {
        let args = build_font_args("/srv/media/show/s01e01.mkv", 0);
        assert_eq!(
            args,
            vec![
                "-nostdin".to_owned(),
                "-hide_banner".to_owned(),
                "-dump_attachment:t:0".to_owned(),
                String::new(),
                "-i".to_owned(),
                "/srv/media/show/s01e01.mkv".to_owned(),
                "-y".to_owned(),
            ]
        );
    }

    #[tokio::test]
    #[cfg(not(feature = "mysql"))]
    #[allow(clippy::too_many_lines)]
    async fn playout_selection_matches_etv_window_and_media_type_filter() {
        let pool = test_db().await;
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'local', 1, 1)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, '/media')"#,
        )
        .await
        .unwrap();
        for id in [10_i64, 20, 30] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (10), (30)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "OtherVideo" ("Id") VALUES (20)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "MovieMetadata" ("Id", "MovieId", "DateAdded", "DateUpdated", "MetadataKind")
               VALUES (100, 10, '2026-05-31 00:00:00', '2026-05-31 00:00:00', 1),
                      (300, 30, '2026-05-31 00:00:00', '2026-05-31 00:00:00', 1)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "OtherVideoMetadata" ("Id", "OtherVideoId", "DateAdded", "DateUpdated", "MetadataKind")
               VALUES (200, 20, '2026-05-31 00:00:00', '2026-05-31 00:00:00', 1)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Subtitle" (
                   "MovieMetadataId", "SubtitleKind", "StreamIndex", "Codec",
                   "Default", "Forced", "SDH", "IsExtracted", "DateAdded", "DateUpdated"
               ) VALUES
                   (100, 0, 2, 'subrip', 0, 0, 0, 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00'),
                   (300, 0, 2, 'subrip', 0, 0, 0, 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Subtitle" (
                   "OtherVideoMetadataId", "SubtitleKind", "StreamIndex", "Codec",
                   "Default", "Forced", "SDH", "IsExtracted", "DateAdded", "DateUpdated"
               ) VALUES (200, 0, 2, 'pgssub', 0, 0, 0, 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "FFmpegProfile" (
                   "Id", "AudioBitrate", "AudioBufferSize", "AudioChannels",
                   "AudioFormat", "AudioSampleRate", "BitDepth", "HardwareAcceleration",
                   "Name", "NormalizeLoudnessMode", "ResolutionId", "ScalingBehavior",
                   "ThreadCount", "VaapiDriver", "VideoBitrate", "VideoBufferSize",
                   "VideoFormat"
               ) VALUES (1, 192, 0, 2, 0, 48000, 8, 0, 'default', 0, 1, 0, 0, 0, 4000, 0, 0)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Channel" (
                   "Id", "UniqueId", "Number", "SortNumber", "Name", "FFmpegProfileId",
                   "StreamingMode", "StreamSelectorMode", "SubtitleMode",
                   "MusicVideoCreditsMode", "SongVideoMode", "PlayoutSource",
                   "PlayoutMode", "TranscodeMode", "IdleBehavior"
               ) VALUES (1, '00000000-0000-0000-0000-000000000001', '1', 1.0, 'Channel 1', 1,
                         0, 0, 2, 0, 0, 0, 0, 0, 0)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, 0, 0)"#)
            .await
            .unwrap();

        let now = Utc::now();
        let inside_start = encode_datetime(now);
        let inside_finish = encode_datetime(now + Duration::minutes(30));
        let outside_start = encode_datetime(now + Duration::hours(2));
        let outside_finish = encode_datetime(now + Duration::hours(3));
        for (id, media_item_id, start, finish) in [
            (1_i64, 10_i64, inside_start.as_str(), inside_finish.as_str()),
            (2, 20, inside_start.as_str(), inside_finish.as_str()),
            (3, 30, outside_start.as_str(), outside_finish.as_str()),
        ] {
            sqlx::query(
                r#"INSERT INTO "PlayoutItem" (
                       "Id", "PlayoutId", "MediaItemId", "Start", "Finish",
                       "InPoint", "OutPoint", "DisableWatermarks", "FillerKind", "GuideGroup"
                   ) VALUES (?, 1, ?, ?, ?, '00:00:00', '00:30:00', 0, 0, 0)"#,
            )
            .bind(id)
            .bind(media_item_id)
            .bind(start)
            .bind(finish)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(
            playout_media_item_ids_with_text_subtitles(&pool, 1)
                .await
                .unwrap(),
            vec![10]
        );

        pool.execute(r#"UPDATE "Channel" SET "SubtitleMode" = 0 WHERE "Id" = 1"#)
            .await
            .unwrap();
        assert!(playout_media_item_ids_with_text_subtitles(&pool, 1)
            .await
            .unwrap()
            .is_empty());
    }
}
