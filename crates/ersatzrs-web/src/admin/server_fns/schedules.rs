//! Program schedule admin functions. Ports the first `/schedules` route layer
//! from ErsatzTV's `Schedules.razor`, `ScheduleEditor.razor`, and the list
//! portion of `ScheduleItemsEditor.razor`.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const SCHEDULES_PAGE_SIZE_KEY: &str = "pages.schedules.page_size";
const SCHEDULES_DETAIL_PAGE_SIZE_KEY: &str = "pages.schedules.detail_page_size";

#[cfg(feature = "ssr")]
type DbPool = sqlx::Pool<ersatzrs_infra::db::Db>;

pub const COLLECTION_TYPE_COLLECTION: i32 = 0;
pub const COLLECTION_TYPE_TELEVISION_SHOW: i32 = 1;
pub const COLLECTION_TYPE_TELEVISION_SEASON: i32 = 2;
pub const COLLECTION_TYPE_ARTIST: i32 = 3;
pub const COLLECTION_TYPE_MULTI_COLLECTION: i32 = 4;
pub const COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
pub const COLLECTION_TYPE_PLAYLIST: i32 = 6;
pub const COLLECTION_TYPE_RERUN_FIRST_RUN: i32 = 7;
pub const COLLECTION_TYPE_RERUN_RERUN: i32 = 8;
pub const COLLECTION_TYPE_SEARCH_QUERY: i32 = 9;

pub const START_TYPE_DYNAMIC: i32 = 1;
pub const START_TYPE_FIXED: i32 = 2;

pub const PLAYOUT_MODE_FLOOD: i32 = 1;
pub const PLAYOUT_MODE_ONE: i32 = 2;
pub const PLAYOUT_MODE_MULTIPLE: i32 = 3;
pub const PLAYOUT_MODE_DURATION: i32 = 4;

pub const PLAYBACK_ORDER_NONE: i32 = 0;
pub const PLAYBACK_ORDER_CHRONOLOGICAL: i32 = 1;
pub const PLAYBACK_ORDER_RANDOM: i32 = 2;
pub const PLAYBACK_ORDER_SHUFFLE: i32 = 3;
pub const PLAYBACK_ORDER_SHUFFLE_IN_ORDER: i32 = 4;
pub const PLAYBACK_ORDER_MULTI_EPISODE_SHUFFLE: i32 = 5;
pub const PLAYBACK_ORDER_SEASON_EPISODE: i32 = 6;
pub const PLAYBACK_ORDER_RANDOM_ROTATION: i32 = 7;
pub const PLAYBACK_ORDER_MARATHON: i32 = 8;

pub const FILL_WITH_GROUP_MODE_NONE: i32 = 0;
pub const GUIDE_MODE_NORMAL: i32 = 0;
pub const MARATHON_GROUP_BY_NONE: i32 = 0;

pub const MULTIPLE_MODE_COUNT: i32 = 0;
pub const MULTIPLE_MODE_COLLECTION_SIZE: i32 = 1;
pub const MULTIPLE_MODE_PLAYLIST_ITEM_SIZE: i32 = 2;
pub const MULTIPLE_MODE_MULTI_EPISODE_GROUP_SIZE: i32 = 3;

