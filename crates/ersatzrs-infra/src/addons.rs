//! Signed repository catalogs and out-of-process add-on lifecycle management.
//!
//! Add-ons are data plus executables. They never load into the ErsatzRS
//! process, and their manifests can only select host-controlled launch modes.

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
    AddonRepositoryIndex, AddonSettingKind, AddonTrustTier, CatalogAddon, CatalogPackage,
    RepositoryManifest, CAPABILITY_ADDON_CHECK_V1, CAPABILITY_MEDIA_LIST_LIST_V1,
    CAPABILITY_MEDIA_LIST_LIST_V2, CAPABILITY_MEDIA_LIST_LIST_V3, CAPABILITY_REMOTE_STREAM_LIST_V1,
    CAPABILITY_REMOTE_STREAM_LIST_V2, CAPABILITY_REMOTE_STREAM_LIST_V3,
    CAPABILITY_REMOTE_STREAM_PLAY_V1, OPERATION_CODE_FAILED, OPERATION_CODE_MISSING_SECRET,
    OPERATION_CODE_MISSING_SETTING,
};
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
use tokio::io::AsyncReadExt as _;
use tokio::process::Command;
use uuid::Uuid;

use crate::addon_media_storage::{
    effective_storage_path, ensure_media_source_in_transaction, prepare_storage_root,
};
use crate::db::{DbPool, DbRow};
use crate::file_system_layout::{addon_cache_folder, addon_data_folder, addon_packages_folder};
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

/// Catalog item paired with the repository that signed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableAddon {
    pub repository_id: String,
    pub addon: CatalogAddon,
    pub host_compatible: bool,
    pub package_available: bool,
}

/// Persisted package selected as the active version of one add-on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledAddon {
    pub addon_id: String,
    pub repository_id: Option<String>,
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
    terminate_on_drop: bool,
}

impl ManagedAddonChild {
    /// Wait for the leader, then terminate any members left in its managed
    /// process group/job before the identifier can be reused.
    pub(crate) async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let result = self.child.wait().await;
        if self.terminate_on_drop {
            let _ = self.child.start_kill();
            self.terminate_on_drop = false;
        }
        result
    }

    /// Terminate the managed process group/job and wait for its leader.
    pub(crate) async fn kill(&mut self) -> std::io::Result<()> {
        if self.terminate_on_drop {
            let result = self.child.start_kill();
            self.terminate_on_drop = false;
            result?;
        }
        self.child.wait().await?;
        Ok(())
    }

    /// Start terminating the managed process group/job without waiting.
    pub(crate) fn start_kill(&mut self) -> std::io::Result<()> {
        if !self.terminate_on_drop {
            return Ok(());
        }
        let result = self.child.start_kill();
        self.terminate_on_drop = false;
        result
    }
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
    )
    .await
}

