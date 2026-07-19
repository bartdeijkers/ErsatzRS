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
#[cfg(feature = "ssr")]
use ersatzrs_infra::db::{DatabaseBackend, DbPool};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::schedules::{SmartShuffleForm, SmartShuffleOverview};

#[cfg(feature = "ssr")]
use super::schedules::ScheduleItemForm;

#[cfg(feature = "ssr")]
use super::schedules::{
    persist_smart_shuffle_attachment_tx, smart_shuffle_form_from_policy,
    smart_shuffle_overview_with_pool, smart_shuffle_policy_from_form, validate_smart_shuffle_form,
};

#[cfg(feature = "ssr")]
use ersatzrs_core::scheduling::block_item_smart_shuffle_config_key;

#[cfg(all(test, feature = "ssr", not(feature = "mysql")))]
use ersatzrs_core::scheduling::{legacy_playout_cooldown_config_key, SchedulingSmartShufflePolicy};

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

/// `CollectionType` discriminants (mirrors `ErsatzTV.Core.Domain.CollectionType`).
pub mod collection_type {
    pub const COLLECTION: i32 = 0;
    pub const TELEVISION_SHOW: i32 = 1;
    pub const TELEVISION_SEASON: i32 = 2;
    pub const MULTI_COLLECTION: i32 = 4;
    pub const SMART_COLLECTION: i32 = 5;
    pub const SEARCH_QUERY: i32 = 9;
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
    pub items: Vec<BlockItemForm>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlayoutPreviewRow {
    pub start: String,
    pub finish: String,
    pub title: String,
    pub duration: String,
}

#[server]
pub async fn preview_block_smart_shuffle_overview(
    items: Vec<BlockItemForm>,
    selected_index: usize,
    query: String,
    tag: String,
    page: u32,
) -> Result<SmartShuffleOverview, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let items = items
        .into_iter()
        .map(|item| ScheduleItemForm {
            id: item.id,
            index: item.index,
            collection_type: item.collection_type,
            collection_id: item.collection_id,
            media_item_id: item.media_item_id,
            multi_collection_id: item.multi_collection_id,
            smart_collection_id: item.smart_collection_id,
            search_title: item.search_title,
            search_query: item.search_query,
            collection_name: item.collection_name,
            playback_order: item.playback_order,
            smart_shuffle: item.smart_shuffle,
            ..ScheduleItemForm::default()
        })
        .collect();
    smart_shuffle_overview_with_pool(&state.pool, items, selected_index, &query, &tag, page).await
}

// ---------------------------------------------------------------------------
// Block groups
// ---------------------------------------------------------------------------

#[server]
pub async fn list_block_groups() -> Result<Vec<BlockGroupRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "BlockGroup" ORDER BY "Name""#,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| BlockGroupRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn create_block_group(name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let name = validate_block_group_name(&state.pool, &name).await?;

    let result = sqlx::query(r#"INSERT INTO "BlockGroup" ("Name") VALUES (?)"#)
        .bind(&name)
        .execute(&state.pool)
        .await?;
    let id = ersatzrs_infra::db::last_insert_id(result);

    Ok(id)
}

#[server]
pub async fn delete_block_group(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "BlockGroup" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Blocks list
// ---------------------------------------------------------------------------

#[server]
pub async fn list_blocks(search: String) -> Result<Vec<BlockRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let like = format!("%{}%", search.trim());

    let rows = sqlx::query(
        r#"SELECT b."Id",
                  b."BlockGroupId",
                  COALESCE(bg."Name", '') AS "GroupName",
                  COALESCE(b."Name", '') AS "Name",
                  b."Minutes",
                  b."StopScheduling"
           FROM "Block" b
           JOIN "BlockGroup" bg ON bg."Id" = b."BlockGroupId"
           WHERE (? = '' OR b."Name" LIKE ?)
           UNION ALL
           SELECT -1 AS "Id",
                  bg."Id" AS "BlockGroupId",
                  COALESCE(bg."Name", '') AS "GroupName",
                  '' AS "Name",
                  0 AS "Minutes",
                  0 AS "StopScheduling"
           FROM "BlockGroup" bg
           WHERE NOT EXISTS (SELECT 1 FROM "Block" b WHERE b."BlockGroupId" = bg."Id")
             AND ? = ''
           ORDER BY "GroupName", "Name""#,
    )
    .bind(search.trim())
    .bind(&like)
    .bind(search.trim())
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| BlockRow {
            id: r.try_get("Id").unwrap_or(0),
            group_id: r.try_get("BlockGroupId").unwrap_or(0),
            group_name: r.try_get("GroupName").unwrap_or_default(),
            name: r.try_get("Name").unwrap_or_default(),
            minutes: r.try_get("Minutes").unwrap_or(15),
            stop_scheduling: r.try_get("StopScheduling").unwrap_or(0),
        })
        .collect())
}

