use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use ersatzrs_domain::{AddonMediaListPublicationErrorCategory, EntityId};

pub(crate) const PUBLICATION_MARKER_FILE: &str = ".ersatzrs-publication.json";
/// Reserved prefix for internal state that lives beside published content.
///
/// The scanner keeps its own sidecars inside a managed folder (the remote-stream
/// playlist sidecar and the NFO write-back state, for example) and rewrites them
/// whenever it runs, including during the publication verification scan. Those
/// files are not part of a published revision, so the tree fingerprint must skip
/// them; otherwise a routine scan invalidates the sealed marker of a revision
/// that never changed.
const RESERVED_ENTRY_PREFIX: &str = ".ersatzrs-";
const MARKER_SCHEMA_VERSION: u32 = 1;
const CLEANUP_TOMBSTONE_SUFFIX: &str = ".cleanup-marker";
pub(crate) const REVISION_BLOB_STORE_DIRECTORY: &str = ".ersatzrs-revision-blobs";

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
pub(crate) struct ArchivedRevisionFile {
    pub(crate) published_relative_path: String,
    pub(crate) storage_relative_path: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
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

    pub(crate) fn redacted_cause(&self) -> (AddonMediaListPublicationErrorCategory, Option<i32>) {
        match self {
            Self::Io { source, .. } => {
                let os_error_code = source.raw_os_error();
                let category = if is_sharing_violation(os_error_code) {
                    AddonMediaListPublicationErrorCategory::SharingViolation
                } else if source.kind() == std::io::ErrorKind::PermissionDenied {
                    AddonMediaListPublicationErrorCategory::PermissionDenied
                } else {
                    AddonMediaListPublicationErrorCategory::FilesystemIo
                };
                (category, os_error_code)
            }
            Self::UnsafeState(_) => (
                AddonMediaListPublicationErrorCategory::UnsafeFilesystemState,
                None,
            ),
            Self::InvalidMarker(_) => (
                AddonMediaListPublicationErrorCategory::InvalidPublicationMarker,
                None,
            ),
            Self::Integrity(_) => (
                AddonMediaListPublicationErrorCategory::PublicationIntegrity,
                None,
            ),
        }
    }
}

#[cfg(windows)]
fn is_sharing_violation(os_error_code: Option<i32>) -> bool {
    matches!(os_error_code, Some(5 | 32 | 33))
}

#[cfg(not(windows))]
const fn is_sharing_violation(_os_error_code: Option<i32>) -> bool {
    false
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
    with_managed_tree_lock(paths, || create_backup_unlocked(paths, identity))
}

fn create_backup_unlocked(
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

/// Prepare a reset-owned candidate without deleting managed content. Callers
/// must re-check their database lease before entering the destructive step.
pub(crate) fn prepare_empty_managed_candidate(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    with_managed_tree_lock(paths, || {
        validate_sibling_paths(paths)?;
        let marker_path = paths.managed.join(PUBLICATION_MARKER_FILE);
        if marker_path.is_file() {
            let marker = read_marker(&paths.managed)?;
            if marker_matches(
                &marker,
                identity,
                PublicationTreeRole::Candidate,
                identity.target_revision,
            ) {
                if marker.state == PublicationTreeState::Complete {
                    return verify_empty_reset_candidate_unlocked(paths, identity);
                }
                return Ok(());
            }
            ensure_reset_may_replace_marker(paths, identity, &marker)?;
        }
        create_backup_unlocked(paths, identity)?;
        Ok(())
    })
}

/// Empty only a candidate prepared for this exact operation and generation.
/// The stable tree lock serializes uncancellable deletion with a reclaimed
/// reset or a new publication backup.
pub(crate) fn empty_prepared_managed_candidate(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    with_managed_tree_lock(paths, || {
        validate_sibling_paths(paths)?;
        let marker = read_marker(&paths.managed)?;
        ensure_reset_marker_current(&marker, identity)?;
        if marker.state == PublicationTreeState::Complete {
            verify_empty_reset_candidate_unlocked(paths, identity)?;
            return Ok(());
        }

        empty_prepared_managed_candidate_unlocked(paths, identity)
    })
}

fn empty_prepared_managed_candidate_unlocked(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    let marker = read_marker(&paths.managed)?;
    ensure_reset_marker_current(&marker, identity)?;
    if marker.state != PublicationTreeState::Building {
        return Err(PublicationFsError::UnsafeState(
            "the managed reset candidate is not ready to be emptied",
        ));
    }
    let mut entries = fs::read_dir(&paths.managed)
        .map_err(|source| io_error("read managed reset candidate", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| io_error("read managed reset entry", source))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if entry.file_name() == PUBLICATION_MARKER_FILE {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|source| io_error("inspect managed reset entry", source))?;
        if file_type.is_symlink() {
            return Err(PublicationFsError::UnsafeState(
                "managed reset candidates may not contain symbolic links",
            ));
        }
        if entry.file_name() == crate::addon_media_list_saved::model::SAVED_LIST_FILE_NAME
            || entry.file_name() == crate::addon_media_list_saved::model::SAVED_ASSETS_DIRECTORY
        {
            continue;
        }
        if file_type.is_dir() {
            fs::remove_dir_all(entry.path())
                .map_err(|source| io_error("remove managed reset directory", source))?;
        } else {
            fs::remove_file(entry.path())
                .map_err(|source| io_error("remove managed reset file", source))?;
        }
    }
    let prepared = prepare_managed_candidate_seal(paths, identity)?;
    persist_prepared_candidate_seal_unlocked(paths, identity, &prepared)?;
    verify_empty_reset_candidate_unlocked(paths, identity)
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
    with_managed_tree_lock(paths, || restore_step_unlocked(paths, identity))
}

// The failed candidate remains intact until the restored tree is complete. A
// retry can reinstall the same immutable assets and exact document bytes.
// False means rejected content; actual storage failures remain errors.
#[allow(clippy::too_many_lines)] // Keep rejection and storage-failure handling beside each bounded operation.
fn carry_saved_content(source: &Path, destination: &Path) -> Result<bool, PublicationFsError> {
    use crate::addon_media_list_saved::{assets, document_io, files, model};

    let invalid = || {
        PublicationFsError::UnsafeState(
            "saved content cannot be safely carried into the restored publication",
        )
    };
    let source_document = source.join(model::SAVED_LIST_FILE_NAME);
    match fs::symlink_metadata(&source_document) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(io_error("inspect saved restore document", error)),
        Ok(metadata) if metadata.is_symlink() || !metadata.is_file() => return Ok(false),
        Ok(_) => {}
    }
    let target = destination.join(model::SAVED_LIST_FILE_NAME);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.is_symlink() || !metadata.is_file() => return Err(invalid()),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(io_error("inspect restored saved document", error));
        }
        _ => {}
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(".ersatzrs-saved-restore-")
        .tempfile_in(destination)
        .map_err(|error| io_error("create restored saved document", error))?;
    let input = File::open(source_document)
        .map_err(|error| io_error("open saved restore document", error))?;
    let count = std::io::copy(
        &mut input.take(document_io::MAX_SAVED_DOCUMENT_BYTES + 1),
        &mut temporary,
    )
    .map_err(|error| io_error("copy saved restore document", error))?;
    if count > document_io::MAX_SAVED_DOCUMENT_BYTES {
        return Ok(false);
    }
    let document = match document_io::read_document(&mut std::io::BufReader::new(
        temporary
            .reopen()
            .map_err(|error| io_error("read restored saved document", error))?,
    )) {
        Ok(document) => document,
        Err(document_io::SavedDocumentError::Io) => {
            return Err(io_error(
                "read restored saved document",
                std::io::Error::other("saved document read failed"),
            ))
        }
        Err(
            document_io::SavedDocumentError::Invalid
            | document_io::SavedDocumentError::TooLarge
            | document_io::SavedDocumentError::UnsupportedVersion,
        ) => return Ok(false),
    };
    for reference in document.draft.files.iter().chain(
        document
            .published
            .iter()
            .flat_map(|publication| publication.files.iter()),
    ) {
        if files::validate_reference(reference).is_err() {
            return Ok(false);
        }
        if reference.relative_path.split('/').next() != Some(model::SAVED_ASSETS_DIRECTORY) {
            // Existing replacement/prepared media remain references, never copies.
            continue;
        }
        let mut path = source.to_owned();
        for component in reference.relative_path.split('/') {
            path.push(component);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(io_error("inspect saved restore asset", error)),
            };
            if metadata.is_symlink() {
                return Ok(false);
            }
        }
        if !fs::symlink_metadata(&path)
            .map_err(|error| io_error("inspect saved restore asset", error))?
            .is_file()
        {
            return Ok(false);
        }
        let mut input =
            File::open(path).map_err(|error| io_error("open saved restore asset", error))?;
        match assets::install(destination, reference, &mut input) {
            Ok(()) => {}
            Err(assets::SavedAssetError::Reference(_)) => return Ok(false),
            Err(assets::SavedAssetError::Filesystem {
                category,
                os_error_code,
            }) => {
                if category == AddonMediaListPublicationErrorCategory::PublicationIntegrity {
                    return Ok(false);
                }
                let error = os_error_code.map_or_else(
                    || std::io::Error::other("saved asset preservation failed"),
                    std::io::Error::from_raw_os_error,
                );
                return Err(io_error("preserve restored saved asset", error));
            }
        }
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| io_error("sync restored saved document", error))?;
    let temporary = temporary.into_temp_path();
    atomicwrites::replace_atomic(&temporary, &target)
        .map_err(|error| io_error("replace restored saved document", error))?;
    Ok(true)
}

