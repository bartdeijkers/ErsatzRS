use async_trait::async_trait;
use ersatzrs_domain::{
    Block, BlockItem, BlockStopScheduling, CollectionType, EntityId, PlaybackOrder,
};

use crate::scheduling::SchedulingSmartShufflePolicy;

use super::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockGroupAdminRow {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockAdminRow {
    pub id: EntityId,
    pub group_id: EntityId,
    pub group_name: String,
    pub name: String,
    pub minutes: i32,
    pub stop_scheduling: BlockStopScheduling,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockEditorOptionAdmin {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockEditorOptionsAdmin {
    pub collections: Vec<BlockEditorOptionAdmin>,
    pub shows: Vec<BlockEditorOptionAdmin>,
    pub seasons: Vec<BlockEditorOptionAdmin>,
    pub multi_collections: Vec<BlockEditorOptionAdmin>,
    pub smart_collections: Vec<BlockEditorOptionAdmin>,
    pub watermarks: Vec<BlockEditorOptionAdmin>,
    pub graphics_elements: Vec<BlockEditorOptionAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockItemAdmin {
    pub id: EntityId,
    pub index: i32,
    pub collection_type: CollectionType,
    pub collection_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub search_title: Option<String>,
    pub search_query: Option<String>,
    pub playback_order: PlaybackOrder,
    pub smart_shuffle: Option<SchedulingSmartShufflePolicy>,
    pub include_in_program_guide: bool,
    pub disable_watermarks: bool,
    pub collection_name: String,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
}

#[async_trait]
pub trait BlockRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<Block>;
    async fn list_for_group(&self, group_id: EntityId) -> Result<Vec<Block>>;
    async fn upsert(&self, block: &Block) -> Result<()>;
    async fn get_item(&self, id: EntityId) -> Result<BlockItem>;
    async fn list_items(&self, block_id: EntityId) -> Result<Vec<BlockItem>>;
    async fn upsert_item(&self, item: &BlockItem) -> Result<()>;
    async fn delete_item(&self, id: EntityId) -> Result<()>;
    async fn list_groups_admin(&self) -> Result<Vec<BlockGroupAdminRow>>;
    async fn list_admin(&self, search: &str) -> Result<Vec<BlockAdminRow>>;
    async fn group_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn block_exists_admin(&self, id: EntityId) -> Result<bool>;
    async fn group_name_exists_admin(&self, name: &str) -> Result<bool>;
    async fn block_name_exists_admin(
        &self,
        group_id: EntityId,
        block_id: Option<EntityId>,
        name: &str,
    ) -> Result<bool>;
    async fn create_group_admin(&self, name: &str) -> Result<EntityId>;
    async fn delete_group_admin(&self, id: EntityId) -> Result<()>;
    async fn create_admin(&self, group_id: EntityId, name: &str) -> Result<EntityId>;
    async fn copy_admin(
        &self,
        block_id: EntityId,
        new_group_id: EntityId,
        name: &str,
    ) -> Result<EntityId>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
    async fn get_admin(&self, id: EntityId) -> Result<Option<BlockAdminRow>>;
    async fn list_items_admin(&self, block_id: EntityId) -> Result<Vec<BlockItemAdmin>>;
    async fn save_admin(&self, block: &BlockAdminRow, items: &[BlockItemAdmin]) -> Result<()>;
    async fn search_collections_admin(&self, prefix: &str) -> Result<Vec<BlockEditorOptionAdmin>>;
    async fn search_smart_collections_admin(
        &self,
        prefix: &str,
    ) -> Result<Vec<BlockEditorOptionAdmin>>;
    async fn search_multi_collections_admin(
        &self,
        prefix: &str,
    ) -> Result<Vec<BlockEditorOptionAdmin>>;
    async fn search_shows_admin(&self, prefix: &str) -> Result<Vec<BlockEditorOptionAdmin>>;
    async fn search_seasons_admin(&self, prefix: &str) -> Result<Vec<BlockEditorOptionAdmin>>;
    async fn editor_options_admin(&self) -> Result<BlockEditorOptionsAdmin>;
}
