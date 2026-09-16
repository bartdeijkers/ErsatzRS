//! Remote Stream playlist manifests backed by installed add-ons.
//!
//! ADR 0066 replaces operator-authored extractor commands with signed,
//! repository-pinned packages. Both list and play operations use the same
//! out-of-process host runner and manifest capability contract.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
pub use ersatzrs_addon_contract::PlaylistEntry;
use ersatzrs_addon_contract::{
    conform_remote_stream_item_output_for_capability, conform_remote_stream_list_output,
    validate_addon_id, AddonMediaListContentKind as WireContentKind,
    AddonMediaListItemAvailability as WireAvailability, RemoteStreamLiveness as WireLiveness,
    CAPABILITY_REMOTE_STREAM_ITEM_V1, CAPABILITY_REMOTE_STREAM_ITEM_V2,
    CAPABILITY_REMOTE_STREAM_LIST_V1, CAPABILITY_REMOTE_STREAM_LIST_V2,
    CAPABILITY_REMOTE_STREAM_LIST_V3, CAPABILITY_REMOTE_STREAM_LIST_V4, MAX_LIST_ITEMS,
    MAX_REMOTE_STREAM_LIST_OUTPUT_BYTES,
};
use ersatzrs_addon_contract::{
    validate_operator_source_url, AddonOperationError, OPERATION_CODE_UNSUPPORTED_URL,
};
use ersatzrs_core::media_chapters::parse_chapter_input;
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, RemoteStreamLiveness,
};
use ersatzrs_infra::addon_chapters::{fragment_guid, fragment_stream_url};
use ersatzrs_infra::addons::{
    invoke_enabled_addon, load_installed_addon, AddonInvocation, AddonRuntimeContext,
    InstalledAddon,
};
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::remote_stream::RemoteStreamPerson;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const ENUMERATION_TIMEOUT: Duration = Duration::from_mins(2);
const PLAYLIST_SIDECAR_VERSION: u32 = 4;
#[cfg(test)]
const MAX_ARTWORK_BYTES: usize = ersatzrs_infra::addon_artwork::MAX_ARTWORK_BYTES;

/// Suffixes that distinguish playlist manifests from ordinary definitions.
pub const MANIFEST_SUFFIXES: &[&str] = &[".playlist.yml", ".playlist.yaml"];

#[derive(Debug, Error)]
pub enum PlaylistError {
    #[error("playlist manifest is invalid: {0}")]
    Manifest(String),
    #[error("remote stream playlist add-on is unavailable: {0}")]
    Addon(String),
    #[error("remote stream playlist enumeration failed: {0}")]
    Enumeration(String),
}

pub type Result<T> = std::result::Result<T, PlaylistError>;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteStreamPlaylistManifest {
    pub name: Option<String>,
    pub url: String,
    pub addon: String,
    pub folder: Option<String>,
    /// Optional validated chapter markers forwarded to a capable list add-on.
    pub chapters: Option<String>,
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
    /// Legacy manifest-level override. Explicit `false` means finite; omission
    /// leaves the provider value in control.
    pub is_live: Option<bool>,
    /// Provider-neutral manifest-level operator override.
    pub liveness: Option<RemoteStreamLiveness>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistEnumeration {
    pub entries: Vec<PlaylistEntry>,
    pub diagnostic_tail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PlaylistSidecarEntry {
    pub id: String,
    /// Stable provider identity used to reconcile membership across generated
    /// file-name or scanner-identity changes. Older sidecars fall back to
    /// `id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub file: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub active: bool,
    /// Validated effective liveness of the generated definition. Health uses
    /// this with `definition_sha256` instead of reparsing every YAML file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness: Option<RemoteStreamLiveness>,
    #[serde(default)]
    pub nfo_file: Option<String>,
    #[serde(default)]
    pub availability: AddonMediaListItemAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nfo_sha256: Option<String>,
    /// Hash of the effective generated Remote Stream definition and reconciled
    /// NFO output. This keeps per-entry change detection compact while
    /// including manifest defaults, tags, and disk-authoritative metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_etag: Option<String>,
    /// Host-owned artwork state. Its hash distinguishes a generated file from
    /// an operator replacement during later refreshes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ManagedPlaylistArtwork>,
    /// Role-aware files selected by the shared metadata editor. The legacy
    /// `artwork` field remains readable for v3 sidecars and represents the
    /// thumbnail role.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub artwork_roles: BTreeMap<String, ManagedPlaylistArtwork>,
    /// Digest-covered, publication-selected subtitle sidecars on the programme clock.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<ManagedPlaylistSubtitle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ManagedPlaylistSubtitle {
    pub artifact_id: i64,
    pub language: String,
    pub actual_language: String,
    pub file: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ManagedPlaylistArtwork {
    pub url: String,
    pub file: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub operator_owned: bool,
}

// Serde's `skip_serializing_if` callback contract passes a shared reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
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
    #[serde(default)]
    pub managed_nfo_files: BTreeMap<String, String>,
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

/// Bounded diagnostic classification for one playlist membership delta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaylistMembershipChangeReason {
    #[default]
    Unchanged,
    Added,
    Removed,
    ChangedOrReplaced,
    Reordered,
    Mixed,
}

impl fmt::Display for PlaylistMembershipChangeReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unchanged => "playlist membership unchanged",
            Self::Added => "playlist membership added",
            Self::Removed => "playlist membership removed",
            Self::ChangedOrReplaced => "playlist membership changed or replaced",
            Self::Reordered => "playlist membership reordered",
            Self::Mixed => "playlist membership changed in multiple ways",
        })
    }
}

/// Deterministic membership changes keyed by stable provider entry identity.
///
/// Identifier vectors are sorted by their case-insensitive provider identity.
/// `Display` and `Debug` intentionally expose only bounded categories and
/// counts so diagnostics cannot leak raw provider identifiers.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct PlaylistMembershipDelta {
    pub added_provider_ids: Vec<String>,
    pub removed_provider_ids: Vec<String>,
    pub changed_provider_ids: Vec<String>,
    pub reordered: bool,
}

impl PlaylistMembershipDelta {
    /// Compare active entries in previous and current sidecar state.
    #[must_use]
    pub fn between(
        previous: Option<&PlaylistSidecarState>,
        current: &PlaylistSidecarState,
    ) -> Self {
        let current_by_provider = active_entries_by_provider(current);
        let Some(previous) = previous else {
            return Self {
                added_provider_ids: current_by_provider
                    .into_values()
                    .map(|entry| entry.provider_entry_id().to_owned())
                    .collect(),
                ..Self::default()
            };
        };
        let previous_by_provider = active_entries_by_provider(previous);
        let added_provider_ids = current_by_provider
            .iter()
            .filter(|(provider_id, _)| !previous_by_provider.contains_key(*provider_id))
            .map(|(_, entry)| entry.provider_entry_id().to_owned())
            .collect();
        let removed_provider_ids = previous_by_provider
            .iter()
            .filter(|(provider_id, _)| !current_by_provider.contains_key(*provider_id))
            .map(|(_, entry)| entry.provider_entry_id().to_owned())
            .collect();
        let changed_provider_ids = current_by_provider
            .iter()
            .filter_map(|(provider_id, current_entry)| {
                previous_by_provider
                    .get(provider_id)
                    .filter(|previous_entry| {
                        sidecar_membership_entry_changed(previous_entry, current_entry)
                    })
                    .map(|_| current_entry.provider_entry_id().to_owned())
            })
            .collect();
        let previous_common_order = active_provider_order(previous)
            .into_iter()
            .filter(|provider_id| current_by_provider.contains_key(provider_id))
            .collect::<Vec<_>>();
        let current_common_order = active_provider_order(current)
            .into_iter()
            .filter(|provider_id| previous_by_provider.contains_key(provider_id))
            .collect::<Vec<_>>();

        Self {
            added_provider_ids,
            removed_provider_ids,
            changed_provider_ids,
            reordered: previous_common_order != current_common_order,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added_provider_ids.is_empty()
            && self.removed_provider_ids.is_empty()
            && self.changed_provider_ids.is_empty()
            && !self.reordered
    }

    #[must_use]
    pub fn reason(&self) -> PlaylistMembershipChangeReason {
        match (
            !self.added_provider_ids.is_empty(),
            !self.removed_provider_ids.is_empty(),
            !self.changed_provider_ids.is_empty(),
            self.reordered,
        ) {
            (false, false, false, false) => PlaylistMembershipChangeReason::Unchanged,
            (true, false, false, false) => PlaylistMembershipChangeReason::Added,
            (false, true, false, false) => PlaylistMembershipChangeReason::Removed,
            (false, false, true, false) => PlaylistMembershipChangeReason::ChangedOrReplaced,
            (false, false, false, true) => PlaylistMembershipChangeReason::Reordered,
            _ => PlaylistMembershipChangeReason::Mixed,
        }
    }
}

impl fmt::Debug for PlaylistMembershipDelta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaylistMembershipDelta")
            .field("reason", &self.reason())
            .field("added_count", &self.added_provider_ids.len())
            .field("removed_count", &self.removed_provider_ids.len())
            .field("changed_count", &self.changed_provider_ids.len())
            .field("reordered", &self.reordered)
            .finish()
    }
}

impl fmt::Display for PlaylistMembershipDelta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} (added={}, removed={}, changed={}, reordered={})",
            self.reason(),
            self.added_provider_ids.len(),
            self.removed_provider_ids.len(),
            self.changed_provider_ids.len(),
            self.reordered
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistFileSync {
    pub state: PlaylistSidecarState,
    pub definitions: Vec<ManagedPlaylistDefinition>,
    /// Previously owned definitions removed during this synchronization.
    pub(crate) removed_definition_paths: Vec<PathBuf>,
    pub changed: bool,
    pub membership_changed: bool,
    pub membership_delta: PlaylistMembershipDelta,
    pub enumerated: bool,
    sidecar_publication: SidecarPublication,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SidecarPublication {
    NotNeeded,
    Staged(PathBuf),
}

impl PlaylistFileSync {
    /// Publish the staged successful sidecar after database membership and
    /// refresh intent have committed. Re-publishing an unchanged cached result
    /// is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when the staged sidecar cannot be atomically published.
    pub fn publish_sidecar(&self) -> Result<()> {
        if let SidecarPublication::Staged(path) = &self.sidecar_publication {
            write_json_atomically(path, &self.state)?;
        }
        Ok(())
    }
}

impl PlaylistSidecarEntry {
    fn provider_entry_id(&self) -> &str {
        self.provider_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.id)
    }
}

fn active_entries_by_provider(
    state: &PlaylistSidecarState,
) -> BTreeMap<String, &PlaylistSidecarEntry> {
    let mut entries = BTreeMap::new();
    for entry in state.entries.iter().filter(|entry| entry.active) {
        entries
            .entry(entry.provider_entry_id().to_ascii_lowercase())
            .or_insert(entry);
    }
    entries
}

fn active_provider_order(state: &PlaylistSidecarState) -> Vec<String> {
    let mut seen = BTreeSet::new();
    state
        .entries
        .iter()
        .filter(|entry| entry.active)
        .filter_map(|entry| {
            let provider_id = entry.provider_entry_id().to_ascii_lowercase();
            seen.insert(provider_id.clone()).then_some(provider_id)
        })
        .collect()
}

fn sidecar_membership_entry_changed(
    previous: &PlaylistSidecarEntry,
    current: &PlaylistSidecarEntry,
) -> bool {
    if previous.provider_entry_id() != current.provider_entry_id()
        || previous.id != current.id
        || previous.file != current.file
    {
        return true;
    }
    match (
        previous.entry_etag.as_deref(),
        current.entry_etag.as_deref(),
    ) {
        (Some(previous_etag), Some(current_etag)) => previous_etag != current_etag,
        _ => known_output_hashes_changed(previous, current).unwrap_or(true),
    }
}

fn known_output_hashes_changed(
    previous: &PlaylistSidecarEntry,
    current: &PlaylistSidecarEntry,
) -> Option<bool> {
    Some(
        previous.definition_sha256.as_deref()? != current.definition_sha256.as_deref()?
            || previous.nfo_sha256.as_deref()? != current.nfo_sha256.as_deref()?,
    )
}

fn fresh_cached_playlist_sync(
    folder: &Path,
    sidecar_path: &Path,
    state: &PlaylistSidecarState,
) -> Result<PlaylistFileSync> {
    let mut current = state.clone();
    for entry in &mut current.entries {
        let nfo_file = entry
            .nfo_file
            .as_deref()
            .or_else(|| state.managed_nfo_files.get(&entry.id).map(String::as_str))
            .ok_or_else(|| {
                PlaylistError::Enumeration(format!("managed entry '{}' has no NFO file", entry.id))
            })?;
        let nfo_path = folder.join(nfo_file);
        ersatzrs_infra::addon_media_nfo::adopt_existing(&nfo_path, entry.nfo_sha256.as_deref())
            .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        let nfo_bytes = std::fs::read(&nfo_path).map_err(|error| {
            PlaylistError::Enumeration(format!("unable to read managed NFO: {error}"))
        })?;
        let definition_sha256 = entry.definition_sha256.as_deref().ok_or_else(|| {
            PlaylistError::Enumeration(format!(
                "managed entry '{}' has no definition hash",
                entry.id
            ))
        })?;
        let nfo_sha256 = hex_hash(&nfo_bytes);
        entry.entry_etag = Some(playlist_entry_etag(definition_sha256, &nfo_sha256));
        entry.nfo_sha256 = Some(nfo_sha256);
    }

    let membership_delta = PlaylistMembershipDelta::between(Some(state), &current);
    let membership_changed = !membership_delta.is_empty();
    let sidecar_publication = if current == *state {
        SidecarPublication::NotNeeded
    } else {
        SidecarPublication::Staged(sidecar_path.to_owned())
    };
    Ok(PlaylistFileSync {
        removed_definition_paths: Vec::new(),
        definitions: definitions_from_state(folder, &current),
        state: current,
        changed: membership_changed,
        membership_changed,
        membership_delta,
        enumerated: false,
        sidecar_publication,
    })
}

