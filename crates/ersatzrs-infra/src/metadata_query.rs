//! Shared read-side metadata query helpers.
//!
//! These are generic, per-metadata-row lookups (genres/tags/directors/
//! writers by owning-column, artwork by kind, multi-value content-rating
//! splitting) originally written for the admin media detail pages and reused
//! by the XMLTV guide loader so both surfaces stay consistent without
//! duplicating SQL.

use ersatzrs_core::repositories::Result;
use ersatzrs_domain::EntityId;
use sqlx::Pool;

use crate::repositories::SqlxResultExt as _;

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
    let sql = format!(r#"SELECT "Name" FROM {table} WHERE "{column}" = ? ORDER BY "Name""#);
    sqlx::query_scalar(crate::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .fetch_all(pool)
        .await
        .repository()
}

/// Load a single `Artwork.Path` of the given kind owned by the given
/// metadata row.
pub(crate) async fn load_artwork_path(
    pool: &Pool<crate::db::Db>,
    owner_column: &str,
    metadata_id: EntityId,
    artwork_kind: i32,
) -> Result<Option<String>> {
    let sql = format!(
        r#"SELECT "Path" FROM "Artwork"
           WHERE "{owner_column}" = ? AND "ArtworkKind" = ? LIMIT 1"#
    );
    sqlx::query_scalar::<_, Option<String>>(crate::db::assert_sql_safe(&sql))
        .bind(metadata_id)
        .bind(artwork_kind)
        .fetch_optional(pool)
        .await
        .repository()
        .map(Option::flatten)
}
