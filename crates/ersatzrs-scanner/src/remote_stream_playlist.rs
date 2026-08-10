//! Remote Stream playlist manifests and allowlisted extractor enumeration.
//!
//! ADR 0065 keeps scan-time process execution separate from hand-authored
//! Remote Stream `script` definitions. A manifest selects one server-configured
//! extractor by name; this module validates the file and runs its bounded
//! `list` command without invoking a shell.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use chrono::{DateTime, Datelike, Utc};
use ersatzrs_infra::remote_stream::{
    parse_script_command, sanitize_remote_stream_diagnostic, RemoteStreamPerson,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const ENUMERATION_TIMEOUT: Duration = Duration::from_mins(2);
const MAX_ENUMERATION_BYTES: usize = 8 * 1024 * 1024;
const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;
const MAX_ENUMERATION_ENTRIES: usize = 10_000;

/// Persisted `ConfigElement` key containing the named extractor array as JSON.
pub const CONFIG_KEY_REMOTE_STREAM_EXTRACTORS: &str = "settings.remote_stream_extractors";

/// Suffixes that distinguish playlist manifests from ordinary definitions.
pub const MANIFEST_SUFFIXES: &[&str] = &[".playlist.yml", ".playlist.yaml"];

#[derive(Debug, Error)]
pub enum PlaylistError {
    #[error("playlist manifest is invalid: {0}")]
    Manifest(String),
    #[error("remote stream extractor settings are invalid: {0}")]
    ExtractorSettings(String),
    #[error("remote stream extractor '{0}' is not configured")]
    UnknownExtractor(String),
    #[error("remote stream extractor '{name}' is unavailable: {reason}")]
    UnavailableExtractor { name: String, reason: String },
    #[error("remote stream playlist enumeration failed: {0}")]
    Enumeration(String),
}

pub type Result<T> = std::result::Result<T, PlaylistError>;

/// Named operator-owned extractor with commands for both native platform families.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteStreamExtractorConfig {
    pub name: String,
    pub windows: String,
    pub unix: String,
}

/// Validated extractor command selected for the current platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPlaylistExtractor {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteStreamPlaylistManifest {
    pub name: Option<String>,
    pub url: String,
    pub extractor: String,
    pub folder: Option<String>,
    #[serde(default)]
    pub sync: PlaylistSyncOptions,
    #[serde(default)]
    pub defaults: PlaylistDefaults,
    #[serde(default)]
    pub filter: PlaylistFilter,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub create_playlist: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlaylistSyncOptions {
    pub interval: String,
    pub max_items: usize,
    pub order: PlaylistOrder,
    pub on_removed: RemovedEntryPolicy,
}

