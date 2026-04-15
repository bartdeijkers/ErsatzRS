use async_trait::async_trait;
use ersatzrs_core::repositories::{ConfigRepository, Result};
use sqlx::{Pool, Sqlite};

#[derive(Clone)]
pub struct SqliteConfigRepository(pub Pool<Sqlite>);

#[async_trait]
impl ConfigRepository for SqliteConfigRepository {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        sqlx::query!("SELECT value FROM config_entries WHERE key = ?", key)
            .fetch_optional(&self.0)
            .await
            .map(|row| row.map(|value| value.value))
            .map_err(Into::into)
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query!(
            "INSERT INTO config_entries (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            key,
            value
        )
        .execute(&self.0)
        .await?;
        Ok(())
    }
}
