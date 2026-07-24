use async_trait::async_trait;
use ersatzrs_core::repositories::{
    RepositoryError, Result, TraktListAdminForm, TraktListAdminRow, TraktRepository,
};
use ersatzrs_core::trakt::TraktListLocator;
use ersatzrs_domain::EntityId;
use sqlx::{Pool, Row};

use crate::repositories::common::decode_bool_column;
use crate::repositories::SqlxResultExt as _;
use crate::trakt::{NoopTraktApiClient, TraktApiClient};
use crate::trakt_sync::TraktSynchronizer;

const PAGE_SIZE_KEY: &str = "pages.trakt.lists_page_size";

#[derive(Clone)]
pub struct SqliteTraktRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl TraktRepository for SqliteTraktRepository {
    async fn list_admin(&self, page: u32, page_size: u32) -> Result<(Vec<TraktListAdminRow>, u64)> {
        let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TraktList""#)
            .fetch_one(&self.0)
            .await
            .repository()?;
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
        .fetch_all(&self.0)
        .await
        .repository()?;

        let rows = rows
            .iter()
            .map(|row| {
                let match_count: i64 = row.try_get("MatchCount").repository()?;
                Ok(TraktListAdminRow {
                    id: row.try_get("Id").repository()?,
                    trakt_id: row.try_get("TraktId").repository()?,
                    user: row
                        .try_get::<Option<String>, _>("User")
                        .repository()?
                        .unwrap_or_default(),
                    list: row
                        .try_get::<Option<String>, _>("List")
                        .repository()?
                        .unwrap_or_default(),
                    name: row.try_get("Name").repository()?,
                    item_count: row.try_get("ItemCount").repository()?,
                    match_count: i32::try_from(match_count).map_err(|_| {
                        RepositoryError::Decode {
                            kind: "Trakt list match count",
                            value: match_count.to_string(),
                        }
                    })?,
                    auto_refresh: decode_bool_column(row, "AutoRefresh")?,
                    generate_playlist: decode_bool_column(row, "GeneratePlaylist")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let total = u64::try_from(total).map_err(|_| RepositoryError::Decode {
            kind: "Trakt list count",
            value: total.to_string(),
        })?;
        Ok((rows, total))
    }

    async fn get_admin_form(&self, id: EntityId) -> Result<Option<TraktListAdminForm>> {
        let row = sqlx::query(
            r#"SELECT "Id", "User", "List", "AutoRefresh", "GeneratePlaylist"
               FROM "TraktList"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| {
            Ok(TraktListAdminForm {
                id: row.try_get("Id").repository()?,
                user: row
                    .try_get::<Option<String>, _>("User")
                    .repository()?
                    .unwrap_or_default(),
                list: row
                    .try_get::<Option<String>, _>("List")
                    .repository()?
                    .unwrap_or_default(),
                auto_refresh: decode_bool_column(&row, "AutoRefresh")?,
                generate_playlist: decode_bool_column(&row, "GeneratePlaylist")?,
            })
        })
        .transpose()
    }

    async fn add_or_refresh(&self, locator: &TraktListLocator) -> Result<EntityId> {
        let client = TraktApiClient::from_env()?;
        TraktSynchronizer::new(self.0.clone(), client)
            .add_or_refresh(locator)
            .await
    }

    async fn match_items(&self, id: EntityId) -> Result<()> {
        TraktSynchronizer::new(self.0.clone(), NoopTraktApiClient)
            .match_list_items(id)
            .await
    }

    async fn update_settings(
        &self,
        id: EntityId,
        auto_refresh: bool,
        generate_playlist: bool,
    ) -> Result<()> {
        TraktSynchronizer::new(self.0.clone(), NoopTraktApiClient)
            .update_settings(id, auto_refresh, generate_playlist)
            .await
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        TraktSynchronizer::new(self.0.clone(), NoopTraktApiClient)
            .delete_list(id)
            .await
    }

    async fn get_page_size(&self) -> Result<u32> {
        let value: Option<String> =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(PAGE_SIZE_KEY)
                .fetch_optional(&self.0)
                .await
                .repository()?;
        Ok(value
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(10))
    }

    async fn set_page_size(&self, page_size: u32) -> Result<()> {
        let updated = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
            .bind(page_size.to_string())
            .bind(PAGE_SIZE_KEY)
            .execute(&self.0)
            .await
            .repository()?;
        if updated.rows_affected() == 0 {
            sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
                .bind(PAGE_SIZE_KEY)
                .bind(page_size.to_string())
                .execute(&self.0)
                .await
                .repository()?;
        }
        Ok(())
    }
}