impl Default for PlaylistSyncOptions {
    fn default() -> Self {
        Self {
            interval: "24h".to_owned(),
            max_items: 500,
            order: PlaylistOrder::Upstream,
            on_removed: RemovedEntryPolicy::Trash,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaylistOrder {
    #[default]
    Upstream,
    NewestFirst,
    Title,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovedEntryPolicy {
    #[default]
    Trash,
    Keep,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlaylistDefaults {
    pub is_live: bool,
    pub content_rating: Option<String>,
    pub year: Option<i32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlaylistFilter {
    pub include_title: Option<String>,
    pub exclude_title: Option<String>,
    pub min_duration: Option<String>,
    pub max_duration: Option<String>,
}

/// Effective manifest values after filename-derived defaults and validation.
#[derive(Clone, Debug)]
pub struct ValidatedPlaylistManifest {
    pub manifest: RemoteStreamPlaylistManifest,
    pub name: String,
    pub folder: String,
    pub interval: Duration,
    pub include_title: Option<Regex>,
    pub exclude_title: Option<Regex>,
    pub min_duration_seconds: Option<u64>,
    pub max_duration_seconds: Option<u64>,
}

/// One provider entry emitted by `<extractor> list`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct PlaylistEntry {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    #[serde(default)]
    pub plot: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub content_rating: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub people: Vec<RemoteStreamPerson>,
    #[serde(default)]
    pub producers: Vec<String>,
    #[serde(default)]
    pub original_broadcasters: Vec<String>,
    #[serde(default)]
    pub broadcasters: Vec<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub is_live: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistEnumeration {
    pub entries: Vec<PlaylistEntry>,
    pub diagnostic_tail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PlaylistSidecarEntry {
    pub id: String,
    pub file: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PlaylistSyncOutcome {
    pub kind: String,
    pub entries: usize,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PlaylistSidecarState {
    pub version: u32,
    pub manifest_etag: String,
    pub playlist_etag: String,
    pub last_sync: DateTime<Utc>,
    pub last_result: PlaylistSyncOutcome,
    pub managed_files: BTreeMap<String, String>,
    pub entries: Vec<PlaylistSidecarEntry>,
    #[serde(default)]
    pub overrides: PlaylistOverrides,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct PlaylistOverrides {
    #[serde(default)]
    pub excluded: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedPlaylistDefinition {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistFileSync {
    pub state: PlaylistSidecarState,
    pub definitions: Vec<ManagedPlaylistDefinition>,
    pub changed: bool,
    pub membership_changed: bool,
    pub enumerated: bool,
}

#[derive(Serialize)]
struct GeneratedRemoteStreamDefinition<'a> {
    script: String,
    is_live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<String>,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    plot: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_date: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_rating: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    genres: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    people: Vec<RemoteStreamPerson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    producers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    original_broadcasters: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    broadcasters: Vec<String>,
}

/// Return whether `path` is a playlist manifest rather than a stream definition.
#[must_use]
pub fn is_playlist_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            MANIFEST_SUFFIXES
                .iter()
                .any(|suffix| lower.ends_with(suffix))
        })
}

/// Parse and validate a manifest using its filename for default name and folder.
///
/// # Errors
///
/// Returns an error when YAML decoding or any manifest invariant fails.
pub fn parse_playlist_manifest(yaml: &str, path: &Path) -> Result<ValidatedPlaylistManifest> {
    let manifest: RemoteStreamPlaylistManifest =
        serde_yaml::from_str(yaml).map_err(|error| PlaylistError::Manifest(error.to_string()))?;
    let stem = manifest_stem(path)?;
    let name = non_empty(manifest.name.as_deref())
        .unwrap_or(&stem)
        .to_owned();
    let folder = non_empty(manifest.folder.as_deref())
        .unwrap_or(&stem)
        .to_owned();
    validate_folder(&folder)?;
    if non_empty(Some(&manifest.url)).is_none() {
        return Err(PlaylistError::Manifest("url is required".to_owned()));
    }
    if non_empty(Some(&manifest.extractor)).is_none() {
        return Err(PlaylistError::Manifest("extractor is required".to_owned()));
    }
    if manifest.sync.max_items > MAX_ENUMERATION_ENTRIES {
        return Err(PlaylistError::Manifest(format!(
            "sync.max_items must be at most {MAX_ENUMERATION_ENTRIES}"
        )));
    }
    let interval = parse_interval(&manifest.sync.interval)?;
    let include_title = compile_optional_regex(
        "filter.include_title",
        manifest.filter.include_title.as_deref(),
    )?;
    let exclude_title = compile_optional_regex(
        "filter.exclude_title",
        manifest.filter.exclude_title.as_deref(),
    )?;
    let min_duration_seconds = parse_optional_duration(
        "filter.min_duration",
        manifest.filter.min_duration.as_deref(),
    )?;
    let max_duration_seconds = parse_optional_duration(
        "filter.max_duration",
        manifest.filter.max_duration.as_deref(),
    )?;
    if min_duration_seconds
        .zip(max_duration_seconds)
        .is_some_and(|(minimum, maximum)| minimum > maximum)
    {
        return Err(PlaylistError::Manifest(
            "filter.min_duration must not exceed filter.max_duration".to_owned(),
        ));
    }
    let mut ids = BTreeSet::new();
    for id in &manifest.exclude {
        let id = id.trim();
        if id.is_empty() {
            return Err(PlaylistError::Manifest(
                "exclude entries must not be empty".to_owned(),
            ));
        }
        if !ids.insert(id.to_ascii_lowercase()) {
            return Err(PlaylistError::Manifest(format!(
                "exclude contains duplicate id '{id}'"
            )));
        }
    }

    Ok(ValidatedPlaylistManifest {
        manifest,
        name,
        folder,
        interval,
        include_title,
        exclude_title,
        min_duration_seconds,
        max_duration_seconds,
    })
}

/// Decode and validate the server-owned named extractor setting.
///
/// # Errors
///
/// Returns an error for invalid JSON, duplicate names, or incomplete commands.
pub fn parse_extractor_settings(json: &str) -> Result<Vec<RemoteStreamExtractorConfig>> {
    let settings: Vec<RemoteStreamExtractorConfig> = serde_json::from_str(json)
        .map_err(|error| PlaylistError::ExtractorSettings(error.to_string()))?;
    let mut names = BTreeSet::new();
    for setting in &settings {
        let name = setting.name.trim();
        if name.is_empty() {
            return Err(PlaylistError::ExtractorSettings(
                "extractor names must not be empty".to_owned(),
            ));
        }
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(PlaylistError::ExtractorSettings(format!(
                "duplicate extractor name '{name}'"
            )));
        }
        if setting.windows.trim().is_empty() || setting.unix.trim().is_empty() {
            return Err(PlaylistError::ExtractorSettings(format!(
                "extractor '{name}' requires both windows and unix commands"
            )));
        }
    }
    Ok(settings)
}

/// Resolve a named command for a requested platform family.
///
/// # Errors
///
/// Returns an error when the name is unknown or the command is not an absolute regular file.
pub fn resolve_extractor_for_platform(
    settings: &[RemoteStreamExtractorConfig],
    name: &str,
    windows: bool,
) -> Result<ResolvedPlaylistExtractor> {
    let setting = settings
        .iter()
        .find(|setting| setting.name.trim().eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| PlaylistError::UnknownExtractor(name.trim().to_owned()))?;
    let raw = if windows {
        setting.windows.trim()
    } else {
        setting.unix.trim()
    };
    let parsed = parse_script_command(raw, Path::new("."))
        .map_err(|error| PlaylistError::ExtractorSettings(error.to_string()))?;
    let program_path = Path::new(&parsed.program);
    if !program_path.is_absolute() {
        return Err(PlaylistError::UnavailableExtractor {
            name: setting.name.clone(),
            reason: "the executable path must be absolute".to_owned(),
        });
    }
    let metadata =
        std::fs::metadata(program_path).map_err(|error| PlaylistError::UnavailableExtractor {
            name: setting.name.clone(),
            reason: error.to_string(),
        })?;
    if !metadata.is_file() {
        return Err(PlaylistError::UnavailableExtractor {
            name: setting.name.clone(),
            reason: "the executable path is not a regular file".to_owned(),
        });
    }
    let working_directory = program_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    Ok(ResolvedPlaylistExtractor {
        name: setting.name.trim().to_owned(),
        program: parsed.program,
        args: parsed.args,
        working_directory,
    })
}

/// Resolve a named extractor for the native build platform.
///
/// # Errors
///
/// Returns an error when the configured native command cannot be resolved.
pub fn resolve_extractor(
    settings: &[RemoteStreamExtractorConfig],
    name: &str,
) -> Result<ResolvedPlaylistExtractor> {
    resolve_extractor_for_platform(settings, name, cfg!(windows))
}

/// Run one bounded playlist enumeration and decode its NDJSON atomically.
///
/// # Errors
///
/// Returns an error for process, timeout, output-bound, exit-status, or NDJSON failures.
pub async fn enumerate_playlist(
    extractor: &ResolvedPlaylistExtractor,
    playlist_url: &str,
) -> Result<PlaylistEnumeration> {
    let mut command = Command::new(&extractor.program);
    command
        .args(&extractor.args)
        .arg("list")
        .arg(playlist_url)
        .current_dir(&extractor.working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        PlaylistError::Enumeration(format!("extractor failed to start: {error}"))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PlaylistError::Enumeration("extractor stdout was unavailable".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| PlaylistError::Enumeration("extractor stderr was unavailable".to_owned()))?;
    let stdout_task = tokio::spawn(read_bounded_output(stdout, MAX_ENUMERATION_BYTES));
    let stderr_task = tokio::spawn(read_tail(stderr, MAX_DIAGNOSTIC_BYTES));
    let status = if let Ok(result) = tokio::time::timeout(ENUMERATION_TIMEOUT, child.wait()).await {
        result.map_err(|error| {
            PlaylistError::Enumeration(format!("extractor wait failed: {error}"))
        })?
    } else {
        let _ = child.kill().await;
        return Err(PlaylistError::Enumeration(
            "extractor exceeded the 120 second timeout".to_owned(),
        ));
    };
    let (stdout, overflowed) = stdout_task
        .await
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    let stderr = stderr_task
        .await
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    let diagnostic_tail = sanitize_remote_stream_diagnostic(&String::from_utf8_lossy(&stderr));
    if overflowed {
        return Err(PlaylistError::Enumeration(format!(
            "extractor output exceeded {MAX_ENUMERATION_BYTES} bytes"
        )));
    }
    if !status.success() {
        let suffix = if diagnostic_tail.is_empty() {
            String::new()
        } else {
            format!(": {diagnostic_tail}")
        };
        return Err(PlaylistError::Enumeration(format!(
            "extractor exited with {status}{suffix}"
        )));
    }
    let text = String::from_utf8(stdout)
        .map_err(|_| PlaylistError::Enumeration("extractor output is not UTF-8".to_owned()))?;
    let entries = parse_ndjson_entries(&text)?;
    Ok(PlaylistEnumeration {
        entries,
        diagnostic_tail,
    })
}

/// Synchronize one validated manifest into ordinary generated YAML files and a
/// versioned ownership sidecar. Database playlist materialization happens only
/// after the scanner has resolved these files to media-item IDs.
///
/// # Errors
///
/// Returns an error when enumeration, generated-file management, or sidecar publication fails.
#[allow(clippy::too_many_lines)]
pub async fn sync_playlist_files(
    manifest_path: &Path,
    manifest_bytes: &[u8],
    manifest: &ValidatedPlaylistManifest,
    extractor: &ResolvedPlaylistExtractor,
) -> Result<PlaylistFileSync> {
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let folder = parent.join(&manifest.folder);
    let sidecar_path = folder.join(".ersatzrs-playlist.json");
    let existing = load_sidecar(&sidecar_path)?;
    let manifest_etag = manifest_etag(manifest_bytes, extractor);
    let now = Utc::now();
    if let Some(state) = existing.as_ref().filter(|state| {
        state.version == 1
            && state.last_result.kind == "ok"
            && state.manifest_etag == manifest_etag
            && now
                .signed_duration_since(state.last_sync)
                .to_std()
                .is_ok_and(|elapsed| elapsed < manifest.interval)
    }) {
        return Ok(PlaylistFileSync {
            definitions: definitions_from_state(&folder, state),
            state: state.clone(),
            changed: false,
            membership_changed: false,
            enumerated: false,
        });
    }

    let enumeration = match enumerate_playlist(extractor, &manifest.manifest.url).await {
        Ok(enumeration) => enumeration,
        Err(error) => {
            persist_failed_sync(&folder, existing.as_ref(), manifest_etag, now)?;
            return Err(error);
        }
    };
    let mut unbounded = manifest.clone();
    unbounded.manifest.sync.max_items = 0;
    let eligible = select_entries(&unbounded, enumeration.entries);
    let truncated =
        manifest.manifest.sync.max_items > 0 && eligible.len() > manifest.manifest.sync.max_items;
    let selected = select_entries(manifest, eligible);
    let overrides = existing
        .as_ref()
        .map(|state| state.overrides.clone())
        .unwrap_or_default();
    let override_ids = overrides
        .excluded
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let selected = selected
        .into_iter()
        .filter(|entry| !override_ids.contains(&entry.id.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    let playlist_etag = playlist_etag(&selected)?;

    if let Some(previous) = existing.as_ref().filter(|state| {
        state.manifest_etag == manifest_etag && state.playlist_etag == playlist_etag
    }) {
        let mut state = previous.clone();
        state.last_sync = now;
        state.last_result = PlaylistSyncOutcome {
            kind: "ok".to_owned(),
            entries: selected.len(),
            truncated,
            error: None,
        };
        std::fs::create_dir_all(&folder).map_err(|error| {
            PlaylistError::Enumeration(format!("unable to create the managed folder: {error}"))
        })?;
        write_json_atomically(&sidecar_path, &state)?;
        return Ok(PlaylistFileSync {
            definitions: definitions_from_state(&folder, &state),
            state,
            changed: false,
            membership_changed: false,
            enumerated: true,
        });
    }

    std::fs::create_dir_all(&folder).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to create the managed folder: {error}"))
    })?;
    let mut managed_files = BTreeMap::new();
    let mut entries = Vec::new();
    let mut definitions = Vec::new();
    for entry in &selected {
        let file = managed_file_name(&entry.id);
        let path = folder.join(&file);
        write_generated_definition(&path, manifest, extractor, entry)?;
        managed_files.insert(entry.id.clone(), file.clone());
        entries.push(PlaylistSidecarEntry {
            id: entry.id.clone(),
            file: file.clone(),
            title: entry.title.clone(),
            duration_seconds: entry.duration_seconds,
            active: true,
        });
        definitions.push(ManagedPlaylistDefinition {
            id: entry.id.clone(),
            path,
            title: entry.title.clone(),
            duration_seconds: entry.duration_seconds,
            active: true,
        });
    }

    if let Some(previous) = &existing {
        for old in &previous.entries {
            if managed_files.contains_key(&old.id) {
                continue;
            }
            let Some(old_file) = previous.managed_files.get(&old.id) else {
                continue;
            };
            let old_path = folder.join(old_file);
            if !old_path.is_file() {
                continue;
            }
            match manifest.manifest.sync.on_removed {
                RemovedEntryPolicy::Trash => move_to_managed_trash(&folder, &old_path, now)?,
                RemovedEntryPolicy::Keep => {
                    managed_files.insert(old.id.clone(), old_file.clone());
                    entries.push(PlaylistSidecarEntry {
                        active: false,
                        ..old.clone()
                    });
                    definitions.push(ManagedPlaylistDefinition {
                        id: old.id.clone(),
                        path: old_path,
                        title: old.title.clone(),
                        duration_seconds: old.duration_seconds,
                        active: false,
                    });
                }
            }
        }
    }

    let membership_changed = existing
        .as_ref()
        .is_none_or(|state| state.playlist_etag != playlist_etag);
    let changed = membership_changed
        || existing
            .as_ref()
            .is_none_or(|state| state.manifest_etag != manifest_etag);
    let state = PlaylistSidecarState {
        version: 1,
        manifest_etag,
        playlist_etag,
        last_sync: now,
        last_result: PlaylistSyncOutcome {
            kind: "ok".to_owned(),
            entries: selected.len(),
            truncated,
            error: None,
        },
        managed_files,
        entries,
        overrides,
    };
    write_json_atomically(&sidecar_path, &state)?;
    Ok(PlaylistFileSync {
        state,
        definitions,
        changed,
        membership_changed,
        enumerated: true,
    })
}

fn persist_failed_sync(
    folder: &Path,
    existing: Option<&PlaylistSidecarState>,
    manifest_etag: String,
    now: DateTime<Utc>,
) -> Result<()> {
    std::fs::create_dir_all(folder).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to create the managed folder: {error}"))
    })?;
    let mut state = existing.cloned().unwrap_or_else(|| PlaylistSidecarState {
        version: 1,
        manifest_etag: manifest_etag.clone(),
        playlist_etag: String::new(),
        last_sync: now,
        last_result: PlaylistSyncOutcome {
            kind: "failed".to_owned(),
            entries: 0,
            truncated: false,
            error: Some("playlist enumeration failed".to_owned()),
        },
        managed_files: BTreeMap::new(),
        entries: Vec::new(),
        overrides: PlaylistOverrides::default(),
    });
    state.manifest_etag = manifest_etag;
    state.last_sync = now;
    state.last_result = PlaylistSyncOutcome {
        kind: "failed".to_owned(),
        entries: state.entries.iter().filter(|entry| entry.active).count(),
        truncated: false,
        error: Some("playlist enumeration failed".to_owned()),
    };
    write_json_atomically(&folder.join(".ersatzrs-playlist.json"), &state)
}

/// Apply manifest exclusions, filters, limit, and requested order.
#[must_use]
pub fn select_entries(
    manifest: &ValidatedPlaylistManifest,
    entries: Vec<PlaylistEntry>,
) -> Vec<PlaylistEntry> {
    let excluded = manifest
        .manifest
        .exclude
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut selected = entries
        .into_iter()
        .filter(|entry| !excluded.contains(&entry.id.to_ascii_lowercase()))
        .filter(|entry| {
            manifest
                .include_title
                .as_ref()
                .is_none_or(|pattern| pattern.is_match(&entry.title))
        })
        .filter(|entry| {
            manifest
                .exclude_title
                .as_ref()
                .is_none_or(|pattern| !pattern.is_match(&entry.title))
        })
        .filter(|entry| {
            entry.duration_seconds.is_none_or(|duration| {
                manifest
                    .min_duration_seconds
                    .is_none_or(|minimum| duration >= minimum)
                    && manifest
                        .max_duration_seconds
                        .is_none_or(|maximum| duration <= maximum)
            })
        })
        .collect::<Vec<_>>();
    match manifest.manifest.sync.order {
        PlaylistOrder::Upstream => {}
        PlaylistOrder::NewestFirst => selected.sort_by(|left, right| {
            right
                .year
                .cmp(&left.year)
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
                .then_with(|| left.id.cmp(&right.id))
        }),
        PlaylistOrder::Title => selected.sort_by(|left, right| {
            left.title
                .to_lowercase()
                .cmp(&right.title.to_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        }),
    }
    if manifest.manifest.sync.max_items > 0 {
        selected.truncate(manifest.manifest.sync.max_items);
    }
    selected
}

#[allow(clippy::assigning_clones)]
fn parse_ndjson_entries(value: &str) -> Result<Vec<PlaylistEntry>> {
    let mut entries = Vec::new();
    let mut ids = BTreeMap::new();
    for (index, line) in value.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut entry: PlaylistEntry = serde_json::from_str(line).map_err(|error| {
            PlaylistError::Enumeration(format!("line {} is invalid JSON: {error}", index + 1))
        })?;
        entry.id = entry.id.trim().to_owned();
        entry.url = entry.url.trim().to_owned();
        entry.title = entry.title.trim().to_owned();
        if entry.id.is_empty() || entry.url.is_empty() || entry.title.is_empty() {
            return Err(PlaylistError::Enumeration(format!(
                "line {} requires non-empty id, url, and title",
                index + 1
            )));
        }
        if entry
            .year
            .is_some_and(|year| !(1..=chrono::Utc::now().year() + 5).contains(&year))
        {
            return Err(PlaylistError::Enumeration(format!(
                "line {} has an invalid year",
                index + 1
            )));
        }
        let normalized_id = entry.id.to_ascii_lowercase();
        if let Some(previous) = ids.insert(normalized_id, index + 1) {
            return Err(PlaylistError::Enumeration(format!(
                "line {} duplicates the id from line {previous}",
                index + 1
            )));
        }
        entries.push(entry);
        if entries.len() > MAX_ENUMERATION_ENTRIES {
            return Err(PlaylistError::Enumeration(format!(
                "extractor returned more than {MAX_ENUMERATION_ENTRIES} entries"
            )));
        }
    }
    Ok(entries)
}

fn load_sidecar(path: &Path) -> Result<Option<PlaylistSidecarState>> {
    let value = match std::fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(PlaylistError::Enumeration(format!(
                "unable to read managed playlist state: {error}"
            )))
        }
    };
    let state: PlaylistSidecarState = serde_json::from_str(&value).map_err(|error| {
        PlaylistError::Enumeration(format!("managed playlist state is invalid: {error}"))
    })?;
    if state.version != 1 {
        return Err(PlaylistError::Enumeration(format!(
            "managed playlist state version {} is unsupported",
            state.version
        )));
    }
    Ok(Some(state))
}

fn definitions_from_state(
    folder: &Path,
    state: &PlaylistSidecarState,
) -> Vec<ManagedPlaylistDefinition> {
    state
        .entries
        .iter()
        .map(|entry| ManagedPlaylistDefinition {
            id: entry.id.clone(),
            path: folder.join(&entry.file),
            title: entry.title.clone(),
            duration_seconds: entry.duration_seconds,
            active: entry.active,
        })
        .collect()
}

fn write_generated_definition(
    path: &Path,
    manifest: &ValidatedPlaylistManifest,
    extractor: &ResolvedPlaylistExtractor,
    entry: &PlaylistEntry,
) -> Result<()> {
    let definition = GeneratedRemoteStreamDefinition {
        script: playback_command(extractor, &entry.url),
        is_live: entry.is_live || manifest.manifest.defaults.is_live,
        duration: entry.duration_seconds.map(format_clock_duration),
        title: &entry.title,
        plot: entry.plot.as_deref(),
        year: entry.year.or(manifest.manifest.defaults.year),
        release_date: entry.release_date.as_deref(),
        content_rating: entry.content_rating.as_deref().or(manifest
            .manifest
            .defaults
            .content_rating
            .as_deref()),
        genres: entry.genres.clone(),
        tags: union_values(&manifest.manifest.tags, &entry.tags),
        collection: entry.collection.as_deref(),
        people: entry.people.clone(),
        producers: entry.producers.clone(),
        original_broadcasters: entry.original_broadcasters.clone(),
        broadcasters: entry.broadcasters.clone(),
    };
    let yaml = serde_yaml::to_string(&definition).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to encode a managed definition: {error}"))
    })?;
    let content = format!(
        "# Generated by ErsatzRS from a Remote Stream playlist manifest. Do not edit.\n{yaml}"
    );
    write_bytes_atomically(path, content.as_bytes())
}

fn union_values(left: &[String], right: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    left.iter()
        .chain(right)
        .filter_map(|value| {
            let value = value.trim();
            (!value.is_empty() && seen.insert(value.to_ascii_lowercase())).then(|| value.to_owned())
        })
        .collect()
}

fn playback_command(extractor: &ResolvedPlaylistExtractor, item_url: &str) -> String {
    extractor
        .args
        .iter()
        .map(String::as_str)
        .chain(["play", item_url, "{seek}"])
        .fold(
            quote_command_argument(&extractor.program),
            |mut output, argument| {
                output.push(' ');
                output.push_str(&quote_command_argument(argument));
                output
            },
        )
}

fn quote_command_argument(value: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn managed_file_name(id: &str) -> String {
    if id.len() <= 120
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        format!("{id}.yml")
    } else {
        format!("{}.yml", hex_hash(id.as_bytes()))
    }
}

fn move_to_managed_trash(folder: &Path, path: &Path, now: DateTime<Utc>) -> Result<()> {
    let trash = folder.join(".ersatzrs-trash");
    std::fs::create_dir_all(&trash).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to create managed trash: {error}"))
    })?;
    let file = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("entry.yml");
    let destination = trash.join(format!("{}-{file}", now.format("%Y%m%dT%H%M%S%fZ")));
    std::fs::rename(path, destination).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to trash a managed definition: {error}"))
    })
}

