//! Exact accepted Next acquisition. Pins are shared with release staging.
//!
//! Automatic packages retain ADR 0117's source/patch provenance and retirement
//! contracts. Explicit source-build overrides remain operator-owned; the bounded
//! version probe checks launch/output, not equivalence to the accepted snapshot.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs::{self, File},
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    sync::RwLock,
    time::Duration,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_ORIGIN: u64 = 64 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(1);
const LOCK_TIMEOUT: Duration = Duration::from_secs(30);
const MANIFEST: &str = include_str!("../../../../scripts/release/next-patches/manifest.json");
static RESOLVED: RwLock<Option<NextRuntimeAcquisition>> = RwLock::new(None);

/// Accepted executable location and acquisition diagnostics. Paths must not be logged.
#[derive(Clone)]
pub struct NextRuntimeAcquisition {
    /// The executable every Next spawn/version/health path uses.
    pub binary: PathBuf,
    /// Acquisition branch; custom overrides are not published-pin acceptance.
    pub source: NextRuntimeSource,
    /// Selected supported platform RID.
    pub rid: String,
    /// Exact accepted automatic distribution, or the custom override's version.
    pub version: String,
    /// True only after all automatic payload/provenance hashes pass.
    pub published_pin_verified: bool,
    folder: PathBuf,
    split_bundle: bool,
}

/// Runtime source, without exposing operator paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextRuntimeSource {
    /// Strict operator-owned path; no automatic fallback.
    Override,
    /// Exact accepted package shipped beside the host.
    Bundled,
    /// Exact accepted persistent installation; no network required.
    Managed,
    /// Exact accepted package acquired during this startup.
    Downloaded,
}

/// Redacted startup failures; no silent native-engine substitution.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NextRuntimeError {
    /// Platform does not match a supported RID.
    #[error("Next runtime acquisition is unsupported on this platform")]
    Platform,
    /// Compiled pin is incomplete or inconsistent.
    #[error("accepted Next runtime manifest is invalid")]
    Manifest,
    /// An explicit path is missing or fails the bounded version probe.
    #[error("explicit ETV_NEXT_FOLDER is invalid; correct or remove the override")]
    Override,
    /// A package/member/provenance identity differs from the accepted pin.
    #[error("Next package identity differs from the accepted distribution")]
    Identity,
    /// Accepted automatic bytes cannot execute the expected bounded version command.
    #[error(
        "accepted Next runtime cannot start; check package permissions and platform dependencies"
    )]
    Execution,
    /// Persistent state cannot be read/staged/published safely.
    #[error(
        "Next runtime state is inaccessible; check the persistent state directory permissions"
    )]
    Storage,
    /// Another startup owns installation beyond the bounded wait.
    #[error("Next runtime installation is busy; retry after the other startup completes")]
    Busy,
    /// Downloads forbidden and no exact local copy exists.
    #[error("accepted Next runtime is missing and downloads are disabled")]
    Offline,
    /// Exact immutable package download failed or exceeded byte/time bounds.
    #[error("accepted Next runtime download failed; check connectivity or install the exact release package offline")]
    Download,
}

/// Explicit startup inputs. The managed directory must be persistent state, not TEMP.
#[derive(Clone)]
pub struct NextRuntimeManagerConfig {
    /// Operator-owned override; invalid overrides never use another branch.
    pub override_folder: Option<PathBuf>,
    /// Host executable, for package-local discovery.
    pub host_executable: PathBuf,
    /// Home used only for the existing leading-tilde override expansion.
    pub home: Option<PathBuf>,
    /// Persistent `<state>/next` install root.
    pub managed_root: PathBuf,
    /// Permit a missing automatic installation to download the fixed pin.
    pub allow_download: bool,
}

