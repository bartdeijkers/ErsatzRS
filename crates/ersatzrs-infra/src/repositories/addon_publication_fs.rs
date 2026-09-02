use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use ersatzrs_domain::EntityId;

pub(crate) const PUBLICATION_MARKER_FILE: &str = ".ersatzrs-publication.json";
const MARKER_SCHEMA_VERSION: u32 = 1;
const CLEANUP_TOMBSTONE_SUFFIX: &str = ".cleanup-marker";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublicationTreeRole {
    Previous,
    Candidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PublicationTreeState {
    Building,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PublicationTreeMarker {
    schema_version: u32,
    list_id: EntityId,
    operation_id: Option<EntityId>,
    generation: Option<i32>,
    role: PublicationTreeRole,
    revision: i32,
    state: PublicationTreeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(default)]
    file_count: u64,
    #[serde(default)]
    byte_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublicationFsIdentity {
    pub(crate) list_id: EntityId,
    pub(crate) operation_id: Option<EntityId>,
    pub(crate) generation: Option<i32>,
    pub(crate) previous_revision: i32,
    pub(crate) target_revision: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicationFsPaths {
    pub(crate) managed: PathBuf,
    pub(crate) backup: PathBuf,
    pub(crate) failed: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TreeFingerprint {
    sha256: String,
    file_count: u64,
    byte_count: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedCandidateSeal {
    expected_marker: PublicationTreeMarker,
    fingerprint: Option<TreeFingerprint>,
}

#[derive(Debug)]
pub(crate) enum PublicationFsError {
    Io {
        action: &'static str,
        source: std::io::Error,
    },
    UnsafeState(&'static str),
    InvalidMarker(String),
    Integrity(&'static str),
}

impl std::fmt::Display for PublicationFsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { action, source } => write!(formatter, "{action}: {source}"),
            Self::UnsafeState(message) | Self::Integrity(message) => formatter.write_str(message),
            Self::InvalidMarker(message) => {
                write!(formatter, "invalid publication marker: {message}")
            }
        }
    }
}

impl std::error::Error for PublicationFsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::UnsafeState(_) | Self::InvalidMarker(_) | Self::Integrity(_) => None,
        }
    }
}

impl PublicationFsError {
    pub(crate) fn raw_os_error(&self) -> Option<i32> {
        match self {
            Self::Io { source, .. } => source.raw_os_error(),
            Self::UnsafeState(_) | Self::InvalidMarker(_) | Self::Integrity(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestoreProgress {
    Advanced,
    Complete,
}

pub(crate) fn publication_paths(
    managed: PathBuf,
    list_id: EntityId,
    operation_id: Option<EntityId>,
    generation: Option<i32>,
) -> Result<PublicationFsPaths, PublicationFsError> {
    let parent = managed
        .parent()
        .ok_or(PublicationFsError::UnsafeState(
            "the managed publication folder has no parent",
        ))?
        .to_path_buf();
    let managed_hash = Sha256::digest(managed.as_os_str().to_string_lossy().as_bytes());
    let mut managed_key = String::with_capacity(12);
    for byte in &managed_hash[..6] {
        write!(&mut managed_key, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let suffix = match (operation_id, generation) {
        (Some(operation_id), Some(generation)) => {
            format!("{list_id}-{operation_id}-{generation}-{managed_key}")
        }
        _ => format!("{list_id}-{managed_key}-{}", uuid::Uuid::new_v4()),
    };
    Ok(PublicationFsPaths {
        managed,
        backup: parent.join(format!(".ersatzrs-publication-{suffix}")),
        failed: parent.join(format!(".ersatzrs-failed-publication-{suffix}")),
    })
}

pub(crate) fn create_backup(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    validate_sibling_paths(paths)?;
    if !paths.managed.is_dir() {
        return Err(PublicationFsError::UnsafeState(
            "the managed publication folder is missing",
        ));
    }
    if paths.failed.exists() {
        return Err(PublicationFsError::UnsafeState(
            "a failed publication folder already exists before staging",
        ));
    }
    if resume_existing_backup(paths, identity)? {
        return Ok(());
    }

    let initializing = backup_initializing_path(&paths.backup)?;
    if resume_backup_initializer(paths, identity, &initializing)? {
        return Ok(());
    }

    fs::create_dir(&initializing)
        .map_err(|source| io_error("create publication backup initializer", source))?;
    write_marker(
        &initializing,
        &building_marker(
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
        ),
    )?;
    copy_tree(&paths.managed, &initializing, Path::new(""))?;
    let fingerprint = fingerprint_tree(&initializing)?;
    write_marker(
        &initializing,
        &complete_marker(
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
            &fingerprint,
        ),
    )?;
    fs::rename(&initializing, &paths.backup)
        .map_err(|source| io_error("publish initialized publication backup", source))?;
    write_candidate_marker(&paths.managed, identity)
}

fn resume_existing_backup(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<bool, PublicationFsError> {
    if let Some(marker) = cleanup_tombstone_marker(&paths.backup)? {
        ensure_previous_marker_owned(
            &marker,
            identity,
            "the publication backup cleanup marker belongs to another journal",
        )?;
        remove_owned_tree(&paths.backup, &marker)?;
    }
    if !paths.backup.exists() {
        return Ok(false);
    }
    if !paths.backup.join(PUBLICATION_MARKER_FILE).is_file() {
        remove_empty_initializer(
            &paths.backup,
            "the publication backup has no ownership marker",
        )?;
        return Ok(false);
    }

    let marker = read_marker(&paths.backup)?;
    ensure_previous_marker_owned(
        &marker,
        identity,
        "the publication backup is not owned by this journal",
    )?;
    match marker.state {
        PublicationTreeState::Building => {
            remove_owned_tree(&paths.backup, &marker)?;
            Ok(false)
        }
        PublicationTreeState::Complete => {
            verify_fingerprint(&paths.backup, &marker)?;
            verify_fingerprint(&paths.managed, &marker)?;
            write_candidate_marker(&paths.managed, identity)?;
            Ok(true)
        }
    }
}

fn resume_backup_initializer(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    initializing: &Path,
) -> Result<bool, PublicationFsError> {
    if let Some(marker) = cleanup_tombstone_marker(initializing)? {
        ensure_previous_marker_owned(
            &marker,
            identity,
            "the publication backup initializer cleanup marker belongs to another journal",
        )?;
        remove_owned_tree(initializing, &marker)?;
    }
    if !initializing.exists() {
        return Ok(false);
    }
    if !initializing.join(PUBLICATION_MARKER_FILE).is_file() {
        remove_empty_initializer(
            initializing,
            "the publication backup initializer has no ownership marker",
        )?;
        return Ok(false);
    }

    let marker = read_marker(initializing)?;
    ensure_previous_marker_owned(
        &marker,
        identity,
        "the publication backup initializer belongs to another journal",
    )?;
    match marker.state {
        PublicationTreeState::Building => {
            remove_owned_tree(initializing, &marker)?;
            Ok(false)
        }
        PublicationTreeState::Complete => {
            verify_fingerprint(initializing, &marker)?;
            verify_fingerprint(&paths.managed, &marker)?;
            fs::rename(initializing, &paths.backup).map_err(|source| {
                io_error("publish completed publication backup initializer", source)
            })?;
            write_candidate_marker(&paths.managed, identity)?;
            Ok(true)
        }
    }
}

fn ensure_previous_marker_owned(
    marker: &PublicationTreeMarker,
    identity: PublicationFsIdentity,
    message: &'static str,
) -> Result<(), PublicationFsError> {
    if marker_matches(
        marker,
        identity,
        PublicationTreeRole::Previous,
        identity.previous_revision,
    ) {
        Ok(())
    } else {
        Err(PublicationFsError::UnsafeState(message))
    }
}

fn remove_empty_initializer(
    path: &Path,
    nonempty_message: &'static str,
) -> Result<(), PublicationFsError> {
    if !directory_is_empty(path)? {
        return Err(PublicationFsError::UnsafeState(nonempty_message));
    }
    fs::remove_dir(path)
        .map_err(|source| io_error("remove empty publication backup initializer", source))
}

fn write_candidate_marker(
    managed: &Path,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    write_marker(
        managed,
        &building_marker(
            identity,
            PublicationTreeRole::Candidate,
            identity.target_revision,
        ),
    )
}

pub(crate) fn bootstrap_legacy_markers(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    validate_sibling_paths(paths)?;
    validate_legacy_directory_paths(paths)?;
    let state = LegacyPublicationState {
        managed_exists: paths.managed.is_dir(),
        backup_exists: paths.backup.is_dir(),
        failed_exists: paths.failed.is_dir(),
        managed_marker: legacy_marker_if_present(&paths.managed)?,
        backup_marker: legacy_marker_if_present(&paths.backup)?,
        failed_marker: legacy_marker_if_present(&paths.failed)?,
    };
    if state.failed_exists
        && !state.failed_marker.as_ref().is_some_and(|marker| {
            marker_matches(
                marker,
                identity,
                PublicationTreeRole::Candidate,
                identity.target_revision,
            )
        })
    {
        return Err(PublicationFsError::UnsafeState(
            "the failed legacy publication folder has no matching ownership marker",
        ));
    }
    if !state.backup_exists {
        return bootstrap_restored_legacy_tree(paths, identity, &state);
    }
    if state.failed_exists {
        return bootstrap_legacy_after_candidate_rename(paths, identity, &state);
    }
    if !state.managed_exists {
        return Err(PublicationFsError::UnsafeState(
            "the staged legacy publication folder is missing",
        ));
    }
    bootstrap_staged_legacy_tree(paths, identity, &state)
}

struct LegacyPublicationState {
    managed_exists: bool,
    backup_exists: bool,
    failed_exists: bool,
    managed_marker: Option<PublicationTreeMarker>,
    backup_marker: Option<PublicationTreeMarker>,
    failed_marker: Option<PublicationTreeMarker>,
}

fn validate_legacy_directory_paths(paths: &PublicationFsPaths) -> Result<(), PublicationFsError> {
    if [&paths.managed, &paths.backup, &paths.failed]
        .into_iter()
        .any(|path| path.exists() && !path.is_dir())
    {
        return Err(PublicationFsError::UnsafeState(
            "a legacy publication recovery path is not a directory",
        ));
    }
    Ok(())
}

fn bootstrap_restored_legacy_tree(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    state: &LegacyPublicationState,
) -> Result<(), PublicationFsError> {
    if !state.managed_exists {
        return Err(PublicationFsError::UnsafeState(
            "the restored legacy publication folder is missing",
        ));
    }
    if let Some(marker) = &state.managed_marker {
        if !is_complete_marker(
            Some(marker),
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
        ) {
            return Err(PublicationFsError::UnsafeState(
                "the restored legacy publication marker belongs to another journal",
            ));
        }
        return verify_fingerprint(&paths.managed, marker);
    }
    if state.failed_exists {
        return Err(PublicationFsError::UnsafeState(
            "the restored legacy publication marker is missing",
        ));
    }

    let fingerprint = fingerprint_tree(&paths.managed)?;
    write_marker(
        &paths.managed,
        &complete_marker(
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
            &fingerprint,
        ),
    )
}

fn bootstrap_legacy_after_candidate_rename(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    state: &LegacyPublicationState,
) -> Result<(), PublicationFsError> {
    if state.managed_exists {
        return Err(PublicationFsError::UnsafeState(
            "legacy publication directories are ambiguous after candidate recovery",
        ));
    }
    let backup = state
        .backup_marker
        .as_ref()
        .ok_or(PublicationFsError::UnsafeState(
            "the legacy publication backup marker is missing after candidate recovery",
        ))?;
    if !is_complete_marker(
        Some(backup),
        identity,
        PublicationTreeRole::Previous,
        identity.previous_revision,
    ) {
        return Err(PublicationFsError::UnsafeState(
            "the legacy publication backup belongs to another journal",
        ));
    }
    verify_fingerprint(&paths.backup, backup)
}

fn bootstrap_staged_legacy_tree(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    state: &LegacyPublicationState,
) -> Result<(), PublicationFsError> {
    match (&state.managed_marker, &state.backup_marker) {
        (None, None) => {
            let fingerprint = fingerprint_tree(&paths.backup)?;
            write_marker(
                &paths.backup,
                &complete_marker(
                    identity,
                    PublicationTreeRole::Previous,
                    identity.previous_revision,
                    &fingerprint,
                ),
            )?;
            write_marker(
                &paths.managed,
                &building_marker(
                    identity,
                    PublicationTreeRole::Candidate,
                    identity.target_revision,
                ),
            )
        }
        (Some(managed), Some(backup))
            if marker_matches(
                managed,
                identity,
                PublicationTreeRole::Candidate,
                identity.target_revision,
            ) && is_complete_marker(
                Some(backup),
                identity,
                PublicationTreeRole::Previous,
                identity.previous_revision,
            ) =>
        {
            verify_fingerprint(&paths.backup, backup)
        }
        (None, Some(backup))
            if marker_matches(
                backup,
                identity,
                PublicationTreeRole::Previous,
                identity.previous_revision,
            ) && backup.state == PublicationTreeState::Complete =>
        {
            verify_fingerprint(&paths.backup, backup)?;
            write_candidate_marker(&paths.managed, identity)
        }
        _ => Err(PublicationFsError::UnsafeState(
            "legacy publication markers are incomplete or belong to another journal",
        )),
    }
}

pub(crate) fn restore_step(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<RestoreProgress, PublicationFsError> {
    validate_sibling_paths(paths)?;
    let managed = marker_if_present(&paths.managed)?;
    let backup = marker_if_present(&paths.backup)?;
    let failed = marker_if_present(&paths.failed)?;

    if is_marker(
        managed.as_ref(),
        identity,
        PublicationTreeRole::Candidate,
        identity.target_revision,
    ) && is_complete_marker(
        backup.as_ref(),
        identity,
        PublicationTreeRole::Previous,
        identity.previous_revision,
    ) && failed.is_none()
    {
        let backup = backup.as_ref().ok_or(PublicationFsError::UnsafeState(
            "the complete publication backup marker disappeared",
        ))?;
        verify_fingerprint(&paths.backup, backup)?;
        fs::rename(&paths.managed, &paths.failed)
            .map_err(|source| io_error("move staged publication aside", source))?;
        return Ok(RestoreProgress::Advanced);
    }

    if managed.is_none()
        && is_complete_marker(
            backup.as_ref(),
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
        )
        && is_marker(
            failed.as_ref(),
            identity,
            PublicationTreeRole::Candidate,
            identity.target_revision,
        )
    {
        let backup = backup.as_ref().ok_or(PublicationFsError::UnsafeState(
            "the complete publication backup marker disappeared",
        ))?;
        verify_fingerprint(&paths.backup, backup)?;
        fs::rename(&paths.backup, &paths.managed)
            .map_err(|source| io_error("restore previous publication", source))?;
        return Ok(RestoreProgress::Advanced);
    }

    if is_complete_marker(
        managed.as_ref(),
        identity,
        PublicationTreeRole::Previous,
        identity.previous_revision,
    ) && backup.is_none()
        && failed.as_ref().is_none_or(|marker| {
            marker_matches(
                marker,
                identity,
                PublicationTreeRole::Candidate,
                identity.target_revision,
            )
        })
    {
        let managed = managed.as_ref().ok_or(PublicationFsError::UnsafeState(
            "the restored publication marker disappeared",
        ))?;
        verify_fingerprint(&paths.managed, managed)?;
        return Ok(RestoreProgress::Complete);
    }

    Err(PublicationFsError::UnsafeState(
        "publication directories are ambiguous or belong to another journal",
    ))
}

pub(crate) fn prepare_managed_candidate_seal(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<PreparedCandidateSeal, PublicationFsError> {
    validate_sibling_paths(paths)?;
    let marker = read_marker(&paths.managed)?;
    if !marker_matches(
        &marker,
        identity,
        PublicationTreeRole::Candidate,
        identity.target_revision,
    ) {
        return Err(PublicationFsError::UnsafeState(
            "the managed publication candidate marker belongs to another journal",
        ));
    }
    let fingerprint = match marker.state {
        PublicationTreeState::Building => {
            let fingerprint = fingerprint_tree(&paths.managed)?;
            if read_marker(&paths.managed)? != marker {
                return Err(PublicationFsError::UnsafeState(
                    "the managed publication marker changed while fingerprinting",
                ));
            }
            Some(fingerprint)
        }
        PublicationTreeState::Complete => {
            verify_fingerprint(&paths.managed, &marker)?;
            None
        }
    };
    Ok(PreparedCandidateSeal {
        expected_marker: marker,
        fingerprint,
    })
}

pub(crate) fn persist_prepared_candidate_seal(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    prepared: &PreparedCandidateSeal,
) -> Result<(), PublicationFsError> {
    validate_sibling_paths(paths)?;
    let current = read_marker(&paths.managed)?;
    if !marker_matches(
        &current,
        identity,
        PublicationTreeRole::Candidate,
        identity.target_revision,
    ) || current != prepared.expected_marker
    {
        return Err(PublicationFsError::UnsafeState(
            "the managed publication marker changed before sealing",
        ));
    }
    match (current.state, prepared.fingerprint.as_ref()) {
        (PublicationTreeState::Building, Some(fingerprint)) => {
            let complete = complete_marker(
                identity,
                PublicationTreeRole::Candidate,
                identity.target_revision,
                fingerprint,
            );
            write_marker(&paths.managed, &complete)?;
            if read_marker(&paths.managed)? != complete {
                return Err(PublicationFsError::UnsafeState(
                    "the managed publication marker changed while sealing",
                ));
            }
            Ok(())
        }
        (PublicationTreeState::Complete, None) => Ok(()),
        _ => Err(PublicationFsError::UnsafeState(
            "the prepared publication seal no longer matches its marker state",
        )),
    }
}

pub(crate) fn verify_managed_complete(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    role: PublicationTreeRole,
) -> Result<(), PublicationFsError> {
    validate_sibling_paths(paths)?;
    let revision = match role {
        PublicationTreeRole::Previous => identity.previous_revision,
        PublicationTreeRole::Candidate => identity.target_revision,
    };
    let marker = read_marker(&paths.managed)?;
    if !marker_matches(&marker, identity, role, revision) {
        return Err(PublicationFsError::UnsafeState(
            "the managed publication marker belongs to another journal",
        ));
    }
    verify_fingerprint(&paths.managed, &marker)
}

pub(crate) fn atomic_write_temporary_directory(path: &Path) -> PathBuf {
    let ordinary_parent = path.parent().unwrap_or_else(|| Path::new("."));
    ordinary_parent
        .ancestors()
        .find(|ancestor| ancestor.join(PUBLICATION_MARKER_FILE).is_file())
        .and_then(Path::parent)
        .unwrap_or(ordinary_parent)
        .to_path_buf()
}

pub(crate) fn cleanup_after_verification(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    outcome: PublicationTreeRole,
) -> Result<(), PublicationFsError> {
    verify_managed_complete(paths, identity, outcome)?;

    cleanup_owned_tree_if_present(
        &paths.backup,
        identity,
        PublicationTreeRole::Previous,
        identity.previous_revision,
    )?;
    cleanup_owned_tree_if_present(
        &paths.failed,
        identity,
        PublicationTreeRole::Candidate,
        identity.target_revision,
    )?;
    Ok(())
}

pub(crate) fn abort_staging(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    validate_sibling_paths(paths)?;
    let backup = if let Some(marker) = cleanup_tombstone_marker(&paths.backup)? {
        Some(marker)
    } else if paths.backup.exists() {
        Some(read_marker(&paths.backup)?)
    } else {
        None
    };
    if let Some(backup) = backup {
        if !marker_matches(
            &backup,
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
        ) {
            return Err(PublicationFsError::UnsafeState(
                "the staging backup belongs to another journal",
            ));
        }
        if paths.managed.is_dir() {
            verify_fingerprint(&paths.managed, &backup)?;
            write_marker(&paths.managed, &backup)?;
        }
        remove_owned_tree(&paths.backup, &backup)?;
    }
    Ok(())
}

pub(crate) const fn is_retryable_windows_code(code: Option<i32>) -> bool {
    matches!(code, Some(5 | 32 | 33))
}

pub(crate) fn retry_delay_millis(operation_id: EntityId, attempt: u32) -> u64 {
    let shift = if attempt > 5 { 5 } else { attempt };
    let base = 100_u64.saturating_mul(1_u64 << shift).min(2_000);
    let window = (base / 5).max(1);
    let seed = operation_id.unsigned_abs().wrapping_add(u64::from(attempt));
    base.saturating_add(seed % window + 1)
}

fn validate_sibling_paths(paths: &PublicationFsPaths) -> Result<(), PublicationFsError> {
    let Some(parent) = paths.managed.parent() else {
        return Err(PublicationFsError::UnsafeState(
            "the managed publication folder has no parent",
        ));
    };
    let backup_name = paths.backup.file_name().and_then(|value| value.to_str());
    let failed_name = paths.failed.file_name().and_then(|value| value.to_str());
    if paths.backup.parent() != Some(parent)
        || paths.failed.parent() != Some(parent)
        || !backup_name.is_some_and(|value| value.starts_with(".ersatzrs-publication-"))
        || !failed_name.is_some_and(|value| value.starts_with(".ersatzrs-failed-publication-"))
    {
        return Err(PublicationFsError::UnsafeState(
            "publication recovery paths failed their ownership boundary",
        ));
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path, relative: &Path) -> Result<(), PublicationFsError> {
    let mut entries = fs::read_dir(source)
        .map_err(|source| io_error("read publication source", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| io_error("read publication source entry", source))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if relative.as_os_str().is_empty() && entry.file_name() == PUBLICATION_MARKER_FILE {
            continue;
        }
        let metadata = entry
            .file_type()
            .map_err(|source| io_error("inspect publication source", source))?;
        if metadata.is_symlink() {
            return Err(PublicationFsError::UnsafeState(
                "managed publication folders may not contain symbolic links",
            ));
        }
        let target = destination.join(entry.file_name());
        let child_relative = relative.join(entry.file_name());
        if metadata.is_dir() {
            fs::create_dir(&target)
                .map_err(|source| io_error("create publication backup directory", source))?;
            copy_tree(&entry.path(), &target, &child_relative)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), target)
                .map_err(|source| io_error("copy publication backup file", source))?;
        }
    }
    Ok(())
}

fn fingerprint_tree(root: &Path) -> Result<TreeFingerprint, PublicationFsError> {
    let mut hasher = Sha256::new();
    let mut file_count = 0_u64;
    let mut byte_count = 0_u64;
    fingerprint_directory(root, root, &mut hasher, &mut file_count, &mut byte_count)?;
    let mut sha256 = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut sha256, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(TreeFingerprint {
        sha256,
        file_count,
        byte_count,
    })
}

fn fingerprint_directory(
    root: &Path,
    directory: &Path,
    hasher: &mut Sha256,
    file_count: &mut u64,
    byte_count: &mut u64,
) -> Result<(), PublicationFsError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|source| io_error("read publication tree", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| io_error("read publication tree entry", source))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let entry_path = entry.path();
        if entry_path == root.join(PUBLICATION_MARKER_FILE) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|source| io_error("inspect publication tree", source))?;
        if file_type.is_symlink() {
            return Err(PublicationFsError::UnsafeState(
                "publication trees may not contain symbolic links",
            ));
        }
        let relative = entry_path.strip_prefix(root).map_err(|_| {
            PublicationFsError::UnsafeState("publication entry escaped its managed root")
        })?;
        let normalized = relative.to_string_lossy().replace('\\', "/");
        hasher.update(normalized.as_bytes());
        hasher.update([0]);
        if file_type.is_dir() {
            hasher.update(b"directory");
            fingerprint_directory(root, &entry_path, hasher, file_count, byte_count)?;
        } else if file_type.is_file() {
            hasher.update(b"file");
            let mut file = File::open(&entry_path)
                .map_err(|source| io_error("open publication tree file", source))?;
            let mut buffer = [0_u8; 16 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|source| io_error("read publication tree file", source))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                *byte_count = (*byte_count).saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
            }
            *file_count = (*file_count).saturating_add(1);
        }
        hasher.update([0xff]);
    }
    Ok(())
}

fn marker_if_present(path: &Path) -> Result<Option<PublicationTreeMarker>, PublicationFsError> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_dir() {
        return Err(PublicationFsError::UnsafeState(
            "a publication recovery path is not a directory",
        ));
    }
    let marker = path.join(PUBLICATION_MARKER_FILE);
    if !marker.is_file() {
        return Err(PublicationFsError::UnsafeState(
            "a publication directory has no ownership marker",
        ));
    }
    read_marker(path).map(Some)
}

fn legacy_marker_if_present(
    path: &Path,
) -> Result<Option<PublicationTreeMarker>, PublicationFsError> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_dir() {
        return Err(PublicationFsError::UnsafeState(
            "a publication recovery path is not a directory",
        ));
    }
    if !path.join(PUBLICATION_MARKER_FILE).is_file() {
        return Ok(None);
    }
    read_marker(path).map(Some)
}

fn read_marker(path: &Path) -> Result<PublicationTreeMarker, PublicationFsError> {
    read_marker_file(&path.join(PUBLICATION_MARKER_FILE))
}

fn read_marker_file(path: &Path) -> Result<PublicationTreeMarker, PublicationFsError> {
    let bytes = fs::read(path).map_err(|source| io_error("read publication marker", source))?;
    let marker: PublicationTreeMarker = serde_json::from_slice(&bytes)
        .map_err(|error| PublicationFsError::InvalidMarker(error.to_string()))?;
    if marker.schema_version != MARKER_SCHEMA_VERSION {
        return Err(PublicationFsError::InvalidMarker(format!(
            "unsupported schema version {}",
            marker.schema_version
        )));
    }
    Ok(marker)
}

fn write_marker(path: &Path, marker: &PublicationTreeMarker) -> Result<(), PublicationFsError> {
    let bytes = serde_json::to_vec(marker)
        .map_err(|error| PublicationFsError::InvalidMarker(error.to_string()))?;
    let final_path = path.join(PUBLICATION_MARKER_FILE);
    let mut temporary = tempfile::NamedTempFile::new_in(marker_temporary_directory(path)?)
        .map_err(|source| io_error("create publication marker", source))?;
    temporary
        .write_all(&bytes)
        .map_err(|source| io_error("write publication marker", source))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| io_error("flush publication marker", source))?;
    temporary
        .persist(&final_path)
        .map_err(|error| io_error("publish publication marker", error.error))?;
    Ok(())
}

fn marker_temporary_directory(path: &Path) -> Result<&Path, PublicationFsError> {
    path.parent().ok_or(PublicationFsError::UnsafeState(
        "the publication marker directory has no parent",
    ))
}

fn building_marker(
    identity: PublicationFsIdentity,
    role: PublicationTreeRole,
    revision: i32,
) -> PublicationTreeMarker {
    PublicationTreeMarker {
        schema_version: MARKER_SCHEMA_VERSION,
        list_id: identity.list_id,
        operation_id: identity.operation_id,
        generation: identity.generation,
        role,
        revision,
        state: PublicationTreeState::Building,
        sha256: None,
        file_count: 0,
        byte_count: 0,
    }
}

fn complete_marker(
    identity: PublicationFsIdentity,
    role: PublicationTreeRole,
    revision: i32,
    fingerprint: &TreeFingerprint,
) -> PublicationTreeMarker {
    PublicationTreeMarker {
        state: PublicationTreeState::Complete,
        sha256: Some(fingerprint.sha256.clone()),
        file_count: fingerprint.file_count,
        byte_count: fingerprint.byte_count,
        ..building_marker(identity, role, revision)
    }
}

fn marker_matches(
    marker: &PublicationTreeMarker,
    identity: PublicationFsIdentity,
    role: PublicationTreeRole,
    revision: i32,
) -> bool {
    marker.schema_version == MARKER_SCHEMA_VERSION
        && marker.list_id == identity.list_id
        && marker.operation_id == identity.operation_id
        && marker.generation == identity.generation
        && marker.role == role
        && marker.revision == revision
}

fn is_marker(
    marker: Option<&PublicationTreeMarker>,
    identity: PublicationFsIdentity,
    role: PublicationTreeRole,
    revision: i32,
) -> bool {
    marker.is_some_and(|marker| marker_matches(marker, identity, role, revision))
}

fn is_complete_marker(
    marker: Option<&PublicationTreeMarker>,
    identity: PublicationFsIdentity,
    role: PublicationTreeRole,
    revision: i32,
) -> bool {
    marker.is_some_and(|marker| {
        marker_matches(marker, identity, role, revision)
            && marker.state == PublicationTreeState::Complete
    })
}

fn verify_fingerprint(
    path: &Path,
    marker: &PublicationTreeMarker,
) -> Result<(), PublicationFsError> {
    if marker.state != PublicationTreeState::Complete {
        return Err(PublicationFsError::Integrity(
            "the publication tree marker is not complete",
        ));
    }
    let actual = fingerprint_tree(path)?;
    if marker.sha256.as_deref() != Some(actual.sha256.as_str())
        || marker.file_count != actual.file_count
        || marker.byte_count != actual.byte_count
    {
        return Err(PublicationFsError::Integrity(
            "the publication tree does not match its integrity marker",
        ));
    }
    Ok(())
}

fn remove_owned_tree(
    path: &Path,
    expected_marker: &PublicationTreeMarker,
) -> Result<(), PublicationFsError> {
    let tombstone = cleanup_tombstone_path(path)?;
    if tombstone.exists() {
        if !tombstone.is_file() {
            return Err(PublicationFsError::UnsafeState(
                "the publication cleanup tombstone is not a file",
            ));
        }
        let current = read_marker_file(&tombstone)?;
        if &current != expected_marker {
            return Err(PublicationFsError::UnsafeState(
                "the publication cleanup marker changed before cleanup",
            ));
        }
        if path.join(PUBLICATION_MARKER_FILE).exists() {
            return Err(PublicationFsError::UnsafeState(
                "the publication directory has both live and cleanup markers",
            ));
        }
    } else {
        let current = read_marker(path)?;
        if &current != expected_marker {
            return Err(PublicationFsError::UnsafeState(
                "the publication directory marker changed before cleanup",
            ));
        }
        fs::rename(path.join(PUBLICATION_MARKER_FILE), &tombstone)
            .map_err(|source| io_error("tombstone publication cleanup marker", source))?;
    }

    if !path.exists() {
        fs::remove_file(&tombstone)
            .map_err(|source| io_error("remove publication cleanup tombstone", source))?;
        return Ok(());
    }
    if !path.is_dir() {
        return Err(PublicationFsError::UnsafeState(
            "a publication cleanup path is not a directory",
        ));
    }
    let mut entries = fs::read_dir(path)
        .map_err(|source| io_error("read publication cleanup directory", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| io_error("read publication cleanup entry", source))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|source| io_error("inspect publication cleanup entry", source))?;
        if file_type.is_dir() && !file_type.is_symlink() {
            fs::remove_dir_all(entry.path())
                .map_err(|source| io_error("remove publication cleanup directory", source))?;
        } else {
            fs::remove_file(entry.path())
                .map_err(|source| io_error("remove publication cleanup file", source))?;
        }
    }
    fs::remove_dir(path).map_err(|source| io_error("remove publication cleanup root", source))?;
    fs::remove_file(&tombstone)
        .map_err(|source| io_error("remove publication cleanup tombstone", source))
}

fn cleanup_tombstone_path(path: &Path) -> Result<PathBuf, PublicationFsError> {
    let parent = path.parent().ok_or(PublicationFsError::UnsafeState(
        "the publication cleanup path has no parent",
    ))?;
    let mut name = path
        .file_name()
        .ok_or(PublicationFsError::UnsafeState(
            "the publication cleanup path has no name",
        ))?
        .to_os_string();
    name.push(CLEANUP_TOMBSTONE_SUFFIX);
    Ok(parent.join(name))
}

fn backup_initializing_path(path: &Path) -> Result<PathBuf, PublicationFsError> {
    let parent = path.parent().ok_or(PublicationFsError::UnsafeState(
        "the publication backup path has no parent",
    ))?;
    let mut name = path
        .file_name()
        .ok_or(PublicationFsError::UnsafeState(
            "the publication backup path has no name",
        ))?
        .to_os_string();
    name.push(".initializing");
    Ok(parent.join(name))
}

fn cleanup_tombstone_marker(
    path: &Path,
) -> Result<Option<PublicationTreeMarker>, PublicationFsError> {
    let tombstone = cleanup_tombstone_path(path)?;
    if !tombstone.exists() {
        return Ok(None);
    }
    if !tombstone.is_file() {
        return Err(PublicationFsError::UnsafeState(
            "the publication cleanup tombstone is not a file",
        ));
    }
    read_marker_file(&tombstone).map(Some)
}

fn cleanup_owned_tree_if_present(
    path: &Path,
    identity: PublicationFsIdentity,
    role: PublicationTreeRole,
    revision: i32,
) -> Result<(), PublicationFsError> {
    let marker = if let Some(marker) = cleanup_tombstone_marker(path)? {
        marker
    } else if !path.exists() {
        return Ok(());
    } else if path.join(PUBLICATION_MARKER_FILE).is_file() {
        read_marker(path)?
    } else if directory_is_empty(path)? {
        fs::remove_dir(path)
            .map_err(|source| io_error("remove empty post-marker publication root", source))?;
        return Ok(());
    } else {
        return Err(PublicationFsError::UnsafeState(
            "a non-empty publication directory has no ownership marker",
        ));
    };
    if !marker_matches(&marker, identity, role, revision) {
        return Err(PublicationFsError::UnsafeState(
            "the publication cleanup marker belongs to another journal",
        ));
    }
    remove_owned_tree(path, &marker)
}

fn directory_is_empty(path: &Path) -> Result<bool, PublicationFsError> {
    let mut entries =
        fs::read_dir(path).map_err(|source| io_error("read publication directory", source))?;
    match entries.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(source)) => Err(io_error("read publication directory entry", source)),
    }
}

