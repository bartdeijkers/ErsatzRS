use async_trait::async_trait;
use ersatzrs_core::repositories::{ConfigRepository, Result};
use sqlx::Pool;

use super::common::decode_string_column;

#[derive(Clone)]
pub struct SqliteConfigRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl ConfigRepository for SqliteConfigRepository {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .fetch_optional(&self.0)
            .await?;

        row.map(|row| decode_config_value(&row)).transpose()
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
             "#,
        )
        .bind(key)
        .bind(value)
        .execute(&self.0)
        .await?;
        Ok(())
    }
}

fn decode_config_value(row: &crate::db::DbRow) -> Result<String> {
    decode_string_column(row, "Value")
}
