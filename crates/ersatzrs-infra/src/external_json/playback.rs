//! Transient playback bindings from `ExternalJsonPlayoutItemProvider`.
//! Library membership is required, but programme/media rows are never inserted.

use std::{collections::BTreeMap, time::Duration};

use chrono::{DateTime, Utc};
use ersatzrs_core::{
    path_replacement::{replace_path, PathPlatform, PathReplacement},
    repositories::{MediaSourceRepository as _, RemoteMediaSourceKind},
};
use ersatzrs_domain::MediaVersion;
use sqlx::Row;
use tokio::{sync::oneshot, time::Instant};

use super::{ExternalJsonError, ExternalJsonProgram};
use crate::{
    channel_playout::load_channel_playout_context,
    db::DbPool,
    ffprobe::{FfprobeExtractor, FfprobeMediaVersion},
    plex::{PlexConnectionConfig, PlexMediaItem},
    repositories::SqliteMediaSourceRepository,
};

/// Redacted failures, including the three upstream Plex precondition diagnostics.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExternalPlaybackError {
    #[error("external JSON database lookup failed")]
    Database,
    #[error(transparent)]
    Schedule(#[from] ExternalJsonError),
    #[error("external JSON schedule file is unavailable")]
    MissingSchedule,
    #[error("external JSON Plex server is unavailable")]
    MissingServer,
    #[error("external JSON Plex library path is unavailable")]
    MissingLibrary,
    #[error("external JSON Plex active connection is unavailable or ambiguous")]
    MissingConnection,
    #[error("external JSON Plex authentication token is unavailable")]
    MissingToken,
    #[error("external JSON Plex media part is invalid")]
    InvalidPart,
    #[error("external JSON Plex metadata is unavailable")]
    Metadata,
    #[error("external JSON local media probe failed")]
    Probe,
    #[error("external JSON playback preparation timed out")]
    Timeout,
    #[error("external JSON playback preparation was cancelled")]
    Cancelled,
}

/// Distinguish an exhausted external timeline from an ordinary stored playout.
/// Consumers must not fall back to stale persisted items for `Unavailable`.
pub enum ExternalPlayoutSelection {
    NotConfigured,
    Unavailable,
    Item(Box<ExternalPlaybackItem>),
}

/// One programme in the requested channel's timeline, including mirror offsets.
#[derive(Clone)]
pub struct ExternalPlaybackItem {
    pub playout_id: i64,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub program: ExternalJsonProgram,
}

/// Playback locators contain no credentials. Remote consumers must use a
/// protected host relay that reads current Plex credentials when opening media.
pub enum ExternalMediaSource {
    Local(String),
    Plex {
        media_source_id: i64,
        part_key: String,
    },
}

/// Fresh metadata plus a transient input binding. Deliberately no `Debug`.
pub struct ResolvedExternalMedia {
    pub occurrence: ExternalPlaybackItem,
    pub source: ExternalMediaSource,
    pub probe: FfprobeMediaVersion,
    pub plex_metadata: Option<Box<PlexMediaItem>>,
}

impl ResolvedExternalMedia {
    /// Bind graphics and dynamic text to this programme's transient metadata.
    #[must_use]
    pub fn graphics_media(&self) -> crate::graphics_template::GraphicsMedia {
        crate::graphics_template::external_media(self)
    }

    /// Original candidates for built-in, YAML and JavaScript track selection.
    #[must_use]
    pub fn tracks(&self) -> crate::playback::transient::TransientPlaybackTracks {
        crate::playback::transient::TransientPlaybackTracks::from_streams(&self.probe.streams)
    }

    /// Match upstream's local episode projection and remote movie/episode shape.
    #[must_use]
    pub fn audio_metadata(&self) -> crate::audio_script::transient::AudioMetadata<'_> {
        use crate::audio_script::transient::AudioMetadata;
        let program = &self.occurrence.program;
        let metadata = self.plex_metadata.as_deref();
        if let Some(movie) = metadata.filter(|_| program.kind.as_deref() != Some("episode")) {
            AudioMetadata::Movie {
                title: Some(&movie.title),
                guids: &movie.guids,
            }
        } else {
            AudioMetadata::Episode {
                show_title: program.show_title.as_deref(),
                show_guids: &[],
                season: metadata.map_or(program.season, |item| item.season_number),
                episode: metadata.map_or(program.episode, |item| item.episode_number),
                guids: metadata.map_or(&[], |item| item.guids.as_slice()),
            }
        }
    }
}

