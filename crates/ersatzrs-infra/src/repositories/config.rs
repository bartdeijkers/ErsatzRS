use async_trait::async_trait;
use ersatzrs_core::repositories::{ConfigRepository, Result};
use sqlx::{Pool, Sqlite};

#[derive(Clone)]
pub struct SqliteConfigRepository(pub Pool<Sqlite>);

#[async_trait]
impl ConfigRepository for SqliteConfigRepository {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .fetch_optional(&self.0)
            .await
            .map_err(Into::into)
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)
             ON CONFLICT("Key") DO UPDATE SET "Value" = "excluded"."Value""#,
        )
        .bind(key)
        .bind(value)
        .execute(&self.0)
        .await?;
        Ok(())
    }
}
