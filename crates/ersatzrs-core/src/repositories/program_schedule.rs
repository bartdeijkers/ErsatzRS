use async_trait::async_trait;
use ersatzrs_domain::{EntityId, ProgramSchedule, ProgramScheduleAlternate, ProgramScheduleItem};

use super::Result;

#[async_trait]
pub trait ProgramScheduleRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<ProgramSchedule>;
    async fn list_for_playout(&self, playout_id: EntityId) -> Result<Vec<ProgramSchedule>>;
    async fn upsert(&self, schedule: &ProgramSchedule) -> Result<()>;
    async fn get_item(&self, id: EntityId) -> Result<ProgramScheduleItem>;
    async fn list_items(&self, schedule_id: EntityId) -> Result<Vec<ProgramScheduleItem>>;
    async fn upsert_item(&self, item: &ProgramScheduleItem) -> Result<()>;
    async fn delete_item(&self, id: EntityId) -> Result<()>;
    async fn list_alternates(&self, schedule_id: EntityId)
        -> Result<Vec<ProgramScheduleAlternate>>;
    async fn upsert_alternate(&self, alternate: &ProgramScheduleAlternate) -> Result<()>;
    async fn delete_alternate(&self, id: EntityId) -> Result<()>;
}