impl NextRuntimeManagerConfig {
    /// Load immutable startup settings without changing process environment.
    /// # Errors
    /// Returns a redacted error when the host executable cannot be located.
    pub fn from_startup() -> Result<Self, NextRuntimeError> {
        Ok(Self {
            override_folder: crate::startup_config::next_folder(),
            host_executable: std::env::current_exe().map_err(|_| NextRuntimeError::Storage)?,
            home: crate::startup_config::StartupConfig::load().home_dir,
            managed_root: crate::file_system_layout::app_state_folder().join("next"),
            allow_download: true,
        })
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
struct Patch {
    file: String,
    sha256: String,
}

#[derive(Clone, Deserialize)]
struct Package {
    target: String,
    asset: String,
    payload_sha256: BTreeMap<String, String>,
    payload_bytes: BTreeMap<String, u64>,
}

#[derive(Clone, Deserialize)]
struct Manifest {
    upstream_repository: String,
    upstream_revision: String,
    source_archive_sha256: String,
    source_lock_sha256: String,
    distribution_version: String,
    distribution_repository: String,
    distribution_tag: String,
    patches: Vec<Patch>,
    artifact_sha256: BTreeMap<String, String>,
    runtime_packages: BTreeMap<String, Package>,
}

impl Manifest {
    fn accepted() -> Result<Self, NextRuntimeError> {
        serde_json::from_str(MANIFEST).map_err(|_| NextRuntimeError::Manifest)
    }

    fn package(&self, rid: &str) -> Result<&Package, NextRuntimeError> {
        let package = self
            .runtime_packages
            .get(rid)
            .ok_or(NextRuntimeError::Platform)?;
        let names = payload_names(rid);
        if package.payload_sha256.keys().ne(names.iter())
            || package.payload_bytes.keys().ne(names.iter())
            || package
                .payload_bytes
                .values()
                .any(|size| *size == 0 || *size > MAX_ARCHIVE)
            || package
                .payload_bytes
                .values()
                .try_fold(0u64, |sum, size| sum.checked_add(*size))
                .is_none_or(|sum| sum > MAX_ARCHIVE)
            || !self.artifact_sha256.contains_key(&package.asset)
            || package.asset
                != format!(
                    "ersatzrs-next-{}-{rid}{}",
                    self.distribution_version,
                    if rid == "win-x64" { ".zip" } else { ".tar.gz" }
                )
        {
            return Err(NextRuntimeError::Manifest);
        }
        Ok(package)
    }

    fn download_base(&self) -> String {
        format!(
            "https://github.com/{}/releases/download/{}",
            self.distribution_repository, self.distribution_tag
        )
    }
}

fn payload_names(rid: &str) -> Vec<String> {
    let mut names = vec![
        "LICENSE".to_owned(),
        format!(
            "ersatztv-channel{}",
            if rid == "win-x64" { ".exe" } else { "" }
        ),
    ];
    if rid == "win-x64" {
        names.extend(["libvpl-LICENSE.txt".to_owned(), "libvpl.dll".to_owned()]);
    }
    names.sort();
    names
}

fn digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn bounded_file(path: &Path, limit: u64) -> Result<Vec<u8>, NextRuntimeError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| NextRuntimeError::Identity)?;
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() == 0 || metadata.len() > limit
    {
        return Err(NextRuntimeError::Identity);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| NextRuntimeError::Identity)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| NextRuntimeError::Identity)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(NextRuntimeError::Identity);
    }
    Ok(bytes)
}

fn verify_files(
    manifest: &Manifest,
    rid: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), NextRuntimeError> {
    let package = manifest.package(rid)?;
    if files.len() != package.payload_sha256.len() + 1 {
        return Err(NextRuntimeError::Identity);
    }
    for (name, expected) in &package.payload_sha256 {
        let bytes = files.get(name).ok_or(NextRuntimeError::Identity)?;
        if digest(bytes) != *expected
            || u64::try_from(bytes.len()).ok() != package.payload_bytes.get(name).copied()
        {
            return Err(NextRuntimeError::Identity);
        }
    }
    let origin: serde_json::Value =
        serde_json::from_slice(files.get("origin.json").ok_or(NextRuntimeError::Identity)?)
            .map_err(|_| NextRuntimeError::Identity)?;
    for (name, expected) in [
        ("kind", "patched-source-build"),
        ("repository", manifest.upstream_repository.as_str()),
        ("source_revision", manifest.upstream_revision.as_str()),
        (
            "source_archive_sha256",
            manifest.source_archive_sha256.as_str(),
        ),
        ("cargo_lock_sha256", manifest.source_lock_sha256.as_str()),
        (
            "distribution_version",
            manifest.distribution_version.as_str(),
        ),
        ("rid", rid),
        ("target", package.target.as_str()),
    ] {
        if origin[name].as_str() != Some(expected) {
            return Err(NextRuntimeError::Identity);
        }
    }
    let patches: Vec<Patch> = serde_json::from_value(origin["patches"].clone())
        .map_err(|_| NextRuntimeError::Identity)?;
    let hashes: BTreeMap<String, String> = serde_json::from_value(origin["payload_sha256"].clone())
        .map_err(|_| NextRuntimeError::Identity)?;
    if patches != manifest.patches || hashes != package.payload_sha256 {
        return Err(NextRuntimeError::Identity);
    }
    Ok(())
}

