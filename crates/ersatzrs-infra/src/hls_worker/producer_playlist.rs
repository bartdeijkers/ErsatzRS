//! One native segment producer's private list, appended to the existing public HLS timeline.

use std::{io, io::Write as _, path::Path, sync::Arc};

use chrono::{DateTime, Utc};
use ersatzrs_ffmpeg::{HlsMpegTsSegments, OutputFormat};
use tempfile::TempDir;
use tokio::{io::AsyncReadExt as _, sync::Mutex};

use super::{
    normalize_hls_ts_continuity, parse_extinf_duration, parse_playlist, render_trimmed_playlist,
    select_retained_segments, HlsSessionWorker, ParsedPlaylist, PlaylistItem, PlaylistSegment,
    PLAYLIST_FILTER_BEFORE_SECONDS, TS_PACKET_SIZE,
};

const MAX_PLAYLIST_BYTES: u64 = 1024 * 1024;

/// The immutable predecessor timeline and the files belonging to one actual child.
/// The directory is private; only completed, numbered TS files enter live.m3u8.
pub(super) struct MpegTsProducerPlaylist {
    directory: Arc<TempDir>,
    prefix: ParsedPlaylist,
    start: DateTime<Utc>,
    start_number: u64,
    observed: Mutex<Vec<PlaylistSegment>>,
}

impl MpegTsProducerPlaylist {
    pub(super) async fn prepare(
        working_dir: &Path,
        playlist_start: DateTime<Utc>,
    ) -> io::Result<Self> {
        let prefix = match read_playlist(&working_dir.join("live.m3u8")).await? {
            Some(body) => checked_playlist(&body, playlist_start)?.ok_or_else(invalid_list)?,
            None => ParsedPlaylist {
                version: "3".into(),
                target_duration: 1,
                items: Vec::new(),
            },
        };
        let mut start = playlist_start;
        let mut highest = None::<u64>;
        for segment in segments(&prefix) {
            let duration = parse_extinf_duration(&segment.extinf_line).ok_or_else(invalid_list)?;
            start = segment
                .start_time
                .checked_add_signed(duration)
                .ok_or_else(invalid_list)?;
            highest = Some(highest.map_or(segment.sequence, |value| value.max(segment.sequence)));
        }
        let start_number = highest
            .map_or(Some(0), |value| value.checked_add(1))
            .filter(|value| i32::try_from(*value).is_ok())
            .ok_or_else(invalid_list)?;
        // A stale tail absent from the producer playlist cannot stand in for this child's close witness.
        // Skipping its numbers would also break the public media sequence after trimming.
        let mut entries = tokio::fs::read_dir(working_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry
                .file_name()
                .to_str()
                .and_then(owned_sequence)
                .is_some_and(|sequence| sequence >= start_number)
            {
                return Err(invalid_list());
            }
        }
        let directory = tempfile::Builder::new()
            .prefix(".mpegts-producer-")
            .tempdir_in(working_dir)?;
        Ok(Self {
            directory: Arc::new(directory),
            prefix,
            start,
            start_number,
            observed: Mutex::new(Vec::new()),
        })
    }

    pub(super) fn directory_owner(&self) -> Arc<TempDir> {
        Arc::clone(&self.directory)
    }

    pub(super) fn options(&self) -> io::Result<HlsMpegTsSegments> {
        let directory = self
            .directory
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(invalid_list)?;
        Ok(HlsMpegTsSegments {
            producer_playlist: format!("{directory}/producer.m3u8"),
            start_number: self.start_number,
        })
    }

    pub(super) async fn publish(
        &self,
        worker: &HlsSessionWorker,
        completed: bool,
    ) -> io::Result<()> {
        self.publish_at(worker, completed, Utc::now()).await
    }

    async fn publish_at(
        &self,
        worker: &HlsSessionWorker,
        completed: bool,
        now: DateTime<Utc>,
    ) -> io::Result<()> {
        let Some(body) = read_playlist(&self.directory.path().join("producer.m3u8")).await? else {
            return if completed {
                Err(invalid_list())
            } else {
                Ok(())
            };
        };
        let Some(mut producer) = checked_playlist(&body, self.start)? else {
            return if completed {
                Err(invalid_list())
            } else {
                Ok(())
            };
        };
        let mut observed = self.observed.lock().await;
        let listed = segments(&producer);
        validate_run(&listed, self.start_number, &observed)?;
        if listed.len() < observed.len() {
            return Ok(());
        }
        validate_public_prefix(worker, &self.prefix, &listed).await?;
        let closed = closed_segments(&worker.working_dir, &listed, completed).await?;
        *observed = listed;
        if closed.is_empty() {
            return if completed {
                Err(invalid_list())
            } else {
                Ok(())
            };
        }
        producer.items.retain(|item| match item {
            PlaylistItem::Segment(segment) => closed
                .last()
                .is_some_and(|last| segment.sequence <= last.sequence),
            PlaylistItem::Discontinuity => true,
        });
        let merged = merge(&self.prefix, producer);
        let retained = select_retained_segments(
            &segments(&merged),
            now - chrono::Duration::seconds(PLAYLIST_FILTER_BEFORE_SECONDS),
            None,
        );
        validate_retained_files(&worker.working_dir, &retained).await?;
        let trimmed = render_trimmed_playlist(OutputFormat::Hls, &merged, &retained, None)
            .ok_or_else(invalid_list)?;
        let mut continuity = worker.ts_continuity.lock().await;
        normalize_hls_ts_continuity(&worker.working_dir, &trimmed, &mut continuity).await?;
        // This tiny bounded manifest is closed before the cross-platform replace.
        atomic_playlist(
            &self.directory,
            &worker.working_dir.join("live.m3u8"),
            &trimmed.playlist,
        )?;
        *worker.playlist_start.lock().await = trimmed.playlist_start;
        Ok(())
    }
}

