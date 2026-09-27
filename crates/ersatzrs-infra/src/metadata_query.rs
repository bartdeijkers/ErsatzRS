//! Shared read-side metadata query helpers.
//!
//! These are generic, per-metadata-row lookups (genres/tags/directors/
//! writers by owning-column, artwork by kind, multi-value content-rating
//! splitting) originally written for the admin media detail pages and reused
//! by the XMLTV guide loader so both surfaces stay consistent without
//! duplicating SQL.

use ersatzrs_core::repositories::{ArtworkReference, Result};
use ersatzrs_domain::EntityId;
use sqlx::{Pool, Row};

use crate::repositories::SqlxResultExt as _;

/// Load artwork selected by scalar ID columns without per-card queries.
pub(crate) async fn load_projected_artwork(
    pool: &Pool<crate::db::Db>,
    rows: &[crate::db::DbRow],
    columns: &[&str],
) -> Result<std::collections::HashMap<EntityId, ArtworkReference>> {
    let mut ids = Vec::new();
    for row in rows {
        for column in columns {
            if let Some(id) = row.try_get::<Option<EntityId>, _>(*column).repository()? {
                ids.push(id);
            }
        }
    }
    load_artwork_references(pool, &ids).await
}

pub(crate) fn projected_artwork(
    row: &crate::db::DbRow,
    column: &str,
    artwork: &std::collections::HashMap<EntityId, ArtworkReference>,
) -> Result<Option<ArtworkReference>> {
    let id: Option<EntityId> = row.try_get(column).repository()?;
    Ok(id.and_then(|id| artwork.get(&id).cloned()))
}

/// Resolve selected artwork identities in bounded batches. Projections select
/// IDs from their owning metadata rows first, so duplicate provider paths never
/// substitute a different image or kind.
pub(crate) async fn load_artwork_references(
    pool: &Pool<crate::db::Db>,
    ids: &[EntityId],
) -> Result<std::collections::HashMap<EntityId, ArtworkReference>> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut result = std::collections::HashMap::new();
    for batch in ids.chunks(400) {
        let placeholders = vec!["?"; batch.len()].join(",");
        let sql = format!(
            r#"SELECT "Id", "ArtworkKind", COALESCE("Path", '') AS "Path" FROM "Artwork" WHERE "Id" IN ({placeholders})"#
        );
        let mut query = sqlx::query(crate::db::assert_sql_safe(&sql));
        for id in batch {
            query = query.bind(id);
        }
        for row in query.fetch_all(pool).await.repository()? {
            let id = row.try_get("Id").repository()?;
            result.insert(
                id,
                ArtworkReference {
                    id,
                    kind: crate::repositories::common::decode_enum_column(
                        "artwork kind",
                        &row,
                        "ArtworkKind",
                    )?,
                    path: row.try_get("Path").repository()?,
                },
            );
        }
    }
    Ok(result)
}

/// Split an `ErsatzTV`-style `/`-delimited multi-value `ContentRating` string
/// (e.g. `"us:PG-13/de:12"`) into its individual values.
pub(crate) fn split_content_ratings(value: Option<String>) -> Vec<String> {
    value
        .map(|value| {
            value
                .split('/')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Load `Name` values from a metadata child table (`Genre`, `Tag`, `Studio`,
/// `Director`, `Writer`, ...) owned by the given metadata row.
pub(crate) async fn load_strings(
    pool: &Pool<crate::db::Db>,
    table: &str,
    column: &str,
    metadata_id: EntityId,
) -> Result<Vec<String>> {
    let tag_filter = if table == r#""Tag""# {
        r#" AND ("ExternalTypeId" IS NULL OR "ExternalTypeId" NOT IN ('319', 'nfo/country'))"#
    } else {
        ""
    };
    let sql =
        format!(r#"SELECT "Name" FROM {table} WHERE "{column}" = ?{tag_filter} ORDER BY "Name""#);
    sqlx::query_scalar(crate::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .fetch_all(pool)
        .await
        .repository()
}

/// Load the lowest-id non-empty `Artwork.Path` of the given kind owned by the
/// given metadata row.
pub(crate) async fn load_artwork_path(
    pool: &Pool<crate::db::Db>,
    owner_column: &str,
    metadata_id: EntityId,
    artwork_kind: i32,
) -> Result<Option<ArtworkReference>> {
    let sql = format!(
        r#"SELECT "Id", "ArtworkKind", "Path" FROM "Artwork"
           WHERE "{owner_column}" = ? AND "ArtworkKind" = ?
             AND NULLIF(TRIM("Path"), '') IS NOT NULL
           ORDER BY "Id" LIMIT 1"#
    );
    sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .bind(artwork_kind)
        .fetch_optional(pool)
        .await
        .repository()
        .and_then(|row| {
            row.map(|row| {
                Ok(ArtworkReference {
                    id: row.try_get("Id").repository()?,
                    kind: crate::repositories::common::decode_enum_column(
                        "artwork kind",
                        &row,
                        "ArtworkKind",
                    )?,
                    path: row.try_get("Path").repository()?,
                })
            })
            .transpose()
        })
}
