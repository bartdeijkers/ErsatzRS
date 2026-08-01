use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{ArtworkRepository, Result};
use sqlx::Pool;

#[derive(Clone)]
pub struct SqliteArtworkRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl ArtworkRepository for SqliteArtworkRepository {
    async fn is_known_path(&self, path: &str) -> Result<bool> {
        let row: Option<i64> = sqlx::query_scalar(r#"SELECT 1 FROM "Artwork" WHERE "Path" = ?"#)
            .bind(path)
            .fetch_optional(&self.0)
            .await
            .repository()?;
        Ok(row.is_some())
    }
}