fn saved_recovery_directory(destination: &Path, identity: PublicationFsIdentity) -> PathBuf {
    destination
        .join(crate::addon_media_list_saved::model::SAVED_ASSETS_DIRECTORY)
        .join(format!(
            "recovery-{}-{}-{}-{}",
            identity.list_id,
            identity.operation_id.unwrap_or(0),
            identity.generation.unwrap_or(0),
            identity.target_revision
        ))
}

/// Moves only reserved evidence, leaving the previous valid document authoritative.
/// The tree lock serializes destination checks and renames. Both-present conflicts
/// are refused rather than guessing which bytes a previous attempt owned.
fn preserve_rejected_saved_content(
    source: &Path,
    destination: &Path,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    use crate::addon_media_list_saved::model::{SAVED_ASSETS_DIRECTORY, SAVED_LIST_FILE_NAME};
    let recovery = saved_recovery_directory(destination, identity);
    ensure_managed_subdirectories(destination, &recovery)?;
    for name in [SAVED_ASSETS_DIRECTORY, SAVED_LIST_FILE_NAME] {
        let from = source.join(name);
        let to = recovery.join(name);
        let source_present = match fs::symlink_metadata(&from) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error("inspect rejected saved content", error)),
        };
        let target_present = match fs::symlink_metadata(&to) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error("inspect saved recovery evidence", error)),
        };
        if source_present && target_present {
            return Err(PublicationFsError::UnsafeState(
                "saved recovery evidence conflicts with the rejected source",
            ));
        }
        if source_present {
            fs::rename(&from, &to)
                .map_err(|error| io_error("preserve rejected saved content", error))?;
        }
    }
    tracing::warn!(list_id = identity.list_id, operation_id = identity.operation_id,
        "rejected saved list content was preserved separately while restoring the previous publication");
    Ok(())
}

fn restore_step_unlocked(
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
        if !carry_saved_content(&paths.failed, &paths.backup)? {
            preserve_rejected_saved_content(&paths.failed, &paths.backup, identity)?;
        }
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
    with_managed_tree_lock(paths, || {
        persist_prepared_candidate_seal_unlocked(paths, identity, prepared)
    })
}

