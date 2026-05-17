//! Local TV-folder scanner.
//!
//! Ports `ErsatzTV.Scanner/Core/Metadata/TelevisionFolderScanner.cs`:
//! one library root contains one folder per show; each show folder contains
//! one folder per season; each season folder contains video files plus
//! optional `tvshow.nfo` / `{episode}.nfo` sidecars and image files
//! (`poster.*`, `fanart.*`, `thumb.*`, `season01-poster.*`,
//! `{episode}-thumb.*`).
//!
//! Show and season `LibraryFolder` rows are persisted (parented as in
//! `ErsatzTV`), each episode `MediaFile` is linked to its season
//! `LibraryFolder`, and the season folder-etag fast path skips episode
//! probing for unchanged folders while still refreshing season
//! metadata/poster. A restore guard disables the fast path for any season
//! holding a previously trashed (`State IN (1,2)`) episode so the file can
//! return to `Normal`, mirroring ErsatzTV's `GetAllTrashedItems` check.
//! After all shows are processed, stale `LibraryFolder` rows are cleaned,
//! mirroring ErsatzTV's `CleanEtagsForLibraryPath`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use chrono::Utc;
use ersatzrs_infra::db::{Db, DbPool};
use ersatzrs_infra::ffprobe::FfprobeExtractor;
use ersatzrs_infra::metadata::{
    artwork_kind, replace_artwork, replace_guids, replace_people, replace_strings, ArtworkRef,
    PersonRef,
};
use sqlx::{Row, Transaction};
use tracing::warn;

use crate::library_folder;
use crate::nfo::{self, EpisodeNfo, NfoActor, NfoGuid, TvShowNfo};
use crate::{
    encode_datetime, insert_media_item, insert_media_version_with_owner, path_hash,
    replace_chapters, replace_streams, update_media_version, LocalLibraryPath, Result,
    ScannerError,
};

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "m4v", "webm"];
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tbn", "webp"];

const METADATA_KIND_FALLBACK: i32 = 0;
const METADATA_KIND_SIDECAR: i32 = 1;

/// One season folder discovered for a show.
struct SeasonWork {
    folder: PathBuf,
    season_number: i32,
    episode_files: Vec<PathBuf>,
}

/// One show folder discovered under a library root.
struct ShowWork {
    folder: PathBuf,
    seasons: Vec<SeasonWork>,
}

/// Walk the library root and enumerate all show/season/episode work units.
///
/// The plan refers to this as "pre-enumerate so the progress fraction is
/// truthful" — same model as the existing local scanner.
fn collect_show_work(root: &str) -> Vec<ShowWork> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut show_folders: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    show_folders.sort();

    let mut shows = Vec::new();
    for show_folder in show_folders {
        if !show_folder.is_dir() {
            continue;
        }
        if file_name_starts_with_dot(&show_folder) {
            continue;
        }

        let Ok(season_entries) = std::fs::read_dir(&show_folder) else {
            continue;
        };
        let mut season_folders: Vec<PathBuf> =
            season_entries.flatten().map(|entry| entry.path()).collect();
        season_folders.sort();

        let mut seasons = Vec::new();
        for season_folder in season_folders {
            if !season_folder.is_dir() {
                continue;
            }
            if file_name_starts_with_dot(&season_folder) {
                continue;
            }
            let Some(season_number) = parse_season_number_for_folder(&season_folder) else {
                continue;
            };

            let mut episode_files = collect_video_files(&season_folder);
            episode_files.sort();
            if episode_files.is_empty() {
                continue;
            }

            seasons.push(SeasonWork {
                folder: season_folder,
                season_number,
                episode_files,
            });
        }

        if seasons.is_empty() {
            continue;
        }

        shows.push(ShowWork {
            folder: show_folder,
            seasons,
        });
    }

    shows
}

fn file_name_starts_with_dot(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn collect_video_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(path) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push_back(path);
                continue;
            }
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("._"))
            {
                continue;
            }
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_ascii_lowercase);
            if extension
                .as_deref()
                .is_some_and(|ext| VIDEO_EXTENSIONS.contains(&ext))
            {
                files.push(path);
            }
        }
    }
    files
}

/// Parse a season number from a folder name. Mirrors
/// `FallbackMetadataProvider.GetSeasonNumberForFolder` in `ErsatzTV.Core`.
fn parse_season_number_for_folder(folder: &Path) -> Option<i32> {
    let name = folder.file_name().and_then(|name| name.to_str())?;

    if let Ok(value) = name.parse::<i32>() {
        return Some(value);
    }

    if let Some(value) = match_season_prefix(name) {
        return Some(value);
    }

    if let Some(last) = name.rsplit(' ').next() {
        if let Ok(value) = last.parse::<i32>() {
            return Some(value);
        }
    }

    if name.eq_ignore_ascii_case("specials") || name.to_ascii_lowercase().ends_with("specials") {
        return Some(0);
    }

    None
}