fn write_json_atomically(path: &Path, value: &PlaylistSidecarState) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to encode managed playlist state: {error}"))
    })?;
    write_bytes_atomically(path, &bytes)
}

fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if std::fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to create managed temporary state: {error}"))
    })?;
    temporary.write_all(bytes).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to write managed temporary state: {error}"))
    })?;
    temporary.flush().map_err(|error| {
        PlaylistError::Enumeration(format!("unable to flush managed temporary state: {error}"))
    })?;
    temporary.persist(path).map_err(|error| {
        PlaylistError::Enumeration(format!(
            "unable to publish managed state atomically: {error}"
        ))
    })?;
    Ok(())
}

fn manifest_etag(bytes: &[u8], extractor: &ResolvedPlaylistExtractor) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.update([0]);
    hasher.update(extractor.name.as_bytes());
    hasher.update([0]);
    hasher.update(extractor.program.as_bytes());
    for argument in &extractor.args {
        hasher.update([0]);
        hasher.update(argument.as_bytes());
    }
    // A wrapper update can change enumeration metadata without changing the
    // configured command path. Include a cheap executable identity stamp so a
    // fresh sidecar does not suppress the first synchronization after an
    // operator deploys a corrected extractor.
    hasher.update([0]);
    if let Ok(metadata) = std::fs::metadata(&extractor.program) {
        hasher.update(metadata.len().to_le_bytes());
        if let Ok(modified) = metadata.modified() {
            if let Ok(since_epoch) = modified.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(since_epoch.as_nanos().to_le_bytes());
            }
        }
    }
    bytes_to_hex(&hasher.finalize())
}