fn validate_folder(
    manifest: &Manifest,
    rid: &str,
    folder: &Path,
    split: bool,
) -> Result<(), NextRuntimeError> {
    let metadata = fs::symlink_metadata(folder).map_err(|_| NextRuntimeError::Identity)?;
    if !metadata.is_dir() || metadata.is_symlink() {
        return Err(NextRuntimeError::Identity);
    }
    if split {
        let metadata =
            fs::symlink_metadata(folder.join("next")).map_err(|_| NextRuntimeError::Identity)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(NextRuntimeError::Identity);
        }
    }
    let mut files = BTreeMap::new();
    for (name, size) in &manifest.package(rid)?.payload_bytes {
        let path = if split && name == "LICENSE" {
            folder.join("next").join(name)
        } else {
            folder.join(name)
        };
        files.insert(name.clone(), bounded_file(&path, *size)?);
    }
    files.insert(
        "origin.json".to_owned(),
        bounded_file(
            &if split {
                folder.join("next/origin.json")
            } else {
                folder.join("origin.json")
            },
            MAX_ORIGIN,
        )?,
    );
    verify_files(manifest, rid, &files)
}

fn automatic_result(
    manifest: &Manifest,
    rid: &str,
    folder: PathBuf,
    split_bundle: bool,
    source: NextRuntimeSource,
) -> NextRuntimeAcquisition {
    NextRuntimeAcquisition {
        binary: folder.join(format!(
            "ersatztv-channel{}",
            if rid == "win-x64" { ".exe" } else { "" }
        )),
        source,
        rid: rid.to_owned(),
        version: manifest.distribution_version.clone(),
        published_pin_verified: true,
        folder,
        split_bundle,
    }
}

fn local_automatic(
    config: &NextRuntimeManagerConfig,
    manifest: &Manifest,
    rid: &str,
) -> Option<NextRuntimeAcquisition> {
    if let Some(bundle) = config.host_executable.parent() {
        if validate_folder(manifest, rid, bundle, true).is_ok() {
            return Some(automatic_result(
                manifest,
                rid,
                bundle.to_owned(),
                true,
                NextRuntimeSource::Bundled,
            ));
        }
    }
    let folder = config
        .managed_root
        .join(&manifest.distribution_tag)
        .join(rid);
    if validate_folder(manifest, rid, &folder, false).is_ok() {
        return Some(automatic_result(
            manifest,
            rid,
            folder,
            false,
            NextRuntimeSource::Managed,
        ));
    }
    None
}

/// Acquire/validate the runtime and publish one accepted location for all consumers.
/// No valid offline bundle/cache triggers a network check. Custom overrides are
/// launch-probed strictly and explicitly labelled unverified against the release.
/// # Errors
/// Returns an actionable redacted error; no native-engine fallback is performed.
pub async fn ensure(
    config: NextRuntimeManagerConfig,
) -> Result<NextRuntimeAcquisition, NextRuntimeError> {
    let manifest = Manifest::accepted()?;
    let base = manifest.download_base();
    let accepted = acquire(config, manifest, &base).await?;
    if accepted.published_pin_verified
        && super::version::probe_binary(&accepted.binary)
            .await
            .as_deref()
            != Some(accepted.version.as_str())
    {
        return Err(NextRuntimeError::Execution);
    }
    *RESOLVED.write().map_err(|_| NextRuntimeError::Storage)? = Some(accepted.clone());
    Ok(accepted)
}

