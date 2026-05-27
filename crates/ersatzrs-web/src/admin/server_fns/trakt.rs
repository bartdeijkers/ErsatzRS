use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use ersatzrs_infra::db::DbPool;

const TRAKT_LISTS_PAGE_SIZE_KEY: &str = "pages.trakt.lists_page_size";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraktListRow {
    pub id: i64,
    pub trakt_id: i32,
    pub slug: String,
    pub name: String,
    pub item_count: i32,
    pub match_count: i32,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraktListForm {
    pub id: i64,
    pub slug: String,
    pub auto_refresh: bool,
    pub generate_playlist: bool,
}

#[server]
pub async fn list_trakt_lists(
    page: u32,
    page_size: u32,
) -> Result<(Vec<TraktListRow>, u64), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_trakt_lists_with_pool(&state.pool, page, page_size).await
}

#[server]
pub async fn get_trakt_lists_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_page_size_with_pool(&state.pool).await
}

#[cfg(feature = "ssr")]
pub async fn list_trakt_lists_with_pool(
    pool: &DbPool,
    page: u32,
    page_size: u32,
) -> Result<(Vec<TraktListRow>, u64), ServerFnError> {
    use sqlx::Row;

    save_config_value(pool, TRAKT_LISTS_PAGE_SIZE_KEY, &page_size.to_string()).await?;

    let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TraktList""#)
        .fetch_one(pool)
        .await?;
    let rows = sqlx::query(
        r#"SELECT "tl"."Id", "tl"."TraktId", "tl"."User", "tl"."List",
                  COALESCE("tl"."Name", '') AS "Name",
                  "tl"."ItemCount", "tl"."AutoRefresh", "tl"."GeneratePlaylist",
                  COUNT("matched"."Id") AS "MatchCount"
           FROM "TraktList" AS "tl"
           LEFT JOIN "TraktListItem" AS "matched"
             ON "matched"."TraktListId" = "tl"."Id" AND "matched"."MediaItemId" IS NOT NULL
           GROUP BY "tl"."Id", "tl"."TraktId", "tl"."User", "tl"."List",
                    "tl"."Name", "tl"."ItemCount", "tl"."AutoRefresh",
                    "tl"."GeneratePlaylist"
           ORDER BY "tl"."Name", "tl"."Id"
           LIMIT ? OFFSET ?"#,
    )
    .bind(i64::from(page_size))
    .bind(i64::from(page) * i64::from(page_size))
    .fetch_all(pool)
    .await?;

    let rows = rows
        .iter()
        .map(|row| {
            let user = row
                .try_get::<Option<String>, _>("User")
                .unwrap_or_default()
                .unwrap_or_default();
            let list = row
                .try_get::<Option<String>, _>("List")
                .unwrap_or_default()
                .unwrap_or_default();
            TraktListRow {
                id: row.try_get("Id").unwrap_or_default(),
                trakt_id: row.try_get("TraktId").unwrap_or_default(),
                slug: format!("{user}/{list}"),
                name: row.try_get("Name").unwrap_or_default(),
                item_count: row.try_get("ItemCount").unwrap_or_default(),
                match_count: i32::try_from(row.try_get::<i64, _>("MatchCount").unwrap_or_default())
                    .unwrap_or(i32::MAX),
                auto_refresh: decode_bool(row.try_get("AutoRefresh").unwrap_or_default()),
                generate_playlist: decode_bool(row.try_get("GeneratePlaylist").unwrap_or_default()),
            }
        })
        .collect();

    #[allow(clippy::cast_sign_loss)]
    Ok((rows, total.max(0) as u64))
}

#[server]
pub async fn get_trakt_list_form(id: i64) -> Result<Option<TraktListForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_trakt_list_form_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn get_trakt_list_form_with_pool(
    pool: &DbPool,
    id: i64,
) -> Result<Option<TraktListForm>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT "Id", "User", "List", "AutoRefresh", "GeneratePlaylist"
           FROM "TraktList"
           WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| {
        let user = row
            .try_get::<Option<String>, _>("User")
            .unwrap_or_default()
            .unwrap_or_default();
        let list = row
            .try_get::<Option<String>, _>("List")
            .unwrap_or_default()
            .unwrap_or_default();
        TraktListForm {
            id: row.try_get("Id").unwrap_or_default(),
            slug: format!("{user}/{list}"),
            auto_refresh: decode_bool(row.try_get("AutoRefresh").unwrap_or_default()),
            generate_playlist: decode_bool(row.try_get("GeneratePlaylist").unwrap_or_default()),
        }
    }))
}

#[server]
pub async fn add_trakt_list(input: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    add_trakt_list_with_pool(&state.pool, input).await
}

#[server]
pub async fn match_trakt_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    match_trakt_list_with_pool(&state.pool, id).await
}

#[server]
pub async fn update_trakt_list(form: TraktListForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    update_trakt_list_with_pool(&state.pool, form).await
}

#[server]
pub async fn delete_trakt_list(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    delete_trakt_list_with_pool(&state.pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn add_trakt_list_with_pool(pool: &DbPool, input: String) -> Result<i64, ServerFnError> {
    use ersatzrs_core::trakt::parse_trakt_list_locator;
    use ersatzrs_infra::trakt::TraktApiClient;
    use ersatzrs_infra::trakt_sync::TraktSynchronizer;

    let locator = parse_trakt_list_locator(&input).map_err(ServerFnError::new)?;
    let client = TraktApiClient::from_env().map_err(ServerFnError::new)?;
    let sync = TraktSynchronizer::new(pool.clone(), client);
    sync.add_or_refresh(&locator)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn match_trakt_list_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    use ersatzrs_infra::trakt::NoopTraktApiClient;
    use ersatzrs_infra::trakt_sync::TraktSynchronizer;

    let sync = TraktSynchronizer::new(pool.clone(), NoopTraktApiClient);
    sync.match_list_items(id).await.map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn update_trakt_list_with_pool(
    pool: &DbPool,
    form: TraktListForm,
) -> Result<(), ServerFnError> {
    use ersatzrs_infra::trakt::NoopTraktApiClient;
    use ersatzrs_infra::trakt_sync::TraktSynchronizer;

    let sync = TraktSynchronizer::new(pool.clone(), NoopTraktApiClient);
    sync.update_settings(form.id, form.auto_refresh, form.generate_playlist)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn delete_trakt_list_with_pool(pool: &DbPool, id: i64) -> Result<(), ServerFnError> {
    use ersatzrs_infra::trakt::NoopTraktApiClient;
    use ersatzrs_infra::trakt_sync::TraktSynchronizer;

    let sync = TraktSynchronizer::new(pool.clone(), NoopTraktApiClient);
    sync.delete_list(id).await.map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn get_page_size_with_pool(pool: &DbPool) -> Result<u32, ServerFnError> {
    let value: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(TRAKT_LISTS_PAGE_SIZE_KEY)
            .fetch_optional(pool)
            .await?;

    Ok(value
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(10))
}

#[cfg(feature = "ssr")]
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

fn decode_bool(value: i64) -> bool {
    value != 0
}
