//! Filler preset admin functions. Ports ErsatzTV's
//! `Pages/FillerPresets.razor`, `Pages/FillerPresetEditor.razor`, and
//! `ErsatzTV.Application/Filler/*`.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(all(test, feature = "ssr"))]
const FILLER_PRESETS_PAGE_SIZE_KEY: &str = "pages.filler_presets.page_size";
const FILLER_KIND_PRE_ROLL: i32 = 1;
#[cfg(test)]
const FILLER_KIND_MID_ROLL: i32 = 2;
#[cfg(test)]
const FILLER_KIND_POST_ROLL: i32 = 3;
#[cfg(test)]
const FILLER_KIND_TAIL: i32 = 4;
#[cfg(test)]
const FILLER_KIND_FALLBACK: i32 = 5;
#[cfg(test)]
const FILLER_MODE_NONE: i32 = 0;
const FILLER_MODE_DURATION: i32 = 1;
#[cfg(test)]
const FILLER_MODE_COUNT: i32 = 2;
#[cfg(test)]
const FILLER_MODE_PAD: i32 = 3;
#[cfg(test)]
const FILLER_MODE_RANDOM_COUNT: i32 = 4;
const COLLECTION_TYPE_COLLECTION: i32 = 0;
#[cfg(test)]
const COLLECTION_TYPE_TELEVISION_SHOW: i32 = 1;
#[cfg(test)]
const COLLECTION_TYPE_TELEVISION_SEASON: i32 = 2;
#[cfg(test)]
const COLLECTION_TYPE_ARTIST: i32 = 3;
#[cfg(test)]
const COLLECTION_TYPE_MULTI_COLLECTION: i32 = 4;
#[cfg(test)]
const COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
#[cfg(test)]
const COLLECTION_TYPE_PLAYLIST: i32 = 6;