/// Match the `s(?:eason)?\s?(\d+)(?![e\d])` regex used by `ErsatzTV` without
/// pulling in a regex crate. Returns the parsed season number on match.
fn match_season_prefix(input: &str) -> Option<i32> {
    let lower = input.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    if bytes.is_empty() || bytes[0] != b's' {
        return None;
    }
    let mut cursor = 1;
    if lower.starts_with("season") {
        cursor = "season".len();
    }
    while cursor < bytes.len() && bytes[cursor] == b' ' {
        cursor += 1;
    }
    let digits_start = cursor;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    if cursor == digits_start {
        return None;
    }
    if cursor < bytes.len() {
        let next = bytes[cursor];
        if next == b'e' || next.is_ascii_digit() {
            return None;
        }
    }
    lower[digits_start..cursor].parse().ok()
}

/// Parse an episode number from a file basename. Mirrors
/// `FallbackMetadataProvider.GetEpisodeMetadata` (single-episode fallback
/// only — multi-episode fragments still keep the first detected number).
fn parse_episode_number_from_basename(basename: &str) -> Option<i32> {
    let bytes = basename.as_bytes();
    let lower = basename.to_ascii_lowercase();
    let mut i = 0;
    while i < bytes.len() {
        if lower.as_bytes()[i] == b's' {
            let s_end = scan_digits(&lower, i + 1);
            if s_end > i + 1 {
                let mut cursor = s_end;
                while cursor < bytes.len() && matches!(lower.as_bytes()[cursor], b'.' | b'_' | b'x')
                {
                    cursor += 1;
                }
                if cursor < bytes.len() && lower.as_bytes()[cursor] == b'e' {
                    let e_end = scan_digits(&lower, cursor + 1);
                    if e_end > cursor + 1 {
                        return lower[cursor + 1..e_end].parse().ok();
                    }
                }
            }
        }
        if lower.as_bytes()[i].is_ascii_digit() {
            let s_end = scan_digits(&lower, i);
            if s_end > i && s_end < bytes.len() {
                let sep = lower.as_bytes()[s_end];
                if matches!(sep, b'x' | b'_' | b'.') {
                    let e_end = scan_digits(&lower, s_end + 1);
                    if e_end > s_end + 1 {
                        return lower[s_end + 1..e_end].parse().ok();
                    }
                }
            }
        }
        i += 1;
    }
    None
}

fn scan_digits(input: &str, start: usize) -> usize {
    let bytes = input.as_bytes();
    let mut cursor = start;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    cursor
}

