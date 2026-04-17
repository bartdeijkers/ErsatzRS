use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_domain::{
    EntityId, Playout, PlayoutBuildStatus, PlayoutGap, PlayoutHistory, PlayoutItem,
    PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex,
};

use super::Result;
use crate::scheduling::SchedulingResult;

#[async_trait]
pub trait PlayoutRepository: Send + Sync {
    async fn get_with_anchor(&self, id: EntityId) -> Result<Playout>;
    async fn items_in_range(
        &self,
        playout_id: EntityId,
        start: DateTime<Utc>,
        finish: DateTime<Utc>,
    ) -> Result<Vec<PlayoutItem>>;
    async fn replace_runtime_state(
        &self,
        playout: &Playout,
        gaps: &[PlayoutGap],
        history: &[PlayoutHistory],
        program_schedule_anchors: &[PlayoutProgramScheduleAnchor],
        fill_group_indices: &[PlayoutScheduleItemFillGroupIndex],
    ) -> Result<()>;
    /// Persist the outcome of a single playout build in one transaction.
    ///
    /// Respects `result.clear_items` (Reset semantics: delete all existing
    /// items, gaps, and history for the playout before inserting the new
    /// rows) vs. Continue semantics (delete only the ids listed in
    /// `result.items_to_remove` / `result.history_to_remove`, then append).
    ///
    /// # Errors
    ///
    /// Returns an error when any database operation inside the transaction
    /// fails.
    async fn apply_build_result(&self, result: &SchedulingResult, playout: &Playout)
        -> Result<()>;
    async fn upsert_item(&self, item: &PlayoutItem) -> Result<()>;
    async fn delete_items_before(&self, playout_id: EntityId, cutoff: DateTime<Utc>) -> Result<()>;
    async fn upsert_build_status(&self, status: &PlayoutBuildStatus) -> Result<()>;
}