async fn acquire(
    config: NextRuntimeManagerConfig,
    manifest: Manifest,
    base: &str,
) -> Result<NextRuntimeAcquisition, NextRuntimeError> {
    let rid = ersatzrs_ffmpeg::runtime::current_ersatzrs_ffmpeg_rid()
        .ok_or(NextRuntimeError::Platform)?
        .to_owned();
    manifest.package(&rid)?;
    if config
        .override_folder
        .as_ref()
        .is_some_and(|folder| !folder.as_os_str().is_empty())
    {
        let binary = super::runtime::resolve_channel_binary(
            config.override_folder.as_deref(),
            &config.host_executable,
            config.home.as_deref(),
        )
        .map_err(|_| NextRuntimeError::Override)?;
        let version = super::version::probe_binary(&binary)
            .await
            .ok_or(NextRuntimeError::Override)?;
        return Ok(NextRuntimeAcquisition {
            folder: binary
                .parent()
                .ok_or(NextRuntimeError::Override)?
                .to_owned(),
            binary,
            source: NextRuntimeSource::Override,
            rid,
            version,
            published_pin_verified: false,
            split_bundle: false,
        });
    }
    let check_config = config.clone();
    let check_manifest = manifest.clone();
    let check_rid = rid.clone();
    if let Some(accepted) = tokio::task::spawn_blocking(move || {
        local_automatic(&check_config, &check_manifest, &check_rid)
    })
    .await
    .map_err(|_| NextRuntimeError::Storage)?
    {
        return Ok(accepted);
    }
    if !config.allow_download {
        return Err(NextRuntimeError::Offline);
    }
    let deadline = tokio::time::Instant::now() + LOCK_TIMEOUT;
    let lock = loop {
        let root = config.managed_root.clone();
        let result = tokio::task::spawn_blocking(move || try_install_lock(&root))
            .await
            .map_err(|_| NextRuntimeError::Storage)??;
        if let Some(lock) = result {
            break lock;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(NextRuntimeError::Busy);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    // Another startup may have installed while this process waited for its lock.
    let check_config = config.clone();
    let check_manifest = manifest.clone();
    let check_rid = rid.clone();
    if let Some(accepted) = tokio::task::spawn_blocking(move || {
        local_automatic(&check_config, &check_manifest, &check_rid)
    })
    .await
    .map_err(|_| NextRuntimeError::Storage)?
    {
        return Ok(accepted);
    }
    let package = manifest.package(&rid)?;
    let archive = download(&format!("{base}/{}", package.asset)).await?;
    // The owned blocking task retains the cross-process lease and its TempDir if
    // startup is cancelled, finishing only bounded file work (no child process).
    tokio::task::spawn_blocking(move || {
        let _lock = lock;
        install(&config.managed_root, &manifest, &rid, &archive)
    })
    .await
    .map_err(|_| NextRuntimeError::Storage)?
}

fn ordinary_directory(path: &Path) -> Result<(), NextRuntimeError> {
    fs::create_dir_all(path).map_err(|_| NextRuntimeError::Storage)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| NextRuntimeError::Storage)?;
    if !metadata.is_dir() || metadata.is_symlink() {
        return Err(NextRuntimeError::Storage);
    }
    Ok(())
}

fn try_install_lock(root: &Path) -> Result<Option<File>, NextRuntimeError> {
    ordinary_directory(root)?;
    let path = root.join(".install.lock");
    if fs::symlink_metadata(&path)
        .is_ok_and(|metadata| !metadata.is_file() || metadata.is_symlink())
    {
        return Err(NextRuntimeError::Storage);
    }
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|_| NextRuntimeError::Storage)?;
    match fs4::FileExt::try_lock(&lock) {
        Ok(()) => Ok(Some(lock)),
        Err(fs4::TryLockError::WouldBlock) => Ok(None),
        Err(_) => Err(NextRuntimeError::Storage),
    }
}

async fn download(url: &str) -> Result<Vec<u8>, NextRuntimeError> {
    download_with_limit(url, MAX_ARCHIVE).await
}

