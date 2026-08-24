//! Server functions for block group / block management and the block editor.
//! Ports:
//! - `Pages/Blocks.razor` → [`list_block_groups`], [`list_blocks`],
//!   [`create_block_group`], [`delete_block_group`], [`create_block`],
//!   [`copy_block`], [`delete_block`]
//! - `Pages/BlockEditor.razor` → [`get_block`], [`get_block_items`],
//!   [`save_block_with_items`], [`preview_block_playout`]
//! - Autocomplete helpers → [`search_collections_for_block`],
//!   [`search_multi_collections_for_block`], [`search_smart_collections_for_block`],
//!   [`search_shows_for_block`], [`search_seasons_for_block`]
#![cfg_attr(test, allow(dead_code))]

#[cfg(all(test, feature = "ssr"))]
use ersatzrs_infra::db::DbPool;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::schedules::SmartShuffleForm;

#[cfg(feature = "ssr")]
use super::schedules::{smart_shuffle_form_from_policy, smart_shuffle_policy_from_form};

#[cfg(all(test, feature = "ssr"))]
use super::schedules::{persist_smart_shuffle_attachment_tx, validate_smart_shuffle_form};

#[cfg(all(test, feature = "ssr"))]
use ersatzrs_core::scheduling::block_item_smart_shuffle_config_key;

#[cfg(all(test, feature = "ssr"))]
use ersatzrs_core::scheduling::{legacy_playout_cooldown_config_key, SchedulingSmartShufflePolicy};

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

/// `CollectionType` discriminants (mirrors `ErsatzTV.Core.Domain.CollectionType`).
pub mod collection_type {
    pub const COLLECTION: i32 = 0;
    pub const TELEVISION_SHOW: i32 = 1;
    pub const TELEVISION_SEASON: i32 = 2;
    pub const ARTIST: i32 = 3;
    pub const MULTI_COLLECTION: i32 = 4;
    pub const SMART_COLLECTION: i32 = 5;
    pub const PLAYLIST: i32 = 6;
    pub const SEARCH_QUERY: i32 = 9;
    pub const MOVIE: i32 = 10;
    pub const EPISODE: i32 = 20;
    pub const MUSIC_VIDEO: i32 = 30;
    pub const OTHER_VIDEO: i32 = 40;
    pub const SONG: i32 = 50;
    pub const IMAGE: i32 = 60;
    pub const REMOTE_STREAM: i32 = 70;
}

/// `PlaybackOrder` discriminants (mirrors `ErsatzTV.Core.Domain.PlaybackOrder`).
pub mod playback_order {
    pub const CHRONOLOGICAL: i32 = 1;
    pub const RANDOM: i32 = 2;
    pub const SHUFFLE: i32 = 3;
    pub const SHUFFLE_IN_ORDER: i32 = 4;
    pub const SEASON_EPISODE: i32 = 6;
    pub const RANDOM_ROTATION: i32 = 7;
}

/// `BlockStopScheduling` discriminants.
pub mod stop_scheduling {
    pub const AFTER_DURATION_END: i32 = 0;
    pub const BEFORE_DURATION_END: i32 = 1;
}

