//! Shared upsert helpers for `ErsatzTV`'s "sparse multi-FK" metadata tables.
//!
//! Tables like `Genre`, `Tag`, `Studio`, `Actor`, `Director`, `Writer`,
//! `Style`, `Mood`, `MusicVideoArtist`, `MetadataGuid`, and `Artwork` are
//! shared across metadata kinds (Movie/Show/Season/Episode/Artist/...) via
//! one nullable `{Kind}MetadataId` column per kind. The DELETE-then-INSERT
//! pattern is identical per table, so this module centralises it behind a
//! `fk_column` parameter and the table identifier.
//!
//! Only the FK-column and table names are interpolated into SQL strings.
//! Both are `&'static str`, never user input, so this is not an injection
//! surface. All variable values use bound parameters.
use crate::repositories::SqlxResultExt as _;

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use ersatzrs_core::language_codes;
use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{MediaStream, MediaStreamKind};
use sqlx::{Row, Transaction};

use crate::db::Db;

/// `ErsatzTV` `SubtitleKind` enum values
/// (see `ErsatzTV.Core/Domain/Metadata/SubtitleKind.cs`).
pub mod subtitle_kind {
    pub const EMBEDDED: i32 = 0;
    pub const SIDECAR: i32 = 1;
    #[allow(dead_code)]
    pub const GENERATED: i32 = 99;
}

#[derive(Debug, Clone, Copy)]
pub enum SubtitleMetadataOwner {
    Episode(i64),
    Movie(i64),
    MusicVideo(i64),
    OtherVideo(i64),
    RemoteStream(i64),
}

impl SubtitleMetadataOwner {
    fn column(self) -> &'static str {
        match self {
            Self::Episode(_) => "EpisodeMetadataId",
            Self::Movie(_) => "MovieMetadataId",
            Self::MusicVideo(_) => "MusicVideoMetadataId",
            Self::OtherVideo(_) => "OtherVideoMetadataId",
            Self::RemoteStream(_) => "RemoteStreamMetadataId",
        }
    }

    fn id(self) -> i64 {
        match self {
            Self::Episode(id)
            | Self::Movie(id)
            | Self::MusicVideo(id)
            | Self::OtherVideo(id)
            | Self::RemoteStream(id) => id,
        }
    }
}

/// `ErsatzTV` `ArtworkKind` enum values
/// (see `ErsatzTV.Core/Domain/Metadata/ArtworkKind.cs`).
pub mod artwork_kind {
    pub const POSTER: i32 = 0;
    pub const THUMBNAIL: i32 = 1;
    pub const LOGO: i32 = 2;
    pub const FAN_ART: i32 = 3;
    pub const WATERMARK: i32 = 4;
}

/// One person row for `Actor` / `Director` / `Writer`. `role` / `order` are
/// only meaningful for `Actor`; the helper ignores them otherwise.
#[derive(Debug, Clone)]
pub struct PersonRef {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i32>,
}

impl PersonRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: None,
            order: None,
        }
    }

    pub fn with_role(name: impl Into<String>, role: Option<String>, order: Option<i32>) -> Self {
        Self {
            name: name.into(),
            role,
            order,
        }
    }
}

/// One artwork row. `date_added` is preserved across replaces so the original
/// scan time is not lost; `date_updated` is set to `now()` by the helper.
#[derive(Debug, Clone)]
pub struct ArtworkRef {
    pub kind: i32,
    pub path: String,
    pub date_added: DateTime<Utc>,
}

