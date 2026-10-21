//! Behavior shared by the Jellyfin and Emby adapters.
//!
//! HTTP authentication, endpoint construction, and response projection remain
//! provider-owned. This module contains only the normalized persistence and
//! pagination contracts that are identical for both adapters.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{ProviderMediaSourceDeclaration, RepositoryError, Result};
use ersatzrs_domain::{
    DolbyVisionMetadataState, MediaChapter, MediaStream, MediaStreamKind, ScanProgressSink,
    VideoScanKind,
};
use sqlx::{Row, Transaction};

use crate::db::DbPool;
use crate::repositories::SqlxResultExt as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JellyfinEmbyProvider {
    Jellyfin,
    Emby,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_provider_dolby_vision_metadata(
    stream: &mut MediaStream,
    provider: &str,
    video_range_type: Option<&str>,
    profile: Option<i32>,
    compatibility_id: Option<i32>,
    bl_present: Option<i32>,
    el_present: Option<i32>,
    rpu_present: Option<i32>,
) {
    let range_detected = video_range_type.is_some_and(|value| {
        let normalized = value.trim().to_ascii_lowercase();
        normalized.contains("dovi") || normalized.contains("dolby")
    });
    let detected = range_detected
        || profile.is_some()
        || compatibility_id.is_some()
        || bl_present.is_some()
        || el_present.is_some()
        || rpu_present.is_some();
    if !detected {
        return;
    }

    if let (Some(profile), Some(compatibility_id), Some(bl), Some(el), Some(rpu)) = (
        profile,
        compatibility_id,
        bl_present,
        el_present,
        rpu_present,
    ) {
        stream.dolby_vision_profile = Some(profile);
        stream.dolby_vision_bl_compatibility_id = Some(compatibility_id);
        stream.dolby_vision_bl_present = Some(bl != 0);
        stream.dolby_vision_el_present = Some(el != 0);
        stream.dolby_vision_rpu_present = Some(rpu != 0);
        stream.dolby_vision_metadata_state = DolbyVisionMetadataState::Complete;
        return;
    }

    stream.dolby_vision_profile = profile;
    stream.dolby_vision_bl_compatibility_id = compatibility_id;
    stream.dolby_vision_bl_present = bl_present.map(|value| value != 0);
    stream.dolby_vision_el_present = el_present.map(|value| value != 0);
    stream.dolby_vision_rpu_present = rpu_present.map(|value| value != 0);
    stream.dolby_vision_metadata_state = DolbyVisionMetadataState::Incomplete;
    stream.dolby_vision_metadata_hint = Some(format!(
        "{provider} reported Dolby Vision but omitted one or more required profile, compatibility, BL, EL, or RPU fields"
    ));
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

fn provider_media_file_hash(version_id: i64, path: &str) -> String {
    use sha2::{Digest as _, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"ersatzrs:provider-media-file:v1\0");
    digest.update(version_id.to_le_bytes());
    digest.update(path.as_bytes());
    let mut hash = String::with_capacity(64);
    for byte in digest.finalize() {
        use std::fmt::Write as _;
        write!(hash, "{byte:02x}").expect("writing to String should not fail");
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
    owner: VersionOwner,
    version_id: i64,
    path: &str,
    declaration: Option<&ProviderMediaSourceDeclaration>,
    streams: &[MediaStream],
) -> Result<()> {
    let (query, item_id) = match owner {
        VersionOwner::Movie(id) => (
            r#"SELECT EXISTS(SELECT 1 FROM "MediaVersion" WHERE "Id"=? AND "MovieId"=?
               AND "EpisodeId" IS NULL AND "MusicVideoId" IS NULL AND "OtherVideoId" IS NULL
               AND "SongId" IS NULL AND "ImageId" IS NULL AND "RemoteStreamId" IS NULL)"#,
            id,
        ),
        VersionOwner::Episode(id) => (
            r#"SELECT EXISTS(SELECT 1 FROM "MediaVersion" WHERE "Id"=? AND "EpisodeId"=?
               AND "MovieId" IS NULL AND "MusicVideoId" IS NULL AND "OtherVideoId" IS NULL
               AND "SongId" IS NULL AND "ImageId" IS NULL AND "RemoteStreamId" IS NULL)"#,
            id,
        ),
    };
    let current: bool = sqlx::query_scalar(query)
        .bind(version_id)
        .bind(item_id)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
    if !current {
        return Err(RepositoryError::Conflict(
            "provider media file version ownership changed".to_owned(),
        ));
    }
    let hash = provider_media_file_hash(version_id, path);
    let existing = owned_media_file(tx, version_id, path, &hash).await?;
    let source_id = declaration
        .filter(|declaration| {
            declaration.path == path
                && !declaration.source_id.trim().is_empty()
                && super::teletext::declaration_matches(declaration, streams)
        })
        .map(|declaration| declaration.source_id.as_str());
    if let Some(id) = existing {
        // Retain a legacy row's identity/hash even when a deep scan restores
        // its path. Never let a global hash conflict move another version's file.
        sqlx::query(r#"UPDATE "MediaFile" SET "Path"=?,"ProviderMediaSourceId"=? WHERE "Id"=? AND "MediaVersionId"=?"#)
            .bind(path).bind(source_id).bind(id).bind(version_id)
            .execute(&mut **tx).await.repository()?;
    } else {
        sqlx::query(r#"INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash","ProviderMediaSourceId") VALUES (?,?,?,?)"#)
            .bind(version_id).bind(path).bind(hash).bind(source_id)
            .execute(&mut **tx).await.repository()?;
    }
    Ok(())
}

async fn owned_media_file(
    tx: &mut Transaction<'_, crate::db::Db>,
    version_id: i64,
    path: &str,
    hash: &str,
) -> Result<Option<i64>> {
    let foreign_collision: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "MediaFile" WHERE "PathHash"=? AND "MediaVersionId"<>?)"#,
    )
    .bind(hash)
    .bind(version_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if foreign_collision {
        return Err(RepositoryError::Conflict(
            "provider media file association changed".to_owned(),
        ));
    }
    let mut ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "MediaFile" WHERE "MediaVersionId"=?
           AND ("PathHash"=? OR "PathHash"=? OR "Path"=? COLLATE BINARY)
           ORDER BY "Id" LIMIT 2"#,
    )
    .bind(version_id)
    .bind(hash)
    .bind(path_hash(path))
    .bind(path)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    if ids.is_empty() {
        // These adapters refresh one selected file per version. A genuine path
        // move must replace its unique owned locator, not leave the old minimum
        // file row eligible for playback with a stale provider-source selector.
        ids = sqlx::query_scalar(
            r#"SELECT "Id" FROM "MediaFile" WHERE "MediaVersionId"=? ORDER BY "Id" LIMIT 2"#,
        )
        .bind(version_id)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
    }
    match ids.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        _ => Err(RepositoryError::Conflict(
            "provider media file association is ambiguous".to_owned(),
        )),
    }
}

