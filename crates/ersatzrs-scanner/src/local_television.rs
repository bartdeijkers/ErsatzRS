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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use ersatzrs_core::metadata_facets::{
    normalize_content_ratings, normalize_language_codes, preferred_facet_values, union_facet_values,
};
use ersatzrs_domain::ExtraVideoClassification;
use ersatzrs_infra::db::{Db, DbPool};
use ersatzrs_infra::ffprobe::{FfprobeExtractor, FfprobeMediaVersion};
use ersatzrs_infra::metadata::{
    artwork_kind, replace_artwork, replace_guids, replace_metadata_languages, replace_people,
    replace_strings, update_local_file_subtitles, ArtworkRef, PersonRef, SubtitleMetadataOwner,
};
use sqlx::{Row, Transaction};
use tracing::{info, warn};

use crate::library_folder;
use crate::nfo::{self, EpisodeNfo, NfoActor, NfoGuid, TvShowNfo};
use crate::{
    audio_stream_languages, encode_datetime, insert_media_item, insert_media_version_with_owner,
    path_hash, replace_chapters, replace_streams, update_media_version, upsert_local_extra_video,
    EmbeddedFacets, LocalLibraryPath, Result, ScannerError,
};

pub mod discovery;

use discovery::collect_show_work;
pub(crate) const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tbn", "webp"];
const SQLITE_LOCK_RETRY_DELAYS_MS: &[u64] = &[250, 500, 1_000, 2_000, 4_000, 8_000];

const METADATA_KIND_FALLBACK: i32 = 0;
const METADATA_KIND_SIDECAR: i32 = 1;
const METADATA_KIND_EMBEDDED: i32 = 3;

/// One season folder discovered for a show.
struct SeasonWork {
    folder: PathBuf,
    season_number: i32,
    episode_files: Vec<PathBuf>,
    extra_files: Vec<ExtraVideoWork>,
}

