//! Provider-neutral add-on media-list contracts.

use async_trait::async_trait;
use ersatzrs_domain::AddonMediaListItemKind;
use serde::{Deserialize, Serialize};

use crate::repositories::Result;

/// One bounded `media-list.list.v1` NDJSON record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddonMediaListRecord {
    /// Header describing the provider-selected list.
    List {
        /// Stable provider identity within the owning add-on.
        provider_id: String,
        /// Human-readable provider list name.
        name: String,
        /// Optional provider description.
        #[serde(default)]
        description: Option<String>,
    },
    /// One ordered item that may be matched to local metadata.
    Item {
        /// Stable item identity within this list.
        provider_id: String,
        /// Zero-based provider order.
        rank: i32,
        /// Human-readable label shown by administration surfaces.
        display_title: String,
        /// Movie or show title used for local matching.
        title: String,
        /// Optional release year used for local matching.
        #[serde(default)]
        year: Option<i32>,
        /// Optional season number used for season or episode matching.
        #[serde(default)]
        season: Option<i32>,
        /// Optional episode number used for episode matching.
        #[serde(default)]
        episode: Option<i32>,
        /// Local media kind represented by this record.
        kind: AddonMediaListItemKind,
        /// Provider GUID candidates in ErsatzRS metadata format.
        #[serde(default)]
        guids: Vec<String>,
        /// Optional stable source URL used to match generated Remote Streams.
        #[serde(default)]
        source_url: Option<String>,
    },
}

/// One complete, validated provider enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumeratedAddonMediaList {
    /// Stable provider identity within the owning add-on.
    pub provider_id: String,
    /// Human-readable provider list name.
    pub name: String,
    /// Optional provider description.
    pub description: Option<String>,
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
    async fn enumerate(&self, addon_id: &str, source_url: &str)
        -> Result<EnumeratedAddonMediaList>;
}
