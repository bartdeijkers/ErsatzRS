//! Server functions for block group / block management and the block editor.
//! Ports:
//! - `Pages/Blocks.razor` → [`list_block_groups`], [`list_blocks`],
//!   [`create_block_group`], [`delete_block_group`], [`create_block`],
//!   [`delete_block`]
//! - `Pages/BlockEditor.razor` → [`get_block`], [`get_block_items`],
//!   [`save_block_with_items`], [`preview_block_playout`]
//! - Autocomplete helpers → [`search_collections_for_block`],
//!   [`search_smart_collections_for_block`], [`search_shows_for_block`],
//!   [`search_seasons_for_block`]
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared value types
// ---------------------------------------------------------------------------

/// `CollectionType` discriminants (mirrors `ErsatzTV.Core.Domain.CollectionType`).
pub mod collection_type {
    pub const COLLECTION: i32 = 0;
    pub const TELEVISION_SHOW: i32 = 1;
    pub const TELEVISION_SEASON: i32 = 2;
    pub const SMART_COLLECTION: i32 = 5;
    pub const SEARCH_QUERY: i32 = 9;
}

/// `PlaybackOrder` discriminants (mirrors `ErsatzTV.Core.Domain.PlaybackOrder`).
pub mod playback_order {
    pub const CHRONOLOGICAL: i32 = 1;
    pub const RANDOM: i32 = 2;
    pub const SHUFFLE: i32 = 3;
    pub const SEASON_EPISODE: i32 = 6;
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
    pub smart_collection_id: Option<i64>,
    pub media_item_id: Option<i64>,
    pub search_title: Option<String>,
    pub search_query: Option<String>,
    pub playback_order: i32,
    pub include_in_program_guide: bool,
    pub disable_watermarks: bool,
    /// Resolved display name for the referenced collection / show / season.
    pub collection_name: String,
    /// IDs from `BlockItemWatermark` — round-tripped through save unchanged.
    pub watermark_ids: Vec<i64>,
    /// IDs from `BlockItemGraphicsElement` — round-tripped through save unchanged.
    pub graphics_element_ids: Vec<i64>,
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

// ---------------------------------------------------------------------------
// Block groups
// ---------------------------------------------------------------------------

#[server]
pub async fn list_block_groups() -> Result<Vec<BlockGroupRow>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "BlockGroup" ORDER BY "Name" COLLATE NOCASE"#)
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
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Block group name is required"));
    }

    let id = sqlx::query(r#"INSERT INTO "BlockGroup" ("Name") VALUES (?)"#)
        .bind(name)
        .execute(&state.pool)
        .await?
        .last_insert_rowid();

    Ok(id)
}

#[server]
pub async fn delete_block_group(id: i64) -> Result<(), ServerFnError> {
    use crate::state::AppState;
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
    use crate::state::AppState;
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
           WHERE (? = '' OR b."Name" LIKE ? COLLATE NOCASE)
           ORDER BY bg."Name" COLLATE NOCASE, b."Name" COLLATE NOCASE"#,
    )
    .bind(search.trim())
    .bind(&like)
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
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Block name is required"));
    }

    let id = sqlx::query(
        r#"INSERT INTO "Block" ("BlockGroupId", "Name", "Minutes", "StopScheduling", "DateUpdated")
           VALUES (?, ?, 60, 0, datetime('now'))"#,
    )
    .bind(group_id)
    .bind(name)
    .execute(&state.pool)
    .await?
    .last_insert_rowid();

    Ok(id)
}

#[server]
pub async fn delete_block(id: i64) -> Result<(), ServerFnError> {
    use crate::state::AppState;
    let state = expect_context::<AppState>();
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

#[server]
pub async fn get_block(id: i64) -> Result<Option<BlockForm>, ServerFnError> {
    use crate::state::AppState;
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let rows = sqlx::query(
        r#"SELECT
               bi."Id",
               bi."Index",
               bi."CollectionType",
               bi."CollectionId",
               bi."SmartCollectionId",
               bi."MediaItemId",
               bi."SearchTitle",
               bi."SearchQuery",
               bi."PlaybackOrder",
               bi."IncludeInProgramGuide",
               bi."DisableWatermarks",
               CASE bi."CollectionType"
                   WHEN 0 THEN COALESCE(c."Name", '')
                   WHEN 1 THEN COALESCE(sm."Title", '')
                   WHEN 2 THEN COALESCE(sh."Title", '') || ' S' || PRINTF('%02d', s."SeasonNumber")
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
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| BlockItemForm {
            id: r.try_get("Id").unwrap_or(0),
            index: r.try_get("Index").unwrap_or(0),
            collection_type: r.try_get("CollectionType").unwrap_or(0),
            collection_id: r.try_get("CollectionId").unwrap_or(None),
            smart_collection_id: r.try_get("SmartCollectionId").unwrap_or(None),
            media_item_id: r.try_get("MediaItemId").unwrap_or(None),
            search_title: r.try_get("SearchTitle").unwrap_or(None),
            search_query: r.try_get("SearchQuery").unwrap_or(None),
            playback_order: r.try_get("PlaybackOrder").unwrap_or(1),
            include_in_program_guide: r
                .try_get::<bool, _>("IncludeInProgramGuide")
                .unwrap_or(false),
            disable_watermarks: r.try_get::<bool, _>("DisableWatermarks").unwrap_or(false),
            collection_name: r.try_get("CollectionName").unwrap_or_default(),
            watermark_ids: parse_id_list(r.try_get("WatermarkIds").unwrap_or(None)),
            graphics_element_ids: parse_id_list(r.try_get("GraphicsElementIds").unwrap_or(None)),
        })
        .collect())
}