#[derive(Serialize)]
struct GeneratedRemoteStreamDefinition<'a> {
    provider_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_provider_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fragment_start_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fragment_end_seconds: Option<u64>,
    url: &'a str,
    addon: &'a str,
    liveness: RemoteStreamLiveness,
    /// Compatibility mirror consumed by the existing Remote Stream parser.
    is_live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<String>,
    availability: AddonMediaListItemAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    availability_reason: Option<&'a str>,
    content_kind: AddonMediaListContentKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    artwork_urls: Vec<&'a str>,
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
    let mut manifest: RemoteStreamPlaylistManifest =
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
    let untrimmed_url = std::mem::take(&mut manifest.url);
    untrimmed_url.trim().clone_into(&mut manifest.url);
    manifest.url = validate_operator_source_url(&manifest.url, &[]).map_err(|error| {
        PlaylistError::Manifest(
            AddonOperationError::new(OPERATION_CODE_UNSUPPORTED_URL, error.to_string()).to_string(),
        )
    })?;
    if validate_addon_id(manifest.addon.trim()).is_err() {
        return Err(PlaylistError::Manifest(
            "addon must be a reverse-DNS identifier".to_owned(),
        ));
    }
    if manifest.sync.max_items > MAX_LIST_ITEMS {
        return Err(PlaylistError::Manifest(format!(
            "sync.max_items must be at most {MAX_LIST_ITEMS}"
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
    validate_playlist_default_liveness(&manifest.defaults)?;
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

async fn resolve_playlist_addon(pool: &DbPool, addon_id: &str) -> Result<InstalledAddon> {
    let addon = load_installed_addon(pool, addon_id)
        .await
        .map_err(|error| PlaylistError::Addon(error.to_string()))?
        .ok_or_else(|| PlaylistError::Addon(format!("{addon_id} is not installed")))?;
    if !addon.enabled {
        return Err(PlaylistError::Addon(format!("{addon_id} is disabled")));
    }
    if !addon
        .manifest
        .has_capability(CAPABILITY_REMOTE_STREAM_LIST_V1)
        && !addon
            .manifest
            .has_capability(CAPABILITY_REMOTE_STREAM_LIST_V2)
        && !addon
            .manifest
            .has_capability(CAPABILITY_REMOTE_STREAM_LIST_V3)
        && !addon
            .manifest
            .has_capability(CAPABILITY_REMOTE_STREAM_LIST_V4)
    {
        return Err(PlaylistError::Addon(format!(
            "{addon_id} does not implement a Remote Stream list capability"
        )));
    }
    if !addon.manifest.has_remote_stream_play_capability() {
        return Err(PlaylistError::Addon(format!(
            "{addon_id} does not implement a Remote Stream play capability"
        )));
    }
    Ok(addon)
}

/// Run one bounded playlist enumeration and decode its NDJSON atomically.
///
/// # Errors
///
/// Returns an error for process, timeout, output-bound, exit-status, or NDJSON failures.
pub async fn enumerate_playlist(
    pool: &DbPool,
    addon_id: &str,
    playlist_url: &str,
    chapter_input: Option<&str>,
    runtime: &AddonRuntimeContext,
) -> Result<PlaylistEnumeration> {
    let addon = resolve_playlist_addon(pool, addon_id).await?;
    let playlist_url =
        validate_operator_source_url(playlist_url, &addon.manifest.permissions.network_hosts)
            .map_err(|error| {
                PlaylistError::Enumeration(
                    AddonOperationError::new(OPERATION_CODE_UNSUPPORTED_URL, error.to_string())
                        .to_string(),
                )
            })?;
    let capability = if addon
        .manifest
        .has_capability(CAPABILITY_REMOTE_STREAM_LIST_V4)
    {
        CAPABILITY_REMOTE_STREAM_LIST_V4
    } else if addon
        .manifest
        .has_capability(CAPABILITY_REMOTE_STREAM_LIST_V3)
    {
        CAPABILITY_REMOTE_STREAM_LIST_V3
    } else if addon
        .manifest
        .has_capability(CAPABILITY_REMOTE_STREAM_LIST_V2)
    {
        CAPABILITY_REMOTE_STREAM_LIST_V2
    } else {
        CAPABILITY_REMOTE_STREAM_LIST_V1
    };
    require_chapter_capability(addon_id, capability, chapter_input)?;
    let environment = BTreeMap::from([
        (
            "ERSATZRS_REMOTE_STREAM_PLAYLIST_URL".to_owned(),
            playlist_url,
        ),
        (
            "ERSATZRS_MEDIA_LIST_CHAPTERS".to_owned(),
            chapter_input.unwrap_or_default().to_owned(),
        ),
    ]);
    let stdout = invoke_enabled_addon(
        pool,
        addon_id,
        AddonInvocation {
            capability,
            operation: "list",
            runtime,
            extra_environment: &environment,
            timeout: ENUMERATION_TIMEOUT,
            max_output_bytes: MAX_REMOTE_STREAM_LIST_OUTPUT_BYTES,
        },
    )
    .await
    .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    let text = String::from_utf8(stdout)
        .map_err(|_| PlaylistError::Enumeration("add-on output is not UTF-8".to_owned()))?;
    let mut entries = parse_ndjson_entries(&text)?;
    validate_list_capability_fields(capability, &entries)?;
    if chapter_input.is_some_and(|input| !input.trim().is_empty()) {
        add_playlist_chapter_fragments(&mut entries, addon_id, chapter_input.unwrap_or_default())?;
    }
    Ok(PlaylistEnumeration {
        entries,
        diagnostic_tail: String::new(),
    })
}

fn validate_list_capability_fields(capability: &str, entries: &[PlaylistEntry]) -> Result<()> {
    if !matches!(
        capability,
        CAPABILITY_REMOTE_STREAM_LIST_V3 | CAPABILITY_REMOTE_STREAM_LIST_V4
    ) && entries.iter().any(|entry| {
        entry.parent_provider_id.is_some()
            || entry.fragment_start_seconds.is_some()
            || entry.fragment_end_seconds.is_some()
            || !entry.additional_image_urls.is_empty()
    }) {
        return Err(PlaylistError::Enumeration(
            "additional images and chapter-fragment fields require remote-stream.list.v3"
                .to_owned(),
        ));
    }
    if capability != CAPABILITY_REMOTE_STREAM_LIST_V4
        && entries.iter().any(|entry| entry.liveness.is_some())
    {
        return Err(PlaylistError::Enumeration(
            "liveness requires remote-stream.list.v4".to_owned(),
        ));
    }
    Ok(())
}

fn require_chapter_capability(
    _addon_id: &str,
    capability: &str,
    chapter_input: Option<&str>,
) -> Result<()> {
    if chapter_input.is_some_and(|input| !input.trim().is_empty())
        && !matches!(
            capability,
            CAPABILITY_REMOTE_STREAM_LIST_V3 | CAPABILITY_REMOTE_STREAM_LIST_V4
        )
    {
        return Err(PlaylistError::Enumeration(
            "chapters require an installed add-on with remote-stream.list.v3 support".to_owned(),
        ));
    }
    Ok(())
}

fn add_playlist_chapter_fragments(
    entries: &mut Vec<PlaylistEntry>,
    addon_id: &str,
    input: &str,
) -> Result<()> {
    if entries.len() != 1 {
        return Err(PlaylistError::Enumeration(
            "a chapter source must resolve to exactly one full video".to_owned(),
        ));
    }
    let full = entries[0].clone();
    let parent_provider_id = full.provider_id.clone().unwrap_or_else(|| full.id.clone());
    let chapters = parse_chapter_input(input, full.duration_seconds)
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    for chapter in chapters {
        let mut fragment = full.clone();
        fragment.id = format!("{}-chapter-{}", full.id, chapter.start_seconds);
        fragment.provider_id = Some(format!(
            "{parent_provider_id}:chapter:{}",
            chapter.start_seconds
        ));
        fragment.url = fragment_stream_url(&full.url, chapter.start_seconds, chapter.end_seconds)
            .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        fragment.title = format!("{} - {}", full.title, chapter.title);
        fragment.duration_seconds = chapter
            .end_seconds
            .map(|end| end.saturating_sub(chapter.start_seconds));
        fragment.parent_provider_id = Some(parent_provider_id.clone());
        fragment.fragment_start_seconds = Some(chapter.start_seconds);
        fragment.fragment_end_seconds = chapter.end_seconds;
        fragment.guids = vec![fragment_guid(
            addon_id,
            &parent_provider_id,
            chapter.start_seconds,
        )];
        entries.push(fragment);
    }
    Ok(())
}

/// Synchronize one validated manifest into generated YAML/NFO pairs and a
/// versioned ownership sidecar. Database playlist materialization happens only
/// after the scanner has resolved these files to media-item IDs.
///
/// # Errors
///
/// Returns an error when enumeration, generated-file management, or sidecar publication fails.
pub async fn sync_playlist_files(
    pool: &DbPool,
    manifest_path: &Path,
    manifest_bytes: &[u8],
    manifest: &ValidatedPlaylistManifest,
    runtime: &AddonRuntimeContext,
) -> Result<PlaylistFileSync> {
    let sync = stage_playlist_files(pool, manifest_path, manifest_bytes, manifest, runtime).await?;
    sync.publish_sidecar()?;
    Ok(sync)
}

/// Stage generated playlist files and the next successful sidecar without
/// publishing the sidecar. The scanner uses this to commit SQL membership and
/// durable Refresh intent first, making a crash at worst replay the same
/// change instead of losing it.
///
/// # Errors
///
/// Returns an error when enumeration, generated-file staging, or sidecar-state
/// preparation fails.
#[allow(clippy::too_many_lines)]
pub async fn stage_playlist_files(
    pool: &DbPool,
    manifest_path: &Path,
    manifest_bytes: &[u8],
    manifest: &ValidatedPlaylistManifest,
    runtime: &AddonRuntimeContext,
) -> Result<PlaylistFileSync> {
    let addon = resolve_playlist_addon(pool, &manifest.manifest.addon).await?;
    let content_kind_override = load_content_kind_override(pool, manifest_path).await?;
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let folder = parent.join(&manifest.folder);
    let sidecar_path = folder.join(".ersatzrs-playlist.json");
    let existing = load_sidecar(&sidecar_path)?;
    let published_selection = published_provider_selection(pool, manifest_path).await?;
    let playback_artifacts =
        publication_artifact_selection(pool, published_selection.as_ref()).await?;
    let playback_durations = selected_playback_durations(
        pool,
        published_selection.as_ref(),
        &playback_artifacts.artifacts,
    )
    .await?;
    let mut manifest_etag = manifest_etag(manifest_bytes, &addon, content_kind_override);
    if !playback_durations.is_empty() {
        // Cache identity follows the exact selected playback durations; legacy
        // manifests retain their original etag and generated bytes.
        manifest_etag = hex_hash(format!("{manifest_etag}:{playback_durations:?}").as_bytes());
    }
    if !playback_artifacts.artifacts.subtitles.is_empty() {
        manifest_etag = hex_hash(
            format!(
                "{manifest_etag}:{:?}:{:?}:{:?}",
                playback_artifacts.artifacts.subtitles,
                playback_artifacts.artifacts.streaming_plans,
                playback_artifacts.fragment_windows
            )
            .as_bytes(),
        );
    }
    let now = Utc::now();
    if let Some(state) = existing.as_ref() {
        let cache_matches_publication = published_selection.as_ref().is_none_or(|selection| {
            !selection.is_publishing
                && (selection.provider_ids.is_empty() || state.last_result.entries > 0)
        });
        let cache_is_fresh = cache_matches_publication
            && state.version == PLAYLIST_SIDECAR_VERSION
            && state.last_result.kind == "ok"
            && state.manifest_etag == manifest_etag
            && managed_files_intact(&folder, state)
            && now
                .signed_duration_since(state.last_sync)
                .to_std()
                .is_ok_and(|elapsed| elapsed < manifest.interval);
        if cache_is_fresh {
            return fresh_cached_playlist_sync(&folder, &sidecar_path, state);
        }
    }

    // A staged publication owns a complete, already validated provider
    // snapshot in AddonMediaListItem. Replaying that snapshot keeps Publish a
    // local materialization boundary: neither media-list.list nor the legacy
    // remote-stream enumeration is invoked again just because the operator
    // changed metadata, inclusion, or the projected library type.
    let staged_selection = published_selection
        .as_ref()
        .filter(|selection| selection.is_publishing);
    let (selected, truncated, enumerated) = if let Some(selection) = staged_selection {
        (
            load_staged_playlist_entries(pool, selection.list_id, content_kind_override).await?,
            false,
            false,
        )
    } else {
        let enumeration = match enumerate_playlist(
            pool,
            &manifest.manifest.addon,
            &manifest.manifest.url,
            manifest.manifest.chapters.as_deref(),
            runtime,
        )
        .await
        {
            Ok(enumeration) => enumeration,
            Err(error) => {
                persist_failed_sync(&folder, existing.as_ref(), manifest_etag, now)?;
                return Err(error);
            }
        };
        let mut unbounded = manifest.clone();
        unbounded.manifest.sync.max_items = 0;
        let eligible = select_entries(&unbounded, enumeration.entries);
        let truncated = manifest.manifest.sync.max_items > 0
            && eligible.len() > manifest.manifest.sync.max_items;
        let mut selected = select_entries(manifest, eligible);
        if content_kind_override != AddonMediaListContentKind::Auto {
            for entry in &mut selected {
                entry.content_kind = wire_content_kind(content_kind_override);
            }
        }
        (selected, truncated, true)
    };
    let overrides = if staged_selection.is_some() {
        PlaylistOverrides::default()
    } else {
        existing
            .as_ref()
            .map(|state| state.overrides.clone())
            .unwrap_or_default()
    };
    let override_ids = overrides
        .excluded
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut selected = selected
        .into_iter()
        .filter(|entry| !override_ids.contains(&entry.id.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    if let Some(published_selection) = published_selection.as_ref() {
        selected.retain(|entry| {
            let provider_id = entry.provider_id.as_deref().unwrap_or(&entry.id);
            published_selection.provider_ids.contains(provider_id)
        });
    }
    // Enrichment runs only after item limits, filters, and exclusions have been
    // applied, so its cost is proportional to the entries actually kept rather
    // than the whole upstream listing.
    if enumerated {
        enrich_selected_entries(
            pool,
            manifest_path,
            &manifest.manifest.addon,
            runtime,
            &mut selected,
        )
        .await;
    }
    let reviewed_metadata = apply_reviewed_metadata(pool, manifest_path, &mut selected).await?;
    let previous_metadata = previous_publication_metadata(pool, manifest_path).await?;
    if enumerated {
        if let Err(error) = append_item_chapter_fragments(
            pool,
            manifest_path,
            &manifest.manifest.addon,
            &mut selected,
        )
        .await
        {
            persist_failed_sync(&folder, existing.as_ref(), manifest_etag.clone(), now)?;
            return Err(error);
        }
    }
    selected.retain(|entry| !override_ids.contains(&entry.id.to_ascii_lowercase()));
    if let Err(error) = validate_fragment_parents(&selected) {
        persist_failed_sync(&folder, existing.as_ref(), manifest_etag.clone(), now)?;
        return Err(error);
    }
    let playlist_etag = playlist_etag(&selected)?;

    std::fs::create_dir_all(&folder).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to create the managed folder: {error}"))
    })?;
    let mut managed_files = BTreeMap::new();
    let mut managed_nfo_files = BTreeMap::new();
    let mut entries = Vec::new();
    let mut definitions = Vec::new();
    let artwork_client = artwork_client()?;
    for entry in &selected {
        let file = managed_file_name(&entry.id);
        let path = folder.join(&file);
        let nfo_file = managed_nfo_file_name(&file);
        let nfo_path = folder.join(&nfo_file);
        let provider_id = entry.provider_id.as_deref().unwrap_or(&entry.id);
        let duration_seconds = playback_inclusion_duration(
            entry.duration_seconds,
            playback_durations.get(provider_id).copied(),
        );
        let mut playback_entry = entry.clone();
        if playback_artifacts
            .artifacts
            .prepared_media
            .contains_key(provider_id)
        {
            // Source status remains on the provider record. The selected local
            // artifact is playable independently of that source (ADR 0094).
            playback_entry.availability = WireAvailability::Available;
            playback_entry.availability_reason = None;
        }
        let definition_bytes = generated_definition_bytes_with_duration(
            manifest,
            &playback_entry,
            playback_durations.get(provider_id).copied(),
        )?;
        let liveness = effective_entry_liveness(&manifest.manifest.defaults, entry)?;
        let previous_entry = existing.as_ref().and_then(|state| {
            state
                .entries
                .iter()
                .find(|previous| previous.id == entry.id)
        });
        let provider_id = entry.provider_id.as_deref().unwrap_or(&entry.id);
        let metadata = reviewed_metadata.get(provider_id);
        let mut artwork_roles = BTreeMap::new();
        if let Some(metadata) = metadata {
            for (role, key) in [
                (
                    ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster,
                    "poster",
                ),
                (
                    ersatzrs_core::repositories::AddonMediaListArtworkRole::Fanart,
                    "fanart",
                ),
                (
                    ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb,
                    "thumb",
                ),
            ] {
                let source = metadata
                    .artwork
                    .iter()
                    .find(|candidate| candidate.role == role)
                    .map(|candidate| candidate.url.as_str());
                let previous = previous_entry.and_then(|previous| {
                    previous.artwork_roles.get(key).or_else(|| {
                        (key == "thumb")
                            .then_some(previous.artwork.as_ref())
                            .flatten()
                    })
                });
                if let Some(materialized) = sync_managed_artwork_role(
                    pool,
                    manifest_path,
                    provider_id,
                    role,
                    &artwork_client,
                    &folder,
                    &file,
                    source,
                    &addon.manifest.permissions.network_hosts,
                    previous,
                )
                .await?
                {
                    artwork_roles.insert(key.to_owned(), materialized);
                }
            }
        }
        let artwork = if metadata.is_some() {
            artwork_roles.get("thumb").cloned()
        } else {
            sync_managed_artwork(
                &artwork_client,
                &folder,
                &file,
                entry
                    .thumbnail_url
                    .as_deref()
                    .or_else(|| entry.additional_image_urls.first().map(String::as_str)),
                &addon.manifest.permissions.network_hosts,
                previous_entry.and_then(|previous| previous.artwork.as_ref()),
            )
            .await
        };
        let generated_nfo = metadata.map_or_else(
            || generated_nfo_bytes(manifest, entry),
            |metadata| generated_reviewed_nfo_bytes(manifest, entry, metadata, &artwork_roles),
        )?;
        let baseline = previous_metadata
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .items
                    .iter()
                    .find(|item| item.provider_id == provider_id)
                    .map(|item| (snapshot, item))
            })
            .map(|(snapshot, item)| {
                legacy_projected_metadata(manifest, entry, snapshot, item, previous_entry)
            })
            .transpose()?;
        let nfo_bytes = ersatzrs_infra::addon_media_nfo::reconcile_generated_metadata(
            &nfo_path,
            item_nfo_root(entry),
            &generated_nfo,
            baseline.as_ref(),
        )
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        adopt_external_metadata(pool, manifest_path, provider_id, &nfo_path, &nfo_bytes).await?;
        let definition_sha256 = hex_hash(&definition_bytes);
        let nfo_sha256 = hex_hash(&nfo_bytes);
        let mut entry_etag = playlist_entry_etag(&definition_sha256, &nfo_sha256);
        write_generated_pair(&path, &definition_bytes, &nfo_path, &nfo_bytes)?;
        let subtitles = materialize_selected_subtitles(
            pool,
            published_selection.as_ref(),
            &playback_artifacts,
            entry,
            &path,
            previous_entry.map_or(&[], |previous| previous.subtitles.as_slice()),
            &ersatzrs_infra::file_system_layout::addon_processed_media_folder(),
        )
        .await?;
        if !subtitles.is_empty() {
            entry_etag = hex_hash(format!("{entry_etag}:{subtitles:?}").as_bytes());
        }
        managed_files.insert(entry.id.clone(), file.clone());
        managed_nfo_files.insert(entry.id.clone(), nfo_file.clone());
        entries.push(PlaylistSidecarEntry {
            id: entry.id.clone(),
            provider_id: Some(provider_id.to_owned()),
            file: file.clone(),
            title: entry.title.clone(),
            duration_seconds,
            active: true,
            liveness: Some(liveness),
            nfo_file: Some(nfo_file),
            availability: domain_availability(entry.availability),
            definition_sha256: Some(definition_sha256),
            nfo_sha256: Some(nfo_sha256),
            entry_etag: Some(entry_etag),
            artwork,
            artwork_roles,
            subtitles,
        });
        definitions.push(ManagedPlaylistDefinition {
            id: entry.id.clone(),
            path,
            title: entry.title.clone(),
            duration_seconds,
            active: true,
        });
    }

    let mut removed_definition_paths = Vec::new();
    if let Some(previous) = &existing {
        for old in &previous.entries {
            if managed_files.contains_key(&old.id) {
                continue;
            }
            let Some(old_file) = previous.managed_files.get(&old.id) else {
                continue;
            };
            let old_path = folder.join(old_file);
            let old_nfo_path = previous
                .managed_nfo_files
                .get(&old.id)
                .map(|file| folder.join(file));
            if !old_path.is_file() {
                continue;
            }
            match manifest.manifest.sync.on_removed {
                RemovedEntryPolicy::Trash => {
                    for subtitle in &old.subtitles {
                        let subtitle_path =
                            validated_subtitle_sidecar_path(&folder, &subtitle.file)?;
                        if subtitle_path.exists() {
                            if !file_has_hash(&subtitle_path, &subtitle.sha256) {
                                return Err(PlaylistError::Enumeration(
                                    "managed subtitle changed before removal".into(),
                                ));
                            }
                            move_to_managed_trash(&folder, &subtitle_path, now)?;
                        }
                    }
                    move_to_managed_trash(&folder, &old_path, now)?;
                    removed_definition_paths.push(old_path);
                    if let Some(old_nfo_path) = old_nfo_path.filter(|path| path.is_file()) {
                        move_to_managed_trash(&folder, &old_nfo_path, now)?;
                    }
                }
                RemovedEntryPolicy::Keep => {
                    managed_files.insert(old.id.clone(), old_file.clone());
                    if let Some(nfo_file) = previous.managed_nfo_files.get(&old.id) {
                        managed_nfo_files.insert(old.id.clone(), nfo_file.clone());
                    }
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

    let manifest_changed = existing
        .as_ref()
        .is_none_or(|state| state.manifest_etag != manifest_etag);
    let mut state = PlaylistSidecarState {
        version: PLAYLIST_SIDECAR_VERSION,
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
        managed_nfo_files,
        entries,
        overrides,
    };
    let sidecar_publication =
        if let Some(previous) = existing.as_ref().filter(|_| staged_selection.is_some()) {
            // Verification replays the sealed publication. An unchanged replay
            // must not invalidate its integrity marker merely by advancing time.
            state.last_sync = previous.last_sync;
            if state == *previous {
                SidecarPublication::NotNeeded
            } else {
                state.last_sync = now;
                SidecarPublication::Staged(sidecar_path)
            }
        } else {
            SidecarPublication::Staged(sidecar_path)
        };
    let membership_delta = PlaylistMembershipDelta::between(existing.as_ref(), &state);
    let membership_changed = !membership_delta.is_empty();
    let changed = membership_changed || manifest_changed;
    Ok(PlaylistFileSync {
        state,
        definitions,
        removed_definition_paths,
        changed,
        membership_changed,
        membership_delta,
        enumerated,
        sidecar_publication,
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
        version: PLAYLIST_SIDECAR_VERSION,
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
        managed_nfo_files: BTreeMap::new(),
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

fn validate_fragment_parents(entries: &[PlaylistEntry]) -> Result<()> {
    let full_provider_ids = entries
        .iter()
        .filter(|entry| entry.parent_provider_id.is_none())
        .map(|entry| entry.provider_id.as_deref().unwrap_or(&entry.id))
        .collect::<BTreeSet<_>>();
    for entry in entries {
        let Some(parent_provider_id) = entry.parent_provider_id.as_deref() else {
            continue;
        };
        if !full_provider_ids.contains(parent_provider_id) {
            return Err(PlaylistError::Enumeration(format!(
                "chapter fragment '{}' references full episode '{}' which is not selected",
                entry.id, parent_provider_id
            )));
        }
    }
    Ok(())
}

fn parse_ndjson_entries(value: &str) -> Result<Vec<PlaylistEntry>> {
    conform_remote_stream_list_output(value.as_bytes())
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))
}

const fn wire_content_kind(kind: AddonMediaListContentKind) -> WireContentKind {
    match kind {
        AddonMediaListContentKind::TelevisionEpisode => WireContentKind::TelevisionEpisode,
        AddonMediaListContentKind::Movie => WireContentKind::Movie,
        AddonMediaListContentKind::MusicVideo => WireContentKind::MusicVideo,
        AddonMediaListContentKind::OtherVideo => WireContentKind::OtherVideo,
        AddonMediaListContentKind::Song => WireContentKind::Song,
        AddonMediaListContentKind::Image => WireContentKind::Image,
        AddonMediaListContentKind::Auto => WireContentKind::Auto,
    }
}

const fn domain_content_kind(kind: WireContentKind) -> AddonMediaListContentKind {
    match kind {
        WireContentKind::Auto => AddonMediaListContentKind::Auto,
        WireContentKind::TelevisionEpisode => AddonMediaListContentKind::TelevisionEpisode,
        WireContentKind::Movie => AddonMediaListContentKind::Movie,
        WireContentKind::MusicVideo => AddonMediaListContentKind::MusicVideo,
        WireContentKind::OtherVideo => AddonMediaListContentKind::OtherVideo,
        WireContentKind::Song => AddonMediaListContentKind::Song,
        WireContentKind::Image => AddonMediaListContentKind::Image,
    }
}

const fn domain_availability(availability: WireAvailability) -> AddonMediaListItemAvailability {
    match availability {
        WireAvailability::Available => AddonMediaListItemAvailability::Available,
        WireAvailability::Unavailable => AddonMediaListItemAvailability::Unavailable,
        WireAvailability::Unknown => AddonMediaListItemAvailability::Unknown,
    }
}

const fn domain_liveness(liveness: WireLiveness) -> RemoteStreamLiveness {
    match liveness {
        WireLiveness::Unknown => RemoteStreamLiveness::Unknown,
        WireLiveness::Finite => RemoteStreamLiveness::Finite,
        WireLiveness::Live => RemoteStreamLiveness::Live,
    }
}

fn validate_playlist_default_liveness(defaults: &PlaylistDefaults) -> Result<()> {
    let legacy = defaults.is_live.map(|is_live| {
        if is_live {
            RemoteStreamLiveness::Live
        } else {
            RemoteStreamLiveness::Finite
        }
    });
    if defaults
        .liveness
        .zip(legacy)
        .is_some_and(|(liveness, legacy)| liveness != legacy)
    {
        return Err(PlaylistError::Manifest(
            "defaults.liveness contradicts defaults.is_live".to_owned(),
        ));
    }
    Ok(())
}

fn default_liveness(defaults: &PlaylistDefaults) -> Option<RemoteStreamLiveness> {
    defaults.liveness.or_else(|| {
        defaults.is_live.map(|is_live| {
            if is_live {
                RemoteStreamLiveness::Live
            } else {
                RemoteStreamLiveness::Finite
            }
        })
    })
}

fn effective_entry_liveness(
    defaults: &PlaylistDefaults,
    entry: &PlaylistEntry,
) -> Result<RemoteStreamLiveness> {
    if let Some(liveness) = default_liveness(defaults) {
        return Ok(liveness);
    }
    entry
        .effective_liveness()
        .map(domain_liveness)
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))
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
    if !matches!(state.version, 1..=PLAYLIST_SIDECAR_VERSION) {
        return Err(PlaylistError::Enumeration(format!(
            "managed playlist state version {} is unsupported",
            state.version
        )));
    }
    Ok(Some(state))
}

async fn load_content_kind_override(
    pool: &DbPool,
    manifest_path: &Path,
) -> Result<AddonMediaListContentKind> {
    let value = sqlx::query_scalar::<_, i32>(
        r#"SELECT "list"."ContentKindOverride"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "managed"."AddonMediaListId"
           WHERE "managed"."ManifestPath" = ?
           LIMIT 1"#,
    )
    .bind(manifest_path.to_string_lossy().as_ref())
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!("unable to load content-kind override: {error}"))
    })?;
    match value.unwrap_or(0) {
        0 => Ok(AddonMediaListContentKind::Auto),
        1 => Ok(AddonMediaListContentKind::TelevisionEpisode),
        2 => Ok(AddonMediaListContentKind::Movie),
        3 => Ok(AddonMediaListContentKind::MusicVideo),
        4 => Ok(AddonMediaListContentKind::OtherVideo),
        value => Err(PlaylistError::Enumeration(format!(
            "stored content-kind override {value} is invalid"
        ))),
    }
}

async fn append_item_chapter_fragments(
    pool: &DbPool,
    manifest_path: &Path,
    addon_id: &str,
    selected: &mut Vec<PlaylistEntry>,
) -> Result<()> {
    let requests = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        r#"SELECT "item"."ProviderId", "item"."ChapterInput",
                  "item"."ProviderChapterInput"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."AddonMediaListId" = "managed"."AddonMediaListId"
           WHERE "managed"."ManifestPath" = ?
             AND "item"."ParentProviderId" IS NULL
             AND "item"."Published" = 1
             AND (COALESCE(TRIM("item"."ChapterInput"), '') <> ''
                  OR COALESCE(TRIM("item"."ProviderChapterInput"), '') <> '')
           ORDER BY "item"."Rank", "item"."Id""#,
    )
    .bind(manifest_path.to_string_lossy().as_ref())
    .fetch_all(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!("unable to load per-item chapter settings: {error}"))
    })?;

    let selected_parents = selected
        .iter()
        .map(|entry| entry.provider_id.as_deref().unwrap_or(&entry.id))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut existing_ids = selected
        .iter()
        .map(|entry| entry.id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();

    for (provider_id, operator_input, provider_input) in requests {
        if !selected_parents.contains(&provider_id) {
            continue;
        }
        let chapters =
            effective_chapter_input(operator_input.as_deref(), provider_input.as_deref())
                .unwrap_or_default();
        let Some(parent) = selected
            .iter()
            .find(|entry| entry.provider_id.as_deref().unwrap_or(&entry.id) == provider_id)
        else {
            continue;
        };
        let mut expanded = vec![parent.clone()];
        add_playlist_chapter_fragments(&mut expanded, addon_id, chapters)?;
        for fragment in expanded.into_iter().skip(1) {
            if existing_ids.insert(fragment.id.to_ascii_lowercase()) {
                selected.push(fragment);
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct PublishedProviderSelection {
    list_id: i64,
    provider_ids: BTreeSet<String>,
    is_publishing: bool,
}

#[cfg(test)]
async fn playback_duration_overrides(
    pool: &DbPool,
    selection: Option<&PublishedProviderSelection>,
) -> Result<BTreeMap<String, u64>> {
    let selected = publication_artifact_selection(pool, selection).await?;
    selected_playback_durations(pool, selection, &selected.artifacts).await
}

#[derive(Debug, Default)]
struct PublicationArtifactSelection {
    artifacts: ersatzrs_core::repositories::AddonMediaListRevisionArtifacts,
    fragment_windows: BTreeMap<String, (Option<u64>, Option<u64>)>,
}

async fn publication_artifact_selection(
    pool: &DbPool,
    selection: Option<&PublishedProviderSelection>,
) -> Result<PublicationArtifactSelection> {
    use ersatzrs_core::repositories::{
        AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
    };

    let Some(selection) = selection else {
        return Ok(PublicationArtifactSelection::default());
    };
    let mut fragment_windows = BTreeMap::new();
    let selected = if selection.is_publishing {
        ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone())
            .publication_playback_selection(selection.list_id)
            .await
            .map_err(|error| PlaylistError::Enumeration(error.to_string()))?
    } else {
        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
                .bind(selection.list_id)
                .fetch_one(pool)
                .await
                .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        if revision <= 0 {
            return Ok(PublicationArtifactSelection::default());
        }
        let snapshot = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone())
            .revision_snapshot(selection.list_id, revision)
            .await
            .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        let Some(snapshot) = snapshot else {
            let bound: bool = sqlx::query_scalar(r#"SELECT EXISTS(
                SELECT 1 FROM "AddonMediaListRevisionItem" AS "item"
                JOIN "AddonMediaListRevision" AS "revision" ON "revision"."Id"="item"."AddonMediaListRevisionId"
                WHERE "revision"."AddonMediaListId"=? AND "revision"."Revision"=?
                  AND ("item"."PreparedArtifactId" IS NOT NULL
                       OR "item"."PlaybackTimelineJson" IS NOT NULL
                       OR "item"."SubtitleArtifactIdsJson" <> '[]'))"#)
                .bind(selection.list_id).bind(revision).fetch_one(pool).await
                .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
            if bound {
                return Err(PlaylistError::Enumeration(
                    "the playback publication snapshot is unavailable".into(),
                ));
            }
            return Ok(PublicationArtifactSelection::default());
        };
        let mut selected = AddonMediaListRevisionArtifacts::default();
        for item in snapshot.items.into_iter().filter(|item| item.published) {
            if !item.subtitle_artifact_ids.is_empty() {
                fragment_windows.insert(
                    item.provider_id.clone(),
                    (item.fragment_start_seconds, item.fragment_end_seconds),
                );
                selected
                    .subtitles
                    .insert(item.provider_id.clone(), item.subtitle_artifact_ids);
            }
            if let Some(id) = item.prepared_artifact_id {
                selected.prepared_media.insert(item.provider_id.clone(), id);
            }
            if let Some(plan) = item.playback_timeline {
                selected.streaming_plans.insert(item.provider_id, plan);
            }
        }
        selected
    };
    if selection.is_publishing && !selected.subtitles.is_empty() {
        let windows = sqlx::query_as::<_, (String, Option<i64>, Option<i64>)>(
            r#"SELECT "ProviderId", "FragmentStartSeconds", "FragmentEndSeconds"
               FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? AND "Published"=1"#,
        )
        .bind(selection.list_id)
        .fetch_all(pool)
        .await
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        for (provider_id, start, end) in windows {
            if selected.subtitles.contains_key(&provider_id) {
                fragment_windows.insert(
                    provider_id.clone(),
                    (
                        staged_u64(start, "subtitle fragment start", &provider_id)?,
                        staged_u64(end, "subtitle fragment end", &provider_id)?,
                    ),
                );
            }
        }
    }
    Ok(PublicationArtifactSelection {
        artifacts: selected,
        fragment_windows,
    })
}

async fn selected_playback_durations(
    pool: &DbPool,
    selection: Option<&PublishedProviderSelection>,
    selected: &ersatzrs_core::repositories::AddonMediaListRevisionArtifacts,
) -> Result<BTreeMap<String, u64>> {
    let Some(selection) = selection else {
        return Ok(BTreeMap::new());
    };
    let mut durations: BTreeMap<_, _> = selected
        .streaming_plans
        .iter()
        .map(|(provider_id, plan)| (provider_id.clone(), plan.duration_milliseconds()))
        .collect();
    for (provider_id, artifact_id) in &selected.prepared_media {
        if durations.contains_key(provider_id) {
            return Err(PlaylistError::Enumeration(
                "the publication selects conflicting playback modes".into(),
            ));
        }
        let duration: Option<i64> = sqlx::query_scalar(
            r#"SELECT "DurationMilliseconds" FROM "AddonMediaListArtifact"
               WHERE "Id"=? AND "AddonMediaListId"=? AND "Kind"=1 AND "State"=2
                 AND "DurationMilliseconds">0"#,
        )
        .bind(artifact_id)
        .bind(selection.list_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        let duration = duration
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| {
                PlaylistError::Enumeration(
                    "the selected prepared media duration is unavailable".into(),
                )
            })?;
        durations.insert(provider_id.clone(), duration);
    }
    Ok(durations)
}

const MAX_MANAGED_SUBTITLE_BYTES: usize = 4 * 1024 * 1024;

#[allow(clippy::case_sensitive_file_extension_comparisons)] // Managed artifact names are normalized lowercase on every OS.
fn validated_subtitle_sidecar_path(folder: &Path, file: &str) -> Result<PathBuf> {
    if !matches!(
        (
            Path::new(file).components().next(),
            Path::new(file).components().count()
        ),
        (Some(Component::Normal(_)), 1)
    ) || !file.ends_with(".srt")
        || file.contains(['\\', ':'])
    {
        return Err(PlaylistError::Enumeration(
            "invalid managed subtitle filename".into(),
        ));
    }
    let path = folder.join(file);
    if std::fs::symlink_metadata(&path).is_ok_and(|metadata| {
        !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_MANAGED_SUBTITLE_BYTES as u64
    }) {
        return Err(PlaylistError::Enumeration(
            "invalid managed subtitle file".into(),
        ));
    }
    Ok(path)
}

async fn materialize_selected_subtitles(
    pool: &DbPool,
    owner: Option<&PublishedProviderSelection>,
    selected: &PublicationArtifactSelection,
    entry: &PlaylistEntry,
    definition_path: &Path,
    previous: &[ManagedPlaylistSubtitle],
    artifact_root: &Path,
) -> Result<Vec<ManagedPlaylistSubtitle>> {
    let Some(owner) = owner else {
        return Ok(Vec::new());
    };
    let provider_id = entry.provider_id.as_deref().unwrap_or(&entry.id);
    let ids = selected
        .artifacts
        .subtitles
        .get(provider_id)
        .map_or(&[][..], Vec::as_slice);
    if ids.len() > 2 {
        return Err(PlaylistError::Enumeration(
            "publication selected too many subtitles".into(),
        ));
    }
    let folder = definition_path
        .parent()
        .ok_or_else(|| PlaylistError::Enumeration("subtitle folder is unavailable".into()))?;
    let stem = definition_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PlaylistError::Enumeration("subtitle stem is unavailable".into()))?;
    let mut next = Vec::new();
    let mut languages = BTreeSet::new();
    for id in ids {
        let artifact = ersatzrs_infra::addon_media_list_subtitles::read_verified_subtitle_artifact(
            pool,
            owner.list_id,
            *id,
            artifact_root,
        )
        .await
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        let logical_language =
            ersatzrs_addon_contract::subtitle_logical_language(&artifact.language)
                .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        let language = ersatzrs_core::language_codes::canonical_language_code(&logical_language)
            .ok_or_else(|| PlaylistError::Enumeration("invalid subtitle language".into()))?;
        if !languages.insert(language.clone()) {
            return Err(PlaylistError::Enumeration(
                "publication selected duplicate subtitle languages".into(),
            ));
        }
        let content = subtitle_programme_content(
            &artifact.content,
            artifact.prepared,
            provider_id,
            selected,
            entry,
        )?;
        if content.is_empty() {
            continue;
        }
        let file = format!("{stem}.{language}.srt");
        let path = validated_subtitle_sidecar_path(folder, &file)?;
        let sha256 = hex_hash(content.as_bytes());
        if path.exists()
            && !file_has_hash(&path, &sha256)
            && !previous
                .iter()
                .any(|old| old.file == file && file_has_hash(&path, &old.sha256))
        {
            return Err(PlaylistError::Enumeration(
                "an existing subtitle conflicts with the publication".into(),
            ));
        }
        if !file_has_hash(&path, &sha256) {
            write_bytes_atomically(&path, content.as_bytes())?;
        }
        next.push(ManagedPlaylistSubtitle {
            artifact_id: *id,
            language,
            actual_language: artifact.language,
            file,
            sha256,
        });
    }
    for old in previous {
        if next.iter().any(|subtitle| subtitle.file == old.file) {
            continue;
        }
        let path = validated_subtitle_sidecar_path(folder, &old.file)?;
        if path.exists() {
            if !file_has_hash(&path, &old.sha256) {
                return Err(PlaylistError::Enumeration(
                    "managed subtitle changed before replacement".into(),
                ));
            }
            std::fs::remove_file(path).map_err(|_| {
                PlaylistError::Enumeration("unable to remove obsolete managed subtitle".into())
            })?;
        }
    }
    Ok(next)
}

fn subtitle_programme_content(
    content: &str,
    prepared: bool,
    provider_id: &str,
    selected: &PublicationArtifactSelection,
    entry: &PlaylistEntry,
) -> Result<String> {
    let prepared_media = selected.artifacts.prepared_media.contains_key(provider_id);
    if prepared != prepared_media {
        return Err(PlaylistError::Enumeration(
            "subtitle clock does not match published media".into(),
        ));
    }
    if prepared {
        // Native preparation applies fragment and SponsorBlock cuts to media and sidecars together.
        return Ok(content.to_owned());
    }
    let timeline = if let Some(plan) = selected.artifacts.streaming_plans.get(provider_id) {
        Some(
            plan.timeline()
                .map_err(|error| PlaylistError::Enumeration(error.to_string()))?,
        )
    } else {
        let (start, end) = selected
            .fragment_windows
            .get(provider_id)
            .copied()
            .unwrap_or((entry.fragment_start_seconds, entry.fragment_end_seconds));
        match (start, end) {
            (None, None) => None,
            (start, Some(end)) => {
                let start = start.unwrap_or(0).checked_mul(1000).ok_or_else(|| {
                    PlaylistError::Enumeration("subtitle fragment overflow".into())
                })?;
                let end = end.checked_mul(1000).ok_or_else(|| {
                    PlaylistError::Enumeration("subtitle fragment overflow".into())
                })?;
                Some(
                    ersatzrs_domain::MediaPlaybackTimeline::from_exclusions(end, start..end, &[])
                        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?,
                )
            }
            _ => {
                return Err(PlaylistError::Enumeration(
                    "subtitle fragment end is unavailable".into(),
                ))
            }
        }
    };
    timeline.map_or_else(
        || Ok(content.to_owned()),
        |timeline| {
            ersatzrs_infra::subtitles::map_srt_through_timeline(
                content,
                &timeline,
                MAX_MANAGED_SUBTITLE_BYTES,
            )
            .map_err(|error| PlaylistError::Enumeration(error.to_string()))
        },
    )
}

/// Return the provider identities in the current published snapshot and
/// whether an explicit publication is currently staged.
///
/// `None` preserves compatibility for unmanaged manifests. Managed draft
/// lists return an empty set, so scanners cannot accidentally publish them.
async fn published_provider_selection(
    pool: &DbPool,
    manifest_path: &Path,
) -> Result<Option<PublishedProviderSelection>> {
    let managed = sqlx::query_as::<_, (i64, i32)>(
        r#"SELECT "managed"."AddonMediaListId", "list"."ReviewState"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "AddonMediaList" AS "list"
             ON "list"."Id" = "managed"."AddonMediaListId"
           WHERE "managed"."ManifestPath" = ? LIMIT 1"#,
    )
    .bind(manifest_path.to_string_lossy().as_ref())
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!(
            "unable to resolve managed list publication: {error}"
        ))
    })?;
    let Some((list_id, review_state)) = managed else {
        return Ok(None);
    };
    let is_publishing = match review_state {
        0..=2 => false,
        3 => true,
        value => {
            return Err(PlaylistError::Enumeration(format!(
                "stored add-on media-list review state {value} is invalid"
            )))
        }
    };
    let values = sqlx::query_scalar::<_, String>(
        r#"SELECT "ProviderId" FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "Published" = 1"#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!("unable to load published list items: {error}"))
    })?;
    Ok(Some(PublishedProviderSelection {
        list_id,
        provider_ids: values.into_iter().collect(),
        is_publishing,
    }))
}