pub const TAIL_MODE_NONE: i32 = 0;
pub const TAIL_MODE_FILLER: i32 = 3;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleListRow {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleItemListRow {
    pub id: i64,
    pub index: i32,
    pub start_type: i32,
    pub start_time: Option<String>,
    pub collection_name: String,
    pub playout_mode: i32,
    pub multiple_mode: Option<i32>,
    pub multiple_count: Option<String>,
    pub duration: Option<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleItemForm {
    pub id: i64,
    pub index: i32,
    pub start_type: i32,
    pub start_time: Option<String>,
    pub fixed_start_time_behavior: Option<i32>,
    pub collection_type: i32,
    pub collection_id: Option<i64>,
    pub media_item_id: Option<i64>,
    pub multi_collection_id: Option<i64>,
    pub smart_collection_id: Option<i64>,
    pub rerun_collection_id: Option<i64>,
    pub playlist_id: Option<i64>,
    pub search_title: Option<String>,
    pub search_query: Option<String>,
    pub collection_name: String,
    pub playback_order: i32,
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
    pub pre_roll_filler_id: Option<i64>,
    pub mid_roll_filler_id: Option<i64>,
    pub post_roll_filler_id: Option<i64>,
    pub tail_filler_id: Option<i64>,
    pub fallback_filler_id: Option<i64>,
    #[serde(default)]
    pub watermark_ids: Vec<i64>,
    #[serde(default)]
    pub graphics_element_ids: Vec<i64>,
    pub preferred_audio_language_code: Option<String>,
    pub preferred_audio_title: String,
    pub preferred_subtitle_language_code: Option<String>,
    pub subtitle_mode: Option<i32>,
}

impl Default for ScheduleItemForm {
    fn default() -> Self {
        Self {
            id: 0,
            index: 0,
            start_type: START_TYPE_DYNAMIC,
            start_time: None,
            fixed_start_time_behavior: None,
            collection_type: COLLECTION_TYPE_COLLECTION,
            collection_id: None,
            media_item_id: None,
            multi_collection_id: None,
            smart_collection_id: None,
            rerun_collection_id: None,
            playlist_id: None,
            search_title: None,
            search_query: None,
            collection_name: String::new(),
            playback_order: PLAYBACK_ORDER_SHUFFLE,
            marathon_group_by: MARATHON_GROUP_BY_NONE,
            marathon_shuffle_groups: false,
            marathon_shuffle_items: false,
            marathon_batch_size: None,
            fill_with_group_mode: FILL_WITH_GROUP_MODE_NONE,
            playout_mode: PLAYOUT_MODE_ONE,
            multiple_mode: MULTIPLE_MODE_COUNT,
            multiple_count: "1".to_string(),
            duration_hours: 1,
            duration_minutes: 0,
            tail_mode: TAIL_MODE_NONE,
            discard_to_fill_attempts: Some(0),
            custom_title: String::new(),
            guide_mode: GUIDE_MODE_NORMAL,
            pre_roll_filler_id: None,
            mid_roll_filler_id: None,
            post_roll_filler_id: None,
            tail_filler_id: None,
            fallback_filler_id: None,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
            preferred_audio_language_code: None,
            preferred_audio_title: String::new(),
            preferred_subtitle_language_code: None,
            subtitle_mode: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleItemOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleLanguageOption {
    pub code: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistOption {
    pub id: i64,
    pub name: String,
    pub group_id: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FillerOption {
    pub id: i64,
    pub name: String,
    pub filler_kind: i32,
    pub pad_to_nearest_minute: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleItemsEditorOptions {
    pub collections: Vec<ScheduleItemOption>,
    pub shows: Vec<ScheduleItemOption>,
    pub seasons: Vec<ScheduleItemOption>,
    pub artists: Vec<ScheduleItemOption>,
    pub multi_collections: Vec<ScheduleItemOption>,
    pub smart_collections: Vec<ScheduleItemOption>,
    pub rerun_collections: Vec<ScheduleItemOption>,
    pub playlist_groups: Vec<ScheduleItemOption>,
    pub playlists: Vec<PlaylistOption>,
    pub fillers: Vec<FillerOption>,
    pub watermarks: Vec<ScheduleItemOption>,
    pub graphics_elements: Vec<ScheduleItemOption>,
    pub languages: Vec<ScheduleLanguageOption>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleForm {
    pub id: i64,
    pub name: String,
    pub keep_multi_part_episodes_together: bool,
    pub treat_collections_as_shows: bool,
    pub shuffle_schedule_items: bool,
    pub random_start_point: bool,
    pub fixed_start_time_behavior: i32,
}

impl Default for ScheduleForm {
    fn default() -> Self {
        Self {
            id: 0,
            name: "New Schedule".to_string(),
            keep_multi_part_episodes_together: true,
            treat_collections_as_shows: false,
            shuffle_schedule_items: false,
            random_start_point: false,
            fixed_start_time_behavior: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleItemsPageData {
    pub schedule: ScheduleForm,
    pub items: Vec<ScheduleItemForm>,
    pub options: ScheduleItemsEditorOptions,
}

#[server]
pub async fn get_schedules_page_size() -> Result<(u32, u32), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok((
        get_page_size_with_pool(&state.pool, SCHEDULES_PAGE_SIZE_KEY).await?,
        get_page_size_with_pool(&state.pool, SCHEDULES_DETAIL_PAGE_SIZE_KEY).await?,
    ))
}

#[server]
pub async fn save_schedules_detail_page_size(page_size: u32) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_config_value(
        &state.pool,
        SCHEDULES_DETAIL_PAGE_SIZE_KEY,
        &page_size.max(1).to_string(),
    )
    .await?;
    Ok(())
}

#[server]
pub async fn list_schedules(
    query: String,
    page: u32,
    page_size: u32,
) -> Result<(Vec<ScheduleListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_schedules_with_pool(&state.pool, &query, page, page_size).await
}

#[server]
pub async fn get_schedule_form(id: i64) -> Result<Option<ScheduleForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_schedule_form_with_pool(&state.pool, id).await
}

#[server]
pub async fn save_schedule(form: ScheduleForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_schedule_with_state(&state, form).await
}

#[server]
pub async fn delete_schedule(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    delete_schedule_with_pool(&state.pool, id).await
}

#[server]
pub async fn copy_schedule(id: i64, name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    copy_schedule_with_pool(&state.pool, id, &name).await
}

#[server]
pub async fn get_schedule_items_page(
    id: i64,
) -> Result<Option<ScheduleItemsPageData>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let Some(schedule) = get_schedule_form_with_pool(&state.pool, id).await? else {
        return Ok(None);
    };
    let items = list_schedule_items_with_pool(&state.pool, id).await?;
    let options = load_schedule_item_options_with_pool(&state.pool).await?;
    Ok(Some(ScheduleItemsPageData {
        schedule,
        items,
        options,
    }))
}

#[server]
pub async fn replace_schedule_items(
    schedule_id: i64,
    items: Vec<ScheduleItemForm>,
) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    replace_schedule_items_with_state(&state, schedule_id, items).await
}

#[cfg(feature = "ssr")]
pub async fn get_page_size_with_pool(pool: &DbPool, key: &str) -> Result<u32, ServerFnError> {
    let value: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .fetch_optional(pool)
            .await?;

    Ok(value
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10))
}

#[cfg(feature = "ssr")]
pub async fn list_schedules_with_pool(
    pool: &DbPool,
    query: &str,
    page: u32,
    page_size: u32,
) -> Result<(Vec<ScheduleListRow>, u64), ServerFnError> {
    use sqlx::Row;

    save_config_value(pool, SCHEDULES_PAGE_SIZE_KEY, &page_size.to_string()).await?;

    let like = format!("%{}%", query.trim());
    let has_query = !query.trim().is_empty();
    let total: i64 = if has_query {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ProgramSchedule" WHERE "Name" LIKE ?"#)
            .bind(&like)
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ProgramSchedule""#)
            .fetch_one(pool)
            .await?
    };

    let offset = i64::from(page) * i64::from(page_size);
    let rows = if has_query {
        sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "ProgramSchedule"
               WHERE "Name" LIKE ?
               ORDER BY "Name" COLLATE NOCASE, "Id"
               LIMIT ? OFFSET ?"#,
        )
        .bind(&like)
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "ProgramSchedule"
               ORDER BY "Name" COLLATE NOCASE, "Id"
               LIMIT ? OFFSET ?"#,
        )
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(pool)
        .await?
    };

    let rows = rows
        .iter()
        .map(|row| ScheduleListRow {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((rows, total.max(0) as u64))
}

#[cfg(feature = "ssr")]
pub async fn get_schedule_form_with_pool(
    pool: &DbPool,
    id: i64,
) -> Result<Option<ScheduleForm>, ServerFnError> {
    use sqlx::Row;

    if id == 0 {
        return Ok(Some(ScheduleForm::default()));
    }

    let row = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                  "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                  "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
           FROM "ProgramSchedule"
           WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| ScheduleForm {
        id: row.try_get("Id").unwrap_or(0),
        name: row.try_get("Name").unwrap_or_default(),
        keep_multi_part_episodes_together: read_bool(&row, "KeepMultiPartEpisodesTogether"),
        treat_collections_as_shows: read_bool(&row, "TreatCollectionsAsShows"),
        shuffle_schedule_items: read_bool(&row, "ShuffleScheduleItems"),
        random_start_point: read_bool(&row, "RandomStartPoint"),
        fixed_start_time_behavior: row.try_get("FixedStartTimeBehavior").unwrap_or(0),
    }))
}

#[cfg(feature = "ssr")]
pub async fn save_schedule_with_state(
    state: &crate::host::state::AppState,
    form: ScheduleForm,
) -> Result<i64, ServerFnError> {
    let existing_id = form.id;
    let needs_refresh = if existing_id == 0 {
        false
    } else {
        schedule_update_needs_refresh(&state.pool, existing_id, &form).await?
    };
    let saved_id = save_schedule_with_pool(&state.pool, form).await?;
    if existing_id != 0 && needs_refresh {
        let playout_ids = playout_ids_using_schedule(&state.pool, saved_id).await?;
        enqueue_playout_refreshes(state, playout_ids);
    }
    Ok(saved_id)
}

#[cfg(feature = "ssr")]
pub async fn save_schedule_with_pool(
    pool: &DbPool,
    form: ScheduleForm,
) -> Result<i64, ServerFnError> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Schedule name is required!"));
    }
    if name.chars().count() > 50 {
        return Err(ServerFnError::new(
            "Schedule name must be 50 characters or less.",
        ));
    }
    validate_unique_schedule_name(pool, form.id, name).await?;
    let treat_collections_as_shows =
        form.keep_multi_part_episodes_together && form.treat_collections_as_shows;

    if form.id == 0 {
        let result = sqlx::query(
            r#"INSERT INTO "ProgramSchedule" (
                   "Name", "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                   "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
               ) VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(name)
        .bind(form.keep_multi_part_episodes_together)
        .bind(treat_collections_as_shows)
        .bind(form.shuffle_schedule_items)
        .bind(form.random_start_point)
        .bind(form.fixed_start_time_behavior)
        .execute(pool)
        .await?;
        Ok(ersatzrs_infra::db::last_insert_id(result))
    } else {
        let result = sqlx::query(
            r#"UPDATE "ProgramSchedule"
               SET "Name" = ?,
                   "KeepMultiPartEpisodesTogether" = ?,
                   "TreatCollectionsAsShows" = ?,
                   "ShuffleScheduleItems" = ?,
                   "RandomStartPoint" = ?,
                   "FixedStartTimeBehavior" = ?
               WHERE "Id" = ?"#,
        )
        .bind(name)
        .bind(form.keep_multi_part_episodes_together)
        .bind(treat_collections_as_shows)
        .bind(form.shuffle_schedule_items)
        .bind(form.random_start_point)
        .bind(form.fixed_start_time_behavior)
        .bind(form.id)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(ServerFnError::new("Schedule does not exist"));
        }
        Ok(form.id)
    }
}

#[cfg(feature = "ssr")]
pub async fn delete_schedule_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    sqlx::query(r#"DELETE FROM "ProgramSchedule" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn copy_schedule_with_pool(
    pool: &DbPool,
    id: i64,
    name: &str,
) -> Result<i64, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("New Schedule Name is required."));
    }
    if name.chars().count() > 50 {
        return Err(ServerFnError::new(
            "Schedule name must be 50 characters or less.",
        ));
    }
    validate_unique_schedule_name(pool, 0, name).await?;

    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        r#"INSERT INTO "ProgramSchedule" (
               "Name", "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
               "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
           )
           SELECT ?, "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                  "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
           FROM "ProgramSchedule"
           WHERE "Id" = ?"#,
    )
    .bind(name)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new("[ProgramScheduleId] does not exist."));
    }
    let new_schedule_id = ersatzrs_infra::db::last_insert_id(result);

    let old_item_ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "ProgramScheduleItem"
           WHERE "ProgramScheduleId" = ?
           ORDER BY "Index", "Id""#,
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    for old_item_id in old_item_ids {
        let item_result = sqlx::query(
            r#"INSERT INTO "ProgramScheduleItem" (
                   "CollectionId", "CollectionType", "CustomTitle", "FakeCollectionKey",
                   "FallbackFillerId", "FillWithGroupMode", "FixedStartTimeBehavior",
                   "GuideMode", "Index", "MarathonBatchSize", "MarathonGroupBy",
                   "MarathonShuffleGroups", "MarathonShuffleItems", "MediaItemId",
                   "MidRollFillerId", "MultiCollectionId", "PlaybackOrder", "PlaylistId",
                   "PostRollFillerId", "PreRollFillerId", "PreferredAudioLanguageCode",
                   "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "ProgramScheduleId",
                   "RerunCollectionId", "SmartCollectionId", "StartTime", "SubtitleMode",
                   "TailFillerId", "SearchQuery", "SearchTitle"
               )
               SELECT "CollectionId", "CollectionType", "CustomTitle", "FakeCollectionKey",
                      "FallbackFillerId", "FillWithGroupMode", "FixedStartTimeBehavior",
                      "GuideMode", "Index", "MarathonBatchSize", "MarathonGroupBy",
                      "MarathonShuffleGroups", "MarathonShuffleItems", "MediaItemId",
                      "MidRollFillerId", "MultiCollectionId", "PlaybackOrder", "PlaylistId",
                      "PostRollFillerId", "PreRollFillerId", "PreferredAudioLanguageCode",
                      "PreferredAudioTitle", "PreferredSubtitleLanguageCode", ?,
                      "RerunCollectionId", "SmartCollectionId", "StartTime", "SubtitleMode",
                      "TailFillerId", "SearchQuery", "SearchTitle"
               FROM "ProgramScheduleItem"
               WHERE "Id" = ?"#,
        )
        .bind(new_schedule_id)
        .bind(old_item_id)
        .execute(&mut *tx)
        .await?;
        let new_item_id = ersatzrs_infra::db::last_insert_id(item_result);

        copy_subtype_and_joins(&mut tx, old_item_id, new_item_id).await?;
    }

    tx.commit().await?;
    Ok(new_schedule_id)
}

