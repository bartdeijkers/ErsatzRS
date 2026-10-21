//! Process-shared reservations for immutable finite provider input files.
//!
//! Every writer/reader must retain the returned lease. Reserved bytes include
//! all simultaneous copies; file contents are append-only until owned cleanup.
//! Unresolved durable receipts are charged after restart, never reclaimed by age
//! or merely because their OS lease is unlocked. Recovery requires independent
//! proof that every former consumer stopped; this module grants no such proof.
use ersatzrs_core::locking::TeletextWorkHold;
use futures_util::{
    future::{BoxFuture, Shared},
    FutureExt as _,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use uuid::Uuid;

#[path = "provider_source_storage/policy.rs"]
mod policy;
pub use policy::{Policy, MAXIMUM_SOURCE_BYTES_KEY, MINIMUM_FREE_BYTES_KEY, TOTAL_BYTES_KEY};
const MAX_RESERVATIONS: usize = 4096;
const MAX_RECEIPT_BYTES: u64 = 512;
const MAX_CONTENT_ENTRIES: usize = 4096;
const CLEANUP_WAIT: Duration = Duration::from_secs(15);
const BUDGET_LOCK: &str = ".budget.lock";
type Completion = Shared<BoxFuture<'static, Result<(), StorageError>>>;

/// Static failures never reveal source locators, paths or credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    #[error("provider source storage policy is invalid")]
    Policy,
    #[error("provider source storage capacity is unavailable")]
    Capacity,
    #[error("provider source storage admission is busy")]
    Busy,
    #[error("provider source storage ownership is invalid")]
    Invalid,
    #[error("provider source storage is unavailable")]
    Unavailable,
    #[error("provider source storage cleanup is unconfirmed")]
    Cleanup,
}

/// One shared configured root must be used by every participating process.
#[derive(Clone)]
pub struct ProviderSourceStore {
    root: PathBuf,
}
impl ProviderSourceStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Reserve the complete peak before writing any bytes; never evicts input.
    /// Actual filesystem work retains admission after the requester disappears.
    /// # Errors
    /// Rejects invalid limits, exhausted capacity, busy admission, unsafe paths,
    /// malformed/orphaned state and unavailable filesystem work.
    pub async fn reserve(
        &self,
        policy: Policy,
        source_limit: u64,
        peak_bytes: u64,
        hold: TeletextWorkHold,
    ) -> Result<Arc<ProviderSourceLease>, StorageError> {
        validate_limits(policy, source_limit, peak_bytes)?;
        let (owner_tx, owner_rx) = oneshot::channel();
        let (released, last_handle) = oneshot::channel();
        let completion = cleanup_owner(owner_rx, last_handle);
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let _hold = hold;
            let owner = reserve_files(&root, policy, source_limit, peak_bytes)?;
            let directory = owner.directory.clone();
            owner_tx.send(owner).map_err(|_| StorageError::Cleanup)?;
            Ok(Arc::new(ProviderSourceLease {
                directory,
                source_limit,
                peak_bytes,
                completion,
                _released: released,
            }))
        })
        .await
        .map_err(|_| StorageError::Unavailable)?
    }
}

/// Clone this Arc into every actual reader, writer and native process owner.
/// The final handle releases only a signal; an independent owner cleans files.
pub struct ProviderSourceLease {
    directory: PathBuf,
    source_limit: u64,
    peak_bytes: u64,
    completion: ProviderSourceCleanup,
    _released: oneshot::Sender<()>,
}
impl ProviderSourceLease {
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    #[must_use]
    pub const fn source_limit(&self) -> u64 {
        self.source_limit
    }
    #[must_use]
    pub const fn charged_bytes(&self) -> u64 {
        self.peak_bytes
    }
    /// This witness owns no source handle and therefore does not prevent cleanup.
    #[must_use]
    pub fn cleanup(&self) -> ProviderSourceCleanup {
        self.completion.clone()
    }
}

