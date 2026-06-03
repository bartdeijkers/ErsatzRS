//! Server functions for deco and deco-template management.
//! Ports:
//! - `Pages/Decos.razor` -> list, add, and delete actions.
//! - `Pages/DecoTemplates.razor` -> list, add, and delete actions.
//! - `Pages/DecoTemplateEditor.razor` -> template metadata, deco add/remove, and item save.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use ersatzrs_infra::db::DbPool;
#[cfg(feature = "ssr")]
use ersatzrs_infra::db::{Db, DbRow};

const DECO_MODE_OVERRIDE: i32 = 2;
const DECO_MODE_MERGE: i32 = 3;
const COLLECTION_TYPE_COLLECTION: i32 = 0;
const COLLECTION_TYPE_TELEVISION_SHOW: i32 = 1;
const COLLECTION_TYPE_TELEVISION_SEASON: i32 = 2;
const COLLECTION_TYPE_ARTIST: i32 = 3;
const COLLECTION_TYPE_MULTI_COLLECTION: i32 = 4;
const COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
const COLLECTION_TYPE_PLAYLIST: i32 = 6;
const PLACEMENT_BLOCK_START: i32 = 0;
const PLACEMENT_BLOCK_FINISH: i32 = 1;
const PLACEMENT_BETWEEN_BLOCK_ITEMS: i32 = 2;
const PLACEMENT_CHAPTER_MARKERS: i32 = 3;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoGroupRow {
    pub id: i64,
    pub name: String,
    pub deco_count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoRow {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoTemplateGroupRow {
    pub id: i64,
    pub name: String,
    pub template_count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoTemplateRow {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoTemplateForm {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoTemplateItemForm {
    pub deco_id: i64,
    #[serde(default)]
    pub deco_name: String,
    pub start_minutes: i32,
    pub end_minutes: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoTemplateWithItemsForm {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    #[serde(default)]
    pub items: Vec<DecoTemplateItemForm>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoPlaylistOption {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoEditorOptions {
    pub watermarks: Vec<DecoOption>,
    pub graphics_elements: Vec<DecoOption>,
    pub collections: Vec<DecoOption>,
    pub shows: Vec<DecoOption>,
    pub seasons: Vec<DecoOption>,
    pub artists: Vec<DecoOption>,
    pub multi_collections: Vec<DecoOption>,
    pub smart_collections: Vec<DecoOption>,
    pub playlist_groups: Vec<DecoOption>,
    pub playlists: Vec<DecoPlaylistOption>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoBreakContentForm {
    pub id: i64,
    pub collection_type: i32,
    pub collection_id: Option<i64>,
    pub media_item_id: Option<i64>,
    pub multi_collection_id: Option<i64>,
    pub smart_collection_id: Option<i64>,
    pub playlist_id: Option<i64>,
    #[serde(default)]
    pub collection_name: String,
    pub placement: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecoForm {
    pub id: i64,
    pub group_id: i64,
    #[serde(default)]
    pub group_name: String,
    pub name: String,
    pub watermark_mode: i32,
    #[serde(default)]
    pub watermark_ids: Vec<i64>,
    pub use_watermark_during_filler: bool,
    pub graphics_elements_mode: i32,
    #[serde(default)]
    pub graphics_element_ids: Vec<i64>,
    pub use_graphics_elements_during_filler: bool,
    pub break_content_mode: i32,
    #[serde(default)]
    pub break_content: Vec<DecoBreakContentForm>,
    pub default_filler_mode: i32,
    pub default_filler_collection_type: i32,
    pub default_filler_collection_id: Option<i64>,
    pub default_filler_media_item_id: Option<i64>,
    pub default_filler_multi_collection_id: Option<i64>,
    pub default_filler_smart_collection_id: Option<i64>,
    pub default_filler_trim_to_fit: bool,
    pub dead_air_fallback_mode: i32,
    pub dead_air_fallback_collection_type: i32,
    pub dead_air_fallback_collection_id: Option<i64>,
    pub dead_air_fallback_media_item_id: Option<i64>,
    pub dead_air_fallback_multi_collection_id: Option<i64>,
    pub dead_air_fallback_smart_collection_id: Option<i64>,
}

#[server]
pub async fn list_deco_groups() -> Result<Vec<DecoGroupRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_deco_groups_with_pool(&state.pool).await
}

#[cfg(feature = "ssr")]
pub async fn list_deco_groups_with_pool(pool: &DbPool) -> Result<Vec<DecoGroupRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT dg."Id",
                  COALESCE(dg."Name", '') AS "Name",
                  COUNT(d."Id") AS "DecoCount"
           FROM "DecoGroup" dg
           LEFT JOIN "Deco" d ON d."DecoGroupId" = dg."Id"
           GROUP BY dg."Id", dg."Name"
           ORDER BY dg."Name", dg."Id""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| DecoGroupRow {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
            deco_count: row.try_get("DecoCount").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn list_decos(search: String) -> Result<Vec<DecoRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_decos_with_pool(&state.pool, &search).await
}

#[cfg(feature = "ssr")]
pub async fn list_decos_with_pool(
    pool: &DbPool,
    search: &str,
) -> Result<Vec<DecoRow>, ServerFnError> {
    use sqlx::Row;

    let trimmed = search.trim();
    let like = format!("%{trimmed}%");
    let rows = sqlx::query(
        r#"SELECT d."Id",
                  d."DecoGroupId",
                  COALESCE(dg."Name", '') AS "GroupName",
                  COALESCE(d."Name", '') AS "Name"
           FROM "Deco" d
           JOIN "DecoGroup" dg ON dg."Id" = d."DecoGroupId"
           WHERE (? = '' OR d."Name" LIKE ?)
           UNION ALL
           SELECT -dg."Id" AS "Id",
                  dg."Id" AS "DecoGroupId",
                  COALESCE(dg."Name", '') AS "GroupName",
                  '(none)' AS "Name"
           FROM "DecoGroup" dg
           WHERE ? = ''
             AND NOT EXISTS (SELECT 1 FROM "Deco" d WHERE d."DecoGroupId" = dg."Id")
           ORDER BY "GroupName", "Name""#,
    )
    .bind(trimmed)
    .bind(&like)
    .bind(trimmed)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| DecoRow {
            id: row.try_get("Id").unwrap_or_default(),
            group_id: row.try_get("DecoGroupId").unwrap_or_default(),
            group_name: row.try_get("GroupName").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn create_deco_group(name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    create_deco_group_with_pool(&state.pool, &name).await
}

#[cfg(feature = "ssr")]
pub async fn create_deco_group_with_pool(pool: &DbPool, name: &str) -> Result<i64, ServerFnError> {
    let name = validate_deco_group_name(pool, name).await?;
    let result = sqlx::query(r#"INSERT INTO "DecoGroup" ("Name") VALUES (?)"#)
        .bind(&name)
        .execute(pool)
        .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

#[server]
pub async fn create_deco(group_id: i64, name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    create_deco_with_pool(&state.pool, group_id, &name).await
}

#[cfg(feature = "ssr")]
pub async fn create_deco_with_pool(
    pool: &DbPool,
    group_id: i64,
    name: &str,
) -> Result<i64, ServerFnError> {
    validate_deco_group_exists(pool, group_id).await?;
    let name = validate_deco_name(pool, group_id, None, name).await?;
    let result = sqlx::query(
        r#"INSERT INTO "Deco" (
               "DecoGroupId", "Name", "BreakContentMode",
               "DefaultFillerMode", "DefaultFillerCollectionType", "DefaultFillerTrimToFit",
               "DeadAirFallbackMode", "DeadAirFallbackCollectionType",
               "GraphicsElementsMode", "UseGraphicsElementsDuringFiller",
               "UseWatermarkDuringFiller", "WatermarkMode"
           ) VALUES (?, ?, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)"#,
    )
    .bind(group_id)
    .bind(&name)
    .execute(pool)
    .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

#[server]
pub async fn delete_deco_group(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    delete_deco_group_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn delete_deco_group_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let result = sqlx::query(r#"DELETE FROM "DecoGroup" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new(format!(
            "DecoGroup {id} does not exist."
        )));
    }
    Ok(())
}

#[server]
pub async fn delete_deco(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    delete_deco_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn delete_deco_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let result = sqlx::query(r#"DELETE FROM "Deco" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new(format!("Deco {id} does not exist.")));
    }
    Ok(())
}

#[server]
pub async fn get_deco(id: i64) -> Result<Option<DecoForm>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    get_deco_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn get_deco_with_pool(pool: &DbPool, id: i64) -> Result<Option<DecoForm>, ServerFnError> {
    use sqlx::Row;

    let Some(row) = sqlx::query(
        r#"SELECT d."Id",
                  d."DecoGroupId",
                  COALESCE(dg."Name", '') AS "GroupName",
                  COALESCE(d."Name", '') AS "Name",
                  d."WatermarkMode",
                  d."UseWatermarkDuringFiller",
                  d."GraphicsElementsMode",
                  d."UseGraphicsElementsDuringFiller",
                  d."BreakContentMode",
                  d."DefaultFillerMode",
                  d."DefaultFillerCollectionType",
                  d."DefaultFillerCollectionId",
                  d."DefaultFillerMediaItemId",
                  d."DefaultFillerMultiCollectionId",
                  d."DefaultFillerSmartCollectionId",
                  d."DefaultFillerTrimToFit",
                  d."DeadAirFallbackMode",
                  d."DeadAirFallbackCollectionType",
                  d."DeadAirFallbackCollectionId",
                  d."DeadAirFallbackMediaItemId",
                  d."DeadAirFallbackMultiCollectionId",
                  d."DeadAirFallbackSmartCollectionId"
           FROM "Deco" d
           JOIN "DecoGroup" dg ON dg."Id" = d."DecoGroupId"
           WHERE d."Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(DecoForm {
        id: row.try_get("Id").unwrap_or_default(),
        group_id: row.try_get("DecoGroupId").unwrap_or_default(),
        group_name: row.try_get("GroupName").unwrap_or_default(),
        name: row.try_get("Name").unwrap_or_default(),
        watermark_mode: row.try_get("WatermarkMode").unwrap_or_default(),
        watermark_ids: load_i64_ids(
            pool,
            r#"SELECT "WatermarkId" AS "Id" FROM "DecoWatermark" WHERE "DecoId" = ? ORDER BY "WatermarkId""#,
            id,
        )
        .await?,
        use_watermark_during_filler: read_bool(&row, "UseWatermarkDuringFiller"),
        graphics_elements_mode: row.try_get("GraphicsElementsMode").unwrap_or_default(),
        graphics_element_ids: load_i64_ids(
            pool,
            r#"SELECT "GraphicsElementId" AS "Id" FROM "DecoGraphicsElement" WHERE "DecoId" = ? ORDER BY "GraphicsElementId""#,
            id,
        )
        .await?,
        use_graphics_elements_during_filler: read_bool(&row, "UseGraphicsElementsDuringFiller"),
        break_content_mode: row.try_get("BreakContentMode").unwrap_or_default(),
        break_content: load_deco_break_content(pool, id).await?,
        default_filler_mode: row.try_get("DefaultFillerMode").unwrap_or_default(),
        default_filler_collection_type: row
            .try_get("DefaultFillerCollectionType")
            .unwrap_or_default(),
        default_filler_collection_id: row.try_get("DefaultFillerCollectionId").unwrap_or(None),
        default_filler_media_item_id: row.try_get("DefaultFillerMediaItemId").unwrap_or(None),
        default_filler_multi_collection_id: row
            .try_get("DefaultFillerMultiCollectionId")
            .unwrap_or(None),
        default_filler_smart_collection_id: row
            .try_get("DefaultFillerSmartCollectionId")
            .unwrap_or(None),
        default_filler_trim_to_fit: read_bool(&row, "DefaultFillerTrimToFit"),
        dead_air_fallback_mode: row.try_get("DeadAirFallbackMode").unwrap_or_default(),
        dead_air_fallback_collection_type: row
            .try_get("DeadAirFallbackCollectionType")
            .unwrap_or_default(),
        dead_air_fallback_collection_id: row
            .try_get("DeadAirFallbackCollectionId")
            .unwrap_or(None),
        dead_air_fallback_media_item_id: row
            .try_get("DeadAirFallbackMediaItemId")
            .unwrap_or(None),
        dead_air_fallback_multi_collection_id: row
            .try_get("DeadAirFallbackMultiCollectionId")
            .unwrap_or(None),
        dead_air_fallback_smart_collection_id: row
            .try_get("DeadAirFallbackSmartCollectionId")
            .unwrap_or(None),
    }))
}

#[server]
pub async fn load_deco_editor_options() -> Result<DecoEditorOptions, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    load_deco_editor_options_with_pool(&state.pool).await
}

#[cfg(feature = "ssr")]
pub async fn load_deco_editor_options_with_pool(
    pool: &DbPool,
) -> Result<DecoEditorOptions, ServerFnError> {
    Ok(DecoEditorOptions {
        watermarks: load_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "ChannelWatermark" ORDER BY "Name", "Id""#,
        )
        .await?,
        graphics_elements: load_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", "Path", '') AS "Name" FROM "GraphicsElement" ORDER BY "Name", "Id""#,
        )
        .await?,
        collections: load_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "Collection" ORDER BY "Name", "Id""#,
        )
        .await?,
        shows: load_options(
            pool,
            r#"SELECT sh."Id", COALESCE(sm."Title", '') AS "Name"
               FROM "Show" sh
               LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
               ORDER BY sm."Title", sh."Id""#,
        )
        .await?,
        seasons: load_options(
            pool,
            r#"SELECT s."Id",
                      COALESCE(sm."Title", 'Season ' || s."SeasonNumber") || ' S' || printf('%02d', s."SeasonNumber") AS "Name"
               FROM "Season" s
               LEFT JOIN "Show" sh ON sh."Id" = s."ShowId"
               LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
               ORDER BY sm."Title", s."SeasonNumber", s."Id""#,
        )
        .await?,
        artists: load_options(
            pool,
            r#"SELECT a."Id", COALESCE(am."Title", '') AS "Name"
               FROM "Artist" a
               LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
               ORDER BY am."Title", a."Id""#,
        )
        .await?,
        multi_collections: load_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "MultiCollection" ORDER BY "Name", "Id""#,
        )
        .await?,
        smart_collections: load_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "SmartCollection" ORDER BY "Name", "Id""#,
        )
        .await?,
        playlist_groups: load_options(
            pool,
            r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "PlaylistGroup" ORDER BY COALESCE("IsSystem", 0) DESC, "Name", "Id""#,
        )
        .await?,
        playlists: load_deco_playlist_options(pool).await?,
    })
}

#[server]
pub async fn save_deco(form: DecoForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    save_deco_with_pool(&state.pool, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_deco_with_pool(pool: &DbPool, mut form: DecoForm) -> Result<(), ServerFnError> {
    validate_deco_exists(pool, form.id).await?;
    validate_deco_group_exists(pool, form.group_id).await?;
    let name = validate_deco_name(pool, form.group_id, Some(form.id), &form.name).await?;
    normalize_deco_form(&mut form);
    validate_deco_references(pool, &form).await?;
    validate_break_content(&form.break_content)?;

    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"UPDATE "Deco"
           SET "Name" = ?,
               "WatermarkMode" = ?,
               "UseWatermarkDuringFiller" = ?,
               "GraphicsElementsMode" = ?,
               "UseGraphicsElementsDuringFiller" = ?,
               "BreakContentMode" = ?,
               "DefaultFillerMode" = ?,
               "DefaultFillerCollectionType" = ?,
               "DefaultFillerCollectionId" = ?,
               "DefaultFillerMediaItemId" = ?,
               "DefaultFillerMultiCollectionId" = ?,
               "DefaultFillerSmartCollectionId" = ?,
               "DefaultFillerTrimToFit" = ?,
               "DeadAirFallbackMode" = ?,
               "DeadAirFallbackCollectionType" = ?,
               "DeadAirFallbackCollectionId" = ?,
               "DeadAirFallbackMediaItemId" = ?,
               "DeadAirFallbackMultiCollectionId" = ?,
               "DeadAirFallbackSmartCollectionId" = ?
           WHERE "Id" = ?"#,
    )
    .bind(&name)
    .bind(form.watermark_mode)
    .bind(form.use_watermark_during_filler)
    .bind(form.graphics_elements_mode)
    .bind(form.use_graphics_elements_during_filler)
    .bind(form.break_content_mode)
    .bind(form.default_filler_mode)
    .bind(form.default_filler_collection_type)
    .bind(form.default_filler_collection_id)
    .bind(form.default_filler_media_item_id)
    .bind(form.default_filler_multi_collection_id)
    .bind(form.default_filler_smart_collection_id)
    .bind(form.default_filler_trim_to_fit)
    .bind(form.dead_air_fallback_mode)
    .bind(form.dead_air_fallback_collection_type)
    .bind(form.dead_air_fallback_collection_id)
    .bind(form.dead_air_fallback_media_item_id)
    .bind(form.dead_air_fallback_multi_collection_id)
    .bind(form.dead_air_fallback_smart_collection_id)
    .bind(form.id)
    .execute(&mut *tx)
    .await?;

    replace_deco_ids(
        &mut tx,
        r#"DELETE FROM "DecoWatermark" WHERE "DecoId" = ?"#,
        r#"INSERT INTO "DecoWatermark" ("DecoId", "WatermarkId") VALUES (?, ?)"#,
        form.id,
        &form.watermark_ids,
    )
    .await?;
    replace_deco_ids(
        &mut tx,
        r#"DELETE FROM "DecoGraphicsElement" WHERE "DecoId" = ?"#,
        r#"INSERT INTO "DecoGraphicsElement" ("DecoId", "GraphicsElementId") VALUES (?, ?)"#,
        form.id,
        &form.graphics_element_ids,
    )
    .await?;

    sqlx::query(r#"DELETE FROM "DecoBreakContent" WHERE "DecoId" = ?"#)
        .bind(form.id)
        .execute(&mut *tx)
        .await?;
    if form.break_content_mode == DECO_MODE_OVERRIDE {
        for item in &form.break_content {
            sqlx::query(
                r#"INSERT INTO "DecoBreakContent" (
                       "DecoId", "CollectionType", "CollectionId", "MediaItemId",
                       "MultiCollectionId", "SmartCollectionId", "PlaylistId", "Placement"
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(form.id)
            .bind(item.collection_type)
            .bind(item.collection_id)
            .bind(item.media_item_id)
            .bind(item.multi_collection_id)
            .bind(item.smart_collection_id)
            .bind(item.playlist_id)
            .bind(item.placement)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

#[server]
pub async fn list_deco_template_groups() -> Result<Vec<DecoTemplateGroupRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_deco_template_groups_with_pool(&state.pool).await
}

#[cfg(feature = "ssr")]
pub async fn list_deco_template_groups_with_pool(
    pool: &DbPool,
) -> Result<Vec<DecoTemplateGroupRow>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT dtg."Id",
                  COALESCE(dtg."Name", '') AS "Name",
                  COUNT(dt."Id") AS "TemplateCount"
           FROM "DecoTemplateGroup" dtg
           LEFT JOIN "DecoTemplate" dt ON dt."DecoTemplateGroupId" = dtg."Id"
           GROUP BY dtg."Id", dtg."Name"
           ORDER BY dtg."Name", dtg."Id""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| DecoTemplateGroupRow {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
            template_count: row.try_get("TemplateCount").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn list_deco_templates(search: String) -> Result<Vec<DecoTemplateRow>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    list_deco_templates_with_pool(&state.pool, &search).await
}

#[cfg(feature = "ssr")]
pub async fn list_deco_templates_with_pool(
    pool: &DbPool,
    search: &str,
) -> Result<Vec<DecoTemplateRow>, ServerFnError> {
    use sqlx::Row;

    let trimmed = search.trim();
    let like = format!("%{trimmed}%");
    let rows = sqlx::query(
        r#"SELECT dt."Id",
                  dt."DecoTemplateGroupId",
                  COALESCE(dtg."Name", '') AS "GroupName",
                  COALESCE(dt."Name", '') AS "Name"
           FROM "DecoTemplate" dt
           JOIN "DecoTemplateGroup" dtg ON dtg."Id" = dt."DecoTemplateGroupId"
           WHERE (? = '' OR dt."Name" LIKE ?)
           UNION ALL
           SELECT -dtg."Id" AS "Id",
                  dtg."Id" AS "DecoTemplateGroupId",
                  COALESCE(dtg."Name", '') AS "GroupName",
                  '(none)' AS "Name"
           FROM "DecoTemplateGroup" dtg
           WHERE ? = ''
             AND NOT EXISTS (
                 SELECT 1 FROM "DecoTemplate" dt WHERE dt."DecoTemplateGroupId" = dtg."Id"
             )
           ORDER BY "GroupName", "Name""#,
    )
    .bind(trimmed)
    .bind(&like)
    .bind(trimmed)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| DecoTemplateRow {
            id: row.try_get("Id").unwrap_or_default(),
            group_id: row.try_get("DecoTemplateGroupId").unwrap_or_default(),
            group_name: row.try_get("GroupName").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[server]
pub async fn create_deco_template_group(name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    create_deco_template_group_with_pool(&state.pool, &name).await
}

#[cfg(feature = "ssr")]
pub async fn create_deco_template_group_with_pool(
    pool: &DbPool,
    name: &str,
) -> Result<i64, ServerFnError> {
    let name = validate_deco_template_group_name(pool, name).await?;
    let result = sqlx::query(r#"INSERT INTO "DecoTemplateGroup" ("Name") VALUES (?)"#)
        .bind(&name)
        .execute(pool)
        .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

#[server]
pub async fn create_deco_template(group_id: i64, name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    create_deco_template_with_pool(&state.pool, group_id, &name).await
}

#[cfg(feature = "ssr")]
pub async fn create_deco_template_with_pool(
    pool: &DbPool,
    group_id: i64,
    name: &str,
) -> Result<i64, ServerFnError> {
    validate_deco_template_group_exists(pool, group_id).await?;
    let name = validate_deco_template_name(pool, group_id, None, name).await?;
    let result = sqlx::query(
        r#"INSERT INTO "DecoTemplate" ("DecoTemplateGroupId", "Name", "DateUpdated")
           VALUES (?, ?, CURRENT_TIMESTAMP)"#,
    )
    .bind(group_id)
    .bind(&name)
    .execute(pool)
    .await?;
    Ok(ersatzrs_infra::db::last_insert_id(result))
}

#[server]
pub async fn delete_deco_template_group(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    delete_deco_template_group_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn delete_deco_template_group_with_pool(
    pool: &DbPool,
    id: i64,
) -> Result<(), ServerFnError> {
    let result = sqlx::query(r#"DELETE FROM "DecoTemplateGroup" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new(format!(
            "DecoTemplateGroup {id} does not exist."
        )));
    }
    Ok(())
}

#[server]
pub async fn delete_deco_template(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    delete_deco_template_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn delete_deco_template_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let result = sqlx::query(r#"DELETE FROM "DecoTemplate" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new(format!(
            "DecoTemplate {id} does not exist."
        )));
    }
    Ok(())
}

#[server]
pub async fn get_deco_template(id: i64) -> Result<Option<DecoTemplateForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_deco_template_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn get_deco_template_with_pool(
    pool: &DbPool,
    id: i64,
) -> Result<Option<DecoTemplateForm>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT dt."Id",
                  dt."DecoTemplateGroupId",
                  COALESCE(dtg."Name", '') AS "GroupName",
                  COALESCE(dt."Name", '') AS "Name"
           FROM "DecoTemplate" dt
           JOIN "DecoTemplateGroup" dtg ON dtg."Id" = dt."DecoTemplateGroupId"
           WHERE dt."Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| DecoTemplateForm {
        id: row.try_get("Id").unwrap_or_default(),
        group_id: row.try_get("DecoTemplateGroupId").unwrap_or_default(),
        group_name: row.try_get("GroupName").unwrap_or_default(),
        name: row.try_get("Name").unwrap_or_default(),
    }))
}

