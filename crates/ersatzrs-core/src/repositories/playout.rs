use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_domain::{
    EntityId, Playout, PlayoutBuildStatus, PlayoutGap, PlayoutHistory, PlayoutItem,
    PlayoutProgramScheduleAnchor, PlayoutScheduleItemFillGroupIndex,
};

use super::Result;
use crate::scheduling::SchedulingResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutListAdminRow {
    pub id: EntityId,
    pub channel_number: String,
    pub channel_name: String,
    pub schedule_kind: i32,
    pub schedule_name: String,
    pub schedule_file: String,
    pub daily_rebuild_minutes: Option<i32>,
    pub build_success: Option<bool>,
    pub build_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutDetailAdminItem {
    pub title: String,
    pub start: String,
    pub finish: String,
    pub scheduling_context: Option<String>,
    pub filler_kind: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutCreateChannelAdmin {
    pub id: EntityId,
    pub number: String,
    pub name: String,
    pub playout_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutCreateScheduleAdmin {
    pub id: EntityId,
    pub name: String,
    pub item_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutCreateAdmin {
    pub channel_id: EntityId,
    pub schedule_kind: i32,
    pub program_schedule_id: Option<EntityId>,
    pub schedule_file: String,
    pub seed: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutDecoGroupAdmin {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutDecoAdmin {
    pub id: EntityId,
    pub group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutKindEditorAdmin {
    pub id: EntityId,
    pub schedule_kind: i32,
    pub channel_number: String,
    pub channel_name: String,
    pub channel_playout_mode: i32,
    pub schedule_file: String,
    pub default_deco_id: Option<EntityId>,
    pub deco_groups: Vec<PlayoutDecoGroupAdmin>,
    pub decos: Vec<PlayoutDecoAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutNestedOptionAdmin {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutNestedChildOptionAdmin {
    pub id: EntityId,
    pub group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutDateFilterAdmin {
    pub limit_to_date_range: bool,
    pub start_month: i32,
    pub start_day: i32,
    pub start_year: Option<i32>,
    pub end_month: i32,
    pub end_day: i32,
    pub end_year: Option<i32>,
    pub days_of_week: Vec<i32>,
    pub days_of_month: Vec<i32>,
    pub months_of_year: Vec<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutAlternateScheduleItemAdmin {
    pub id: EntityId,
    pub index: i32,
    pub program_schedule_id: EntityId,
    pub program_schedule_name: String,
    pub date_filter: PlayoutDateFilterAdmin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutAlternateSchedulesAdmin {
    pub playout_id: EntityId,
    pub channel_number: String,
    pub channel_name: String,
    pub schedules: Vec<PlayoutNestedOptionAdmin>,
    pub items: Vec<PlayoutAlternateScheduleItemAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutTemplateItemAdmin {
    pub id: EntityId,
    pub index: i32,
    pub template_group_id: EntityId,
    pub template_id: EntityId,
    pub template_name: String,
    pub deco_template_group_id: Option<EntityId>,
    pub deco_template_id: Option<EntityId>,
    pub deco_template_name: Option<String>,
    pub date_filter: PlayoutDateFilterAdmin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayoutTemplatesAdmin {
    pub playout_id: EntityId,
    pub channel_number: String,
    pub channel_name: String,
    pub template_groups: Vec<PlayoutNestedOptionAdmin>,
    pub templates: Vec<PlayoutNestedChildOptionAdmin>,
    pub deco_template_groups: Vec<PlayoutNestedOptionAdmin>,
    pub deco_templates: Vec<PlayoutNestedChildOptionAdmin>,
    pub items: Vec<PlayoutTemplateItemAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequentialPlayoutAdmin {
    pub id: EntityId,
    pub channel_number: String,
    pub channel_name: String,
    pub yaml_content: String,
}

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
    async fn apply_build_result(&self, result: &SchedulingResult, playout: &Playout) -> Result<()>;
    async fn upsert_item(&self, item: &PlayoutItem) -> Result<()>;
    async fn delete_items_before(&self, playout_id: EntityId, cutoff: DateTime<Utc>) -> Result<()>;
    async fn upsert_build_status(&self, status: &PlayoutBuildStatus) -> Result<()>;
    async fn list_admin(
        &self,
        query: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PlayoutListAdminRow>>;
    async fn count_admin(&self) -> Result<u64>;
    async fn count_failed_builds_admin(&self) -> Result<u32>;
    async fn list_detail_admin(
        &self,
        playout_id: EntityId,
        show_filler: bool,
        now: DateTime<Utc>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PlayoutDetailAdminItem>>;
    async fn count_detail_admin(
        &self,
        playout_id: EntityId,
        show_filler: bool,
        now: DateTime<Utc>,
    ) -> Result<u64>;
    async fn list_create_channels_admin(&self) -> Result<Vec<PlayoutCreateChannelAdmin>>;
    async fn list_create_schedules_admin(&self) -> Result<Vec<PlayoutCreateScheduleAdmin>>;
    async fn create_admin(&self, playout: &PlayoutCreateAdmin) -> Result<EntityId>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
    async fn list_id_kinds_admin(&self) -> Result<Vec<(EntityId, i32)>>;
    async fn save_daily_reset_admin(
        &self,
        id: EntityId,
        daily_rebuild_minutes: Option<i32>,
    ) -> Result<()>;
    async fn save_schedule_file_admin(
        &self,
        id: EntityId,
        schedule_kind: i32,
        schedule_file: &str,
    ) -> Result<()>;
    async fn get_kind_editor_admin(
        &self,
        id: EntityId,
        schedule_kind: i32,
    ) -> Result<Option<PlayoutKindEditorAdmin>>;
    async fn deco_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn save_default_deco_admin(&self, id: EntityId, deco_id: Option<EntityId>) -> Result<()>;
    async fn get_alternate_schedules_admin(
        &self,
        id: EntityId,
    ) -> Result<Option<PlayoutAlternateSchedulesAdmin>>;
    async fn save_alternate_schedules_admin(
        &self,
        id: EntityId,
        items: &[PlayoutAlternateScheduleItemAdmin],
    ) -> Result<()>;
    async fn has_items_admin(&self, id: EntityId) -> Result<bool>;
    async fn get_templates_admin(&self, id: EntityId) -> Result<Option<PlayoutTemplatesAdmin>>;
    async fn save_templates_admin(
        &self,
        id: EntityId,
        items: &[PlayoutTemplateItemAdmin],
    ) -> Result<()>;
    async fn erase_items_admin(&self, id: EntityId) -> Result<()>;
    async fn get_sequential_admin(&self, id: EntityId) -> Result<Option<SequentialPlayoutAdmin>>;
    async fn erase_items_and_history_admin(&self, id: EntityId) -> Result<()>;
    async fn enrich_scheduling_context_admin(&self, serialized: &str) -> Result<String>;
}
