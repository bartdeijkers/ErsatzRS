use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_domain::{EntityId, Playout, PlayoutBuildStatus, PlayoutItem};

use super::Result;

#[async_trait]
pub trait PlayoutRepository: Send + Sync {
    async fn get_with_anchor(&self, id: EntityId) -> Result<Playout>;
    async fn items_in_range(
        &self,
        playout_id: EntityId,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Vec<PlayoutItem>>;
    async fn upsert_item(&self, item: &PlayoutItem) -> Result<()>;
    async fn delete_items_before(&self, playout_id: EntityId, cutoff: DateTime<Utc>) -> Result<()>;
    async fn upsert_build_status(&self, status: &PlayoutBuildStatus) -> Result<()>;
}