#[server]
pub async fn get_deco_template_items(
    template_id: i64,
) -> Result<Vec<DecoTemplateItemForm>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    get_deco_template_items_with_pool(&state.pool, template_id).await
}

#[cfg(feature = "ssr")]
pub async fn get_deco_template_items_with_pool(
    pool: &DbPool,
    template_id: i64,
) -> Result<Vec<DecoTemplateItemForm>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT dti."DecoId",
                  dti."StartTime",
                  dti."EndTime",
                  COALESCE(d."Name", '') AS "DecoName"
           FROM "DecoTemplateItem" dti
           JOIN "Deco" d ON d."Id" = dti."DecoId"
           WHERE dti."DecoTemplateId" = ?
           ORDER BY dti."StartTime", dti."Id""#,
    )
    .bind(template_id)
    .fetch_all(pool)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let start =
            parse_time_to_minutes(&row.try_get::<String, _>("StartTime").unwrap_or_default())
                .unwrap_or_default();
        let end = parse_time_to_minutes(&row.try_get::<String, _>("EndTime").unwrap_or_default())
            .unwrap_or_default();
        if start < end || end == 0 {
            items.push(DecoTemplateItemForm {
                deco_id: row.try_get("DecoId").unwrap_or_default(),
                deco_name: row.try_get("DecoName").unwrap_or_default(),
                start_minutes: start,
                end_minutes: end,
            });
        }
    }
    Ok(items)
}