#[cfg(feature = "ssr")]
async fn copy_subtype_and_joins(
    tx: &mut sqlx::Transaction<'_, ersatzrs_infra::db::Db>,
    old_item_id: i64,
    new_item_id: i64,
) -> Result<(), ServerFnError> {
    sqlx::query(
        r#"INSERT INTO "ProgramScheduleDurationItem" ("Id", "PlayoutDuration", "TailMode", "DiscardToFillAttempts")
           SELECT ?, "PlayoutDuration", "TailMode", "DiscardToFillAttempts"
           FROM "ProgramScheduleDurationItem" WHERE "Id" = ?"#,
    )
    .bind(new_item_id)
    .bind(old_item_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO "ProgramScheduleMultipleItem" ("Id", "Count", "MultipleMode")
           SELECT ?, "Count", "MultipleMode" FROM "ProgramScheduleMultipleItem" WHERE "Id" = ?"#,
    )
    .bind(new_item_id)
    .bind(old_item_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO "ProgramScheduleOneItem" ("Id")
           SELECT ? FROM "ProgramScheduleOneItem" WHERE "Id" = ?"#,
    )
    .bind(new_item_id)
    .bind(old_item_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO "ProgramScheduleFloodItem" ("Id")
           SELECT ? FROM "ProgramScheduleFloodItem" WHERE "Id" = ?"#,
    )
    .bind(new_item_id)
    .bind(old_item_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO "ProgramScheduleItemWatermark" ("ProgramScheduleItemId", "WatermarkId")
           SELECT ?, "WatermarkId" FROM "ProgramScheduleItemWatermark" WHERE "ProgramScheduleItemId" = ?"#,
    )
    .bind(new_item_id)
    .bind(old_item_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO "ProgramScheduleItemGraphicsElement" ("ProgramScheduleItemId", "GraphicsElementId")
           SELECT ?, "GraphicsElementId" FROM "ProgramScheduleItemGraphicsElement" WHERE "ProgramScheduleItemId" = ?"#,
    )
    .bind(new_item_id)
    .bind(old_item_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_unique_schedule_name(
    pool: &DbPool,
    schedule_id: i64,
    name: &str,
) -> Result<(), ServerFnError> {
    let duplicate: i64 = if schedule_id == 0 {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ProgramSchedule" WHERE "Name" = ?"#)
            .bind(name)
            .fetch_one(pool)
            .await?
    } else {
        sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ProgramSchedule" WHERE "Name" = ? AND "Id" <> ?"#,
        )
        .bind(name)
        .bind(schedule_id)
        .fetch_one(pool)
        .await?
    };

    if duplicate > 0 {
        return Err(ServerFnError::new("Schedule name must be unique"));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn schedule_update_needs_refresh(
    pool: &DbPool,
    schedule_id: i64,
    form: &ScheduleForm,
) -> Result<bool, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                  "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
           FROM "ProgramSchedule"
           WHERE "Id" = ?"#,
    )
    .bind(schedule_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Err(ServerFnError::new("Schedule does not exist"));
    };

    let treat_collections_as_shows =
        form.keep_multi_part_episodes_together && form.treat_collections_as_shows;
    Ok(
        read_bool(&row, "KeepMultiPartEpisodesTogether") != form.keep_multi_part_episodes_together
            || read_bool(&row, "TreatCollectionsAsShows") != treat_collections_as_shows
            || read_bool(&row, "ShuffleScheduleItems") != form.shuffle_schedule_items
            || read_bool(&row, "RandomStartPoint") != form.random_start_point
            || row.try_get::<i32, _>("FixedStartTimeBehavior").unwrap_or(0)
                != form.fixed_start_time_behavior,
    )
}

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub async fn list_schedule_items_with_pool(
    pool: &DbPool,
    schedule_id: i64,
) -> Result<Vec<ScheduleItemForm>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT psi."Id", psi."Index", psi."StartTime", psi."FixedStartTimeBehavior",
                  psi."CollectionType", psi."PlaybackOrder", psi."MarathonGroupBy",
                  psi."MarathonShuffleGroups", psi."MarathonShuffleItems",
                  psi."MarathonBatchSize", psi."FillWithGroupMode", psi."CustomTitle",
                  psi."GuideMode", psi."PreRollFillerId", psi."MidRollFillerId",
                  psi."PostRollFillerId", psi."TailFillerId", psi."FallbackFillerId",
                  psi."PreferredAudioLanguageCode", psi."PreferredAudioTitle",
                  psi."PreferredSubtitleLanguageCode", psi."SubtitleMode",
                  psi."CollectionId", psi."MediaItemId", psi."MultiCollectionId",
                  psi."SmartCollectionId", psi."RerunCollectionId", psi."PlaylistId",
                  psi."SearchQuery", psi."SearchTitle",
                  c."Name" AS "CollectionName",
                  mc."Name" AS "MultiCollectionName",
                  sc."Name" AS "SmartCollectionName",
                  rc."Name" AS "RerunCollectionName",
                  p."Name" AS "PlaylistName",
                  am."Title" AS "ArtistName",
                  showm."Title" AS "ShowName",
                  seasonm."Title" AS "SeasonName",
                  season."SeasonNumber",
                  psdi."Id" AS "DurationItemId", psdi."PlayoutDuration",
                  psdi."TailMode", psdi."DiscardToFillAttempts",
                  psmi."Id" AS "MultipleItemId", psmi."Count", psmi."MultipleMode",
                  psoi."Id" AS "OneItemId", psfi."Id" AS "FloodItemId",
                  (SELECT GROUP_CONCAT(psiw."WatermarkId")
                   FROM "ProgramScheduleItemWatermark" psiw
                   WHERE psiw."ProgramScheduleItemId" = psi."Id") AS "WatermarkIds",
                  (SELECT GROUP_CONCAT(psige."GraphicsElementId")
                   FROM "ProgramScheduleItemGraphicsElement" psige
                   WHERE psige."ProgramScheduleItemId" = psi."Id") AS "GraphicsElementIds"
           FROM "ProgramScheduleItem" psi
           LEFT JOIN "Collection" c ON c."Id" = psi."CollectionId"
           LEFT JOIN "MultiCollection" mc ON mc."Id" = psi."MultiCollectionId"
           LEFT JOIN "SmartCollection" sc ON sc."Id" = psi."SmartCollectionId"
           LEFT JOIN "RerunCollection" rc ON rc."Id" = psi."RerunCollectionId"
           LEFT JOIN "Playlist" p ON p."Id" = psi."PlaylistId"
           LEFT JOIN "Artist" a ON a."Id" = psi."MediaItemId" AND psi."CollectionType" = 3
           LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id"
           LEFT JOIN "Show" show ON show."Id" = psi."MediaItemId" AND psi."CollectionType" = 1
           LEFT JOIN "ShowMetadata" showm ON showm."ShowId" = show."Id"
           LEFT JOIN "Season" season ON season."Id" = psi."MediaItemId" AND psi."CollectionType" = 2
           LEFT JOIN "SeasonMetadata" seasonm ON seasonm."SeasonId" = season."Id"
           LEFT JOIN "ProgramScheduleDurationItem" psdi ON psdi."Id" = psi."Id"
           LEFT JOIN "ProgramScheduleMultipleItem" psmi ON psmi."Id" = psi."Id"
           LEFT JOIN "ProgramScheduleOneItem" psoi ON psoi."Id" = psi."Id"
           LEFT JOIN "ProgramScheduleFloodItem" psfi ON psfi."Id" = psi."Id"
           WHERE psi."ProgramScheduleId" = ?
           ORDER BY psi."Index", psi."Id""#,
    )
    .bind(schedule_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| {
            let id = row.try_get("Id").unwrap_or(0);
            let collection_name = item_collection_name(row, id);
            let start_time = row.try_get("StartTime").ok().flatten();
            let playout_mode = playout_mode_from_row(row);
            let (duration_hours, duration_minutes) =
                parse_duration_parts(row.try_get("PlayoutDuration").ok().flatten());
            ScheduleItemForm {
                id,
                index: row.try_get("Index").unwrap_or(0),
                start_type: if start_time.is_some() {
                    START_TYPE_FIXED
                } else {
                    START_TYPE_DYNAMIC
                },
                start_time,
                fixed_start_time_behavior: row.try_get("FixedStartTimeBehavior").ok().flatten(),
                collection_type: row
                    .try_get("CollectionType")
                    .unwrap_or(COLLECTION_TYPE_COLLECTION),
                collection_id: row.try_get("CollectionId").ok().flatten(),
                media_item_id: row.try_get("MediaItemId").ok().flatten(),
                multi_collection_id: row.try_get("MultiCollectionId").ok().flatten(),
                smart_collection_id: row.try_get("SmartCollectionId").ok().flatten(),
                rerun_collection_id: row.try_get("RerunCollectionId").ok().flatten(),
                playlist_id: row.try_get("PlaylistId").ok().flatten(),
                search_title: row.try_get("SearchTitle").ok().flatten(),
                search_query: row.try_get("SearchQuery").ok().flatten(),
                collection_name,
                playback_order: row
                    .try_get("PlaybackOrder")
                    .unwrap_or(PLAYBACK_ORDER_SHUFFLE),
                marathon_group_by: row
                    .try_get("MarathonGroupBy")
                    .unwrap_or(MARATHON_GROUP_BY_NONE),
                marathon_shuffle_groups: read_bool(row, "MarathonShuffleGroups"),
                marathon_shuffle_items: read_bool(row, "MarathonShuffleItems"),
                marathon_batch_size: row.try_get("MarathonBatchSize").ok().flatten(),
                fill_with_group_mode: row
                    .try_get("FillWithGroupMode")
                    .unwrap_or(FILL_WITH_GROUP_MODE_NONE),
                playout_mode,
                multiple_mode: row.try_get("MultipleMode").unwrap_or(MULTIPLE_MODE_COUNT),
                multiple_count: row.try_get("Count").unwrap_or_else(|_| "1".to_string()),
                duration_hours,
                duration_minutes,
                tail_mode: row.try_get("TailMode").unwrap_or(TAIL_MODE_NONE),
                discard_to_fill_attempts: row.try_get("DiscardToFillAttempts").ok().flatten(),
                custom_title: row
                    .try_get::<Option<String>, _>("CustomTitle")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                guide_mode: row.try_get("GuideMode").unwrap_or(GUIDE_MODE_NORMAL),
                pre_roll_filler_id: row.try_get("PreRollFillerId").ok().flatten(),
                mid_roll_filler_id: row.try_get("MidRollFillerId").ok().flatten(),
                post_roll_filler_id: row.try_get("PostRollFillerId").ok().flatten(),
                tail_filler_id: row.try_get("TailFillerId").ok().flatten(),
                fallback_filler_id: row.try_get("FallbackFillerId").ok().flatten(),
                watermark_ids: parse_id_list(row.try_get("WatermarkIds").unwrap_or(None)),
                graphics_element_ids: parse_id_list(
                    row.try_get("GraphicsElementIds").unwrap_or(None),
                ),
                preferred_audio_language_code: row
                    .try_get("PreferredAudioLanguageCode")
                    .ok()
                    .flatten(),
                preferred_audio_title: row
                    .try_get::<Option<String>, _>("PreferredAudioTitle")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                preferred_subtitle_language_code: row
                    .try_get("PreferredSubtitleLanguageCode")
                    .ok()
                    .flatten(),
                subtitle_mode: row.try_get("SubtitleMode").ok().flatten(),
            }
        })
        .collect())
}