/// Locate an artwork file in a folder by base-name candidates and supported
/// image extensions. The first match in `(name, ext)` order wins, mirroring
/// `TelevisionFolderScanner.LocateArtworkForShow`.
fn locate_artwork(folder: &Path, names: &[&str]) -> Option<PathBuf> {
    for name in names {
        for ext in IMAGE_EXTENSIONS {
            let candidate = folder.join(format!("{name}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn locate_thumbnail(video_path: &Path) -> Option<PathBuf> {
    let folder = video_path.parent()?;
    let stem = video_path.file_stem()?.to_str()?;
    for ext in IMAGE_EXTENSIONS {
        let candidate = folder.join(format!("{stem}-thumb.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn locate_season_poster(show_folder: &Path, season_number: i32) -> Option<PathBuf> {
    let prefix = format!("season{season_number:02}-poster");
    for ext in IMAGE_EXTENSIONS {
        let candidate = show_folder.join(format!("{prefix}.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn locate_show_nfo(show_folder: &Path) -> Option<PathBuf> {
    let candidate = show_folder.join("tvshow.nfo");
    candidate.is_file().then_some(candidate)
}

fn locate_episode_nfo(video_path: &Path) -> Option<PathBuf> {
    let candidate = video_path.with_extension("nfo");
    candidate.is_file().then_some(candidate)
}

fn artwork_ref(path: &Path, kind: i32) -> ArtworkRef {
    ArtworkRef {
        kind,
        path: path.display().to_string(),
        date_added: Utc::now(),
    }
}

fn guids_from_nfo(guids: &[NfoGuid]) -> Vec<String> {
    guids.iter().filter_map(NfoGuid::as_metadata_guid).collect()
}

fn actors_from_nfo(actors: &[NfoActor]) -> Vec<PersonRef> {
    actors
        .iter()
        .filter_map(|actor| {
            actor.name.as_ref().map(|name| PersonRef {
                name: name.clone(),
                role: actor.role.clone(),
                order: actor.order,
            })
        })
        .collect()
}

fn directors_from(values: &[String]) -> Vec<PersonRef> {
    values.iter().cloned().map(PersonRef::new).collect()
}

/// Resolve a folder's parent and create/load its `LibraryFolder` row,
/// mirroring ErsatzTV's `GetParentFolderId` + `GetOrAddFolder` pairing used
/// before both the show and season upserts. Returns the persisted row, or
/// `None` after logging when the folder cannot be resolved or created — the
/// caller treats `None` as "skip this folder", matching the existing
/// per-item warn-and-continue style. `kind` is only used for log context.
async fn ensure_library_folder(
    pool: &DbPool,
    library_path_id: i64,
    folder: &Path,
    kind: &str,
) -> Option<library_folder::LibraryFolderRow> {
    let parent_id = match library_folder::get_parent_folder_id(pool, library_path_id, folder).await
    {
        Ok(parent_id) => parent_id,
        Err(err) => {
            warn!(
                folder = %folder.display(),
                error = %err,
                "failed to resolve {kind} parent folder; skipping"
            );
            return None;
        }
    };

    match library_folder::get_or_add_library_folder(pool, library_path_id, parent_id, folder).await
    {
        Ok(row) => Some(row),
        Err(err) => {
            warn!(
                folder = %folder.display(),
                error = %err,
                "failed to create {kind} library folder; skipping"
            );
            None
        }
    }
}

/// Port of `MediaItemRepository.GetAllTrashedItems`, scoped to episodes:
/// every episode `MediaFile.Path` in this library path whose owning
/// `MediaItem.State` is `FileNotFound (1)` or `Unavailable (2)`. ErsatzTV uses
/// `State IN (1,2)` here, so `RemoteOnly (3)` is intentionally excluded — a
/// remote-only item is not "trashed" and must not disable the etag fast path.
async fn trashed_episode_paths(pool: &DbPool, library_path_id: i64) -> Result<Vec<String>> {
    let rows = sqlx::query(
        r#"SELECT "mf"."Path"
           FROM "MediaItem" AS "mi"
           JOIN "MediaVersion" AS "mv" ON "mi"."Id" = "mv"."EpisodeId"
           JOIN "MediaFile" AS "mf" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "mi"."State" IN (1, 2) AND "mi"."LibraryPathId" = ?"#,
    )
    .bind(library_path_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| row.get::<Option<String>, _>("Path"))
        .collect())
}

/// Decide whether a season's episode scan can be skipped, mirroring the
/// `etagMatch` logic in ErsatzTV's `ScanSeasons`: the stored etag must equal
/// the current etag, and the restore guard must not fire. The guard disables
/// the fast path when a previously trashed episode is now present under the
/// season folder, so the restored file can return to `Normal` — ErsatzTV's
/// `allTrashedItems.Any(f => f.StartsWith(seasonFolder, OrdinalIgnoreCase))`.
/// `trashed_paths` must already be lowercased for the case-insensitive match.
fn season_etag_match(
    stored_etag: Option<&str>,
    current_etag: &str,
    season_folder: &Path,
    trashed_paths: &[String],
) -> bool {
    if stored_etag != Some(current_etag) {
        return false;
    }
    let season_prefix = season_folder.to_string_lossy().to_lowercase();
    if trashed_paths
        .iter()
        .any(|path| path.starts_with(&season_prefix))
    {
        warn!(
            folder = %season_folder.display(),
            "previously trashed items are present; rescanning season"
        );
        return false;
    }
    true
}

/// Sync one library path of `LibraryMediaKind::Shows`. Returns the number of
/// episode files processed so the caller can roll the total scan report.
/// `on_episode(count)` advances scan progress by `count` episode files. It is
/// called once per scanned episode (`count == 1`) and once per season skipped
/// by the etag fast path (`count == skipped episodes`), so the progress sink
/// still reaches 100% even though skipped episodes are not probed. Skipped
/// episodes do not contribute to the returned count.
pub(crate) async fn sync_local_show_library(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    extractor: &FfprobeExtractor,
    mut on_episode: impl FnMut(usize),
) -> Result<usize> {
    let shows = collect_show_work(&library_path.path);
    let mut episode_count = 0usize;

    // Load trashed episode paths once per library path, like ErsatzTV's
    // `GetAllTrashedItems` call at the top of `ScanFolder`. Lowercased so the
    // per-season prefix check can be case-insensitive, matching ErsatzTV's
    // `StartsWith(..., StringComparison.OrdinalIgnoreCase)`.
    let trashed_paths: Vec<String> = trashed_episode_paths(pool, library_path.id)
        .await?
        .into_iter()
        .map(|path| path.to_lowercase())
        .collect();

    for show in shows {
        // Create the show LibraryFolder before the show upsert, mirroring
        // ErsatzTV's `ScanFolder`. The show item does not use this row
        // itself; it exists so season folders can point at it via ParentId.
        if ensure_library_folder(pool, library_path.id, &show.folder, "show")
            .await
            .is_none()
        {
            continue;
        }

        let show_id = match upsert_show(pool, library_path, &show).await {
            Ok(id) => id,
            Err(err) => {
                warn!(
                    folder = %show.folder.display(),
                    error = %err,
                    "failed to upsert show; skipping"
                );
                continue;
            }
        };

        for season in &show.seasons {
            // Create the season LibraryFolder before the season upsert,
            // mirroring ErsatzTV's `ScanSeasons`. Episodes link their
            // `MediaFile` to this row.
            let Some(season_folder) =
                ensure_library_folder(pool, library_path.id, &season.folder, "season").await
            else {
                continue;
            };

            let current_etag = crate::folder_etag::folder_etag_with_subfolders(&season.folder);
            let etag_match = season_etag_match(
                season_folder.etag.as_deref(),
                &current_etag,
                &season.folder,
                &trashed_paths,
            );

            // Always run the season upsert so season metadata and poster
            // refresh still happen on unchanged folders.
            let season_id =
                match upsert_season(pool, library_path, show_id, &show.folder, season).await {
                    Ok(id) => id,
                    Err(err) => {
                        warn!(
                            folder = %season.folder.display(),
                            error = %err,
                            "failed to upsert season; skipping"
                        );
                        continue;
                    }
                };

            // Etag fast path: skip all episode probing/upsert work for an
            // unchanged season, but still advance progress for the skipped
            // episodes so the sink reaches 100%.
            if etag_match {
                on_episode(season.episode_files.len());
                continue;
            }

            for episode_path in &season.episode_files {
                match sync_local_episode(
                    pool,
                    library_path,
                    season_id,
                    season.season_number,
                    season_folder.id,
                    episode_path,
                    extractor,
                )
                .await
                {
                    Ok(()) => {
                        episode_count += 1;
                        on_episode(1);
                    }
                    Err(err) => {
                        warn!(
                            path = %episode_path.display(),
                            error = %err,
                            "failed to upsert episode; skipping"
                        );
                    }
                }
            }

            // Persist the new etag after the non-skipped episode scan, like
            // ErsatzTV's `SetEtag` call following `ScanEpisodes`.
            if let Err(err) =
                library_folder::set_library_folder_etag(pool, season_folder.id, &current_etag).await
            {
                warn!(
                    folder = %season.folder.display(),
                    error = %err,
                    "failed to persist season folder etag"
                );
            }
        }
    }

    // After all shows are processed for this library path, drop stale
    // `LibraryFolder` rows whose path no longer exists, mirroring ErsatzTV's
    // `CleanEtagsForLibraryPath` call at the end of `ScanFolder`. A failure
    // here is non-fatal: the scan results are already persisted.
    if let Err(err) = library_folder::clean_library_folders_for_path(pool, library_path.id).await {
        warn!(
            library_path_id = library_path.id,
            error = %err,
            "failed to clean stale library folders"
        );
    }

    Ok(episode_count)
}

/// Count how many episode files exist under a library root. Used by the
/// progress sink so the fraction reflects the same units as the worker loop.
pub(crate) fn count_episode_files(root: &str) -> usize {
    collect_show_work(root)
        .iter()
        .map(|show| {
            show.seasons
                .iter()
                .map(|s| s.episode_files.len())
                .sum::<usize>()
        })
        .sum()
}

#[allow(clippy::too_many_lines)]
async fn upsert_show(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    show: &ShowWork,
) -> Result<i64> {
    let nfo_path = locate_show_nfo(&show.folder);
    let nfo = nfo_path.as_ref().and_then(|path| match nfo::parse_tvshow_nfo(path) {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            warn!(path = %path.display(), error = %err, "failed to parse tvshow.nfo; falling back");
            None
        }
    });

    let folder_name = show
        .folder
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    let fallback_title = folder_name.clone();
    let fallback_year = parse_year_from_folder_name(&folder_name);

    let title = nfo
        .as_ref()
        .and_then(|n| n.title.clone())
        .unwrap_or(fallback_title);
    let sort_title = nfo
        .as_ref()
        .and_then(|n| n.sort_title.clone())
        .unwrap_or_else(|| nfo::sort_title(&title));
    let year = nfo.as_ref().and_then(|n| n.year).or(fallback_year);
    let metadata_kind = if nfo.is_some() {
        METADATA_KIND_SIDECAR
    } else {
        METADATA_KIND_FALLBACK
    };

    let mut tx = pool.begin().await?;

    let existing = sqlx::query(
        r#"SELECT "s"."Id" AS "ShowId", "sm"."Id" AS "ShowMetadataId"
           FROM "Show" AS "s"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "s"."Id"
           LEFT JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
           WHERE "mi"."LibraryPathId" = ?
             AND (
               COALESCE("sm"."Title", '') = ? OR
               COALESCE("sm"."OriginalTitle", '') = ?
             )
           LIMIT 1"#,
    )
    .bind(library_path.id)
    .bind(&title)
    .bind(&folder_name)
    .fetch_optional(&mut *tx)
    .await?;

    let now = encode_datetime(Utc::now());
    let release_date = year.map(|y| format!("{y:04}-01-01"));

    let (show_id, metadata_id) = if let Some(row) = existing {
        let show_id: i64 = row.get("ShowId");
        let metadata_id: Option<i64> = row.get("ShowMetadataId");
        let metadata_id = if let Some(id) = metadata_id {
            sqlx::query(
                r#"UPDATE "ShowMetadata"
                   SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
                       "OriginalTitle" = ?, "SortTitle" = ?, "Year" = ?,
                       "ReleaseDate" = ?, "ContentRating" = ?, "Plot" = ?,
                       "Outline" = ?, "Tagline" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(&now)
            .bind(metadata_kind)
            .bind(&title)
            .bind(&folder_name)
            .bind(&sort_title)
            .bind(year)
            .bind(&release_date)
            .bind(nfo.as_ref().and_then(|n| n.content_rating.clone()))
            .bind(nfo.as_ref().and_then(|n| n.plot.clone()))
            .bind(nfo.as_ref().and_then(|n| n.outline.clone()))
            .bind(nfo.as_ref().and_then(|n| n.tagline.clone()))
            .bind(id)
            .execute(&mut *tx)
            .await?;
            id
        } else {
            insert_show_metadata(
                &mut tx,
                show_id,
                &title,
                &folder_name,
                &sort_title,
                year,
                release_date.as_deref(),
                metadata_kind,
                nfo.as_ref(),
                &now,
            )
            .await?
        };
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = 0 WHERE "Id" = ?"#)
            .bind(show_id)
            .execute(&mut *tx)
            .await?;
        (show_id, metadata_id)
    } else {
        let media_item_id = insert_media_item(&mut tx, library_path.id).await?;
        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
            .bind(media_item_id)
            .execute(&mut *tx)
            .await?;
        let metadata_id = insert_show_metadata(
            &mut tx,
            media_item_id,
            &title,
            &folder_name,
            &sort_title,
            year,
            release_date.as_deref(),
            metadata_kind,
            nfo.as_ref(),
            &now,
        )
        .await?;
        (media_item_id, metadata_id)
    };

    if let Some(parsed) = nfo.as_ref() {
        replace_strings(
            &mut tx,
            "Genre",
            "ShowMetadataId",
            metadata_id,
            &parsed.genres,
        )
        .await?;
        replace_strings(&mut tx, "Tag", "ShowMetadataId", metadata_id, &parsed.tags).await?;
        replace_strings(
            &mut tx,
            "Studio",
            "ShowMetadataId",
            metadata_id,
            &parsed.studios,
        )
        .await?;
        replace_people(
            &mut tx,
            "Actor",
            "ShowMetadataId",
            metadata_id,
            &actors_from_nfo(&parsed.actors),
        )
        .await?;
        replace_guids(
            &mut tx,
            "ShowMetadataId",
            metadata_id,
            &guids_from_nfo(&parsed.guids),
        )
        .await?;
    } else {
        replace_strings(&mut tx, "Genre", "ShowMetadataId", metadata_id, &[]).await?;
        replace_strings(&mut tx, "Tag", "ShowMetadataId", metadata_id, &[]).await?;
        replace_strings(&mut tx, "Studio", "ShowMetadataId", metadata_id, &[]).await?;
        replace_people(&mut tx, "Actor", "ShowMetadataId", metadata_id, &[]).await?;
        replace_guids(&mut tx, "ShowMetadataId", metadata_id, &[]).await?;
    }

    let mut artwork = Vec::new();
    if let Some(path) = locate_artwork(&show.folder, &["poster", "folder"]) {
        artwork.push(artwork_ref(&path, artwork_kind::POSTER));
    }
    if let Some(path) = locate_artwork(&show.folder, &["fanart", "backdrop"]) {
        artwork.push(artwork_ref(&path, artwork_kind::FAN_ART));
    }
    if let Some(path) = locate_artwork(&show.folder, &["thumb"]) {
        artwork.push(artwork_ref(&path, artwork_kind::THUMBNAIL));
    }
    replace_artwork(
        &mut tx,
        "ShowMetadataId",
        metadata_id,
        &[
            artwork_kind::POSTER,
            artwork_kind::FAN_ART,
            artwork_kind::THUMBNAIL,
        ],
        &artwork,
    )
    .await?;

    tx.commit().await?;
    Ok(show_id)
}

#[allow(clippy::too_many_arguments)]
async fn insert_show_metadata(
    tx: &mut Transaction<'_, Db>,
    show_id: i64,
    title: &str,
    original_title: &str,
    sort_title: &str,
    year: Option<i32>,
    release_date: Option<&str>,
    metadata_kind: i32,
    nfo: Option<&TvShowNfo>,
    now: &str,
) -> Result<i64> {
    let result = sqlx::query(
        r#"INSERT INTO "ShowMetadata" (
            "ShowId", "DateAdded", "DateUpdated", "MetadataKind",
            "Title", "OriginalTitle", "SortTitle", "Year", "ReleaseDate",
            "ContentRating", "Plot", "Outline", "Tagline"
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(show_id)
    .bind(now)
    .bind(now)
    .bind(metadata_kind)
    .bind(title)
    .bind(original_title)
    .bind(sort_title)
    .bind(year)
    .bind(release_date)
    .bind(nfo.and_then(|n| n.content_rating.clone()))
    .bind(nfo.and_then(|n| n.plot.clone()))
    .bind(nfo.and_then(|n| n.outline.clone()))
    .bind(nfo.and_then(|n| n.tagline.clone()))
    .execute(&mut **tx)
    .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

fn parse_year_from_folder_name(name: &str) -> Option<i32> {
    let bytes = name.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if (i == 0 || matches!(bytes[i - 1], b'(' | b'[' | b'.' | b' '))
            && bytes[i..i + 4].iter().all(u8::is_ascii_digit)
        {
            if let Ok(value) = std::str::from_utf8(&bytes[i..i + 4])
                .unwrap_or_default()
                .parse::<i32>()
            {
                if (1900..=2100).contains(&value) {
                    return Some(value);
                }
            }
        }
        i += 1;
    }
    None
}

async fn upsert_season(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    show_id: i64,
    show_folder: &Path,
    season: &SeasonWork,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let now = encode_datetime(Utc::now());
    let title = if season.season_number == 0 {
        "Specials".to_owned()
    } else {
        format!("Season {}", season.season_number)
    };
    let sort_title = if season.season_number == 0 {
        "00".to_owned()
    } else {
        format!("{:02}", season.season_number)
    };

    let existing: Option<(i64, Option<i64>)> = sqlx::query(
        r#"SELECT "s"."Id" AS "SeasonId", "sm"."Id" AS "SeasonMetadataId"
           FROM "Season" AS "s"
           LEFT JOIN "SeasonMetadata" AS "sm" ON "sm"."SeasonId" = "s"."Id"
           WHERE "s"."ShowId" = ? AND "s"."SeasonNumber" = ?
           LIMIT 1"#,
    )
    .bind(show_id)
    .bind(season.season_number)
    .fetch_optional(&mut *tx)
    .await?
    .map(|row| (row.get("SeasonId"), row.get("SeasonMetadataId")));

    let (season_id, metadata_id) = if let Some((season_id, metadata_id)) = existing {
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path.id)
            .bind(season_id)
            .execute(&mut *tx)
            .await?;
        let metadata_id = if let Some(id) = metadata_id {
            sqlx::query(
                r#"UPDATE "SeasonMetadata"
                   SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
                       "SortTitle" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(&now)
            .bind(METADATA_KIND_FALLBACK)
            .bind(&title)
            .bind(&sort_title)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            id
        } else {
            insert_season_metadata(&mut tx, season_id, &title, &sort_title, &now).await?
        };
        (season_id, metadata_id)
    } else {
        let media_item_id = insert_media_item(&mut tx, library_path.id).await?;
        sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#)
            .bind(media_item_id)
            .bind(show_id)
            .bind(season.season_number)
            .execute(&mut *tx)
            .await?;
        let metadata_id =
            insert_season_metadata(&mut tx, media_item_id, &title, &sort_title, &now).await?;
        (media_item_id, metadata_id)
    };

    let mut artwork = Vec::new();
    if let Some(path) = locate_season_poster(show_folder, season.season_number) {
        artwork.push(artwork_ref(&path, artwork_kind::POSTER));
    } else if let Some(path) = locate_artwork(&season.folder, &["poster", "folder"]) {
        artwork.push(artwork_ref(&path, artwork_kind::POSTER));
    }
    if let Some(path) = locate_artwork(&season.folder, &["fanart"]) {
        artwork.push(artwork_ref(&path, artwork_kind::FAN_ART));
    }
    replace_artwork(
        &mut tx,
        "SeasonMetadataId",
        metadata_id,
        &[artwork_kind::POSTER, artwork_kind::FAN_ART],
        &artwork,
    )
    .await?;

    tx.commit().await?;
    Ok(season_id)
}

async fn insert_season_metadata(
    tx: &mut Transaction<'_, Db>,
    season_id: i64,
    title: &str,
    sort_title: &str,
    now: &str,
) -> Result<i64> {
    let result = sqlx::query(
        r#"INSERT INTO "SeasonMetadata" (
            "SeasonId", "DateAdded", "DateUpdated", "MetadataKind",
            "Title", "SortTitle"
        ) VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(season_id)
    .bind(now)
    .bind(now)
    .bind(METADATA_KIND_FALLBACK)
    .bind(title)
    .bind(sort_title)
    .execute(&mut **tx)
    .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

#[allow(clippy::too_many_lines)]
async fn sync_local_episode(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    season_id: i64,
    season_number: i32,
    library_folder_id: i64,
    video_path: &Path,
    extractor: &FfprobeExtractor,
) -> Result<()> {
    let probed = extractor.extract(video_path).await?;
    let nfo_path = locate_episode_nfo(video_path);
    let nfo = nfo_path
        .as_ref()
        .and_then(|path| match nfo::parse_episode_nfo(path) {
            Ok(parsed) => Some(parsed),
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to parse episode NFO; falling back"
                );
                None
            }
        });

    let basename = video_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_owned();
    let fallback_title = basename.clone();
    let fallback_episode = parse_episode_number_from_basename(&basename).unwrap_or(0);

    let title = nfo
        .as_ref()
        .and_then(|n| n.title.clone())
        .unwrap_or(fallback_title.clone());
    let sort_title = nfo::sort_title(&title);
    let episode_number = nfo
        .as_ref()
        .and_then(|n| n.episode)
        .unwrap_or(fallback_episode);
    let metadata_kind = if nfo.is_some() {
        METADATA_KIND_SIDECAR
    } else {
        METADATA_KIND_FALLBACK
    };
    let release_date = nfo
        .as_ref()
        .and_then(|n| n.aired.or(n.premiered))
        .map(|dt| dt.naive_utc().format("%Y-%m-%d").to_string());
    let year = nfo.as_ref().and_then(|n| {
        n.year.or_else(|| {
            n.aired
                .or(n.premiered)
                .and_then(|dt| dt.naive_utc().date().format("%Y").to_string().parse().ok())
        })
    });

    let mut tx = pool.begin().await?;
    let path_text = video_path.display().to_string();
    let hash = path_hash(&path_text);
    let now = encode_datetime(Utc::now());

    let existing = sqlx::query(
        r#"SELECT "mf"."MediaVersionId",
                  "mv"."EpisodeId" AS "MediaItemId"
           FROM "MediaFile" AS "mf"
           JOIN "MediaVersion" AS "mv" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "mf"."PathHash" = ?"#,
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;

    let (episode_id, version_id) = if let Some(row) = existing {
        let episode_id: i64 = row
            .get::<Option<i64>, _>("MediaItemId")
            .ok_or_else(|| ScannerError::Sqlx(sqlx::Error::RowNotFound))?;
        let version_id: i64 = row.get("MediaVersionId");
        sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
            .bind(season_id)
            .bind(episode_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"UPDATE "MediaItem" SET "LibraryPathId" = ?, "State" = 0 WHERE "Id" = ?"#)
            .bind(library_path.id)
            .bind(episode_id)
            .execute(&mut *tx)
            .await?;
        // ErsatzTV's `UpdateLibraryFolderId`: repair the head MediaFile's
        // season-folder link for restored/moved episodes.
        library_folder::update_media_file_library_folder_id(&mut tx, version_id, library_folder_id)
            .await?;
        (episode_id, version_id)
    } else {
        let media_item_id = insert_media_item(&mut tx, library_path.id).await?;
        sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
            .bind(media_item_id)
            .bind(season_id)
            .execute(&mut *tx)
            .await?;
        let version_id =
            insert_media_version_with_owner(&mut tx, media_item_id, &probed, "EpisodeId").await?;
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("LibraryFolderId", "MediaVersionId", "Path", "PathHash")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(library_folder_id)
        .bind(version_id)
        .bind(&path_text)
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
        (media_item_id, version_id)
    };

    update_media_version(&mut tx, version_id, &probed).await?;
    replace_streams(&mut tx, version_id, &probed.streams).await?;
    replace_chapters(&mut tx, version_id, &probed.chapters).await?;

    let metadata_id = upsert_episode_metadata(
        &mut tx,
        episode_id,
        season_number,
        &title,
        &fallback_title,
        &sort_title,
        episode_number,
        year,
        release_date.as_deref(),
        metadata_kind,
        nfo.as_ref(),
        &now,
    )
    .await?;

    if let Some(parsed) = nfo.as_ref() {
        replace_strings(
            &mut tx,
            "Genre",
            "EpisodeMetadataId",
            metadata_id,
            &parsed.genres,
        )
        .await?;
        replace_strings(
            &mut tx,
            "Tag",
            "EpisodeMetadataId",
            metadata_id,
            &parsed.tags,
        )
        .await?;
        replace_people(
            &mut tx,
            "Actor",
            "EpisodeMetadataId",
            metadata_id,
            &actors_from_nfo(&parsed.actors),
        )
        .await?;
        replace_people(
            &mut tx,
            "Director",
            "EpisodeMetadataId",
            metadata_id,
            &directors_from(&parsed.directors),
        )
        .await?;
        replace_people(
            &mut tx,
            "Writer",
            "EpisodeMetadataId",
            metadata_id,
            &directors_from(&parsed.writers),
        )
        .await?;
        replace_guids(
            &mut tx,
            "EpisodeMetadataId",
            metadata_id,
            &guids_from_nfo(&parsed.guids),
        )
        .await?;
    } else {
        replace_strings(&mut tx, "Genre", "EpisodeMetadataId", metadata_id, &[]).await?;
        replace_strings(&mut tx, "Tag", "EpisodeMetadataId", metadata_id, &[]).await?;
        replace_people(&mut tx, "Actor", "EpisodeMetadataId", metadata_id, &[]).await?;
        replace_people(&mut tx, "Director", "EpisodeMetadataId", metadata_id, &[]).await?;
        replace_people(&mut tx, "Writer", "EpisodeMetadataId", metadata_id, &[]).await?;
        replace_guids(&mut tx, "EpisodeMetadataId", metadata_id, &[]).await?;
    }

    let mut artwork = Vec::new();
    if let Some(path) = locate_thumbnail(video_path) {
        artwork.push(artwork_ref(&path, artwork_kind::THUMBNAIL));
    }
    replace_artwork(
        &mut tx,
        "EpisodeMetadataId",
        metadata_id,
        &[artwork_kind::THUMBNAIL],
        &artwork,
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_episode_metadata(
    tx: &mut Transaction<'_, Db>,
    episode_id: i64,
    _season_number: i32,
    title: &str,
    original_title: &str,
    sort_title: &str,
    episode_number: i32,
    year: Option<i32>,
    release_date: Option<&str>,
    metadata_kind: i32,
    nfo: Option<&EpisodeNfo>,
    now: &str,
) -> Result<i64> {
    let existing: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "EpisodeMetadata" WHERE "EpisodeId" = ? LIMIT 1"#)
            .bind(episode_id)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some(id) = existing {
        sqlx::query(
            r#"UPDATE "EpisodeMetadata"
               SET "DateUpdated" = ?, "MetadataKind" = ?, "Title" = ?,
                   "OriginalTitle" = ?, "SortTitle" = ?, "EpisodeNumber" = ?,
                   "Year" = ?, "ReleaseDate" = ?, "Plot" = ?, "Outline" = ?,
                   "Tagline" = ?
               WHERE "Id" = ?"#,
        )
        .bind(now)
        .bind(metadata_kind)
        .bind(title)
        .bind(original_title)
        .bind(sort_title)
        .bind(episode_number)
        .bind(year)
        .bind(release_date)
        .bind(nfo.and_then(|n| n.plot.clone()))
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(id)
        .execute(&mut **tx)
        .await?;
        Ok(id)
    } else {
        let result = sqlx::query(
            r#"INSERT INTO "EpisodeMetadata" (
                "EpisodeId", "DateAdded", "DateUpdated", "MetadataKind",
                "Title", "OriginalTitle", "SortTitle", "EpisodeNumber",
                "Year", "ReleaseDate", "Plot", "Outline", "Tagline"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(episode_id)
        .bind(now)
        .bind(now)
        .bind(metadata_kind)
        .bind(title)
        .bind(original_title)
        .bind(sort_title)
        .bind(episode_number)
        .bind(year)
        .bind(release_date)
        .bind(nfo.and_then(|n| n.plot.clone()))
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .execute(&mut **tx)
        .await?;
        Ok(ersatzrs_infra::db::last_insert_id(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_season_number_from_folder_name() {
        assert_eq!(
            parse_season_number_for_folder(Path::new("/x/Season 01")),
            Some(1)
        );
        assert_eq!(
            parse_season_number_for_folder(Path::new("/x/season12")),
            Some(12)
        );
        assert_eq!(parse_season_number_for_folder(Path::new("/x/S 3")), Some(3));
        assert_eq!(parse_season_number_for_folder(Path::new("/x/2")), Some(2));
        assert_eq!(
            parse_season_number_for_folder(Path::new("/x/Specials")),
            Some(0)
        );
        assert_eq!(
            parse_season_number_for_folder(Path::new("/x/Behind the Scenes")),
            None
        );
    }

    #[test]
    fn parses_episode_number_from_basename() {
        assert_eq!(
            parse_episode_number_from_basename("ShowName.S01E04.mkv"),
            Some(4)
        );
        assert_eq!(
            parse_episode_number_from_basename("ShowName - 1x12 - Title"),
            Some(12)
        );
        assert_eq!(
            parse_episode_number_from_basename("ShowName.s2e07.mp4"),
            Some(7)
        );
        assert_eq!(parse_episode_number_from_basename("randomfile"), None);
    }

    #[test]
    fn parses_year_from_show_folder_name() {
        assert_eq!(
            parse_year_from_folder_name("The Expanse (2015)"),
            Some(2015)
        );
        assert_eq!(parse_year_from_folder_name("Show.2020.4K"), Some(2020));
        assert_eq!(parse_year_from_folder_name("NoYearShow"), None);
    }
}