/// Select the current external programme without touching persisted timeline rows.
///
/// # Errors
/// Returns redacted database, schedule and timestamp validation failures.
pub async fn select(
    pool: &DbPool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<ExternalPlayoutSelection, ExternalPlaybackError> {
    let Some(context) = load_channel_playout_context(pool, channel_id)
        .await
        .map_err(|_| ExternalPlaybackError::Database)?
    else {
        return Ok(ExternalPlayoutSelection::NotConfigured);
    };
    let row = sqlx::query(
        r#"SELECT "Id", "ScheduleFile" FROM "Playout"
           WHERE "ChannelId" = ? AND "ScheduleKind" IN (20, 'ExternalJson')
           ORDER BY "Id" LIMIT 1"#,
    )
    .bind(context.source_channel_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ExternalPlaybackError::Database)?;
    let Some(row) = row else {
        return Ok(ExternalPlayoutSelection::NotConfigured);
    };
    let playout_id = row
        .try_get("Id")
        .map_err(|_| ExternalPlaybackError::Database)?;
    let path = row
        .try_get::<Option<String>, _>("ScheduleFile")
        .map_err(|_| ExternalPlaybackError::Database)?
        .filter(|path| !path.trim().is_empty())
        .ok_or(ExternalPlaybackError::MissingSchedule)?;
    let schedule = super::load(std::path::Path::new(&path))
        .await?
        .ok_or(ExternalPlaybackError::MissingSchedule)?;
    let source_now = context
        .source_time(now)
        .map_err(|_| ExternalJsonError::TimeRange)?;
    let Some(item) = schedule.active_item(source_now) else {
        return Ok(ExternalPlayoutSelection::Unavailable);
    };
    Ok(ExternalPlayoutSelection::Item(Box::new(
        ExternalPlaybackItem {
            playout_id,
            start: context
                .target_time(item.start)
                .map_err(|_| ExternalJsonError::TimeRange)?,
            finish: context
                .target_time(item.finish)
                .map_err(|_| ExternalJsonError::TimeRange)?,
            program: item.program.clone(),
        },
    )))
}

/// Prefer an available server-mapped local file, otherwise resolve Plex metadata.
/// Cancellation of a local probe awaits the existing probe owner's cleanup.
/// No database writes or provider credentials escape the resolver.
///
/// # Errors
/// Returns redacted precondition, provider, probe, cancellation or budget errors.
pub async fn resolve(
    pool: &DbPool,
    occurrence: ExternalPlaybackItem,
    extractor: &FfprobeExtractor,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
) -> Result<ResolvedExternalMedia, ExternalPlaybackError> {
    let prepared = tokio::select! {
        biased;
        _ = &mut cancellation => Err(ExternalPlaybackError::Cancelled),
        result = tokio::time::timeout_at(deadline, prepare(pool, &occurrence.program)) =>
            result.map_err(|_| ExternalPlaybackError::Timeout)?,
    };
    let prepared = prepared.inspect_err(|error| {
        tracing::warn!(playout_id = occurrence.playout_id, %error, "Unable to prepare external JSON programme");
    })?;
    let (source, probe, plex_metadata) = match prepared {
        PreparedSource::Local(path) => {
            let input = ersatzrs_ffmpeg::InputFile::video(&path);
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(15));
            let probe = extractor
                .extract_input_cancellable(&input, remaining, cancellation)
                .await
                .map_err(|error| match error {
                    crate::ffprobe::FfprobeError::Cancelled { .. } => {
                        ExternalPlaybackError::Cancelled
                    }
                    crate::ffprobe::FfprobeError::Timeout { .. } => ExternalPlaybackError::Timeout,
                    _ => ExternalPlaybackError::Probe,
                })?;
            (ExternalMediaSource::Local(path), probe, None)
        }
        PreparedSource::Plex {
            media_source_id,
            part_key,
            metadata,
        } => {
            let probe = plex_probe(&metadata);
            (
                ExternalMediaSource::Plex {
                    media_source_id,
                    part_key,
                },
                probe,
                Some(metadata),
            )
        }
    };
    Ok(ResolvedExternalMedia {
        occurrence,
        source,
        probe,
        plex_metadata,
    })
}

enum PreparedSource {
    Local(String),
    Plex {
        media_source_id: i64,
        part_key: String,
        metadata: Box<PlexMediaItem>,
    },
}

