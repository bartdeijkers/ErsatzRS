//! Stable, dependency-light contracts shared by the ErsatzRS host and add-ons.
//!
//! Add-ons are separate processes. This crate defines package metadata,
//! repository catalogs, settings, declarative panels, and bounded invocation
//! results; it is not a native plugin ABI.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Datelike;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

/// Current package-manifest schema. Schema v1 remains readable.
pub const ADDON_MANIFEST_VERSION: u32 = 3;
/// Current repository-index schema. Schema v1 remains readable.
pub const ADDON_REPOSITORY_SCHEMA_VERSION: u32 = 2;
/// Health capability implemented by every executable add-on.
pub const CAPABILITY_ADDON_CHECK_V1: &str = "addon.check.v1";
/// Enumerate a Remote Stream playlist as bounded `NDJSON`.
pub const CAPABILITY_REMOTE_STREAM_LIST_V1: &str = "remote-stream.list.v1";
/// Enumerate Remote Streams with stable identity and availability metadata.
pub const CAPABILITY_REMOTE_STREAM_LIST_V2: &str = "remote-stream.list.v2";
pub const CAPABILITY_REMOTE_STREAM_LIST_V3: &str = "remote-stream.list.v3";
/// Describe already-enumerated Remote Streams the host chose to keep.
///
/// Enumeration stays cheap and identity-only; this answers with descriptive
/// metadata for a bounded set of entries, so the cost of rich metadata scales
/// with the items actually retained rather than the whole upstream listing.
pub const CAPABILITY_REMOTE_STREAM_ITEM_V1: &str = "remote-stream.item.v1";
/// Describe retained Remote Streams, including provider-supplied chapter text.
pub const CAPABILITY_REMOTE_STREAM_ITEM_V2: &str = "remote-stream.item.v2";
/// Produce Remote Stream bytes for playback.
pub const CAPABILITY_REMOTE_STREAM_PLAY_V1: &str = "remote-stream.play.v1";
/// Enumerate a provider list whose entries are matched to local media.
pub const CAPABILITY_MEDIA_LIST_LIST_V1: &str = "media-list.list.v1";
/// Enumerate a provider list with explicit per-item availability.
pub const CAPABILITY_MEDIA_LIST_LIST_V2: &str = "media-list.list.v2";
pub const CAPABILITY_MEDIA_LIST_LIST_V3: &str = "media-list.list.v3";
/// Host-rendered declarative add-on panel schema.
pub const CAPABILITY_DECLARATIVE_PANEL_V1: &str = "declarative-panel.v1";
/// Maximum UTF-8 bytes for provider/list/item identities stored and indexed by the host.
pub const MAX_PROVIDER_ID_BYTES: usize = 384;
/// Maximum UTF-8 bytes for a provider-neutral availability diagnostic.
pub const MAX_AVAILABILITY_REASON_BYTES: usize = 2_048;
/// Maximum UTF-8 size of provider-supplied timestamp/title chapter text.
pub const MAX_CHAPTER_INPUT_BYTES: usize = 64 * 1_024;
/// Maximum UTF-8 bytes in one add-on operation diagnostic message.
pub const MAX_OPERATION_ERROR_MESSAGE_BYTES: usize = 1_024;
/// Maximum captured output accepted from `media-list.list`.
pub const MAX_MEDIA_LIST_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum captured output accepted from `remote-stream.list`.
pub const MAX_REMOTE_STREAM_LIST_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of items accepted from one list invocation.
pub const MAX_LIST_ITEMS: usize = 10_000;
/// Maximum compressed package size accepted from a repository catalog.
pub const MAX_ADDON_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;
/// Prefix used when a structured operation failure crosses a textual host boundary.
pub const ADDON_OPERATION_ERROR_MARKER: &str = "ERSATZRS_ADDON_OPERATION_ERROR:";
pub const OPERATION_CODE_MISSING_SETTING: &str = "missing-setting";
pub const OPERATION_CODE_MISSING_SECRET: &str = "missing-secret";
pub const OPERATION_CODE_PROVIDER_UNREACHABLE: &str = "provider-unreachable";
pub const OPERATION_CODE_PROVIDER_REJECTED: &str = "provider-rejected";
pub const OPERATION_CODE_PROVIDER_RATE_LIMITED: &str = "provider-rate-limited";
pub const OPERATION_CODE_NOT_FOUND: &str = "not-found";
pub const OPERATION_CODE_UNSUPPORTED_URL: &str = "unsupported-url";
pub const OPERATION_CODE_FAILED: &str = "operation-failed";
const MAX_PACKAGE_VERSION_BYTES: usize = 128;
const MAX_RECORD_PARSE_DETAIL_BYTES: usize = 256;

/// Media kinds accepted from a provider-neutral add-on list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListItemKind {
    #[default]
    Movie,
    Show,
    Season,
    Episode,
    RemoteStream,
}

/// Provider-reported availability for an add-on supplied item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListItemAvailability {
    #[default]
    Available,
    Unavailable,
    Unknown,
}

/// Descriptive content classification for an add-on supplied stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListContentKind {
    #[default]
    Auto,
    TelevisionEpisode,
    Movie,
    MusicVideo,
    OtherVideo,
}

/// One bounded `media-list.list.v1` or `media-list.list.v2` NDJSON record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddonMediaListRecord {
    List {
        provider_id: String,
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    Item {
        provider_id: String,
        rank: i32,
        display_title: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        year: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        season: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        episode: Option<i32>,
        kind: AddonMediaListItemKind,
        #[serde(default, deserialize_with = "deserialize_vec_or_default")]
        guids: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_url: Option<String>,
        #[serde(default, skip_serializing_if = "availability_is_available")]
        availability: AddonMediaListItemAvailability,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        availability_reason: Option<String>,
        #[serde(default, skip_serializing_if = "content_kind_is_auto")]
        content_kind: AddonMediaListContentKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_seconds: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_provider_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fragment_start_seconds: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fragment_end_seconds: Option<u64>,
        #[serde(
            default,
            deserialize_with = "deserialize_vec_or_default",
            skip_serializing_if = "Vec::is_empty"
        )]
        additional_image_urls: Vec<String>,
    },
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip predicate signature
const fn availability_is_available(value: &AddonMediaListItemAvailability) -> bool {
    matches!(value, AddonMediaListItemAvailability::Available)
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip predicate signature
const fn content_kind_is_auto(value: &AddonMediaListContentKind) -> bool {
    matches!(value, AddonMediaListContentKind::Auto)
}

impl AddonMediaListRecord {
    /// Construct a list header with the required fields populated.
    #[must_use]
    pub fn list(provider_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::List {
            provider_id: provider_id.into(),
            name: name.into(),
            description: None,
        }
    }

    /// Construct a list item with safe defaults for optional v2 metadata.
    #[must_use]
    pub fn item(
        provider_id: impl Into<String>,
        rank: i32,
        display_title: impl Into<String>,
        title: impl Into<String>,
        kind: AddonMediaListItemKind,
    ) -> Self {
        Self::Item {
            provider_id: provider_id.into(),
            rank,
            display_title: display_title.into(),
            title: title.into(),
            year: None,
            season: None,
            episode: None,
            kind,
            guids: Vec::new(),
            source_url: None,
            availability: AddonMediaListItemAvailability::Available,
            availability_reason: None,
            content_kind: AddonMediaListContentKind::Auto,
            duration_seconds: None,
            parent_provider_id: None,
            fragment_start_seconds: None,
            fragment_end_seconds: None,
            additional_image_urls: Vec::new(),
        }
    }

    /// Validate the bounds and cross-field rules for one record.
    ///
    /// # Errors
    ///
    /// Returns an error when the record is not safe for host ingestion.
    pub fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::List {
                provider_id, name, ..
            } => {
                validate_provider_id(provider_id)?;
                if name.trim().is_empty() {
                    return invalid("media list name", "must not be empty");
                }
            }
            Self::Item {
                provider_id,
                rank,
                display_title,
                title,
                kind,
                source_url,
                availability_reason,
                parent_provider_id,
                fragment_start_seconds,
                fragment_end_seconds,
                additional_image_urls,
                ..
            } => {
                validate_provider_id(provider_id)?;
                if *rank < 0 {
                    return invalid("media list item rank", "must not be negative");
                }
                if display_title.trim().is_empty() || title.trim().is_empty() {
                    return invalid("media list item title", "must not be empty");
                }
                if *kind == AddonMediaListItemKind::RemoteStream
                    && source_url.as_deref().is_none_or(str::is_empty)
                {
                    return invalid(
                        "media list item source_url",
                        "is required for remote_stream items",
                    );
                }
                validate_availability_reason(availability_reason.as_deref())?;
                validate_fragment(
                    provider_id,
                    parent_provider_id.as_deref(),
                    *fragment_start_seconds,
                    *fragment_end_seconds,
                )?;
                if additional_image_urls
                    .iter()
                    .any(|url| url.trim().is_empty())
                {
                    return invalid(
                        "media list item additional_image_urls",
                        "must not contain empty URLs",
                    );
                }
            }
        }
        Ok(())
    }
}

