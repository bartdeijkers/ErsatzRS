use std::collections::BTreeSet;

use ersatzrs_core::repositories::Result;
use sqlx::{Row, Transaction};

use super::PlexLabel;
use crate::{db::Db, repositories::SqlxResultExt as _};

const LABEL_TYPE: &str = "plex/label";

pub(super) enum ContainerKind {
    Show,
    Season,
}

/// Labels attach to metadata, including shows/seasons without playable media.
pub(super) async fn replace_container_labels(
    tx: &mut Transaction<'_, Db>,
    kind: ContainerKind,
    item_id: i64,
    title: &str,
    dates: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
    labels: &[PlexLabel],
) -> Result<()> {
    let (kind, owner_column) = match kind {
        ContainerKind::Show => ("Show", "ShowMetadataId"),
        ContainerKind::Season => ("Season", "SeasonMetadataId"),
    };
    let select =
        format!(r#"SELECT "Id" FROM "{kind}Metadata" WHERE "{kind}Id" = ? ORDER BY "Id" LIMIT 1"#);
    let existing: Option<i64> = sqlx::query_scalar(crate::db::assert_sql_safe(&select))
        .bind(item_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?;
    let metadata_id = if let Some(id) = existing {
        // Preserve other metadata fields and facets while refreshing the title.
        let update = format!(
            r#"UPDATE "{kind}Metadata" SET "Title" = ?, "SortTitle" = ?, "DateUpdated" = ? WHERE "Id" = ?"#
        );
        sqlx::query(crate::db::assert_sql_safe(&update))
            .bind(title)
            .bind(title)
            .bind(super::encode_datetime(dates.1))
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        id
    } else {
        let insert = format!(
            r#"INSERT INTO "{kind}Metadata" ("{kind}Id", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated")
               VALUES (?, 2, ?, ?, ?, ?)"#
        );
        let result = sqlx::query(crate::db::assert_sql_safe(&insert))
            .bind(item_id)
            .bind(title)
            .bind(title)
            .bind(super::encode_datetime(dates.0))
            .bind(super::encode_datetime(dates.1))
            .execute(&mut **tx)
            .await
            .repository()?;
        crate::db::last_insert_id(result)
    };
    replace_labels(tx, owner_column, metadata_id, labels).await
}

/// Reconcile only library-owned labels, including pre-ownership untyped rows.
/// Collection and network scanners own all other rows independently.
pub(super) async fn replace_labels(
    tx: &mut Transaction<'_, Db>,
    owner_column: &'static str,
    metadata_id: i64,
    labels: &[PlexLabel],
) -> Result<()> {
    let mut incoming = labels
        .iter()
        .filter(|label| !label.name.trim().is_empty())
        .map(|label| (label.external_id.clone(), label.name.trim().to_owned()))
        .collect::<BTreeSet<_>>();
    let select = format!(
        r#"SELECT "Id", "Name", "ExternalCollectionId" FROM "Tag"
           WHERE "{owner_column}" = ? AND ("ExternalTypeId" = ? OR
             ("ExternalTypeId" IS NULL AND "ExternalCollectionId" IS NULL))
           ORDER BY "Id""#
    );
    let existing = sqlx::query(crate::db::assert_sql_safe(&select))
        .bind(metadata_id)
        .bind(LABEL_TYPE)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
    for row in existing {
        let id: i64 = row.try_get("Id").repository()?;
        let name: String = row.try_get("Name").repository()?;
        let external_id: Option<String> = row.try_get("ExternalCollectionId").repository()?;
        if incoming.remove(&(external_id, name)) {
            // Upgrade a retained legacy row without changing its identity.
            sqlx::query(r#"UPDATE "Tag" SET "ExternalTypeId" = ? WHERE "Id" = ?"#)
                .bind(LABEL_TYPE)
                .bind(id)
                .execute(&mut **tx)
                .await
                .repository()?;
        } else {
            sqlx::query(r#"DELETE FROM "Tag" WHERE "Id" = ?"#)
                .bind(id)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
    }
    let insert = format!(
        r#"INSERT INTO "Tag" ("Name", "ExternalCollectionId", "ExternalTypeId", "{owner_column}")
           VALUES (?, ?, ?, ?)"#
    );
    for (external_id, name) in incoming {
        sqlx::query(crate::db::assert_sql_safe(&insert))
            .bind(name)
            .bind(external_id)
            .bind(LABEL_TYPE)
            .bind(metadata_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}
