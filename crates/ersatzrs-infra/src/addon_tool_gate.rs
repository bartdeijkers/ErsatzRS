//! Cross-process tool use/update exclusion with conservative cleanup receipts.
//!
//! OS locks coordinate live callers. A receipt outlives a dropped lease because
//! host death does not prove that a spawned process tree has stopped. Only the
//! caller holding process-tree cleanup evidence may confirm cleanup. This module
//! never discovers, kills, or recovers processes.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use sha2::{Digest as _, Sha256};

/// Distinguishes unresolved process cleanup from ordinary live lock contention.
pub const UNRESOLVED_CLEANUP_MESSAGE: &str = "add-on tool cleanup is unresolved";
const STORAGE_ERROR: &str = "add-on tool gate storage is unavailable";
const INVALID_STORAGE: &str = "add-on tool gate storage is invalid";
const MAX_TOOLS: usize = 64;
const MAX_RECEIPTS: usize = 4096;

/// Ordinary tool use shares access; an update requires immediate exclusive access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolLeaseMode {
    /// Multiple invocations may use the same executable concurrently.
    Use,
    /// Never wait for or queue an exclusive lock ahead of playback.
    Update,
}

impl ToolLeaseMode {
    const fn directory(self) -> &'static str {
        match self {
            Self::Use => "use",
            Self::Update => "update",
        }
    }
}

/// Holds every executable lock and its durable pre-spawn receipt.
///
/// Dropping releases OS locks but deliberately retains receipts. Call
/// [`Self::confirm_cleanup`] only after a failed spawn or confirmed process-tree
/// cleanup. Synchronous filesystem operations should run off async workers.
pub struct ToolLease {
    locks: Vec<File>,
    receipts: Vec<PathBuf>,
}

impl std::fmt::Debug for ToolLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolLease")
            .field("tools", &self.locks.len())
            .finish_non_exhaustive()
    }
}

impl ToolLease {
    /// Acquire all sorted, canonical executable identities without waiting.
    ///
    /// `Ok(None)` means another live lease holds an incompatible lock. An empty
    /// tool set returns an empty lease. All locks are acquired and old receipts
    /// checked before new receipts are written, so contention on a later tool
    /// cannot leave a receipt for an invocation that never started.
    ///
    /// # Errors
    /// Returns a redacted I/O error for inaccessible or unsafe owned storage.
    /// `ErrorKind::Other` with [`UNRESOLVED_CLEANUP_MESSAGE`] means retained use
    /// receipts prohibit an update, or a retained update prohibits either mode.
    pub fn try_acquire(
        root: &Path,
        tool_paths: &[PathBuf],
        mode: ToolLeaseMode,
    ) -> io::Result<Option<Self>> {
        if tool_paths.len() > MAX_TOOLS {
            return Err(invalid());
        }
        let identities = tool_paths
            .iter()
            .map(|path| tool_identity(path))
            .collect::<io::Result<BTreeSet<_>>>()?;
        let mut lease = Self {
            locks: Vec::with_capacity(identities.len()),
            receipts: Vec::with_capacity(identities.len()),
        };
        if identities.is_empty() {
            return Ok(Some(lease));
        }
        // The supplied root is a trusted configuration boundary, as for the
        // existing add-on stores. All entries below it must be ordinary files
        // or directories, never symlinks.
        fs::create_dir_all(root).map_err(redacted)?;
        let root = dunce::canonicalize(root).map_err(redacted)?;
        let mut stores = Vec::with_capacity(identities.len());
        for identity in identities {
            let store = root.join(identity);
            ensure_directory(&store)?;
            let lock = open_lock(&store.join("lease.lock"))?;
            let acquired = match mode {
                ToolLeaseMode::Use => fs4::FileExt::try_lock_shared(&lock),
                ToolLeaseMode::Update => fs4::FileExt::try_lock(&lock),
            };
            match acquired {
                Ok(()) => lease.locks.push(lock),
                Err(fs4::TryLockError::WouldBlock) => return Ok(None),
                Err(_) => return Err(io::Error::other(STORAGE_ERROR)),
            }
            stores.push(store);
        }
        for store in &stores {
            let uses = store.join("use");
            let updates = store.join("update");
            ensure_directory(&uses)?;
            ensure_directory(&updates)?;
            // Shared holders may add/remove their own use receipts concurrently.
            // Exclusive ownership proves any remaining use receipt is unresolved.
            // Playback does not enumerate old use receipts: their number must
            // not become an accidental playback admission limit.
            if has_receipts(&updates)? || (mode == ToolLeaseMode::Update && has_receipts(&uses)?) {
                return Err(io::Error::other(UNRESOLVED_CLEANUP_MESSAGE));
            }
        }
        let invocation = uuid::Uuid::new_v4().simple().to_string();
        for store in stores {
            let receipt = store
                .join(mode.directory())
                .join(format!("{invocation}.receipt"));
            let result = lease.create_receipt(receipt);
            if let Err(error) = result {
                // No process can have spawned before this function returns. It
                // is therefore safe to remove this failed admission's receipts.
                lease.confirm_cleanup()?;
                return Err(error);
            }
        }
        Ok(Some(lease))
    }