/// Clonable terminal witness; callers must drop all source handles before waiting.
#[derive(Clone)]
pub struct ProviderSourceCleanup {
    completion: Completion,
}
impl ProviderSourceCleanup {
    /// Await actual filesystem completion, including when the original caller left.
    /// # Errors
    /// Failed or cancelled cleanup preserves the durable reservation charge.
    pub async fn wait(&self) -> Result<(), StorageError> {
        self.completion.clone().await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u8,
    id: Uuid,
    source_limit: u64,
    charged_bytes: u64,
}
struct OwnedReservation {
    root: PathBuf,
    directory: PathBuf,
    receipt: Receipt,
    lease: File,
}

fn cleanup_owner(
    owner: oneshot::Receiver<OwnedReservation>,
    last_handle: oneshot::Receiver<()>,
) -> ProviderSourceCleanup {
    let task = tokio::spawn(async move {
        let Ok(owner) = owner.await else {
            return Ok(());
        };
        let _ = last_handle.await;
        tokio::task::spawn_blocking(move || cleanup_files(owner))
            .await
            .map_err(|_| StorageError::Cleanup)?
    });
    ProviderSourceCleanup {
        completion: async move { task.await.map_err(|_| StorageError::Cleanup)? }
            .boxed()
            .shared(),
    }
}
fn validate_limits(policy: Policy, source_limit: u64, peak_bytes: u64) -> Result<(), StorageError> {
    policy.validate()?;
    if source_limit == 0 || source_limit > policy.maximum_source_bytes || peak_bytes < source_limit
    {
        return Err(StorageError::Policy);
    }
    if peak_bytes > policy.total_bytes {
        return Err(StorageError::Capacity);
    }
    Ok(())
}
fn reserve_files(
    root: &Path,
    policy: Policy,
    source_limit: u64,
    peak_bytes: u64,
) -> Result<OwnedReservation, StorageError> {
    fs::create_dir_all(root).map_err(|_| StorageError::Unavailable)?;
    directory(root)?;
    let root = dunce::canonicalize(root).map_err(|_| StorageError::Unavailable)?;
    let _admission = admission_lock(&root)?;
    let accounting = accounting(&root)?;
    let free = fs4::available_space(&root).map_err(|_| StorageError::Unavailable)?;
    check_capacity(policy, accounting, peak_bytes, free)?;
    let receipt = Receipt {
        version: 1,
        id: Uuid::new_v4(),
        source_limit,
        charged_bytes: peak_bytes,
    };
    let directory = root.join(receipt.id.simple().to_string());
    fs::create_dir(&directory).map_err(|_| StorageError::Unavailable)?;
    let lease_path = record_path(&root, receipt.id, "lease");
    let result = (|| {
        let lease = new_lease(&lease_path)?;
        write_receipt(&root, &receipt)?;
        Ok(OwnedReservation {
            root: root.clone(),
            directory: directory.clone(),
            receipt,
            lease,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&lease_path);
        let _ = fs::remove_dir(&directory);
    }
    result
}
fn admission_lock(root: &Path) -> Result<File, StorageError> {
    let path = root.join(BUDGET_LOCK);
    if fs::symlink_metadata(&path)
        .is_ok_and(|metadata| !metadata.is_file() || metadata.is_symlink())
    {
        return Err(StorageError::Invalid);
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|_| StorageError::Unavailable)?;
    match fs4::FileExt::try_lock(&file) {
        Ok(()) => Ok(file),
        Err(fs4::TryLockError::WouldBlock) => Err(StorageError::Busy),
        Err(_) => Err(StorageError::Unavailable),
    }
}
fn new_lease(path: &Path) -> Result<File, StorageError> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| StorageError::Unavailable)?;
    fs4::FileExt::try_lock(&file).map_err(|_| StorageError::Unavailable)?;
    Ok(file)
}
fn record_path(root: &Path, id: Uuid, extension: &str) -> PathBuf {
    root.join(format!("{}.{extension}", id.simple()))
}
fn directory(path: &Path) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::Invalid)?;
    if !metadata.is_dir() || metadata.is_symlink() {
        return Err(StorageError::Invalid);
    }
    Ok(())
}
fn ordinary_file(path: &Path) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::Invalid)?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(StorageError::Invalid);
    }
    Ok(())
}
fn write_receipt(root: &Path, receipt: &Receipt) -> Result<(), StorageError> {
    let mut pending =
        tempfile::NamedTempFile::new_in(root).map_err(|_| StorageError::Unavailable)?;
    serde_json::to_writer(pending.as_file_mut(), receipt).map_err(|_| StorageError::Unavailable)?;
    pending.flush().map_err(|_| StorageError::Unavailable)?;
    pending
        .as_file()
        .sync_all()
        .map_err(|_| StorageError::Unavailable)?;
    pending
        .persist_noclobber(record_path(root, receipt.id, "receipt"))
        .map_err(|_| StorageError::Unavailable)?;
    Ok(())
}
fn read_receipt(path: &Path, id: Uuid) -> Result<Receipt, StorageError> {
    ordinary_file(path)?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| StorageError::Invalid)?
        .take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StorageError::Invalid)?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(StorageError::Invalid);
    }
    let receipt: Receipt = serde_json::from_slice(&bytes).map_err(|_| StorageError::Invalid)?;
    if receipt.version != 1
        || receipt.id != id
        || receipt.source_limit == 0
        || receipt.charged_bytes < receipt.source_limit
    {
        return Err(StorageError::Invalid);
    }
    Ok(receipt)
}