async fn download_with_limit(url: &str, limit: u64) -> Result<Vec<u8>, NextRuntimeError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(DOWNLOAD_TIMEOUT)
        .https_only(!cfg!(test))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("ErsatzRS-Next-runtime-manager")
        .build()
        .map_err(|_| NextRuntimeError::Download)?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| NextRuntimeError::Download)?
        .error_for_status()
        .map_err(|_| NextRuntimeError::Download)?;
    if response.content_length().is_some_and(|size| size > limit) {
        return Err(NextRuntimeError::Download);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| NextRuntimeError::Download)?
    {
        if chunk.len()
            > usize::try_from(limit)
                .unwrap_or(usize::MAX)
                .saturating_sub(bytes.len())
        {
            return Err(NextRuntimeError::Download);
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(NextRuntimeError::Download);
    }
    Ok(bytes)
}

fn archive_files(
    manifest: &Manifest,
    rid: &str,
    archive: &[u8],
) -> Result<BTreeMap<String, Vec<u8>>, NextRuntimeError> {
    let package = manifest.package(rid)?;
    if archive.is_empty()
        || u64::try_from(archive.len()).unwrap_or(u64::MAX) > MAX_ARCHIVE
        || manifest.artifact_sha256.get(&package.asset) != Some(&digest(archive))
    {
        return Err(NextRuntimeError::Identity);
    }
    let root = package
        .asset
        .trim_end_matches(".tar.gz")
        .trim_end_matches(".zip");
    let mut expected = package.payload_bytes.clone();
    expected.insert("origin.json".to_owned(), MAX_ORIGIN);
    let mut result = BTreeMap::new();
    let mut add = |path: &str, size: u64, reader: &mut dyn Read| -> Result<(), NextRuntimeError> {
        let name = path
            .strip_prefix(&format!("{root}/"))
            .ok_or(NextRuntimeError::Identity)?;
        let limit = *expected.get(name).ok_or(NextRuntimeError::Identity)?;
        if size == 0 || size > limit || result.contains_key(name) {
            return Err(NextRuntimeError::Identity);
        }
        let mut bytes = Vec::new();
        reader
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| NextRuntimeError::Identity)?;
        if u64::try_from(bytes.len()).ok() != Some(size) {
            return Err(NextRuntimeError::Identity);
        }
        result.insert(name.to_owned(), bytes);
        Ok(())
    };
    if rid == "win-x64" {
        let mut zip =
            zip::ZipArchive::new(Cursor::new(archive)).map_err(|_| NextRuntimeError::Identity)?;
        if zip.len() > expected.len() + 1 {
            return Err(NextRuntimeError::Identity);
        }
        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|_| NextRuntimeError::Identity)?;
            let path = entry
                .name()
                .map_err(|_| NextRuntimeError::Identity)?
                .into_owned();
            if entry.is_dir() && (path == root || path == format!("{root}/")) && entry.size() == 0 {
                continue;
            }
            if entry.is_dir()
                || entry
                    .unix_mode()
                    .is_some_and(|mode| mode & 0o170_000 != 0 && mode & 0o170_000 != 0o100_000)
            {
                return Err(NextRuntimeError::Identity);
            }
            add(&path, entry.size(), &mut entry)?;
        }
    } else {
        let mut tar = tar::Archive::new(
            flate2::read::GzDecoder::new(Cursor::new(archive))
                .take(MAX_ARCHIVE + MAX_ORIGIN + 8192),
        );
        let mut count = 0;
        for entry in tar.entries().map_err(|_| NextRuntimeError::Identity)? {
            count += 1;
            if count > expected.len() + 1 {
                return Err(NextRuntimeError::Identity);
            }
            let mut entry = entry.map_err(|_| NextRuntimeError::Identity)?;
            let path = entry
                .path()
                .map_err(|_| NextRuntimeError::Identity)?
                .to_str()
                .ok_or(NextRuntimeError::Identity)?
                .to_owned();
            if entry.header().entry_type().is_dir()
                && (path == root || path == format!("{root}/"))
                && entry.size() == 0
            {
                continue;
            }
            if !entry.header().entry_type().is_file() {
                return Err(NextRuntimeError::Identity);
            }
            add(&path, entry.size(), &mut entry)?;
        }
    }
    verify_files(manifest, rid, &result)?;
    Ok(result)
}