/// One credited person in a Remote Stream list record.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteStreamPerson {
    pub name: String,
    pub role: String,
}

/// One provider entry emitted by `remote-stream.list.v1` or v2.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaylistEntry {
    pub id: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub original_title: Option<String>,
    #[serde(default)]
    pub sort_title: Option<String>,
    #[serde(default)]
    pub outline: Option<String>,
    #[serde(default)]
    pub tagline: Option<String>,
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
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub genres: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub people: Vec<RemoteStreamPerson>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub producers: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub original_broadcasters: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub broadcasters: Vec<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_vec_or_default",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub additional_image_urls: Vec<String>,
    #[serde(default)]
    pub parent_provider_id: Option<String>,
    #[serde(default)]
    pub fragment_start_seconds: Option<u64>,
    #[serde(default)]
    pub fragment_end_seconds: Option<u64>,
    #[serde(default)]
    pub is_live: bool,
    #[serde(default)]
    pub availability: AddonMediaListItemAvailability,
    #[serde(default)]
    pub availability_reason: Option<String>,
    #[serde(default)]
    pub content_kind: AddonMediaListContentKind,
    #[serde(default)]
    pub show_title: Option<String>,
    #[serde(default)]
    pub season: Option<i32>,
    #[serde(default)]
    pub episode: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub guids: Vec<String>,
}

/// Descriptive metadata for one already-enumerated entry, emitted by
/// `remote-stream.item.v1`.
///
/// Enumeration is expected to stay cheap, which for most providers means a
/// flat listing that carries identity and little else. This record carries the
/// descriptive fields such a listing cannot supply, for the entries the host
/// actually kept after applying item limits, filters, and exclusions. Every
/// field is optional: an omitted field leaves the enumerated value untouched,
/// so a provider may answer with only what it knows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaylistEntryMetadata {
    /// Correlates the response with the requested entry.
    pub provider_id: String,
    #[serde(default)]
    pub plot: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub content_rating: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub genres: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub people: Vec<RemoteStreamPerson>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub producers: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub original_broadcasters: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub broadcasters: Vec<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_default")]
    pub additional_image_urls: Vec<String>,
    /// Present when enumeration could not determine availability and the
    /// per-item response can.
    #[serde(default)]
    pub availability: Option<AddonMediaListItemAvailability>,
    #[serde(default)]
    pub availability_reason: Option<String>,
    /// Provider-supplied timestamp/title chapter markers. The host validates
    /// and expands these; the add-on remains responsible for provider parsing.
    #[serde(default)]
    pub chapter_input: Option<String>,
}

impl PlaylistEntryMetadata {
    /// Validate the bounded fields this record shares with a list entry.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider identity or any bounded descriptive
    /// field is outside its documented limit.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_provider_id(&self.provider_id)?;
        validate_availability_reason(self.availability_reason.as_deref())?;
        if self
            .chapter_input
            .as_ref()
            .is_some_and(|value| value.len() > MAX_CHAPTER_INPUT_BYTES)
        {
            return invalid(
                "remote stream item chapter_input",
                "exceeds the 64 KiB limit",
            );
        }
        Ok(())
    }

    /// Apply this metadata to its enumerated entry.
    ///
    /// An omitted or empty field never clears a value the listing already
    /// supplied, so enrichment can only add information.
    pub fn apply_to(self, entry: &mut PlaylistEntry) {
        fn fill<T>(target: &mut Option<T>, value: Option<T>) {
            if let Some(value) = value {
                *target = Some(value);
            }
        }
        fn fill_vec<T>(target: &mut Vec<T>, value: Vec<T>) {
            if !value.is_empty() {
                *target = value;
            }
        }

        fill(&mut entry.plot, self.plot);
        fill(&mut entry.year, self.year);
        fill(&mut entry.release_date, self.release_date);
        fill(&mut entry.content_rating, self.content_rating);
        fill(&mut entry.collection, self.collection);
        fill(&mut entry.thumbnail_url, self.thumbnail_url);
        fill(&mut entry.availability_reason, self.availability_reason);
        fill_vec(&mut entry.genres, self.genres);
        fill_vec(&mut entry.tags, self.tags);
        fill_vec(&mut entry.people, self.people);
        fill_vec(&mut entry.producers, self.producers);
        fill_vec(&mut entry.original_broadcasters, self.original_broadcasters);
        fill_vec(&mut entry.broadcasters, self.broadcasters);
        fill_vec(&mut entry.additional_image_urls, self.additional_image_urls);
        if let Some(availability) = self.availability {
            entry.availability = availability;
        }
    }
}

impl PlaylistEntry {
    /// Construct a Remote Stream list entry with safe optional defaults.
    #[must_use]
    pub fn new(id: impl Into<String>, url: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            title: title.into(),
            ..Self::default()
        }
    }

    /// Validate one Remote Stream list record before emitting or accepting it.
    ///
    /// # Errors
    ///
    /// Returns an error when required identity, URL, or bounds are invalid.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_provider_id(&self.id)?;
        if let Some(provider_id) = &self.provider_id {
            validate_provider_id(provider_id)?;
        }
        if self.url.trim().is_empty() {
            return invalid("remote stream URL", "must not be empty");
        }
        if self.title.trim().is_empty() {
            return invalid("remote stream title", "must not be empty");
        }
        validate_availability_reason(self.availability_reason.as_deref())?;
        validate_fragment(
            self.provider_id.as_deref().unwrap_or(&self.id),
            self.parent_provider_id.as_deref(),
            self.fragment_start_seconds,
            self.fragment_end_seconds,
        )?;
        if self
            .additional_image_urls
            .iter()
            .any(|url| url.trim().is_empty())
        {
            return invalid(
                "remote stream additional_image_urls",
                "must not contain empty URLs",
            );
        }
        Ok(())
    }
}

fn validate_fragment(
    provider_id: &str,
    parent_provider_id: Option<&str>,
    start_seconds: Option<u64>,
    end_seconds: Option<u64>,
) -> Result<(), ContractError> {
    match (parent_provider_id, start_seconds, end_seconds) {
        (None, None, None) => Ok(()),
        (Some(parent), Some(start), end) => {
            validate_provider_id(parent)?;
            if parent == provider_id {
                return invalid(
                    "fragment parent_provider_id",
                    "must differ from provider_id",
                );
            }
            if end.is_some_and(|end| end <= start) {
                return invalid("fragment end", "must be greater than start");
            }
            Ok(())
        }
        _ => invalid(
            "fragment bounds",
            "parent_provider_id and fragment_start_seconds are required together",
        ),
    }
}

fn deserialize_vec_or_default<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// Manifest validation failure.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    /// A required or constrained field is invalid.
    #[error("invalid {field}: {message}")]
    Invalid {
        /// Field or section name.
        field: &'static str,
        /// Safe validation detail.
        message: String,
    },
}

/// Supported standalone contract documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractDocumentKind {
    AddonManifest,
    RepositoryManifest,
    RepositoryIndex,
}

/// Parse or contract failure from the standalone validation entrypoint.
#[derive(Debug, thiserror::Error)]
pub enum ContractDocumentError {
    #[error("invalid {document}: {message}")]
    Parse {
        document: &'static str,
        message: String,
    },
    #[error(transparent)]
    Contract(#[from] ContractError),
}

/// Parse and validate one public add-on contract document.
///
/// This is the shared implementation used by the host-facing validator tools.
/// Filesystem publication rules such as icon existence and package-directory
/// naming deliberately remain outside this contract.
///
/// # Errors
///
/// Returns a bounded parse diagnostic or the exact contract validation error.
pub fn validate_contract_document(
    kind: ContractDocumentKind,
    contents: &str,
) -> Result<(), ContractDocumentError> {
    match kind {
        ContractDocumentKind::AddonManifest => {
            let manifest: AddonManifest = parse_toml_document("addon manifest", contents)?;
            manifest.validate()?;
        }
        ContractDocumentKind::RepositoryManifest => {
            let manifest: RepositoryManifest =
                parse_toml_document("repository manifest", contents)?;
            manifest.validate()?;
        }
        ContractDocumentKind::RepositoryIndex => {
            let index: AddonRepositoryIndex =
                serde_json::from_str(contents).map_err(|error| ContractDocumentError::Parse {
                    document: "repository index",
                    message: bounded_parse_message(&error.to_string()),
                })?;
            index.validate()?;
        }
    }
    Ok(())
}

fn parse_toml_document<T>(
    document: &'static str,
    contents: &str,
) -> Result<T, ContractDocumentError>
where
    T: serde::de::DeserializeOwned,
{
    toml::from_str(contents).map_err(|error| ContractDocumentError::Parse {
        document,
        message: bounded_parse_message(&error.to_string()),
    })
}

fn bounded_parse_message(message: &str) -> String {
    bound_utf8(message.lines().next().unwrap_or("parse failed"), 384)
}

/// A package manifest stored at the package root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonManifest {
    /// Manifest schema version.
    pub manifest_version: u32,
    /// Stable reverse-DNS add-on identifier.
    pub id: String,
    /// `SemVer` package version.
    pub version: Version,
    /// Localized display names, requiring en-US.
    pub name: BTreeMap<String, String>,
    /// Localized short summaries, requiring en-US.
    pub summary: BTreeMap<String, String>,
    /// Optional package-local icon rendered by the host.
    #[serde(default)]
    pub icon: Option<AddonPackageIcon>,
    /// SPDX license expression or identifier.
    pub license: String,
    /// Public source-code URL.
    pub source_url: String,
    /// Supported ErsatzRS host versions.
    pub host_version: VersionReq,
    /// Capabilities implemented by this package.
    #[serde(default)]
    pub capabilities: Vec<AddonCapability>,
    /// Other add-ons required or optionally integrated by this package.
    #[serde(default)]
    pub dependencies: Vec<AddonDependency>,
    /// Entrypoints selected by exact `RID` or the portable `any` fallback.
    pub entrypoints: BTreeMap<String, AddonEntrypoint>,
    /// Operator-visible privilege declaration. This is not an OS sandbox.
    #[serde(default)]
    pub permissions: AddonPermissions,
    /// Host-rendered configuration fields.
    #[serde(default)]
    pub settings: Vec<AddonSettingDefinition>,
    /// Optional host-managed storage for file-backed media-list libraries.
    #[serde(default)]
    pub media_list_storage: Option<AddonMediaListStorage>,
    /// Host-rendered declarative panels.
    #[serde(default)]
    pub panels: Vec<AddonPanel>,
}