fn playlist_etag(entries: &[PlaylistEntry]) -> Result<String> {
    let bytes = serde_json::to_vec(entries).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to hash playlist entries: {error}"))
    })?;
    Ok(hex_hash(&bytes))
}

fn hex_hash(value: &[u8]) -> String {
    bytes_to_hex(&Sha256::digest(value))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn format_clock_duration(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

async fn read_bounded_output<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::new();
    let mut overflowed = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..read.min(remaining)]);
        overflowed |= read > remaining;
    }
    Ok((output, overflowed))
}

async fn read_tail<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<Vec<u8>> {
    let mut output = VecDeque::with_capacity(limit);
    let mut buffer = [0_u8; 4096];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        output.extend(&buffer[..read]);
        while output.len() > limit {
            output.pop_front();
        }
    }
    Ok(output.into_iter().collect())
}

fn manifest_stem(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PlaylistError::Manifest("file name is not valid UTF-8".to_owned()))?;
    let lower = name.to_ascii_lowercase();
    let suffix = MANIFEST_SUFFIXES
        .iter()
        .find(|suffix| lower.ends_with(**suffix))
        .ok_or_else(|| {
            PlaylistError::Manifest(
                "file name must end in .playlist.yml or .playlist.yaml".to_owned(),
            )
        })?;
    let stem = &name[..name.len() - suffix.len()];
    non_empty(Some(stem))
        .map(ToOwned::to_owned)
        .ok_or_else(|| PlaylistError::Manifest("manifest name must not be empty".to_owned()))
}