/// Install or update an add-on from locally supplied package bytes.
///
/// The package receives the permanent `sideloaded` trust tier and starts
/// disabled. A sideload can only update another sideload of the same identity.
///
/// # Errors
///
/// Returns an error when unknown sources are disabled, the archive or manifest
/// is invalid, the platform is unsupported, persistence races, or the package
/// would replace a different source or downgrade an installed sideload.
pub async fn install_addon_from_package(
    pool: &DbPool,
    archive: Vec<u8>,
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
    let mut transaction = pool.begin().await.map_err(database_error)?;
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
                   "LastCheckCode" = NULL, "LastCheckMessage" = NULL, "LastCheckAt" = NULL
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
    let mut transaction = pool.begin().await.map_err(database_error)?;
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
                   "LastCheckMessage" = NULL, "LastCheckAt" = NULL
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
    let entrypoint = manifest.entrypoint_for(rid).ok_or_else(|| {
        AddonError::Validation("manifest has no entrypoint for this platform".to_owned())
    })?;
    let entrypoint_path = extracted_root.join(&entrypoint.path);
    if !entrypoint_path.is_file() {
        return Err(AddonError::Validation(
            "manifest entrypoint is missing from package".to_owned(),
        ));
    }
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
        r#"SELECT "AddonId", "RepositoryId", "TrustTier", "Version", "Enabled", "PackageSha256", "ContentSha256",
                  "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt",
                  "StateRevision", "LastCheckCode", "LastCheckMessage", "LastCheckAt"
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
        r#"SELECT "AddonId", "RepositoryId", "TrustTier", "Version", "Enabled", "PackageSha256", "ContentSha256",
                  "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt",
                  "StateRevision", "LastCheckCode", "LastCheckMessage", "LastCheckAt"
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
        r#"SELECT "AddonId", "RepositoryId", "TrustTier", "Version", "Enabled", "PackageSha256", "ContentSha256",
                  "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt",
                  "StateRevision", "LastCheckCode", "LastCheckMessage", "LastCheckAt"
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
        r#"SELECT "AddonId", "RepositoryId", "TrustTier", "Version", "Enabled", "PackageSha256", "ContentSha256",
                  "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt",
                  "StateRevision", "LastCheckCode", "LastCheckMessage", "LastCheckAt"
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

    let mut transaction = pool.begin().await.map_err(database_error)?;
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
               "LastCheckCode" = NULL, "LastCheckMessage" = NULL, "LastCheckAt" = NULL
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
    sqlx::query(
        r#"UPDATE "AddonInstallation"
           SET "LastCheckCode" = ?, "LastCheckMessage" = ?, "LastCheckAt" = ?
           WHERE "AddonId" = ? AND "StateRevision" = ?"#,
    )
    .bind(&result.code)
    .bind(&result.message)
    .bind(Utc::now().to_rfc3339())
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
    let mut transaction = pool.begin().await.map_err(database_error)?;
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
    let mut transaction = pool.begin().await.map_err(database_error)?;
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
               "LastCheckCode" = NULL, "LastCheckMessage" = NULL, "LastCheckAt" = NULL
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
    let mut transaction = pool.begin().await.map_err(database_error)?;
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
    let executable = tokio::fs::canonicalize(installation.install_path.join(&entrypoint.path))
        .await
        .map_err(package_io_error)?;
    if !executable.starts_with(&install_root) || !executable.is_file() {
        return Err(AddonError::Validation(
            "installed entrypoint escapes or is missing from package".to_owned(),
        ));
    }
    let settings = load_addon_settings(pool, addon_id).await?;
    let environment = resolve_setting_environment(
        &installation.manifest,
        &installation.data_namespace,
        &settings,
        runtime,
    )?;
    let (program, mut args) = platform_command(entrypoint.kind, &executable)?;
    args.push(operation.to_owned());
    Ok(ResolvedAddonCommand {
        addon_id: addon_id.to_owned(),
        program,
        args,
        working_directory: install_root,
        environment,
        state_revision: installation.state_revision,
    })
}

fn validate_supported_operation(capability: &str, operation: &str) -> Result<(), AddonError> {
    let supported = matches!(
        (capability, operation),
        (CAPABILITY_ADDON_CHECK_V1, "check")
            | (
                CAPABILITY_MEDIA_LIST_LIST_V1
                    | CAPABILITY_MEDIA_LIST_LIST_V2
                    | CAPABILITY_MEDIA_LIST_LIST_V3
                    | CAPABILITY_REMOTE_STREAM_LIST_V1
                    | CAPABILITY_REMOTE_STREAM_LIST_V2
                    | CAPABILITY_REMOTE_STREAM_LIST_V3,
                "list",
            )
            | (CAPABILITY_REMOTE_STREAM_PLAY_V1, "play")
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

async fn run_bounded_command(
    command: &ResolvedAddonCommand,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<Vec<u8>, AddonError> {
    let mut process = addon_process(command);
    process
        .current_dir(&command.working_directory)
        .env_clear()
        .envs(addon_process_environment())
        .envs(&command.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = spawn_managed_addon_process(process)
        .map_err(|_| AddonError::Process("entrypoint could not be started".to_owned()))?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| AddonError::Process("entrypoint stdout is unavailable".to_owned()))?;
    let stderr = child
        .stderr()
        .take()
        .ok_or_else(|| AddonError::Process("entrypoint stderr is unavailable".to_owned()))?;
    let stdout_task = tokio::spawn(read_limited(stdout, max_output_bytes));
    let stderr_task = tokio::spawn(read_limited(stderr, MAX_CHECK_OUTPUT_BYTES));
    let Ok(wait_result) = tokio::time::timeout(timeout, child.wait()).await else {
        let _ = child.kill().await;
        stdout_task.abort();
        stderr_task.abort();
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        return Err(AddonError::Process("entrypoint timed out".to_owned()));
    };
    let Ok(status) = wait_result else {
        let _ = child.start_kill();
        stdout_task.abort();
        stderr_task.abort();
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        return Err(AddonError::Process("entrypoint wait failed".to_owned()));
    };
    let _ = child.start_kill();
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
            status,
        )));
    }
    Ok(stdout)
}