impl AddonManifest {
    /// Validate cross-field invariants that serde alone cannot express.
    ///
    /// # Errors
    ///
    /// Returns a safe validation error when the package cannot be installed.
    pub fn validate(&self) -> Result<(), ContractError> {
        if !(1..=ADDON_MANIFEST_VERSION).contains(&self.manifest_version) {
            return invalid("manifest_version", "unsupported schema version");
        }
        validate_addon_id(&self.id)?;
        validate_package_version(&self.version)?;
        require_english(&self.name, "name")?;
        require_english(&self.summary, "summary")?;
        if self.manifest_version == 1 && self.icon.is_some() {
            return invalid("icon", "requires manifest schema version 2");
        }
        if let Some(icon) = &self.icon {
            icon.validate()?;
        }
        if self.license.trim().is_empty() {
            return invalid("license", "must not be empty");
        }
        if !is_https_url(&self.source_url) {
            return invalid("source_url", "must use HTTPS");
        }
        if self.capabilities.is_empty() {
            return invalid("capabilities", "at least one capability is required");
        }
        if !self.has_capability(CAPABILITY_ADDON_CHECK_V1) {
            return invalid(
                "capabilities",
                "executable add-ons must implement addon.check.v1",
            );
        }
        if self.entrypoints.is_empty() {
            return invalid("entrypoints", "at least one entrypoint is required");
        }

        let mut capability_ids = BTreeSet::new();
        for capability in &self.capabilities {
            validate_capability_id(&capability.id)?;
            if !capability_ids.insert(capability.id.as_str()) {
                return invalid("capabilities", "duplicate capability id");
            }
        }
        validate_dependencies(&self.id, &self.dependencies)?;
        for (rid, entrypoint) in &self.entrypoints {
            if rid != "any" && !is_supported_rid(rid) {
                return invalid("entrypoints", "unknown ErsatzRS RID");
            }
            entrypoint.validate()?;
        }
        self.permissions.validate()?;

        let mut setting_keys = BTreeSet::new();
        for setting in &self.settings {
            setting.validate(self.manifest_version)?;
            if !setting_keys.insert(setting.key.as_str()) {
                return invalid("settings", "duplicate setting key");
            }
        }
        if let Some(storage) = &self.media_list_storage {
            if self.manifest_version < 3 {
                return invalid("media_list_storage", "requires manifest schema version 3");
            }
            storage.validate(&self.settings)?;
            for capability in [CAPABILITY_REMOTE_STREAM_PLAY_V1] {
                if !self.has_capability(capability) {
                    return invalid("media_list_storage", format!("requires {capability}"));
                }
            }
            if !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V1)
                && !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V2)
                && !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V3)
            {
                return invalid(
                    "media_list_storage",
                    "requires media-list.list.v1 or media-list.list.v2",
                );
            }
            if !self.has_capability(CAPABILITY_REMOTE_STREAM_LIST_V1)
                && !self.has_capability(CAPABILITY_REMOTE_STREAM_LIST_V2)
                && !self.has_capability(CAPABILITY_REMOTE_STREAM_LIST_V3)
            {
                return invalid(
                    "media_list_storage",
                    "requires remote-stream.list.v1 or remote-stream.list.v2",
                );
            }
        }

        let mut panel_ids = BTreeSet::new();
        if !self.panels.is_empty() && !self.has_capability(CAPABILITY_DECLARATIVE_PANEL_V1) {
            return invalid("panels", "declarative panels require declarative-panel.v1");
        }
        for panel in &self.panels {
            panel.validate()?;
            if !panel_ids.insert(panel.id.as_str()) {
                return invalid("panels", "duplicate panel id");
            }
        }
        Ok(())
    }

    /// Return the entrypoint for an exact `RID`, then the portable fallback.
    #[must_use]
    pub fn entrypoint_for(&self, rid: &str) -> Option<&AddonEntrypoint> {
        self.entrypoints
            .get(rid)
            .or_else(|| self.entrypoints.get("any"))
    }

    /// Whether the package declares a capability.
    #[must_use]
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|item| item.id == capability)
    }
}

/// Host-owned file storage used to materialize add-on lists as local Remote
/// Stream libraries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonMediaListStorage {
    /// Manifest setting that allows the operator to override the root path.
    pub path_setting: String,
    /// Safe folder below the ErsatzRS profile used when no override is stored.
    pub default_subdirectory: String,
    /// Stable operator-facing name for the managed local media source.
    pub source_name: String,
}

impl AddonMediaListStorage {
    fn validate(&self, settings: &[AddonSettingDefinition]) -> Result<(), ContractError> {
        let setting = settings
            .iter()
            .find(|setting| setting.key == self.path_setting)
            .ok_or_else(|| ContractError::Invalid {
                field: "media_list_storage.path_setting",
                message: "must reference a declared setting".to_owned(),
            })?;
        if setting.kind != AddonSettingKind::Path {
            return invalid(
                "media_list_storage.path_setting",
                "must reference a path setting",
            );
        }
        if setting.default.is_some() {
            return invalid(
                "media_list_storage.path_setting",
                "host-managed path defaults must not declare a static default",
            );
        }
        if setting.required {
            return invalid(
                "media_list_storage.path_setting",
                "must be optional so the host default can be used",
            );
        }
        if !is_safe_single_component(&self.default_subdirectory) {
            return invalid(
                "media_list_storage.default_subdirectory",
                "must be one safe folder name",
            );
        }
        if self.source_name.trim().is_empty()
            || self.source_name.len() > 128
            || self.source_name.chars().any(char::is_control)
        {
            return invalid(
                "media_list_storage.source_name",
                "must be a non-empty display-safe name",
            );
        }
        Ok(())
    }
}

/// Optional package-local add-on icon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonPackageIcon {
    /// Relative path inside the extracted package.
    pub path: String,
    /// Exact supported media type.
    pub media_type: AddonIconMediaType,
}

impl AddonPackageIcon {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_safe_relative_path(&self.path) {
            return invalid("icon.path", "must stay inside the package");
        }
        let expected_extension = match self.media_type {
            AddonIconMediaType::Png => ".png",
            AddonIconMediaType::Webp => ".webp",
        };
        if !self.path.to_ascii_lowercase().ends_with(expected_extension) {
            return invalid("icon.path", "extension does not match media type");
        }
        Ok(())
    }
}

/// Image formats accepted for add-on icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonIconMediaType {
    Png,
    Webp,
}

impl AddonIconMediaType {
    #[must_use]
    pub const fn as_mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }
}

/// One versioned executable capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonCapability {
    /// Capability identifier ending in `.v<major>`.
    pub id: String,
}

/// One version-constrained add-on dependency.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonDependency {
    /// Stable reverse-DNS add-on identity.
    pub id: String,
    /// Accepted installed versions.
    pub version: VersionReq,
    /// Optional integrations do not block readiness when absent.
    #[serde(default)]
    pub optional: bool,
}

/// How the host starts one package-local entrypoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonEntrypoint {
    /// Relative package-local executable or script path.
    pub path: String,
    /// Host-controlled interpreter kind.
    pub kind: AddonEntrypointKind,
}

impl AddonEntrypoint {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_safe_relative_path(&self.path) {
            return invalid("entrypoint.path", "must stay inside the package");
        }
        Ok(())
    }
}

