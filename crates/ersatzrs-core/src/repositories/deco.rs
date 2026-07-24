use async_trait::async_trait;
use ersatzrs_domain::{CollectionType, Deco, DecoBreakPlacement, DecoMode, EntityId};

use super::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoGroupAdminRow {
    pub id: EntityId,
    pub name: String,
    pub deco_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoAdminRow {
    pub id: EntityId,
    pub group_id: EntityId,
    pub group_name: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoOptionAdmin {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoPlaylistOptionAdmin {
    pub id: EntityId,
    pub group_id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DecoEditorOptionsAdmin {
    pub watermarks: Vec<DecoOptionAdmin>,
    pub graphics_elements: Vec<DecoOptionAdmin>,
    pub collections: Vec<DecoOptionAdmin>,
    pub shows: Vec<DecoOptionAdmin>,
    pub seasons: Vec<DecoOptionAdmin>,
    pub artists: Vec<DecoOptionAdmin>,
    pub multi_collections: Vec<DecoOptionAdmin>,
    pub smart_collections: Vec<DecoOptionAdmin>,
    pub playlist_groups: Vec<DecoOptionAdmin>,
    pub playlists: Vec<DecoPlaylistOptionAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoBreakContentAdmin {
    pub id: EntityId,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub playlist_id: Option<EntityId>,
    pub collection_name: String,
    pub placement: DecoBreakPlacement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoAdminRecord {
    pub id: EntityId,
    pub group_id: EntityId,
    pub group_name: String,
    pub name: String,
    pub watermark_mode: DecoMode,
    pub watermark_ids: Vec<EntityId>,
    pub use_watermark_during_filler: bool,
    pub graphics_elements_mode: DecoMode,
    pub graphics_element_ids: Vec<EntityId>,
    pub use_graphics_elements_during_filler: bool,
    pub break_content_mode: DecoMode,
    pub break_content: Vec<DecoBreakContentAdmin>,
    pub default_filler_mode: DecoMode,
    pub default_filler_collection_type: CollectionType,
    pub default_filler_collection_id: Option<EntityId>,
    pub default_filler_media_item_id: Option<EntityId>,
    pub default_filler_multi_collection_id: Option<EntityId>,
    pub default_filler_smart_collection_id: Option<EntityId>,
    pub default_filler_trim_to_fit: bool,
    pub dead_air_fallback_mode: DecoMode,
    pub dead_air_fallback_collection_type: CollectionType,
    pub dead_air_fallback_collection_id: Option<EntityId>,
    pub dead_air_fallback_media_item_id: Option<EntityId>,
    pub dead_air_fallback_multi_collection_id: Option<EntityId>,
    pub dead_air_fallback_smart_collection_id: Option<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoTemplateGroupAdminRow {
    pub id: EntityId,
    pub name: String,
    pub template_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoTemplateAdminRow {
    pub id: EntityId,
    pub group_id: EntityId,
    pub group_name: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoTemplateItemAdmin {
    pub deco_id: EntityId,
    pub deco_name: String,
    pub start_minutes: i32,
    pub end_minutes: i32,
}

#[async_trait]
pub trait DecoRepository: Send + Sync {
    async fn get_with_watermarks(&self, id: EntityId) -> Result<Deco>;
    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Deco>>;
    async fn upsert(&self, deco: &Deco) -> Result<()>;
    async fn list_groups_admin(&self) -> Result<Vec<DecoGroupAdminRow>>;
    async fn list_admin(&self, search: &str) -> Result<Vec<DecoAdminRow>>;
    async fn group_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn deco_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn group_name_exists_admin(&self, name: &str) -> Result<bool>;
    async fn deco_name_exists_admin(
        &self,
        group_id: EntityId,
        deco_id: Option<EntityId>,
        name: &str,
    ) -> Result<bool>;
    async fn create_group_admin(&self, name: &str) -> Result<EntityId>;
    async fn create_admin(&self, group_id: EntityId, name: &str) -> Result<EntityId>;
    async fn delete_group_admin(&self, id: EntityId) -> Result<()>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
    async fn get_admin(&self, id: EntityId) -> Result<Option<DecoAdminRecord>>;
    async fn editor_options_admin(&self) -> Result<DecoEditorOptionsAdmin>;
    async fn save_admin(&self, deco: &DecoAdminRecord) -> Result<()>;
    async fn list_template_groups_admin(&self) -> Result<Vec<DecoTemplateGroupAdminRow>>;
    async fn list_templates_admin(&self, search: &str) -> Result<Vec<DecoTemplateAdminRow>>;
    async fn template_group_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn template_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn template_group_name_exists_admin(&self, name: &str) -> Result<bool>;
    async fn template_name_exists_admin(
        &self,
        group_id: EntityId,
        template_id: Option<EntityId>,
        name: &str,
    ) -> Result<bool>;
    async fn create_template_group_admin(&self, name: &str) -> Result<EntityId>;
    async fn create_template_admin(&self, group_id: EntityId, name: &str) -> Result<EntityId>;
    async fn delete_template_group_admin(&self, id: EntityId) -> Result<()>;
    async fn delete_template_admin(&self, id: EntityId) -> Result<()>;
    async fn get_template_admin(&self, id: EntityId) -> Result<Option<DecoTemplateAdminRow>>;
    async fn list_template_items_admin(
        &self,
        template_id: EntityId,
    ) -> Result<Vec<DecoTemplateItemAdmin>>;
    async fn save_template_admin(
        &self,
        id: EntityId,
        name: &str,
        items: &[DecoTemplateItemAdmin],
    ) -> Result<()>;
}
