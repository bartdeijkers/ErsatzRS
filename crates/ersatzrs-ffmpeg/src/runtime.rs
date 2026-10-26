use std::fmt;
use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use flate2::read::GzDecoder;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::locator::{
    first_non_empty_env, path_env_var, resolve_existing_executable, resolve_ffmpeg_on_path,
    resolve_ffprobe_on_path,
};
use crate::version::{
    check_ffmpeg_runtime_with_config, FfmpegCheckError, FfmpegRuntimeCheckConfig,
    FfmpegRuntimeReport, DEFAULT_FFMPEG_PATH, DEFAULT_FFPROBE_PATH, FFMPEG_PATH_ENV_VARS,
    FFPROBE_PATH_ENV_VARS,
};

/// First-party `FFmpeg` build repository used by ADR 0008 runtime acquisition.
pub const ERSATZRS_FFMPEG_REPOSITORY: &str = "bartdeijkers/ErsatzRS-ffmpeg";
/// Known-good `ErsatzRS-ffmpeg` release tag used when no runtime is available.
pub const DEFAULT_MANAGED_FFMPEG_TAG: &str = "v2.0.7";
/// Optional operator pin for the managed `FFmpeg` release tag.
pub const MANAGED_FFMPEG_TAG_ENV_VAR: &str = "ERSATZRS_FFMPEG_RELEASE_TAG";
/// Set to `0`, `false`, or `no` to forbid startup downloads.
pub const MANAGED_FFMPEG_DOWNLOAD_ENV_VAR: &str = "ERSATZRS_FFMPEG_DOWNLOAD";

const CURRENT_DIR_NAME: &str = "current";
const CURRENT_TAG_FILE: &str = "current-tag";
const USER_AGENT_VALUE: &str = "ErsatzRS-runtime-manager";

/// Where the accepted runtime came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FfmpegRuntimeSource {
    /// Runtime source was not resolved through the ADR 0008 manager.
    #[default]
    Unknown,
    /// Explicit operator override through environment or persisted config.
    Override,
    /// Previously installed managed runtime under the state directory.
    Managed,
    /// Developer/system `PATH` fallback.
    Path,
    /// Downloaded from `ErsatzRS-ffmpeg` during this startup.
    Downloaded,
}

impl fmt::Display for FfmpegRuntimeSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => f.write_str("unknown"),
            Self::Override => f.write_str("override"),
            Self::Managed => f.write_str("managed"),
            Self::Path => f.write_str("PATH"),
            Self::Downloaded => f.write_str("downloaded"),
        }
    }
}

/// Runtime acquisition metadata attached to the startup `FFmpeg` report.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FfmpegRuntimeAcquisition {
    /// Accepted runtime source.
    pub source: FfmpegRuntimeSource,
    /// Current platform RID used for managed package selection.
    pub rid: Option<String>,
    /// Managed runtime root, when configured by the host.
    pub managed_root: Option<PathBuf>,
    /// Installed `ErsatzRS-ffmpeg` tag for managed runtimes.
    pub installed_tag: Option<String>,
    /// Latest release tag observed during the best-effort update check.
    pub latest_tag: Option<String>,
    /// Previously installed managed tag when this startup downloaded an update.
    pub previous_tag: Option<String>,
    /// True when this startup downloaded a newer managed runtime.
    pub downloaded_update: bool,
    /// True when a managed runtime is older than the latest release but was not updated.
    pub update_available: bool,
    /// Non-fatal update-check or update-download failure text.
    pub update_check_error: Option<String>,
    /// Release asset used for a fresh download.
    pub asset_name: Option<String>,
}

/// Host-provided runtime manager settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegRuntimeManagerConfig {
    /// `<state>/ffmpeg` root for managed installs.
    pub managed_root: PathBuf,
    /// Release tag to download when no valid runtime exists.
    pub release_tag: String,
    /// Whether `release_tag` was explicitly pinned by the operator.
    pub release_tag_pinned: bool,
    /// Whether the manager may download a missing runtime.
    pub allow_download: bool,
}

impl FfmpegRuntimeManagerConfig {
    /// Build config from a managed root and process environment.
    #[must_use]
    pub fn from_env(managed_root: impl Into<PathBuf>) -> Self {
        let release_tag_override = managed_release_tag_override_from_env();
        Self {
            managed_root: managed_root.into(),
            release_tag: release_tag_override
                .clone()
                .unwrap_or_else(|| DEFAULT_MANAGED_FFMPEG_TAG.to_owned()),
            release_tag_pinned: release_tag_override.is_some(),
            allow_download: managed_download_enabled_from_env(),
        }
    }

    /// Build config with the default known-good release tag.
    #[must_use]
    pub fn new(managed_root: impl Into<PathBuf>) -> Self {
        Self {
            managed_root: managed_root.into(),
            release_tag: DEFAULT_MANAGED_FFMPEG_TAG.to_owned(),
            release_tag_pinned: false,
            allow_download: true,
        }
    }
}

/// One GitHub release asset relevant to managed runtime acquisition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegReleaseAsset {
    /// Asset file name.
    pub name: String,
    /// Browser download URL.
    pub download_url: String,
    /// Optional GitHub-provided digest, usually `sha256:<hex>`.
    pub digest: Option<String>,
}

/// GitHub release metadata used by the runtime manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegReleaseMetadata {
    /// Release tag.
    pub tag_name: String,
    /// Assets attached to the release.
    pub assets: Vec<FfmpegReleaseAsset>,
}

/// Release metadata/download client. Tests provide a fake implementation.
#[async_trait]
pub trait FfmpegReleaseClient: Send + Sync {
    /// Fetch the latest published `ErsatzRS-ffmpeg` release.
    async fn latest_release(&self) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError>;

    /// Fetch one release by tag.
    async fn release_by_tag(
        &self,
        tag: &str,
    ) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError>;

    /// Download an asset body.
    async fn download_asset(&self, url: &str) -> Result<Vec<u8>, FfmpegRuntimeManagerError>;
}

/// Runtime validator. Startup supplies the profile-aware validator; tests fake it.
#[async_trait]
pub trait FfmpegRuntimeValidator: Send + Sync {
    /// Validate an `ffmpeg`/`ffprobe` pair and return the runtime report.
    async fn validate(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
    ) -> Result<FfmpegRuntimeReport, FfmpegCheckError>;
}

/// Default validator that runs the normal capability gate.
#[derive(Debug, Default)]
pub struct CapabilityRuntimeValidator;

#[async_trait]
impl FfmpegRuntimeValidator for CapabilityRuntimeValidator {
    async fn validate(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
    ) -> Result<FfmpegRuntimeReport, FfmpegCheckError> {
        let config = FfmpegRuntimeCheckConfig::with_paths(ffmpeg_path, ffprobe_path);
        check_ffmpeg_runtime_with_config(&config).await
    }
}

/// GitHub API-backed release client.
#[derive(Clone)]
pub struct GitHubFfmpegReleaseClient {
    client: reqwest::Client,
    repository: String,
}

impl GitHubFfmpegReleaseClient {
    /// Create a client for the default first-party `FFmpeg` repository.
    #[must_use]
    pub fn default_repository() -> Self {
        Self::new(ERSATZRS_FFMPEG_REPOSITORY)
    }

    /// Create a client for a specific `owner/repo` repository.
    #[must_use]
    pub fn new(repository: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            repository: repository.into(),
        }
    }
}

#[async_trait]
impl FfmpegReleaseClient for GitHubFfmpegReleaseClient {
    async fn latest_release(&self) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.repository
        );
        self.get_release(&url).await
    }

    async fn release_by_tag(
        &self,
        tag: &str,
    ) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/tags/{tag}",
            self.repository
        );
        self.get_release(&url).await
    }

    async fn download_asset(&self, url: &str) -> Result<Vec<u8>, FfmpegRuntimeManagerError> {
        let response = self
            .client
            .get(url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .send()
            .await?
            .error_for_status()?;
        Ok(response.bytes().await?.to_vec())
    }
}