/// Supported entrypoint launchers. Arbitrary interpreters are deliberately absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonEntrypointKind {
    /// Native executable contained in the package.
    Native,
    /// POSIX sh script.
    PosixShell,
    /// Windows batch script.
    WindowsBatch,
}

/// Operator-visible permissions requested by an add-on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AddonPermissions {
    /// Network host patterns the add-on expects to contact.
    pub network_hosts: Vec<String>,
    /// Whether media input paths are supplied to the process.
    pub media_read: bool,
    /// Whether the host supplies its managed `FFmpeg`/`ffprobe` paths.
    pub managed_ffmpeg: bool,
    /// External executable names expected by the package.
    pub external_commands: Vec<String>,
}

impl AddonPermissions {
    /// Validate bounded, display-safe permission declarations.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid host patterns or executable names.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.network_hosts.len() > 100 || self.external_commands.len() > 100 {
            return invalid("permissions", "too many declared permissions");
        }
        if self.network_hosts.iter().any(|host| {
            host.is_empty()
                || host.len() > 255
                || host.contains("//")
                || host.contains('/')
                || host.contains(char::is_whitespace)
                || host.chars().any(char::is_control)
        }) {
            return invalid(
                "permissions.network_hosts",
                "contains an invalid host pattern",
            );
        }
        if self.external_commands.iter().any(|command| {
            command.is_empty()
                || command.len() > 128
                || command.contains(['/', '\\', ':'])
                || command.contains(char::is_whitespace)
                || command.chars().any(char::is_control)
        }) {
            return invalid(
                "permissions.external_commands",
                "contains an invalid executable name",
            );
        }
        Ok(())
    }
}

/// One host-rendered configuration field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonSettingDefinition {
    /// Stable uppercase snake-case key used for process environment projection.
    pub key: String,
    /// Localized label map, requiring en-US.
    pub label: BTreeMap<String, String>,
    /// Value kind and rendering behavior.
    pub kind: AddonSettingKind,
    /// Whether enabling requires a configured value.
    #[serde(default)]
    pub required: bool,
    /// Optional non-secret default.
    pub default: Option<String>,
    /// Allowed values for enum settings.
    #[serde(default)]
    pub options: Vec<String>,
    /// Safe host-rendered default for a secret *reference*, never its value.
    #[serde(default)]
    pub suggested_reference: Option<AddonSuggestedSecretReference>,
}

impl AddonSettingDefinition {
    fn validate(&self, manifest_version: u32) -> Result<(), ContractError> {
        if !is_setting_key(&self.key) {
            return invalid("settings.key", "must be uppercase snake case");
        }
        require_english(&self.label, "settings.label")?;
        if self.kind == AddonSettingKind::SecretReference && self.default.is_some() {
            return invalid("settings.default", "secret references cannot have defaults");
        }
        if let Some(reference) = &self.suggested_reference {
            if manifest_version < 3 {
                return invalid(
                    "settings.suggested_reference",
                    "requires manifest schema version 3",
                );
            }
            if self.kind != AddonSettingKind::SecretReference {
                return invalid(
                    "settings.suggested_reference",
                    "only secret references can have a suggested reference",
                );
            }
            reference.validate()?;
        }
        if self.kind == AddonSettingKind::Enum && self.options.is_empty() {
            return invalid("settings.options", "enum settings require options");
        }
        Ok(())
    }
}

/// Non-sensitive location suggested by an add-on for a protected setting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonSuggestedSecretReference {
    /// Secret-store location type.
    pub kind: AddonSuggestedSecretReferenceKind,
    /// Environment-variable name or file path, never the resolved value.
    pub reference: String,
}

impl AddonSuggestedSecretReference {
    fn validate(&self) -> Result<(), ContractError> {
        match self.kind {
            AddonSuggestedSecretReferenceKind::Environment
                if !is_environment_reference_name(&self.reference) =>
            {
                invalid(
                    "settings.suggested_reference.reference",
                    "environment reference must be uppercase snake case",
                )
            }
            AddonSuggestedSecretReferenceKind::File if self.reference.trim().is_empty() => invalid(
                "settings.suggested_reference.reference",
                "file reference must not be empty",
            ),
            _ => Ok(()),
        }
    }
}

/// Supported safe locations for an add-on secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonSuggestedSecretReferenceKind {
    Environment,
    File,
}

/// Supported host-rendered setting types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonSettingKind {
    String,
    Boolean,
    Integer,
    Enum,
    Path,
    Executable,
    /// Reference to an environment variable or file, never a raw secret.
    SecretReference,
}

/// One declarative page rendered entirely by the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonPanel {
    /// Stable route-local identifier.
    pub id: String,
    /// Localized panel title.
    pub title: BTreeMap<String, String>,
    /// Ordered safe blocks.
    pub blocks: Vec<AddonPanelBlock>,
}

impl AddonPanel {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_lower_identifier(&self.id) {
            return invalid("panels.id", "must be lowercase ASCII with dashes");
        }
        require_english(&self.title, "panels.title")?;
        if self.blocks.len() > 100 {
            return invalid("panels.blocks", "too many blocks");
        }
        for block in &self.blocks {
            block.validate()?;
        }
        Ok(())
    }
}

/// Safe panel elements. No executable markup or styling is accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddonPanelBlock {
    Text {
        text: BTreeMap<String, String>,
    },
    Status {
        label: BTreeMap<String, String>,
        code: String,
    },
    KeyValue {
        label: BTreeMap<String, String>,
        value: String,
    },
    Link {
        label: BTreeMap<String, String>,
        url: String,
    },
}

impl AddonPanelBlock {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Text { text } => require_english(text, "panels.blocks.text"),
            Self::Status { label, code } => {
                require_english(label, "panels.blocks.label")?;
                if !is_lower_identifier(code) {
                    return invalid("panels.blocks.code", "must be a lowercase identifier");
                }
                Ok(())
            }
            Self::KeyValue { label, value } => {
                require_english(label, "panels.blocks.label")?;
                if value.len() > 4_096 || value.chars().any(char::is_control) {
                    return invalid("panels.blocks.value", "contains unsafe or excessive text");
                }
                Ok(())
            }
            Self::Link { label, url } => {
                require_english(label, "panels.blocks.label")?;
                if !is_https_url(url) {
                    return invalid("panels.blocks.url", "must use HTTPS without userinfo");
                }
                Ok(())
            }
        }
    }
}

/// One signed repository catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonRepositoryIndex {
    pub schema_version: u32,
    pub repository_id: String,
    pub repository_name: String,
    pub sequence: u64,
    pub generated_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub addons: Vec<CatalogAddon>,
}

impl AddonRepositoryIndex {
    /// Validate repository-index structure independent of one trust record and
    /// the current wall clock.
    ///
    /// # Errors
    ///
    /// Returns an error when schema, identity, validity interval, add-on, icon,
    /// or package metadata violates the public repository contract.
    pub fn validate(&self) -> Result<(), ContractError> {
        if !(1..=ADDON_REPOSITORY_SCHEMA_VERSION).contains(&self.schema_version) {
            return invalid("schema_version", "unsupported repository schema version");
        }
        validate_addon_id(&self.repository_id)?;
        if self.repository_name.trim().is_empty() || self.repository_name.len() > 200 {
            return invalid("repository_name", "must be between 1 and 200 characters");
        }
        let generated_at =
            chrono::DateTime::parse_from_rfc3339(&self.generated_at).map_err(|_| {
                ContractError::Invalid {
                    field: "generated_at",
                    message: "must be RFC 3339".to_owned(),
                }
            })?;
        let expires_at = chrono::DateTime::parse_from_rfc3339(&self.expires_at).map_err(|_| {
            ContractError::Invalid {
                field: "expires_at",
                message: "must be RFC 3339".to_owned(),
            }
        })?;
        if expires_at <= generated_at {
            return invalid("expires_at", "must be later than generated_at");
        }

        let mut addon_ids = BTreeSet::new();
        for addon in &self.addons {
            if self.schema_version == 1 && addon.icon.is_some() {
                return invalid("addons.icon", "requires repository schema version 2");
            }
            addon.validate()?;
            if !addon_ids.insert(addon.id.as_str()) {
                return invalid("addons", "duplicate add-on identifier");
            }
        }
        Ok(())
    }
}

/// Add-on metadata shown before package download.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogAddon {
    pub id: String,
    pub version: Version,
    pub name: BTreeMap<String, String>,
    pub summary: BTreeMap<String, String>,
    #[serde(default)]
    pub icon: Option<CatalogAddonIcon>,
    pub license: String,
    pub source_url: String,
    pub host_version: VersionReq,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<AddonDependency>,
    #[serde(default)]
    pub permissions: AddonPermissions,
    pub packages: BTreeMap<String, CatalogPackage>,
}

