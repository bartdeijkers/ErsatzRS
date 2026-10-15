//! Optional local descriptor acquisition, associated with a held file identity.
//!
//! Bookended identity/size/filesystem-mtime equality associates two probe opens;
//! it does not certify immutable bytes, defeat restored in-place mutations, or
//! prove transmitted pages. Windows identity inherits `same-file`'s file-index
//! limitations. The original handle remains held until the caller drops it.

use std::fs::{File, Metadata, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_domain::teletext::SourceTeletextRevision;
use same_file::Handle;
use sha2::{Digest, Sha256};
use tokio::time::Instant;

use super::teletext_probe::{self, TeletextProbeEncoding, TeletextProbePid, TeletextProbeSnapshot};
use super::{
    local, FfprobeCommand, FfprobeError, FfprobeExtractor, FfprobeMediaVersion, FfprobeOutput,
};

pub use headers::{LocalTeletextHeaderProblem, LocalTeletextHeaders};

mod headers;
mod source_window;

pub(crate) use source_window::{
    capture_source_packet_span_owned, capture_source_window_owned, LocalTeletextSourceWindow,
};
pub use source_window::{
    SourceTeletextDecodeWindowEvidence, SourceTeletextPacketSpan, SourceTeletextPacketSpanError,
};

const AUXILIARY_TIMEOUT: Duration = Duration::from_secs(10);

/// Ordinary projection plus an independently optional, report-only association.
#[derive(Debug)]
pub struct LocalMediaProbe {
    pub media: FfprobeMediaVersion,
    pub teletext: Option<Arc<LocalTeletextAcquisition>>,
}

/// Retains the originally opened file through the caller's publication check.
#[derive(Debug)]
pub struct LocalTeletextAcquisition {
    snapshot: TeletextProbeSnapshot,
    revision: SourceTeletextRevision,
    guard: LocalFileRevision,
    headers: LocalTeletextHeaders,
}

/// Holds the current local file associated with an already acquired revision.
///
/// This observes the same file identity, size and filesystem modification time
/// used by local acquisition. It does not certify immutable bytes, prevent a
/// restored in-place mutation, or make acquired page metadata ready for playback.
/// Keep this owner alive through the consuming worker and recheck its actual
/// resolved input path before use. The handle neither locks the file against
/// mutation nor forces an external process to consume this open handle.
pub struct LocalTeletextSourceGuard {
    guard: LocalFileRevision,
}

impl std::fmt::Debug for LocalTeletextSourceGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LocalTeletextSourceGuard(<held association>)")
    }
}

/// Opaque source-association failure, without a private locator or revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LocalTeletextSourceGuardError {
    /// The locator cannot safely establish a regular local file association.
    #[error("local teletext source association is unavailable")]
    Unavailable,
    /// The observed source no longer matches the acquired revision.
    #[error("local teletext source association has changed")]
    Changed,
}

impl LocalTeletextSourceGuard {
    /// Open and retain a regular, nonsymlink source matching an acquired revision.
    ///
    /// This performs synchronous filesystem operations; asynchronous consumers
    /// must use their existing blocking-work owner. A path replacement may later
    /// use a different locator only when [`Self::is_current`] confirms that it
    /// still names this exact physical source with the same observed metadata.
    ///
    /// # Errors
    /// Returns an opaque unavailable failure for unsafe or unreadable locators,
    /// or changed when the observed revision or the bookended path differs.
    pub fn open(
        path: &Path,
        expected_revision: &SourceTeletextRevision,
    ) -> Result<Self, LocalTeletextSourceGuardError> {
        let guard = LocalFileRevision::open(path)
            .map_err(|_| LocalTeletextSourceGuardError::Unavailable)?;
        if guard.revision().as_ref() != Some(expected_revision) || !guard.is_current(path) {
            return Err(LocalTeletextSourceGuardError::Changed);
        }
        Ok(Self { guard })
    }

    /// Check the held file and actual resolved path against the original association.
    ///
    /// A different hard-link locator for the same regular file is accepted; a
    /// symlink, unrelated copy, changed file or missing source is rejected. This
    /// synchronous check retains the acquisition's observed-filesystem limits.
    #[must_use]
    pub fn is_current(&self, resolved_path: &Path) -> bool {
        self.guard.is_current(resolved_path)
    }
}