fn persist_prepared_candidate_seal_unlocked(
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

/// Reseal only a verified legacy revision, under the same lock used by tree writers.
pub(crate) fn upgrade_managed_fingerprint(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    role: PublicationTreeRole,
) -> Result<(), PublicationFsError> {
    with_managed_tree_lock(paths, || {
        verify_managed_complete(paths, identity, role)?;
        let mut marker = read_marker(&paths.managed)?;
        let content = fingerprint_tree(&paths.managed)?;
        if marker.sha256.as_deref() != Some(content.sha256.as_str())
            || marker.file_count != content.file_count
            || marker.byte_count != content.byte_count
        {
            marker.sha256 = Some(content.sha256);
            marker.file_count = content.file_count;
            marker.byte_count = content.byte_count;
            write_marker(&paths.managed, &marker)?;
        }
        Ok(())
    })
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

/// Copy every published file into immutable, list-scoped content storage.
///
/// The blob store is host-owned beside the `SQLite` database, so managed-root
/// relocation and publication scratch cleanup cannot remove it. The tree lock
/// and final marker verification make the returned file set one sealed revision.
pub(crate) fn archive_managed_revision(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    storage_parent: &Path,
    storage_owner: &str,
) -> Result<Vec<ArchivedRevisionFile>, PublicationFsError> {
    with_managed_tree_lock(paths, || {
        verify_managed_complete(paths, identity, PublicationTreeRole::Candidate)?;
        validate_storage_owner(storage_owner)?;
        let mut files = Vec::new();
        archive_revision_directory(
            &paths.managed,
            &paths.managed,
            storage_parent,
            storage_owner,
            identity.list_id,
            &mut files,
        )?;
        // Detect an out-of-band writer even if it did not honor the tree lock.
        verify_managed_complete(paths, identity, PublicationTreeRole::Candidate)?;
        Ok(files)
    })
}

pub(crate) fn revision_blob_path(
    storage_parent: &Path,
    storage_owner: &str,
    storage_relative_path: &str,
) -> Result<PathBuf, PublicationFsError> {
    validate_storage_owner(storage_owner)?;
    let relative = Path::new(storage_relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || relative.components().next().is_none_or(|component| {
            component.as_os_str() != std::ffi::OsStr::new(REVISION_BLOB_STORE_DIRECTORY)
        })
    {
        return Err(PublicationFsError::UnsafeState(
            "the retained revision blob path escaped host-owned storage",
        ));
    }
    let mut components = relative.components();
    let _store_directory = components.next();
    if components
        .next()
        .is_none_or(|component| component.as_os_str() != std::ffi::OsStr::new(storage_owner))
    {
        return Err(PublicationFsError::UnsafeState(
            "the retained revision blob belongs to another storage owner",
        ));
    }
    let mut resolved = storage_parent.to_path_buf();
    for component in relative.components() {
        resolved.push(component.as_os_str());
        match fs::symlink_metadata(&resolved) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(PublicationFsError::UnsafeState(
                    "retained revision storage may not contain symbolic links",
                ));
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(io_error("inspect retained revision storage", source)),
        }
    }
    Ok(resolved)
}

/// Replace a building publication candidate with one retained file snapshot.
///
/// All blob identities are checked before the managed tree is touched. A
/// partial copy remains recoverable through the existing publication backup,
/// and repeating the call recreates the same candidate from scratch.
pub(crate) fn materialize_archived_revision(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    storage_parent: &Path,
    storage_owner: &str,
    files: &[ArchivedRevisionFile],
) -> Result<(), PublicationFsError> {
    with_managed_tree_lock(paths, || {
        let marker = read_marker(&paths.managed)?;
        if marker.state != PublicationTreeState::Building
            || !marker_matches(
                &marker,
                identity,
                PublicationTreeRole::Candidate,
                identity.target_revision,
            )
        {
            return Err(PublicationFsError::UnsafeState(
                "a retained revision requires the current building publication candidate",
            ));
        }
        let sources = validate_revision_materialization(
            storage_parent,
            storage_owner,
            identity.list_id,
            files,
        )?;
        clear_managed_revision_contents(&paths.managed, &paths.managed)?;
        for (file, source) in files.iter().zip(sources) {
            copy_revision_blob_into_candidate(&source, &paths.managed, file)?;
        }
        Ok(())
    })
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
    with_managed_tree_lock(paths, || {
        cleanup_after_verification_unlocked(paths, identity, outcome)
    })
}

fn cleanup_after_verification_unlocked(
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

pub(crate) fn cleanup_reset_scratch(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    with_managed_tree_lock(paths, || {
        cleanup_owned_tree_if_present(
            &paths.backup,
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
        )?;
        cleanup_owned_tree_if_present(
            &backup_initializing_path(&paths.backup)?,
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
        )?;
        cleanup_owned_tree_if_present(
            &paths.failed,
            identity,
            PublicationTreeRole::Candidate,
            identity.target_revision,
        )
    })
}

pub(crate) fn abort_staging(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    with_managed_tree_lock(paths, || abort_staging_unlocked(paths, identity))
}

fn abort_staging_unlocked(
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

fn with_managed_tree_lock<T>(
    paths: &PublicationFsPaths,
    action: impl FnOnce() -> Result<T, PublicationFsError>,
) -> Result<T, PublicationFsError> {
    validate_sibling_paths(paths)?;
    let _guard = ManagedTreeGuard::acquire(&paths.managed)?;
    action()
}

/// Own the existing cross-process tree lock through the last filesystem writer.
///
/// An `Arc` clone can travel with blocking asset work. Cancelling its async waiter
/// must not release the lock while that work is still running. This guard only
/// fences cooperating tree writers; the caller still validates root/operation
/// ownership and must acquire it before opening a database write transaction.
pub struct ManagedTreeGuard {
    _lock: File,
    managed: PathBuf,
}

impl ManagedTreeGuard {
    /// Acquire in blocking context, before database write ownership. Never acquire
    /// the same tree again while retaining this guard: file locks are not reentrant.
    ///
    /// # Errors
    /// Returns a filesystem error if the parent or cooperating lock is unavailable.
    pub(crate) fn acquire(managed: &Path) -> Result<Self, PublicationFsError> {
        let lock_path = managed_tree_lock_path(managed)?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|source| io_error("open managed publication lock", source))?;
        lock.lock()
            .map_err(|source| io_error("lock managed publication tree", source))?;
        Ok(Self {
            _lock: lock,
            managed: managed.to_owned(),
        })
    }

    /// Return the caller-validated root associated with this lock.
    #[must_use]
    pub fn managed_root(&self) -> &Path {
        &self.managed
    }
}

fn managed_tree_lock_path(managed: &Path) -> Result<PathBuf, PublicationFsError> {
    let parent = managed.parent().ok_or(PublicationFsError::UnsafeState(
        "the managed publication folder has no parent",
    ))?;
    Ok(parent.join(format!(
        ".ersatzrs-managed-tree-{}.lock",
        managed_path_key(managed)
    )))
}

fn managed_path_key(managed: &Path) -> String {
    let hash = Sha256::digest(managed.as_os_str().to_string_lossy().as_bytes());
    let mut key = String::with_capacity(24);
    for byte in &hash[..12] {
        write!(&mut key, "{byte:02x}").expect("writing to a String cannot fail");
    }
    key
}

fn ensure_reset_may_replace_marker(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
    marker: &PublicationTreeMarker,
) -> Result<(), PublicationFsError> {
    if marker.state != PublicationTreeState::Complete {
        return Err(PublicationFsError::UnsafeState(
            "the managed tree is being changed by another operation",
        ));
    }
    verify_fingerprint(&paths.managed, marker)?;
    if marker.revision != identity.previous_revision
        && (marker.file_count != 0 || marker.byte_count != 0)
    {
        return Err(PublicationFsError::UnsafeState(
            "the managed tree belongs to a newer published revision",
        ));
    }
    Ok(())
}

fn ensure_reset_marker_current(
    marker: &PublicationTreeMarker,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    if !marker_matches(
        marker,
        identity,
        PublicationTreeRole::Candidate,
        identity.target_revision,
    ) {
        return Err(PublicationFsError::UnsafeState(
            "the managed reset candidate operation or generation changed",
        ));
    }
    Ok(())
}

pub(crate) fn verify_empty_reset_candidate(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    with_managed_tree_lock(paths, || {
        verify_empty_reset_candidate_unlocked(paths, identity)
    })
}

fn verify_empty_reset_candidate_unlocked(
    paths: &PublicationFsPaths,
    identity: PublicationFsIdentity,
) -> Result<(), PublicationFsError> {
    let marker = read_marker(&paths.managed)?;
    ensure_reset_marker_current(&marker, identity)?;
    verify_fingerprint(&paths.managed, &marker)?;
    if marker.file_count != 0 || marker.byte_count != 0 {
        return Err(PublicationFsError::Integrity(
            "the reset candidate is not empty",
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

fn archive_revision_directory(
    root: &Path,
    directory: &Path,
    storage_parent: &Path,
    storage_owner: &str,
    list_id: EntityId,
    files: &mut Vec<ArchivedRevisionFile>,
) -> Result<(), PublicationFsError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|source| io_error("read retained revision source", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| io_error("read retained revision source entry", source))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let entry_path = entry.path();
        let relative = entry_path.strip_prefix(root).map_err(|_| {
            PublicationFsError::UnsafeState("a retained revision entry escaped its managed root")
        })?;
        if is_publication_internal_path(relative) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|source| io_error("inspect retained revision source", source))?;
        if file_type.is_symlink() {
            return Err(PublicationFsError::UnsafeState(
                "retained revision sources may not contain symbolic links",
            ));
        }
        if file_type.is_dir() {
            archive_revision_directory(
                root,
                &entry_path,
                storage_parent,
                storage_owner,
                list_id,
                files,
            )?;
        } else if file_type.is_file() {
            let published_relative_path = normalized_relative_path(relative)?;
            let (storage_relative_path, sha256, byte_length) =
                archive_revision_file(&entry_path, storage_parent, storage_owner, list_id)?;
            files.push(ArchivedRevisionFile {
                published_relative_path,
                storage_relative_path,
                sha256,
                byte_length,
            });
        } else {
            return Err(PublicationFsError::UnsafeState(
                "retained revision sources may contain only files and directories",
            ));
        }
    }
    Ok(())
}

fn is_publication_internal_path(relative: &Path) -> bool {
    relative.components().count() == 1
        && relative
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".ersatzrs-"))
}

fn normalized_relative_path(relative: &Path) -> Result<String, PublicationFsError> {
    if relative.is_absolute()
        || relative.components().next().is_none()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(PublicationFsError::UnsafeState(
            "a retained revision path is not a safe relative path",
        ));
    }
    relative
        .to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or(PublicationFsError::UnsafeState(
            "retained revision paths must be valid UTF-8",
        ))
}

fn validate_storage_owner(storage_owner: &str) -> Result<(), PublicationFsError> {
    if storage_owner.len() != 32
        || !storage_owner
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(PublicationFsError::UnsafeState(
            "the retained revision storage owner is invalid",
        ));
    }
    Ok(())
}

fn archive_revision_file(
    source: &Path,
    storage_parent: &Path,
    storage_owner: &str,
    list_id: EntityId,
) -> Result<(String, String, u64), PublicationFsError> {
    let store = storage_parent.join(REVISION_BLOB_STORE_DIRECTORY);
    ensure_plain_directory(&store)?;
    let owner_store = store.join(storage_owner);
    ensure_plain_directory(&owner_store)?;
    let list_store = owner_store.join(list_id.to_string());
    ensure_plain_directory(&list_store)?;

    let mut temporary = tempfile::NamedTempFile::new_in(&list_store)
        .map_err(|source| io_error("create retained revision blob", source))?;
    let mut input = File::open(source)
        .map_err(|source| io_error("open retained revision source file", source))?;
    let mut hasher = Sha256::new();
    let mut byte_length = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|source| io_error("read retained revision source file", source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        temporary
            .write_all(&buffer[..read])
            .map_err(|source| io_error("write retained revision blob", source))?;
        byte_length = byte_length
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .ok_or(PublicationFsError::Integrity(
                "a retained revision file is too large",
            ))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| io_error("flush retained revision blob", source))?;
    let digest = hasher.finalize();
    let sha256 = hex_digest(&digest);
    let prefix_store = list_store.join(&sha256[..2]);
    ensure_plain_directory(&prefix_store)?;
    let final_path = prefix_store.join(&sha256);
    if final_path.exists() {
        verify_revision_blob(&final_path, &sha256, byte_length)?;
    } else {
        match temporary.persist_noclobber(&final_path) {
            Ok(_) => verify_revision_blob(&final_path, &sha256, byte_length)?,
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                verify_revision_blob(&final_path, &sha256, byte_length)?;
            }
            Err(error) => return Err(io_error("publish retained revision blob", error.error)),
        }
    }
    let storage_relative_path = format!(
        "{REVISION_BLOB_STORE_DIRECTORY}/{storage_owner}/{list_id}/{}/{}",
        &sha256[..2],
        sha256
    );
    Ok((storage_relative_path, sha256, byte_length))
}

