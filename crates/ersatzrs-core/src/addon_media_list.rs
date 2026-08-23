//! Provider-neutral add-on media-list contracts.

use async_trait::async_trait;
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListItemKind,
    AddonMediaListItemRole,
};

use crate::repositories::{AddonMediaListNfoMetadata, Result};

/// One complete, validated provider enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumeratedAddonMediaList {
    /// Stable provider identity within the owning add-on.
    pub provider_id: String,
    /// Human-readable provider list name.
    pub name: String,
    /// Optional provider description.
    pub description: Option<String>,
    /// Rich list-level metadata supplied through the shared add-on contract.
    pub metadata: AddonMediaListNfoMetadata,
    /// Ordered provider items.
    pub items: Vec<EnumeratedAddonMediaListItem>,
}

/// One validated item returned by a provider add-on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumeratedAddonMediaListItem {
    /// Stable item identity within the list.
    pub provider_id: String,
    /// Zero-based provider order.
    pub rank: i32,
    /// Human-readable provider label.
    pub display_title: String,
    /// Movie or show title used for local matching.
    pub title: String,
    /// Optional release year.
    pub year: Option<i32>,
    /// Optional season number.
    pub season: Option<i32>,
    /// Optional episode number.
    pub episode: Option<i32>,
    /// Local media kind represented by this item.
    pub kind: AddonMediaListItemKind,
    /// Provider GUID candidates in ErsatzRS metadata format.
    pub guids: Vec<String>,
    /// Optional stable source URL used to match generated Remote Streams.
    pub source_url: Option<String>,
    /// Provider-reported availability.
    pub availability: AddonMediaListItemAvailability,
    /// Optional stable provider reason code.
    pub availability_reason: Option<String>,
    /// Provider-inferred descriptive content type.
    pub content_kind: AddonMediaListContentKind,
    /// Provider-supplied collection role; independent from list rank.
    pub collection_role: AddonMediaListItemRole,
    /// Provider-reported full item duration in whole seconds.
    pub duration_seconds: Option<u64>,
    /// Provider-neutral Remote Stream playback behavior.
    pub liveness: ersatzrs_domain::RemoteStreamLiveness,
    /// Stable provider identity of the full item for a fragment row.
    pub parent_provider_id: Option<String>,
    /// Inclusive fragment start in whole seconds.
    pub fragment_start_seconds: Option<u64>,
    /// Exclusive fragment end, or open-ended when the full duration is unknown.
    pub fragment_end_seconds: Option<u64>,
    /// Additional provider artwork beyond the primary thumbnail.
    pub additional_image_urls: Vec<String>,
    /// Rich item metadata supplied through the shared add-on contract.
    pub metadata: AddonMediaListNfoMetadata,
}

/// Host adapter that invokes one installed list-provider add-on.
#[async_trait]
pub trait AddonMediaListProvider: Send + Sync {
    /// Enumerate the provider list selected by `source_url` through `addon_id`.
    ///
    /// # Errors
    ///
    /// Returns an error when the add-on is unavailable or its output violates
    /// the bounded media-list contract.
    async fn enumerate(
        &self,
        addon_id: &str,
        source_url: &str,
        chapter_input: &str,
    ) -> Result<EnumeratedAddonMediaList>;
}