/// Rebuild the scanner input from the reviewed control-plane snapshot.
///
/// These are the same provider identities and playback fields persisted by
/// `media-list.list.v4` during Import. Publication must not invoke an add-on a
/// second time merely to convert the already imported list into another
/// library hierarchy.
async fn load_staged_playlist_entries(
    pool: &DbPool,
    list_id: i64,
    content_kind_override: AddonMediaListContentKind,
) -> Result<Vec<PlaylistEntry>> {
    type StagedItemRow = (
        String,
        Option<String>,
        Option<String>,
        String,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        i32,
        Option<String>,
        i32,
        Option<String>,
        i32,
    );

    let rows = sqlx::query_as::<_, StagedItemRow>(
        r#"SELECT "ProviderId", "SourceUrl", "Title", "DisplayTitle", "DurationSeconds",
                  "ParentProviderId", "FragmentStartSeconds", "FragmentEndSeconds",
                  "PublishedAvailability", "PublishedAvailabilityReason",
                  "ContentKind", "AdditionalImageUrls",
                  "PublishedLiveness"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "Published" = 1
           ORDER BY "Rank", "Id""#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!(
            "unable to load the staged add-on media-list snapshot: {error}"
        ))
    })?;

    rows.into_iter()
        .map(
            |(
                provider_id,
                source_url,
                title,
                display_title,
                duration_seconds,
                parent_provider_id,
                fragment_start_seconds,
                fragment_end_seconds,
                availability,
                availability_reason,
                content_kind,
                additional_image_urls,
                published_liveness,
            )| {
                let source_url = source_url
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        PlaylistError::Enumeration(format!(
                            "staged item '{provider_id}' has no playback URL"
                        ))
                    })?;
                let duration_seconds = staged_u64(duration_seconds, "duration", &provider_id)?;
                let fragment_start_seconds =
                    staged_u64(fragment_start_seconds, "fragment start", &provider_id)?;
                let fragment_end_seconds =
                    staged_u64(fragment_end_seconds, "fragment end", &provider_id)?;
                let additional_image_urls = additional_image_urls
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| {
                        serde_json::from_str(&value).map_err(|error| {
                            PlaylistError::Enumeration(format!(
                                "staged item '{provider_id}' has invalid artwork URLs: {error}"
                            ))
                        })
                    })
                    .transpose()?
                    .unwrap_or_default();
                let content_kind = if content_kind_override == AddonMediaListContentKind::Auto {
                    stored_content_kind(content_kind, &provider_id)?
                } else {
                    content_kind_override
                };
                Ok(PlaylistEntry {
                    id: provider_id.clone(),
                    provider_id: Some(provider_id.clone()),
                    url: source_url,
                    title: title.unwrap_or(display_title),
                    duration_seconds,
                    additional_image_urls,
                    parent_provider_id,
                    fragment_start_seconds,
                    fragment_end_seconds,
                    availability: stored_availability(availability, &provider_id)?,
                    availability_reason,
                    content_kind: wire_content_kind(content_kind),
                    liveness: Some(stored_liveness(published_liveness, &provider_id)?),
                    ..PlaylistEntry::default()
                })
            },
        )
        .collect()
}

fn staged_u64(value: Option<i64>, field: &str, provider_id: &str) -> Result<Option<u64>> {
    value
        .map(|value| {
            u64::try_from(value).map_err(|_| {
                PlaylistError::Enumeration(format!(
                    "staged item '{provider_id}' has an invalid {field}"
                ))
            })
        })
        .transpose()
}

fn stored_availability(value: i32, provider_id: &str) -> Result<WireAvailability> {
    match value {
        0 => Ok(WireAvailability::Available),
        1 => Ok(WireAvailability::Unavailable),
        2 => Ok(WireAvailability::Unknown),
        _ => Err(PlaylistError::Enumeration(format!(
            "staged item '{provider_id}' has invalid availability {value}"
        ))),
    }
}

fn stored_content_kind(value: i32, provider_id: &str) -> Result<AddonMediaListContentKind> {
    match value {
        0 => Ok(AddonMediaListContentKind::Auto),
        1 => Ok(AddonMediaListContentKind::TelevisionEpisode),
        2 => Ok(AddonMediaListContentKind::Movie),
        3 => Ok(AddonMediaListContentKind::MusicVideo),
        4 => Ok(AddonMediaListContentKind::OtherVideo),
        5 => Ok(AddonMediaListContentKind::Song),
        6 => Ok(AddonMediaListContentKind::Image),
        _ => Err(PlaylistError::Enumeration(format!(
            "staged item '{provider_id}' has invalid content kind {value}"
        ))),
    }
}

fn stored_liveness(value: i32, provider_id: &str) -> Result<WireLiveness> {
    match value {
        0 => Ok(WireLiveness::Unknown),
        1 => Ok(WireLiveness::Finite),
        2 => Ok(WireLiveness::Live),
        _ => Err(PlaylistError::Enumeration(format!(
            "staged item '{provider_id}' has invalid liveness {value}"
        ))),
    }
}

async fn adopt_external_metadata(
    pool: &DbPool,
    manifest_path: &Path,
    provider_id: &str,
    path: &Path,
    bytes: &[u8],
) -> Result<()> {
    let edits = ersatzrs_infra::addon_media_nfo::external_overrides(path)
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    if edits.is_empty() {
        return Ok(());
    }
    let row = sqlx::query_as::<_, (i64, String)>(
        r#"SELECT "item"."Id", "item"."MetadataOverrideJson"
        FROM "AddonMediaListItem" AS "item" JOIN "AddonMediaListLibrary" AS "managed"
        ON "managed"."AddonMediaListId"="item"."AddonMediaListId"
        WHERE "managed"."ManifestPath"=? AND "item"."ProviderId"=? AND "item"."Published"=1"#,
    )
    .bind(manifest_path.to_string_lossy().as_ref())
    .bind(provider_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    let Some((id, before)) = row else {
        return Ok(());
    };
    let mut overrides: BTreeMap<String, serde_json::Value> = serde_json::from_str(&before)
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    overrides.extend(edits);
    let after = serde_json::to_string(&overrides)
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    let saved = sqlx::query(
        r#"UPDATE "AddonMediaListItem" SET "MetadataOverrideJson"=?
        WHERE "Id"=? AND "MetadataOverrideJson"=?"#,
    )
    .bind(after)
    .bind(id)
    .bind(before)
    .execute(pool)
    .await
    .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    if saved.rows_affected() != 1 {
        return Err(PlaylistError::Enumeration(
            "metadata changed while adopting external edits".into(),
        ));
    }
    ersatzrs_infra::addon_media_nfo::acknowledge_external_overrides(path, &hex_hash(bytes))
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))
}

async fn previous_publication_metadata(
    pool: &DbPool,
    manifest_path: &Path,
) -> Result<Option<ersatzrs_core::repositories::AddonMediaListRevisionSnapshot>> {
    use ersatzrs_core::repositories::AddonMediaListLifecycleRepository as _;
    let owner = sqlx::query_as::<_, (i64, i32)>(
        r#"SELECT "list"."Id", "list"."PublishedRevision"
        FROM "AddonMediaList" AS "list" JOIN "AddonMediaListLibrary" AS "managed"
        ON "managed"."AddonMediaListId"="list"."Id" WHERE "managed"."ManifestPath"=?"#,
    )
    .bind(manifest_path.to_string_lossy().as_ref())
    .fetch_optional(pool)
    .await
    .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    let Some((id, revision)) = owner.filter(|(_, revision)| *revision > 0) else {
        return Ok(None);
    };
    ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone())
        .revision_snapshot(id, revision)
        .await
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))
}

fn legacy_projected_metadata(
    manifest: &ValidatedPlaylistManifest,
    entry: &PlaylistEntry,
    snapshot: &ersatzrs_core::repositories::AddonMediaListRevisionSnapshot,
    item: &ersatzrs_core::repositories::AddonMediaListRevisionItem,
    previous: Option<&PlaylistSidecarEntry>,
) -> Result<ersatzrs_core::repositories::AddonMediaListNfoMetadata> {
    use ersatzrs_core::repositories::AddonMediaListNfoMetadata;
    let mut metadata: AddonMediaListNfoMetadata = serde_json::from_str(&item.metadata_json)
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
    if metadata.title.trim().is_empty() {
        metadata.title.clone_from(&item.title);
    }
    metadata.year = metadata.year.or(item.year);
    if snapshot.presentation_kind == ersatzrs_domain::AddonMediaListPresentationKind::TelevisionShow
    {
        let parent: AddonMediaListNfoMetadata = serde_json::from_str(&snapshot.metadata_json)
            .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
        metadata.show_title = Some(if parent.title.trim().is_empty() {
            snapshot.name.clone()
        } else {
            parent.title
        });
        metadata.season = metadata.season.or(item.season);
        metadata.episode = metadata.episode.or(item.episode);
    }
    let artwork = previous.map_or_else(BTreeMap::new, |previous| previous.artwork_roles.clone());
    let bytes = generated_reviewed_nfo_bytes(manifest, entry, &metadata, &artwork)?;
    ersatzrs_infra::addon_media_nfo::parse(&bytes)
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))
}

/// Apply the provider snapshot plus field-level operator overrides retained by
/// the shared metadata editor. This runs after add-on enrichment so both
/// Beeld & Geluid and yt-dlp reach the exact same publication boundary.
async fn apply_reviewed_metadata(
    pool: &DbPool,
    manifest_path: &Path,
    selected: &mut [PlaylistEntry],
) -> Result<BTreeMap<String, ersatzrs_core::repositories::AddonMediaListNfoMetadata>> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        r#"SELECT "item"."ProviderId", "item"."ProviderMetadataJson",
                  "item"."MetadataOverrideJson"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."AddonMediaListId" = "managed"."AddonMediaListId"
           WHERE "managed"."ManifestPath" = ? AND "item"."Published" = 1"#,
    )
    .bind(manifest_path.to_string_lossy().as_ref())
    .fetch_all(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!("unable to load reviewed metadata: {error}"))
    })?;
    let mut by_provider = BTreeMap::new();
    for (provider_id, provider_json, override_json) in rows {
        let mut provider: serde_json::Value =
            serde_json::from_str(&provider_json).map_err(|error| {
                PlaylistError::Enumeration(format!("stored provider metadata is invalid: {error}"))
            })?;
        let overrides: serde_json::Value =
            serde_json::from_str(&override_json).map_err(|error| {
                PlaylistError::Enumeration(format!("stored metadata override is invalid: {error}"))
            })?;
        if let (Some(provider), Some(overrides)) = (provider.as_object_mut(), overrides.as_object())
        {
            for (field, value) in overrides {
                provider.insert(field.clone(), value.clone());
            }
        }
        let metadata: ersatzrs_core::repositories::AddonMediaListNfoMetadata =
            serde_json::from_value(provider).map_err(|error| {
                PlaylistError::Enumeration(format!("stored reviewed metadata is invalid: {error}"))
            })?;
        by_provider.insert(provider_id, metadata);
    }
    for entry in selected {
        let provider_id = entry.provider_id.as_deref().unwrap_or(&entry.id);
        let Some(metadata) = by_provider.get(provider_id) else {
            continue;
        };
        apply_metadata_to_entry(entry, metadata);
    }
    Ok(by_provider)
}

fn apply_metadata_to_entry(
    entry: &mut PlaylistEntry,
    metadata: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
) {
    entry.title.clone_from(&metadata.title);
    entry.original_title.clone_from(&metadata.original_title);
    entry.sort_title.clone_from(&metadata.sort_title);
    entry.outline.clone_from(&metadata.outline);
    entry.plot.clone_from(&metadata.plot);
    entry.tagline.clone_from(&metadata.tagline);
    entry.show_title.clone_from(&metadata.show_title);
    entry.season = metadata.season;
    entry.episode = metadata.episode;
    entry.year = metadata.year;
    entry.release_date.clone_from(&metadata.release_date);
    entry.content_rating = metadata.content_ratings.first().cloned();
    entry.genres.clone_from(&metadata.genres);
    entry.tags.clone_from(&metadata.tags);
    entry.collection.clone_from(&metadata.collection);
    entry.people = metadata
        .people
        .iter()
        .map(|person| ersatzrs_addon_contract::RemoteStreamPerson {
            name: person.name.clone(),
            role: person.role.clone().unwrap_or_default(),
        })
        .collect();
    entry.producers.clone_from(&metadata.producers);
    entry
        .original_broadcasters
        .clone_from(&metadata.original_broadcasters);
    entry.broadcasters.clone_from(&metadata.broadcasters);
    let preferred_artwork = metadata
        .artwork
        .iter()
        .find(|candidate| {
            candidate.role == ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb
        })
        .or_else(|| {
            metadata.artwork.iter().find(|candidate| {
                candidate.role == ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster
            })
        })
        .or_else(|| metadata.artwork.first());
    entry.thumbnail_url = preferred_artwork
        .map(|candidate| candidate.url.clone())
        .or_else(|| metadata.artwork_urls.first().cloned());
    let mut additional_image_urls = Vec::new();
    for url in metadata
        .artwork
        .iter()
        .map(|candidate| candidate.url.as_str())
        .chain(metadata.artwork_urls.iter().map(String::as_str))
    {
        if entry.thumbnail_url.as_deref() != Some(url)
            && !additional_image_urls.iter().any(|existing| existing == url)
        {
            additional_image_urls.push(url.to_owned());
        }
    }
    entry.additional_image_urls = additional_image_urls;
    entry.guids.clone_from(&metadata.guids);
}

fn effective_chapter_input<'a>(
    operator: Option<&'a str>,
    provider: Option<&'a str>,
) -> Option<&'a str> {
    operator
        .filter(|value| !value.trim().is_empty())
        .or_else(|| provider.filter(|value| !value.trim().is_empty()))
}