fn validate_revision_materialization(
    storage_parent: &Path,
    storage_owner: &str,
    list_id: EntityId,
    files: &[ArchivedRevisionFile],
) -> Result<Vec<PathBuf>, PublicationFsError> {
    validate_storage_owner(storage_owner)?;
    let mut published_paths = std::collections::BTreeSet::new();
    let mut sources = Vec::with_capacity(files.len());
    for file in files {
        let relative = Path::new(&file.published_relative_path);
        if normalized_relative_path(relative)? != file.published_relative_path {
            return Err(PublicationFsError::UnsafeState(
                "retained revision published paths must use canonical separators",
            ));
        }
        if !published_paths.insert(relative.to_path_buf())
            || relative
                .ancestors()
                .skip(1)
                .take_while(|ancestor| !ancestor.as_os_str().is_empty())
                .any(|ancestor| published_paths.contains(ancestor))
            || published_paths
                .iter()
                .any(|candidate| candidate != relative && candidate.starts_with(relative))
        {
            return Err(PublicationFsError::UnsafeState(
                "retained revision published paths overlap",
            ));
        }
        if file.sha256.len() != 64
            || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || file.sha256.bytes().any(|byte| byte.is_ascii_uppercase())
        {
            return Err(PublicationFsError::Integrity(
                "a retained revision blob has an invalid content digest",
            ));
        }
        let expected_storage_path = format!(
            "{REVISION_BLOB_STORE_DIRECTORY}/{}/{list_id}/{}/{}",
            storage_owner,
            &file.sha256[..2],
            file.sha256
        );
        if file.storage_relative_path != expected_storage_path {
            return Err(PublicationFsError::UnsafeState(
                "a retained revision blob has the wrong storage identity",
            ));
        }
        let source =
            revision_blob_path(storage_parent, storage_owner, &file.storage_relative_path)?;
        verify_revision_blob(&source, &file.sha256, file.byte_length)?;
        sources.push(source);
    }
    Ok(sources)
}

fn clear_managed_revision_contents(
    root: &Path,
    directory: &Path,
) -> Result<(), PublicationFsError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|source| io_error("read retained revision candidate", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| io_error("read retained revision candidate entry", source))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let entry_path = entry.path();
        let relative = entry_path.strip_prefix(root).map_err(|_| {
            PublicationFsError::UnsafeState("a retained revision candidate escaped its root")
        })?;
        if is_publication_internal_path(relative) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|source| io_error("inspect retained revision candidate", source))?;
        if file_type.is_symlink() {
            return Err(PublicationFsError::UnsafeState(
                "retained revision candidates may not contain symbolic links",
            ));
        }
        if file_type.is_dir() {
            fs::remove_dir_all(&entry_path).map_err(|source| {
                io_error("remove retained revision candidate directory", source)
            })?;
        } else if file_type.is_file() {
            fs::remove_file(&entry_path)
                .map_err(|source| io_error("remove retained revision candidate file", source))?;
        } else {
            return Err(PublicationFsError::UnsafeState(
                "retained revision candidates may contain only files and directories",
            ));
        }
    }
    Ok(())
}

fn copy_revision_blob_into_candidate(
    source: &Path,
    managed: &Path,
    file: &ArchivedRevisionFile,
) -> Result<(), PublicationFsError> {
    let relative = Path::new(&file.published_relative_path);
    let destination = managed.join(relative);
    let parent = destination.parent().ok_or(PublicationFsError::UnsafeState(
        "a retained revision target has no parent",
    ))?;
    ensure_managed_subdirectories(managed, parent)?;

    let mut input =
        File::open(source).map_err(|source| io_error("open retained revision blob", source))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|source| io_error("create retained revision candidate file", source))?;
    copy_verified_bytes(&mut input, &mut temporary, &file.sha256, file.byte_length)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| io_error("flush retained revision candidate file", source))?;
    temporary
        .persist(&destination)
        .map_err(|error| io_error("publish retained revision candidate file", error.error))?;
    verify_revision_blob(&destination, &file.sha256, file.byte_length)
}

/// Shared bounded copy for retained revision candidates and immutable saved assets.
/// The caller owns source confinement, the temporary destination and installation.
pub(crate) fn copy_verified_bytes(
    input: &mut impl std::io::Read,
    output: &mut impl std::io::Write,
    expected_sha256: &str,
    expected_byte_length: u64,
) -> Result<(), PublicationFsError> {
    let mut hasher = Sha256::new();
    let mut byte_length = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|source| io_error("read retained file content", source))?;
        if read == 0 {
            break;
        }
        byte_length = byte_length
            .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
            .filter(|length| *length <= expected_byte_length)
            .ok_or(PublicationFsError::Integrity(
                "retained file content exceeds its recorded length",
            ))?;
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|source| io_error("write retained file content", source))?;
    }
    let digest = hasher.finalize();
    if byte_length != expected_byte_length || hex_digest(&digest) != expected_sha256 {
        return Err(PublicationFsError::Integrity(
            "a retained revision blob changed while it was materialized",
        ));
    }
    Ok(())
}

pub(crate) fn ensure_managed_subdirectories(
    root: &Path,
    directory: &Path,
) -> Result<(), PublicationFsError> {
    let relative = directory.strip_prefix(root).map_err(|_| {
        PublicationFsError::UnsafeState("a retained revision target escaped its managed root")
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(PublicationFsError::UnsafeState(
                "a retained revision target is not a safe relative path",
            ));
        }
        current.push(component.as_os_str());
        ensure_plain_directory(&current)?;
    }
    Ok(())
}

fn ensure_plain_directory(path: &Path) -> Result<(), PublicationFsError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(PublicationFsError::UnsafeState(
            "retained revision storage may not contain symbolic links",
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(PublicationFsError::UnsafeState(
            "a retained revision storage path is not a directory",
        )),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)
                .map_err(|source| io_error("create retained revision storage directory", source))?;
            let metadata = fs::symlink_metadata(path)
                .map_err(|source| io_error("inspect retained revision storage", source))?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                Ok(())
            } else {
                Err(PublicationFsError::UnsafeState(
                    "retained revision storage must be a real directory",
                ))
            }
        }
        Err(source) => Err(io_error("inspect retained revision storage", source)),
    }
}

pub(crate) fn verify_revision_blob(
    path: &Path,
    expected_sha256: &str,
    expected_byte_length: u64,
) -> Result<(), PublicationFsError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| io_error("inspect retained revision blob", source))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PublicationFsError::UnsafeState(
            "a retained revision blob is not a regular file",
        ));
    }
    if metadata.len() != expected_byte_length {
        return Err(PublicationFsError::Integrity(
            "a retained revision blob has the wrong length",
        ));
    }
    let mut file =
        File::open(path).map_err(|source| io_error("open retained revision blob", source))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| io_error("read retained revision blob", source))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    if hex_digest(&digest) != expected_sha256 {
        return Err(PublicationFsError::Integrity(
            "a retained revision blob failed its content digest",
        ));
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

/// Which entries one fingerprint walk covers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FingerprintScope {
    /// Published content only: every reserved internal entry is skipped.
    Content,
    /// Content plus the scanner sidecars, excluding only the root marker.
    ///
    /// Markers sealed before the sidecars were excluded recorded this shape.
    /// Verification still accepts it so an already published revision stays
    /// verifiable; nothing seals a new marker with it.
    LegacyWithReservedState,
}