fn operation_error_from_stderr(
    stderr: &[u8],
    environment: &BTreeMap<String, String>,
    status: ExitStatus,
) -> AddonOperationError {
    let raw = String::from_utf8_lossy(stderr);
    if let Some(diagnostic) = structured_operation_error(&raw, environment) {
        return diagnostic;
    }
    let detail = redact_and_bound(&raw, environment, 1_024);
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
) -> Option<AddonOperationError> {
    if let Some(line) = stderr.lines().rev().find(|line| !line.trim().is_empty()) {
        if let Ok(diagnostic) = serde_json::from_str::<AddonOperationError>(line.trim()) {
            if diagnostic.validate().is_ok() {
                return Some(AddonOperationError::new(
                    &diagnostic.code,
                    redact_and_bound(&diagnostic.message, environment, 1_024),
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
    process.wrap(KillOnDrop);
    process.spawn().map(|child| ManagedAddonChild {
        child,
        terminate_on_drop: true,
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
    #[cfg(windows)]
    const ALLOWED_NAMES: &[&str] = &[
        "PATH",
        "PATHEXT",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "TEMP",
        "TMP",
    ];
    #[cfg(not(windows))]
    const ALLOWED_NAMES: &[&str] = &["PATH", "LANG", "LC_ALL", "TMPDIR"];

    ALLOWED_NAMES
        .iter()
        .filter_map(|name| {
            crate::startup_config::environment_variable(name)
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

    let canonical_root = tokio::fs::canonicalize(&configured_root)
        .await
        .map_err(package_io_error)?;
    match tokio::fs::canonicalize(stored_path).await {
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
    let install_root = tokio::fs::canonicalize(stored_path)
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
    let root = tokio::fs::canonicalize(root)
        .await
        .map_err(package_io_error)?;
    hash_addon_tree(root).await
}

async fn hash_addon_tree(root: PathBuf) -> Result<String, AddonError> {
    tokio::task::spawn_blocking(move || hash_addon_tree_sync(&root))
        .await
        .map_err(|_| AddonError::Package("package hashing task failed".to_owned()))?
}

fn hash_addon_tree_sync(root: &Path) -> Result<String, AddonError> {
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
            AddonError::Package("installed package path escaped its root".to_owned())
        })?;
        let mut encoded_path = Vec::new();
        for (index, component) in relative.components().enumerate() {
            let std::path::Component::Normal(component) = component else {
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
        let canonical_file = std::fs::canonicalize(&path).map_err(package_io_error)?;
        if !canonical_file.starts_with(root) || !canonical_file.is_file() {
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
        AddonEntrypointKind::Native => Ok((path, Vec::new())),
        AddonEntrypointKind::WindowsBatch => Ok((path, Vec::new())),
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

/// Revalidate stored catalog structure and its pinned public key for health
/// reporting. The detached signature was verified before this JSON was stored.
pub(crate) fn validate_stored_catalog_state(
    repository_id: &str,
    public_key: &str,
    catalog_json: &str,
) -> Result<(), AddonError> {
    if !public_key.trim().is_empty() {
        decode_public_key(public_key)?;
    }
    let index: AddonRepositoryIndex = serde_json::from_str(catalog_json)
        .map_err(|_| AddonError::Validation("stored catalog is invalid".to_owned()))?;
    let repository = TrustedAddonRepository {
        id: repository_id.to_owned(),
        name: repository_id.to_owned(),
        index_url: OFFICIAL_REPOSITORY_INDEX_URL.to_owned(),
        public_key: public_key.to_owned(),
        is_official: false,
        is_enabled: true,
        last_sequence: None,
        last_refresh: None,
        last_error: None,
    };
    validate_catalog(&index, &repository, Utc::now())
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

fn redact_and_bound(value: &str, environment: &BTreeMap<String, String>, limit: usize) -> String {
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
        AddonCapability, AddonDependency, AddonEntrypoint, AddonPermissions, ADDON_MANIFEST_VERSION,
    };
    use httpmock::{Method::GET, MockServer};
    use std::io::Cursor;

    fn localized(value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("en-US".to_owned(), value.to_owned())])
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
    async fn timed_out_add_on_kills_its_descendant_process_group() {
        let temp = tempfile::tempdir().expect("temp directory");
        let marker = temp.path().join("orphan-marker");
        let command = ResolvedAddonCommand {
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
    async fn aborted_add_on_invocation_kills_its_descendant_process_group() {
        let temp = tempfile::tempdir().expect("temp directory");
        let marker = temp.path().join("aborted-marker");
        let ready = temp.path().join("invocation-ready");
        let command = ResolvedAddonCommand {
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
        assert!(validate_supported_operation(CAPABILITY_ADDON_CHECK_V1, "check").is_ok());
        assert!(validate_supported_operation(CAPABILITY_MEDIA_LIST_LIST_V1, "list").is_ok());
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_LIST_V1, "list").is_ok());
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_PLAY_V1, "play").is_ok());
        assert!(validate_supported_operation("remote-stream.play.v2", "play").is_err());
        assert!(validate_supported_operation(CAPABILITY_REMOTE_STREAM_PLAY_V1, "list").is_err());
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

        let addon_error = install_addon_from_package(&pool, b"not a zip".to_vec())
            .await
            .expect_err("sideload must be gated");
        assert!(addon_error.to_string().contains("unverified sources"));

        let repository_error = install_repository_from_package(&pool, b"not a zip".to_vec())
            .await
            .expect_err("repository package must be gated");
        assert!(repository_error.to_string().contains("unverified sources"));
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

    #[tokio::test]
    async fn official_repository_never_falls_back_to_unverified_refresh() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        set_unverified_sources_enabled(&pool, true)
            .await
            .expect("enable gate");
        sqlx::query(r#"UPDATE "AddonRepository" SET "PublicKey" = '' WHERE "Id" = ?"#)
            .bind(OFFICIAL_REPOSITORY_ID)
            .execute(&pool)
            .await
            .expect("simulate missing official key");

        let error = refresh_repository(&pool, &reqwest::Client::new(), OFFICIAL_REPOSITORY_ID)
            .await
            .expect_err("keyless official repository must fail");
        assert!(error.to_string().contains("official repository"));
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

    #[cfg(not(feature = "mysql"))]
    #[tokio::test]
    async fn forced_reference_cleanup_removes_media_lists_and_only_generated_playlists() {
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
        for (id, is_system) in [(700_i64, 1_i64), (701, 0)] {
            sqlx::query(
                r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
                   VALUES (?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(is_system)
            .bind(format!("Fixture {id}"))
            .bind(group_id)
            .execute(&pool)
            .await
            .expect("insert playlist");
            sqlx::query(
                r#"INSERT INTO "AddonMediaList" (
                       "AddonId", "ProviderId", "SourceUrl", "Name", "PlaylistId"
                   ) VALUES (?, ?, 'https://example.test/list', 'Fixture', ?)"#,
            )
            .bind(&manifest.id)
            .bind(format!("fixture-{id}"))
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert add-on media list");
        }

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
            .map(|id| AddonCapability { id: id.to_owned() }),
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

    /// Fresh database plus the official repository row the migration seeds.
    async fn seeded_official_repository() -> (tempfile::TempDir, DbPool, TrustedAddonRepository) {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_url = format!("sqlite:{}", temp.path().join("addons.db").display());
        let pool = crate::db::init(&database_url)
            .await
            .expect("initialize database");
        let seeded = load_repository_row(&pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("load repository")
            .expect("seeded repository");
        (temp, pool, seeded)
    }

    /// Give the seeded official repository cached catalog state, then pin a new
    /// public key over it. Returns the pinned key and the reloaded row.
    async fn pin_official_public_key(
        pool: &DbPool,
        seeded: TrustedAddonRepository,
    ) -> (String, TrustedAddonRepository) {
        let signing_key = SigningKey::from_bytes(&[9; 32]);
        let public_key = base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().as_bytes());
        sqlx::query(
            r#"UPDATE "AddonRepository"
               SET "LastSequence" = 7, "CatalogJson" = 'old',
                   "CatalogEtag" = 'old-etag', "CatalogLastModified" = 'old-date'
               WHERE "Id" = ?"#,
        )
        .bind(OFFICIAL_REPOSITORY_ID)
        .execute(pool)
        .await
        .expect("seed previous trust state");
        save_trusted_repository(
            pool,
            &TrustedAddonRepository {
                public_key: public_key.clone(),
                is_official: false,
                ..seeded
            },
        )
        .await
        .expect("pin public key");
        let updated = load_repository_row(pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("reload repository")
            .expect("updated repository");
        (public_key, updated)
    }

    #[tokio::test]
    async fn migration_seeds_an_operator_replaceable_official_repository() {
        let (_temp, _pool, seeded) = seeded_official_repository().await;
        assert!(seeded.is_official);
        assert_eq!(seeded.index_url, OFFICIAL_REPOSITORY_INDEX_URL);
        // The 202608111200 seed shipped an empty key because the official
        // repository was not published yet; 202608121200 pins the production
        // key, so a fresh database must now come up able to verify the signed
        // catalog rather than failing every refresh.
        assert!(
            !seeded.public_key.is_empty(),
            "the official repository must ship with its production public key"
        );
        decode_public_key(&seeded.public_key).expect("seeded key is a canonical Ed25519 key");
    }

    #[tokio::test]
    async fn pinning_a_new_official_public_key_clears_cached_catalog_state() {
        let (_temp, pool, seeded) = seeded_official_repository().await;
        let (public_key, updated) = pin_official_public_key(&pool, seeded).await;

        // The official flag is owned by the migration, not by the caller.
        assert!(updated.is_official);
        assert_eq!(updated.public_key, public_key);
        assert_eq!(updated.last_sequence, None);
        let catalog = load_catalog_json(&pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("load cleared catalog");
        assert_eq!(catalog, None);
        let cleared_cache = load_catalog_cache(&pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("load cleared cache");
        assert!(cleared_cache.raw_json.is_none());
        assert!(cleared_cache.etag.is_none());
        assert!(cleared_cache.last_modified.is_none());
        assert_eq!(cleared_cache.public_key, public_key);
    }

    #[tokio::test]
    async fn saving_unchanged_official_trust_state_preserves_cached_catalog_state() {
        let (_temp, pool, seeded) = seeded_official_repository().await;
        let (_public_key, updated) = pin_official_public_key(&pool, seeded).await;

        sqlx::query(
            r#"UPDATE "AddonRepository"
               SET "LastSequence" = 8, "CatalogJson" = 'preserved',
                   "CatalogEtag" = 'preserved-etag',
                   "CatalogLastModified" = 'preserved-date'
               WHERE "Id" = ?"#,
        )
        .bind(OFFICIAL_REPOSITORY_ID)
        .execute(&pool)
        .await
        .expect("seed same-trust state");
        save_trusted_repository(
            &pool,
            &TrustedAddonRepository {
                name: "Renamed official add-ons".to_owned(),
                ..updated
            },
        )
        .await
        .expect("update display metadata");
        let preserved = load_catalog_json(&pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("load preserved catalog");
        assert_eq!(preserved.as_deref(), Some("preserved"));
        let preserved_cache = load_catalog_cache(&pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("load preserved cache");
        assert_eq!(preserved_cache.etag.as_deref(), Some("preserved-etag"));
        assert_eq!(
            preserved_cache.last_modified.as_deref(),
            Some("preserved-date")
        );
        let repository = load_repository_row(&pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("load unchanged repository")
            .expect("unchanged repository");
        save_trusted_repository(&pool, &repository)
            .await
            .expect("reconcile unchanged repository");
        let unchanged_cache = load_catalog_cache(&pool, OFFICIAL_REPOSITORY_ID)
            .await
            .expect("load unchanged cache");
        assert_eq!(
            unchanged_cache.state_revision,
            preserved_cache.state_revision
        );
        assert_eq!(
            unchanged_cache.catalog_revision,
            preserved_cache.catalog_revision
        );
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
        )
        .expect("structured diagnostic");
        assert_eq!(diagnostic.code, "future-provider-code");
        assert_eq!(diagnostic.message, FILE_SECRET_DIAGNOSTIC);
    }
}