/// Ask the add-on to describe the entries this synchronization kept.
///
/// Enrichment is strictly additive and never fatal. An add-on without the
/// capability, a failed or timed-out invocation, and a malformed response all
/// leave the enumerated entries exactly as they were, because richer metadata
/// must never turn a working synchronization into a failed one.
async fn enrich_selected_entries(
    pool: &DbPool,
    manifest_path: &Path,
    addon_id: &str,
    runtime: &AddonRuntimeContext,
    selected: &mut [PlaylistEntry],
) {
    if selected.is_empty() {
        return;
    }
    let Ok(addon) = load_installed_addon(pool, addon_id).await else {
        return;
    };
    let Some(addon) = addon else {
        return;
    };
    let Some(capability) = retained_item_capability(&addon) else {
        return;
    };

    // Fragments describe a slice of a parent that is itself in the request, so
    // asking about them would duplicate provider work for no new metadata.
    let requested = selected
        .iter()
        .filter(|entry| entry.parent_provider_id.is_none())
        .map(|entry| {
            entry
                .provider_id
                .clone()
                .unwrap_or_else(|| entry.id.clone())
        })
        .collect::<Vec<_>>();
    if requested.is_empty() {
        return;
    }
    let environment = BTreeMap::from([(
        "ERSATZRS_REMOTE_STREAM_ITEM_IDS".to_owned(),
        requested.join("\n"),
    )]);

    let stdout = match invoke_enabled_addon(
        pool,
        addon_id,
        AddonInvocation {
            capability,
            operation: "item",
            runtime,
            extra_environment: &environment,
            timeout: ENUMERATION_TIMEOUT,
            max_output_bytes: MAX_REMOTE_STREAM_LIST_OUTPUT_BYTES,
        },
    )
    .await
    {
        Ok(stdout) => stdout,
        Err(error) => {
            tracing::warn!(
                addon_id,
                requested = requested.len(),
                failure_kind = "invoke",
                operation_message = %error,
                "add-on item metadata was skipped"
            );
            return;
        }
    };
    let records = match conform_remote_stream_item_output_for_capability(
        &stdout,
        capability == CAPABILITY_REMOTE_STREAM_ITEM_V2,
    ) {
        Ok(records) => records,
        Err(error) => {
            tracing::warn!(
                addon_id,
                requested = requested.len(),
                failure_kind = "decode",
                output_bytes = stdout.len(),
                operation_message = %error,
                "add-on item metadata was skipped"
            );
            return;
        }
    };

    if capability == CAPABILITY_REMOTE_STREAM_ITEM_V2 {
        persist_provider_chapter_inputs(pool, manifest_path, addon_id, &records).await;
    }
    let mut by_provider_id = records
        .into_iter()
        .map(|record| (record.provider_id.to_ascii_lowercase(), record))
        .collect::<BTreeMap<_, _>>();
    for entry in selected.iter_mut() {
        let key = entry
            .provider_id
            .clone()
            .unwrap_or_else(|| entry.id.clone())
            .to_ascii_lowercase();
        if let Some(record) = by_provider_id.remove(&key) {
            record.apply_to(entry);
        }
    }
}

fn retained_item_capability(addon: &InstalledAddon) -> Option<&'static str> {
    if addon
        .manifest
        .has_capability(CAPABILITY_REMOTE_STREAM_ITEM_V2)
    {
        Some(CAPABILITY_REMOTE_STREAM_ITEM_V2)
    } else if addon
        .manifest
        .has_capability(CAPABILITY_REMOTE_STREAM_ITEM_V1)
    {
        Some(CAPABILITY_REMOTE_STREAM_ITEM_V1)
    } else {
        None
    }
}

async fn persist_provider_chapter_inputs(
    pool: &DbPool,
    manifest_path: &Path,
    addon_id: &str,
    records: &[ersatzrs_addon_contract::PlaylistEntryMetadata],
) {
    for record in records {
        if let Err(error) = sqlx::query(
            r#"UPDATE "AddonMediaListItem"
               SET "ProviderChapterInput" = ?
               WHERE "ProviderId" = ? AND "ParentProviderId" IS NULL
                 AND "AddonMediaListId" = (
                     SELECT "AddonMediaListId" FROM "AddonMediaListLibrary"
                     WHERE "ManifestPath" = ?
                 )"#,
        )
        .bind(
            record
                .chapter_input
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
        )
        .bind(&record.provider_id)
        .bind(manifest_path.to_string_lossy().as_ref())
        .execute(pool)
        .await
        {
            tracing::warn!(
                addon_id,
                provider_id = %record.provider_id,
                operation_message = %error,
                "provider chapter text could not be persisted"
            );
        }
    }
}

fn managed_files_intact(folder: &Path, state: &PlaylistSidecarState) -> bool {
    state.entries.iter().all(|entry| {
        let definition_path = folder.join(&entry.file);
        let Some(definition_hash) = entry.definition_sha256.as_deref() else {
            return false;
        };
        let Some(nfo_file) = entry
            .nfo_file
            .as_deref()
            .or_else(|| state.managed_nfo_files.get(&entry.id).map(String::as_str))
        else {
            return false;
        };
        file_has_hash(&definition_path, definition_hash)
            && folder.join(nfo_file).is_file()
            && entry.subtitles.iter().all(|subtitle| {
                validated_subtitle_sidecar_path(folder, &subtitle.file)
                    .is_ok_and(|path| file_has_hash(&path, &subtitle.sha256))
            })
            && entry
                .artwork
                .as_ref()
                .is_none_or(|artwork| file_has_hash(&folder.join(&artwork.file), &artwork.sha256))
    })
}

fn file_has_hash(path: &Path, expected: &str) -> bool {
    std::fs::read(path).is_ok_and(|bytes| hex_hash(&bytes) == expected)
}

// Inclusion and cached scanner definitions expose coarse seconds. Round up so
// Positive subsecond edited media remains schedulable; YAML retains exact ms.
fn playback_inclusion_duration(
    provider_seconds: Option<u64>,
    playback_ms: Option<u64>,
) -> Option<u64> {
    playback_ms
        .map(|milliseconds| milliseconds.div_ceil(1000))
        .or(provider_seconds)
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

#[cfg(test)]
fn generated_definition_bytes(
    manifest: &ValidatedPlaylistManifest,
    entry: &PlaylistEntry,
) -> Result<Vec<u8>> {
    generated_definition_bytes_with_duration(manifest, entry, None)
}

#[cfg(test)]
pub(crate) fn prepared_definition_bytes_for_test(
    manifest: &ValidatedPlaylistManifest,
    entry: &PlaylistEntry,
    milliseconds: u64,
) -> Result<Vec<u8>> {
    generated_definition_bytes_with_duration(manifest, entry, Some(milliseconds))
}

fn generated_definition_bytes_with_duration(
    manifest: &ValidatedPlaylistManifest,
    entry: &PlaylistEntry,
    playback_duration_milliseconds: Option<u64>,
) -> Result<Vec<u8>> {
    let mut artwork_urls = entry
        .thumbnail_url
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    for url in &entry.additional_image_urls {
        if !artwork_urls.contains(&url.as_str()) {
            artwork_urls.push(url);
        }
    }
    let liveness = effective_entry_liveness(&manifest.manifest.defaults, entry)?;
    let definition = GeneratedRemoteStreamDefinition {
        provider_id: entry.provider_id.as_deref().unwrap_or(&entry.id),
        parent_provider_id: entry.parent_provider_id.as_deref(),
        fragment_start_seconds: entry.fragment_start_seconds,
        fragment_end_seconds: entry.fragment_end_seconds,
        url: &entry.url,
        addon: &manifest.manifest.addon,
        liveness,
        is_live: liveness.legacy_is_live(),
        duration: match playback_duration_milliseconds {
            Some(0) => {
                return Err(PlaylistError::Enumeration(
                    "playback duration must be positive".into(),
                ))
            }
            Some(milliseconds) => Some(format!(
                "{}.{:03}",
                format_clock_duration(milliseconds / 1000),
                milliseconds % 1000
            )),
            None => entry.duration_seconds.map(format_clock_duration),
        },
        availability: domain_availability(entry.availability),
        availability_reason: entry.availability_reason.as_deref(),
        content_kind: domain_content_kind(entry.content_kind),
        artwork_urls,
    };
    let yaml = serde_yaml::to_string(&definition).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to encode a managed definition: {error}"))
    })?;
    let content = format!(
        "# Generated by ErsatzRS from a Remote Stream playlist manifest. Do not edit.\n{yaml}"
    );
    Ok(content.into_bytes())
}

#[cfg(test)]
fn write_generated_nfo(
    path: &Path,
    manifest: &ValidatedPlaylistManifest,
    entry: &PlaylistEntry,
) -> Result<()> {
    let bytes = generated_nfo_bytes(manifest, entry)?;
    write_bytes_atomically(path, &bytes)
}

#[allow(clippy::too_many_lines)]
fn generated_nfo_bytes(
    manifest: &ValidatedPlaylistManifest,
    entry: &PlaylistEntry,
) -> Result<Vec<u8>> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    write_xml_event(
        &mut writer,
        Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)),
    )?;
    let root_name = match entry.content_kind {
        WireContentKind::TelevisionEpisode => "episodedetails",
        WireContentKind::MusicVideo => "musicvideo",
        WireContentKind::Auto
        | WireContentKind::Movie
        | WireContentKind::OtherVideo
        | WireContentKind::Song
        | WireContentKind::Image => "movie",
    };
    let mut root = BytesStart::new(root_name);
    root.push_attribute(("xmlns:ersatzrs", "urn:ersatzrs:nfo:1"));
    write_xml_event(&mut writer, Event::Start(root))?;

    write_xml_text(&mut writer, "title", Some(&entry.title))?;
    write_xml_text(
        &mut writer,
        "originaltitle",
        entry.original_title.as_deref(),
    )?;
    write_xml_text(&mut writer, "sorttitle", entry.sort_title.as_deref())?;
    write_xml_text(&mut writer, "outline", entry.outline.as_deref())?;
    write_xml_text(&mut writer, "plot", entry.plot.as_deref())?;
    write_xml_text(&mut writer, "tagline", entry.tagline.as_deref())?;
    let year = entry.year.or(manifest.manifest.defaults.year);
    write_xml_text(
        &mut writer,
        "year",
        year.as_ref().map(ToString::to_string).as_deref(),
    )?;
    let release_day = entry
        .release_date
        .as_deref()
        .and_then(|value| value.get(..10))
        .filter(|value| {
            value.chars().enumerate().all(|(index, character)| {
                matches!(index, 4 | 7) && character == '-'
                    || !matches!(index, 4 | 7) && character.is_ascii_digit()
            })
        });
    write_xml_text(&mut writer, "premiered", release_day)?;
    write_xml_text(&mut writer, "releasedate", release_day)?;
    let runtime = entry
        .duration_seconds
        .filter(|seconds| *seconds > 0)
        .map(|seconds| seconds.div_ceil(60).to_string());
    write_xml_text(&mut writer, "runtime", runtime.as_deref())?;
    write_xml_text(
        &mut writer,
        "mpaa",
        entry
            .content_rating
            .as_deref()
            .or(manifest.manifest.defaults.content_rating.as_deref()),
    )?;
    if root_name == "episodedetails" {
        write_xml_text(&mut writer, "showtitle", entry.show_title.as_deref())?;
        write_xml_text(
            &mut writer,
            "season",
            entry.season.as_ref().map(ToString::to_string).as_deref(),
        )?;
        write_xml_text(
            &mut writer,
            "episode",
            entry.episode.as_ref().map(ToString::to_string).as_deref(),
        )?;
    }
    for value in &entry.genres {
        write_xml_text(&mut writer, "genre", Some(value))?;
    }
    for value in union_values(&manifest.manifest.tags, &entry.tags) {
        write_xml_text(&mut writer, "tag", Some(&value))?;
    }
    if let Some(collection) = entry.collection.as_deref() {
        write_xml_text(&mut writer, "tag", Some(collection))?;
    }
    let studios = union_values(
        &union_values(&entry.original_broadcasters, &entry.broadcasters),
        &entry.producers,
    );
    for value in studios {
        write_xml_text(&mut writer, "studio", Some(&value))?;
    }
    let mut directors = Vec::new();
    let mut writers = Vec::new();
    let mut actors = Vec::new();
    let mut artists = Vec::new();
    for person in &entry.people {
        match person.role.trim().to_ascii_lowercase().as_str() {
            "director" => directors.push(person.name.as_str()),
            "writer" | "screenplay" => writers.push(person.name.as_str()),
            "artist" if root_name == "musicvideo" => artists.push(person.name.as_str()),
            "producer" => {}
            _ => actors.push(person),
        }
    }
    for person in actors {
        write_nfo_actor(&mut writer, person)?;
    }
    for director in directors {
        write_xml_text(&mut writer, "director", Some(director))?;
    }
    for writer_name in writers {
        write_xml_text(&mut writer, "credits", Some(writer_name))?;
    }
    for artist in artists {
        write_xml_text(&mut writer, "artist", Some(artist))?;
    }
    write_unique_id(
        &mut writer,
        &manifest.manifest.addon,
        entry.provider_id.as_deref().unwrap_or(&entry.id),
        true,
    )?;
    for guid in &entry.guids {
        if let Some((source, value)) = guid.split_once("://") {
            write_unique_id(&mut writer, source, value, false)?;
        }
    }
    write_xml_text(&mut writer, "thumb", entry.thumbnail_url.as_deref())?;
    write_xml_text(
        &mut writer,
        "ersatzrs:release_datetime",
        entry.release_date.as_deref(),
    )?;
    write_xml_text(
        &mut writer,
        "ersatzrs:collection",
        entry.collection.as_deref(),
    )?;
    for producer in &entry.producers {
        write_xml_text(&mut writer, "ersatzrs:producer", Some(producer))?;
    }
    for broadcaster in &entry.original_broadcasters {
        write_xml_text(
            &mut writer,
            "ersatzrs:original_broadcaster",
            Some(broadcaster),
        )?;
    }
    for broadcaster in &entry.broadcasters {
        write_xml_text(&mut writer, "ersatzrs:broadcaster", Some(broadcaster))?;
    }
    write_xml_event(&mut writer, Event::End(BytesEnd::new(root_name)))?;
    let mut bytes = writer.into_inner();
    bytes.push(b'\n');
    let xml = std::str::from_utf8(&bytes).map_err(|error| {
        PlaylistError::Enumeration(format!("generated NFO is not UTF-8: {error}"))
    })?;
    crate::nfo::parse_other_video_nfo_str(xml).map_err(|error| {
        PlaylistError::Enumeration(format!("generated NFO failed validation: {error}"))
    })?;
    Ok(bytes)
}

fn item_nfo_root(entry: &PlaylistEntry) -> &'static str {
    match entry.content_kind {
        WireContentKind::TelevisionEpisode => "episodedetails",
        WireContentKind::MusicVideo => "musicvideo",
        WireContentKind::Auto
        | WireContentKind::Movie
        | WireContentKind::OtherVideo
        | WireContentKind::Song
        | WireContentKind::Image => "movie",
    }
}

fn generated_reviewed_nfo_bytes(
    manifest: &ValidatedPlaylistManifest,
    entry: &PlaylistEntry,
    metadata: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
    artwork_roles: &BTreeMap<String, ManagedPlaylistArtwork>,
) -> Result<Vec<u8>> {
    use ersatzrs_core::repositories::AddonMediaListArtworkRole;

    let root = item_nfo_root(entry);
    let mut published = metadata.clone();
    published.tags = union_values(&manifest.manifest.tags, &published.tags);
    if published.year.is_none() {
        published.year = manifest.manifest.defaults.year;
    }
    if published.content_ratings.is_empty() {
        if let Some(rating) = &manifest.manifest.defaults.content_rating {
            published.content_ratings.push(rating.clone());
        }
    }
    published.artwork.retain_mut(|candidate| {
        let key = match candidate.role {
            AddonMediaListArtworkRole::Poster => "poster",
            AddonMediaListArtworkRole::Fanart => "fanart",
            AddonMediaListArtworkRole::Thumb => "thumb",
            AddonMediaListArtworkRole::Other => return false,
        };
        let Some(materialized) = artwork_roles.get(key) else {
            return false;
        };
        candidate.url.clone_from(&materialized.file);
        true
    });
    published.artwork_urls.clear();
    ersatzrs_infra::addon_media_nfo::serialize(root, &published)
        .map_err(|error| PlaylistError::Enumeration(error.to_string()))
}

fn write_generated_pair(
    definition_path: &Path,
    definition_bytes: &[u8],
    nfo_path: &Path,
    nfo_bytes: &[u8],
) -> Result<()> {
    if std::fs::read(definition_path).is_ok_and(|current| current == definition_bytes)
        && std::fs::read(nfo_path).is_ok_and(|current| current == nfo_bytes)
    {
        return Ok(());
    }
    let definition_previous = std::fs::read(definition_path).ok();
    let definition_temporary = stage_bytes(definition_path, definition_bytes)?;
    let nfo_temporary = stage_bytes(nfo_path, nfo_bytes)?;

    definition_temporary
        .persist(definition_path)
        .map_err(|error| {
            PlaylistError::Enumeration(format!(
                "unable to publish managed definition atomically: {error}"
            ))
        })?;
    if let Err(error) = nfo_temporary.persist(nfo_path) {
        let rollback = restore_previous_file(definition_path, definition_previous.as_deref());
        return Err(PlaylistError::Enumeration(match rollback {
            Ok(()) => format!("unable to publish managed NFO atomically: {error}"),
            Err(rollback_error) => format!(
                "unable to publish managed NFO atomically: {error}; definition rollback failed: {rollback_error}"
            ),
        }));
    }
    Ok(())
}

fn stage_bytes(path: &Path, bytes: &[u8]) -> Result<tempfile::NamedTempFile> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to create managed temporary file: {error}"))
    })?;
    temporary.write_all(bytes).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to write managed temporary file: {error}"))
    })?;
    temporary.flush().map_err(|error| {
        PlaylistError::Enumeration(format!("unable to flush managed temporary file: {error}"))
    })?;
    Ok(temporary)
}

fn restore_previous_file(path: &Path, previous: Option<&[u8]>) -> Result<()> {
    if let Some(previous) = previous {
        write_bytes_atomically(path, previous)
    } else {
        std::fs::remove_file(path).map_err(|error| {
            PlaylistError::Enumeration(format!("unable to remove rolled-back definition: {error}"))
        })
    }
}

fn write_nfo_actor(writer: &mut Writer<Vec<u8>>, person: &RemoteStreamPerson) -> Result<()> {
    write_xml_event(writer, Event::Start(BytesStart::new("actor")))?;
    write_xml_text(writer, "name", Some(&person.name))?;
    write_xml_text(writer, "role", Some(&person.role))?;
    write_xml_event(writer, Event::End(BytesEnd::new("actor")))
}

fn write_unique_id(
    writer: &mut Writer<Vec<u8>>,
    source: &str,
    value: &str,
    is_default: bool,
) -> Result<()> {
    let mut element = BytesStart::new("uniqueid");
    element.push_attribute(("type", source));
    element.push_attribute(("default", if is_default { "true" } else { "false" }));
    write_xml_event(writer, Event::Start(element))?;
    write_xml_event(writer, Event::Text(BytesText::new(value)))?;
    write_xml_event(writer, Event::End(BytesEnd::new("uniqueid")))
}

fn write_xml_text(writer: &mut Writer<Vec<u8>>, name: &str, value: Option<&str>) -> Result<()> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    write_xml_event(writer, Event::Start(BytesStart::new(name)))?;
    write_xml_event(writer, Event::Text(BytesText::new(value)))?;
    write_xml_event(writer, Event::End(BytesEnd::new(name)))
}

fn write_xml_event(writer: &mut Writer<Vec<u8>>, event: Event<'_>) -> Result<()> {
    writer.write_event(event).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to encode generated NFO: {error}"))
    })
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

fn managed_nfo_file_name(definition_file: &str) -> String {
    Path::new(definition_file)
        .with_extension("nfo")
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("entry.nfo")
        .to_owned()
}

fn artwork_client() -> Result<Client> {
    ersatzrs_infra::addon_artwork::client().map_err(PlaylistError::Enumeration)
}

#[allow(clippy::too_many_arguments)]
async fn sync_managed_artwork_role(
    pool: &DbPool,
    manifest_path: &Path,
    provider_id: &str,
    role: ersatzrs_core::repositories::AddonMediaListArtworkRole,
    client: &Client,
    folder: &Path,
    definition_file: &str,
    source: Option<&str>,
    network_hosts: &[String],
    previous: Option<&ManagedPlaylistArtwork>,
) -> Result<Option<ManagedPlaylistArtwork>> {
    let Some(source) = source.map(str::trim).filter(|source| !source.is_empty()) else {
        remove_generated_artwork(folder, previous);
        clear_artwork_materialization(pool, manifest_path, provider_id, role).await?;
        return Ok(None);
    };
    let (canonical_source, bytes, extension) =
        if let Some(upload_id) = ersatzrs_infra::addon_artwork::upload_id(source) {
            let role_value = artwork_role_value(role);
            let row = sqlx::query_as::<_, (Vec<u8>, String)>(
                r#"SELECT "upload"."Bytes", "upload"."ContentType"
                   FROM "AddonMediaListArtworkUpload" AS "upload"
                   JOIN "AddonMediaListLibrary" AS "managed"
                     ON "managed"."AddonMediaListId" = "upload"."AddonMediaListId"
                   JOIN "AddonMediaListItem" AS "item"
                     ON "item"."Id" = "upload"."AddonMediaListItemId"
                   WHERE "upload"."Id" = ? AND "managed"."ManifestPath" = ?
                     AND "item"."ProviderId" = ? AND "upload"."Role" = ?"#,
            )
            .bind(upload_id)
            .bind(manifest_path.to_string_lossy().as_ref())
            .bind(provider_id)
            .bind(role_value)
            .fetch_optional(pool)
            .await
            .map_err(|error| {
                PlaylistError::Enumeration(format!("unable to load uploaded artwork: {error}"))
            })?
            .ok_or_else(|| {
                PlaylistError::Enumeration(
                    "selected artwork upload does not belong to this item and role".to_owned(),
                )
            })?;
            let (_, extension) = ersatzrs_infra::addon_artwork::validate_image(&row.1, &row.0)
                .map_err(|error| {
                    PlaylistError::Enumeration(format!("uploaded artwork is invalid: {error}"))
                })?;
            (source.to_owned(), row.0, extension)
        } else {
            let canonical = validate_operator_source_url(source, network_hosts)
                .map_err(|error| PlaylistError::Enumeration(error.to_string()))?;
            let (bytes, extension) = download_artwork(client, &canonical)
                .await
                .map_err(PlaylistError::Enumeration)?;
            (canonical, bytes, extension)
        };
    if previous.is_some_and(|artwork| {
        artwork.url == canonical_source
            && file_has_hash(&folder.join(&artwork.file), &artwork.sha256)
    }) {
        return Ok(previous.cloned());
    }
    let file = artwork_file_name(definition_file, role, extension).ok_or_else(|| {
        PlaylistError::Enumeration("unable to derive a safe artwork filename".to_owned())
    })?;
    if let Some(operator) =
        existing_operator_role_artwork(folder, definition_file, role, previous, &canonical_source)
    {
        if previous.is_some_and(|previous| previous.file != operator.file) {
            remove_generated_artwork(folder, previous);
        }
        record_artwork_materialization(
            pool,
            manifest_path,
            provider_id,
            role,
            &canonical_source,
            &operator,
        )
        .await?;
        return Ok(Some(operator));
    }
    let path = folder.join(&file);
    if path.parent() != Some(folder) {
        return Err(PlaylistError::Enumeration(
            "managed artwork path escaped its library folder".to_owned(),
        ));
    }
    write_bytes_atomically(&path, &bytes)?;
    let artwork = ManagedPlaylistArtwork {
        url: canonical_source.clone(),
        file,
        sha256: hex_hash(&bytes),
        operator_owned: false,
    };
    if previous.is_some_and(|previous| previous.file != artwork.file) {
        remove_generated_artwork(folder, previous);
    }
    record_artwork_materialization(
        pool,
        manifest_path,
        provider_id,
        role,
        &canonical_source,
        &artwork,
    )
    .await?;
    Ok(Some(artwork))
}

fn artwork_file_name(
    definition_file: &str,
    role: ersatzrs_core::repositories::AddonMediaListArtworkRole,
    extension: &str,
) -> Option<String> {
    use ersatzrs_core::repositories::AddonMediaListArtworkRole;

    let stem = Path::new(definition_file).file_stem()?.to_str()?;
    Some(match role {
        AddonMediaListArtworkRole::Thumb => format!("{stem}.{extension}"),
        AddonMediaListArtworkRole::Poster => format!("{stem}-poster.{extension}"),
        AddonMediaListArtworkRole::Fanart => format!("{stem}-fanart.{extension}"),
        AddonMediaListArtworkRole::Other => return None,
    })
}

