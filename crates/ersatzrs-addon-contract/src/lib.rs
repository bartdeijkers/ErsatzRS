//! Stable, dependency-light contracts shared by the ErsatzRS host and add-ons.
//!
//! Add-ons are separate processes. This crate defines package metadata,
//! repository catalogs, settings, declarative panels, and bounded invocation
//! results; it is not a native plugin ABI.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Datelike;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

mod media_list_operations;
pub use media_list_operations::*;
mod media_list_subtitles;
pub use media_list_subtitles::*;
mod remote_stream_retained_play;
pub use remote_stream_retained_play::*;
mod tool_update;
pub use tool_update::*;

/// Current package-manifest schema. Schema v1 remains readable.
pub const ADDON_MANIFEST_VERSION: u32 = 6;
/// Current repository-index schema. Schema v1 remains readable.
pub const ADDON_REPOSITORY_SCHEMA_VERSION: u32 = 2;
/// Current schema for an offline add-on repository checkout or bundle.
pub const LOCAL_ADDON_REPOSITORY_SCHEMA_VERSION: u32 = 1;
/// Health capability implemented by every executable add-on.
pub const CAPABILITY_ADDON_CHECK_V1: &str = "addon.check.v1";
/// Enumerate a Remote Stream playlist as bounded `NDJSON`.
pub const CAPABILITY_REMOTE_STREAM_LIST_V1: &str = "remote-stream.list.v1";
/// Enumerate Remote Streams with stable identity and availability metadata.
pub const CAPABILITY_REMOTE_STREAM_LIST_V2: &str = "remote-stream.list.v2";
pub const CAPABILITY_REMOTE_STREAM_LIST_V3: &str = "remote-stream.list.v3";
/// Enumerate Remote Streams with explicit provider-neutral playback behavior.
pub const CAPABILITY_REMOTE_STREAM_LIST_V4: &str = "remote-stream.list.v4";
/// Describe already-enumerated Remote Streams the host chose to keep.
///
/// Enumeration stays cheap and identity-only; this answers with descriptive
/// metadata for a bounded set of entries, so the cost of rich metadata scales
/// with the items actually retained rather than the whole upstream listing.
pub const CAPABILITY_REMOTE_STREAM_ITEM_V1: &str = "remote-stream.item.v1";
/// Describe retained Remote Streams, including provider-supplied chapter text.
pub const CAPABILITY_REMOTE_STREAM_ITEM_V2: &str = "remote-stream.item.v2";
/// Describe retained Remote Streams with the shared rich metadata document.
/// Produce Remote Stream bytes for playback.
pub const CAPABILITY_REMOTE_STREAM_PLAY_V1: &str = "remote-stream.play.v1";
/// Produce Remote Stream bytes with a host-supplied maximum video height.
pub const CAPABILITY_REMOTE_STREAM_PLAY_V2: &str = "remote-stream.play.v2";
/// Enumerate a provider list whose entries are matched to local media.
pub const CAPABILITY_MEDIA_LIST_LIST_V1: &str = "media-list.list.v1";
/// Enumerate a provider list with explicit per-item availability.
pub const CAPABILITY_MEDIA_LIST_LIST_V2: &str = "media-list.list.v2";
pub const CAPABILITY_MEDIA_LIST_LIST_V3: &str = "media-list.list.v3";
/// Enumerate a provider list with shared list/item metadata documents.
pub const CAPABILITY_MEDIA_LIST_LIST_V4: &str = "media-list.list.v4";
/// Enumerate provider lists with explicit Remote Stream playback behavior.
pub const CAPABILITY_MEDIA_LIST_LIST_V5: &str = "media-list.list.v5";
/// Enumerate provider lists with typed terminal availability reasons.
pub const CAPABILITY_MEDIA_LIST_LIST_V6: &str = "media-list.list.v6";
/// Page list overviews and enrich one item at a time through host-owned jobs.
pub const CAPABILITY_MEDIA_LIST_IMPORT_V1: &str = "media-list.import.v1";
/// Discover full or incremental list generations through a staged host-owned archive.
pub const CAPABILITY_MEDIA_LIST_IMPORT_V2: &str = "media-list.import.v2";
/// Declare host-rendered, provider-neutral media-list options.
pub const CAPABILITY_MEDIA_LIST_OPTIONS_V1: &str = "media-list.options.v1";
/// Probe one media-list source for changes without enumerating its contents.
pub const CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1: &str = "media-list.source-check.v1";
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
/// Maximum captured output accepted from one paged overview invocation.
pub const MAX_MEDIA_LIST_IMPORT_PAGE_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum captured output accepted from one item-enrichment invocation.
pub const MAX_MEDIA_LIST_IMPORT_ITEM_OUTPUT_BYTES: usize = 1024 * 1024;
/// Maximum serialized JSON request accepted by an import operation.
pub const MAX_MEDIA_LIST_IMPORT_REQUEST_BYTES: usize = 512 * 1024;
/// Maximum UTF-8 size of an opaque provider continuation cursor.
pub const MAX_MEDIA_LIST_IMPORT_CURSOR_BYTES: usize = 4 * 1024;
/// Maximum number of overview items accepted from one discovery page.
pub const MAX_MEDIA_LIST_IMPORT_PAGE_ITEMS: usize = 250;
/// Maximum UTF-8 bytes in a host-supplied staged archive path.
pub const MAX_MEDIA_LIST_IMPORT_ARCHIVE_PATH_BYTES: usize = 4 * 1024;
/// Maximum bytes in one staged incremental archive.
pub const MAX_MEDIA_LIST_IMPORT_ARCHIVE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum entries in one staged incremental archive.
pub const MAX_MEDIA_LIST_IMPORT_ARCHIVE_ENTRIES: usize = MAX_LIST_ITEMS;
/// Maximum option definitions declared by one media-list option schema.
pub const MAX_MEDIA_LIST_OPTION_DEFINITIONS: usize = 32;
/// Maximum enum choices declared by one media-list option.
pub const MAX_MEDIA_LIST_OPTION_CHOICES: usize = 32;
/// Maximum media-list option values carried in one typed envelope.
pub const MAX_MEDIA_LIST_OPTION_VALUES: usize = 32;
/// Maximum UTF-8 bytes in one option key or enum value.
pub const MAX_MEDIA_LIST_OPTION_IDENTIFIER_BYTES: usize = 64;
/// Maximum serialized JSON size of one effective option envelope.
pub const MAX_MEDIA_LIST_OPTION_VALUES_BYTES: usize = 16 * 1024;
/// Maximum serialized request accepted by one source-change probe.
pub const MAX_MEDIA_LIST_SOURCE_CHECK_REQUEST_BYTES: usize = 64 * 1024;
/// Maximum serialized result accepted from one source-change probe.
pub const MAX_MEDIA_LIST_SOURCE_CHECK_OUTPUT_BYTES: usize = 64 * 1024;
/// Maximum UTF-8 bytes in one package-owned conditional validator.
pub const MAX_MEDIA_LIST_SOURCE_CHECK_VALIDATOR_BYTES: usize = 4 * 1024;
/// Maximum source update instants accepted from one source-change probe.
pub const MAX_MEDIA_LIST_SOURCE_UPDATE_INSTANTS: usize = 16;
/// Maximum captured output accepted from `remote-stream.list`.
pub const MAX_REMOTE_STREAM_LIST_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of items accepted from one list invocation.
pub const MAX_LIST_ITEMS: usize = 10_000;
/// Maximum number of repeated values accepted in one metadata document.
pub const MAX_METADATA_VALUES: usize = 1_024;
/// Maximum UTF-8 bytes accepted across the textual values of one metadata document.
pub const MAX_METADATA_TEXT_BYTES: usize = 256 * 1_024;
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

/// Provider-neutral terminal reason for an unavailable add-on list item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListAvailabilityReason {
    /// Playback requires credentials that were not available to the add-on.
    AuthenticationRequired,
    /// Playback is restricted by age, membership, subscription, or visibility policy.
    ContentRestricted,
    /// The provider reports no more specific portable reason.
    Unavailable,
}

/// Provider-neutral playback behavior for a Remote Stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RemoteStreamLiveness {
    #[default]
    Unknown,
    Finite,
    Live,
}

impl RemoteStreamLiveness {
    /// Adapt a legacy optional boolean and duration conservatively.
    #[must_use]
    pub const fn from_legacy(is_live: Option<bool>, duration_seconds: Option<u64>) -> Self {
        match is_live {
            Some(true) => Self::Live,
            Some(false) => Self::Finite,
            None if matches!(duration_seconds, Some(value) if value > 0) => Self::Finite,
            None => Self::Unknown,
        }
    }

    /// Compatibility value for legacy boolean consumers.
    #[must_use]
    pub const fn legacy_is_live(self) -> bool {
        matches!(self, Self::Live)
    }
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
    Song,
    Image,
}

/// Semantic role of one provider source inside its collection.
///
/// Rank remains the original provider order; it must never be used as an
/// implicit movie feature/extra classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListItemRole {
    #[default]
    Auto,
    Primary,
    Extra,
}

/// Whether missing item details are fetched automatically after publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaListDetailImportPolicy {
    #[default]
    Automatic,
    ManualOnly,
}

/// One credited person in the provider-neutral metadata document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AddonMetadataPerson {
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub thumb: Option<String>,
}

/// Semantic role suggested for one provider artwork candidate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonArtworkRole {
    Poster,
    Fanart,
    Thumb,
    #[default]
    Other,
}

/// One provider artwork candidate. ErsatzRS validates and materializes the URL.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AddonArtworkCandidate {
    pub url: String,
    #[serde(default)]
    pub role: AddonArtworkRole,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub language: Option<String>,
}

/// Complete provider-neutral descriptive metadata shared by list and item
/// review. Operational identity, playback URL, availability, and duration stay
/// on their existing records.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AddonMetadataDocument {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub outline: Option<String>,
    pub plot: Option<String>,
    pub tagline: Option<String>,
    pub show_title: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub year: Option<i32>,
    pub release_date: Option<String>,
    pub content_ratings: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub people: Vec<AddonMetadataPerson>,
    pub writers: Vec<String>,
    pub directors: Vec<String>,
    pub artists: Vec<String>,
    pub producers: Vec<String>,
    pub original_broadcasters: Vec<String>,
    pub broadcasters: Vec<String>,
    pub collection: Option<String>,
    pub artwork: Vec<AddonArtworkCandidate>,
    pub guids: Vec<String>,
}

impl AddonMetadataDocument {
    /// Validate aggregate bounds before the document crosses into host storage.
    ///
    /// # Errors
    ///
    /// Returns an error when repeated values or aggregate text exceed the
    /// provider-neutral contract limits, or an artwork/person entry is empty.
    pub fn validate(&self) -> Result<(), ContractError> {
        self.validate_collection_bounds()?;
        self.validate_people_and_artwork()?;
        self.validate_identity()?;
        self.validate_repeated_text()?;

        let bytes = serde_json::to_vec(self)
            .map_err(|_| ContractError::Invalid {
                field: "metadata document",
                message: "cannot be encoded".to_owned(),
            })?
            .len();
        if bytes > MAX_METADATA_TEXT_BYTES {
            return invalid("metadata document", "exceeds the 256 KiB limit");
        }
        Ok(())
    }

    fn validate_collection_bounds(&self) -> Result<(), ContractError> {
        let repeated = [
            self.content_ratings.len(),
            self.genres.len(),
            self.tags.len(),
            self.studios.len(),
            self.music_labels.len(),
            self.languages.len(),
            self.people.len(),
            self.writers.len(),
            self.directors.len(),
            self.artists.len(),
            self.producers.len(),
            self.original_broadcasters.len(),
            self.broadcasters.len(),
            self.artwork.len(),
            self.guids.len(),
        ];
        if repeated
            .into_iter()
            .any(|count| count > MAX_METADATA_VALUES)
        {
            return invalid("metadata document", "contains too many repeated values");
        }
        Ok(())
    }

    fn validate_people_and_artwork(&self) -> Result<(), ContractError> {
        if self
            .people
            .iter()
            .any(|person| person.name.trim().is_empty())
        {
            return invalid("metadata people", "must not contain an empty name");
        }
        if self.people.iter().any(|person| {
            person
                .thumb
                .as_deref()
                .is_some_and(|thumb| !is_https_url(thumb))
        }) {
            return invalid("metadata people", "thumb URLs must use bounded HTTPS");
        }
        if self.artwork.iter().any(|item| {
            !is_https_url(&item.url)
                || item.width.is_some_and(|width| width == 0)
                || item.height.is_some_and(|height| height == 0)
        }) {
            return invalid(
                "metadata artwork",
                "URLs must use bounded HTTPS and dimensions must be positive",
            );
        }
        Ok(())
    }

    fn validate_identity(&self) -> Result<(), ContractError> {
        if self.season.is_some_and(|season| season < 0)
            || self.episode.is_some_and(|episode| episode < 1)
        {
            return invalid(
                "metadata numbering",
                "season must be non-negative and episode must be positive",
            );
        }
        if self
            .year
            .is_some_and(|year| !(1..=chrono::Utc::now().year() + 5).contains(&year))
        {
            return invalid("metadata year", "must be valid host metadata");
        }
        if self
            .release_date
            .as_deref()
            .is_some_and(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err())
        {
            return invalid("metadata release_date", "must use YYYY-MM-DD");
        }
        Ok(())
    }

    fn validate_repeated_text(&self) -> Result<(), ContractError> {
        let repeated_text = [
            &self.content_ratings,
            &self.genres,
            &self.tags,
            &self.studios,
            &self.music_labels,
            &self.languages,
            &self.writers,
            &self.directors,
            &self.artists,
            &self.producers,
            &self.original_broadcasters,
            &self.broadcasters,
            &self.guids,
        ];
        if repeated_text
            .into_iter()
            .any(|values| values.iter().any(|value| value.trim().is_empty()))
        {
            return invalid(
                "metadata document",
                "repeated text values must not be empty",
            );
        }
        Ok(())
    }
}

/// One bounded `media-list.list.v1` through `media-list.list.v6` NDJSON record.
///
/// Both variants intentionally retain the frozen, strongly typed record shape.
/// Boxing either metadata field would be a source-level API break for add-ons
/// while leaving the serialized contract unchanged.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddonMediaListRecord {
    List {
        provider_id: String,
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<AddonMetadataDocument>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        availability_reason_code: Option<AddonMediaListAvailabilityReason>,
        #[serde(default, skip_serializing_if = "content_kind_is_auto")]
        content_kind: AddonMediaListContentKind,
        #[serde(default, skip_serializing_if = "item_role_is_auto")]
        collection_role: AddonMediaListItemRole,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_seconds: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        liveness: Option<RemoteStreamLiveness>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<AddonMetadataDocument>,
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

#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip predicate signature
const fn item_role_is_auto(value: &AddonMediaListItemRole) -> bool {
    matches!(value, AddonMediaListItemRole::Auto)
}

impl AddonMediaListRecord {
    /// Construct a list header with the required fields populated.
    #[must_use]
    pub fn list(provider_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::List {
            provider_id: provider_id.into(),
            name: name.into(),
            description: None,
            metadata: None,
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
            availability_reason_code: None,
            content_kind: AddonMediaListContentKind::Auto,
            collection_role: AddonMediaListItemRole::Auto,
            duration_seconds: None,
            liveness: None,
            parent_provider_id: None,
            fragment_start_seconds: None,
            fragment_end_seconds: None,
            additional_image_urls: Vec::new(),
            metadata: None,
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
                provider_id,
                name,
                metadata,
                ..
            } => {
                validate_provider_id(provider_id)?;
                if name.trim().is_empty() {
                    return invalid("media list name", "must not be empty");
                }
                if let Some(metadata) = metadata {
                    metadata.validate()?;
                }
            }
            Self::Item {
                provider_id,
                rank,
                display_title,
                title,
                kind,
                source_url,
                availability,
                availability_reason,
                availability_reason_code,
                parent_provider_id,
                fragment_start_seconds,
                fragment_end_seconds,
                additional_image_urls,
                metadata,
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
                if availability_reason_code.is_some()
                    && *availability != AddonMediaListItemAvailability::Unavailable
                {
                    return invalid(
                        "media list item availability_reason_code",
                        "requires unavailable availability",
                    );
                }
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
                if let Some(metadata) = metadata {
                    metadata.validate()?;
                }
            }
        }
        Ok(())
    }
}