impl GitHubFfmpegReleaseClient {
    async fn get_release(
        &self,
        url: &str,
    ) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError> {
        let response = self
            .client
            .get(url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, "application/vnd.github+json")
            .send()
            .await?
            .error_for_status()?;
        let release: GitHubReleaseResponse = response.json().await?;
        Ok(release.into())
    }
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseResponse {
    tag_name: String,
    assets: Vec<GitHubReleaseAssetResponse>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseAssetResponse {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

impl From<GitHubReleaseResponse> for FfmpegReleaseMetadata {
    fn from(value: GitHubReleaseResponse) -> Self {
        Self {
            tag_name: value.tag_name,
            assets: value
                .assets
                .into_iter()
                .map(|asset| FfmpegReleaseAsset {
                    name: asset.name,
                    download_url: asset.browser_download_url,
                    digest: asset.digest,
                })
                .collect(),
        }
    }
}

/// Errors raised by ADR 0008 runtime acquisition.
#[derive(thiserror::Error, Debug)]
pub enum FfmpegRuntimeManagerError {
    /// Current target has no supported RID mapping.
    #[error("unsupported platform for ErsatzRS-ffmpeg runtime acquisition: {0}")]
    UnsupportedRid(String),
    /// Explicit operator override failed validation.
    #[error("explicit FFmpeg override is invalid: {0}")]
    ExplicitRuntimeInvalid(#[source] FfmpegCheckError),
    /// No usable runtime was found and downloads were disabled.
    #[error("no usable FFmpeg runtime was found and managed downloads are disabled")]
    DownloadDisabled,
    /// No release asset matched the current RID.
    #[error("release {tag} has no ErsatzRS-ffmpeg asset for {rid}")]
    AssetNotFound {
        /// Release tag.
        tag: String,
        /// Platform RID.
        rid: String,
    },
    /// No checksum was published for the selected asset.
    #[error("release asset {asset} has no SHA-256 checksum")]
    MissingChecksum {
        /// Asset name.
        asset: String,
    },
    /// Downloaded asset hash did not match the published checksum.
    #[error("checksum mismatch for {asset}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// Asset name.
        asset: String,
        /// Expected SHA-256 hex.
        expected: String,
        /// Actual SHA-256 hex.
        actual: String,
    },
    /// Archive did not contain an ffmpeg/ffprobe pair.
    #[error("release asset {asset} did not contain an ffmpeg/ffprobe pair")]
    MissingTools {
        /// Asset name.
        asset: String,
    },
    /// The downloaded runtime failed validation after install.
    #[error("downloaded FFmpeg runtime failed validation: {0}")]
    DownloadedRuntimeInvalid(#[source] FfmpegCheckError),
    /// All local candidates failed validation before download.
    #[error("no local FFmpeg runtime candidate passed validation")]
    NoUsableLocalRuntime,
    /// HTTP/API failure.
    #[error("FFmpeg release request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// Filesystem failure.
    #[error("FFmpeg runtime filesystem operation failed: {0}")]
    Io(#[from] io::Error),
    /// Zip extraction failure.
    #[error("FFmpeg zip extraction failed: {0}")]
    Zip(#[from] zip::result::ZipError),
}

/// Resolve and validate a runtime, downloading the managed package when needed.
///
/// # Errors
///
/// Returns an error when explicit overrides are invalid, no runtime can be
/// resolved, the release asset/checksum is missing or invalid, installation
/// fails, or the accepted runtime fails the capability gate.
pub async fn resolve_or_acquire_ffmpeg_runtime(
    config: &FfmpegRuntimeManagerConfig,
    ffmpeg_config: Option<&str>,
    ffprobe_config: Option<&str>,
) -> Result<FfmpegRuntimeReport, FfmpegRuntimeManagerError> {
    let client = GitHubFfmpegReleaseClient::default_repository();
    let validator = CapabilityRuntimeValidator;
    resolve_or_acquire_ffmpeg_runtime_with(
        config,
        ffmpeg_config,
        ffprobe_config,
        &client,
        &validator,
    )
    .await
}

/// Resolve and validate a runtime with injected client/validator.
///
/// # Errors
///
/// See [`resolve_or_acquire_ffmpeg_runtime`].
pub async fn resolve_or_acquire_ffmpeg_runtime_with(
    config: &FfmpegRuntimeManagerConfig,
    ffmpeg_config: Option<&str>,
    ffprobe_config: Option<&str>,
    client: &dyn FfmpegReleaseClient,
    validator: &dyn FfmpegRuntimeValidator,
) -> Result<FfmpegRuntimeReport, FfmpegRuntimeManagerError> {
    let rid = current_ersatzrs_ffmpeg_rid()
        .ok_or_else(|| FfmpegRuntimeManagerError::UnsupportedRid(platform_id()))?;

    if let Some(candidate) = explicit_candidate(ffmpeg_config, ffprobe_config) {
        return validate_local_candidate(candidate, rid, config, validator)
            .await
            .map_err(FfmpegRuntimeManagerError::ExplicitRuntimeInvalid);
    }

    if let Some(candidate) = managed_candidate(&config.managed_root) {
        if let Ok(report) =
            resolve_managed_candidate(candidate, rid, config, client, validator).await
        {
            return Ok(report);
        }
    }

    let mut download_error = None;
    if config.allow_download {
        match release_for_install(config, client).await {
            Ok(release) => {
                match download_release_install_and_validate(
                    config, rid, release, None, client, validator,
                )
                .await
                {
                    Ok(report) => return Ok(report),
                    Err(error) => download_error = Some(error),
                }
            }
            Err(error) => download_error = Some(error),
        }
    }

    if let Some(candidate) = path_candidate() {
        if let Ok(mut report) = validate_local_candidate(candidate, rid, config, validator).await {
            report.acquisition.update_check_error = download_error.map(|error| error.to_string());
            return Ok(report);
        }
    }

    if !config.allow_download {
        return Err(FfmpegRuntimeManagerError::DownloadDisabled);
    }

    Err(download_error.unwrap_or(FfmpegRuntimeManagerError::NoUsableLocalRuntime))
}

async fn validate_local_candidate(
    candidate: RuntimeCandidate,
    rid: &str,
    config: &FfmpegRuntimeManagerConfig,
    validator: &dyn FfmpegRuntimeValidator,
) -> Result<FfmpegRuntimeReport, FfmpegCheckError> {
    let mut report = validator
        .validate(&candidate.ffmpeg_path, &candidate.ffprobe_path)
        .await?;
    report.acquisition = FfmpegRuntimeAcquisition {
        source: candidate.source,
        rid: Some(rid.to_owned()),
        managed_root: Some(config.managed_root.clone()),
        installed_tag: candidate.installed_tag,
        latest_tag: None,
        previous_tag: None,
        downloaded_update: false,
        update_available: false,
        update_check_error: None,
        asset_name: candidate.asset_name,
    };
    Ok(report)
}

async fn resolve_managed_candidate(
    candidate: RuntimeCandidate,
    rid: &str,
    config: &FfmpegRuntimeManagerConfig,
    client: &dyn FfmpegReleaseClient,
    validator: &dyn FfmpegRuntimeValidator,
) -> Result<FfmpegRuntimeReport, FfmpegCheckError> {
    let mut report = validate_local_candidate(candidate, rid, config, validator).await?;
    match release_for_update(config, client).await {
        Ok(release) => {
            report.acquisition.latest_tag = Some(release.tag_name.clone());
            let installed = report.acquisition.installed_tag.clone();
            if installed
                .as_deref()
                .is_some_and(|tag| tag_is_newer(&release.tag_name, tag))
            {
                if config.allow_download {
                    match download_release_install_and_validate(
                        config,
                        rid,
                        release,
                        installed.clone(),
                        client,
                        validator,
                    )
                    .await
                    {
                        Ok(updated) => return Ok(updated),
                        Err(error) => {
                            report.acquisition.update_available = true;
                            report.acquisition.update_check_error = Some(error.to_string());
                        }
                    }
                } else {
                    report.acquisition.update_available = true;
                    report.acquisition.update_check_error =
                        Some("managed FFmpeg downloads are disabled".to_owned());
                }
            }
        }
        Err(error) => {
            report.acquisition.update_check_error = Some(error.to_string());
        }
    }

    Ok(report)
}

async fn release_for_update(
    config: &FfmpegRuntimeManagerConfig,
    client: &dyn FfmpegReleaseClient,
) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError> {
    if config.release_tag_pinned {
        return client.release_by_tag(&config.release_tag).await;
    }

    client.latest_release().await
}

async fn release_for_install(
    config: &FfmpegRuntimeManagerConfig,
    client: &dyn FfmpegReleaseClient,
) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError> {
    if config.release_tag_pinned {
        return client.release_by_tag(&config.release_tag).await;
    }

    match client.latest_release().await {
        Ok(release) => Ok(release),
        Err(_) => client.release_by_tag(&config.release_tag).await,
    }
}

async fn download_release_install_and_validate(
    config: &FfmpegRuntimeManagerConfig,
    rid: &str,
    release: FfmpegReleaseMetadata,
    previous_tag: Option<String>,
    client: &dyn FfmpegReleaseClient,
    validator: &dyn FfmpegRuntimeValidator,
) -> Result<FfmpegRuntimeReport, FfmpegRuntimeManagerError> {
    let package = select_package_asset(&release, rid)
        .ok_or_else(|| FfmpegRuntimeManagerError::AssetNotFound {
            tag: release.tag_name.clone(),
            rid: rid.to_owned(),
        })?
        .clone();
    let archive = client.download_asset(&package.download_url).await?;
    let expected = expected_checksum(client, &release, &package).await?;
    let actual = sha256_hex(&archive);
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(FfmpegRuntimeManagerError::ChecksumMismatch {
            asset: package.name.clone(),
            expected,
            actual,
        });
    }

    let staged = stage_runtime_archive(&archive, &package.name, &config.managed_root)?;

    let mut report = validator
        .validate(&staged.ffmpeg_path, &staged.ffprobe_path)
        .await
        .map_err(|error| {
            let _ = remove_path(&staged.temp_dir);
            FfmpegRuntimeManagerError::DownloadedRuntimeInvalid(error)
        })?;
    promote_staged_runtime(&staged, &config.managed_root, &release.tag_name)?;
    let current = config.managed_root.join(CURRENT_DIR_NAME);
    let (current_ffmpeg, current_ffprobe) =
        find_tool_pair(&current).ok_or_else(|| FfmpegRuntimeManagerError::MissingTools {
            asset: package.name.clone(),
        })?;
    report.ffmpeg.path = current_ffmpeg.to_string_lossy().into_owned();
    report.ffprobe.path = current_ffprobe.to_string_lossy().into_owned();
    report.acquisition = FfmpegRuntimeAcquisition {
        source: FfmpegRuntimeSource::Downloaded,
        rid: Some(rid.to_owned()),
        managed_root: Some(config.managed_root.clone()),
        installed_tag: Some(release.tag_name.clone()),
        latest_tag: Some(release.tag_name),
        previous_tag: previous_tag.clone(),
        downloaded_update: previous_tag.is_some(),
        update_available: false,
        update_check_error: None,
        asset_name: Some(package.name),
    };
    Ok(report)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeCandidate {
    ffmpeg_path: String,
    ffprobe_path: String,
    source: FfmpegRuntimeSource,
    installed_tag: Option<String>,
    asset_name: Option<String>,
}

fn explicit_candidate(
    ffmpeg_config: Option<&str>,
    ffprobe_config: Option<&str>,
) -> Option<RuntimeCandidate> {
    let candidate = explicit_candidate_from_parts(
        first_non_empty_env(FFMPEG_PATH_ENV_VARS),
        first_non_empty_env(FFPROBE_PATH_ENV_VARS),
        ffmpeg_config.map(str::to_owned),
        ffprobe_config.map(str::to_owned),
    )?;
    Some(resolve_candidate_executables(
        candidate,
        path_env_var().as_deref(),
    ))
}

fn resolve_candidate_executables(
    mut candidate: RuntimeCandidate,
    path_var: Option<&str>,
) -> RuntimeCandidate {
    // Next rebases relative configuration paths instead of searching PATH.
    // Validate and publish the same selected executables to every consumer.
    for path in [&mut candidate.ffmpeg_path, &mut candidate.ffprobe_path] {
        if let Some(resolved) = resolve_existing_executable(Path::new(path), path_var)
            .and_then(|resolved| resolved.to_str().map(str::to_owned))
        {
            *path = resolved;
        }
    }
    // Unresolved values reach the existing runtime checks and failure report.
    candidate
}

fn explicit_candidate_from_parts(
    ffmpeg_env: Option<String>,
    ffprobe_env: Option<String>,
    ffmpeg_config: Option<String>,
    ffprobe_config: Option<String>,
) -> Option<RuntimeCandidate> {
    let ffmpeg_config = non_empty_owned(ffmpeg_config);
    let ffprobe_config = non_empty_owned(ffprobe_config);
    if ffmpeg_env.is_none()
        && ffprobe_env.is_none()
        && ffmpeg_config.is_none()
        && ffprobe_config.is_none()
    {
        return None;
    }

    Some(RuntimeCandidate {
        ffmpeg_path: ffmpeg_env
            .or(ffmpeg_config)
            .unwrap_or_else(|| DEFAULT_FFMPEG_PATH.to_owned()),
        ffprobe_path: ffprobe_env
            .or(ffprobe_config)
            .unwrap_or_else(|| DEFAULT_FFPROBE_PATH.to_owned()),
        source: FfmpegRuntimeSource::Override,
        installed_tag: None,
        asset_name: None,
    })
}

fn managed_candidate(managed_root: &Path) -> Option<RuntimeCandidate> {
    let current = managed_root.join(CURRENT_DIR_NAME);
    let (ffmpeg_path, ffprobe_path) = find_tool_pair(&current)?;
    Some(RuntimeCandidate {
        ffmpeg_path: ffmpeg_path.to_string_lossy().into_owned(),
        ffprobe_path: ffprobe_path.to_string_lossy().into_owned(),
        source: FfmpegRuntimeSource::Managed,
        installed_tag: read_current_tag(managed_root),
        asset_name: None,
    })
}

fn path_candidate() -> Option<RuntimeCandidate> {
    let ffmpeg = resolve_ffmpeg_on_path()?;
    let ffprobe = resolve_ffprobe_on_path()?;
    Some(RuntimeCandidate {
        ffmpeg_path: ffmpeg.path,
        ffprobe_path: ffprobe.path,
        source: FfmpegRuntimeSource::Path,
        installed_tag: None,
        asset_name: None,
    })
}

fn non_empty_owned(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn read_current_tag(managed_root: &Path) -> Option<String> {
    fs::read_to_string(managed_root.join(CURRENT_TAG_FILE))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

async fn expected_checksum(
    client: &dyn FfmpegReleaseClient,
    release: &FfmpegReleaseMetadata,
    package: &FfmpegReleaseAsset,
) -> Result<String, FfmpegRuntimeManagerError> {
    if let Some(digest) = package.digest.as_deref().and_then(parse_digest_sha256) {
        return Ok(digest);
    }

    let checksum_asset = select_checksum_asset(release, &package.name).ok_or_else(|| {
        FfmpegRuntimeManagerError::MissingChecksum {
            asset: package.name.clone(),
        }
    })?;
    let bytes = client.download_asset(&checksum_asset.download_url).await?;
    let text = String::from_utf8_lossy(&bytes);
    parse_checksum_text(&text).ok_or_else(|| FfmpegRuntimeManagerError::MissingChecksum {
        asset: package.name.clone(),
    })
}

fn select_package_asset<'a>(
    release: &'a FfmpegReleaseMetadata,
    rid: &str,
) -> Option<&'a FfmpegReleaseAsset> {
    let rid_suffix = format!("-{rid}");
    release
        .assets
        .iter()
        .filter(|asset| {
            let name = asset.name.to_ascii_lowercase();
            [".tar.gz", ".tgz", ".tar", ".zip"].iter().any(|extension| {
                name.strip_suffix(extension)
                    .is_some_and(|stem| stem.ends_with(&rid_suffix))
            })
        })
        .max_by_key(|asset| asset.name.contains("ersatzrs-ffmpeg"))
}

fn select_checksum_asset<'a>(
    release: &'a FfmpegReleaseMetadata,
    package_name: &str,
) -> Option<&'a FfmpegReleaseAsset> {
    let exact = format!("{package_name}.sha256");
    release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(&exact))
        .or_else(|| {
            release.assets.iter().find(|asset| {
                asset.name.contains(package_name) && is_checksum_asset_name(&asset.name)
            })
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    TarGz,
    Tar,
}

fn supported_archive_kind(asset_name: &str) -> Option<ArchiveKind> {
    let path = Path::new(asset_name);
    let extension = path.extension().and_then(|value| value.to_str())?;
    if extension.eq_ignore_ascii_case("zip") {
        return Some(ArchiveKind::Zip);
    }
    if extension.eq_ignore_ascii_case("tgz") {
        return Some(ArchiveKind::TarGz);
    }
    if extension.eq_ignore_ascii_case("tar") {
        return Some(ArchiveKind::Tar);
    }
    if extension.eq_ignore_ascii_case("gz") {
        let stem = path.file_stem().and_then(|value| value.to_str())?;
        if Path::new(stem)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("tar"))
        {
            return Some(ArchiveKind::TarGz);
        }
    }
    None
}

fn is_checksum_asset_name(asset_name: &str) -> bool {
    Path::new(asset_name)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("sha256"))
}

fn parse_digest_sha256(digest: &str) -> Option<String> {
    digest
        .strip_prefix("sha256:")
        .map(str::trim)
        .filter(|value| is_sha256_hex(value))
        .map(str::to_ascii_lowercase)
}

fn parse_checksum_text(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|token| is_sha256_hex(token))
        .map(str::to_ascii_lowercase)
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

struct StagedRuntimeInstall {
    temp_dir: PathBuf,
    install_dir: PathBuf,
    ffmpeg_path: String,
    ffprobe_path: String,
}

fn stage_runtime_archive(
    archive: &[u8],
    asset_name: &str,
    managed_root: &Path,
) -> Result<StagedRuntimeInstall, FfmpegRuntimeManagerError> {
    fs::create_dir_all(managed_root)?;
    let temp = managed_root.join(format!(".partial-{}", unique_suffix()));
    if temp.exists() {
        remove_path(&temp)?;
    }
    fs::create_dir_all(&temp)?;

    let extracted = temp.join("extracted");
    fs::create_dir_all(&extracted)?;
    extract_archive(archive, asset_name, &extracted)?;
    let (ffmpeg, ffprobe) =
        find_tool_pair(&extracted).ok_or_else(|| FfmpegRuntimeManagerError::MissingTools {
            asset: asset_name.to_owned(),
        })?;
    let source_dir = ffmpeg
        .parent()
        .ok_or_else(|| FfmpegRuntimeManagerError::MissingTools {
            asset: asset_name.to_owned(),
        })?;
    let ffprobe_dir = ffprobe
        .parent()
        .ok_or_else(|| FfmpegRuntimeManagerError::MissingTools {
            asset: asset_name.to_owned(),
        })?;
    if source_dir != ffprobe_dir {
        return Err(FfmpegRuntimeManagerError::MissingTools {
            asset: asset_name.to_owned(),
        });
    }

    let staged = temp.join("install");
    if source_dir
        .file_name()
        .is_some_and(|name| name == std::ffi::OsStr::new("bin"))
    {
        let package_root = source_dir.parent().unwrap_or(source_dir);
        copy_dir_contents(package_root, &staged)?;
    } else {
        copy_dir_contents(source_dir, &staged.join("bin"))?;
    }

    let (staged_ffmpeg, staged_ffprobe) =
        find_tool_pair(&staged).ok_or_else(|| FfmpegRuntimeManagerError::MissingTools {
            asset: asset_name.to_owned(),
        })?;
    #[cfg(unix)]
    prepare_managed_tool_launchers(&staged_ffmpeg, &staged_ffprobe)?;
    Ok(StagedRuntimeInstall {
        temp_dir: temp,
        install_dir: staged,
        ffmpeg_path: staged_ffmpeg.to_string_lossy().into_owned(),
        ffprobe_path: staged_ffprobe.to_string_lossy().into_owned(),
    })
}

#[cfg(unix)]
fn prepare_managed_tool_launchers(ffmpeg: &Path, ffprobe: &Path) -> io::Result<()> {
    write_unix_managed_tool_launcher(ffmpeg)?;
    write_unix_managed_tool_launcher(ffprobe)
}

#[cfg(all(unix, target_os = "macos"))]
const MANAGED_LIBRARY_PATH_ENV: &str = "DYLD_LIBRARY_PATH";

#[cfg(all(unix, not(target_os = "macos")))]
const MANAGED_LIBRARY_PATH_ENV: &str = "LD_LIBRARY_PATH";

#[cfg(unix)]
fn write_unix_managed_tool_launcher(tool_path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if !managed_library_dir_exists(tool_path) {
        return Ok(());
    }

    let Some(file_name) = tool_path.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let real_name = format!("{file_name}.real");
    let real_path = tool_path.with_file_name(&real_name);
    fs::rename(tool_path, &real_path)?;
    fs::write(
        tool_path,
        unix_managed_tool_launcher_script(&real_name, MANAGED_LIBRARY_PATH_ENV),
    )?;
    fs::set_permissions(tool_path, fs::Permissions::from_mode(0o755))
}

#[cfg(unix)]
fn managed_library_dir_exists(tool_path: &Path) -> bool {
    tool_path.parent().is_some_and(|dir| {
        dir.join("lib").is_dir()
            || dir
                .parent()
                .is_some_and(|parent| parent.join("lib").is_dir())
    })
}

#[cfg(unix)]
fn unix_managed_tool_launcher_script(real_name: &str, library_env: &str) -> String {
    let script = r#"#!/usr/bin/env sh
set -eu
tool_dir="$(dirname "$0")"
for lib_dir in "$tool_dir/lib" "$tool_dir/../lib"; do
    if [ -d "$lib_dir" ]; then
        export __LIBRARY_ENV__="$lib_dir${__LIBRARY_ENV__:+:$__LIBRARY_ENV__}"
    fi
done
exec "$tool_dir/__REAL_NAME__" "$@"
"#;
    script
        .replace("__LIBRARY_ENV__", library_env)
        .replace("__REAL_NAME__", real_name)
}

fn promote_staged_runtime(
    staged: &StagedRuntimeInstall,
    managed_root: &Path,
    tag: &str,
) -> Result<(), FfmpegRuntimeManagerError> {
    let current = managed_root.join(CURRENT_DIR_NAME);
    let backup = managed_root.join(format!(".previous-{}", unique_suffix()));
    if current.exists() {
        fs::rename(&current, &backup)?;
    }
    if let Err(error) = fs::rename(&staged.install_dir, &current) {
        if backup.exists() {
            let _ = fs::rename(&backup, &current);
        }
        let _ = remove_path(&staged.temp_dir);
        return Err(FfmpegRuntimeManagerError::Io(error));
    }
    fs::write(managed_root.join(CURRENT_TAG_FILE), tag)?;
    if backup.exists() {
        remove_path(&backup)?;
    }
    remove_path(&staged.temp_dir)?;
    Ok(())
}

fn extract_archive(
    archive: &[u8],
    asset_name: &str,
    destination: &Path,
) -> Result<(), FfmpegRuntimeManagerError> {
    match supported_archive_kind(asset_name) {
        Some(ArchiveKind::Zip) => {
            let mut zip = zip::ZipArchive::new(Cursor::new(archive))?;
            for index in 0..zip.len() {
                let mut file = zip.by_index(index)?;
                let Some(enclosed) = file.enclosed_name() else {
                    continue;
                };
                let path = destination.join(enclosed);
                if file.is_dir() {
                    fs::create_dir_all(&path)?;
                } else {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let mut out = File::create(&path)?;
                    io::copy(&mut file, &mut out)?;
                    #[cfg(unix)]
                    set_zip_permissions(&path, file.unix_mode())?;
                }
            }
            Ok(())
        }
        Some(ArchiveKind::TarGz) => {
            let decoder = GzDecoder::new(Cursor::new(archive));
            let mut tar = tar::Archive::new(decoder);
            tar.unpack(destination)?;
            Ok(())
        }
        Some(ArchiveKind::Tar) => {
            let mut tar = tar::Archive::new(Cursor::new(archive));
            tar.unpack(destination)?;
            Ok(())
        }
        None => Err(FfmpegRuntimeManagerError::MissingTools {
            asset: asset_name.to_owned(),
        }),
    }
}

#[cfg(unix)]
fn set_zip_permissions(path: &Path, mode: Option<u32>) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(mode) = mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

fn find_tool_pair(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let ffmpeg_name = executable_name(DEFAULT_FFMPEG_PATH);
    let ffprobe_name = executable_name(DEFAULT_FFPROBE_PATH);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let ffmpeg = dir.join(&ffmpeg_name);
        let ffprobe = dir.join(&ffprobe_name);
        if ffmpeg.is_file() && ffprobe.is_file() {
            return Some((ffmpeg, ffprobe));
        }
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    None
}

#[cfg(windows)]
fn executable_name(base: &str) -> String {
    if base.to_ascii_lowercase().ends_with(".exe") {
        base.to_owned()
    } else {
        format!("{base}.exe")
    }
}

#[cfg(not(windows))]
fn executable_name(base: &str) -> String {
    base.to_owned()
}

fn copy_dir_contents(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if from.is_dir() {
            copy_dir_contents(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn remove_path(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("{}-{nanos}", std::process::id())
}

/// Map the current target to an ADR 0008 `ErsatzRS-ffmpeg` RID.
#[must_use]
pub fn current_ersatzrs_ffmpeg_rid() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("linux", "arm") => Some("linux-arm"),
        ("macos", "x86_64") => Some("osx-x64"),
        ("macos", "aarch64") => Some("osx-arm64"),
        ("windows", "x86_64") => Some("win-x64"),
        _ => None,
    }
}

fn platform_id() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn tag_is_newer(latest: &str, installed: &str) -> bool {
    let Some(latest) = parse_tag_numbers(latest) else {
        return latest != installed;
    };
    let Some(installed) = parse_tag_numbers(installed) else {
        return false;
    };
    latest > installed
}

fn parse_tag_numbers(tag: &str) -> Option<Vec<u32>> {
    let normalized = tag.trim().trim_start_matches('v');
    let mut out = Vec::new();
    for part in normalized.split('.') {
        out.push(part.parse().ok()?);
    }
    (!out.is_empty()).then_some(out)
}

#[allow(clippy::disallowed_methods)]
fn managed_release_tag_override_from_env() -> Option<String> {
    std::env::var(MANAGED_FFMPEG_TAG_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[allow(clippy::disallowed_methods)]
fn managed_download_enabled_from_env() -> bool {
    std::env::var(MANAGED_FFMPEG_DOWNLOAD_ENV_VAR)
        .ok()
        .is_none_or(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone)]
    struct FakeClient {
        latest: Arc<Mutex<Result<FfmpegReleaseMetadata, String>>>,
        by_tag: Arc<Mutex<HashMap<String, FfmpegReleaseMetadata>>>,
        downloads: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        latest_calls: Arc<Mutex<u32>>,
        release_calls: Arc<Mutex<u32>>,
        download_calls: Arc<Mutex<u32>>,
    }

    impl Default for FakeClient {
        fn default() -> Self {
            Self {
                latest: Arc::new(Mutex::new(Ok(FfmpegReleaseMetadata {
                    tag_name: "v1.0.1".to_owned(),
                    assets: Vec::new(),
                }))),
                by_tag: Arc::new(Mutex::new(HashMap::new())),
                downloads: Arc::new(Mutex::new(HashMap::new())),
                latest_calls: Arc::new(Mutex::new(0)),
                release_calls: Arc::new(Mutex::new(0)),
                download_calls: Arc::new(Mutex::new(0)),
            }
        }
    }

    impl FakeClient {
        fn with_latest(tag: &str) -> Self {
            let client = Self::default();
            *client.latest.lock().unwrap() = Ok(FfmpegReleaseMetadata {
                tag_name: tag.to_owned(),
                assets: Vec::new(),
            });
            client
        }

        fn with_latest_release(release: FfmpegReleaseMetadata) -> Self {
            let client = Self::default();
            *client.latest.lock().unwrap() = Ok(release);
            client
        }

        fn add_release(&self, release: FfmpegReleaseMetadata) {
            self.by_tag
                .lock()
                .unwrap()
                .insert(release.tag_name.clone(), release);
        }

        fn add_download(&self, url: &str, bytes: Vec<u8>) {
            self.downloads.lock().unwrap().insert(url.to_owned(), bytes);
        }

        fn download_call_count(&self) -> u32 {
            *self.download_calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl FfmpegReleaseClient for FakeClient {
        async fn latest_release(&self) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError> {
            *self.latest_calls.lock().unwrap() += 1;
            self.latest.lock().unwrap().clone().map_err(fake_error)
        }

        async fn release_by_tag(
            &self,
            tag: &str,
        ) -> Result<FfmpegReleaseMetadata, FfmpegRuntimeManagerError> {
            *self.release_calls.lock().unwrap() += 1;
            self.by_tag
                .lock()
                .unwrap()
                .get(tag)
                .cloned()
                .ok_or_else(|| fake_error(format!("missing release {tag}")))
        }

        async fn download_asset(&self, url: &str) -> Result<Vec<u8>, FfmpegRuntimeManagerError> {
            *self.download_calls.lock().unwrap() += 1;
            self.downloads
                .lock()
                .unwrap()
                .get(url)
                .cloned()
                .ok_or_else(|| fake_error(format!("missing download {url}")))
        }
    }

    fn fake_error(message: impl Into<String>) -> FfmpegRuntimeManagerError {
        FfmpegRuntimeManagerError::Io(io::Error::other(message.into()))
    }

    #[derive(Default)]
    struct FakeValidator {
        invalid_contains: Vec<String>,
    }

    #[async_trait]
    impl FfmpegRuntimeValidator for FakeValidator {
        async fn validate(
            &self,
            ffmpeg_path: &str,
            ffprobe_path: &str,
        ) -> Result<FfmpegRuntimeReport, FfmpegCheckError> {
            if self
                .invalid_contains
                .iter()
                .any(|needle| ffmpeg_path.contains(needle) || ffprobe_path.contains(needle))
            {
                return Err(FfmpegCheckError::ParseError {
                    tool: "ffmpeg",
                    output: "invalid fake runtime".to_owned(),
                });
            }

            let mut report = FfmpegRuntimeReport::for_tests();
            report.ffmpeg.path = ffmpeg_path.to_owned();
            report.ffprobe.path = ffprobe_path.to_owned();
            Ok(report)
        }
    }

    #[test]
    fn explicit_config_candidate_wins_before_managed_or_path() {
        let candidate = explicit_candidate_from_parts(
            None,
            None,
            Some("/custom/ffmpeg".to_owned()),
            Some("/custom/ffprobe".to_owned()),
        )
        .expect("candidate");

        assert_eq!(candidate.source, FfmpegRuntimeSource::Override);
        assert_eq!(candidate.ffmpeg_path, "/custom/ffmpeg");
        assert_eq!(candidate.ffprobe_path, "/custom/ffprobe");
    }

    fn executable_fixture(directory: &Path, name: &str) -> PathBuf {
        let path = directory.join(executable_name(name));
        fs::write(&path, b"fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[test]
    fn explicit_runtime_freezes_path_companion_without_replacing_selected_tool() {
        let selected = tempfile::tempdir().unwrap();
        let search = tempfile::tempdir().unwrap();
        let ffmpeg = executable_fixture(selected.path(), "ffmpeg");
        let ffprobe = executable_fixture(search.path(), "ffprobe");
        executable_fixture(search.path(), "ffmpeg");
        let candidate = explicit_candidate_from_parts(
            Some(ffmpeg.to_str().unwrap().to_owned()),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(candidate.ffprobe_path, "ffprobe");
        let resolved = resolve_candidate_executables(candidate, search.path().to_str());
        assert_eq!(resolved.ffmpeg_path, ffmpeg.to_str().unwrap());
        assert_eq!(resolved.ffprobe_path, ffprobe.to_str().unwrap());
        assert!(Path::new(&resolved.ffprobe_path).is_absolute());
        assert_eq!(resolved.source, FfmpegRuntimeSource::Override);
    }

    #[test]
    fn explicit_relative_runtime_is_anchored_to_host_directory() {
        let directory = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let executable = executable_fixture(directory.path(), "custom-probe");
        let relative = Path::new(".")
            .join(directory.path().file_name().unwrap())
            .join(executable.file_name().unwrap());
        let candidate = explicit_candidate_from_parts(
            None,
            Some(relative.to_str().unwrap().to_owned()),
            None,
            None,
        )
        .unwrap();
        let resolved = resolve_candidate_executables(candidate, None);
        assert_eq!(resolved.ffprobe_path, executable.to_str().unwrap());
        assert_eq!(resolved.ffmpeg_path, "ffmpeg");
    }

    #[test]
    fn unresolved_runtime_remains_available_to_existing_failure_checks() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join(executable_name("missing-probe"));
        let candidate = explicit_candidate_from_parts(
            Some("missing-command".to_owned()),
            Some(missing.to_str().unwrap().to_owned()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            resolve_candidate_executables(candidate.clone(), None),
            candidate
        );
    }

    #[test]
    fn current_target_maps_to_supported_rid() {
        assert!(current_ersatzrs_ffmpeg_rid().is_some());
    }

    #[test]
    fn default_manager_uses_ffmpeg_nine_package_line() {
        let config = FfmpegRuntimeManagerConfig::new("/tmp/ersatzrs-ffmpeg-test");

        assert_eq!(config.release_tag, "v2.0.7");
        assert!(!config.release_tag_pinned);
    }

    #[tokio::test]
    async fn preplaced_managed_runtime_bypasses_network_download() {
        let temp = tempfile::tempdir().expect("temp");
        seed_managed_runtime(temp.path(), "v1.0.1");
        let config = FfmpegRuntimeManagerConfig::new(temp.path());
        let client = FakeClient::with_latest("v1.0.1");
        let validator = FakeValidator::default();

        let report =
            resolve_or_acquire_ffmpeg_runtime_with(&config, None, None, &client, &validator)
                .await
                .expect("runtime");

        assert_eq!(report.acquisition.source, FfmpegRuntimeSource::Managed);
        assert_eq!(report.acquisition.installed_tag.as_deref(), Some("v1.0.1"));
        assert_eq!(client.download_call_count(), 0);
    }

    #[tokio::test]
    async fn outdated_managed_runtime_downloads_update() {
        let temp = tempfile::tempdir().expect("temp");
        seed_managed_runtime(temp.path(), "v1.0.1");
        let rid = current_ersatzrs_ffmpeg_rid().expect("rid");
        let archive = package_zip();
        let checksum = sha256_hex(&archive);
        let archive_name = format!("ersatzrs-ffmpeg-1.0.2-{rid}.zip");
        let archive_url = "https://example.test/update.zip";
        let checksum_url = "https://example.test/update.zip.sha256";
        let release =
            release_with_package("v1.0.2", &archive_name, archive_url, checksum_url, None);
        let config = FfmpegRuntimeManagerConfig::new(temp.path());
        let client = FakeClient::with_latest_release(release);
        client.add_download(archive_url, archive);
        client.add_download(
            checksum_url,
            format!("{checksum}  {archive_name}\n").into_bytes(),
        );
        let validator = FakeValidator::default();

        let report =
            resolve_or_acquire_ffmpeg_runtime_with(&config, None, None, &client, &validator)
                .await
                .expect("runtime");

        assert_eq!(report.acquisition.source, FfmpegRuntimeSource::Downloaded);
        assert!(report.acquisition.downloaded_update);
        assert_eq!(report.acquisition.previous_tag.as_deref(), Some("v1.0.1"));
        assert_eq!(report.acquisition.latest_tag.as_deref(), Some("v1.0.2"));
        assert!(Path::new(&report.ffmpeg.path).starts_with(temp.path().join(CURRENT_DIR_NAME)));
        assert!(Path::new(&report.ffprobe.path).starts_with(temp.path().join(CURRENT_DIR_NAME)));
        assert!(!report.acquisition.update_available);
        assert_eq!(
            fs::read_to_string(temp.path().join(CURRENT_TAG_FILE))
                .expect("current tag")
                .as_str(),
            "v1.0.2"
        );
    }

    #[tokio::test]
    async fn failed_managed_update_keeps_existing_runtime() {
        let temp = tempfile::tempdir().expect("temp");
        seed_managed_runtime(temp.path(), "v1.0.1");
        let config = FfmpegRuntimeManagerConfig::new(temp.path());
        let client = FakeClient::with_latest("v1.0.2");
        let validator = FakeValidator::default();

        let report =
            resolve_or_acquire_ffmpeg_runtime_with(&config, None, None, &client, &validator)
                .await
                .expect("runtime");

        assert_eq!(report.acquisition.source, FfmpegRuntimeSource::Managed);
        assert_eq!(report.acquisition.latest_tag.as_deref(), Some("v1.0.2"));
        assert!(report.acquisition.update_available);
        assert!(report.acquisition.update_check_error.is_some());
        assert_eq!(
            fs::read_to_string(temp.path().join(CURRENT_TAG_FILE))
                .expect("current tag")
                .as_str(),
            "v1.0.1"
        );
    }

    #[tokio::test]
    async fn pinned_release_tag_updates_older_managed_runtime() {
        let temp = tempfile::tempdir().expect("temp");
        seed_managed_runtime(temp.path(), "v1.0.1");
        let rid = current_ersatzrs_ffmpeg_rid().expect("rid");
        let archive = package_zip();
        let checksum = sha256_hex(&archive);
        let archive_name = format!("ersatzrs-ffmpeg-1.0.2-{rid}.zip");
        let archive_url = "https://example.test/pinned.zip";
        let checksum_url = "https://example.test/pinned.zip.sha256";
        let release = release_with_package(
            "v1.0.2",
            &archive_name,
            archive_url,
            checksum_url,
            Some(format!("sha256:{checksum}")),
        );
        let config = FfmpegRuntimeManagerConfig {
            managed_root: temp.path().to_path_buf(),
            release_tag: "v1.0.2".to_owned(),
            release_tag_pinned: true,
            allow_download: true,
        };
        let client = FakeClient::default();
        client.add_release(release);
        client.add_download(archive_url, archive);
        let validator = FakeValidator::default();

        let report =
            resolve_or_acquire_ffmpeg_runtime_with(&config, None, None, &client, &validator)
                .await
                .expect("runtime");

        assert_eq!(report.acquisition.source, FfmpegRuntimeSource::Downloaded);
        assert_eq!(report.acquisition.previous_tag.as_deref(), Some("v1.0.1"));
        assert_eq!(report.acquisition.installed_tag.as_deref(), Some("v1.0.2"));
    }

    #[tokio::test]
    async fn missing_runtime_downloads_rid_specific_package() {
        let temp = tempfile::tempdir().expect("temp");
        let rid = current_ersatzrs_ffmpeg_rid().expect("rid");
        let archive = package_zip();
        let checksum = sha256_hex(&archive);
        let archive_name = format!("ersatzrs-ffmpeg-1.0.1-{rid}.zip");
        let archive_url = "https://example.test/archive.zip";
        let checksum_url = "https://example.test/archive.zip.sha256";
        let release =
            release_with_package("v1.0.1", &archive_name, archive_url, checksum_url, None);
        let config = FfmpegRuntimeManagerConfig::new(temp.path());
        let client = FakeClient::with_latest_release(release);
        client.add_download(archive_url, archive);
        client.add_download(
            checksum_url,
            format!("{checksum}  {archive_name}\n").into_bytes(),
        );
        let validator = FakeValidator::default();

        let report =
            resolve_or_acquire_ffmpeg_runtime_with(&config, None, None, &client, &validator)
                .await
                .expect("runtime");

        assert_eq!(report.acquisition.source, FfmpegRuntimeSource::Downloaded);
        assert_eq!(
            report.acquisition.asset_name.as_deref(),
            Some(archive_name.as_str())
        );
        assert!(Path::new(&report.ffmpeg.path).starts_with(temp.path().join(CURRENT_DIR_NAME)));
        assert!(Path::new(&report.ffprobe.path).starts_with(temp.path().join(CURRENT_DIR_NAME)));
        assert!(temp
            .path()
            .join(CURRENT_DIR_NAME)
            .join("bin")
            .join(executable_name(DEFAULT_FFMPEG_PATH))
            .is_file());
    }

    #[cfg(unix)]
    #[test]
    fn stage_runtime_archive_wraps_unix_tools_when_package_has_lib_dir() {
        let temp = tempfile::tempdir().expect("temp");
        let archive = package_zip_with_root_lib_dir();

        let staged =
            stage_runtime_archive(&archive, "ersatzrs-ffmpeg-1.0.1-linux-x64.zip", temp.path())
                .expect("staged runtime");

        let ffmpeg = PathBuf::from(&staged.ffmpeg_path);
        let ffprobe = PathBuf::from(&staged.ffprobe_path);
        assert!(ffmpeg.with_file_name("ffmpeg.real").is_file());
        assert!(ffprobe.with_file_name("ffprobe.real").is_file());
        assert!(ffmpeg.parent().expect("tool dir").join("lib").is_dir());
        assert!(fs::read_to_string(&ffmpeg)
            .expect("launcher script")
            .contains(MANAGED_LIBRARY_PATH_ENV));

        remove_path(&staged.temp_dir).expect("cleanup");
    }

    #[tokio::test]
    async fn checksum_mismatch_is_rejected() {
        let temp = tempfile::tempdir().expect("temp");
        let rid = current_ersatzrs_ffmpeg_rid().expect("rid");
        let archive = package_zip();
        let archive_name = format!("ersatzrs-ffmpeg-1.0.1-{rid}.zip");
        let archive_url = "https://example.test/archive.zip";
        let release = FfmpegReleaseMetadata {
            tag_name: "v1.0.1".to_owned(),
            assets: vec![FfmpegReleaseAsset {
                name: archive_name,
                download_url: archive_url.to_owned(),
                digest: Some(format!("sha256:{}", "0".repeat(64))),
            }],
        };
        let config = FfmpegRuntimeManagerConfig::new(temp.path());
        let client = FakeClient::with_latest_release(release);
        client.add_download(archive_url, archive);
        let validator = FakeValidator {
            invalid_contains: vec![executable_name(DEFAULT_FFMPEG_PATH)],
        };

        let error =
            resolve_or_acquire_ffmpeg_runtime_with(&config, None, None, &client, &validator)
                .await
                .expect_err("checksum error");

        assert!(matches!(
            error,
            FfmpegRuntimeManagerError::ChecksumMismatch { .. }
        ));
    }

    #[test]
    fn package_selection_distinguishes_arm_architectures_in_any_asset_order() {
        let names = [
            "ersatzrs-ffmpeg-2.0.5-linux-arm.tar.gz",
            "ersatzrs-ffmpeg-2.0.5-linux-arm64.tar.gz",
            "ersatzrs-ffmpeg-2.0.4-linux-x64.tar.gz",
            "ersatzrs-ffmpeg-2.0.4-win-x64.zip",
        ];
        let mut release = FfmpegReleaseMetadata {
            tag_name: "v2.0.5".to_owned(),
            assets: names
                .iter()
                .map(|name| FfmpegReleaseAsset {
                    name: (*name).to_owned(),
                    download_url: format!("https://example.test/{name}"),
                    digest: None,
                })
                .collect(),
        };
        for _ in 0..2 {
            assert_eq!(
                select_package_asset(&release, "linux-arm").map(|asset| asset.name.as_str()),
                Some(names[0])
            );
            assert_eq!(
                select_package_asset(&release, "linux-arm64").map(|asset| asset.name.as_str()),
                Some(names[1])
            );
            assert!(select_package_asset(&release, "osx-arm64").is_none());
            release.assets.reverse();
        }
        release.assets.retain(|asset| asset.name != names[0]);
        assert!(select_package_asset(&release, "linux-arm").is_none());
    }

    // Use an independently qualified archive and its pinned digest. Exercise the
    // installer behind explicit override selection without changing process env,
    // production release pins, or ADR 0008's update/PATH fallback policy.
    #[tokio::test]
    #[ignore = "requires qualified native original-PTS archive and SHA256 fixture env"]
    #[allow(
        clippy::disallowed_methods,
        reason = "manual opt-in test reads only its explicit archive fixture settings"
    )]
    async fn real_original_pts_candidate_installs_and_reuses_managed_runtime() {
        let path = PathBuf::from(
            std::env::var("ERSATZRS_FFMPEG_TEST_PACKAGE_ARCHIVE").expect("archive fixture"),
        );
        let checksum =
            std::env::var("ERSATZRS_FFMPEG_TEST_PACKAGE_SHA256").expect("pinned archive digest");
        assert!(is_sha256_hex(&checksum));
        let archive = fs::read(&path).expect("read qualified archive");
        assert_eq!(sha256_hex(&archive), checksum.to_ascii_lowercase());
        let rid = current_ersatzrs_ffmpeg_rid().expect("native RID");
        let archive_name = path.file_name().unwrap().to_str().unwrap();
        let archive_url = "https://example.test/qualified-original-pts";
        let tag = "v2.0.7-teletext-pts-local";
        let release = FfmpegReleaseMetadata {
            tag_name: tag.to_owned(),
            assets: vec![FfmpegReleaseAsset {
                name: archive_name.to_owned(),
                download_url: archive_url.to_owned(),
                digest: Some(format!("sha256:{checksum}")),
            }],
        };
        assert!(select_package_asset(&release, rid).is_some());
        let temp = tempfile::tempdir().expect("isolated managed root");
        let mut config = FfmpegRuntimeManagerConfig::new(temp.path());
        config.release_tag = tag.to_owned();
        config.release_tag_pinned = true;
        let client = FakeClient::default();
        client.add_release(release.clone());
        client.add_download(archive_url, archive);
        let validator = CapabilityRuntimeValidator;
        let selected = release_for_install(&config, &client).await.unwrap();
        let installed = download_release_install_and_validate(
            &config, rid, selected, None, &client, &validator,
        )
        .await
        .expect("real archive passes staging capability gate and promotion");
        assert_eq!(
            installed.acquisition.source,
            FfmpegRuntimeSource::Downloaded
        );
        assert_eq!(installed.acquisition.installed_tag.as_deref(), Some(tag));
        assert!(installed
            .capabilities
            .teletext_decoder
            .preserves_original_pts());
        assert!(!installed.has_errors());
        assert_eq!(client.download_call_count(), 1);
        assert_eq!(*client.latest_calls.lock().unwrap(), 0);
        assert_eq!(*client.release_calls.lock().unwrap(), 1);

        // Revalidate the promoted paths, including generated Unix launchers and
        // their relocated library layout, with unavailable release metadata.
        config.allow_download = false;
        let offline = FakeClient::default();
        let candidate = managed_candidate(temp.path()).expect("promoted pair");
        let mut cached = resolve_managed_candidate(candidate, rid, &config, &offline, &validator)
            .await
            .expect("actual installed runtime remains usable offline");
        assert_eq!(cached.acquisition.source, FfmpegRuntimeSource::Managed);
        assert_eq!(cached.acquisition.installed_tag.as_deref(), Some(tag));
        assert!(cached.acquisition.update_check_error.is_some());
        assert_eq!(offline.download_call_count(), 0);
        assert!(!cached.has_errors());
        assert!(cached
            .capabilities
            .teletext_decoder
            .preserves_original_pts());
        assert_eq!(cached.ffmpeg.sha256, installed.ffmpeg.sha256);
        assert_eq!(cached.ffprobe.sha256, installed.ffprobe.sha256);
        assert_published_runtime_identity(&mut cached);

        // A rejected replacement must leave the accepted installation intact.
        // Digest enforcement precedes staging and real validation; no PATH or
        // override fallback can mask this direct installer-boundary failure.
        let before_ffmpeg = fs::read(&cached.ffmpeg.path).unwrap();
        let before_ffprobe = fs::read(&cached.ffprobe.path).unwrap();
        let before_tag = fs::read(temp.path().join(CURRENT_TAG_FILE)).unwrap();
        let bad = FakeClient::default();
        bad.add_download(archive_url, b"corrupt replacement".to_vec());
        let error = download_release_install_and_validate(
            &config,
            rid,
            release,
            Some(tag.to_owned()),
            &bad,
            &validator,
        )
        .await
        .expect_err("checksum mismatch before promotion");
        assert!(matches!(
            error,
            FfmpegRuntimeManagerError::ChecksumMismatch { .. }
        ));
        assert_eq!(fs::read(&cached.ffmpeg.path).unwrap(), before_ffmpeg);
        assert_eq!(fs::read(&cached.ffprobe.path).unwrap(), before_ffprobe);
        assert_eq!(
            fs::read(temp.path().join(CURRENT_TAG_FILE)).unwrap(),
            before_tag
        );
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    }

    fn assert_published_runtime_identity(report: &mut FfmpegRuntimeReport) {
        let actual_hash = |path: &str| Some(sha256_hex(&fs::read(path).unwrap()));
        assert_eq!(report.ffmpeg.sha256, actual_hash(&report.ffmpeg.path));
        assert_eq!(report.ffprobe.sha256, actual_hash(&report.ffprobe.path));
        report.set_generation(1);
        let identity = report.capabilities.runtime_identity.as_ref().unwrap();
        assert_eq!(identity.ffmpeg_path, report.ffmpeg.path);
        assert_eq!(identity.ffprobe_path, report.ffprobe.path);
        assert_eq!(identity.ffmpeg_sha256, report.ffmpeg.sha256);
        assert_eq!(identity.ffprobe_sha256, report.ffprobe.sha256);
        assert!(Path::new(&identity.ffmpeg_path).is_file());
        assert!(Path::new(&identity.ffprobe_path).is_file());
    }

    fn seed_managed_runtime(root: &Path, tag: &str) {
        let bin = root.join(CURRENT_DIR_NAME).join("bin");
        fs::create_dir_all(&bin).expect("bin dir");
        fs::write(bin.join(executable_name(DEFAULT_FFMPEG_PATH)), b"ffmpeg").expect("ffmpeg");
        fs::write(bin.join(executable_name(DEFAULT_FFPROBE_PATH)), b"ffprobe").expect("ffprobe");
        fs::write(root.join(CURRENT_TAG_FILE), tag).expect("tag");
    }

    fn release_with_package(
        tag: &str,
        archive_name: &str,
        archive_url: &str,
        checksum_url: &str,
        digest: Option<String>,
    ) -> FfmpegReleaseMetadata {
        FfmpegReleaseMetadata {
            tag_name: tag.to_owned(),
            assets: vec![
                FfmpegReleaseAsset {
                    name: archive_name.to_owned(),
                    download_url: archive_url.to_owned(),
                    digest,
                },
                FfmpegReleaseAsset {
                    name: format!("{archive_name}.sha256"),
                    download_url: checksum_url.to_owned(),
                    digest: None,
                },
            ],
        }
    }

    fn package_zip() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file(
                format!("package/bin/{}", executable_name(DEFAULT_FFMPEG_PATH)),
                options,
            )
            .expect("ffmpeg entry");
            zip.write_all(b"ffmpeg").expect("ffmpeg bytes");
            zip.start_file(
                format!("package/bin/{}", executable_name(DEFAULT_FFPROBE_PATH)),
                options,
            )
            .expect("ffprobe entry");
            zip.write_all(b"ffprobe").expect("ffprobe bytes");
            zip.finish().expect("finish zip");
        }
        bytes.into_inner()
    }

    #[cfg(unix)]
    fn package_zip_with_root_lib_dir() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file(
                format!("ffmpeg/{}", executable_name(DEFAULT_FFMPEG_PATH)),
                options,
            )
            .expect("ffmpeg entry");
            zip.write_all(b"ffmpeg").expect("ffmpeg bytes");
            zip.start_file(
                format!("ffmpeg/{}", executable_name(DEFAULT_FFPROBE_PATH)),
                options,
            )
            .expect("ffprobe entry");
            zip.write_all(b"ffprobe").expect("ffprobe bytes");
            zip.start_file("ffmpeg/lib/libplacebo.so.349", options)
                .expect("lib entry");
            zip.write_all(b"libplacebo").expect("lib bytes");
            zip.finish().expect("zip finish");
        }
        bytes.into_inner()
    }
}
