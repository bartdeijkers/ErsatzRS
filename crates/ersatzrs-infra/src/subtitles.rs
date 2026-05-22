//! Embedded text-subtitle and font extraction for deep show scans and channel
//! save side effects.
//!
//! Ports `ErsatzTV.Application/Subtitles/Commands/ExtractEmbeddedShowSubtitlesHandler.cs`
//! and `ExtractEmbeddedSubtitlesHandlerBase.cs`, scoped to TV episodes. ErsatzTV
//! queues `ExtractEmbeddedShowSubtitles` after a successful deep show scan
//! (`QueueShowScanByLibraryIdHandler` lines 92-95) and
//! `ExtractEmbeddedSubtitles(playoutId)` after a channel with subtitle mode
//! enabled is saved (`UpdateChannelHandler` lines 143-150).
//!
//! Two deliberate divergences from ErsatzTV, both consistent with existing
//! ErsatzRS patterns:
//!   * **Input path**: ErsatzTV's `GetMediaItemPath` builds internal
//!     streaming-proxy URLs for Plex/Jellyfin/Emby. ErsatzRS feeds ffmpeg the
//!     `MediaFile."Path"` with path-replacements applied, mirroring
//!     `crates/ersatzrs-web/src/transport/iptv/stream.rs` `load_path_replacements`.
//!   * **ffmpeg path**: resolved via `ersatzrs_ffmpeg::ffmpeg_path_from_env`
//!     instead of the `FFmpegPath` config key / `FFmpegPathMustExist`.

use std::path::{Path, PathBuf};

use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::Result;
use ersatzrs_ffmpeg::ffmpeg_path_from_env;
use md5::{Digest, Md5};
use sqlx::{Pool, Row};
use tokio::process::Command;

use crate::db::Db;
use crate::ffmpeg_config::{extract_embedded_subtitles, use_embedded_subtitles};
use crate::file_system_layout::{fonts_cache_folder, subtitle_cache_folder};
use crate::metadata::subtitle_kind;

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

/// One subtitle row scheduled for extraction.
struct SubtitleToExtract {
    id: i64,
    stream_index: i32,
    codec: String,
    relative_output: PathBuf,
}

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

/// Resolve the episode head-version `MediaFile."Path"`, with path-replacements
/// applied (target platform Unix, matching `stream.rs`).
async fn resolve_input_path(
    pool: &Pool<Db>,
    episode_id: i64,
    replacements: &[PathReplacement],
) -> Result<Option<String>> {
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "Path"
           FROM "MediaVersion" mv
           JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE mv."EpisodeId" = ? AND mf."Path" IS NOT NULL
           ORDER BY mv."Id" ASC, mf."Id" ASC
           LIMIT 1"#,
    )
    .bind(episode_id)
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
    let placeholders = IMAGE_CODECS
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"SELECT DISTINCT E."Id"
           FROM "Episode" E
           INNER JOIN "Season" S ON S."Id" = E."SeasonId"
           INNER JOIN "EpisodeMetadata" EM ON EM."EpisodeId" = E."Id"
           INNER JOIN "Subtitle" SUB ON SUB."EpisodeMetadataId" = EM."Id"
           WHERE S."ShowId" = ?
             AND SUB."SubtitleKind" = {embedded}
             AND (SUB."IsExtracted" = 0 OR SUB."Path" IS NULL)
             AND ("SUB"."Codec" IS NULL OR "SUB"."Codec" NOT IN ({placeholders}))"#,
        embedded = subtitle_kind::EMBEDDED,
    );
    let mut query = sqlx::query(&sql).bind(show_id);
    for codec in IMAGE_CODECS {
        query = query.bind(*codec);
    }
    let rows = query.fetch_all(pool).await?;
    rows.into_iter().map(|row| Ok(row.try_get("Id")?)).collect()
}

async fn playout_episode_ids_with_text_subtitles(
    pool: &Pool<Db>,
    playout_id: i64,
) -> Result<Vec<i64>> {
    let placeholders = IMAGE_CODECS
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        r#"SELECT DISTINCT E."Id" AS "Id"
           FROM "PlayoutItem" PI
           INNER JOIN "Episode" E ON E."Id" = PI."MediaItemId"
           INNER JOIN "EpisodeMetadata" EM ON EM."EpisodeId" = E."Id"
           INNER JOIN "Subtitle" SUB ON SUB."EpisodeMetadataId" = EM."Id"
           WHERE PI."PlayoutId" = ?
             AND SUB."SubtitleKind" = {embedded}
             AND SUB."Codec" IS NOT NULL
             AND SUB."Codec" NOT IN ({placeholders})
           ORDER BY E."Id""#,
        embedded = subtitle_kind::EMBEDDED,
    );
    let mut query = sqlx::query(&sql).bind(playout_id);
    for codec in IMAGE_CODECS {
        query = query.bind(*codec);
    }
    let rows = query.fetch_all(pool).await?;
    rows.into_iter().map(|row| Ok(row.try_get("Id")?)).collect()
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