/// Host limits sent with every bounded overview discovery request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListDiscoverLimits {
    pub max_items: u16,
    pub max_output_bytes: u32,
}

impl Default for MediaListDiscoverLimits {
    fn default() -> Self {
        Self {
            max_items: u16::try_from(MAX_MEDIA_LIST_IMPORT_PAGE_ITEMS)
                .expect("the page item limit fits in u16"),
            max_output_bytes: u32::try_from(MAX_MEDIA_LIST_IMPORT_PAGE_OUTPUT_BYTES)
                .expect("the page output limit fits in u32"),
        }
    }
}

/// One `media-list.import.v1 discover` request written as JSON to stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListDiscoverRequest {
    pub source_url: String,
    pub record_capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limits: MediaListDiscoverLimits,
}

impl MediaListDiscoverRequest {
    /// Validate host and add-on authored request bounds.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported schemas, empty URLs, or excessive
    /// cursor and page limits.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_import_record_capability(&self.record_capability)?;
        if self.source_url.trim().is_empty() || self.source_url.len() > 8 * 1024 {
            return invalid(
                "media list discover source_url",
                "must be between 1 and 8192 UTF-8 bytes",
            );
        }
        validate_import_cursor(self.cursor.as_deref())?;
        validate_media_list_discover_limits(self.limits)?;
        validate_import_request_size(self)
    }
}

/// Whether one `media-list.import.v2 discover` invocation enumerates a full
/// generation or only identities not present in the staged archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaListImportMode {
    Full,
    Incremental,
}

/// Package-declared archive behavior for one v2 discovery invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaListIncrementalStrategy {
    /// Enumerate the bounded source while suppressing archived identities.
    SkipKnown,
    /// Stop at the first archived identity for a stable newest-first source.
    NewestFirstEarlyStop,
}

/// Host-owned staged archive made available to one v2 discovery invocation.
///
/// The package may mutate the file at `staged_path`, but must not emit its
/// path or contents. The host validates and promotes the successor after the
/// corresponding database page commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListArchiveDescriptor {
    pub staged_path: String,
    pub generation: u32,
    pub entry_count: u32,
    pub sha256: String,
    pub max_bytes: u32,
    pub max_entries: u32,
}

impl MediaListArchiveDescriptor {
    /// Validate the portable bounds on a host-owned staged archive.
    ///
    /// Path containment and platform-specific absolute-path validation remain
    /// host responsibilities because this contract is shared with add-ons on
    /// every supported target.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty or excessive path, invalid generation or
    /// digest, or archive limits beyond the import contract.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.staged_path.is_empty()
            || self.staged_path.len() > MAX_MEDIA_LIST_IMPORT_ARCHIVE_PATH_BYTES
            || self.staged_path.chars().any(char::is_control)
        {
            return invalid(
                "media list import archive staged_path",
                "must be 1 to 4096 UTF-8 bytes without control characters",
            );
        }
        if self.generation == 0 {
            return invalid("media list import archive generation", "must be positive");
        }
        if !is_sha256_hex(&self.sha256) {
            return invalid(
                "media list import archive sha256",
                "must be lowercase SHA-256 hex",
            );
        }
        if self.max_bytes == 0
            || usize::try_from(self.max_bytes)
                .ok()
                .is_none_or(|value| value > MAX_MEDIA_LIST_IMPORT_ARCHIVE_BYTES)
        {
            return invalid(
                "media list import archive max_bytes",
                "must be between 1 and 4194304",
            );
        }
        if self.max_entries == 0
            || usize::try_from(self.max_entries)
                .ok()
                .is_none_or(|value| value > MAX_MEDIA_LIST_IMPORT_ARCHIVE_ENTRIES)
        {
            return invalid(
                "media list import archive max_entries",
                "must be between 1 and 10000",
            );
        }
        if self.entry_count > self.max_entries {
            return invalid(
                "media list import archive entry_count",
                "must not exceed max_entries",
            );
        }
        Ok(())
    }
}

/// One `media-list.import.v2 discover` request written as JSON to stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListDiscoverV2Request {
    pub source_url: String,
    pub record_capability: String,
    pub mode: MediaListImportMode,
    pub archive: MediaListArchiveDescriptor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limits: MediaListDiscoverLimits,
}

impl MediaListDiscoverV2Request {
    /// Validate host and add-on authored v2 discovery bounds.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported schemas, empty URLs, invalid archive
    /// descriptors, or excessive cursor and page limits.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_import_record_capability(&self.record_capability)?;
        if self.source_url.trim().is_empty() || self.source_url.len() > 8 * 1024 {
            return invalid(
                "media list discover source_url",
                "must be between 1 and 8192 UTF-8 bytes",
            );
        }
        self.archive.validate()?;
        validate_import_cursor(self.cursor.as_deref())?;
        validate_media_list_discover_limits(self.limits)?;
        validate_import_request_size(self)
    }
}

/// One `media-list.import.v1 enrich` request written as JSON to stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListEnrichRequest {
    pub source_url: String,
    pub record_capability: String,
    pub provider_id: String,
    pub overview_fingerprint: String,
    pub item: AddonMediaListRecord,
}

impl MediaListEnrichRequest {
    /// Validate one bounded, identity-preserving enrichment request.
    ///
    /// # Errors
    ///
    /// Returns an error when the request is oversized, its schema is not
    /// supported, or the embedded item identity does not match.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_import_record_capability(&self.record_capability)?;
        if self.source_url.trim().is_empty() || self.source_url.len() > 8 * 1024 {
            return invalid(
                "media list enrich source_url",
                "must be between 1 and 8192 UTF-8 bytes",
            );
        }
        validate_provider_id(&self.provider_id)?;
        if self.overview_fingerprint.trim().is_empty()
            || self.overview_fingerprint.len() > MAX_PROVIDER_ID_BYTES
        {
            return invalid(
                "media list enrich overview_fingerprint",
                "must be between 1 and 384 UTF-8 bytes",
            );
        }
        self.item.validate()?;
        match &self.item {
            AddonMediaListRecord::Item { provider_id, .. } if provider_id == &self.provider_id => {}
            AddonMediaListRecord::Item { .. } => {
                return invalid(
                    "media list enrich item",
                    "provider_id must match the requested identity",
                );
            }
            AddonMediaListRecord::List { .. } => {
                return invalid("media list enrich item", "must be an item record");
            }
        }
        validate_import_request_size(self)
    }
}

/// One bounded `media-list.source-check.v1 check` request written as JSON to stdin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListSourceCheckRequest {
    pub source_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validator: Option<String>,
}

impl MediaListSourceCheckRequest {
    /// Validate the source and opaque conditional validator bounds.
    ///
    /// # Errors
    ///
    /// Returns an error when the source, validator, or serialized request is
    /// empty or exceeds its protocol bound.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.source_url.trim().is_empty() || self.source_url.len() > 8 * 1024 {
            return invalid(
                "media list source check source_url",
                "must be between 1 and 8192 UTF-8 bytes",
            );
        }
        validate_source_check_validator(self.validator.as_deref())?;
        validate_source_check_serialized_size(
            self,
            "media list source check request",
            MAX_MEDIA_LIST_SOURCE_CHECK_REQUEST_BYTES,
        )
    }
}

/// Typed terminal result of one bounded source-change probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaListSourceCheckOutcome {
    Changed,
    Unchanged,
    TransientFailure,
    PermanentFailure,
}

/// One `media-list.source-check.v1 check` result emitted as bounded JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaListSourceCheckResult {
    pub outcome: MediaListSourceCheckOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validator: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_updated_at: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u32>,
}

impl MediaListSourceCheckResult {
    /// Validate one source-check result before it influences durable scheduling.
    ///
    /// Successful outcomes may carry an opaque validator and up to sixteen UTC
    /// update instants. Failure outcomes carry only a canonical diagnostic code,
    /// plus an optional bounded retry hint for transient failures.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, malformed, or contradictory fields.
    pub fn validate(&self) -> Result<(), ContractError> {
        let succeeded = matches!(
            self.outcome,
            MediaListSourceCheckOutcome::Changed | MediaListSourceCheckOutcome::Unchanged
        );
        if succeeded {
            if self.code.is_some() || self.retry_after_seconds.is_some() {
                return invalid(
                    "media list source check result",
                    "changed and unchanged outcomes must not carry failure fields",
                );
            }
            validate_source_check_validator(self.validator.as_deref())?;
            validate_source_update_instants(&self.source_updated_at)?;
        } else {
            if self.validator.is_some() || !self.source_updated_at.is_empty() {
                return invalid(
                    "media list source check result",
                    "failure outcomes must not carry a validator or source update instants",
                );
            }
            let code = self.code.as_deref().ok_or_else(|| ContractError::Invalid {
                field: "media list source check code",
                message: "is required for failure outcomes".to_owned(),
            })?;
            if code != sanitize_addon_diagnostic_code(code) {
                return invalid(
                    "media list source check code",
                    "must be 1 to 128 lowercase ASCII code characters",
                );
            }
            if self.retry_after_seconds.is_some_and(|value| value > 86_400) {
                return invalid(
                    "media list source check retry_after_seconds",
                    "must not exceed 86400 seconds",
                );
            }
            if self.outcome != MediaListSourceCheckOutcome::TransientFailure
                && self.retry_after_seconds.is_some()
            {
                return invalid(
                    "media list source check retry_after_seconds",
                    "is only valid for transient_failure",
                );
            }
        }
        validate_source_check_serialized_size(
            self,
            "media list source check result",
            MAX_MEDIA_LIST_SOURCE_CHECK_OUTPUT_BYTES,
        )
    }
}

fn validate_source_check_validator(validator: Option<&str>) -> Result<(), ContractError> {
    if validator.is_some_and(|value| {
        value.is_empty()
            || value.len() > MAX_MEDIA_LIST_SOURCE_CHECK_VALIDATOR_BYTES
            || value.chars().any(char::is_control)
    }) {
        return invalid(
            "media list source check validator",
            "must be 1 to 4096 UTF-8 bytes without control characters",
        );
    }
    Ok(())
}

fn validate_source_update_instants(instants: &[String]) -> Result<(), ContractError> {
    if instants.len() > MAX_MEDIA_LIST_SOURCE_UPDATE_INSTANTS {
        return invalid(
            "media list source check source_updated_at",
            "must contain at most 16 instants",
        );
    }
    let mut unique = BTreeSet::new();
    for value in instants {
        if value.len() > 64 {
            return invalid(
                "media list source check source_updated_at",
                "must contain bounded RFC 3339 UTC instants",
            );
        }
        let instant =
            chrono::DateTime::parse_from_rfc3339(value).map_err(|_| ContractError::Invalid {
                field: "media list source check source_updated_at",
                message: "must contain RFC 3339 UTC instants".to_owned(),
            })?;
        if instant.offset().local_minus_utc() != 0 || !unique.insert(instant) {
            return invalid(
                "media list source check source_updated_at",
                "must contain unique UTC instants",
            );
        }
    }
    Ok(())
}

fn validate_source_check_serialized_size<T: Serialize>(
    value: &T,
    field: &'static str,
    limit: usize,
) -> Result<(), ContractError> {
    let bytes = serde_json::to_vec(value).map_err(|error| ContractError::Invalid {
        field,
        message: error.to_string(),
    })?;
    if bytes.len() > limit {
        return invalid(field, format!("exceeds the {limit}-byte limit"));
    }
    Ok(())
}

/// First NDJSON record emitted by `media-list.import.v1 discover`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaListDiscoverControl {
    Page {
        complete: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_hint: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revision_token: Option<String>,
    },
}

/// Validated output from one overview page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaListDiscoveryPage {
    pub complete: bool,
    pub next_cursor: Option<String>,
    pub total_hint: Option<u32>,
    pub revision_token: Option<String>,
    pub records: Vec<AddonMediaListRecord>,
}

/// First NDJSON record emitted by `media-list.import.v2 discover`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaListDiscoverV2Control {
    Page {
        complete: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_hint: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revision_token: Option<String>,
        strategy: MediaListIncrementalStrategy,
        examined_count: u32,
        archived_skipped_count: u32,
        emitted_count: u32,
    },
}

/// Validated output from one v2 full or incremental overview page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaListDiscoveryV2Page {
    pub complete: bool,
    pub next_cursor: Option<String>,
    pub total_hint: Option<u32>,
    pub revision_token: Option<String>,
    pub strategy: MediaListIncrementalStrategy,
    pub examined_count: u32,
    pub archived_skipped_count: u32,
    pub emitted_count: u32,
    pub records: Vec<AddonMediaListRecord>,
}

/// Typed outcome of one item enrichment attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaListEnrichOutcomeKind {
    Complete,
    Unavailable,
    TransientFailure,
    PermanentFailure,
}

/// First NDJSON record emitted by `media-list.import.v1 enrich`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaListEnrichControl {
    Outcome {
        outcome: MediaListEnrichOutcomeKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_after_seconds: Option<u32>,
    },
}

/// Validated result of one item enrichment invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaListEnrichmentResult {
    pub outcome: MediaListEnrichOutcomeKind,
    pub code: Option<String>,
    pub message: Option<String>,
    pub retry_after_seconds: Option<u32>,
    pub item: Option<AddonMediaListRecord>,
}

fn validate_import_record_capability(capability: &str) -> Result<(), ContractError> {
    if matches!(
        capability,
        CAPABILITY_MEDIA_LIST_LIST_V1
            | CAPABILITY_MEDIA_LIST_LIST_V2
            | CAPABILITY_MEDIA_LIST_LIST_V3
            | CAPABILITY_MEDIA_LIST_LIST_V4
            | CAPABILITY_MEDIA_LIST_LIST_V5
            | CAPABILITY_MEDIA_LIST_LIST_V6
    ) {
        Ok(())
    } else {
        invalid(
            "media list import record_capability",
            "must name a supported media-list.list schema",
        )
    }
}

fn validate_import_cursor(cursor: Option<&str>) -> Result<(), ContractError> {
    if cursor.is_some_and(|value| {
        value.is_empty()
            || value.len() > MAX_MEDIA_LIST_IMPORT_CURSOR_BYTES
            || value.chars().any(char::is_control)
    }) {
        return invalid(
            "media list import cursor",
            "must be 1 to 4096 UTF-8 bytes without control characters",
        );
    }
    Ok(())
}