impl CatalogAddon {
    /// Validate signed metadata shown before a package is downloaded.
    ///
    /// # Errors
    ///
    /// Returns an error when catalog metadata violates the v1 contract.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_addon_id(&self.id)?;
        validate_package_version(&self.version)?;
        require_english(&self.name, "name")?;
        require_english(&self.summary, "summary")?;
        if let Some(icon) = &self.icon {
            icon.validate()?;
        }
        if self.license.trim().is_empty() {
            return invalid("license", "must not be empty");
        }
        if !is_https_url(&self.source_url) {
            return invalid("source_url", "must use HTTPS without userinfo");
        }
        let mut capability_ids = BTreeSet::new();
        for capability in &self.capabilities {
            validate_capability_id(capability)?;
            if !capability_ids.insert(capability.as_str()) {
                return invalid("capabilities", "duplicate capability id");
            }
        }
        validate_dependencies(&self.id, &self.dependencies)?;
        self.permissions.validate()?;
        if self.packages.is_empty() {
            return invalid("packages", "at least one package is required");
        }
        for (rid, package) in &self.packages {
            if rid != "any" && !is_supported_rid(rid) {
                return invalid("packages", "unknown ErsatzRS RID");
            }
            if !is_package_url(&package.url) {
                return invalid(
                    "packages.url",
                    "must use HTTPS without userinfo, query, or fragment",
                );
            }
            if package.size == 0 {
                return invalid("packages.size", "must be positive");
            }
            if package.size > MAX_ADDON_PACKAGE_BYTES {
                return invalid("packages.size", "exceeds the 256 MiB package limit");
            }
            if !is_sha256_hex(&package.sha256) {
                return invalid("packages.sha256", "must be a hexadecimal SHA-256 digest");
            }
        }
        Ok(())
    }
}

/// Signed remote icon metadata for a catalog entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogAddonIcon {
    pub url: String,
    pub media_type: AddonIconMediaType,
    pub size: u64,
    pub sha256: String,
}

impl CatalogAddonIcon {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_https_url(&self.url) {
            return invalid("icon.url", "must use HTTPS without userinfo");
        }
        if self.size == 0 || self.size > 2 * 1024 * 1024 {
            return invalid("icon.size", "must be between 1 byte and 2 MiB");
        }
        if !is_sha256_hex(&self.sha256) {
            return invalid("icon.sha256", "must be lowercase SHA-256 hex");
        }
        Ok(())
    }
}

/// Persisted provenance assigned when an add-on is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonTrustTier {
    Verified,
    Unverified,
    Sideloaded,
}

impl AddonTrustTier {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Sideloaded => "sideloaded",
        }
    }
}

/// Strict manifest carried by a repository installation ZIP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryManifest {
    pub id: String,
    pub name: String,
    pub index_url: String,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub description: BTreeMap<String, String>,
}

impl RepositoryManifest {
    /// Validate a repository-package manifest before it becomes a trust record.
    ///
    /// # Errors
    ///
    /// Returns an error when the identity, display metadata, index URL, or
    /// localized description violates the repository-package contract.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_addon_id(&self.id)?;
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return invalid("name", "must be between 1 and 200 characters");
        }
        if !is_https_url(&self.index_url) {
            return invalid("index_url", "must use HTTPS without userinfo");
        }
        if !self.description.is_empty() {
            require_english(&self.description, "description")?;
        }
        Ok(())
    }
}

/// One immutable package asset selected by `RID` or `any`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackage {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Result of addon.check.v1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonCheckResult {
    pub status: AddonCheckStatus,
    pub code: String,
    pub message: String,
}

/// One bounded structured failure emitted as the final stderr line by `list`
/// and `play` operations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonOperationError {
    pub code: String,
    pub message: String,
}

impl AddonOperationError {
    #[must_use]
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: sanitize_addon_diagnostic_code(code),
            message: bound_utf8(&message.into(), MAX_OPERATION_ERROR_MESSAGE_BYTES),
        }
    }

    /// Validate an add-on-authored operation diagnostic before the host trusts
    /// its code as a localization key.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-canonical code or an overlong message.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.code != sanitize_addon_diagnostic_code(&self.code) {
            return invalid(
                "operation error code",
                "must be 1 to 128 lowercase ASCII code characters",
            );
        }
        if self.message.len() > MAX_OPERATION_ERROR_MESSAGE_BYTES {
            return invalid("operation error message", "exceeds the 1024-byte limit");
        }
        Ok(())
    }
}

impl std::fmt::Display for AddonOperationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let json = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        write!(formatter, "{ADDON_OPERATION_ERROR_MARKER}{json}")
    }
}

/// Normalize one diagnostic code using the same vocabulary rules as readiness
/// check codes.
#[must_use]
pub fn sanitize_addon_diagnostic_code(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| character.to_ascii_lowercase())
        .filter(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_' | '.')
        })
        .take(128)
        .collect::<String>();
    if sanitized.is_empty() {
        "invalid-operation-code".to_owned()
    } else {
        sanitized
    }
}

fn bound_utf8(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end].to_owned()
}

/// Stable readiness state returned by an add-on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonCheckStatus {
    Ready,
    Warning,
    Unavailable,
}

impl AddonCheckStatus {
    /// The persisted spelling, identical to the serialized wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Warning => "warning",
            Self::Unavailable => "unavailable",
        }
    }

    /// Parse a persisted readiness status, returning `None` when the stored
    /// value predates the column or is not a recognized status.
    #[must_use]
    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "ready" => Some(Self::Ready),
            "warning" => Some(Self::Warning),
            "unavailable" => Some(Self::Unavailable),
            _ => None,
        }
    }
}

/// Supported release `RID`s shared with the host runtime matrix.
#[must_use]
pub fn is_supported_rid(value: &str) -> bool {
    matches!(
        value,
        "linux-x64" | "linux-arm64" | "linux-arm" | "osx-x64" | "osx-arm64" | "win-x64"
    )
}

/// Validate and canonicalize one operator-supplied source URL against an
/// add-on's declared network hosts.
///
/// Empty host declarations and `*` allow every HTTP(S) host. Exact patterns
/// match one host; patterns beginning with `.` or `*.` match subdomains only.
///
/// # Errors
///
/// Returns the canonical URL serialization that must be forwarded to the
/// add-on. Returns an error when the URL is not bounded HTTP(S), contains
/// userinfo, or targets a host outside a restrictive declaration.
pub fn validate_operator_source_url(
    value: &str,
    network_hosts: &[String],
) -> Result<String, ContractError> {
    let value = value.trim();
    if value.len() > 2_048 {
        return invalid(
            "operator source URL",
            "must use HTTP or HTTPS without credentials",
        );
    }
    let parsed = url::Url::parse(value).map_err(|_| ContractError::Invalid {
        field: "operator source URL",
        message: "must use HTTP or HTTPS without credentials".to_owned(),
    })?;
    let host = parsed.host_str().ok_or_else(|| ContractError::Invalid {
        field: "operator source URL",
        message: "must use HTTP or HTTPS without credentials".to_owned(),
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return invalid(
            "operator source URL",
            "must use HTTP or HTTPS without credentials",
        );
    }
    if !network_hosts.is_empty()
        && !network_hosts
            .iter()
            .any(|pattern| network_host_pattern_matches(pattern, host))
    {
        return invalid(
            "operator source URL",
            format!("host '{host}' is not declared by this add-on"),
        );
    }
    Ok(parsed.to_string())
}

fn network_host_pattern_matches(pattern: &str, host: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let pattern = pattern.to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    let suffix = pattern
        .strip_prefix("*.")
        .or_else(|| pattern.strip_prefix('.'));
    match suffix {
        Some(suffix) => host
            .strip_suffix(suffix)
            .and_then(|prefix| prefix.strip_suffix('.'))
            .is_some_and(|subdomain| !subdomain.is_empty()),
        None => host == pattern,
    }
}

/// Parse and validate captured `media-list.list.v1` or v2 NDJSON output.
///
/// This function is intended for both host ingestion and add-on conformance
/// tests. It validates record order, per-record bounds, and list-wide identity
/// and rank uniqueness.
///
/// # Errors
///
/// Returns an error when the bytes are not valid UTF-8 NDJSON or violate the
/// bounded media-list contract.
pub fn conform_media_list_output(
    output: &[u8],
) -> Result<Vec<AddonMediaListRecord>, ContractError> {
    if output.len() > MAX_MEDIA_LIST_OUTPUT_BYTES {
        return invalid("media list output", "exceeds the 16 MiB limit");
    }
    let text = std::str::from_utf8(output).map_err(|_| ContractError::Invalid {
        field: "media list output",
        message: "must be UTF-8".to_owned(),
    })?;
    let mut records = Vec::new();
    let mut provider_ids = BTreeSet::new();
    let mut ranks = BTreeSet::new();
    for (index, line) in text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let record = serde_json::from_str::<AddonMediaListRecord>(line)
            .map_err(|error| record_parse_error("media list output", index, &error))?;
        record.validate()?;
        match &record {
            AddonMediaListRecord::List { .. } if index == 0 => {}
            AddonMediaListRecord::List { .. } => {
                return invalid("media list output", "list header may only appear first");
            }
            AddonMediaListRecord::Item {
                provider_id, rank, ..
            } if index > 0 => {
                if records.len() > MAX_LIST_ITEMS {
                    return invalid("media list output", "contains more than 10000 items");
                }
                if !provider_ids.insert(provider_id.clone()) || !ranks.insert(*rank) {
                    return invalid(
                        "media list output",
                        "item provider_id and rank must be unique",
                    );
                }
            }
            AddonMediaListRecord::Item { .. } => {
                return invalid("media list output", "first record must be a list header");
            }
        }
        records.push(record);
    }
    if records.is_empty() {
        return invalid("media list output", "is missing a list header");
    }
    Ok(records)
}

