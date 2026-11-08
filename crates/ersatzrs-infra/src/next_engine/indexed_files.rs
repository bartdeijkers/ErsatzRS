use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use chrono::{DateTime, Utc};

use super::super::io_error;
use super::{
    contract::{
        INDEX_FILENAME, MAX_DAY_BYTES, MAX_INDEX_BYTES, MAX_TOTAL_BYTES, READER_LOCK_FILENAME,
    },
    planner, DayReference, PlayoutIndex, PublicationError,
};
use crate::next_engine::protocol::Playout;

#[derive(Default)]
pub(super) struct CreatedChunks {
    paths: Vec<PathBuf>,
    retained: bool,
}

pub(super) struct ExclusiveReaders {
    _file: File,
}

pub(super) struct RecoveryInventory {
    pub entries: Vec<std::fs::DirEntry>,
    pub bytes: u64,
}

pub(super) enum RecoveryChunk {
    Current,
    Missing,
    Replace(u64),
}

impl CreatedChunks {
    pub(super) fn retain(&mut self) {
        self.retained = true;
    }
}

impl Drop for CreatedChunks {
    fn drop(&mut self) {
        if !self.retained {
            for path in &self.paths {
                if let Err(error) = std::fs::remove_file(path) {
                    // Reclamation retries unreachable owned files on the next
                    // admitted operation; never disclose a media path here.
                    if error.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!(kind = ?error.kind(), "staged Next day cleanup failed");
                    }
                }
            }
        }
    }
}

pub(super) fn prepare_directory(folder: &Path) -> Result<(), PublicationError> {
    std::fs::create_dir_all(folder).map_err(|error| io_error("create index directory", &error))?;
    let kind = std::fs::symlink_metadata(folder)
        .map_err(|error| io_error("inspect index directory", &error))?
        .file_type();
    if !kind.is_dir() || kind.is_symlink() {
        return Err(PublicationError::InvalidIndex);
    }
    Ok(())
}

fn read_plain(path: &Path, limit: usize) -> Result<Vec<u8>, PublicationError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| io_error("inspect indexed file", &error))?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(PublicationError::InvalidIndex);
    }
    if metadata.len() > limit as u64 {
        return Err(PublicationError::Limit);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| io_error("open indexed file", &error))?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read indexed file", &error))?;
    if bytes.len() > limit {
        return Err(PublicationError::Limit);
    }
    Ok(bytes)
}

pub(super) fn load_previous(
    folder: &Path,
    now: DateTime<Utc>,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<Option<planner::Existing>, PublicationError> {
    let path = folder.join(INDEX_FILENAME);
    if matches!(std::fs::symlink_metadata(&path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(None);
    }
    let index: PlayoutIndex = serde_json::from_slice(&read_plain(&path, MAX_INDEX_BYTES)?)
        .map_err(|_| PublicationError::InvalidIndex)?;
    index.validate()?;
    let mut live = BTreeMap::new();
    let mut total = 0;
    let cutoff = now
        .checked_sub_signed(chrono::Duration::minutes(5))
        .ok_or(PublicationError::InvalidWindow)?;
    for reference in &index.days {
        super::check(is_current, deadline)?;
        let bytes = read_plain(
            &folder.join(&reference.file),
            MAX_DAY_BYTES.min(MAX_TOTAL_BYTES - total),
        )?;
        total += bytes.len();
        if planner::digest(&bytes) != reference.sha256 {
            return Err(PublicationError::InvalidIndex);
        }
        let playout: Playout =
            serde_json::from_slice(&bytes).map_err(|_| PublicationError::InvalidIndex)?;
        super::super::validate(&playout).map_err(|_| PublicationError::InvalidIndex)?;
        if playout
            .items
            .iter()
            .any(|item| item.finish <= reference.start || item.start >= reference.finish)
        {
            return Err(PublicationError::InvalidIndex);
        }
        let hash = if reference.finish <= cutoff {
            None
        } else {
            planner::live_hash(&playout, now)?
        };
        live.insert(reference.start, hash);
    }
    Ok(Some(planner::Existing { index, live }))
}

pub(super) fn stage_chunk(
    folder: &Path,
    reference: &DayReference,
    bytes: &[u8],
    created: &mut CreatedChunks,
) -> Result<(), PublicationError> {
    let path = folder.join(&reference.file);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {
            if read_plain(&path, MAX_DAY_BYTES)? != bytes {
                return Err(PublicationError::InvalidIndex);
            }
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("inspect indexed day", &error)),
    }
    let mut pending = tempfile::Builder::new()
        .prefix(".next-day-")
        .tempfile_in(folder)
        .map_err(|error| io_error("stage indexed day", &error))?;
    pending
        .write_all(bytes)
        .map_err(|error| io_error("write indexed day", &error))?;
    pending
        .as_file()
        .sync_all()
        .map_err(|error| io_error("flush indexed day", &error))?;
    pending
        .persist_noclobber(&path)
        .map_err(|error| io_error("publish indexed day", &error.error))?;
    created.paths.push(path);
    Ok(())
}

pub(super) fn reclaim(
    folder: &Path,
    previous: Option<&planner::Existing>,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<(), PublicationError> {
    let lease = exclusive_readers(folder)?;
    let entries = bounded_entries(folder, is_current, deadline)?;
    if entries
        .iter()
        .any(|entry| entry.file_name().to_str().is_some_and(rollback_name))
    {
        return Err(PublicationError::RecoveryRequired);
    }
    let live = previous.map(|old| &old.index);
    reclaim_entries(entries, live, &lease, false, is_current, deadline)
}

pub(super) fn exclusive_readers(folder: &Path) -> Result<ExclusiveReaders, PublicationError> {
    let lock_path = folder.join(READER_LOCK_FILENAME);
    if std::fs::symlink_metadata(&lock_path).is_ok_and(|meta| !meta.is_file() || meta.is_symlink())
    {
        return Err(PublicationError::InvalidIndex);
    }
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|error| io_error("open indexed reader lease", &error))?;
    match lease.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Err(PublicationError::ReadersBusy),
        Err(TryLockError::Error(error)) => return Err(io_error("lock indexed readers", &error)),
    }
    Ok(ExclusiveReaders { _file: lease })
}