/// DELETE-then-INSERT rows in a "shared string" table (`Genre`, `Tag`,
/// `Studio`, `Style`, `Mood`, `MusicVideoArtist`). Each row has a single
/// `Name` payload column plus the `{Kind}MetadataId` FK.
///
/// # Errors
///
/// Returns an error when the underlying DELETE or INSERT statements fail.
pub async fn replace_strings(
    tx: &mut Transaction<'_, Db>,
    table: &'static str,
    fk_column: &'static str,
    metadata_id: i64,
    values: &[String],
) -> Result<()> {
    let delete_sql = format!(r#"DELETE FROM "{table}" WHERE "{fk_column}" = ?"#);
    sqlx::query(crate::db::assert_sql_safe(&delete_sql))
        .bind(metadata_id)
        .execute(&mut **tx)
        .await
        .repository()?;

    if values.is_empty() {
        return Ok(());
    }

    let insert_sql = format!(r#"INSERT INTO "{table}" ("Name", "{fk_column}") VALUES (?, ?)"#);
    for value in values {
        sqlx::query(crate::db::assert_sql_safe(&insert_sql))
            .bind(value)
            .bind(metadata_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}

/// DELETE-then-INSERT rows in a "person" table (`Actor`, `Director`,
/// `Writer`). `Actor` has additional `Role` and `Order` columns; for
/// `Director`/`Writer` the helper writes NULL into both.
///
/// # Errors
///
/// Returns an error when the underlying DELETE or INSERT statements fail.
pub async fn replace_people(
    tx: &mut Transaction<'_, Db>,
    table: &'static str,
    fk_column: &'static str,
    metadata_id: i64,
    people: &[PersonRef],
) -> Result<()> {
    let delete_sql = format!(r#"DELETE FROM "{table}" WHERE "{fk_column}" = ?"#);
    sqlx::query(crate::db::assert_sql_safe(&delete_sql))
        .bind(metadata_id)
        .execute(&mut **tx)
        .await
        .repository()?;

    if people.is_empty() {
        return Ok(());
    }

    let is_actor = table == "Actor";
    let insert_sql = if is_actor {
        format!(
            r#"INSERT INTO "{table}" ("Name", "Role", "Order", "{fk_column}") VALUES (?, ?, ?, ?)"#
        )
    } else {
        format!(r#"INSERT INTO "{table}" ("Name", "{fk_column}") VALUES (?, ?)"#)
    };

    for person in people {
        if is_actor {
            sqlx::query(crate::db::assert_sql_safe(&insert_sql))
                .bind(&person.name)
                .bind(&person.role)
                .bind(person.order)
                .bind(metadata_id)
                .execute(&mut **tx)
                .await
                .repository()?;
        } else {
            sqlx::query(crate::db::assert_sql_safe(&insert_sql))
                .bind(&person.name)
                .bind(metadata_id)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
    }
    Ok(())
}

/// DELETE-then-INSERT artwork rows for the given metadata FK. `scope` lists
/// which `ArtworkKind`s the caller manages on this pass; rows of those kinds
/// are deleted unconditionally so the source-of-truth can shrink (a media item
/// that loses its poster between scans must not keep stale rows). Kinds
/// outside `scope` are left untouched.
///
/// # Errors
///
/// Returns an error when the underlying DELETE or INSERT statements fail.
pub async fn replace_artwork(
    tx: &mut Transaction<'_, Db>,
    fk_column: &'static str,
    metadata_id: i64,
    scope: &[i32],
    items: &[ArtworkRef],
) -> Result<()> {
    if scope.is_empty() {
        return Ok(());
    }

    let placeholders = scope.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let delete_sql = format!(
        r#"DELETE FROM "Artwork" WHERE "{fk_column}" = ? AND "ArtworkKind" IN ({placeholders})"#
    );
    let mut delete = sqlx::query(crate::db::assert_sql_safe(&delete_sql)).bind(metadata_id);
    for kind in scope {
        delete = delete.bind(*kind);
    }
    delete.execute(&mut **tx).await.repository()?;

    if items.is_empty() {
        return Ok(());
    }

    let insert_sql = format!(
        r#"INSERT INTO "Artwork" (
            "ArtworkKind", "{fk_column}", "DateAdded", "DateUpdated", "Path"
        ) VALUES (?, ?, ?, ?, ?)"#
    );
    let now = encode_datetime(Utc::now());
    for item in items {
        sqlx::query(crate::db::assert_sql_safe(&insert_sql))
            .bind(item.kind)
            .bind(metadata_id)
            .bind(encode_datetime(item.date_added))
            .bind(&now)
            .bind(&item.path)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}

/// DELETE-then-INSERT provider GUIDs in `MetadataGuid` for the given
/// metadata FK.
///
/// # Errors
///
/// Returns an error when the underlying DELETE or INSERT statements fail.
pub async fn replace_guids(
    tx: &mut Transaction<'_, Db>,
    fk_column: &'static str,
    metadata_id: i64,
    guids: &[String],
) -> Result<()> {
    let delete_sql = format!(r#"DELETE FROM "MetadataGuid" WHERE "{fk_column}" = ?"#);
    sqlx::query(crate::db::assert_sql_safe(&delete_sql))
        .bind(metadata_id)
        .execute(&mut **tx)
        .await
        .repository()?;

    if guids.is_empty() {
        return Ok(());
    }

    let insert_sql = format!(r#"INSERT INTO "MetadataGuid" ("Guid", "{fk_column}") VALUES (?, ?)"#);
    for guid in guids {
        sqlx::query(crate::db::assert_sql_safe(&insert_sql))
            .bind(guid)
            .bind(metadata_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}

/// One incoming subtitle row derived from a `MediaStream`.
///
/// Mirrors `ErsatzTV.Core/Domain/Metadata/Subtitle.cs` `FromMediaStream`:
/// `SDH` is intentionally left at its default (`false`) because the C# factory
/// does not set it; `Path` carries the external `FileName` (sidecar) or NULL.
///
/// The three `bool` fields mirror `Subtitle` table columns one-to-one, so the
/// `struct_excessive_bools` lint (which suggests a state machine) does not apply.
#[allow(clippy::struct_excessive_bools)]
struct IncomingSubtitle {
    stream_index: i32,
    codec: Option<String>,
    title: Option<String>,
    default: bool,
    forced: bool,
    sdh: bool,
    language: Option<String>,
    path: Option<String>,
    subtitle_kind: i32,
    date_updated: Option<String>,
}

impl IncomingSubtitle {
    /// Port of `Subtitle.FromMediaStream` (`Subtitle.cs`):
    /// `ExternalSubtitle -> Sidecar (1)`, otherwise `Embedded (0)`.
    fn from_media_stream(stream: &MediaStream) -> Self {
        let subtitle_kind = if stream.kind == MediaStreamKind::ExternalSubtitle {
            subtitle_kind::SIDECAR
        } else {
            subtitle_kind::EMBEDDED
        };
        let path = stream
            .file_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(ToOwned::to_owned);
        Self {
            stream_index: stream.index,
            codec: stream.codec.clone(),
            title: stream.title.clone(),
            default: stream.default,
            forced: stream.forced,
            sdh: false,
            language: stream.language.clone(),
            path,
            subtitle_kind,
            date_updated: None,
        }
    }

    fn sidecar(
        stream_index: i32,
        codec: String,
        language: Option<String>,
        forced: bool,
        sdh: bool,
        path: String,
        date_updated: String,
    ) -> Self {
        Self {
            stream_index,
            codec: Some(codec),
            title: None,
            default: false,
            forced,
            sdh,
            language,
            path: Some(path),
            subtitle_kind: subtitle_kind::SIDECAR,
            date_updated: Some(date_updated),
        }
    }
}

/// The four `bool` fields mirror `Subtitle` table columns one-to-one, so the
/// `struct_excessive_bools` lint does not apply.
#[allow(clippy::struct_excessive_bools)]
struct ExistingSubtitle {
    id: i64,
    stream_index: i32,
    codec: Option<String>,
    default: bool,
    forced: bool,
    sdh: bool,
    language: Option<String>,
    is_extracted: bool,
    path: Option<String>,
    subtitle_kind: i32,
}

async fn load_existing_subtitles(
    tx: &mut Transaction<'_, Db>,
    owner: SubtitleMetadataOwner,
) -> Result<Vec<ExistingSubtitle>> {
    let sql = format!(
        r#"SELECT "Id", "StreamIndex", "Codec", "Default", "Forced", "SDH",
                  "Language", "IsExtracted", "Path", "SubtitleKind"
           FROM "Subtitle"
           WHERE "{}" = ?"#,
        owner.column()
    );
    let rows = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(owner.id())
        .fetch_all(&mut **tx)
        .await
        .repository()?;

    rows.iter()
        .map(|row| {
            Ok(ExistingSubtitle {
                id: row.try_get("Id").repository()?,
                stream_index: row.try_get("StreamIndex").repository()?,
                codec: row.try_get("Codec").repository()?,
                default: row.try_get::<i64, _>("Default").repository()? != 0,
                forced: row.try_get::<i64, _>("Forced").repository()? != 0,
                sdh: row.try_get::<i64, _>("SDH").repository()? != 0,
                language: row.try_get("Language").repository()?,
                is_extracted: row.try_get::<i64, _>("IsExtracted").repository()? != 0,
                path: row.try_get("Path").repository()?,
                subtitle_kind: row.try_get("SubtitleKind").repository()?,
            })
        })
        .collect()
}

async fn insert_subtitle(
    tx: &mut Transaction<'_, Db>,
    owner: SubtitleMetadataOwner,
    inc: &IncomingSubtitle,
    now: &str,
) -> Result<()> {
    let sql = format!(
        r#"INSERT INTO "Subtitle" (
            "{}", "SubtitleKind", "Title", "StreamIndex", "Codec",
            "Default", "Forced", "SDH", "Language", "IsExtracted", "Path",
            "DateAdded", "DateUpdated"
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)"#,
        owner.column()
    );
    sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(owner.id())
        .bind(inc.subtitle_kind)
        .bind(&inc.title)
        .bind(inc.stream_index)
        .bind(&inc.codec)
        .bind(inc.default)
        .bind(inc.forced)
        .bind(inc.sdh)
        .bind(&inc.language)
        .bind(&inc.path)
        .bind(now)
        .bind(inc.date_updated.as_deref().unwrap_or(now))
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(())
}

async fn update_subtitle(
    tx: &mut Transaction<'_, Db>,
    existing: &ExistingSubtitle,
    inc: &IncomingSubtitle,
    now: &str,
) -> Result<()> {
    // recompute IsExtracted/Path the same way ETV's UpdateSubtitles does
    let mut is_extracted = existing.is_extracted;
    let mut path = existing.path.clone();

    if existing.is_extracted {
        let different_kind = existing.subtitle_kind != inc.subtitle_kind;
        let different_codec = existing.codec != inc.codec;
        let different_language = existing.language != inc.language;
        let different_flags = existing.default != inc.default
            || existing.forced != inc.forced
            || existing.sdh != inc.sdh;

        if different_kind || different_codec || different_language || different_flags {
            is_extracted = false;
            path = inc.path.clone();
        } else {
            // only re-extract if path changed
            let different_path = existing.path != inc.path;
            if inc.path.as_deref().is_some_and(|p| !p.is_empty()) && different_path {
                is_extracted = false;
                path = inc.path.clone();
            }
        }
    }

    sqlx::query(
        r#"UPDATE "Subtitle"
           SET "Default" = ?, "Forced" = ?, "SDH" = ?, "Language" = ?,
               "SubtitleKind" = ?, "StreamIndex" = ?, "Codec" = ?, "Title" = ?,
               "IsExtracted" = ?, "Path" = ?, "DateUpdated" = ?
           WHERE "Id" = ?"#,
    )
    .bind(inc.default)
    .bind(inc.forced)
    .bind(inc.sdh)
    .bind(&inc.language)
    .bind(inc.subtitle_kind)
    .bind(inc.stream_index)
    .bind(&inc.codec)
    .bind(&inc.title)
    .bind(is_extracted)
    .bind(&path)
    .bind(now)
    .bind(existing.id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

/// Port of `MetadataRepository.UpdateSubtitles` (`MetadataRepository.cs`
/// lines 626/666-717) scoped to `EpisodeMetadata`.
///
/// Diffs the just-written subtitle/external-subtitle `MediaStream`s for the
/// episode's media version against existing `Subtitle` rows keyed by subtitle
/// kind plus stream index, or by sidecar path when a sidecar path is available:
/// adds new rows, removes stale ones, and updates the rest. When the kind,
/// codec, language, or flags
/// (`Default`/`Forced`/`SDH`) change on a previously extracted row — or the
/// path changes — `IsExtracted` is reset to 0 so the extractor re-runs.
///
/// # Errors
///
/// Returns an error when the underlying SELECT/INSERT/UPDATE/DELETE fail.
pub async fn update_episode_subtitles(
    tx: &mut Transaction<'_, Db>,
    episode_metadata_id: i64,
    streams: &[MediaStream],
) -> Result<()> {
    update_metadata_subtitles(
        tx,
        SubtitleMetadataOwner::Episode(episode_metadata_id),
        streams,
    )
    .await
}

/// Update subtitle rows for the given metadata owner from provider streams.
///
/// # Errors
///
/// Returns an error when subtitle rows cannot be loaded, inserted, updated, or
/// removed.
pub async fn update_metadata_subtitles(
    tx: &mut Transaction<'_, Db>,
    owner: SubtitleMetadataOwner,
    streams: &[MediaStream],
) -> Result<()> {
    let incoming: Vec<IncomingSubtitle> = streams
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                MediaStreamKind::Subtitle | MediaStreamKind::ExternalSubtitle
            )
        })
        .map(IncomingSubtitle::from_media_stream)
        .collect();

    update_subtitle_rows(tx, owner, incoming).await
}

/// Update embedded and discovered sidecar subtitle rows for a local media item.
///
/// # Errors
///
/// Returns an error when subtitle rows cannot be loaded, inserted, updated, or
/// removed.
pub async fn update_local_file_subtitles(
    tx: &mut Transaction<'_, Db>,
    owner: SubtitleMetadataOwner,
    streams: &[MediaStream],
    media_item_path: &Path,
    save_full_path: bool,
) -> Result<()> {
    let mut incoming: Vec<IncomingSubtitle> = streams
        .iter()
        .filter(|s| s.kind == MediaStreamKind::Subtitle)
        .map(IncomingSubtitle::from_media_stream)
        .collect();
    incoming.extend(locate_external_subtitles(media_item_path, save_full_path));

    update_subtitle_rows(tx, owner, incoming).await
}

async fn update_subtitle_rows(
    tx: &mut Transaction<'_, Db>,
    owner: SubtitleMetadataOwner,
    incoming: Vec<IncomingSubtitle>,
) -> Result<()> {
    let existing = load_existing_subtitles(tx, owner).await?;
    let now = encode_datetime(Utc::now());

    // remove: existing rows whose stable identity is not in the incoming set
    for existing_row in &existing {
        if !incoming
            .iter()
            .any(|inc| subtitle_identity_matches(existing_row, inc))
        {
            sqlx::query(r#"DELETE FROM "Subtitle" WHERE "Id" = ?"#)
                .bind(existing_row.id)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
    }

    for inc in &incoming {
        match existing
            .iter()
            .find(|existing_row| subtitle_identity_matches(existing_row, inc))
        {
            None => insert_subtitle(tx, owner, inc, &now).await?,
            Some(existing_row) => update_subtitle(tx, existing_row, inc, &now).await?,
        }
    }

    Ok(())
}

fn subtitle_identity_matches(existing: &ExistingSubtitle, incoming: &IncomingSubtitle) -> bool {
    if existing.subtitle_kind != incoming.subtitle_kind {
        return false;
    }

    if incoming.subtitle_kind == subtitle_kind::SIDECAR {
        match (existing.path.as_deref(), incoming.path.as_deref()) {
            (Some(existing_path), Some(incoming_path))
                if !existing_path.is_empty() && !incoming_path.is_empty() =>
            {
                return existing_path == incoming_path;
            }
            _ => {}
        }
    }

    existing.stream_index == incoming.stream_index
}

fn locate_external_subtitles(
    media_item_path: &Path,
    save_full_path: bool,
) -> Vec<IncomingSubtitle> {
    let Some(folder) = media_item_path.parent() else {
        return Vec::new();
    };
    let Some(stem) = media_item_path.file_stem().and_then(|value| value.to_str()) else {
        return Vec::new();
    };

    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let stem_lower = stem.to_ascii_lowercase();

    let mut sidecars: Vec<_> = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }

            let file_name = path.file_name()?.to_str()?;
            let file_stem = path.file_stem()?.to_str()?;
            let file_stem_lower = file_stem.to_ascii_lowercase();
            let suffix = sidecar_suffix(&file_stem_lower, &stem_lower)?;
            let tokens = sidecar_tokens(suffix);

            let codec = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
                "ssa" | "ass" => "ass",
                "srt" => "subrip",
                "vtt" => "webvtt",
                _ => return None,
            }
            .to_owned();

            let forced = tokens.contains(&"forced");
            let sdh = tokens.iter().any(|token| *token == "sdh" || *token == "cc");
            let language = Some(sidecar_language(&tokens));

            let path_value = if save_full_path {
                path.display().to_string()
            } else {
                file_name.to_owned()
            };
            let date_updated = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .map(DateTime::<Utc>::from)
                .map_or_else(|_| encode_datetime(Utc::now()), encode_datetime);

            Some((path_value, codec, language, forced, sdh, date_updated))
        })
        .collect();

    sidecars.sort_by(|left, right| left.0.cmp(&right.0));
    sidecars
        .into_iter()
        .enumerate()
        .map(
            |(index, (path_value, codec, language, forced, sdh, date_updated))| {
                IncomingSubtitle::sidecar(
                    i32::try_from(index).unwrap_or(i32::MAX),
                    codec,
                    language,
                    forced,
                    sdh,
                    path_value,
                    date_updated,
                )
            },
        )
        .collect()
}

fn sidecar_suffix<'a>(file_stem_lower: &'a str, media_stem_lower: &str) -> Option<&'a str> {
    if file_stem_lower == media_stem_lower {
        Some("")
    } else {
        file_stem_lower
            .strip_prefix(media_stem_lower)
            .and_then(|suffix| suffix.strip_prefix('.'))
    }
}

fn sidecar_tokens(suffix: &str) -> Vec<&str> {
    suffix
        .split('.')
        .filter(|token| !token.trim().is_empty())
        .collect()
}

fn sidecar_language(tokens: &[&str]) -> String {
    tokens
        .iter()
        .copied()
        .find(|token| !matches!(*token, "forced" | "sdh" | "cc"))
        .map_or_else(|| "und".to_owned(), normalize_sidecar_language)
}

fn normalize_sidecar_language(language: &str) -> String {
    let language = language.trim().to_ascii_lowercase();
    for (primary, _) in language_codes::language_options() {
        let group = language_codes::all_language_codes([primary.as_str()]);
        if group.iter().any(|code| code == &language) {
            return primary;
        }
    }
    language
}

fn encode_datetime(value: DateTime<Utc>) -> String {
    value.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecars_without_language_use_und() {
        assert_eq!(sidecar_language(&[]), "und");
        assert_eq!(sidecar_language(&["forced"]), "und");
        assert_eq!(sidecar_language(&["forced", "sdh"]), "und");
    }

    #[test]
    fn sidecars_with_language_keep_canonical_language_code() {
        assert_eq!(sidecar_language(&["en"]), "eng");
        assert_eq!(sidecar_language(&["de", "forced"]), "ger");
    }

    #[test]
    fn forced_in_media_title_is_not_a_sidecar_flag() {
        let media_stem = "bloodfist.iii.forced.to.fight.1991";

        assert_eq!(
            sidecar_suffix(media_stem, media_stem),
            Some(""),
            "the bare sidecar has no language or flag suffix"
        );
        assert_eq!(
            sidecar_suffix("bloodfist.iii.forced.to.fight.1991.en.forced", media_stem),
            Some("en.forced")
        );
    }

    #[test]
    fn external_subtitle_scan_persists_und_for_suffixless_sidecars() {
        let folder = tempfile::tempdir().expect("subtitle folder");
        let media = folder.path().join("Bloodfist.III.Forced.To.Fight.1991.mkv");
        fs::write(&media, []).expect("media fixture");
        for name in [
            "Bloodfist.III.Forced.To.Fight.1991.srt",
            "Bloodfist.III.Forced.To.Fight.1991.forced.srt",
            "Bloodfist.III.Forced.To.Fight.1991.en.srt",
        ] {
            fs::write(folder.path().join(name), []).expect("subtitle fixture");
        }

        let subtitles = locate_external_subtitles(&media, false);
        let bare = subtitles
            .iter()
            .find(|subtitle| {
                subtitle.path.as_deref() == Some("Bloodfist.III.Forced.To.Fight.1991.srt")
            })
            .expect("bare subtitle");
        let forced = subtitles
            .iter()
            .find(|subtitle| {
                subtitle.path.as_deref() == Some("Bloodfist.III.Forced.To.Fight.1991.forced.srt")
            })
            .expect("forced subtitle");
        let english = subtitles
            .iter()
            .find(|subtitle| {
                subtitle.path.as_deref() == Some("Bloodfist.III.Forced.To.Fight.1991.en.srt")
            })
            .expect("English subtitle");

        assert_eq!(bare.language.as_deref(), Some("und"));
        assert!(!bare.forced);
        assert_eq!(forced.language.as_deref(), Some("und"));
        assert!(forced.forced);
        assert_eq!(english.language.as_deref(), Some("eng"));
        assert!(!english.forced);
    }
}