/// Establish a source association outside the async executor under the current
/// cleanup scope, retaining admitted capacity until the actual worker exits.
pub(crate) async fn open_source_guard_owned(
    original: PathBuf,
    resolved: PathBuf,
    expected: SourceTeletextRevision,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> super::Result<Arc<LocalTeletextSourceGuard>> {
    headers::run_source_guard_owned(deadline, hold, move |stop| {
        stop.check_source_guard()?;
        let guard = LocalTeletextSourceGuard::open(&original, &expected)
            .map_err(|_| source_guard_changed())?;
        check_source_guard_paths(&guard, &original, &resolved)?;
        stop.check_source_guard()?;
        check_source_guard_paths(&guard, &original, &resolved)?;
        stop.check_source_guard()?;
        Ok(Arc::new(guard))
    })
    .await
}

/// Recheck both locators before or after source-consuming work without running
/// filesystem operations on the async executor or abandoning admitted work.
pub(crate) async fn check_source_guard_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    original: PathBuf,
    resolved: PathBuf,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> super::Result<()> {
    headers::run_source_guard_owned(deadline, hold, move |stop| {
        stop.check_source_guard()?;
        check_source_guard_paths(&guard, &original, &resolved)?;
        stop.check_source_guard()
    })
    .await
}

fn check_source_guard_paths(
    guard: &LocalTeletextSourceGuard,
    original: &Path,
    resolved: &Path,
) -> super::Result<()> {
    if !guard.is_current(original) || !guard.is_current(resolved) {
        return Err(source_guard_changed());
    }
    Ok(())
}

fn source_guard_changed() -> FfprobeError {
    FfprobeError::InvalidOutput {
        path: "local teletext source association".into(),
    }
}

impl LocalTeletextAcquisition {
    /// Actual bounded header observations, separate from the packed descriptor projection.
    #[must_use]
    pub fn headers(&self) -> &LocalTeletextHeaders {
        &self.headers
    }

    /// Exact auxiliary projection, without inferred page readiness.
    #[must_use]
    pub fn snapshot(&self) -> &TeletextProbeSnapshot {
        &self.snapshot
    }

    /// Opaque observed filesystem revision, not a source-byte certificate.
    #[must_use]
    pub fn revision(&self) -> &SourceTeletextRevision {
        &self.revision
    }

    /// Compare both the still-held original and a fresh path open. This is an
    /// observed filesystem association, not a source-byte integrity certificate.
    #[must_use]
    pub fn is_current(&self, path: &Path) -> bool {
        self.guard.is_current(path)
    }
}

pub(super) async fn extract(
    extractor: &FfprobeExtractor,
    path: &Path,
) -> super::Result<LocalMediaProbe> {
    extract_with_timeout(extractor, path, AUXILIARY_TIMEOUT).await
}

async fn extract_with_timeout(
    extractor: &FfprobeExtractor,
    path: &Path,
    auxiliary_timeout: Duration,
) -> super::Result<LocalMediaProbe> {
    let guard = LocalFileRevision::open(path).ok();
    let bytes = local::capture(extractor.command_for(path), path.to_owned(), None).await?;
    let primary: FfprobeOutput =
        serde_json::from_slice(&bytes).map_err(|source| FfprobeError::Json {
            path: path.display().to_string(),
            source,
        })?;
    let media = super::project_to_media_version(path, &primary);
    let primary_snapshot = teletext_probe::project(&primary, TeletextProbeEncoding::Ffprobe9Base64);
    let teletext = match guard.filter(|guard| guard.is_current(path)) {
        Some(guard) if identities(&primary_snapshot).is_some_and(|ids| !ids.is_empty()) => {
            acquire(extractor, path, guard, &primary_snapshot, auxiliary_timeout).await?
        }
        _ => None,
    };
    Ok(LocalMediaProbe { media, teletext })
}

async fn acquire(
    extractor: &FfprobeExtractor,
    path: &Path,
    guard: LocalFileRevision,
    primary: &TeletextProbeSnapshot,
    timeout: Duration,
) -> super::Result<Option<Arc<LocalTeletextAcquisition>>> {
    // Check immediately before the second source open as well as after it.
    if !guard.is_current(path) {
        return Ok(None);
    }
    let result = local::capture(
        auxiliary_command(extractor, path),
        path.to_owned(),
        Some(Instant::now() + timeout),
    )
    .await;
    match result {
        Err(error) => optional_failure(error),
        Ok(bytes) => {
            let Ok(auxiliary) = serde_json::from_slice::<FfprobeOutput>(&bytes) else {
                return Ok(None);
            };
            let snapshot =
                teletext_probe::project(&auxiliary, TeletextProbeEncoding::Ffprobe9Base64);
            if identities(primary) != identities(&snapshot) || !guard.is_current(path) {
                return Ok(None);
            }
            let Some(revision) = guard.revision() else {
                return Ok(None);
            };
            let acquired = LocalTeletextAcquisition {
                snapshot,
                revision,
                guard,
                headers: LocalTeletextHeaders::NotAcquired(
                    LocalTeletextHeaderProblem::NoManagedScope,
                ),
            };
            Ok(Some(Arc::new(headers::acquire(acquired, path).await?)))
        }
    }
}

fn optional_failure(error: FfprobeError) -> super::Result<Option<Arc<LocalTeletextAcquisition>>> {
    match error {
        FfprobeError::Cancelled { .. } | FfprobeError::CleanupUnconfirmed { .. } => Err(error),
        _ => Ok(None),
    }
}

fn identities(snapshot: &TeletextProbeSnapshot) -> Option<Vec<(i32, u16)>> {
    let TeletextProbeSnapshot::Available(streams) = snapshot else {
        return None;
    };
    let mut identities = streams
        .iter()
        .map(|stream| match stream.pid {
            TeletextProbePid::Available(pid) => Some((stream.stream_index, pid)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    identities.sort_unstable();
    let mut pids = std::collections::BTreeSet::new();
    identities
        .iter()
        .all(|(_, pid)| pids.insert(*pid))
        .then_some(identities)
}

fn auxiliary_command(extractor: &FfprobeExtractor, path: &Path) -> FfprobeCommand {
    // FFprobe aliases merge nested program streams with global stream options;
    // the output cap bounds these copies, not process memory or restricted keys.
    FfprobeCommand {
        program: extractor.ffprobe_path.clone(),
        args: [
            "-hide_banner", "-print_format", "json", "-select_streams", "s",
            "-show_streams", "-show_programs", "-show_format", "-show_entries",
            "stream=index,id,codec_name,codec_type,extradata,extradata_size:stream_tags=language:program=program_id,program_num,pmt_pid,pcr_pid:program_stream=index:format=format_name",
            "-show_data", "-data_dump_format", "base64", "-i",
        ].into_iter().map(str::to_owned).chain([path.to_string_lossy().into_owned()]).collect(),
    }
}

struct LocalFileRevision {
    original: Handle,
    len: u64,
    modified: SystemTime,
}

impl std::fmt::Debug for LocalFileRevision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LocalFileRevision(<held association>)")
    }
}

impl LocalFileRevision {
    fn open(path: &Path) -> io::Result<Self> {
        let file = open_regular(path)?;
        let metadata = file.metadata()?;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified()?,
            original: Handle::from_file(file)?,
        })
    }

    fn matches(&self, metadata: &Metadata) -> bool {
        metadata.is_file()
            && metadata.len() == self.len
            && metadata.modified().ok() == Some(self.modified)
    }

    fn is_current(&self, path: &Path) -> bool {
        let Ok(held) = self.original.as_file().metadata() else {
            return false;
        };
        if !self.matches(&held) {
            return false;
        }
        let Ok(fresh) = Self::open(path) else {
            return false;
        };
        self.original == fresh.original && self.len == fresh.len && self.modified == fresh.modified
    }

    fn revision(&self) -> Option<SourceTeletextRevision> {
        let mut hash = RevisionHasher(Sha256::new());
        hash.write(b"ersatzrs:local-teletext-observation:v1\0");
        self.original.hash(&mut hash);
        hash.write(&self.len.to_le_bytes());
        let (sign, duration) = match self.modified.duration_since(UNIX_EPOCH) {
            Ok(duration) => (0, duration),
            Err(error) => (1, error.duration()),
        };
        hash.write(&[sign]);
        hash.write(&duration.as_secs().to_le_bytes());
        hash.write(&duration.subsec_nanos().to_le_bytes());
        let mut revision = String::with_capacity(82);
        revision.push_str("local-observed-v1:");
        for byte in hash.0.finalize() {
            use std::fmt::Write as _;
            let _ = write!(revision, "{byte:02x}");
        }
        SourceTeletextRevision::new(revision).ok()
    }
}

fn open_regular(path: &Path) -> io::Result<File> {
    // Existing FFprobe commands carry Unicode strings. A lossy locator could
    // open a different file than this held handle; never bind that observation.
    if path.to_str().is_none() {
        return Err(io::Error::other("local teletext locator is not lossless"));
    }
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::other(
            "local teletext source must be a regular file",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    // Avoid blocking if a regular Unix path is replaced by a FIFO before open.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NONBLOCK);
    }
    // Refuse following a replacement reparse point on Windows. A normal file
    // open has no FIFO semantics; regular-file metadata is checked again below.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other(
            "local teletext source must be a regular file",
        ));
    }
    Ok(file)
}

struct RevisionHasher(Sha256);

impl Hasher for RevisionHasher {
    fn finish(&self) -> u64 {
        let digest = self.0.clone().finalize();
        let mut prefix = [0; 8];
        prefix.copy_from_slice(&digest[..8]);
        u64::from_le_bytes(prefix)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }
}

#[cfg(test)]
#[path = "local_teletext_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "local_teletext_source_guard_tests.rs"]
mod source_guard_tests;
