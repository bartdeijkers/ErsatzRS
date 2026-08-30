use async_trait::async_trait;
use ersatzrs_core::repositories::{
    RepositoryError, Result, SavedSearchAdminRow, SavedSearchRepository,
};
use ersatzrs_domain::EntityId;
use sqlx::{Pool, Row};

use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;

#[derive(Clone)]
pub struct SqliteSavedSearchRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl SavedSearchRepository for SqliteSavedSearchRepository {
    async fn list_admin(&self) -> Result<Vec<SavedSearchAdminRow>> {
        let rows = sqlx::query(
            r#"SELECT "Id", "Name", "Query"
               FROM "SavedSearch"
               ORDER BY "Name", "Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter().map(map_row).collect()
    }

    async fn get_admin(&self, id: EntityId) -> Result<Option<SavedSearchAdminRow>> {
        sqlx::query(
            r#"SELECT "Id", "Name", "Query"
               FROM "SavedSearch"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .as_ref()
        .map(map_row)
        .transpose()
    }

    async fn name_exists(&self, id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "SavedSearch"
               WHERE "Name" = ? COLLATE NOCASE AND "Id" != ?"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn save_admin(&self, id: EntityId, name: &str, query: &str) -> Result<EntityId> {
        if id == 0 {
            let mut transaction = crate::db::begin_write(&self.0).await.repository()?;
            let result = sqlx::query(
                r#"INSERT INTO "SavedSearch" ("Name", "Query")
                   VALUES (?, ?)"#,
            )
            .bind(name)
            .bind(query)
            .execute(&mut *transaction)
            .await
            .repository()?;
            let id = crate::db::inserted_id(&mut transaction, result)
                .await
                .repository()?;
            transaction.commit().await.repository()?;
            Ok(id)
        } else {
            let result = sqlx::query(
                r#"UPDATE "SavedSearch"
                   SET "Name" = ?, "Query" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(name)
            .bind(query)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
            if result.rows_affected() == 0 {
                Err(RepositoryError::NotFound {
                    kind: "saved search",
                    id,
                })
            } else {
                Ok(id)
            }
        }
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        let result = sqlx::query(r#"DELETE FROM "SavedSearch" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            Err(RepositoryError::NotFound {
                kind: "saved search",
                id,
            })
        } else {
            Ok(())
        }
    }
}

fn map_row(row: &DbRow) -> Result<SavedSearchAdminRow> {
    Ok(SavedSearchAdminRow {
        id: row.try_get("Id").repository()?,
        name: row.try_get("Name").repository()?,
        query: row.try_get("Query").repository()?,
    })
}