#[cfg(test)]
#[path = "jellyfin_emby/provider_source_tests.rs"]
mod provider_source_tests;

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
                "BitsPerRawSample", "AttachedPic", "FileName", "MimeType",
                "Rotation", "MasteringDisplay", "MaxCll", "MaxFall",
                "DolbyVisionProfile", "DolbyVisionBlCompatibilityId",
                "DolbyVisionBlPresent", "DolbyVisionElPresent", "DolbyVisionRpuPresent",
                "DolbyVisionMetadataState", "DolbyVisionMetadataHint"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .bind(stream.rotation)
        .bind(&stream.mastering_display)
        .bind(stream.max_cll)
        .bind(stream.max_fall)
        .bind(stream.dolby_vision_profile)
        .bind(stream.dolby_vision_bl_compatibility_id)
        .bind(stream.dolby_vision_bl_present)
        .bind(stream.dolby_vision_el_present)
        .bind(stream.dolby_vision_rpu_present)
        .bind(stream.dolby_vision_metadata_state.as_i32())
        .bind(&stream.dolby_vision_metadata_hint)
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

    use ersatzrs_domain::{DolbyVisionMetadataState, MediaStream, ScanProgressSink};

    use super::replace_streams;
    use super::{
        apply_provider_dolby_vision_metadata, sort_title, PageProgress, ProviderItemTable,
    };

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

    #[test]
    fn complete_provider_dolby_vision_metadata_is_typed() {
        let mut stream = MediaStream::default();
        apply_provider_dolby_vision_metadata(
            &mut stream,
            "Jellyfin",
            Some("DOVI"),
            Some(8),
            Some(1),
            Some(1),
            Some(0),
            Some(1),
        );

        assert_eq!(
            stream.dolby_vision_metadata_state,
            DolbyVisionMetadataState::Complete
        );
        assert_eq!(stream.dolby_vision_profile, Some(8));
        assert_eq!(stream.dolby_vision_bl_present, Some(true));
        assert_eq!(stream.dolby_vision_el_present, Some(false));
        assert_eq!(stream.dolby_vision_rpu_present, Some(true));
        assert!(stream.dolby_vision_metadata_hint.is_none());
    }

    #[test]
    fn positive_provider_detection_without_all_fields_stays_incomplete() {
        let mut stream = MediaStream::default();
        apply_provider_dolby_vision_metadata(
            &mut stream,
            "Emby",
            Some("DolbyVision"),
            Some(8),
            None,
            None,
            None,
            None,
        );

        assert_eq!(
            stream.dolby_vision_metadata_state,
            DolbyVisionMetadataState::Incomplete
        );
        assert!(stream
            .dolby_vision_metadata_hint
            .as_deref()
            .is_some_and(|hint| hint.contains("Emby")));
    }

    #[tokio::test]
    async fn provider_stream_persistence_round_trips_dolby_vision_and_hdr_fields() {
        use ersatzrs_domain::MediaStreamKind;
        use sqlx::Row as _;

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open provider stream database");
        sqlx::query(
            r#"CREATE TABLE "MediaStream" (
                "Id" INTEGER PRIMARY KEY,
                "Index" INTEGER NOT NULL, "Codec" TEXT, "Profile" TEXT,
                "MediaStreamKind" INTEGER NOT NULL, "Language" TEXT,
                "Channels" INTEGER NOT NULL, "Title" TEXT, "Default" INTEGER NOT NULL,
                "Forced" INTEGER NOT NULL, "MediaVersionId" INTEGER NOT NULL,
                "PixelFormat" TEXT, "ColorRange" TEXT, "ColorSpace" TEXT,
                "ColorTransfer" TEXT, "ColorPrimaries" TEXT,
                "BitsPerRawSample" INTEGER NOT NULL, "AttachedPic" INTEGER NOT NULL,
                "FileName" TEXT, "MimeType" TEXT, "Rotation" INTEGER,
                "MasteringDisplay" TEXT, "MaxCll" INTEGER, "MaxFall" INTEGER,
                "DolbyVisionProfile" INTEGER, "DolbyVisionBlCompatibilityId" INTEGER,
                "DolbyVisionBlPresent" INTEGER, "DolbyVisionElPresent" INTEGER,
                "DolbyVisionRpuPresent" INTEGER,
                "DolbyVisionMetadataState" INTEGER NOT NULL DEFAULT 0,
                "DolbyVisionMetadataHint" TEXT
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create media stream table");

        let stream = MediaStream {
            index: 0,
            kind: MediaStreamKind::Video,
            codec: Some("hevc".to_owned()),
            profile: Some("Main 10".to_owned()),
            rotation: Some(90),
            mastering_display: Some("G(8500,39850)".to_owned()),
            max_cll: Some(1000),
            max_fall: Some(400),
            dolby_vision_profile: Some(8),
            dolby_vision_bl_compatibility_id: Some(1),
            dolby_vision_bl_present: Some(true),
            dolby_vision_el_present: Some(false),
            dolby_vision_rpu_present: Some(true),
            dolby_vision_metadata_state: DolbyVisionMetadataState::Complete,
            ..MediaStream::default()
        };
        let mut tx = pool.begin().await.expect("begin stream replacement");
        replace_streams(&mut tx, 42, &[stream])
            .await
            .expect("replace provider streams");
        tx.commit().await.expect("commit provider streams");

        let row = sqlx::query(
            r#"SELECT "Rotation", "MasteringDisplay", "MaxCll", "MaxFall",
                      "DolbyVisionProfile", "DolbyVisionBlCompatibilityId",
                      "DolbyVisionBlPresent", "DolbyVisionElPresent",
                      "DolbyVisionRpuPresent", "DolbyVisionMetadataState"
               FROM "MediaStream" WHERE "MediaVersionId" = 42"#,
        )
        .fetch_one(&pool)
        .await
        .expect("reload provider stream");

        assert_eq!(row.get::<i32, _>("Rotation"), 90);
        assert_eq!(row.get::<String, _>("MasteringDisplay"), "G(8500,39850)");
        assert_eq!(row.get::<i32, _>("MaxCll"), 1000);
        assert_eq!(row.get::<i32, _>("MaxFall"), 400);
        assert_eq!(row.get::<i32, _>("DolbyVisionProfile"), 8);
        assert_eq!(row.get::<i32, _>("DolbyVisionBlCompatibilityId"), 1);
        assert_eq!(row.get::<i32, _>("DolbyVisionBlPresent"), 1);
        assert_eq!(row.get::<i32, _>("DolbyVisionElPresent"), 0);
        assert_eq!(row.get::<i32, _>("DolbyVisionRpuPresent"), 1);
        assert_eq!(
            row.get::<i32, _>("DolbyVisionMetadataState"),
            DolbyVisionMetadataState::Complete.as_i32()
        );
    }
}