fn existing_operator_role_artwork(
    folder: &Path,
    definition_file: &str,
    role: ersatzrs_core::repositories::AddonMediaListArtworkRole,
    previous: Option<&ManagedPlaylistArtwork>,
    source: &str,
) -> Option<ManagedPlaylistArtwork> {
    ["jpg", "jpeg", "png", "webp"]
        .into_iter()
        .find_map(|extension| {
            let file = artwork_file_name(definition_file, role, extension)?;
            let path = folder.join(&file);
            if !path.is_file()
                || previous.is_some_and(|artwork| {
                    !artwork.operator_owned
                        && artwork.file == file
                        && file_has_hash(&path, &artwork.sha256)
                })
            {
                return None;
            }
            let bytes = std::fs::read(path).ok()?;
            Some(ManagedPlaylistArtwork {
                url: source.to_owned(),
                file,
                sha256: hex_hash(&bytes),
                operator_owned: true,
            })
        })
}

const fn artwork_role_value(role: ersatzrs_core::repositories::AddonMediaListArtworkRole) -> i32 {
    use ersatzrs_core::repositories::AddonMediaListArtworkRole;
    match role {
        AddonMediaListArtworkRole::Poster => 1,
        AddonMediaListArtworkRole::Fanart => 2,
        AddonMediaListArtworkRole::Thumb => 3,
        AddonMediaListArtworkRole::Other => 0,
    }
}

async fn record_artwork_materialization(
    pool: &DbPool,
    manifest_path: &Path,
    provider_id: &str,
    role: ersatzrs_core::repositories::AddonMediaListArtworkRole,
    source: &str,
    artwork: &ManagedPlaylistArtwork,
) -> Result<()> {
    let owner = sqlx::query_as::<_, (i64, i64, i32)>(
        r#"SELECT "list"."Id", "item"."Id", "list"."PublishedRevision"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "AddonMediaList" AS "list" ON "list"."Id" = "managed"."AddonMediaListId"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."AddonMediaListId" = "list"."Id"
           WHERE "managed"."ManifestPath" = ? AND "item"."ProviderId" = ?"#,
    )
    .bind(manifest_path.to_string_lossy().as_ref())
    .bind(provider_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!("unable to resolve artwork owner: {error}"))
    })?;
    let Some((list_id, item_id, revision)) = owner else {
        return Ok(());
    };
    let role_value = artwork_role_value(role);
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaListArtworkMaterialization"
           SET "Source" = ?, "FileName" = ?, "Sha256" = ?, "PublishedRevision" = ?
           WHERE "AddonMediaListId" = ? AND "ScopeItemId" = ? AND "Role" = ?"#,
    )
    .bind(source)
    .bind(&artwork.file)
    .bind(&artwork.sha256)
    .bind(revision.saturating_add(1))
    .bind(list_id)
    .bind(item_id)
    .bind(role_value)
    .execute(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!("unable to update artwork publication: {error}"))
    })?;
    if updated.rows_affected() == 0 {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtworkMaterialization" (
                   "AddonMediaListId", "AddonMediaListItemId", "ScopeItemId", "Role",
                   "Source", "FileName", "Sha256", "PublishedRevision"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(item_id)
        .bind(item_id)
        .bind(role_value)
        .bind(source)
        .bind(&artwork.file)
        .bind(&artwork.sha256)
        .bind(revision.saturating_add(1))
        .execute(pool)
        .await
        .map_err(|error| {
            PlaylistError::Enumeration(format!("unable to record artwork publication: {error}"))
        })?;
    }
    Ok(())
}

async fn clear_artwork_materialization(
    pool: &DbPool,
    manifest_path: &Path,
    provider_id: &str,
    role: ersatzrs_core::repositories::AddonMediaListArtworkRole,
) -> Result<()> {
    sqlx::query(
        r#"DELETE FROM "AddonMediaListArtworkMaterialization"
           WHERE "Role" = ? AND "AddonMediaListItemId" IN (
               SELECT "item"."Id" FROM "AddonMediaListItem" AS "item"
               JOIN "AddonMediaListLibrary" AS "managed"
                 ON "managed"."AddonMediaListId" = "item"."AddonMediaListId"
               WHERE "managed"."ManifestPath" = ? AND "item"."ProviderId" = ?
           )"#,
    )
    .bind(artwork_role_value(role))
    .bind(manifest_path.to_string_lossy().as_ref())
    .bind(provider_id)
    .execute(pool)
    .await
    .map_err(|error| {
        PlaylistError::Enumeration(format!("unable to clear artwork publication: {error}"))
    })?;
    Ok(())
}

/// Materialize the first provider image beside one generated definition.
///
/// Remote Stream scanning deliberately recognizes the definition stem plus a
/// standard image extension (`entry.jpg`, `entry.png`, or `entry.webp`). This
/// is the Remote Stream equivalent of Kodi's thumbnail convention; it does not
/// claim separate poster/fanart semantics for a list item that only supplied a
/// single primary image.
async fn sync_managed_artwork(
    client: &Client,
    folder: &Path,
    definition_file: &str,
    source_url: Option<&str>,
    network_hosts: &[String],
    previous: Option<&ManagedPlaylistArtwork>,
) -> Option<ManagedPlaylistArtwork> {
    let Some(source_url) = source_url else {
        remove_generated_artwork(folder, previous);
        return None;
    };
    let source_url = match validate_operator_source_url(source_url, network_hosts) {
        Ok(url) => url,
        Err(error) => {
            tracing::warn!(
                definition_file,
                failure_kind = "invalid_url",
                operation_message = %error,
                "add-on artwork was skipped"
            );
            return previous.cloned();
        }
    };
    if previous.is_some_and(|artwork| {
        artwork.url == source_url && file_has_hash(&folder.join(&artwork.file), &artwork.sha256)
    }) {
        return previous.cloned();
    }

    // Any untracked file is an operator replacement under ADR 0068. Do not
    // replace or delete it merely because the provider has a newer URL.
    if let Some(path) = existing_operator_artwork(folder, definition_file, previous) {
        tracing::debug!(
            definition_file,
            artwork_file = %path.display(),
            "preserving operator-managed add-on artwork"
        );
        return None;
    }

    let (bytes, extension) = match download_artwork(client, &source_url).await {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(
                definition_file,
                failure_kind = "fetch",
                operation_message = %error,
                "add-on artwork was skipped"
            );
            return previous.cloned();
        }
    };
    let file = Path::new(definition_file)
        .with_extension(extension)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)?;
    let path = folder.join(&file);
    // `definition_file` is a host-generated basename, but keep the write guard
    // adjacent to the fetch so this untrusted URL can never influence a path.
    if path.parent() != Some(folder) {
        tracing::warn!(
            definition_file,
            "add-on artwork path escaped managed folder"
        );
        return previous.cloned();
    }
    if let Err(error) = write_bytes_atomically(&path, &bytes) {
        tracing::warn!(
            definition_file,
            failure_kind = "write",
            operation_message = %error,
            "add-on artwork was skipped"
        );
        return previous.cloned();
    }
    let artwork = ManagedPlaylistArtwork {
        url: source_url,
        file,
        sha256: hex_hash(&bytes),
        operator_owned: false,
    };
    if previous.is_some_and(|previous| previous.file != artwork.file) {
        remove_generated_artwork(folder, previous);
    }
    Some(artwork)
}

fn existing_operator_artwork(
    folder: &Path,
    definition_file: &str,
    previous: Option<&ManagedPlaylistArtwork>,
) -> Option<PathBuf> {
    let stem = Path::new(definition_file).file_stem()?.to_str()?;
    ["jpg", "jpeg", "png", "webp"]
        .into_iter()
        .map(|extension| folder.join(format!("{stem}.{extension}")))
        .find(|path| {
            path.is_file()
                && !previous.is_some_and(|artwork| {
                    path.file_name()
                        .is_some_and(|name| name == std::ffi::OsStr::new(&artwork.file))
                        && file_has_hash(path, &artwork.sha256)
                })
        })
}

fn remove_generated_artwork(folder: &Path, previous: Option<&ManagedPlaylistArtwork>) {
    let Some(previous) = previous else {
        return;
    };
    let path = folder.join(&previous.file);
    if !previous.operator_owned
        && path.parent() == Some(folder)
        && file_has_hash(&path, &previous.sha256)
    {
        if let Err(error) = std::fs::remove_file(&path) {
            tracing::warn!(
                artwork_file = %path.display(),
                operation_message = %error,
                "unable to remove stale generated add-on artwork"
            );
        }
    }
}

async fn download_artwork(
    client: &Client,
    url: &str,
) -> std::result::Result<(Vec<u8>, &'static str), String> {
    ersatzrs_infra::addon_artwork::download(client, url).await
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
    if std::fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    let temporary = stage_bytes(path, bytes)?;
    temporary.persist(path).map_err(|error| {
        PlaylistError::Enumeration(format!(
            "unable to publish managed state atomically: {error}"
        ))
    })?;
    Ok(())
}

fn manifest_etag(
    bytes: &[u8],
    addon: &InstalledAddon,
    content_kind_override: AddonMediaListContentKind,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.update([0]);
    hasher.update(addon.addon_id.as_bytes());
    hasher.update([0]);
    hasher.update(addon.version.to_string().as_bytes());
    hasher.update([0]);
    hasher.update(addon.package_sha256.as_bytes());
    hasher.update([0]);
    hasher.update([match content_kind_override {
        AddonMediaListContentKind::Auto => 0,
        AddonMediaListContentKind::TelevisionEpisode => 1,
        AddonMediaListContentKind::Movie => 2,
        AddonMediaListContentKind::MusicVideo => 3,
        AddonMediaListContentKind::OtherVideo => 4,
        AddonMediaListContentKind::Song => 5,
        AddonMediaListContentKind::Image => 6,
    }]);
    bytes_to_hex(&hasher.finalize())
}

fn playlist_etag(entries: &[PlaylistEntry]) -> Result<String> {
    let bytes = serde_json::to_vec(entries).map_err(|error| {
        PlaylistError::Enumeration(format!("unable to hash playlist entries: {error}"))
    })?;
    Ok(hex_hash(&bytes))
}