    fn create_receipt(&mut self, path: PathBuf) -> io::Result<()> {
        let mut receipt = File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(redacted)?;
        self.receipts.push(path);
        receipt.write_all(b"1\n").map_err(redacted)?;
        receipt.sync_all().map_err(redacted)
    }

    /// Remove only this invocation's receipts while all OS locks remain held.
    ///
    /// Call only when spawning failed or the complete process tree has stopped.
    /// Consuming the lease prevents further use without a fresh receipt.
    ///
    /// # Errors
    /// Returns a redacted error if an owned receipt cannot safely be removed.
    /// Remaining receipts continue to prohibit unsafe future updates.
    pub fn confirm_cleanup(self) -> io::Result<()> {
        for receipt in &self.receipts {
            // Check every owned ancestor again before deletion. Do not follow a
            // replaced receipts directory outside the host-owned store.
            let directory = receipt.parent().ok_or_else(invalid)?;
            let store = directory.parent().ok_or_else(invalid)?;
            require_directory(store)?;
            require_directory(directory)?;
            require_file(receipt)?;
            fs::remove_file(receipt).map_err(redacted)?;
        }
        Ok(())
    }
}

fn tool_identity(path: &Path) -> io::Result<String> {
    let canonical = dunce::canonicalize(path).map_err(redacted)?;
    if !fs::metadata(&canonical).map_err(redacted)?.is_file() {
        return Err(invalid());
    }
    hash_identity(&canonical)
}

fn hash_identity(canonical: &Path) -> io::Result<String> {
    use std::fmt::Write as _;
    let mut identity = String::with_capacity(64);
    for byte in Sha256::digest(identity_bytes(canonical)?) {
        write!(identity, "{byte:02x}").map_err(|_| invalid())?;
    }
    Ok(identity)
}

/// Resolve one offline recovery identity, including an interrupted replacement.
///
/// An absent absolute executable uses its canonical existing parent and filename.
/// A known gate hash can instead be supplied directly to [`recover_tool_receipts`].
///
/// # Errors
/// Returns a redacted error for relative, inaccessible, or invalid paths.
pub fn recovery_tool_identity(path: &Path) -> io::Result<String> {
    if !path.is_absolute() {
        return Err(invalid());
    }
    match fs::symlink_metadata(path) {
        Ok(_) => tool_identity(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent =
                dunce::canonicalize(path.parent().ok_or_else(invalid)?).map_err(redacted)?;
            let canonical = parent.join(path.file_name().ok_or_else(invalid)?);
            hash_identity(&canonical)
        }
        Err(error) => Err(redacted(error)),
    }
}