fn fingerprint_tree(root: &Path) -> Result<TreeFingerprint, PublicationFsError> {
    fingerprint_tree_scoped(root, FingerprintScope::Content)
}

fn fingerprint_tree_scoped(
    root: &Path,
    scope: FingerprintScope,
) -> Result<TreeFingerprint, PublicationFsError> {
    let mut hasher = Sha256::new();
    let mut file_count = 0_u64;
    let mut byte_count = 0_u64;
    fingerprint_directory(
        root,
        root,
        scope,
        &mut hasher,
        &mut file_count,
        &mut byte_count,
    )?;
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
    scope: FingerprintScope,
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
        let skip = match scope {
            FingerprintScope::Content => is_reserved_entry(&entry.file_name()),
            FingerprintScope::LegacyWithReservedState => {
                entry_path == root.join(PUBLICATION_MARKER_FILE)
            }
        };
        if skip {
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
        // A content fingerprint describes files, not the directories holding
        // them, because each file contributes its own root-relative path. The
        // scanner creates and empties generated folders on its own schedule, so
        // hashing a bare directory entry would let an empty folder invalidate a
        // sealed revision whose published files never changed.
        let hash_directory_entries = scope == FingerprintScope::LegacyWithReservedState;
        if file_type.is_dir() && !hash_directory_entries {
            fingerprint_directory(root, &entry_path, scope, hasher, file_count, byte_count)?;
            continue;
        }
        hasher.update(normalized.as_bytes());
        hasher.update([0]);
        if file_type.is_dir() {
            hasher.update(b"directory");
            fingerprint_directory(root, &entry_path, scope, hasher, file_count, byte_count)?;
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

/// Whether one directory entry holds internal state rather than published content.
///
/// The check is by name at any depth, so it covers the root publication marker
/// and every scanner-owned sidecar nested under a generated media folder.
pub(crate) fn is_reserved_entry(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with(RESERVED_ENTRY_PREFIX)
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
    if fingerprint_matches(path, marker, FingerprintScope::Content)?
        || fingerprint_matches(path, marker, FingerprintScope::LegacyWithReservedState)?
    {
        return Ok(());
    }
    Err(PublicationFsError::Integrity(
        "the publication tree does not match its integrity marker",
    ))
}

fn fingerprint_matches(
    path: &Path,
    marker: &PublicationTreeMarker,
    scope: FingerprintScope,
) -> Result<bool, PublicationFsError> {
    let actual = fingerprint_tree_scoped(path, scope)?;
    Ok(marker.sha256.as_deref() == Some(actual.sha256.as_str())
        && marker.file_count == actual.file_count
        && marker.byte_count == actual.byte_count)
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

    const STORAGE_OWNER: &str = "0123456789abcdef0123456789abcdef";
    const OTHER_STORAGE_OWNER: &str = "abcdef0123456789abcdef0123456789";

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
    fn stale_reset_generation_cannot_empty_a_new_generation_tree() {
        let (_temp, old_paths, old_identity) = fixture();
        prepare_empty_managed_candidate(&old_paths, old_identity)
            .expect("prepare first reset generation");
        empty_prepared_managed_candidate(&old_paths, old_identity)
            .expect("complete first reset generation");

        let new_identity = PublicationFsIdentity {
            operation_id: Some(12),
            generation: Some(3),
            ..old_identity
        };
        let new_paths = publication_paths(
            old_paths.managed.clone(),
            new_identity.list_id,
            new_identity.operation_id,
            new_identity.generation,
        )
        .expect("new reset paths");
        prepare_empty_managed_candidate(&new_paths, new_identity)
            .expect("prepare new reset generation");
        fs::write(new_paths.managed.join("new.yml"), b"new generation")
            .expect("seed new-generation file");

        let stale_generation = empty_prepared_managed_candidate(&old_paths, old_identity)
            .expect_err("older generation must not empty new candidate");
        assert!(stale_generation
            .to_string()
            .contains("operation or generation"));
        assert_eq!(
            fs::read(new_paths.managed.join("new.yml")).expect("new-generation item survives"),
            b"new generation"
        );

        empty_prepared_managed_candidate(&new_paths, new_identity)
            .expect("current generation empties its candidate");
        assert!(!new_paths.managed.join("new.yml").exists());
        verify_empty_reset_candidate(&new_paths, new_identity)
            .expect("current generation retains empty sealed ownership");
    }

    #[test]
    fn restore_carries_exact_newer_saved_document_and_assets_with_retry() {
        use crate::addon_media_list_delivery::portable::PortableDeliveryPolicy;
        use crate::addon_media_list_saved::model::*;
        use ersatzrs_domain::*;
        let (_temp, paths, identity) = fixture();
        let mut document = SavedListDocument {
            version: SAVED_LIST_VERSION,
            addon_id: "org.example.restore".into(),
            provider_id: "list".into(),
            source_url: "https://example.invalid/list".into(),
            draft: SavedListDraft {
                revision: 1,
                presentation: SavedListPresentation::default(),
                metadata: SavedMetadata::default(),
                detail_import_policy: AddonMediaListDetailPolicy::ManualOnly,
                chapter_input: String::new(),
                refresh_cadence: AddonMediaListRefreshCadence::Daily,
                refresh_publication_policy: AddonMediaListRefreshPublicationPolicy::Review,
                disk_warning_gb: 25,
                delivery: PortableDeliveryPolicy {
                    mode: AddonMediaListDeliveryMode::Stream,
                    cutoff_at: None,
                    before_cutoff: Vec::new(),
                    after_cutoff: Vec::new(),
                },
                options: SavedOptions::default(),
                items: Vec::new(),
                files: Vec::new(),
            },
            published: None,
        };
        fs::write(
            paths.managed.join(SAVED_LIST_FILE_NAME),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        create_backup(&paths, identity).unwrap();
        let asset = SavedFile {
            relative_path: format!("{SAVED_ASSETS_DIRECTORY}/poster.png"),
            sha256: hex_digest(&Sha256::digest(b"new artwork")),
            byte_length: 11,
        };
        fs::create_dir(paths.managed.join(SAVED_ASSETS_DIRECTORY)).unwrap();
        fs::write(paths.managed.join(&asset.relative_path), b"new artwork").unwrap();
        document.draft.revision = 2;
        document.draft.files.push(asset.clone());
        let bytes = serde_json::to_vec_pretty(&document).unwrap();
        fs::write(paths.managed.join(SAVED_LIST_FILE_NAME), &bytes).unwrap();
        assert_eq!(
            restore_step(&paths, identity).unwrap(),
            RestoreProgress::Advanced
        );
        // Simulate a retry after carry-over but before the directory rename.
        carry_saved_content(&paths.failed, &paths.backup).unwrap();
        assert_eq!(
            restore_step(&paths, identity).unwrap(),
            RestoreProgress::Advanced
        );
        assert_eq!(
            restore_step(&paths, identity).unwrap(),
            RestoreProgress::Complete
        );
        assert_eq!(
            fs::read(paths.managed.join(SAVED_LIST_FILE_NAME)).unwrap(),
            bytes
        );
        assert_eq!(
            fs::read(paths.managed.join(asset.relative_path)).unwrap(),
            b"new artwork"
        );
    }

    #[test]
    fn rejected_saved_document_survives_restore_cleanup_and_reset() {
        use crate::addon_media_list_saved::model::{SAVED_ASSETS_DIRECTORY, SAVED_LIST_FILE_NAME};
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).unwrap();
        fs::write(
            paths.managed.join(SAVED_LIST_FILE_NAME),
            b"invalid newer draft",
        )
        .unwrap();
        abort_staging(&paths, identity).unwrap();
        assert_eq!(
            fs::read(paths.managed.join(SAVED_LIST_FILE_NAME)).unwrap(),
            b"invalid newer draft"
        );
        fs::remove_file(paths.managed.join(SAVED_LIST_FILE_NAME)).unwrap();
        create_backup(&paths, identity).unwrap();
        fs::write(
            paths.managed.join(SAVED_LIST_FILE_NAME),
            b"invalid newer draft",
        )
        .unwrap();
        fs::create_dir(paths.managed.join(SAVED_ASSETS_DIRECTORY)).unwrap();
        fs::write(
            paths
                .managed
                .join(SAVED_ASSETS_DIRECTORY)
                .join("evidence.bin"),
            b"reserved evidence",
        )
        .unwrap();
        assert_eq!(
            restore_step(&paths, identity).unwrap(),
            RestoreProgress::Advanced
        );
        // Resume an interrupted preservation after its assets-only move.
        let recovery = saved_recovery_directory(&paths.backup, identity);
        ensure_managed_subdirectories(&paths.backup, &recovery).unwrap();
        fs::rename(
            paths.failed.join(SAVED_ASSETS_DIRECTORY),
            recovery.join(SAVED_ASSETS_DIRECTORY),
        )
        .unwrap();
        assert_eq!(
            restore_step(&paths, identity).unwrap(),
            RestoreProgress::Advanced
        );
        assert_eq!(
            restore_step(&paths, identity).unwrap(),
            RestoreProgress::Complete
        );
        cleanup_after_verification(&paths, identity, PublicationTreeRole::Previous).unwrap();
        assert!(paths.managed.join("item.yml").is_file());
        assert!(!paths.managed.join(SAVED_LIST_FILE_NAME).exists());
        let recovery = saved_recovery_directory(&paths.managed, identity);
        assert_eq!(
            fs::read(recovery.join(SAVED_LIST_FILE_NAME)).unwrap(),
            b"invalid newer draft"
        );
        assert_eq!(
            fs::read(recovery.join(SAVED_ASSETS_DIRECTORY).join("evidence.bin")).unwrap(),
            b"reserved evidence"
        );
        assert!(!paths.failed.exists());
        let reset = PublicationFsIdentity {
            operation_id: Some(99),
            generation: Some(4),
            target_revision: identity.target_revision + 1,
            ..identity
        };
        let reset_paths = publication_paths(
            paths.managed.clone(),
            reset.list_id,
            reset.operation_id,
            reset.generation,
        )
        .unwrap();
        prepare_empty_managed_candidate(&reset_paths, reset).unwrap();
        empty_prepared_managed_candidate(&reset_paths, reset).unwrap();
        assert_eq!(
            fs::read(recovery.join(SAVED_LIST_FILE_NAME)).unwrap(),
            b"invalid newer draft"
        );
        assert_eq!(
            fs::read(recovery.join(SAVED_ASSETS_DIRECTORY).join("evidence.bin")).unwrap(),
            b"reserved evidence"
        );
    }

    #[test]
    fn rejected_saved_recovery_never_overwrites_conflicting_evidence() {
        use crate::addon_media_list_saved::model::SAVED_LIST_FILE_NAME;
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).unwrap();
        fs::write(
            paths.managed.join(SAVED_LIST_FILE_NAME),
            b"new rejected document",
        )
        .unwrap();
        restore_step(&paths, identity).unwrap();
        let recovery = saved_recovery_directory(&paths.backup, identity);
        ensure_managed_subdirectories(&paths.backup, &recovery).unwrap();
        fs::write(
            recovery.join(SAVED_LIST_FILE_NAME),
            b"existing recovery evidence",
        )
        .unwrap();
        assert!(restore_step(&paths, identity).is_err());
        assert_eq!(
            fs::read(paths.failed.join(SAVED_LIST_FILE_NAME)).unwrap(),
            b"new rejected document"
        );
        assert_eq!(
            fs::read(recovery.join(SAVED_LIST_FILE_NAME)).unwrap(),
            b"existing recovery evidence"
        );
    }

    #[test]
    fn saved_content_is_outside_publication_fingerprints_and_survives_reset() {
        use crate::addon_media_list_saved::model::{SAVED_ASSETS_DIRECTORY, SAVED_LIST_FILE_NAME};
        let (_temp, paths, identity) = fixture();
        let document = paths.managed.join(SAVED_LIST_FILE_NAME);
        let assets = paths.managed.join(SAVED_ASSETS_DIRECTORY);
        fs::write(&document, b"saved draft one").unwrap();
        fs::create_dir(&assets).unwrap();
        fs::write(assets.join("poster.png"), b"saved artwork").unwrap();
        create_backup(&paths, identity).unwrap();
        seal_candidate(&paths, identity);
        let marker = read_marker(&paths.managed).unwrap();
        fs::write(&document, b"saved draft two").unwrap();
        verify_fingerprint(&paths.managed, &marker).unwrap();

        let reset = PublicationFsIdentity {
            operation_id: Some(99),
            generation: Some(4),
            previous_revision: identity.target_revision,
            target_revision: identity.target_revision + 1,
            ..identity
        };
        let reset_paths = publication_paths(
            paths.managed.clone(),
            reset.list_id,
            reset.operation_id,
            reset.generation,
        )
        .unwrap();
        prepare_empty_managed_candidate(&reset_paths, reset).unwrap();
        empty_prepared_managed_candidate(&reset_paths, reset).unwrap();
        verify_empty_reset_candidate(&reset_paths, reset).unwrap();
        assert_eq!(fs::read(&document).unwrap(), b"saved draft two");
        assert_eq!(
            fs::read(assets.join("poster.png")).unwrap(),
            b"saved artwork"
        );
        assert!(!reset_paths.managed.join("item.yml").exists());
    }

    // Unix-only fixture setup; the production FileType refusal is platform-neutral.
    #[cfg(unix)]
    #[test]
    fn saved_content_names_do_not_bypass_reset_symlink_refusal() {
        use crate::addon_media_list_saved::model::{SAVED_ASSETS_DIRECTORY, SAVED_LIST_FILE_NAME};
        for name in [SAVED_LIST_FILE_NAME, SAVED_ASSETS_DIRECTORY] {
            let (temp, paths, identity) = fixture();
            prepare_empty_managed_candidate(&paths, identity).unwrap();
            let outside = temp.path().join("outside.txt");
            fs::write(&outside, b"unowned").unwrap();
            std::os::unix::fs::symlink(&outside, paths.managed.join(name)).unwrap();
            assert!(empty_prepared_managed_candidate(&paths, identity).is_err());
            assert_eq!(fs::read(&outside).unwrap(), b"unowned");
        }
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

    /// Seal a restored revision, then let the scanner write into it the way the
    /// publication verification scan does before the tree is verified.
    fn restore_then_run_scanner(paths: &PublicationFsPaths, identity: PublicationFsIdentity) {
        create_backup(paths, identity).expect("create backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("candidate item");
        assert_eq!(
            restore_step(paths, identity).expect("move candidate aside"),
            RestoreProgress::Advanced
        );
        assert_eq!(
            restore_step(paths, identity).expect("restore previous publication"),
            RestoreProgress::Advanced
        );
        let generated = paths.managed.join("streams");
        fs::create_dir(&generated).expect("generated media folder");
        fs::write(
            generated.join(".ersatzrs-playlist.json"),
            b"{\"entries\":[]}",
        )
        .expect("scanner playlist sidecar");
        fs::write(generated.join(".ersatzrs-nfo-state.json"), b"{}").expect("scanner nfo state");
    }

    #[test]
    fn scanner_sidecars_written_after_sealing_keep_the_revision_verifiable() {
        let (_temp, paths, identity) = fixture();
        restore_then_run_scanner(&paths, identity);

        verify_managed_complete(&paths, identity, PublicationTreeRole::Previous)
            .expect("scanner state beside published content must not break the sealed marker");
    }

    #[test]
    fn scanner_sidecars_do_not_hide_tampering_with_published_content() {
        let (_temp, paths, identity) = fixture();
        restore_then_run_scanner(&paths, identity);
        fs::write(paths.managed.join("item.yml"), b"tampered").expect("tamper restored item");

        let error = verify_managed_complete(&paths, identity, PublicationTreeRole::Previous)
            .expect_err("published bytes must still be covered by the marker");

        assert!(error.to_string().contains("integrity marker"));
    }

    #[test]
    fn markers_sealed_over_scanner_sidecars_still_verify() {
        let (_temp, paths, identity) = fixture();
        let generated = paths.managed.join("streams");
        fs::create_dir(&generated).expect("generated media folder");
        fs::write(generated.join(".ersatzrs-playlist.json"), b"{}").expect("scanner sidecar");
        // A revision published before sidecars were excluded recorded them in
        // its fingerprint; that marker must keep verifying after the upgrade.
        let legacy =
            fingerprint_tree_scoped(&paths.managed, FingerprintScope::LegacyWithReservedState)
                .expect("legacy fingerprint");
        write_marker(
            &paths.managed,
            &complete_marker(
                identity,
                PublicationTreeRole::Previous,
                identity.previous_revision,
                &legacy,
            ),
        )
        .expect("legacy marker");

        verify_managed_complete(&paths, identity, PublicationTreeRole::Previous)
            .expect("a marker sealed over scanner sidecars must remain verifiable");
        upgrade_managed_fingerprint(&paths, identity, PublicationTreeRole::Previous)
            .expect("upgrade intact legacy marker before scanning");
        let upgraded = read_marker(&paths.managed).expect("upgraded marker");
        upgrade_managed_fingerprint(&paths, identity, PublicationTreeRole::Previous)
            .expect("upgrade is restart safe");
        assert_eq!(read_marker(&paths.managed).expect("marker"), upgraded);
        fs::write(
            generated.join(".ersatzrs-playlist.json"),
            b"{\"entries\":[]}",
        )
        .expect("scan rewrites sidecar");
        verify_managed_complete(&paths, identity, PublicationTreeRole::Previous)
            .expect("legacy revision remains verifiable after scanning");
        fs::write(paths.managed.join("item.yml"), b"tampered").expect("tamper content");
        assert!(
            upgrade_managed_fingerprint(&paths, identity, PublicationTreeRole::Previous).is_err()
        );
        assert_eq!(read_marker(&paths.managed).expect("marker"), upgraded);
    }

    #[test]
    fn legacy_fingerprint_upgrade_refuses_changed_sidecars() {
        let (_temp, paths, identity) = fixture();
        let sidecar = paths.managed.join(".ersatzrs-playlist.json");
        fs::write(&sidecar, b"{}").expect("sidecar");
        let legacy =
            fingerprint_tree_scoped(&paths.managed, FingerprintScope::LegacyWithReservedState)
                .expect("legacy fingerprint");
        let marker = complete_marker(
            identity,
            PublicationTreeRole::Previous,
            identity.previous_revision,
            &legacy,
        );
        write_marker(&paths.managed, &marker).expect("legacy marker");
        fs::write(sidecar, b"changed").expect("change before upgrade");
        assert!(
            upgrade_managed_fingerprint(&paths, identity, PublicationTreeRole::Previous).is_err()
        );
        assert_eq!(
            read_marker(&paths.managed).expect("unchanged marker"),
            marker
        );
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
    fn revision_archive_is_content_addressed_idempotent_and_cleanup_safe() {
        let (temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("candidate item");
        fs::create_dir(paths.managed.join("nested")).expect("nested directory");
        fs::write(paths.managed.join("nested/copy.yml"), b"candidate")
            .expect("duplicate candidate bytes");
        fs::write(paths.managed.join(".ersatzrs-internal.tmp"), b"host state")
            .expect("host-owned internal file");
        seal_candidate(&paths, identity);

        let archived = archive_managed_revision(&paths, identity, temp.path(), STORAGE_OWNER)
            .expect("archive revision");
        assert_eq!(
            archived
                .iter()
                .map(|file| file.published_relative_path.as_str())
                .collect::<Vec<_>>(),
            ["item.yml", "nested/copy.yml"]
        );
        assert_eq!(
            archived[0].storage_relative_path,
            archived[1].storage_relative_path
        );
        assert_eq!(archived[0].sha256, archived[1].sha256);
        assert_eq!(archived[0].byte_length, 9);
        let blob = revision_blob_path(
            temp.path(),
            STORAGE_OWNER,
            &archived[0].storage_relative_path,
        )
        .expect("resolve blob path");
        assert_eq!(fs::read(&blob).expect("read retained bytes"), b"candidate");
        assert_eq!(
            archive_managed_revision(&paths, identity, temp.path(), STORAGE_OWNER)
                .expect("repeat archive"),
            archived
        );

        cleanup_after_verification(&paths, identity, PublicationTreeRole::Candidate)
            .expect("clean publication scratch");
        assert!(
            blob.is_file(),
            "publication cleanup must preserve revision blobs"
        );
    }

    #[test]
    fn revision_archive_rejects_a_corrupt_existing_blob() {
        let (temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("candidate item");
        seal_candidate(&paths, identity);
        let archived = archive_managed_revision(&paths, identity, temp.path(), STORAGE_OWNER)
            .expect("archive revision");
        let blob = revision_blob_path(
            temp.path(),
            STORAGE_OWNER,
            &archived[0].storage_relative_path,
        )
        .expect("resolve blob path");
        fs::write(blob, b"corrupted").expect("corrupt retained blob");

        let error = archive_managed_revision(&paths, identity, temp.path(), STORAGE_OWNER)
            .expect_err("an existing content address must be verified");
        assert!(error.to_string().contains("content digest"));
    }

    #[test]
    fn retained_revision_survives_managed_root_relocation_and_repeated_content() {
        let (temp, source_paths, source_identity) = fixture();
        fs::write(source_paths.managed.join("item.yml"), b"retained").expect("retained source");
        create_backup(&source_paths, source_identity).expect("source backup");
        seal_candidate(&source_paths, source_identity);
        let archived =
            archive_managed_revision(&source_paths, source_identity, temp.path(), STORAGE_OWNER)
                .expect("archive source revision");
        cleanup_after_verification(
            &source_paths,
            source_identity,
            PublicationTreeRole::Candidate,
        )
        .expect("finish source publication");

        let moved_root = temp.path().join("moved-root");
        fs::create_dir(&moved_root).expect("moved storage root");
        let moved_managed = moved_root.join("managed");
        fs::rename(&source_paths.managed, &moved_managed).expect("relocate managed library");
        let moved_paths = publication_paths(
            moved_managed,
            source_identity.list_id,
            source_identity.operation_id,
            source_identity.generation,
        )
        .expect("moved publication paths");
        let repeated =
            archive_managed_revision(&moved_paths, source_identity, temp.path(), STORAGE_OWNER)
                .expect("archive repeated content after relocation");
        assert_eq!(repeated, archived);
        let independently_owned = archive_managed_revision(
            &moved_paths,
            source_identity,
            temp.path(),
            OTHER_STORAGE_OWNER,
        )
        .expect("archive identical list identity for another database owner");
        assert_ne!(
            independently_owned[0].storage_relative_path, archived[0].storage_relative_path,
            "independent databases must not share list-scoped storage ownership"
        );

        let restore_identity = PublicationFsIdentity {
            operation_id: Some(12),
            generation: Some(3),
            previous_revision: source_identity.target_revision,
            target_revision: source_identity.target_revision + 1,
            ..source_identity
        };
        let restore_paths = publication_paths(
            moved_paths.managed,
            restore_identity.list_id,
            restore_identity.operation_id,
            restore_identity.generation,
        )
        .expect("restore publication paths");
        create_backup(&restore_paths, restore_identity).expect("restore backup");
        materialize_archived_revision(
            &restore_paths,
            restore_identity,
            temp.path(),
            STORAGE_OWNER,
            &archived,
        )
        .expect("materialize revision after relocation");
        assert_eq!(
            fs::read(restore_paths.managed.join("item.yml")).expect("restored item"),
            b"retained"
        );
    }

    #[test]
    fn retained_revision_materialization_is_exact_and_idempotent() {
        let (temp, source_paths, source_identity) = fixture();
        fs::write(source_paths.managed.join("item.yml"), b"retained").expect("retained source");
        create_backup(&source_paths, source_identity).expect("source backup");
        seal_candidate(&source_paths, source_identity);
        let archived =
            archive_managed_revision(&source_paths, source_identity, temp.path(), STORAGE_OWNER)
                .expect("archive retained revision");
        cleanup_after_verification(
            &source_paths,
            source_identity,
            PublicationTreeRole::Candidate,
        )
        .expect("finish retained source publication");

        let current_identity = PublicationFsIdentity {
            operation_id: Some(12),
            generation: Some(3),
            previous_revision: source_identity.target_revision,
            target_revision: 9,
            ..source_identity
        };
        let current_paths = publication_paths(
            source_paths.managed.clone(),
            current_identity.list_id,
            current_identity.operation_id,
            current_identity.generation,
        )
        .expect("current paths");
        create_backup(&current_paths, current_identity).expect("current backup");
        fs::write(current_paths.managed.join("item.yml"), b"current").expect("current publication");
        fs::write(current_paths.managed.join("stale.yml"), b"stale")
            .expect("stale publication file");
        seal_candidate(&current_paths, current_identity);
        cleanup_after_verification(
            &current_paths,
            current_identity,
            PublicationTreeRole::Candidate,
        )
        .expect("finish current publication");

        let target_identity = PublicationFsIdentity {
            operation_id: Some(13),
            generation: Some(4),
            previous_revision: current_identity.target_revision,
            target_revision: 10,
            ..current_identity
        };
        let target_paths = publication_paths(
            current_paths.managed.clone(),
            target_identity.list_id,
            target_identity.operation_id,
            target_identity.generation,
        )
        .expect("target paths");
        create_backup(&target_paths, target_identity).expect("target backup");

        materialize_archived_revision(
            &target_paths,
            target_identity,
            temp.path(),
            STORAGE_OWNER,
            &archived,
        )
        .expect("materialize retained revision");
        assert_eq!(
            fs::read(target_paths.managed.join("item.yml")).expect("materialized item"),
            b"retained"
        );
        assert!(!target_paths.managed.join("stale.yml").exists());
        assert_eq!(
            fs::read(target_paths.backup.join("item.yml")).expect("current backup item"),
            b"current",
            "restore staging must preserve the current publication for recovery"
        );
        assert_eq!(
            read_marker(&target_paths.managed)
                .expect("candidate marker")
                .state,
            PublicationTreeState::Building
        );
        materialize_archived_revision(
            &target_paths,
            target_identity,
            temp.path(),
            STORAGE_OWNER,
            &archived,
        )
        .expect("repeat materialization");
        assert_eq!(
            fs::read(target_paths.managed.join("item.yml")).expect("repeated item"),
            b"retained"
        );
    }

    #[test]
    fn retained_revision_validates_all_blobs_before_replacing_the_candidate() {
        let (temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        fs::write(paths.managed.join("item.yml"), b"candidate").expect("candidate item");
        seal_candidate(&paths, identity);
        let archived = archive_managed_revision(&paths, identity, temp.path(), STORAGE_OWNER)
            .expect("archive revision");
        let blob = revision_blob_path(
            temp.path(),
            STORAGE_OWNER,
            &archived[0].storage_relative_path,
        )
        .expect("resolve blob path");
        fs::write(blob, b"corrupted").expect("corrupt retained blob");

        let next_identity = PublicationFsIdentity {
            operation_id: Some(12),
            generation: Some(3),
            previous_revision: identity.target_revision,
            target_revision: identity.target_revision + 1,
            ..identity
        };
        let next_paths = publication_paths(
            paths.managed.clone(),
            next_identity.list_id,
            next_identity.operation_id,
            next_identity.generation,
        )
        .expect("next publication paths");
        cleanup_after_verification(&paths, identity, PublicationTreeRole::Candidate)
            .expect("clean first publication scratch");
        create_backup(&next_paths, next_identity).expect("create next backup");
        let before = fs::read(next_paths.managed.join("item.yml")).expect("candidate before error");

        let error = materialize_archived_revision(
            &next_paths,
            next_identity,
            temp.path(),
            STORAGE_OWNER,
            &archived,
        )
        .expect_err("corrupt source must stop before replacement");
        assert!(error.to_string().contains("content digest"));
        assert_eq!(
            fs::read(next_paths.managed.join("item.yml")).expect("candidate after error"),
            before
        );
    }

    #[test]
    fn revision_blob_paths_cannot_escape_host_owned_storage() {
        let (temp, _paths, _identity) = fixture();
        let valid = format!(".ersatzrs-revision-blobs/{STORAGE_OWNER}/7/ab/abcdef");
        assert!(revision_blob_path(temp.path(), STORAGE_OWNER, &valid).is_ok());
        assert!(revision_blob_path(temp.path(), STORAGE_OWNER, "../outside").is_err());
        assert!(revision_blob_path(temp.path(), STORAGE_OWNER, "other/7/ab/abcdef").is_err());
        let foreign = valid.replace(STORAGE_OWNER, OTHER_STORAGE_OWNER);
        assert!(revision_blob_path(temp.path(), STORAGE_OWNER, &foreign).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn revision_archive_rejects_a_symlinked_blob_store() {
        use std::os::unix::fs::symlink;

        let (temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        seal_candidate(&paths, identity);
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).expect("outside directory");
        symlink(&outside, temp.path().join(REVISION_BLOB_STORE_DIRECTORY))
            .expect("symlink blob store");

        let error = archive_managed_revision(&paths, identity, temp.path(), STORAGE_OWNER)
            .expect_err("retained storage must not follow symlinks");
        assert!(error.to_string().contains("symbolic links"));
    }

    #[test]
    fn cleanup_verification_and_scratch_removal_share_the_tree_lock() {
        let (_temp, paths, identity) = fixture();
        create_backup(&paths, identity).expect("create backup");
        seal_candidate(&paths, identity);
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(managed_tree_lock_path(&paths.managed).expect("tree lock path"))
            .expect("open tree lock");
        lock.lock().expect("hold tree lock");
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let cleanup_paths = paths.clone();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).expect("signal cleanup start");
            let result = cleanup_after_verification(
                &cleanup_paths,
                identity,
                PublicationTreeRole::Candidate,
            );
            done_tx.send(result).expect("return cleanup result");
        });
        started_rx.recv().expect("cleanup worker started");

        assert!(done_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        assert!(paths.backup.exists());
        drop(lock);

        done_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("cleanup completes after lock release")
            .expect("cleanup publication");
        worker.join().expect("join cleanup worker");
        assert!(!paths.backup.exists());
    }

    #[tokio::test]
    async fn owned_tree_guard_survives_cancellation_of_running_blocking_work() {
        let (_temp, paths, _identity) = fixture();
        let guard = std::sync::Arc::new(ManagedTreeGuard::acquire(&paths.managed).unwrap());
        assert_eq!(guard.managed_root(), paths.managed);
        let probe = File::options()
            .read(true)
            .write(true)
            .open(managed_tree_lock_path(&paths.managed).unwrap())
            .unwrap();
        let worker_guard = std::sync::Arc::clone(&guard);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = tokio::task::spawn_blocking(move || {
            let _guard = worker_guard;
            started_tx.send(()).unwrap();
            // Channel ownership also releases the worker if the assertion fails.
            let _ = release_rx.recv();
        });
        started_rx.await.unwrap();
        worker.abort();
        drop(guard);
        assert!(matches!(
            probe.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        release_tx.send(()).unwrap();
        worker.await.unwrap();
        probe
            .try_lock()
            .expect("last filesystem owner released the lock");
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
        assert_eq!(
            error.redacted_cause().0,
            AddonMediaListPublicationErrorCategory::SharingViolation
        );
        drop(held);
        assert_eq!(
            restore_step(&paths, identity).expect("retry after handle release"),
            RestoreProgress::Advanced
        );
    }
}