#[server]
pub async fn save_deco_template_with_items(
    form: DecoTemplateWithItemsForm,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    save_deco_template_with_pool(&state.pool, form).await
}

#[cfg(feature = "ssr")]
pub async fn save_deco_template_with_pool(
    pool: &DbPool,
    form: DecoTemplateWithItemsForm,
) -> Result<(), ServerFnError> {
    validate_deco_template_exists(pool, form.id).await?;
    validate_deco_template_group_exists(pool, form.group_id).await?;
    let name = validate_deco_template_name(pool, form.group_id, Some(form.id), &form.name).await?;
    validate_deco_template_items(pool, &form.items).await?;

    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"UPDATE "DecoTemplate"
           SET "Name" = ?, "DateUpdated" = CURRENT_TIMESTAMP
           WHERE "Id" = ?"#,
    )
    .bind(&name)
    .bind(form.id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(r#"DELETE FROM "DecoTemplateItem" WHERE "DecoTemplateId" = ?"#)
        .bind(form.id)
        .execute(&mut *tx)
        .await?;

    for item in &form.items {
        let end = if item.end_minutes >= 24 * 60 {
            0
        } else {
            item.end_minutes
        };
        sqlx::query(
            r#"INSERT INTO "DecoTemplateItem" ("DecoTemplateId", "DecoId", "StartTime", "EndTime")
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(form.id)
        .bind(item.deco_id)
        .bind(format_minutes(item.start_minutes))
        .bind(format_minutes(end))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_deco_group_name(pool: &DbPool, name: &str) -> Result<String, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Deco group name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(
            "Deco group name must be 50 characters or fewer",
        ));
    }
    let duplicate: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "DecoGroup" WHERE "Name" = ?"#)
        .bind(name)
        .fetch_one(pool)
        .await?;
    if duplicate > 0 {
        return Err(ServerFnError::new("Deco group name must be unique"));
    }
    Ok(name.to_string())
}

#[cfg(feature = "ssr")]
async fn validate_deco_group_exists(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "DecoGroup" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Deco group does not exist."));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_deco_name(
    pool: &DbPool,
    group_id: i64,
    deco_id: Option<i64>,
    name: &str,
) -> Result<String, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Deco name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(format!(
            "Deco name \"{name}\" is invalid"
        )));
    }
    let duplicate: i64 = match deco_id {
        Some(id) => {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Deco"
                   WHERE "DecoGroupId" = ? AND "Name" = ? AND "Id" != ?"#,
            )
            .bind(group_id)
            .bind(name)
            .bind(id)
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Deco"
                   WHERE "DecoGroupId" = ? AND "Name" = ?"#,
            )
            .bind(group_id)
            .bind(name)
            .fetch_one(pool)
            .await?
        }
    };
    if duplicate > 0 {
        return Err(ServerFnError::new(format!(
            "A deco named \"{name}\" already exists in that deco group"
        )));
    }
    Ok(name.to_string())
}

