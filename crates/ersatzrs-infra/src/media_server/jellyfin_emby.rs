//! Behavior shared by the Jellyfin and Emby adapters.
//!
//! HTTP authentication, endpoint construction, and response projection remain
//! provider-owned. This module contains only the normalized persistence and
//! pagination contracts that are identical for both adapters.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    MediaChapter, MediaStream, MediaStreamKind, ScanProgressSink, VideoScanKind,
};
use sqlx::{Row, Transaction};

use crate::db::DbPool;
use crate::repositories::SqlxResultExt as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JellyfinEmbyProvider {
    Jellyfin,
    Emby,
}

impl JellyfinEmbyProvider {
    const fn http_error_kind(self) -> &'static str {
        match self {
            Self::Jellyfin => "jellyfin http",
            Self::Emby => "emby http",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderItemTable {
    JellyfinMovie,
    JellyfinEpisode,
    EmbyMovie,
    EmbyEpisode,
}

impl ProviderItemTable {
    #[cfg(test)]
    const ALL: [Self; 4] = [
        Self::JellyfinMovie,
        Self::JellyfinEpisode,
        Self::EmbyMovie,
        Self::EmbyEpisode,
    ];

    const fn table_name(self) -> &'static str {
        match self {
            Self::JellyfinMovie => "JellyfinMovie",
            Self::JellyfinEpisode => "JellyfinEpisode",
            Self::EmbyMovie => "EmbyMovie",
            Self::EmbyEpisode => "EmbyEpisode",
        }
    }

    const fn metadata_projection(self) -> (&'static str, &'static str) {
        match self {
            Self::JellyfinMovie => (
                r#""MovieMetadata"."Id" AS "MetadataId""#,
                r#"LEFT JOIN "MovieMetadata" ON "MovieMetadata"."MovieId" = "JellyfinMovie"."Id""#,
            ),
            Self::JellyfinEpisode => (
                r#""EpisodeMetadata"."Id" AS "MetadataId""#,
                r#"LEFT JOIN "EpisodeMetadata" ON "EpisodeMetadata"."EpisodeId" = "JellyfinEpisode"."Id""#,
            ),
            Self::EmbyMovie => (
                r#""MovieMetadata"."Id" AS "MetadataId""#,
                r#"LEFT JOIN "MovieMetadata" ON "MovieMetadata"."MovieId" = "EmbyMovie"."Id""#,
            ),
            Self::EmbyEpisode => (
                r#""EpisodeMetadata"."Id" AS "MetadataId""#,
                r#"LEFT JOIN "EpisodeMetadata" ON "EpisodeMetadata"."EpisodeId" = "EmbyEpisode"."Id""#,
            ),
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
pub(crate) fn provider_error(
    provider: JellyfinEmbyProvider,
    error: reqwest::Error,
) -> RepositoryError {
    RepositoryError::Decode {
        kind: provider.http_error_kind(),
        value: error.to_string(),
    }
}

pub(crate) fn nonempty_strings(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect()
}

pub(crate) fn sort_title(title: &str) -> String {
    let lower = title.to_ascii_lowercase();
    if lower.starts_with("the ") {
        title[4..].to_owned()
    } else if lower.starts_with("a ") {
        title[2..].to_owned()
    } else if lower.starts_with("an ") {
        title[3..].to_owned()
    } else if title.starts_with('Æ') {
        title.replace('Æ', "E")
    } else {
        title.to_owned()
    }
}

#[allow(clippy::similar_names)]
pub(crate) fn guids_from_provider_ids(
    imdb: Option<&str>,
    tmdb_id: Option<&str>,
    tvdb_id: Option<&str>,
) -> Vec<String> {
    [("imdb", imdb), ("tmdb", tmdb_id), ("tvdb", tvdb_id)]
        .into_iter()
        .filter_map(|(provider, value)| {
            value
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("{provider}://{value}"))
        })
        .collect()
}

pub(crate) fn is_anamorphic(width: i32, height: i32, aspect_ratio: &str) -> bool {
    let Some((num, den)) = aspect_ratio.split_once(':') else {
        return false;
    };
    let Ok(num) = num.parse::<f64>() else {
        return false;
    };
    let Ok(den) = den.parse::<f64>() else {
        return false;
    };
    den != 0.0 && ((f64::from(width) / f64::from(height)) - (num / den)).abs() > 0.01
}

pub(crate) fn encode_datetime(value: DateTime<Utc>) -> String {
    value.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub(crate) fn encode_duration(value: Duration) -> String {
    let total_seconds = value.num_seconds().max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

pub(crate) const fn encode_stream_kind(kind: MediaStreamKind) -> i32 {
    // Matches ErsatzTV.Core/Domain/MediaItem/MediaStreamKind.cs: 1-based enum.
    match kind {
        MediaStreamKind::Video => 1,
        MediaStreamKind::Audio => 2,
        MediaStreamKind::Subtitle => 3,
        MediaStreamKind::Attachment => 4,
        MediaStreamKind::ExternalSubtitle => 5,
    }
}

pub(crate) fn path_hash(path: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(path.as_bytes());
    let mut hash = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hash, "{byte:02x}").expect("writing to String should not fail");
    }
    hash
}

pub(crate) trait VersionMediaItem {
    fn date_created(&self) -> DateTime<Utc>;
}

pub(crate) trait VersionPlayback {
    fn duration(&self) -> Duration;
    fn width(&self) -> i32;
    fn height(&self) -> i32;
    fn sample_aspect_ratio(&self) -> Option<&str>;
    fn display_aspect_ratio(&self) -> Option<&str>;
    fn r_frame_rate(&self) -> Option<&str>;
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum VersionOwner {
    Movie(i64),
    Episode(i64),
}

pub(crate) async fn ensure_media_version<I, P>(
    tx: &mut Transaction<'_, crate::db::Db>,
    owner: VersionOwner,
    item: &I,
    playback: &P,
) -> Result<i64>
where
    I: VersionMediaItem,
    P: VersionPlayback,
{
    let (column, item_id) = match owner {
        VersionOwner::Movie(item_id) => ("MovieId", item_id),
        VersionOwner::Episode(item_id) => ("EpisodeId", item_id),
    };
    let sql = format!(r#"SELECT "Id" FROM "MediaVersion" WHERE "{column}" = ?"#);
    if let Some(row) = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
    {
        let id = row.get("Id");
        update_media_version(tx, id, item, playback).await?;
        return Ok(id);
    }

    let sql = format!(
        r#"INSERT INTO "MediaVersion" (
            "DateAdded", "DateUpdated", "DisplayAspectRatio", "Duration", "Height",
            "Name", "{column}", "RFrameRate", "SampleAspectRatio",
            "VideoScanKind", "Width"
        ) VALUES (?, ?, ?, ?, ?, 'Main', ?, ?, ?, ?, ?)"#
    );
    let result = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(encode_datetime(item.date_created()))
        .bind(encode_datetime(Utc::now()))
        .bind(playback.display_aspect_ratio())
        .bind(encode_duration(playback.duration()))
        .bind(playback.height())
        .bind(item_id)
        .bind(playback.r_frame_rate())
        .bind(playback.sample_aspect_ratio())
        .bind(VideoScanKind::Progressive.db_value())
        .bind(playback.width())
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(crate::db::last_insert_id(result))
}

async fn update_media_version<I, P>(
    tx: &mut Transaction<'_, crate::db::Db>,
    id: i64,
    item: &I,
    playback: &P,
) -> Result<()>
where
    I: VersionMediaItem,
    P: VersionPlayback,
{
    sqlx::query(
        r#"UPDATE "MediaVersion"
           SET "DateAdded" = ?, "DateUpdated" = ?, "DisplayAspectRatio" = ?,
               "Duration" = ?, "Height" = ?, "RFrameRate" = ?,
               "SampleAspectRatio" = ?, "Width" = ?
           WHERE "Id" = ?"#,
    )
    .bind(encode_datetime(item.date_created()))
    .bind(encode_datetime(Utc::now()))
    .bind(playback.display_aspect_ratio())
    .bind(encode_duration(playback.duration()))
    .bind(playback.height())
    .bind(playback.r_frame_rate())
    .bind(playback.sample_aspect_ratio())
    .bind(playback.width())
    .bind(id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

pub(crate) async fn ensure_media_file(
    tx: &mut Transaction<'_, crate::db::Db>,
    version_id: i64,
    path: &str,
) -> Result<()> {
    sqlx::query(
        r#"REPLACE INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
           VALUES (?, ?, ?)"#,
    )
    .bind(version_id)
    .bind(path)
    .bind(path_hash(path))
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

pub(crate) async fn replace_streams(
    tx: &mut Transaction<'_, crate::db::Db>,
    version_id: i64,
    streams: &[MediaStream],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "MediaStream" WHERE "MediaVersionId" = ?"#)
        .bind(version_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for stream in streams {
        sqlx::query(
            r#"INSERT INTO "MediaStream" (
                "Index", "Codec", "Profile", "MediaStreamKind", "Language", "Channels",
                "Title", "Default", "Forced", "MediaVersionId",
                "PixelFormat", "ColorRange", "ColorSpace", "ColorTransfer", "ColorPrimaries",
                "BitsPerRawSample", "AttachedPic", "FileName", "MimeType"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(stream.index)
        .bind(&stream.codec)
        .bind(&stream.profile)
        .bind(encode_stream_kind(stream.kind))
        .bind(&stream.language)
        .bind(stream.channels.unwrap_or_default())
        .bind(&stream.title)
        .bind(stream.default)
        .bind(stream.forced)
        .bind(version_id)
        .bind(&stream.pixel_format)
        .bind(&stream.color_range)
        .bind(&stream.color_space)
        .bind(&stream.color_transfer)
        .bind(&stream.color_primaries)
        .bind(stream.bits_per_raw_sample)
        .bind(stream.attached_pic)
        .bind(&stream.file_name)
        .bind(&stream.mime_type)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

pub(crate) async fn replace_chapters(
    tx: &mut Transaction<'_, crate::db::Db>,
    version_id: i64,
    chapters: &[MediaChapter],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "MediaChapter" WHERE "MediaVersionId" = ?"#)
        .bind(version_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for chapter in chapters {
        sqlx::query(
            r#"INSERT INTO "MediaChapter" (
                "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
            ) VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(version_id)
        .bind(chapter.chapter_id)
        .bind(encode_duration(chapter.start_time))
        .bind(encode_duration(chapter.end_time))
        .bind(&chapter.title)
        .execute(&mut **tx)
        .await
        .repository()?;
    }
    Ok(())
}

pub(crate) async fn item_etag_matches(
    pool: &DbPool,
    item_table: ProviderItemTable,
    library_id: i64,
    item_id: &str,
    etag: Option<&str>,
) -> Result<bool> {
    let table = item_table.table_name();
    let (metadata_select, metadata_join) = item_table.metadata_projection();
    let sql = format!(
        r#"SELECT "{table}"."Etag" AS "Etag", "MediaItem"."State" AS "State",
                  {metadata_select}
           FROM "{table}"
           INNER JOIN "MediaItem" ON "MediaItem"."Id" = "{table}"."Id"
           INNER JOIN "LibraryPath" ON "LibraryPath"."Id" = "MediaItem"."LibraryPathId"
           {metadata_join}
           WHERE "{table}"."ItemId" = ? AND "LibraryPath"."LibraryId" = ?"#
    );
    let Some(row) = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(item_id)
        .bind(library_id)
        .fetch_optional(pool)
        .await
        .repository()?
    else {
        return Ok(false);
    };

    let existing_etag: Option<String> = row.try_get("Etag").repository()?;
    let state: i64 = row.try_get("State").repository()?;
    let has_metadata = row
        .try_get::<Option<i64>, _>("MetadataId")
        .repository()?
        .is_some();
    Ok(super::provider_item_state_allows_etag_skip(state)
        && has_metadata
        && existing_etag.as_deref() == etag)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PageProgress {
    start_index: i32,
    page_size: i32,
    reported_initial: bool,
}

impl PageProgress {
    pub(crate) const fn new(page_size: i32) -> Self {
        Self {
            start_index: 0,
            page_size,
            reported_initial: false,
        }
    }

    pub(crate) const fn start_index(self) -> i32 {
        self.start_index
    }

    pub(crate) fn report_initial(&mut self, progress: Option<&dyn ScanProgressSink>) {
        if !self.reported_initial {
            if let Some(sink) = progress {
                sink.report(0.0);
            }
            self.reported_initial = true;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn report(
        progress: Option<&dyn ScanProgressSink>,
        completed: usize,
        total_record_count: i32,
    ) {
        if let Some(sink) = progress {
            if total_record_count > 0 {
                let total = total_record_count as f32;
                sink.report((completed as f32) / total);
            }
        }
    }

    pub(crate) fn advance(&mut self, page_len: i32, total_record_count: i32) -> bool {
        self.start_index += self.page_size;
        self.start_index < total_record_count && page_len != 0
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use ersatzrs_domain::ScanProgressSink;

    use super::{sort_title, PageProgress, ProviderItemTable};

    #[derive(Default)]
    struct RecordingProgress(Mutex<Vec<f32>>);

    impl ScanProgressSink for RecordingProgress {
        fn report(&self, fraction: f32) {
            self.0.lock().expect("progress lock").push(fraction);
        }
    }

    #[test]
    fn provider_item_table_registry_is_complete_and_stable() {
        assert_eq!(
            ProviderItemTable::ALL.map(ProviderItemTable::table_name),
            [
                "JellyfinMovie",
                "JellyfinEpisode",
                "EmbyMovie",
                "EmbyEpisode",
            ]
        );
    }

    #[test]
    fn shared_sort_title_preserves_provider_projection_rules() {
        assert_eq!(sort_title("The Movie"), "Movie");
        assert_eq!(sort_title("Æon Flux"), "Eon Flux");
    }

    #[test]
    fn page_progress_preserves_initial_fraction_and_page_termination() {
        let sink = RecordingProgress::default();
        let mut pages = PageProgress::new(100);

        pages.report_initial(Some(&sink));
        pages.report_initial(Some(&sink));
        PageProgress::report(Some(&sink), 25, 100);

        assert!(pages.advance(100, 250));
        assert_eq!(pages.start_index(), 100);
        assert!(!pages.advance(0, 250));
        assert_eq!(*sink.0.lock().expect("progress lock"), [0.0, 0.25]);
    }
}