#[server]
pub async fn create_block(group_id: i64, name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let name = validate_block_name(&state.pool, group_id, None, &name).await?;

    let result = sqlx::query(
        r#"INSERT INTO "Block" ("BlockGroupId", "Name", "Minutes", "StopScheduling", "DateUpdated")
           VALUES (?, ?, 30, 0, CURRENT_TIMESTAMP)"#,
    )
    .bind(group_id)
    .bind(&name)
    .execute(&state.pool)
    .await?;
    let id = ersatzrs_infra::db::last_insert_id(result);

    Ok(id)
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
    copy_block_with_pool(&state.pool, block_id, new_group_id, &new_name).await
}

#[cfg(feature = "ssr")]
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
    let item_ids: Vec<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "BlockItem" WHERE "BlockId" = ?"#)
            .bind(id)
            .fetch_all(&state.pool)
            .await?;
    for item_id in item_ids {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(block_item_smart_shuffle_config_key(item_id))
            .execute(&state.pool)
            .await?;
    }
    sqlx::query(r#"DELETE FROM "Block" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Block editor
// ---------------------------------------------------------------------------

fn parse_id_list(s: Option<String>) -> Vec<i64> {
    s.unwrap_or_default()
        .split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<i64>().ok())
        .collect()
}

#[cfg(feature = "ssr")]
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

#[cfg(feature = "ssr")]
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

fn validate_block_duration(minutes: i32) -> Result<(), ServerFnError> {
    if minutes > 0 && minutes % 5 == 0 && minutes <= 24 * 60 {
        Ok(())
    } else {
        Err(ServerFnError::new(
            "Block duration must be between 5 minutes and 24 hours",
        ))
    }
}

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
        collection_type::SEARCH_QUERY if item.search_query.as_deref().is_none_or(str::is_empty) => {
            Err(ServerFnError::new(
                "[SearchQuery] is required for collection type 'SearchQuery'",
            ))
        }
        collection_type::COLLECTION
        | collection_type::TELEVISION_SHOW
        | collection_type::TELEVISION_SEASON
        | collection_type::MULTI_COLLECTION
        | collection_type::SMART_COLLECTION
        | collection_type::SEARCH_QUERY => Ok(()),
        _ => Err(ServerFnError::new("[CollectionType] is invalid")),
    }
}

#[cfg(feature = "ssr")]
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
        validate_block_item(item)?;
        validate_smart_shuffle_form(&item.smart_shuffle)?;
    }
    Ok(())
}

#[server]
pub async fn get_block(id: i64) -> Result<Option<BlockForm>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
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
    .fetch_optional(&state.pool)
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
    get_block_items_with_pool(&state.pool, block_id).await
}