// ---------------------------------------------------------------------------
// Row / form types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockGroupRow {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockRow {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub name: String,
    pub minutes: i32,
    pub stop_scheduling: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockForm {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub name: String,
    pub minutes: i32,
    pub stop_scheduling: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockItemForm {
    pub id: i64,
    pub index: i32,
    pub collection_type: i32,
    pub collection_id: Option<i64>,
    pub multi_collection_id: Option<i64>,
    pub smart_collection_id: Option<i64>,
    #[serde(default)]
    pub playlist_id: Option<i64>,
    pub media_item_id: Option<i64>,
    pub search_title: Option<String>,
    pub search_query: Option<String>,
    pub playback_order: i32,
    #[serde(default)]
    pub smart_shuffle: SmartShuffleForm,
    pub include_in_program_guide: bool,
    pub disable_watermarks: bool,
    /// Resolved display name for the referenced collection / show / season.
    pub collection_name: String,
    /// IDs from `BlockItemWatermark` — round-tripped through save unchanged.
    #[serde(default)]
    pub watermark_ids: Vec<i64>,
    /// IDs from `BlockItemGraphicsElement` — round-tripped through save unchanged.
    #[serde(default)]
    pub graphics_element_ids: Vec<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockEditorOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockEditorOptions {
    pub collections: Vec<BlockEditorOption>,
    pub shows: Vec<BlockEditorOption>,
    pub seasons: Vec<BlockEditorOption>,
    pub multi_collections: Vec<BlockEditorOption>,
    pub smart_collections: Vec<BlockEditorOption>,
    pub watermarks: Vec<BlockEditorOption>,
    pub graphics_elements: Vec<BlockEditorOption>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockWithItemsForm {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub minutes: i32,
    pub stop_scheduling: i32,
    #[serde(default)]
    pub items: Vec<BlockItemForm>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayoutPreviewRow {
    pub start: String,
    pub finish: String,
    pub title: String,
    pub duration: String,
}

#[cfg(feature = "ssr")]
pub(crate) fn block_form_into_command(
    form: BlockWithItemsForm,
) -> ersatzrs_app::commands::SaveBlock {
    ersatzrs_app::commands::SaveBlock {
        id: form.id,
        group_id: form.group_id,
        name: form.name,
        minutes: form.minutes,
        stop_scheduling: form.stop_scheduling,
        items: form
            .items
            .into_iter()
            .map(|item| ersatzrs_app::commands::BlockItemAdminForm {
                id: item.id,
                index: item.index,
                collection_type: item.collection_type,
                collection_id: item.collection_id,
                multi_collection_id: item.multi_collection_id,
                smart_collection_id: item.smart_collection_id,
                playlist_id: item.playlist_id,
                media_item_id: item.media_item_id,
                search_title: item.search_title,
                search_query: item.search_query,
                playback_order: item.playback_order,
                smart_shuffle: smart_shuffle_policy_from_form(&item.smart_shuffle),
                inherited_legacy_smart_shuffle: item.smart_shuffle.inherited_legacy,
                include_in_program_guide: item.include_in_program_guide,
                disable_watermarks: item.disable_watermarks,
                collection_name: item.collection_name,
                watermark_ids: item.watermark_ids,
                graphics_element_ids: item.graphics_element_ids,
            })
            .collect(),
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn block_form_from_admin(
    block: ersatzrs_core::repositories::BlockAdminRow,
) -> BlockForm {
    BlockForm {
        id: block.id,
        group_id: block.group_id,
        group_name: block.group_name,
        name: block.name,
        minutes: block.minutes,
        stop_scheduling: stop_scheduling_value(block.stop_scheduling),
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn block_item_form_from_admin(
    item: ersatzrs_app::commands::BlockItemAdminForm,
) -> BlockItemForm {
    let inherited = item.inherited_legacy_smart_shuffle;
    let mut smart_shuffle = smart_shuffle_form_from_policy(&item.smart_shuffle);
    smart_shuffle.inherited_legacy = inherited;
    BlockItemForm {
        id: item.id,
        index: item.index,
        collection_type: item.collection_type,
        collection_id: item.collection_id,
        multi_collection_id: item.multi_collection_id,
        smart_collection_id: item.smart_collection_id,
        playlist_id: item.playlist_id,
        media_item_id: item.media_item_id,
        search_title: item.search_title,
        search_query: item.search_query,
        playback_order: item.playback_order,
        smart_shuffle,
        include_in_program_guide: item.include_in_program_guide,
        disable_watermarks: item.disable_watermarks,
        collection_name: item.collection_name,
        watermark_ids: item.watermark_ids,
        graphics_element_ids: item.graphics_element_ids,
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn block_editor_options_from_admin(
    options: ersatzrs_core::repositories::BlockEditorOptionsAdmin,
) -> BlockEditorOptions {
    BlockEditorOptions {
        collections: map_block_editor_options(options.collections),
        shows: map_block_editor_options(options.shows),
        seasons: map_block_editor_options(options.seasons),
        multi_collections: map_block_editor_options(options.multi_collections),
        smart_collections: map_block_editor_options(options.smart_collections),
        watermarks: map_block_editor_options(options.watermarks),
        graphics_elements: map_block_editor_options(options.graphics_elements),
    }
}

// ---------------------------------------------------------------------------
// Block groups
// ---------------------------------------------------------------------------

#[server]
pub async fn list_block_groups() -> Result<Vec<BlockGroupRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_block_groups()
        .await
        .map(|groups| {
            groups
                .into_iter()
                .map(|group| BlockGroupRow {
                    id: group.id,
                    name: group.name,
                })
                .collect()
        })
        .map_err(ServerFnError::new)
}

#[server]
pub async fn create_block_group(name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .create_block_group(ersatzrs_app::commands::CreateBlockGroup { name })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn delete_block_group(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .delete_block_group(ersatzrs_app::commands::DeleteBlockGroup { id })
        .await
        .map_err(ServerFnError::new)
}

// ---------------------------------------------------------------------------
// Blocks list
// ---------------------------------------------------------------------------

#[server]
pub async fn list_blocks(search: String) -> Result<Vec<BlockRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_blocks(&search)
        .await
        .map(|blocks| {
            blocks
                .into_iter()
                .map(|block| BlockRow {
                    id: block.id,
                    group_id: block.group_id,
                    group_name: block.group_name,
                    name: block.name,
                    minutes: block.minutes,
                    stop_scheduling: stop_scheduling_value(block.stop_scheduling),
                })
                .collect()
        })
        .map_err(ServerFnError::new)
}

#[server]
pub async fn create_block(group_id: i64, name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .create_block(ersatzrs_app::commands::CreateBlock { group_id, name })
        .await
        .map_err(ServerFnError::new)
}

#[server]
#[allow(clippy::too_many_lines)]
pub async fn copy_block(
    block_id: i64,
    new_group_id: i64,
    new_name: String,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .copy_block(ersatzrs_app::commands::CopyBlock {
            block_id,
            new_group_id,
            name: new_name,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
async fn copy_block_with_pool(
    pool: &DbPool,
    block_id: i64,
    new_group_id: i64,
    new_name: &str,
) -> Result<i64, ServerFnError> {
    use sqlx::Row;

    let new_name = validate_block_name(pool, new_group_id, None, new_name).await?;
    let block = sqlx::query(r#"SELECT "Minutes", "StopScheduling" FROM "Block" WHERE "Id" = ?"#)
        .bind(block_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ServerFnError::new("Block does not exist."))?;

    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"INSERT INTO "Block" ("BlockGroupId", "Name", "Minutes", "StopScheduling", "DateUpdated")
           VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)"#,
    )
    .bind(new_group_id)
    .bind(&new_name)
    .bind(block.try_get::<i32, _>("Minutes").unwrap_or(30))
    .bind(block.try_get::<i32, _>("StopScheduling").unwrap_or(0))
    .execute(&mut *tx)
    .await?;
    let new_block_id = ersatzrs_infra::db::last_insert_id(result);

    let items = sqlx::query(
        r#"SELECT "Id", "Index", "CollectionType", "CollectionId", "MultiCollectionId",
                  "SmartCollectionId", "MediaItemId", "SearchTitle", "SearchQuery",
                  "PlaybackOrder", "IncludeInProgramGuide", "DisableWatermarks"
           FROM "BlockItem"
           WHERE "BlockId" = ?
           ORDER BY "Index""#,
    )
    .bind(block_id)
    .fetch_all(&mut *tx)
    .await?;

    for item in items {
        let old_item_id = item.try_get::<i64, _>("Id").unwrap_or(0);
        let result = sqlx::query(
            r#"INSERT INTO "BlockItem"
               ("BlockId", "Index", "CollectionType", "CollectionId", "MultiCollectionId",
                "SmartCollectionId", "MediaItemId", "SearchTitle", "SearchQuery",
                "PlaybackOrder", "IncludeInProgramGuide", "DisableWatermarks")
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(new_block_id)
        .bind(item.try_get::<i32, _>("Index").unwrap_or(0))
        .bind(
            item.try_get::<i32, _>("CollectionType")
                .unwrap_or(collection_type::COLLECTION),
        )
        .bind(
            item.try_get::<Option<i64>, _>("CollectionId")
                .unwrap_or(None),
        )
        .bind(
            item.try_get::<Option<i64>, _>("MultiCollectionId")
                .unwrap_or(None),
        )
        .bind(
            item.try_get::<Option<i64>, _>("SmartCollectionId")
                .unwrap_or(None),
        )
        .bind(
            item.try_get::<Option<i64>, _>("MediaItemId")
                .unwrap_or(None),
        )
        .bind(
            item.try_get::<Option<String>, _>("SearchTitle")
                .unwrap_or(None),
        )
        .bind(
            item.try_get::<Option<String>, _>("SearchQuery")
                .unwrap_or(None),
        )
        .bind(
            item.try_get::<i32, _>("PlaybackOrder")
                .unwrap_or(playback_order::CHRONOLOGICAL),
        )
        .bind(
            item.try_get::<bool, _>("IncludeInProgramGuide")
                .unwrap_or(false),
        )
        .bind(
            item.try_get::<bool, _>("DisableWatermarks")
                .unwrap_or(false),
        )
        .execute(&mut *tx)
        .await?;
        let new_item_id = ersatzrs_infra::db::last_insert_id(result);

        copy_block_item_join_rows(
            &mut tx,
            r#""BlockItemWatermark""#,
            r#""WatermarkId""#,
            old_item_id,
            new_item_id,
        )
        .await?;
        copy_block_item_join_rows(
            &mut tx,
            r#""BlockItemGraphicsElement""#,
            r#""GraphicsElementId""#,
            old_item_id,
            new_item_id,
        )
        .await?;
        let value = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(block_item_smart_shuffle_config_key(old_item_id))
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(json) = value {
            let policy = serde_json::from_str(&json).map_err(ServerFnError::new)?;
            persist_smart_shuffle_attachment_tx(
                &mut tx,
                &block_item_smart_shuffle_config_key(new_item_id),
                &policy,
            )
            .await?;
        }
    }

    tx.commit().await?;
    Ok(new_block_id)
}

#[server]
pub async fn delete_block(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    state
        .application
        .delete_block(ersatzrs_app::commands::DeleteBlock { id })
        .await
        .map_err(ServerFnError::new)
}

// ---------------------------------------------------------------------------
// Block editor
// ---------------------------------------------------------------------------

#[cfg(test)]
fn parse_id_list(s: Option<String>) -> Vec<i64> {
    s.unwrap_or_default()
        .split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<i64>().ok())
        .collect()
}

#[cfg(all(test, feature = "ssr"))]
#[allow(dead_code)]
async fn validate_block_group_name(pool: &DbPool, name: &str) -> Result<String, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Block group name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(format!(
            "Block group name \"{name}\" is invalid"
        )));
    }
    let duplicate: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "BlockGroup" WHERE "Name" = ?"#)
            .bind(name)
            .fetch_one(pool)
            .await?;
    if duplicate > 0 {
        return Err(ServerFnError::new("Block group name must be unique"));
    }
    Ok(name.to_string())
}

#[cfg(all(test, feature = "ssr"))]
async fn validate_block_name(
    pool: &DbPool,
    group_id: i64,
    block_id: Option<i64>,
    name: &str,
) -> Result<String, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Block name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(format!(
            "Block name \"{name}\" is invalid"
        )));
    }
    let duplicate: i64 = match block_id {
        Some(id) => {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Block"
                   WHERE "BlockGroupId" = ? AND "Name" = ? AND "Id" != ?"#,
            )
            .bind(group_id)
            .bind(name)
            .bind(id)
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Block"
                   WHERE "BlockGroupId" = ? AND "Name" = ?"#,
            )
            .bind(group_id)
            .bind(name)
            .fetch_one(pool)
            .await?
        }
    };
    if duplicate > 0 {
        return Err(ServerFnError::new(
            "A block named with that value already exists in that block group",
        ));
    }
    Ok(name.to_string())
}

#[cfg(test)]
fn validate_block_duration(minutes: i32) -> Result<(), ServerFnError> {
    if minutes > 0 && minutes % 5 == 0 && minutes <= 24 * 60 {
        Ok(())
    } else {
        Err(ServerFnError::new(
            "Block duration must be between 5 minutes and 24 hours",
        ))
    }
}

#[cfg(test)]
fn validate_block_item(item: &BlockItemForm) -> Result<(), ServerFnError> {
    match item.collection_type {
        collection_type::COLLECTION if item.collection_id.is_none() => Err(ServerFnError::new(
            "[Collection] is required for collection type 'Collection'",
        )),
        collection_type::TELEVISION_SHOW if item.media_item_id.is_none() => Err(
            ServerFnError::new("[MediaItem] is required for collection type 'TelevisionShow'"),
        ),
        collection_type::TELEVISION_SEASON if item.media_item_id.is_none() => Err(
            ServerFnError::new("[MediaItem] is required for collection type 'TelevisionSeason'"),
        ),
        collection_type::ARTIST
        | collection_type::MOVIE
        | collection_type::EPISODE
        | collection_type::MUSIC_VIDEO
        | collection_type::OTHER_VIDEO
        | collection_type::SONG
        | collection_type::IMAGE
        | collection_type::REMOTE_STREAM
            if item.media_item_id.is_none() =>
        {
            Err(ServerFnError::new(
                "[MediaItem] is required for this collection type",
            ))
        }
        collection_type::MULTI_COLLECTION if item.multi_collection_id.is_none() => {
            Err(ServerFnError::new(
                "[MultiCollection] is required for collection type 'MultiCollection'",
            ))
        }
        collection_type::SMART_COLLECTION if item.smart_collection_id.is_none() => {
            Err(ServerFnError::new(
                "[SmartCollection] is required for collection type 'SmartCollection'",
            ))
        }
        collection_type::PLAYLIST if item.playlist_id.is_none() => Err(ServerFnError::new(
            "[Playlist] is required for collection type 'Playlist'",
        )),
        collection_type::SEARCH_QUERY
            if item
                .search_query
                .as_deref()
                .is_none_or(|query| query.trim().is_empty()) =>
        {
            Err(ServerFnError::new(
                "[SearchQuery] is required for collection type 'SearchQuery'",
            ))
        }
        collection_type::COLLECTION
        | collection_type::TELEVISION_SHOW
        | collection_type::TELEVISION_SEASON
        | collection_type::ARTIST
        | collection_type::MULTI_COLLECTION
        | collection_type::SMART_COLLECTION
        | collection_type::PLAYLIST
        | collection_type::SEARCH_QUERY
        | collection_type::MOVIE
        | collection_type::EPISODE
        | collection_type::MUSIC_VIDEO
        | collection_type::OTHER_VIDEO
        | collection_type::SONG
        | collection_type::IMAGE
        | collection_type::REMOTE_STREAM => Ok(()),
        value => Err(ServerFnError::new(format!(
            "Collection type {value} is not supported by the real block scheduler"
        ))),
    }
}

#[cfg(all(test, feature = "ssr"))]
async fn validate_block_form(
    pool: &DbPool,
    form: &BlockWithItemsForm,
) -> Result<(), ServerFnError> {
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Block" WHERE "Id" = ?"#)
        .bind(form.id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("[BlockId] does not exist."));
    }
    validate_block_duration(form.minutes)?;
    validate_block_name(pool, form.group_id, Some(form.id), &form.name).await?;
    for item in &form.items {
        if item.collection_type == collection_type::PLAYLIST {
            return Err(ServerFnError::new(
                "Playlist block items can be previewed but are not persisted by ErsatzTV's BlockItem schema",
            ));
        }
        validate_block_item(item)?;
        validate_smart_shuffle_form(&item.smart_shuffle)?;
    }
    Ok(())
}

#[server]
pub async fn get_block(id: i64) -> Result<Option<BlockForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_block(id)
        .await
        .map(|block| {
            block.map(|block| BlockForm {
                id: block.id,
                group_id: block.group_id,
                group_name: block.group_name,
                name: block.name,
                minutes: block.minutes,
                stop_scheduling: stop_scheduling_value(block.stop_scheduling),
            })
        })
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
pub async fn get_block_with_pool(
    pool: &DbPool,
    id: i64,
) -> Result<Option<BlockForm>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT b."Id",
                  b."BlockGroupId",
                  COALESCE(bg."Name", '') AS "GroupName",
                  COALESCE(b."Name", '') AS "Name",
                  b."Minutes",
                  b."StopScheduling"
           FROM "Block" b
           JOIN "BlockGroup" bg ON bg."Id" = b."BlockGroupId"
           WHERE b."Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| BlockForm {
        id: r.try_get("Id").unwrap_or(0),
        group_id: r.try_get("BlockGroupId").unwrap_or(0),
        group_name: r.try_get("GroupName").unwrap_or_default(),
        name: r.try_get("Name").unwrap_or_default(),
        minutes: r.try_get("Minutes").unwrap_or(15),
        stop_scheduling: r.try_get("StopScheduling").unwrap_or(0),
    }))
}

