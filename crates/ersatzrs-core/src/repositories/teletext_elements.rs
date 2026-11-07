//! Storage port for the teletext element library and channel links.

use async_trait::async_trait;
use ersatzrs_domain::{
    ChannelTeletextElement, EntityId, TeletextElement, TeletextElementSummary, TeletextElementUsage,
};

use super::Result;

/// Persistence for user-supplied teletext pages. Saving grants no publication
/// or broadcast authority; channel links are declarative only.
#[async_trait]
pub trait TeletextElementRepository: Send + Sync {
    /// All elements with usage counts, ordered by name.
    async fn list_summaries(&self) -> Result<Vec<TeletextElementSummary>>;
    async fn get(&self, id: EntityId) -> Result<Option<TeletextElement>>;
    /// Insert a new element; `id` and `revision` are ignored and assigned.
    /// # Errors
    /// Returns [`super::RepositoryError::Conflict`] for a duplicate name.
    async fn create(&self, element: &TeletextElement) -> Result<EntityId>;
    /// Replace an element when its stored revision equals `expected_revision`.
    /// # Errors
    /// Returns [`super::RepositoryError::Conflict`] on a stale revision or a
    /// duplicate name, and `NotFound` when the element no longer exists.
    async fn update(&self, element: &TeletextElement, expected_revision: i64) -> Result<i64>;
    /// Delete an element and its channel links. Returns whether it existed.
    async fn delete(&self, id: EntityId) -> Result<bool>;
    /// Channels that link this element, ordered by channel number.
    async fn usage(&self, element_id: EntityId) -> Result<Vec<TeletextElementUsage>>;
    /// A channel's element links, ordered by effective page number.
    async fn list_channel_elements(
        &self,
        channel_id: EntityId,
    ) -> Result<Vec<ChannelTeletextElement>>;
    /// Create or update the single link between a channel and an element.
    async fn save_channel_element(
        &self,
        channel_id: EntityId,
        element_id: EntityId,
        page_number: Option<u16>,
        enabled: bool,
    ) -> Result<EntityId>;
    /// Remove one link of this channel. Returns whether it existed.
    async fn remove_channel_element(&self, channel_id: EntityId, link_id: EntityId)
        -> Result<bool>;
}