#[cfg(feature = "ssr")]
async fn get_block_items_with_pool(
    pool: &DbPool,
    block_id: i64,
) -> Result<Vec<BlockItemForm>, ServerFnError> {
    use sqlx::Row;

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
                   WHEN 4 THEN COALESCE(mc."Name", '')
                   WHEN 5 THEN COALESCE(sc."Name", '')
                   WHEN 9 THEN COALESCE(bi."SearchTitle", bi."SearchQuery", '')
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
           WHERE bi."BlockId" = ?
           ORDER BY bi."Index""#,
    )
    .bind(block_id)
    .fetch_all(pool)
    .await?;

    let mut items = rows
        .iter()
        .map(|r| BlockItemForm {
            id: r.try_get("Id").unwrap_or(0),
            index: r.try_get("Index").unwrap_or(0),
            collection_type: r.try_get("CollectionType").unwrap_or(0),
            collection_id: r.try_get("CollectionId").unwrap_or(None),
            multi_collection_id: r.try_get("MultiCollectionId").unwrap_or(None),
            smart_collection_id: r.try_get("SmartCollectionId").unwrap_or(None),
            media_item_id: r.try_get("MediaItemId").unwrap_or(None),
            search_title: r.try_get("SearchTitle").unwrap_or(None),
            search_query: r.try_get("SearchQuery").unwrap_or(None),
            playback_order: r.try_get("PlaybackOrder").unwrap_or(1),
            smart_shuffle: SmartShuffleForm::default(),
            include_in_program_guide: r
                .try_get::<bool, _>("IncludeInProgramGuide")
                .unwrap_or(false),
            disable_watermarks: r.try_get::<bool, _>("DisableWatermarks").unwrap_or(false),
            collection_name: if r.try_get::<i32, _>("CollectionType").unwrap_or(0)
                == collection_type::TELEVISION_SEASON
            {
                season_label(
                    &r.try_get::<String, _>("CollectionName").unwrap_or_default(),
                    r.try_get("SeasonNumber").unwrap_or(0),
                )
            } else {
                r.try_get("CollectionName").unwrap_or_default()
            },
            watermark_ids: parse_id_list(r.try_get("WatermarkIds").unwrap_or(None)),
            graphics_element_ids: parse_id_list(r.try_get("GraphicsElementIds").unwrap_or(None)),
        })
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

/// Replace the block metadata and all its items atomically.
#[server]
pub async fn save_block_with_items(form: BlockWithItemsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_block_with_pool(&state.pool, form).await
}

#[cfg(feature = "ssr")]
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
// Preview playout (simplified: list media items from each block item's
// collection in playback order, accumulating start times from MediaVersion
// durations).
// ---------------------------------------------------------------------------

#[server]
pub async fn preview_block_playout(
    form: BlockWithItemsForm,
) -> Result<Vec<PlayoutPreviewRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();

    let mut rows: Vec<PlayoutPreviewRow> = Vec::new();
    let mut elapsed_secs: i64 = 0;

    for item in &form.items {
        let media_rows = query_items_for_preview(item, &state.pool, state.database_backend).await;
        for (title, dur_secs) in media_rows {
            let start = format_hms(elapsed_secs);
            elapsed_secs += dur_secs;
            let finish = format_hms(elapsed_secs);
            let duration = format_hms(dur_secs);
            rows.push(PlayoutPreviewRow {
                start,
                finish,
                title,
                duration,
            });
        }
    }

    Ok(rows)
}