async fn read_playlist(path: &Path) -> io::Result<Option<String>> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_PLAYLIST_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > MAX_PLAYLIST_BYTES {
        return Err(invalid_list());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| invalid_list())
}

fn checked_playlist(body: &str, start: DateTime<Utc>) -> io::Result<Option<ParsedPlaylist>> {
    if !body.ends_with('\n') {
        return Ok(None);
    }
    if body.lines().next() != Some("#EXTM3U") {
        return Err(invalid_list());
    }
    let mut count = 0;
    let mut end = start;
    for line in body.lines().filter(|line| line.starts_with("#EXTINF:")) {
        let seconds = line
            .trim_end_matches(',')
            .split_once(':')
            .and_then(|(_, value)| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(invalid_list)?;
        let duration =
            std::time::Duration::try_from_secs_f64(seconds).map_err(|_| invalid_list())?;
        let duration = chrono::Duration::from_std(duration).map_err(|_| invalid_list())?;
        end = end.checked_add_signed(duration).ok_or_else(invalid_list)?;
        count += 1;
    }
    let parsed = parse_playlist(body, start, false)
        .filter(|parsed| parsed.target_duration > 0)
        .ok_or_else(invalid_list)?;
    if segments(&parsed).len() != count {
        return Ok(None);
    }
    Ok(Some(parsed))
}

fn segments(parsed: &ParsedPlaylist) -> Vec<PlaylistSegment> {
    parsed
        .items
        .iter()
        .filter_map(|item| match item {
            PlaylistItem::Segment(segment) => Some(segment.clone()),
            PlaylistItem::Discontinuity => None,
        })
        .collect()
}

fn owned_sequence(uri: &str) -> Option<u64> {
    let digits = uri.strip_prefix("live")?.strip_suffix(".ts")?;
    if digits.len() < 6 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let sequence: u64 = digits.parse().ok()?;
    (uri == format!("live{sequence:06}.ts")).then_some(sequence)
}

fn validate_run(
    listed: &[PlaylistSegment],
    start: u64,
    previous: &[PlaylistSegment],
) -> io::Result<()> {
    for (index, segment) in listed.iter().enumerate() {
        let expected = start
            .checked_add(u64::try_from(index).map_err(|_| invalid_list())?)
            .ok_or_else(invalid_list)?;
        if owned_sequence(&segment.uri) != Some(expected)
            || segment.sequence != expected
            || segment.map_uri.is_some()
            || previous
                .get(index)
                .is_some_and(|previous| previous != segment)
        {
            return Err(invalid_list());
        }
    }
    Ok(())
}

async fn closed_segments(
    working_dir: &Path,
    listed: &[PlaylistSegment],
    completed: bool,
) -> io::Result<Vec<PlaylistSegment>> {
    let mut closed = Vec::new();
    for (index, segment) in listed.iter().enumerate() {
        let last = index + 1 == listed.len();
        if last && !completed {
            let next = segment.sequence.checked_add(1).ok_or_else(invalid_list)?;
            if !tokio::fs::try_exists(working_dir.join(format!("live{next:06}.ts"))).await? {
                break;
            }
        }
        closed.push(segment.clone());
    }
    Ok(closed)
}

async fn validate_retained_files(
    working_dir: &Path,
    retained: &[PlaylistSegment],
) -> io::Result<()> {
    for segment in retained {
        let metadata = tokio::fs::symlink_metadata(working_dir.join(&segment.uri)).await?;
        if !metadata.file_type().is_file()
            || metadata.len() == 0
            || metadata.len() % TS_PACKET_SIZE as u64 != 0
        {
            return Err(invalid_list());
        }
    }
    Ok(())
}

async fn validate_public_prefix(
    worker: &HlsSessionWorker,
    prefix: &ParsedPlaylist,
    run: &[PlaylistSegment],
) -> io::Result<()> {
    let Some(body) = read_playlist(&worker.working_dir.join("live.m3u8")).await? else {
        return Ok(());
    };
    let start = *worker.playlist_start.lock().await;
    let current = checked_playlist(&body, start)?.ok_or_else(invalid_list)?;
    // Trimming may remove old entries; another producer may not replace them.
    let prefix = segments(prefix);
    for segment in segments(&current) {
        if !prefix.iter().chain(run).any(|known| known == &segment) {
            return Err(invalid_list());
        }
    }
    Ok(())
}

fn merge(prefix: &ParsedPlaylist, mut producer: ParsedPlaylist) -> ParsedPlaylist {
    let mut items = prefix.items.clone();
    items.append(&mut producer.items);
    ParsedPlaylist {
        version: prefix
            .version
            .parse::<u32>()
            .unwrap_or(3)
            .max(producer.version.parse::<u32>().unwrap_or(3))
            .to_string(),
        target_duration: prefix.target_duration.max(producer.target_duration),
        items,
    }
}

fn atomic_playlist(directory: &TempDir, destination: &Path, body: &str) -> io::Result<()> {
    if body.len() as u64 > MAX_PLAYLIST_BYTES {
        return Err(invalid_list());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory.path())?;
    temporary.write_all(body.as_bytes())?;
    temporary.flush()?;
    temporary
        .into_temp_path()
        .persist(destination)
        .map_err(|error| error.error)
}

fn invalid_list() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid private MPEG-TS producer playlist",
    )
}

#[cfg(test)]
mod tests;