#[cfg(feature = "ssr")]
async fn validate_deco_exists(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Deco" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Deco does not exist"));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_deco_references(pool: &DbPool, form: &DecoForm) -> Result<(), ServerFnError> {
    if form.watermark_mode == DECO_MODE_OVERRIDE || form.watermark_mode == DECO_MODE_MERGE {
        validate_ids_exist(
            pool,
            r#""ChannelWatermark""#,
            &form.watermark_ids,
            "Watermark",
        )
        .await?;
    }
    if form.graphics_elements_mode == DECO_MODE_OVERRIDE
        || form.graphics_elements_mode == DECO_MODE_MERGE
    {
        validate_ids_exist(
            pool,
            r#""GraphicsElement""#,
            &form.graphics_element_ids,
            "Graphics element",
        )
        .await?;
    }

    if form.default_filler_mode == DECO_MODE_OVERRIDE {
        validate_collection_selection(
            pool,
            form.default_filler_collection_type,
            form.default_filler_collection_id,
            form.default_filler_media_item_id,
            form.default_filler_multi_collection_id,
            form.default_filler_smart_collection_id,
            "Default filler",
        )
        .await?;
    }
    if form.dead_air_fallback_mode == DECO_MODE_OVERRIDE {
        validate_collection_selection(
            pool,
            form.dead_air_fallback_collection_type,
            form.dead_air_fallback_collection_id,
            form.dead_air_fallback_media_item_id,
            form.dead_air_fallback_multi_collection_id,
            form.dead_air_fallback_smart_collection_id,
            "Dead air fallback",
        )
        .await?;
    }

    if form.break_content_mode == DECO_MODE_OVERRIDE {
        for item in &form.break_content {
            validate_break_content_reference(pool, item).await?;
        }
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_collection_selection(
    pool: &DbPool,
    collection_type: i32,
    collection_id: Option<i64>,
    media_item_id: Option<i64>,
    multi_collection_id: Option<i64>,
    smart_collection_id: Option<i64>,
    label: &str,
) -> Result<(), ServerFnError> {
    match collection_type {
        COLLECTION_TYPE_COLLECTION => {
            validate_optional_reference(pool, r#""Collection""#, collection_id, label).await
        }
        COLLECTION_TYPE_MULTI_COLLECTION => {
            validate_optional_reference(pool, r#""MultiCollection""#, multi_collection_id, label)
                .await
        }
        COLLECTION_TYPE_SMART_COLLECTION => {
            validate_optional_reference(pool, r#""SmartCollection""#, smart_collection_id, label)
                .await
        }
        COLLECTION_TYPE_TELEVISION_SHOW
        | COLLECTION_TYPE_TELEVISION_SEASON
        | COLLECTION_TYPE_ARTIST => {
            validate_optional_reference(pool, r#""MediaItem""#, media_item_id, label).await
        }
        _ => Err(ServerFnError::new(format!(
            "{label} has unsupported collection type."
        ))),
    }
}

#[cfg(feature = "ssr")]
async fn validate_break_content_reference(
    pool: &DbPool,
    item: &DecoBreakContentForm,
) -> Result<(), ServerFnError> {
    match item.collection_type {
        COLLECTION_TYPE_COLLECTION => {
            validate_optional_reference(
                pool,
                r#""Collection""#,
                item.collection_id,
                "Break content collection",
            )
            .await
        }
        COLLECTION_TYPE_MULTI_COLLECTION => {
            validate_optional_reference(
                pool,
                r#""MultiCollection""#,
                item.multi_collection_id,
                "Break content multi collection",
            )
            .await
        }
        COLLECTION_TYPE_SMART_COLLECTION => {
            validate_optional_reference(
                pool,
                r#""SmartCollection""#,
                item.smart_collection_id,
                "Break content smart collection",
            )
            .await
        }
        COLLECTION_TYPE_TELEVISION_SHOW
        | COLLECTION_TYPE_TELEVISION_SEASON
        | COLLECTION_TYPE_ARTIST => {
            validate_optional_reference(
                pool,
                r#""MediaItem""#,
                item.media_item_id,
                "Break content media item",
            )
            .await
        }
        COLLECTION_TYPE_PLAYLIST => {
            validate_optional_reference(
                pool,
                r#""Playlist""#,
                item.playlist_id,
                "Break content playlist",
            )
            .await
        }
        _ => Err(ServerFnError::new(
            "Break content has unsupported collection type.",
        )),
    }
}

#[cfg(feature = "ssr")]
fn validate_break_content(items: &[DecoBreakContentForm]) -> Result<(), ServerFnError> {
    let count = |placement| {
        items
            .iter()
            .filter(|item| item.placement == placement)
            .count()
    };
    if count(PLACEMENT_BLOCK_START) > 1 {
        return Err(ServerFnError::new(
            "Deco may only contain one [Block Start] break content",
        ));
    }
    if count(PLACEMENT_BETWEEN_BLOCK_ITEMS) > 1 {
        return Err(ServerFnError::new(
            "Deco may only contain one [Between Block Items] break content",
        ));
    }
    if count(PLACEMENT_CHAPTER_MARKERS) > 1 {
        return Err(ServerFnError::new(
            "Deco may only contain one [At Chapter Markers] break content",
        ));
    }
    if count(PLACEMENT_BLOCK_FINISH) > 1 {
        return Err(ServerFnError::new(
            "Deco may only contain one [Block Finish] break content",
        ));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_optional_reference(
    pool: &DbPool,
    table: &str,
    id: Option<i64>,
    label: &str,
) -> Result<(), ServerFnError> {
    let Some(id) = id.filter(|value| *value > 0) else {
        return Err(ServerFnError::new(format!("{label} must have valid item")));
    };
    let exists: i64 = sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(format!(
        r#"SELECT COUNT(*) FROM {table} WHERE "Id" = ?"#
    )))
    .bind(id)
    .fetch_one(pool)
    .await?;
    if exists == 0 {
        return Err(ServerFnError::new(format!("{label} item does not exist.")));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_ids_exist(
    pool: &DbPool,
    table: &str,
    ids: &[i64],
    label: &str,
) -> Result<(), ServerFnError> {
    for id in ids.iter().copied().filter(|id| *id > 0) {
        let exists: i64 = sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(format!(
            r#"SELECT COUNT(*) FROM {table} WHERE "Id" = ?"#
        )))
        .bind(id)
        .fetch_one(pool)
        .await?;
        if exists == 0 {
            return Err(ServerFnError::new(format!("{label} {id} does not exist.")));
        }
    }
    Ok(())
}

fn normalize_deco_form(form: &mut DecoForm) {
    form.watermark_ids.sort_unstable();
    form.watermark_ids.dedup();
    form.graphics_element_ids.sort_unstable();
    form.graphics_element_ids.dedup();

    if form.watermark_mode != DECO_MODE_OVERRIDE && form.watermark_mode != DECO_MODE_MERGE {
        form.watermark_ids.clear();
        form.use_watermark_during_filler = false;
    } else if form.watermark_mode != DECO_MODE_OVERRIDE {
        form.use_watermark_during_filler = false;
    }

    if form.graphics_elements_mode != DECO_MODE_OVERRIDE
        && form.graphics_elements_mode != DECO_MODE_MERGE
    {
        form.graphics_element_ids.clear();
        form.use_graphics_elements_during_filler = false;
    } else if form.graphics_elements_mode != DECO_MODE_OVERRIDE {
        form.use_graphics_elements_during_filler = false;
    }

    if form.break_content_mode != DECO_MODE_OVERRIDE {
        form.break_content.clear();
    }

    normalize_collection_fields(
        form.default_filler_mode,
        form.default_filler_collection_type,
        &mut form.default_filler_collection_id,
        &mut form.default_filler_media_item_id,
        &mut form.default_filler_multi_collection_id,
        &mut form.default_filler_smart_collection_id,
    );
    normalize_collection_fields(
        form.dead_air_fallback_mode,
        form.dead_air_fallback_collection_type,
        &mut form.dead_air_fallback_collection_id,
        &mut form.dead_air_fallback_media_item_id,
        &mut form.dead_air_fallback_multi_collection_id,
        &mut form.dead_air_fallback_smart_collection_id,
    );

    for item in &mut form.break_content {
        normalize_break_content_fields(item);
    }
}

fn normalize_collection_fields(
    mode: i32,
    collection_type: i32,
    collection_id: &mut Option<i64>,
    media_item_id: &mut Option<i64>,
    multi_collection_id: &mut Option<i64>,
    smart_collection_id: &mut Option<i64>,
) {
    if mode != DECO_MODE_OVERRIDE {
        *collection_id = None;
        *media_item_id = None;
        *multi_collection_id = None;
        *smart_collection_id = None;
        return;
    }

    match collection_type {
        COLLECTION_TYPE_COLLECTION => {
            *media_item_id = None;
            *multi_collection_id = None;
            *smart_collection_id = None;
        }
        COLLECTION_TYPE_MULTI_COLLECTION => {
            *collection_id = None;
            *media_item_id = None;
            *smart_collection_id = None;
        }
        COLLECTION_TYPE_SMART_COLLECTION => {
            *collection_id = None;
            *media_item_id = None;
            *multi_collection_id = None;
        }
        _ => {
            *collection_id = None;
            *multi_collection_id = None;
            *smart_collection_id = None;
        }
    }
}

fn normalize_break_content_fields(item: &mut DecoBreakContentForm) {
    match item.collection_type {
        COLLECTION_TYPE_COLLECTION => {
            item.media_item_id = None;
            item.multi_collection_id = None;
            item.smart_collection_id = None;
            item.playlist_id = None;
        }
        COLLECTION_TYPE_MULTI_COLLECTION => {
            item.collection_id = None;
            item.media_item_id = None;
            item.smart_collection_id = None;
            item.playlist_id = None;
        }
        COLLECTION_TYPE_SMART_COLLECTION => {
            item.collection_id = None;
            item.media_item_id = None;
            item.multi_collection_id = None;
            item.playlist_id = None;
        }
        COLLECTION_TYPE_PLAYLIST => {
            item.collection_id = None;
            item.media_item_id = None;
            item.multi_collection_id = None;
            item.smart_collection_id = None;
        }
        _ => {
            item.collection_id = None;
            item.multi_collection_id = None;
            item.smart_collection_id = None;
            item.playlist_id = None;
        }
    }
}

#[cfg(feature = "ssr")]
async fn load_options(pool: &DbPool, sql: &str) -> Result<Vec<DecoOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(ersatzrs_infra::db::assert_sql_safe(sql))
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| DecoOption {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_deco_playlist_options(
    pool: &DbPool,
) -> Result<Vec<DecoPlaylistOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Id", "PlaylistGroupId", COALESCE("Name", '') AS "Name"
           FROM "Playlist"
           ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| DecoPlaylistOption {
            id: row.try_get("Id").unwrap_or_default(),
            group_id: row.try_get("PlaylistGroupId").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_i64_ids(pool: &DbPool, sql: &str, id: i64) -> Result<Vec<i64>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(ersatzrs_infra::db::assert_sql_safe(sql))
        .bind(id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .filter_map(|row| row.try_get("Id").ok())
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_deco_break_content(
    pool: &DbPool,
    deco_id: i64,
) -> Result<Vec<DecoBreakContentForm>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT dbc."Id",
                  dbc."CollectionType",
                  dbc."CollectionId",
                  dbc."MediaItemId",
                  dbc."MultiCollectionId",
                  dbc."SmartCollectionId",
                  dbc."PlaylistId",
                  dbc."Placement",
                  COALESCE(c."Name", mc."Name", sc."Name", p."Name", shm."Title",
                           CASE WHEN sem."Title" IS NOT NULL THEN sem."Title" || ' S' || printf('%02d', s."SeasonNumber") END,
                           am."Title", '') AS "CollectionName"
           FROM "DecoBreakContent" dbc
           LEFT JOIN "Collection" c ON c."Id" = dbc."CollectionId"
           LEFT JOIN "MultiCollection" mc ON mc."Id" = dbc."MultiCollectionId"
           LEFT JOIN "SmartCollection" sc ON sc."Id" = dbc."SmartCollectionId"
           LEFT JOIN "Playlist" p ON p."Id" = dbc."PlaylistId"
           LEFT JOIN "Show" sh ON sh."Id" = dbc."MediaItemId"
           LEFT JOIN "ShowMetadata" shm ON shm."ShowId" = sh."Id"
           LEFT JOIN "Season" s ON s."Id" = dbc."MediaItemId"
           LEFT JOIN "SeasonMetadata" sem ON sem."SeasonId" = s."Id"
           LEFT JOIN "Artist" a ON a."Id" = dbc."MediaItemId"
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           WHERE dbc."DecoId" = ?
           ORDER BY dbc."Placement", dbc."Id""#,
    )
    .bind(deco_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| DecoBreakContentForm {
            id: row.try_get("Id").unwrap_or_default(),
            collection_type: row.try_get("CollectionType").unwrap_or_default(),
            collection_id: row.try_get("CollectionId").unwrap_or(None),
            media_item_id: row.try_get("MediaItemId").unwrap_or(None),
            multi_collection_id: row.try_get("MultiCollectionId").unwrap_or(None),
            smart_collection_id: row.try_get("SmartCollectionId").unwrap_or(None),
            playlist_id: row.try_get("PlaylistId").unwrap_or(None),
            collection_name: row.try_get("CollectionName").unwrap_or_default(),
            placement: row.try_get("Placement").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn replace_deco_ids(
    tx: &mut sqlx::Transaction<'_, Db>,
    delete_sql: &str,
    insert_sql: &str,
    deco_id: i64,
    ids: &[i64],
) -> Result<(), ServerFnError> {
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(delete_sql))
        .bind(deco_id)
        .execute(&mut **tx)
        .await?;
    for id in ids.iter().copied().filter(|id| *id > 0) {
        sqlx::query(ersatzrs_infra::db::assert_sql_safe(insert_sql))
            .bind(deco_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

#[cfg(feature = "ssr")]
fn read_bool(row: &DbRow, column: &str) -> bool {
    use sqlx::Row;

    row.try_get::<bool, _>(column)
        .or_else(|_| row.try_get::<i64, _>(column).map(|value| value != 0))
        .unwrap_or(false)
}

#[cfg(feature = "ssr")]
async fn validate_deco_template_group_name(
    pool: &DbPool,
    name: &str,
) -> Result<String, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Deco template group name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(
            "Deco template group name must be 50 characters or fewer",
        ));
    }
    let duplicate: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "DecoTemplateGroup" WHERE "Name" = ?"#)
            .bind(name)
            .fetch_one(pool)
            .await?;
    if duplicate > 0 {
        return Err(ServerFnError::new(
            "Deco template group name must be unique",
        ));
    }
    Ok(name.to_string())
}

#[cfg(feature = "ssr")]
async fn validate_deco_template_group_exists(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let exists: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "DecoTemplateGroup" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(pool)
            .await?;
    if exists == 0 {
        return Err(ServerFnError::new("Deco template group does not exist."));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_deco_template_exists(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "DecoTemplate" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(ServerFnError::new("[DecoTemplateId] does not exist."));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_deco_template_name(
    pool: &DbPool,
    group_id: i64,
    template_id: Option<i64>,
    name: &str,
) -> Result<String, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Deco template name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new(format!(
            "Deco template name \"{name}\" is invalid"
        )));
    }
    let duplicate: i64 = match template_id {
        Some(id) => {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "DecoTemplate"
                   WHERE "DecoTemplateGroupId" = ? AND "Name" = ? AND "Id" != ?"#,
            )
            .bind(group_id)
            .bind(name)
            .bind(id)
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "DecoTemplate"
                   WHERE "DecoTemplateGroupId" = ? AND "Name" = ?"#,
            )
            .bind(group_id)
            .bind(name)
            .fetch_one(pool)
            .await?
        }
    };
    if duplicate > 0 {
        return Err(ServerFnError::new(format!(
            "A deco template named \"{name}\" already exists in that deco template group"
        )));
    }
    Ok(name.to_string())
}

#[cfg(feature = "ssr")]
async fn validate_deco_template_items(
    pool: &DbPool,
    items: &[DecoTemplateItemForm],
) -> Result<(), ServerFnError> {
    if items.is_empty() {
        return Ok(());
    }

    for item in items {
        let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Deco" WHERE "Id" = ?"#)
            .bind(item.deco_id)
            .fetch_one(pool)
            .await?;
        if exists == 0 {
            return Err(ServerFnError::new("[DecoId] does not exist."));
        }
    }

    let mut ranges = Vec::with_capacity(items.len());
    for item in items {
        let start = item.start_minutes.clamp(0, 24 * 60);
        let end = if item.end_minutes == 0 {
            24 * 60
        } else {
            item.end_minutes.clamp(0, 24 * 60)
        };
        if start >= end {
            return Err(ServerFnError::new("Deco template items cannot be empty."));
        }
        ranges.push((start, end));
    }

    for (index, (start, end)) in ranges.iter().enumerate() {
        for (other_start, other_end) in ranges.iter().skip(index + 1) {
            if *start < *other_end && *other_start < *end {
                return Err(ServerFnError::new(format!(
                    "Deco from {} to {} intersects deco from {} to {}",
                    format_minutes(*start),
                    format_minutes(*end),
                    format_minutes(*other_start),
                    format_minutes(*other_end)
                )));
            }
        }
    }
    Ok(())
}

pub fn format_minutes(minutes: i32) -> String {
    let clamped = minutes.clamp(0, 24 * 60);
    let hours = clamped / 60;
    let mins = clamped % 60;
    format!("{hours:02}:{mins:02}:00")
}

pub fn format_short_time(minutes: i32) -> String {
    let clamped = minutes.clamp(0, 24 * 60);
    let hours = clamped / 60;
    let mins = clamped % 60;
    format!("{hours:02}:{mins:02}")
}

pub fn parse_time_to_minutes(value: &str) -> Option<i32> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<i32>().ok()?;
    let minutes = parts.next()?.parse::<i32>().ok()?;
    Some((hours * 60 + minutes).clamp(0, 24 * 60))
}