#[server]
pub async fn get_block_items(block_id: i64) -> Result<Vec<BlockItemForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_block_items(block_id)
        .await
        .map(|items| {
            items
                .into_iter()
                .map(|item| {
                    let inherited = item.inherited_legacy_smart_shuffle;
                    let mut smart_shuffle = smart_shuffle_form_from_policy(&item.smart_shuffle);
                    smart_shuffle.inherited_legacy = inherited;
                    BlockItemForm {
                        id: item.id,
                        index: item.index,
                        collection_type: item.collection_type,
                        collection_id: item.collection_id,
                        multi_collection_id: item.multi_collection_id,
                        smart_collection_id: item.smart_collection_id,
                        playlist_id: item.playlist_id,
                        media_item_id: item.media_item_id,
                        search_title: item.search_title,
                        search_query: item.search_query,
                        playback_order: item.playback_order,
                        smart_shuffle,
                        include_in_program_guide: item.include_in_program_guide,
                        disable_watermarks: item.disable_watermarks,
                        collection_name: item.collection_name,
                        watermark_ids: item.watermark_ids,
                        graphics_element_ids: item.graphics_element_ids,
                    }
                })
                .collect()
        })
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
pub async fn get_block_items_with_pool(
    pool: &DbPool,
    block_id: i64,
) -> Result<Vec<BlockItemForm>, ServerFnError> {
    let rows = sqlx::query(
        r#"SELECT
               bi."Id",
               bi."Index",
               bi."CollectionType",
               bi."CollectionId",
               bi."MultiCollectionId",
               bi."SmartCollectionId",
               bi."MediaItemId",
               bi."SearchTitle",
               bi."SearchQuery",
               bi."PlaybackOrder",
               bi."IncludeInProgramGuide",
               bi."DisableWatermarks",
               s."SeasonNumber",
               CASE bi."CollectionType"
                   WHEN 0 THEN COALESCE(c."Name", '')
                   WHEN 1 THEN COALESCE(sm."Title", '')
                   WHEN 2 THEN COALESCE(sh."Title", '')
                   WHEN 3 THEN COALESCE(am."Title", '')
                   WHEN 4 THEN COALESCE(mc."Name", '')
                   WHEN 5 THEN COALESCE(sc."Name", '')
                   WHEN 9 THEN COALESCE(bi."SearchTitle", bi."SearchQuery", '')
                   WHEN 10 THEN COALESCE(mm."Title", '')
                   WHEN 20 THEN COALESCE(em."Title", '')
                   WHEN 30 THEN COALESCE(mvm."Title", '')
                   WHEN 40 THEN COALESCE(ovm."Title", '')
                   WHEN 50 THEN COALESCE(som."Title", '')
                   WHEN 60 THEN COALESCE(im."Title", '')
                   WHEN 70 THEN COALESCE(rsm."Title", '')
                   ELSE ''
               END AS "CollectionName",
               (SELECT GROUP_CONCAT(biw."WatermarkId")
                FROM "BlockItemWatermark" biw WHERE biw."BlockItemId" = bi."Id") AS "WatermarkIds",
               (SELECT GROUP_CONCAT(bige."GraphicsElementId")
                FROM "BlockItemGraphicsElement" bige WHERE bige."BlockItemId" = bi."Id") AS "GraphicsElementIds"
           FROM "BlockItem" bi
           LEFT JOIN "Collection"     c  ON c."Id"  = bi."CollectionId"
           LEFT JOIN "MultiCollection" mc ON mc."Id" = bi."MultiCollectionId"
           LEFT JOIN "SmartCollection" sc ON sc."Id" = bi."SmartCollectionId"
           LEFT JOIN "Season"         s  ON s."Id"  = bi."MediaItemId" AND bi."CollectionType" = 2
           LEFT JOIN "Show"           sh_row ON sh_row."Id" = s."ShowId"
           LEFT JOIN "ShowMetadata"   sh ON sh."ShowId" = sh_row."Id"
           LEFT JOIN "Show"           show_tv ON show_tv."Id" = bi."MediaItemId" AND bi."CollectionType" = 1
           LEFT JOIN "ShowMetadata"   sm ON sm."ShowId" = show_tv."Id"
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = bi."MediaItemId"
           LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = bi."MediaItemId"
           LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = bi."MediaItemId"
           LEFT JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = bi."MediaItemId"
           LEFT JOIN "OtherVideoMetadata" ovm ON ovm."OtherVideoId" = bi."MediaItemId"
           LEFT JOIN "SongMetadata" som ON som."SongId" = bi."MediaItemId"
           LEFT JOIN "ImageMetadata" im ON im."ImageId" = bi."MediaItemId"
           LEFT JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = bi."MediaItemId"
           WHERE bi."BlockId" = ?
           ORDER BY bi."Index""#,
    )
    .bind(block_id)
    .fetch_all(pool)
    .await?;

    let mut items = rows
        .iter()
        .map(block_item_form_from_row)
        .collect::<Vec<_>>();
    for item in &mut items {
        let value = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#,
        )
        .bind(block_item_smart_shuffle_config_key(item.id))
        .fetch_optional(pool)
        .await?;
        if let Some(json) = value {
            let policy = serde_json::from_str(&json).map_err(ServerFnError::new)?;
            item.smart_shuffle = smart_shuffle_form_from_policy(&policy);
            item.smart_shuffle.inherited_legacy = false;
        } else {
            item.smart_shuffle.inherited_legacy = true;
        }
    }
    Ok(items)
}