/// Parse and validate captured `remote-stream.list.v1` or v2 NDJSON output.
///
/// # Errors
///
/// Returns an error when the bytes are not valid UTF-8 NDJSON, an entry is
/// invalid, or case-insensitive provider identities are duplicated.
pub fn conform_remote_stream_list_output(
    output: &[u8],
) -> Result<Vec<PlaylistEntry>, ContractError> {
    if output.len() > MAX_REMOTE_STREAM_LIST_OUTPUT_BYTES {
        return invalid("remote stream list output", "exceeds the 8 MiB limit");
    }
    let text = std::str::from_utf8(output).map_err(|_| ContractError::Invalid {
        field: "remote stream list output",
        message: "must be UTF-8".to_owned(),
    })?;
    let mut entries = Vec::new();
    let mut ids = BTreeSet::new();
    for (index, line) in text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let mut entry = serde_json::from_str::<PlaylistEntry>(line)
            .map_err(|error| record_parse_error("remote stream list output", index, &error))?;
        let untrimmed_id = std::mem::take(&mut entry.id);
        untrimmed_id.trim().clone_into(&mut entry.id);
        let untrimmed_url = std::mem::take(&mut entry.url);
        untrimmed_url.trim().clone_into(&mut entry.url);
        let untrimmed_title = std::mem::take(&mut entry.title);
        untrimmed_title.trim().clone_into(&mut entry.title);
        entry.validate()?;
        if entry
            .year
            .is_some_and(|year| !(1..=chrono::Utc::now().year() + 5).contains(&year))
        {
            return invalid("remote stream year", "must be valid host metadata");
        }
        if !ids.insert(entry.id.to_ascii_lowercase()) {
            return invalid(
                "remote stream list output",
                "entry duplicates an earlier entry id",
            );
        }
        entries.push(entry);
        if entries.len() > MAX_LIST_ITEMS {
            return invalid(
                "remote stream list output",
                "contains more than 10000 entries",
            );
        }
    }
    Ok(entries)
}

/// Parse and validate captured `remote-stream.item.v1` NDJSON output.
///
/// The response carries at most one record per requested provider identity and
/// may omit an entry entirely when the provider knows nothing further about it.
///
/// # Errors
///
/// Returns an error when the bytes are not valid UTF-8 NDJSON, a record is
/// invalid, or case-insensitive provider identities are duplicated.
pub fn conform_remote_stream_item_output(
    output: &[u8],
) -> Result<Vec<PlaylistEntryMetadata>, ContractError> {
    conform_remote_stream_item_output_for_capability(output, false)
}

/// Parse `remote-stream.item.v1` or v2 output with version-specific fields.
///
/// # Errors
///
/// Returns an error for malformed output or when v2 chapter text is emitted
/// against the frozen v1 contract.
pub fn conform_remote_stream_item_output_for_capability(
    output: &[u8],
    supports_chapter_input: bool,
) -> Result<Vec<PlaylistEntryMetadata>, ContractError> {
    if output.len() > MAX_REMOTE_STREAM_LIST_OUTPUT_BYTES {
        return invalid("remote stream item output", "exceeds the 8 MiB limit");
    }
    let text = std::str::from_utf8(output).map_err(|_| ContractError::Invalid {
        field: "remote stream item output",
        message: "must be UTF-8".to_owned(),
    })?;
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    for (index, line) in text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        let mut record = serde_json::from_str::<PlaylistEntryMetadata>(line)
            .map_err(|error| record_parse_error("remote stream item output", index, &error))?;
        let untrimmed = std::mem::take(&mut record.provider_id);
        untrimmed.trim().clone_into(&mut record.provider_id);
        record.validate()?;
        if !supports_chapter_input && record.chapter_input.is_some() {
            return invalid(
                "remote stream item chapter_input",
                "requires remote-stream.item.v2",
            );
        }
        if record
            .year
            .is_some_and(|year| !(1..=chrono::Utc::now().year() + 5).contains(&year))
        {
            return invalid("remote stream item year", "must be valid host metadata");
        }
        if !ids.insert(record.provider_id.to_ascii_lowercase()) {
            return invalid(
                "remote stream item output",
                "record duplicates an earlier provider id",
            );
        }
        records.push(record);
        if records.len() > MAX_LIST_ITEMS {
            return invalid(
                "remote stream item output",
                "contains more than 10000 records",
            );
        }
    }
    Ok(records)
}

fn record_parse_error(
    field: &'static str,
    index: usize,
    error: &serde_json::Error,
) -> ContractError {
    let raw = error.to_string();
    let message = if raw.starts_with("unknown field ") {
        let detail = raw
            .split_once(", expected")
            .map_or(raw.as_str(), |(detail, _)| detail);
        format!(
            "line {} contains {}",
            index + 1,
            bound_utf8(detail, MAX_RECORD_PARSE_DETAIL_BYTES)
        )
    } else {
        format!("line {} is not a valid record", index + 1)
    };
    ContractError::Invalid { field, message }
}

fn validate_provider_id(value: &str) -> Result<(), ContractError> {
    if value.trim().is_empty() || value.len() > MAX_PROVIDER_ID_BYTES {
        return invalid(
            "provider_id",
            "must be non-empty and at most 384 UTF-8 bytes",
        );
    }
    Ok(())
}

fn validate_availability_reason(value: Option<&str>) -> Result<(), ContractError> {
    if value.is_some_and(|reason| reason.len() > MAX_AVAILABILITY_REASON_BYTES) {
        return invalid("availability_reason", "exceeds the 2048-byte limit");
    }
    Ok(())
}

