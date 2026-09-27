use async_trait::async_trait;
use ersatzrs_domain::{Collection, EntityId, PinnedStreamSelection, Playlist, SmartCollection};

use super::{ArtworkReference, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionAdminRow {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartCollectionAdminRow {
    pub id: EntityId,
    pub name: String,
    pub query: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiCollectionAdminRow {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiCollectionItem {
    pub collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub name: String,
    pub collection_type: i32,
    pub use_custom_playback_order: bool,
    pub schedule_as_group: bool,
    pub playback_order: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiCollectionAdmin {
    pub id: EntityId,
    pub name: String,
    pub items: Vec<MultiCollectionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiCollectionSelectOption {
    pub id: EntityId,
    pub name: String,
    pub collection_type: i32,
    pub use_custom_playback_order: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RerunCollectionAdmin {
    pub id: EntityId,
    pub name: String,
    pub collection_type: i32,
    pub collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub first_run_playback_order: i32,
    pub rerun_playback_order: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RerunCollectionAdminRow {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RerunCollectionSelectOption {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RerunCollectionEditorOptions {
    pub collections: Vec<RerunCollectionSelectOption>,
    pub multi_collections: Vec<RerunCollectionSelectOption>,
    pub smart_collections: Vec<RerunCollectionSelectOption>,
    pub shows: Vec<RerunCollectionSelectOption>,
    pub seasons: Vec<RerunCollectionSelectOption>,
    pub artists: Vec<RerunCollectionSelectOption>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RerunCollectionReferenceKind {
    Collection,
    Show,
    Season,
    Artist,
    MultiCollection,
    SmartCollection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistAdminRow {
    pub id: EntityId,
    pub playlist_group_id: EntityId,
    pub name: String,
    pub is_system: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistGroupAdminRow {
    pub id: EntityId,
    pub name: String,
    pub is_system: bool,
    pub playlists: Vec<PlaylistAdminRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistItemAdmin {
    pub id: EntityId,
    pub index: i32,
    pub collection_type: i32,
    pub collection_id: Option<EntityId>,
    pub multi_collection_id: Option<EntityId>,
    pub smart_collection_id: Option<EntityId>,
    pub media_item_id: Option<EntityId>,
    pub item_name: String,
    pub playback_order: i32,
    pub count: Option<i32>,
    pub play_all: bool,
    pub include_in_program_guide: bool,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<i32>,
    pub pinned_stream_selection: Option<PinnedStreamSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistEditorAdmin {
    pub id: EntityId,
    pub playlist_group_id: EntityId,
    pub name: String,
    pub is_system: bool,
    pub items: Vec<PlaylistItemAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistItemOption {
    pub id: EntityId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistEditorOptions {
    pub collections: Vec<PlaylistItemOption>,
    pub multi_collections: Vec<PlaylistItemOption>,
    pub smart_collections: Vec<PlaylistItemOption>,
    pub shows: Vec<PlaylistItemOption>,
    pub seasons: Vec<PlaylistItemOption>,
    pub artists: Vec<PlaylistItemOption>,
    pub movies: Vec<PlaylistItemOption>,
    pub episodes: Vec<PlaylistItemOption>,
    pub music_videos: Vec<PlaylistItemOption>,
    pub other_videos: Vec<PlaylistItemOption>,
    pub songs: Vec<PlaylistItemOption>,
    pub images: Vec<PlaylistItemOption>,
    pub remote_streams: Vec<PlaylistItemOption>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaybackOverrides {
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: Option<String>,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistItemInsert {
    pub collection_type: i32,
    pub media_item_id: EntityId,
    pub playback: PlaybackOverrides,
    pub pinned_stream_selection: Option<PinnedStreamSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionItemCardAdmin {
    pub collection_item_id: EntityId,
    pub media_item_id: EntityId,
    pub kind: String,
    pub title: String,
    pub sort_title: String,
    pub subtitle: Option<String>,
    pub artwork_path: Option<ArtworkReference>,
    pub custom_index: i64,
    pub playback: PlaybackOverrides,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionDetailAdmin {
    pub id: EntityId,
    pub name: String,
    pub use_custom_playback_order: bool,
    pub items: Vec<CollectionItemCardAdmin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionMemberAdmin {
    pub media_item_id: EntityId,
    pub title: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaItemSuggestionAdmin {
    pub id: EntityId,
    pub title: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiCollectionEditorOptions {
    pub collections: Vec<MultiCollectionSelectOption>,
    pub smart_collections: Vec<MultiCollectionSelectOption>,
}

#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn get_with_items(&self, id: EntityId) -> Result<Collection>;
    async fn list_for_media_item(&self, item_id: EntityId) -> Result<Vec<Collection>>;
    async fn get_smart(&self, id: EntityId) -> Result<SmartCollection>;
    async fn get_playlist_with_items(&self, id: EntityId) -> Result<Playlist>;
    async fn upsert(&self, collection: &Collection) -> Result<()>;
    async fn list_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<CollectionAdminRow>, u64)>;
    async fn get_admin(&self, id: EntityId) -> Result<Option<CollectionAdminRow>>;
    async fn admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn save_admin(&self, id: EntityId, name: &str) -> Result<EntityId>;
    async fn create_with_default_item_admin(
        &self,
        name: &str,
        media_item_id: EntityId,
    ) -> Result<EntityId>;
    async fn delete_admin(&self, id: EntityId) -> Result<()>;
    async fn add_default_item(
        &self,
        collection_id: EntityId,
        media_item_id: EntityId,
    ) -> Result<()>;
    async fn remove_item(&self, collection_id: EntityId, media_item_id: EntityId) -> Result<()>;
    async fn playout_ids_using(&self, collection_id: EntityId) -> Result<Vec<EntityId>>;
    async fn list_smart_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<SmartCollectionAdminRow>, u64)>;
    async fn get_smart_admin(&self, id: EntityId) -> Result<Option<SmartCollectionAdminRow>>;
    async fn smart_admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn save_smart_admin(&self, id: EntityId, name: &str, query: &str) -> Result<EntityId>;
    async fn delete_smart_admin(&self, id: EntityId) -> Result<()>;
    async fn playout_ids_using_smart(&self, id: EntityId) -> Result<Vec<EntityId>>;
    async fn list_multi_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<MultiCollectionAdminRow>, u64)>;
    async fn get_multi_admin(&self, id: EntityId) -> Result<Option<MultiCollectionAdmin>>;
    async fn get_multi_editor_options(&self) -> Result<MultiCollectionEditorOptions>;
    async fn multi_admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn save_multi_admin(
        &self,
        id: EntityId,
        name: &str,
        items: &[MultiCollectionItem],
        replace_items: bool,
    ) -> Result<EntityId>;
    async fn delete_multi_admin(&self, id: EntityId) -> Result<()>;
    async fn playout_ids_using_multi(&self, id: EntityId) -> Result<Vec<EntityId>>;
    async fn list_rerun_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<RerunCollectionAdminRow>, u64)>;
    async fn get_rerun_admin(&self, id: EntityId) -> Result<Option<RerunCollectionAdmin>>;
    async fn get_rerun_editor_options(&self) -> Result<RerunCollectionEditorOptions>;
    async fn rerun_admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool>;
    async fn rerun_reference_exists(
        &self,
        kind: RerunCollectionReferenceKind,
        id: EntityId,
    ) -> Result<bool>;
    async fn save_rerun_admin(&self, form: &RerunCollectionAdmin) -> Result<EntityId>;
    async fn delete_rerun_admin(&self, id: EntityId) -> Result<()>;
    async fn playout_ids_using_rerun(&self, id: EntityId) -> Result<Vec<EntityId>>;
    async fn list_playlist_groups_admin(&self) -> Result<Vec<PlaylistGroupAdminRow>>;
    async fn get_playlist_group_admin(&self, id: EntityId)
        -> Result<Option<PlaylistGroupAdminRow>>;
    async fn create_playlist_group_admin(&self, name: &str) -> Result<PlaylistGroupAdminRow>;
    async fn playlist_name_exists(&self, group_id: EntityId, name: &str) -> Result<bool>;
    async fn create_playlist_admin(
        &self,
        group_id: EntityId,
        name: &str,
    ) -> Result<PlaylistAdminRow>;
    async fn delete_playlist_group_admin(&self, id: EntityId) -> Result<()>;
    async fn delete_playlist_admin(&self, id: EntityId) -> Result<()>;
    async fn get_playlist_editor_admin(&self, id: EntityId) -> Result<Option<PlaylistEditorAdmin>>;
    async fn get_playlist_editor_options(&self) -> Result<PlaylistEditorOptions>;
    async fn save_playlist_admin(&self, form: &PlaylistEditorAdmin) -> Result<()>;
    async fn suggest_media_items_admin(
        &self,
        prefix: &str,
    ) -> Result<Vec<MediaItemSuggestionAdmin>>;
    async fn media_items_exist(&self, ids: &[EntityId]) -> Result<bool>;
    async fn add_collection_items_admin(
        &self,
        collection_id: EntityId,
        media_item_ids: &[EntityId],
        playback: &PlaybackOverrides,
    ) -> Result<u64>;
    async fn add_playlist_items_admin(
        &self,
        playlist_id: EntityId,
        items: &[PlaylistItemInsert],
    ) -> Result<u64>;
    async fn get_collection_detail_admin(
        &self,
        id: EntityId,
    ) -> Result<Option<CollectionDetailAdmin>>;
    async fn set_collection_custom_order_enabled(
        &self,
        id: EntityId,
        enabled: bool,
    ) -> Result<bool>;
    async fn remove_collection_rows(
        &self,
        collection_id: EntityId,
        row_ids: &[EntityId],
    ) -> Result<u64>;
    async fn update_collection_custom_order(
        &self,
        collection_id: EntityId,
        ordered_row_ids: &[EntityId],
    ) -> Result<u64>;
    async fn list_collection_members_admin(
        &self,
        collection_id: EntityId,
    ) -> Result<Vec<CollectionMemberAdmin>>;
}