fn validate_media_list_discover_limits(
    limits: MediaListDiscoverLimits,
) -> Result<(), ContractError> {
    if limits.max_items == 0 || usize::from(limits.max_items) > MAX_MEDIA_LIST_IMPORT_PAGE_ITEMS {
        return invalid(
            "media list discover limits.max_items",
            "must be between 1 and 250",
        );
    }
    if limits.max_output_bytes == 0
        || usize::try_from(limits.max_output_bytes)
            .ok()
            .is_none_or(|value| value > MAX_MEDIA_LIST_IMPORT_PAGE_OUTPUT_BYTES)
    {
        return invalid(
            "media list discover limits.max_output_bytes",
            "must be between 1 and 4194304",
        );
    }
    Ok(())
}

fn validate_import_request_size<T: Serialize>(request: &T) -> Result<(), ContractError> {
    let bytes = serde_json::to_vec(request).map_err(|error| ContractError::Invalid {
        field: "media list import request",
        message: error.to_string(),
    })?;
    if bytes.len() > MAX_MEDIA_LIST_IMPORT_REQUEST_BYTES {
        return invalid("media list import request", "exceeds the 512 KiB limit");
    }
    Ok(())
}

/// One credited person in a Remote Stream list record.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteStreamPerson {
    pub name: String,
    pub role: String,
}

/// One provider entry emitted by `remote-stream.list.v1` through v4.
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
    /// Legacy two-state playback flag. `None` distinguishes omission from an
    /// explicitly confirmed finite source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_live: Option<bool>,
    /// Authoritative provider-neutral playback behavior from
    /// `remote-stream.list.v4`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness: Option<RemoteStreamLiveness>,
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
        self.effective_liveness()?;
        Ok(())
    }

    /// Resolve the v4 value or conservatively adapt legacy fields.
    ///
    /// # Errors
    ///
    /// Returns an error when new and legacy fields contradict each other.
    pub fn effective_liveness(&self) -> Result<RemoteStreamLiveness, ContractError> {
        match (self.liveness, self.is_live) {
            (Some(liveness), Some(is_live))
                if RemoteStreamLiveness::from_legacy(Some(is_live), self.duration_seconds)
                    != liveness =>
            {
                invalid(
                    "remote stream liveness",
                    "liveness contradicts legacy is_live",
                )
            }
            (Some(liveness), _) => Ok(liveness),
            (None, is_live) => Ok(RemoteStreamLiveness::from_legacy(
                is_live,
                self.duration_seconds,
            )),
        }
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
            let manifest: LocalRepositoryManifest =
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
    /// Hosts this package claims as their default operator-source handler.
    ///
    /// This is a narrower statement than `permissions.network_hosts`: that field
    /// records which hosts the package is permitted to contact, while this one
    /// records which hosts it offers to process on the operator's behalf. Every
    /// entry must also be permitted, and no entry may claim every host.
    #[serde(default)]
    pub default_source_hosts: Vec<String>,
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
            capability.validate_options(self.manifest_version)?;
        }
        self.validate_media_list_import_capability()?;
        self.validate_media_list_options_capability()?;
        self.validate_media_list_source_check_capability()?;
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
        self.validate_media_list_storage()?;
        self.validate_tool_update_capability()?;

        self.validate_default_source_hosts()?;

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

    fn validate_tool_update_capability(&self) -> Result<(), ContractError> {
        let capability = self.has_capability(CAPABILITY_TOOL_UPDATE_V1);
        let permission = !self.permissions.tool_update.is_empty();
        if (capability || permission) && self.manifest_version < 6 {
            return invalid(
                "permissions.tool_update",
                "requires manifest schema version 6",
            );
        }
        if capability != permission {
            return invalid(
                "permissions.tool_update",
                "tool.update.v1 and executable update permissions must be declared together",
            );
        }
        for key in &self.permissions.tool_update {
            if !self
                .settings
                .iter()
                .any(|setting| setting.key == *key && setting.kind == AddonSettingKind::Executable)
            {
                return invalid(
                    "permissions.tool_update",
                    "must name a declared executable setting",
                );
            }
        }
        Ok(())
    }

    fn validate_media_list_import_capability(&self) -> Result<(), ContractError> {
        let import_capability = [
            CAPABILITY_MEDIA_LIST_IMPORT_V1,
            CAPABILITY_MEDIA_LIST_IMPORT_V2,
        ]
        .into_iter()
        .find(|capability| self.has_capability(capability));
        if let Some(import_capability) = import_capability {
            let has_record_schema = [
                CAPABILITY_MEDIA_LIST_LIST_V1,
                CAPABILITY_MEDIA_LIST_LIST_V2,
                CAPABILITY_MEDIA_LIST_LIST_V3,
                CAPABILITY_MEDIA_LIST_LIST_V4,
                CAPABILITY_MEDIA_LIST_LIST_V5,
                CAPABILITY_MEDIA_LIST_LIST_V6,
            ]
            .into_iter()
            .any(|capability| self.has_capability(capability));
            if !has_record_schema {
                return invalid(
                    "capabilities",
                    format!(
                        "{import_capability} requires a supported media-list.list record schema"
                    ),
                );
            }
        }
        Ok(())
    }

    fn validate_media_list_options_capability(&self) -> Result<(), ContractError> {
        if self.has_capability(CAPABILITY_MEDIA_LIST_OPTIONS_V1)
            && ![
                CAPABILITY_MEDIA_LIST_LIST_V1,
                CAPABILITY_MEDIA_LIST_LIST_V2,
                CAPABILITY_MEDIA_LIST_LIST_V3,
                CAPABILITY_MEDIA_LIST_LIST_V4,
                CAPABILITY_MEDIA_LIST_LIST_V5,
                CAPABILITY_MEDIA_LIST_LIST_V6,
            ]
            .into_iter()
            .any(|capability| self.has_capability(capability))
        {
            return invalid(
                "capabilities",
                "media-list.options.v1 requires a supported media-list.list capability",
            );
        }
        Ok(())
    }

    fn validate_media_list_source_check_capability(&self) -> Result<(), ContractError> {
        if self.has_capability(CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1)
            && ![
                CAPABILITY_MEDIA_LIST_LIST_V1,
                CAPABILITY_MEDIA_LIST_LIST_V2,
                CAPABILITY_MEDIA_LIST_LIST_V3,
                CAPABILITY_MEDIA_LIST_LIST_V4,
                CAPABILITY_MEDIA_LIST_LIST_V5,
                CAPABILITY_MEDIA_LIST_LIST_V6,
            ]
            .into_iter()
            .any(|capability| self.has_capability(capability))
        {
            return invalid(
                "capabilities",
                "media-list.source-check.v1 requires a supported media-list.list capability",
            );
        }
        Ok(())
    }

    fn validate_media_list_storage(&self) -> Result<(), ContractError> {
        let Some(storage) = &self.media_list_storage else {
            return Ok(());
        };
        if self.manifest_version < 3 {
            return invalid("media_list_storage", "requires manifest schema version 3");
        }
        storage.validate(&self.settings)?;
        if !self.has_remote_stream_play_capability() {
            return invalid(
                "media_list_storage",
                "requires a supported remote-stream.play capability",
            );
        }
        if !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V1)
            && !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V2)
            && !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V3)
            && !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V4)
            && !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V5)
            && !self.has_capability(CAPABILITY_MEDIA_LIST_LIST_V6)
        {
            return invalid(
                "media_list_storage",
                "requires a supported media-list.list capability",
            );
        }
        if !self.has_capability(CAPABILITY_REMOTE_STREAM_LIST_V1)
            && !self.has_capability(CAPABILITY_REMOTE_STREAM_LIST_V2)
            && !self.has_capability(CAPABILITY_REMOTE_STREAM_LIST_V3)
            && !self.has_capability(CAPABILITY_REMOTE_STREAM_LIST_V4)
        {
            return invalid(
                "media_list_storage",
                "requires a supported remote-stream.list capability",
            );
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

    /// Validate the default-handler host claims against schema and permissions.
    fn validate_default_source_hosts(&self) -> Result<(), ContractError> {
        if !self.default_source_hosts.is_empty() {
            if self.manifest_version < 4 {
                return invalid("default_source_hosts", "requires manifest schema version 4");
            }
            if self.default_source_hosts.len() > 100 {
                return invalid("default_source_hosts", "too many declared hosts");
            }
            let mut claimed = BTreeSet::new();
            for host in &self.default_source_hosts {
                if !is_valid_host_pattern(host) {
                    return invalid("default_source_hosts", "contains an invalid host pattern");
                }
                if claims_every_host(host) {
                    return invalid(
                        "default_source_hosts",
                        "must not claim every host or a whole top-level domain",
                    );
                }
                if !claimed.insert(host.to_ascii_lowercase()) {
                    return invalid("default_source_hosts", "duplicate host");
                }
                // An empty declaration permits every host, mirroring
                // `validate_operator_source_url`, so nothing needs covering.
                if !self.permissions.network_hosts.is_empty()
                    && !self
                        .permissions
                        .network_hosts
                        .iter()
                        .any(|permitted| host_pattern_covers(permitted, host))
                {
                    return invalid(
                        "default_source_hosts",
                        format!("host '{host}' is not declared in permissions.network_hosts"),
                    );
                }
            }
        }
        Ok(())
    }

    /// Whether the package declares a capability.
    #[must_use]
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|item| item.id == capability)
    }

    /// Return the supported media-list option schema, when declared.
    #[must_use]
    pub fn media_list_option_schema(&self) -> Option<&AddonCapability> {
        self.capabilities
            .iter()
            .find(|capability| capability.id == CAPABILITY_MEDIA_LIST_OPTIONS_V1)
    }

    /// Whether the package can perform a bounded media-list source probe.
    #[must_use]
    pub fn has_media_list_source_check_capability(&self) -> bool {
        self.has_capability(CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1)
    }

    /// Serialize the validated option schema into a deterministic JSON snapshot.
    ///
    /// The capability identifier is the schema version. Localized maps use
    /// [`BTreeMap`], while vector order is declaration order, so the returned
    /// bytes are stable and suitable for host-side comparison or hashing.
    ///
    /// # Errors
    ///
    /// Returns an error if the declared schema is invalid or cannot be encoded.
    pub fn media_list_option_schema_snapshot(&self) -> Result<Option<String>, ContractError> {
        let Some(schema) = self.media_list_option_schema() else {
            return Ok(None);
        };
        schema.validate_options(self.manifest_version)?;
        serde_json::to_string(schema)
            .map(Some)
            .map_err(|_| ContractError::Invalid {
                field: "capabilities.options",
                message: "could not serialize the option schema".to_owned(),
            })
    }

    /// Whether the package can produce Remote Stream playback bytes.
    #[must_use]
    pub fn has_remote_stream_play_capability(&self) -> bool {
        self.has_capability(CAPABILITY_REMOTE_STREAM_PLAY_V1)
            || self.has_capability(CAPABILITY_REMOTE_STREAM_PLAY_V2)
            || self.has_capability(CAPABILITY_REMOTE_STREAM_PLAY_V3)
            || self.has_capability(CAPABILITY_REMOTE_STREAM_PLAY_V4)
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

/// One versioned executable or declarative capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonCapability {
    /// Capability identifier ending in `.v<major>`.
    pub id: String,
    /// Ordered host-rendered option definitions for `media-list.options.v1`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<AddonMediaListOptionDefinition>,
}

impl AddonCapability {
    fn validate_options(&self, manifest_version: u32) -> Result<(), ContractError> {
        if self.options.is_empty() {
            if self.id == CAPABILITY_MEDIA_LIST_OPTIONS_V1 {
                return invalid(
                    "capabilities.options",
                    "media-list.options.v1 requires at least one definition",
                );
            }
            return Ok(());
        }
        if manifest_version < 5 {
            return invalid("capabilities.options", "requires manifest schema version 5");
        }
        if self.id != CAPABILITY_MEDIA_LIST_OPTIONS_V1 {
            return invalid(
                "capabilities.options",
                "definitions require the supported media-list.options.v1 capability",
            );
        }
        if self.options.len() > MAX_MEDIA_LIST_OPTION_DEFINITIONS {
            return invalid("capabilities.options", "too many option definitions");
        }
        let mut keys = BTreeSet::new();
        for definition in &self.options {
            definition.validate()?;
            if !keys.insert(definition.key.as_str()) {
                return invalid("capabilities.options", "duplicate option key");
            }
        }
        Ok(())
    }

    /// Validate stored option values against this exact schema and owner scope.
    ///
    /// This validation is intentionally independent from an operation or item
    /// applicability. A finite-only list default remains valid stored state;
    /// effective operation validation decides whether it may be used for one
    /// particular item.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown schemas or keys, type/enum mismatches,
    /// unsupported scopes, or an oversized envelope.
    pub fn validate_stored_option_values(
        &self,
        values: &AddonMediaListOptionValues,
        scope: AddonMediaListOptionValueScope,
    ) -> Result<(), ContractError> {
        self.validate_option_value_envelope(values)?;
        for key in values.values.keys() {
            self.definition_for_stored_value(key, scope)?;
        }
        Ok(())
    }

    fn validate_option_value_envelope(
        &self,
        values: &AddonMediaListOptionValues,
    ) -> Result<(), ContractError> {
        if self.id != CAPABILITY_MEDIA_LIST_OPTIONS_V1 || values.schema != self.id {
            return invalid(
                "media_list_option_values.schema",
                "unsupported option schema",
            );
        }
        self.validate_options(ADDON_MANIFEST_VERSION)?;
        if values.values.len() > MAX_MEDIA_LIST_OPTION_VALUES {
            return invalid("media_list_option_values.values", "too many option values");
        }
        let serialized = serde_json::to_vec(values).map_err(|_| ContractError::Invalid {
            field: "media_list_option_values",
            message: "could not serialize option values".to_owned(),
        })?;
        if serialized.len() > MAX_MEDIA_LIST_OPTION_VALUES_BYTES {
            return invalid(
                "media_list_option_values",
                "serialized values are too large",
            );
        }
        for (key, value) in &values.values {
            let definition = self.definition_for_value(key)?;
            definition.validate_value(value)?;
        }
        Ok(())
    }

    fn definition_for_value(
        &self,
        key: &str,
    ) -> Result<&AddonMediaListOptionDefinition, ContractError> {
        self.options
            .iter()
            .find(|definition| definition.key == key)
            .ok_or_else(|| ContractError::Invalid {
                field: "media_list_option_values.values",
                message: format!("unknown option key '{key}'"),
            })
    }

    fn definition_for_stored_value(
        &self,
        key: &str,
        scope: AddonMediaListOptionValueScope,
    ) -> Result<&AddonMediaListOptionDefinition, ContractError> {
        let definition = self.definition_for_value(key)?;
        if !definition.scope.accepts(scope) {
            return invalid(
                "media_list_option_values.values",
                format!("option '{key}' is not supported at this scope"),
            );
        }
        Ok(definition)
    }