fn format_hms(secs: i64) -> String {
    let s = secs.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn parse_hms(s: &str) -> i64 {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() < 3 {
        return 0;
    }
    let h: i64 = parts[0].parse().unwrap_or(0);
    let m: i64 = parts[1].parse().unwrap_or(0);
    let sec: i64 = parts[2].parse().unwrap_or(0);
    h * 3600 + m * 60 + sec
}

fn duration_secs(duration: Option<&str>) -> i64 {
    duration.map_or(0, parse_hms)
}

#[allow(clippy::too_many_lines)]
#[cfg(feature = "ssr")]
async fn query_items_for_preview(
    item: &BlockItemForm,
    pool: &DbPool,
    database_backend: DatabaseBackend,
) -> Vec<(String, i64)> {
    use sqlx::Row;

    match item.collection_type {
        // Manual collection — list its media items with title + duration
        0 => {
            let Some(cid) = item.collection_id else {
                return vec![];
            };
            let order_clause = playback_order_clause(item.playback_order, false, database_backend);
            let sql = format!(
                r#"SELECT "Title", "Duration" FROM (
                       SELECT COALESCE(mm."Title", '') AS "Title",
                              mvv."Duration" AS "Duration"
                       FROM "CollectionItem" ci
                       JOIN "MediaItem" mi ON mi."Id" = ci."MediaItemId"
                       LEFT JOIN "Movie" mv_row ON mv_row."Id" = mi."Id"
                       LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = mv_row."Id"
                       LEFT JOIN "MediaVersion" mvv ON mvv."MovieId" = mv_row."Id"
                       WHERE ci."CollectionId" = ?
                       UNION ALL
                       SELECT COALESCE(sm."Title", '') AS "Title",
                              mvv."Duration" AS "Duration"
                       FROM "CollectionItem" ci
                       JOIN "MediaItem" mi ON mi."Id" = ci."MediaItemId"
                       LEFT JOIN "Episode" ep ON ep."Id" = mi."Id"
                       LEFT JOIN "EpisodeMetadata" sm ON sm."EpisodeId" = ep."Id"
                       LEFT JOIN "MediaVersion" mvv ON mvv."EpisodeId" = ep."Id"
                       WHERE ci."CollectionId" = ?
                   ) ORDER BY {order_clause}"#,
            );
            sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
                .bind(cid)
                .bind(cid)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let title: String = r.try_get("Title").unwrap_or_default();
                    let duration: Option<String> = r.try_get("Duration").unwrap_or(None);
                    let dur = duration_secs(duration.as_deref());
                    (title, dur)
                })
                .collect()
        }

        // Television show — list episodes
        1 => {
            let Some(show_id) = item.media_item_id else {
                return vec![];
            };
            let order_clause = playback_order_clause(item.playback_order, true, database_backend);
            let sql = format!(
                r#"SELECT COALESCE(sm."Title", '') AS "Title",
                          se."SeasonNumber" AS "SeasonNum",
                          em."EpisodeNumber" AS "EpNum",
                          mv."Duration" AS "Duration"
                   FROM "Episode" e
                   JOIN "Season" se ON se."Id" = e."SeasonId"
                   JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
                   LEFT JOIN "MediaVersion" mv ON mv."EpisodeId" = e."Id"
                   WHERE se."ShowId" = ?
                   ORDER BY {order_clause}"#,
            );
            sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
                .bind(show_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let title: String = r.try_get("Title").unwrap_or_default();
                    let duration: Option<String> = r.try_get("Duration").unwrap_or(None);
                    let dur = duration_secs(duration.as_deref());
                    (title, dur)
                })
                .collect()
        }

        // Television season — list episodes of the season
        2 => {
            let Some(season_id) = item.media_item_id else {
                return vec![];
            };
            let sql = r#"SELECT COALESCE(em."Title", '') AS "Title",
                                mv."Duration" AS "Duration"
                         FROM "Episode" e
                         JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
                         LEFT JOIN "MediaVersion" mv ON mv."EpisodeId" = e."Id"
                         WHERE e."SeasonId" = ?
                         ORDER BY em."EpisodeNumber""#;
            sqlx::query(ersatzrs_infra::db::assert_sql_safe(sql))
                .bind(season_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let title: String = r.try_get("Title").unwrap_or_default();
                    let duration: Option<String> = r.try_get("Duration").unwrap_or(None);
                    let dur = duration_secs(duration.as_deref());
                    (title, dur)
                })
                .collect()
        }

        // Multi collection — expand nested collection and smart collection items.
        4 => {
            let Some(mid) = item.multi_collection_id else {
                return vec![];
            };
            let rows = sqlx::query(
                r#"SELECT ci."MediaItemId", COALESCE(mm."Title", em."Title", '') AS "Title",
                          COALESCE(mvm."Duration", mve."Duration") AS "Duration"
                   FROM "MultiCollectionItem" mci
                   JOIN "CollectionItem" ci ON ci."CollectionId" = mci."CollectionId"
                   LEFT JOIN "Movie" mv ON mv."Id" = ci."MediaItemId"
                   LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = mv."Id"
                   LEFT JOIN "MediaVersion" mvm ON mvm."MovieId" = mv."Id"
                   LEFT JOIN "Episode" ep ON ep."Id" = ci."MediaItemId"
                   LEFT JOIN "EpisodeMetadata" em ON em."EpisodeId" = ep."Id"
                   LEFT JOIN "MediaVersion" mve ON mve."EpisodeId" = ep."Id"
                   WHERE mci."MultiCollectionId" = ?
                   ORDER BY "Title""#,
            )
            .bind(mid)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

            rows.iter()
                .map(|r| {
                    let title: String = r.try_get("Title").unwrap_or_default();
                    let duration: Option<String> = r.try_get("Duration").unwrap_or(None);
                    (title, duration_secs(duration.as_deref()))
                })
                .collect()
        }

        // Search query — name placeholder
        9 => {
            let label = item
                .search_title
                .as_deref()
                .or(item.search_query.as_deref())
                .unwrap_or("(search query)")
                .to_string();
            vec![(label, 0)]
        }

        _ => vec![],
    }
}