fn playlist_entry_etag(definition_sha256: &str, nfo_sha256: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ersatzrs.remote-stream-playlist.entry-output.v1");
    hasher.update([0]);
    hasher.update(definition_sha256.as_bytes());
    hasher.update([0]);
    hasher.update(nfo_sha256.as_bytes());
    bytes_to_hex(&hasher.finalize())
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
addon: org.ersatzrs.addon.fixture
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
    fn prepared_duration_keeps_milliseconds_and_definition_identity() {
        let manifest =
            parse_playlist_manifest(valid_yaml(), Path::new("Fixture.playlist.yml")).unwrap();
        let mut entry = PlaylistEntry::new("one", "https://example.test/one", "One");
        entry.duration_seconds = Some(120);
        let legacy = generated_definition_bytes(&manifest, &entry).unwrap();
        let prepared =
            generated_definition_bytes_with_duration(&manifest, &entry, Some(90_250)).unwrap();
        let read = |bytes: &[u8]| -> serde_yaml::Value { serde_yaml::from_slice(bytes).unwrap() };
        assert_eq!(read(&legacy)["duration"].as_str(), Some("00:02:00"));
        assert_eq!(read(&prepared)["duration"].as_str(), Some("00:01:30.250"));
        assert_eq!(read(&legacy)["url"], read(&prepared)["url"]);
        assert_eq!(
            membership_entry_from_outputs(&entry, &legacy, b"nfo").file,
            membership_entry_from_outputs(&entry, &prepared, b"nfo").file
        );
        assert_eq!(
            generated_definition_bytes_with_duration(&manifest, &entry, Some(90_250)).unwrap(),
            prepared
        );
        assert_eq!(
            generated_definition_bytes_with_duration(&manifest, &entry, None).unwrap(),
            legacy
        );
        assert!(generated_definition_bytes_with_duration(&manifest, &entry, Some(0)).is_err());
    }

    #[test]
    fn prepared_duration_without_provider_duration_survives_cached_inclusion() {
        let manifest =
            parse_playlist_manifest(valid_yaml(), Path::new("Fixture.playlist.yml")).unwrap();
        let entry = PlaylistEntry::new("one", "https://example.test/one", "One");
        assert!(entry.duration_seconds.is_none());
        for (milliseconds, expected_seconds) in [(250, 1), (90_250, 91)] {
            let duration = playback_inclusion_duration(entry.duration_seconds, Some(milliseconds));
            assert_eq!(duration, Some(expected_seconds));
            let definition =
                generated_definition_bytes_with_duration(&manifest, &entry, Some(milliseconds))
                    .unwrap();
            let value: serde_yaml::Value = serde_yaml::from_slice(&definition).unwrap();
            assert_eq!(
                value["duration"].as_str(),
                Some(if milliseconds == 250 {
                    "00:00:00.250"
                } else {
                    "00:01:30.250"
                })
            );
            let mut sidecar_entry = membership_entry("one", "one", "prepared");
            // The staging path assigns this same coarse value to both its
            // immediate ManagedPlaylistDefinition and its persisted sidecar.
            sidecar_entry.duration_seconds = duration;
            let state = membership_sidecar(vec![sidecar_entry]);
            let encoded = serde_json::to_vec(&state).unwrap();
            let restored = serde_json::from_slice(&encoded).unwrap();
            let cached = definitions_from_state(Path::new("Fixture"), &restored);
            assert_eq!(cached[0].duration_seconds, duration);
            assert_eq!(cached[0].path, Path::new("Fixture/one.yml"));
            assert!(cached[0]
                .duration_seconds
                .is_some_and(|seconds| seconds > 0));
        }
        assert_eq!(playback_inclusion_duration(Some(120), None), Some(120));
        assert_eq!(playback_inclusion_duration(None, None), None);
    }

    #[test]
    fn subtitle_staging_uses_frozen_intervals_and_does_not_recut_prepared_captions() {
        let source = "1\n00:00:01,000 --> 00:00:07,000\nSpanning cut\n\n";
        let entry = PlaylistEntry::new("one", "https://example.test/one", "One");
        let mut selected = PublicationArtifactSelection::default();
        selected.artifacts.streaming_plans.insert(
            "one".into(),
            ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan::new(
                10_000,
                0..10_000,
                std::slice::from_ref(&(3_000..5_000)),
                "a".repeat(64),
            )
            .unwrap(),
        );
        let mapped = subtitle_programme_content(source, false, "one", &selected, &entry).unwrap();
        assert_eq!(mapped, "1\n00:00:01,000 --> 00:00:03,000\nSpanning cut\n\n2\n00:00:03,000 --> 00:00:05,000\nSpanning cut\n\n");
        selected.artifacts.streaming_plans.clear();
        selected.artifacts.prepared_media.insert("one".into(), 1);
        assert_eq!(
            subtitle_programme_content(&mapped, true, "one", &selected, &entry).unwrap(),
            mapped
        );
        assert!(subtitle_programme_content(source, false, "one", &selected, &entry).is_err());
    }

    #[test]
    fn subtitle_staging_uses_published_fragment_bounds_instead_of_new_enumeration() {
        let mut entry = PlaylistEntry::new("one", "https://example.test/one", "One");
        entry.fragment_start_seconds = Some(20);
        entry.fragment_end_seconds = Some(30);
        let mut selected = PublicationArtifactSelection::default();
        selected
            .fragment_windows
            .insert("one".into(), (Some(2), Some(5)));
        let source = "1\n00:00:01,000 --> 00:00:04,000\nFragment\n\n";
        assert_eq!(
            subtitle_programme_content(source, false, "one", &selected, &entry).unwrap(),
            "1\n00:00:00,000 --> 00:00:02,000\nFragment\n\n"
        );
        assert!(subtitle_programme_content(
            "1\n00:00:10,000 --> 00:00:11,000\nOutside\n\n",
            false,
            "one",
            &selected,
            &entry
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn subtitle_sidecar_cache_requires_the_recorded_bytes_and_retains_legacy_shape() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("one.yml"), b"definition").unwrap();
        std::fs::write(root.path().join("one.nfo"), b"nfo").unwrap();
        let mut entry = membership_entry("one", "one", "same");
        entry.definition_sha256 = Some(hex_hash(b"definition"));
        assert!(serde_json::to_value(&entry)
            .unwrap()
            .get("subtitles")
            .is_none());
        entry.subtitles.push(ManagedPlaylistSubtitle {
            artifact_id: 10,
            actual_language: "en-US".into(),
            language: "en".into(),
            file: "one.en.srt".into(),
            sha256: hex_hash(b"caption"),
        });
        let state = membership_sidecar(vec![entry]);
        assert!(!managed_files_intact(root.path(), &state));
        std::fs::write(root.path().join("one.en.srt"), b"caption").unwrap();
        assert!(managed_files_intact(root.path(), &state));
        std::fs::write(root.path().join("one.en.srt"), b"changed").unwrap();
        assert!(!managed_files_intact(root.path(), &state));
        assert!(validated_subtitle_sidecar_path(root.path(), "../outside.srt").is_err());
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // One fixture follows publication, replay and owned cleanup.
    async fn subtitle_staging_verifies_artifact_and_replays_or_removes_owned_sidecar() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = ersatzrs_infra::db::init(&format!(
            "sqlite:{}",
            temporary.path().join("subtitles.db").display()
        ))
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "AddonInstallation" (
            "AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
            VALUES ('org.ersatzrs.addon.fixture','1.0.0',0,'fixture','fixture','{}',?,'2026-09-05 12:00:00','2026-09-05 12:00:00')"#)
            .bind(temporary.path().join("unused-package").to_string_lossy().as_ref())
            .execute(&pool).await.unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name")
            VALUES (1,'org.ersatzrs.addon.fixture','list','https://example.test/list','Fixture')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let artifact_root = temporary.path().join("processed");
        let folder = temporary.path().join("candidate");
        std::fs::create_dir_all(&artifact_root).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        let source = "1\n00:00:01,000 --> 00:00:04,000\nManual regional caption\n\n";
        std::fs::write(artifact_root.join("captions.srt"), source).unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" (
            "Id","AddonMediaListId","CacheKey","Kind","State","CreatedAt","UpdatedAt",
            "RelativePath","ByteLength","Sha256","CompletedAt","SubtitleLanguage","SubtitleTimeline")
            VALUES (10,1,'subtitle-fixture',0,2,'2026-09-05 12:00:00','2026-09-05 12:00:00',
            'captions.srt',?,?,'2026-09-05 12:00:00','en-US',0)"#)
            .bind(i64::try_from(source.len()).unwrap()).bind(hex_hash(source.as_bytes()))
            .execute(&pool).await.unwrap();
        let owner = PublishedProviderSelection {
            list_id: 1,
            provider_ids: BTreeSet::from(["one".into()]),
            is_publishing: true,
        };
        let mut selected = PublicationArtifactSelection::default();
        selected.artifacts.subtitles.insert("one".into(), vec![10]);
        selected
            .fragment_windows
            .insert("one".into(), (Some(2), Some(5)));
        let entry = PlaylistEntry::new("one", "https://example.test/one", "One");
        let definition = folder.join("one.yml");
        let first = materialize_selected_subtitles(
            &pool,
            Some(&owner),
            &selected,
            &entry,
            &definition,
            &[],
            &artifact_root,
        )
        .await
        .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].language, "eng");
        assert_eq!(first[0].actual_language, "en-US");
        assert_eq!(first[0].file, "one.eng.srt");
        let published = folder.join(&first[0].file);
        assert_eq!(
            std::fs::read_to_string(&published).unwrap(),
            "1\n00:00:00,000 --> 00:00:02,000\nManual regional caption\n\n"
        );
        assert_eq!(
            materialize_selected_subtitles(
                &pool,
                Some(&owner),
                &selected,
                &entry,
                &definition,
                &first,
                &artifact_root,
            )
            .await
            .unwrap(),
            first
        );
        std::fs::write(artifact_root.join("captions.srt"), "corrupted").unwrap();
        assert!(materialize_selected_subtitles(
            &pool,
            Some(&owner),
            &selected,
            &entry,
            &definition,
            &first,
            &artifact_root,
        )
        .await
        .is_err());
        selected.artifacts.subtitles.clear();
        assert!(materialize_selected_subtitles(
            &pool,
            Some(&owner),
            &selected,
            &entry,
            &definition,
            &first,
            &artifact_root,
        )
        .await
        .unwrap()
        .is_empty());
        assert!(!published.exists());
    }

    #[tokio::test]
    async fn staged_duration_uses_only_journal_selection_and_rejects_corruption() {
        let temporary = tempfile::tempdir().unwrap();
        let pool = ersatzrs_infra::db::init(&format!(
            "sqlite:{}",
            temporary.path().join("duration.db").display()
        ))
        .await
        .unwrap();
        // The list owns a real installation row even though this storage-only
        // regression never resolves or executes its synthetic package.
        sqlx::query(r#"INSERT INTO "AddonInstallation" (
            "AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
            VALUES ('org.ersatzrs.addon.fixture','1.0.0',0,'fixture','fixture','{}',?,'2026-09-05 12:00:00','2026-09-05 12:00:00')"#)
            .bind(temporary.path().join("unused-package").to_string_lossy().as_ref())
            .execute(&pool).await.unwrap();
        let journal = serde_json::json!({
            "previous_review_state": ersatzrs_domain::AddonMediaListReviewState::Ready,
            "previous_presentation_kind": ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            "previous_items": [], "previous_playlist": null,
            "prepared_media": {"one": 10}
        });
        sqlx::query(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","ReviewState","PublicationJournalJson")
            VALUES (1,'org.ersatzrs.addon.fixture','list','https://example.test/list','Fixture',3,?)"#)
            .bind(journal.to_string()).execute(&pool).await.unwrap();
        for (id, duration) in [(10, 90_250), (11, 42_000)] {
            sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","CacheKey","Kind","State","CreatedAt","UpdatedAt","DurationMilliseconds")
                VALUES (?,1,?,1,2,'2026-09-05 12:00:00','2026-09-05 12:00:00',?)"#)
                .bind(id).bind(format!("fixture-{id}")).bind(duration).execute(&pool).await.unwrap();
        }
        let selection = PublishedProviderSelection {
            list_id: 1,
            provider_ids: BTreeSet::from(["one".into()]),
            is_publishing: true,
        };
        let expected = BTreeMap::from([("one".into(), 90_250)]);
        assert_eq!(
            playback_duration_overrides(&pool, Some(&selection))
                .await
                .unwrap(),
            expected
        );
        assert_eq!(
            playback_duration_overrides(&pool, Some(&selection))
                .await
                .unwrap(),
            expected
        );
        let mut restored = journal;
        restored["prepared_media"]["one"] = serde_json::json!(11);
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"=? WHERE "Id"=1"#)
            .bind(restored.to_string())
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            playback_duration_overrides(&pool, Some(&selection))
                .await
                .unwrap(),
            BTreeMap::from([("one".into(), 42_000)])
        );
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"='invalid' WHERE "Id"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(playback_duration_overrides(&pool, Some(&selection))
            .await
            .is_err());
    }

    fn streaming_duration_plan(
    ) -> ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan {
        ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan::new(
            20_000,
            0..20_000,
            &[5_000..7_125, 18_500..20_000],
            "a".repeat(64),
        )
        .unwrap()
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // Keep frozen selection, replay and corruption proof together.
    async fn streaming_duration_follows_frozen_journal_and_retained_revision() {
        use ersatzrs_core::repositories::{
            AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
        };

        let temporary = tempfile::tempdir().unwrap();
        let pool = ersatzrs_infra::db::init(&format!(
            "sqlite:{}",
            temporary.path().join("streaming-duration.db").display()
        ))
        .await
        .unwrap();
        sqlx::raw_sql(
            r#"
            INSERT INTO "AddonInstallation" (
                "AddonId","Version","Enabled","PackageSha256","ContentSha256",
                "ManifestJson","InstallPath","InstalledAt","UpdatedAt")
            VALUES ('org.ersatzrs.addon.fixture','1.0.0',0,'fixture','fixture',
                '{}','unused-package','2026-09-06 08:00:00','2026-09-06 08:00:00');
            INSERT INTO "AddonMediaList" (
                "Id","AddonId","ProviderId","SourceUrl","Name","ReviewState",
                "DraftRevision","PublishedRevision")
            VALUES (1,'org.ersatzrs.addon.fixture','list','https://example.test/list',
                'Fixture',3,1,1);
            INSERT INTO "AddonMediaListItem" (
                "Id","AddonMediaListId","ProviderId","Rank","DisplayTitle","Title","Kind",
                "Published","EffectiveLiveness","DurationSeconds")
            VALUES (10,1,'one',0,'One','One',0,1,1,20);
        "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let plan = streaming_duration_plan();
        let mut journal = serde_json::json!({
            "previous_review_state": ersatzrs_domain::AddonMediaListReviewState::Ready,
            "previous_presentation_kind": ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            "previous_items": [], "previous_playlist": null,
            "streaming_plans": {"one": plan}
        });
        let mut selection = PublishedProviderSelection {
            list_id: 1,
            provider_ids: BTreeSet::from(["one".into()]),
            is_publishing: true,
        };
        let expected = BTreeMap::from([("one".into(), 16_375)]);
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"=? WHERE "Id"=1"#)
            .bind(journal.to_string())
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            playback_duration_overrides(&pool, Some(&selection))
                .await
                .unwrap(),
            expected
        );

        // A contradictory mode selection must not silently choose either duration.
        journal["prepared_media"] = serde_json::json!({"one": 99});
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"=? WHERE "Id"=1"#)
            .bind(journal.to_string())
            .execute(&pool)
            .await
            .unwrap();
        assert!(playback_duration_overrides(&pool, Some(&selection))
            .await
            .is_err());

        let repository = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
        repository
            .retain_published_revision(
                1,
                1,
                1,
                Utc::now(),
                &[],
                &AddonMediaListRevisionArtifacts {
                    streaming_plans: BTreeMap::from([("one".into(), plan)]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // A completed publication ignores both newer draft timing and the old journal.
        sqlx::raw_sql(
            r#"
            UPDATE "AddonMediaList" SET "ReviewState"=2 WHERE "Id"=1;
            UPDATE "AddonMediaListItem" SET "DurationSeconds"=99 WHERE "Id"=10;
        "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        selection.is_publishing = false;
        assert_eq!(
            playback_duration_overrides(&pool, Some(&selection))
                .await
                .unwrap(),
            expected
        );

        // A well-formed but tampered plan must not fall back to source duration.
        sqlx::query(
            r#"UPDATE "AddonMediaListRevisionItem"
            SET "PlaybackTimelineJson"=REPLACE("PlaybackTimelineJson", '7125', '7126')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(playback_duration_overrides(&pool, Some(&selection))
            .await
            .is_err());
    }

    fn membership_entry(
        provider_id: &str,
        generated_id: &str,
        entry_etag: &str,
    ) -> PlaylistSidecarEntry {
        PlaylistSidecarEntry {
            id: generated_id.to_owned(),
            provider_id: Some(provider_id.to_owned()),
            file: format!("{generated_id}.yml"),
            title: format!("Title for {generated_id}"),
            duration_seconds: Some(60),
            active: true,
            liveness: Some(RemoteStreamLiveness::Finite),
            nfo_file: Some(format!("{generated_id}.nfo")),
            availability: AddonMediaListItemAvailability::Available,
            definition_sha256: Some(format!("definition-{entry_etag}")),
            nfo_sha256: Some(format!("nfo-{entry_etag}")),
            entry_etag: Some(entry_etag.to_owned()),
            artwork: None,
            artwork_roles: BTreeMap::new(),
            subtitles: Vec::new(),
        }
    }

    fn membership_entry_from_outputs(
        entry: &PlaylistEntry,
        definition_bytes: &[u8],
        nfo_bytes: &[u8],
    ) -> PlaylistSidecarEntry {
        let file = managed_file_name(&entry.id);
        let definition_sha256 = hex_hash(definition_bytes);
        let nfo_sha256 = hex_hash(nfo_bytes);
        PlaylistSidecarEntry {
            id: entry.id.clone(),
            provider_id: Some(entry.provider_id.as_deref().unwrap_or(&entry.id).to_owned()),
            nfo_file: Some(managed_nfo_file_name(&file)),
            file,
            title: entry.title.clone(),
            duration_seconds: entry.duration_seconds,
            active: true,
            liveness: Some(
                entry
                    .effective_liveness()
                    .map(domain_liveness)
                    .expect("fixture liveness"),
            ),
            availability: domain_availability(entry.availability),
            entry_etag: Some(playlist_entry_etag(&definition_sha256, &nfo_sha256)),
            definition_sha256: Some(definition_sha256),
            nfo_sha256: Some(nfo_sha256),
            artwork: None,
            artwork_roles: BTreeMap::new(),
            subtitles: Vec::new(),
        }
    }

    fn membership_sidecar(entries: Vec<PlaylistSidecarEntry>) -> PlaylistSidecarState {
        PlaylistSidecarState {
            version: PLAYLIST_SIDECAR_VERSION,
            manifest_etag: "manifest".to_owned(),
            playlist_etag: "playlist".to_owned(),
            last_sync: Utc::now(),
            last_result: PlaylistSyncOutcome {
                kind: "ok".to_owned(),
                entries: entries.len(),
                truncated: false,
                error: None,
            },
            managed_files: BTreeMap::new(),
            managed_nfo_files: BTreeMap::new(),
            entries,
            overrides: PlaylistOverrides::default(),
        }
    }

    fn cached_nfo_sidecar(folder: &Path) -> PlaylistSidecarState {
        let manifest = parse_playlist_manifest(valid_yaml(), Path::new("Fixture.playlist.yml"))
            .expect("manifest");
        let mut entry = PlaylistEntry::new("one", "https://example.test/1", "One");
        entry.provider_id = Some("provider:one".to_owned());
        let definition = generated_definition_bytes(&manifest, &entry).expect("definition");
        let nfo = generated_nfo_bytes(&manifest, &entry).expect("NFO");
        let sidecar_entry = membership_entry_from_outputs(&entry, &definition, &nfo);
        std::fs::write(folder.join(&sidecar_entry.file), definition).expect("write definition");
        std::fs::write(folder.join("one.nfo"), nfo).expect("write NFO");
        let mut state = membership_sidecar(vec![sidecar_entry]);
        state
            .managed_files
            .insert("one".to_owned(), "one.yml".to_owned());
        state
            .managed_nfo_files
            .insert("one".to_owned(), "one.nfo".to_owned());
        state
    }

    #[test]
    fn fresh_cache_nfo_only_effective_metadata_edit_emits_scan_delta() {
        let temp = tempfile::tempdir().expect("managed folder");
        let previous = cached_nfo_sidecar(temp.path());
        let nfo_path = temp.path().join("one.nfo");
        let original_nfo = std::fs::read_to_string(&nfo_path).expect("generated NFO");
        let edited_nfo =
            original_nfo.replacen("<title>One</title>", "<title>Operator One</title>", 1);
        assert_ne!(edited_nfo, original_nfo);
        std::fs::write(&nfo_path, &edited_nfo).expect("operator-edited NFO");
        let sidecar_path = temp.path().join(".ersatzrs-playlist.json");

        let sync = fresh_cached_playlist_sync(temp.path(), &sidecar_path, &previous)
            .expect("fresh cache reconciliation");

        assert!(!sync.enumerated);
        assert!(sync.changed);
        assert!(sync.membership_changed);
        assert_eq!(sync.membership_delta.changed_provider_ids, ["provider:one"]);
        assert!(sync.membership_delta.added_provider_ids.is_empty());
        assert!(sync.membership_delta.removed_provider_ids.is_empty());
        assert_eq!(
            sync.membership_delta.reason(),
            PlaylistMembershipChangeReason::ChangedOrReplaced
        );
        let current_entry = &sync.state.entries[0];
        let nfo_sha256 = hex_hash(edited_nfo.as_bytes());
        let definition_sha256 = previous.entries[0]
            .definition_sha256
            .as_deref()
            .expect("definition hash");
        assert_eq!(
            current_entry.nfo_sha256.as_deref(),
            Some(nfo_sha256.as_str())
        );
        assert_eq!(
            current_entry.entry_etag,
            Some(playlist_entry_etag(definition_sha256, &nfo_sha256))
        );
        assert_eq!(
            crate::nfo::parse_other_video_nfo(&nfo_path)
                .expect("effective NFO")
                .title
                .as_deref(),
            Some("Operator One")
        );
        assert_eq!(
            sync.sidecar_publication,
            SidecarPublication::Staged(sidecar_path.clone())
        );
        sync.publish_sidecar().expect("publish reconciled identity");
        assert_eq!(
            load_sidecar(&sidecar_path)
                .expect("load sidecar")
                .expect("published sidecar"),
            sync.state
        );
    }

    #[test]
    fn fresh_cache_unchanged_and_legacy_equivalent_nfo_are_noops() {
        let temp = tempfile::tempdir().expect("managed folder");
        let current = cached_nfo_sidecar(temp.path());
        let sidecar_path = temp.path().join(".ersatzrs-playlist.json");

        let unchanged = fresh_cached_playlist_sync(temp.path(), &sidecar_path, &current)
            .expect("unchanged fresh cache");

        assert!(!unchanged.changed);
        assert!(!unchanged.membership_changed);
        assert_eq!(
            unchanged.membership_delta,
            PlaylistMembershipDelta::default()
        );
        assert_eq!(unchanged.state, current);
        assert_eq!(unchanged.sidecar_publication, SidecarPublication::NotNeeded);

        let mut legacy = current;
        legacy.version = 2;
        legacy.entries[0].entry_etag = None;
        let equivalent = fresh_cached_playlist_sync(temp.path(), &sidecar_path, &legacy)
            .expect("equivalent legacy cache");

        assert!(!equivalent.changed);
        assert!(!equivalent.membership_changed);
        assert_eq!(
            equivalent.membership_delta,
            PlaylistMembershipDelta::default()
        );
        assert_eq!(
            equivalent.state.entries[0].nfo_sha256,
            legacy.entries[0].nfo_sha256
        );
        assert!(equivalent.state.entries[0].entry_etag.is_some());
        assert_eq!(
            equivalent.sidecar_publication,
            SidecarPublication::Staged(sidecar_path)
        );
    }

    #[test]
    fn fresh_cache_legacy_nfo_without_output_hash_is_conservatively_changed() {
        let temp = tempfile::tempdir().expect("managed folder");
        let mut legacy = cached_nfo_sidecar(temp.path());
        legacy.version = 2;
        legacy.entries[0].entry_etag = None;
        legacy.entries[0].nfo_sha256 = None;
        let sidecar_path = temp.path().join(".ersatzrs-playlist.json");

        let sync = fresh_cached_playlist_sync(temp.path(), &sidecar_path, &legacy)
            .expect("legacy fresh cache");

        assert!(sync.changed);
        assert!(sync.membership_changed);
        assert_eq!(sync.membership_delta.changed_provider_ids, ["provider:one"]);
        assert!(sync.state.entries[0].nfo_sha256.is_some());
        assert!(sync.state.entries[0].entry_etag.is_some());
    }

    #[test]
    fn membership_delta_is_empty_for_unchanged_sidecars() {
        let previous = membership_sidecar(vec![
            membership_entry("provider:one", "one", "one-v1"),
            membership_entry("provider:two", "two", "two-v1"),
        ]);
        let mut current = previous.clone();
        current.last_sync += chrono::Duration::minutes(5);

        let delta = PlaylistMembershipDelta::between(Some(&previous), &current);

        assert_eq!(delta, PlaylistMembershipDelta::default());
        assert_eq!(delta.reason(), PlaylistMembershipChangeReason::Unchanged);
    }

    #[test]
    fn membership_delta_detects_manifest_default_and_tag_output_changes() {
        let manifest_path = Path::new("Fixture.playlist.yml");
        let previous_manifest =
            parse_playlist_manifest(valid_yaml(), manifest_path).expect("previous manifest");
        let current_yaml = format!(
            "{}\ndefaults:\n  content_rating: TV-14\n  year: 2024\ntags:\n  - Featured\n",
            valid_yaml()
        );
        let current_manifest =
            parse_playlist_manifest(&current_yaml, manifest_path).expect("current manifest");
        let entry = PlaylistEntry::new("one", "https://example.test/1", "One");
        let previous_definition =
            generated_definition_bytes(&previous_manifest, &entry).expect("previous definition");
        let current_definition =
            generated_definition_bytes(&current_manifest, &entry).expect("current definition");
        let temp = tempfile::tempdir().expect("NFO directory");
        let nfo_path = temp.path().join("one.nfo");
        let previous_generated_nfo =
            generated_nfo_bytes(&previous_manifest, &entry).expect("previous generated NFO");
        let previous_nfo = ersatzrs_infra::addon_media_nfo::reconcile_generated(
            &nfo_path,
            &previous_generated_nfo,
        )
        .expect("previous effective NFO");
        let previous = membership_sidecar(vec![membership_entry_from_outputs(
            &entry,
            &previous_definition,
            &previous_nfo,
        )]);
        let current_generated_nfo =
            generated_nfo_bytes(&current_manifest, &entry).expect("current generated NFO");
        let current_nfo =
            ersatzrs_infra::addon_media_nfo::reconcile_generated(&nfo_path, &current_generated_nfo)
                .expect("current effective NFO");
        let mut current = membership_sidecar(vec![membership_entry_from_outputs(
            &entry,
            &current_definition,
            &current_nfo,
        )]);
        current.manifest_etag = "metadata-defaults-and-tags".to_owned();

        assert_eq!(previous_definition, current_definition);
        assert_ne!(previous_nfo, current_nfo);
        let delta = PlaylistMembershipDelta::between(Some(&previous), &current);
        let membership_changed = !delta.is_empty();

        assert_eq!(delta.changed_provider_ids, ["one"]);
        assert!(membership_changed);
    }

    #[test]
    fn membership_delta_detects_reviewed_nfo_output_changes() {
        let manifest = parse_playlist_manifest(valid_yaml(), Path::new("Fixture.playlist.yml"))
            .expect("manifest");
        let entry = PlaylistEntry::new("one", "https://example.test/1", "One");
        let definition = generated_definition_bytes(&manifest, &entry).expect("definition");
        let mut previous_metadata = ersatzrs_core::repositories::AddonMediaListNfoMetadata {
            title: "One".to_owned(),
            languages: vec!["en".to_owned()],
            ..Default::default()
        };
        let temp = tempfile::tempdir().expect("NFO directory");
        let nfo_path = temp.path().join("one.nfo");
        let previous_generated_nfo =
            generated_reviewed_nfo_bytes(&manifest, &entry, &previous_metadata, &BTreeMap::new())
                .expect("previous generated NFO");
        let previous_nfo = ersatzrs_infra::addon_media_nfo::reconcile_generated(
            &nfo_path,
            &previous_generated_nfo,
        )
        .expect("previous effective NFO");
        let previous = membership_sidecar(vec![membership_entry_from_outputs(
            &entry,
            &definition,
            &previous_nfo,
        )]);
        previous_metadata.languages = vec!["nl".to_owned()];
        let current_generated_nfo =
            generated_reviewed_nfo_bytes(&manifest, &entry, &previous_metadata, &BTreeMap::new())
                .expect("current generated NFO");
        let current_nfo =
            ersatzrs_infra::addon_media_nfo::reconcile_generated(&nfo_path, &current_generated_nfo)
                .expect("current effective NFO");
        let current = membership_sidecar(vec![membership_entry_from_outputs(
            &entry,
            &definition,
            &current_nfo,
        )]);

        assert_ne!(previous_nfo, current_nfo);
        let delta = PlaylistMembershipDelta::between(Some(&previous), &current);

        assert_eq!(delta.changed_provider_ids, ["one"]);
        assert!(!delta.is_empty());
    }

    #[test]
    fn membership_delta_ignores_output_equivalent_manifest_and_sidecar_upgrade() {
        let manifest_path = Path::new("Fixture.playlist.yml");
        let previous_manifest =
            parse_playlist_manifest(valid_yaml(), manifest_path).expect("previous manifest");
        let current_yaml = format!(
            "{}\ndefaults:\n  content_rating: TV-14\n  year: 2024\ntags:\n  - Featured\n",
            valid_yaml()
        );
        let current_manifest =
            parse_playlist_manifest(&current_yaml, manifest_path).expect("current manifest");
        let mut entry = PlaylistEntry::new("one", "https://example.test/1", "One");
        entry.content_rating = Some("TV-14".to_owned());
        entry.year = Some(2024);
        entry.tags = vec!["Featured".to_owned()];
        let previous_definition =
            generated_definition_bytes(&previous_manifest, &entry).expect("previous definition");
        let current_definition =
            generated_definition_bytes(&current_manifest, &entry).expect("current definition");
        let temp = tempfile::tempdir().expect("NFO directory");
        let nfo_path = temp.path().join("one.nfo");
        let previous_generated_nfo =
            generated_nfo_bytes(&previous_manifest, &entry).expect("previous generated NFO");
        let previous_nfo = ersatzrs_infra::addon_media_nfo::reconcile_generated(
            &nfo_path,
            &previous_generated_nfo,
        )
        .expect("previous effective NFO");
        let mut previous_entry =
            membership_entry_from_outputs(&entry, &previous_definition, &previous_nfo);
        previous_entry.provider_id = None;
        previous_entry.entry_etag = None;
        let mut previous = membership_sidecar(vec![previous_entry]);
        previous.version = 2;
        previous.manifest_etag = "legacy-manifest".to_owned();
        let current_generated_nfo =
            generated_nfo_bytes(&current_manifest, &entry).expect("current generated NFO");
        let current_nfo =
            ersatzrs_infra::addon_media_nfo::reconcile_generated(&nfo_path, &current_generated_nfo)
                .expect("current effective NFO");
        let mut current = membership_sidecar(vec![membership_entry_from_outputs(
            &entry,
            &current_definition,
            &current_nfo,
        )]);
        current.manifest_etag = "upgraded-manifest".to_owned();

        assert_eq!(previous_definition, current_definition);
        assert_eq!(previous_nfo, current_nfo);
        let delta = PlaylistMembershipDelta::between(Some(&previous), &current);
        let membership_changed = !delta.is_empty();

        assert_eq!(delta, PlaylistMembershipDelta::default());
        assert!(!membership_changed);
    }

    #[test]
    fn membership_delta_distinguishes_additions_and_removals() {
        let one = membership_sidecar(vec![membership_entry("provider:one", "one", "one-v1")]);
        let two = membership_sidecar(vec![
            membership_entry("provider:one", "one", "one-v1"),
            membership_entry("provider:two", "two", "two-v1"),
        ]);

        let added = PlaylistMembershipDelta::between(Some(&one), &two);
        assert_eq!(added.added_provider_ids, ["provider:two"]);
        assert_eq!(added.reason(), PlaylistMembershipChangeReason::Added);

        let removed = PlaylistMembershipDelta::between(Some(&two), &one);
        assert_eq!(removed.removed_provider_ids, ["provider:two"]);
        assert_eq!(removed.reason(), PlaylistMembershipChangeReason::Removed);
    }

    #[test]
    fn membership_delta_classifies_stable_identity_replacement_as_changed() {
        let previous = membership_sidecar(vec![membership_entry(
            "provider:episode",
            "old-generated-id",
            "episode-v1",
        )]);
        let current = membership_sidecar(vec![membership_entry(
            "provider:episode",
            "replacement-generated-id",
            "episode-v2",
        )]);

        let delta = PlaylistMembershipDelta::between(Some(&previous), &current);

        assert_eq!(delta.changed_provider_ids, ["provider:episode"]);
        assert!(delta.added_provider_ids.is_empty());
        assert!(delta.removed_provider_ids.is_empty());
        assert_eq!(
            delta.reason(),
            PlaylistMembershipChangeReason::ChangedOrReplaced
        );
    }

    #[test]
    fn membership_delta_detects_relative_reordering() {
        let previous = membership_sidecar(vec![
            membership_entry("provider:one", "one", "one-v1"),
            membership_entry("provider:two", "two", "two-v1"),
            membership_entry("provider:three", "three", "three-v1"),
        ]);
        let current = membership_sidecar(vec![
            membership_entry("provider:three", "three", "three-v1"),
            membership_entry("provider:one", "one", "one-v1"),
            membership_entry("provider:two", "two", "two-v1"),
        ]);

        let delta = PlaylistMembershipDelta::between(Some(&previous), &current);

        assert!(delta.reordered);
        assert!(delta.added_provider_ids.is_empty());
        assert!(delta.removed_provider_ids.is_empty());
        assert!(delta.changed_provider_ids.is_empty());
        assert_eq!(delta.reason(), PlaylistMembershipChangeReason::Reordered);
    }

    #[test]
    fn membership_delta_is_deterministic_redacted_and_mixed() {
        let previous = membership_sidecar(vec![
            membership_entry("provider:alpha", "alpha", "alpha-v1"),
            membership_entry("provider:beta", "beta", "beta-v1"),
            membership_entry("provider:gamma", "gamma", "gamma-v1"),
        ]);
        let current = membership_sidecar(vec![
            membership_entry("provider:gamma", "gamma", "gamma-v2"),
            membership_entry("provider:alpha", "alpha", "alpha-v1"),
            membership_entry("provider:delta", "delta", "delta-v1"),
        ]);

        let delta = PlaylistMembershipDelta::between(Some(&previous), &current);

        assert_eq!(delta.added_provider_ids, ["provider:delta"]);
        assert_eq!(delta.removed_provider_ids, ["provider:beta"]);
        assert_eq!(delta.changed_provider_ids, ["provider:gamma"]);
        assert!(delta.reordered);
        assert_eq!(delta.reason(), PlaylistMembershipChangeReason::Mixed);
        let diagnostic = delta.to_string();
        let debug = format!("{delta:?}");
        for provider_id in ["provider:alpha", "provider:beta", "provider:gamma"] {
            assert!(!diagnostic.contains(provider_id));
            assert!(!debug.contains(provider_id));
        }
    }

    #[tokio::test]
    async fn managed_artwork_is_bounded_idempotent_and_preserves_operator_files() {
        use httpmock::{Method::GET, MockServer};

        let server = MockServer::start_async().await;
        let image = server
            .mock_async(|when, then| {
                when.method(GET).path("/image");
                then.status(200)
                    .header("content-type", "image/png")
                    .body(b"\x89PNG\r\n\x1a\nfixture png bytes");
            })
            .await;
        let directory = tempfile::tempdir().expect("managed artwork directory");
        let client = artwork_client().expect("artwork client");
        let url = format!("{}/image", server.base_url());

        let generated = sync_managed_artwork(
            &client,
            directory.path(),
            "episode.yml",
            Some(&url),
            &[],
            None,
        )
        .await
        .expect("downloaded artwork");
        assert_eq!(generated.file, "episode.png");
        assert!(directory.path().join(&generated.file).is_file());

        let refreshed = sync_managed_artwork(
            &client,
            directory.path(),
            "episode.yml",
            Some(&url),
            &[],
            Some(&generated),
        )
        .await
        .expect("existing generated artwork");
        assert_eq!(refreshed, generated);
        image.assert_calls_async(1).await;

        let operator_path = directory.path().join("operator.jpg");
        std::fs::write(&operator_path, "operator replacement").expect("operator artwork");
        assert!(sync_managed_artwork(
            &client,
            directory.path(),
            "operator.yml",
            Some(&url),
            &[],
            None,
        )
        .await
        .is_none());
        assert_eq!(
            std::fs::read_to_string(operator_path).expect("preserved operator artwork"),
            "operator replacement"
        );
        image.assert_calls_async(1).await;
    }

    #[tokio::test]
    async fn managed_artwork_rejects_unbounded_and_non_image_responses() {
        use httpmock::{Method::GET, MockServer};

        let server = MockServer::start_async().await;
        let non_image = server
            .mock_async(|when, then| {
                when.method(GET).path("/text");
                then.status(200)
                    .header("content-type", "text/plain")
                    .body("not artwork");
            })
            .await;
        let oversized = server
            .mock_async(|when, then| {
                when.method(GET).path("/large");
                then.status(200)
                    .header("content-type", "image/jpeg")
                    .header("content-length", (MAX_ARTWORK_BYTES + 1).to_string())
                    .body("small body");
            })
            .await;
        let directory = tempfile::tempdir().expect("managed artwork directory");
        let client = artwork_client().expect("artwork client");

        for path in ["text", "large"] {
            let url = format!("{}/{}", server.base_url(), path);
            assert!(sync_managed_artwork(
                &client,
                directory.path(),
                "episode.yml",
                Some(&url),
                &[],
                None,
            )
            .await
            .is_none());
        }
        assert!(!directory.path().join("episode.jpg").exists());
        assert!(!directory.path().join("episode.png").exists());
        non_image.assert_calls_async(1).await;
        oversized.assert_calls_async(1).await;
    }

    #[test]
    fn reviewed_item_artwork_uses_role_specific_kodi_files_and_detects_operator_edits() {
        use ersatzrs_core::repositories::AddonMediaListArtworkRole;

        assert_eq!(
            artwork_file_name("episode.yml", AddonMediaListArtworkRole::Poster, "jpg").as_deref(),
            Some("episode-poster.jpg")
        );
        assert_eq!(
            artwork_file_name("episode.yml", AddonMediaListArtworkRole::Fanart, "webp").as_deref(),
            Some("episode-fanart.webp")
        );
        assert_eq!(
            artwork_file_name("episode.yml", AddonMediaListArtworkRole::Thumb, "png").as_deref(),
            Some("episode.png")
        );

        let directory = tempfile::tempdir().expect("role artwork directory");
        let path = directory.path().join("episode-poster.jpg");
        std::fs::write(&path, b"operator poster").expect("operator poster");
        let previous = ManagedPlaylistArtwork {
            url: "https://example.test/old.jpg".to_owned(),
            file: "episode-poster.jpg".to_owned(),
            sha256: hex_hash(b"old generated poster"),
            operator_owned: false,
        };
        let detected = existing_operator_role_artwork(
            directory.path(),
            "episode.yml",
            AddonMediaListArtworkRole::Poster,
            Some(&previous),
            "https://example.test/new.jpg",
        )
        .expect("operator replacement");
        assert!(detected.operator_owned);
        assert_eq!(detected.file, "episode-poster.jpg");
        assert_eq!(detected.sha256, hex_hash(b"operator poster"));
    }

    #[cfg(unix)]
    async fn fixture_addon_pool(root: &Path) -> DbPool {
        let database_path = root
            .parent()
            .expect("fixture package has a parent")
            .join("addons.db");
        let database_url = format!("sqlite:{}", database_path.display());
        let pool = ersatzrs_infra::db::init(&database_url)
            .await
            .expect("fixture database");
        let manifest = serde_json::json!({
            "manifest_version": 1,
            "id": "org.ersatzrs.addon.fixture",
            "version": "1.0.0",
            "name": {"en-US": "Fixture"},
            "summary": {"en-US": "Fixture playlist add-on"},
            "license": "GPL-3.0-or-later",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": [
                {"id": "addon.check.v1"},
                {"id": "remote-stream.list.v1"},
                {"id": "remote-stream.play.v1"}
            ],
            "entrypoints": {
                "any": {"path": "fixture.sh", "kind": "posix_shell"}
            },
            "permissions": {},
            "settings": [],
            "panels": []
        });
        let now = Utc::now().to_rfc3339();
        let content_sha256 = ersatzrs_infra::addons::calculate_addon_content_sha256(root)
            .await
            .expect("hash fixture package");
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "RepositoryId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, NULL, '1.0.0', 1, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("org.ersatzrs.addon.fixture")
        .bind("a".repeat(64))
        .bind(content_sha256)
        .bind(manifest.to_string())
        .bind(root.to_string_lossy().into_owned())
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("fixture add-on installation");
        pool
    }

    #[cfg(unix)]
    async fn update_fixture_content_hash(pool: &DbPool, root: &Path) {
        let content_sha256 = ersatzrs_infra::addons::calculate_addon_content_sha256(root)
            .await
            .expect("hash updated fixture package");
        sqlx::query(
            r#"UPDATE "AddonInstallation"
               SET "PackageSha256" = ?, "ContentSha256" = ?
               WHERE "AddonId" = ?"#,
        )
        .bind(&content_sha256)
        .bind(content_sha256)
        .bind("org.ersatzrs.addon.fixture")
        .execute(pool)
        .await
        .expect("record updated fixture content");
    }

    #[cfg(unix)]
    fn write_fixture_playlist_script(script: &Path, first_url: &str, include_second: bool) {
        use std::os::unix::fs::PermissionsExt as _;

        let mut lines = vec![format!(
            r#"'{{"id":"one","url":"{first_url}","title":"One","duration_seconds":60}}'"#
        )];
        if include_second {
            lines.push(
                r#"'{"id":"two","url":"https://example.test/2","title":"Two","duration_seconds":120}'"#
                    .to_owned(),
            );
        }
        std::fs::write(
            script,
            format!("#!/bin/sh\nprintf '%s\\n' {}\n", lines.join(" ")),
        )
        .expect("write fixture script");
        let mut permissions = std::fs::metadata(script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(script, permissions).expect("fixture permissions");
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
                    is_live: Some(false),
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
                    is_live: Some(false),
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
                    is_live: Some(false),
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
            "addon: org.ersatzrs.addon.fixture",
            "addon: org.ersatzrs.addon.fixture\nfolder: ../escape",
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
    fn manifest_requires_a_stable_addon_identifier() {
        let invalid = valid_yaml().replace("org.ersatzrs.addon.fixture", "fixture command");
        assert!(
            parse_playlist_manifest(&invalid, Path::new("x.playlist.yml"))
                .expect_err("invalid add-on")
                .to_string()
                .contains("reverse-DNS")
        );
    }

    #[test]
    fn manifest_rejects_unsafe_operator_source_urls() {
        for url in [
            "file:///tmp/list",
            "javascript:alert(1)",
            "example.test/list",
            "https://user:secret@example.test/list",
        ] {
            let yaml = valid_yaml().replace("https://example.test/playlist", url);
            let error = parse_playlist_manifest(&yaml, Path::new("x.playlist.yml"))
                .expect_err("unsafe source URL")
                .to_string();
            assert!(error.contains("unsupported-url"), "{url}: {error}");
            assert!(!error.contains("user:secret"));
        }
    }

    #[test]
    fn manifest_stores_the_validated_canonical_source_url() {
        let yaml = valid_yaml().replace(
            "https://example.test/playlist",
            r"https://allowed.example\\@evil.example/playlist",
        );
        let manifest = parse_playlist_manifest(&yaml, Path::new("x.playlist.yml"))
            .expect("canonical source URL manifest");

        assert_eq!(
            manifest.manifest.url,
            "https://allowed.example/@evil.example/playlist"
        );
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

    #[test]
    fn ndjson_availability_accepts_only_canonical_or_omitted_values() {
        let canonical = concat!(
            r#"{"id":"available","url":"https://example.test/1","title":"Available","availability":"available"}"#,
            "\n",
            r#"{"id":"unavailable","url":"https://example.test/2","title":"Unavailable","availability":"unavailable"}"#,
            "\n",
            r#"{"id":"unknown","url":"https://example.test/3","title":"Unknown","availability":"unknown"}"#,
            "\n",
            r#"{"id":"legacy","url":"https://example.test/4","title":"Legacy"}"#
        );
        let entries = parse_ndjson_entries(canonical).expect("canonical availability");
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.availability)
                .collect::<Vec<_>>(),
            [
                WireAvailability::Available,
                WireAvailability::Unavailable,
                WireAvailability::Unknown,
                WireAvailability::Available,
            ]
        );

        let invalid = r#"{"id":"invalid","url":"https://example.test/5","title":"Invalid","availability":"maybe_later"}"#;
        let error = parse_ndjson_entries(invalid).expect_err("unknown availability must fail");
        let message = error.to_string();
        assert!(message.contains("is not a valid record"));
        assert!(!message.contains("maybe_later"));
        assert!(message.len() < 512);
    }

    #[test]
    fn fragment_parent_validation_is_order_independent_and_rejects_omissions() {
        let fragment = PlaylistEntry {
            id: "episode-chapter-0".to_owned(),
            provider_id: Some("episode:1:chapter:0".to_owned()),
            parent_provider_id: Some("episode:1".to_owned()),
            url: "https://example.test/episode?start=0&end=30".to_owned(),
            title: "Episode - Intro".to_owned(),
            ..PlaylistEntry::default()
        };
        let parent = PlaylistEntry {
            id: "episode".to_owned(),
            provider_id: Some("episode:1".to_owned()),
            url: "https://example.test/episode".to_owned(),
            title: "Episode".to_owned(),
            ..PlaylistEntry::default()
        };

        validate_fragment_parents(&[fragment.clone(), parent.clone()])
            .expect("fragment-before-parent is valid");
        let manifest = parse_playlist_manifest(
            &valid_yaml().replace("exclude: [skip]", "exclude: [episode]"),
            Path::new("x.playlist.yml"),
        )
        .expect("filtered manifest");
        let selected = select_entries(&manifest, vec![fragment, parent]);
        let error = validate_fragment_parents(&selected)
            .expect_err("a filter that omits the full episode must reject its fragment");
        assert!(error.to_string().contains("is not selected"));
    }

    #[test]
    fn playlist_chapters_require_remote_stream_v3() {
        let error = require_chapter_capability(
            "org.example.video",
            CAPABILITY_REMOTE_STREAM_LIST_V2,
            Some("0:00 Intro"),
        )
        .expect_err("v2 must not derive chapter fragments");
        assert!(error.to_string().contains("remote-stream.list.v3"));

        require_chapter_capability(
            "org.example.video",
            CAPABILITY_REMOTE_STREAM_LIST_V3,
            Some("0:00 Intro"),
        )
        .expect("v3 supports chapter fragments");
    }

    #[test]
    fn liveness_requires_remote_stream_v4() {
        let mut entry = PlaylistEntry::new("episode", "https://example.test/episode", "Episode");
        entry.liveness = Some(WireLiveness::Live);

        let error =
            validate_list_capability_fields(CAPABILITY_REMOTE_STREAM_LIST_V3, &[entry.clone()])
                .expect_err("v3 is frozen without liveness");
        assert!(error.to_string().contains("remote-stream.list.v4"));
        validate_list_capability_fields(CAPABILITY_REMOTE_STREAM_LIST_V4, &[entry])
            .expect("v4 accepts liveness");
    }

    #[test]
    fn manifest_liveness_adapts_explicit_legacy_values() {
        let finite = parse_playlist_manifest(
            &format!("{}\ndefaults:\n  is_live: false\n", valid_yaml()),
            Path::new("finite.playlist.yml"),
        )
        .expect("explicit legacy false remains finite");
        assert_eq!(
            default_liveness(&finite.manifest.defaults),
            Some(RemoteStreamLiveness::Finite)
        );

        let conflict = parse_playlist_manifest(
            &format!(
                "{}\ndefaults:\n  is_live: false\n  liveness: live\n",
                valid_yaml()
            ),
            Path::new("conflict.playlist.yml"),
        )
        .expect_err("contradictory values must be refused");
        assert!(conflict.to_string().contains("contradicts"));
    }

    #[test]
    fn generated_definitions_include_authoritative_and_legacy_liveness() {
        let manifest = parse_playlist_manifest(valid_yaml(), Path::new("Fixture.playlist.yml"))
            .expect("manifest");
        for (wire, expected, expected_legacy) in [
            (WireLiveness::Unknown, RemoteStreamLiveness::Unknown, false),
            (WireLiveness::Finite, RemoteStreamLiveness::Finite, false),
            (WireLiveness::Live, RemoteStreamLiveness::Live, true),
        ] {
            let mut entry = PlaylistEntry::new("one", "https://example.test/1", "One");
            entry.liveness = Some(wire);
            let bytes = generated_definition_bytes(&manifest, &entry).expect("definition");
            let yaml = std::str::from_utf8(&bytes).expect("UTF-8 definition");
            let parsed = ersatzrs_infra::remote_stream::parse_remote_stream_definition(yaml)
                .expect("generated definition must parse through production contract");

            assert_eq!(parsed.liveness, Some(expected));
            assert_eq!(parsed.is_live, expected_legacy);
        }
    }

    #[test]
    fn operator_chapters_override_and_can_restore_provider_text() {
        assert_eq!(
            effective_chapter_input(Some("0:00 Corrected"), Some("0:00 Provider")),
            Some("0:00 Corrected")
        );
        assert_eq!(
            effective_chapter_input(Some("  "), Some("0:00 Provider")),
            Some("0:00 Provider")
        );
        assert_eq!(effective_chapter_input(None, None), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn playlist_enumeration_rejects_unknown_availability_operationally() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' ",
                "'{\"id\":\"invalid\",\"url\":\"https://example.test/1\",",
                "\"title\":\"Invalid\",\"availability\":\"maybe_later\"}'\n"
            ),
        )
        .expect("fixture script");
        let mut permissions = std::fs::metadata(&script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("fixture permissions");
        let pool = fixture_addon_pool(&package_root).await;

        let error = enumerate_playlist(
            &pool,
            "org.ersatzrs.addon.fixture",
            "https://example.test/list",
            None,
            &AddonRuntimeContext::default(),
        )
        .await
        .expect_err("unknown availability must reject enumeration");
        let message = error.to_string();
        assert!(message.contains("is not a valid record"));
        assert!(!message.contains("maybe_later"));
        assert!(message.len() < 512);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unknown_field_refresh_preserves_complete_managed_generation() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        write_fixture_playlist_script(&script, "https://example.test/1", true);
        let pool = fixture_addon_pool(&package_root).await;
        let runtime = AddonRuntimeContext::default();
        let manifest_path = temp.path().join("Strict.playlist.yml");
        let manifest =
            parse_playlist_manifest(valid_yaml(), &manifest_path).expect("strict manifest");
        let first = sync_playlist_files(
            &pool,
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &runtime,
        )
        .await
        .expect("initial complete generation");
        let folder = temp.path().join("Strict");
        let original_files = ["one.yml", "one.nfo", "two.yml", "two.nfo"].map(|name| {
            (
                name,
                std::fs::read(folder.join(name)).expect("original managed file"),
            )
        });

        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' \\\n",
                "  '{\"id\":\"new\",\"url\":\"https://example.test/new\",\"title\":\"New\",\"duration_seconds\":60}' \\\n",
                "  '{\"id\":\"invalid\",\"url\":\"https://example.test/invalid\",\"title\":\"Invalid\",\"duration_seconds\":60,\"geners\":[]}'\n"
            ),
        )
        .expect("write strictness failure fixture");
        update_fixture_content_hash(&pool, &package_root).await;

        let error = sync_playlist_files(
            &pool,
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &runtime,
        )
        .await
        .expect_err("unknown field must reject the complete refresh");
        assert!(error.to_string().contains("unknown field `geners`"));
        assert!(!folder.join("new.yml").exists());
        assert!(!folder.join("new.nfo").exists());
        for (name, bytes) in original_files {
            assert_eq!(
                std::fs::read(folder.join(name)).expect("preserved managed file"),
                bytes
            );
        }
        let failed = load_sidecar(&folder.join(".ersatzrs-playlist.json"))
            .expect("failed state")
            .expect("persisted failed state");
        assert_eq!(failed.playlist_etag, first.state.playlist_etag);
        assert_eq!(failed.entries, first.state.entries);
        assert_eq!(failed.last_result.kind, "failed");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn playlist_enumeration_enforces_installed_addon_network_hosts() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "set -eu\n",
                "test \"$ERSATZRS_REMOTE_STREAM_PLAYLIST_URL\" = ",
                "'https://allowed.example.test/@evil.example/list'\n",
                "printf '%s\\n' ",
                "'{\"id\":\"one\",\"url\":\"https://example.test/1\",\"title\":\"One\"}'\n"
            ),
        )
        .expect("fixture script");
        let mut permissions = std::fs::metadata(&script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("fixture permissions");
        let pool = fixture_addon_pool(&package_root).await;
        let manifest_json: String = sqlx::query_scalar(
            r#"SELECT "ManifestJson" FROM "AddonInstallation" WHERE "AddonId" = ?"#,
        )
        .bind("org.ersatzrs.addon.fixture")
        .fetch_one(&pool)
        .await
        .expect("installed manifest");
        let mut manifest: serde_json::Value =
            serde_json::from_str(&manifest_json).expect("manifest JSON");
        manifest["permissions"]["network_hosts"] = serde_json::json!(["allowed.example.test"]);
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson" = ? WHERE "AddonId" = ?"#)
            .bind(manifest.to_string())
            .bind("org.ersatzrs.addon.fixture")
            .execute(&pool)
            .await
            .expect("restrict network hosts");

        let error = enumerate_playlist(
            &pool,
            "org.ersatzrs.addon.fixture",
            "https://example.test/list",
            None,
            &AddonRuntimeContext::default(),
        )
        .await
        .expect_err("undeclared host");
        let message = error.to_string();
        assert!(message.contains("unsupported-url"));
        assert!(message.contains("example.test"));
        assert!(!message.contains("/list"));

        let enumeration = enumerate_playlist(
            &pool,
            "org.ersatzrs.addon.fixture",
            r"https://allowed.example.test\@evil.example/list",
            None,
            &AddonRuntimeContext::default(),
        )
        .await
        .expect("add-on receives the validated canonical URL");
        assert_eq!(enumeration.entries.len(), 1);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn generated_nfo_round_trips_every_generic_content_kind() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let manifest = parse_playlist_manifest(valid_yaml(), Path::new("Fixture.playlist.yml"))
            .expect("manifest");
        let base = PlaylistEntry {
            id: "entry".to_owned(),
            provider_id: Some("provider-entry".to_owned()),
            url: "https://example.test/watch/entry".to_owned(),
            title: "Example title".to_owned(),
            original_title: Some("Original example".to_owned()),
            sort_title: Some("Example, The".to_owned()),
            outline: Some("Example outline".to_owned()),
            tagline: Some("Example tagline".to_owned()),
            duration_seconds: Some(121),
            plot: Some("Example plot".to_owned()),
            year: Some(2001),
            release_date: Some("2001-02-03T04:05:06Z".to_owned()),
            content_rating: Some("12".to_owned()),
            genres: vec!["Documentary".to_owned()],
            tags: vec!["Archive".to_owned()],
            collection: Some("Example Collection".to_owned()),
            people: vec![
                RemoteStreamPerson {
                    name: "Alex Actor".to_owned(),
                    role: "Example role".to_owned(),
                },
                RemoteStreamPerson {
                    name: "Drew Director".to_owned(),
                    role: "director".to_owned(),
                },
                RemoteStreamPerson {
                    name: "Robin Writer".to_owned(),
                    role: "writer".to_owned(),
                },
                RemoteStreamPerson {
                    name: "Sam Artist".to_owned(),
                    role: "artist".to_owned(),
                },
            ],
            producers: vec!["Example Producer".to_owned()],
            original_broadcasters: vec!["Original Network".to_owned()],
            broadcasters: vec!["Current Network".to_owned()],
            thumbnail_url: Some("https://example.test/thumb.jpg".to_owned()),
            availability: WireAvailability::Available,
            show_title: Some("Example Show".to_owned()),
            season: Some(2),
            episode: Some(3),
            guids: vec!["catalog://abc-123".to_owned()],
            ..PlaylistEntry::default()
        };

        for (content_kind, expected_root) in [
            (WireContentKind::TelevisionEpisode, "episodedetails"),
            (WireContentKind::Movie, "movie"),
            (WireContentKind::MusicVideo, "musicvideo"),
            (WireContentKind::OtherVideo, "movie"),
        ] {
            let entry = PlaylistEntry {
                content_kind,
                ..base.clone()
            };
            let path = temp.path().join(format!("{content_kind:?}.nfo"));
            write_generated_nfo(&path, &manifest, &entry).expect("write generated NFO");
            let xml = std::fs::read_to_string(&path).expect("read generated NFO");
            assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
            assert!(xml.contains(&format!(
                "<{expected_root} xmlns:ersatzrs=\"urn:ersatzrs:nfo:1\">"
            )));
            assert!(xml.ends_with(&format!("</{expected_root}>\n")));
            assert!(xml.contains("<runtime>3</runtime>"));
            assert!(xml.contains("<thumb>https://example.test/thumb.jpg</thumb>"));

            let parsed = crate::nfo::parse_other_video_nfo(&path).expect("round-trip NFO");
            assert_eq!(parsed.title.as_deref(), Some("Example title"));
            assert_eq!(parsed.original_title.as_deref(), Some("Original example"));
            assert_eq!(parsed.sort_title.as_deref(), Some("Example, The"));
            assert_eq!(parsed.outline.as_deref(), Some("Example outline"));
            assert_eq!(parsed.plot.as_deref(), Some("Example plot"));
            assert_eq!(parsed.tagline.as_deref(), Some("Example tagline"));
            assert_eq!(parsed.year, Some(2001));
            assert_eq!(
                parsed
                    .release_datetime
                    .map(|value| value.to_rfc3339())
                    .as_deref(),
                Some("2001-02-03T04:05:06+00:00")
            );
            assert_eq!(parsed.content_ratings, ["12"]);
            assert_eq!(parsed.genres, ["Documentary"]);
            assert!(parsed.tags.contains(&"Archive".to_owned()));
            assert!(parsed.tags.contains(&"Example Collection".to_owned()));
            assert_eq!(parsed.collection.as_deref(), Some("Example Collection"));
            assert_eq!(parsed.producers, ["Example Producer"]);
            assert_eq!(parsed.original_broadcasters, ["Original Network"]);
            assert_eq!(parsed.broadcasters, ["Current Network"]);
            assert_eq!(parsed.directors, ["Drew Director"]);
            assert_eq!(parsed.writers, ["Robin Writer"]);
            assert_eq!(parsed.guids.len(), 2);
            if content_kind == WireContentKind::TelevisionEpisode {
                assert_eq!(parsed.show_title.as_deref(), Some("Example Show"));
                assert_eq!(parsed.season, Some(2));
                assert_eq!(parsed.episode, Some(3));
            } else {
                assert!(parsed.show_title.is_none());
                assert!(parsed.season.is_none());
                assert!(parsed.episode.is_none());
            }
            if content_kind == WireContentKind::MusicVideo {
                assert_eq!(parsed.artists, ["Sam Artist"]);
            } else {
                assert!(parsed.artists.is_empty());
            }
        }

        let entry = PlaylistEntry {
            content_kind: WireContentKind::TelevisionEpisode,
            show_title: None,
            season: None,
            episode: None,
            ..base
        };
        let path = temp.path().join("episode-without-numbers.nfo");
        write_generated_nfo(&path, &manifest, &entry).expect("write numberless episode NFO");
        let xml = std::fs::read_to_string(path).expect("read numberless episode NFO");
        assert!(!xml.contains("<showtitle>"));
        assert!(!xml.contains("<season>"));
        assert!(!xml.contains("<episode>"));
    }

    #[test]
    fn generated_pair_rolls_back_definition_when_nfo_publication_fails() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let definition_path = temp.path().join("entry.yml");
        let nfo_path = temp.path().join("entry.nfo");
        std::fs::write(&definition_path, b"old definition").expect("old definition");
        std::fs::create_dir(&nfo_path).expect("blocking NFO directory");

        let error =
            write_generated_pair(&definition_path, b"new definition", &nfo_path, b"new NFO")
                .expect_err("NFO publication must fail");

        assert!(error.to_string().contains("managed NFO"));
        assert_eq!(
            std::fs::read(&definition_path).expect("restored definition"),
            b"old definition"
        );
        assert!(nfo_path.is_dir());
    }

    #[test]
    fn generated_pair_does_not_publish_nfo_when_definition_publication_fails() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let definition_path = temp.path().join("entry.yml");
        let nfo_path = temp.path().join("entry.nfo");
        std::fs::create_dir(&definition_path).expect("blocking definition directory");
        std::fs::write(&nfo_path, b"old NFO").expect("old NFO");

        let error =
            write_generated_pair(&definition_path, b"new definition", &nfo_path, b"new NFO")
                .expect_err("definition publication must fail");

        assert!(error.to_string().contains("publish managed definition"));
        assert!(definition_path.is_dir());
        assert_eq!(std::fs::read(&nfo_path).expect("unchanged NFO"), b"old NFO");
    }

    #[test]
    fn state_publication_failure_cannot_create_a_partial_sidecar() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let sidecar_path = temp.path().join(".ersatzrs-playlist.json");
        std::fs::create_dir(&sidecar_path).expect("blocking sidecar directory");
        let state = PlaylistSidecarState {
            version: 2,
            manifest_etag: "manifest".to_owned(),
            playlist_etag: "playlist".to_owned(),
            last_sync: Utc::now(),
            last_result: PlaylistSyncOutcome {
                kind: "ok".to_owned(),
                entries: 0,
                truncated: false,
                error: None,
            },
            managed_files: BTreeMap::new(),
            managed_nfo_files: BTreeMap::new(),
            entries: Vec::new(),
            overrides: PlaylistOverrides::default(),
        };

        let error =
            write_json_atomically(&sidecar_path, &state).expect_err("state publication must fail");
        assert!(error.to_string().contains("publish managed state"));
        assert!(sidecar_path.is_dir());
        assert!(std::fs::read_dir(temp.path())
            .expect("read temp folder")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .contains("tmp")));
    }

    #[test]
    fn staged_sidecar_is_invisible_until_database_boundary_publishes_it() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let sidecar_path = temp.path().join(".ersatzrs-playlist.json");
        let sync = PlaylistFileSync {
            state: membership_sidecar(Vec::new()),
            definitions: Vec::new(),
            removed_definition_paths: Vec::new(),
            changed: true,
            membership_changed: true,
            membership_delta: PlaylistMembershipDelta::default(),
            enumerated: true,
            sidecar_publication: SidecarPublication::Staged(sidecar_path.clone()),
        };

        assert!(!sidecar_path.exists());
        sync.publish_sidecar().expect("publish staged sidecar");
        assert_eq!(
            load_sidecar(&sidecar_path)
                .expect("load published sidecar")
                .expect("published state"),
            sync.state
        );
    }

    #[test]
    fn sidecar_integrity_keeps_definitions_strict_and_defers_nfo_validation() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let definition = b"definition";
        let nfo = b"nfo";
        std::fs::write(temp.path().join("entry.yml"), definition).expect("definition");
        std::fs::write(temp.path().join("entry.nfo"), nfo).expect("NFO");
        let state = PlaylistSidecarState {
            version: 2,
            manifest_etag: "manifest".to_owned(),
            playlist_etag: "playlist".to_owned(),
            last_sync: Utc::now(),
            last_result: PlaylistSyncOutcome {
                kind: "ok".to_owned(),
                entries: 1,
                truncated: false,
                error: None,
            },
            managed_files: BTreeMap::from([("entry".to_owned(), "entry.yml".to_owned())]),
            managed_nfo_files: BTreeMap::from([("entry".to_owned(), "entry.nfo".to_owned())]),
            entries: vec![PlaylistSidecarEntry {
                id: "entry".to_owned(),
                provider_id: None,
                file: "entry.yml".to_owned(),
                title: "Entry".to_owned(),
                duration_seconds: None,
                active: true,
                liveness: None,
                nfo_file: Some("entry.nfo".to_owned()),
                availability: AddonMediaListItemAvailability::Available,
                definition_sha256: Some(hex_hash(definition)),
                nfo_sha256: Some(hex_hash(nfo)),
                entry_etag: None,
                artwork: None,
                artwork_roles: BTreeMap::new(),
                subtitles: Vec::new(),
            }],
            overrides: PlaylistOverrides::default(),
        };
        assert!(managed_files_intact(temp.path(), &state));

        std::fs::write(temp.path().join("entry.nfo"), b"different generation")
            .expect("replace NFO");
        assert!(managed_files_intact(temp.path(), &state));
        assert!(ersatzrs_infra::addon_media_nfo::adopt_existing(
            &temp.path().join("entry.nfo"),
            state.entries[0].nfo_sha256.as_deref(),
        )
        .is_err());

        std::fs::write(temp.path().join("entry.yml"), b"different generation")
            .expect("replace definition");
        assert!(!managed_files_intact(temp.path(), &state));
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn file_sync_preserves_identity_order_and_skips_fresh_enumeration() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        std::fs::write(
            &script,
            concat!(
                "#!/bin/sh\n",
                "printf '%s\\n' \\\n",
                "  '{\"id\":\"one\",\"url\":\"https://example.test/1\",\"title\":\"One\",\"duration_seconds\":60,\"plot\":\"Plot\",\"year\":2001,\"release_date\":\"2001-02-03T04:05:06Z\",\"content_rating\":\"12\",\"genres\":[\"Documentary\"],\"tags\":[\"Archive\"],\"collection\":\"Example Collection\",\"people\":[{\"name\":\"Alex Example\",\"role\":\"narrator\"}],\"producers\":[\"Example Producer\"],\"original_broadcasters\":[\"Original Network\"],\"broadcasters\":[\"Current Network\"]}' \\\n",
                "  '{\"id\":\"two\",\"url\":\"https://example.test/2\",\"title\":\"Two\",\"duration_seconds\":120}'\n"
            ),
        )
        .expect("fixture script");
        let mut permissions = std::fs::metadata(&script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("fixture permissions");
        let pool = fixture_addon_pool(&package_root).await;
        let runtime = AddonRuntimeContext::default();
        let manifest_path = temp.path().join("Fixture.playlist.yml");
        let manifest_bytes = valid_yaml().as_bytes();
        let manifest = parse_playlist_manifest(valid_yaml(), &manifest_path).expect("manifest");

        let first = sync_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
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
        assert!(parsed.release_date.is_none());
        assert!(parsed.content_rating.is_none());
        assert!(parsed.genres.is_empty());
        let nfo_path = first.definitions[0].path.with_extension("nfo");
        let nfo = crate::nfo::parse_other_video_nfo(&nfo_path).expect("generated NFO");
        assert_eq!(
            nfo.release_datetime
                .map(|value| value.to_rfc3339())
                .as_deref(),
            Some("2001-02-03T04:05:06+00:00")
        );
        assert_eq!(nfo.content_ratings, ["12"]);
        assert_eq!(nfo.genres, ["Documentary"]);
        assert!(nfo.tags.contains(&"Archive".to_owned()));
        assert_eq!(nfo.collection.as_deref(), Some("Example Collection"));
        assert_eq!(nfo.actors[0].role.as_deref(), Some("narrator"));
        assert_eq!(nfo.producers, ["Example Producer"]);
        assert_eq!(nfo.original_broadcasters, ["Original Network"]);
        assert_eq!(nfo.broadcasters, ["Current Network"]);
        assert_eq!(first.state.version, PLAYLIST_SIDECAR_VERSION);
        assert_eq!(
            first.state.entries[0].liveness,
            Some(RemoteStreamLiveness::Finite)
        );
        let resolved = ersatzrs_infra::remote_stream::resolve_remote_stream_with_addons(
            &pool, &parsed, &runtime,
        )
        .await
        .expect("generated command");
        let producer = ersatzrs_infra::remote_stream::remote_stream_producer(&resolved)
            .expect("generated add-on producer");
        assert_eq!(
            producer
                .environment
                .get("ERSATZRS_REMOTE_STREAM_URL")
                .map(String::as_str),
            Some("https://example.test/1")
        );
        assert_eq!(parsed.addon.as_deref(), Some("org.ersatzrs.addon.fixture"));

        let skipped =
            sync_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
                .await
                .expect("interval skip");
        assert!(!skipped.enumerated);
        assert_eq!(skipped.definitions.len(), 2);

        std::fs::write(&script, "#!/bin/sh\necho addon-updated >&2\nexit 1\n")
            .expect("replace fixture");
        let content_sha256 = ersatzrs_infra::addons::calculate_addon_content_sha256(&package_root)
            .await
            .expect("hash updated package");
        sqlx::query(
            r#"UPDATE "AddonInstallation" SET "PackageSha256" = ?, "ContentSha256" = ?
               WHERE "AddonId" = ?"#,
        )
        .bind("b".repeat(64))
        .bind(content_sha256)
        .bind("org.ersatzrs.addon.fixture")
        .execute(&pool)
        .await
        .expect("record fixture package update");
        let error = sync_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
            .await
            .expect_err("updated add-on must bypass the fresh interval");
        assert!(error.to_string().contains("enumeration failed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn staged_publication_replays_the_import_snapshot_without_addon_enumeration() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        write_fixture_playlist_script(&script, "https://example.test/1", true);
        let pool = fixture_addon_pool(&package_root).await;
        let runtime = AddonRuntimeContext::default();
        let manifest_path = temp.path().join("Managed.playlist.yml");
        let manifest_bytes = valid_yaml().as_bytes();
        let manifest = parse_playlist_manifest(valid_yaml(), &manifest_path).expect("manifest");

        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(&pool)
            .await
            .expect("media source");
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Managed', 7, 1)"#,
        )
        .execute(&pool)
        .await
        .expect("library");
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ReviewState"
               ) VALUES (1, 'org.ersatzrs.addon.fixture', 'fixture/list',
                         'https://example.test/playlist', 'Managed', 1)"#,
        )
        .execute(&pool)
        .await
        .expect("managed list");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (1, 1, 'Managed', ?)"#,
        )
        .bind(manifest_path.to_string_lossy().as_ref())
        .execute(&pool)
        .await
        .expect("managed library");
        for (provider_id, rank, title) in [("one", 0, "One"), ("two", 1, "Two")] {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItem" (
                       "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank",
                       "Title", "SourceUrl", "Published", "ProviderMetadataJson"
                   ) VALUES (1, ?, ?, 4, ?, ?, ?, 0, ?)"#,
            )
            .bind(provider_id)
            .bind(title)
            .bind(rank)
            .bind(title)
            .bind(format!("https://example.test/{provider_id}"))
            .bind(serde_json::json!({"title": title}).to_string())
            .execute(&pool)
            .await
            .expect("managed item");
        }

        let draft = sync_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
            .await
            .expect("draft sync");
        assert!(draft.enumerated);
        assert!(draft.definitions.is_empty());
        assert_eq!(draft.state.last_result.entries, 0);

        sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewState" = 2 WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .expect("simulate affected publication");
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "Published" = 1
               WHERE "AddonMediaListId" = 1"#,
        )
        .execute(&pool)
        .await
        .expect("publish item selection");

        let recovered =
            sync_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
                .await
                .expect("recover published empty cache");
        assert!(recovered.enumerated);
        assert_eq!(recovered.definitions.len(), 2);

        sqlx::query(
            r#"UPDATE "AddonMediaListItem"
               SET "MetadataOverrideJson" = '{"title":"Operator One","tags":["edited"]}'
               WHERE "AddonMediaListId" = 1 AND "ProviderId" = 'one'"#,
        )
        .execute(&pool)
        .await
        .expect("save reviewed metadata override");
        std::fs::write(&script, "#!/bin/sh\necho must-not-run >&2\nexit 1\n")
            .expect("replace fixture with a failing provider");
        let content_sha256 = ersatzrs_infra::addons::calculate_addon_content_sha256(&package_root)
            .await
            .expect("hash updated package");
        sqlx::query(
            r#"UPDATE "AddonInstallation" SET "PackageSha256" = ?, "ContentSha256" = ?
               WHERE "AddonId" = ?"#,
        )
        .bind("b".repeat(64))
        .bind(content_sha256)
        .bind("org.ersatzrs.addon.fixture")
        .execute(&pool)
        .await
        .expect("record fixture package update");
        let legacy_journal = serde_json::json!({
            "previous_review_state": ersatzrs_domain::AddonMediaListReviewState::Published,
            "previous_presentation_kind": ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            "previous_items": [], "previous_playlist": null
        });
        sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewState" = 3, "PublicationJournalJson" = ? WHERE "Id" = 1"#)
            .bind(legacy_journal.to_string())
            .execute(&pool)
            .await
            .expect("stage publication");
        let publishing =
            sync_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
                .await
                .expect("publishing sync");
        assert!(!publishing.enumerated);
        assert_eq!(
            publishing
                .definitions
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        let reviewed_nfo =
            crate::nfo::parse_other_video_nfo(publishing.definitions[0].path.with_extension("nfo"))
                .expect("reviewed NFO");
        assert_eq!(reviewed_nfo.title.as_deref(), Some("Operator One"));
        assert!(reviewed_nfo.tags.contains(&"edited".to_owned()));

        assert_staged_replay_preserves_files(
            &pool,
            &manifest_path,
            manifest_bytes,
            &manifest,
            &runtime,
        )
        .await;
        sqlx::query(r#"INSERT INTO "AddonMediaListArtifact"
            ("Id","AddonMediaListId","CacheKey","Kind","State","CreatedAt","UpdatedAt","DurationMilliseconds")
            VALUES (10,1,'prepared-replay',1,2,'2026-09-05 12:00:00','2026-09-05 12:00:00',60000)"#)
            .execute(&pool).await.expect("prepared artifact");
        let mut prepared_journal = legacy_journal.clone();
        prepared_journal["prepared_media"] = serde_json::json!({"one":10});
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=120
            WHERE "AddonMediaListId"=1 AND "ProviderId"='one'"#,
        )
        .execute(&pool)
        .await
        .expect("provider duration differs from prepared duration");
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"=? WHERE "Id"=1"#)
            .bind(prepared_journal.to_string())
            .execute(&pool)
            .await
            .expect("prepared selection");
        let prepared =
            stage_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
                .await
                .expect("changed prepared publication");
        assert!(matches!(
            prepared.sidecar_publication,
            SidecarPublication::Staged(_)
        ));
        assert_eq!(prepared.definitions[0].duration_seconds, Some(60));
        prepared.publish_sidecar().expect("publish changed sidecar");
        assert_staged_replay_preserves_files(
            &pool,
            &manifest_path,
            manifest_bytes,
            &manifest,
            &runtime,
        )
        .await;
        let mut streaming_journal = legacy_journal.clone();
        streaming_journal["streaming_plans"] =
            serde_json::json!({"one": streaming_duration_plan()});
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"=? WHERE "Id"=1"#)
            .bind(streaming_journal.to_string())
            .execute(&pool)
            .await
            .expect("interval streaming selection");
        let streaming =
            stage_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
                .await
                .expect("changed streaming publication");
        assert!(matches!(
            streaming.sidecar_publication,
            SidecarPublication::Staged(_)
        ));
        assert_eq!(streaming.definitions[0].duration_seconds, Some(17));
        let definition: serde_yaml::Value =
            serde_yaml::from_slice(&std::fs::read(&streaming.definitions[0].path).unwrap())
                .unwrap();
        assert_eq!(definition["duration"].as_str(), Some("00:00:16.375"));
        streaming
            .publish_sidecar()
            .expect("publish streaming sidecar");
        assert_staged_replay_preserves_files(
            &pool,
            &manifest_path,
            manifest_bytes,
            &manifest,
            &runtime,
        )
        .await;
        // Restore the legacy selection before the original completed-cache check.
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"=? WHERE "Id"=1"#)
            .bind(legacy_journal.to_string())
            .execute(&pool)
            .await
            .expect("legacy selection");
        sync_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
            .await
            .expect("restore legacy generated files");

        sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewState" = 2 WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .expect("commit publication");
        let published =
            sync_playlist_files(&pool, &manifest_path, manifest_bytes, &manifest, &runtime)
                .await
                .expect("published cache sync");
        assert!(!published.enumerated);
        assert_eq!(published.definitions.len(), 2);
    }

    #[cfg(unix)]
    async fn assert_staged_replay_preserves_files(
        pool: &DbPool,
        manifest_path: &Path,
        manifest_bytes: &[u8],
        manifest: &ValidatedPlaylistManifest,
        runtime: &AddonRuntimeContext,
    ) {
        fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
            let mut files = BTreeMap::new();
            let mut pending = vec![root.to_path_buf()];
            while let Some(directory) = pending.pop() {
                for entry in std::fs::read_dir(directory).expect("read managed tree") {
                    let path = entry.expect("managed entry").path();
                    if path.is_dir() {
                        pending.push(path);
                    } else {
                        files.insert(path.clone(), std::fs::read(path).expect("managed bytes"));
                    }
                }
            }
            files
        }
        let folder = manifest_path.parent().unwrap().join(&manifest.folder);
        let before = snapshot(&folder);
        let replay = stage_playlist_files(pool, manifest_path, manifest_bytes, manifest, runtime)
            .await
            .expect("verification replay");
        assert!(!replay.enumerated);
        assert!(matches!(
            replay.sidecar_publication,
            SidecarPublication::NotNeeded
        ));
        replay.publish_sidecar().expect("publish replay sidecar");
        assert_eq!(snapshot(&folder), before);
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn file_sync_heals_missing_and_v1_pairs_and_honors_removal_policies() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        write_fixture_playlist_script(&script, "https://example.test/1", true);
        let pool = fixture_addon_pool(&package_root).await;
        let runtime = AddonRuntimeContext::default();

        let trash_manifest_path = temp.path().join("Trash.playlist.yml");
        let trash_manifest =
            parse_playlist_manifest(valid_yaml(), &trash_manifest_path).expect("trash manifest");
        let first = sync_playlist_files(
            &pool,
            &trash_manifest_path,
            valid_yaml().as_bytes(),
            &trash_manifest,
            &runtime,
        )
        .await
        .expect("initial trash-policy sync");
        let folder = temp.path().join("Trash");
        let sidecar_path = folder.join(".ersatzrs-playlist.json");
        let manual_path = folder.join("operator-notes.txt");
        std::fs::write(&manual_path, "keep me").expect("manual sibling");

        std::fs::remove_file(folder.join("one.nfo")).expect("remove managed NFO");
        let healed_nfo = sync_playlist_files(
            &pool,
            &trash_manifest_path,
            valid_yaml().as_bytes(),
            &trash_manifest,
            &runtime,
        )
        .await
        .expect("heal missing NFO");
        assert!(healed_nfo.enumerated);
        assert!(folder.join("one.nfo").is_file());

        std::fs::remove_file(folder.join("two.yml")).expect("remove managed definition");
        let healed_definition = sync_playlist_files(
            &pool,
            &trash_manifest_path,
            valid_yaml().as_bytes(),
            &trash_manifest,
            &runtime,
        )
        .await
        .expect("heal missing definition");
        assert!(healed_definition.enumerated);
        assert!(folder.join("two.yml").is_file());

        let mut legacy = healed_definition.state;
        legacy.version = 1;
        legacy.last_sync = Utc::now();
        for entry in &mut legacy.entries {
            entry.definition_sha256 = None;
            entry.nfo_sha256 = None;
        }
        write_json_atomically(&sidecar_path, &legacy).expect("write v1-compatible state");
        let upgraded = sync_playlist_files(
            &pool,
            &trash_manifest_path,
            valid_yaml().as_bytes(),
            &trash_manifest,
            &runtime,
        )
        .await
        .expect("upgrade v1 state");
        assert!(upgraded.enumerated);
        assert_eq!(upgraded.state.version, PLAYLIST_SIDECAR_VERSION);
        assert!(upgraded
            .state
            .entries
            .iter()
            .all(|entry| { entry.definition_sha256.is_some() && entry.nfo_sha256.is_some() }));

        write_fixture_playlist_script(&script, "https://example.test/1", false);
        update_fixture_content_hash(&pool, &package_root).await;
        let trashed = sync_playlist_files(
            &pool,
            &trash_manifest_path,
            valid_yaml().as_bytes(),
            &trash_manifest,
            &runtime,
        )
        .await
        .expect("trash removed entry");
        assert_eq!(trashed.state.entries.len(), 1);
        assert!(!folder.join("two.yml").exists());
        assert!(!folder.join("two.nfo").exists());
        let trash_names = std::fs::read_dir(folder.join(".ersatzrs-trash"))
            .expect("managed trash")
            .map(|entry| {
                entry
                    .expect("trash entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert!(trash_names.iter().any(|name| name.ends_with("-two.yml")));
        assert!(trash_names.iter().any(|name| name.ends_with("-two.nfo")));
        assert_eq!(
            std::fs::read_to_string(&manual_path).expect("manual sibling"),
            "keep me"
        );

        write_fixture_playlist_script(&script, "https://example.test/1", true);
        update_fixture_content_hash(&pool, &package_root).await;
        let keep_yaml = valid_yaml().replace("on_removed: trash", "on_removed: keep");
        let keep_manifest_path = temp.path().join("Keep.playlist.yml");
        let keep_manifest =
            parse_playlist_manifest(&keep_yaml, &keep_manifest_path).expect("keep manifest");
        sync_playlist_files(
            &pool,
            &keep_manifest_path,
            keep_yaml.as_bytes(),
            &keep_manifest,
            &runtime,
        )
        .await
        .expect("initial keep-policy sync");
        let keep_folder = temp.path().join("Keep");
        let keep_manual_path = keep_folder.join("operator-notes.txt");
        std::fs::write(&keep_manual_path, "also keep me").expect("keep manual sibling");
        write_fixture_playlist_script(&script, "https://example.test/1", false);
        update_fixture_content_hash(&pool, &package_root).await;
        let kept = sync_playlist_files(
            &pool,
            &keep_manifest_path,
            keep_yaml.as_bytes(),
            &keep_manifest,
            &runtime,
        )
        .await
        .expect("keep removed entry");
        let inactive = kept
            .state
            .entries
            .iter()
            .find(|entry| entry.id == "two")
            .expect("inactive retained entry");
        assert!(!inactive.active);
        assert!(keep_folder.join("two.yml").is_file());
        assert!(keep_folder.join("two.nfo").is_file());
        assert_eq!(
            std::fs::read_to_string(keep_manual_path).expect("keep manual sibling"),
            "also keep me"
        );
        assert!(managed_files_intact(&keep_folder, &kept.state));
        assert_eq!(first.state.entries.len(), 2);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn later_pair_failure_preserves_state_and_next_sync_heals_generation() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        write_fixture_playlist_script(&script, "https://example.test/original", true);
        let pool = fixture_addon_pool(&package_root).await;
        let runtime = AddonRuntimeContext::default();
        let manifest_path = temp.path().join("Failure.playlist.yml");
        let manifest =
            parse_playlist_manifest(valid_yaml(), &manifest_path).expect("failure manifest");
        let first = sync_playlist_files(
            &pool,
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &runtime,
        )
        .await
        .expect("initial sync");
        let folder = temp.path().join("Failure");
        let sidecar_path = folder.join(".ersatzrs-playlist.json");
        let original_state_bytes = std::fs::read(&sidecar_path).expect("original state");

        std::fs::remove_file(folder.join("two.nfo")).expect("remove second NFO");
        std::fs::create_dir(folder.join("two.nfo")).expect("block second NFO publication");
        write_fixture_playlist_script(&script, "https://example.test/changed", true);
        update_fixture_content_hash(&pool, &package_root).await;
        let error = sync_playlist_files(
            &pool,
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &runtime,
        )
        .await
        .expect_err("second pair publication must fail");
        assert!(error.to_string().contains("managed NFO"));
        assert_eq!(
            std::fs::read(&sidecar_path).expect("unchanged state"),
            original_state_bytes
        );
        assert!(std::fs::read_to_string(folder.join("one.yml"))
            .expect("first updated definition")
            .contains("https://example.test/changed"));
        assert!(!managed_files_intact(&folder, &first.state));

        std::fs::remove_dir(folder.join("two.nfo")).expect("remove NFO blocker");
        let healed = sync_playlist_files(
            &pool,
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &runtime,
        )
        .await
        .expect("heal interrupted generation");
        assert!(healed.enumerated);
        assert!(managed_files_intact(&folder, &healed.state));
        assert!(folder.join("two.nfo").is_file());
        assert!(std::fs::read_dir(&folder)
            .expect("managed folder")
            .all(|entry| !entry
                .expect("managed entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".tmp")));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_sync_is_persisted_and_sidecar_exclusions_change_membership() {
        use std::os::unix::fs::PermissionsExt as _;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let package_root = temp.path().join("addon-package");
        std::fs::create_dir_all(&package_root).expect("package root");
        let script = package_root.join("fixture.sh");
        let manifest_path = temp.path().join("Fixture.playlist.yml");
        let manifest = parse_playlist_manifest(valid_yaml(), &manifest_path).expect("manifest");
        std::fs::write(&script, "#!/bin/sh\necho failed >&2\nexit 1\n").expect("failed fixture");
        let mut permissions = std::fs::metadata(&script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("fixture permissions");
        let pool = fixture_addon_pool(&package_root).await;
        let runtime = AddonRuntimeContext::default();

        sync_playlist_files(
            &pool,
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &runtime,
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
        update_fixture_content_hash(&pool, &package_root).await;
        let first = sync_playlist_files(
            &pool,
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &runtime,
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
        update_fixture_content_hash(&pool, &package_root).await;
        let second = sync_playlist_files(
            &pool,
            &manifest_path,
            valid_yaml().as_bytes(),
            &manifest,
            &runtime,
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
