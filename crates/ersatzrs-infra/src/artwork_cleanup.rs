//! Bounded database-only cleanup of actors and artwork left without owners.

use sqlx::{Pool, QueryBuilder};

use crate::db::{assert_sql_safe, Db};

pub(crate) const ACTOR_ORPHAN: &str = r#"
    "ArtistMetadataId" IS NULL AND "EpisodeMetadataId" IS NULL
    AND "ImageMetadataId" IS NULL AND "MovieMetadataId" IS NULL
    AND "MusicVideoMetadataId" IS NULL AND "OtherVideoMetadataId" IS NULL
    AND "RemoteStreamMetadataId" IS NULL AND "SeasonMetadataId" IS NULL
    AND "ShowMetadataId" IS NULL AND "SongMetadataId" IS NULL"#;
pub(crate) const ARTWORK_ORPHAN: &str = r#"
    "IsMetadataOrphan" = 1 AND NOT EXISTS (
        SELECT 1 FROM "Actor" WHERE "Actor"."ArtworkId" = "Artwork"."Id"
    )"#;

/// Counts of rows actually removed; cached image files are never touched.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OrphanedArtworkCleanupReport {
    /// Actors with no metadata owner.
    pub actors_deleted: u64,
    /// Artwork with neither a metadata/channel owner nor an actor reference.
    pub artwork_deleted: u64,
}

/// Delete orphan actors before deleting orphan artwork.
///
/// Matches upstream `ArtworkRepository`: each entity kind has its own limit,
/// checked between batches of 5,000, with deletes in chunks of 100. Consequently
/// a positive limit can be exceeded by the remainder of a batch. Nonpositive
/// limits do nothing. Each batch is transactional: concurrent owner changes
/// cannot race selection/deletion, and a failed chunk rolls back its batch.
/// Earlier completed batches remain committed if a later batch fails.
///
/// # Errors
///
/// Returns an error if a database transaction, selection, or deletion fails.
pub async fn clean_orphaned_artwork(
    pool: &Pool<Db>,
    limit: i32,
) -> Result<OrphanedArtworkCleanupReport, sqlx::Error> {
    let limit = u64::try_from(limit).unwrap_or_default();
    if limit == 0 {
        return Ok(OrphanedArtworkCleanupReport::default());
    }
    let actors_deleted = delete_batches(pool, "Actor", ACTOR_ORPHAN, limit).await?;
    let artwork_deleted = delete_batches(pool, "Artwork", ARTWORK_ORPHAN, limit).await?;
    Ok(OrphanedArtworkCleanupReport {
        actors_deleted,
        artwork_deleted,
    })
}

async fn delete_batches(
    pool: &Pool<Db>,
    table: &'static str,
    predicate: &'static str,
    limit: u64,
) -> Result<u64, sqlx::Error> {
    let mut deleted = 0;
    while deleted < limit {
        let mut tx = crate::db::begin_write(pool).await?;
        let ids: Vec<i64> = sqlx::query_scalar(assert_sql_safe(format!(
            r#"SELECT "Id" FROM "{table}" WHERE {predicate} ORDER BY "Id" LIMIT 5000"#,
        )))
        .fetch_all(&mut *tx)
        .await?;
        if ids.is_empty() {
            tx.commit().await?;
            break;
        }
        let mut batch_deleted = 0;
        for chunk in ids.chunks(100) {
            let mut query = QueryBuilder::<Db>::new(format!(
                r#"DELETE FROM "{table}" WHERE ({predicate}) AND "Id" IN ("#,
            ));
            let mut values = query.separated(", ");
            for id in chunk {
                values.push_bind(*id);
            }
            values.push_unseparated(")");
            batch_deleted += query.build().execute(&mut *tx).await?.rows_affected();
        }
        tx.commit().await?;
        deleted += batch_deleted;
        // A trigger may retain or reassign selected rows. Do not spin forever.
        if batch_deleted == 0 {
            break;
        }
    }
    Ok(deleted)
}