pub(super) fn finish_recovery(
    entries: Vec<std::fs::DirEntry>,
    accepted: &PlayoutIndex,
    lease: &ExclusiveReaders,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<(), PublicationError> {
    reclaim_entries(entries, Some(accepted), lease, true, is_current, deadline)
}

fn reclaim_entries(
    entries: Vec<std::fs::DirEntry>,
    accepted: Option<&PlayoutIndex>,
    _lease: &ExclusiveReaders,
    retire_rollback: bool,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<(), PublicationError> {
    let live = accepted
        .map(|index| {
            index
                .days
                .iter()
                .map(|day| day.file.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    // Inspect the complete bounded directory before deleting anything: a
    // retained rollback index may refer to chunks absent from the visible one.
    for entry in entries {
        super::check(is_current, deadline)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if owned_staging(name)
            || (owned_chunk(name) && !live.contains(name))
            || (retire_rollback && rollback_name(name))
        {
            let kind = entry
                .file_type()
                .map_err(|error| io_error("inspect indexed chunk type", &error))?;
            if !kind.is_file() || kind.is_symlink() {
                return Err(PublicationError::InvalidIndex);
            }
            std::fs::remove_file(entry.path())
                .map_err(|error| io_error("reclaim indexed chunk", &error))?;
        }
    }
    Ok(())
}

fn bounded_entries(
    folder: &Path,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<Vec<std::fs::DirEntry>, PublicationError> {
    let mut entries = Vec::new();
    for entry in
        std::fs::read_dir(folder).map_err(|error| io_error("scan indexed chunks", &error))?
    {
        super::check(is_current, deadline)?;
        if entries.len() >= 4096 {
            return Err(PublicationError::Limit);
        }
        let entry = entry.map_err(|error| io_error("inspect indexed chunk", &error))?;
        entries.push(entry);
    }
    Ok(entries)
}

fn rollback_name(name: &str) -> bool {
    name.starts_with(".next-rollback-")
}

pub(super) fn recovery_inventory(
    folder: &Path,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<RecoveryInventory, PublicationError> {
    let entries = bounded_entries(folder, is_current, deadline)?;
    let mut bytes = 0u64;
    for entry in &entries {
        super::check(is_current, deadline)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name == INDEX_FILENAME
            || name == READER_LOCK_FILENAME
            || rollback_name(name)
            || owned_staging(name)
            || owned_chunk(name)
        {
            let metadata = std::fs::symlink_metadata(entry.path())
                .map_err(|error| io_error("inspect recovery state", &error))?;
            if !metadata.is_file() || metadata.is_symlink() {
                return Err(PublicationError::InvalidIndex);
            }
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or(PublicationError::Limit)?;
        }
    }
    Ok(RecoveryInventory { entries, bytes })
}

pub(super) fn inspect_recovery_chunk(
    folder: &Path,
    reference: &DayReference,
    bytes: &[u8],
) -> Result<RecoveryChunk, PublicationError> {
    let path = folder.join(&reference.file);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.is_symlink() {
                return Err(PublicationError::InvalidIndex);
            }
            if metadata.len() <= MAX_DAY_BYTES as u64 && read_plain(&path, MAX_DAY_BYTES)? == bytes
            {
                Ok(RecoveryChunk::Current)
            } else {
                Ok(RecoveryChunk::Replace(metadata.len()))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RecoveryChunk::Missing),
        Err(error) => Err(io_error("inspect recovery chunk", &error)),
    }
}

pub(super) fn repair_chunk(
    path: &Path,
    bytes: &[u8],
    is_new: bool,
    created: &mut CreatedChunks,
    is_current: &impl Fn() -> bool,
    deadline: Option<Instant>,
) -> Result<(), PublicationError> {
    let folder = path.parent().ok_or(PublicationError::InvalidIndex)?;
    let mut file = super::super::prepare_with(
        folder,
        |file| {
            file.write_all(bytes)
                .map_err(|error| io_error("repair indexed day", &error))
        },
        is_current,
        deadline,
    )?;
    path.clone_into(&mut file.destination);
    file.publish(is_current)?;
    if is_new {
        created.paths.push(path.to_owned());
    }
    Ok(())
}

fn owned_staging(name: &str) -> bool {
    [".next-day-", ".next-restore-", ".next-"]
        .iter()
        .any(|prefix| {
            name.strip_prefix(prefix).is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|c| c.is_ascii_alphanumeric())
            })
        })
}

fn owned_chunk(name: &str) -> bool {
    name.strip_prefix("day-")
        .and_then(|name| name.strip_suffix(".json"))
        .is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
}
