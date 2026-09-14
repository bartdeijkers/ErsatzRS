//! Local repository snapshots and out-of-process add-on lifecycle management.
//!
//! Add-ons are data plus executables. They never load into the ErsatzRS
//! process, and their manifests can only select host-controlled launch modes.

mod tool_execution;
pub(crate) use tool_execution::acquire_tool_lease;
pub use tool_execution::invoke_tool_update;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read as _, Write as _};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use base64::Engine as _;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use ersatzrs_addon_contract::{
    sanitize_addon_diagnostic_code, validate_addon_id, AddonCheckResult, AddonCheckStatus,
    AddonEntrypointKind, AddonIconMediaType, AddonManifest, AddonOperationError,
    AddonRepositoryIndex, AddonSettingKind, AddonToolUpdateWarning, AddonTrustTier, CatalogAddon,
    CatalogPackage, LocalRepositoryManifest, RepositoryManifest, CAPABILITY_ADDON_CHECK_V1,
    CAPABILITY_MEDIA_LIST_ACCESS_CHECK_V1, CAPABILITY_MEDIA_LIST_ENRICH_V1,
    CAPABILITY_MEDIA_LIST_IMPORT_V1, CAPABILITY_MEDIA_LIST_IMPORT_V2,
    CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1, CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1,
    CAPABILITY_MEDIA_LIST_LIST_V1, CAPABILITY_MEDIA_LIST_LIST_V2, CAPABILITY_MEDIA_LIST_LIST_V3,
    CAPABILITY_MEDIA_LIST_LIST_V4, CAPABILITY_MEDIA_LIST_LIST_V5, CAPABILITY_MEDIA_LIST_LIST_V6,
    CAPABILITY_MEDIA_LIST_PREPARE_V1, CAPABILITY_MEDIA_LIST_PREPARE_V2,
    CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1, CAPABILITY_MEDIA_LIST_SUBTITLES_V1,
    CAPABILITY_REMOTE_STREAM_ITEM_V1, CAPABILITY_REMOTE_STREAM_ITEM_V2,
    CAPABILITY_REMOTE_STREAM_LIST_V1, CAPABILITY_REMOTE_STREAM_LIST_V2,
    CAPABILITY_REMOTE_STREAM_LIST_V3, CAPABILITY_REMOTE_STREAM_LIST_V4,
    CAPABILITY_REMOTE_STREAM_PLAY_V1, CAPABILITY_REMOTE_STREAM_PLAY_V2,
    CAPABILITY_REMOTE_STREAM_PLAY_V3, CAPABILITY_REMOTE_STREAM_PLAY_V4, CAPABILITY_TOOL_UPDATE_V1,
    OPERATION_CODE_FAILED, OPERATION_CODE_MISSING_SECRET, OPERATION_CODE_MISSING_SETTING,
};
use ersatzrs_domain::EntityId;
use futures_util::StreamExt as _;
#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use sqlx::Transaction;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::process::Command;
use uuid::Uuid;

use crate::addon_media_storage::{
    effective_storage_path, ensure_media_source_in_transaction, prepare_storage_root,
};
use crate::db::{DbPool, DbRow};
use crate::file_system_layout::{
    addon_cache_folder, addon_checkouts_folder, addon_data_folder, addon_packages_folder,
    addon_repository_bundles_folder,
};
use crate::repositories::common::{decode_bool_column, decode_optional_string_column};
use crate::repositories::SqlxResultExt as _;

/// Deployment override containing the complete trusted repository allowlist.
pub const TRUSTED_REPOSITORIES_FILE_ENV: &str = "ERSATZRS_ADDONS__TRUSTED_REPOSITORIES_FILE";
/// Stable identifier of the repository enabled on a default installation.
pub const OFFICIAL_REPOSITORY_ID: &str = "org.ersatzrs.addons.official";
/// Default official catalog location.
pub const OFFICIAL_REPOSITORY_INDEX_URL: &str =
    "https://bartdeijkers.github.io/ErsatzRS-addons/index-v1.json";
/// Persisted opt-in required for unsigned repositories and local packages.
pub const ALLOW_UNVERIFIED_SOURCES_KEY: &str = "addons.allow_unverified_sources";
/// Configuration key holding the add-on used when no package claims a host.
pub const DEFAULT_REMOTE_STREAM_ADDON_KEY: &str = "addons.default_remote_stream_addon";

const MAX_REPOSITORY_FILE_BYTES: usize = 256 * 1024;
const MAX_REPOSITORY_PACKAGE_BYTES: usize = 256 * 1024;
const MAX_INDEX_BYTES: usize = 4 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: usize = 1024;
const MAX_ETAG_BYTES: usize = 1_024;
const MAX_LAST_MODIFIED_BYTES: usize = 128;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 2_048;
const MAX_CHECK_OUTPUT_BYTES: usize = 64 * 1024;
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const OUTPUT_PIPE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_REPOSITORY_RECONCILE_ATTEMPTS: usize = 8;
const FILE_SECRET_DIAGNOSTIC: &str =
    "add-on diagnostics unavailable while a protected file reference is configured";

/// Safe add-on lifecycle failure.
#[derive(Debug, thiserror::Error)]
pub enum AddonError {
    /// The deployment allowlist is invalid.
    #[error("trusted add-on repository configuration is invalid: {0}")]
    Configuration(String),
    /// Persistence failed.
    #[error("add-on persistence failed: {0}")]
    Database(String),
    /// A trusted repository or catalog entry was not found.
    #[error("{0}")]
    NotFound(String),
    /// Repository authenticity or rollback protection failed.
    #[error("add-on repository trust verification failed: {0}")]
    Trust(String),
    /// A remote artifact could not be retrieved safely.
    #[error("add-on repository download failed: {0}")]
    Download(String),
    /// A catalog, package, manifest, or setting violated its contract.
    #[error("add-on validation failed: {0}")]
    Validation(String),
    /// Installation or removal failed.
    #[error("add-on package operation failed: {0}")]
    Package(String),
    /// The add-on process failed its bounded invocation contract.
    #[error("add-on process failed: {0}")]
    Process(String),
    /// Staging must remain reserved because process termination was not confirmed.
    #[error("add-on process termination could not be confirmed")]
    TerminationUnconfirmed,
    /// The tool is currently in use; maintenance must defer without draining playback.
    #[error("add-on tool is in use")]
    ToolBusy,
    /// A previous managed invocation has no confirmed process-tree cleanup.
    #[error("add-on tool process cleanup is required")]
    ToolCleanupRequired,
    /// A list or play operation returned one structured bounded failure.
    #[error("{0}")]
    Operation(AddonOperationError),
}

impl AddonError {
    #[must_use]
    pub fn operation(code: &str, message: impl Into<String>) -> Self {
        Self::Operation(AddonOperationError::new(code, message))
    }

    fn into_operation(self) -> Self {
        match self {
            Self::Operation(_) => self,
            other => Self::operation(OPERATION_CODE_FAILED, other.to_string()),
        }
    }
}

/// One repository currently allowed by database or deployment configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedAddonRepository {
    pub id: String,
    pub name: String,
    pub index_url: String,
    /// `Base64`-encoded raw `Ed25519` public key.
    pub public_key: String,
    pub is_official: bool,
    pub is_enabled: bool,
    pub last_sequence: Option<u64>,
    pub last_refresh: Option<String>,
    pub last_error: Option<String>,
}

impl TrustedAddonRepository {
    #[must_use]
    pub fn trust_tier(&self) -> AddonTrustTier {
        if self.public_key.trim().is_empty() {
            AddonTrustTier::Unverified
        } else {
            AddonTrustTier::Verified
        }
    }
}

/// Add-on offer paired with its source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableAddon {
    pub repository_id: String,
    pub addon: CatalogAddon,
    pub host_compatible: bool,
    pub package_available: bool,
    pub has_local_icon: bool,
}

/// Bounded icon loaded from one unchanged local repository offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAddonIcon {
    pub bytes: Vec<u8>,
    pub media_type: AddonIconMediaType,
}

/// One operator-managed checkout or uploaded offline repository bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalAddonSource {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub root_path: PathBuf,
    pub last_scan: String,
    pub last_error: Option<String>,
}

/// Persisted package selected as the active version of one add-on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledAddon {
    pub addon_id: String,
    pub repository_id: Option<String>,
    pub source_id: Option<String>,
    pub version: Version,
    pub enabled: bool,
    pub trust_tier: AddonTrustTier,
    pub package_sha256: String,
    pub content_sha256: String,
    pub manifest: AddonManifest,
    pub install_path: PathBuf,
    pub data_namespace: String,
    pub installed_at: String,
    pub updated_at: String,
    pub state_revision: String,
    pub last_check_code: Option<String>,
    pub last_check_message: Option<String>,
    pub last_check_tool_update: Option<AddonToolUpdateWarning>,
    pub last_check_at: Option<String>,
}

/// Stored value or reference for one manifest-declared setting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonSettingValue {
    pub key: String,
    pub value: Option<String>,
    pub reference_kind: Option<AddonSecretReferenceKind>,
    pub reference: Option<String>,
}

/// Secret location understood by the host without persisting secret contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonSecretReferenceKind {
    Environment,
    File,
}

impl AddonSecretReferenceKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::File => "file",
        }
    }

    fn parse(value: &str) -> Result<Self, AddonError> {
        match value {
            "environment" => Ok(Self::Environment),
            "file" => Ok(Self::File),
            _ => Err(AddonError::Validation(
                "unknown secret reference kind".to_owned(),
            )),
        }
    }
}

/// Host-controlled command assembled from an installed package manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedAddonCommand {
    pub addon_id: String,
    pub program: String,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub state_revision: String,
    /// Canonical configured executables protected against concurrent maintenance.
    pub tool_paths: Vec<PathBuf>,
}

/// Context paths supplied only when a manifest declares managed `FFmpeg` access.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddonRuntimeContext {
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
}

/// One bounded host-owned add-on operation.
#[derive(Clone, Debug)]
pub struct AddonInvocation<'a> {
    pub capability: &'a str,
    pub operation: &'a str,
    pub runtime: &'a AddonRuntimeContext,
    pub extra_environment: &'a BTreeMap<String, String>,
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

/// Child process whose managed process group or job is terminated on drop.
///
/// `process-wrap` projects group-aware `start_kill` behavior, while Tokio's
/// built-in kill-on-drop flag only targets the immediate child on Unix. This
/// outer owner closes that abort/teardown gap by invoking the wrapped kill
/// operation before the wrapper chain is dropped.
#[derive(Debug)]
pub(crate) struct ManagedAddonChild {
    child: Box<dyn ChildWrapper>,
    tool_lease: Option<crate::addon_tool_gate::ToolLease>,
    terminate_on_drop: bool,
    #[cfg(unix)]
    process_group: nix::unistd::Pid,
    #[cfg(windows)]
    process_job: crate::addon_process_job::TrackedJob,
}

impl ManagedAddonChild {
    /// Wait for the leader, terminate remaining members, and confirm the
    /// managed process group/job is empty before reporting completion.
    pub(crate) async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        #[cfg(unix)]
        if self.terminate_on_drop {
            // Keep the exited leader waitable until signalling: its unreaped
            // PID pins the process-group identity across this await boundary.
            observe_process_leader_exit(self.process_group).await?;
        }
        // The dependency's Windows wait may itself wait for descendants. Wait
        // only for the leader so leftovers can be terminated immediately.
        #[cfg(windows)]
        let status = self.child.inner_mut().wait().await?;
        self.start_kill()?;
        #[cfg(unix)]
        let status = {
            // Preserve waitpid-based reaping when the host is PID 1 or a
            // subreaper. Windows instead uses the independent tracking job.
            let status = self.child.wait().await?;
            wait_for_process_group_exit(self.process_group).await?;
            status
        };
        #[cfg(windows)]
        self.process_job.wait_for_exit().await?;
        self.confirm_tool_cleanup()?;
        Ok(status)
    }

    /// Terminate the managed process group/job and wait for its leader.
    pub(crate) async fn kill(&mut self) -> std::io::Result<()> {
        self.start_kill()?;
        self.child.inner_mut().wait().await?;
        #[cfg(unix)]
        {
            self.child.wait().await?;
            wait_for_process_group_exit(self.process_group).await?;
        }
        #[cfg(windows)]
        self.process_job.wait_for_exit().await?;
        self.confirm_tool_cleanup()?;
        Ok(())
    }

    fn confirm_tool_cleanup(&mut self) -> std::io::Result<()> {
        if let Some(lease) = self.tool_lease.take() {
            lease.confirm_cleanup()?;
        }
        Ok(())
    }

    pub(crate) fn retain_tool_lease(&mut self, lease: crate::addon_tool_gate::ToolLease) {
        self.tool_lease = Some(lease);
    }

    /// Start terminating the managed process group/job without waiting.
    pub(crate) fn start_kill(&mut self) -> std::io::Result<()> {
        if !self.terminate_on_drop {
            return Ok(());
        }
        if let Err(error) = self.child.start_kill() {
            // Unix reports ESRCH when the group has already exited. Windows
            // terminates its job object instead and must preserve any error.
            #[cfg(unix)]
            let already_exited = error.raw_os_error() == Some(libc::ESRCH);
            #[cfg(not(unix))]
            let already_exited = false;
            if !already_exited {
                // Retain the drop fallback and permit an awaited kill retry.
                return Err(error);
            }
        }
        self.terminate_on_drop = false;
        Ok(())
    }
}

/// Observe only; the caller owns the timeout and must kill before any reaping.
/// Every syscall is nonblocking, so cancellation leaves the leader waitable.
#[cfg(unix)]
async fn observe_process_leader_exit(group: nix::unistd::Pid) -> std::io::Result<()> {
    use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};

    let leader = Pid::from_raw(group.as_raw())
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    loop {
        match waitid(
            WaitId::Pid(leader),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        ) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) | Err(rustix::io::Errno::INTR) => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => return Err(std::io::Error::from(error)),
        }
    }
}

/// `waitpid` can exhaust waitable children while orphaned group members still
/// exist. Probe without signalling again after the first kill, so a recycled
/// identifier cannot be targeted by a cleanup retry. Uncertain exit keeps the
/// caller's reservation charged; even zombies are conservatively not absence.
#[cfg(unix)]
async fn wait_for_process_group_exit(group: nix::unistd::Pid) -> std::io::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match nix::sys::signal::killpg(group, None) {
                Err(nix::errno::Errno::ESRCH) => return Ok(()),
                Err(error) => return Err(std::io::Error::from(error)),
                Ok(()) => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        }
    })
    .await
    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
}

impl Deref for ManagedAddonChild {
    type Target = dyn ChildWrapper;

    fn deref(&self) -> &Self::Target {
        self.child.as_ref()
    }
}

impl DerefMut for ManagedAddonChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.child.as_mut()
    }
}