/// Validate a reverse-DNS add-on identifier.
///
/// # Errors
///
/// Returns a safe validation error when the identifier is not stable/safe.
pub fn validate_addon_id(value: &str) -> Result<(), ContractError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if value.len() > 255
        || parts.len() < 3
        || parts.iter().any(|part| {
            part.is_empty()
                || part.starts_with('-')
                || part.ends_with('-')
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
    {
        return invalid("id", "must be a lowercase reverse-DNS identifier");
    }
    Ok(())
}

fn validate_package_version(version: &Version) -> Result<(), ContractError> {
    if version.to_string().len() > MAX_PACKAGE_VERSION_BYTES {
        return invalid("version", "serialized SemVer is too long");
    }
    Ok(())
}

fn validate_dependencies(
    addon_id: &str,
    dependencies: &[AddonDependency],
) -> Result<(), ContractError> {
    if dependencies.len() > 100 {
        return invalid("dependencies", "too many dependencies");
    }
    let mut ids = BTreeSet::new();
    for dependency in dependencies {
        validate_addon_id(&dependency.id)?;
        if dependency.id == addon_id {
            return invalid("dependencies", "an add-on cannot depend on itself");
        }
        if !ids.insert(dependency.id.as_str()) {
            return invalid("dependencies", "duplicate dependency id");
        }
    }
    Ok(())
}

fn validate_capability_id(value: &str) -> Result<(), ContractError> {
    if value.len() > 128 {
        return invalid("capability.id", "is too long");
    }
    let Some((name, version)) = value.rsplit_once(".v") else {
        return invalid("capability.id", "must end in .v<major>");
    };
    if name.is_empty()
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
        || version.parse::<u32>().is_err()
    {
        return invalid(
            "capability.id",
            "contains unsupported characters or version",
        );
    }
    Ok(())
}

fn require_english(
    values: &BTreeMap<String, String>,
    field: &'static str,
) -> Result<(), ContractError> {
    if values
        .get("en-US")
        .is_none_or(|value| value.trim().is_empty())
    {
        return invalid(field, "requires a non-empty en-US value");
    }
    Ok(())
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1_024
        && !value.starts_with(['/', '\\'])
        && !value.contains(':')
        && value
            .split(['/', '\\'])
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn is_safe_single_component(value: &str) -> bool {
    is_safe_relative_path(value) && !value.contains(['/', '\\'])
}

fn is_setting_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('_')
        && !value.ends_with('_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_environment_reference_name(value: &str) -> bool {
    is_setting_key(value) && !value.starts_with(|character: char| character.is_ascii_digit())
}

fn is_lower_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_https_url(value: &str) -> bool {
    value.len() <= 2_048
        && url::Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
}

fn is_package_url(value: &str) -> bool {
    is_https_url(value)
        && url::Url::parse(value).is_ok_and(|url| url.query().is_none() && url.fragment().is_none())
}

fn invalid<T>(field: &'static str, message: impl Into<String>) -> Result<T, ContractError> {
    Err(ContractError::Invalid {
        field,
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn localized(value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("en-US".to_owned(), value.to_owned())])
    }

    fn valid_manifest() -> AddonManifest {
        AddonManifest {
            manifest_version: 1,
            id: "org.ersatzrs.addon.fixture".to_owned(),
            version: Version::new(1, 0, 0),
            name: localized("Fixture"),
            summary: localized("Fixture add-on"),
            icon: None,
            license: "GPL-3.0-or-later".to_owned(),
            source_url: "https://example.test/source".to_owned(),
            host_version: VersionReq::STAR,
            capabilities: vec![AddonCapability {
                id: CAPABILITY_ADDON_CHECK_V1.to_owned(),
            }],
            dependencies: Vec::new(),
            entrypoints: BTreeMap::from([(
                "any".to_owned(),
                AddonEntrypoint {
                    path: "bin/fixture".to_owned(),
                    kind: AddonEntrypointKind::Native,
                },
            )]),
            permissions: AddonPermissions::default(),
            settings: Vec::new(),
            media_list_storage: None,
            panels: Vec::new(),
        }
    }

    #[test]
    fn manifest_rejects_paths_outside_the_package() {
        let mut manifest = valid_manifest();
        manifest.entrypoints.get_mut("any").unwrap().path = "../escape".to_owned();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn schema_v1_remains_readable_but_cannot_declare_an_icon() {
        let mut manifest = valid_manifest();
        manifest.icon = Some(AddonPackageIcon {
            path: "icon.png".to_owned(),
            media_type: AddonIconMediaType::Png,
        });
        assert!(manifest.validate().is_err());

        manifest.manifest_version = 2;
        manifest.validate().expect("schema v2 icon");
    }

    #[test]
    fn schema_v3_secret_setting_can_suggest_only_a_reference_location() {
        let mut manifest = valid_manifest();
        manifest.manifest_version = 3;
        manifest.settings.push(AddonSettingDefinition {
            key: "CLIENT_ID".to_owned(),
            label: localized("Client ID"),
            kind: AddonSettingKind::SecretReference,
            required: true,
            default: None,
            options: Vec::new(),
            suggested_reference: Some(AddonSuggestedSecretReference {
                kind: AddonSuggestedSecretReferenceKind::Environment,
                reference: "TRAKT__CLIENTID".to_owned(),
            }),
        });
        manifest.validate().expect("safe suggested reference");

        manifest.manifest_version = 1;
        assert!(manifest.validate().is_err());

        manifest.manifest_version = 2;
        assert!(manifest.validate().is_err());

        manifest.manifest_version = 3;

        manifest.settings[0]
            .suggested_reference
            .as_mut()
            .unwrap()
            .reference = "literal value".to_owned();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn schema_v3_media_list_storage_requires_the_complete_stream_contract() {
        let mut manifest = valid_manifest();
        manifest.manifest_version = 3;
        manifest.settings.push(AddonSettingDefinition {
            key: "MEDIA_STORAGE_PATH".to_owned(),
            label: localized("Media storage location"),
            kind: AddonSettingKind::Path,
            required: false,
            default: None,
            options: Vec::new(),
            suggested_reference: None,
        });
        manifest.media_list_storage = Some(AddonMediaListStorage {
            path_setting: "MEDIA_STORAGE_PATH".to_owned(),
            default_subdirectory: "fixture_media".to_owned(),
            source_name: "fixture".to_owned(),
        });
        assert!(manifest.validate().is_err());

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
            .validate()
            .expect("complete managed storage contract");

        manifest.manifest_version = 2;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn repository_package_manifest_accepts_an_optional_key() {
        let manifest = RepositoryManifest {
            id: "org.example.addons".to_owned(),
            name: "Example add-ons".to_owned(),
            index_url: "https://example.test/index-v2.json".to_owned(),
            public_key: None,
            description: localized("Example repository"),
        };
        manifest.validate().expect("valid repository package");
    }

    fn valid_repository_index() -> AddonRepositoryIndex {
        AddonRepositoryIndex {
            schema_version: 2,
            repository_id: "org.example.addons".to_owned(),
            repository_name: "Example add-ons".to_owned(),
            sequence: 1,
            generated_at: "2026-08-16T00:00:00Z".to_owned(),
            expires_at: "2026-09-06T00:00:00Z".to_owned(),
            addons: vec![CatalogAddon {
                id: "org.example.addon.fixture".to_owned(),
                version: Version::new(1, 0, 0),
                name: localized("Fixture"),
                summary: localized("Fixture add-on"),
                icon: None,
                license: "Zlib".to_owned(),
                source_url: "https://example.test/source".to_owned(),
                host_version: VersionReq::STAR,
                capabilities: vec![CAPABILITY_ADDON_CHECK_V1.to_owned()],
                dependencies: Vec::new(),
                permissions: AddonPermissions::default(),
                packages: BTreeMap::from([(
                    "any".to_owned(),
                    CatalogPackage {
                        url: "https://example.test/fixture.zip".to_owned(),
                        sha256: "a".repeat(64),
                        size: 123,
                    },
                )]),
            }],
        }
    }

    #[test]
    fn repository_index_validates_shared_structure() {
        valid_repository_index()
            .validate()
            .expect("valid repository index");

        let mut duplicate = valid_repository_index();
        duplicate.addons.push(duplicate.addons[0].clone());
        assert!(duplicate.validate().is_err());

        let mut invalid_interval = valid_repository_index();
        invalid_interval.expires_at = invalid_interval.generated_at.clone();
        assert!(invalid_interval.validate().is_err());
    }

    #[test]
    fn standalone_document_validation_uses_the_contract_types() {
        let manifest = toml::to_string(&valid_manifest()).expect("manifest TOML");
        validate_contract_document(ContractDocumentKind::AddonManifest, &manifest)
            .expect("valid add-on manifest");

        let mut unknown = manifest;
        unknown.push_str("\nunknown_field = true\n");
        assert!(validate_contract_document(ContractDocumentKind::AddonManifest, &unknown).is_err());

        let catalog = serde_json::to_string(&valid_repository_index()).expect("catalog JSON");
        validate_contract_document(ContractDocumentKind::RepositoryIndex, &catalog)
            .expect("valid repository index");
    }

    #[test]
    fn manifest_rejects_self_and_duplicate_dependencies() {
        let dependency = AddonDependency {
            id: "org.ersatzrs.addon.fixture.dependency".to_owned(),
            version: VersionReq::STAR,
            optional: false,
        };
        let mut self_dependent = valid_manifest();
        self_dependent.dependencies.push(AddonDependency {
            id: self_dependent.id.clone(),
            version: VersionReq::STAR,
            optional: false,
        });
        assert!(self_dependent.validate().is_err());

        let mut duplicated = valid_manifest();
        duplicated.dependencies = vec![dependency.clone(), dependency];
        assert!(duplicated.validate().is_err());
    }

    #[test]
    fn package_version_is_bounded_for_database_and_path_portability() {
        let version = Version::parse(&format!("1.0.0-{}", "a".repeat(129)))
            .expect("syntactically valid long SemVer");

        assert!(validate_package_version(&version).is_err());
    }

    #[test]
    fn signed_package_url_cannot_persist_query_credentials() {
        assert!(is_package_url("https://example.test/addon.zip"));
        assert!(!is_package_url(
            "https://example.test/addon.zip?token=secret"
        ));
        assert!(!is_package_url("https://example.test/addon.zip#download"));
    }

    #[test]
    fn supported_rids_match_the_release_matrix() {
        for rid in [
            "linux-x64",
            "linux-arm64",
            "linux-arm",
            "osx-x64",
            "osx-arm64",
            "win-x64",
        ] {
            assert!(is_supported_rid(rid));
        }
        assert!(!is_supported_rid("freebsd-x64"));
    }

    #[test]
    fn declarative_panel_rejects_executable_links() {
        let panel = AddonPanel {
            id: "unsafe".to_owned(),
            title: localized("Unsafe"),
            blocks: vec![AddonPanelBlock::Link {
                label: localized("Open"),
                url: "javascript:alert(1)".to_owned(),
            }],
        };
        assert!(panel.validate().is_err());
    }

    #[test]
    fn operation_errors_are_bounded_validated_and_textually_round_trip() {
        let diagnostic = AddonOperationError::new(
            OPERATION_CODE_PROVIDER_UNREACHABLE,
            "provider request failed",
        );
        diagnostic.validate().expect("valid operation diagnostic");
        let rendered = diagnostic.to_string();
        let json = rendered
            .strip_prefix(ADDON_OPERATION_ERROR_MARKER)
            .expect("stable diagnostic marker");
        assert_eq!(
            serde_json::from_str::<AddonOperationError>(json).expect("diagnostic JSON"),
            diagnostic
        );

        let invalid = AddonOperationError {
            code: "Provider Failed".to_owned(),
            message: "failure".to_owned(),
        };
        assert!(invalid.validate().is_err());
        assert_eq!(
            AddonOperationError::new("Provider Failed", "failure").code,
            "providerfailed"
        );

        let bounded = AddonOperationError::new("failure", "é".repeat(1_024));
        assert!(bounded.message.len() <= MAX_OPERATION_ERROR_MESSAGE_BYTES);
        assert!(bounded.message.is_char_boundary(bounded.message.len()));
    }

    #[test]
    fn media_list_conformance_enforces_order_and_uniqueness() {
        let valid = br#"{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Movie","title":"Movie","kind":"movie"}
"#;
        assert_eq!(conform_media_list_output(valid).unwrap().len(), 2);

        let duplicate = br#"{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Movie","title":"Movie","kind":"movie"}
{"record_type":"item","provider_id":"movie:1","rank":1,"display_title":"Other","title":"Other","kind":"movie"}
"#;
        assert!(conform_media_list_output(duplicate).is_err());
    }

    #[test]
    fn list_contracts_reject_and_name_unknown_fields() {
        let media_list = br#"{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Movie","title":"Movie","kind":"movie","geners":[]}
"#;
        let media_error = conform_media_list_output(media_list)
            .expect_err("media-list unknown field must reject the complete output")
            .to_string();
        assert!(media_error.contains("unknown field `geners`"));
        assert!(media_error.len() < 512);

        let remote_stream =
            br#"{"id":"video:1","url":"https://example.test/video","title":"Video","geners":[]}
"#;
        let remote_error = conform_remote_stream_list_output(remote_stream)
            .expect_err("Remote Stream unknown field must reject the complete output")
            .to_string();
        assert!(remote_error.contains("unknown field `geners`"));
        assert!(remote_error.len() < 512);

        let oversized_field = "x".repeat(4_096);
        let oversized = format!(
            "{{\"id\":\"video:1\",\"url\":\"https://example.test/video\",\"title\":\"Video\",\"{oversized_field}\":[]}}\n"
        );
        let bounded = conform_remote_stream_list_output(oversized.as_bytes())
            .expect_err("oversized unknown field must stay bounded")
            .to_string();
        assert!(bounded.contains("unknown field `"));
        assert!(bounded.len() < 512);
    }

    #[test]
    fn typed_media_list_defaults_keep_the_v1_boundary_shape() {
        let record = AddonMediaListRecord::item(
            "movie:1",
            0,
            "Movie",
            "Movie",
            AddonMediaListItemKind::Movie,
        );
        assert_eq!(
            serde_json::to_string(&record).unwrap(),
            r#"{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Movie","title":"Movie","kind":"movie","guids":[]}"#
        );
    }

    #[test]
    fn list_contracts_treat_null_and_omitted_arrays_as_absent() {
        let media_null = br#"{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Movie","title":"Movie","kind":"movie","guids":null}
"#;
        let media_omitted = br#"{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Movie","title":"Movie","kind":"movie"}
"#;
        assert_eq!(
            conform_media_list_output(media_null).unwrap(),
            conform_media_list_output(media_omitted).unwrap()
        );

        let remote_null = br#"{"id":"video:1","url":"https://example.test/video","title":"Video","genres":null,"tags":null,"people":null,"producers":null,"original_broadcasters":null,"broadcasters":null,"guids":null}
"#;
        let remote_omitted =
            br#"{"id":"video:1","url":"https://example.test/video","title":"Video"}
"#;
        assert_eq!(
            conform_remote_stream_list_output(remote_null).unwrap(),
            conform_remote_stream_list_output(remote_omitted).unwrap()
        );
    }

    #[test]
    fn remote_stream_conformance_rejects_case_insensitive_duplicate_ids() {
        let output = br#"{"id":"Video:1","url":"https://example.test/one","title":"One"}
{"id":"video:1","url":"https://example.test/two","title":"Two"}
"#;
        assert!(conform_remote_stream_list_output(output).is_err());
    }

    #[test]
    fn remote_stream_conformance_rejects_whitespace_normalized_duplicate_ids() {
        let output = br#"{"id":"video:1","url":"https://example.test/one","title":"One"}
{"id":" video:1 ","url":"https://example.test/two","title":"Two"}
"#;
        assert!(conform_remote_stream_list_output(output).is_err());
    }

    #[test]
    fn remote_stream_conformance_rejects_years_the_host_rejects() {
        let year_zero =
            br#"{"id":"video:1","url":"https://example.test/one","title":"One","year":0}
"#;
        assert!(conform_remote_stream_list_output(year_zero).is_err());

        let too_far_in_future = format!(
            "{{\"id\":\"video:1\",\"url\":\"https://example.test/one\",\"title\":\"One\",\"year\":{}}}\n",
            chrono::Utc::now().year() + 6
        );
        assert!(conform_remote_stream_list_output(too_far_in_future.as_bytes()).is_err());
    }

    #[test]
    fn remote_stream_item_metadata_only_adds_to_an_enumerated_entry() {
        let mut entry = PlaylistEntry::new("video:1", "https://example.test/one", "One");
        entry.plot = Some("listed plot".to_owned());
        entry.tags = vec!["listed".to_owned()];

        let output = br#"{"provider_id":"video:1","year":2026,"genres":["Documentary"],"tags":[]}
"#;
        let records = conform_remote_stream_item_output(output).unwrap();
        assert_eq!(records.len(), 1);
        records.into_iter().next().unwrap().apply_to(&mut entry);

        // An omitted field and an empty array both leave enumeration untouched.
        assert_eq!(entry.plot.as_deref(), Some("listed plot"));
        assert_eq!(entry.tags, vec!["listed".to_owned()]);
        // A supplied field fills in.
        assert_eq!(entry.year, Some(2026));
        assert_eq!(entry.genres, vec!["Documentary".to_owned()]);
    }

    #[test]
    fn remote_stream_item_conformance_rejects_unknown_and_duplicate_records() {
        let unknown_field = br#"{"provider_id":"video:1","nope":true}
"#;
        let error = conform_remote_stream_item_output(unknown_field).unwrap_err();
        assert!(error.to_string().contains("nope"), "{error}");

        let duplicates = br#"{"provider_id":"Video:1"}
{"provider_id":"video:1"}
"#;
        assert!(conform_remote_stream_item_output(duplicates).is_err());

        let empty_id = br#"{"provider_id":"   "}
"#;
        assert!(conform_remote_stream_item_output(empty_id).is_err());
    }

    #[test]
    fn provider_chapter_text_is_bounded_and_requires_item_v2() {
        let output = br#"{"provider_id":"video:1","chapter_input":"0:00 Intro\n0:20 Topic"}
"#;
        assert!(conform_remote_stream_item_output(output).is_err());
        let records = conform_remote_stream_item_output_for_capability(output, true).unwrap();
        assert_eq!(
            records[0].chapter_input.as_deref(),
            Some("0:00 Intro\n0:20 Topic")
        );

        let oversized = PlaylistEntryMetadata {
            provider_id: "video:1".to_owned(),
            chapter_input: Some("x".repeat(MAX_CHAPTER_INPUT_BYTES + 1)),
            ..PlaylistEntryMetadata::default()
        };
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn operator_source_urls_are_bounded_http_without_userinfo() {
        for valid in ["https://example.test/list", "http://localhost:8080/list"] {
            validate_operator_source_url(valid, &[]).expect("valid operator URL");
        }
        for invalid in [
            "file:///tmp/list",
            "javascript:alert(1)",
            "example.test/list",
            "https://user:secret@example.test/list",
        ] {
            assert!(
                validate_operator_source_url(invalid, &[]).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn operator_source_urls_return_the_validated_canonical_serialization() {
        let canonical = validate_operator_source_url(
            r"https://allowed.example\@evil.example/list",
            &["allowed.example".to_owned()],
        )
        .expect("URL parser assigns the declared host");

        assert_eq!(canonical, "https://allowed.example/@evil.example/list");
        let reparsed = url::Url::parse(&canonical).expect("canonical URL");
        assert_eq!(reparsed.host_str(), Some("allowed.example"));
    }

    #[test]
    fn operator_source_urls_honor_exact_suffix_and_wildcard_hosts() {
        let url = "https://video.media.example.test/list";
        validate_operator_source_url(url, &["*".to_owned()]).expect("wildcard host");
        validate_operator_source_url(url, &["VIDEO.MEDIA.EXAMPLE.TEST".to_owned()])
            .expect("case-insensitive exact host");
        validate_operator_source_url(url, &[".example.test".to_owned()])
            .expect("leading-dot suffix");
        validate_operator_source_url(url, &["*.media.example.test".to_owned()])
            .expect("wildcard suffix");
        assert!(validate_operator_source_url(url, &["example.test".to_owned()]).is_err());
        assert!(validate_operator_source_url(
            "https://example.test/list",
            &["*.example.test".to_owned()]
        )
        .is_err());
        assert!(validate_operator_source_url(
            "https://badexample.test/list",
            &[".example.test".to_owned()]
        )
        .is_err());
    }
}
