//! ADR 0137: translate selected provider identities only at physical-input boundaries.
use std::collections::BTreeMap;

use ersatzrs_core::repositories::{RepositoryError, Result};
use serde::Serialize;
use sqlx::Row as _;

use crate::{db::DbPool, repositories::SqlxResultExt as _};

/// One atomic persisted mapping snapshot, bound to its version and owner revision.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StreamIndexes {
    version: i64,
    movie: Option<i64>,
    episode: Option<i64>,
    updated: String,
    indexes: BTreeMap<i32, i32>,
}

pub(crate) async fn load(pool: &DbPool, version: i64) -> Result<Option<StreamIndexes>> {
    // One SQLite statement observes map, owner, revision and readiness together.
    // Reads are scoped to one version; no whole-library projection is allocated.
    let invalid_map = crate::jellyfin::INVALID_SOURCE_INDEX_MAP;
    let sql = format!(
        r#"SELECT v."MovieId",v."EpisodeId",v."DateUpdated",v."ProviderStreamIndexVersion",
        (EXISTS(SELECT 1 FROM "JellyfinMovie" WHERE "Id"=v."MovieId")
         OR EXISTS(SELECT 1 FROM "JellyfinEpisode" WHERE "Id"=v."EpisodeId")) AS "Jellyfin",
        EXISTS(SELECT 1 FROM "MediaItem" i
            JOIN "LibraryPath" p ON p."Id"=i."LibraryPathId"
            JOIN "JellyfinLibrary" l ON l."Id"=p."LibraryId"
            WHERE i."Id"=COALESCE(v."MovieId",v."EpisodeId")) AS "JellyfinLibrary",
        ({invalid_map}) AS "InvalidMap",m."StreamIndex",m."SourceStreamIndex"
        FROM "MediaVersion" v LEFT JOIN "ProviderStreamIndex" m ON m."MediaVersionId"=v."Id"
        WHERE v."Id"=? ORDER BY m."StreamIndex""#
    );
    let rows = sqlx::query(crate::db::assert_sql_safe(&sql))
        .bind(version)
        .fetch_all(pool)
        .await
        .repository()?;
    let row = rows.first().ok_or_else(unavailable)?;
    let jellyfin = row.try_get::<bool, _>("Jellyfin").repository()?;
    let jellyfin_library = row.try_get::<bool, _>("JellyfinLibrary").repository()?;
    if !jellyfin_library {
        return if jellyfin
            || row
                .try_get::<i64, _>("ProviderStreamIndexVersion")
                .repository()?
                != 0
            || row
                .try_get::<Option<i64>, _>("StreamIndex")
                .repository()?
                .is_some()
        {
            Err(unavailable())
        } else {
            Ok(None)
        };
    }
    // A locally resolved file does not need remote item/connection metadata.
    // Its owning Jellyfin library still requires the persisted physical map;
    // remote source identity independently requires the exact provider item.
    if row.try_get::<bool, _>("InvalidMap").repository()? {
        return Err(unavailable());
    }
    let mut map = StreamIndexes {
        version,
        movie: row.try_get("MovieId").repository()?,
        episode: row.try_get("EpisodeId").repository()?,
        updated: row.try_get("DateUpdated").repository()?,
        indexes: BTreeMap::new(),
    };
    for row in rows {
        let logical: i32 = row.try_get("StreamIndex").repository()?;
        let physical: i32 = row.try_get("SourceStreamIndex").repository()?;
        map.indexes.insert(logical, physical);
    }
    Ok(Some(map))
}

pub(crate) fn physical(map: Option<&StreamIndexes>, logical: usize) -> Result<usize> {
    let Some(map) = map else { return Ok(logical) };
    let logical = i32::try_from(logical).map_err(|_| unavailable())?;
    usize::try_from(*map.indexes.get(&logical).ok_or_else(unavailable)?).map_err(|_| unavailable())
}

/// Published Next audio is already physical; pins/selectors still use provider IDs.
pub(crate) fn logical(map: Option<&StreamIndexes>, physical: usize) -> Result<usize> {
    let Some(map) = map else { return Ok(physical) };
    let physical = i32::try_from(physical).map_err(|_| unavailable())?;
    map.indexes
        .iter()
        .find_map(|(logical, value)| (*value == physical).then_some(*logical))
        .and_then(|logical| usize::try_from(logical).ok())
        .ok_or_else(unavailable)
}

pub(crate) async fn ensure_current(
    pool: &DbPool,
    version: i64,
    captured: Option<&StreamIndexes>,
) -> Result<()> {
    if load(pool, version).await?.as_ref() != captured {
        return Err(unavailable());
    }
    Ok(())
}

fn unavailable() -> RepositoryError {
    RepositoryError::Conflict(
        "Jellyfin physical stream mapping is missing, invalid or changed; rescan the library"
            .into(),
    )
}

/// Translate inside the caller's writer/read transaction, including readiness.
/// The caller has already established Jellyfin ownership in that transaction.
pub(crate) async fn physical_in_transaction(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    version: i64,
    logical: i32,
) -> Result<i32> {
    let invalid = crate::jellyfin::INVALID_SOURCE_INDEX_MAP;
    let sql = format!(
        r#"SELECT m."SourceStreamIndex" FROM "MediaVersion" v
        JOIN "ProviderStreamIndex" m ON m."MediaVersionId"=v."Id"
        WHERE v."Id"=? AND m."StreamIndex"=? AND NOT ({invalid})"#
    );
    sqlx::query_scalar(crate::db::assert_sql_safe(&sql))
        .bind(version)
        .bind(logical)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
        .ok_or_else(unavailable)
}