impl Drop for ManagedAddonChild {
    fn drop(&mut self) {
        if self.terminate_on_drop {
            let _ = self.child.start_kill();
        }
        // Keep the gate owned while the drop fallback's process-group/job kill
        // completes. A host crash or unconfirmed exit retains the durable receipt.
        if let Some(lease) = self.tool_lease.take() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                #[cfg(unix)]
                let group = self.process_group;
                #[cfg(windows)]
                let job = self.process_job.clone();
                runtime.spawn(async move {
                    #[cfg(unix)]
                    let result = wait_for_process_group_exit(group).await;
                    #[cfg(windows)]
                    let result = job.wait_for_exit().await;
                    if result.is_ok() {
                        let _ = lease.confirm_cleanup();
                    }
                });
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedRepositoryFile {
    repositories: Vec<TrustedRepositoryEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedRepositoryEntry {
    id: String,
    name: String,
    index_url: String,
    #[serde(default)]
    public_key: String,
    #[serde(default)]
    official: bool,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct StoredCatalogCache {
    raw_json: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    state_revision: i64,
    catalog_revision: i64,
    index_url: String,
    public_key: String,
    is_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PersistedRepositoryState {
    name: String,
    index_url: String,
    public_key: String,
    is_official: bool,
    is_enabled: bool,
    state_revision: i64,
}

struct RefreshedCatalog {
    index: AddonRepositoryIndex,
    raw_json: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

enum CatalogIndexDownload {
    Modified {
        bytes: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
    NotModified {
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

const fn default_true() -> bool {
    true
}

/// Whether repository trust is controlled by the deployment file.
#[must_use]
pub fn repositories_are_deployment_managed() -> bool {
    crate::startup_config::environment_variable(TRUSTED_REPOSITORIES_FILE_ENV).is_some()
}

/// Whether the administrator accepted unsigned repositories and local packages.
///
/// # Errors
///
/// Returns an error when the persisted configuration cannot be read.
pub async fn unverified_sources_enabled(pool: &DbPool) -> Result<bool, AddonError> {
    let value =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(ALLOW_UNVERIFIED_SOURCES_KEY)
            .fetch_optional(pool)
            .await
            .map_err(database_error)?;
    Ok(value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )
    }))
}

/// Persist the explicit administrator opt-in for unverified sources.
///
/// # Errors
///
/// Returns an error when the configuration cannot be persisted.
pub async fn set_unverified_sources_enabled(
    pool: &DbPool,
    enabled: bool,
) -> Result<(), AddonError> {
    let value = if enabled { "true" } else { "false" };
    let updated = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
        .bind(value)
        .bind(ALLOW_UNVERIFIED_SOURCES_KEY)
        .execute(pool)
        .await
        .map_err(database_error)?;
    if updated.rows_affected() == 0 {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(ALLOW_UNVERIFIED_SOURCES_KEY)
            .bind(value)
            .execute(pool)
            .await
            .map_err(database_error)?;
    }
    Ok(())
}

/// The add-on used for operator source URLs no package claims, if configured.
///
/// # Errors
///
/// Returns an error when the persisted configuration cannot be read.
pub async fn default_remote_stream_addon(pool: &DbPool) -> Result<Option<String>, AddonError> {
    let value =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(DEFAULT_REMOTE_STREAM_ADDON_KEY)
            .fetch_optional(pool)
            .await
            .map_err(database_error)?;
    Ok(value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty()))
}

/// Persist or clear the default remote-stream add-on.
///
/// An empty selection clears the key rather than storing a blank value, so a
/// cleared default is indistinguishable from one that was never set.
///
/// # Errors
///
/// Returns an error when the add-on is unusable or cannot be persisted.
pub async fn set_default_remote_stream_addon(
    pool: &DbPool,
    addon_id: Option<&str>,
) -> Result<(), AddonError> {
    let addon_id = addon_id.map(str::trim).filter(|value| !value.is_empty());
    let Some(addon_id) = addon_id else {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(DEFAULT_REMOTE_STREAM_ADDON_KEY)
            .execute(pool)
            .await
            .map_err(database_error)?;
        return Ok(());
    };
    validate_addon_id(addon_id).map_err(|error| AddonError::Validation(error.to_string()))?;
    let installed = load_installed_addon(pool, addon_id)
        .await?
        .ok_or_else(|| AddonError::NotFound(format!("add-on '{addon_id}' is not installed")))?;
    if !installed.enabled {
        return Err(AddonError::Validation(format!(
            "add-on '{addon_id}' is not enabled"
        )));
    }
    if !installed.manifest.has_remote_stream_play_capability() {
        return Err(AddonError::Validation(format!(
            "add-on '{addon_id}' does not implement a Remote Stream play capability"
        )));
    }
    let updated = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
        .bind(addon_id)
        .bind(DEFAULT_REMOTE_STREAM_ADDON_KEY)
        .execute(pool)
        .await
        .map_err(database_error)?;
    if updated.rows_affected() == 0 {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(DEFAULT_REMOTE_STREAM_ADDON_KEY)
            .bind(addon_id)
            .execute(pool)
            .await
            .map_err(database_error)?;
    }
    Ok(())
}

/// Load the effective trusted repository allowlist.
///
/// When [`TRUSTED_REPOSITORIES_FILE_ENV`] is set, that file is authoritative:
/// database-only repositories are not returned and therefore cannot refresh or
/// install packages.
///
/// # Errors
///
/// Returns an error for an invalid deployment file or persistence failure.
pub async fn list_trusted_repositories(
    pool: &DbPool,
) -> Result<Vec<TrustedAddonRepository>, AddonError> {
    if let Some(path) = crate::startup_config::environment_variable(TRUSTED_REPOSITORIES_FILE_ENV) {
        let entries = read_repository_file(Path::new(&path)).await?;
        let mut repositories = Vec::with_capacity(entries.len());
        for entry in entries {
            upsert_repository_row(pool, &entry).await?;
            repositories.push(load_repository_row(pool, &entry.id).await?.ok_or_else(|| {
                AddonError::Database("managed repository was not persisted".to_owned())
            })?);
        }
        return Ok(repositories);
    }

    let rows = sqlx::query(
        r#"SELECT "Id", "Name", "IndexUrl", "PublicKey", "IsOfficial", "IsEnabled",
                  "LastSequence", "LastRefresh", "LastError"
           FROM "AddonRepository" ORDER BY "IsOfficial" DESC, "Name", "Id""#,
    )
    .fetch_all(pool)
    .await
    .map_err(database_error)?;
    rows.iter().map(repository_from_row).collect()
}

/// Add or update a database-managed trusted repository.
///
/// # Errors
///
/// Returns an error when a deployment file owns trust or the entry is invalid.
pub async fn save_trusted_repository(
    pool: &DbPool,
    repository: &TrustedAddonRepository,
) -> Result<(), AddonError> {
    if repositories_are_deployment_managed() {
        return Err(AddonError::Configuration(
            "repository trust is managed by the deployment file".to_owned(),
        ));
    }
    let is_official = load_repository_row(pool, &repository.id)
        .await?
        .is_some_and(|existing| existing.is_official);
    let entry = TrustedRepositoryEntry {
        id: repository.id.clone(),
        name: repository.name.clone(),
        index_url: repository.index_url.clone(),
        public_key: repository.public_key.clone(),
        official: is_official,
        enabled: repository.is_enabled,
    };
    validate_repository_entry(&entry)?;
    if entry.public_key.trim().is_empty() && !unverified_sources_enabled(pool).await? {
        return Err(AddonError::Trust(
            "enable unverified sources before adding an unsigned repository".to_owned(),
        ));
    }
    upsert_repository_row(pool, &entry).await
}

/// Remove a repository from the database allowlist without uninstalling its
/// already installed packages.
///
/// # Errors
///
/// Returns an error when deployment configuration owns trust or deletion fails.
pub async fn remove_trusted_repository(pool: &DbPool, id: &str) -> Result<(), AddonError> {
    if repositories_are_deployment_managed() {
        return Err(AddonError::Configuration(
            "repository trust is managed by the deployment file".to_owned(),
        ));
    }
    sqlx::query(r#"DELETE FROM "AddonRepository" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await
        .map_err(database_error)?;
    Ok(())
}

/// Download, authenticate, validate, and persist one repository catalog.
///
/// # Errors
///
/// Returns an error without replacing the last valid catalog when refresh
/// fails. Safe failure detail is persisted for the settings page and health.
#[allow(clippy::too_many_lines)]
pub async fn refresh_repository(
    pool: &DbPool,
    client: &reqwest::Client,
    repository_id: &str,
) -> Result<AddonRepositoryIndex, AddonError> {
    let repository = effective_repository(pool, repository_id).await?;
    if !repository.is_enabled {
        return Err(AddonError::Validation("repository is disabled".to_owned()));
    }
    if repository.public_key.trim().is_empty() {
        if repository.is_official {
            return Err(AddonError::Trust(
                "the official repository has no pinned signing key".to_owned(),
            ));
        }
        if !unverified_sources_enabled(pool).await? {
            return Err(AddonError::Trust(
                "enable unverified sources before refreshing an unsigned repository".to_owned(),
            ));
        }
    }

    let previous_cache = load_catalog_cache(pool, repository_id).await?;
    if previous_cache.index_url != repository.index_url
        || previous_cache.public_key != repository.public_key
        || previous_cache.is_enabled != repository.is_enabled
    {
        return Err(AddonError::Trust(
            "repository trust changed before refresh started".to_owned(),
        ));
    }
    let result = refresh_repository_inner(client, &repository, &previous_cache)
        .await
        .and_then(|refreshed| {
            validate_repeated_catalog_sequence(
                repository.last_sequence,
                previous_cache.raw_json.as_deref(),
                refreshed.index.sequence,
                &refreshed.raw_json,
            )?;
            Ok(refreshed)
        });
    match result {
        Ok(refreshed) => {
            let now = Utc::now().to_rfc3339();
            let sequence = i64::try_from(refreshed.index.sequence).map_err(|_| {
                AddonError::Validation("catalog sequence exceeds database range".to_owned())
            })?;
            let catalog_changed = repository.last_sequence != Some(refreshed.index.sequence)
                || previous_cache.raw_json.as_deref() != Some(refreshed.raw_json.as_str());
            let updated = if catalog_changed {
                sqlx::query(
                    r#"UPDATE "AddonRepository"
                       SET "LastSequence" = ?, "LastRefresh" = ?, "LastError" = NULL,
                           "CatalogJson" = ?, "CatalogEtag" = ?, "CatalogLastModified" = ?,
                           "StateRevision" = "StateRevision" + 1,
                           "CatalogRevision" = "CatalogRevision" + 1
                       WHERE "Id" = ? AND "StateRevision" = ?"#,
                )
                .bind(sequence)
                .bind(&now)
                .bind(&refreshed.raw_json)
                .bind(&refreshed.etag)
                .bind(&refreshed.last_modified)
                .bind(repository_id)
                .bind(previous_cache.state_revision)
                .execute(pool)
                .await
                .map_err(database_error)?
            } else {
                sqlx::query(
                    r#"UPDATE "AddonRepository"
                       SET "LastSequence" = ?, "LastRefresh" = ?, "LastError" = NULL,
                           "CatalogJson" = ?, "CatalogEtag" = ?, "CatalogLastModified" = ?,
                           "StateRevision" = "StateRevision" + 1
                       WHERE "Id" = ? AND "StateRevision" = ?"#,
                )
                .bind(sequence)
                .bind(&now)
                .bind(&refreshed.raw_json)
                .bind(&refreshed.etag)
                .bind(&refreshed.last_modified)
                .bind(repository_id)
                .bind(previous_cache.state_revision)
                .execute(pool)
                .await
                .map_err(database_error)?
            };
            if updated.rows_affected() != 1 {
                return Err(AddonError::Trust(
                    "repository trust or cached catalog changed while refresh was in progress"
                        .to_owned(),
                ));
            }
            Ok(refreshed.index)
        }
        Err(error) => {
            let safe = bounded_text(&error.to_string(), 1_024);
            sqlx::query(
                r#"UPDATE "AddonRepository"
                   SET "LastError" = ?, "StateRevision" = "StateRevision" + 1
                   WHERE "Id" = ? AND "StateRevision" = ?"#,
            )
            .bind(safe)
            .bind(repository_id)
            .bind(previous_cache.state_revision)
            .execute(pool)
            .await
            .map_err(database_error)?;
            Err(error)
        }
    }
}

async fn refresh_repository_inner(
    client: &reqwest::Client,
    repository: &TrustedAddonRepository,
    previous_cache: &StoredCatalogCache,
) -> Result<RefreshedCatalog, AddonError> {
    if repository.is_official && repository.public_key.trim().is_empty() {
        return Err(AddonError::Trust(
            "the official repository has no pinned signing key".to_owned(),
        ));
    }
    if !repository.public_key.trim().is_empty() {
        decode_public_key(&repository.public_key)?;
    }
    let download = download_catalog_index(
        client,
        &repository.index_url,
        previous_cache.etag.as_deref(),
        previous_cache.last_modified.as_deref(),
    )
    .await?;
    let (raw_json, etag, last_modified, not_modified) = match download {
        CatalogIndexDownload::Modified {
            bytes,
            etag,
            last_modified,
        } => {
            if !repository.public_key.trim().is_empty() {
                let signature_url = format!("{}.sig", repository.index_url);
                let signature_bytes =
                    download_bounded(client, &signature_url, MAX_SIGNATURE_BYTES).await?;
                verify_detached_signature(repository.public_key.trim(), &bytes, &signature_bytes)?;
            }
            let raw_json = String::from_utf8(bytes)
                .map_err(|_| AddonError::Validation("catalog is not UTF-8 JSON".to_owned()))?;
            (raw_json, etag, last_modified, false)
        }
        CatalogIndexDownload::NotModified {
            etag,
            last_modified,
        } => {
            let raw_json = previous_cache.raw_json.clone().ok_or_else(|| {
                AddonError::Validation(
                    "repository returned not modified without a cached catalog".to_owned(),
                )
            })?;
            (
                raw_json,
                etag.or_else(|| previous_cache.etag.clone()),
                last_modified.or_else(|| previous_cache.last_modified.clone()),
                true,
            )
        }
    };
    let index: AddonRepositoryIndex = serde_json::from_str(&raw_json).map_err(|_| {
        AddonError::Validation("catalog JSON does not match a supported schema".to_owned())
    })?;
    if not_modified && repository.last_sequence != Some(index.sequence) {
        return Err(AddonError::Trust(
            "cached catalog sequence does not match trusted repository state".to_owned(),
        ));
    }
    validate_catalog(&index, repository, Utc::now())?;
    Ok(RefreshedCatalog {
        index,
        raw_json,
        etag,
        last_modified,
    })
}

/// List validated catalog entries from the last successful refresh.
///
/// # Errors
///
/// Invalid or expired stored catalogs are omitted. Their repository refresh
/// and health state remains visible, while another repository can still be
/// administered.
pub async fn list_available_addons(pool: &DbPool) -> Result<Vec<AvailableAddon>, AddonError> {
    let repositories = list_trusted_repositories(pool).await?;
    let host_version = host_version()?;
    let rid = current_rid()?;
    let mut available = Vec::new();
    for repository in repositories.into_iter().filter(|item| item.is_enabled) {
        let catalog = load_catalog_json(pool, &repository.id).await?;
        let Some(catalog) = catalog else {
            continue;
        };
        let Ok(index) = serde_json::from_str::<AddonRepositoryIndex>(&catalog) else {
            tracing::warn!(
                repository_id = %repository.id,
                "ignoring an invalid stored add-on catalog"
            );
            continue;
        };
        if let Err(error) = validate_catalog(&index, &repository, Utc::now()) {
            tracing::warn!(
                repository_id = %repository.id,
                error = %error,
                "ignoring an unavailable stored add-on catalog"
            );
            continue;
        }
        available.extend(index.addons.into_iter().map(|addon| AvailableAddon {
            host_compatible: addon.host_version.matches(&host_version),
            package_available: addon.packages.contains_key(rid)
                || addon.packages.contains_key("any"),
            repository_id: repository.id.clone(),
            has_local_icon: false,
            addon,
        }));
    }
    available.sort_by(|left, right| {
        localized_name(&left.addon)
            .cmp(localized_name(&right.addon))
            .then_with(|| left.addon.id.cmp(&right.addon.id))
            .then_with(|| left.repository_id.cmp(&right.repository_id))
    });
    Ok(available)
}

/// Download and install one exact catalog package. Installation never enables
/// the add-on and never runs it.
///
/// # Errors
///
/// Returns an error for an untrusted, incompatible, malformed, oversized, or
/// hash-mismatched package.
pub async fn install_addon(
    pool: &DbPool,
    client: &reqwest::Client,
    repository_id: &str,
    addon_id: &str,
    runtime: &AddonRuntimeContext,
) -> Result<InstalledAddon, AddonError> {
    let repository = effective_repository(pool, repository_id).await?;
    let catalog_cache = load_catalog_cache(pool, repository_id).await?;
    if catalog_cache.index_url != repository.index_url
        || catalog_cache.public_key != repository.public_key
        || !catalog_cache.is_enabled
    {
        return Err(AddonError::Trust(
            "repository trust changed before installation started".to_owned(),
        ));
    }
    let raw_catalog = catalog_cache
        .raw_json
        .as_deref()
        .ok_or_else(|| AddonError::NotFound("repository has not been refreshed".to_owned()))?;
    let index: AddonRepositoryIndex = serde_json::from_str(raw_catalog)
        .map_err(|_| AddonError::Validation("stored catalog is invalid".to_owned()))?;
    validate_catalog(&index, &repository, Utc::now())?;
    let addon = index
        .addons
        .iter()
        .find(|item| item.id == addon_id)
        .ok_or_else(|| AddonError::NotFound("add-on is not in this repository".to_owned()))?;
    if !addon.host_version.matches(&host_version()?) {
        return Err(AddonError::Validation(
            "add-on does not support this ErsatzRS version".to_owned(),
        ));
    }
    let installed = load_installed_addon(pool, addon_id).await?;
    if let Some(installed) = &installed {
        if installed.repository_id.as_deref() != Some(repository_id) {
            return Err(AddonError::Trust(
                "installed add-on belongs to a different repository; uninstall it before changing sources"
                    .to_owned(),
            ));
        }
        if addon.version == installed.version {
            return Err(AddonError::Validation(
                "this add-on version is already installed".to_owned(),
            ));
        }
        if addon.version < installed.version {
            return Err(AddonError::Trust(
                "catalog package would downgrade the installed add-on".to_owned(),
            ));
        }
    }
    let rid = current_rid()?;
    let package = addon
        .packages
        .get(rid)
        .or_else(|| addon.packages.get("any"))
        .ok_or_else(|| {
            AddonError::Validation("add-on has no package for this platform".to_owned())
        })?;
    install_catalog_package(
        pool,
        client,
        repository_id,
        repository.trust_tier(),
        catalog_cache.catalog_revision,
        installed.as_ref(),
        addon,
        package,
        runtime,
    )
    .await
}

/// Ensure the operator-managed checkout root exists and return its path.
///
/// # Errors
///
/// Returns an error when the state directory cannot be created.
pub async fn ensure_addon_checkouts_folder() -> Result<PathBuf, AddonError> {
    let path = addon_checkouts_folder();
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(package_io_error)?;
    Ok(path)
}

/// Scan every direct child of the operator-managed checkout directory.
///
/// Git acquisition is deliberately outside ErsatzRS. Each valid checkout is
/// reconciled atomically; a failed scan leaves its previous offers intact.
///
/// # Errors
///
/// Returns an error when unknown sources are disabled or the checkout root
/// cannot be enumerated. Individual invalid repositories are recorded/logged
/// without preventing other checkouts from being discovered.
pub async fn scan_local_addon_repositories(pool: &DbPool) -> Result<usize, AddonError> {
    if !unverified_sources_enabled(pool).await? {
        return Err(AddonError::Trust(
            "enable unknown sources before scanning local add-on repositories".to_owned(),
        ));
    }
    let checkout_root = ensure_addon_checkouts_folder().await?;
    let mut entries = tokio::fs::read_dir(&checkout_root)
        .await
        .map_err(package_io_error)?;
    let mut scanned = 0_usize;
    let mut discovered_roots = BTreeSet::new();
    while let Some(entry) = entries.next_entry().await.map_err(package_io_error)? {
        if !entry.file_type().await.map_err(package_io_error)?.is_dir() {
            continue;
        }
        let path = entry.path();
        let recorded_path = canonicalize_addon_path(&path).await.unwrap_or(path.clone());
        discovered_roots.insert(recorded_path.to_string_lossy().into_owned());
        match scan_local_repository_root(pool, &path, "checkout").await {
            Ok(_) => scanned = scanned.saturating_add(1),
            Err(error) => {
                record_local_source_scan_error(pool, &recorded_path, &error).await;
                tracing::warn!(path = %path.display(), %error, "local add-on repository scan failed");
            }
        }
    }
    reconcile_missing_checkout_sources(pool, &discovered_roots).await?;
    Ok(scanned)
}

/// List local checkout and offline-bundle sources.
///
/// # Errors
///
/// Returns an error when source state cannot be decoded.
pub async fn list_local_addon_sources(pool: &DbPool) -> Result<Vec<LocalAddonSource>, AddonError> {
    let rows = sqlx::query(
        r#"SELECT "Id", "Name", "Kind", "RootPath", "LastScan", "LastError"
           FROM "AddonSource" ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await
    .map_err(database_error)?;
    rows.iter()
        .map(|row| {
            Ok(LocalAddonSource {
                id: row.try_get("Id").repository().map_err(repository_error)?,
                name: row.try_get("Name").repository().map_err(repository_error)?,
                kind: row.try_get("Kind").repository().map_err(repository_error)?,
                root_path: PathBuf::from(
                    row.try_get::<String, _>("RootPath")
                        .repository()
                        .map_err(repository_error)?,
                ),
                last_scan: row
                    .try_get("LastScan")
                    .repository()
                    .map_err(repository_error)?,
                last_error: decode_optional_string_column(row, "LastError")
                    .map_err(repository_error)?,
            })
        })
        .collect()
}

/// List add-on offers from the last valid local-source scans.
///
/// The existing catalog-shaped read model is retained at the application
/// boundary so the mature manager/detail UI can keep its card and permission
/// rendering. Package URLs are inert local placeholders and are never fetched.
///
/// # Errors
///
/// Returns an error when persisted source or manifest state is invalid.
pub async fn list_local_available_addons(pool: &DbPool) -> Result<Vec<AvailableAddon>, AddonError> {
    let rows = sqlx::query(
        r#"SELECT "SourceId", "AddonId", "ManifestJson", "ContentSha256"
           FROM "AddonSourceOffer" ORDER BY "SourceId", "AddonId""#,
    )
    .fetch_all(pool)
    .await
    .map_err(database_error)?;
    let host = host_version()?;
    let rid = current_rid()?;
    let mut offers = Vec::with_capacity(rows.len());
    for row in rows {
        let source_id: String = row
            .try_get("SourceId")
            .repository()
            .map_err(repository_error)?;
        let manifest_json: String = row
            .try_get("ManifestJson")
            .repository()
            .map_err(repository_error)?;
        let manifest: AddonManifest = serde_json::from_str(&manifest_json).map_err(|_| {
            AddonError::Validation("stored local add-on manifest is invalid".to_owned())
        })?;
        manifest
            .validate()
            .map_err(|error| AddonError::Validation(error.to_string()))?;
        let digest: String = row
            .try_get("ContentSha256")
            .repository()
            .map_err(repository_error)?;
        let addon = catalog_view_from_manifest(&manifest, rid, &digest);
        offers.push(AvailableAddon {
            repository_id: source_id,
            host_compatible: manifest.host_version.matches(&host),
            package_available: manifest.entrypoint_for(rid).is_some(),
            has_local_icon: manifest.icon.is_some(),
            addon,
        });
    }
    offers.sort_by(|left, right| {
        localized_name(&left.addon)
            .cmp(localized_name(&right.addon))
            .then_with(|| left.addon.id.cmp(&right.addon.id))
            .then_with(|| left.repository_id.cmp(&right.repository_id))
    });
    Ok(offers)
}

/// Load an offer icon only while the local source still matches its scan.
///
/// # Errors
///
/// Returns an error when persisted source state is invalid, the checkout has
/// changed since its scan, or the declared icon is no longer safe and valid.
pub async fn load_local_addon_icon(
    pool: &DbPool,
    source_id: &str,
    addon_id: &str,
) -> Result<Option<LocalAddonIcon>, AddonError> {
    let Some(row) = sqlx::query(
        r#"SELECT source."RootPath", offer."RelativePath", offer."ManifestJson",
                  offer."ContentSha256"
           FROM "AddonSourceOffer" AS offer
           JOIN "AddonSource" AS source ON source."Id" = offer."SourceId"
           WHERE offer."SourceId" = ? AND offer."AddonId" = ?"#,
    )
    .bind(source_id)
    .bind(addon_id)
    .fetch_optional(pool)
    .await
    .map_err(database_error)?
    else {
        return Ok(None);
    };
    let manifest: AddonManifest = serde_json::from_str(
        &row.try_get::<String, _>("ManifestJson")
            .repository()
            .map_err(repository_error)?,
    )
    .map_err(|_| AddonError::Validation("stored local add-on manifest is invalid".to_owned()))?;
    let Some(icon) = manifest.icon else {
        return Ok(None);
    };
    let root = PathBuf::from(
        row.try_get::<String, _>("RootPath")
            .repository()
            .map_err(repository_error)?,
    )
    .join(
        row.try_get::<String, _>("RelativePath")
            .repository()
            .map_err(repository_error)?,
    );
    let expected_digest: String = row
        .try_get("ContentSha256")
        .repository()
        .map_err(repository_error)?;
    if hash_addon_tree(root.clone()).await? != expected_digest {
        return Err(AddonError::Trust(
            "local repository changed after its last scan; rescan before loading artwork"
                .to_owned(),
        ));
    }
    let icon_path = root.join(icon.path);
    let metadata = tokio::fs::metadata(&icon_path)
        .await
        .map_err(package_io_error)?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err(AddonError::Validation(
            "local offer icon is not a bounded regular file".to_owned(),
        ));
    }
    let bytes = tokio::fs::read(icon_path).await.map_err(package_io_error)?;
    if !valid_icon_bytes(&bytes, icon.media_type) {
        return Err(AddonError::Validation(
            "local offer icon bytes do not match the declared media type".to_owned(),
        ));
    }
    Ok(Some(LocalAddonIcon {
        bytes,
        media_type: icon.media_type,
    }))
}

/// Receipt from the committed local repository scan, without private paths.
#[derive(Debug)]
pub struct LocalRepositoryImport {
    pub id: String,
    pub name: String,
    pub available_count: usize,
}

/// Import a self-contained offline repository bundle.
///
/// # Errors
///
/// Returns an error for a disabled trust gate, unsafe archive, invalid local
/// repository layout, or source-kind identity conflict.
pub async fn install_local_repository_bundle(
    pool: &DbPool,
    archive: Vec<u8>,
) -> Result<LocalRepositoryImport, AddonError> {
    if !unverified_sources_enabled(pool).await? {
        return Err(AddonError::Trust(
            "enable unknown sources before installing a local repository bundle".to_owned(),
        ));
    }
    if archive.is_empty() || archive.len() as u64 > 512 * 1024 * 1024 {
        return Err(AddonError::Validation(
            "repository bundle is empty or too large".to_owned(),
        ));
    }
    let bundle_root = addon_repository_bundles_folder();
    tokio::fs::create_dir_all(&bundle_root)
        .await
        .map_err(package_io_error)?;
    let staging = bundle_root.join(format!(".staging-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(package_io_error)?;
    let extraction_path = staging.clone();
    let extraction =
        tokio::task::spawn_blocking(move || extract_zip_safely(&archive, &extraction_path))
            .await
            .map_err(|_| AddonError::Package("repository extraction task failed".to_owned()))?;
    let result = async {
        extraction?;
        let manifest = read_local_repository_manifest(&staging).await?;
        let existing =
            sqlx::query(r#"SELECT "Kind", "RootPath" FROM "AddonSource" WHERE "Id" = ?"#)
                .bind(&manifest.id)
                .fetch_optional(pool)
                .await
                .map_err(database_error)?;
        let previous_root = if let Some(row) = existing {
            let kind: String = row.try_get("Kind").repository().map_err(repository_error)?;
            if kind != "bundle" {
                return Err(AddonError::Trust(
                    "a checkout already owns this local repository identity".to_owned(),
                ));
            }
            Some(PathBuf::from(
                row.try_get::<String, _>("RootPath")
                    .repository()
                    .map_err(repository_error)?,
            ))
        } else {
            None
        };
        let digest = hash_addon_tree(staging.clone()).await?;
        let destination = bundle_root.join(&manifest.id).join(&digest[..16]);
        if destination.exists() {
            tokio::fs::remove_dir_all(&staging)
                .await
                .map_err(package_io_error)?;
        } else {
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(package_io_error)?;
            }
            tokio::fs::rename(&staging, &destination)
                .await
                .map_err(package_io_error)?;
        }
        let receipt = scan_local_repository_root(pool, &destination, "bundle").await?;
        if let Some(previous_root) = previous_root.filter(|path| path != &destination) {
            if let Err(error) = tokio::fs::remove_dir_all(&previous_root).await {
                tracing::warn!(
                    path = %previous_root.display(),
                    %error,
                    "failed to remove replaced offline add-on repository bundle"
                );
            }
        }
        Ok(receipt)
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_dir_all(&staging).await;
    }
    result
}

/// Remove one uploaded offline repository bundle without uninstalling package snapshots.
///
/// Checkout sources are operator-owned and cannot be removed by ErsatzRS. Deleting a bundle
/// source cascades its offers and clears `AddonInstallation.SourceId`; installed packages,
/// settings, data namespaces, and immutable package directories remain untouched.
///
/// # Errors
///
/// Returns an error when the source does not exist, is not a bundle, escaped the managed bundle
/// root, or its database/filesystem state cannot be removed safely.
pub async fn remove_local_repository_bundle(
    pool: &DbPool,
    repository_id: &str,
) -> Result<(), AddonError> {
    remove_local_repository_bundle_from(pool, repository_id, &addon_repository_bundles_folder())
        .await
}

async fn remove_local_repository_bundle_from(
    pool: &DbPool,
    repository_id: &str,
    bundle_root: &Path,
) -> Result<(), AddonError> {
    let repository_id = repository_id.trim();
    if repository_id.is_empty() {
        return Err(AddonError::Validation(
            "repository id must not be empty".to_owned(),
        ));
    }
    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    let row = sqlx::query(r#"SELECT "Kind", "RootPath" FROM "AddonSource" WHERE "Id" = ?"#)
        .bind(repository_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(database_error)?
        .ok_or_else(|| AddonError::NotFound("local add-on repository was not found".to_owned()))?;
    let kind: String = row.try_get("Kind").repository().map_err(repository_error)?;
    if kind != "bundle" {
        return Err(AddonError::Validation(
            "only uploaded repository bundles can be removed by ErsatzRS".to_owned(),
        ));
    }
    let stored_root = PathBuf::from(
        row.try_get::<String, _>("RootPath")
            .repository()
            .map_err(repository_error)?,
    );
    let removable_root = validated_bundle_path_for_removal(&stored_root, bundle_root).await?;

    tokio::fs::create_dir_all(bundle_root)
        .await
        .map_err(package_io_error)?;
    let tombstone = bundle_root.join(format!(".removing-{}", Uuid::new_v4()));
    let root_exists = tokio::fs::try_exists(&removable_root)
        .await
        .map_err(package_io_error)?;
    let deleted = sqlx::query(r#"DELETE FROM "AddonSource" WHERE "Id" = ? AND "Kind" = 'bundle'"#)
        .bind(repository_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    if deleted.rows_affected() != 1 {
        return Err(AddonError::NotFound(
            "local add-on repository was not found".to_owned(),
        ));
    }
    if root_exists {
        tokio::fs::rename(&removable_root, &tombstone)
            .await
            .map_err(package_io_error)?;
    }
    if let Err(error) = transaction.commit().await.map_err(database_error) {
        if root_exists {
            let _ = tokio::fs::rename(&tombstone, &removable_root).await;
        }
        return Err(error);
    }
    if root_exists {
        tokio::fs::remove_dir_all(&tombstone)
            .await
            .map_err(package_io_error)?;
        remove_empty_bundle_parent(&removable_root, bundle_root).await;
    }
    Ok(())
}

async fn validated_bundle_path_for_removal(
    stored_path: &Path,
    bundle_root: &Path,
) -> Result<PathBuf, AddonError> {
    let relative = stored_path.strip_prefix(bundle_root).map_err(|_| {
        AddonError::Package("stored repository path is outside the bundle root".to_owned())
    })?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(AddonError::Package(
            "stored repository path is outside the bundle root".to_owned(),
        ));
    }
    tokio::fs::create_dir_all(bundle_root)
        .await
        .map_err(package_io_error)?;
    let canonical_root = containment_path_async(bundle_root)
        .await
        .map_err(package_io_error)?;
    match containment_path_async(stored_path).await {
        Ok(canonical_path)
            if canonical_path != canonical_root && canonical_path.starts_with(&canonical_root) =>
        {
            Ok(canonical_path)
        }
        Ok(_) => Err(AddonError::Package(
            "stored repository path is outside the bundle root".to_owned(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(stored_path.to_path_buf()),
        Err(error) => Err(package_io_error(error)),
    }
}

async fn remove_empty_bundle_parent(removed_root: &Path, bundle_root: &Path) {
    let Some(parent) = removed_root.parent() else {
        return;
    };
    if parent == bundle_root {
        return;
    }
    let Ok(mut entries) = tokio::fs::read_dir(parent).await else {
        return;
    };
    if entries
        .next_entry()
        .await
        .is_ok_and(|entry| entry.is_none())
    {
        let _ = tokio::fs::remove_dir(parent).await;
    }
}

/// Install or update one exact offer from a local source snapshot.
///
/// `adopt_legacy` is the only path that may replace a catalog-owned package or
/// accept the same semantic version. Settings and the existing data namespace
/// are preserved in that explicit transition.
///
/// # Errors
///
/// Returns an error for stale source contents, incompatible manifests, source
/// conflicts, downgrades, or persistence races.
pub async fn install_addon_from_local_source(
    pool: &DbPool,
    source_id: &str,
    addon_id: &str,
    adopt_legacy: bool,
    runtime: &AddonRuntimeContext,
) -> Result<InstalledAddon, AddonError> {
    if !unverified_sources_enabled(pool).await? {
        return Err(AddonError::Trust(
            "enable unknown sources before installing from a local repository".to_owned(),
        ));
    }
    let row = sqlx::query(
        r#"SELECT source."RootPath", source."StateRevision", offer."ManifestJson",
                  offer."RelativePath", offer."ContentSha256"
           FROM "AddonSourceOffer" AS offer
           JOIN "AddonSource" AS source ON source."Id" = offer."SourceId"
           WHERE offer."SourceId" = ? AND offer."AddonId" = ?"#,
    )
    .bind(source_id)
    .bind(addon_id)
    .fetch_optional(pool)
    .await
    .map_err(database_error)?
    .ok_or_else(|| AddonError::NotFound("add-on is not offered by this local source".to_owned()))?;
    let root = PathBuf::from(
        row.try_get::<String, _>("RootPath")
            .repository()
            .map_err(repository_error)?,
    );
    let relative: String = row
        .try_get("RelativePath")
        .repository()
        .map_err(repository_error)?;
    let source_revision: i64 = row
        .try_get("StateRevision")
        .repository()
        .map_err(repository_error)?;
    let expected_digest: String = row
        .try_get("ContentSha256")
        .repository()
        .map_err(repository_error)?;
    let addon_root = root.join(relative);
    let manifest = validate_extracted_manifest(&addon_root, current_rid()?).await?;
    if manifest.id != addon_id {
        return Err(AddonError::Trust(
            "local offer identity changed after scan".to_owned(),
        ));
    }
    let actual_digest = hash_addon_tree(addon_root.clone()).await?;
    if actual_digest != expected_digest {
        return Err(AddonError::Trust(
            "local repository changed after its last scan; rescan before installing".to_owned(),
        ));
    }
    if !manifest.host_version.matches(&host_version()?) {
        return Err(AddonError::Validation(
            "add-on does not support this ErsatzRS version".to_owned(),
        ));
    }
    let previous = load_installed_addon(pool, addon_id).await?;
    if let Some(previous) = &previous {
        let legacy = previous.source_id.is_none() && previous.repository_id.is_some();
        if legacy && !adopt_legacy {
            return Err(AddonError::Trust(
                "confirm adoption before replacing a legacy catalog installation".to_owned(),
            ));
        }
        if !legacy && previous.source_id.as_deref() != Some(source_id) {
            return Err(AddonError::Trust(
                "installed add-on belongs to a different source".to_owned(),
            ));
        }
        if manifest.version < previous.version {
            return Err(AddonError::Trust(
                "local source would downgrade the installed add-on".to_owned(),
            ));
        }
        if manifest.version == previous.version && !legacy {
            return Err(AddonError::Validation(
                "this add-on version is already installed".to_owned(),
            ));
        }
    }
    let archive = archive_local_addon(addon_root).await?;
    let package_sha256 = sha256_hex(&archive);
    let staged = stage_sideload_package(current_rid()?, archive, package_sha256).await?;
    persist_local_addon(pool, source_id, source_revision, previous.as_ref(), &staged).await?;
    restore_enabled_after_update(pool, addon_id, previous.as_ref(), &staged.manifest, runtime)
        .await;
    load_installed_addon(pool, addon_id)
        .await?
        .ok_or_else(|| AddonError::Database("local add-on was not persisted".to_owned()))
}

async fn scan_local_repository_root(
    pool: &DbPool,
    root: &Path,
    kind: &str,
) -> Result<LocalRepositoryImport, AddonError> {
    let manifest = read_local_repository_manifest(root).await?;
    let canonical_root = canonicalize_addon_path(root)
        .await
        .map_err(package_io_error)?;
    let offers = collect_local_offers(&canonical_root).await?;
    let manifest_json = serde_json::to_string(&manifest).map_err(|_| {
        AddonError::Validation("repository manifest could not be stored".to_owned())
    })?;
    let mut fingerprint = Sha256::new();
    fingerprint.update(manifest_json.as_bytes());
    for (addon, _, digest) in &offers {
        fingerprint.update(addon.id.as_bytes());
        fingerprint.update(digest.as_bytes());
    }
    let content_sha256 = hex_digest(fingerprint.finalize());
    let root_path = canonical_root.to_string_lossy().into_owned();
    let now = Utc::now().to_rfc3339();
    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    let existing_kind =
        sqlx::query_scalar::<_, String>(r#"SELECT "Kind" FROM "AddonSource" WHERE "Id" = ?"#)
            .bind(&manifest.id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(database_error)?;
    if existing_kind
        .as_deref()
        .is_some_and(|existing| existing != kind)
    {
        return Err(AddonError::Trust(
            "another local source kind already owns this repository identity".to_owned(),
        ));
    }
    sqlx::query(
        r#"INSERT INTO "AddonSource" (
               "Id", "Name", "Kind", "RootPath", "ManifestJson", "ContentSha256",
               "LastScan", "LastError", "StateRevision"
           ) VALUES (?, ?, ?, ?, ?, ?, ?, NULL, 1)
           ON CONFLICT("Id") DO UPDATE SET
               "Name" = excluded."Name", "RootPath" = excluded."RootPath",
               "ManifestJson" = excluded."ManifestJson",
               "ContentSha256" = excluded."ContentSha256", "LastScan" = excluded."LastScan",
               "LastError" = NULL, "StateRevision" = "AddonSource"."StateRevision" + 1"#,
    )
    .bind(&manifest.id)
    .bind(&manifest.name)
    .bind(kind)
    .bind(root_path)
    .bind(manifest_json)
    .bind(content_sha256)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    sqlx::query(r#"DELETE FROM "AddonSourceOffer" WHERE "SourceId" = ?"#)
        .bind(&manifest.id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    let available_count = offers.len();
    for (addon, relative, digest) in offers {
        let addon_json = serde_json::to_string(&addon).map_err(|_| {
            AddonError::Validation("add-on manifest could not be stored".to_owned())
        })?;
        sqlx::query(
            r#"INSERT INTO "AddonSourceOffer" (
                   "SourceId", "AddonId", "Version", "ManifestJson", "RelativePath", "ContentSha256"
               ) VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&manifest.id)
        .bind(&addon.id)
        .bind(addon.version.to_string())
        .bind(addon_json)
        .bind(relative)
        .bind(digest)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    }
    transaction.commit().await.map_err(database_error)?;
    Ok(LocalRepositoryImport {
        id: manifest.id,
        name: manifest.name,
        available_count,
    })
}

async fn collect_local_offers(
    repository_root: &Path,
) -> Result<Vec<(AddonManifest, String, String)>, AddonError> {
    let mut entries = tokio::fs::read_dir(repository_root.join("addons"))
        .await
        .map_err(|_| AddonError::Validation("repository has no addons directory".to_owned()))?;
    let mut offers = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(package_io_error)? {
        let file_type = entry.file_type().await.map_err(package_io_error)?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let addon_directory = entry.path();
        let addon = validate_local_offer_manifest(&addon_directory, current_rid()?).await?;
        let folder_name = entry.file_name().to_string_lossy().into_owned();
        if folder_name != addon.id {
            return Err(AddonError::Validation(
                "add-on folder name must match its manifest identity".to_owned(),
            ));
        }
        let digest = hash_addon_tree(addon_directory).await?;
        offers.push((addon, format!("addons/{folder_name}"), digest));
    }
    if offers.is_empty() {
        return Err(AddonError::Validation(
            "repository contains no compatible add-ons".to_owned(),
        ));
    }
    offers.sort_by(|left, right| left.0.id.cmp(&right.0.id));
    Ok(offers)
}

async fn read_local_repository_manifest(
    root: &Path,
) -> Result<LocalRepositoryManifest, AddonError> {
    let path = root.join("repository.toml");
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AddonError::Validation("repository has no root repository.toml".to_owned()))?;
    if !metadata.is_file() || metadata.len() > MAX_REPOSITORY_FILE_BYTES as u64 {
        return Err(AddonError::Validation(
            "repository.toml is not a bounded regular file".to_owned(),
        ));
    }
    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|_| AddonError::Validation("repository.toml is not UTF-8".to_owned()))?;
    let manifest: LocalRepositoryManifest = toml::from_str(&text).map_err(|_| {
        AddonError::Validation("repository.toml does not match the local schema".to_owned())
    })?;
    manifest
        .validate()
        .map_err(|error| AddonError::Validation(error.to_string()))?;
    Ok(manifest)
}

async fn record_local_source_scan_error(pool: &DbPool, path: &Path, error: &AddonError) {
    let _ = sqlx::query(r#"UPDATE "AddonSource" SET "LastError" = ? WHERE "RootPath" = ?"#)
        .bind(bounded_text(&error.to_string(), 1_024))
        .bind(path.to_string_lossy().into_owned())
        .execute(pool)
        .await;
}

async fn reconcile_missing_checkout_sources(
    pool: &DbPool,
    discovered_roots: &BTreeSet<String>,
) -> Result<(), AddonError> {
    let rows =
        sqlx::query(r#"SELECT "Id", "RootPath" FROM "AddonSource" WHERE "Kind" = 'checkout'"#)
            .fetch_all(pool)
            .await
            .map_err(database_error)?;
    for row in rows {
        let source_id: String = row.try_get("Id").repository().map_err(repository_error)?;
        let root_path: String = row
            .try_get("RootPath")
            .repository()
            .map_err(repository_error)?;
        if discovered_roots.contains(&root_path) {
            continue;
        }
        let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
        sqlx::query(r#"DELETE FROM "AddonSourceOffer" WHERE "SourceId" = ?"#)
            .bind(&source_id)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        sqlx::query(
            r#"UPDATE "AddonSource"
               SET "LastError" = 'checkout directory is missing',
                   "StateRevision" = "StateRevision" + 1
               WHERE "Id" = ? AND "RootPath" = ?"#,
        )
        .bind(&source_id)
        .bind(&root_path)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        transaction.commit().await.map_err(database_error)?;
    }
    Ok(())
}

fn catalog_view_from_manifest(manifest: &AddonManifest, rid: &str, digest: &str) -> CatalogAddon {
    CatalogAddon {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        name: manifest.name.clone(),
        summary: manifest.summary.clone(),
        icon: None,
        license: manifest.license.clone(),
        source_url: manifest.source_url.clone(),
        host_version: manifest.host_version.clone(),
        capabilities: manifest
            .capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect(),
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        packages: BTreeMap::from([(
            rid.to_owned(),
            CatalogPackage {
                url: format!("https://local.invalid/{digest}.zip"),
                sha256: digest.to_owned(),
                size: 1,
            },
        )]),
    }
}

async fn archive_local_addon(root: PathBuf) -> Result<Vec<u8>, AddonError> {
    tokio::task::spawn_blocking(move || archive_local_addon_sync(&root))
        .await
        .map_err(|_| AddonError::Package("local package archive task failed".to_owned()))?
}

fn archive_local_addon_sync(root: &Path) -> Result<Vec<u8>, AddonError> {
    let root = dunce::canonicalize(root).map_err(package_io_error)?;
    let mut directories = vec![root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).map_err(package_io_error)? {
            let entry = entry.map_err(package_io_error)?;
            let kind = entry.file_type().map_err(package_io_error)?;
            if kind.is_symlink() {
                return Err(AddonError::Package(
                    "local add-on contains a symlink".to_owned(),
                ));
            }
            if kind.is_dir() {
                directories.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    if files.len() > MAX_ARCHIVE_ENTRIES {
        return Err(AddonError::Package(
            "local add-on contains too many files".to_owned(),
        ));
    }
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    let mut total = 0_u64;
    for path in files {
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| AddonError::Package("local add-on path escaped its root".to_owned()))?;
        let name = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let bytes = std::fs::read(&path).map_err(package_io_error)?;
        total = total.saturating_add(bytes.len() as u64);
        if total > MAX_EXTRACTED_BYTES {
            return Err(AddonError::Package(
                "local add-on exceeds its extracted size limit".to_owned(),
            ));
        }
        writer
            .start_file(name, options)
            .map_err(|error| AddonError::Package(error.to_string()))?;
        writer.write_all(&bytes).map_err(package_io_error)?;
    }
    let bytes = writer
        .finish()
        .map_err(|error| AddonError::Package(error.to_string()))?
        .into_inner();
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        return Err(AddonError::Package(
            "local add-on package exceeds its size limit".to_owned(),
        ));
    }
    Ok(bytes)
}

async fn persist_local_addon(
    pool: &DbPool,
    source_id: &str,
    source_revision: i64,
    previous: Option<&InstalledAddon>,
    staged: &StagedAddonPackage,
) -> Result<(), AddonError> {
    let now = Utc::now().to_rfc3339();
    let manifest_json = serde_json::to_string(&staged.manifest)
        .map_err(|_| AddonError::Validation("manifest could not be persisted".to_owned()))?;
    let install_path = staged.destination.to_string_lossy().into_owned();
    let state_revision = Uuid::new_v4().to_string();
    let data_namespace = previous.map_or(source_id, |item| item.data_namespace.as_str());
    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    lock_addon_lifecycle(&mut transaction).await?;
    let changed = if let Some(previous) = previous {
        sqlx::query(
            r#"UPDATE "AddonInstallation"
               SET "RepositoryId" = NULL, "SourceId" = ?, "TrustTier" = 'local',
                   "Version" = ?, "Enabled" = 0, "PackageSha256" = ?,
                   "ContentSha256" = ?, "ManifestJson" = ?, "InstallPath" = ?,
                   "DataNamespace" = ?, "UpdatedAt" = ?, "StateRevision" = ?,
                   "LastCheckCode" = NULL, "LastCheckMessage" = NULL,
                   "LastCheckStatus" = NULL, "LastCheckAt" = NULL, "LastCheckToolUpdateJson" = NULL
               WHERE "AddonId" = ? AND "StateRevision" = ?
                 AND EXISTS (SELECT 1 FROM "AddonSource" WHERE "Id" = ? AND "StateRevision" = ?)"#,
        )
        .bind(source_id)
        .bind(staged.manifest.version.to_string())
        .bind(&staged.package_sha256)
        .bind(&staged.content_sha256)
        .bind(&manifest_json)
        .bind(&install_path)
        .bind(data_namespace)
        .bind(&now)
        .bind(&state_revision)
        .bind(&staged.manifest.id)
        .bind(&previous.state_revision)
        .bind(source_id)
        .bind(source_revision)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
    } else {
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "RepositoryId", "SourceId", "TrustTier", "Version", "Enabled",
                   "PackageSha256", "ContentSha256", "ManifestJson", "InstallPath",
                   "DataNamespace", "InstalledAt", "UpdatedAt", "StateRevision"
               ) SELECT ?, NULL, ?, 'local', ?, 0, ?, ?, ?, ?, ?, ?, ?, ?
               WHERE EXISTS (SELECT 1 FROM "AddonSource" WHERE "Id" = ? AND "StateRevision" = ?)
                 AND NOT EXISTS (SELECT 1 FROM "AddonInstallation" WHERE "AddonId" = ?)"#,
        )
        .bind(&staged.manifest.id)
        .bind(source_id)
        .bind(staged.manifest.version.to_string())
        .bind(&staged.package_sha256)
        .bind(&staged.content_sha256)
        .bind(&manifest_json)
        .bind(&install_path)
        .bind(source_id)
        .bind(&now)
        .bind(&now)
        .bind(&state_revision)
        .bind(source_id)
        .bind(source_revision)
        .bind(&staged.manifest.id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
    };
    if changed.rows_affected() != 1 {
        return Err(AddonError::Trust(
            "local source or installed add-on changed during installation".to_owned(),
        ));
    }
    transaction.commit().await.map_err(database_error)
}

/// Install or update an add-on from locally supplied package bytes.
///
/// The package receives the permanent `sideloaded` trust tier. A first install
/// starts disabled; an update of an enabled sideload is re-enabled again when
/// its readiness check passes and it asks for no new permissions. A sideload can
/// only update another sideload of the same identity.
///
/// # Errors
///
/// Returns an error when unknown sources are disabled, the archive or manifest
/// is invalid, the platform is unsupported, persistence races, or the package
/// would replace a different source or downgrade an installed sideload.
pub async fn install_addon_from_package(
    pool: &DbPool,
    archive: Vec<u8>,
    runtime: &AddonRuntimeContext,
) -> Result<InstalledAddon, AddonError> {
    if !unverified_sources_enabled(pool).await? {
        return Err(AddonError::Trust(
            "enable unverified sources before installing a local add-on package".to_owned(),
        ));
    }
    if archive.is_empty() || archive.len() as u64 > MAX_PACKAGE_BYTES {
        return Err(AddonError::Validation(
            "package is empty or too large".to_owned(),
        ));
    }
    let package_sha256 = sha256_hex(&archive);
    let staged = stage_sideload_package(current_rid()?, archive, package_sha256).await?;
    let previous = load_installed_addon(pool, &staged.manifest.id).await?;
    if let Some(previous) = &previous {
        if previous.trust_tier != AddonTrustTier::Sideloaded {
            return Err(AddonError::Trust(
                "installed add-on belongs to a repository; uninstall it before sideloading"
                    .to_owned(),
            ));
        }
        if staged.manifest.version == previous.version {
            return Err(AddonError::Validation(
                "this add-on version is already installed".to_owned(),
            ));
        }
        if staged.manifest.version < previous.version {
            return Err(AddonError::Trust(
                "local package would downgrade the installed add-on".to_owned(),
            ));
        }
    }
    persist_sideloaded_addon(pool, previous.as_ref(), &staged).await?;
    restore_enabled_after_update(
        pool,
        &staged.manifest.id,
        previous.as_ref(),
        &staged.manifest,
        runtime,
    )
    .await;
    load_installed_addon(pool, &staged.manifest.id)
        .await?
        .ok_or_else(|| AddonError::Database("sideloaded package was not persisted".to_owned()))
}

/// Install a repository trust record from a bounded local ZIP package.
///
/// # Errors
///
/// Returns an error when repository administration is deployment-managed,
/// unknown sources are disabled, the archive or manifest is invalid, the
/// signing key is malformed, or the package targets the official repository.
pub async fn install_repository_from_package(
    pool: &DbPool,
    archive: Vec<u8>,
) -> Result<TrustedAddonRepository, AddonError> {
    if repositories_are_deployment_managed() {
        return Err(AddonError::Configuration(
            "repository trust is managed by the deployment file".to_owned(),
        ));
    }
    if !unverified_sources_enabled(pool).await? {
        return Err(AddonError::Trust(
            "enable unverified sources before installing a repository package".to_owned(),
        ));
    }
    if archive.is_empty() || archive.len() > MAX_REPOSITORY_PACKAGE_BYTES {
        return Err(AddonError::Validation(
            "repository package is empty or too large".to_owned(),
        ));
    }
    let staging_root =
        addon_packages_folder().join(format!(".repository-staging-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&staging_root)
        .await
        .map_err(package_io_error)?;
    let extraction_path = staging_root.clone();
    let extraction =
        tokio::task::spawn_blocking(move || extract_zip_safely(&archive, &extraction_path))
            .await
            .map_err(|_| AddonError::Package("repository extraction task failed".to_owned()))?;
    let result = async {
        extraction?;
        let manifest_path = staging_root.join("repository.toml");
        let metadata = tokio::fs::metadata(&manifest_path).await.map_err(|_| {
            AddonError::Validation("package has no root repository.toml".to_owned())
        })?;
        if !metadata.is_file() || metadata.len() > MAX_REPOSITORY_FILE_BYTES as u64 {
            return Err(AddonError::Validation(
                "repository.toml is not a bounded regular file".to_owned(),
            ));
        }
        let text = tokio::fs::read_to_string(&manifest_path)
            .await
            .map_err(|_| AddonError::Validation("repository.toml is not UTF-8".to_owned()))?;
        let manifest: RepositoryManifest = toml::from_str(&text).map_err(|_| {
            AddonError::Validation("repository.toml does not match its schema".to_owned())
        })?;
        manifest
            .validate()
            .map_err(|error| AddonError::Validation(error.to_string()))?;
        if load_repository_row(pool, &manifest.id)
            .await?
            .is_some_and(|repository| repository.is_official)
        {
            return Err(AddonError::Trust(
                "a repository package cannot replace the official repository".to_owned(),
            ));
        }
        let repository = TrustedAddonRepository {
            id: manifest.id,
            name: manifest.name,
            index_url: manifest.index_url,
            public_key: manifest.public_key.unwrap_or_default(),
            is_official: false,
            is_enabled: true,
            last_sequence: None,
            last_refresh: None,
            last_error: None,
        };
        save_trusted_repository(pool, &repository).await?;
        load_repository_row(pool, &repository.id)
            .await?
            .ok_or_else(|| AddonError::Database("repository package was not persisted".to_owned()))
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&staging_root).await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn install_catalog_package(
    pool: &DbPool,
    client: &reqwest::Client,
    repository_id: &str,
    trust_tier: AddonTrustTier,
    repository_catalog_revision: i64,
    previous_installation: Option<&InstalledAddon>,
    addon: &CatalogAddon,
    package: &CatalogPackage,
    runtime: &AddonRuntimeContext,
) -> Result<InstalledAddon, AddonError> {
    let rid = current_rid()?;
    let (archive, package_sha256) = download_catalog_package(client, package).await?;
    let staged = stage_catalog_package(addon, rid, archive, package_sha256).await?;
    persist_staged_addon(
        pool,
        repository_id,
        trust_tier,
        repository_catalog_revision,
        previous_installation,
        addon,
        &staged,
    )
    .await?;
    restore_enabled_after_update(
        pool,
        &addon.id,
        previous_installation,
        &staged.manifest,
        runtime,
    )
    .await;
    load_installed_addon(pool, &addon.id)
        .await?
        .ok_or_else(|| AddonError::Database("installed package was not persisted".to_owned()))
}

struct StagedAddonPackage {
    manifest: AddonManifest,
    package_sha256: String,
    content_sha256: String,
    destination: PathBuf,
}

async fn download_catalog_package(
    client: &reqwest::Client,
    package: &CatalogPackage,
) -> Result<(Vec<u8>, String), AddonError> {
    if package.size > MAX_PACKAGE_BYTES {
        return Err(AddonError::Validation("package is too large".to_owned()));
    }
    let max_download = usize::try_from(package.size)
        .map_err(|_| AddonError::Validation("package is too large".to_owned()))?;
    let archive = download_bounded(client, &package.url, max_download).await?;
    if u64::try_from(archive.len()).ok() != Some(package.size) {
        return Err(AddonError::Validation(
            "download size does not match signed catalog".to_owned(),
        ));
    }
    let actual_sha256 = sha256_hex(&archive);
    if !actual_sha256.eq_ignore_ascii_case(&package.sha256) {
        return Err(AddonError::Trust(
            "package SHA-256 does not match signed catalog".to_owned(),
        ));
    }
    Ok((archive, actual_sha256))
}

async fn stage_catalog_package(
    addon: &CatalogAddon,
    rid: &str,
    archive: Vec<u8>,
    package_sha256: String,
) -> Result<StagedAddonPackage, AddonError> {
    let package_root = addon_packages_folder();
    tokio::fs::create_dir_all(&package_root)
        .await
        .map_err(package_io_error)?;
    let staging_root = package_root.join(format!(".staging-{}", Uuid::new_v4()));
    let extracted_root = staging_root.join("content");
    tokio::fs::create_dir_all(&extracted_root)
        .await
        .map_err(package_io_error)?;
    let result = stage_catalog_package_inner(
        addon,
        rid,
        archive,
        package_sha256,
        &package_root,
        &extracted_root,
    )
    .await;
    let _ = tokio::fs::remove_dir_all(&staging_root).await;
    result
}

async fn stage_sideload_package(
    rid: &str,
    archive: Vec<u8>,
    package_sha256: String,
) -> Result<StagedAddonPackage, AddonError> {
    let package_root = addon_packages_folder();
    tokio::fs::create_dir_all(&package_root)
        .await
        .map_err(package_io_error)?;
    let staging_root = package_root.join(format!(".staging-{}", Uuid::new_v4()));
    let extracted_root = staging_root.join("content");
    tokio::fs::create_dir_all(&extracted_root)
        .await
        .map_err(package_io_error)?;
    let extraction_path = extracted_root.clone();
    let extraction =
        tokio::task::spawn_blocking(move || extract_zip_safely(&archive, &extraction_path))
            .await
            .map_err(|_| AddonError::Package("package extraction task failed".to_owned()))?;
    let result = async {
        extraction?;
        let manifest = validate_extracted_manifest(&extracted_root, rid).await?;
        let content_sha256 = hash_addon_tree(extracted_root.clone()).await?;
        let destination = package_root
            .join(&manifest.id)
            .join(manifest.version.to_string())
            .join(&package_sha256[..16])
            .join(rid);
        if destination.exists() {
            verify_addon_content_path(&destination, &content_sha256).await?;
        } else {
            let parent = destination.parent().ok_or_else(|| {
                AddonError::Package("could not resolve package destination".to_owned())
            })?;
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(package_io_error)?;
            tokio::fs::rename(&extracted_root, &destination)
                .await
                .map_err(package_io_error)?;
        }
        Ok(StagedAddonPackage {
            manifest,
            package_sha256,
            content_sha256,
            destination,
        })
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&staging_root).await;
    result
}

async fn stage_catalog_package_inner(
    addon: &CatalogAddon,
    rid: &str,
    archive: Vec<u8>,
    package_sha256: String,
    package_root: &Path,
    extracted_root: &Path,
) -> Result<StagedAddonPackage, AddonError> {
    let archive_for_extract = archive;
    let extraction_path = extracted_root.to_path_buf();
    let extraction = tokio::task::spawn_blocking(move || {
        extract_zip_safely(&archive_for_extract, &extraction_path)
    })
    .await
    .map_err(|_| AddonError::Package("package extraction task failed".to_owned()))?;
    extraction?;

    let manifest = validate_extracted_manifest(extracted_root, rid).await?;
    ensure_manifest_matches_catalog(&manifest, addon)?;
    let content_sha256 = hash_addon_tree(extracted_root.to_path_buf()).await?;

    let destination = package_root
        .join(&addon.id)
        .join(addon.version.to_string())
        .join(&package_sha256[..16])
        .join(rid);
    if destination.exists() {
        verify_addon_content_path(&destination, &content_sha256).await?;
        return Ok(StagedAddonPackage {
            manifest,
            package_sha256,
            content_sha256,
            destination,
        });
    }
    let parent = destination
        .parent()
        .ok_or_else(|| AddonError::Package("could not resolve package destination".to_owned()))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(package_io_error)?;
    tokio::fs::rename(extracted_root, &destination)
        .await
        .map_err(package_io_error)?;
    Ok(StagedAddonPackage {
        manifest,
        package_sha256,
        content_sha256,
        destination,
    })
}

async fn persist_staged_addon(
    pool: &DbPool,
    repository_id: &str,
    trust_tier: AddonTrustTier,
    repository_catalog_revision: i64,
    previous_installation: Option<&InstalledAddon>,
    addon: &CatalogAddon,
    staged: &StagedAddonPackage,
) -> Result<(), AddonError> {
    let now = Utc::now().to_rfc3339();
    let manifest_json = serde_json::to_string(&staged.manifest)
        .map_err(|_| AddonError::Validation("manifest could not be persisted".to_owned()))?;
    let install_path = staged.destination.to_string_lossy().into_owned();
    let state_revision = Uuid::new_v4().to_string();
    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    lock_addon_lifecycle(&mut transaction).await?;
    let installed = list_installed_addons_in_transaction(&mut transaction).await?;
    let enabled_dependents = dependent_addon_ids_from_installations(&installed, &addon.id, true);
    if !enabled_dependents.is_empty() {
        return Err(AddonError::Validation(format!(
            "disable dependent add-on(s) before updating {}: {}",
            addon.id,
            enabled_dependents.join(", ")
        )));
    }
    let changed = if let Some(previous) = previous_installation {
        sqlx::query(
            r#"UPDATE "AddonInstallation"
               SET "RepositoryId" = ?, "TrustTier" = ?, "Version" = ?, "Enabled" = 0,
                   "PackageSha256" = ?, "ContentSha256" = ?, "ManifestJson" = ?,
                   "InstallPath" = ?, "DataNamespace" = ?, "UpdatedAt" = ?, "StateRevision" = ?,
                   "LastCheckCode" = NULL, "LastCheckMessage" = NULL,
                   "LastCheckStatus" = NULL, "LastCheckAt" = NULL, "LastCheckToolUpdateJson" = NULL
               WHERE "AddonId" = ? AND "RepositoryId" = ? AND "Version" = ?
                 AND "StateRevision" = ?
                 AND EXISTS (
                     SELECT 1 FROM "AddonRepository"
                     WHERE "Id" = ? AND "CatalogRevision" = ? AND "IsEnabled" = 1
                 )"#,
        )
        .bind(repository_id)
        .bind(trust_tier.as_str())
        .bind(addon.version.to_string())
        .bind(&staged.package_sha256)
        .bind(&staged.content_sha256)
        .bind(&manifest_json)
        .bind(&install_path)
        .bind(repository_id)
        .bind(&now)
        .bind(&state_revision)
        .bind(&addon.id)
        .bind(repository_id)
        .bind(previous.version.to_string())
        .bind(&previous.state_revision)
        .bind(repository_id)
        .bind(repository_catalog_revision)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
    } else {
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "RepositoryId", "TrustTier", "Version", "Enabled", "PackageSha256",
                   "ContentSha256", "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt",
                   "UpdatedAt", "StateRevision"
               )
               SELECT ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?
               WHERE EXISTS (
                   SELECT 1 FROM "AddonRepository"
                   WHERE "Id" = ? AND "CatalogRevision" = ? AND "IsEnabled" = 1
               ) AND NOT EXISTS (
                   SELECT 1 FROM "AddonInstallation" WHERE "AddonId" = ?
               )"#,
        )
        .bind(&addon.id)
        .bind(repository_id)
        .bind(trust_tier.as_str())
        .bind(addon.version.to_string())
        .bind(&staged.package_sha256)
        .bind(&staged.content_sha256)
        .bind(&manifest_json)
        .bind(&install_path)
        .bind(repository_id)
        .bind(&now)
        .bind(&now)
        .bind(&state_revision)
        .bind(repository_id)
        .bind(repository_catalog_revision)
        .bind(&addon.id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
    };
    if changed.rows_affected() != 1 {
        return Err(AddonError::Trust(
            "repository or installed add-on changed while installation was in progress".to_owned(),
        ));
    }
    transaction.commit().await.map_err(database_error)?;
    Ok(())
}

async fn persist_sideloaded_addon(
    pool: &DbPool,
    previous_installation: Option<&InstalledAddon>,
    staged: &StagedAddonPackage,
) -> Result<(), AddonError> {
    let now = Utc::now().to_rfc3339();
    let manifest_json = serde_json::to_string(&staged.manifest)
        .map_err(|_| AddonError::Validation("manifest could not be persisted".to_owned()))?;
    let install_path = staged.destination.to_string_lossy().into_owned();
    let state_revision = Uuid::new_v4().to_string();
    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    lock_addon_lifecycle(&mut transaction).await?;
    let installed = list_installed_addons_in_transaction(&mut transaction).await?;
    let enabled_dependents =
        dependent_addon_ids_from_installations(&installed, &staged.manifest.id, true);
    if !enabled_dependents.is_empty() {
        return Err(AddonError::Validation(format!(
            "disable dependent add-on(s) before updating {}: {}",
            staged.manifest.id,
            enabled_dependents.join(", ")
        )));
    }
    let changed = if let Some(previous) = previous_installation {
        sqlx::query(
            r#"UPDATE "AddonInstallation"
               SET "RepositoryId" = NULL, "TrustTier" = 'sideloaded', "Version" = ?,
                   "Enabled" = 0, "PackageSha256" = ?, "ContentSha256" = ?,
                   "ManifestJson" = ?, "InstallPath" = ?, "DataNamespace" = 'sideload',
                   "UpdatedAt" = ?, "StateRevision" = ?, "LastCheckCode" = NULL,
                   "LastCheckMessage" = NULL, "LastCheckStatus" = NULL, "LastCheckAt" = NULL, "LastCheckToolUpdateJson" = NULL
               WHERE "AddonId" = ? AND "TrustTier" = 'sideloaded' AND "Version" = ?
                 AND "StateRevision" = ?"#,
        )
        .bind(staged.manifest.version.to_string())
        .bind(&staged.package_sha256)
        .bind(&staged.content_sha256)
        .bind(&manifest_json)
        .bind(&install_path)
        .bind(&now)
        .bind(&state_revision)
        .bind(&staged.manifest.id)
        .bind(previous.version.to_string())
        .bind(&previous.state_revision)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
    } else {
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "RepositoryId", "TrustTier", "Version", "Enabled",
                   "PackageSha256", "ContentSha256", "ManifestJson", "InstallPath",
                   "DataNamespace", "InstalledAt", "UpdatedAt", "StateRevision"
               ) SELECT ?, NULL, 'sideloaded', ?, 0, ?, ?, ?, ?, 'sideload', ?, ?, ?
               WHERE NOT EXISTS (
                   SELECT 1 FROM "AddonInstallation" WHERE "AddonId" = ?
               )"#,
        )
        .bind(&staged.manifest.id)
        .bind(staged.manifest.version.to_string())
        .bind(&staged.package_sha256)
        .bind(&staged.content_sha256)
        .bind(&manifest_json)
        .bind(&install_path)
        .bind(&now)
        .bind(&now)
        .bind(&state_revision)
        .bind(&staged.manifest.id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?
    };
    if changed.rows_affected() != 1 {
        return Err(AddonError::Trust(
            "installed add-on changed while sideloading".to_owned(),
        ));
    }
    transaction.commit().await.map_err(database_error)?;
    Ok(())
}

async fn validate_extracted_manifest(
    extracted_root: &Path,
    rid: &str,
) -> Result<AddonManifest, AddonError> {
    let manifest = read_extracted_manifest(extracted_root).await?;
    let entrypoint = manifest.entrypoint_for(rid).ok_or_else(|| {
        AddonError::Validation("manifest has no entrypoint for this platform".to_owned())
    })?;
    let entrypoint_path = extracted_root.join(&entrypoint.path);
    if !entrypoint_path.is_file() {
        return Err(AddonError::Validation(
            "manifest entrypoint is missing from package".to_owned(),
        ));
    }
    Ok(manifest)
}

async fn validate_local_offer_manifest(
    extracted_root: &Path,
    rid: &str,
) -> Result<AddonManifest, AddonError> {
    let manifest = read_extracted_manifest(extracted_root).await?;
    if let Some(entrypoint) = manifest.entrypoint_for(rid) {
        if !extracted_root.join(&entrypoint.path).is_file() {
            return Err(AddonError::Validation(
                "manifest entrypoint is missing from local repository".to_owned(),
            ));
        }
    }
    Ok(manifest)
}

async fn read_extracted_manifest(extracted_root: &Path) -> Result<AddonManifest, AddonError> {
    let manifest_path = extracted_root.join("addon.toml");
    let manifest_metadata = tokio::fs::metadata(&manifest_path)
        .await
        .map_err(|_| AddonError::Validation("package has no root addon.toml".to_owned()))?;
    if !manifest_metadata.is_file() || manifest_metadata.len() > MAX_MANIFEST_BYTES {
        return Err(AddonError::Validation(
            "package addon.toml is not a bounded regular file".to_owned(),
        ));
    }
    let manifest_text = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|_| AddonError::Validation("package has no root addon.toml".to_owned()))?;
    let manifest: AddonManifest = toml::from_str(&manifest_text).map_err(|_| {
        AddonError::Validation("addon.toml does not match a supported schema".to_owned())
    })?;
    manifest
        .validate()
        .map_err(|error| AddonError::Validation(error.to_string()))?;
    if let Some(icon) = &manifest.icon {
        let icon_path = extracted_root.join(&icon.path);
        let metadata = tokio::fs::metadata(&icon_path).await.map_err(|_| {
            AddonError::Validation("manifest icon is missing from package".to_owned())
        })?;
        if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
            return Err(AddonError::Validation(
                "manifest icon is not a bounded regular file".to_owned(),
            ));
        }
        let bytes = tokio::fs::read(&icon_path)
            .await
            .map_err(|_| AddonError::Validation("manifest icon is unreadable".to_owned()))?;
        if !valid_icon_bytes(&bytes, icon.media_type) {
            return Err(AddonError::Validation(
                "manifest icon bytes do not match the declared media type".to_owned(),
            ));
        }
    }
    Ok(manifest)
}

fn valid_icon_bytes(bytes: &[u8], media_type: AddonIconMediaType) -> bool {
    match media_type {
        AddonIconMediaType::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        AddonIconMediaType::Webp => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
    }
}

fn ensure_manifest_matches_catalog(
    manifest: &AddonManifest,
    addon: &CatalogAddon,
) -> Result<(), AddonError> {
    if manifest.id != addon.id || manifest.version != addon.version {
        return Err(AddonError::Trust(
            "package manifest identity differs from catalog".to_owned(),
        ));
    }
    let manifest_capabilities = manifest
        .capabilities
        .iter()
        .map(|capability| capability.id.as_str())
        .collect::<BTreeSet<_>>();
    let catalog_capabilities = addon
        .capabilities
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if manifest.name != addon.name
        || manifest.summary != addon.summary
        || manifest.license != addon.license
        || manifest.source_url != addon.source_url
        || manifest.host_version != addon.host_version
        || manifest.dependencies != addon.dependencies
        || manifest.permissions != addon.permissions
        || manifest_capabilities != catalog_capabilities
    {
        return Err(AddonError::Trust(
            "package manifest metadata differs from signed catalog".to_owned(),
        ));
    }
    Ok(())
}

/// List active package selections, including disabled packages.
///
/// # Errors
///
/// Returns an error when a row or its stored manifest cannot be decoded.
pub async fn list_installed_addons(pool: &DbPool) -> Result<Vec<InstalledAddon>, AddonError> {
    let rows = sqlx::query(
        r#"SELECT "AddonId", "RepositoryId", "SourceId", "TrustTier", "Version", "Enabled", "PackageSha256", "ContentSha256",
                  "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt",
                  "StateRevision", "LastCheckCode", "LastCheckMessage", "LastCheckAt", "LastCheckToolUpdateJson"
           FROM "AddonInstallation" ORDER BY "AddonId""#,
    )
    .fetch_all(pool)
    .await
    .map_err(database_error)?;
    rows.iter().map(installed_from_row).collect()
}

/// Load one installed package.
///
/// # Errors
///
/// Returns an error when persistence or the stored manifest is invalid.
pub async fn load_installed_addon(
    pool: &DbPool,
    addon_id: &str,
) -> Result<Option<InstalledAddon>, AddonError> {
    let row = sqlx::query(
        r#"SELECT "AddonId", "RepositoryId", "SourceId", "TrustTier", "Version", "Enabled", "PackageSha256", "ContentSha256",
                  "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt",
                  "StateRevision", "LastCheckCode", "LastCheckMessage", "LastCheckAt", "LastCheckToolUpdateJson"
           FROM "AddonInstallation" WHERE "AddonId" = ?"#,
    )
    .bind(addon_id)
    .fetch_optional(pool)
    .await
    .map_err(database_error)?;
    row.as_ref().map(installed_from_row).transpose()
}

async fn lock_addon_lifecycle(
    transaction: &mut Transaction<'_, crate::db::Db>,
) -> Result<(), AddonError> {
    let locked = sqlx::query(
        r#"UPDATE "AddonLifecycleLock" SET "Revision" = "Revision" + 1 WHERE "Id" = 1"#,
    )
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    if locked.rows_affected() != 1 {
        return Err(AddonError::Database(
            "add-on lifecycle lock is unavailable".to_owned(),
        ));
    }
    Ok(())
}

async fn load_installed_addon_in_transaction(
    transaction: &mut Transaction<'_, crate::db::Db>,
    addon_id: &str,
) -> Result<Option<InstalledAddon>, AddonError> {
    let row = sqlx::query(
        r#"SELECT "AddonId", "RepositoryId", "SourceId", "TrustTier", "Version", "Enabled", "PackageSha256", "ContentSha256",
                  "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt",
                  "StateRevision", "LastCheckCode", "LastCheckMessage", "LastCheckAt", "LastCheckToolUpdateJson"
           FROM "AddonInstallation" WHERE "AddonId" = ?"#,
    )
    .bind(addon_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?;
    row.as_ref().map(installed_from_row).transpose()
}

async fn list_installed_addons_in_transaction(
    transaction: &mut Transaction<'_, crate::db::Db>,
) -> Result<Vec<InstalledAddon>, AddonError> {
    let rows = sqlx::query(
        r#"SELECT "AddonId", "RepositoryId", "SourceId", "TrustTier", "Version", "Enabled", "PackageSha256", "ContentSha256",
                  "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt",
                  "StateRevision", "LastCheckCode", "LastCheckMessage", "LastCheckAt", "LastCheckToolUpdateJson"
           FROM "AddonInstallation" ORDER BY "AddonId""#,
    )
    .fetch_all(&mut **transaction)
    .await
    .map_err(database_error)?;
    rows.iter().map(installed_from_row).collect()
}

/// Persist one manifest-declared setting. Secret settings accept only a named
/// environment variable or file reference.
///
/// # Errors
///
/// Returns an error for unknown fields, raw secret values, or invalid values.
pub async fn save_addon_setting(
    pool: &DbPool,
    addon_id: &str,
    setting: &AddonSettingValue,
) -> Result<(), AddonError> {
    let installation = load_installed_addon(pool, addon_id)
        .await?
        .ok_or_else(|| AddonError::NotFound("add-on is not installed".to_owned()))?;
    let definition = installation
        .manifest
        .settings
        .iter()
        .find(|item| item.key == setting.key)
        .ok_or_else(|| AddonError::Validation("setting is not declared by manifest".to_owned()))?;
    let clear_setting = should_clear_optional_setting(&installation.manifest, definition, setting);
    if !clear_setting {
        validate_setting_value(definition, setting)?;
        validate_storage_path_override(&installation.manifest, setting)?;
    }

    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    lock_addon_lifecycle(&mut transaction).await?;
    let installed = list_installed_addons_in_transaction(&mut transaction).await?;
    let enabled_dependents = dependent_addon_ids_from_installations(&installed, addon_id, true);
    if !enabled_dependents.is_empty() {
        return Err(AddonError::Validation(format!(
            "disable dependent add-on(s) before changing {} settings: {}",
            addon_id,
            enabled_dependents.join(", ")
        )));
    }
    if clear_setting {
        sqlx::query(r#"DELETE FROM "AddonSetting" WHERE "AddonId" = ? AND "Key" = ?"#)
            .bind(addon_id)
            .bind(&setting.key)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
    } else {
        let updated = sqlx::query(
            r#"UPDATE "AddonSetting" SET "Value" = ?, "ReferenceKind" = ?, "Reference" = ?
               WHERE "AddonId" = ? AND "Key" = ?"#,
        )
        .bind(&setting.value)
        .bind(setting.reference_kind.map(AddonSecretReferenceKind::as_str))
        .bind(&setting.reference)
        .bind(addon_id)
        .bind(&setting.key)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
        if updated.rows_affected() == 0 {
            sqlx::query(
                r#"INSERT INTO "AddonSetting" (
                       "AddonId", "Key", "Value", "ReferenceKind", "Reference"
                   ) VALUES (?, ?, ?, ?, ?)"#,
            )
            .bind(addon_id)
            .bind(&setting.key)
            .bind(&setting.value)
            .bind(setting.reference_kind.map(AddonSecretReferenceKind::as_str))
            .bind(&setting.reference)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        }
    }
    let now = Utc::now().to_rfc3339();
    let next_revision = Uuid::new_v4().to_string();
    let changed = sqlx::query(
        r#"UPDATE "AddonInstallation"
           SET "Enabled" = 0, "UpdatedAt" = ?, "StateRevision" = ?,
               "LastCheckCode" = NULL, "LastCheckMessage" = NULL,
                   "LastCheckStatus" = NULL, "LastCheckAt" = NULL, "LastCheckToolUpdateJson" = NULL
           WHERE "AddonId" = ? AND "StateRevision" = ?"#,
    )
    .bind(now)
    .bind(next_revision)
    .bind(addon_id)
    .bind(&installation.state_revision)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    if changed.rows_affected() != 1 {
        return Err(AddonError::Trust(
            "installed add-on changed while its setting was being saved".to_owned(),
        ));
    }
    transaction.commit().await.map_err(database_error)?;
    Ok(())
}

/// Load persisted settings without resolving referenced secrets.
///
/// # Errors
///
/// Returns an error when persistence contains an invalid reference kind.
pub async fn load_addon_settings(
    pool: &DbPool,
    addon_id: &str,
) -> Result<Vec<AddonSettingValue>, AddonError> {
    let rows = sqlx::query(
        r#"SELECT "Key", "Value", "ReferenceKind", "Reference"
           FROM "AddonSetting" WHERE "AddonId" = ? ORDER BY "Key""#,
    )
    .bind(addon_id)
    .fetch_all(pool)
    .await
    .map_err(database_error)?;
    rows.iter().map(setting_from_row).collect()
}

/// Run `addon.check.v1` and persist only redacted, bounded status text.
///
/// # Errors
///
/// Returns an error when the package cannot be resolved, started, or decoded.
pub async fn check_addon(
    pool: &DbPool,
    addon_id: &str,
    runtime: &AddonRuntimeContext,
) -> Result<AddonCheckResult, AddonError> {
    check_addon_with_revision(pool, addon_id, runtime)
        .await
        .map(|(result, _)| result)
}

async fn check_addon_with_revision(
    pool: &DbPool,
    addon_id: &str,
    runtime: &AddonRuntimeContext,
) -> Result<(AddonCheckResult, String), AddonError> {
    let command = resolve_addon_command(
        pool,
        addon_id,
        CAPABILITY_ADDON_CHECK_V1,
        "check",
        runtime,
        false,
    )
    .await?;
    let stdout = run_bounded_command(&command, CHECK_TIMEOUT, MAX_CHECK_OUTPUT_BYTES).await?;
    let mut result: AddonCheckResult = serde_json::from_slice(&stdout)
        .map_err(|_| AddonError::Process("check returned invalid JSON".to_owned()))?;
    result.code = sanitize_addon_diagnostic_code(&result.code);
    if result.code == "invalid-operation-code" {
        "invalid-check-code".clone_into(&mut result.code);
    }
    result.message = redact_and_bound(&result.message, &command.environment, 1_024);
    result.tool_update = result.tool_update.filter(|context| {
        result.status == AddonCheckStatus::Warning
            && result.code == "tool-update-failed-stale"
            && check_tool_update_is_safe(context, &command.environment)
    });
    let tool_update_json = result
        .tool_update
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| AddonError::Process("invalid tool update context".to_owned()))?;
    sqlx::query(
        r#"UPDATE "AddonInstallation"
           SET "LastCheckCode" = ?, "LastCheckMessage" = ?, "LastCheckStatus" = ?,
               "LastCheckAt" = ?, "LastCheckToolUpdateJson" = ?
           WHERE "AddonId" = ? AND "StateRevision" = ?"#,
    )
    .bind(&result.code)
    .bind(&result.message)
    .bind(result.status.as_str())
    .bind(Utc::now().to_rfc3339())
    .bind(tool_update_json)
    .bind(addon_id)
    .bind(&command.state_revision)
    .execute(pool)
    .await
    .map_err(database_error)
    .and_then(|changed| {
        if changed.rows_affected() == 1 {
            Ok(())
        } else {
            Err(AddonError::Trust(
                "installed add-on changed while its readiness check was running".to_owned(),
            ))
        }
    })?;
    Ok((result, command.state_revision))
}

/// Put an add-on that was running before an update back into service.
///
/// A package update always lands disabled with its readiness result cleared,
/// because the code behind the add-on has been replaced. Leaving it there is
/// what an operator experiences as "updating breaks my add-on": a working
/// installation silently stops after a routine update. So the update finishes
/// the lifecycle itself and runs the same validation `enable_addon` runs — the
/// fresh readiness check ADR 0066 asks for — instead of waiting for a click.
///
/// The one thing it will not decide on the operator's behalf is a privilege
/// change. Permissions are shown at install time and accepted by enabling, so a
/// version that wants to reach new hosts, read media it could not read before,
/// or run new external commands stays disabled until someone reviews it.
///
/// A first install is untouched: it has nothing to restore and must be
/// configured before it can pass a check.
async fn restore_enabled_after_update(
    pool: &DbPool,
    addon_id: &str,
    previous_installation: Option<&InstalledAddon>,
    manifest: &AddonManifest,
    runtime: &AddonRuntimeContext,
) {
    let Some(previous) = previous_installation.filter(|previous| previous.enabled) else {
        return;
    };
    if let Some(added) = added_permissions(&previous.manifest, manifest) {
        tracing::info!(
            addon_id,
            added_permissions = %added,
            "updated add-on stays disabled until its new permissions are reviewed"
        );
        return;
    }
    match enable_addon(pool, addon_id, runtime).await {
        Ok(result) => tracing::info!(
            addon_id,
            code = %result.code,
            "re-enabled add-on after update"
        ),
        Err(error) => tracing::warn!(
            addon_id,
            %error,
            "updated add-on stays disabled because it is not ready"
        ),
    }
}

/// Describe the privileges the new version asks for on top of the old one.
///
/// Returns `None` when the update stays within what the running version already
/// declared. Dropped permissions are not a review reason.
fn added_permissions(previous: &AddonManifest, next: &AddonManifest) -> Option<String> {
    let previous_hosts = previous
        .permissions
        .network_hosts
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let previous_commands = previous
        .permissions
        .external_commands
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut added = Vec::new();
    added.extend(
        next.permissions
            .network_hosts
            .iter()
            .filter(|host| !previous_hosts.contains(host.as_str()))
            .map(|host| format!("network host {host}")),
    );
    added.extend(
        next.permissions
            .external_commands
            .iter()
            .filter(|command| !previous_commands.contains(command.as_str()))
            .map(|command| format!("external command {command}")),
    );
    if next.permissions.media_read && !previous.permissions.media_read {
        added.push("media read access".to_owned());
    }
    if next.permissions.managed_ffmpeg && !previous.permissions.managed_ffmpeg {
        added.push("managed FFmpeg access".to_owned());
    }
    added.extend(
        next.permissions
            .tool_update
            .iter()
            .filter(|key| !previous.permissions.tool_update.contains(key))
            .map(|key| format!("self-update executable {key}")),
    );
    (!added.is_empty()).then(|| added.join(", "))
}

/// Enable only after required configuration and `addon.check.v1` succeed.
/// Warnings remain operator-visible but do not block enabling.
///
/// # Errors
///
/// Returns an error when validation/check fails or persistence is unavailable.
pub async fn enable_addon(
    pool: &DbPool,
    addon_id: &str,
    runtime: &AddonRuntimeContext,
) -> Result<AddonCheckResult, AddonError> {
    let (result, checked_revision) = check_addon_with_revision(pool, addon_id, runtime).await?;
    if result.status == AddonCheckStatus::Unavailable {
        return Err(AddonError::Process(format!(
            "readiness check reported unavailable ({})",
            result.code
        )));
    }
    let settings = load_addon_settings(pool, addon_id).await?;
    let storage_root = prepare_storage_root(
        &load_installed_addon(pool, addon_id)
            .await?
            .ok_or_else(|| AddonError::NotFound("add-on is not installed".to_owned()))?
            .manifest,
        &settings,
    )
    .await?;
    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    lock_addon_lifecycle(&mut transaction).await?;
    let installation = load_installed_addon_in_transaction(&mut transaction, addon_id)
        .await?
        .ok_or_else(|| AddonError::NotFound("add-on is not installed".to_owned()))?;
    if installation.state_revision != checked_revision {
        return Err(AddonError::Trust(
            "installed add-on changed after its readiness check".to_owned(),
        ));
    }
    let installed = list_installed_addons_in_transaction(&mut transaction).await?;
    validate_dependencies_from_installations(&installed, &installation.manifest)?;
    if let (Some(storage), Some(root)) = (
        installation.manifest.media_list_storage.as_ref(),
        storage_root.as_deref(),
    ) {
        ensure_media_source_in_transaction(&mut transaction, addon_id, storage, root).await?;
    }
    let next_revision = Uuid::new_v4().to_string();
    let changed = sqlx::query(
        r#"UPDATE "AddonInstallation"
           SET "Enabled" = 1, "StateRevision" = ?
           WHERE "AddonId" = ? AND "StateRevision" = ?"#,
    )
    .bind(next_revision)
    .bind(addon_id)
    .bind(&checked_revision)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    if changed.rows_affected() != 1 {
        return Err(AddonError::Trust(
            "installed add-on changed while it was being enabled".to_owned(),
        ));
    }
    transaction.commit().await.map_err(database_error)?;
    Ok(result)
}

/// Disable an installed add-on. Existing Remote Streams remain linked and
/// become unavailable until it is enabled again.
///
/// # Errors
///
/// Returns an error when persistence fails.
pub async fn disable_addon(pool: &DbPool, addon_id: &str) -> Result<(), AddonError> {
    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    lock_addon_lifecycle(&mut transaction).await?;
    let installation = load_installed_addon_in_transaction(&mut transaction, addon_id)
        .await?
        .ok_or_else(|| AddonError::NotFound("add-on is not installed".to_owned()))?;
    let installed = list_installed_addons_in_transaction(&mut transaction).await?;
    let dependents = dependent_addon_ids_from_installations(&installed, addon_id, true);
    if !dependents.is_empty() {
        return Err(AddonError::Validation(format!(
            "add-on is required by enabled add-on(s): {}",
            dependents.join(", ")
        )));
    }
    let next_revision = Uuid::new_v4().to_string();
    let changed = sqlx::query(
        r#"UPDATE "AddonInstallation"
           SET "Enabled" = 0, "StateRevision" = ?,
               "LastCheckCode" = NULL, "LastCheckMessage" = NULL,
                   "LastCheckStatus" = NULL, "LastCheckAt" = NULL, "LastCheckToolUpdateJson" = NULL
           WHERE "AddonId" = ? AND "StateRevision" = ?"#,
    )
    .bind(next_revision)
    .bind(addon_id)
    .bind(&installation.state_revision)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    if changed.rows_affected() != 1 {
        return Err(AddonError::Trust(
            "installed add-on changed while it was being disabled".to_owned(),
        ));
    }
    transaction.commit().await.map_err(database_error)?;
    Ok(())
}

/// Uninstall a package. Linked Remote Streams and provider media lists block
/// removal unless `force` is explicit; persistent private add-on data is
/// retained for recovery/reinstallation.
///
/// # Errors
///
/// Returns an error for active references, unsafe paths, persistence, or I/O.
pub async fn uninstall_addon(pool: &DbPool, addon_id: &str, force: bool) -> Result<(), AddonError> {
    let installation = load_installed_addon(pool, addon_id)
        .await?
        .ok_or_else(|| AddonError::NotFound("add-on is not installed".to_owned()))?;
    let install_path = validated_install_path_for_removal(&installation.install_path).await?;
    let cache_path = addon_cache_folder(&installation.data_namespace, addon_id);
    let mut transaction = crate::db::begin_write(pool).await.map_err(database_error)?;
    lock_addon_lifecycle(&mut transaction).await?;
    let current = load_installed_addon_in_transaction(&mut transaction, addon_id)
        .await?
        .ok_or_else(|| AddonError::NotFound("add-on is not installed".to_owned()))?;
    if current.state_revision != installation.state_revision {
        return Err(AddonError::Trust(
            "installed add-on changed while uninstall was starting".to_owned(),
        ));
    }
    let remote_stream_references =
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "RemoteStream" WHERE "AddonId" = ?"#)
            .bind(addon_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(database_error)?;
    let media_list_references = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM "AddonMediaList" WHERE "AddonId" = ?"#,
    )
    .bind(addon_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(database_error)?;
    if (remote_stream_references > 0 || media_list_references > 0) && !force {
        return Err(AddonError::Validation(format!(
            "add-on is referenced by {remote_stream_references} Remote Stream item(s) and \
             {media_list_references} add-on media list(s)"
        )));
    }
    let installed = list_installed_addons_in_transaction(&mut transaction).await?;
    let enabled_dependents = dependent_addon_ids_from_installations(&installed, addon_id, true);
    if !enabled_dependents.is_empty() {
        return Err(AddonError::Validation(format!(
            "add-on is required by enabled add-on(s): {}",
            enabled_dependents.join(", ")
        )));
    }
    let dependents = dependent_addon_ids_from_installations(&installed, addon_id, false);
    if !dependents.is_empty() && !force {
        return Err(AddonError::Validation(format!(
            "add-on is required by installed add-on(s): {}",
            dependents.join(", ")
        )));
    }
    if force && remote_stream_references > 0 {
        sqlx::query(r#"UPDATE "RemoteStream" SET "AddonId" = NULL WHERE "AddonId" = ?"#)
            .bind(addon_id)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
    }
    if force && media_list_references > 0 {
        remove_owned_media_lists(&mut transaction, addon_id).await?;
    }
    sqlx::query(
        r#"DELETE FROM "MediaSource"
           WHERE "Id" IN (
               SELECT "MediaSourceId" FROM "AddonMediaSource" WHERE "AddonId" = ?
           )"#,
    )
    .bind(addon_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    let deleted = sqlx::query(
        r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = ? AND "StateRevision" = ?"#,
    )
    .bind(addon_id)
    .bind(&installation.state_revision)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    if deleted.rows_affected() != 1 {
        return Err(AddonError::Trust(
            "installed add-on changed while it was being uninstalled".to_owned(),
        ));
    }
    transaction.commit().await.map_err(database_error)?;

    remove_directory_if_present(&install_path).await?;
    remove_directory_if_present(&cache_path).await
}

async fn remove_owned_media_lists(
    transaction: &mut Transaction<'_, crate::db::Db>,
    addon_id: &str,
) -> Result<(), AddonError> {
    let active_finalization = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1
               FROM "AddonMediaListImportJob" AS "job"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "job"."AddonMediaListId"
               WHERE "list"."AddonId" = ? AND "job"."OverviewComplete" = 1
                 AND "job"."LeaseOwner" IS NOT NULL
           )"#,
    )
    .bind(addon_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(database_error)?;
    if active_finalization {
        return Err(AddonError::Validation(
            "an owned add-on media list import is finalizing; wait for it to finish".to_owned(),
        ));
    }
    let list_ids = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "Id" FROM "AddonMediaList" WHERE "AddonId" = ?"#,
    )
    .bind(addon_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(database_error)?;
    for list_id in list_ids {
        crate::addon_media_list_sync::unlink_revision_artifacts_for_list_in_tx(
            transaction,
            list_id,
        )
        .await
        .map_err(database_error)?;
    }
    sqlx::query(
        r#"DELETE FROM "Playlist"
           WHERE "IsSystem" = 1
             AND "Id" IN (
                 SELECT "PlaylistId" FROM "AddonMediaList"
                 WHERE "AddonId" = ? AND "PlaylistId" IS NOT NULL
             )"#,
    )
    .bind(addon_id)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    sqlx::query(r#"DELETE FROM "AddonMediaList" WHERE "AddonId" = ?"#)
        .bind(addon_id)
        .execute(&mut **transaction)
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn remove_directory_if_present(path: &Path) -> Result<(), AddonError> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(package_io_error(error)),
    }
}

/// Resolve an enabled add-on operation to a host-controlled command.
///
/// # Errors
///
/// Returns an error for disabled/incompatible packages or invalid settings.
pub async fn resolve_enabled_addon_command(
    pool: &DbPool,
    addon_id: &str,
    capability: &str,
    operation: &str,
    runtime: &AddonRuntimeContext,
) -> Result<ResolvedAddonCommand, AddonError> {
    resolve_addon_command(pool, addon_id, capability, operation, runtime, true).await
}

async fn resolve_addon_command(
    pool: &DbPool,
    addon_id: &str,
    capability: &str,
    operation: &str,
    runtime: &AddonRuntimeContext,
    require_enabled: bool,
) -> Result<ResolvedAddonCommand, AddonError> {
    validate_supported_operation(capability, operation)?;
    let installation = load_installed_addon(pool, addon_id)
        .await?
        .ok_or_else(|| AddonError::NotFound("add-on is not installed".to_owned()))?;
    if require_enabled && !installation.enabled {
        return Err(AddonError::Validation("add-on is disabled".to_owned()));
    }
    if !installation.manifest.has_capability(capability) {
        return Err(AddonError::Validation(format!(
            "add-on does not implement {capability}"
        )));
    }
    if !installation.manifest.host_version.matches(&host_version()?) {
        return Err(AddonError::Validation(
            "installed add-on is incompatible with this ErsatzRS version".to_owned(),
        ));
    }
    validate_installed_dependencies(pool, &installation.manifest).await?;
    let rid = current_rid()?;
    let entrypoint = installation.manifest.entrypoint_for(rid).ok_or_else(|| {
        AddonError::Validation("installed add-on has no entrypoint for this platform".to_owned())
    })?;
    let install_root = verify_installed_addon_contents(&installation).await?;
    let entrypoint_path = installation.install_path.join(&entrypoint.path);
    // The command keeps the `dunce` form, which `cmd.exe` can resolve for batch
    // entrypoints; only the containment check compares raw canonical forms.
    let executable = canonicalize_addon_path(&entrypoint_path)
        .await
        .map_err(package_io_error)?;
    let contained_executable = containment_path_async(&entrypoint_path)
        .await
        .map_err(package_io_error)?;
    let contained_root = containment_path_async(&install_root)
        .await
        .map_err(package_io_error)?;
    if !contained_executable.starts_with(&contained_root) || !contained_executable.is_file() {
        tracing::warn!(
            addon_id,
            entrypoint = %contained_executable.display(),
            root = %contained_root.display(),
            "add-on entrypoint resolved outside its package root"
        );
        return Err(AddonError::Validation(
            "installed entrypoint escapes or is missing from package".to_owned(),
        ));
    }
    let settings = load_addon_settings(pool, addon_id).await?;
    let mut environment = resolve_setting_environment(
        &installation.manifest,
        &installation.data_namespace,
        &settings,
        runtime,
    )?;
    let (program, mut args) = platform_command(entrypoint.kind, &executable)?;
    args.push(operation.to_owned());
    let tool_paths =
        tool_execution::resolve_tool_paths(&installation.manifest, &mut environment, &install_root)
            .await?;
    Ok(ResolvedAddonCommand {
        addon_id: addon_id.to_owned(),
        program,
        args,
        working_directory: install_root,
        environment,
        state_revision: installation.state_revision,
        tool_paths,
    })
}

fn validate_supported_operation(capability: &str, operation: &str) -> Result<(), AddonError> {
    let supported = matches!(
        (capability, operation),
        (CAPABILITY_TOOL_UPDATE_V1, "update")
            | (
                CAPABILITY_ADDON_CHECK_V1 | CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1,
                "check",
            )
            | (
                CAPABILITY_MEDIA_LIST_LIST_V1
                    | CAPABILITY_MEDIA_LIST_LIST_V2
                    | CAPABILITY_MEDIA_LIST_LIST_V3
                    | CAPABILITY_MEDIA_LIST_LIST_V4
                    | CAPABILITY_MEDIA_LIST_LIST_V5
                    | CAPABILITY_MEDIA_LIST_LIST_V6
                    | CAPABILITY_REMOTE_STREAM_LIST_V1
                    | CAPABILITY_REMOTE_STREAM_LIST_V2
                    | CAPABILITY_REMOTE_STREAM_LIST_V3
                    | CAPABILITY_REMOTE_STREAM_LIST_V4,
                "list",
            )
            | (
                CAPABILITY_REMOTE_STREAM_ITEM_V1 | CAPABILITY_REMOTE_STREAM_ITEM_V2,
                "item"
            )
            | (
                CAPABILITY_MEDIA_LIST_IMPORT_V1 | CAPABILITY_MEDIA_LIST_IMPORT_V2,
                "discover" | "enrich",
            )
            | (CAPABILITY_MEDIA_LIST_ENRICH_V1, "enrich-options")
            | (CAPABILITY_MEDIA_LIST_ACCESS_CHECK_V1, "test-access")
            | (CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1, "runtime-info")
            | (
                CAPABILITY_MEDIA_LIST_PREPARE_V1 | CAPABILITY_MEDIA_LIST_PREPARE_V2,
                "prepare"
            )
            | (CAPABILITY_REMOTE_STREAM_PLAY_V4, "play-intervals")
            | (CAPABILITY_MEDIA_LIST_SUBTITLES_V1, "subtitles")
            | (
                CAPABILITY_MEDIA_LIST_INTERVAL_METADATA_V1,
                "interval-metadata"
            )
            | (
                CAPABILITY_REMOTE_STREAM_PLAY_V1
                    | CAPABILITY_REMOTE_STREAM_PLAY_V2
                    | CAPABILITY_REMOTE_STREAM_PLAY_V3,
                "play"
            )
    );
    if !supported {
        return Err(AddonError::Validation(format!(
            "unsupported add-on capability operation {capability}/{operation}"
        )));
    }
    Ok(())
}

pub(crate) async fn validate_installed_dependencies(
    pool: &DbPool,
    manifest: &AddonManifest,
) -> Result<(), AddonError> {
    let installed = list_installed_addons(pool).await?;
    validate_dependencies_from_installations(&installed, manifest)
}

fn validate_dependencies_from_installations(
    installed: &[InstalledAddon],
    manifest: &AddonManifest,
) -> Result<(), AddonError> {
    for dependency in &manifest.dependencies {
        let available = installed.iter().any(|installed| {
            installed.addon_id == dependency.id
                && installed.enabled
                && dependency.version.matches(&installed.version)
        });
        if !available && !dependency.optional {
            return Err(AddonError::Validation(format!(
                "required dependency {} {} is not installed and enabled",
                dependency.id, dependency.version
            )));
        }
    }
    Ok(())
}

fn dependent_addon_ids_from_installations(
    installed: &[InstalledAddon],
    addon_id: &str,
    enabled_only: bool,
) -> Vec<String> {
    let mut dependents = installed
        .iter()
        .filter(|installed| !enabled_only || installed.enabled)
        .filter(|installed| {
            installed
                .manifest
                .dependencies
                .iter()
                .any(|dependency| !dependency.optional && dependency.id == addon_id)
        })
        .map(|installed| installed.addon_id.clone())
        .collect::<Vec<_>>();
    dependents.sort();
    dependents
}

/// Invoke one enabled capability with bounded stdout/stderr and a timeout.
/// Extra environment is supplied by the host operation, never by repository
/// metadata.
///
/// # Errors
///
/// Returns an error when resolution or the process contract fails.
pub async fn invoke_enabled_addon(
    pool: &DbPool,
    addon_id: &str,
    invocation: AddonInvocation<'_>,
) -> Result<Vec<u8>, AddonError> {
    let mut command = resolve_enabled_addon_command(
        pool,
        addon_id,
        invocation.capability,
        invocation.operation,
        invocation.runtime,
    )
    .await
    .map_err(AddonError::into_operation)?;
    command
        .environment
        .extend(invocation.extra_environment.clone());
    command.environment.insert(
        "ERSATZRS_ADDON_CAPABILITY".to_owned(),
        invocation.capability.to_owned(),
    );
    run_bounded_command(&command, invocation.timeout, invocation.max_output_bytes)
        .await
        .map_err(AddonError::into_operation)
}

/// Invoke one enabled capability with one bounded JSON request on stdin.
///
/// # Errors
///
/// Returns an error when resolution, stdin delivery, timeout, process-group
/// cleanup, or the bounded output contract fails.
pub async fn invoke_enabled_addon_with_input(
    pool: &DbPool,
    addon_id: &str,
    invocation: AddonInvocation<'_>,
    input: &[u8],
) -> Result<Vec<u8>, AddonError> {
    invoke_enabled_addon_with_input_and_redactions(pool, addon_id, invocation, input, &[]).await
}

/// Invoke one enabled capability while treating invocation-private values as
/// sensitive in both successful output and diagnostics.
pub(crate) async fn invoke_enabled_addon_with_input_and_redactions(
    pool: &DbPool,
    addon_id: &str,
    invocation: AddonInvocation<'_>,
    input: &[u8],
    sensitive_values: &[String],
) -> Result<Vec<u8>, AddonError> {
    let mut command = resolve_enabled_addon_command(
        pool,
        addon_id,
        invocation.capability,
        invocation.operation,
        invocation.runtime,
    )
    .await
    .map_err(AddonError::into_operation)?;
    command
        .environment
        .extend(invocation.extra_environment.clone());
    command.environment.insert(
        "ERSATZRS_ADDON_CAPABILITY".to_owned(),
        invocation.capability.to_owned(),
    );
    run_bounded_command_with_input(
        &command,
        invocation.timeout,
        invocation.max_output_bytes,
        input,
        sensitive_values,
    )
    .await
    .map_err(AddonError::into_operation)
}

async fn run_bounded_command(
    command: &ResolvedAddonCommand,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<Vec<u8>, AddonError> {
    run_bounded_command_inner(
        command,
        timeout,
        max_output_bytes,
        None,
        &[],
        None,
        crate::addon_tool_gate::ToolLeaseMode::Use,
    )
    .await
}

pub(crate) async fn run_bounded_command_with_input(
    command: &ResolvedAddonCommand,
    timeout: Duration,
    max_output_bytes: usize,
    input: &[u8],
    sensitive_values: &[String],
) -> Result<Vec<u8>, AddonError> {
    run_bounded_command_inner(
        command,
        timeout,
        max_output_bytes,
        Some(input),
        sensitive_values,
        None,
        crate::addon_tool_gate::ToolLeaseMode::Use,
    )
    .await
}

/// Cancel cooperatively so callers can await managed process termination before
/// releasing files or capacity. Dropping the sender also requests cancellation.
pub(crate) async fn run_bounded_command_with_input_cancellable(
    command: &ResolvedAddonCommand,
    timeout: Duration,
    max_output_bytes: usize,
    input: &[u8],
    sensitive_values: &[String],
    cancellation: tokio::sync::oneshot::Receiver<()>,
) -> Result<Vec<u8>, AddonError> {
    run_bounded_command_inner(
        command,
        timeout,
        max_output_bytes,
        Some(input),
        sensitive_values,
        Some(cancellation),
        crate::addon_tool_gate::ToolLeaseMode::Use,
    )
    .await
}

async fn run_bounded_command_inner(
    command: &ResolvedAddonCommand,
    timeout: Duration,
    max_output_bytes: usize,
    input: Option<&[u8]>,
    sensitive_values: &[String],
    mut cancellation: Option<tokio::sync::oneshot::Receiver<()>>,
    lease_mode: crate::addon_tool_gate::ToolLeaseMode,
) -> Result<Vec<u8>, AddonError> {
    if cancellation.as_mut().is_some_and(|cancellation| {
        matches!(
            cancellation.try_recv(),
            Ok(()) | Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        )
    }) {
        return Err(AddonError::Process("entrypoint cancelled".to_owned()));
    }
    let mut child =
        tool_execution::spawn_bounded_addon_process(command, input.is_some(), lease_mode)?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| AddonError::Process("entrypoint stdout is unavailable".to_owned()))?;
    let stderr = child
        .stderr()
        .take()
        .ok_or_else(|| AddonError::Process("entrypoint stderr is unavailable".to_owned()))?;
    let stdin_task = if let Some(input) = input {
        let mut stdin = child
            .stdin()
            .take()
            .ok_or_else(|| AddonError::Process("entrypoint stdin is unavailable".to_owned()))?;
        let input = input.to_vec();
        Some(tokio::spawn(async move {
            stdin.write_all(&input).await?;
            stdin.shutdown().await
        }))
    } else {
        None
    };
    let stdout_task = tokio::spawn(read_limited(stdout, max_output_bytes));
    let stderr_task = tokio::spawn(read_limited(stderr, MAX_CHECK_OUTPUT_BYTES));
    let wait_result = tokio::select! {
        result = tokio::time::timeout(timeout, child.wait()) => match result {
            Ok(Ok(status)) => Ok(status),
            Ok(Err(_)) => Err("entrypoint wait failed"),
            Err(_) => Err("entrypoint timed out"),
        },
        () = async {
            if let Some(cancellation) = cancellation {
                let _ = cancellation.await;
            } else {
                std::future::pending::<()>().await;
            }
        } => Err("entrypoint cancelled"),
    };
    let status = match wait_result {
        Ok(status) => status,
        Err(reason) => {
            let stopped = tokio::time::timeout(Duration::from_secs(5), child.kill())
                .await
                .is_ok_and(|result| result.is_ok());
            stdout_task.abort();
            stderr_task.abort();
            if let Some(task) = stdin_task {
                task.abort();
                let _ = task.await;
            }
            let _ = tokio::join!(stdout_task, stderr_task);
            return Err(if stopped {
                AddonError::Process(reason.to_owned())
            } else {
                AddonError::TerminationUnconfirmed
            });
        }
    };
    if let Some(task) = stdin_task {
        task.await
            .map_err(|_| AddonError::Process("stdin writer failed".to_owned()))?
            .map_err(|_| AddonError::Process("entrypoint stdin write failed".to_owned()))?;
    }
    let (stdout, stderr) = collect_bounded_output(stdout_task, stderr_task).await?;
    if stdout.len() > max_output_bytes || stderr.len() > MAX_CHECK_OUTPUT_BYTES {
        return Err(AddonError::Process(
            "entrypoint output exceeded its limit".to_owned(),
        ));
    }
    if !status.success() {
        return Err(AddonError::Operation(operation_error_from_stderr(
            &stderr,
            &command.environment,
            sensitive_values,
            status,
        )));
    }
    if output_contains_sensitive_value(&stdout, sensitive_values) {
        return Err(AddonError::Operation(AddonOperationError::new(
            OPERATION_CODE_FAILED,
            "add-on output contained private host data",
        )));
    }
    Ok(stdout)
}

fn output_contains_sensitive_value(output: &[u8], sensitive_values: &[String]) -> bool {
    if sensitive_values.iter().any(|value| {
        !value.is_empty()
            && output
                .windows(value.len())
                .any(|window| window == value.as_bytes())
    }) {
        return true;
    }
    let Ok(output) = std::str::from_utf8(output) else {
        return false;
    };
    output.lines().any(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .is_ok_and(|value| json_value_contains_sensitive_value(&value, sensitive_values))
    })
}

fn json_value_contains_sensitive_value(
    value: &serde_json::Value,
    sensitive_values: &[String],
) -> bool {
    match value {
        serde_json::Value::String(value) => sensitive_values
            .iter()
            .any(|sensitive| !sensitive.is_empty() && value.contains(sensitive)),
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| json_value_contains_sensitive_value(value, sensitive_values)),
        serde_json::Value::Object(values) => values.iter().any(|(key, value)| {
            sensitive_values
                .iter()
                .any(|sensitive| !sensitive.is_empty() && key.contains(sensitive))
                || json_value_contains_sensitive_value(value, sensitive_values)
        }),
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            false
        }
    }
}

fn operation_error_from_stderr(
    stderr: &[u8],
    environment: &BTreeMap<String, String>,
    sensitive_values: &[String],
    status: ExitStatus,
) -> AddonOperationError {
    let raw = String::from_utf8_lossy(stderr);
    if let Some(diagnostic) = structured_operation_error(&raw, environment, sensitive_values) {
        return diagnostic;
    }
    let detail = redact_and_bound_with_values(&raw, environment, sensitive_values, 1_024);
    let message = if detail.is_empty() {
        format!("add-on entrypoint exited with {status}")
    } else {
        detail
    };
    AddonOperationError::new(OPERATION_CODE_FAILED, message)
}

fn structured_operation_error(
    stderr: &str,
    environment: &BTreeMap<String, String>,
    sensitive_values: &[String],
) -> Option<AddonOperationError> {
    if let Some(line) = stderr.lines().rev().find(|line| !line.trim().is_empty()) {
        if let Ok(diagnostic) = serde_json::from_str::<AddonOperationError>(line.trim()) {
            if diagnostic.validate().is_ok() {
                return Some(AddonOperationError::new(
                    &diagnostic.code,
                    redact_and_bound_with_values(
                        &diagnostic.message,
                        environment,
                        sensitive_values,
                        1_024,
                    ),
                ));
            }
        }
    }
    None
}

async fn collect_bounded_output(
    mut stdout_task: tokio::task::JoinHandle<Result<Vec<u8>, AddonError>>,
    mut stderr_task: tokio::task::JoinHandle<Result<Vec<u8>, AddonError>>,
) -> Result<(Vec<u8>, Vec<u8>), AddonError> {
    let readers = tokio::time::timeout(OUTPUT_PIPE_SHUTDOWN_TIMEOUT, async {
        let stdout = (&mut stdout_task)
            .await
            .map_err(|_| AddonError::Process("stdout reader failed".to_owned()))??;
        let stderr = (&mut stderr_task)
            .await
            .map_err(|_| AddonError::Process("stderr reader failed".to_owned()))??;
        Ok::<_, AddonError>((stdout, stderr))
    })
    .await;
    if let Ok(result) = readers {
        result
    } else {
        stdout_task.abort();
        stderr_task.abort();
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        Err(AddonError::Process(
            "entrypoint output pipes did not close".to_owned(),
        ))
    }
}

pub(crate) fn spawn_managed_addon_process(process: Command) -> std::io::Result<ManagedAddonChild> {
    let mut process = CommandWrap::from(process);
    #[cfg(unix)]
    process.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    process.wrap(JobObject);
    #[cfg(windows)]
    let process_job = crate::addon_process_job::TrackedJob::attach(&mut process)?;
    process.wrap(KillOnDrop);
    let child = process.spawn()?;
    #[cfg(unix)]
    let process_group = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .filter(|id| *id > 1)
        .map(nix::unistd::Pid::from_raw)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
    Ok(ManagedAddonChild {
        child,
        tool_lease: None,
        terminate_on_drop: true,
        #[cfg(unix)]
        process_group,
        #[cfg(windows)]
        process_job,
    })
}

#[cfg(not(windows))]
fn addon_process(command: &ResolvedAddonCommand) -> Command {
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    process
}

#[cfg(windows)]
fn addon_process(command: &ResolvedAddonCommand) -> Command {
    use std::os::windows::process::CommandExt as _;

    let extension = Path::new(&command.program)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd") {
        let mut process = Command::new("cmd.exe");
        process
            .as_std_mut()
            .raw_arg(windows_addon_raw_arguments(command));
        process
    } else {
        let mut process = Command::new(&command.program);
        process.args(&command.args);
        process
    }
}

#[cfg(any(windows, test))]
fn windows_addon_raw_arguments(command: &ResolvedAddonCommand) -> String {
    let quoted = std::iter::once(&command.program)
        .chain(command.args.iter())
        .map(|value| format!("\"{}\"", value.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");
    format!("/D /S /C \"{quoted}\"")
}

pub(crate) fn addon_process_environment() -> BTreeMap<String, String> {
    addon_process_environment_with(crate::startup_config::environment_variable)
}

fn addon_process_environment_with(
    mut lookup: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> BTreeMap<String, String> {
    #[cfg(windows)]
    const ALLOWED_NAMES: &[&str] = &[
        "PATH",
        "PATHEXT",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "TEMP",
        "TMP",
        // Native browser discovery uses these locations (ADR 0085). Cookie
        // extraction remains owned by the explicitly selected provider option.
        "APPDATA",
        "LOCALAPPDATA",
    ];
    #[cfg(not(windows))]
    const ALLOWED_NAMES: &[&str] = &["PATH", "LANG", "LC_ALL", "TMPDIR"];

    ALLOWED_NAMES
        .iter()
        .filter_map(|name| {
            lookup(name)
                .and_then(|value| value.into_string().ok())
                .map(|value| ((*name).to_owned(), value))
        })
        .collect()
}

async fn validated_install_path_for_removal(stored_path: &Path) -> Result<PathBuf, AddonError> {
    let configured_root = addon_packages_folder();
    let relative = stored_path.strip_prefix(&configured_root).map_err(|_| {
        AddonError::Package("stored install path is outside the add-on package root".to_owned())
    })?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(AddonError::Package(
            "stored install path is outside the add-on package root".to_owned(),
        ));
    }

    let canonical_root = containment_path_async(&configured_root)
        .await
        .map_err(package_io_error)?;
    match containment_path_async(stored_path).await {
        Ok(canonical_path)
            if canonical_path != canonical_root && canonical_path.starts_with(&canonical_root) =>
        {
            Ok(canonical_path)
        }
        Ok(_) => Err(AddonError::Package(
            "stored install path is outside the add-on package root".to_owned(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(stored_path.to_path_buf()),
        Err(error) => Err(package_io_error(error)),
    }
}

/// Verify that an active package still matches the content selected at install
/// time and return its canonical package root.
///
/// # Errors
///
/// Returns an error for an escaped path, changed package content, or I/O.
pub async fn verify_installed_addon_contents(
    installation: &InstalledAddon,
) -> Result<PathBuf, AddonError> {
    verify_addon_content_path(&installation.install_path, &installation.content_sha256).await
}

pub(crate) async fn verify_addon_content_path(
    stored_path: &Path,
    expected_sha256: &str,
) -> Result<PathBuf, AddonError> {
    let install_root = canonicalize_addon_path(stored_path)
        .await
        .map_err(package_io_error)?;
    let actual = hash_addon_tree(install_root.clone()).await?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(AddonError::Trust(
            "installed add-on content no longer matches the verified package".to_owned(),
        ));
    }
    Ok(install_root)
}

/// Calculate the deterministic v1 content digest used for installed-package
/// verification and package-fixture tooling.
///
/// # Errors
///
/// Returns an error for unsupported entries, limits, or I/O.
pub async fn calculate_addon_content_sha256(root: &Path) -> Result<String, AddonError> {
    let root = canonicalize_addon_path(root)
        .await
        .map_err(package_io_error)?;
    hash_addon_tree(root).await
}

/// Canonicalize a path the way add-on package containment checks compare them.
///
/// `std::fs::canonicalize` returns `\\?\`-prefixed verbatim paths on Windows.
/// Those never compare equal to the plain `C:\...` paths the package folders are
/// built from, and `cmd.exe` cannot resolve them for batch entrypoints, so the
/// whole add-on install path has to stay on `dunce`, which keeps the ordinary
/// form whenever Windows accepts it.
async fn canonicalize_addon_path(path: &Path) -> std::io::Result<PathBuf> {
    let path = path.to_path_buf();
    match tokio::task::spawn_blocking(move || dunce::canonicalize(&path)).await {
        Ok(result) => result,
        Err(_) => Err(std::io::Error::other(
            "package path canonicalization task failed",
        )),
    }
}

/// Canonicalize a path purely to compare it against a package root.
///
/// `dunce` gives back the compatible form *when Windows accepts it* and the
/// verbatim `\\?\` form otherwise — for a path over `MAX_PATH`, a reserved DOS
/// name such as `con.txt`, or a UNC share. That fallback is per path, so a short
/// package root and a deeply nested file inside it can come back in different
/// forms and `starts_with` reports the file as escaping its own package. Prefix
/// checks therefore run on the raw canonical form, which is uniform on both
/// sides, and only the paths handed to a child process stay on `dunce`.
fn containment_path(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

async fn containment_path_async(path: &Path) -> std::io::Result<PathBuf> {
    let path = path.to_path_buf();
    match tokio::task::spawn_blocking(move || containment_path(&path)).await {
        Ok(result) => result,
        Err(_) => Err(std::io::Error::other(
            "package path canonicalization task failed",
        )),
    }
}

async fn hash_addon_tree(root: PathBuf) -> Result<String, AddonError> {
    tokio::task::spawn_blocking(move || hash_addon_tree_sync(&root))
        .await
        .map_err(|_| AddonError::Package("package hashing task failed".to_owned()))?
}

fn hash_addon_tree_sync(root: &Path) -> Result<String, AddonError> {
    // The walk starts at `root` but re-canonicalizes every file it reads, so the
    // root itself must already be canonical or every file looks like an escape:
    // `\\?\C:\...` vs `C:\...` on Windows, `/private/var` vs `/var` on macOS.
    // Packages are staged and installed under plain, uncanonicalized paths.
    let root = dunce::canonicalize(root).map_err(package_io_error)?;
    let root = root.as_path();
    // Prefix checks compare raw canonical forms so a long or reserved file name
    // inside a short package root cannot look like an escape; see
    // `containment_path`.
    let containment_root = containment_path(root).map_err(package_io_error)?;
    let mut directories = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut entry_count = 0_usize;
    let mut total_size = 0_u64;
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).map_err(package_io_error)? {
            let entry = entry.map_err(package_io_error)?;
            entry_count = entry_count.saturating_add(1);
            if entry_count > MAX_ARCHIVE_ENTRIES {
                return Err(AddonError::Package(
                    "installed package contains too many files".to_owned(),
                ));
            }
            let file_type = entry.file_type().map_err(package_io_error)?;
            if file_type.is_symlink() {
                return Err(AddonError::Package(
                    "installed package contains a symlink".to_owned(),
                ));
            }
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if file_type.is_file() {
                let size = entry.metadata().map_err(package_io_error)?.len();
                total_size = total_size.saturating_add(size);
                if total_size > MAX_EXTRACTED_BYTES {
                    return Err(AddonError::Package(
                        "installed package exceeds its size limit".to_owned(),
                    ));
                }
                files.push(entry.path());
            } else {
                return Err(AddonError::Package(
                    "installed package contains an unsupported file type".to_owned(),
                ));
            }
        }
    }
    files.sort();

    let mut digest = Sha256::new();
    digest.update(b"ersatzrs-addon-content-v1\0");
    let mut buffer = vec![0_u8; 64 * 1024];
    for path in files {
        let relative = path.strip_prefix(root).map_err(|_| {
            tracing::warn!(
                path = %path.display(),
                root = %root.display(),
                "package file is not below the package root"
            );
            AddonError::Package("installed package path escaped its root".to_owned())
        })?;
        let mut encoded_path = Vec::new();
        for (index, component) in relative.components().enumerate() {
            let std::path::Component::Normal(component) = component else {
                tracing::warn!(
                    path = %path.display(),
                    "package file path contains a component that is not a plain name"
                );
                return Err(AddonError::Package(
                    "installed package path escaped its root".to_owned(),
                ));
            };
            if index > 0 {
                encoded_path.push(b'/');
            }
            encoded_path.extend_from_slice(component.as_encoded_bytes());
        }
        digest.update(
            u64::try_from(encoded_path.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        digest.update(&encoded_path);
        let canonical_file = containment_path(&path).map_err(package_io_error)?;
        if !canonical_file.starts_with(&containment_root) || !canonical_file.is_file() {
            tracing::warn!(
                path = %canonical_file.display(),
                root = %containment_root.display(),
                "package file resolved outside the package root"
            );
            return Err(AddonError::Package(
                "installed package path escaped its root".to_owned(),
            ));
        }
        let mut file = std::fs::File::open(canonical_file).map_err(package_io_error)?;
        let file_size = file.metadata().map_err(package_io_error)?.len();
        digest.update(file_size.to_le_bytes());
        loop {
            let read = file.read(&mut buffer).map_err(package_io_error)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        digest.update([0]);
    }
    Ok(hex_digest(digest.finalize()))
}

async fn read_limited(
    reader: impl tokio::io::AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, AddonError> {
    let mut output = Vec::new();
    reader
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut output)
        .await
        .map_err(|_| AddonError::Process("entrypoint output could not be read".to_owned()))?;
    Ok(output)
}

fn resolve_setting_environment(
    manifest: &AddonManifest,
    data_namespace: &str,
    settings: &[AddonSettingValue],
    runtime: &AddonRuntimeContext,
) -> Result<BTreeMap<String, String>, AddonError> {
    let configured = settings
        .iter()
        .map(|setting| (setting.key.as_str(), setting))
        .collect::<BTreeMap<_, _>>();
    let mut environment = BTreeMap::from([
        (
            "ERSATZRS_ADDON_CHECK_CONTEXT_VERSION".to_owned(),
            "1".to_owned(),
        ),
        ("ERSATZRS_ADDON_ID".to_owned(), manifest.id.clone()),
        (
            "ERSATZRS_ADDON_VERSION".to_owned(),
            manifest.version.to_string(),
        ),
        (
            "ERSATZRS_ADDON_DATA_DIR".to_owned(),
            addon_data_folder(data_namespace, &manifest.id)
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "ERSATZRS_ADDON_CACHE_DIR".to_owned(),
            addon_cache_folder(data_namespace, &manifest.id)
                .to_string_lossy()
                .into_owned(),
        ),
    ]);
    for definition in &manifest.settings {
        let setting = configured.get(definition.key.as_str()).copied();
        if definition.kind == AddonSettingKind::SecretReference {
            let Some(setting) = setting else {
                if definition.required {
                    return Err(AddonError::operation(
                        OPERATION_CODE_MISSING_SECRET,
                        "required add-on secret reference is not configured",
                    ));
                }
                continue;
            };
            validate_setting_value(definition, setting)?;
            let reference = setting.reference.as_deref().unwrap_or_default();
            match setting.reference_kind {
                Some(AddonSecretReferenceKind::Environment) => {
                    let secret = crate::startup_config::environment_variable(reference)
                        .and_then(|value| value.into_string().ok())
                        .ok_or_else(|| {
                            AddonError::operation(
                                OPERATION_CODE_MISSING_SECRET,
                                "configured add-on secret reference is unavailable",
                            )
                        })?;
                    environment.insert(format!("ERSATZRS_ADDON_SECRET_{}", definition.key), secret);
                }
                Some(AddonSecretReferenceKind::File) => {
                    environment.insert(
                        format!("ERSATZRS_ADDON_SECRET_FILE_{}", definition.key),
                        reference.to_owned(),
                    );
                }
                None => unreachable!("validated secret reference"),
            }
        } else {
            let mut value = setting
                .and_then(|item| item.value.clone())
                .or_else(|| definition.default.clone());
            if value.is_none()
                && manifest
                    .media_list_storage
                    .as_ref()
                    .is_some_and(|storage| storage.path_setting == definition.key)
            {
                value = effective_storage_path(manifest, settings)?
                    .map(|path| path.to_string_lossy().into_owned());
            }
            if definition.required && value.as_deref().is_none_or(str::is_empty) {
                return Err(AddonError::operation(
                    OPERATION_CODE_MISSING_SETTING,
                    "required add-on setting is not configured",
                ));
            }
            if let Some(value) = value {
                environment.insert(format!("ERSATZRS_ADDON_SETTING_{}", definition.key), value);
            }
        }
    }
    if manifest.permissions.managed_ffmpeg {
        let ffmpeg = runtime.ffmpeg_path.as_ref().ok_or_else(|| {
            AddonError::Validation("managed FFmpeg path is unavailable".to_owned())
        })?;
        let ffprobe = runtime.ffprobe_path.as_ref().ok_or_else(|| {
            AddonError::Validation("managed ffprobe path is unavailable".to_owned())
        })?;
        environment.insert("FFMPEG_BIN".to_owned(), ffmpeg.clone());
        environment.insert("FFPROBE_BIN".to_owned(), ffprobe.clone());
    }
    Ok(environment)
}

fn should_clear_optional_setting(
    manifest: &AddonManifest,
    definition: &ersatzrs_addon_contract::AddonSettingDefinition,
    setting: &AddonSettingValue,
) -> bool {
    if definition.required {
        return false;
    }
    (definition.kind == AddonSettingKind::SecretReference
        && setting
            .reference
            .as_deref()
            .is_none_or(|value| value.trim().is_empty()))
        || (manifest
            .media_list_storage
            .as_ref()
            .is_some_and(|storage| storage.path_setting == setting.key)
            && setting
                .value
                .as_deref()
                .is_none_or(|value| value.trim().is_empty()))
}

fn validate_storage_path_override(
    manifest: &AddonManifest,
    setting: &AddonSettingValue,
) -> Result<(), AddonError> {
    if manifest
        .media_list_storage
        .as_ref()
        .is_some_and(|storage| storage.path_setting == setting.key)
        && setting
            .value
            .as_deref()
            .is_some_and(|value| !Path::new(value.trim()).is_absolute())
    {
        return Err(AddonError::Validation(format!(
            "{} must be an absolute path",
            setting.key
        )));
    }
    Ok(())
}

fn validate_setting_value(
    definition: &ersatzrs_addon_contract::AddonSettingDefinition,
    setting: &AddonSettingValue,
) -> Result<(), AddonError> {
    if definition.kind == AddonSettingKind::SecretReference {
        if setting.value.is_some() {
            return Err(AddonError::Validation(
                "secret settings cannot store a raw value".to_owned(),
            ));
        }
        let kind = setting.reference_kind.ok_or_else(|| {
            AddonError::Validation("secret setting requires a reference kind".to_owned())
        })?;
        let reference = setting
            .reference
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AddonError::Validation("secret reference is empty".to_owned()))?;
        match kind {
            AddonSecretReferenceKind::Environment if !is_environment_name(reference) => {
                return Err(AddonError::Validation(
                    "environment reference name is invalid".to_owned(),
                ));
            }
            AddonSecretReferenceKind::File if !Path::new(reference).is_absolute() => {
                return Err(AddonError::Validation(
                    "file secret reference must be an absolute path".to_owned(),
                ));
            }
            _ => {}
        }
        return Ok(());
    }
    if setting.reference_kind.is_some() || setting.reference.is_some() {
        return Err(AddonError::Validation(
            "non-secret setting cannot contain a secret reference".to_owned(),
        ));
    }
    let value = setting.value.as_deref().unwrap_or_default();
    match definition.kind {
        AddonSettingKind::Boolean if !matches!(value, "true" | "false") => Err(
            AddonError::Validation("boolean setting is invalid".to_owned()),
        ),
        AddonSettingKind::Integer if value.parse::<i64>().is_err() => Err(AddonError::Validation(
            "integer setting is invalid".to_owned(),
        )),
        AddonSettingKind::Enum if !definition.options.iter().any(|item| item == value) => {
            Err(AddonError::Validation("enum setting is invalid".to_owned()))
        }
        AddonSettingKind::SecretReference => unreachable!("handled above"),
        _ => Ok(()),
    }
}

fn is_environment_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with(|character: char| character.is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(not(windows))]
fn platform_command(
    kind: AddonEntrypointKind,
    executable: &Path,
) -> Result<(String, Vec<String>), AddonError> {
    let path = executable.to_string_lossy().into_owned();
    match kind {
        AddonEntrypointKind::Native => Ok((path, Vec::new())),
        AddonEntrypointKind::PosixShell => Ok(("/bin/sh".to_owned(), vec![path])),
        AddonEntrypointKind::WindowsBatch => Err(AddonError::Validation(
            "Windows batch entrypoint cannot run on this platform".to_owned(),
        )),
    }
}

#[cfg(windows)]
fn platform_command(
    kind: AddonEntrypointKind,
    executable: &Path,
) -> Result<(String, Vec<String>), AddonError> {
    let path = executable.to_string_lossy().into_owned();
    match kind {
        AddonEntrypointKind::Native | AddonEntrypointKind::WindowsBatch => Ok((path, Vec::new())),
        AddonEntrypointKind::PosixShell => Err(AddonError::Validation(
            "POSIX shell entrypoint cannot run on this platform".to_owned(),
        )),
    }
}

async fn read_repository_file(path: &Path) -> Result<Vec<TrustedRepositoryEntry>, AddonError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| AddonError::Configuration("repository file is unavailable".to_owned()))?;
    if metadata.len() > MAX_REPOSITORY_FILE_BYTES as u64 {
        return Err(AddonError::Configuration(
            "repository file is too large".to_owned(),
        ));
    }
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|_| AddonError::Configuration("repository file is not UTF-8".to_owned()))?;
    let parsed: TrustedRepositoryFile = toml::from_str(&content).map_err(|_| {
        AddonError::Configuration("repository file does not match its schema".to_owned())
    })?;
    let mut ids = BTreeSet::new();
    let mut urls = BTreeSet::new();
    for entry in &parsed.repositories {
        validate_repository_entry(entry)?;
        if !ids.insert(entry.id.as_str()) || !urls.insert(entry.index_url.as_str()) {
            return Err(AddonError::Configuration(
                "repository identifiers and URLs must be unique".to_owned(),
            ));
        }
    }
    Ok(parsed.repositories)
}

fn validate_repository_entry(entry: &TrustedRepositoryEntry) -> Result<(), AddonError> {
    validate_addon_id(&entry.id).map_err(|error| AddonError::Configuration(error.to_string()))?;
    if entry.name.trim().is_empty() || entry.name.len() > 255 {
        return Err(AddonError::Configuration(
            "repository name is empty or too long".to_owned(),
        ));
    }
    if entry.index_url.len() > 767 {
        return Err(AddonError::Configuration(
            "repository index URL is too long".to_owned(),
        ));
    }
    validate_https_url(&entry.index_url, "repository index")?;
    let index_url = reqwest::Url::parse(&entry.index_url)
        .map_err(|_| AddonError::Configuration("repository index URL is invalid".to_owned()))?;
    if index_url.query().is_some() || index_url.fragment().is_some() {
        return Err(AddonError::Configuration(
            "repository index URL cannot contain a query or fragment".to_owned(),
        ));
    }
    if !entry.public_key.trim().is_empty() {
        decode_public_key(&entry.public_key)?;
    }
    Ok(())
}

async fn upsert_repository_row(
    pool: &DbPool,
    entry: &TrustedRepositoryEntry,
) -> Result<(), AddonError> {
    validate_repository_entry(entry)?;
    for _ in 0..MAX_REPOSITORY_RECONCILE_ATTEMPTS {
        let Some(existing) = load_persisted_repository_state(pool, &entry.id).await? else {
            if insert_repository_row(pool, entry).await? {
                return Ok(());
            }
            continue;
        };
        let unchanged = existing.name == entry.name
            && existing.index_url == entry.index_url
            && existing.public_key == entry.public_key
            && existing.is_official == entry.official
            && existing.is_enabled == entry.enabled;
        if unchanged {
            return Ok(());
        }
        if update_repository_row(pool, entry, &existing).await? == 1 {
            return Ok(());
        }
    }
    Err(AddonError::Trust(
        "repository configuration kept changing during reconciliation".to_owned(),
    ))
}

async fn insert_repository_row(
    pool: &DbPool,
    entry: &TrustedRepositoryEntry,
) -> Result<bool, AddonError> {
    let insert = sqlx::query(
        r#"INSERT INTO "AddonRepository" (
               "Id", "Name", "IndexUrl", "PublicKey", "IsOfficial", "IsEnabled"
           ) VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&entry.id)
    .bind(&entry.name)
    .bind(&entry.index_url)
    .bind(&entry.public_key)
    .bind(i32::from(entry.official))
    .bind(i32::from(entry.enabled))
    .execute(pool)
    .await;
    match insert {
        Ok(_) => Ok(true),
        Err(error) => {
            if load_persisted_repository_state(pool, &entry.id)
                .await?
                .is_none()
            {
                return Err(database_error(error));
            }
            Ok(false)
        }
    }
}

async fn update_repository_row(
    pool: &DbPool,
    entry: &TrustedRepositoryEntry,
    existing: &PersistedRepositoryState,
) -> Result<u64, AddonError> {
    let trust_changed =
        existing.index_url != entry.index_url || existing.public_key != entry.public_key;
    let updated = if trust_changed {
        sqlx::query(
            r#"UPDATE "AddonRepository"
               SET "Name" = ?, "IndexUrl" = ?, "PublicKey" = ?,
                   "IsOfficial" = ?, "IsEnabled" = ?, "LastSequence" = NULL,
                   "LastRefresh" = NULL, "LastError" = NULL, "CatalogJson" = NULL,
                   "CatalogEtag" = NULL, "CatalogLastModified" = NULL,
                   "StateRevision" = "StateRevision" + 1,
                   "CatalogRevision" = "CatalogRevision" + 1
               WHERE "Id" = ? AND "StateRevision" = ?"#,
        )
        .bind(&entry.name)
        .bind(&entry.index_url)
        .bind(&entry.public_key)
        .bind(i32::from(entry.official))
        .bind(i32::from(entry.enabled))
        .bind(&entry.id)
        .bind(existing.state_revision)
        .execute(pool)
        .await
        .map_err(database_error)?
    } else if existing.is_enabled != entry.enabled {
        sqlx::query(
            r#"UPDATE "AddonRepository"
               SET "Name" = ?, "IndexUrl" = ?, "PublicKey" = ?,
                   "IsOfficial" = ?, "IsEnabled" = ?,
                   "StateRevision" = "StateRevision" + 1,
                   "CatalogRevision" = "CatalogRevision" + 1
               WHERE "Id" = ? AND "StateRevision" = ?"#,
        )
        .bind(&entry.name)
        .bind(&entry.index_url)
        .bind(&entry.public_key)
        .bind(i32::from(entry.official))
        .bind(i32::from(entry.enabled))
        .bind(&entry.id)
        .bind(existing.state_revision)
        .execute(pool)
        .await
        .map_err(database_error)?
    } else {
        sqlx::query(
            r#"UPDATE "AddonRepository"
               SET "Name" = ?, "IndexUrl" = ?, "PublicKey" = ?,
                   "IsOfficial" = ?, "IsEnabled" = ?,
                   "StateRevision" = "StateRevision" + 1
               WHERE "Id" = ? AND "StateRevision" = ?"#,
        )
        .bind(&entry.name)
        .bind(&entry.index_url)
        .bind(&entry.public_key)
        .bind(i32::from(entry.official))
        .bind(i32::from(entry.enabled))
        .bind(&entry.id)
        .bind(existing.state_revision)
        .execute(pool)
        .await
        .map_err(database_error)?
    };
    Ok(updated.rows_affected())
}

async fn load_persisted_repository_state(
    pool: &DbPool,
    id: &str,
) -> Result<Option<PersistedRepositoryState>, AddonError> {
    let row = sqlx::query(
        r#"SELECT "Name", "IndexUrl", "PublicKey", "IsOfficial", "IsEnabled", "StateRevision"
           FROM "AddonRepository" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(database_error)?;
    row.as_ref()
        .map(|row| {
            Ok(PersistedRepositoryState {
                name: row.try_get("Name").repository().map_err(repository_error)?,
                index_url: row
                    .try_get("IndexUrl")
                    .repository()
                    .map_err(repository_error)?,
                public_key: row
                    .try_get("PublicKey")
                    .repository()
                    .map_err(repository_error)?,
                is_official: decode_bool_column(row, "IsOfficial").map_err(repository_error)?,
                is_enabled: decode_bool_column(row, "IsEnabled").map_err(repository_error)?,
                state_revision: row
                    .try_get("StateRevision")
                    .repository()
                    .map_err(repository_error)?,
            })
        })
        .transpose()
}

async fn load_repository_row(
    pool: &DbPool,
    id: &str,
) -> Result<Option<TrustedAddonRepository>, AddonError> {
    let row = sqlx::query(
        r#"SELECT "Id", "Name", "IndexUrl", "PublicKey", "IsOfficial", "IsEnabled",
                  "LastSequence", "LastRefresh", "LastError"
           FROM "AddonRepository" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(database_error)?;
    row.as_ref().map(repository_from_row).transpose()
}

async fn effective_repository(
    pool: &DbPool,
    repository_id: &str,
) -> Result<TrustedAddonRepository, AddonError> {
    list_trusted_repositories(pool)
        .await?
        .into_iter()
        .find(|item| item.id == repository_id)
        .ok_or_else(|| {
            AddonError::NotFound("repository is not in the trusted allowlist".to_owned())
        })
}

fn repository_from_row(row: &DbRow) -> Result<TrustedAddonRepository, AddonError> {
    let sequence = row
        .try_get::<Option<i64>, _>("LastSequence")
        .repository()
        .map_err(repository_error)?
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                AddonError::Validation("stored repository sequence is negative".to_owned())
            })
        })
        .transpose()?;
    Ok(TrustedAddonRepository {
        id: row.try_get("Id").repository().map_err(repository_error)?,
        name: row.try_get("Name").repository().map_err(repository_error)?,
        index_url: row
            .try_get("IndexUrl")
            .repository()
            .map_err(repository_error)?,
        public_key: row
            .try_get("PublicKey")
            .repository()
            .map_err(repository_error)?,
        is_official: decode_bool_column(row, "IsOfficial").map_err(repository_error)?,
        is_enabled: decode_bool_column(row, "IsEnabled").map_err(repository_error)?,
        last_sequence: sequence,
        last_refresh: decode_optional_string_column(row, "LastRefresh")
            .map_err(repository_error)?,
        last_error: decode_optional_string_column(row, "LastError").map_err(repository_error)?,
    })
}

fn installed_from_row(row: &DbRow) -> Result<InstalledAddon, AddonError> {
    let addon_id: String = row
        .try_get("AddonId")
        .repository()
        .map_err(repository_error)?;
    let version_text: String = row
        .try_get("Version")
        .repository()
        .map_err(repository_error)?;
    let version = Version::parse(&version_text)
        .map_err(|_| AddonError::Validation("stored add-on version is invalid".to_owned()))?;
    let manifest_json: String = row
        .try_get("ManifestJson")
        .repository()
        .map_err(repository_error)?;
    let manifest: AddonManifest = serde_json::from_str(&manifest_json)
        .map_err(|_| AddonError::Validation("stored add-on manifest is invalid".to_owned()))?;
    manifest
        .validate()
        .map_err(|error| AddonError::Validation(error.to_string()))?;
    if manifest.id != addon_id || manifest.version != version {
        return Err(AddonError::Validation(
            "stored add-on identity differs from its manifest".to_owned(),
        ));
    }
    Ok(InstalledAddon {
        addon_id,
        repository_id: decode_optional_string_column(row, "RepositoryId")
            .map_err(repository_error)?,
        source_id: decode_optional_string_column(row, "SourceId").map_err(repository_error)?,
        version,
        enabled: decode_bool_column(row, "Enabled").map_err(repository_error)?,
        trust_tier: match row
            .try_get::<String, _>("TrustTier")
            .repository()
            .map_err(repository_error)?
            .as_str()
        {
            "verified" => AddonTrustTier::Verified,
            "unverified" => AddonTrustTier::Unverified,
            "sideloaded" => AddonTrustTier::Sideloaded,
            "local" => AddonTrustTier::Local,
            _ => {
                return Err(AddonError::Validation(
                    "stored add-on trust tier is invalid".to_owned(),
                ));
            }
        },
        package_sha256: row
            .try_get("PackageSha256")
            .repository()
            .map_err(repository_error)?,
        content_sha256: row
            .try_get("ContentSha256")
            .repository()
            .map_err(repository_error)?,
        manifest,
        install_path: PathBuf::from(
            row.try_get::<String, _>("InstallPath")
                .repository()
                .map_err(repository_error)?,
        ),
        data_namespace: row
            .try_get("DataNamespace")
            .repository()
            .map_err(repository_error)?,
        installed_at: row
            .try_get("InstalledAt")
            .repository()
            .map_err(repository_error)?,
        updated_at: row
            .try_get("UpdatedAt")
            .repository()
            .map_err(repository_error)?,
        state_revision: row
            .try_get("StateRevision")
            .repository()
            .map_err(repository_error)?,
        last_check_code: decode_optional_string_column(row, "LastCheckCode")
            .map_err(repository_error)?,
        last_check_message: decode_optional_string_column(row, "LastCheckMessage")
            .map_err(repository_error)?,
        last_check_tool_update: decode_optional_string_column(row, "LastCheckToolUpdateJson")
            .map_err(repository_error)?
            .and_then(|json| serde_json::from_str::<AddonToolUpdateWarning>(&json).ok())
            .filter(|context| context.validate().is_ok()),
        last_check_at: decode_optional_string_column(row, "LastCheckAt")
            .map_err(repository_error)?,
    })
}

fn setting_from_row(row: &DbRow) -> Result<AddonSettingValue, AddonError> {
    let reference_kind = decode_optional_string_column(row, "ReferenceKind")
        .map_err(repository_error)?
        .map(|value| AddonSecretReferenceKind::parse(&value))
        .transpose()?;
    Ok(AddonSettingValue {
        key: row.try_get("Key").repository().map_err(repository_error)?,
        value: decode_optional_string_column(row, "Value").map_err(repository_error)?,
        reference_kind,
        reference: decode_optional_string_column(row, "Reference").map_err(repository_error)?,
    })
}

async fn load_catalog_json(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Option<String>, AddonError> {
    sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "CatalogJson" FROM "AddonRepository" WHERE "Id" = ?"#,
    )
    .bind(repository_id)
    .fetch_optional(pool)
    .await
    .map(Option::flatten)
    .map_err(database_error)
}

async fn load_catalog_cache(
    pool: &DbPool,
    repository_id: &str,
) -> Result<StoredCatalogCache, AddonError> {
    let row = sqlx::query(
        r#"SELECT "CatalogJson", "CatalogEtag", "CatalogLastModified",
                  "StateRevision", "CatalogRevision", "IndexUrl", "PublicKey", "IsEnabled"
           FROM "AddonRepository" WHERE "Id" = ?"#,
    )
    .bind(repository_id)
    .fetch_optional(pool)
    .await
    .map_err(database_error)?
    .ok_or_else(|| AddonError::NotFound("repository is not persisted".to_owned()))?;
    Ok(StoredCatalogCache {
        raw_json: decode_optional_string_column(&row, "CatalogJson").map_err(repository_error)?,
        etag: decode_optional_string_column(&row, "CatalogEtag").map_err(repository_error)?,
        last_modified: decode_optional_string_column(&row, "CatalogLastModified")
            .map_err(repository_error)?,
        state_revision: row
            .try_get("StateRevision")
            .repository()
            .map_err(repository_error)?,
        catalog_revision: row
            .try_get("CatalogRevision")
            .repository()
            .map_err(repository_error)?,
        index_url: row
            .try_get("IndexUrl")
            .repository()
            .map_err(repository_error)?,
        public_key: row
            .try_get("PublicKey")
            .repository()
            .map_err(repository_error)?,
        is_enabled: decode_bool_column(&row, "IsEnabled").map_err(repository_error)?,
    })
}

fn validate_repeated_catalog_sequence(
    last_sequence: Option<u64>,
    previous_catalog: Option<&str>,
    current_sequence: u64,
    current_catalog: &str,
) -> Result<(), AddonError> {
    if last_sequence == Some(current_sequence) && previous_catalog != Some(current_catalog) {
        return Err(AddonError::Trust(
            "catalog reused its sequence for different signed content".to_owned(),
        ));
    }
    Ok(())
}

fn validate_catalog(
    index: &AddonRepositoryIndex,
    repository: &TrustedAddonRepository,
    now: DateTime<Utc>,
) -> Result<(), AddonError> {
    index
        .validate()
        .map_err(|error| AddonError::Validation(error.to_string()))?;
    if index.repository_id != repository.id {
        return Err(AddonError::Trust(
            "signed repository identity does not match allowlist".to_owned(),
        ));
    }
    if repository
        .last_sequence
        .is_some_and(|sequence| index.sequence < sequence)
    {
        return Err(AddonError::Trust(
            "catalog sequence would roll back trusted metadata".to_owned(),
        ));
    }
    let generated_at = DateTime::parse_from_rfc3339(&index.generated_at)
        .map_err(|_| AddonError::Validation("generated_at is not RFC 3339".to_owned()))?
        .with_timezone(&Utc);
    let expires_at = DateTime::parse_from_rfc3339(&index.expires_at)
        .map_err(|_| AddonError::Validation("expires_at is not RFC 3339".to_owned()))?
        .with_timezone(&Utc);
    if expires_at <= now {
        return Err(AddonError::Trust("signed catalog has expired".to_owned()));
    }
    if generated_at > now + chrono::Duration::hours(24) || expires_at <= generated_at {
        return Err(AddonError::Validation(
            "catalog validity interval is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn verify_detached_signature(
    public_key_b64: &str,
    payload: &[u8],
    signature_bytes: &[u8],
) -> Result<(), AddonError> {
    let key = decode_public_key(public_key_b64)?;
    let signature_text = std::str::from_utf8(signature_bytes)
        .map_err(|_| AddonError::Trust("signature is not base64 text".to_owned()))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(signature_text.trim())
        .map_err(|_| AddonError::Trust("signature is not valid base64".to_owned()))?;
    let signature = Signature::from_slice(&decoded)
        .map_err(|_| AddonError::Trust("signature has the wrong length".to_owned()))?;
    key.verify(payload, &signature)
        .map_err(|_| AddonError::Trust("catalog signature is invalid".to_owned()))
}

fn decode_public_key(value: &str) -> Result<VerifyingKey, AddonError> {
    if value.len() != 44 {
        return Err(AddonError::Trust(
            "public key is not canonical base64".to_owned(),
        ));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| AddonError::Trust("public key is not valid base64".to_owned()))?;
    if base64::engine::general_purpose::STANDARD.encode(&decoded) != value {
        return Err(AddonError::Trust(
            "public key is not canonical base64".to_owned(),
        ));
    }
    let bytes: [u8; 32] = decoded
        .try_into()
        .map_err(|_| AddonError::Trust("public key has the wrong length".to_owned()))?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|_| AddonError::Trust("public key is not valid Ed25519".to_owned()))
}

async fn download_catalog_index(
    client: &reqwest::Client,
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<CatalogIndexDownload, AddonError> {
    validate_https_url(url, "download")?;
    let mut request = client.get(url);
    let mut sent_validator = false;
    if let Some(value) = bounded_request_header(etag, MAX_ETAG_BYTES) {
        request = request.header(reqwest::header::IF_NONE_MATCH, value);
        sent_validator = true;
    }
    if let Some(value) = bounded_request_header(last_modified, MAX_LAST_MODIFIED_BYTES) {
        request = request.header(reqwest::header::IF_MODIFIED_SINCE, value);
        sent_validator = true;
    }
    let response = request
        .send()
        .await
        .map_err(|_| AddonError::Download("request could not be completed".to_owned()))?;
    validate_https_url(response.url().as_str(), "redirected download")?;
    let response_etag =
        bounded_response_header(response.headers(), &reqwest::header::ETAG, MAX_ETAG_BYTES);
    let response_last_modified = bounded_response_header(
        response.headers(),
        &reqwest::header::LAST_MODIFIED,
        MAX_LAST_MODIFIED_BYTES,
    );
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        if !sent_validator {
            return Err(AddonError::Download(
                "server returned not modified for an unconditional request".to_owned(),
            ));
        }
        return Ok(CatalogIndexDownload::NotModified {
            etag: response_etag,
            last_modified: response_last_modified,
        });
    }
    if !response.status().is_success() {
        return Err(AddonError::Download(format!(
            "server returned HTTP {}",
            response.status().as_u16()
        )));
    }
    let bytes = read_response_bounded(response, MAX_INDEX_BYTES).await?;
    Ok(CatalogIndexDownload::Modified {
        bytes,
        etag: response_etag,
        last_modified: response_last_modified,
    })
}

fn bounded_request_header(
    value: Option<&str>,
    limit: usize,
) -> Option<reqwest::header::HeaderValue> {
    value
        .filter(|value| !value.is_empty() && value.len() <= limit)
        .and_then(|value| reqwest::header::HeaderValue::from_str(value).ok())
}

fn bounded_response_header(
    headers: &reqwest::header::HeaderMap,
    name: &reqwest::header::HeaderName,
    limit: usize,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= limit)
        .map(str::to_owned)
}

async fn download_bounded(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
) -> Result<Vec<u8>, AddonError> {
    validate_https_url(url, "download")?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| AddonError::Download("request could not be completed".to_owned()))?;
    validate_https_url(response.url().as_str(), "redirected download")?;
    if !response.status().is_success() {
        return Err(AddonError::Download(format!(
            "server returned HTTP {}",
            response.status().as_u16()
        )));
    }
    read_response_bounded(response, limit).await
}

async fn read_response_bounded(
    response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, AddonError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AddonError::Download(
            "response exceeds its size limit".to_owned(),
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|_| AddonError::Download("response body was interrupted".to_owned()))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(AddonError::Download(
                "response exceeds its size limit".to_owned(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn extract_zip_safely(archive: &[u8], destination: &Path) -> Result<(), AddonError> {
    let reader = std::io::Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|_| AddonError::Package("package is not a valid ZIP archive".to_owned()))?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err(AddonError::Package(
            "package contains too many files".to_owned(),
        ));
    }
    let mut total_size = 0_u64;
    let mut portable_paths = BTreeSet::new();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|_| AddonError::Package("package entry is invalid".to_owned()))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| AddonError::Package("package path escapes archive root".to_owned()))?;
        let portable_path = enclosed
            .to_str()
            .ok_or_else(|| AddonError::Package("package path is not UTF-8".to_owned()))?
            .replace('\\', "/")
            .to_ascii_lowercase();
        if !portable_paths.insert(portable_path) {
            return Err(AddonError::Package(
                "package contains duplicate portable paths".to_owned(),
            ));
        }
        let mode = entry.unix_mode();
        if archive_entry_is_symlink(mode) {
            return Err(AddonError::Package(
                "package symlinks are not allowed".to_owned(),
            ));
        }
        let declared_size = entry.size();
        let remaining = MAX_EXTRACTED_BYTES.saturating_sub(total_size);
        if declared_size > remaining {
            return Err(AddonError::Package(
                "expanded package exceeds its size limit".to_owned(),
            ));
        }
        let output = destination.join(enclosed);
        if entry.is_dir() {
            std::fs::create_dir_all(&output).map_err(package_io_error)?;
            continue;
        }
        let parent = output
            .parent()
            .ok_or_else(|| AddonError::Package("package path has no parent".to_owned()))?;
        std::fs::create_dir_all(parent).map_err(package_io_error)?;
        let mut file = std::fs::File::create(&output).map_err(package_io_error)?;
        let copied = std::io::copy(
            &mut entry.by_ref().take(remaining.saturating_add(1)),
            &mut file,
        )
        .map_err(package_io_error)?;
        if copied != declared_size || copied > remaining {
            return Err(AddonError::Package(
                "package entry size does not match its archive metadata".to_owned(),
            ));
        }
        total_size = total_size.saturating_add(copied);
        file.flush().map_err(package_io_error)?;
        set_archive_permissions(&output, mode)?;
    }
    harden_extracted_directory_permissions(destination)?;
    Ok(())
}

fn archive_entry_is_symlink(mode: Option<u32>) -> bool {
    mode.is_some_and(|value| value & 0o170_000 == 0o120_000)
}

#[cfg(unix)]
fn set_archive_permissions(path: &Path, mode: Option<u32>) -> Result<(), AddonError> {
    use std::os::unix::fs::PermissionsExt as _;

    let safe_mode = if mode.unwrap_or_default() & 0o111 == 0 {
        0o444
    } else {
        0o555
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(safe_mode))
        .map_err(package_io_error)?;
    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_archive_permissions(_path: &Path, _mode: Option<u32>) -> Result<(), AddonError> {
    Ok(())
}

#[cfg(unix)]
fn harden_extracted_directory_permissions(path: &Path) -> Result<(), AddonError> {
    use std::os::unix::fs::PermissionsExt as _;

    for entry in std::fs::read_dir(path).map_err(package_io_error)? {
        let entry = entry.map_err(package_io_error)?;
        if entry.file_type().map_err(package_io_error)?.is_dir() {
            harden_extracted_directory_permissions(&entry.path())?;
        }
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(package_io_error)
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn harden_extracted_directory_permissions(_path: &Path) -> Result<(), AddonError> {
    Ok(())
}

fn validate_https_url(value: &str, kind: &str) -> Result<(), AddonError> {
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| AddonError::Validation(format!("{kind} URL is invalid")))?;
    let test_loopback_http = cfg!(test)
        && parsed.scheme() == "http"
        && parsed
            .host_str()
            .and_then(|host| host.parse::<std::net::IpAddr>().ok())
            .is_some_and(|address| address.is_loopback());
    if (parsed.scheme() != "https" && !test_loopback_http)
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(AddonError::Validation(format!(
            "{kind} URL must use HTTPS without userinfo"
        )));
    }
    Ok(())
}

fn current_rid() -> Result<&'static str, AddonError> {
    ersatzrs_ffmpeg::current_ersatzrs_ffmpeg_rid().ok_or_else(|| {
        AddonError::Validation("current platform has no supported ErsatzRS RID".to_owned())
    })
}

fn host_version() -> Result<Version, AddonError> {
    Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| AddonError::Validation("host version is invalid".to_owned()))
}

fn localized_name(addon: &CatalogAddon) -> &str {
    addon
        .name
        .get("en-US")
        .map_or(addon.id.as_str(), String::as_str)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn check_tool_update_is_safe(
    context: &AddonToolUpdateWarning,
    environment: &BTreeMap<String, String>,
) -> bool {
    context.validate().is_ok()
        && [
            &context.program,
            &context.installed_version,
            &context.latest_version,
        ]
        .iter()
        .all(|value| redact_and_bound(value, environment, 128) == **value)
}

fn redact_and_bound(value: &str, environment: &BTreeMap<String, String>, limit: usize) -> String {
    redact_and_bound_with_values(value, environment, &[], limit)
}

fn redact_and_bound_with_values(
    value: &str,
    environment: &BTreeMap<String, String>,
    sensitive_values: &[String],
    limit: usize,
) -> String {
    if environment
        .keys()
        .any(|key| key.starts_with("ERSATZRS_ADDON_SECRET_FILE_"))
    {
        return bounded_text(FILE_SECRET_DIAGNOSTIC, limit);
    }
    let mut redactions = environment
        .iter()
        .filter(|(key, value)| {
            (key.starts_with("ERSATZRS_ADDON_SECRET_")
                || key.ends_with("_DIR")
                || key.ends_with("_URL"))
                && !value.is_empty()
        })
        .map(|(_, value)| value.as_str())
        .collect::<Vec<_>>();
    redactions.extend(
        sensitive_values
            .iter()
            .map(String::as_str)
            .filter(|value| !value.is_empty()),
    );
    redactions.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
    redactions.dedup();
    let redacted = redactions.iter().fold(value.to_owned(), |safe, secret| {
        safe.replace(secret, "[redacted]")
    });
    let safe = redacted
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    bounded_text(&safe, limit)
}

fn bounded_text(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end].to_owned()
}

#[allow(clippy::needless_pass_by_value)] // `map_err` supplies the owned error.
fn database_error(error: sqlx::Error) -> AddonError {
    AddonError::Database(error.to_string())
}

#[allow(clippy::needless_pass_by_value)] // `map_err` supplies the owned error.
fn repository_error(error: ersatzrs_core::repositories::RepositoryError) -> AddonError {
    AddonError::Database(error.to_string())
}

#[allow(clippy::needless_pass_by_value)] // `map_err` supplies the owned error.
fn package_io_error(error: std::io::Error) -> AddonError {
    AddonError::Package(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};
    use ersatzrs_addon_contract::{
        AddonCapability, AddonDependency, AddonEntrypoint, AddonPackageIcon, AddonPermissions,
        ADDON_MANIFEST_VERSION,
    };
    use httpmock::{Method::GET, MockServer};
    use std::io::Cursor;

    fn localized(value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("en-US".to_owned(), value.to_owned())])
    }

    #[test]
    fn check_tool_update_context_respects_diagnostic_redaction() {
        let context = AddonToolUpdateWarning {
            program: "Example Tool".to_owned(),
            installed_version: "1.0".to_owned(),
            latest_version: "2.0".to_owned(),
        };
        assert!(check_tool_update_is_safe(&context, &BTreeMap::new()));
        for (key, value) in [
            ("ERSATZRS_ADDON_SECRET_fixture", "Example"),
            ("ERSATZRS_ADDON_SECRET_fixture", "1.0"),
            (
                "ERSATZRS_ADDON_SECRET_FILE_fixture",
                "fixture-secret-reference",
            ),
            ("ERSATZRS_ADDON_DATA_DIR", "Example"),
        ] {
            let environment = BTreeMap::from([(key.to_owned(), value.to_owned())]);
            assert!(!check_tool_update_is_safe(&context, &environment));
        }
    }

    // POSIX fixture entrypoints intentionally run only on Unix.
    #[cfg(unix)]
    #[tokio::test]
    async fn check_tool_update_context_persists_and_clears_with_readiness() {
        let temp = tempfile::tempdir().unwrap();
        let pool = crate::db::init(&format!(
            "sqlite:{}",
            temp.path().join("addons.db").display()
        ))
        .await
        .unwrap();
        let package = temp.path().join("package");
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("addon.sh"), br#"test "$ERSATZRS_ADDON_CHECK_CONTEXT_VERSION" = "1" || exit 1
printf '%s' '{"status":"warning","code":"tool-update-failed-stale","message":"Still usable","tool_update":{"program":"Example Tool","installed_version":"1.0","latest_version":"2.0"}}'
"#).unwrap();
        let manifest = fixture_manifest("org.ersatzrs.addon.fixture");
        insert_package_installation(&pool, &manifest, &package, false).await;
        let result = check_addon(&pool, &manifest.id, &AddonRuntimeContext::default())
            .await
            .unwrap();
        assert!(result.tool_update.is_some());
        let installation = load_installed_addon(&pool, &manifest.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(installation.last_check_tool_update, result.tool_update);
        disable_addon(&pool, &manifest.id).await.unwrap();
        let installation = load_installed_addon(&pool, &manifest.id)
            .await
            .unwrap()
            .unwrap();
        assert!(installation.last_check_tool_update.is_none());
        assert!(installation.last_check_code.is_none());
    }

    fn fixture_manifest(id: &str) -> AddonManifest {
        AddonManifest {
            manifest_version: ADDON_MANIFEST_VERSION,
            id: id.to_owned(),
            version: Version::new(1, 0, 0),
            name: localized("Fixture"),
            summary: localized("Fixture add-on"),
            icon: None,
            license: "GPL-3.0-or-later".to_owned(),
            source_url: "https://example.test/source".to_owned(),
            host_version: semver::VersionReq::STAR,
            capabilities: vec![AddonCapability {
                id: CAPABILITY_ADDON_CHECK_V1.to_owned(),
                options: Vec::new(),
            }],
            dependencies: Vec::new(),
            entrypoints: BTreeMap::from([(
                "any".to_owned(),
                AddonEntrypoint {
                    path: "addon.sh".to_owned(),
                    kind: AddonEntrypointKind::PosixShell,
                },
            )]),
            permissions: AddonPermissions::default(),
            settings: Vec::new(),
            media_list_storage: None,
            default_source_hosts: Vec::new(),
            panels: Vec::new(),
        }
    }

    async fn insert_fixture_installation(pool: &DbPool, manifest: &AddonManifest, enabled: bool) {
        let manifest_json = serde_json::to_string(manifest).expect("serialize fixture manifest");
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, ?, ?, 'package', 'content', ?, 'fixture-install', 'now', 'now')"#,
        )
        .bind(&manifest.id)
        .bind(manifest.version.to_string())
        .bind(i64::from(enabled))
        .bind(manifest_json)
        .execute(pool)
        .await
        .expect("insert fixture installation");
    }

    /// Insert an installation backed by a real package directory, so the
    /// readiness path can verify its content hash and run its entrypoint.
    #[cfg(unix)]
    async fn insert_package_installation(
        pool: &DbPool,
        manifest: &AddonManifest,
        package: &Path,
        enabled: bool,
    ) {
        let manifest_json = serde_json::to_string(manifest).expect("serialize fixture manifest");
        let content_sha256 = hash_addon_tree_sync(package).expect("hash fixture package");
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision"
               ) VALUES (?, ?, ?, 'package', ?, ?, ?, 'now', 'now', ?)"#,
        )
        .bind(&manifest.id)
        .bind(manifest.version.to_string())
        .bind(i64::from(enabled))
        .bind(content_sha256)
        .bind(manifest_json)
        .bind(package.to_string_lossy().into_owned())
        .bind(Uuid::new_v4().to_string())
        .execute(pool)
        .await
        .expect("insert fixture installation");
    }

    #[test]
    fn detached_signature_covers_exact_catalog_bytes() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let payload = br#"{"schema_version":1}"#;
        let signature = signing_key.sign(payload);
        let public_key = base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().as_bytes());
        let signature = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
        verify_detached_signature(&public_key, payload, signature.as_bytes()).expect("valid");
        assert!(verify_detached_signature(&public_key, b"changed", signature.as_bytes()).is_err());
    }

    #[test]
    fn zip_extraction_rejects_parent_traversal() {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("../escape", options)
                .expect("start fixture entry");
            writer.write_all(b"nope").expect("write fixture entry");
            writer.finish().expect("finish fixture archive");
        }
        let temp = tempfile::tempdir().expect("temp directory");
        assert!(extract_zip_safely(bytes.get_ref(), temp.path()).is_err());
        assert!(!temp.path().join("escape").exists());
    }

    #[test]
    fn zip_extraction_rejects_case_colliding_portable_paths() {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("Addon.toml", options)
                .expect("start first fixture entry");
            writer.write_all(b"first").expect("write first entry");
            writer
                .start_file("addon.toml", options)
                .expect("start second fixture entry");
            writer.write_all(b"second").expect("write second entry");
            writer.finish().expect("finish fixture archive");
        }
        let temp = tempfile::tempdir().expect("temp directory");

        assert!(extract_zip_safely(bytes.get_ref(), temp.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn zip_extraction_strips_all_write_bits_from_package_files() {
        use std::os::unix::fs::PermissionsExt as _;

        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default().unix_permissions(0o777);
            writer
                .start_file("addon.sh", options)
                .expect("start executable fixture");
            writer.write_all(b"#!/bin/sh\n").expect("write fixture");
            writer.finish().expect("finish fixture archive");
        }
        let temp = tempfile::tempdir().expect("temp directory");

        extract_zip_safely(bytes.get_ref(), temp.path()).expect("extract fixture");

        let mode = std::fs::metadata(temp.path().join("addon.sh"))
            .expect("fixture metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o555);
    }

    #[test]
    fn repeated_catalog_sequence_requires_identical_bytes() {
        validate_repeated_catalog_sequence(Some(7), Some("catalog"), 7, "catalog")
            .expect("identical refresh");
        assert!(
            validate_repeated_catalog_sequence(Some(7), Some("catalog"), 7, "changed").is_err()
        );
        validate_repeated_catalog_sequence(Some(7), Some("catalog"), 8, "changed")
            .expect("higher sequence");
    }

    #[test]
    fn secret_setting_rejects_raw_values() {
        let definition = ersatzrs_addon_contract::AddonSettingDefinition {
            key: "API_TOKEN".to_owned(),
            label: localized("API token"),
            kind: AddonSettingKind::SecretReference,
            required: true,
            default: None,
            options: Vec::new(),
            suggested_reference: None,
        };
        let value = AddonSettingValue {
            key: "API_TOKEN".to_owned(),
            value: Some("secret".to_owned()),
            reference_kind: None,
            reference: None,
        };
        assert!(validate_setting_value(&definition, &value).is_err());
    }

    #[test]
    fn missing_required_settings_use_stable_operation_codes() {
        for (kind, expected) in [
            (AddonSettingKind::String, OPERATION_CODE_MISSING_SETTING),
            (
                AddonSettingKind::SecretReference,
                OPERATION_CODE_MISSING_SECRET,
            ),
        ] {
            let mut manifest = fixture_manifest("org.ersatzrs.addon.fixture");
            manifest
                .settings
                .push(ersatzrs_addon_contract::AddonSettingDefinition {
                    key: "TOKEN".to_owned(),
                    label: localized("Credential"),
                    kind,
                    required: true,
                    default: None,
                    options: Vec::new(),
                    suggested_reference: None,
                });
            let error = resolve_setting_environment(
                &manifest,
                "fixture",
                &[],
                &AddonRuntimeContext::default(),
            )
            .expect_err("required setting must fail");
            let AddonError::Operation(diagnostic) = error else {
                panic!("expected structured operation error");
            };
            assert_eq!(diagnostic.code, expected);
            assert!(!diagnostic.message.contains("TOKEN"));
        }
    }

    #[test]
    fn managed_tool_paths_require_manifest_permission() {
        let mut manifest = fixture_manifest("org.ersatzrs.addon.fixture");
        let runtime = AddonRuntimeContext {
            ffmpeg_path: Some("managed-ffmpeg".to_owned()),
            ffprobe_path: Some("managed-ffprobe".to_owned()),
        };

        let denied = resolve_setting_environment(&manifest, "fixture", &[], &runtime)
            .expect("manifest without managed tool access");
        assert!(!denied.contains_key("FFMPEG_BIN"));
        assert!(!denied.contains_key("FFPROBE_BIN"));

        manifest.permissions.managed_ffmpeg = true;
        let allowed = resolve_setting_environment(&manifest, "fixture", &[], &runtime)
            .expect("manifest with managed tool access");
        assert_eq!(
            allowed.get("FFMPEG_BIN").map(String::as_str),
            Some("managed-ffmpeg")
        );
        assert_eq!(
            allowed.get("FFPROBE_BIN").map(String::as_str),
            Some("managed-ffprobe")
        );
        assert!(resolve_setting_environment(
            &manifest,
            "fixture",
            &[],
            &AddonRuntimeContext::default(),
        )
        .is_err());
    }

    #[test]
    fn installed_content_hash_changes_with_package_bytes() {
        let temp = tempfile::tempdir().expect("temp directory");
        std::fs::write(temp.path().join("addon.toml"), b"first").expect("write fixture");
        let first = hash_addon_tree_sync(temp.path()).expect("hash fixture");
        std::fs::write(temp.path().join("addon.toml"), b"second").expect("change fixture");
        let second = hash_addon_tree_sync(temp.path()).expect("rehash fixture");

        assert_ne!(first, second);
    }

    #[test]
    fn installed_content_hash_frames_each_file_length() {
        let first = tempfile::tempdir().expect("first temp directory");
        std::fs::write(first.path().join("a"), b"X").expect("write first a");
        std::fs::write(first.path().join("b"), b"Y").expect("write first b");
        let second = tempfile::tempdir().expect("second temp directory");
        let mut ambiguous_without_lengths = Vec::from(&b"X\0"[..]);
        ambiguous_without_lengths.extend_from_slice(&1_u64.to_le_bytes());
        ambiguous_without_lengths.extend_from_slice(b"bY");
        std::fs::write(second.path().join("a"), ambiguous_without_lengths).expect("write second a");

        let first_hash = hash_addon_tree_sync(first.path()).expect("hash first tree");
        let second_hash = hash_addon_tree_sync(second.path()).expect("hash second tree");

        assert_ne!(first_hash, second_hash);
    }

    #[test]
    fn installed_content_hash_accepts_a_non_canonical_root() {
        // Packages are staged and installed under plain, uncanonicalized paths;
        // hashing must not mistake the root's own spelling for a path escape.
        let temp = tempfile::tempdir().expect("temp directory");
        let content = temp.path().join("content");
        std::fs::create_dir(&content).expect("create content directory");
        std::fs::create_dir(temp.path().join("sibling")).expect("create sibling directory");
        std::fs::write(content.join("addon.toml"), b"fixture").expect("write fixture");

        let canonical = hash_addon_tree_sync(&content).expect("hash canonical root");
        let indirect =
            hash_addon_tree_sync(&temp.path().join("sibling").join("..").join("content"))
                .expect("hash non-canonical root");

        assert_eq!(canonical, indirect);
    }

    #[test]
    fn installed_content_hash_accepts_a_deeply_nested_file() {
        // A file past MAX_PATH resolves to the verbatim `\\?\` form on Windows
        // while its shorter package root does not, and a containment check that
        // mixes the two forms reports the package as escaping itself. Windows
        // may refuse to create such a tree at all, which is a different failure
        // and not this test's subject, so the test skips rather than fails when
        // the filesystem will not hold the fixture.
        let temp = tempfile::tempdir().expect("temp directory");
        let content = temp.path().join("content");
        let mut nested = content.clone();
        for _ in 0..8 {
            nested = nested.join("a-directory-name-long-enough-to-pass-max-path");
        }
        if std::fs::create_dir_all(&nested).is_err()
            || std::fs::write(nested.join("addon.toml"), b"fixture").is_err()
        {
            return;
        }

        hash_addon_tree_sync(&content).expect("hash deeply nested tree");
    }

    #[cfg(unix)]
    #[test]
    fn installed_content_hash_accepts_a_symlinked_root() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp directory");
        let content = temp.path().join("content");
        std::fs::create_dir(&content).expect("create content directory");
        std::fs::write(content.join("addon.toml"), b"fixture").expect("write fixture");
        let alias = temp.path().join("alias");
        symlink(&content, &alias).expect("create fixture symlink");

        let canonical = hash_addon_tree_sync(&content).expect("hash canonical root");
        let through_link = hash_addon_tree_sync(&alias).expect("hash symlinked root");

        assert_eq!(canonical, through_link);
    }

    #[cfg(unix)]
    #[test]
    fn installed_content_hash_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp directory");
        std::fs::write(temp.path().join("addon.toml"), b"fixture").expect("write fixture");
        symlink(temp.path().join("addon.toml"), temp.path().join("alias"))
            .expect("create fixture symlink");

        assert!(hash_addon_tree_sync(temp.path()).is_err());
    }

    #[test]
    fn windows_batch_arguments_keep_the_package_path_quoted() {
        let command = ResolvedAddonCommand {
            tool_paths: Vec::new(),
            addon_id: "org.ersatzrs.addon.fixture".to_owned(),
            program: r"C:\Program Files\ErsatzRS Add-ons\addon.bat".to_owned(),
            args: vec!["check".to_owned()],
            working_directory: PathBuf::new(),
            environment: BTreeMap::new(),
            state_revision: "fixture".to_owned(),
        };

        assert_eq!(
            windows_addon_raw_arguments(&command),
            r#"/D /S /C ""C:\Program Files\ErsatzRS Add-ons\addon.bat" "check"""#
        );
    }

    #[test]
    fn addon_process_environment_preserves_only_platform_allowlisted_values() {
        // Synthetic values avoid inspecting profiles or mutating process-global
        // environment while other tests run.
        let variables = BTreeMap::from([
            ("PATH", "fixture-bin"),
            ("APPDATA", "fixture-roaming"),
            ("LOCALAPPDATA", "fixture-local"),
            ("UNDECLARED_FIXTURE_VALUE", "must-not-reach-the-addon"),
        ]);
        let environment = addon_process_environment_with(|name| {
            variables.get(name).copied().map(std::ffi::OsString::from)
        });
        assert_eq!(
            environment.get("PATH").map(String::as_str),
            Some("fixture-bin")
        );
        assert!(!environment.contains_key("UNDECLARED_FIXTURE_VALUE"));
        #[cfg(windows)]
        {
            assert_eq!(
                environment.get("APPDATA").map(String::as_str),
                Some("fixture-roaming")
            );
            assert_eq!(
                environment.get("LOCALAPPDATA").map(String::as_str),
                Some("fixture-local")
            );
            assert_eq!(environment.len(), 3);
        }
        #[cfg(not(windows))]
        assert_eq!(environment.len(), 1);
    }

    #[test]
    fn diagnostic_redaction_handles_multiline_environment_secrets() {
        let environment = BTreeMap::from([(
            "ERSATZRS_ADDON_SECRET_TOKEN".to_owned(),
            "first-line\nsecond-line".to_owned(),
        )]);

        let redacted = redact_and_bound("failure: first-line\nsecond-line", &environment, 1_024);

        assert_eq!(redacted, "failure: [redacted]");
        assert!(!redacted.contains("first-line"));
        assert!(!redacted.contains("second-line"));
    }

    #[test]
    fn file_backed_secrets_suppress_add_on_controlled_diagnostics() {
        let environment = BTreeMap::from([(
            "ERSATZRS_ADDON_SECRET_FILE_TOKEN".to_owned(),
            "/run/secrets/token".to_owned(),
        )]);

        let redacted = redact_and_bound("raw file secret echoed by add-on", &environment, 1_024);

        assert_eq!(redacted, FILE_SECRET_DIAGNOSTIC);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_group_exit_requires_more_than_reaping_its_leader() {
        let mut process = Command::new("/bin/sh");
        process
            .args(["-c", "sleep 30 >/dev/null 2>&1 & exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_managed_addon_process(process).unwrap();
        let group = child.process_group;
        // Observe without reaping; cleanup must retain the original group
        // identity even after its leader has exited.
        tokio::time::timeout(Duration::from_secs(5), observe_process_leader_exit(group))
            .await
            .unwrap()
            .unwrap();
        assert!(nix::sys::signal::killpg(group, None).is_ok());
        assert!(tokio::time::timeout(
            Duration::from_millis(75),
            wait_for_process_group_exit(group),
        )
        .await
        .is_err());
        child.kill().await.unwrap();
        assert_eq!(
            nix::sys::signal::killpg(group, None),
            Err(nix::errno::Errno::ESRCH)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_group_leader_observation_does_not_reap_before_kill() {
        use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};

        let mut process = Command::new("/bin/sh");
        process
            .args(["-c", "exit 7"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_managed_addon_process(process).unwrap();
        let group = child.process_group;
        tokio::time::timeout(Duration::from_secs(5), observe_process_leader_exit(group))
            .await
            .unwrap()
            .unwrap();
        let leader = Pid::from_raw(group.as_raw()).unwrap();
        // A second observation succeeds only while the leader is still waitable.
        let status = waitid(
            WaitId::Pid(leader),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        )
        .unwrap()
        .expect("unreaped leader");
        assert!(status.exited());
        assert!(child.terminate_on_drop);
        assert!(nix::sys::signal::killpg(group, None).is_ok());
        let status = child.wait().await.unwrap();
        assert_eq!(status.code(), Some(7));
        assert_eq!(
            waitid(
                WaitId::Pid(leader),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG
            )
            .unwrap_err(),
            rustix::io::Errno::CHILD
        );
        assert_eq!(child.wait().await.unwrap().code(), Some(7));
    }

    // Linux-specific subreaper behavior is isolated in a fresh test process,
    // so this process-wide setting cannot affect concurrently running tests.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[allow(clippy::disallowed_methods)] // Test-only flag separates the isolated subreaper child from its parent.
    async fn managed_group_exit_reaps_adopted_descendants() {
        const HELPER: &str = "ERSATZRS_TEST_ADOPTED_CHILD_REAPER";
        if std::env::var_os(HELPER).is_none() {
            let mut helper = Command::new(std::env::current_exe().unwrap());
            helper
                .args([
                    "--exact",
                    "addons::tests::managed_group_exit_reaps_adopted_descendants",
                    "--nocapture",
                ])
                .env(HELPER, "1")
                .kill_on_drop(true);
            let output = tokio::time::timeout(Duration::from_secs(15), helper.output())
                .await
                .unwrap()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated subreaper regression failed"
            );
            return;
        }
        nix::sys::prctl::set_child_subreaper(true).unwrap();
        let mut process = Command::new("/bin/sh");
        process
            .args(["-c", "sleep 30 >/dev/null 2>&1 & exit 0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_managed_addon_process(process).unwrap();
        let group = child.process_group;
        assert!(tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success());
        assert_eq!(
            nix::sys::signal::killpg(group, None),
            Err(nix::errno::Errno::ESRCH)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timed_out_add_on_kills_its_descendant_process_group() {
        let temp = tempfile::tempdir().expect("temp directory");
        let marker = temp.path().join("orphan-marker");
        let command = ResolvedAddonCommand {
            tool_paths: Vec::new(),
            addon_id: "org.ersatzrs.addon.fixture".to_owned(),
            program: "/bin/sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                "(sleep 1; printf orphan > \"$1\") & wait".to_owned(),
                "addon".to_owned(),
                marker.to_string_lossy().into_owned(),
            ],
            working_directory: temp.path().to_path_buf(),
            environment: BTreeMap::new(),
            state_revision: "fixture".to_owned(),
        };

        let result = run_bounded_command(&command, Duration::from_millis(100), 1_024).await;
        assert!(result.is_err());
        tokio::time::sleep(Duration::from_millis(1_200)).await;
        assert!(
            !marker.exists(),
            "descendant survived the process-group kill"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn successful_add_on_leader_cannot_leave_a_descendant_running() {
        let temp = tempfile::tempdir().expect("temp directory");
        let marker = temp.path().join("background-marker");
        let command = ResolvedAddonCommand {
            tool_paths: Vec::new(),
            addon_id: "org.ersatzrs.addon.fixture".to_owned(),
            program: "/bin/sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                "(sleep 1; printf orphan > \"$1\") & exit 0".to_owned(),
                "addon".to_owned(),
                marker.to_string_lossy().into_owned(),
            ],
            working_directory: temp.path().to_path_buf(),
            environment: BTreeMap::new(),
            state_revision: "fixture".to_owned(),
        };

        run_bounded_command(&command, Duration::from_secs(5), 1_024)
            .await
            .expect("leader should complete cleanly");
        tokio::time::sleep(Duration::from_millis(1_200)).await;
        assert!(
            !marker.exists(),
            "background descendant survived leader exit"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelled_add_on_invocation_reaps_leader_before_returning() {
        let temp = tempfile::tempdir().expect("temp directory");
        let ready = temp.path().join("invocation-ready");
        let marker = temp.path().join("cancelled-marker");
        let command = ResolvedAddonCommand {
            tool_paths: Vec::new(),
            addon_id: "org.ersatzrs.addon.fixture".to_owned(),
            program: "/bin/sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                "printf '%s' \"$$\" > \"$2\"; (sleep 1; printf orphan > \"$1\") & wait".to_owned(),
                "addon".to_owned(),
                marker.to_string_lossy().into_owned(),
                ready.to_string_lossy().into_owned(),
            ],
            working_directory: temp.path().to_path_buf(),
            environment: BTreeMap::new(),
            state_revision: "fixture".to_owned(),
        };
        let (cancel, cancelled) = tokio::sync::oneshot::channel();
        let invocation = tokio::spawn(async move {
            super::run_bounded_command_with_input_cancellable(
                &command,
                Duration::from_secs(5),
                1_024,
                b"",
                &[],
                cancelled,
            )
            .await
        });
        let pid = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(pid) = tokio::fs::read_to_string(&ready).await {
                    if !pid.is_empty() {
                        break pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("invocation started");
        cancel.send(()).expect("request cancellation");
        let result = invocation.await.expect("runner completed");
        assert!(
            matches!(result, Err(AddonError::Process(message)) if message == "entrypoint cancelled")
        );
        let alive = std::process::Command::new("/bin/sh")
            .args(["-c", "kill -0 \"$1\" 2>/dev/null", "probe", &pid])
            .status()
            .expect("probe synthetic process");
        assert!(!alive.success(), "cancelled leader was not reaped");
        tokio::time::sleep(Duration::from_millis(1_200)).await;
        assert!(!marker.exists(), "cancelled descendant kept writing");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn aborted_add_on_invocation_kills_its_descendant_process_group() {
        let temp = tempfile::tempdir().expect("temp directory");
        let marker = temp.path().join("aborted-marker");
        let ready = temp.path().join("invocation-ready");
        let command = ResolvedAddonCommand {
            tool_paths: Vec::new(),
            addon_id: "org.ersatzrs.addon.fixture".to_owned(),
            program: "/bin/sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                "printf ready > \"$2\"; (sleep 1; printf orphan > \"$1\") & wait".to_owned(),
                "addon".to_owned(),
                marker.to_string_lossy().into_owned(),
                ready.to_string_lossy().into_owned(),
            ],
            working_directory: temp.path().to_path_buf(),
            environment: BTreeMap::new(),
            state_revision: "fixture".to_owned(),
        };
        let invocation = tokio::spawn(async move {
            run_bounded_command(&command, Duration::from_secs(5), 1_024).await
        });

        tokio::time::timeout(Duration::from_secs(2), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("add-on invocation should start");
        invocation.abort();
        assert!(invocation.await.is_err());

        tokio::time::sleep(Duration::from_millis(1_200)).await;
        assert!(
            !marker.exists(),
            "descendant survived invocation task abort"
        );
    }

    #[test]
    fn fixture_manifest_requires_the_check_capability() {
        let manifest = fixture_manifest("org.ersatzrs.addon.fixture");
        manifest.validate().expect("valid manifest");
    }

    #[test]
    fn only_host_supported_capability_operations_can_dispatch() {
        for (capability, operation) in [
            (CAPABILITY_MEDIA_LIST_ENRICH_V1, "enrich-options"),
            (CAPABILITY_MEDIA_LIST_ACCESS_CHECK_V1, "test-access"),
            (CAPABILITY_MEDIA_LIST_ITEM_RUNTIME_V1, "runtime-info"),
            (CAPABILITY_MEDIA_LIST_PREPARE_V1, "prepare"),
            (CAPABILITY_MEDIA_LIST_PREPARE_V2, "prepare"),
            (CAPABILITY_MEDIA_LIST_SUBTITLES_V1, "subtitles"),
            (CAPABILITY_REMOTE_STREAM_PLAY_V3, "play"),
            (CAPABILITY_REMOTE_STREAM_PLAY_V4, "play-intervals"),
        ] {
            assert!(validate_supported_operation(capability, operation).is_ok());
            assert!(validate_supported_operation(capability, "discover").is_err());
        }
        assert!(
            validate_supported_operation(CAPABILITY_MEDIA_LIST_IMPORT_V2, "enrich-options")
                .is_err()
        );
        assert!(validate_supported_operation(CAPABILITY_ADDON_CHECK_V1, "check").is_ok());
        assert!(validate_supported_operation(CAPABILITY_MEDIA_LIST_LIST_V1, "list").is_ok());
        assert!(
            validate_supported_operation(CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1, "check").is_ok()
        );
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_LIST_V1, "list").is_ok());
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_PLAY_V1, "play").is_ok());
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_PLAY_V2, "play").is_ok());
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_PLAY_V1, "list").is_err());
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_PLAY_V3, "resolve").is_err());
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_PLAY_V4, "play").is_err());
    }

    #[tokio::test]
    async fn unverified_repository_requires_explicit_opt_in() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let repository = TrustedAddonRepository {
            id: "org.example.addons".to_owned(),
            name: "Example add-ons".to_owned(),
            index_url: "https://example.test/index-v2.json".to_owned(),
            public_key: String::new(),
            is_official: false,
            is_enabled: true,
            last_sequence: None,
            last_refresh: None,
            last_error: None,
        };

        assert!(!unverified_sources_enabled(&pool).await.expect("load gate"));
        assert!(save_trusted_repository(&pool, &repository).await.is_err());
        set_unverified_sources_enabled(&pool, true)
            .await
            .expect("enable gate");
        save_trusted_repository(&pool, &repository)
            .await
            .expect("save unverified repository");
        assert_eq!(
            load_repository_row(&pool, &repository.id)
                .await
                .expect("load repository")
                .expect("repository row")
                .trust_tier(),
            AddonTrustTier::Unverified
        );
    }

    #[tokio::test]
    async fn local_packages_require_explicit_opt_in_before_archive_processing() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");

        let addon_error = install_addon_from_package(
            &pool,
            b"not a zip".to_vec(),
            &AddonRuntimeContext::default(),
        )
        .await
        .expect_err("sideload must be gated");
        assert!(addon_error.to_string().contains("unverified sources"));

        let repository_error = install_local_repository_bundle(&pool, b"not a zip".to_vec())
            .await
            .expect_err("repository package must be gated");
        assert!(repository_error.to_string().contains("unknown sources"));
    }

    #[tokio::test]
    async fn local_repository_scan_replaces_offers_only_after_full_validation() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let repository_root = temp.path().join("repository");
        let addon_id = "org.ersatzrs.addon.local-fixture";
        let addon_root = repository_root.join("addons").join(addon_id);
        std::fs::create_dir_all(&addon_root).expect("create local repository");
        let repository = LocalRepositoryManifest {
            schema_version: ersatzrs_addon_contract::LOCAL_ADDON_REPOSITORY_SCHEMA_VERSION,
            id: "org.ersatzrs.repository.fixture".to_owned(),
            name: "Fixture repository".to_owned(),
            description: localized("Fixture local repository"),
        };
        std::fs::write(
            repository_root.join("repository.toml"),
            toml::to_string(&repository).expect("serialize repository"),
        )
        .expect("write repository manifest");
        let mut addon = fixture_manifest(addon_id);
        addon.icon = Some(AddonPackageIcon {
            path: "icon.png".to_owned(),
            media_type: AddonIconMediaType::Png,
        });
        std::fs::write(
            addon_root.join("addon.toml"),
            toml::to_string(&addon).expect("serialize add-on"),
        )
        .expect("write add-on manifest");
        std::fs::write(addon_root.join("addon.sh"), b"#!/bin/sh\n").expect("write entrypoint");
        let icon_bytes = b"\x89PNG\r\n\x1a\nfixture";
        std::fs::write(addon_root.join("icon.png"), icon_bytes).expect("write icon");

        scan_local_repository_root(&pool, &repository_root, "checkout")
            .await
            .expect("scan local repository");
        let sources = list_local_addon_sources(&pool)
            .await
            .expect("list local sources");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, repository.id);
        assert_eq!(sources[0].kind, "checkout");
        let offers = list_local_available_addons(&pool)
            .await
            .expect("list local offers");
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].repository_id, repository.id);
        assert_eq!(offers[0].addon.id, addon_id);
        assert!(offers[0].has_local_icon);
        let icon = load_local_addon_icon(&pool, &repository.id, addon_id)
            .await
            .expect("load local icon")
            .expect("local icon");
        assert_eq!(icon.bytes, icon_bytes);
        assert_eq!(icon.media_type, AddonIconMediaType::Png);