#[cfg(feature = "ssr")]
fn playback_order_clause(
    order: i32,
    is_episode: bool,
    database_backend: DatabaseBackend,
) -> &'static str {
    match order {
        6 if is_episode => r#""SeasonNum", "EpNum""#,
        2 | 3 => ersatzrs_infra::db::random_order_expression(database_backend),
        _ => r#""Title""#,
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
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(vec![]);
    }
    let like = format!("{prefix}%");

    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "Collection"
           WHERE "Name" LIKE ?
           ORDER BY "Name"
           LIMIT 10"#,
    )
    .bind(&like)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get("Id").unwrap_or(0),
                r.try_get("Name").unwrap_or_default(),
            )
        })
        .collect())
}

#[server]
pub async fn search_smart_collections_for_block(
    prefix: String,
) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(vec![]);
    }
    let like = format!("{prefix}%");

    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "SmartCollection"
           WHERE "Name" LIKE ?
           ORDER BY "Name"
           LIMIT 10"#,
    )
    .bind(&like)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get("Id").unwrap_or(0),
                r.try_get("Name").unwrap_or_default(),
            )
        })
        .collect())
}

#[server]
pub async fn search_multi_collections_for_block(
    prefix: String,
) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(vec![]);
    }
    let like = format!("{prefix}%");

    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "MultiCollection"
           WHERE "Name" LIKE ?
           ORDER BY "Name"
           LIMIT 10"#,
    )
    .bind(&like)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get("Id").unwrap_or(0),
                r.try_get("Name").unwrap_or_default(),
            )
        })
        .collect())
}

#[server]
pub async fn search_shows_for_block(prefix: String) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(vec![]);
    }
    let like = format!("{prefix}%");

    let rows = sqlx::query(
        r#"SELECT s."Id", COALESCE(sm."Title", '') AS "Title"
           FROM "Show" s
           JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id"
           WHERE sm."Title" LIKE ?
           ORDER BY sm."Title"
           LIMIT 10"#,
    )
    .bind(&like)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get("Id").unwrap_or(0),
                r.try_get("Title").unwrap_or_default(),
            )
        })
        .collect())
}

#[server]
pub async fn search_seasons_for_block(prefix: String) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(vec![]);
    }
    let like = format!("{prefix}%");

    let rows = sqlx::query(
        r#"SELECT se."Id",
                  COALESCE(sm."Title", '') AS "Title",
                  se."SeasonNumber"
           FROM "Season" se
           JOIN "Show" sh ON sh."Id" = se."ShowId"
           JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
           WHERE sm."Title" LIKE ?
           ORDER BY sm."Title", se."SeasonNumber"
           LIMIT 20"#,
    )
    .bind(&like)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            (
                r.try_get("Id").unwrap_or(0),
                season_label(
                    &r.try_get::<String, _>("Title").unwrap_or_default(),
                    r.try_get("SeasonNumber").unwrap_or(0),
                ),
            )
        })
        .collect())
}

fn season_label(title: &str, season_number: i64) -> String {
    format!("{title} S{season_number:02}")
}

#[cfg(feature = "ssr")]
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

#[cfg(feature = "ssr")]
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
    load_block_editor_options_with_pool(&state.pool).await
}

#[cfg(feature = "ssr")]
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

// suppress dead-code warning on parse_hms which is only used transitively
#[allow(dead_code)]
const _: fn(&str) -> i64 = parse_hms;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(feature = "ssr", not(feature = "mysql")))]
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

    #[cfg(all(feature = "ssr", not(feature = "mysql")))]
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

    #[cfg(all(feature = "ssr", not(feature = "mysql")))]
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

    #[cfg(all(feature = "ssr", not(feature = "mysql")))]
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

    #[cfg(all(feature = "ssr", not(feature = "mysql")))]
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