fn io_error(action: &'static str, source: std::io::Error) -> PublicationFsError {
    PublicationFsError::Io { action, source }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, PublicationFsPaths, PublicationFsIdentity) {
        let temp = tempfile::tempdir().expect("temp directory");
        let managed = temp.path().join("managed");
        fs::create_dir(&managed).expect("managed directory");
        fs::write(managed.join("item.yml"), b"previous").expect("previous item");
        let identity = PublicationFsIdentity {
            list_id: 7,
            operation_id: Some(11),
            generation: Some(2),
            previous_revision: 3,
            target_revision: 4,
        };
        let paths = publication_paths(
            managed,
            identity.list_id,
            identity.operation_id,
            identity.generation,
        )
        .expect("publication paths");
        (temp, paths, identity)
    }

    fn prepare_legacy_restore(paths: &PublicationFsPaths, identity: PublicationFsIdentity) {
        fs::create_dir(&paths.backup).expect("legacy backup directory");
        fs::copy(
            paths.managed.join("item.yml"),
            paths.backup.join("item.yml"),
        )
        .expect("legacy backup item");
        bootstrap_legacy_markers(paths, identity).expect("bootstrap initial legacy markers");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("candidate item");
    }

    fn seal_candidate(paths: &PublicationFsPaths, identity: PublicationFsIdentity) {
        let prepared =
            prepare_managed_candidate_seal(paths, identity).expect("prepare candidate seal");
        persist_prepared_candidate_seal(paths, identity, &prepared)
            .expect("persist candidate seal");
    }

    #[test]
    fn backup_is_byte_isolated_from_candidate_mutation() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("mutate candidate");
        assert_eq!(
            fs::read(paths.backup.join("item.yml")).expect("backup item"),
            b"previous"
        );
        let marker = read_marker(&paths.backup).expect("backup marker");
        verify_fingerprint(&paths.backup, &marker).expect("backup remains intact");
    }

    #[test]
    fn verification_rejects_an_unsealed_building_candidate() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");

        let error = verify_managed_complete(&paths, identity, PublicationTreeRole::Candidate)
            .expect_err("verification must not seal a building candidate");

        assert!(error.to_string().contains("marker is not complete"));
        assert_eq!(
            read_marker(&paths.managed)
                .expect("building marker remains")
                .state,
            PublicationTreeState::Building
        );
    }

    #[test]
    fn prepared_candidate_seal_is_read_only_and_rejects_a_reclaimed_marker() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");

        let prepared =
            prepare_managed_candidate_seal(&paths, identity).expect("prepare candidate seal");
        assert_eq!(
            read_marker(&paths.managed)
                .expect("unmodified building marker")
                .state,
            PublicationTreeState::Building
        );

        let reclaimed_identity = PublicationFsIdentity {
            generation: Some(3),
            ..identity
        };
        let reclaimed_marker = building_marker(
            reclaimed_identity,
            PublicationTreeRole::Candidate,
            reclaimed_identity.target_revision,
        );
        write_marker(&paths.managed, &reclaimed_marker).expect("reclaimed marker");
        let error = persist_prepared_candidate_seal(&paths, identity, &prepared)
            .expect_err("stale preparation must not seal a reclaimed marker");

        assert!(error.to_string().contains("changed before sealing"));
        assert_eq!(
            read_marker(&paths.managed).expect("reclaimed marker remains"),
            reclaimed_marker
        );
    }

    #[test]
    fn sealed_candidate_tampering_cannot_be_reblessed() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("candidate item");
        seal_candidate(&paths, identity);
        let sealed = read_marker(&paths.managed).expect("sealed marker");
        let idempotent = prepare_managed_candidate_seal(&paths, identity)
            .expect("verify already sealed candidate");
        persist_prepared_candidate_seal(&paths, identity, &idempotent)
            .expect("complete marker needs no replacement");
        assert_eq!(read_marker(&paths.managed).expect("sealed marker"), sealed);

        fs::write(paths.managed.join("item.yml"), b"tampered").expect("tamper candidate");
        let verification_error =
            verify_managed_complete(&paths, identity, PublicationTreeRole::Candidate)
                .expect_err("verification must reject tampering after sealing");
        let reseal_error = prepare_managed_candidate_seal(&paths, identity)
            .expect_err("idempotent sealing must not replace the persisted fingerprint");

        assert!(verification_error.to_string().contains("integrity marker"));
        assert!(reseal_error.to_string().contains("integrity marker"));
        assert_eq!(read_marker(&paths.managed).expect("sealed marker"), sealed);
    }

    #[test]
    fn every_partial_restore_state_resumes_idempotently() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("mutate candidate");

        assert_eq!(
            restore_step(&paths, identity).expect("first restore step"),
            RestoreProgress::Advanced
        );
        assert!(!paths.managed.exists());
        assert!(paths.backup.exists());
        assert!(paths.failed.exists());

        assert_eq!(
            restore_step(&paths, identity).expect("second restore step"),
            RestoreProgress::Advanced
        );
        assert_eq!(
            restore_step(&paths, identity).expect("completed restore"),
            RestoreProgress::Complete
        );
        assert_eq!(
            fs::read(paths.managed.join("item.yml")).expect("restored item"),
            b"previous"
        );
    }

    #[test]
    fn restored_publication_tampering_fails_previous_tree_verification() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("candidate item");
        assert_eq!(
            restore_step(&paths, identity).expect("move candidate aside"),
            RestoreProgress::Advanced
        );
        assert_eq!(
            restore_step(&paths, identity).expect("restore previous publication"),
            RestoreProgress::Advanced
        );
        fs::write(paths.managed.join("item.yml"), b"tampered").expect("tamper restored item");

        let error = verify_managed_complete(&paths, identity, PublicationTreeRole::Previous)
            .expect_err("restored bytes must retain their sealed backup fingerprint");

        assert!(error.to_string().contains("integrity marker"));
    }

    #[test]
    fn foreign_or_corrupt_directories_stop_without_deletion() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::write(paths.backup.join("item.yml"), b"tampered").expect("tamper backup");
        let error = restore_step(&paths, identity).expect_err("corrupt backup must stop");
        assert!(error.to_string().contains("integrity marker"));
        assert!(paths.managed.exists());
        assert!(paths.backup.exists());
    }

    #[test]
    fn cleanup_keeps_marker_until_verified_tree_contents_are_removed() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        seal_candidate(&paths, identity);
        cleanup_after_verification(&paths, identity, PublicationTreeRole::Candidate)
            .expect("cleanup publication");
        assert!(paths.managed.join(PUBLICATION_MARKER_FILE).is_file());
        assert!(!paths.backup.exists());
        assert!(!paths.failed.exists());
    }

    #[test]
    fn cleanup_resumes_after_the_scratch_marker_was_removed_or_tombstoned() {
        for crash_state in 0..3 {
            let (_temp, paths, identity) = fixture();
            create_backup(&paths, identity).expect("create backup");
            seal_candidate(&paths, identity);

            let tombstone = cleanup_tombstone_path(&paths.backup).expect("cleanup tombstone");
            if crash_state == 0 {
                fs::remove_file(paths.backup.join(PUBLICATION_MARKER_FILE))
                    .expect("simulate the legacy marker-last cleanup gap");
            } else {
                fs::rename(paths.backup.join(PUBLICATION_MARKER_FILE), &tombstone)
                    .expect("persist cleanup ownership outside scratch tree");
            }
            fs::remove_file(paths.backup.join("item.yml")).expect("empty scratch tree");
            if crash_state == 2 {
                fs::remove_dir(&paths.backup).expect("remove scratch root");
            }

            cleanup_after_verification(&paths, identity, PublicationTreeRole::Candidate)
                .expect("resume cleanup from tombstone");
            assert!(!paths.backup.exists());
            assert!(!tombstone.exists());
        }
    }

    #[test]
    fn cleanup_refuses_a_nonempty_markerless_scratch_tree() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        seal_candidate(&paths, identity);
        fs::remove_file(paths.backup.join(PUBLICATION_MARKER_FILE))
            .expect("remove scratch ownership marker");

        let error = cleanup_after_verification(&paths, identity, PublicationTreeRole::Candidate)
            .expect_err("non-empty markerless scratch must stop");

        assert!(error.to_string().contains("non-empty"));
        assert_eq!(
            fs::read(paths.backup.join("item.yml")).expect("untouched backup item"),
            b"previous"
        );
    }

    #[test]
    fn legacy_bootstrap_resumes_after_only_the_backup_marker_was_persisted() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::remove_file(paths.managed.join(PUBLICATION_MARKER_FILE))
            .expect("simulate exit before candidate marker");

        bootstrap_legacy_markers(&paths, identity).expect("resume legacy marker bootstrap");

        let managed = read_marker(&paths.managed).expect("managed marker");
        assert!(marker_matches(
            &managed,
            identity,
            PublicationTreeRole::Candidate,
            identity.target_revision,
        ));
        let backup = read_marker(&paths.backup).expect("backup marker");
        verify_fingerprint(&paths.backup, &backup).expect("backup remains complete");
    }

    #[test]
    fn legacy_bootstrap_accepts_restart_after_the_candidate_rename() {
        let (_temp, paths, identity) = fixture();
        prepare_legacy_restore(&paths, identity);
        assert_eq!(
            restore_step(&paths, identity).expect("move candidate aside"),
            RestoreProgress::Advanced
        );

        bootstrap_legacy_markers(&paths, identity)
            .expect("resume legacy markers after candidate rename");

        assert!(!paths.managed.exists());
        assert!(paths.backup.exists());
        assert!(paths.failed.exists());
    }

    #[test]
    fn legacy_bootstrap_accepts_restart_after_the_backup_rename() {
        let (_temp, paths, identity) = fixture();
        prepare_legacy_restore(&paths, identity);
        assert_eq!(
            restore_step(&paths, identity).expect("move candidate aside"),
            RestoreProgress::Advanced
        );
        assert_eq!(
            restore_step(&paths, identity).expect("restore backup"),
            RestoreProgress::Advanced
        );

        bootstrap_legacy_markers(&paths, identity)
            .expect("resume legacy markers after backup rename");

        assert_eq!(
            restore_step(&paths, identity).expect("finish restored state"),
            RestoreProgress::Complete
        );
        assert_eq!(
            fs::read(paths.managed.join("item.yml")).expect("restored item"),
            b"previous"
        );
    }

    #[test]
    fn backup_publication_recovers_an_empty_unowned_initializer_atomically() {
        for leave_old_final_path in [false, true] {
            let (_temp, paths, identity) = fixture();
            let initializing = backup_initializing_path(&paths.backup).expect("initializer path");
            let interrupted = if leave_old_final_path {
                &paths.backup
            } else {
                &initializing
            };
            fs::create_dir(interrupted).expect("simulate interrupted empty initializer");

            create_backup(&paths, identity).expect("atomically publish owned backup");

            let backup = read_marker(&paths.backup).expect("backup marker");
            assert_eq!(backup.state, PublicationTreeState::Complete);
            verify_fingerprint(&paths.backup, &backup).expect("complete backup fingerprint");
            assert!(
                !initializing.exists(),
                "successful publication must not leave its atomic initializer behind"
            );
        }
    }

    #[test]
    fn completed_backup_initializer_is_promoted_after_restart() {
        let (_temp, paths, identity) = fixture();
        let initializing = backup_initializing_path(&paths.backup).expect("initializer path");
        fs::create_dir(&initializing).expect("initializer directory");
        write_marker(
            &initializing,
            &building_marker(
                identity,
                PublicationTreeRole::Previous,
                identity.previous_revision,
            ),
        )
        .expect("owned initializer");
        copy_tree(&paths.managed, &initializing, Path::new("")).expect("copy previous publication");
        let fingerprint = fingerprint_tree(&initializing).expect("initializer fingerprint");
        write_marker(
            &initializing,
            &complete_marker(
                identity,
                PublicationTreeRole::Previous,
                identity.previous_revision,
                &fingerprint,
            ),
        )
        .expect("complete initializer before simulated exit");

        create_backup(&paths, identity).expect("promote completed initializer");

        assert!(!initializing.exists());
        let backup = read_marker(&paths.backup).expect("promoted backup marker");
        verify_fingerprint(&paths.backup, &backup).expect("promoted backup fingerprint");
    }

    #[test]
    fn interrupted_marker_temporaries_live_outside_fingerprinted_trees() {
        let (_temp, paths, identity) = fixture();
        assert_eq!(
            marker_temporary_directory(&paths.managed).expect("marker temporary directory"),
            paths.managed.parent().expect("managed parent")
        );

        let before = fingerprint_tree(&paths.managed).expect("managed fingerprint");
        let interrupted_temporary = paths
            .managed
            .parent()
            .expect("managed parent")
            .join(".interrupted-publication-marker");
        fs::write(&interrupted_temporary, b"partial marker").expect("interrupted marker write");
        let after = fingerprint_tree(&paths.managed).expect("managed fingerprint after crash");
        assert_eq!(after, before);

        write_marker(
            &paths.managed,
            &building_marker(
                identity,
                PublicationTreeRole::Candidate,
                identity.target_revision,
            ),
        )
        .expect("write marker through sibling temporary");
        assert!(paths.managed.join(PUBLICATION_MARKER_FILE).is_file());
    }

    #[test]
    fn publication_content_temporaries_cannot_enter_the_tree_fingerprint() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        let nested = paths.managed.join("season");
        fs::create_dir(&nested).expect("nested publication directory");
        let target = nested.join("episode.nfo");
        let temporary_directory = atomic_write_temporary_directory(&target);
        assert_eq!(
            temporary_directory,
            paths.managed.parent().expect("managed parent")
        );

        let before = fingerprint_tree(&paths.managed).expect("publication fingerprint");
        fs::write(
            temporary_directory.join(".interrupted-publication-content"),
            b"partial NFO or artwork",
        )
        .expect("interrupted content staging file");
        let after = fingerprint_tree(&paths.managed).expect("fingerprint after interrupted write");
        assert_eq!(after, before);

        let ordinary = paths
            .managed
            .parent()
            .expect("managed parent")
            .join("ordinary");
        fs::create_dir(&ordinary).expect("ordinary directory");
        assert_eq!(
            atomic_write_temporary_directory(&ordinary.join("file.nfo")),
            ordinary
        );
    }

    #[test]
    fn retry_policy_is_bounded_and_only_accepts_windows_sharing_codes() {
        assert!(is_retryable_windows_code(Some(5)));
        assert!(is_retryable_windows_code(Some(32)));
        assert!(is_retryable_windows_code(Some(33)));
        assert!(!is_retryable_windows_code(Some(13)));
        assert!(!is_retryable_windows_code(None));

        let delays = (0..6)
            .map(|attempt| retry_delay_millis(11, attempt))
            .collect::<Vec<_>>();
        assert!(delays.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(retry_delay_millis(11, 6) <= 2_400);
    }

    #[cfg(unix)]
    #[test]
    fn posix_open_handle_does_not_block_restore() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        let held = File::open(paths.managed.join("item.yml")).expect("held item");
        assert_eq!(
            restore_step(&paths, identity).expect("POSIX rename with held handle"),
            RestoreProgress::Advanced
        );
        drop(held);
    }

    #[cfg(windows)]
    #[test]
    fn windows_held_handle_reports_a_retryable_restore_error() {
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt as _;

        const FILE_SHARE_READ: u32 = 0x0000_0001;
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        let held = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(paths.managed.join("item.yml"))
            .expect("held item without delete sharing");
        let error = restore_step(&paths, identity).expect_err("held handle blocks rename");
        assert!(is_retryable_windows_code(error.raw_os_error()));
        drop(held);
        assert_eq!(
            restore_step(&paths, identity).expect("retry after handle release"),
            RestoreProgress::Advanced
        );
    }
}