fn install(
    root: &Path,
    manifest: &Manifest,
    rid: &str,
    archive: &[u8],
) -> Result<NextRuntimeAcquisition, NextRuntimeError> {
    let files = archive_files(manifest, rid, archive)?;
    let parent = root.join(&manifest.distribution_tag);
    ordinary_directory(&parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".next-")
        .tempdir_in(&parent)
        .map_err(|_| NextRuntimeError::Storage)?;
    for (name, bytes) in files {
        let path = staging.path().join(&name);
        fs::write(&path, bytes).map_err(|_| NextRuntimeError::Storage)?;
        #[cfg(unix)]
        if name == "ersatztv-channel" {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .map_err(|_| NextRuntimeError::Storage)?;
        }
        // Windows packages use native regular-file permissions; no Unix mode applies.
    }
    validate_folder(manifest, rid, staging.path(), false)?;
    let destination = parent.join(rid);
    let backup = parent.join(format!(".previous-{}", uuid::Uuid::new_v4()));
    let existed = destination
        .try_exists()
        .map_err(|_| NextRuntimeError::Storage)?;
    if existed {
        let metadata = fs::symlink_metadata(&destination).map_err(|_| NextRuntimeError::Storage)?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(NextRuntimeError::Storage);
        }
        fs::rename(&destination, &backup).map_err(|_| NextRuntimeError::Storage)?;
    }
    if fs::rename(staging.path(), &destination).is_err() {
        if existed {
            let _ = fs::rename(&backup, &destination);
        }
        return Err(NextRuntimeError::Storage);
    }
    if existed {
        fs::remove_dir_all(backup).map_err(|_| NextRuntimeError::Storage)?;
    }
    Ok(automatic_result(
        manifest,
        rid,
        destination,
        false,
        NextRuntimeSource::Downloaded,
    ))
}

/// Return the already accepted startup location. Before composition-root startup,
/// existing explicit/sibling resolution remains available to scoped callers.
/// # Errors
/// Returns a redacted error for missing or inaccessible executables.
pub fn channel_binary() -> io::Result<PathBuf> {
    if let Some(accepted) = RESOLVED
        .read()
        .map_err(|_| io::Error::other("Next runtime state is unavailable"))?
        .as_ref()
    {
        if fs::metadata(&accepted.binary).is_ok_and(|metadata| metadata.is_file()) {
            return Ok(accepted.binary.clone());
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "accepted Next executable is unavailable",
        ));
    }
    let host =
        std::env::current_exe().map_err(|_| io::Error::other("host executable is unavailable"))?;
    super::runtime::resolve_channel_binary(
        crate::startup_config::next_folder().as_deref(),
        &host,
        crate::startup_config::StartupConfig::load()
            .home_dir
            .as_deref(),
    )
}

/// Revalidate the same published startup runtime for health, never acquire/network.
/// # Errors
/// Returns an identity/launch failure if the accepted runtime has changed.
pub async fn validate_published() -> Result<(), NextRuntimeError> {
    let accepted = RESOLVED
        .read()
        .map_err(|_| NextRuntimeError::Storage)?
        .clone();
    let Some(accepted) = accepted else {
        return Ok(());
    };
    if accepted.source == NextRuntimeSource::Override {
        if super::version::probe_binary(&accepted.binary)
            .await
            .as_deref()
            != Some(accepted.version.as_str())
        {
            return Err(NextRuntimeError::Override);
        }
        return Ok(());
    }
    validate_automatic(
        &accepted,
        Manifest::accepted()?,
        super::version::probe_binary(&accepted.binary),
    )
    .await
}

// The lazy probe must not be polled until the automatic package is trusted.
// Keeping that ordering here also lets the portable synthetic fixture observe
// whether an identity mismatch attempts any launch without executing test bytes.
async fn validate_automatic(
    accepted: &NextRuntimeAcquisition,
    manifest: Manifest,
    probe: impl std::future::Future<Output = Option<String>>,
) -> Result<(), NextRuntimeError> {
    let check = accepted.clone();
    tokio::task::spawn_blocking(move || {
        validate_folder(&manifest, &check.rid, &check.folder, check.split_bundle)
    })
    .await
    .map_err(|_| NextRuntimeError::Storage)??;
    if probe.await.as_deref() != Some(accepted.version.as_str()) {
        return Err(NextRuntimeError::Execution);
    }
    Ok(())
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