/// Replace the block metadata and all its items atomically.
#[server]
pub async fn save_block_with_items(form: BlockWithItemsForm) -> Result<(), ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let name = form.name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Block name is required"));
    }

    let mut tx = state.pool.begin().await?;

    sqlx::query(
        r#"UPDATE "Block"
           SET "Name" = ?, "Minutes" = ?, "StopScheduling" = ?, "DateUpdated" = datetime('now')
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
        let new_id = sqlx::query(
            r#"INSERT INTO "BlockItem"
               ("BlockId", "Index", "CollectionType", "CollectionId",
                "SmartCollectionId", "MediaItemId", "SearchTitle", "SearchQuery",
                "PlaybackOrder", "IncludeInProgramGuide", "DisableWatermarks")
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(form.id)
        .bind(item.index)
        .bind(item.collection_type)
        .bind(item.collection_id)
        .bind(item.smart_collection_id)
        .bind(item.media_item_id)
        .bind(item.search_title.as_deref())
        .bind(item.search_query.as_deref())
        .bind(item.playback_order)
        .bind(item.include_in_program_guide)
        .bind(item.disable_watermarks)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();

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
pub async fn preview_block_playout(block_id: i64) -> Result<Vec<PlayoutPreviewRow>, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();

    let items = get_block_items(block_id).await?;

    let mut rows: Vec<PlayoutPreviewRow> = Vec::new();
    let mut elapsed_secs: i64 = 0;

    for item in &items {
        let media_rows = query_items_for_preview(item, &state.pool).await;
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

#[allow(clippy::too_many_lines)]
async fn query_items_for_preview(
    item: &BlockItemForm,
    pool: &sqlx::SqlitePool,
) -> Vec<(String, i64)> {
    use sqlx::Row;

    match item.collection_type {
        // Manual collection — list its media items with title + duration
        0 => {
            let Some(cid) = item.collection_id else {
                return vec![];
            };
            let order_clause = playback_order_clause(item.playback_order, false);
            let sql = format!(
                r#"SELECT "Title", "DurSecs" FROM (
                       SELECT COALESCE(mm."Title", '') AS "Title",
                              {dur_expr} AS "DurSecs"
                       FROM "CollectionItem" ci
                       JOIN "MediaItem" mi ON mi."Id" = ci."MediaItemId"
                       LEFT JOIN "Movie" mv_row ON mv_row."Id" = mi."Id"
                       LEFT JOIN "MovieMetadata" mm ON mm."MovieId" = mv_row."Id"
                       LEFT JOIN "MediaVersion" mvv ON mvv."MovieId" = mv_row."Id"
                       WHERE ci."CollectionId" = ?
                       UNION ALL
                       SELECT COALESCE(sm."Title", '') AS "Title",
                              {dur_expr_ep} AS "DurSecs"
                       FROM "CollectionItem" ci
                       JOIN "MediaItem" mi ON mi."Id" = ci."MediaItemId"
                       LEFT JOIN "Episode" ep ON ep."Id" = mi."Id"
                       LEFT JOIN "EpisodeMetadata" sm ON sm."EpisodeId" = ep."Id"
                       LEFT JOIN "MediaVersion" mvv ON mvv."EpisodeId" = ep."Id"
                       WHERE ci."CollectionId" = ?
                   ) ORDER BY {order_clause}"#,
                dur_expr = "CAST(SUBSTR(mvv.\"Duration\", 1, 2) AS INTEGER) * 3600 + CAST(SUBSTR(mvv.\"Duration\", 4, 2) AS INTEGER) * 60 + CAST(SUBSTR(mvv.\"Duration\", 7, 2) AS INTEGER)",
                dur_expr_ep = "CAST(SUBSTR(mvv.\"Duration\", 1, 2) AS INTEGER) * 3600 + CAST(SUBSTR(mvv.\"Duration\", 4, 2) AS INTEGER) * 60 + CAST(SUBSTR(mvv.\"Duration\", 7, 2) AS INTEGER)",
                order_clause = order_clause,
            );
            sqlx::query(&sql)
                .bind(cid)
                .bind(cid)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let title: String = r.try_get("Title").unwrap_or_default();
                    let dur: i64 = r.try_get("DurSecs").unwrap_or(0);
                    (title, dur)
                })
                .collect()
        }

        // Television show — list episodes
        1 => {
            let Some(show_id) = item.media_item_id else {
                return vec![];
            };
            let order_clause = playback_order_clause(item.playback_order, true);
            let sql = format!(
                r#"SELECT COALESCE(sm."Title", '') AS "Title",
                          se."SeasonNumber" AS "SeasonNum",
                          em."EpisodeNumber" AS "EpNum",
                          CAST(SUBSTR(mv."Duration", 1, 2) AS INTEGER) * 3600 +
                          CAST(SUBSTR(mv."Duration", 4, 2) AS INTEGER) * 60 +
                          CAST(SUBSTR(mv."Duration", 7, 2) AS INTEGER) AS "DurSecs"
                   FROM "Episode" e
                   JOIN "Season" se ON se."Id" = e."SeasonId"
                   JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
                   LEFT JOIN "MediaVersion" mv ON mv."EpisodeId" = e."Id"
                   WHERE se."ShowId" = ?
                   ORDER BY {order_clause}"#,
            );
            sqlx::query(&sql)
                .bind(show_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let title: String = r.try_get("Title").unwrap_or_default();
                    let dur: i64 = r.try_get("DurSecs").unwrap_or(0);
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
                                CAST(SUBSTR(mv."Duration", 1, 2) AS INTEGER) * 3600 +
                                CAST(SUBSTR(mv."Duration", 4, 2) AS INTEGER) * 60 +
                                CAST(SUBSTR(mv."Duration", 7, 2) AS INTEGER) AS "DurSecs"
                         FROM "Episode" e
                         JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
                         LEFT JOIN "MediaVersion" mv ON mv."EpisodeId" = e."Id"
                         WHERE e."SeasonId" = ?
                         ORDER BY em."EpisodeNumber""#;
            sqlx::query(sql)
                .bind(season_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let title: String = r.try_get("Title").unwrap_or_default();
                    let dur: i64 = r.try_get("DurSecs").unwrap_or(0);
                    (title, dur)
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

fn playback_order_clause(order: i32, is_episode: bool) -> &'static str {
    match order {
        6 if is_episode => r#""SeasonNum", "EpNum""#,
        2 | 3 => "RANDOM()",
        _ => r#""Title" COLLATE NOCASE"#,
    }
}

// ---------------------------------------------------------------------------
// Autocomplete helpers
// ---------------------------------------------------------------------------

#[server]
pub async fn search_collections_for_block(
    prefix: String,
) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::state::AppState;
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
           WHERE "Name" LIKE ? COLLATE NOCASE
           ORDER BY "Name" COLLATE NOCASE
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
    use crate::state::AppState;
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
           WHERE "Name" LIKE ? COLLATE NOCASE
           ORDER BY "Name" COLLATE NOCASE
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
    use crate::state::AppState;
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
           WHERE sm."Title" LIKE ? COLLATE NOCASE
           ORDER BY sm."Title" COLLATE NOCASE
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
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Ok(vec![]);
    }
    let like = format!("{prefix}%");

    let rows = sqlx::query(
        r#"SELECT se."Id",
                  COALESCE(sm."Title", '') || ' S' || PRINTF('%02d', se."SeasonNumber") AS "Label"
           FROM "Season" se
           JOIN "Show" sh ON sh."Id" = se."ShowId"
           JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id"
           WHERE sm."Title" LIKE ? COLLATE NOCASE
           ORDER BY sm."Title" COLLATE NOCASE, se."SeasonNumber"
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
                r.try_get("Label").unwrap_or_default(),
            )
        })
        .collect())
}

/// Human-readable label for a `PlaybackOrder` integer.
#[must_use]
pub fn playback_order_label(order: i32, collection_type: i32) -> &'static str {
    match order {
        2 => "Random",
        3 => "Shuffle",
        6 if collection_type == 1 => "Season, Episode",
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