    /// Validate a bounded option-value envelope for one effective operation.
    ///
    /// Storage scope no longer matters after schema defaults, list defaults,
    /// and item overrides have been resolved. Values outside the requested
    /// operation or item applicability are rejected rather than silently ignored.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown schemas or keys, type/enum mismatches,
    /// unsupported contexts, or an oversized envelope.
    pub fn validate_option_values(
        &self,
        values: &AddonMediaListOptionValues,
        operation: AddonMediaListOptionOperation,
        item_is_finite: bool,
    ) -> Result<(), ContractError> {
        self.validate_option_value_envelope(values)?;
        for key in values.values.keys() {
            let definition = self.definition_for_value(key)?;
            if !definition.operation_targets.contains(&operation) {
                return invalid(
                    "media_list_option_values.values",
                    format!("option '{key}' is not supported for this operation"),
                );
            }
            if definition.applicability == AddonMediaListOptionApplicability::FiniteOnly
                && !item_is_finite
            {
                return invalid(
                    "media_list_option_values.values",
                    format!("option '{key}' applies only to finite media"),
                );
            }
        }
        Ok(())
    }
}

/// Where an operator may set one media-list option.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListOptionScope {
    ListDefault,
    ItemOverride,
    Both,
}

impl AddonMediaListOptionScope {
    const fn accepts(self, value_scope: AddonMediaListOptionValueScope) -> bool {
        matches!(self, Self::Both)
            || matches!(
                (self, value_scope),
                (
                    Self::ListDefault,
                    AddonMediaListOptionValueScope::ListDefault
                ) | (
                    Self::ItemOverride,
                    AddonMediaListOptionValueScope::ItemOverride
                )
            )
    }
}

/// Concrete owner of values being validated against an option schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListOptionValueScope {
    ListDefault,
    ItemOverride,
}

/// Bounded operation families to which one option can apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListOptionOperation {
    Enrich,
    Artifact,
    Preparation,
    Playback,
    /// Check access using saved draft options before publication.
    TestAccess,
}

/// Media applicability declared independently from provider identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListOptionApplicability {
    All,
    FiniteOnly,
}

/// Primitive rendered for one provider-neutral media-list option.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListOptionKind {
    Boolean,
    Enum,
}

/// One localized enum choice declared by a package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonMediaListOptionChoice {
    pub value: String,
    pub label: BTreeMap<String, String>,
}

/// One host-rendered media-list option definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonMediaListOptionDefinition {
    pub key: String,
    pub label: BTreeMap<String, String>,
    pub description: BTreeMap<String, String>,
    pub scope: AddonMediaListOptionScope,
    pub operation_targets: Vec<AddonMediaListOptionOperation>,
    pub applicability: AddonMediaListOptionApplicability,
    pub kind: AddonMediaListOptionKind,
    pub default: AddonMediaListOptionValue,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<AddonMediaListOptionChoice>,
    /// When this Boolean option is effectively true, require a prepared artifact.
    /// Only finite-media options targeting preparation may declare this flag.
    /// Omission preserves the serialized identity of existing option schemas.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub requires_preparation: bool,
}

impl AddonMediaListOptionDefinition {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_media_list_option_identifier(&self.key) {
            return invalid(
                "capabilities.options.key",
                "must be bounded lowercase snake case",
            );
        }
        validate_option_localization(&self.label, "capabilities.options.label", 256)?;
        validate_option_localization(&self.description, "capabilities.options.description", 1_024)?;
        if self.operation_targets.is_empty() || self.operation_targets.len() > 5 {
            return invalid(
                "capabilities.options.operation_targets",
                "must contain between one and five operations",
            );
        }
        if self.operation_targets.iter().collect::<BTreeSet<_>>().len()
            != self.operation_targets.len()
        {
            return invalid(
                "capabilities.options.operation_targets",
                "contains a duplicate operation",
            );
        }
        if self.requires_preparation
            && (self.kind != AddonMediaListOptionKind::Boolean
                || self.applicability != AddonMediaListOptionApplicability::FiniteOnly
                || !self
                    .operation_targets
                    .contains(&AddonMediaListOptionOperation::Preparation))
        {
            return invalid(
                "capabilities.options.requires_preparation",
                "requires a finite-only Boolean option targeting preparation",
            );
        }
        match self.kind {
            AddonMediaListOptionKind::Boolean => {
                if !matches!(self.default, AddonMediaListOptionValue::Boolean(_)) {
                    return invalid(
                        "capabilities.options.default",
                        "must be Boolean for a Boolean option",
                    );
                }
                if !self.choices.is_empty() {
                    return invalid(
                        "capabilities.options.choices",
                        "Boolean options cannot declare choices",
                    );
                }
            }
            AddonMediaListOptionKind::Enum => {
                let AddonMediaListOptionValue::Enum(default) = &self.default else {
                    return invalid(
                        "capabilities.options.default",
                        "must be an enum value for an enum option",
                    );
                };
                if self.choices.is_empty() || self.choices.len() > MAX_MEDIA_LIST_OPTION_CHOICES {
                    return invalid(
                        "capabilities.options.choices",
                        "enum options require a bounded non-empty choice list",
                    );
                }
                let mut choices = BTreeSet::new();
                for choice in &self.choices {
                    if !is_media_list_option_identifier(&choice.value) {
                        return invalid(
                            "capabilities.options.choices.value",
                            "must be bounded lowercase snake case",
                        );
                    }
                    validate_option_localization(
                        &choice.label,
                        "capabilities.options.choices.label",
                        256,
                    )?;
                    if !choices.insert(choice.value.as_str()) {
                        return invalid(
                            "capabilities.options.choices",
                            "contains a duplicate value",
                        );
                    }
                }
                if !choices.contains(default.as_str()) {
                    return invalid(
                        "capabilities.options.default",
                        "must match a declared enum choice",
                    );
                }
            }
        }
        Ok(())
    }

    fn validate_value(&self, value: &AddonMediaListOptionValue) -> Result<(), ContractError> {
        match (self.kind, value) {
            (AddonMediaListOptionKind::Boolean, AddonMediaListOptionValue::Boolean(_)) => Ok(()),
            (AddonMediaListOptionKind::Enum, AddonMediaListOptionValue::Enum(value))
                if self.choices.iter().any(|choice| choice.value == *value) =>
            {
                Ok(())
            }
            (AddonMediaListOptionKind::Enum, AddonMediaListOptionValue::Enum(_)) => invalid(
                "media_list_option_values.values",
                "contains an undeclared enum value",
            ),
            _ => invalid(
                "media_list_option_values.values",
                "contains a value of the wrong type",
            ),
        }
    }
}

/// Typed Boolean or enum value; JSON/TOML retain the primitive type directly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AddonMediaListOptionValue {
    Boolean(bool),
    Enum(String),
}

/// Values bound to one exact media-list option capability version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonMediaListOptionValues {
    pub schema: String,
    #[serde(default)]
    pub values: BTreeMap<String, AddonMediaListOptionValue>,
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
    /// Executable-setting keys whose external tools may be updated by the package.
    /// Empty declarations are omitted to preserve legacy permission fingerprints.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_update: Vec<String>,
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
        if self.tool_update.len() > MAX_TOOL_UPDATE_PERMISSIONS {
            return invalid("permissions.tool_update", "too many executable settings");
        }
        let mut tool_keys = BTreeSet::new();
        for key in &self.tool_update {
            if !is_setting_key(key) || !tool_keys.insert(key.as_str()) {
                return invalid(
                    "permissions.tool_update",
                    "invalid or duplicate setting key",
                );
            }
        }
        if !self
            .network_hosts
            .iter()
            .all(|host| is_valid_host_pattern(host))
        {
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
    Local,
}

impl AddonTrustTier {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Sideloaded => "sideloaded",
            Self::Local => "local",
        }
    }
}

/// Strict metadata at the root of a local repository checkout or offline ZIP.
///
/// The repository contains add-ons below `addons/<id>/`. It deliberately has
/// no URL, signing key, or credential field: acquiring a Git checkout remains
/// an operator action and an offline bundle is self-contained.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalRepositoryManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: BTreeMap<String, String>,
}

impl LocalRepositoryManifest {
    /// Validate repository identity and localized display metadata.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported schema, invalid identity, empty
    /// name, or a description without the required English fallback.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != LOCAL_ADDON_REPOSITORY_SCHEMA_VERSION {
            return invalid(
                "schema_version",
                "unsupported local repository schema version",
            );
        }
        validate_addon_id(&self.id)?;
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return invalid("name", "must be between 1 and 200 characters");
        }
        if !self.description.is_empty() {
            require_english(&self.description, "description")?;
        }
        Ok(())
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_check_tool_update"
    )]
    pub tool_update: Option<AddonToolUpdateWarning>,
}

/// Bounded display context for a stale-tool update warning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonToolUpdateWarning {
    pub program: String,
    pub installed_version: String,
    pub latest_version: String,
}

impl AddonToolUpdateWarning {
    /// Validate display-only identifiers without paths or diagnostic separators.
    /// # Errors
    /// Rejects empty, oversized or unsafe program names and versions.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.program.trim().is_empty()
            || self.program != self.program.trim()
            || self.program.len() > 80
            || !self
                .program
                .chars()
                .all(|c| c.is_alphanumeric() || " -_.+()".contains(c))
        {
            return invalid("tool update program", "requires a bounded display name");
        }
        validate_tool_version(&self.installed_version)?;
        validate_tool_version(&self.latest_version)
    }
}

// Invalid optional context must not discard an otherwise valid readiness result.
fn deserialize_check_tool_update<'de, D>(
    deserializer: D,
) -> Result<Option<AddonToolUpdateWarning>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value::<AddonToolUpdateWarning>(value)
        .ok()
        .filter(|context| context.validate().is_ok()))
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

#[must_use]
pub fn network_host_pattern_matches(pattern: &str, host: &str) -> bool {
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

/// Reject host declarations that are empty, oversized, or not a bare host.
#[must_use]
fn is_valid_host_pattern(host: &str) -> bool {
    !(host.is_empty()
        || host.len() > 255
        || host.contains("//")
        || host.contains('/')
        || host.contains(char::is_whitespace)
        || host.chars().any(char::is_control))
}

/// Report whether a pattern claims every host or a whole top-level domain.
///
/// `permissions.network_hosts` may legitimately be permissive, but a package
/// that claimed `*` or `*.com` as its default handler would win auto-selection
/// for unrelated providers.
#[must_use]
fn claims_every_host(pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern
        .strip_prefix("*.")
        .or_else(|| pattern.strip_prefix('.'))
    {
        // A single-label suffix is a whole top-level domain.
        Some(suffix) => !suffix.contains('.'),
        None => false,
    }
}

/// Report whether `outer` permits everything `inner` claims.
#[must_use]
fn host_pattern_covers(outer: &str, inner: &str) -> bool {
    let outer = outer.to_ascii_lowercase();
    let inner = inner.to_ascii_lowercase();
    if outer == "*" || outer == inner {
        return true;
    }
    let Some(outer_suffix) = outer.strip_prefix("*.").or_else(|| outer.strip_prefix('.')) else {
        return false;
    };
    let inner_host = inner
        .strip_prefix("*.")
        .or_else(|| inner.strip_prefix('.'))
        .unwrap_or(inner.as_str());
    network_host_pattern_matches(&format!("*.{outer_suffix}"), inner_host)
}

/// The outcome of matching one operator source URL against declared handlers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceAddonMatch {
    /// No installed candidate claims the host.
    None,
    /// Exactly one candidate claims the host most specifically.
    Declared(String),
    /// Several candidates claim the host equally well; the host must not guess.
    Ambiguous(Vec<String>),
}

/// Select the add-on that declares itself the default handler for `host`.
///
/// Candidates are `(addon_id, default_source_hosts)` pairs. An exact host
/// declaration outranks a wildcard, and a longer wildcard suffix outranks a
/// shorter one. A genuine tie returns [`SourceAddonMatch::Ambiguous`] so the
/// caller can ask the operator instead of picking arbitrarily.
#[must_use]
pub fn match_declared_source_host(
    host: &str,
    candidates: &[(&str, &[String])],
) -> SourceAddonMatch {
    let mut best_score = 0usize;
    let mut winners: Vec<String> = Vec::new();
    for (addon_id, hosts) in candidates {
        let Some(score) = hosts
            .iter()
            .filter_map(|pattern| declared_host_score(pattern, host))
            .max()
        else {
            continue;
        };
        if score > best_score {
            best_score = score;
            winners.clear();
            winners.push((*addon_id).to_owned());
        } else if score == best_score {
            winners.push((*addon_id).to_owned());
        }
    }
    match winners.len() {
        0 => SourceAddonMatch::None,
        1 => SourceAddonMatch::Declared(winners.remove(0)),
        _ => {
            winners.sort_unstable();
            SourceAddonMatch::Ambiguous(winners)
        }
    }
}

/// Score how specifically one declaration claims a host; `None` means no match.
///
/// An exact declaration always outranks any wildcard, and among wildcards a
/// longer suffix wins.
fn declared_host_score(pattern: &str, host: &str) -> Option<usize> {
    if !network_host_pattern_matches(pattern, host) {
        return None;
    }
    match pattern
        .strip_prefix("*.")
        .or_else(|| pattern.strip_prefix('.'))
    {
        Some(suffix) => Some(suffix.len() + 1),
        None => Some(usize::MAX),
    }
}