#[cfg(all(test, feature = "ssr"))]
fn block_item_form_from_row(row: &ersatzrs_infra::db::DbRow) -> BlockItemForm {
    use sqlx::Row;

    let collection_type = row.try_get("CollectionType").unwrap_or(0);
    let collection_name: String = row.try_get("CollectionName").unwrap_or_default();
    BlockItemForm {
        id: row.try_get("Id").unwrap_or(0),
        index: row.try_get("Index").unwrap_or(0),
        collection_type,
        collection_id: row.try_get("CollectionId").unwrap_or(None),
        multi_collection_id: row.try_get("MultiCollectionId").unwrap_or(None),
        smart_collection_id: row.try_get("SmartCollectionId").unwrap_or(None),
        playlist_id: None,
        media_item_id: row.try_get("MediaItemId").unwrap_or(None),
        search_title: row.try_get("SearchTitle").unwrap_or(None),
        search_query: row.try_get("SearchQuery").unwrap_or(None),
        playback_order: row.try_get("PlaybackOrder").unwrap_or(1),
        smart_shuffle: SmartShuffleForm::default(),
        include_in_program_guide: row
            .try_get::<bool, _>("IncludeInProgramGuide")
            .unwrap_or(false),
        disable_watermarks: row.try_get::<bool, _>("DisableWatermarks").unwrap_or(false),
        collection_name: if collection_type == collection_type::TELEVISION_SEASON {
            season_label(&collection_name, row.try_get("SeasonNumber").unwrap_or(0))
        } else {
            collection_name
        },
        watermark_ids: parse_id_list(row.try_get("WatermarkIds").unwrap_or(None)),
        graphics_element_ids: parse_id_list(row.try_get("GraphicsElementIds").unwrap_or(None)),
    }
}