/// Retire one tool's receipts after explicit offline process-cleanup acknowledgement.
///
/// The operator must have stopped every host, scanner, provider and related tool
/// process. Exclusive lock acquisition alone does not prove orphan cleanup. No
/// age heuristic, process killing, executable repair, or media deletion occurs.
///
/// # Errors
/// Rejects missing acknowledgement, unsafe owned entries, malformed identities,
/// and live leases (`WouldBlock`). Validates all receipts before removing any.
pub fn recover_tool_receipts(
    root: &Path,
    identity: &str,
    processes_stopped: bool,
) -> io::Result<usize> {
    if !processes_stopped {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "offline process cleanup acknowledgement is required",
        ));
    }
    if identity.len() != 64
        || !identity
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid());
    }
    let root = match dunce::canonicalize(root) {
        Ok(root) => root,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(redacted(error)),
    };
    let store = root.join(identity);
    match fs::symlink_metadata(&store) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(redacted(error)),
        Ok(_) => require_directory(&store)?,
    }
    let lock_path = store.join("lease.lock");
    require_file(&lock_path)?;
    let lock = File::options()
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(redacted)?;
    match fs4::FileExt::try_lock(&lock) {
        Ok(()) => (),
        Err(fs4::TryLockError::WouldBlock) => {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "add-on tool is in use",
            ))
        }
        Err(_) => return Err(io::Error::other(STORAGE_ERROR)),
    }
    let mut receipts = Vec::new();
    for mode in [ToolLeaseMode::Use, ToolLeaseMode::Update] {
        let directory = store.join(mode.directory());
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(redacted(error)),
            Ok(_) => require_directory(&directory)?,
        }
        for entry in fs::read_dir(&directory).map_err(redacted)? {
            if receipts.len() >= MAX_RECEIPTS {
                return Err(invalid());
            }
            let entry = entry.map_err(redacted)?;
            let name = entry.file_name();
            if !name
                .to_str()
                .and_then(|name| name.strip_suffix(".receipt"))
                .is_some_and(|name| name.len() == 32 && uuid::Uuid::parse_str(name).is_ok())
            {
                return Err(invalid());
            }
            let path = entry.path();
            require_file(&path)?;
            if fs::metadata(&path).map_err(redacted)?.len() > 2 {
                return Err(invalid());
            }
            // A crash during creation may leave a prefix of the version marker.
            if !b"1\n".starts_with(&fs::read(&path).map_err(redacted)?) {
                return Err(invalid());
            }
            receipts.push(path);
        }
    }
    for receipt in &receipts {
        require_directory(&store)?;
        require_directory(receipt.parent().ok_or_else(invalid)?)?;
        require_file(receipt)?;
        fs::remove_file(receipt).map_err(redacted)?;
    }
    // Keep the exclusive lock through validation and retirement of both modes.
    drop(lock);
    Ok(receipts.len())
}

#[cfg(windows)]
fn identity_bytes(path: &Path) -> io::Result<Vec<u8>> {
    // dunce removes verbatim prefixes; case aliases must share the same gate on
    // the supported Windows filesystem. Reject ill-formed Unicode rather than
    // hashing a lossy replacement that could alias a different executable.
    Ok(path
        .to_str()
        .ok_or_else(invalid)?
        .to_lowercase()
        .into_bytes())
}

#[cfg(not(windows))]
#[allow(clippy::unnecessary_wraps)] // Keep the fallible Windows counterpart's signature.
fn identity_bytes(path: &Path) -> io::Result<Vec<u8>> {
    Ok(path.as_os_str().as_encoded_bytes().to_vec())
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(redacted(error)),
    }
    require_directory(path)
}

fn require_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(redacted)?;
    if metadata.is_symlink() || !metadata.is_dir() {
        return Err(invalid());
    }
    Ok(())
}

fn require_file(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(redacted)?;
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(invalid());
    }
    Ok(())
}

fn open_lock(path: &Path) -> io::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_symlink() || !metadata.is_file() => return Err(invalid()),
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(redacted(error)),
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(redacted)?;
    require_file(path)?;
    Ok(file)
}

fn has_receipts(directory: &Path) -> io::Result<bool> {
    for (count, entry) in fs::read_dir(directory).map_err(redacted)?.enumerate() {
        if count >= MAX_RECEIPTS {
            return Err(invalid());
        }
        let entry = entry.map_err(redacted)?;
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            // A concurrent reader may just have confirmed its own cleanup.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(redacted(error)),
        };
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(invalid());
        }
        let name = entry.file_name();
        let valid = name
            .to_str()
            .and_then(|name| name.strip_suffix(".receipt"))
            .is_some_and(|name| name.len() == 32 && uuid::Uuid::parse_str(name).is_ok());
        if !valid {
            return Err(invalid());
        }
        return Ok(true);
    }
    Ok(false)
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, INVALID_STORAGE)
}