#[cfg(all(test, feature = "ssr"))]
type DbPool = sqlx::Pool<ersatzrs_infra::db::Db>;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FillerPresetListRow {
    pub id: i64,
    pub name: String,
    pub filler_kind: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FillerPresetSelectOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FillerPresetPlaylistOption {
    pub id: i64,
    pub name: String,
    pub playlist_group_id: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FillerPresetEditorOptions {
    pub collections: Vec<FillerPresetSelectOption>,
    pub multi_collections: Vec<FillerPresetSelectOption>,
    pub smart_collections: Vec<FillerPresetSelectOption>,
    pub television_shows: Vec<FillerPresetSelectOption>,
    pub television_seasons: Vec<FillerPresetSelectOption>,
    pub artists: Vec<FillerPresetSelectOption>,
    pub playlist_groups: Vec<FillerPresetSelectOption>,
    pub playlists: Vec<FillerPresetPlaylistOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillerPresetForm {
    pub id: i64,
    pub name: String,
    pub filler_kind: i32,
    pub filler_mode: i32,
    pub duration_hours: i32,
    pub duration_minutes: i32,
    pub duration_seconds: i32,
    pub count: Option<i32>,
    pub pad_to_nearest_minute: Option<i32>,
    pub allow_watermarks: bool,
    pub collection_type: i32,
    pub collection_id: Option<i64>,
    pub media_item_id: Option<i64>,
    pub multi_collection_id: Option<i64>,
    pub smart_collection_id: Option<i64>,
    pub playlist_id: Option<i64>,
    pub expression: String,
    pub use_chapters_as_media_items: bool,
}

impl Default for FillerPresetForm {
    fn default() -> Self {
        Self {
            id: 0,
            name: "New Filler Preset".to_string(),
            filler_kind: FILLER_KIND_PRE_ROLL,
            filler_mode: FILLER_MODE_DURATION,
            duration_hours: 0,
            duration_minutes: 0,
            duration_seconds: 1,
            count: None,
            pad_to_nearest_minute: None,
            allow_watermarks: false,
            collection_type: COLLECTION_TYPE_COLLECTION,
            collection_id: None,
            media_item_id: None,
            multi_collection_id: None,
            smart_collection_id: None,
            playlist_id: None,
            expression: String::new(),
            use_chapters_as_media_items: false,
        }
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn filler_form_into_admin(
    form: FillerPresetForm,
) -> ersatzrs_app::commands::FillerPresetAdminForm {
    ersatzrs_app::commands::FillerPresetAdminForm {
        id: form.id,
        name: form.name,
        filler_kind: form.filler_kind,
        filler_mode: form.filler_mode,
        duration_hours: form.duration_hours,
        duration_minutes: form.duration_minutes,
        duration_seconds: form.duration_seconds,
        count: form.count,
        pad_to_nearest_minute: form.pad_to_nearest_minute,
        allow_watermarks: form.allow_watermarks,
        collection_type: form.collection_type,
        collection_id: form.collection_id,
        media_item_id: form.media_item_id,
        multi_collection_id: form.multi_collection_id,
        smart_collection_id: form.smart_collection_id,
        playlist_id: form.playlist_id,
        expression: form.expression,
        use_chapters_as_media_items: form.use_chapters_as_media_items,
    }
}

#[cfg(feature = "ssr")]
fn filler_form_from_admin(form: ersatzrs_app::commands::FillerPresetAdminForm) -> FillerPresetForm {
    FillerPresetForm {
        id: form.id,
        name: form.name,
        filler_kind: form.filler_kind,
        filler_mode: form.filler_mode,
        duration_hours: form.duration_hours,
        duration_minutes: form.duration_minutes,
        duration_seconds: form.duration_seconds,
        count: form.count,
        pad_to_nearest_minute: form.pad_to_nearest_minute,
        allow_watermarks: form.allow_watermarks,
        collection_type: form.collection_type,
        collection_id: form.collection_id,
        media_item_id: form.media_item_id,
        multi_collection_id: form.multi_collection_id,
        smart_collection_id: form.smart_collection_id,
        playlist_id: form.playlist_id,
        expression: form.expression,
        use_chapters_as_media_items: form.use_chapters_as_media_items,
    }
}

#[server(prefix = "/admin-api")]
pub async fn get_filler_presets_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_filler_presets_page_size()
        .await
        .map_err(ServerFnError::new)
}

#[server(prefix = "/admin-api")]
pub async fn list_filler_presets(
    page: u32,
    page_size: u32,
) -> Result<(Vec<FillerPresetListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_filler_presets(page, page_size)
        .await
        .map(|(rows, total)| {
            (
                rows.into_iter()
                    .map(|row| FillerPresetListRow {
                        id: row.id,
                        name: row.name,
                        filler_kind: row.filler_kind,
                    })
                    .collect(),
                total,
            )
        })
        .map_err(ServerFnError::new)
}

#[server(prefix = "/admin-api")]
pub async fn get_filler_preset_form(id: i64) -> Result<Option<FillerPresetForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_filler_preset(id)
        .await
        .map(|form| form.map(filler_form_from_admin))
        .map_err(ServerFnError::new)
}

#[server(prefix = "/admin-api")]
pub async fn get_filler_preset_editor_options() -> Result<FillerPresetEditorOptions, ServerFnError>
{
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_filler_editor_options()
        .await
        .map(|options| FillerPresetEditorOptions {
            collections: options
                .collections
                .into_iter()
                .map(select_from_read)
                .collect(),
            multi_collections: options
                .multi_collections
                .into_iter()
                .map(select_from_read)
                .collect(),
            smart_collections: options
                .smart_collections
                .into_iter()
                .map(select_from_read)
                .collect(),
            television_shows: options
                .television_shows
                .into_iter()
                .map(select_from_read)
                .collect(),
            television_seasons: options
                .television_seasons
                .into_iter()
                .map(select_from_read)
                .collect(),
            artists: options.artists.into_iter().map(select_from_read).collect(),
            playlist_groups: options
                .playlist_groups
                .into_iter()
                .map(select_from_read)
                .collect(),
            playlists: options
                .playlists
                .into_iter()
                .map(|row| FillerPresetPlaylistOption {
                    id: row.id,
                    name: row.name,
                    playlist_group_id: row.playlist_group_id,
                })
                .collect(),
        })
        .map_err(ServerFnError::new)
}

#[server(prefix = "/admin-api")]
pub async fn save_filler_preset(form: FillerPresetForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .save_filler_preset(ersatzrs_app::commands::SaveFillerPreset {
            form: filler_form_into_admin(form),
        })
        .await
        .map_err(ServerFnError::new)
}

#[server(prefix = "/admin-api")]
pub async fn delete_filler_preset(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_filler_preset(ersatzrs_app::commands::DeleteFillerPreset { id })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
fn select_from_read(row: ersatzrs_app::queries::FillerSelectRead) -> FillerPresetSelectOption {
    FillerPresetSelectOption {
        id: row.id,
        name: row.name,
    }
}

#[cfg(all(test, feature = "ssr"))]
pub async fn get_page_size_with_pool(pool: &DbPool) -> Result<u32, ServerFnError> {
    let value: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(FILLER_PRESETS_PAGE_SIZE_KEY)
            .fetch_optional(pool)
            .await?;

    Ok(value
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10))
}

#[cfg(all(test, feature = "ssr"))]
pub async fn list_filler_presets_with_pool(
    pool: &DbPool,
    page: u32,
    page_size: u32,
) -> Result<(Vec<FillerPresetListRow>, u64), ServerFnError> {
    use sqlx::Row;

    save_config_value(pool, FILLER_PRESETS_PAGE_SIZE_KEY, &page_size.to_string()).await?;

    let offset = i64::from(page) * i64::from(page_size);
    let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FillerPreset""#)
        .fetch_one(pool)
        .await?;
    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name", "FillerKind"
           FROM "FillerPreset"
           ORDER BY "Name" COLLATE NOCASE
           LIMIT ? OFFSET ?"#,
    )
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let rows = rows
        .iter()
        .map(|row| FillerPresetListRow {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
            filler_kind: row.try_get("FillerKind").unwrap_or(0),
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((rows, total.max(0) as u64))
}

#[cfg(all(test, feature = "ssr"))]
pub async fn get_filler_preset_form_with_pool(
    pool: &DbPool,
    id: i64,
) -> Result<Option<FillerPresetForm>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(r#"SELECT * FROM "FillerPreset" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|row| {
        let duration_text: Option<String> = row.try_get("Duration").unwrap_or_default();
        let duration = duration_text.as_deref().and_then(parse_duration);
        FillerPresetForm {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
            filler_kind: row.try_get("FillerKind").unwrap_or(FILLER_KIND_PRE_ROLL),
            filler_mode: row.try_get("FillerMode").unwrap_or(FILLER_MODE_DURATION),
            duration_hours: duration.map_or(0, |seconds| seconds / 3600),
            duration_minutes: duration.map_or(0, |seconds| (seconds % 3600) / 60),
            duration_seconds: duration.map_or(0, |seconds| seconds % 60),
            count: row.try_get("Count").unwrap_or_default(),
            pad_to_nearest_minute: row.try_get("PadToNearestMinute").unwrap_or_default(),
            allow_watermarks: row.try_get("AllowWatermarks").unwrap_or(false),
            collection_type: row
                .try_get("CollectionType")
                .unwrap_or(COLLECTION_TYPE_COLLECTION),
            collection_id: row.try_get("CollectionId").unwrap_or_default(),
            media_item_id: row.try_get("MediaItemId").unwrap_or_default(),
            multi_collection_id: row.try_get("MultiCollectionId").unwrap_or_default(),
            smart_collection_id: row.try_get("SmartCollectionId").unwrap_or_default(),
            playlist_id: row.try_get("PlaylistId").unwrap_or_default(),
            expression: row.try_get("Expression").unwrap_or_default(),
            use_chapters_as_media_items: row.try_get("UseChaptersAsMediaItems").unwrap_or(false),
        }
    }))
}

#[cfg(all(test, feature = "ssr"))]
pub async fn get_filler_preset_editor_options_with_pool(
    pool: &DbPool,
) -> Result<FillerPresetEditorOptions, ServerFnError> {
    Ok(FillerPresetEditorOptions {
        collections: select_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "Collection" ORDER BY "Name" COLLATE NOCASE"#).await?,
        multi_collections: select_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "MultiCollection" ORDER BY "Name" COLLATE NOCASE"#).await?,
        smart_collections: select_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "SmartCollection" ORDER BY "Name" COLLATE NOCASE"#).await?,
        television_shows: select_options(pool, r#"SELECT s."Id", COALESCE(sm."Title", '') AS "Name" FROM "Show" s LEFT JOIN "ShowMetadata" sm ON sm."ShowId" = s."Id" ORDER BY sm."Title" COLLATE NOCASE, s."Id""#).await?,
        television_seasons: select_options(pool, r#"SELECT s."Id", COALESCE(sm."Title", '') AS "Name" FROM "Season" s LEFT JOIN "SeasonMetadata" sm ON sm."SeasonId" = s."Id" ORDER BY sm."Title" COLLATE NOCASE, s."Id""#).await?,
        artists: select_options(pool, r#"SELECT a."Id", COALESCE(am."Title", '') AS "Name" FROM "Artist" a LEFT JOIN "ArtistMetadata" am ON am."ArtistId" = a."Id" ORDER BY am."Title" COLLATE NOCASE, a."Id""#).await?,
        playlist_groups: select_options(pool, r#"SELECT "Id", COALESCE("Name", '') AS "Name" FROM "PlaylistGroup" ORDER BY COALESCE("IsSystem", 0) DESC, "Name" COLLATE NOCASE"#).await?,
        playlists: playlist_options(pool).await?,
    })
}

#[cfg(all(test, feature = "ssr"))]
pub async fn save_filler_preset_with_pool(
    pool: &DbPool,
    mut form: FillerPresetForm,
) -> Result<i64, ServerFnError> {
    normalize_filler_preset_form(&mut form);
    validate_filler_preset(pool, &form).await?;

    let duration = duration_seconds(&form).map(format_duration);
    let expression = (form.filler_kind == FILLER_KIND_MID_ROLL)
        .then(|| form.expression.trim().to_string())
        .filter(|value| !value.is_empty());
    let use_chapters = form.filler_kind != FILLER_KIND_FALLBACK && form.use_chapters_as_media_items;

    if form.id == 0 {
        let result = sqlx::query(
            r#"INSERT INTO "FillerPreset" (
                "Name", "FillerKind", "FillerMode", "Duration", "Count", "PadToNearestMinute",
                "AllowWatermarks", "CollectionType", "CollectionId", "MediaItemId",
                "MultiCollectionId", "SmartCollectionId", "PlaylistId", "Expression",
                "UseChaptersAsMediaItems"
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(form.name.trim())
        .bind(form.filler_kind)
        .bind(form.filler_mode)
        .bind(duration)
        .bind(form.count)
        .bind(form.pad_to_nearest_minute)
        .bind(form.allow_watermarks)
        .bind(form.collection_type)
        .bind(form.collection_id)
        .bind(form.media_item_id)
        .bind(form.multi_collection_id)
        .bind(form.smart_collection_id)
        .bind(form.playlist_id)
        .bind(expression)
        .bind(use_chapters)
        .execute(pool)
        .await?;
        Ok(ersatzrs_infra::db::last_insert_id(result))
    } else {
        let exists: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FillerPreset" WHERE "Id" = ?"#)
                .bind(form.id)
                .fetch_one(pool)
                .await?;
        if exists == 0 {
            return Err(ServerFnError::new("Filler preset does not exist"));
        }

        sqlx::query(
            r#"UPDATE "FillerPreset"
               SET "Name" = ?, "FillerKind" = ?, "FillerMode" = ?, "Duration" = ?,
                   "Count" = ?, "PadToNearestMinute" = ?, "AllowWatermarks" = ?,
                   "CollectionType" = ?, "CollectionId" = ?, "MediaItemId" = ?,
                   "MultiCollectionId" = ?, "SmartCollectionId" = ?, "PlaylistId" = ?,
                   "Expression" = ?, "UseChaptersAsMediaItems" = ?
               WHERE "Id" = ?"#,
        )
        .bind(form.name.trim())
        .bind(form.filler_kind)
        .bind(form.filler_mode)
        .bind(duration)
        .bind(form.count)
        .bind(form.pad_to_nearest_minute)
        .bind(form.allow_watermarks)
        .bind(form.collection_type)
        .bind(form.collection_id)
        .bind(form.media_item_id)
        .bind(form.multi_collection_id)
        .bind(form.smart_collection_id)
        .bind(form.playlist_id)
        .bind(expression)
        .bind(use_chapters)
        .bind(form.id)
        .execute(pool)
        .await?;
        Ok(form.id)
    }
}

#[cfg(all(test, feature = "ssr"))]
pub async fn delete_filler_preset_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    let result = sqlx::query(r#"DELETE FROM "FillerPreset" WHERE "Id" = ?"#)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerFnError::new(format!(
            "FillerPreset {id} does not exist."
        )));
    }
    Ok(())
}

#[cfg(all(test, feature = "ssr"))]
async fn select_options(
    pool: &DbPool,
    sql: &str,
) -> Result<Vec<FillerPresetSelectOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(ersatzrs_infra::db::assert_sql_safe(sql))
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| FillerPresetSelectOption {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn playlist_options(pool: &DbPool) -> Result<Vec<FillerPresetPlaylistOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Id", COALESCE("Name", '') AS "Name", "PlaylistGroupId"
           FROM "Playlist"
           ORDER BY "Name" COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|row| FillerPresetPlaylistOption {
            id: row.try_get("Id").unwrap_or(0),
            name: row.try_get("Name").unwrap_or_default(),
            playlist_group_id: row.try_get("PlaylistGroupId").unwrap_or(0),
        })
        .collect())
}