async fn prepare(
    pool: &DbPool,
    program: &ExternalJsonProgram,
) -> Result<PreparedSource, ExternalPlaybackError> {
    let row = sqlx::query(r#"SELECT "Id", "ClientIdentifier" FROM "PlexMediaSource" WHERE "ServerName" = ? ORDER BY "Id" LIMIT 1"#)
        .bind(program.server_key.as_deref()).fetch_optional(pool).await
        .map_err(|_| ExternalPlaybackError::Database)?.ok_or(ExternalPlaybackError::MissingServer)?;
    let id: i64 = row
        .try_get("Id")
        .map_err(|_| ExternalPlaybackError::Database)?;
    let client: Option<String> = row
        .try_get("ClientIdentifier")
        .map_err(|_| ExternalPlaybackError::Database)?;
    let library: Option<i64> = sqlx::query_scalar(
        r#"SELECT lp."Id" FROM "LibraryPath" lp JOIN "Library" l ON l."Id" = lp."LibraryId"
           JOIN "PlexLibrary" plex ON plex."Id" = l."Id"
           WHERE l."MediaSourceId" = ? ORDER BY lp."Id" LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ExternalPlaybackError::Database)?;
    if library.is_none() {
        return Err(ExternalPlaybackError::MissingLibrary);
    }
    if let Some(path) = program
        .file
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        let replacements = SqliteMediaSourceRepository(pool.clone())
            .list_path_replacements_admin(RemoteMediaSourceKind::Plex, id)
            .await
            .map_err(|_| ExternalPlaybackError::Database)?
            .into_iter()
            .map(|replacement| PathReplacement {
                source_platform: if replacement.remote_path.contains('\\')
                    || replacement.remote_path.contains(':')
                {
                    PathPlatform::Windows
                } else {
                    PathPlatform::Unix
                },
                source_path: replacement.remote_path,
                replacement_path: replacement.local_path,
            })
            .collect::<Vec<_>>();
        let path = replace_path(&replacements, path, platform());
        if tokio::fs::metadata(&path)
            .await
            .is_ok_and(|metadata| metadata.is_file())
        {
            return Ok(PreparedSource::Local(path));
        }
    }
    let connections: Vec<Option<String>> = sqlx::query_scalar(
        r#"SELECT "Uri" FROM "PlexConnection" WHERE "PlexMediaSourceId" = ? AND "IsActive" = 1 ORDER BY "Id" LIMIT 2"#,
    ).bind(id).fetch_all(pool).await.map_err(|_| ExternalPlaybackError::Database)?;
    let [Some(uri)] = connections.as_slice() else {
        return Err(ExternalPlaybackError::MissingConnection);
    };
    let token = crate::plex::load_plex_token(pool, client.as_deref().unwrap_or_default())
        .await
        .map_err(|error| match error {
            ersatzrs_core::repositories::RepositoryError::Decode {
                kind: "plex config",
                ..
            } => ExternalPlaybackError::MissingToken,
            _ => ExternalPlaybackError::Database,
        })?;
    let part_key = part_key(program.plex_file.as_deref().unwrap_or_default())?;
    let config = PlexConnectionConfig {
        uri: uri.clone(),
        token,
    };
    let metadata =
        crate::plex::external::fetch(&config, program.rating_key.as_deref().unwrap_or_default())
            .await
            .map_err(|()| ExternalPlaybackError::Metadata)?;
    Ok(PreparedSource::Plex {
        media_source_id: id,
        part_key,
        metadata: Box::new(metadata),
    })
}

pub(crate) fn part_key(value: &str) -> Result<String, ExternalPlaybackError> {
    let key = format!("/{}", value.trim_start_matches('/'));
    let base = reqwest::Url::parse("http://external.invalid")
        .map_err(|_| ExternalPlaybackError::InvalidPart)?;
    let url = base
        .join(&key)
        .map_err(|_| ExternalPlaybackError::InvalidPart)?;
    if url.origin() != base.origin()
        || !url.path().starts_with("/library/parts/")
        || url.fragment().is_some()
        || value.contains(['\r', '\n', '\\'])
    {
        return Err(ExternalPlaybackError::InvalidPart);
    }
    Ok(format!(
        "{}{}",
        url.path(),
        url.query()
            .map_or_else(String::new, |query| format!("?{query}"))
    ))
}

fn plex_probe(metadata: &PlexMediaItem) -> FfprobeMediaVersion {
    FfprobeMediaVersion {
        version: MediaVersion {
            duration: metadata.duration,
            width: metadata.width,
            height: metadata.height,
            sample_aspect_ratio: metadata.sample_aspect_ratio.clone(),
            display_aspect_ratio: metadata.display_aspect_ratio.clone(),
            r_frame_rate: metadata.r_frame_rate.clone(),
            video_scan_kind: metadata.video_scan_kind,
            date_added: metadata.date_added,
            date_updated: metadata.date_updated,
            ..MediaVersion::default()
        },
        streams: metadata.streams.clone(),
        field_orders: BTreeMap::new(),
        chapters: Vec::new(),
        format_metadata: BTreeMap::new(),
        format_name: None,
    }
}

#[cfg(windows)]
const fn platform() -> PathPlatform {
    PathPlatform::Windows
}
#[cfg(not(windows))]
const fn platform() -> PathPlatform {
    PathPlatform::Unix
}