/// One show folder discovered under a library root.
struct ShowWork {
    folder: PathBuf,
    seasons: Vec<SeasonWork>,
    extra_files: Vec<ExtraVideoWork>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalShowIdentity {
    normalized_title: String,
    year: Option<i32>,
    provider_guids: BTreeMap<String, String>,
    normalized_folder: String,
}

#[derive(Debug, Clone)]
struct LocalShowDescriptor {
    identity: LocalShowIdentity,
    folder_name: String,
    title: String,
    sort_title: String,
    metadata_kind: i32,
    nfo: Option<TvShowNfo>,
}

#[derive(Debug, Clone)]
struct OwnedEpisode {
    show_id: i64,
    season_id: i64,
    season_number: i32,
    episode_id: i64,
    season_folder_id: i64,
    normalized_show_folder: String,
}

#[derive(Debug, Clone, Default)]
struct ReconciliationStats {
    moved_episodes: usize,
    moved_extras: usize,
    created_shows: usize,
    created_seasons: usize,
}

struct ExtraVideoWork {
    path: PathBuf,
    extra: ExtraVideoClassification,
}

struct LocalEpisodeScan {
    metadata_complete: bool,
    probed: FfprobeMediaVersion,
    path_text: String,
    hash: String,
    title: String,
    fallback_title: String,
    sort_title: String,
    episode_number: i32,
    year: Option<i32>,
    release_date: Option<String>,
    content_rating: Option<String>,
    metadata_kind: i32,
    nfo: Option<EpisodeNfo>,
}

#[derive(Debug, PartialEq, Eq)]
struct EpisodeFacetValues {
    genres: Vec<String>,
    studios: Vec<String>,
    music_labels: Vec<String>,
    tags: Vec<String>,
    languages: Vec<String>,
}

fn episode_facet_values(scan: &LocalEpisodeScan) -> EpisodeFacetValues {
    let embedded = EmbeddedFacets::from_probe(&scan.probed);
    let preferred =
        |sidecar: Vec<String>, embedded: &[String]| preferred_facet_values(&sidecar, embedded, &[]);
    let genres = preferred(
        scan.nfo
            .as_ref()
            .map(|parsed| parsed.genres.clone())
            .unwrap_or_default(),
        &embedded.genres,
    );
    let studios = preferred(
        scan.nfo
            .as_ref()
            .map(|parsed| parsed.studios.clone())
            .unwrap_or_default(),
        &embedded.studios,
    );
    let music_labels = preferred(
        scan.nfo
            .as_ref()
            .map(|parsed| parsed.music_labels.clone())
            .unwrap_or_default(),
        &embedded.music_labels,
    );
    let tags = preferred(
        scan.nfo
            .as_ref()
            .map(|parsed| parsed.tags.clone())
            .unwrap_or_default(),
        &embedded.tags,
    );
    let explicit_languages = preferred(
        scan.nfo
            .as_ref()
            .map(|parsed| normalize_language_codes(&parsed.languages))
            .unwrap_or_default(),
        &embedded.languages,
    );
    let languages = union_facet_values(&explicit_languages, &audio_stream_languages(&scan.probed));
    EpisodeFacetValues {
        genres,
        studios,
        music_labels,
        tags,
        languages,
    }
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
pub(crate) fn locate_artwork<S: AsRef<str>>(folder: &Path, names: &[S]) -> Option<PathBuf> {
    for name in names {
        for ext in IMAGE_EXTENSIONS {
            let candidate = folder.join(format!("{}.{ext}", name.as_ref()));
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

pub(crate) fn artwork_ref(path: &Path, kind: i32) -> ArtworkRef {
    ArtworkRef {
        kind,
        path: path.display().to_string(),
        date_added: Utc::now(),
    }
}

pub(crate) fn guids_from_nfo(guids: &[NfoGuid]) -> Vec<String> {
    guids.iter().filter_map(NfoGuid::as_metadata_guid).collect()
}

fn normalize_identity_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn normalize_folder_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

fn normalize_persisted_guid(value: &str) -> Option<(String, String)> {
    let (provider, identifier) = value.split_once("://")?;
    let provider = provider.trim().to_lowercase();
    let identifier = identifier.trim().to_lowercase();
    if provider.is_empty() || identifier.is_empty() {
        return None;
    }
    Some((provider, identifier))
}

fn normalized_nfo_guids(guids: &[NfoGuid]) -> BTreeMap<String, String> {
    guids
        .iter()
        .filter_map(NfoGuid::as_metadata_guid)
        .filter_map(|guid| normalize_persisted_guid(&guid))
        .collect()
}

fn identities_conflict(left: &LocalShowIdentity, right: &LocalShowIdentity) -> bool {
    if left.year.is_some() && right.year.is_some() && left.year != right.year {
        return true;
    }

    left.provider_guids.iter().any(|(provider, value)| {
        right
            .provider_guids
            .get(provider)
            .is_some_and(|other| other != value)
    })
}

fn identities_match(left: &LocalShowIdentity, right: &LocalShowIdentity) -> bool {
    if identities_conflict(left, right) {
        return false;
    }

    if left.provider_guids.iter().any(|(provider, value)| {
        right
            .provider_guids
            .get(provider)
            .is_some_and(|other| other == value)
    }) {
        return true;
    }

    left.year.is_some()
        && left.year == right.year
        && left.normalized_title == right.normalized_title
}

#[cfg(test)]
fn should_reuse_show(
    exclusive_same_folder: bool,
    existing: &LocalShowIdentity,
    incoming: &LocalShowIdentity,
) -> bool {
    exclusive_same_folder || identities_match(existing, incoming)
}

fn describe_show(show: &ShowWork) -> LocalShowDescriptor {
    let nfo = locate_show_nfo(&show.folder).and_then(|path| match nfo::parse_tvshow_nfo(&path) {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            warn!(error = %err, "failed to parse tvshow.nfo; falling back");
            None
        }
    });
    let folder_name = show
        .folder
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    let title = nfo
        .as_ref()
        .and_then(|value| value.title.clone())
        .unwrap_or_else(|| folder_name.clone());
    let sort_title = nfo
        .as_ref()
        .and_then(|value| value.sort_title.clone())
        .unwrap_or_else(|| nfo::sort_title(&title));
    let year = nfo
        .as_ref()
        .and_then(|value| value.year)
        .or_else(|| parse_year_from_folder_name(&folder_name));
    let provider_guids = nfo
        .as_ref()
        .map_or_else(BTreeMap::new, |value| normalized_nfo_guids(&value.guids));

    LocalShowDescriptor {
        identity: LocalShowIdentity {
            normalized_title: normalize_identity_text(&title),
            year,
            provider_guids,
            normalized_folder: normalize_folder_path(&show.folder),
        },
        folder_name,
        title,
        sort_title,
        metadata_kind: if nfo.is_some() {
            METADATA_KIND_SIDECAR
        } else {
            METADATA_KIND_FALLBACK
        },
        nfo,
    }
}

pub(crate) fn actors_from_nfo(actors: &[NfoActor]) -> Vec<PersonRef> {
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

pub(crate) fn directors_from(values: &[String]) -> Vec<PersonRef> {
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

async fn load_owned_episodes(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
) -> Result<Vec<OwnedEpisode>> {
    let rows = sqlx::query(
        r#"SELECT season."ShowId", season."Id" AS "SeasonId",
                  season."SeasonNumber", episode."Id" AS "EpisodeId",
                  season_folder."Id" AS "SeasonFolderId",
                  show_folder."Path" AS "ShowFolderPath"
           FROM "Season" season
           JOIN "Episode" episode ON episode."SeasonId" = season."Id"
           JOIN "MediaItem" media ON media."Id" = episode."Id"
           JOIN "MediaVersion" version ON version."EpisodeId" = episode."Id"
           JOIN "MediaFile" file ON file."MediaVersionId" = version."Id"
           JOIN "LibraryFolder" season_folder ON season_folder."Id" = file."LibraryFolderId"
           JOIN "LibraryFolder" show_folder ON show_folder."Id" = season_folder."ParentId"
           WHERE media."LibraryPathId" = ?
           ORDER BY season."ShowId", season."Id", episode."Id", file."Id""#,
    )
    .bind(library_path_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut seen = BTreeSet::new();
    let mut episodes = Vec::new();
    for row in rows {
        let episode_id: i64 = row.get("EpisodeId");
        if !seen.insert(episode_id) {
            continue;
        }
        let show_folder_path: Option<String> = row.get("ShowFolderPath");
        let Some(show_folder_path) = show_folder_path else {
            continue;
        };
        episodes.push(OwnedEpisode {
            show_id: row.get("ShowId"),
            season_id: row.get("SeasonId"),
            season_number: row.get("SeasonNumber"),
            episode_id,
            season_folder_id: row.get("SeasonFolderId"),
            normalized_show_folder: normalize_folder_path(Path::new(&show_folder_path)),
        });
    }
    Ok(episodes)
}

async fn load_show_identities(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
) -> Result<BTreeMap<i64, LocalShowIdentity>> {
    let rows = sqlx::query(
        r#"SELECT s."Id" AS "ShowId", sm."Title", sm."Year", mg."Guid"
           FROM "Show" AS s
           JOIN "MediaItem" AS mi ON mi."Id" = s."Id"
           LEFT JOIN "ShowMetadata" AS sm ON sm."ShowId" = s."Id"
           LEFT JOIN "MetadataGuid" AS mg ON mg."ShowMetadataId" = sm."Id"
           WHERE mi."LibraryPathId" = ?
           ORDER BY s."Id", sm."Id", mg."Id""#,
    )
    .bind(library_path_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut identities = BTreeMap::new();
    for row in rows {
        let show_id: i64 = row.get("ShowId");
        let title = row.get::<Option<String>, _>("Title").unwrap_or_default();
        let year = row.get::<Option<i32>, _>("Year");
        let identity = identities
            .entry(show_id)
            .or_insert_with(|| LocalShowIdentity {
                normalized_title: normalize_identity_text(&title),
                year,
                provider_guids: BTreeMap::new(),
                normalized_folder: String::new(),
            });
        if let Some(guid) = row.get::<Option<String>, _>("Guid") {
            if let Some((provider, value)) = normalize_persisted_guid(&guid) {
                identity.provider_guids.insert(provider, value);
            }
        }
    }
    Ok(identities)
}

async fn find_identity_candidate(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
    identity: &LocalShowIdentity,
    excluded_show_id: Option<i64>,
) -> Result<Option<i64>> {
    let candidates = load_show_identities(tx, library_path_id).await?;
    Ok(candidates.into_iter().find_map(|(show_id, candidate)| {
        (Some(show_id) != excluded_show_id && identities_match(identity, &candidate))
            .then_some(show_id)
    }))
}

async fn resolve_show_id(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
    identity: &LocalShowIdentity,
) -> Result<Option<i64>> {
    let owned = load_owned_episodes(tx, library_path_id).await?;
    let mut folders_by_show: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
    for episode in owned {
        folders_by_show
            .entry(episode.show_id)
            .or_default()
            .insert(episode.normalized_show_folder);
    }
    let extra_rows = sqlx::query(
        r#"SELECT COALESCE(parent_show."Id", season."ShowId") AS "ShowId", file."Path"
           FROM "MediaItem" extra
           LEFT JOIN "Show" parent_show ON parent_show."Id" = extra."ParentMediaItemId"
           LEFT JOIN "Season" season ON season."Id" = extra."ParentMediaItemId"
           JOIN "OtherVideo" video ON video."Id" = extra."Id"
           JOIN "MediaVersion" version ON version."OtherVideoId" = video."Id"
           JOIN "MediaFile" file ON file."MediaVersionId" = version."Id"
           WHERE extra."LibraryPathId" = ? AND extra."ContentKind" = 1
             AND COALESCE(parent_show."Id", season."ShowId") IS NOT NULL
           ORDER BY "ShowId", extra."Id", file."Id""#,
    )
    .bind(library_path_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut extras_by_show: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for row in extra_rows {
        if let Some(path) = row.get::<Option<String>, _>("Path") {
            extras_by_show
                .entry(row.get("ShowId"))
                .or_default()
                .push(normalize_folder_path(Path::new(&path)));
        }
    }
    let mut folder_candidates = folders_by_show.keys().copied().collect::<BTreeSet<_>>();
    folder_candidates.extend(extras_by_show.keys().copied());
    for show_id in folder_candidates {
        let episode_folders = folders_by_show.get(&show_id);
        let episodes_match = episode_folders.is_none_or(|folders| {
            folders.len() == 1 && folders.contains(&identity.normalized_folder)
        });
        let extra_paths = extras_by_show.get(&show_id);
        let extras_match = extra_paths.is_none_or(|paths| {
            paths.iter().all(|path| {
                path == &identity.normalized_folder
                    || path.starts_with(&format!("{}/", identity.normalized_folder))
            })
        });
        if episodes_match && extras_match && (episode_folders.is_some() || extra_paths.is_some()) {
            return Ok(Some(show_id));
        }
    }

    find_identity_candidate(tx, library_path_id, identity, None).await
}

fn descriptor_group_for_path(path: &str, folder_groups: &HashMap<String, usize>) -> Option<usize> {
    let normalized = normalize_folder_path(Path::new(path));
    folder_groups.iter().find_map(|(folder, group)| {
        (normalized == *folder || normalized.starts_with(&format!("{folder}/"))).then_some(*group)
    })
}

async fn create_show_for_descriptor(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
    descriptor: &LocalShowDescriptor,
) -> Result<i64> {
    let show_id = insert_media_item(tx, library_path_id).await?;
    sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
        .bind(show_id)
        .execute(&mut **tx)
        .await?;
    let now = encode_datetime(Utc::now());
    let release_date = descriptor
        .identity
        .year
        .map(|year| format!("{year:04}-01-01"));
    let metadata_id = insert_show_metadata(
        tx,
        show_id,
        &descriptor.title,
        &descriptor.folder_name,
        &descriptor.sort_title,
        descriptor.identity.year,
        release_date.as_deref(),
        descriptor.metadata_kind,
        descriptor.nfo.as_ref(),
        &now,
    )
    .await?;
    let guids = descriptor
        .nfo
        .as_ref()
        .map_or_else(Vec::new, |value| guids_from_nfo(&value.guids));
    replace_guids(tx, "ShowMetadataId", metadata_id, &guids).await?;
    Ok(show_id)
}

async fn find_or_create_target_season(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
    show_id: i64,
    season_number: i32,
    source_season_id: i64,
    stats: &mut ReconciliationStats,
) -> Result<i64> {
    if let Some(season_id) = sqlx::query_scalar(
        r#"SELECT "Id" FROM "Season"
           WHERE "ShowId" = ? AND "SeasonNumber" = ?
           ORDER BY "Id" LIMIT 1"#,
    )
    .bind(show_id)
    .bind(season_number)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(season_id);
    }

    let season_id = insert_media_item(tx, library_path_id).await?;
    sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#)
        .bind(season_id)
        .bind(show_id)
        .bind(season_number)
        .execute(&mut **tx)
        .await?;
    let cloned = sqlx::query(
        r#"INSERT INTO "SeasonMetadata" (
              "Outline", "SeasonId", "MetadataKind", "Title", "OriginalTitle",
              "SortTitle", "ReleaseDate", "DateAdded", "DateUpdated", "Year"
           )
           SELECT "Outline", ?, "MetadataKind", "Title", "OriginalTitle",
                  "SortTitle", "ReleaseDate", "DateAdded", "DateUpdated", "Year"
           FROM "SeasonMetadata" WHERE "SeasonId" = ?
           ORDER BY "Id" LIMIT 1"#,
    )
    .bind(season_id)
    .bind(source_season_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if cloned == 0 {
        let now = encode_datetime(Utc::now());
        let title = if season_number == 0 {
            "Specials".to_owned()
        } else {
            format!("Season {season_number}")
        };
        let sort_title = format!("{season_number:02}");
        insert_season_metadata(tx, season_id, &title, &sort_title, &now).await?;
    }
    stats.created_seasons += 1;
    Ok(season_id)
}

#[allow(clippy::too_many_lines)]
async fn reconcile_one_merged_show(
    pool: &DbPool,
    library_path_id: i64,
    merged_show_id: i64,
    episodes: &[OwnedEpisode],
    descriptors: &[&LocalShowDescriptor],
    canonical_folder: &str,
    inject_failure: bool,
) -> Result<ReconciliationStats> {
    let mut identity_groups: Vec<&LocalShowDescriptor> = Vec::new();
    let mut folder_groups = HashMap::new();
    for descriptor in descriptors {
        let group = identity_groups
            .iter()
            .position(|candidate| identities_match(&descriptor.identity, &candidate.identity))
            .unwrap_or_else(|| {
                identity_groups.push(descriptor);
                identity_groups.len() - 1
            });
        folder_groups.insert(descriptor.identity.normalized_folder.clone(), group);
    }
    if identity_groups.len() < 2 {
        return Ok(ReconciliationStats::default());
    }
    let canonical_group = *folder_groups.get(canonical_folder).ok_or_else(|| {
        ScannerError::Reconciliation("canonical folder has no identity".to_owned())
    })?;

    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    let mut stats = ReconciliationStats::default();
    let mut target_shows = BTreeMap::from([(canonical_group, merged_show_id)]);
    for (group, descriptor) in identity_groups.iter().enumerate() {
        if group == canonical_group {
            continue;
        }
        let show_id = if let Some(existing) = find_identity_candidate(
            &mut tx,
            library_path_id,
            &descriptor.identity,
            Some(merged_show_id),
        )
        .await?
        {
            existing
        } else {
            stats.created_shows += 1;
            create_show_for_descriptor(&mut tx, library_path_id, descriptor).await?
        };
        target_shows.insert(group, show_id);
    }

    let mut by_season: BTreeMap<i64, Vec<&OwnedEpisode>> = BTreeMap::new();
    for episode in episodes {
        by_season
            .entry(episode.season_id)
            .or_default()
            .push(episode);
    }
    let mut target_seasons: HashMap<(i64, usize), i64> = HashMap::new();
    for (source_season_id, season_episodes) in &by_season {
        let season_number = season_episodes[0].season_number;
        let mut episode_groups: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
        for episode in season_episodes {
            let group = *folder_groups
                .get(&episode.normalized_show_folder)
                .ok_or_else(|| {
                    ScannerError::Reconciliation("episode folder has no identity".to_owned())
                })?;
            episode_groups
                .entry(group)
                .or_default()
                .push(episode.episode_id);
        }

        if episode_groups.len() == 1 {
            let group = *episode_groups.keys().next().expect("one group");
            if group == canonical_group {
                target_seasons.insert((*source_season_id, group), *source_season_id);
            } else {
                let target_show_id = target_shows[&group];
                let existing_target: Option<i64> = sqlx::query_scalar(
                    r#"SELECT "Id" FROM "Season"
                       WHERE "ShowId" = ? AND "SeasonNumber" = ? AND "Id" <> ?
                       ORDER BY "Id" LIMIT 1"#,
                )
                .bind(target_show_id)
                .bind(season_number)
                .bind(source_season_id)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some(target_season_id) = existing_target {
                    for episode_id in &episode_groups[&group] {
                        sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
                            .bind(target_season_id)
                            .bind(episode_id)
                            .execute(&mut *tx)
                            .await?;
                        stats.moved_episodes += 1;
                    }
                    target_seasons.insert((*source_season_id, group), target_season_id);
                } else {
                    sqlx::query(r#"UPDATE "Season" SET "ShowId" = ? WHERE "Id" = ?"#)
                        .bind(target_show_id)
                        .bind(source_season_id)
                        .execute(&mut *tx)
                        .await?;
                    stats.moved_episodes += episode_groups[&group].len();
                    target_seasons.insert((*source_season_id, group), *source_season_id);
                }
            }
            continue;
        }

        target_seasons.insert((*source_season_id, canonical_group), *source_season_id);
        for (group, episode_ids) in episode_groups {
            if group == canonical_group {
                continue;
            }
            let target_season_id = find_or_create_target_season(
                &mut tx,
                library_path_id,
                target_shows[&group],
                season_number,
                *source_season_id,
                &mut stats,
            )
            .await?;
            for episode_id in episode_ids {
                sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
                    .bind(target_season_id)
                    .bind(episode_id)
                    .execute(&mut *tx)
                    .await?;
                stats.moved_episodes += 1;
            }
            target_seasons.insert((*source_season_id, group), target_season_id);
        }
    }

    let source_season_ids = by_season.keys().copied().collect::<BTreeSet<_>>();
    let extra_rows = sqlx::query(
        r#"SELECT media."Id", media."ParentMediaItemId", file."Path"
           FROM "MediaItem" media
           JOIN "OtherVideo" video ON video."Id" = media."Id"
           JOIN "MediaVersion" version ON version."OtherVideoId" = video."Id"
           JOIN "MediaFile" file ON file."MediaVersionId" = version."Id"
           WHERE media."ContentKind" = 1 AND media."LibraryPathId" = ?
           ORDER BY media."Id", file."Id""#,
    )
    .bind(library_path_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut seen_extras = BTreeSet::new();
    for row in extra_rows {
        let extra_id: i64 = row.get("Id");
        if !seen_extras.insert(extra_id) {
            continue;
        }
        let parent_id: Option<i64> = row.get("ParentMediaItemId");
        if parent_id != Some(merged_show_id)
            && !parent_id.is_some_and(|id| source_season_ids.contains(&id))
        {
            continue;
        }
        let Some(path) = row.get::<Option<String>, _>("Path") else {
            continue;
        };
        let Some(group) = descriptor_group_for_path(&path, &folder_groups) else {
            continue;
        };
        let new_parent = if parent_id == Some(merged_show_id) {
            target_shows[&group]
        } else {
            target_seasons
                .get(&(parent_id.expect("season parent"), group))
                .copied()
                .unwrap_or(parent_id.expect("season parent"))
        };
        if Some(new_parent) != parent_id {
            sqlx::query(r#"UPDATE "MediaItem" SET "ParentMediaItemId" = ? WHERE "Id" = ?"#)
                .bind(new_parent)
                .bind(extra_id)
                .execute(&mut *tx)
                .await?;
            stats.moved_extras += 1;
        }
    }

    let folder_ids = episodes
        .iter()
        .map(|episode| episode.season_folder_id)
        .collect::<BTreeSet<_>>();
    for folder_id in folder_ids {
        sqlx::query(r#"UPDATE "LibraryFolder" SET "Etag" = NULL WHERE "Id" = ?"#)
            .bind(folder_id)
            .execute(&mut *tx)
            .await?;
    }
    let folder_rows = sqlx::query(
        r#"SELECT parent."Path", child."Id" AS "SeasonFolderId"
           FROM "LibraryFolder" parent
           JOIN "LibraryFolder" child ON child."ParentId" = parent."Id"
           WHERE parent."LibraryPathId" = ?"#,
    )
    .bind(library_path_id)
    .fetch_all(&mut *tx)
    .await?;
    for row in folder_rows {
        let Some(path) = row.get::<Option<String>, _>("Path") else {
            continue;
        };
        if folder_groups.contains_key(&normalize_folder_path(Path::new(&path))) {
            sqlx::query(r#"UPDATE "LibraryFolder" SET "Etag" = NULL WHERE "Id" = ?"#)
                .bind(row.get::<i64, _>("SeasonFolderId"))
                .execute(&mut *tx)
                .await?;
        }
    }

    if inject_failure {
        return Err(ScannerError::Reconciliation(
            "injected rollback after hierarchy updates".to_owned(),
        ));
    }
    tx.commit().await?;
    info!(
        library_path_id,
        retained_show_id = merged_show_id,
        created_shows = stats.created_shows,
        created_seasons = stats.created_seasons,
        moved_episodes = stats.moved_episodes,
        moved_extras = stats.moved_extras,
        "reconciled merged local television identities"
    );
    Ok(stats)
}

async fn reconcile_merged_local_shows(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    shows: &[(ShowWork, LocalShowDescriptor)],
    inject_failure: bool,
) -> Result<()> {
    let descriptors_by_folder = shows
        .iter()
        .map(|(_, descriptor)| (descriptor.identity.normalized_folder.clone(), descriptor))
        .collect::<HashMap<_, _>>();
    // Read-only transaction: this only takes a consistent snapshot of the owned
    // episodes before reconciling them one show at a time. Keep the deferred
    // `begin()` here — `begin_write` would take the write lock and serialise
    // readers that WAL otherwise runs concurrently. See `ersatzrs_infra::db::begin_write`.
    let mut tx = pool.begin().await?;
    let owned = load_owned_episodes(&mut tx, library_path.id).await?;
    tx.commit().await?;

    let mut by_show: BTreeMap<i64, Vec<OwnedEpisode>> = BTreeMap::new();
    for episode in owned {
        if descriptors_by_folder.contains_key(&episode.normalized_show_folder) {
            by_show.entry(episode.show_id).or_default().push(episode);
        }
    }
    for (show_id, episodes) in by_show {
        let folders = episodes
            .iter()
            .map(|episode| episode.normalized_show_folder.clone())
            .collect::<BTreeSet<_>>();
        if folders.len() < 2 {
            continue;
        }
        let descriptors = folders
            .iter()
            .filter_map(|folder| descriptors_by_folder.get(folder).copied())
            .collect::<Vec<_>>();
        let has_conflict = descriptors.iter().enumerate().any(|(index, left)| {
            descriptors
                .iter()
                .skip(index + 1)
                .any(|right| !identities_match(&left.identity, &right.identity))
        });
        if !has_conflict {
            continue;
        }

        let mut ranked = folders
            .iter()
            .map(|folder| {
                let folder_episodes = episodes
                    .iter()
                    .filter(|episode| &episode.normalized_show_folder == folder)
                    .collect::<Vec<_>>();
                let season_count = folder_episodes
                    .iter()
                    .map(|episode| episode.season_id)
                    .collect::<BTreeSet<_>>()
                    .len();
                (folder.clone(), folder_episodes.len(), season_count)
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| right.2.cmp(&left.2))
                .then_with(|| left.0.cmp(&right.0))
        });
        let canonical_folder = &ranked[0].0;
        reconcile_one_merged_show(
            pool,
            library_path.id,
            show_id,
            &episodes,
            &descriptors,
            canonical_folder,
            inject_failure,
        )
        .await?;
    }
    Ok(())
}

async fn cleanup_empty_local_television(pool: &DbPool, library_path_id: i64) -> Result<()> {
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    let empty_seasons: Vec<i64> = sqlx::query_scalar(
        r#"SELECT season."Id"
           FROM "Season" season
           JOIN "MediaItem" media ON media."Id" = season."Id"
           WHERE media."LibraryPathId" = ?
             AND NOT EXISTS (SELECT 1 FROM "Episode" episode WHERE episode."SeasonId" = season."Id")
             AND NOT EXISTS (
                 SELECT 1 FROM "MediaItem" extra
                 WHERE extra."ParentMediaItemId" = season."Id" AND extra."ContentKind" = 1
             )
           ORDER BY season."Id""#,
    )
    .bind(library_path_id)
    .fetch_all(&mut *tx)
    .await?;
    for season_id in empty_seasons {
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(season_id)
            .execute(&mut *tx)
            .await?;
    }

    let empty_shows: Vec<i64> = sqlx::query_scalar(
        r#"SELECT s."Id"
           FROM "Show" AS s
           JOIN "MediaItem" AS media ON media."Id" = s."Id"
           WHERE media."LibraryPathId" = ?
             AND NOT EXISTS (SELECT 1 FROM "Season" season WHERE season."ShowId" = s."Id")
             AND NOT EXISTS (
                 SELECT 1 FROM "MediaItem" extra
                 WHERE extra."ParentMediaItemId" = s."Id" AND extra."ContentKind" = 1
             )
           ORDER BY s."Id""#,
    )
    .bind(library_path_id)
    .fetch_all(&mut *tx)
    .await?;
    for show_id in empty_shows {
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(show_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Sync one library path of `LibraryMediaKind::Shows`. Returns the number of
/// episode files processed so the caller can roll the total scan report.
/// `on_episode(count)` advances scan progress by `count` episode files. It is
/// called once per scanned episode (`count == 1`) and once per season skipped
/// by the etag fast path (`count == skipped episodes`), so the progress sink
/// still reaches 100% even though skipped episodes are not probed. Skipped
/// episodes do not contribute to the returned count.
#[allow(clippy::too_many_lines)]
pub(crate) async fn sync_local_show_library(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    extractor: &FfprobeExtractor,
    deep_scan: bool,
    mut on_episode: impl FnMut(usize),
) -> Result<usize> {
    let repaired_aliases =
        library_folder::reconcile_television_folder_aliases(pool, library_path.id).await?;
    if repaired_aliases != 0 {
        info!(
            library_path_id = library_path.id,
            repaired_aliases, "reconciled case-only local television folder aliases"
        );
    }
    let (shows, discovery) = discovery::collect_show_work_with_report(&library_path.path);
    info!(
        library_path_id = library_path.id,
        inspected_entries = discovery.inspected_entries,
        shows = discovery.shows,
        seasons = discovery.seasons,
        episodes = discovery.episodes,
        extras = discovery.extras,
        hidden_entries = discovery.hidden_entries,
        issues = ?discovery.issue_counts,
        "television discovery completed"
    );
    let shows = shows
        .into_iter()
        .map(|show| {
            let descriptor = describe_show(&show);
            (show, descriptor)
        })
        .collect::<Vec<_>>();
    let mut episode_count = 0usize;

    reconcile_merged_local_shows(pool, library_path, &shows, false).await?;

    // Load trashed episode paths once per library path, like ErsatzTV's
    // `GetAllTrashedItems` call at the top of `ScanFolder`. Lowercased so the
    // per-season prefix check can be case-insensitive, matching ErsatzTV's
    // `StartsWith(..., StringComparison.OrdinalIgnoreCase)`.
    let trashed_paths: Vec<String> = trashed_episode_paths(pool, library_path.id)
        .await?
        .into_iter()
        .map(|path| path.to_lowercase())
        .collect();

    for (show, descriptor) in shows {
        // Create the show LibraryFolder before the show upsert, mirroring
        // ErsatzTV's `ScanFolder`. The show item does not use this row
        // itself; it exists so season folders can point at it via ParentId.
        if ensure_library_folder(pool, library_path.id, &show.folder, "show")
            .await
            .is_none()
        {
            continue;
        }

        let show_id = match upsert_show(pool, library_path, &show, &descriptor).await {
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

        for extra in &show.extra_files {
            match sync_extra_video(pool, library_path, show_id, extra, extractor).await {
                Ok(()) => {
                    episode_count += 1;
                    on_episode(1);
                }
                Err(err) => {
                    warn!(
                        path = %extra.path.display(),
                        error = %err,
                        "failed to upsert show extra; skipping"
                    );
                }
            }
        }

        for season in &show.seasons {
            // Create the season LibraryFolder before the season upsert,
            // mirroring ErsatzTV's `ScanSeasons`. Episodes link their
            // `MediaFile` to this row.
            let Some(season_folder) =
                ensure_library_folder(pool, library_path.id, &season.folder, "season").await
            else {
                continue;
            };

            // Previously cached folders may already contain the newly supported
            // MPEG/TS files. Invalidate the old discovery contract once, even
            // when no source mtime changed; subsequent scans keep the fast path.
            let current_etag = format!(
                "tv-discovery-v2:{}",
                crate::folder_etag::folder_etag_with_subfolders(&season.folder)
            );
            // ErsatzTV row195: a deep scan refreshes even unchanged seasons.
            let etag_match = !deep_scan
                && season_etag_match(
                    season_folder.etag.as_deref(),
                    &current_etag,
                    &season.folder,
                    &trashed_paths,
                );

            // Reprocessing invalidates the previous completion checkpoint
            // before any work can fail or be interrupted. In particular, a
            // deep scan with unchanged mtimes must not leave an old matching
            // etag that hides its incomplete episode metadata on the next scan.
            if !etag_match && season_folder.etag.is_some() {
                sqlx::query(r#"UPDATE "LibraryFolder" SET "Etag" = NULL WHERE "Id" = ?"#)
                    .bind(season_folder.id)
                    .execute(pool)
                    .await?;
            }

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

            let mut complete = true;
            for extra in &season.extra_files {
                match sync_extra_video(pool, library_path, season_id, extra, extractor).await {
                    Ok(()) => {
                        episode_count += 1;
                        on_episode(1);
                    }
                    Err(err) => {
                        warn!(
                            path = %extra.path.display(),
                            error = %err,
                            "failed to upsert season extra; skipping"
                        );
                        complete = false;
                    }
                }
            }

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
                    Ok(metadata_complete) => {
                        complete &= metadata_complete;
                        episode_count += 1;
                        on_episode(1);
                    }
                    Err(err) => {
                        warn!(
                            path = %episode_path.display(),
                            error = %err,
                            "failed to upsert episode; skipping"
                        );
                        complete = false;
                    }
                }
            }

            // Like TelevisionFolderScanner.ScanEpisodes, publish only after
            // every episode succeeds. Metadata fallback remains playable, but
            // a failed sidecar must be retried even if its mtime never changes.
            if !complete {
                continue;
            }
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

    cleanup_empty_local_television(pool, library_path.id).await?;

    Ok(episode_count)
}

/// Count how many episode files exist under a library root. Used by the
/// progress sink so the fraction reflects the same units as the worker loop.
pub(crate) fn count_episode_files(root: &str) -> usize {
    collect_show_work(root)
        .iter()
        .map(|show| {
            show.extra_files.len()
                + show
                    .seasons
                    .iter()
                    .map(|s| s.episode_files.len() + s.extra_files.len())
                    .sum::<usize>()
        })
        .sum()
}

async fn sync_extra_video(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    parent_media_item_id: i64,
    extra: &ExtraVideoWork,
    extractor: &FfprobeExtractor,
) -> Result<()> {
    let probed = extractor.extract(&extra.path).await?;
    upsert_local_extra_video(
        pool,
        library_path,
        &extra.path,
        probed,
        &extra.extra.sub_kind,
        Some(parent_media_item_id),
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn upsert_show(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    show: &ShowWork,
    descriptor: &LocalShowDescriptor,
) -> Result<i64> {
    let nfo = descriptor.nfo.as_ref();
    let folder_name = &descriptor.folder_name;
    let title = &descriptor.title;
    let sort_title = &descriptor.sort_title;
    let year = descriptor.identity.year;
    let metadata_kind = descriptor.metadata_kind;
    let content_rating = nfo.and_then(|value| {
        let values = normalize_content_ratings(&value.content_ratings);
        (!values.is_empty()).then(|| values.join("/"))
    });

    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    let existing_show_id = resolve_show_id(&mut tx, library_path.id, &descriptor.identity).await?;
    let existing = if let Some(show_id) = existing_show_id {
        sqlx::query(
            r#"SELECT "s"."Id" AS "ShowId", "sm"."Id" AS "ShowMetadataId"
               FROM "Show" AS "s"
               LEFT JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
               WHERE "s"."Id" = ?
               ORDER BY "sm"."Id"
               LIMIT 1"#,
        )
        .bind(show_id)
        .fetch_optional(&mut *tx)
        .await?
    } else {
        None
    };

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
            .bind(title)
            .bind(folder_name)
            .bind(sort_title)
            .bind(year)
            .bind(&release_date)
            .bind(&content_rating)
            .bind(nfo.and_then(|n| n.plot.clone()))
            .bind(nfo.and_then(|n| n.outline.clone()))
            .bind(nfo.and_then(|n| n.tagline.clone()))
            .bind(id)
            .execute(&mut *tx)
            .await?;
            id
        } else {
            insert_show_metadata(
                &mut tx,
                show_id,
                title,
                folder_name,
                sort_title,
                year,
                release_date.as_deref(),
                metadata_kind,
                nfo,
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
            title,
            folder_name,
            sort_title,
            year,
            release_date.as_deref(),
            metadata_kind,
            nfo,
            &now,
        )
        .await?;
        (media_item_id, metadata_id)
    };

    if let Some(parsed) = nfo {
        replace_strings(
            &mut tx,
            "Genre",
            "ShowMetadataId",
            metadata_id,
            &parsed.genres,
        )
        .await?;
        replace_strings(
            &mut tx,
            "MusicLabel",
            "ShowMetadataId",
            metadata_id,
            &parsed.music_labels,
        )
        .await?;
        replace_metadata_languages(
            &mut tx,
            "ShowMetadataId",
            metadata_id,
            &normalize_language_codes(&parsed.languages),
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
        replace_strings(&mut tx, "MusicLabel", "ShowMetadataId", metadata_id, &[]).await?;
        replace_metadata_languages(&mut tx, "ShowMetadataId", metadata_id, &[]).await?;
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
    .bind(nfo.and_then(|value| {
        let values = normalize_content_ratings(&value.content_ratings);
        (!values.is_empty()).then(|| values.join("/"))
    }))
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
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
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
) -> Result<bool> {
    let scan = probe_local_episode(video_path, extractor).await?;

    for (attempt, delay_ms) in SQLITE_LOCK_RETRY_DELAYS_MS.iter().copied().enumerate() {
        match sync_local_episode_once(
            pool,
            library_path,
            season_id,
            season_number,
            library_folder_id,
            video_path,
            &scan,
        )
        .await
        {
            Ok(()) => return Ok(scan.metadata_complete),
            Err(err) if is_sqlite_lock_error(&err) => {
                warn!(
                    path = %video_path.display(),
                    attempt = attempt + 1,
                    retry_delay_ms = delay_ms,
                    error = %err,
                    "database locked while upserting episode; retrying"
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            Err(err) => return Err(err),
        }
    }

    sync_local_episode_once(
        pool,
        library_path,
        season_id,
        season_number,
        library_folder_id,
        video_path,
        &scan,
    )
    .await?;
    Ok(scan.metadata_complete)
}

async fn probe_local_episode(
    video_path: &Path,
    extractor: &FfprobeExtractor,
) -> Result<LocalEpisodeScan> {
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
    let embedded = EmbeddedFacets::from_probe(&probed);
    let metadata_kind = if nfo.is_some() {
        METADATA_KIND_SIDECAR
    } else if !embedded.is_empty() {
        METADATA_KIND_EMBEDDED
    } else {
        METADATA_KIND_FALLBACK
    };
    let content_ratings = preferred_facet_values(
        &nfo.as_ref()
            .map(|value| normalize_content_ratings(&value.content_ratings))
            .unwrap_or_default(),
        &embedded.content_ratings,
        &[],
    );
    let content_rating = (!content_ratings.is_empty()).then(|| content_ratings.join("/"));
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
    let path_text = video_path.display().to_string();
    let hash = path_hash(&path_text);

    Ok(LocalEpisodeScan {
        metadata_complete: nfo_path.is_none() || nfo.is_some(),
        probed,
        path_text,
        hash,
        title,
        fallback_title,
        sort_title,
        episode_number,
        year,
        release_date,
        content_rating,
        metadata_kind,
        nfo,
    })
}

async fn find_existing_local_episode(
    tx: &mut Transaction<'_, Db>,
    library_path_id: i64,
    path: &str,
    hash: &str,
    case_insensitive: bool,
) -> Result<Option<sqlx::sqlite::SqliteRow>> {
    let exact = sqlx::query(
        r#"SELECT mf."Id" AS "MediaFileId", mf."MediaVersionId", mv."EpisodeId" AS "MediaItemId"
           FROM "MediaFile" mf
           JOIN "MediaVersion" mv ON mv."Id" = mf."MediaVersionId"
           JOIN "MediaItem" mi ON mi."Id" = mv."EpisodeId"
           WHERE mf."PathHash" = ? AND mi."LibraryPathId" = ?
           ORDER BY mf."Id" LIMIT 1"#,
    )
    .bind(hash)
    .bind(library_path_id)
    .fetch_optional(&mut **tx)
    .await?;
    if exact.is_some() || !case_insensitive {
        return Ok(exact);
    }
    let aliases = sqlx::query(
        r#"SELECT mf."Id" AS "MediaFileId", mf."MediaVersionId", mv."EpisodeId" AS "MediaItemId", mf."Path"
           FROM "MediaFile" mf
           JOIN "MediaVersion" mv ON mv."Id" = mf."MediaVersionId"
           JOIN "MediaItem" mi ON mi."Id" = mv."EpisodeId"
           WHERE mi."LibraryPathId" = ? AND mf."Path" = ? COLLATE NOCASE
           ORDER BY mf."Id""#,
    )
    .bind(library_path_id)
    .bind(path)
    .fetch_all(&mut **tx)
    .await?;
    Ok(aliases.into_iter().find(|row| {
        row.get::<Option<String>, _>("Path")
            .is_some_and(|stored| library_folder::same_local_path(&stored, path))
    }))
}

#[allow(clippy::too_many_lines)]
async fn sync_local_episode_once(
    pool: &DbPool,
    library_path: &LocalLibraryPath,
    season_id: i64,
    season_number: i32,
    library_folder_id: i64,
    video_path: &Path,
    scan: &LocalEpisodeScan,
) -> Result<()> {
    let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
    let now = encode_datetime(Utc::now());

    let existing = find_existing_local_episode(
        &mut tx,
        library_path.id,
        &scan.path_text,
        &scan.hash,
        library_folder::case_insensitive_paths(),
    )
    .await?;

    let (episode_id, version_id) = if let Some(row) = existing {
        let episode_id: i64 = row
            .get::<Option<i64>, _>("MediaItemId")
            .ok_or_else(|| ScannerError::Sqlx(sqlx::Error::RowNotFound))?;
        let version_id: i64 = row.get("MediaVersionId");
        sqlx::query(r#"UPDATE "MediaFile" SET "Path" = ?, "PathHash" = ? WHERE "Id" = ?"#)
            .bind(&scan.path_text)
            .bind(&scan.hash)
            .bind(row.get::<i64, _>("MediaFileId"))
            .execute(&mut *tx)
            .await?;
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
            insert_media_version_with_owner(&mut tx, media_item_id, &scan.probed, "EpisodeId")
                .await?;
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("LibraryFolderId", "MediaVersionId", "Path", "PathHash")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(library_folder_id)
        .bind(version_id)
        .bind(&scan.path_text)
        .bind(&scan.hash)
        .execute(&mut *tx)
        .await?;
        (media_item_id, version_id)
    };

    update_media_version(&mut tx, version_id, &scan.probed).await?;
    replace_streams(&mut tx, version_id, &scan.probed.streams).await?;
    replace_chapters(&mut tx, version_id, &scan.probed.chapters).await?;

    let metadata_id = upsert_episode_metadata(
        &mut tx,
        episode_id,
        season_number,
        &scan.title,
        &scan.fallback_title,
        &scan.sort_title,
        scan.episode_number,
        scan.year,
        scan.release_date.as_deref(),
        scan.content_rating.as_deref(),
        scan.metadata_kind,
        scan.nfo.as_ref(),
        &now,
    )
    .await?;

    let facets = episode_facet_values(scan);
    replace_strings(
        &mut tx,
        "Genre",
        "EpisodeMetadataId",
        metadata_id,
        &facets.genres,
    )
    .await?;
    replace_strings(
        &mut tx,
        "Studio",
        "EpisodeMetadataId",
        metadata_id,
        &facets.studios,
    )
    .await?;
    replace_strings(
        &mut tx,
        "MusicLabel",
        "EpisodeMetadataId",
        metadata_id,
        &facets.music_labels,
    )
    .await?;
    replace_strings(
        &mut tx,
        "Tag",
        "EpisodeMetadataId",
        metadata_id,
        &facets.tags,
    )
    .await?;
    replace_metadata_languages(&mut tx, "EpisodeMetadataId", metadata_id, &facets.languages)
        .await?;

    if let Some(parsed) = scan.nfo.as_ref() {
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
    update_local_file_subtitles(
        &mut tx,
        SubtitleMetadataOwner::Episode(metadata_id),
        &scan.probed.streams,
        video_path,
        true,
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

fn is_sqlite_lock_error(err: &ScannerError) -> bool {
    let ScannerError::Sqlx(sqlx::Error::Database(database_error)) = err else {
        return false;
    };

    if database_error
        .code()
        .as_deref()
        .is_some_and(is_sqlite_lock_code)
    {
        return true;
    }

    database_error
        .message()
        .to_ascii_lowercase()
        .contains("database is locked")
}

fn is_sqlite_lock_code(code: &str) -> bool {
    matches!(code, "5" | "6")
        || code.eq_ignore_ascii_case("SQLITE_BUSY")
        || code.eq_ignore_ascii_case("SQLITE_LOCKED")
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
    content_rating: Option<&str>,
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
                   "Year" = ?, "ReleaseDate" = ?, "ContentRating" = ?,
                   "Plot" = ?, "Outline" = ?, "Tagline" = ?
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
        .bind(content_rating)
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
                "Year", "ReleaseDate", "ContentRating", "Plot", "Outline", "Tagline"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .bind(content_rating)
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

    use ersatzrs_domain::LibraryMediaKind;
    use sqlx::{sqlite::SqlitePoolOptions, Executor};

    #[test]
    fn episode_facets_fall_back_to_embedded_values_per_field() {
        let probed = FfprobeMediaVersion {
            field_orders: std::collections::BTreeMap::new(),
            version: ersatzrs_domain::MediaVersion::default(),
            streams: vec![ersatzrs_domain::MediaStream {
                kind: ersatzrs_domain::MediaStreamKind::Audio,
                language: Some("eng".to_owned()),
                ..ersatzrs_domain::MediaStream::default()
            }],
            chapters: Vec::new(),
            format_name: None,
            format_metadata: [
                ("genre".to_owned(), "Embedded Genre".to_owned()),
                ("studio".to_owned(), "Embedded Studio".to_owned()),
                ("publisher".to_owned(), "Embedded Music Label".to_owned()),
                ("tags".to_owned(), "embedded-one;embedded-two".to_owned()),
                ("language".to_owned(), "nl".to_owned()),
            ]
            .into_iter()
            .collect(),
        };
        let scan = LocalEpisodeScan {
            metadata_complete: true,
            probed,
            path_text: String::new(),
            hash: String::new(),
            title: String::new(),
            fallback_title: String::new(),
            sort_title: String::new(),
            episode_number: 1,
            year: None,
            release_date: None,
            content_rating: None,
            metadata_kind: METADATA_KIND_SIDECAR,
            nfo: Some(EpisodeNfo {
                genres: vec!["Sidecar Genre".to_owned()],
                studios: vec![" ".to_owned()],
                music_labels: vec!["Sidecar Music Label".to_owned()],
                languages: vec![String::new()],
                ..EpisodeNfo::default()
            }),
        };

        assert_eq!(
            episode_facet_values(&scan),
            EpisodeFacetValues {
                genres: vec!["Sidecar Genre".to_owned()],
                studios: vec!["Embedded Studio".to_owned()],
                music_labels: vec!["Sidecar Music Label".to_owned()],
                tags: vec!["embedded-one".to_owned(), "embedded-two".to_owned()],
                languages: vec!["dut".to_owned(), "eng".to_owned()],
            }
        );
    }

    fn identity(
        folder: &str,
        title: &str,
        year: Option<i32>,
        guids: &[(&str, &str)],
    ) -> LocalShowIdentity {
        LocalShowIdentity {
            normalized_title: normalize_identity_text(title),
            year,
            provider_guids: guids
                .iter()
                .map(|(provider, value)| (provider.to_lowercase(), value.to_lowercase()))
                .collect(),
            normalized_folder: normalize_folder_path(Path::new(folder)),
        }
    }

    fn descriptor(folder: &str, year: i32, imdb: &str) -> LocalShowDescriptor {
        let nfo = TvShowNfo {
            title: Some("The Kids in the Hall".to_owned()),
            year: Some(year),
            guids: vec![NfoGuid {
                source: Some("imdb".to_owned()),
                value: Some(imdb.to_owned()),
                ..NfoGuid::default()
            }],
            ..TvShowNfo::default()
        };
        LocalShowDescriptor {
            identity: LocalShowIdentity {
                normalized_title: normalize_identity_text("The Kids in the Hall"),
                year: Some(year),
                provider_guids: normalized_nfo_guids(&nfo.guids),
                normalized_folder: normalize_folder_path(Path::new(folder)),
            },
            folder_name: "The Kids in the Hall".to_owned(),
            title: "The Kids in the Hall".to_owned(),
            sort_title: "Kids in the Hall, The".to_owned(),
            metadata_kind: METADATA_KIND_SIDECAR,
            nfo: Some(nfo),
        }
    }

    async fn reconciliation_test_db() -> DbPool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("create reconciliation test database");
        pool.execute("PRAGMA foreign_keys = ON")
            .await
            .expect("enable foreign keys");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        pool
    }

    #[tokio::test]
    async fn case_alias_episode_lookup_preserves_identity_and_library_scope() {
        let pool = reconciliation_test_db().await;
        seed_merged_kids_in_the_hall(&pool).await;
        let (_root, lower, upper) = library_folder::tests::case_alias_fixture();
        let lower_file = lower.join("Season 01").join("e01.mkv");
        let upper_file = upper.join("Season 01").join("e01.mkv");
        std::fs::write(&lower_file, b"synthetic fixture").unwrap();
        let lower_text = lower_file.to_string_lossy();
        let upper_text = upper_file.to_string_lossy();
        sqlx::query(
            r#"UPDATE "MediaFile" SET "Path" = ?, "PathHash" = ? WHERE "MediaVersionId" = 11000"#,
        )
        .bind(lower_text.as_ref())
        .bind(path_hash(&lower_text))
        .execute(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let matched =
            find_existing_local_episode(&mut tx, 1, &upper_text, &path_hash(&upper_text), true)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(matched.get::<i64, _>("MediaItemId"), 1000);
        assert_eq!(matched.get::<i64, _>("MediaVersionId"), 11000);
        assert!(find_existing_local_episode(
            &mut tx,
            1,
            &upper_text,
            &path_hash(&upper_text),
            false
        )
        .await
        .unwrap()
        .is_none());
        assert!(find_existing_local_episode(
            &mut tx,
            2,
            &upper_text,
            &path_hash(&upper_text),
            true
        )
        .await
        .unwrap()
        .is_none());
        let exact =
            find_existing_local_episode(&mut tx, 1, &lower_text, &path_hash(&lower_text), false)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(exact.get::<i64, _>("MediaItemId"), 1000);
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_merged_kids_in_the_hall(pool: &DbPool) {
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Local TV', 2, 1)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path")
               VALUES (1, 1, '/library')"#,
        )
        .await
        .unwrap();

        pool.execute(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (100, 1, 0)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (100)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "ShowMetadata"
               ("Id", "ShowId", "DateAdded", "DateUpdated", "MetadataKind", "Title", "SortTitle", "Year")
               VALUES (100, 100, '2024-01-01', '2024-01-01', 1,
                       'The Kids in the Hall', 'Kids in the Hall, The', 2022)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "MetadataGuid" ("ShowMetadataId", "Guid")
               VALUES (100, 'imdb://tt11921588')"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (1, 'Comedy', 0)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (1, 100)"#,
        )
        .await
        .unwrap();

        pool.execute(
            r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "Path") VALUES
               (10, 1, '/library/original'), (20, 1, '/library/reboot')"#,
        )
        .await
        .unwrap();
        for season_number in 1..=5 {
            let season_id = 100 + i64::from(season_number);
            let folder_id = 10 + i64::from(season_number);
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(season_id)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, 100, ?)"#,
            )
            .bind(season_id)
            .bind(season_number)
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "SeasonMetadata"
                   ("SeasonId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated")
                   VALUES (?, 0, ?, ?, '2024-01-01', '2024-01-01')"#,
            )
            .bind(season_id)
            .bind(format!("Season {season_number}"))
            .bind(format!("{season_number:02}"))
            .execute(pool)
            .await
            .unwrap();
            sqlx::query(
                r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "ParentId", "Path", "Etag")
                   VALUES (?, 1, 10, ?, 'cached')"#,
            )
            .bind(folder_id)
            .bind(format!("/library/original/Season {season_number:02}"))
            .execute(pool)
            .await
            .unwrap();
        }
        pool.execute(
            r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "ParentId", "Path", "Etag")
               VALUES (21, 1, 20, '/library/reboot/Season 01', 'cached')"#,
        )
        .await
        .unwrap();

        let original_per_season = [6, 4, 4, 4, 4];
        let mut episode_id = 1_000_i64;
        for (index, count) in original_per_season.into_iter().enumerate() {
            let season_number = i32::try_from(index + 1).unwrap();
            for episode_number in 1..=count {
                seed_episode(
                    pool,
                    episode_id,
                    100 + i64::from(season_number),
                    10 + i64::from(season_number),
                    &format!(
                        "/library/original/Season {season_number:02}/e{episode_number:02}.mkv"
                    ),
                )
                .await;
                episode_id += 1;
            }
        }
        for episode_number in 1..=8 {
            seed_episode(
                pool,
                2_000 + i64::from(episode_number),
                101,
                21,
                &format!("/library/reboot/Season 01/e{episode_number:02}.mkv"),
            )
            .await;
        }
        seed_extra(pool, 3_000, 100, "/library/reboot/Featurettes/reunion.mkv").await;
        seed_extra(
            pool,
            3_001,
            101,
            "/library/reboot/Season 01/Trailers/preview.mkv",
        )
        .await;
    }

    async fn seed_episode(
        pool: &DbPool,
        episode_id: i64,
        season_id: i64,
        folder_id: i64,
        path: &str,
    ) {
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#)
            .bind(episode_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
            .bind(episode_id)
            .bind(season_id)
            .execute(pool)
            .await
            .unwrap();
        let version_id = episode_id + 10_000;
        sqlx::query(
            r#"INSERT INTO "MediaVersion"
               ("Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind", "EpisodeId")
               VALUES (?, '2024-01-01', '2024-01-01', '00:20:00', 1080, 1920, 0, ?)"#,
        )
        .bind(version_id)
        .bind(episode_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("LibraryFolderId", "MediaVersionId", "Path", "PathHash")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(folder_id)
        .bind(version_id)
        .bind(path)
        .bind(format!("hash-{episode_id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_extra(pool: &DbPool, id: i64, parent_id: i64, path: &str) {
        sqlx::query(
            r#"INSERT INTO "MediaItem"
               ("Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId")
               VALUES (?, 1, 0, 1, 'featurette', ?)"#,
        )
        .bind(id)
        .bind(parent_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "OtherVideo" ("Id") VALUES (?)"#)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        let version_id = id + 10_000;
        sqlx::query(
            r#"INSERT INTO "MediaVersion"
               ("Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind", "OtherVideoId")
               VALUES (?, '2024-01-01', '2024-01-01', '00:02:00', 1080, 1920, 0, ?)"#,
        )
        .bind(version_id)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (?, ?, ?)"#,
        )
        .bind(version_id)
        .bind(path)
        .bind(format!("hash-{id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    fn reconciliation_work() -> Vec<(ShowWork, LocalShowDescriptor)> {
        [
            ("/library/original", 1988, "tt0096623"),
            ("/library/reboot", 2022, "tt11921588"),
        ]
        .into_iter()
        .map(|(folder, year, imdb)| {
            (
                ShowWork {
                    folder: PathBuf::from(folder),
                    seasons: Vec::new(),
                    extra_files: Vec::new(),
                },
                descriptor(folder, year, imdb),
            )
        })
        .collect()
    }

    async fn run_reconciliation(pool: &DbPool, inject_failure: bool) -> Result<()> {
        reconcile_merged_local_shows(
            pool,
            &LocalLibraryPath {
                id: 1,
                path: "/library".to_owned(),
                media_kind: LibraryMediaKind::Shows,
            },
            &reconciliation_work(),
            inject_failure,
        )
        .await
    }

    #[tokio::test]
    async fn reconciliation_splits_mixed_season_and_preserves_canonical_ids() {
        let pool = reconciliation_test_db().await;
        seed_merged_kids_in_the_hall(&pool).await;
        run_reconciliation(&pool, false).await.unwrap();

        let shows: Vec<(i64, i64, i64)> = sqlx::query_as(
            r#"SELECT show."Id", COUNT(DISTINCT season."Id"), COUNT(DISTINCT episode."Id")
               FROM "Show" show
               LEFT JOIN "Season" season ON season."ShowId" = show."Id"
               LEFT JOIN "Episode" episode ON episode."SeasonId" = season."Id"
               GROUP BY show."Id" ORDER BY show."Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(shows.len(), 2);
        assert_eq!(shows[0], (100, 5, 22));
        assert_eq!((shows[1].1, shows[1].2), (1, 8));

        let canonical_season_show: i64 =
            sqlx::query_scalar(r#"SELECT "ShowId" FROM "Season" WHERE "Id" = 101"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(canonical_season_show, 100);
        let collection_parent: i64 = sqlx::query_scalar(
            r#"SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(collection_parent, 100);
        let extra_parents: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "ParentMediaItemId" FROM "MediaItem" WHERE "Id" IN (3000, 3001) ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_ne!(extra_parents[0], 100);
        assert_ne!(extra_parents[1], 101);
        let cached_etags: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "LibraryFolder" WHERE "Id" IN (11, 12, 13, 14, 15, 21) AND "Etag" IS NOT NULL"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cached_etags, 0);

        run_reconciliation(&pool, false).await.unwrap();
        let show_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Show""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(show_count, 2, "reconciliation must be idempotent");
    }

    #[tokio::test]
    async fn reconciliation_rolls_back_the_entire_split_on_error() {
        let pool = reconciliation_test_db().await;
        seed_merged_kids_in_the_hall(&pool).await;
        assert!(run_reconciliation(&pool, true).await.is_err());
        let show_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Show""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        let merged_episodes: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Episode" episode
               JOIN "Season" season ON season."Id" = episode."SeasonId"
               WHERE season."ShowId" = 100"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let season_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Season""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        let extra_parents: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "ParentMediaItemId" FROM "MediaItem" WHERE "Id" IN (3000, 3001) ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let cached_etags: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "LibraryFolder" WHERE "Id" IN (11, 12, 13, 14, 15, 21) AND "Etag" = 'cached'"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(show_count, 1);
        assert_eq!(season_count, 5);
        assert_eq!(merged_episodes, 30);
        assert_eq!(extra_parents, vec![100, 101]);
        assert_eq!(cached_etags, 6);
    }

    #[tokio::test]
    async fn cleanup_removes_empty_parents_but_keeps_extras_only_parents() {
        let pool = reconciliation_test_db().await;
        seed_merged_kids_in_the_hall(&pool).await;
        for id in [400_i64, 401, 500, 600, 601] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (400), (500), (600)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber")
               VALUES (401, 400, 1), (601, 600, 1)"#,
        )
        .await
        .unwrap();
        seed_extra(
            &pool,
            3_100,
            500,
            "/library/extras-only/Featurettes/one.mkv",
        )
        .await;
        seed_extra(
            &pool,
            3_101,
            601,
            "/library/season-extras/Season 01/Trailers/one.mkv",
        )
        .await;

        cleanup_empty_local_television(&pool, 1).await.unwrap();
        let remaining: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "MediaItem" WHERE "Id" IN (400, 401, 500, 600, 601) ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, vec![500, 600, 601]);
    }

    #[test]
    fn local_show_identity_decision_table() {
        let original = identity(
            "/library/original",
            "The Kids in the Hall",
            Some(1988),
            &[("imdb", "tt0096623")],
        );
        let reboot = identity(
            "/library/reboot",
            "The Kids in the Hall",
            Some(2022),
            &[("imdb", "tt11921588")],
        );
        assert!(!identities_match(&original, &reboot));

        let renamed = identity(
            "/library/kids-in-the-hall",
            "Kids in the Hall, The",
            Some(1988),
            &[("IMDB", "TT0096623")],
        );
        assert!(identities_match(&original, &renamed));

        let corrected = identity(
            "/library/original",
            "The Kids in the Hall",
            Some(1989),
            &[("imdb", "corrected")],
        );
        assert_eq!(original.normalized_folder, corrected.normalized_folder);
        assert!(identities_conflict(&original, &corrected));
        assert!(should_reuse_show(true, &original, &corrected));

        let wrong_imdb = identity(
            "/library/other",
            "The Kids in the Hall",
            Some(1988),
            &[("imdb", "tt0000000")],
        );
        assert!(!identities_match(&original, &wrong_imdb));

        let unknown_one = identity("/library/one", "Shared title", None, &[]);
        let unknown_two = identity("/library/two", "Shared title", None, &[]);
        assert!(!identities_match(&unknown_one, &unknown_two));

        let title_year_one = identity("/library/a", "Exact title", Some(2001), &[]);
        let title_year_two = identity("/library/b", " exact   TITLE ", Some(2001), &[]);
        assert!(identities_match(&title_year_one, &title_year_two));
    }

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

    #[test]
    fn show_work_splits_featurettes_from_episodes() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let show = temp.path().join("Example Show");
        let season = show.join("Season 01");
        let show_extra = show.join("Featurettes");
        let season_extra = season.join("Trailers");
        std::fs::create_dir_all(&season).expect("season");
        std::fs::create_dir_all(&show_extra).expect("show extra");
        std::fs::create_dir_all(&season_extra).expect("season extra");
        std::fs::write(season.join("Example.S01E01.mkv"), b"episode").expect("episode");
        std::fs::write(show_extra.join("Making Of.mkv"), b"show extra").expect("show extra file");
        std::fs::write(season_extra.join("Season One Trailer.mkv"), b"season extra")
            .expect("season extra file");

        let shows = collect_show_work(&temp.path().display().to_string());
        assert_eq!(shows.len(), 1);
        assert_eq!(shows[0].extra_files.len(), 1);
        assert_eq!(shows[0].extra_files[0].extra.sub_kind, "featurette");
        assert_eq!(shows[0].seasons.len(), 1);
        assert_eq!(shows[0].seasons[0].episode_files.len(), 1);
        assert_eq!(shows[0].seasons[0].extra_files.len(), 1);
        assert_eq!(shows[0].seasons[0].extra_files[0].extra.sub_kind, "trailer");
        assert_eq!(count_episode_files(&temp.path().display().to_string()), 3);
    }

    #[test]
    fn recognizes_sqlite_lock_codes() {
        assert!(is_sqlite_lock_code("5"));
        assert!(is_sqlite_lock_code("6"));
        assert!(is_sqlite_lock_code("SQLITE_BUSY"));
        assert!(is_sqlite_lock_code("sqlite_locked"));
        assert!(!is_sqlite_lock_code("19"));
        assert!(!is_sqlite_lock_code("SQLITE_CONSTRAINT"));
    }
}