fn validate_folder(folder: &str) -> Result<()> {
    let path = Path::new(folder);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(PlaylistError::Manifest(
            "folder must be one relative path component".to_owned(),
        ));
    }
    Ok(())
}

fn compile_optional_regex(field: &str, value: Option<&str>) -> Result<Option<Regex>> {
    non_empty(value)
        .map(|pattern| {
            Regex::new(pattern)
                .map_err(|error| PlaylistError::Manifest(format!("{field} is invalid: {error}")))
        })
        .transpose()
}

fn parse_interval(value: &str) -> Result<Duration> {
    let seconds = parse_compact_duration(value).ok_or_else(|| {
        PlaylistError::Manifest(
            "sync.interval must be a positive duration such as 30m, 6h, or 1d".to_owned(),
        )
    })?;
    if seconds < 60 {
        return Err(PlaylistError::Manifest(
            "sync.interval must be at least 1m".to_owned(),
        ));
    }
    Ok(Duration::from_secs(seconds))
}

fn parse_optional_duration(field: &str, value: Option<&str>) -> Result<Option<u64>> {
    non_empty(value)
        .map(|value| {
            parse_clock_duration(value).ok_or_else(|| {
                PlaylistError::Manifest(format!(
                    "{field} must use HH:MM:SS with an optional fractional second"
                ))
            })
        })
        .transpose()
}