/// Extract embedded text subtitles + font attachments for every episode in one
/// playout. This is the ErsatzRS equivalent of ErsatzTV's
/// `ExtractEmbeddedSubtitles(Some(playoutId))` background request.
///
/// # Errors
///
/// Returns an error only when a database query fails; ffmpeg spawn/exit
/// failures are logged and do not abort the run.
pub async fn extract_embedded_playout_subtitles(pool: &Pool<Db>, playout_id: i64) -> Result<()> {
    extract_embedded_episode_subtitles(
        pool,
        playout_episode_ids_with_text_subtitles(pool, playout_id).await?,
    )
    .await
}

async fn extract_embedded_episode_subtitles(pool: &Pool<Db>, episode_ids: Vec<i64>) -> Result<()> {
    // Divergence: ffmpeg path from env, not the FFmpegPath config key.
    let ffmpeg_path = ffmpeg_path_from_env();

    // Gate (mirror handler lines 48-64): both keys must be enabled.
    if !use_embedded_subtitles(pool).await? || !extract_embedded_subtitles(pool).await? {
        return Ok(());
    }

    if episode_ids.is_empty() {
        return Ok(());
    }

    let replacements = load_path_replacements(pool).await?;
    for episode_id in episode_ids {
        let Some(input_path) = resolve_input_path(pool, episode_id, &replacements).await? else {
            tracing::debug!(episode_id, "no media path for episode; skipping extraction");
            continue;
        };
        extract_subtitles(pool, episode_id, &input_path, &ffmpeg_path).await?;
        extract_fonts(pool, episode_id, &input_path, &ffmpeg_path).await?;
    }

    Ok(())
}

/// Extract this episode's embedded text subtitles. Ports `ExtractSubtitles`.
async fn extract_subtitles(
    pool: &Pool<Db>,
    episode_id: i64,
    input_path: &str,
    ffmpeg_path: &str,
) -> Result<()> {
    // All embedded subtitles for this episode. Image codecs and "already
    // extracted" rows are filtered in-memory below, mirroring the `.Filter(...)`
    // chain in ErsatzTV's `ExtractSubtitles`.
    let sql = format!(
        r#"SELECT SUB."Id" AS "Id", SUB."StreamIndex" AS "StreamIndex",
                  SUB."Codec" AS "Codec", SUB."IsExtracted" AS "IsExtracted",
                  SUB."Path" AS "Path"
           FROM "Subtitle" SUB
           INNER JOIN "EpisodeMetadata" EM ON EM."Id" = SUB."EpisodeMetadataId"
           WHERE EM."EpisodeId" = ?
             AND SUB."SubtitleKind" = {embedded}
             AND SUB."Codec" IS NOT NULL"#,
        embedded = subtitle_kind::EMBEDDED,
    );
    let rows = sqlx::query(&sql).bind(episode_id).fetch_all(pool).await?;

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
        let Some(relative_output) = relative_output_path(episode_id, stream_index, &codec) else {
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
            tracing::error!(%error, episode_id, "failed to spawn ffmpeg for subtitle extraction");
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
            episode_id,
            count = to_extract.len(),
            "successfully extracted subtitles"
        );
    } else {
        tracing::error!(
            episode_id,
            stderr = %String::from_utf8_lossy(&output.stderr),
            "failed to extract subtitles"
        );
    }

    Ok(())
}

/// Extract this episode's font attachments. Ports `ExtractFonts`. ffmpeg exits
/// non-zero even on a successful dump, so success is detected by the output
/// file existing.
async fn extract_fonts(
    pool: &Pool<Db>,
    episode_id: i64,
    input_path: &str,
    ffmpeg_path: &str,
) -> Result<()> {
    // Attachment streams for the episode head version, ordered by Index. The
    // 0-based position in this list is the `-dump_attachment:t:<i>` index.
    let rows = sqlx::query(
        r#"SELECT ms."FileName" AS "FileName", ms."MimeType" AS "MimeType"
           FROM "MediaVersion" mv
           JOIN "MediaStream" ms ON ms."MediaVersionId" = mv."Id"
           WHERE mv."EpisodeId" = ? AND ms."MediaStreamKind" = 4
           ORDER BY mv."Id" ASC, ms."Index" ASC"#,
    )
    .bind(episode_id)
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
                tracing::error!(%error, episode_id, "failed to spawn ffmpeg for font extraction");
                continue;
            }
        };

        // ffmpeg returns exit code 1 even on a successful dump; check the file.
        if full.exists() {
            tracing::debug!(episode_id, font = %file_name, "successfully extracted font");
        } else {
            tracing::error!(episode_id, font = %file_name, %stderr, "failed to extract attached font");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
