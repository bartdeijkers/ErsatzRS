use crate::scheduling::SchedulingSmartShufflePolicy;
use async_trait::async_trait;
use ersatzrs_domain::{
    EntityId, PinnedStreamSelection, ProgramSchedule, ProgramScheduleAlternate, ProgramScheduleItem,
};

use super::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleAdminRow {
    pub id: EntityId,
    pub name: String,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScheduleAdminForm {
    pub id: EntityId,
    pub name: String,
    pub keep_multi_part_episodes_together: bool,
    pub treat_collections_as_shows: bool,
    pub shuffle_schedule_items: bool,
    pub random_start_point: bool,
    pub fixed_start_time_behavior: i32,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleItemAdmin {
    pub id: EntityId,
    pub index: i32,
    pub start_type: i32,
    pub start_time: Option<String>,
    pub fixed_start_time_behavior: Option<i32>,
    pub collection_type: i32,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub rerun_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub search_title: Option<String>,
    pub search_query: Option<String>,
    pub collection_name: String,
    pub playback_order: i32,
    pub smart_shuffle: SchedulingSmartShufflePolicy,
    pub inherited_legacy_smart_shuffle: bool,
    pub marathon_group_by: i32,
    pub marathon_shuffle_groups: bool,
    pub marathon_shuffle_items: bool,
    pub marathon_batch_size: Option<i32>,
    pub fill_with_group_mode: i32,
    pub playout_mode: i32,
    pub multiple_mode: i32,
    pub multiple_count: String,
    pub duration_hours: i32,
    pub duration_minutes: i32,
    pub tail_mode: i32,
    pub discard_to_fill_attempts: Option<i32>,
    pub custom_title: String,
    pub guide_mode: i32,
    pub pre_roll_filler_id: Option<EntityId>,
    pub mid_roll_filler_id: Option<EntityId>,
    pub post_roll_filler_id: Option<EntityId>,
    pub tail_filler_id: Option<EntityId>,
    pub fallback_filler_id: Option<EntityId>,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
    pub graphics_element_variables: std::collections::BTreeMap<EntityId, String>,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: String,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<i32>,
    pub pinned_stream_selection: Option<PinnedStreamSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleItemOption {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulePlaylistOption {
    pub id: EntityId,
    pub name: String,
    pub group_id: EntityId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleFillerOption {
    pub id: EntityId,
    pub name: String,
    pub filler_kind: i32,
    pub pad_to_nearest_minute: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleLanguageOption {
    pub code: String,
    pub name: String,
}

/// One schedule edit waiting for, or consumed by, a later Classic build.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScheduleDeferredChange {
    pub playout_id: EntityId,
    pub channel_name: String,
    pub saved_at: chrono::DateTime<chrono::Utc>,
    pub effective_after: Option<chrono::DateTime<chrono::Utc>>,
    pub applied_from: Option<chrono::DateTime<chrono::Utc>>,
    pub last_build: Option<chrono::DateTime<chrono::Utc>>,
    pub build_succeeded: Option<bool>,
    pub build_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleItemOptions {
    pub collections: Vec<ScheduleItemOption>,
    pub shows: Vec<ScheduleItemOption>,
    pub seasons: Vec<ScheduleItemOption>,
    pub artists: Vec<ScheduleItemOption>,
    pub multi_collections: Vec<ScheduleItemOption>,
    pub smart_collections: Vec<ScheduleItemOption>,
    pub rerun_collections: Vec<ScheduleItemOption>,
    pub playlist_groups: Vec<ScheduleItemOption>,
    pub playlists: Vec<SchedulePlaylistOption>,
    pub fillers: Vec<ScheduleFillerOption>,
    pub watermarks: Vec<ScheduleItemOption>,
    pub graphics_elements: Vec<ScheduleItemOption>,
    pub languages: Vec<ScheduleLanguageOption>,
}

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
    async fn list_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<ScheduleAdminRow>, u64)>;
    async fn get_admin(&self, id: EntityId) -> Result<Option<ScheduleAdminForm>>;
    async fn admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn save_admin(&self, form: &ScheduleAdminForm) -> Result<EntityId>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
    async fn copy_admin(&self, id: EntityId, name: &str) -> Result<EntityId>;
    async fn playout_ids_using_admin(&self, id: EntityId) -> Result<Vec<EntityId>>;
    async fn list_items_admin(&self, id: EntityId) -> Result<Vec<ScheduleItemAdmin>>;
    async fn get_item_options_admin(&self) -> Result<ScheduleItemOptions>;
    async fn replace_items_admin(
        &self,
        schedule_id: EntityId,
        items: &[ScheduleItemAdmin],
    ) -> Result<()>;
    /// Save compatible edits without rebuilding or replacing item identities.
    /// The caller holds each listed playout's build lock; storage rechecks the
    /// complete affected set and publishes settings/receipts atomically.
    async fn defer_items_admin(
        &self,
        schedule_id: EntityId,
        items: &[ScheduleItemAdmin],
        locked_playouts: &[EntityId],
    ) -> Result<Vec<ScheduleDeferredChange>>;
    async fn deferred_changes_admin(
        &self,
        schedule_id: EntityId,
    ) -> Result<Vec<ScheduleDeferredChange>>;
    async fn append_one_item_admin(
        &self,
        schedule_id: EntityId,
        collection_type: i32,
        media_item_id: EntityId,
        pinned_stream_selection: Option<PinnedStreamSelection>,
    ) -> Result<()>;
}