fn parse_compact_duration(value: &str) -> Option<u64> {
    let value = value.trim();
    let split = value.find(|character: char| !character.is_ascii_digit())?;
    let amount = value[..split].parse::<u64>().ok()?;
    if amount == 0 {
        return None;
    }
    let multiplier = match &value[split..].to_ascii_lowercase()[..] {
        "m" => 60,
        "h" => 60 * 60,
        "d" => 24 * 60 * 60,
        _ => return None,
    };
    amount.checked_mul(multiplier)
}

fn parse_clock_duration(value: &str) -> Option<u64> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let seconds = parts.next()?.split('.').next()?.parse::<u64>().ok()?;
    if parts.next().is_some() || minutes >= 60 || seconds >= 60 {
        return None;
    }
    hours
        .checked_mul(3600)?
        .checked_add(minutes * 60)?
        .checked_add(seconds)
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_yaml() -> &'static str {
        r#"
url: "https://example.test/playlist"
extractor: fixture
sync:
  interval: 6h
  max_items: 2
  order: upstream
  on_removed: trash
filter:
  min_duration: "00:00:30"
exclude: [skip]
create_playlist: true
"#
    }

    #[test]
    fn manifest_defaults_name_folder_and_selection() {
        let manifest = parse_playlist_manifest(
            valid_yaml(),
            Path::new("/library/Official Videos.playlist.yml"),
        )
        .expect("parse manifest");
        assert_eq!(manifest.name, "Official Videos");
        assert_eq!(manifest.folder, "Official Videos");
        assert_eq!(manifest.interval, Duration::from_hours(6));
        let selected = select_entries(
            &manifest,
            vec![
                PlaylistEntry {
                    id: "skip".to_owned(),
                    url: "https://example.test/skip".to_owned(),
                    title: "Skipped".to_owned(),
                    duration_seconds: Some(60),
                    plot: None,
                    year: None,
                    thumbnail_url: None,
                    is_live: false,
                    ..PlaylistEntry::default()
                },
                PlaylistEntry {
                    id: "short".to_owned(),
                    url: "https://example.test/short".to_owned(),
                    title: "Short".to_owned(),
                    duration_seconds: Some(20),
                    plot: None,
                    year: None,
                    thumbnail_url: None,
                    is_live: false,
                    ..PlaylistEntry::default()
                },
                PlaylistEntry {
                    id: "one".to_owned(),
                    url: "https://example.test/one".to_owned(),
                    title: "One".to_owned(),
                    duration_seconds: Some(60),
                    plot: None,
                    year: Some(2020),
                    thumbnail_url: None,
                    is_live: false,
                    ..PlaylistEntry::default()
                },
            ],
        );
        assert_eq!(
            selected
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["one"]
        );
    }

    #[test]
    fn manifest_rejects_traversal_and_unknown_fields() {
        let traversal = valid_yaml().replace(
            "extractor: fixture",
            "extractor: fixture\nfolder: ../escape",
        );
        assert!(
            parse_playlist_manifest(&traversal, Path::new("x.playlist.yml"))
                .expect_err("reject traversal")
                .to_string()
                .contains("one relative path component")
        );
        let unknown = format!("{}\ncommand: bad", valid_yaml());
        assert!(
            parse_playlist_manifest(&unknown, Path::new("x.playlist.yml"))
                .expect_err("reject unknown")
                .to_string()
                .contains("unknown field")
        );
    }

    #[test]
    fn extractor_settings_require_both_platforms_and_unique_names() {
        let valid =
            r#"[{"name":"fixture","windows":"C:\\\\tools\\\\fixture.bat","unix":"/opt/fixture"}]"#;
        assert_eq!(parse_extractor_settings(valid).expect("settings").len(), 1);
        let duplicate = r#"[
            {"name":"fixture","windows":"C:\\\\one.bat","unix":"/one"},
            {"name":"FIXTURE","windows":"C:\\\\two.bat","unix":"/two"}
        ]"#;
        assert!(parse_extractor_settings(duplicate)
            .expect_err("duplicate")
            .to_string()
            .contains("duplicate"));
    }

    #[test]
    fn ndjson_is_atomic_and_rejects_duplicate_ids() {
        let duplicate = concat!(
            r#"{"id":"one","url":"https://example.test/1","title":"One"}"#,
            "\n",
            r#"{"id":"ONE","url":"https://example.test/2","title":"Two"}"#
        );
        assert!(parse_ndjson_entries(duplicate)
            .expect_err("duplicate")
            .to_string()
            .contains("duplicates"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn file_sync_preserves_identity_order_and_skips_fresh_enumeration() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let script = temp.path().join("fixture.sh");
        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' \\\n",
                "  '{\"id\":\"one\",\"url\":\"https://example.test/1\",\"title\":\"One\",\"duration_seconds\":60,\"plot\":\"Plot\",\"year\":1992,\"release_date\":\"1992-05-30T21:15:00Z\",\"content_rating\":\"nl:12\",\"genres\":[\"showbizzprogramma\"],\"tags\":[\"televisie\"],\"collection\":\"Publieke omroepen\",\"people\":[{\"name\":\"Gert-Jan Dröge\",\"role\":\"presenter\"}],\"producers\":[\"John de Mol Produkties\"],\"original_broadcasters\":[\"AVRO\"],\"broadcasters\":[\"AVROTROS\"]}' \\\n",
                "  '{\"id\":\"two\",\"url\":\"https://example.test/2\",\"title\":\"Two\",\"duration_seconds\":120}'\n"
            ),
        )
        .expect("fixture script");
        let mut permissions = std::fs::metadata(&script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("fixture permissions");
        let manifest_path = temp.path().join("Fixture.playlist.yml");
        let manifest_bytes = valid_yaml().as_bytes();
        let manifest = parse_playlist_manifest(valid_yaml(), &manifest_path).expect("manifest");
        let extractor = ResolvedPlaylistExtractor {
            name: "fixture".to_owned(),
            program: script.display().to_string(),
            args: Vec::new(),
            working_directory: temp.path().to_path_buf(),
        };

        let first = sync_playlist_files(&manifest_path, manifest_bytes, &manifest, &extractor)
            .await
            .expect("first sync");
        assert!(first.enumerated);
        assert_eq!(
            first
                .definitions
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert!(first.definitions[0].path.ends_with("one.yml"));
        let generated =
            std::fs::read_to_string(&first.definitions[0].path).expect("generated definition");
        assert!(generated
            .starts_with("# Generated by ErsatzRS from a Remote Stream playlist manifest."));
        assert!(generated.contains("duration: 00:01:00"));
        let parsed = ersatzrs_infra::remote_stream::parse_remote_stream_definition(&generated)
            .expect("generated YAML");
        assert_eq!(parsed.release_date.as_deref(), Some("1992-05-30T21:15:00Z"));
        assert_eq!(parsed.content_rating.as_deref(), Some("nl:12"));
        assert_eq!(parsed.genres, ["showbizzprogramma"]);
        assert_eq!(parsed.tags, ["televisie"]);
        assert_eq!(parsed.collection.as_deref(), Some("Publieke omroepen"));
        assert_eq!(parsed.people[0].role, "presenter");
        assert_eq!(parsed.producers, ["John de Mol Produkties"]);
        assert_eq!(parsed.original_broadcasters, ["AVRO"]);
        assert_eq!(parsed.broadcasters, ["AVROTROS"]);
        let resolved = ersatzrs_infra::remote_stream::resolve_remote_stream(
            &parsed,
            &first.definitions[0].path,
        )
        .expect("generated command");
        let producer = ersatzrs_infra::remote_stream::remote_stream_producer(&resolved)
            .expect("generated producer");
        assert_eq!(
            &producer.args[producer.args.len() - 3..],
            ["play", "https://example.test/1", "{seek}"]
        );

        let skipped = sync_playlist_files(&manifest_path, manifest_bytes, &manifest, &extractor)
            .await
            .expect("interval skip");
        assert!(!skipped.enumerated);
        assert_eq!(skipped.definitions.len(), 2);

        std::fs::write(&script, "#!/bin/sh\necho extractor-updated >&2\nexit 1\n")
            .expect("replace fixture");
        let error = sync_playlist_files(&manifest_path, manifest_bytes, &manifest, &extractor)
            .await
            .expect_err("updated extractor must bypass the fresh interval");
        assert!(error.to_string().contains("enumeration failed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_sync_is_persisted_and_sidecar_exclusions_change_membership() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let script = temp.path().join("fixture.sh");
        let manifest_path = temp.path().join("Fixture.playlist.yml");
        let manifest = parse_playlist_manifest(valid_yaml(), &manifest_path).expect("manifest");
        let extractor = ResolvedPlaylistExtractor {
            name: "fixture".to_owned(),
            program: script.display().to_string(),
            args: Vec::new(),
            working_directory: temp.path().to_path_buf(),
        };
        std::fs::write(&script, "#!/bin/sh\necho failed >&2\nexit 1\n").expect("failed fixture");
        let mut permissions = std::fs::metadata(&script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("fixture permissions");

        sync_playlist_files(
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &extractor,
        )
        .await
        .expect_err("enumeration should fail");
        let sidecar_path = temp.path().join("Fixture").join(".ersatzrs-playlist.json");
        let failed: PlaylistSidecarState =
            serde_json::from_str(&std::fs::read_to_string(&sidecar_path).expect("failed sidecar"))
                .expect("decode failed sidecar");
        assert_eq!(failed.last_result.kind, "failed");
        assert_eq!(
            failed.last_result.error.as_deref(),
            Some("playlist enumeration failed")
        );

        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' \\\n",
                "  '{\"id\":\"one\",\"url\":\"https://example.test/1\",\"title\":\"One\",\"duration_seconds\":60}' \\\n",
                "  '{\"id\":\"two\",\"url\":\"https://example.test/2\",\"title\":\"Two\",\"duration_seconds\":120}'\n"
            ),
        )
        .expect("successful fixture");
        let first = sync_playlist_files(
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &extractor,
        )
        .await
        .expect("successful sync");
        assert!(first.membership_changed);
        let mut overridden = first.state;
        overridden.last_sync = Utc::now() - chrono::Duration::hours(7);
        overridden.overrides.excluded = vec!["two".to_owned()];
        write_json_atomically(&sidecar_path, &overridden).expect("override sidecar");
        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' \\\n",
                "  '{\"id\":\"one\",\"url\":\"https://example.test/changed\",\"title\":\"One\",\"duration_seconds\":60}' \\\n",
                "  '{\"id\":\"two\",\"url\":\"https://example.test/2\",\"title\":\"Two\",\"duration_seconds\":120}'\n"
            ),
        )
        .expect("changed URL fixture");
        let second = sync_playlist_files(
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &extractor,
        )
        .await
        .expect("override sync");
        assert!(second.membership_changed);
        assert_eq!(second.state.entries.len(), 1);
        assert_eq!(second.state.entries[0].id, "one");
        assert!(std::fs::read_to_string(&second.definitions[0].path)
            .expect("updated definition")
            .contains("https://example.test/changed"));
    }
}