#[allow(clippy::needless_pass_by_value)] // map_err boundary discards owned private diagnostics.
fn redacted(error: io::Error) -> io::Error {
    io::Error::new(error.kind(), STORAGE_ERROR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_recovery_requires_acknowledgement_and_is_tool_scoped_and_repeatable() {
        for mode in [ToolLeaseMode::Use, ToolLeaseMode::Update] {
            let (_directory, root, tools) = fixture();
            drop(acquire(&root, &tools, mode));
            let identity = recovery_tool_identity(&tools[0]).unwrap();
            assert_eq!(
                recover_tool_receipts(&root, &identity, false)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            assert_unresolved(&root, &tools[..1], ToolLeaseMode::Update);
            assert_eq!(recover_tool_receipts(&root, &identity, true).unwrap(), 1);
            assert_eq!(recover_tool_receipts(&root, &identity, true).unwrap(), 0);
            acquire(&root, &tools[..1], ToolLeaseMode::Update)
                .confirm_cleanup()
                .unwrap();
            assert_unresolved(&root, &tools[1..], ToolLeaseMode::Update);
            assert_eq!(fs::read(&tools[0]).unwrap(), b"fixture executable identity");
        }
    }

    #[test]
    fn offline_recovery_refuses_live_leases_and_handles_missing_update_executable() {
        let (_directory, root, tools) = fixture();
        let lease = acquire(&root, &tools[..1], ToolLeaseMode::Update);
        let identity = recovery_tool_identity(&tools[0]).unwrap();
        assert_eq!(
            recover_tool_receipts(&root, &identity, true)
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        drop(lease);
        fs::remove_file(&tools[0]).unwrap();
        assert_eq!(recovery_tool_identity(&tools[0]).unwrap(), identity);
        assert_eq!(recover_tool_receipts(&root, &identity, true).unwrap(), 1);
        assert!(!tools[0].exists());
    }

    #[cfg(unix)]
    #[test]
    fn recovery_validates_all_receipts_before_retiring_any() {
        let (_directory, root, tools) = fixture();
        let lease = acquire(&root, &tools[..1], ToolLeaseMode::Use);
        let retained = lease.receipts[0].clone();
        drop(lease);
        let identity = recovery_tool_identity(&tools[0]).unwrap();
        let unsafe_receipt = root
            .join(&identity)
            .join("update")
            .join(format!("{}.receipt", uuid::Uuid::new_v4().simple()));
        std::os::unix::fs::symlink(&tools[0], unsafe_receipt).unwrap();
        assert_eq!(
            recover_tool_receipts(&root, &identity, true)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(retained.is_file());
        assert_eq!(fs::read(&tools[0]).unwrap(), b"fixture executable identity");
    }

    #[test]
    #[ignore = "subprocess fixture invoked by offline_recovery_after_process_death"]
    fn recovery_process_death_fixture() {
        let Some(root) =
            crate::startup_config::environment_variable("ERSATZRS_TOOL_RECOVERY_TEST_ROOT")
        else {
            return;
        };
        let tool = crate::startup_config::environment_variable("ERSATZRS_TOOL_RECOVERY_TEST_TOOL")
            .unwrap();
        let _lease = acquire(
            Path::new(&root),
            &[PathBuf::from(tool)],
            ToolLeaseMode::Update,
        );
        // Exit bypasses Rust destructors, matching abrupt owner death while the
        // OS releases its locks. No provider process is launched by this fixture.
        std::process::exit(0);
    }

    #[test]
    fn offline_recovery_after_process_death() {
        let (_directory, root, tools) = fixture();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "addon_tool_gate::tests::recovery_process_death_fixture",
                "--ignored",
            ])
            .env("ERSATZRS_TOOL_RECOVERY_TEST_ROOT", &root)
            .env("ERSATZRS_TOOL_RECOVERY_TEST_TOOL", &tools[0])
            .status()
            .unwrap();
        assert!(status.success());
        assert_unresolved(&root, &tools[..1], ToolLeaseMode::Use);
        let identity = recovery_tool_identity(&tools[0]).unwrap();
        assert_eq!(recover_tool_receipts(&root, &identity, true).unwrap(), 1);
        acquire(&root, &tools[..1], ToolLeaseMode::Use)
            .confirm_cleanup()
            .unwrap();
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, Vec<PathBuf>) {
        let directory = tempfile::tempdir().expect("fixture directory");
        let root = directory.path().join("gate");
        let tools = ["first-tool", "second-tool"]
            .map(|name| {
                let path = directory.path().join(name);
                fs::write(&path, b"fixture executable identity").expect("fixture file");
                path
            })
            .to_vec();
        (directory, root, tools)
    }

    fn acquire(root: &Path, paths: &[PathBuf], mode: ToolLeaseMode) -> ToolLease {
        ToolLease::try_acquire(root, paths, mode)
            .expect("acquire result")
            .expect("available lease")
    }

    fn assert_unresolved(root: &Path, paths: &[PathBuf], mode: ToolLeaseMode) {
        let error = ToolLease::try_acquire(root, paths, mode).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), UNRESOLVED_CLEANUP_MESSAGE);
    }

    #[test]
    fn readers_coexist_and_nonblocking_updates_do_not_queue_ahead_of_readers() {
        let (_directory, root, tools) = fixture();
        let first = acquire(&root, &tools, ToolLeaseMode::Use);
        assert!(ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update)
            .unwrap()
            .is_none());
        let second = acquire(&root, &tools, ToolLeaseMode::Use);
        first.confirm_cleanup().unwrap();
        assert!(ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Update)
            .unwrap()
            .is_none());
        second.confirm_cleanup().unwrap();
        acquire(&root, &tools, ToolLeaseMode::Update)
            .confirm_cleanup()
            .unwrap();
        acquire(&root, &tools, ToolLeaseMode::Use)
            .confirm_cleanup()
            .unwrap();
    }

    #[test]
    fn dropped_use_blocks_update_but_permits_playback() {
        let (_directory, root, tools) = fixture();
        drop(acquire(&root, &tools, ToolLeaseMode::Use));
        assert_unresolved(&root, &tools, ToolLeaseMode::Update);
        acquire(&root, &tools, ToolLeaseMode::Use)
            .confirm_cleanup()
            .unwrap();
        assert_unresolved(&root, &tools, ToolLeaseMode::Update);
    }

    #[test]
    fn dropped_update_blocks_both_modes() {
        let (_directory, root, tools) = fixture();
        drop(acquire(&root, &tools, ToolLeaseMode::Update));
        assert_unresolved(&root, &tools, ToolLeaseMode::Use);
        assert_unresolved(&root, &tools, ToolLeaseMode::Update);
    }

    #[test]
    fn partial_acquisition_contention_creates_no_receipts() {
        let (_directory, root, mut tools) = fixture();
        tools.sort_by_key(|path| tool_identity(path).unwrap());
        let busy = acquire(&root, &tools[1..], ToolLeaseMode::Update);
        assert!(ToolLease::try_acquire(&root, &tools, ToolLeaseMode::Use)
            .unwrap()
            .is_none());
        // The first tool's shared lock has been released, without a use receipt.
        acquire(&root, &tools[..1], ToolLeaseMode::Update)
            .confirm_cleanup()
            .unwrap();
        busy.confirm_cleanup().unwrap();
        acquire(&root, &tools, ToolLeaseMode::Update)
            .confirm_cleanup()
            .unwrap();
    }

    #[test]
    fn repeated_canonical_paths_take_one_lock_and_receipt() {
        let (_directory, root, tools) = fixture();
        let aliases = vec![tools[0].clone(), tools[0].clone()];
        let lease = acquire(&root, &aliases, ToolLeaseMode::Update);
        assert_eq!(lease.locks.len(), 1);
        assert_eq!(lease.receipts.len(), 1);
        assert!(
            ToolLease::try_acquire(&root, &tools[..1], ToolLeaseMode::Use)
                .unwrap()
                .is_none()
        );
        lease.confirm_cleanup().unwrap();
    }

    // Creating Windows symlinks requires privileges or Developer Mode. The
    // production confinement checks apply on Windows as well as Unix.
    #[cfg(unix)]
    #[test]
    fn executable_aliases_share_identity_and_owned_symlinks_fail_closed() {
        let (directory, root, tools) = fixture();
        let alias = directory.path().join("alias");
        std::os::unix::fs::symlink(&tools[0], &alias).unwrap();
        let lease = acquire(
            &root,
            &[tools[0].clone(), alias.clone()],
            ToolLeaseMode::Use,
        );
        assert_eq!(lease.locks.len(), 1);
        assert!(
            ToolLease::try_acquire(&root, &[alias], ToolLeaseMode::Update)
                .unwrap()
                .is_none()
        );
        lease.confirm_cleanup().unwrap();
        let store = root.join(tool_identity(&tools[0]).unwrap());
        let receipts = store.join("update");
        let receipt = receipts.join(format!("{}.receipt", uuid::Uuid::new_v4().simple()));
        std::os::unix::fs::symlink(&tools[0], receipt).unwrap();
        let error = ToolLease::try_acquire(&root, &tools[..1], ToolLeaseMode::Use).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(&tools[0]).unwrap(), b"fixture executable identity");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_lock_and_store_directories_fail_closed() {
        let (directory, root, tools) = fixture();
        fs::create_dir(&root).unwrap();
        let store = root.join(tool_identity(&tools[0]).unwrap());
        let outside = directory.path().join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, &store).unwrap();
        assert_eq!(
            ToolLease::try_acquire(&root, &tools[..1], ToolLeaseMode::Use)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        fs::remove_file(&store).unwrap();
        fs::create_dir(&store).unwrap();
        std::os::unix::fs::symlink(&tools[0], store.join("lease.lock")).unwrap();
        assert_eq!(
            ToolLease::try_acquire(&root, &tools[..1], ToolLeaseMode::Use)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