#[cfg(all(test, feature = "ssr"))]
async fn save_config_value(pool: &DbPool, key: &str, value: &str) -> Result<(), ServerFnError> {
    let updated = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
        .bind(value)
        .bind(key)
        .execute(pool)
        .await?;
    if updated.rows_affected() == 0 {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(key)
            .bind(value)
            .execute(pool)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
fn normalize_filler_preset_form(form: &mut FillerPresetForm) {
    form.name = form.name.trim().to_string();
    if matches!(form.filler_kind, FILLER_KIND_FALLBACK | FILLER_KIND_TAIL) {
        form.filler_mode = FILLER_MODE_NONE;
    }
    if form.collection_type == COLLECTION_TYPE_PLAYLIST {
        form.filler_mode = FILLER_MODE_COUNT;
    }
    if form.filler_kind != FILLER_KIND_MID_ROLL {
        form.expression.clear();
    }
    if form.filler_kind == FILLER_KIND_FALLBACK {
        form.use_chapters_as_media_items = false;
    }

    if form.filler_mode != FILLER_MODE_DURATION {
        form.duration_hours = 0;
        form.duration_minutes = 0;
        form.duration_seconds = 0;
    }
    if !matches!(
        form.filler_mode,
        FILLER_MODE_COUNT | FILLER_MODE_RANDOM_COUNT
    ) {
        form.count = None;
    }
    if form.filler_mode != FILLER_MODE_PAD {
        form.pad_to_nearest_minute = None;
    }

    if form.collection_type != COLLECTION_TYPE_COLLECTION {
        form.collection_id = None;
    }
    if !matches!(
        form.collection_type,
        COLLECTION_TYPE_TELEVISION_SHOW
            | COLLECTION_TYPE_TELEVISION_SEASON
            | COLLECTION_TYPE_ARTIST
    ) {
        form.media_item_id = None;
    }
    if form.collection_type != COLLECTION_TYPE_MULTI_COLLECTION {
        form.multi_collection_id = None;
    }
    if form.collection_type != COLLECTION_TYPE_SMART_COLLECTION {
        form.smart_collection_id = None;
    }
    if form.collection_type != COLLECTION_TYPE_PLAYLIST {
        form.playlist_id = None;
    }
}

#[cfg(all(test, feature = "ssr"))]
async fn validate_filler_preset(
    pool: &DbPool,
    form: &FillerPresetForm,
) -> Result<(), ServerFnError> {
    validate_filler_preset_shape(form)?;

    let duplicate: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "FillerPreset" WHERE "Name" = ? AND "Id" != ?"#)
            .bind(&form.name)
            .bind(form.id)
            .fetch_one(pool)
            .await?;
    if duplicate > 0 {
        return Err(ServerFnError::new("Filler preset name must be unique"));
    }

    Ok(())
}

#[cfg(test)]
fn validate_filler_preset_shape(form: &FillerPresetForm) -> Result<(), ServerFnError> {
    if form.name.is_empty() {
        return Err(ServerFnError::new("Name must not be empty"));
    }
    if form.name.chars().count() > 50 {
        return Err(ServerFnError::new("Name must be 50 characters or fewer"));
    }
    if !matches!(
        form.filler_kind,
        FILLER_KIND_PRE_ROLL
            | FILLER_KIND_MID_ROLL
            | FILLER_KIND_POST_ROLL
            | FILLER_KIND_TAIL
            | FILLER_KIND_FALLBACK
    ) {
        return Err(ServerFnError::new("Filler kind must not be None"));
    }
    if !matches!(form.filler_kind, FILLER_KIND_FALLBACK | FILLER_KIND_TAIL)
        && form.filler_mode == FILLER_MODE_NONE
    {
        return Err(ServerFnError::new("Filler mode must not be None"));
    }
    if form.filler_mode == FILLER_MODE_COUNT && form.count.is_none_or(|count| count < 0) {
        return Err(ServerFnError::new(
            "Count must be greater than or equal to 0",
        ));
    }
    if form.filler_mode == FILLER_MODE_DURATION && duration_seconds(form).is_none() {
        return Err(ServerFnError::new("Duration must not be empty"));
    }
    if form.filler_mode == FILLER_MODE_PAD && form.pad_to_nearest_minute.is_none() {
        return Err(ServerFnError::new(
            "Pad To Nearest Minute must not be empty",
        ));
    }
    if form.filler_kind == FILLER_KIND_FALLBACK && form.use_chapters_as_media_items {
        return Err(ServerFnError::new(
            "Use Chapters As Media Items must not be enabled for fallback filler",
        ));
    }
    match form.collection_type {
        COLLECTION_TYPE_COLLECTION if form.collection_id.is_none() => {
            Err(ServerFnError::new("Collection must not be empty"))
        }
        COLLECTION_TYPE_MULTI_COLLECTION if form.multi_collection_id.is_none() => {
            Err(ServerFnError::new("Multi Collection must not be empty"))
        }
        COLLECTION_TYPE_SMART_COLLECTION if form.smart_collection_id.is_none() => {
            Err(ServerFnError::new("Smart Collection must not be empty"))
        }
        COLLECTION_TYPE_TELEVISION_SHOW
        | COLLECTION_TYPE_TELEVISION_SEASON
        | COLLECTION_TYPE_ARTIST
            if form.media_item_id.is_none() =>
        {
            Err(ServerFnError::new("Media item must not be empty"))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
fn duration_seconds(form: &FillerPresetForm) -> Option<i32> {
    if form.filler_mode != FILLER_MODE_DURATION {
        return None;
    }
    Some(
        ((form.duration_hours.clamp(0, 24) * 3600)
            + (form.duration_minutes.clamp(0, 59) * 60)
            + form.duration_seconds.clamp(0, 59))
        .clamp(1, 24 * 3600),
    )
}

#[cfg(test)]
fn format_duration(total_seconds: i32) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(all(test, feature = "ssr"))]
fn parse_duration(value: &str) -> Option<i32> {
    let (_, time_text) = value.rsplit_once('.').unwrap_or(("", value));
    let mut parts = time_text.split(':');
    let hours = parts.next()?.parse::<i32>().ok()?;
    let minutes = parts.next()?.parse::<i32>().ok()?;
    let seconds_text = parts.next()?;
    let seconds = seconds_text
        .split_once('.')
        .map_or(seconds_text, |(whole, _)| whole)
        .parse::<i32>()
        .ok()?;
    Some(hours * 3600 + minutes * 60 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_normalization_forces_none_mode_and_disables_chapters() {
        let mut form = FillerPresetForm {
            filler_kind: FILLER_KIND_FALLBACK,
            filler_mode: FILLER_MODE_COUNT,
            count: Some(2),
            use_chapters_as_media_items: true,
            expression: "num == 1".to_string(),
            ..FillerPresetForm::default()
        };

        normalize_filler_preset_form(&mut form);

        assert_eq!(form.filler_mode, FILLER_MODE_NONE);
        assert_eq!(form.count, None);
        assert!(!form.use_chapters_as_media_items);
        assert!(form.expression.is_empty());
    }

    #[test]
    fn playlist_normalization_forces_count_mode() {
        let mut form = FillerPresetForm {
            collection_type: COLLECTION_TYPE_PLAYLIST,
            playlist_id: Some(1),
            filler_mode: FILLER_MODE_DURATION,
            ..FillerPresetForm::default()
        };

        normalize_filler_preset_form(&mut form);

        assert_eq!(form.filler_mode, FILLER_MODE_COUNT);
    }

    #[test]
    fn duration_is_clamped_to_ersatztv_bounds() {
        let form = FillerPresetForm {
            duration_hours: 99,
            duration_minutes: 99,
            duration_seconds: 99,
            ..FillerPresetForm::default()
        };

        assert_eq!(duration_seconds(&form), Some(24 * 3600));
        assert_eq!(
            format_duration(duration_seconds(&form).unwrap()),
            "24:00:00"
        );
    }
}
