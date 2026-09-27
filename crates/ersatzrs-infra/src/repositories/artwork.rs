use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{ArtworkReference, ArtworkRepository, Result};
use ersatzrs_domain::EntityId;
use sqlx::{Pool, Row};

#[derive(Clone)]
pub struct SqliteArtworkRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl ArtworkRepository for SqliteArtworkRepository {
    async fn find_by_id(&self, id: EntityId) -> Result<Option<ArtworkReference>> {
        let row = sqlx::query(r#"SELECT "Id", "ArtworkKind", COALESCE("Path", '') AS "Path" FROM "Artwork" WHERE "Id" = ?"#)
            .bind(id).fetch_optional(&self.0).await.repository()?;
        row.map(|row| {
            Ok(ArtworkReference {
                id: row.try_get("Id").repository()?,
                kind: super::common::decode_enum_column("artwork kind", &row, "ArtworkKind")?,
                path: row.try_get("Path").repository()?,
            })
        })
        .transpose()
    }

    async fn is_known_path(&self, path: &str) -> Result<bool> {
        let row: Option<i64> = sqlx::query_scalar(r#"SELECT 1 FROM "Artwork" WHERE "Path" = ?"#)
            .bind(path)
            .fetch_optional(&self.0)
            .await
            .repository()?;
        Ok(row.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::ArtworkKind;
    use sqlx::Executor;

    #[tokio::test]
    async fn artwork_identity_resolves_only_the_persisted_row() {
        let pool = crate::db::init("sqlite::memory:").await.unwrap();
        pool.execute(r#"INSERT INTO "Artwork" ("Id", "ArtworkKind", "Path", "DateAdded", "DateUpdated") VALUES
            (1, 0, 'jellyfin://Items/fixture/Images/Primary?tag=stored', '2026-09-27', '2026-09-27'),
            (2, 1, 'emby://Items/fixture/Images/Primary?tag=stored', '2026-09-27', '2026-09-27')"#).await.unwrap();
        let repository = SqliteArtworkRepository(pool.clone());
        let poster = repository.find_by_id(1).await.unwrap().unwrap();
        assert_eq!(poster.id, 1);
        assert_eq!(poster.kind, ArtworkKind::Poster);
        assert_eq!(
            poster.path,
            "jellyfin://Items/fixture/Images/Primary?tag=stored"
        );
        let thumbnail = repository.find_by_id(2).await.unwrap().unwrap();
        assert_eq!(thumbnail.kind, ArtworkKind::Thumbnail);
        assert!(thumbnail.path.starts_with("emby://"));
        assert!(repository.find_by_id(3).await.unwrap().is_none());
        pool.execute(r#"UPDATE "Artwork" SET "ArtworkKind" = 999 WHERE "Id" = 1"#)
            .await
            .unwrap();
        assert!(repository.find_by_id(1).await.is_err());
        pool.close().await;
        assert!(repository.find_by_id(2).await.is_err());
    }
}
