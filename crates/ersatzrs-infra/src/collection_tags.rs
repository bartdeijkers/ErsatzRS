//! Collection tag projection shares the membership transaction. Each provider
//! owns only its typed external collection ID on the affected metadata rows.
use ersatzrs_core::repositories::{RepositoryError, Result};
use sqlx::{Row, Transaction};

use crate::{db::Db, repositories::SqlxResultExt as _};

#[derive(Clone, Copy)]
pub(crate) enum Provider {
    Plex,
    Jellyfin,
    Emby,
}

pub(crate) async fn members(tx: &mut Transaction<'_, Db>, id: i64) -> Result<Vec<i64>> {
    sqlx::query_scalar(r#"SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ?"#)
        .bind(id)
        .fetch_all(&mut **tx)
        .await
        .repository()
}

pub(crate) async fn reconcile(
    tx: &mut Transaction<'_, Db>,
    provider: Provider,
    collection_id: i64,
    previous: &[i64],
) -> Result<()> {
    let (table, key, tag_type) = match provider {
        Provider::Plex => ("PlexCollection", "Key", "plex/collection"),
        Provider::Jellyfin => ("JellyfinCollection", "ItemId", "jellyfin/collection"),
        Provider::Emby => ("EmbyCollection", "ItemId", "emby/collection"),
    };
    let collection = format!(
        r#"SELECT "{key}" AS "Key", "Name", "MediaSourceId" FROM "{table}" WHERE "Id" = ?"#
    );
    let row = sqlx::query(crate::db::assert_sql_safe(&collection))
        .bind(collection_id)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
    let external_id: String = row.try_get("Key").repository()?;
    let name: String = row.try_get("Name").repository()?;
    let source_id: i64 = row
        .try_get::<Option<i64>, _>("MediaSourceId")
        .repository()?
        .ok_or_else(|| RepositoryError::Decode {
            kind: "provider collection",
            value: "collection has no source owner".to_owned(),
        })?;
    let previous = serde_json::json!(previous).to_string();
    for kind in ["Movie", "Show", "Season", "Episode", "OtherVideo"] {
        let affected = format!(
            r#"SELECT m."Id", EXISTS(SELECT 1 FROM "CollectionItem" ci
                 WHERE ci."CollectionId" = ? AND ci."MediaItemId" = m."{kind}Id") AS "Wanted"
               FROM "{kind}Metadata" m
               JOIN "MediaItem" media ON media."Id" = m."{kind}Id"
               JOIN "LibraryPath" path ON path."Id" = media."LibraryPathId"
               JOIN "Library" library ON library."Id" = path."LibraryId"
               WHERE library."MediaSourceId" = ? AND (m."{kind}Id" IN (
                 SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ?
                 UNION SELECT value FROM json_each(?))
               OR EXISTS (
                 SELECT 1 FROM "Tag" t WHERE t."{kind}MetadataId" = m."Id"
                   AND t."ExternalTypeId" = ? AND t."ExternalCollectionId" = ?))"#
        );
        let rows = sqlx::query(crate::db::assert_sql_safe(&affected))
            .bind(collection_id)
            .bind(source_id)
            .bind(collection_id)
            .bind(&previous)
            .bind(tag_type)
            .bind(&external_id)
            .fetch_all(&mut **tx)
            .await
            .repository()?;
        for row in rows {
            let id: i64 = row.try_get("Id").repository()?;
            let wanted: bool = row.try_get("Wanted").repository()?;
            let owned = format!(
                r#"SELECT "Id" FROM "Tag" WHERE "{kind}MetadataId" = ?
                   AND "ExternalTypeId" = ? AND "ExternalCollectionId" = ? ORDER BY "Id""#
            );
            let existing: Vec<i64> = sqlx::query_scalar(crate::db::assert_sql_safe(&owned))
                .bind(id)
                .bind(tag_type)
                .bind(&external_id)
                .fetch_all(&mut **tx)
                .await
                .repository()?;
            let keep = wanted.then(|| existing.first().copied()).flatten();
            for tag_id in existing {
                if Some(tag_id) == keep {
                    sqlx::query(r#"UPDATE "Tag" SET "Name" = ? WHERE "Id" = ?"#)
                        .bind(&name)
                        .bind(tag_id)
                        .execute(&mut **tx)
                        .await
                        .repository()?;
                } else {
                    sqlx::query(r#"DELETE FROM "Tag" WHERE "Id" = ?"#)
                        .bind(tag_id)
                        .execute(&mut **tx)
                        .await
                        .repository()?;
                }
            }
            if wanted && keep.is_none() {
                let insert = format!(
                    r#"INSERT INTO "Tag" ("{kind}MetadataId", "Name", "ExternalTypeId", "ExternalCollectionId")
                       VALUES (?, ?, ?, ?)"#
                );
                sqlx::query(crate::db::assert_sql_safe(&insert))
                    .bind(id)
                    .bind(&name)
                    .bind(tag_type)
                    .bind(&external_id)
                    .execute(&mut **tx)
                    .await
                    .repository()?;
            }
        }
    }
    Ok(())
}