#[derive(Default, Clone, Copy)]
struct Accounting {
    charged: u64,
    physical: u64,
    reservations: usize,
}
#[derive(Default)]
struct Entry {
    directory: Option<PathBuf>,
    receipt: Option<PathBuf>,
}
fn accounting(root: &Path) -> Result<Accounting, StorageError> {
    let mut entries = BTreeMap::<Uuid, Entry>::new();
    for (index, entry) in fs::read_dir(root)
        .map_err(|_| StorageError::Invalid)?
        .enumerate()
    {
        if index > MAX_RESERVATIONS * 3 {
            return Err(StorageError::Capacity);
        }
        let entry = entry.map_err(|_| StorageError::Invalid)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| StorageError::Invalid)?;
        if name == BUDGET_LOCK {
            ordinary_file(&entry.path())?;
            continue;
        }
        let (text, kind) = name
            .split_once('.')
            .map_or((name.as_str(), "directory"), |value| value);
        let id = Uuid::parse_str(text).map_err(|_| StorageError::Invalid)?;
        if id.simple().to_string() != text {
            return Err(StorageError::Invalid);
        }
        let slot = entries.entry(id).or_default();
        match kind {
            "directory" => {
                directory(&entry.path())?;
                slot.directory = Some(entry.path());
            }
            "receipt" => {
                ordinary_file(&entry.path())?;
                slot.receipt = Some(entry.path());
            }
            "lease" => ordinary_file(&entry.path())?,
            _ => return Err(StorageError::Invalid),
        }
    }
    let mut total = Accounting::default();
    for (id, entry) in entries {
        let receipt = read_receipt(&entry.receipt.ok_or(StorageError::Invalid)?, id)?;
        let physical = entry
            .directory
            .map_or(Ok(0), |path| content_bytes(&path, 0, &mut 0))?;
        if physical > receipt.charged_bytes {
            return Err(StorageError::Invalid);
        }
        total.charged = total
            .charged
            .checked_add(receipt.charged_bytes)
            .ok_or(StorageError::Invalid)?;
        total.physical = total
            .physical
            .checked_add(physical)
            .ok_or(StorageError::Invalid)?;
        total.reservations += 1;
    }
    Ok(total)
}
fn content_bytes(path: &Path, depth: usize, count: &mut usize) -> Result<u64, StorageError> {
    if depth > 8 {
        return Err(StorageError::Invalid);
    }
    directory(path)?;
    let mut bytes = 0_u64;
    for entry in fs::read_dir(path).map_err(|_| StorageError::Invalid)? {
        *count += 1;
        if *count > MAX_CONTENT_ENTRIES {
            return Err(StorageError::Invalid);
        }
        let entry = entry.map_err(|_| StorageError::Invalid)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| StorageError::Invalid)?;
        let size = if metadata.is_symlink() {
            return Err(StorageError::Invalid);
        } else if metadata.is_dir() {
            content_bytes(&entry.path(), depth + 1, count)?
        } else if metadata.is_file() {
            metadata.len()
        } else {
            return Err(StorageError::Invalid);
        };
        bytes = bytes.checked_add(size).ok_or(StorageError::Invalid)?;
    }
    Ok(bytes)
}
fn check_capacity(
    policy: Policy,
    current: Accounting,
    peak_bytes: u64,
    free: u64,
) -> Result<(), StorageError> {
    let total = current
        .charged
        .checked_add(peak_bytes)
        .ok_or(StorageError::Capacity)?;
    let outstanding = current
        .charged
        .checked_sub(current.physical)
        .ok_or(StorageError::Invalid)?;
    let required = policy
        .minimum_free_bytes
        .checked_add(outstanding)
        .and_then(|bytes| bytes.checked_add(peak_bytes))
        .ok_or(StorageError::Capacity)?;
    if total > policy.total_bytes || free < required || current.reservations >= MAX_RESERVATIONS {
        return Err(StorageError::Capacity);
    }
    Ok(())
}
fn cleanup_files(owner: OwnedReservation) -> Result<(), StorageError> {
    let deadline = Instant::now() + CLEANUP_WAIT;
    let _admission = loop {
        directory(&owner.root)?;
        match admission_lock(&owner.root) {
            Err(StorageError::Busy) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            result => break result?,
        }
    };
    let receipt_path = record_path(&owner.root, owner.receipt.id, "receipt");
    if read_receipt(&receipt_path, owner.receipt.id)? != owner.receipt {
        return Err(StorageError::Invalid);
    }
    match fs::symlink_metadata(&owner.directory) {
        Ok(_) => {
            content_bytes(&owner.directory, 0, &mut 0)?;
            fs::remove_dir_all(&owner.directory).map_err(|_| StorageError::Cleanup)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(StorageError::Cleanup),
    }
    let lease_path = record_path(&owner.root, owner.receipt.id, "lease");
    ordinary_file(&lease_path)?;
    // Windows also needs the now-unused lease handle closed before unlinking it.
    // The global admission lock still serializes receipt/charge removal.
    drop(owner.lease);
    fs::remove_file(lease_path).map_err(|_| StorageError::Cleanup)?;
    fs::remove_file(receipt_path).map_err(|_| StorageError::Cleanup)?;
    Ok(())
}

#[cfg(test)]
#[path = "provider_source_storage/tests.rs"]
mod tests;