/// Replace the block metadata and all its items atomically.
#[server]
pub async fn save_block_with_items(form: BlockWithItemsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .save_block(block_form_into_command(form))
        .await
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
pub async fn save_block_with_pool(
    pool: &DbPool,
    form: BlockWithItemsForm,
) -> Result<(), ServerFnError> {
    validate_block_form(pool, &form).await?;
    let name = form.name.trim();
    let old_item_ids: Vec<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = ?"#)
            .bind(form.id)
            .fetch_all(pool)
            .await?;

    let mut tx = pool.begin().await?;
    for old_item_id in old_item_ids {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(block_item_smart_shuffle_config_key(old_item_id))
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query(
        r#"UPDATE "Block"
           SET "Name" = ?, "Minutes" = ?, "StopScheduling" = ?, "DateUpdated" = CURRENT_TIMESTAMP
           WHERE "Id" = ?"#,
    )
    .bind(name)
    .bind(form.minutes)
    .bind(form.stop_scheduling)
    .bind(form.id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(r#"DELETE FROM "BlockItem" WHERE "BlockId" = ?"#)
        .bind(form.id)
        .execute(&mut *tx)
        .await?;

    for item in &form.items {
        let result = sqlx::query(
            r#"INSERT INTO "BlockItem"
               ("BlockId", "Index", "CollectionType", "CollectionId",
                "MultiCollectionId", "SmartCollectionId", "MediaItemId", "SearchTitle", "SearchQuery",
                "PlaybackOrder", "IncludeInProgramGuide", "DisableWatermarks")
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(form.id)
        .bind(item.index)
        .bind(item.collection_type)
        .bind(item.collection_id)
        .bind(item.multi_collection_id)
        .bind(item.smart_collection_id)
        .bind(item.media_item_id)
        .bind(item.search_title.as_deref())
        .bind(item.search_query.as_deref())
        .bind(item.playback_order)
        .bind(item.include_in_program_guide)
        .bind(item.disable_watermarks)
        .execute(&mut *tx)
        .await?;
        let new_id = ersatzrs_infra::db::last_insert_id(result);
        if !(item.smart_shuffle.inherited_legacy && item.smart_shuffle.preset == "off") {
            persist_smart_shuffle_attachment_tx(
                &mut tx,
                &block_item_smart_shuffle_config_key(new_id),
                &smart_shuffle_policy_from_form(&item.smart_shuffle),
            )
            .await?;
        }

        for wm_id in &item.watermark_ids {
            sqlx::query(
                r#"INSERT INTO "BlockItemWatermark" ("BlockItemId", "WatermarkId") VALUES (?, ?)"#,
            )
            .bind(new_id)
            .bind(wm_id)
            .execute(&mut *tx)
            .await?;
        }

        for ge_id in &item.graphics_element_ids {
            sqlx::query(
                r#"INSERT INTO "BlockItemGraphicsElement" ("BlockItemId", "GraphicsElementId") VALUES (?, ?)"#,
            )
            .bind(new_id)
            .bind(ge_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Preview playout. Transport adaptation stays here; temporary scheduling
// state, real Reset-mode building, and enrichment live in ersatzrs-app.
// ---------------------------------------------------------------------------

#[server]
pub async fn preview_block_playout(
    form: BlockWithItemsForm,
) -> Result<Vec<PlayoutPreviewRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let query = block_preview_query(form)?;
    let rows = state
        .application
        .preview_block_playout(query)
        .await
        .map_err(ServerFnError::new)?;

    Ok(rows
        .into_iter()
        .map(|row| PlayoutPreviewRow {
            start: row.start.format("%H:%M:%S").to_string(),
            finish: row.finish.format("%H:%M:%S").to_string(),
            title: row.title,
            duration: format_hms(row.duration.num_seconds()),
        })
        .collect())
}

fn format_hms(secs: i64) -> String {
    let s = secs.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

#[cfg(not(target_arch = "wasm32"))]
fn block_preview_query(
    form: BlockWithItemsForm,
) -> Result<ersatzrs_app::queries::PreviewBlockPlayout, ServerFnError> {
    use ersatzrs_app::queries::{PreviewBlockItem, PreviewBlockPlayout};
    use ersatzrs_core::scheduling::SchedulingCollectionKey;

    let stop_scheduling = match form.stop_scheduling {
        stop_scheduling::AFTER_DURATION_END => {
            ersatzrs_domain::BlockStopScheduling::AfterDurationEnd
        }
        stop_scheduling::BEFORE_DURATION_END => {
            ersatzrs_domain::BlockStopScheduling::BeforeDurationEnd
        }
        value => {
            return Err(ServerFnError::new(format!(
                "invalid stop scheduling: {value}"
            )))
        }
    };
    let items = form
        .items
        .into_iter()
        .map(|item| {
            Ok(PreviewBlockItem {
                index: item.index,
                collection_key: SchedulingCollectionKey {
                    collection_type: preview_collection_type(item.collection_type)?,
                    collection_id: item.collection_id,
                    multi_collection_id: item.multi_collection_id,
                    smart_collection_id: item.smart_collection_id,
                    media_item_id: item.media_item_id,
                    playlist_id: item.playlist_id,
                    search_query: item.search_query,
                    fake_collection_key: None,
                },
                playback_order: preview_playback_order(item.playback_order)?,
                include_in_program_guide: item.include_in_program_guide,
                disable_watermarks: item.disable_watermarks,
                watermark_ids: item.watermark_ids,
                graphics_element_ids: item.graphics_element_ids,
                smart_shuffle_policy: smart_shuffle_policy_from_form(&item.smart_shuffle),
            })
        })
        .collect::<Result<Vec<_>, ServerFnError>>()?;

    Ok(PreviewBlockPlayout {
        name: form.name,
        minutes: form.minutes,
        stop_scheduling,
        items,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn preview_collection_type(value: i32) -> Result<ersatzrs_domain::CollectionType, ServerFnError> {
    use collection_type as kind;
    use ersatzrs_domain::CollectionType;

    match value {
        kind::COLLECTION => Ok(CollectionType::Collection),
        kind::TELEVISION_SHOW => Ok(CollectionType::TelevisionShow),
        kind::TELEVISION_SEASON => Ok(CollectionType::TelevisionSeason),
        kind::ARTIST => Ok(CollectionType::Artist),
        kind::MULTI_COLLECTION => Ok(CollectionType::MultiCollection),
        kind::SMART_COLLECTION => Ok(CollectionType::SmartCollection),
        kind::PLAYLIST => Ok(CollectionType::Playlist),
        kind::SEARCH_QUERY => Ok(CollectionType::SearchQuery),
        kind::MOVIE => Ok(CollectionType::Movie),
        kind::EPISODE => Ok(CollectionType::Episode),
        kind::MUSIC_VIDEO => Ok(CollectionType::MusicVideo),
        kind::OTHER_VIDEO => Ok(CollectionType::OtherVideo),
        kind::SONG => Ok(CollectionType::Song),
        kind::IMAGE => Ok(CollectionType::Image),
        kind::REMOTE_STREAM => Ok(CollectionType::RemoteStream),
        _ => Err(ServerFnError::new(format!(
            "unsupported block collection type: {value}"
        ))),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn preview_playback_order(value: i32) -> Result<ersatzrs_domain::PlaybackOrder, ServerFnError> {
    use ersatzrs_domain::PlaybackOrder;

    match value {
        0 => Ok(PlaybackOrder::None),
        playback_order::CHRONOLOGICAL => Ok(PlaybackOrder::Chronological),
        playback_order::RANDOM => Ok(PlaybackOrder::Random),
        playback_order::SHUFFLE => Ok(PlaybackOrder::Shuffle),
        playback_order::SHUFFLE_IN_ORDER => Ok(PlaybackOrder::ShuffleInOrder),
        5 => Ok(PlaybackOrder::MultiEpisodeShuffle),
        playback_order::SEASON_EPISODE => Ok(PlaybackOrder::SeasonEpisode),
        playback_order::RANDOM_ROTATION => Ok(PlaybackOrder::RandomRotation),
        8 => Ok(PlaybackOrder::Marathon),
        _ => Err(ServerFnError::new(format!(
            "unsupported block playback order: {value}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Autocomplete helpers
// ---------------------------------------------------------------------------

#[server]
pub async fn search_collections_for_block(
    prefix: String,
) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    search_block_options(
        &state,
        ersatzrs_app::queries::BlockOptionKind::Collection,
        &prefix,
    )
    .await
}

#[server]
pub async fn search_smart_collections_for_block(
    prefix: String,
) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    search_block_options(
        &state,
        ersatzrs_app::queries::BlockOptionKind::SmartCollection,
        &prefix,
    )
    .await
}

#[server]
pub async fn search_multi_collections_for_block(
    prefix: String,
) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    search_block_options(
        &state,
        ersatzrs_app::queries::BlockOptionKind::MultiCollection,
        &prefix,
    )
    .await
}

#[server]
pub async fn search_shows_for_block(prefix: String) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    search_block_options(
        &state,
        ersatzrs_app::queries::BlockOptionKind::Show,
        &prefix,
    )
    .await
}

#[server]
pub async fn search_seasons_for_block(prefix: String) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    search_block_options(
        &state,
        ersatzrs_app::queries::BlockOptionKind::Season,
        &prefix,
    )
    .await
}

#[cfg(feature = "ssr")]
async fn search_block_options(
    state: &crate::host::state::AppState,
    kind: ersatzrs_app::queries::BlockOptionKind,
    prefix: &str,
) -> Result<Vec<(i64, String)>, ServerFnError> {
    state
        .application
        .search_block_options(kind, prefix)
        .await
        .map(|options| {
            options
                .into_iter()
                .map(|option| (option.id, option.name))
                .collect()
        })
        .map_err(ServerFnError::new)
}

#[cfg(test)]
fn season_label(title: &str, season_number: i64) -> String {
    format!("{title} S{season_number:02}")
}

#[cfg(all(test, feature = "ssr"))]
async fn load_block_options(
    pool: &DbPool,
    sql: &str,
) -> Result<Vec<BlockEditorOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(ersatzrs_infra::db::assert_sql_safe(sql))
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| BlockEditorOption {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn copy_block_item_join_rows(
    tx: &mut sqlx::Transaction<'_, ersatzrs_infra::db::Db>,
    table: &str,
    target_column: &str,
    old_item_id: i64,
    new_item_id: i64,
) -> Result<(), ServerFnError> {
    let sql = format!(
        r#"INSERT INTO {table} ("BlockItemId", {target_column})
           SELECT ?, {target_column} FROM {table} WHERE "BlockItemId" = ?"#
    );
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(new_item_id)
        .bind(old_item_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[server]
pub async fn load_block_editor_options() -> Result<BlockEditorOptions, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .load_block_editor_options()
        .await
        .map(|options| BlockEditorOptions {
            collections: map_block_editor_options(options.collections),
            shows: map_block_editor_options(options.shows),
            seasons: map_block_editor_options(options.seasons),
            multi_collections: map_block_editor_options(options.multi_collections),
            smart_collections: map_block_editor_options(options.smart_collections),
            watermarks: map_block_editor_options(options.watermarks),
            graphics_elements: map_block_editor_options(options.graphics_elements),
        })
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
fn map_block_editor_options(
    options: Vec<ersatzrs_core::repositories::BlockEditorOptionAdmin>,
) -> Vec<BlockEditorOption> {
    options
        .into_iter()
        .map(|option| BlockEditorOption {
            id: option.id,
            name: option.name,
        })
        .collect()
}

#[cfg(all(test, feature = "ssr"))]
pub async fn load_block_editor_options_with_pool(
    pool: &DbPool,
) -> Result<BlockEditorOptions, ServerFnError> {
    Ok(BlockEditorOptions {
        collections: load_block_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "Collection" ORDER BY "Name""#,
        )
        .await?,
        shows: load_block_options(
            pool,
            r#"SELECT sh."Id", COALESCE(sm."Title", '') AS "Name"
               FROM "Show" sh
               LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
               ORDER BY sm."Title", sh."Id""#,
        )
        .await?,
        seasons: load_block_options(
            pool,
            r#"SELECT s."Id",
                      COALESCE(sm."Title", 'Season ' || s."SeasonNumber") || ' S' || printf('%02d', s."SeasonNumber") AS "Name"
               FROM "Season" s
               LEFT JOIN "Show" sh ON sh."Id" = s."ShowId"
               LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
               ORDER BY sm."Title", s."SeasonNumber", s."Id""#,
        )
        .await?,
        multi_collections: load_block_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "MultiCollection" ORDER BY "Name""#,
        )
        .await?,
        smart_collections: load_block_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "SmartCollection" ORDER BY "Name""#,
        )
        .await?,
        watermarks: load_block_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "ChannelWatermark" ORDER BY "Name""#,
        )
        .await?,
        graphics_elements: load_block_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", "Path", '') AS "Name" FROM "GraphicsElement" ORDER BY "Name", "Id""#,
        )
        .await?,
    })
}

/// Human-readable label for a `PlaybackOrder` integer.
#[must_use]
pub fn playback_order_label(order: i32, collection_type: i32) -> &'static str {
    match order {
        2 => "Random",
        3 => "Shuffle",
        4 => "Shuffle In Order",
        6 if collection_type == collection_type::TELEVISION_SHOW => "Season, Episode",
        7 => "Random Rotation",
        _ => "Chronological",
    }
}

/// Human-readable label for a `BlockStopScheduling` integer.
#[must_use]
pub fn stop_scheduling_label(v: i32) -> &'static str {
    if v == 1 {
        "Before Duration End"
    } else {
        "After Duration End"
    }
}

fn stop_scheduling_value(value: ersatzrs_domain::BlockStopScheduling) -> i32 {
    match value {
        ersatzrs_domain::BlockStopScheduling::AfterDurationEnd => 0,
        ersatzrs_domain::BlockStopScheduling::BeforeDurationEnd => 1,
    }
}

/// Duration in `Xh Ym` form for display.
#[must_use]
pub fn format_duration_label(minutes: i32) -> String {
    let h = minutes / 60;
    let m = minutes % 60;
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "ssr")]
    async fn migrated_test_db() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect test database");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        pool
    }

    #[test]
    fn block_duration_validation_matches_ersatztv_bounds() {
        assert!(validate_block_duration(5).is_ok());
        assert!(validate_block_duration(24 * 60).is_ok());

        assert!(validate_block_duration(0).is_err());
        assert!(validate_block_duration(7).is_err());
        assert!(validate_block_duration(24 * 60 + 5).is_err());
    }

    #[test]
    fn block_item_validation_requires_matching_target() {
        let mut item = BlockItemForm {
            collection_type: collection_type::COLLECTION,
            ..BlockItemForm::default()
        };
        assert!(validate_block_item(&item)
            .unwrap_err()
            .to_string()
            .contains("Collection"));

        item.collection_id = Some(10);
        assert!(validate_block_item(&item).is_ok());

        item.collection_type = collection_type::MULTI_COLLECTION;
        item.collection_id = None;
        assert!(validate_block_item(&item)
            .unwrap_err()
            .to_string()
            .contains("MultiCollection"));
        item.multi_collection_id = Some(20);
        assert!(validate_block_item(&item).is_ok());

        item.collection_type = collection_type::SEARCH_QUERY;
        item.multi_collection_id = None;
        item.search_query = Some(String::new());
        assert!(validate_block_item(&item)
            .unwrap_err()
            .to_string()
            .contains("SearchQuery"));
        item.search_query = Some("genre:comedy".to_string());
        assert!(validate_block_item(&item).is_ok());
    }

    #[test]
    fn block_labels_cover_copy_and_multi_collection_options() {
        assert_eq!(
            playback_order_label(
                playback_order::SHUFFLE_IN_ORDER,
                collection_type::MULTI_COLLECTION
            ),
            "Shuffle In Order"
        );
        assert_eq!(
            playback_order_label(playback_order::RANDOM_ROTATION, collection_type::COLLECTION),
            "Random Rotation"
        );
        assert_eq!(format_duration_label(30), "30m");
    }

    #[test]
    fn preview_transport_maps_every_real_block_collection_kind() {
        let mut items = vec![
            BlockItemForm {
                collection_type: collection_type::COLLECTION,
                collection_id: Some(1),
                ..BlockItemForm::default()
            },
            BlockItemForm {
                collection_type: collection_type::TELEVISION_SHOW,
                media_item_id: Some(2),
                ..BlockItemForm::default()
            },
            BlockItemForm {
                collection_type: collection_type::TELEVISION_SEASON,
                media_item_id: Some(3),
                ..BlockItemForm::default()
            },
            BlockItemForm {
                collection_type: collection_type::ARTIST,
                media_item_id: Some(4),
                ..BlockItemForm::default()
            },
            BlockItemForm {
                collection_type: collection_type::MULTI_COLLECTION,
                multi_collection_id: Some(5),
                ..BlockItemForm::default()
            },
            BlockItemForm {
                collection_type: collection_type::SMART_COLLECTION,
                smart_collection_id: Some(6),
                ..BlockItemForm::default()
            },
            BlockItemForm {
                collection_type: collection_type::PLAYLIST,
                playlist_id: Some(7),
                ..BlockItemForm::default()
            },
            BlockItemForm {
                collection_type: collection_type::SEARCH_QUERY,
                search_query: Some("title:pilot".to_owned()),
                ..BlockItemForm::default()
            },
        ];
        for kind in [
            collection_type::MOVIE,
            collection_type::EPISODE,
            collection_type::MUSIC_VIDEO,
            collection_type::OTHER_VIDEO,
            collection_type::SONG,
            collection_type::IMAGE,
            collection_type::REMOTE_STREAM,
        ] {
            items.push(BlockItemForm {
                collection_type: kind,
                media_item_id: Some(i64::from(kind)),
                ..BlockItemForm::default()
            });
        }
        for (index, item) in items.iter_mut().enumerate() {
            item.index = i32::try_from(index).unwrap_or_default();
            item.playback_order = playback_order::CHRONOLOGICAL;
        }

        let query = block_preview_query(BlockWithItemsForm {
            name: "Unsaved".to_owned(),
            minutes: 60,
            stop_scheduling: stop_scheduling::BEFORE_DURATION_END,
            items,
            ..BlockWithItemsForm::default()
        })
        .expect("transport adaptation succeeds");

        assert_eq!(
            query
                .items
                .iter()
                .map(|item| item.collection_key.collection_type)
                .collect::<Vec<_>>(),
            vec![
                ersatzrs_domain::CollectionType::Collection,
                ersatzrs_domain::CollectionType::TelevisionShow,
                ersatzrs_domain::CollectionType::TelevisionSeason,
                ersatzrs_domain::CollectionType::Artist,
                ersatzrs_domain::CollectionType::MultiCollection,
                ersatzrs_domain::CollectionType::SmartCollection,
                ersatzrs_domain::CollectionType::Playlist,
                ersatzrs_domain::CollectionType::SearchQuery,
                ersatzrs_domain::CollectionType::Movie,
                ersatzrs_domain::CollectionType::Episode,
                ersatzrs_domain::CollectionType::MusicVideo,
                ersatzrs_domain::CollectionType::OtherVideo,
                ersatzrs_domain::CollectionType::Song,
                ersatzrs_domain::CollectionType::Image,
                ersatzrs_domain::CollectionType::RemoteStream,
            ]
        );
        assert_eq!(
            query.stop_scheduling,
            ersatzrs_domain::BlockStopScheduling::BeforeDurationEnd
        );
    }

    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn saving_block_item_off_persists_disabled_legacy_override() {
        let pool = migrated_test_db().await;
        sqlx::query(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'Group')"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "Block" (
                "Id", "BlockGroupId", "DateUpdated", "Minutes", "Name", "StopScheduling"
            ) VALUES (1, 1, CURRENT_TIMESTAMP, 15, 'Block', 0)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "BlockItem" (
                "Id", "Index", "BlockId", "CollectionType", "SearchQuery", "PlaybackOrder"
            ) VALUES (10, 0, 1, 9, 'genre:comedy', 2)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let legacy_policy = SchedulingSmartShufflePolicy::for_preset(
            ersatzrs_core::scheduling::SchedulingSmartShufflePreset::Balanced,
        );
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(legacy_playout_cooldown_config_key(100))
            .bind(serde_json::to_string(&legacy_policy).unwrap())
            .execute(&pool)
            .await
            .unwrap();

        save_block_with_pool(
            &pool,
            BlockWithItemsForm {
                id: 1,
                group_id: 1,
                name: "Block".to_owned(),
                minutes: 15,
                stop_scheduling: 0,
                items: vec![BlockItemForm {
                    index: 0,
                    collection_type: collection_type::SEARCH_QUERY,
                    search_query: Some("genre:comedy".to_owned()),
                    playback_order: playback_order::RANDOM,
                    smart_shuffle: SmartShuffleForm::default(),
                    ..BlockItemForm::default()
                }],
            },
        )
        .await
        .expect("save block with explicit Off policy");

        let new_item_id: i64 =
            sqlx::query_scalar(r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_ne!(new_item_id, 10);
        let value: String =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(block_item_smart_shuffle_config_key(new_item_id))
                .fetch_one(&pool)
                .await
                .expect("disabled item attachment persists");
        let policy: SchedulingSmartShufflePolicy = serde_json::from_str(&value).unwrap();
        assert_eq!(policy, SchedulingSmartShufflePolicy::default());
        let legacy_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(legacy_playout_cooldown_config_key(100))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(legacy_count, 1);
    }

    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn saving_inherited_block_item_off_preserves_missing_attachment() {
        let pool = migrated_test_db().await;
        sqlx::query(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'Group')"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "Block" (
                "Id", "BlockGroupId", "DateUpdated", "Minutes", "Name", "StopScheduling"
            ) VALUES (1, 1, CURRENT_TIMESTAMP, 15, 'Block', 0)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "BlockItem" (
                "Id", "Index", "BlockId", "CollectionType", "SearchQuery", "PlaybackOrder"
            ) VALUES (10, 0, 1, 9, 'genre:comedy', 2)"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        save_block_with_pool(
            &pool,
            BlockWithItemsForm {
                id: 1,
                group_id: 1,
                name: "Block".to_owned(),
                minutes: 15,
                stop_scheduling: 0,
                items: vec![BlockItemForm {
                    index: 0,
                    collection_type: collection_type::SEARCH_QUERY,
                    search_query: Some("genre:comedy".to_owned()),
                    playback_order: playback_order::RANDOM,
                    smart_shuffle: SmartShuffleForm {
                        inherited_legacy: true,
                        ..SmartShuffleForm::default()
                    },
                    ..BlockItemForm::default()
                }],
            },
        )
        .await
        .expect("save inherited Block item");

        let new_item_id: i64 =
            sqlx::query_scalar(r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_ne!(new_item_id, 10);
        let attachment_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(block_item_smart_shuffle_config_key(new_item_id))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attachment_count, 0);
    }

    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn loading_block_items_distinguishes_missing_and_explicit_attachments() {
        let pool = migrated_test_db().await;
        sqlx::query(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'Group')"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "Block" (
                "Id", "BlockGroupId", "DateUpdated", "Minutes", "Name", "StopScheduling"
            ) VALUES (1, 1, CURRENT_TIMESTAMP, 15, 'Block', 0)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, index) in [(10_i64, 0_i32), (11, 1), (12, 2)] {
            sqlx::query(
                r#"INSERT INTO "BlockItem" (
                    "Id", "Index", "BlockId", "CollectionType", "SearchQuery", "PlaybackOrder"
                ) VALUES (?, ?, 1, 9, 'genre:comedy', 2)"#,
            )
            .bind(id)
            .bind(index)
            .execute(&pool)
            .await
            .unwrap();
        }
        let disabled = SchedulingSmartShufflePolicy::default();
        let active = SchedulingSmartShufflePolicy::for_preset(
            ersatzrs_core::scheduling::SchedulingSmartShufflePreset::Balanced,
        );
        for (id, policy) in [(11_i64, disabled), (12_i64, active)] {
            sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
                .bind(block_item_smart_shuffle_config_key(id))
                .bind(serde_json::to_string(&policy).unwrap())
                .execute(&pool)
                .await
                .unwrap();
        }

        let items = get_block_items_with_pool(&pool, 1).await.unwrap();

        assert!(items[0].smart_shuffle.inherited_legacy);
        assert_eq!(items[0].smart_shuffle.preset, "off");
        assert!(!items[1].smart_shuffle.inherited_legacy);
        assert_eq!(items[1].smart_shuffle.preset, "off");
        assert!(!items[2].smart_shuffle.inherited_legacy);
        assert_eq!(items[2].smart_shuffle.preset, "balanced");
    }

    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn copying_block_preserves_attachment_presence() {
        let pool = migrated_test_db().await;
        sqlx::query(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'Group')"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "Block" (
                "Id", "BlockGroupId", "DateUpdated", "Minutes", "Name", "StopScheduling"
            ) VALUES (1, 1, CURRENT_TIMESTAMP, 15, 'Original', 0)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, index) in [(10_i64, 0_i32), (11, 1)] {
            sqlx::query(
                r#"INSERT INTO "BlockItem" (
                    "Id", "Index", "BlockId", "CollectionType", "SearchQuery", "PlaybackOrder"
                ) VALUES (?, ?, 1, 9, 'genre:comedy', 2)"#,
            )
            .bind(id)
            .bind(index)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(block_item_smart_shuffle_config_key(11))
            .bind(serde_json::to_string(&SchedulingSmartShufflePolicy::default()).unwrap())
            .execute(&pool)
            .await
            .unwrap();

        let copied_block_id = copy_block_with_pool(&pool, 1, 1, "Copy").await.unwrap();
        let copied_item_ids: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = ? ORDER BY "Index""#,
        )
        .bind(copied_block_id)
        .fetch_all(&pool)
        .await
        .unwrap();

        let missing_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(block_item_smart_shuffle_config_key(copied_item_ids[0]))
                .fetch_one(&pool)
                .await
                .unwrap();
        let explicit_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(block_item_smart_shuffle_config_key(copied_item_ids[1]))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(missing_count, 0);
        assert_eq!(explicit_count, 1);
    }
}
