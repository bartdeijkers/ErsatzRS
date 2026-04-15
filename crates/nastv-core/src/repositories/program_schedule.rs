use async_trait::async_trait;
use nastv_domain::{EntityId, ProgramSchedule};

use super::Result;

#[async_trait]
pub trait ProgramScheduleRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<ProgramSchedule>;
    async fn list_for_playout(&self, playout_id: EntityId) -> Result<Vec<ProgramSchedule>>;
    async fn upsert(&self, schedule: &ProgramSchedule) -> Result<()>;
}