#[cfg(feature = "ssr")]
pub async fn load_schedule_item_options_with_pool(
    pool: &DbPool,
) -> Result<ScheduleItemsEditorOptions, ServerFnError> {
    Ok(ScheduleItemsEditorOptions {
        collections: load_simple_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "Collection" ORDER BY "Name""#).await?,
        shows: load_simple_options(pool, r#"SELECT sh."Id", COALESCE(sm."Title", '') AS "Name" FROM "Show" sh LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = sh."Id" ORDER BY sm."Title", sh."Id""#).await?,
        seasons: load_simple_options(pool, r#"SELECT s."Id", COALESCE(sm."Title", 'Season ' || s."SeasonNumber") AS "Name" FROM "Season" s LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id" ORDER BY sm."Title", s."SeasonNumber", s."Id""#).await?,
        artists: load_simple_options(pool, r#"SELECT a."Id", COALESCE(am."Title", '') AS "Name" FROM "Artist" a LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id" ORDER BY am."Title", a."Id""#).await?,
        multi_collections: load_simple_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "MultiCollection" ORDER BY "Name""#).await?,
        smart_collections: load_simple_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "SmartCollection" ORDER BY "Name""#).await?,
        rerun_collections: load_simple_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "RerunCollection" ORDER BY "Name""#).await?,
        playlist_groups: load_simple_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "PlaylistGroup" ORDER BY "Name""#).await?,
        playlists: load_playlist_options(pool).await?,
        fillers: load_filler_options(pool).await?,
        watermarks: load_simple_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "ChannelWatermark" ORDER BY "Name""#).await?,
        graphics_elements: load_simple_options(pool, r#"SELECT "Id", COALESCE("Name", "Path", '') AS "Name" FROM "GraphicsElement" ORDER BY "Name", "Id""#).await?,
        languages: load_language_options(pool).await?,
    })
}

#[cfg(feature = "ssr")]
async fn load_simple_options(
    pool: &DbPool,
    sql: &str,
) -> Result<Vec<ScheduleItemOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(sql).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| ScheduleItemOption {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_playlist_options(pool: &DbPool) -> Result<Vec<PlaylistOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name", "PlaylistGroupId"
           FROM "Playlist"
           ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| PlaylistOption {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
            group_id: row.try_get("PlaylistGroupId").unwrap_or(0),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_filler_options(pool: &DbPool) -> Result<Vec<FillerOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name", "FillerKind",
                  "PadToNearestMinute"
           FROM "FillerPreset"
           ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| FillerOption {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
            filler_kind: row.try_get("FillerKind").unwrap_or(0),
            pad_to_nearest_minute: read_bool(row, "PadToNearestMinute"),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_language_options(
    pool: &DbPool,
) -> Result<Vec<ScheduleLanguageOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "LanguageCode"
           FROM (
               SELECT "Language" AS "LanguageCode" FROM "MediaStream" WHERE "Language" IS NOT NULL
               UNION ALL
               SELECT "Language" AS "LanguageCode" FROM "Subtitle" WHERE "Language" IS NOT NULL
               UNION ALL
               SELECT "PreferredAudioLanguageCode" AS "LanguageCode" FROM "Channel" WHERE "PreferredAudioLanguageCode" IS NOT NULL
           ) codes
           GROUP BY "LanguageCode"
           ORDER BY COUNT("LanguageCode") DESC, "LanguageCode""#,
    )
    .fetch_all(pool)
    .await?;
    let mut seen = HashSet::new();
    let mut options = Vec::new();

    for row in rows {
        let media_code = row
            .try_get::<String, _>("LanguageCode")
            .unwrap_or_default()
            .trim()
            .to_string();
        if media_code.is_empty() {
            continue;
        }

        let expanded = ersatzrs_core::language_codes::all_language_codes([media_code.as_str()]);
        let resolved = expanded
            .into_iter()
            .filter(|code| code.len() == 3)
            .find_map(|code| {
                ersatzrs_core::language_codes::english_name(&code).map(|name| (code, name))
            });

        let (code, name) = resolved.unwrap_or_else(|| (media_code.clone(), media_code));
        if seen.insert(code.clone()) {
            options.push(ScheduleLanguageOption { code, name });
        }
    }

    Ok(options)
}

#[cfg(feature = "ssr")]
pub async fn replace_schedule_items_with_state(
    state: &crate::host::state::AppState,
    schedule_id: i64,
    items: Vec<ScheduleItemForm>,
) -> Result<(), ServerFnError> {
    replace_schedule_items_with_pool(&state.pool, schedule_id, items).await?;
    let playout_ids = playout_ids_using_schedule(&state.pool, schedule_id).await?;
    enqueue_playout_refreshes(state, playout_ids);
    Ok(())
}

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub async fn replace_schedule_items_with_pool(
    pool: &DbPool,
    schedule_id: i64,
    items: Vec<ScheduleItemForm>,
) -> Result<(), ServerFnError> {
    let exists: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ProgramSchedule" WHERE "Id" = ?"#)
            .bind(schedule_id)
            .fetch_one(pool)
            .await?;
    if exists == 0 {
        return Err(ServerFnError::new("[ProgramScheduleId] does not exist."));
    }

    let mut normalized_items = items;
    for item in &mut normalized_items {
        normalize_schedule_item_dependencies(item);
    }
    let padded_filler_ids = padded_filler_ids(pool).await?;
    validate_schedule_items(&normalized_items, &padded_filler_ids)?;

    let mut tx = pool.begin().await?;
    sqlx::query(r#"DELETE FROM "ProgramScheduleItem" WHERE "ProgramScheduleId" = ?"#)
        .bind(schedule_id)
        .execute(&mut *tx)
        .await?;

    let mut ordered = normalized_items;
    ordered.sort_by_key(|item| (item.index, item.id));
    for (index, item) in ordered.iter().enumerate() {
        let fixed_start_time_behavior = if item.start_type == START_TYPE_FIXED {
            item.fixed_start_time_behavior
        } else {
            None
        };
        let start_time = if item.start_type == START_TYPE_FIXED {
            item.start_time.as_deref()
        } else {
            None
        };
        let result = sqlx::query(
            r#"INSERT INTO "ProgramScheduleItem" (
                   "CollectionId", "CollectionType", "CustomTitle", "FakeCollectionKey",
                   "FallbackFillerId", "FillWithGroupMode", "FixedStartTimeBehavior",
                   "GuideMode", "Index", "MarathonBatchSize", "MarathonGroupBy",
                   "MarathonShuffleGroups", "MarathonShuffleItems", "MediaItemId",
                   "MidRollFillerId", "MultiCollectionId", "PlaybackOrder", "PlaylistId",
                   "PostRollFillerId", "PreRollFillerId", "PreferredAudioLanguageCode",
                   "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "ProgramScheduleId",
                   "RerunCollectionId", "SmartCollectionId", "StartTime", "SubtitleMode",
                   "TailFillerId", "SearchQuery", "SearchTitle"
               ) VALUES (?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(item.collection_id)
        .bind(item.collection_type)
        .bind(trimmed_optional(&item.custom_title))
        .bind(item.fallback_filler_id)
        .bind(fill_with_group_mode_for(item))
        .bind(fixed_start_time_behavior)
        .bind(item.guide_mode)
        .bind(i32::try_from(index).unwrap_or(i32::MAX))
        .bind(item.marathon_batch_size)
        .bind(item.marathon_group_by)
        .bind(item.marathon_shuffle_groups)
        .bind(item.marathon_shuffle_items)
        .bind(item.media_item_id)
        .bind(item.mid_roll_filler_id)
        .bind(item.multi_collection_id)
        .bind(item.playback_order)
        .bind(item.playlist_id)
        .bind(item.post_roll_filler_id)
        .bind(item.pre_roll_filler_id)
        .bind(item.preferred_audio_language_code.as_deref())
        .bind(trimmed_optional(&item.preferred_audio_title))
        .bind(item.preferred_subtitle_language_code.as_deref())
        .bind(schedule_id)
        .bind(item.rerun_collection_id)
        .bind(item.smart_collection_id)
        .bind(start_time)
        .bind(item.subtitle_mode)
        .bind(item.tail_filler_id)
        .bind(item.search_query.as_deref())
        .bind(item.search_title.as_deref())
        .execute(&mut *tx)
        .await?;
        let new_item_id = ersatzrs_infra::db::last_insert_id(result);

        match item.playout_mode {
            PLAYOUT_MODE_FLOOD => {
                sqlx::query(r#"INSERT INTO "ProgramScheduleFloodItem" ("Id") VALUES (?)"#)
                    .bind(new_item_id)
                    .execute(&mut *tx)
                    .await?;
            }
            PLAYOUT_MODE_MULTIPLE => {
                let count = if item.multiple_mode == MULTIPLE_MODE_COUNT {
                    item.multiple_count.trim()
                } else {
                    "0"
                };
                sqlx::query(
                    r#"INSERT INTO "ProgramScheduleMultipleItem" ("Id", "Count", "MultipleMode")
                       VALUES (?, ?, ?)"#,
                )
                .bind(new_item_id)
                .bind(count)
                .bind(item.multiple_mode)
                .execute(&mut *tx)
                .await?;
            }
            PLAYOUT_MODE_DURATION => {
                sqlx::query(
                    r#"INSERT INTO "ProgramScheduleDurationItem" ("Id", "PlayoutDuration", "TailMode", "DiscardToFillAttempts")
                       VALUES (?, ?, ?, ?)"#,
                )
                .bind(new_item_id)
                .bind(format_duration(item.duration_hours, item.duration_minutes))
                .bind(item.tail_mode)
                .bind(discard_to_fill_attempts_for(item))
                .execute(&mut *tx)
                .await?;
            }
            _ => {
                sqlx::query(r#"INSERT INTO "ProgramScheduleOneItem" ("Id") VALUES (?)"#)
                    .bind(new_item_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        for watermark_id in &item.watermark_ids {
            sqlx::query(
                r#"INSERT INTO "ProgramScheduleItemWatermark" ("ProgramScheduleItemId", "WatermarkId") VALUES (?, ?)"#,
            )
            .bind(new_item_id)
            .bind(watermark_id)
            .execute(&mut *tx)
            .await?;
        }
        for graphics_element_id in &item.graphics_element_ids {
            sqlx::query(
                r#"INSERT INTO "ProgramScheduleItemGraphicsElement" ("ProgramScheduleItemId", "GraphicsElementId") VALUES (?, ?)"#,
            )
            .bind(new_item_id)
            .bind(graphics_element_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(feature = "ssr")]
async fn padded_filler_ids(pool: &DbPool) -> Result<HashSet<i64>, ServerFnError> {
    let rows: Vec<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "FillerPreset" WHERE "PadToNearestMinute" = ?"#)
            .bind(true)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

#[cfg(feature = "ssr")]
async fn playout_ids_using_schedule(
    pool: &DbPool,
    schedule_id: i64,
) -> Result<Vec<i64>, ServerFnError> {
    Ok(sqlx::query_scalar(
        r#"SELECT "Id" FROM "Playout" WHERE "ProgramScheduleId" = ? ORDER BY "Id""#,
    )
    .bind(schedule_id)
    .fetch_all(pool)
    .await?)
}

#[cfg(feature = "ssr")]
fn enqueue_playout_refreshes(state: &crate::host::state::AppState, playout_ids: Vec<i64>) {
    for playout_id in playout_ids {
        let request = ersatzrs_core::scheduling::PlayoutBuildRequest {
            playout_id,
            mode: ersatzrs_core::scheduling::PlayoutBuildMode::Refresh,
        };
        if let Err(error) = state.playout_build_tx.try_send(request) {
            tracing::warn!(
                playout_id,
                %error,
                "dropping requested playout refresh; periodic scheduler will retry later"
            );
        }
    }
}

fn validate_schedule_items(
    items: &[ScheduleItemForm],
    padded_filler_ids: &HashSet<i64>,
) -> Result<(), ServerFnError> {
    let mut playback_by_key: HashMap<String, i32> = HashMap::new();

    for item in items {
        validate_schedule_item(item, padded_filler_ids)?;
        if let Some(key) = collection_key(item) {
            if let Some(existing) = playback_by_key.insert(key, item.playback_order) {
                if existing != item.playback_order {
                    return Err(ServerFnError::new(
                        "A collection must use the same playback order in every schedule item.",
                    ));
                }
            }
        }
        if item.playback_order == PLAYBACK_ORDER_SHUFFLE_IN_ORDER
            && item.fill_with_group_mode != FILL_WITH_GROUP_MODE_NONE
        {
            return Err(ServerFnError::new(
                "Shuffle In Order cannot be used with Fill With Group Mode.",
            ));
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_schedule_item(
    item: &ScheduleItemForm,
    padded_filler_ids: &HashSet<i64>,
) -> Result<(), ServerFnError> {
    if item.start_type == START_TYPE_FIXED && item.start_time.as_deref().unwrap_or("").is_empty() {
        return Err(ServerFnError::new(
            "Fixed schedule items require a start time.",
        ));
    }
    if item.collection_type == COLLECTION_TYPE_MULTI_COLLECTION
        && matches!(
            item.playback_order,
            PLAYBACK_ORDER_CHRONOLOGICAL
                | PLAYBACK_ORDER_RANDOM
                | PLAYBACK_ORDER_MULTI_EPISODE_SHUFFLE
                | PLAYBACK_ORDER_SEASON_EPISODE
                | PLAYBACK_ORDER_RANDOM_ROTATION
        )
    {
        return Err(ServerFnError::new(
            "Multi Collection items can only use Shuffle or Shuffle In Order.",
        ));
    }
    if item.multiple_mode == MULTIPLE_MODE_PLAYLIST_ITEM_SIZE
        && item.collection_type != COLLECTION_TYPE_PLAYLIST
    {
        return Err(ServerFnError::new(
            "Playlist Item Size can only be used with Playlist schedule items.",
        ));
    }
    if item.playout_mode == PLAYOUT_MODE_MULTIPLE
        && item.multiple_mode == MULTIPLE_MODE_COUNT
        && item.multiple_count.trim().is_empty()
    {
        return Err(ServerFnError::new(
            "Multiple playout mode with Count requires a count expression.",
        ));
    }
    if item.playout_mode == PLAYOUT_MODE_DURATION {
        let minutes = item.duration_hours * 60 + item.duration_minutes;
        if minutes <= 0 || minutes > 24 * 60 {
            return Err(ServerFnError::new(
                "Duration playout mode requires a duration between 1 minute and 24 hours.",
            ));
        }
        if item.discard_to_fill_attempts.unwrap_or(-1) < 0 {
            return Err(ServerFnError::new(
                "Duration playout mode requires discard-to-fill attempts.",
            ));
        }
        if item.tail_mode == TAIL_MODE_FILLER && item.tail_filler_id.is_none() {
            return Err(ServerFnError::new(
                "Tail Mode Filler requires a tail filler preset.",
            ));
        }
    }
    if item.tail_mode != TAIL_MODE_FILLER && item.tail_filler_id.is_some() {
        return Err(ServerFnError::new(
            "Tail filler can only be set when Tail Mode is Filler.",
        ));
    }

    match item.collection_type {
        COLLECTION_TYPE_COLLECTION if item.collection_id.is_none() => {
            return Err(ServerFnError::new("Collection is required."));
        }
        COLLECTION_TYPE_TELEVISION_SHOW
        | COLLECTION_TYPE_TELEVISION_SEASON
        | COLLECTION_TYPE_ARTIST
            if item.media_item_id.is_none() =>
        {
            return Err(ServerFnError::new("Media item is required."));
        }
        COLLECTION_TYPE_MULTI_COLLECTION if item.multi_collection_id.is_none() => {
            return Err(ServerFnError::new("Multi Collection is required."));
        }
        COLLECTION_TYPE_SMART_COLLECTION if item.smart_collection_id.is_none() => {
            return Err(ServerFnError::new("Smart Collection is required."));
        }
        COLLECTION_TYPE_PLAYLIST if item.playlist_id.is_none() => {
            return Err(ServerFnError::new("Playlist is required."));
        }
        COLLECTION_TYPE_RERUN_FIRST_RUN | COLLECTION_TYPE_RERUN_RERUN
            if item.rerun_collection_id.is_none() =>
        {
            return Err(ServerFnError::new("Rerun Collection is required."));
        }
        COLLECTION_TYPE_SEARCH_QUERY
            if item.search_query.as_deref().unwrap_or("").trim().is_empty() =>
        {
            return Err(ServerFnError::new("Search Query is required."));
        }
        _ => {}
    }

    let padded_pre_mid_post = [
        item.pre_roll_filler_id,
        item.mid_roll_filler_id,
        item.post_roll_filler_id,
    ]
    .into_iter()
    .flatten()
    .filter(|id| padded_filler_ids.contains(id))
    .count();
    if padded_pre_mid_post > 1 {
        return Err(ServerFnError::new(
            "Only one pre-, mid-, or post-roll filler can pad to the nearest minute.",
        ));
    }
    if padded_pre_mid_post > 0 && item.fallback_filler_id.is_none() {
        return Err(ServerFnError::new(
            "Padding filler requires a fallback filler.",
        ));
    }

    Ok(())
}

fn collection_key(item: &ScheduleItemForm) -> Option<String> {
    match item.collection_type {
        COLLECTION_TYPE_COLLECTION => item.collection_id.map(|id| format!("collection:{id}")),
        COLLECTION_TYPE_TELEVISION_SHOW => item.media_item_id.map(|id| format!("show:{id}")),
        COLLECTION_TYPE_TELEVISION_SEASON => item.media_item_id.map(|id| format!("season:{id}")),
        COLLECTION_TYPE_ARTIST => item.media_item_id.map(|id| format!("artist:{id}")),
        COLLECTION_TYPE_MULTI_COLLECTION => item
            .multi_collection_id
            .map(|id| format!("multi_collection:{id}")),
        COLLECTION_TYPE_SMART_COLLECTION => item
            .smart_collection_id
            .map(|id| format!("smart_collection:{id}")),
        COLLECTION_TYPE_PLAYLIST => item.playlist_id.map(|id| format!("playlist:{id}")),
        COLLECTION_TYPE_RERUN_FIRST_RUN | COLLECTION_TYPE_RERUN_RERUN => item
            .rerun_collection_id
            .map(|id| format!("rerun:{}:{id}", item.collection_type)),
        COLLECTION_TYPE_SEARCH_QUERY => item
            .search_query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("search:{value}")),
        _ => None,
    }
}

fn normalize_schedule_item_dependencies(item: &mut ScheduleItemForm) {
    if item.collection_type != COLLECTION_TYPE_PLAYLIST
        && item.multiple_mode == MULTIPLE_MODE_PLAYLIST_ITEM_SIZE
    {
        item.multiple_mode = MULTIPLE_MODE_COUNT;
    }

    if matches!(
        item.collection_type,
        COLLECTION_TYPE_PLAYLIST | COLLECTION_TYPE_RERUN_FIRST_RUN | COLLECTION_TYPE_RERUN_RERUN
    ) {
        item.playback_order = PLAYBACK_ORDER_NONE;
    }

    if item.collection_type == COLLECTION_TYPE_MULTI_COLLECTION {
        item.playback_order = PLAYBACK_ORDER_SHUFFLE;
    }

    if item.playback_order != PLAYBACK_ORDER_CHRONOLOGICAL
        && item.multiple_mode == MULTIPLE_MODE_MULTI_EPISODE_GROUP_SIZE
    {
        item.multiple_mode = MULTIPLE_MODE_COUNT;
    }

    if item.playback_order != PLAYBACK_ORDER_MARATHON {
        item.marathon_group_by = MARATHON_GROUP_BY_NONE;
        item.marathon_shuffle_groups = false;
        item.marathon_shuffle_items = false;
        item.marathon_batch_size = None;
    }

    if !can_fill_with_groups(item) {
        item.fill_with_group_mode = FILL_WITH_GROUP_MODE_NONE;
    }
}

fn can_fill_with_groups(item: &ScheduleItemForm) -> bool {
    matches!(
        item.playout_mode,
        PLAYOUT_MODE_MULTIPLE | PLAYOUT_MODE_DURATION
    ) && item.playback_order != PLAYBACK_ORDER_SHUFFLE_IN_ORDER
        && matches!(
            item.collection_type,
            COLLECTION_TYPE_COLLECTION
                | COLLECTION_TYPE_MULTI_COLLECTION
                | COLLECTION_TYPE_SMART_COLLECTION
        )
}

fn fill_with_group_mode_for(item: &ScheduleItemForm) -> i32 {
    if matches!(item.playout_mode, PLAYOUT_MODE_FLOOD | PLAYOUT_MODE_ONE) {
        FILL_WITH_GROUP_MODE_NONE
    } else {
        item.fill_with_group_mode
    }
}

fn discard_to_fill_attempts_for(item: &ScheduleItemForm) -> i32 {
    if matches!(
        item.playback_order,
        PLAYBACK_ORDER_RANDOM | PLAYBACK_ORDER_SHUFFLE
    ) {
        item.discard_to_fill_attempts.unwrap_or(0)
    } else {
        0
    }
}

fn trimmed_optional(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_id_list(s: Option<String>) -> Vec<i64> {
    s.unwrap_or_default()
        .split(',')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse::<i64>().ok())
        .collect()
}

fn parse_duration_parts(duration: Option<String>) -> (i32, i32) {
    let Some(duration) = duration else {
        return (1, 0);
    };
    let parts: Vec<&str> = duration.split(':').collect();
    if parts.len() < 2 {
        return (1, 0);
    }
    let hours = parts[0].parse::<i32>().unwrap_or(1);
    let minutes = parts[1].parse::<i32>().unwrap_or(0);
    (hours, minutes)
}

fn format_duration(hours: i32, minutes: i32) -> String {
    format!("{:02}:{:02}:00", hours.max(0), minutes.clamp(0, 59))
}

#[cfg(feature = "ssr")]
fn read_bool(row: &ersatzrs_infra::db::DbRow, column: &'static str) -> bool {
    use sqlx::Row;
    row.try_get::<bool, _>(column)
        .or_else(|_| row.try_get::<i64, _>(column).map(|value| value != 0))
        .unwrap_or(false)
}

#[cfg(feature = "ssr")]
fn playout_mode_from_row(row: &ersatzrs_infra::db::DbRow) -> i32 {
    use sqlx::Row;
    if row
        .try_get::<Option<i64>, _>("FloodItemId")
        .ok()
        .flatten()
        .is_some()
    {
        1
    } else if row
        .try_get::<Option<i64>, _>("OneItemId")
        .ok()
        .flatten()
        .is_some()
    {
        2
    } else if row
        .try_get::<Option<i64>, _>("MultipleItemId")
        .ok()
        .flatten()
        .is_some()
    {
        3
    } else {
        4
    }
}

#[cfg(feature = "ssr")]
fn item_collection_name(row: &ersatzrs_infra::db::DbRow, id: i64) -> String {
    use sqlx::Row;
    for column in [
        "CollectionName",
        "MultiCollectionName",
        "SmartCollectionName",
        "RerunCollectionName",
        "PlaylistName",
        "ArtistName",
        "ShowName",
        "SeasonName",
    ] {
        if let Some(value) = row.try_get::<Option<String>, _>(column).ok().flatten() {
            if !value.trim().is_empty() {
                return value;
            }
        }
    }

    if let Some(value) = row
        .try_get::<Option<String>, _>("SearchTitle")
        .ok()
        .flatten()
    {
        if !value.trim().is_empty() {
            return value;
        }
    }
    if let Some(value) = row
        .try_get::<Option<String>, _>("SearchQuery")
        .ok()
        .flatten()
    {
        if !value.trim().is_empty() {
            return value;
        }
    }
    if let Some(media_item_id) = row.try_get::<Option<i64>, _>("MediaItemId").ok().flatten() {
        return format!("Media Item #{media_item_id}");
    }
    format!("Schedule Item #{id}")
}

#[cfg(feature = "ssr")]
async fn save_config_value(pool: &DbPool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    let result = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
        .bind(value)
        .bind(key)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(key)
            .bind(value)
            .execute(pool)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_program_schedule_table(pool: &DbPool) {
        sqlx::query(
            r#"CREATE TABLE "ProgramSchedule" (
                "Id" INTEGER PRIMARY KEY,
                "Name" TEXT NOT NULL,
                "KeepMultiPartEpisodesTogether" BOOLEAN NOT NULL DEFAULT 0,
                "TreatCollectionsAsShows" BOOLEAN NOT NULL DEFAULT 0,
                "ShuffleScheduleItems" BOOLEAN NOT NULL DEFAULT 0,
                "RandomStartPoint" BOOLEAN NOT NULL DEFAULT 0,
                "FixedStartTimeBehavior" INTEGER NOT NULL DEFAULT 0
            )"#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    fn valid_item() -> ScheduleItemForm {
        ScheduleItemForm {
            collection_id: Some(1),
            collection_name: "Collection".to_string(),
            ..ScheduleItemForm::default()
        }
    }

    #[sqlx::test]
    async fn padded_filler_ids_only_returns_true_flags(pool: DbPool) {
        sqlx::query(
            r#"CREATE TABLE "FillerPreset" (
                "Id" INTEGER PRIMARY KEY,
                "PadToNearestMinute" BOOLEAN NULL
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(r#"INSERT INTO "FillerPreset" ("Id", "PadToNearestMinute") VALUES (?, ?)"#)
            .bind(1_i64)
            .bind(false)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "FillerPreset" ("Id", "PadToNearestMinute") VALUES (?, ?)"#)
            .bind(2_i64)
            .bind(true)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "FillerPreset" ("Id", "PadToNearestMinute") VALUES (?, NULL)"#)
            .bind(3_i64)
            .execute(&pool)
            .await
            .unwrap();

        let ids = padded_filler_ids(&pool).await.unwrap();

        assert_eq!(ids, HashSet::from([2]));
    }

    #[sqlx::test]
    async fn load_language_options_matches_media_language_codes(pool: DbPool) {
        sqlx::query(
            r#"CREATE TABLE "MediaStream" (
                "Id" INTEGER PRIMARY KEY,
                "Language" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"CREATE TABLE "Subtitle" (
                "Id" INTEGER PRIMARY KEY,
                "Language" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"CREATE TABLE "Channel" (
                "Id" INTEGER PRIMARY KEY,
                "PreferredAudioLanguageCode" TEXT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(r#"INSERT INTO "MediaStream" ("Id", "Language") VALUES (1, 'eng')"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "Subtitle" ("Id", "Language") VALUES (1, 'spa')"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "Channel" ("Id", "PreferredAudioLanguageCode") VALUES (1, 'zzx')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let options = load_language_options(&pool).await.unwrap();

        assert!(options.contains(&ScheduleLanguageOption {
            code: "eng".to_string(),
            name: "English".to_string()
        }));
        assert!(options.contains(&ScheduleLanguageOption {
            code: "spa".to_string(),
            name: "Spanish".to_string()
        }));
        assert!(options.contains(&ScheduleLanguageOption {
            code: "zzx".to_string(),
            name: "zzx".to_string()
        }));
    }

    #[sqlx::test]
    async fn save_config_value_updates_or_inserts_without_backend_specific_upsert(pool: DbPool) {
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT PRIMARY KEY,
                "Value" TEXT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        save_config_value(&pool, "pages.schedules.page_size", "25")
            .await
            .unwrap();
        save_config_value(&pool, "pages.schedules.page_size", "50")
            .await
            .unwrap();

        let value: String =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind("pages.schedules.page_size")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(value, "50");
    }

    #[sqlx::test]
    async fn save_schedule_enforces_unique_and_max_length_names(pool: DbPool) {
        create_program_schedule_table(&pool).await;
        sqlx::query(
            r#"INSERT INTO "ProgramSchedule" (
                "Id", "Name", "KeepMultiPartEpisodesTogether", "TreatCollectionsAsShows",
                "ShuffleScheduleItems", "RandomStartPoint", "FixedStartTimeBehavior"
            ) VALUES (1, 'Existing', 1, 0, 0, 0, 0)"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let duplicate = save_schedule_with_pool(
            &pool,
            ScheduleForm {
                name: "Existing".to_string(),
                ..ScheduleForm::default()
            },
        )
        .await
        .unwrap_err();
        assert!(duplicate.to_string().contains("unique"));

        let too_long = save_schedule_with_pool(
            &pool,
            ScheduleForm {
                name: "x".repeat(51),
                ..ScheduleForm::default()
            },
        )
        .await
        .unwrap_err();
        assert!(too_long.to_string().contains("50"));
    }

    #[sqlx::test]
    async fn save_schedule_persists_treat_collections_only_when_keep_multi_part_is_enabled(
        pool: DbPool,
    ) {
        create_program_schedule_table(&pool).await;

        let id = save_schedule_with_pool(
            &pool,
            ScheduleForm {
                name: "Schedule".to_string(),
                keep_multi_part_episodes_together: false,
                treat_collections_as_shows: true,
                ..ScheduleForm::default()
            },
        )
        .await
        .unwrap();

        let stored: bool = sqlx::query_scalar(
            r#"SELECT "TreatCollectionsAsShows" FROM "ProgramSchedule" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!stored);
    }

    #[test]
    fn validation_requires_fixed_start_time() {
        let mut item = valid_item();
        item.start_type = START_TYPE_FIXED;
        item.start_time = None;

        let err = validate_schedule_items(&[item], &HashSet::new()).unwrap_err();

        assert!(err.to_string().contains("start time"));
    }

    #[test]
    fn validation_rejects_invalid_multi_collection_playback_order() {
        let mut item = valid_item();
        item.collection_type = COLLECTION_TYPE_MULTI_COLLECTION;
        item.collection_id = None;
        item.multi_collection_id = Some(1);
        item.playback_order = PLAYBACK_ORDER_CHRONOLOGICAL;

        let err = validate_schedule_items(&[item], &HashSet::new()).unwrap_err();

        assert!(err.to_string().contains("Multi Collection"));
    }

    #[test]
    fn validation_requires_multiple_count_expression() {
        let mut item = valid_item();
        item.playout_mode = PLAYOUT_MODE_MULTIPLE;
        item.multiple_mode = MULTIPLE_MODE_COUNT;
        item.multiple_count.clear();

        let err = validate_schedule_items(&[item], &HashSet::new()).unwrap_err();

        assert!(err.to_string().contains("Count"));
    }

    #[test]
    fn validation_requires_duration_bounds() {
        let mut item = valid_item();
        item.playout_mode = PLAYOUT_MODE_DURATION;
        item.duration_hours = 25;

        let err = validate_schedule_items(&[item], &HashSet::new()).unwrap_err();

        assert!(err.to_string().contains("24 hours"));
    }

    #[test]
    fn validation_rejects_mixed_playback_order_for_same_collection() {
        let first = valid_item();
        let mut second = valid_item();
        second.playback_order = PLAYBACK_ORDER_RANDOM;

        let err = validate_schedule_items(&[first, second], &HashSet::new()).unwrap_err();

        assert!(err.to_string().contains("same playback order"));
    }

    #[test]
    fn validation_requires_fallback_for_padded_filler() {
        let mut item = valid_item();
        item.pre_roll_filler_id = Some(10);
        let padded = HashSet::from([10]);

        let err = validate_schedule_items(&[item], &padded).unwrap_err();

        assert!(err.to_string().contains("fallback filler"));
    }

    #[test]
    fn normalization_matches_schedule_item_view_model_dependency_rules() {
        let mut playlist = valid_item();
        playlist.collection_type = COLLECTION_TYPE_PLAYLIST;
        playlist.collection_id = None;
        playlist.playlist_id = Some(3);
        playlist.playback_order = PLAYBACK_ORDER_SHUFFLE;
        playlist.multiple_mode = MULTIPLE_MODE_COLLECTION_SIZE;
        playlist.marathon_group_by = 1;
        playlist.marathon_shuffle_groups = true;
        playlist.marathon_shuffle_items = true;
        playlist.marathon_batch_size = Some(2);
        playlist.fill_with_group_mode = 1;
        normalize_schedule_item_dependencies(&mut playlist);
        assert_eq!(playlist.playback_order, PLAYBACK_ORDER_NONE);
        assert_eq!(playlist.marathon_group_by, MARATHON_GROUP_BY_NONE);
        assert!(!playlist.marathon_shuffle_groups);
        assert!(!playlist.marathon_shuffle_items);
        assert_eq!(playlist.marathon_batch_size, None);
        assert_eq!(playlist.fill_with_group_mode, FILL_WITH_GROUP_MODE_NONE);

        let mut multi_collection = valid_item();
        multi_collection.collection_type = COLLECTION_TYPE_MULTI_COLLECTION;
        multi_collection.collection_id = None;
        multi_collection.multi_collection_id = Some(7);
        multi_collection.playback_order = PLAYBACK_ORDER_CHRONOLOGICAL;
        multi_collection.multiple_mode = MULTIPLE_MODE_PLAYLIST_ITEM_SIZE;
        normalize_schedule_item_dependencies(&mut multi_collection);
        assert_eq!(multi_collection.playback_order, PLAYBACK_ORDER_SHUFFLE);
        assert_eq!(multi_collection.multiple_mode, MULTIPLE_MODE_COUNT);
    }

    #[test]
    fn schedule_item_form_deserializes_default_multi_selects_when_omitted() {
        let json = serde_json::json!({
            "id": 0,
            "index": 0,
            "start_type": START_TYPE_DYNAMIC,
            "start_time": null,
            "fixed_start_time_behavior": null,
            "collection_type": COLLECTION_TYPE_COLLECTION,
            "collection_id": 1,
            "media_item_id": null,
            "multi_collection_id": null,
            "smart_collection_id": null,
            "rerun_collection_id": null,
            "playlist_id": null,
            "search_title": null,
            "search_query": null,
            "collection_name": "Collection",
            "playback_order": PLAYBACK_ORDER_SHUFFLE,
            "marathon_group_by": MARATHON_GROUP_BY_NONE,
            "marathon_shuffle_groups": false,
            "marathon_shuffle_items": false,
            "marathon_batch_size": null,
            "fill_with_group_mode": FILL_WITH_GROUP_MODE_NONE,
            "playout_mode": PLAYOUT_MODE_ONE,
            "multiple_mode": MULTIPLE_MODE_COUNT,
            "multiple_count": "1",
            "duration_hours": 1,
            "duration_minutes": 0,
            "tail_mode": TAIL_MODE_NONE,
            "discard_to_fill_attempts": 0,
            "custom_title": "",
            "guide_mode": GUIDE_MODE_NORMAL,
            "pre_roll_filler_id": null,
            "mid_roll_filler_id": null,
            "post_roll_filler_id": null,
            "tail_filler_id": null,
            "fallback_filler_id": null,
            "preferred_audio_language_code": null,
            "preferred_audio_title": "",
            "preferred_subtitle_language_code": null,
            "subtitle_mode": null
        });

        let item: ScheduleItemForm = serde_json::from_value(json).unwrap();

        assert!(item.watermark_ids.is_empty());
        assert!(item.graphics_element_ids.is_empty());
    }
}