        std::fs::write(addon_root.join("addon.toml"), b"invalid = true\n")
            .expect("break add-on manifest");
        scan_local_repository_root(&pool, &repository_root, "checkout")
            .await
            .expect_err("invalid rescan must fail");
        let preserved = list_local_available_addons(&pool)
            .await
            .expect("list preserved offers");
        assert_eq!(preserved.len(), 1);
        assert_eq!(preserved[0].addon.id, addon_id);
        assert!(load_local_addon_icon(&pool, &repository.id, addon_id)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn removing_bundle_source_preserves_installed_snapshot_and_cleans_offers() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let bundle_root = temp.path().join("repository-bundles");
        let repository_id = "org.ersatzrs.repository.bundle-fixture";
        let source_root = bundle_root.join(repository_id).join("0123456789abcdef");
        let addon_id = "org.ersatzrs.addon.bundle-fixture";
        tokio::fs::create_dir_all(source_root.join("addons").join(addon_id))
            .await
            .expect("create bundle source");
        tokio::fs::write(source_root.join("repository.toml"), b"fixture")
            .await
            .expect("write bundle marker");
        sqlx::query(
            r#"INSERT INTO "AddonSource" (
                   "Id", "Name", "Kind", "RootPath", "ManifestJson", "ContentSha256",
                   "LastScan", "LastError", "StateRevision"
               ) VALUES (?, 'Bundle fixture', 'bundle', ?, '{}', 'digest', 'now', NULL, 1)"#,
        )
        .bind(repository_id)
        .bind(source_root.to_string_lossy().into_owned())
        .execute(&pool)
        .await
        .expect("insert bundle source");
        sqlx::query(
            r#"INSERT INTO "AddonSourceOffer" (
                   "SourceId", "AddonId", "Version", "ManifestJson", "RelativePath",
                   "ContentSha256"
               ) VALUES (?, ?, '1.0.0', '{}', ?, 'digest')"#,
        )
        .bind(repository_id)
        .bind(addon_id)
        .bind(format!("addons/{addon_id}"))
        .execute(&pool)
        .await
        .expect("insert bundle offer");
        let manifest = fixture_manifest(addon_id);
        insert_fixture_installation(&pool, &manifest, true).await;
        sqlx::query(r#"UPDATE "AddonInstallation" SET "SourceId" = ? WHERE "AddonId" = ?"#)
            .bind(repository_id)
            .bind(addon_id)
            .execute(&pool)
            .await
            .expect("attach installation provenance");
        sqlx::query(
            r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value")
               VALUES (?, 'DISPLAY_LABEL', 'Preserved value')"#,
        )
        .bind(addon_id)
        .execute(&pool)
        .await
        .expect("insert installed setting");

        remove_local_repository_bundle_from(&pool, repository_id, &bundle_root)
            .await
            .expect("remove bundle source");

        let sources =
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "AddonSource" WHERE "Id" = ?"#)
                .bind(repository_id)
                .fetch_one(&pool)
                .await
                .expect("count sources");
        let offers = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "AddonSourceOffer" WHERE "SourceId" = ?"#,
        )
        .bind(repository_id)
        .fetch_one(&pool)
        .await
        .expect("count offers");
        let installation_source = sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT "SourceId" FROM "AddonInstallation" WHERE "AddonId" = ?"#,
        )
        .bind(addon_id)
        .fetch_one(&pool)
        .await
        .expect("load installation provenance");
        let setting = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "AddonSetting" WHERE "AddonId" = ? AND "Key" = 'DISPLAY_LABEL'"#,
        )
        .bind(addon_id)
        .fetch_one(&pool)
        .await
        .expect("load preserved setting");
        assert_eq!(sources, 0);
        assert_eq!(offers, 0);
        assert_eq!(installation_source, None);
        assert_eq!(setting, "Preserved value");
        assert!(load_installed_addon(&pool, addon_id)
            .await
            .expect("load installation")
            .is_some());
        assert!(!source_root.exists());
        assert!(!bundle_root.join(repository_id).exists());
    }

    #[tokio::test]
    async fn removing_checkout_source_is_rejected_without_mutation() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let bundle_root = temp.path().join("repository-bundles");
        sqlx::query(
            r#"INSERT INTO "AddonSource" (
                   "Id", "Name", "Kind", "RootPath", "ManifestJson", "ContentSha256",
                   "LastScan", "LastError", "StateRevision"
               ) VALUES ('org.example.checkout', 'Checkout', 'checkout', 'operator-owned',
                         '{}', 'digest', 'now', NULL, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("insert checkout source");

        let error =
            remove_local_repository_bundle_from(&pool, "org.example.checkout", &bundle_root)
                .await
                .expect_err("checkout removal must be rejected");

        assert!(error.to_string().contains("uploaded repository bundles"));
        let sources = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "AddonSource" WHERE "Id" = 'org.example.checkout'"#,
        )
        .fetch_one(&pool)
        .await
        .expect("count checkout source");
        assert_eq!(sources, 1);
    }

    #[tokio::test]
    async fn sideload_persistence_records_provenance_and_starts_disabled() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let manifest = fixture_manifest("org.ersatzrs.addon.sideload-fixture");
        let install_path = temp.path().join("installed-package");
        std::fs::create_dir_all(&install_path).expect("create package directory");
        std::fs::write(
            install_path.join("addon.toml"),
            toml::to_string(&manifest).expect("serialize manifest"),
        )
        .expect("write manifest");
        std::fs::write(install_path.join("addon.sh"), b"#!/bin/sh\n").expect("write entrypoint");
        let staged = StagedAddonPackage {
            manifest: manifest.clone(),
            package_sha256: "a".repeat(64),
            content_sha256: hash_addon_tree_sync(&install_path).expect("hash package"),
            destination: install_path,
        };

        persist_sideloaded_addon(&pool, None, &staged)
            .await
            .expect("persist sideload");
        let installed = load_installed_addon(&pool, &manifest.id)
            .await
            .expect("load sideload")
            .expect("installed row");

        assert_eq!(installed.trust_tier, AddonTrustTier::Sideloaded);
        assert_eq!(installed.repository_id, None);
        assert_eq!(installed.data_namespace, "sideload");
        assert!(!installed.enabled);
    }

    #[test]
    fn only_new_privileges_hold_an_updated_add_on_disabled() {
        let previous = {
            let mut manifest = fixture_manifest("org.ersatzrs.addon.fixture");
            manifest.permissions.network_hosts = vec!["api.example.test".to_owned()];
            manifest.permissions.external_commands = vec!["yt-dlp".to_owned()];
            manifest.permissions.media_read = true;
            manifest
        };

        assert_eq!(added_permissions(&previous, &previous.clone()), None);

        let mut narrowed = previous.clone();
        narrowed.permissions.network_hosts.clear();
        narrowed.permissions.media_read = false;
        assert_eq!(added_permissions(&previous, &narrowed), None);

        let mut new_host = previous.clone();
        new_host
            .permissions
            .network_hosts
            .push("cdn.example.test".to_owned());
        assert_eq!(
            added_permissions(&previous, &new_host).as_deref(),
            Some("network host cdn.example.test")
        );

        let mut new_command = previous.clone();
        new_command
            .permissions
            .external_commands
            .push("ffmpeg".to_owned());
        new_command.permissions.managed_ffmpeg = true;
        assert_eq!(
            added_permissions(&previous, &new_command).as_deref(),
            Some("external command ffmpeg, managed FFmpeg access")
        );

        let mut native_update = previous.clone();
        native_update
            .permissions
            .tool_update
            .push("YTDLP_PATH".to_owned());
        assert_eq!(
            added_permissions(&previous, &native_update).as_deref(),
            Some("self-update executable YTDLP_PATH")
        );
        assert_eq!(
            added_permissions(&native_update, &native_update.clone()),
            None
        );
        assert_eq!(added_permissions(&native_update, &previous), None);
    }

    #[tokio::test]
    async fn tool_resolution_binds_the_leased_executable_and_rejects_replacement_gaps() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("configured-tool");
        std::fs::write(&executable, b"fixture identity").unwrap();
        let mut manifest = fixture_manifest("org.ersatzrs.addon.fixture");
        manifest
            .settings
            .push(ersatzrs_addon_contract::AddonSettingDefinition {
                key: "TOOL_PATH".to_owned(),
                label: localized("Executable"),
                kind: AddonSettingKind::Executable,
                required: false,
                default: None,
                options: Vec::new(),
                suggested_reference: None,
            });
        let key = "ERSATZRS_ADDON_SETTING_TOOL_PATH".to_owned();
        let mut environment = BTreeMap::from([(key.clone(), "./configured-tool".to_owned())]);
        let paths =
            tool_execution::resolve_tool_paths(&manifest, &mut environment, temporary.path())
                .await
                .unwrap();
        let expected = dunce::canonicalize(&executable).unwrap();
        assert_eq!(paths, vec![expected.clone()]);
        assert_eq!(environment.get(&key).unwrap(), expected.to_str().unwrap());
        // A native updater may temporarily remove the executable. Never return
        // an empty lease set and let the provider race its replacement.
        std::fs::remove_file(executable).unwrap();
        let error =
            tool_execution::resolve_tool_paths(&manifest, &mut environment, temporary.path())
                .await
                .unwrap_err();
        assert!(
            matches!(&error, AddonError::Operation(operation) if operation.code == OPERATION_CODE_MISSING_SETTING)
        );
        assert!(!error
            .to_string()
            .contains(temporary.path().to_str().unwrap()));
        environment.remove(&key);
        assert!(
            tool_execution::resolve_tool_paths(&manifest, &mut environment, temporary.path())
                .await
                .unwrap()
                .is_empty()
        );
        environment.insert(key, String::new());
        assert!(
            tool_execution::resolve_tool_paths(&manifest, &mut environment, temporary.path())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_updated_add_on_returns_to_service_unless_it_wants_more() {
        // The update itself has already landed disabled by the time this runs,
        // so both cases start from the same freshly updated row and differ only
        // in what the new version declares.
        for (added_host, expected_enabled) in [(None, true), (Some("cdn.example.test"), false)] {
            let temp = tempfile::tempdir().expect("temp directory");
            let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
            let pool = crate::db::init(&database_url)
                .await
                .expect("initialize database");
            let package = temp.path().join("package");
            std::fs::create_dir(&package).expect("create package directory");
            std::fs::write(
                package.join("addon.sh"),
                b"printf '{\"status\":\"ready\",\"code\":\"ready\",\"message\":\"ok\"}'\n",
            )
            .expect("write fixture entrypoint");

            let previous = fixture_manifest("org.ersatzrs.addon.fixture");
            let mut updated = previous.clone();
            updated.version = Version::new(1, 1, 0);
            if let Some(host) = added_host {
                updated.permissions.network_hosts.push(host.to_owned());
            }
            insert_package_installation(&pool, &updated, &package, false).await;
            let mut previous_installation = load_installed_addon(&pool, &previous.id)
                .await
                .expect("load fixture installation")
                .expect("fixture installation");
            previous_installation.manifest = previous.clone();
            previous_installation.enabled = true;

            restore_enabled_after_update(
                &pool,
                &previous.id,
                Some(&previous_installation),
                &updated,
                &AddonRuntimeContext::default(),
            )
            .await;

            let installation = load_installed_addon(&pool, &previous.id)
                .await
                .expect("reload installation")
                .expect("installation");
            assert_eq!(
                installation.enabled, expected_enabled,
                "added host {added_host:?} decided enablement wrongly"
            );
        }
    }

    #[tokio::test]
    async fn required_dependencies_guard_runtime_and_disable() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let dependency = fixture_manifest("org.ersatzrs.addon.dependency");
        let mut consumer = fixture_manifest("org.ersatzrs.addon.consumer");
        consumer.dependencies.push(AddonDependency {
            id: dependency.id.clone(),
            version: semver::VersionReq::parse("^1.0").expect("version requirement"),
            optional: false,
        });
        insert_fixture_installation(&pool, &dependency, true).await;
        insert_fixture_installation(&pool, &consumer, true).await;

        validate_installed_dependencies(&pool, &consumer)
            .await
            .expect("enabled compatible dependency");
        assert!(disable_addon(&pool, &dependency.id).await.is_err());

        sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 0 WHERE "AddonId" = ?"#)
            .bind(&dependency.id)
            .execute(&pool)
            .await
            .expect("disable dependency fixture directly");
        assert!(validate_installed_dependencies(&pool, &consumer)
            .await
            .is_err());
    }

    async fn insert_pinned_addon_media_list(
        pool: &DbPool,
        addon_id: &str,
        playlist_id: EntityId,
        is_system: i64,
        group_id: EntityId,
    ) -> EntityId {
        sqlx::query(
            r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(playlist_id)
        .bind(is_system)
        .bind(format!("Fixture {playlist_id}"))
        .bind(group_id)
        .execute(pool)
        .await
        .expect("insert playlist");
        let list = sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "AddonId", "ProviderId", "SourceUrl", "Name", "PlaylistId"
               ) VALUES (?, ?, 'https://example.test/list', 'Fixture', ?)"#,
        )
        .bind(addon_id)
        .bind(format!("fixture-{playlist_id}"))
        .bind(playlist_id)
        .execute(pool)
        .await
        .expect("insert add-on media list");
        let list_id = list.last_insert_rowid();
        let revision = sqlx::query(
            r#"INSERT INTO "AddonMediaListRevision" (
                   "AddonMediaListId", "Revision", "OptionValuesJson"
               ) VALUES (?, 1, '{}')"#,
        )
        .bind(list_id)
        .execute(pool)
        .await
        .expect("insert retained revision");
        let artifact = sqlx::query(
            r#"INSERT INTO "AddonMediaListArtifact" (
                   "AddonMediaListId", "CacheKey", "Kind", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (?, ?, 1, 0, '2026-09-04 12:00:00', '2026-09-04 12:00:00')"#,
        )
        .bind(list_id)
        .bind(format!("fixture-artifact-{playlist_id}"))
        .execute(pool)
        .await
        .expect("insert retained artifact");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionArtifact" (
                   "AddonMediaListRevisionId", "AddonMediaListArtifactId"
               ) VALUES (?, ?)"#,
        )
        .bind(revision.last_insert_rowid())
        .bind(artifact.last_insert_rowid())
        .execute(pool)
        .await
        .expect("pin retained artifact");
        list_id
    }

    #[tokio::test]
    async fn forced_uninstall_cleanup_removes_pinned_lists_and_only_generated_playlists() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let manifest = fixture_manifest("org.ersatzrs.addon.fixture");
        insert_fixture_installation(&pool, &manifest, true).await;
        let group_id = 699_i64;
        sqlx::query(
            r#"INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
               VALUES (?, 'Fixture', 1)"#,
        )
        .bind(group_id)
        .execute(&pool)
        .await
        .expect("insert playlist group");
        let mut list_ids = Vec::new();
        for (id, is_system) in [(700_i64, 1_i64), (701, 0)] {
            list_ids.push(
                insert_pinned_addon_media_list(&pool, &manifest.id, id, is_system, group_id).await,
            );
        }

        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                   "AddonMediaListId", "Generation", "State", "RecordCapability",
                   "OverviewComplete", "FinalizationPhase", "LeaseOwner", "LeaseExpiresAt",
                   "CreatedAt", "UpdatedAt"
               ) VALUES (?, 1, 1, 'media-list.list.v5', 1, 2, 'finalizer',
                         '2099-01-01T00:00:00Z', '2026-09-04T12:00:00Z',
                         '2026-09-04T12:00:00Z')"#,
        )
        .bind(list_ids[0])
        .execute(&pool)
        .await
        .expect("insert active list finalization");

        let mut blocked = pool.begin().await.expect("begin blocked transaction");
        let error = remove_owned_media_lists(&mut blocked, &manifest.id)
            .await
            .expect_err("active finalization must block forced list removal");
        assert!(matches!(error, AddonError::Validation(_)));
        blocked.rollback().await.expect("roll back blocked removal");
        let retained_lists =
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "AddonMediaList""#)
                .fetch_one(&pool)
                .await
                .expect("count retained lists");
        assert_eq!(retained_lists, 2);
        sqlx::query(
            r#"UPDATE "AddonMediaListImportJob"
               SET "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL"#,
        )
        .execute(&pool)
        .await
        .expect("release list finalization");

        let mut transaction = pool.begin().await.expect("begin transaction");
        remove_owned_media_lists(&mut transaction, &manifest.id)
            .await
            .expect("remove owned lists");
        transaction.commit().await.expect("commit cleanup");

        let media_list_count =
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "AddonMediaList""#)
                .fetch_one(&pool)
                .await
                .expect("count media lists");
        let remaining_playlists =
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" >= 700"#)
                .fetch_one(&pool)
                .await
                .expect("count fixture playlists");
        let remaining_playlist_id =
            sqlx::query_scalar::<_, i64>(r#"SELECT "Id" FROM "Playlist" WHERE "Id" >= 700"#)
                .fetch_one(&pool)
                .await
                .expect("load remaining playlist");
        assert_eq!(media_list_count, 0);
        assert_eq!((remaining_playlists, remaining_playlist_id), (1, 701));
        let retained_state: i64 = sqlx::query_scalar(
            r#"SELECT
                   (SELECT COUNT(*) FROM "AddonMediaListArtifact") +
                   (SELECT COUNT(*) FROM "AddonMediaListRevisionArtifact")"#,
        )
        .fetch_one(&pool)
        .await
        .expect("count deleted retained artifacts");
        assert_eq!(retained_state, 0);
    }

    #[tokio::test]
    async fn optional_secret_reference_can_be_cleared() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let mut manifest = fixture_manifest("org.ersatzrs.addon.fixture");
        manifest
            .settings
            .push(ersatzrs_addon_contract::AddonSettingDefinition {
                key: "API_TOKEN".to_owned(),
                label: localized("API token"),
                kind: AddonSettingKind::SecretReference,
                required: false,
                default: None,
                options: Vec::new(),
                suggested_reference: None,
            });
        insert_fixture_installation(&pool, &manifest, false).await;
        let configured = AddonSettingValue {
            key: "API_TOKEN".to_owned(),
            value: None,
            reference_kind: Some(AddonSecretReferenceKind::Environment),
            reference: Some("ERSATZRS_TEST_TOKEN".to_owned()),
        };
        save_addon_setting(&pool, &manifest.id, &configured)
            .await
            .expect("save optional reference");
        let cleared = AddonSettingValue {
            reference: None,
            ..configured
        };

        save_addon_setting(&pool, &manifest.id, &cleared)
            .await
            .expect("clear optional reference");

        assert!(load_addon_settings(&pool, &manifest.id)
            .await
            .expect("load settings")
            .is_empty());
    }

    #[tokio::test]
    async fn empty_storage_override_restores_the_profile_default() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let mut manifest = fixture_manifest("org.ersatzrs.addon.fixture");
        manifest.manifest_version = 3;
        manifest.capabilities.extend(
            [
                CAPABILITY_MEDIA_LIST_LIST_V1,
                CAPABILITY_REMOTE_STREAM_LIST_V1,
                CAPABILITY_REMOTE_STREAM_PLAY_V1,
            ]
            .into_iter()
            .map(|id| AddonCapability {
                id: id.to_owned(),
                options: Vec::new(),
            }),
        );
        manifest
            .settings
            .push(ersatzrs_addon_contract::AddonSettingDefinition {
                key: "MEDIA_STORAGE_PATH".to_owned(),
                label: localized("Media storage location"),
                kind: AddonSettingKind::Path,
                required: false,
                default: None,
                options: Vec::new(),
                suggested_reference: None,
            });
        manifest.media_list_storage = Some(ersatzrs_addon_contract::AddonMediaListStorage {
            path_setting: "MEDIA_STORAGE_PATH".to_owned(),
            default_subdirectory: "fixture_media".to_owned(),
            source_name: "fixture".to_owned(),
        });
        insert_fixture_installation(&pool, &manifest, false).await;
        save_addon_setting(
            &pool,
            &manifest.id,
            &AddonSettingValue {
                key: "MEDIA_STORAGE_PATH".to_owned(),
                value: Some(temp.path().join("custom").to_string_lossy().into_owned()),
                reference_kind: None,
                reference: None,
            },
        )
        .await
        .expect("save storage override");

        save_addon_setting(
            &pool,
            &manifest.id,
            &AddonSettingValue {
                key: "MEDIA_STORAGE_PATH".to_owned(),
                value: Some(String::new()),
                reference_kind: None,
                reference: None,
            },
        )
        .await
        .expect("clear storage override");

        let settings = load_addon_settings(&pool, &manifest.id)
            .await
            .expect("load settings");
        assert!(settings.is_empty());
        assert_eq!(
            effective_storage_path(&manifest, &settings).expect("resolve storage"),
            Some(crate::file_system_layout::app_state_folder().join("fixture_media"))
        );
    }

    #[test]
    fn persistent_add_on_data_is_namespaced_by_repository() {
        let addon_id = "org.ersatzrs.addon.fixture";
        assert_ne!(
            addon_data_folder("org.ersatzrs.addons.official", addon_id),
            addon_data_folder("org.example.addons", addon_id)
        );
        assert_ne!(
            addon_cache_folder("org.ersatzrs.addons.official", addon_id),
            addon_cache_folder("org.example.addons", addon_id)
        );
    }

    #[tokio::test]
    async fn stored_installation_identity_must_match_manifest() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let manifest = fixture_manifest("org.ersatzrs.addon.fixture");
        insert_fixture_installation(&pool, &manifest, false).await;
        sqlx::query(r#"UPDATE "AddonInstallation" SET "Version" = '2.0.0' WHERE "AddonId" = ?"#)
            .bind(&manifest.id)
            .execute(&pool)
            .await
            .expect("tamper with stored version");

        assert!(load_installed_addon(&pool, &manifest.id).await.is_err());
    }

    #[tokio::test]
    async fn migration_removes_the_seeded_network_repository_from_fresh_databases() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");

        let repository = load_repository_row(&pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("load repository");
        assert_eq!(repository, None);
        assert!(list_trusted_repositories(&pool)
            .await
            .expect("list repositories")
            .is_empty());
    }

    #[tokio::test]
    async fn repository_refresh_reuses_verified_catalog_after_http_not_modified() {
        let server = MockServer::start_async().await;
        let repository_id = "org.ersatzrs.addons.fixture";
        let index_path = "/index-v1.json";
        let index_url = format!("{}{index_path}", server.base_url());
        let signing_key = SigningKey::from_bytes(&[11; 32]);
        let public_key = base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().as_bytes());
        let now = Utc::now();
        let index = AddonRepositoryIndex {
            schema_version: ersatzrs_addon_contract::ADDON_REPOSITORY_SCHEMA_VERSION,
            repository_id: repository_id.to_owned(),
            repository_name: "Fixture add-ons".to_owned(),
            sequence: 7,
            generated_at: (now - chrono::Duration::minutes(1)).to_rfc3339(),
            expires_at: (now + chrono::Duration::days(1)).to_rfc3339(),
            addons: Vec::new(),
        };
        let raw_json = serde_json::to_string(&index).expect("serialize catalog");
        let signature = base64::engine::general_purpose::STANDARD
            .encode(signing_key.sign(raw_json.as_bytes()).to_bytes());
        let last_modified = "Wed, 21 Oct 2015 07:28:00 GMT";
        let initial_index = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path(index_path)
                    .header_missing("if-none-match");
                then.status(200)
                    .header("etag", "\"catalog-v7\"")
                    .header("last-modified", last_modified)
                    .body(raw_json.clone());
            })
            .await;
        let initial_signature = server
            .mock_async(|when, then| {
                when.method(GET).path(format!("{index_path}.sig"));
                then.status(200).body(signature.clone());
            })
            .await;
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        save_trusted_repository(
            &pool,
            &TrustedAddonRepository {
                id: repository_id.to_owned(),
                name: "Fixture add-ons".to_owned(),
                index_url,
                public_key,
                is_official: false,
                is_enabled: true,
                last_sequence: None,
                last_refresh: None,
                last_error: None,
            },
        )
        .await
        .expect("save repository");
        let client = reqwest::Client::new();

        let first = refresh_repository(&pool, &client, repository_id)
            .await
            .expect("initial signed refresh");
        assert_eq!(first, index);
        initial_index.assert_async().await;
        initial_signature.assert_async().await;
        initial_index.delete_async().await;
        initial_signature.delete_async().await;

        let not_modified = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path(index_path)
                    .header("if-none-match", "\"catalog-v7\"")
                    .header("if-modified-since", last_modified);
                then.status(304).header("etag", "\"catalog-v7\"");
            })
            .await;
        let second = refresh_repository(&pool, &client, repository_id)
            .await
            .expect("conditional refresh");

        not_modified.assert_async().await;
        assert_eq!(second, index);
        let cache = load_catalog_cache(&pool, repository_id)
            .await
            .expect("load cache validators");
        assert_eq!(cache.raw_json.as_deref(), Some(raw_json.as_str()));
        assert_eq!(cache.etag.as_deref(), Some("\"catalog-v7\""));
        assert_eq!(cache.last_modified.as_deref(), Some(last_modified));
    }

    #[tokio::test]
    async fn invalid_stored_catalog_does_not_block_the_available_list() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        sqlx::query(r#"UPDATE "AddonRepository" SET "CatalogJson" = ? WHERE "Id" = ?"#)
            .bind("not-json")
            .bind(OFFICIAL_REPOSITORY_ID)
            .execute(&pool)
            .await
            .expect("store invalid catalog fixture");

        let available = list_available_addons(&pool)
            .await
            .expect("invalid repository remains isolated");

        assert!(available.is_empty());
    }

    #[test]
    fn structured_operation_diagnostics_use_the_final_stderr_line_and_redact_secrets() {
        let environment = BTreeMap::from([(
            "ERSATZRS_ADDON_SECRET_TOKEN".to_owned(),
            "secret-value".to_owned(),
        )]);
        let diagnostic = structured_operation_error(
            "provider prose\n{\"code\":\"provider-rejected\",\"message\":\"secret-value was rejected\"}\n",
            &environment,
            &[],
        )
        .expect("structured diagnostic");
        assert_eq!(diagnostic.code, "provider-rejected");
        assert_eq!(diagnostic.message, "[redacted] was rejected");
    }

    #[test]
    fn structured_operation_diagnostics_suppress_file_secret_details() {
        let environment = BTreeMap::from([(
            "ERSATZRS_ADDON_SECRET_FILE_TOKEN".to_owned(),
            "/run/secrets/token".to_owned(),
        )]);
        let diagnostic = structured_operation_error(
            "{\"code\":\"future-provider-code\",\"message\":\"provider detail\"}",
            &environment,
            &[],
        )
        .expect("structured diagnostic");
        assert_eq!(diagnostic.code, "future-provider-code");
        assert_eq!(diagnostic.message, FILE_SECRET_DIAGNOSTIC);
    }

    #[test]
    fn structured_operation_diagnostics_redact_invocation_private_values() {
        let private_path = "private staging/archive.txt".to_owned();
        let diagnostic = structured_operation_error(
            "{\"code\":\"provider-failed\",\"message\":\"private staging/archive.txt failed\"}",
            &BTreeMap::new(),
            &[private_path],
        )
        .expect("structured diagnostic");
        assert_eq!(diagnostic.message, "[redacted] failed");
    }

    #[test]
    fn successful_output_cannot_echo_an_invocation_private_value() {
        let private_path = "private staging/archive.txt".to_owned();
        let output = br#"{"record_type":"list","name":"private staging/archive.txt"}"#;
        assert!(output_contains_sensitive_value(
            output,
            std::slice::from_ref(&private_path)
        ));

        let windows_path = r"C:\private staging\archive.txt".to_owned();
        let escaped_windows =
            br#"{"record_type":"list","name":"C:\\private staging\\archive.txt"}"#;
        assert!(output_contains_sensitive_value(
            escaped_windows,
            &[windows_path]
        ));

        let unicode_escaped =
            br#"{"record_type":"list","name":"\u0070rivate staging/archive.txt"}"#;
        assert!(output_contains_sensitive_value(
            unicode_escaped,
            &[private_path]
        ));
    }
}