/// Parse and validate one bounded `media-list.source-check.v1` JSON result.
///
/// # Errors
///
/// Returns an error when the output is oversized, malformed, or contradicts
/// its typed outcome.
pub fn parse_media_list_source_check_output(
    output: &[u8],
) -> Result<MediaListSourceCheckResult, ContractError> {
    if output.len() > MAX_MEDIA_LIST_SOURCE_CHECK_OUTPUT_BYTES {
        return invalid(
            "media list source check output",
            "exceeds the 65536-byte limit",
        );
    }
    let result = serde_json::from_slice::<MediaListSourceCheckResult>(output).map_err(|error| {
        ContractError::Invalid {
            field: "media list source check output",
            message: error.to_string(),
        }
    })?;
    result.validate()?;
    Ok(result)
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

/// Parse and validate one `media-list.import.v1 discover` NDJSON page.
///
/// The first output line is a page control record. A first page then carries
/// one list header followed by item stubs; continuation pages carry only item
/// records. Cross-page identity, rank, cursor-loop, and revision checks remain
/// the durable host job's responsibility.
///
/// # Errors
///
/// Returns an error when output is malformed, oversized, internally
/// inconsistent, or violates the page bounds.
pub fn conform_media_list_discover_output(
    output: &[u8],
    first_page: bool,
) -> Result<MediaListDiscoveryPage, ContractError> {
    if output.len() > MAX_MEDIA_LIST_IMPORT_PAGE_OUTPUT_BYTES {
        return invalid("media list discover output", "exceeds the 4 MiB limit");
    }
    let text = std::str::from_utf8(output).map_err(|_| ContractError::Invalid {
        field: "media list discover output",
        message: "must be UTF-8".to_owned(),
    })?;
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let control_line = lines.first().ok_or_else(|| ContractError::Invalid {
        field: "media list discover output",
        message: "is missing a page control record".to_owned(),
    })?;
    let control = parse_media_list_discover_control(control_line)?;
    let MediaListDiscoverControl::Page {
        complete,
        next_cursor,
        total_hint,
        revision_token,
    } = control;

    let mut records = Vec::new();
    let mut provider_ids = BTreeSet::new();
    let mut ranks = BTreeSet::new();
    let mut list_headers = 0usize;
    for (index, line) in lines.iter().skip(1).enumerate() {
        let record = serde_json::from_str::<AddonMediaListRecord>(line)
            .map_err(|error| record_parse_error("media list discover output", index + 1, &error))?;
        record.validate()?;
        match &record {
            AddonMediaListRecord::List { .. } => {
                list_headers += 1;
                if !first_page || !records.is_empty() || list_headers > 1 {
                    return invalid(
                        "media list discover output",
                        "a list header may only be the first payload record on the first page",
                    );
                }
            }
            AddonMediaListRecord::Item {
                provider_id, rank, ..
            } => {
                if first_page && list_headers == 0 {
                    return invalid(
                        "media list discover output",
                        "the first page must carry a list header before items",
                    );
                }
                if !provider_ids.insert(provider_id.clone()) || !ranks.insert(*rank) {
                    return invalid(
                        "media list discover output",
                        "item provider_id and rank must be unique within the page",
                    );
                }
                if provider_ids.len() > MAX_MEDIA_LIST_IMPORT_PAGE_ITEMS {
                    return invalid("media list discover output", "contains more than 250 items");
                }
            }
        }
        records.push(record);
    }
    if first_page && list_headers != 1 {
        return invalid(
            "media list discover output",
            "the first page must contain exactly one list header",
        );
    }
    if !complete && provider_ids.is_empty() {
        return invalid(
            "media list discover output",
            "an incomplete page must make item progress",
        );
    }
    Ok(MediaListDiscoveryPage {
        complete,
        next_cursor,
        total_hint,
        revision_token,
        records,
    })
}

/// Parse and validate one `media-list.import.v2 discover` NDJSON page.
///
/// The request supplies the import mode, current cursor, page bounds, and the
/// input-only staged archive descriptor. Full generations retain the v1 list
/// header rule. The first incremental page may omit or refresh the list header
/// and may validly emit no items when every examined identity was archived.
///
/// # Errors
///
/// Returns an error when the request or output is malformed, oversized,
/// internally inconsistent, or fails to make cursor or item progress.
#[allow(clippy::too_many_lines)] // Keep page framing, record validation, and count checks together.
pub fn conform_media_list_discover_v2_output(
    output: &[u8],
    request: &MediaListDiscoverV2Request,
) -> Result<MediaListDiscoveryV2Page, ContractError> {
    request.validate()?;
    let requested_output_limit =
        usize::try_from(request.limits.max_output_bytes).map_err(|_| ContractError::Invalid {
            field: "media list discover v2 max_output_bytes",
            message: "does not fit the host address space".to_owned(),
        })?;
    if output.len() > requested_output_limit {
        return invalid(
            "media list discover v2 output",
            "exceeds the requested output limit",
        );
    }
    let text = std::str::from_utf8(output).map_err(|_| ContractError::Invalid {
        field: "media list discover v2 output",
        message: "must be UTF-8".to_owned(),
    })?;
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let control_line = lines.first().ok_or_else(|| ContractError::Invalid {
        field: "media list discover v2 output",
        message: "is missing a page control record".to_owned(),
    })?;
    let control = parse_media_list_discover_v2_control(control_line, request)?;
    let MediaListDiscoverV2Control::Page {
        complete,
        next_cursor,
        total_hint,
        revision_token,
        strategy,
        examined_count,
        archived_skipped_count,
        emitted_count,
    } = control;

    let first_page = request.cursor.is_none();
    let mut records = Vec::new();
    let mut provider_ids = BTreeSet::new();
    let mut ranks = BTreeSet::new();
    let mut list_headers = 0usize;
    for (index, line) in lines.iter().skip(1).enumerate() {
        let record = serde_json::from_str::<AddonMediaListRecord>(line).map_err(|error| {
            record_parse_error("media list discover v2 output", index + 1, &error)
        })?;
        record.validate()?;
        match &record {
            AddonMediaListRecord::List { .. } => {
                list_headers += 1;
                if !first_page || !records.is_empty() || list_headers > 1 {
                    return invalid(
                        "media list discover v2 output",
                        "a list header may only be the first payload record on the first page",
                    );
                }
            }
            AddonMediaListRecord::Item {
                provider_id, rank, ..
            } => {
                if request.mode == MediaListImportMode::Full && first_page && list_headers == 0 {
                    return invalid(
                        "media list discover v2 output",
                        "the first full page must carry a list header before items",
                    );
                }
                if !provider_ids.insert(provider_id.clone()) || !ranks.insert(*rank) {
                    return invalid(
                        "media list discover v2 output",
                        "item provider_id and rank must be unique within the page",
                    );
                }
                if provider_ids.len() > usize::from(request.limits.max_items) {
                    return invalid(
                        "media list discover v2 output",
                        "contains more items than the requested page limit",
                    );
                }
            }
        }
        records.push(record);
    }
    if request.mode == MediaListImportMode::Full && first_page && list_headers != 1 {
        return invalid(
            "media list discover v2 output",
            "the first full page must contain exactly one list header",
        );
    }
    if u32::try_from(provider_ids.len()).ok() != Some(emitted_count) {
        return invalid(
            "media list discover v2 emitted_count",
            "must equal the number of emitted item records",
        );
    }
    if !complete && emitted_count == 0 && examined_count == 0 {
        return invalid(
            "media list discover v2 output",
            "an incomplete page must make examined or item progress",
        );
    }

    Ok(MediaListDiscoveryV2Page {
        complete,
        next_cursor,
        total_hint,
        revision_token,
        strategy,
        examined_count,
        archived_skipped_count,
        emitted_count,
        records,
    })
}

fn parse_media_list_discover_v2_control(
    control_line: &str,
    request: &MediaListDiscoverV2Request,
) -> Result<MediaListDiscoverV2Control, ContractError> {
    let control = serde_json::from_str::<MediaListDiscoverV2Control>(control_line)
        .map_err(|error| record_parse_error("media list discover v2 output", 0, &error))?;
    let MediaListDiscoverV2Control::Page {
        complete,
        next_cursor,
        total_hint,
        revision_token,
        examined_count,
        archived_skipped_count,
        emitted_count,
        ..
    } = &control;
    validate_import_cursor(next_cursor.as_deref())?;
    if *complete == next_cursor.is_some() {
        return invalid(
            "media list discover v2 page",
            "complete pages must omit next_cursor and incomplete pages must provide it",
        );
    }
    if next_cursor.is_some() && next_cursor.as_deref() == request.cursor.as_deref() {
        return invalid(
            "media list discover v2 next_cursor",
            "must advance beyond the request cursor",
        );
    }
    if total_hint.is_some_and(|value| {
        usize::try_from(value)
            .ok()
            .is_none_or(|value| value > MAX_LIST_ITEMS)
    }) {
        return invalid(
            "media list discover v2 total_hint",
            "must not exceed 10000 items",
        );
    }
    if revision_token.as_ref().is_some_and(|value| {
        value.trim().is_empty()
            || value.len() > MAX_PROVIDER_ID_BYTES
            || value.chars().any(char::is_control)
    }) {
        return invalid(
            "media list discover v2 revision_token",
            "must be 1 to 384 UTF-8 bytes without control characters",
        );
    }
    let Some(classified_count) = archived_skipped_count.checked_add(*emitted_count) else {
        return invalid("media list discover v2 counts", "must not overflow");
    };
    if *examined_count != classified_count {
        return invalid(
            "media list discover v2 counts",
            "examined_count must equal archived_skipped_count plus emitted_count",
        );
    }
    if *examined_count > request.archive.max_entries {
        return invalid(
            "media list discover v2 examined_count",
            "must not exceed the requested archive entry limit",
        );
    }
    if *examined_count > u32::from(request.limits.max_items) {
        return invalid(
            "media list discover v2 examined_count",
            "must not exceed the requested page item limit",
        );
    }
    if *emitted_count > u32::from(request.limits.max_items) {
        return invalid(
            "media list discover v2 emitted_count",
            "must not exceed the requested page item limit",
        );
    }
    if request.mode == MediaListImportMode::Full && *archived_skipped_count != 0 {
        return invalid(
            "media list discover v2 archived_skipped_count",
            "must be zero for a full generation",
        );
    }
    Ok(control)
}

fn parse_media_list_discover_control(
    control_line: &str,
) -> Result<MediaListDiscoverControl, ContractError> {
    let control = serde_json::from_str::<MediaListDiscoverControl>(control_line)
        .map_err(|error| record_parse_error("media list discover output", 0, &error))?;
    let MediaListDiscoverControl::Page {
        complete,
        next_cursor,
        total_hint,
        revision_token,
    } = &control;
    validate_import_cursor(next_cursor.as_deref())?;
    if *complete == next_cursor.is_some() {
        return invalid(
            "media list discover page",
            "complete pages must omit next_cursor and incomplete pages must provide it",
        );
    }
    if total_hint.is_some_and(|value| {
        usize::try_from(value)
            .ok()
            .is_none_or(|value| value > MAX_LIST_ITEMS)
    }) {
        return invalid(
            "media list discover total_hint",
            "must not exceed 10000 items",
        );
    }
    if revision_token.as_ref().is_some_and(|value| {
        value.trim().is_empty()
            || value.len() > MAX_PROVIDER_ID_BYTES
            || value.chars().any(char::is_control)
    }) {
        return invalid(
            "media list discover revision_token",
            "must be 1 to 384 UTF-8 bytes without control characters",
        );
    }
    Ok(control)
}

/// Parse and validate one `media-list.import.v1 enrich` NDJSON result.
///
/// Complete and known-unavailable outcomes carry exactly one item record for
/// the requested identity. Failure outcomes carry no item, so failures can
/// never silently replace or omit the committed overview data.
///
/// # Errors
///
/// Returns an error when the response is malformed, oversized, has the wrong
/// identity, or contradicts its typed outcome.
pub fn conform_media_list_enrich_output(
    output: &[u8],
    expected_provider_id: &str,
) -> Result<MediaListEnrichmentResult, ContractError> {
    if output.len() > MAX_MEDIA_LIST_IMPORT_ITEM_OUTPUT_BYTES {
        return invalid("media list enrich output", "exceeds the 1 MiB limit");
    }
    validate_provider_id(expected_provider_id)?;
    let text = std::str::from_utf8(output).map_err(|_| ContractError::Invalid {
        field: "media list enrich output",
        message: "must be UTF-8".to_owned(),
    })?;
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let control_line = lines.first().ok_or_else(|| ContractError::Invalid {
        field: "media list enrich output",
        message: "is missing an outcome control record".to_owned(),
    })?;
    let control = parse_media_list_enrich_control(control_line, lines.len())?;
    let MediaListEnrichControl::Outcome {
        outcome,
        code,
        message,
        retry_after_seconds,
    } = control;
    let expects_item = matches!(
        outcome,
        MediaListEnrichOutcomeKind::Complete | MediaListEnrichOutcomeKind::Unavailable
    );
    let item = if expects_item {
        Some(parse_media_list_enrichment_item(
            lines[1],
            expected_provider_id,
            outcome,
        )?)
    } else {
        None
    };
    Ok(MediaListEnrichmentResult {
        outcome,
        code,
        message,
        retry_after_seconds,
        item,
    })
}

fn parse_media_list_enrich_control(
    control_line: &str,
    line_count: usize,
) -> Result<MediaListEnrichControl, ContractError> {
    let control = serde_json::from_str::<MediaListEnrichControl>(control_line)
        .map_err(|error| record_parse_error("media list enrich output", 0, &error))?;
    let MediaListEnrichControl::Outcome {
        outcome,
        code,
        message,
        retry_after_seconds,
    } = &control;
    if code.as_ref().is_some_and(|value| {
        value.is_empty()
            || value.len() > 128
            || value
                .bytes()
                .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
    }) {
        return invalid(
            "media list enrich outcome code",
            "must use 1 to 128 lowercase ASCII code characters",
        );
    }
    if message
        .as_ref()
        .is_some_and(|value| value.len() > MAX_OPERATION_ERROR_MESSAGE_BYTES)
    {
        return invalid(
            "media list enrich outcome message",
            "exceeds the 1024-byte limit",
        );
    }
    if retry_after_seconds.is_some_and(|value| value > 86_400) {
        return invalid(
            "media list enrich retry_after_seconds",
            "must not exceed 86400 seconds",
        );
    }

    let expects_item = matches!(
        outcome,
        MediaListEnrichOutcomeKind::Complete | MediaListEnrichOutcomeKind::Unavailable
    );
    if line_count != if expects_item { 2 } else { 1 } {
        return invalid(
            "media list enrich output",
            if expects_item {
                "complete and unavailable outcomes require exactly one item record"
            } else {
                "failure outcomes must not carry item records"
            },
        );
    }
    if !matches!(outcome, MediaListEnrichOutcomeKind::TransientFailure)
        && retry_after_seconds.is_some()
    {
        return invalid(
            "media list enrich retry_after_seconds",
            "is only valid for transient_failure",
        );
    }
    if matches!(
        outcome,
        MediaListEnrichOutcomeKind::TransientFailure | MediaListEnrichOutcomeKind::PermanentFailure
    ) && code.is_none()
    {
        return invalid(
            "media list enrich outcome code",
            "is required for failure outcomes",
        );
    }
    Ok(control)
}

fn parse_media_list_enrichment_item(
    line: &str,
    expected_provider_id: &str,
    outcome: MediaListEnrichOutcomeKind,
) -> Result<AddonMediaListRecord, ContractError> {
    let record = serde_json::from_str::<AddonMediaListRecord>(line)
        .map_err(|error| record_parse_error("media list enrich output", 1, &error))?;
    record.validate()?;
    match &record {
        AddonMediaListRecord::Item {
            provider_id,
            availability,
            ..
        } if provider_id == expected_provider_id => {
            if matches!(outcome, MediaListEnrichOutcomeKind::Unavailable)
                && !matches!(availability, AddonMediaListItemAvailability::Unavailable)
            {
                return invalid(
                    "media list enrich unavailable item",
                    "must report unavailable availability",
                );
            }
        }
        AddonMediaListRecord::Item { .. } => {
            return invalid(
                "media list enrich item",
                "provider_id does not match the requested identity",
            );
        }
        AddonMediaListRecord::List { .. } => {
            return invalid("media list enrich item", "must be an item record");
        }
    }
    Ok(record)
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

fn is_media_list_option_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MEDIA_LIST_OPTION_IDENTIFIER_BYTES
        && !value.starts_with('_')
        && !value.ends_with('_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn validate_option_localization(
    values: &BTreeMap<String, String>,
    field: &'static str,
    max_value_bytes: usize,
) -> Result<(), ContractError> {
    require_english(values, field)?;
    if values.len() > 16
        || values.iter().any(|(culture, value)| {
            culture.is_empty()
                || culture.len() > 32
                || culture.chars().any(char::is_control)
                || value.trim().is_empty()
                || value.len() > max_value_bytes
                || value.chars().any(char::is_control)
        })
    {
        return invalid(field, "contains unsafe or excessive localized text");
    }
    Ok(())
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

    #[test]
    fn check_tool_update_context_preserves_legacy_and_rejects_invalid_details() {
        let legacy =
            r#"{"status":"warning","code":"tool-update-failed-stale","message":"still usable"}"#;
        let result: AddonCheckResult = serde_json::from_str(legacy).unwrap();
        assert!(result.tool_update.is_none());
        assert_eq!(serde_json::to_string(&result).unwrap(), legacy);
        let context = AddonToolUpdateWarning {
            program: "Example Tool (+)".to_owned(),
            installed_version: "2026.01.01".to_owned(),
            latest_version: "2026.09.01".to_owned(),
        };
        assert!(context.validate().is_ok());
        let mut extended = result.clone();
        extended.tool_update = Some(context.clone());
        assert_eq!(
            serde_json::from_str::<AddonCheckResult>(&serde_json::to_string(&extended).unwrap())
                .unwrap(),
            extended
        );
        for invalid in [
            serde_json::json!(42),
            serde_json::json!({"program":"missing versions"}),
            serde_json::json!({"program":"tool; other warning", "installed_version":"1", "latest_version":"2"}),
        ] {
            let mut value = serde_json::to_value(&result).unwrap();
            value["tool_update"] = invalid;
            assert_eq!(
                serde_json::from_value::<AddonCheckResult>(value).unwrap(),
                result
            );
        }
        for program in [
            "",
            " tool",
            "tool ",
            "tool/path",
            "tool:bad",
            "tool; bad",
            "tool\nname",
        ] {
            let mut invalid = context.clone();
            invalid.program = program.to_owned();
            assert!(invalid.validate().is_err());
        }
        let mut invalid = context;
        invalid.latest_version = "2; injected".to_owned();
        assert!(invalid.validate().is_err());
    }

    fn localized(value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("en-US".to_owned(), value.to_owned())])
    }

    #[test]
    fn manifest_recognizes_retained_play_v4_without_legacy_playback() {
        let mut manifest = valid_manifest();
        assert!(!manifest.has_remote_stream_play_capability());
        manifest.capabilities.push(AddonCapability {
            id: CAPABILITY_REMOTE_STREAM_PLAY_V4.to_owned(),
            options: Vec::new(),
        });
        assert!(manifest.has_remote_stream_play_capability());
        manifest.validate().unwrap();
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
                options: Vec::new(),
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
            default_source_hosts: Vec::new(),
            panels: Vec::new(),
        }
    }

    fn boolean_option(key: &str) -> AddonMediaListOptionDefinition {
        AddonMediaListOptionDefinition {
            key: key.to_owned(),
            label: localized("Remove segments"),
            description: localized("Prepare finite media without marked segments."),
            scope: AddonMediaListOptionScope::Both,
            operation_targets: vec![
                AddonMediaListOptionOperation::Preparation,
                AddonMediaListOptionOperation::Playback,
            ],
            applicability: AddonMediaListOptionApplicability::FiniteOnly,
            kind: AddonMediaListOptionKind::Boolean,
            default: AddonMediaListOptionValue::Boolean(true),
            choices: Vec::new(),
            requires_preparation: false,
        }
    }

    #[test]
    fn tool_update_requires_v6_capability_permission_and_executable_setting() {
        let mut manifest = valid_manifest();
        manifest.manifest_version = 5;
        manifest.validate().unwrap();
        manifest.capabilities.push(AddonCapability {
            id: CAPABILITY_TOOL_UPDATE_V1.to_owned(),
            options: Vec::new(),
        });
        assert!(manifest.validate().is_err());
        manifest.manifest_version = 6;
        assert!(manifest.validate().is_err());
        manifest.permissions.tool_update.push("TOOL_BIN".to_owned());
        assert!(manifest.validate().is_err());
        manifest.settings.push(AddonSettingDefinition {
            key: "TOOL_BIN".to_owned(),
            label: localized("Tool executable"),
            kind: AddonSettingKind::String,
            required: true,
            default: None,
            options: Vec::new(),
            suggested_reference: None,
        });
        assert!(manifest.validate().is_err());
        manifest.settings[0].kind = AddonSettingKind::Executable;
        manifest.validate().unwrap();
        manifest.manifest_version = 5;
        assert!(manifest.validate().is_err());
        manifest.manifest_version = 6;
        manifest.capabilities.pop();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn tool_update_permissions_preserve_old_bytes_and_reject_duplicates_or_bad_keys() {
        let old = r#"{"network_hosts":[],"media_read":false,"managed_ffmpeg":false,"external_commands":[]}"#;
        let mut permissions: AddonPermissions = serde_json::from_str(old).unwrap();
        assert_eq!(serde_json::to_string(&permissions).unwrap(), old);
        permissions.tool_update = vec!["TOOL_BIN".to_owned()];
        permissions.validate().unwrap();
        assert_ne!(serde_json::to_string(&permissions).unwrap(), old);
        permissions.tool_update.push("TOOL_BIN".to_owned());
        assert!(permissions.validate().is_err());
        permissions.tool_update = vec!["bin/tool".to_owned()];
        assert!(permissions.validate().is_err());
        permissions.tool_update = (0..=MAX_TOOL_UPDATE_PERMISSIONS)
            .map(|index| format!("TOOL_{index}"))
            .collect();
        assert!(permissions.validate().is_err());
    }

    fn enum_option(key: &str) -> AddonMediaListOptionDefinition {
        AddonMediaListOptionDefinition {
            key: key.to_owned(),
            label: localized("Browser profile"),
            description: localized("Select a browser family without exposing credential data."),
            scope: AddonMediaListOptionScope::Both,
            operation_targets: vec![
                AddonMediaListOptionOperation::Enrich,
                AddonMediaListOptionOperation::Artifact,
                AddonMediaListOptionOperation::Preparation,
                AddonMediaListOptionOperation::Playback,
            ],
            applicability: AddonMediaListOptionApplicability::All,
            kind: AddonMediaListOptionKind::Enum,
            default: AddonMediaListOptionValue::Enum("none".to_owned()),
            requires_preparation: false,
            choices: vec![
                AddonMediaListOptionChoice {
                    value: "none".to_owned(),
                    label: localized("None"),
                },
                AddonMediaListOptionChoice {
                    value: "firefox".to_owned(),
                    label: localized("Firefox"),
                },
            ],
        }
    }

    fn manifest_with_option_schema() -> AddonManifest {
        let mut manifest = valid_manifest();
        manifest.manifest_version = 5;
        manifest.capabilities.extend([
            AddonCapability {
                id: CAPABILITY_MEDIA_LIST_LIST_V6.to_owned(),
                options: Vec::new(),
            },
            AddonCapability {
                id: CAPABILITY_MEDIA_LIST_OPTIONS_V1.to_owned(),
                options: vec![
                    enum_option("browser_profile"),
                    boolean_option("remove_segments"),
                ],
            },
        ]);
        manifest
    }

    fn media_list_v2_request(
        mode: MediaListImportMode,
        cursor: Option<&str>,
    ) -> MediaListDiscoverV2Request {
        MediaListDiscoverV2Request {
            source_url: "https://example.test/programmes".to_owned(),
            record_capability: CAPABILITY_MEDIA_LIST_LIST_V6.to_owned(),
            mode,
            archive: MediaListArchiveDescriptor {
                staged_path: "private/import/archive.txt".to_owned(),
                generation: 1,
                entry_count: 0,
                sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_owned(),
                max_bytes: u32::try_from(MAX_MEDIA_LIST_IMPORT_ARCHIVE_BYTES)
                    .expect("archive limit fits in u32"),
                max_entries: u32::try_from(MAX_MEDIA_LIST_IMPORT_ARCHIVE_ENTRIES)
                    .expect("archive entry limit fits in u32"),
            },
            cursor: cursor.map(str::to_owned),
            limits: MediaListDiscoverLimits::default(),
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
    fn media_list_import_v2_manifest_requires_a_list_record_schema() {
        let mut manifest = valid_manifest();
        manifest.capabilities.push(AddonCapability {
            id: CAPABILITY_MEDIA_LIST_IMPORT_V2.to_owned(),
            options: Vec::new(),
        });
        assert!(manifest.validate().is_err());

        manifest.capabilities.push(AddonCapability {
            id: CAPABILITY_MEDIA_LIST_LIST_V6.to_owned(),
            options: Vec::new(),
        });
        manifest
            .validate()
            .expect("import v2 with a supported list record schema");
    }

    #[test]
    fn schema_v5_round_trips_a_versioned_media_list_option_schema() {
        let manifest = manifest_with_option_schema();
        manifest.validate().expect("valid option schema");

        let document = toml::to_string(&manifest).expect("manifest TOML");
        let reparsed = toml::from_str::<AddonManifest>(&document).expect("parse manifest TOML");
        assert_eq!(reparsed, manifest);
        assert_eq!(
            manifest.media_list_option_schema_snapshot().unwrap(),
            reparsed.media_list_option_schema_snapshot().unwrap()
        );

        let mut legacy = valid_manifest();
        legacy.manifest_version = 4;
        let legacy_document = toml::to_string(&legacy).expect("legacy manifest TOML");
        assert!(!legacy_document.contains("options"));
        toml::from_str::<AddonManifest>(&legacy_document)
            .expect("legacy manifest remains readable")
            .validate()
            .expect("legacy manifest remains valid");
    }

    #[test]
    fn source_check_capability_requires_a_supported_media_list_schema() {
        let mut manifest = valid_manifest();
        manifest.capabilities.push(AddonCapability {
            id: CAPABILITY_MEDIA_LIST_SOURCE_CHECK_V1.to_owned(),
            options: Vec::new(),
        });
        assert!(manifest.validate().is_err());

        manifest.capabilities.push(AddonCapability {
            id: CAPABILITY_MEDIA_LIST_LIST_V6.to_owned(),
            options: Vec::new(),
        });
        manifest
            .validate()
            .expect("source check is paired with a media-list schema");
        assert!(manifest.has_media_list_source_check_capability());
    }

    #[test]
    fn source_check_request_and_success_result_are_bounded_and_round_trip() {
        let request = MediaListSourceCheckRequest {
            source_url: "https://example.test/feed".to_owned(),
            validator: Some("opaque-etag".to_owned()),
        };
        request.validate().expect("bounded request");
        let encoded = serde_json::to_vec(&request).expect("serialize request");
        assert_eq!(
            std::str::from_utf8(&encoded).unwrap(),
            r#"{"source_url":"https://example.test/feed","validator":"opaque-etag"}"#
        );
        assert_eq!(
            serde_json::from_slice::<MediaListSourceCheckRequest>(&encoded).unwrap(),
            request
        );

        let result = MediaListSourceCheckResult {
            outcome: MediaListSourceCheckOutcome::Changed,
            validator: Some("next-opaque-etag".to_owned()),
            source_updated_at: vec![
                "2026-09-03T12:00:00Z".to_owned(),
                "2026-09-04T12:00:00+00:00".to_owned(),
            ],
            code: None,
            retry_after_seconds: None,
        };
        let encoded = serde_json::to_vec(&result).expect("serialize source-check result");
        assert_eq!(
            std::str::from_utf8(&encoded).unwrap(),
            r#"{"outcome":"changed","validator":"next-opaque-etag","source_updated_at":["2026-09-03T12:00:00Z","2026-09-04T12:00:00+00:00"]}"#
        );
        assert_eq!(
            parse_media_list_source_check_output(&encoded).unwrap(),
            result
        );

        let mut oversized_validator = request;
        oversized_validator.validator =
            Some("x".repeat(MAX_MEDIA_LIST_SOURCE_CHECK_VALIDATOR_BYTES + 1));
        assert!(oversized_validator.validate().is_err());

        let mut too_many_instants = result;
        too_many_instants.source_updated_at = (0..=MAX_MEDIA_LIST_SOURCE_UPDATE_INSTANTS)
            .map(|second| format!("2026-09-04T12:00:{second:02}Z"))
            .collect();
        assert!(too_many_instants.validate().is_err());
    }

    #[test]
    fn source_check_result_rejects_contradictory_or_unsafe_fields() {
        let transient = MediaListSourceCheckResult {
            outcome: MediaListSourceCheckOutcome::TransientFailure,
            validator: None,
            source_updated_at: Vec::new(),
            code: Some("provider-rate-limited".to_owned()),
            retry_after_seconds: Some(120),
        };
        transient.validate().expect("bounded transient retry hint");

        let mut permanent_with_retry = transient.clone();
        permanent_with_retry.outcome = MediaListSourceCheckOutcome::PermanentFailure;
        assert!(permanent_with_retry.validate().is_err());

        let mut failure_with_validator = transient;
        failure_with_validator.validator = Some("must-not-survive".to_owned());
        assert!(failure_with_validator.validate().is_err());

        let changed_with_failure = MediaListSourceCheckResult {
            outcome: MediaListSourceCheckOutcome::Changed,
            validator: None,
            source_updated_at: Vec::new(),
            code: Some("contradictory".to_owned()),
            retry_after_seconds: None,
        };
        assert!(changed_with_failure.validate().is_err());

        let non_utc = MediaListSourceCheckResult {
            outcome: MediaListSourceCheckOutcome::Unchanged,
            validator: None,
            source_updated_at: vec!["2026-09-04T13:00:00+01:00".to_owned()],
            code: None,
            retry_after_seconds: None,
        };
        assert!(non_utc.validate().is_err());

        let oversized_output = vec![b'x'; MAX_MEDIA_LIST_SOURCE_CHECK_OUTPUT_BYTES + 1];
        assert!(parse_media_list_source_check_output(&oversized_output).is_err());
    }

    #[test]
    fn preparation_requirement_is_finite_boolean_and_preparation_only() {
        let mut definition = boolean_option("process_media");
        definition.requires_preparation = true;
        for scope in [
            AddonMediaListOptionScope::ListDefault,
            AddonMediaListOptionScope::ItemOverride,
            AddonMediaListOptionScope::Both,
        ] {
            definition.scope = scope;
            definition
                .validate()
                .expect("valid preparation requirement");
        }
        let serialized = serde_json::to_string(&definition).unwrap();
        assert!(serialized.contains("\"requires_preparation\":true"));
        assert_eq!(
            serde_json::from_str::<AddonMediaListOptionDefinition>(&serialized).unwrap(),
            definition
        );

        let mut invalid = definition.clone();
        invalid.applicability = AddonMediaListOptionApplicability::All;
        assert!(invalid.validate().is_err());
        for operation in [
            AddonMediaListOptionOperation::Enrich,
            AddonMediaListOptionOperation::Artifact,
            AddonMediaListOptionOperation::Playback,
            AddonMediaListOptionOperation::TestAccess,
        ] {
            let mut invalid = definition.clone();
            invalid.operation_targets = vec![operation];
            assert!(invalid.validate().is_err());
        }
        let mut invalid = enum_option("process_media");
        invalid.applicability = AddonMediaListOptionApplicability::FiniteOnly;
        invalid.requires_preparation = true;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn legacy_option_json_retains_exact_serialization() {
        let legacy = concat!(
            "{\"key\":\"process_media\",\"label\":{\"en-US\":\"Process media\"},",
            "\"description\":{\"en-US\":\"Prepare finite media.\"},\"scope\":\"both\",",
            "\"operation_targets\":[\"preparation\",\"playback\"],",
            "\"applicability\":\"finite_only\",\"kind\":\"boolean\",\"default\":true}"
        );
        let definition: AddonMediaListOptionDefinition = serde_json::from_str(legacy).unwrap();
        definition.validate().expect("legacy declaration");
        assert!(!definition.requires_preparation);
        assert_eq!(serde_json::to_string(&definition).unwrap(), legacy);

        let mut future = serde_json::to_value(&definition).unwrap();
        future["future_requirement"] = serde_json::json!(true);
        assert!(serde_json::from_value::<AddonMediaListOptionDefinition>(future).is_err());
    }

    #[test]
    fn test_access_options_require_explicit_declaration_and_remain_bounded() {
        let mut definition = enum_option("browser_profile");
        definition
            .operation_targets
            .push(AddonMediaListOptionOperation::TestAccess);
        definition.validate().expect("all five operation targets");
        let mut capability = AddonCapability {
            id: CAPABILITY_MEDIA_LIST_OPTIONS_V1.to_owned(),
            options: vec![definition],
        };
        let values = AddonMediaListOptionValues {
            schema: CAPABILITY_MEDIA_LIST_OPTIONS_V1.to_owned(),
            values: BTreeMap::from([(
                "browser_profile".to_owned(),
                AddonMediaListOptionValue::Enum("firefox".to_owned()),
            )]),
        };
        capability
            .validate_option_values(&values, AddonMediaListOptionOperation::TestAccess, false)
            .expect("explicit access options");
        assert_eq!(
            serde_json::to_string(&AddonMediaListOptionOperation::TestAccess).unwrap(),
            "\"test_access\""
        );
        capability.options[0].operation_targets.pop();
        assert!(capability
            .validate_option_values(&values, AddonMediaListOptionOperation::TestAccess, false)
            .is_err());
        capability.options[0].operation_targets.extend([
            AddonMediaListOptionOperation::TestAccess,
            AddonMediaListOptionOperation::TestAccess,
        ]);
        assert!(capability.options[0].validate().is_err());
        assert!(
            serde_json::from_str::<AddonMediaListOptionOperation>("\"future_operation\"").is_err()
        );
    }

    #[test]
    fn media_list_option_definitions_fail_closed_without_breaking_inert_capabilities() {
        let mut inert = valid_manifest();
        inert.capabilities.push(AddonCapability {
            id: "media-list.options.v99".to_owned(),
            options: Vec::new(),
        });
        inert.validate().expect("unknown capability remains inert");

        inert.manifest_version = 5;
        inert.capabilities.last_mut().unwrap().options = vec![boolean_option("fixture")];
        assert!(inert.validate().is_err());

        let mut empty = manifest_with_option_schema();
        empty
            .capabilities
            .iter_mut()
            .find(|capability| capability.id == CAPABILITY_MEDIA_LIST_OPTIONS_V1)
            .unwrap()
            .options
            .clear();
        assert!(empty.validate().is_err());

        let mut old_schema = manifest_with_option_schema();
        old_schema.manifest_version = 4;
        assert!(old_schema.validate().is_err());
    }

    #[test]
    fn media_list_option_definitions_validate_types_bounds_and_localization() {
        let mut manifest = manifest_with_option_schema();
        let schema = manifest
            .capabilities
            .iter_mut()
            .find(|capability| capability.id == CAPABILITY_MEDIA_LIST_OPTIONS_V1)
            .unwrap();
        schema.options[0].default = AddonMediaListOptionValue::Enum("unknown".to_owned());
        assert!(manifest.validate().is_err());

        let mut manifest = manifest_with_option_schema();
        let schema = manifest
            .capabilities
            .iter_mut()
            .find(|capability| capability.id == CAPABILITY_MEDIA_LIST_OPTIONS_V1)
            .unwrap();
        schema.options[0]
            .operation_targets
            .push(AddonMediaListOptionOperation::Enrich);
        assert!(manifest.validate().is_err());

        let mut manifest = manifest_with_option_schema();
        let schema = manifest
            .capabilities
            .iter_mut()
            .find(|capability| capability.id == CAPABILITY_MEDIA_LIST_OPTIONS_V1)
            .unwrap();
        schema.options[1].key = schema.options[0].key.clone();
        assert!(manifest.validate().is_err());

        let mut manifest = manifest_with_option_schema();
        let schema = manifest
            .capabilities
            .iter_mut()
            .find(|capability| capability.id == CAPABILITY_MEDIA_LIST_OPTIONS_V1)
            .unwrap();
        schema.options[0].description.clear();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn stored_and_effective_media_list_option_values_fail_closed() {
        let manifest = manifest_with_option_schema();
        let schema = manifest.media_list_option_schema().unwrap();
        let valid = AddonMediaListOptionValues {
            schema: CAPABILITY_MEDIA_LIST_OPTIONS_V1.to_owned(),
            values: BTreeMap::from([
                (
                    "browser_profile".to_owned(),
                    AddonMediaListOptionValue::Enum("firefox".to_owned()),
                ),
                (
                    "remove_segments".to_owned(),
                    AddonMediaListOptionValue::Boolean(true),
                ),
            ]),
        };
        schema
            .validate_option_values(&valid, AddonMediaListOptionOperation::Playback, true)
            .expect("valid typed option values");

        let mut invalid = valid.clone();
        invalid.schema = "media-list.options.v99".to_owned();
        assert!(schema
            .validate_stored_option_values(&invalid, AddonMediaListOptionValueScope::ItemOverride,)
            .is_err());

        let mut invalid = valid.clone();
        invalid.values.insert(
            "undeclared".to_owned(),
            AddonMediaListOptionValue::Boolean(true),
        );
        assert!(schema
            .validate_stored_option_values(&invalid, AddonMediaListOptionValueScope::ItemOverride,)
            .is_err());

        let mut invalid = valid.clone();
        invalid.values.insert(
            "browser_profile".to_owned(),
            AddonMediaListOptionValue::Boolean(false),
        );
        assert!(schema
            .validate_stored_option_values(&invalid, AddonMediaListOptionValueScope::ItemOverride,)
            .is_err());

        let mut invalid = valid.clone();
        invalid.values.insert(
            "browser_profile".to_owned(),
            AddonMediaListOptionValue::Enum("unknown".to_owned()),
        );
        assert!(schema
            .validate_stored_option_values(&invalid, AddonMediaListOptionValueScope::ItemOverride,)
            .is_err());

        assert!(schema
            .validate_option_values(&valid, AddonMediaListOptionOperation::Playback, false,)
            .is_err());

        schema
            .validate_stored_option_values(&valid, AddonMediaListOptionValueScope::ListDefault)
            .expect("finite-only list defaults remain configurable");

        assert!(schema
            .validate_option_values(&valid, AddonMediaListOptionOperation::Playback, false,)
            .is_err());

        assert!(schema
            .validate_option_values(&valid, AddonMediaListOptionOperation::Enrich, true,)
            .is_err());

        let mut list_only_schema = schema.clone();
        list_only_schema.options[0].scope = AddonMediaListOptionScope::ListDefault;
        let browser_value = AddonMediaListOptionValues {
            schema: CAPABILITY_MEDIA_LIST_OPTIONS_V1.to_owned(),
            values: BTreeMap::from([(
                "browser_profile".to_owned(),
                AddonMediaListOptionValue::Enum("firefox".to_owned()),
            )]),
        };
        assert!(list_only_schema
            .validate_stored_option_values(
                &browser_value,
                AddonMediaListOptionValueScope::ItemOverride,
            )
            .is_err());

        let mut split_scope_schema = schema.clone();
        split_scope_schema.options[0].scope = AddonMediaListOptionScope::ListDefault;
        split_scope_schema.options[1].scope = AddonMediaListOptionScope::ItemOverride;
        split_scope_schema
            .validate_option_values(&valid, AddonMediaListOptionOperation::Playback, true)
            .expect("effective values may combine inherited and overridden scopes");

        let oversized = AddonMediaListOptionValues {
            schema: CAPABILITY_MEDIA_LIST_OPTIONS_V1.to_owned(),
            values: BTreeMap::from([(
                "x".repeat(MAX_MEDIA_LIST_OPTION_VALUES_BYTES),
                AddonMediaListOptionValue::Boolean(true),
            )]),
        };
        assert!(schema
            .validate_stored_option_values(&oversized, AddonMediaListOptionValueScope::ListDefault,)
            .is_err());
    }

    #[test]
    fn schema_v4_default_source_hosts_must_be_permitted_and_bounded() {
        let mut manifest = valid_manifest();
        manifest.default_source_hosts = vec!["schatkamer.example.test".to_owned()];
        manifest.permissions.network_hosts = vec!["schatkamer.example.test".to_owned()];

        // The field is frozen out of every earlier schema version.
        for version in 1..4 {
            manifest.manifest_version = version;
            assert!(manifest.validate().is_err(), "schema v{version}");
        }

        manifest.manifest_version = 4;
        manifest.validate().expect("permitted default host");

        // A claimed host the package may not even contact would be selected and
        // then refused by `validate_operator_source_url`.
        manifest.permissions.network_hosts = vec!["other.example.test".to_owned()];
        assert!(manifest.validate().is_err());

        // A permitted wildcard covers the subdomains it permits.
        manifest.permissions.network_hosts = vec!["*.example.test".to_owned()];
        manifest.validate().expect("wildcard covers subdomain");

        // No package may claim every host or a whole top-level domain.
        for claim in ["*", "*.test", ".test"] {
            manifest.default_source_hosts = vec![claim.to_owned()];
            manifest.permissions.network_hosts = vec!["*".to_owned()];
            assert!(manifest.validate().is_err(), "claim {claim}");
        }

        // An unrestricted permission declaration still permits a narrow claim.
        manifest.default_source_hosts = vec!["schatkamer.example.test".to_owned()];
        manifest.permissions.network_hosts = Vec::new();
        manifest.validate().expect("unrestricted permissions");

        manifest.default_source_hosts = vec![
            "schatkamer.example.test".to_owned(),
            "SCHATKAMER.example.test".to_owned(),
        ];
        assert!(manifest.validate().is_err(), "duplicate claim");
    }

    #[test]
    fn declared_source_hosts_rank_exact_matches_above_wildcards() {
        let exact = vec!["media.example.test".to_owned()];
        let narrow = vec!["*.media.example.test".to_owned()];
        let broad = vec!["*.example.test".to_owned()];

        assert_eq!(
            match_declared_source_host(
                "media.example.test",
                &[("broad", broad.as_slice()), ("exact", exact.as_slice())],
            ),
            SourceAddonMatch::Declared("exact".to_owned()),
        );
        assert_eq!(
            match_declared_source_host(
                "clips.media.example.test",
                &[("broad", broad.as_slice()), ("narrow", narrow.as_slice())],
            ),
            SourceAddonMatch::Declared("narrow".to_owned()),
        );
        assert_eq!(
            match_declared_source_host("unrelated.invalid", &[("broad", broad.as_slice())]),
            SourceAddonMatch::None,
        );
    }

    #[test]
    fn equally_specific_claims_are_ambiguous_rather_than_arbitrary() {
        let hosts = vec!["media.example.test".to_owned()];
        assert_eq!(
            match_declared_source_host(
                "media.example.test",
                &[("second", hosts.as_slice()), ("first", hosts.as_slice())],
            ),
            SourceAddonMatch::Ambiguous(vec!["first".to_owned(), "second".to_owned()]),
        );
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
                reference: "PROVIDER__CLIENT_ID".to_owned(),
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
            .map(|id| AddonCapability {
                id: id.to_owned(),
                options: Vec::new(),
            }),
        );
        manifest
            .validate()
            .expect("complete managed storage contract");

        manifest
            .capabilities
            .retain(|capability| capability.id != CAPABILITY_REMOTE_STREAM_PLAY_V1);
        manifest.capabilities.push(AddonCapability {
            id: CAPABILITY_REMOTE_STREAM_PLAY_V2.to_owned(),
            options: Vec::new(),
        });
        manifest
            .validate()
            .expect("v2 play capability also completes managed storage contract");

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

    #[test]
    fn local_repository_manifest_has_no_network_trust_fields() {
        let manifest = LocalRepositoryManifest {
            schema_version: LOCAL_ADDON_REPOSITORY_SCHEMA_VERSION,
            id: "org.example.addons".to_owned(),
            name: "Example add-ons".to_owned(),
            description: localized("Example local repository"),
        };
        manifest.validate().expect("valid local repository");

        let document = r#"
schema_version = 1
id = "org.example.addons"
name = "Example add-ons"

[description]
"en-US" = "Example local repository"
"#;
        validate_contract_document(ContractDocumentKind::RepositoryManifest, document)
            .expect("valid standalone local repository document");

        let remote_document = document.replace(
            "[description]",
            "index_url = \"https://example.test/index.json\"\n\n[description]",
        );
        assert!(validate_contract_document(
            ContractDocumentKind::RepositoryManifest,
            &remote_document
        )
        .is_err());
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
    fn media_list_v6_conformance_preserves_typed_restriction_reason() {
        let output = br#"{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"stream:1","rank":0,"display_title":"Restricted","title":"Restricted","kind":"remote_stream","source_url":"https://example.test/restricted","availability":"unavailable","availability_reason":"provider category","availability_reason_code":"content_restricted"}
"#;
        let records = conform_media_list_output(output).expect("valid v6 record shape");
        assert!(matches!(
            &records[1],
            AddonMediaListRecord::Item {
                availability_reason_code: Some(AddonMediaListAvailabilityReason::ContentRestricted),
                ..
            }
        ));
    }

    #[test]
    fn media_list_import_requests_lock_policy_schema_and_bounds() {
        let request = MediaListDiscoverRequest {
            source_url: "https://example.test/programmes".to_owned(),
            record_capability: CAPABILITY_MEDIA_LIST_LIST_V5.to_owned(),
            cursor: Some("page-2".to_owned()),
            limits: MediaListDiscoverLimits::default(),
        };
        request.validate().expect("bounded discover request");
        assert_eq!(
            serde_json::to_string(&MediaListDetailImportPolicy::ManualOnly).unwrap(),
            r#""manual_only""#
        );
        assert!(serde_json::from_str::<MediaListDetailImportPolicy>(r#""essential""#).is_err());

        let mut invalid_schema = request.clone();
        invalid_schema.record_capability = "media-list.list.v99".to_owned();
        assert!(invalid_schema.validate().is_err());

        let mut invalid_cursor = request;
        invalid_cursor.cursor = Some("x".repeat(MAX_MEDIA_LIST_IMPORT_CURSOR_BYTES + 1));
        assert!(invalid_cursor.validate().is_err());
    }

    #[test]
    fn media_list_import_v2_request_bounds_the_staged_archive() {
        let request = media_list_v2_request(MediaListImportMode::Incremental, Some("page-2"));
        request.validate().expect("bounded v2 discover request");

        let encoded = serde_json::to_string(&request).expect("request JSON");
        assert!(encoded.contains(r#""mode":"incremental""#));
        assert!(encoded.contains(r#""staged_path":"private/import/archive.txt""#));

        let mut invalid_digest = request.clone();
        invalid_digest.archive.sha256 = "A".repeat(64);
        assert!(invalid_digest.validate().is_err());

        let mut invalid_generation = request.clone();
        invalid_generation.archive.generation = 0;
        assert!(invalid_generation.validate().is_err());

        let mut invalid_entries = request;
        invalid_entries.archive.entry_count = invalid_entries.archive.max_entries + 1;
        assert!(invalid_entries.validate().is_err());
    }

    #[test]
    fn media_list_import_v2_full_and_incremental_headers_are_distinct() {
        let full = br#"{"record_type":"page","complete":true,"strategy":"skip_known","examined_count":1,"archived_skipped_count":0,"emitted_count":1}
{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"Episode 1","title":"Episode 1","kind":"episode"}
"#;
        let full_request = media_list_v2_request(MediaListImportMode::Full, None);
        let page = conform_media_list_discover_v2_output(full, &full_request)
            .expect("valid full generation");
        assert_eq!(page.emitted_count, 1);
        assert_eq!(page.records.len(), 2);

        let full_without_header = br#"{"record_type":"page","complete":true,"strategy":"skip_known","examined_count":1,"archived_skipped_count":0,"emitted_count":1}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"Episode 1","title":"Episode 1","kind":"episode"}
"#;
        assert!(conform_media_list_discover_v2_output(full_without_header, &full_request).is_err());

        let incremental_header_only = br#"{"record_type":"page","complete":true,"strategy":"skip_known","examined_count":0,"archived_skipped_count":0,"emitted_count":0}
{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
"#;
        let incremental_request = media_list_v2_request(MediaListImportMode::Incremental, None);
        let page =
            conform_media_list_discover_v2_output(incremental_header_only, &incremental_request)
                .expect("incremental refresh may update only its list header");
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.emitted_count, 0);

        let incremental_without_header = br#"{"record_type":"page","complete":true,"strategy":"skip_known","examined_count":0,"archived_skipped_count":0,"emitted_count":0}
"#;
        assert!(conform_media_list_discover_v2_output(
            incremental_without_header,
            &incremental_request,
        )
        .is_ok());
    }

    #[test]
    fn media_list_import_v2_validates_counts_and_all_known_progress() {
        let request = media_list_v2_request(MediaListImportMode::Incremental, None);
        let all_known = br#"{"record_type":"page","complete":false,"next_cursor":"page-2","strategy":"newest_first_early_stop","examined_count":2,"archived_skipped_count":2,"emitted_count":0}
"#;
        let page = conform_media_list_discover_v2_output(all_known, &request)
            .expect("all-known page makes examined and cursor progress");
        assert_eq!(page.archived_skipped_count, 2);
        assert!(page.records.is_empty());

        let no_progress = br#"{"record_type":"page","complete":false,"next_cursor":"page-2","strategy":"skip_known","examined_count":0,"archived_skipped_count":0,"emitted_count":0}
"#;
        assert!(conform_media_list_discover_v2_output(no_progress, &request).is_err());

        let wrong_counts = br#"{"record_type":"page","complete":true,"strategy":"skip_known","examined_count":2,"archived_skipped_count":0,"emitted_count":1}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"Episode 1","title":"Episode 1","kind":"episode"}
"#;
        assert!(conform_media_list_discover_v2_output(wrong_counts, &request).is_err());

        let oversized_examined_page = br#"{"record_type":"page","complete":false,"next_cursor":"page-251","strategy":"skip_known","examined_count":251,"archived_skipped_count":251,"emitted_count":0}
"#;
        assert!(conform_media_list_discover_v2_output(oversized_examined_page, &request).is_err());

        let leaked_path = br#"{"record_type":"page","complete":true,"strategy":"skip_known","examined_count":0,"archived_skipped_count":0,"emitted_count":0,"staged_path":"private/archive.txt"}
"#;
        assert!(conform_media_list_discover_v2_output(leaked_path, &request).is_err());

        let continuation_request =
            media_list_v2_request(MediaListImportMode::Incremental, Some("page-2"));
        let repeated_cursor = br#"{"record_type":"page","complete":false,"next_cursor":"page-2","strategy":"skip_known","examined_count":1,"archived_skipped_count":1,"emitted_count":0}
"#;
        assert!(
            conform_media_list_discover_v2_output(repeated_cursor, &continuation_request,).is_err()
        );

        let full_with_skip = br#"{"record_type":"page","complete":true,"strategy":"skip_known","examined_count":1,"archived_skipped_count":1,"emitted_count":0}
{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
"#;
        let full_request = media_list_v2_request(MediaListImportMode::Full, None);
        assert!(conform_media_list_discover_v2_output(full_with_skip, &full_request).is_err());
    }

    #[test]
    fn media_list_import_discovery_pages_preserve_overview_metadata() {
        let first = br#"{"record_type":"page","complete":false,"next_cursor":"page-2","total_hint":2,"revision_token":"rev-1"}
{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"Episode 1","title":"Episode 1","kind":"episode","metadata":{"outline":"Overview one","release_date":"2026-08-01","artwork":[{"url":"https://example.test/one.jpg","role":"thumb"}]}}
"#;
        let page = conform_media_list_discover_output(first, true).expect("valid first page");
        assert!(!page.complete);
        assert_eq!(page.next_cursor.as_deref(), Some("page-2"));
        assert_eq!(page.records.len(), 2);
        assert!(matches!(
            &page.records[1],
            AddonMediaListRecord::Item {
                metadata: Some(AddonMetadataDocument {
                    release_date: Some(date),
                    outline: Some(outline),
                    artwork,
                    ..
                }),
                ..
            } if date == "2026-08-01" && outline == "Overview one" && artwork.len() == 1
        ));

        let continuation = br#"{"record_type":"page","complete":true,"revision_token":"rev-1"}
{"record_type":"item","provider_id":"episode:2","rank":1,"display_title":"Episode 2","title":"Episode 2","kind":"episode"}
"#;
        assert!(conform_media_list_discover_output(continuation, false).is_ok());
    }

    #[test]
    fn media_list_import_discovery_rejects_no_progress_and_page_conflicts() {
        let no_progress = br#"{"record_type":"page","complete":false,"next_cursor":"same"}
{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
"#;
        assert!(conform_media_list_discover_output(no_progress, true).is_err());

        let duplicate = br#"{"record_type":"page","complete":true}
{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"One","title":"One","kind":"episode"}
{"record_type":"item","provider_id":"episode:1","rank":1,"display_title":"Two","title":"Two","kind":"episode"}
"#;
        assert!(conform_media_list_discover_output(duplicate, true).is_err());

        let contradictory = br#"{"record_type":"page","complete":true,"next_cursor":"page-2"}
{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}
"#;
        assert!(conform_media_list_discover_output(contradictory, true).is_err());
    }

    #[test]
    fn media_list_import_enforces_page_item_and_output_bounds() {
        use std::fmt::Write as _;

        let mut too_many = String::from(
            "{\"record_type\":\"page\",\"complete\":true}\n\
             {\"record_type\":\"list\",\"provider_id\":\"fixture/list\",\"name\":\"Fixture\"}\n",
        );
        for rank in 0..=MAX_MEDIA_LIST_IMPORT_PAGE_ITEMS {
            writeln!(
                too_many,
                "{{\"record_type\":\"item\",\"provider_id\":\"episode:{rank}\",\"rank\":{rank},\"display_title\":\"Episode {rank}\",\"title\":\"Episode {rank}\",\"kind\":\"episode\"}}"
            )
            .expect("writing to a String cannot fail");
        }
        assert!(conform_media_list_discover_output(too_many.as_bytes(), true).is_err());

        let oversized_page = vec![b'x'; MAX_MEDIA_LIST_IMPORT_PAGE_OUTPUT_BYTES + 1];
        assert!(conform_media_list_discover_output(&oversized_page, true).is_err());
        let oversized_item = vec![b'x'; MAX_MEDIA_LIST_IMPORT_ITEM_OUTPUT_BYTES + 1];
        assert!(conform_media_list_enrich_output(&oversized_item, "episode:1").is_err());

        let mut invalid_limits = MediaListDiscoverRequest {
            source_url: "https://example.test/programmes".to_owned(),
            record_capability: CAPABILITY_MEDIA_LIST_LIST_V5.to_owned(),
            cursor: None,
            limits: MediaListDiscoverLimits::default(),
        };
        invalid_limits.limits.max_items = 0;
        assert!(invalid_limits.validate().is_err());
        invalid_limits.limits = MediaListDiscoverLimits::default();
        invalid_limits.limits.max_output_bytes = 0;
        assert!(invalid_limits.validate().is_err());
    }

    #[test]
    fn media_list_import_enrichment_requires_typed_identity_preserving_results() {
        let complete = br#"{"record_type":"outcome","outcome":"complete"}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"One","title":"One","kind":"episode","metadata":{"plot":"Full detail"}}
"#;
        let result =
            conform_media_list_enrich_output(complete, "episode:1").expect("valid complete result");
        assert_eq!(result.outcome, MediaListEnrichOutcomeKind::Complete);
        assert!(result.item.is_some());

        let unavailable = br#"{"record_type":"outcome","outcome":"unavailable","code":"not-found","message":"Removed"}
{"record_type":"item","provider_id":"episode:1","rank":0,"display_title":"One","title":"One","kind":"episode","availability":"unavailable","availability_reason":"not-found"}
"#;
        assert!(conform_media_list_enrich_output(unavailable, "episode:1").is_ok());

        let transient = br#"{"record_type":"outcome","outcome":"transient_failure","code":"provider-rate-limited","message":"Try later","retry_after_seconds":120}
"#;
        assert!(conform_media_list_enrich_output(transient, "episode:1").is_ok());

        let wrong_identity = complete
            .windows(b"episode:1".len())
            .position(|window| window == b"episode:1")
            .map(|_| complete)
            .expect("fixture contains identity");
        assert!(conform_media_list_enrich_output(wrong_identity, "episode:2").is_err());

        let missing_item = br#"{"record_type":"outcome","outcome":"complete"}
"#;
        assert!(conform_media_list_enrich_output(missing_item, "episode:1").is_err());
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
    fn media_list_liveness_accepts_three_values_and_treats_null_as_unknown() {
        for (value, expected) in [
            ("unknown", RemoteStreamLiveness::Unknown),
            ("finite", RemoteStreamLiveness::Finite),
            ("live", RemoteStreamLiveness::Live),
        ] {
            let output = format!(
                "{{\"record_type\":\"list\",\"provider_id\":\"fixture/list\",\"name\":\"Fixture\"}}\n\
                 {{\"record_type\":\"item\",\"provider_id\":\"video:1\",\"rank\":0,\"display_title\":\"Video\",\"title\":\"Video\",\"kind\":\"remote_stream\",\"source_url\":\"https://example.test/video\",\"liveness\":\"{value}\"}}\n"
            );
            let records = conform_media_list_output(output.as_bytes()).expect("valid liveness");
            assert!(matches!(
                &records[1],
                AddonMediaListRecord::Item {
                    liveness: Some(actual),
                    ..
                } if *actual == expected
            ));
        }

        for suffix in ["", ",\"liveness\":null"] {
            let output = format!(
                "{{\"record_type\":\"list\",\"provider_id\":\"fixture/list\",\"name\":\"Fixture\"}}\n\
                 {{\"record_type\":\"item\",\"provider_id\":\"video:1\",\"rank\":0,\"display_title\":\"Video\",\"title\":\"Video\",\"kind\":\"remote_stream\",\"source_url\":\"https://example.test/video\"{suffix}}}\n"
            );
            let records = conform_media_list_output(output.as_bytes()).expect("absent liveness");
            assert!(matches!(
                &records[1],
                AddonMediaListRecord::Item { liveness: None, .. }
            ));
        }
    }

    #[test]
    fn remote_stream_liveness_adapts_legacy_values_and_rejects_conflicts() {
        let cases = [
            (
                r#"{"id":"one","url":"https://example.test/one","title":"One","is_live":true}"#,
                RemoteStreamLiveness::Live,
            ),
            (
                r#"{"id":"one","url":"https://example.test/one","title":"One","is_live":false}"#,
                RemoteStreamLiveness::Finite,
            ),
            (
                r#"{"id":"one","url":"https://example.test/one","title":"One","duration_seconds":60}"#,
                RemoteStreamLiveness::Finite,
            ),
            (
                r#"{"id":"one","url":"https://example.test/one","title":"One"}"#,
                RemoteStreamLiveness::Unknown,
            ),
            (
                r#"{"id":"one","url":"https://example.test/one","title":"One","liveness":"unknown"}"#,
                RemoteStreamLiveness::Unknown,
            ),
        ];
        for (line, expected) in cases {
            let entries = conform_remote_stream_list_output(line.as_bytes())
                .expect("compatible Remote Stream liveness");
            assert_eq!(entries[0].effective_liveness().unwrap(), expected);
        }

        let contradictory = br#"{"id":"one","url":"https://example.test/one","title":"One","is_live":false,"liveness":"live"}"#;
        assert!(conform_remote_stream_list_output(contradictory).is_err());
    }

    #[test]
    fn rich_metadata_document_round_trips_every_v4_field() {
        let metadata = AddonMetadataDocument {
            title: Some("Episode title".to_owned()),
            original_title: Some("Original title".to_owned()),
            sort_title: Some("Title, Episode".to_owned()),
            outline: Some("Outline".to_owned()),
            plot: Some("Plot".to_owned()),
            tagline: Some("Tagline".to_owned()),
            show_title: Some("Fixture show".to_owned()),
            season: Some(0),
            episode: Some(1),
            year: Some(2026),
            release_date: Some("2026-08-21".to_owned()),
            content_ratings: vec!["NL-AL".to_owned()],
            genres: vec!["Documentary".to_owned()],
            tags: vec!["archive".to_owned()],
            studios: vec!["Fixture studio".to_owned()],
            music_labels: vec!["Fixture label".to_owned()],
            languages: vec!["nl".to_owned()],
            people: vec![AddonMetadataPerson {
                name: "Fixture presenter".to_owned(),
                role: Some("Presenter".to_owned()),
                order: Some(0),
                thumb: Some("https://cdn.example.test/person.jpg".to_owned()),
            }],
            writers: vec!["Fixture writer".to_owned()],
            directors: vec!["Fixture director".to_owned()],
            artists: vec!["Fixture artist".to_owned()],
            producers: vec!["Fixture producer".to_owned()],
            original_broadcasters: vec!["Fixture broadcaster".to_owned()],
            broadcasters: vec!["Fixture network".to_owned()],
            collection: Some("Fixture collection".to_owned()),
            artwork: vec![AddonArtworkCandidate {
                url: "https://cdn.example.test/poster.jpg".to_owned(),
                role: AddonArtworkRole::Poster,
                width: Some(1000),
                height: Some(1500),
                language: Some("nl".to_owned()),
            }],
            guids: vec!["fixture://episode/1".to_owned()],
        };
        metadata.validate().expect("complete bounded document");
        let record = AddonMediaListRecord::List {
            provider_id: "fixture/list".to_owned(),
            name: "Fixture".to_owned(),
            description: None,
            metadata: Some(metadata.clone()),
        };
        let line = format!("{}\n", serde_json::to_string(&record).unwrap());
        let parsed = conform_media_list_output(line.as_bytes()).expect("v4 metadata output");
        assert_eq!(parsed, vec![record]);
    }

    #[test]
    fn rich_metadata_rejects_unsafe_artwork_and_invalid_kodi_identity() {
        let mut metadata = AddonMetadataDocument {
            artwork: vec![AddonArtworkCandidate {
                url: "http://cdn.example.test/poster.jpg".to_owned(),
                ..AddonArtworkCandidate::default()
            }],
            ..AddonMetadataDocument::default()
        };
        assert!(metadata.validate().is_err());

        metadata.artwork.clear();
        metadata.episode = Some(0);
        assert!(metadata.validate().is_err());

        metadata.episode = Some(1);
        metadata.release_date = Some("21-08-2026".to_owned());
        assert!(metadata.validate().is_err());
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
