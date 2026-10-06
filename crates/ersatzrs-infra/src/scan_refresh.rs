//! Restart-safe scan-to-playout refresh intent persistence.

use chrono::Utc;
use ersatzrs_core::scheduling::{PlayoutPublicationGuard, PlayoutSourceRevision};
use ersatzrs_domain::{EntityId, ScanPlayoutRefreshReason};
use sqlx::{Row, Transaction};

use crate::db::{begin_write, Db, DbPool};

const RESOLUTION_STATE_RESOLVING: i32 = 0;
const RESOLUTION_STATE_RESOLVED: i32 = 1;
const RESOLUTION_STATE_FAILED: i32 = 2;
const INTENT_STATE_PENDING: i32 = 0;
const INTENT_STATE_BUILDING: i32 = 1;
const INTENT_STATE_FAILED: i32 = 2;
const RESOLUTION_FAILED_DIAGNOSTIC: &str = "scan-impact-resolution-failed";
const REFRESH_FAILED_DIAGNOSTIC: &str = "playout-refresh-build-failed";

const SQLITE_BEGIN_RESOLUTION_UPSERT_SQL: &str = r#"INSERT INTO "ScanRefreshResolution" (
       "LibraryId", "Generation", "State", "Reason", "Diagnostic",
       "StartedAt", "UpdatedAt"
   ) VALUES (?, 1, ?, ?, NULL, ?, ?)
   ON CONFLICT("LibraryId") DO UPDATE SET
       "Generation" = "ScanRefreshResolution"."Generation" + 1,
       "State" = excluded."State",
       "Reason" = excluded."Reason",
       "Diagnostic" = NULL,
       "StartedAt" = excluded."StartedAt",
       "UpdatedAt" = excluded."UpdatedAt""#;

const SQLITE_REFRESH_INTENT_UPSERT_SQL: &str = r#"INSERT INTO "ScanPlayoutRefreshIntent" (
       "PlayoutId", "LibraryId", "Generation", "State", "Reason",
       "Diagnostic", "CreatedAt", "UpdatedAt"
   ) VALUES (?, ?, 1, ?, ?, NULL, ?, ?)
   ON CONFLICT("PlayoutId") DO UPDATE SET
       "LibraryId" = CASE
           WHEN "ScanPlayoutRefreshIntent"."LibraryId" IS excluded."LibraryId"
               THEN excluded."LibraryId"
           ELSE NULL
       END,
       "Generation" = "ScanPlayoutRefreshIntent"."Generation" + 1,
       "State" = excluded."State",
       "Reason" = excluded."Reason",
       "Diagnostic" = NULL,
       "UpdatedAt" = excluded."UpdatedAt""#;

/// Capture source work before reading any scheduling inputs.
///
/// # Errors
/// Returns a storage error if the revision cannot be loaded.
pub async fn publication_guard(
    pool: &DbPool,
    playout_id: EntityId,
) -> Result<PlayoutPublicationGuard, sqlx::Error> {
    let revision: Option<(String, i64)> = sqlx::query_as(
        r#"SELECT "CreatedAt", "Generation" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_optional(pool)
    .await?;
    Ok(PlayoutPublicationGuard {
        playout_id,
        deferred_schedule_revision: crate::classic_deferred::revision(pool, playout_id).await?,
        source_revision: revision.map(|(identity, generation)| PlayoutSourceRevision {
            identity,
            generation,
        }),
    })
}

pub(crate) async fn publication_guard_matches(
    tx: &mut Transaction<'_, Db>,
    playout_id: EntityId,
    guard: &PlayoutPublicationGuard,
) -> Result<bool, sqlx::Error> {
    if guard.playout_id != playout_id {
        return Ok(false);
    }
    if crate::classic_deferred::revision(&mut **tx, playout_id).await?
        != guard.deferred_schedule_revision
    {
        return Ok(false);
    }
    let revision: Option<(String, i64)> = sqlx::query_as(
        r#"SELECT "CreatedAt", "Generation" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(
        revision.map(|(identity, generation)| PlayoutSourceRevision {
            identity,
            generation,
        }) == guard.source_revision,
    )
}

/// One durable refresh recovered after a process restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DurableScanPlayoutRefresh {
    pub playout_id: EntityId,
    pub reason: ScanPlayoutRefreshReason,
}

/// One scan-impact resolution interrupted before its membership transaction
/// could publish durable playout refresh intents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterruptedScanResolution {
    pub library_id: EntityId,
    pub generation: i64,
}

/// Load scan-impact resolutions that must be replayed before startup admits
/// durable playout refresh intents.
///
/// A resolving row is written before scanner-owned media rows can change and
/// is completed in the later membership/intent transaction. It is therefore a
/// durable replay marker for the otherwise unavoidable multi-row scan window.
///
/// # Errors
///
/// Returns an error when interrupted resolutions cannot be loaded.
pub async fn interrupted_resolutions(
    pool: &DbPool,
) -> Result<Vec<InterruptedScanResolution>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT "LibraryId", "Generation"
           FROM "ScanRefreshResolution"
           WHERE "State" = ?
           ORDER BY "LibraryId""#,
    )
    .bind(RESOLUTION_STATE_RESOLVING)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(InterruptedScanResolution {
                library_id: row.try_get("LibraryId")?,
                generation: row.try_get("Generation")?,
            })
        })
        .collect()
}

/// Complete an interrupted marker after a full replay scan proves that no
/// newer membership transaction was necessary.
///
/// If replay created a newer generation, this generation guard intentionally
/// leaves the newer state untouched.
///
/// # Errors
///
/// Returns an error when the marker cannot be updated.
pub async fn complete_interrupted_replay(
    pool: &DbPool,
    library_id: EntityId,
    generation: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE "ScanRefreshResolution"
           SET "State" = ?, "Diagnostic" = NULL, "UpdatedAt" = ?
           WHERE "LibraryId" = ? AND "Generation" = ? AND "State" = ?"#,
    )
    .bind(RESOLUTION_STATE_RESOLVED)
    .bind(Utc::now().to_rfc3339())
    .bind(library_id)
    .bind(generation)
    .bind(RESOLUTION_STATE_RESOLVING)
    .execute(pool)
    .await?;
    Ok(())
}

async fn fail_resolution_generation_in_tx(
    tx: &mut Transaction<'_, Db>,
    library_id: EntityId,
    generation: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"UPDATE "ScanRefreshResolution"
           SET "State" = ?, "Diagnostic" = ?, "UpdatedAt" = ?
           WHERE "LibraryId" = ? AND "Generation" = ? AND "State" = ?"#,
    )
    .bind(RESOLUTION_STATE_FAILED)
    .bind(RESOLUTION_FAILED_DIAGNOSTIC)
    .bind(Utc::now().to_rfc3339())
    .bind(library_id)
    .bind(generation)
    .bind(RESOLUTION_STATE_RESOLVING)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Fail only the exact interrupted marker that is still active.
///
/// A stale replay failure is intentionally an idempotent no-op. It must never
/// advance, overwrite, or otherwise transfer failure to newer work.
///
/// # Errors
///
/// Returns an error when the replay marker cannot be updated.
pub async fn fail_interrupted_replay(
    pool: &DbPool,
    library_id: EntityId,
    interrupted_generation: i64,
) -> Result<(), sqlx::Error> {
    let mut tx = begin_write(pool).await?;
    fail_resolution_generation_in_tx(&mut tx, library_id, interrupted_generation).await?;
    tx.commit().await?;
    Ok(())
}

/// Mark one library's membership impact as being resolved.
///
/// The returned generation must be supplied when recording the terminal state
/// so an older scan cannot overwrite a newer attempt.
///
/// # Errors
///
/// Returns an error when the resolution row cannot be allocated or read.
pub async fn begin_resolution(
    pool: &DbPool,
    library_id: EntityId,
    reason: ScanPlayoutRefreshReason,
) -> Result<i64, sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let mut tx = begin_write(pool).await?;
    sqlx::query(SQLITE_BEGIN_RESOLUTION_UPSERT_SQL)
        .bind(library_id)
        .bind(RESOLUTION_STATE_RESOLVING)
        .bind(reason.as_str())
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    // SQLite's write transaction prevents another writer from changing the row
    // between allocation and this read.
    let generation: i64 = sqlx::query_scalar(
        r#"SELECT "Generation" FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#,
    )
    .bind(library_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(generation)
}

/// Mark the matching resolution generation complete inside the caller's
/// membership transaction.
///
/// # Errors
///
/// Returns an error when persistence fails or the generation is no longer current.
pub async fn complete_resolution_in_tx(
    tx: &mut Transaction<'_, Db>,
    library_id: EntityId,
    generation: i64,
    reason: ScanPlayoutRefreshReason,
) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        r#"UPDATE "ScanRefreshResolution"
           SET "State" = ?, "Reason" = ?, "Diagnostic" = NULL, "UpdatedAt" = ?
           WHERE "LibraryId" = ? AND "Generation" = ?"#,
    )
    .bind(RESOLUTION_STATE_RESOLVED)
    .bind(reason.as_str())
    .bind(Utc::now().to_rfc3339())
    .bind(library_id)
    .bind(generation)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(sqlx::Error::Protocol(
            "scan refresh resolution generation changed before membership commit".to_owned(),
        ));
    }
    Ok(())
}

/// Retain a bounded diagnostic for an impact resolution that could not finish.
///
/// # Errors
///
/// Returns an error when the failure state cannot be persisted.
pub async fn fail_resolution(
    pool: &DbPool,
    library_id: EntityId,
    generation: i64,
    _diagnostic: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE "ScanRefreshResolution"
           SET "State" = ?, "Diagnostic" = ?, "UpdatedAt" = ?
           WHERE "LibraryId" = ? AND "Generation" = ?"#,
    )
    .bind(RESOLUTION_STATE_FAILED)
    .bind(RESOLUTION_FAILED_DIAGNOSTIC)
    .bind(Utc::now().to_rfc3339())
    .bind(library_id)
    .bind(generation)
    .execute(pool)
    .await?;
    Ok(())
}

/// Upsert generation-guarded playout intents inside the caller's membership
/// transaction.
///
/// # Errors
///
/// Returns an error when an intent cannot be read or persisted.
pub async fn persist_refresh_intents(
    tx: &mut Transaction<'_, Db>,
    library_id: EntityId,
    reason: ScanPlayoutRefreshReason,
    playout_ids: &[EntityId],
) -> Result<(), sqlx::Error> {
    persist_refresh_intents_with_owner(tx, Some(library_id), reason, playout_ids).await
}

/// Upsert refresh intents for an atomic dependency mutation that has no
/// managed Local-library owner, such as a legacy generated add-on playlist.
///
/// No separate resolution marker is required: the dependency rows and these
/// nullable-owner intents commit in the same transaction.
///
/// # Errors
///
/// Returns an error when an intent cannot be read or persisted.
pub async fn persist_unowned_refresh_intents(
    tx: &mut Transaction<'_, Db>,
    reason: ScanPlayoutRefreshReason,
    playout_ids: &[EntityId],
) -> Result<(), sqlx::Error> {
    persist_refresh_intents_with_owner(tx, None, reason, playout_ids).await
}

async fn persist_refresh_intents_with_owner(
    tx: &mut Transaction<'_, Db>,
    library_id: Option<EntityId>,
    reason: ScanPlayoutRefreshReason,
    playout_ids: &[EntityId],
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    for &playout_id in playout_ids {
        let previous: Option<String> = sqlx::query_scalar(
            r#"SELECT "Reason" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
        )
        .bind(playout_id)
        .fetch_optional(&mut **tx)
        .await?;
        let merged = previous
            .as_deref()
            .and_then(ScanPlayoutRefreshReason::from_token)
            .map_or(reason, |previous| previous.merge(reason));
        sqlx::query(SQLITE_REFRESH_INTENT_UPSERT_SQL)
            .bind(playout_id)
            .bind(library_id)
            .bind(INTENT_STATE_PENDING)
            .bind(merged.as_str())
            .bind(&now)
            .bind(&now)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Load work that was pending or building when this process started.
///
/// # Errors
///
/// Returns an error when persisted intents cannot be loaded or decoded.
pub async fn recoverable_refreshes(
    pool: &DbPool,
) -> Result<Vec<DurableScanPlayoutRefresh>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT "PlayoutId", "Reason"
           FROM "ScanPlayoutRefreshIntent"
           WHERE "State" IN (?, ?) AND ("NextAttempt" IS NULL OR "NextAttempt" <= ?)
           ORDER BY "PlayoutId""#,
    )
    .bind(INTENT_STATE_PENDING)
    .bind(INTENT_STATE_BUILDING)
    .bind(crate::repositories::common::encode_datetime(Utc::now()))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let reason: String = row.try_get("Reason")?;
            Ok(DurableScanPlayoutRefresh {
                playout_id: row.try_get("PlayoutId")?,
                reason: ScanPlayoutRefreshReason::from_token(&reason)
                    .unwrap_or(ScanPlayoutRefreshReason::RemoteStreamMembershipMixed),
            })
        })
        .collect()
}

/// Load pending work for the normal retry sweep without re-admitting builds
/// that are currently in progress.
///
/// # Errors
///
/// Returns an error when persisted intents cannot be loaded or decoded.
pub async fn pending_refreshes(
    pool: &DbPool,
) -> Result<Vec<DurableScanPlayoutRefresh>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT "PlayoutId", "Reason"
           FROM "ScanPlayoutRefreshIntent"
           WHERE "State" = ? AND ("NextAttempt" IS NULL OR "NextAttempt" <= ?)
           ORDER BY "PlayoutId""#,
    )
    .bind(INTENT_STATE_PENDING)
    .bind(crate::repositories::common::encode_datetime(Utc::now()))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let reason: String = row.try_get("Reason")?;
            Ok(DurableScanPlayoutRefresh {
                playout_id: row.try_get("PlayoutId")?,
                reason: ScanPlayoutRefreshReason::from_token(&reason)
                    .unwrap_or(ScanPlayoutRefreshReason::RemoteStreamMembershipMixed),
            })
        })
        .collect()
}

/// Capture the current intent generation when a Refresh actually starts.
///
/// # Errors
///
/// Returns an error when the intent cannot be read or marked as building.
pub async fn mark_refresh_building(
    pool: &DbPool,
    playout_id: EntityId,
) -> Result<Option<i64>, sqlx::Error> {
    let mut tx = begin_write(pool).await?;
    let generation: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Generation" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(generation) = generation {
        sqlx::query(
            r#"UPDATE "ScanPlayoutRefreshIntent"
               SET "State" = ?, "Diagnostic" = NULL, "UpdatedAt" = ?
               WHERE "PlayoutId" = ? AND "Generation" = ?"#,
        )
        .bind(INTENT_STATE_BUILDING)
        .bind(Utc::now().to_rfc3339())
        .bind(playout_id)
        .bind(generation)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(generation)
}

/// Remove an intent only when the successful build consumed that generation.
///
/// # Errors
///
/// Returns an error when the matching intent cannot be removed.
pub async fn complete_refresh(
    pool: &DbPool,
    playout_id: EntityId,
    generation: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"DELETE FROM "ScanPlayoutRefreshIntent"
           WHERE "PlayoutId" = ? AND "Generation" = ?"#,
    )
    .bind(playout_id)
    .bind(generation)
    .execute(pool)
    .await?;
    Ok(())
}

/// Retain the latest generation and a bounded build failure diagnostic.
///
/// # Errors
///
/// Returns an error when the matching intent cannot be marked failed.
pub async fn fail_refresh(
    pool: &DbPool,
    playout_id: EntityId,
    generation: i64,
    _diagnostic: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE "ScanPlayoutRefreshIntent"
           SET "State" = ?, "Diagnostic" = ?, "UpdatedAt" = ?
           WHERE "PlayoutId" = ? AND "Generation" = ?"#,
    )
    .bind(INTENT_STATE_FAILED)
    .bind(REFRESH_FAILED_DIAGNOSTIC)
    .bind(Utc::now().to_rfc3339())
    .bind(playout_id)
    .bind(generation)
    .execute(pool)
    .await?;
    Ok(())
}

/// Retry an unchanged intent generation after a transient publication race.
/// New observations preserve this cooldown; stale builders cannot defer them.
///
/// # Errors
/// Returns a storage error if retry eligibility cannot be persisted.
pub async fn defer_refresh(
    pool: &DbPool,
    playout_id: EntityId,
    generation: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    defer_refresh_with_diagnostic(
        pool,
        playout_id,
        generation,
        now,
        "scripted-source-publication-deferred",
    )
    .await
}

pub(crate) async fn defer_boundary_refresh(
    pool: &DbPool,
    playout_id: EntityId,
    generation: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    defer_refresh_with_diagnostic(
        pool,
        playout_id,
        generation,
        now,
        "playout-programme-boundary-elapsed",
    )
    .await
}

pub(crate) async fn defer_session_publication(
    pool: &DbPool,
    playout_id: EntityId,
    generation: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    defer_refresh_with_diagnostic(
        pool,
        playout_id,
        generation,
        now,
        "playout-session-publication-pending",
    )
    .await
}

async fn defer_refresh_with_diagnostic(
    pool: &DbPool,
    playout_id: EntityId,
    generation: i64,
    now: chrono::DateTime<Utc>,
    diagnostic: &'static str,
) -> Result<(), sqlx::Error> {
    let mut tx = begin_write(pool).await?;
    let attempts: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Attempts" FROM "ScanPlayoutRefreshIntent"
        WHERE "PlayoutId" = ? AND "Generation" = ? AND "State" = ?"#,
    )
    .bind(playout_id)
    .bind(generation)
    .bind(INTENT_STATE_BUILDING)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(attempts) = attempts {
        let delay = 30_i64
            .saturating_mul(1_i64 << attempts.clamp(0, 4))
            .min(300);
        sqlx::query(
            r#"UPDATE "ScanPlayoutRefreshIntent" SET "State" = ?, "Attempts" = "Attempts" + 1,
            "NextAttempt" = ?, "Diagnostic" = ?, "UpdatedAt" = ?
            WHERE "PlayoutId" = ? AND "Generation" = ?"#,
        )
        .bind(INTENT_STATE_PENDING)
        .bind(crate::repositories::common::encode_datetime(
            now + chrono::Duration::seconds(delay),
        ))
        .bind(diagnostic)
        .bind(now.to_rfc3339())
        .bind(playout_id)
        .bind(generation)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::Executor as _;
    use tokio::sync::Barrier;

    use super::*;

    async fn pool() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        pool.execute("PRAGMA foreign_keys = OFF")
            .await
            .expect("disable fixture foreign keys");
        pool
    }

    #[tokio::test]
    async fn publication_retry_preserves_backoff_and_newer_generations() {
        let pool = pool().await;
        let mut tx = begin_write(&pool).await.unwrap();
        persist_unowned_refresh_intents(
            &mut tx,
            ScanPlayoutRefreshReason::ScriptedSourceChanged,
            &[1],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let now = chrono::DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap();
        for delay in [30, 60, 120, 240, 300, 300] {
            let generation = mark_refresh_building(&pool, 1).await.unwrap().unwrap();
            defer_refresh(&pool, 1, generation, now).await.unwrap();
            let next: String = sqlx::query_scalar(
                r#"SELECT "NextAttempt" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 1"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                crate::repositories::common::decode_datetime("retry", &next).unwrap(),
                now + chrono::Duration::seconds(delay)
            );
            assert!(pending_refreshes(&pool).await.unwrap().is_empty());
        }
        let before: (i64, String) = sqlx::query_as(r#"SELECT "Attempts", "NextAttempt" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 1"#).fetch_one(&pool).await.unwrap();
        let mut tx = begin_write(&pool).await.unwrap();
        persist_unowned_refresh_intents(
            &mut tx,
            ScanPlayoutRefreshReason::ScriptedSourceChanged,
            &[1],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        defer_refresh(&pool, 1, 1, now + chrono::Duration::hours(1))
            .await
            .unwrap();
        let after: (i64, String) = sqlx::query_as(r#"SELECT "Attempts", "NextAttempt" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 1"#).fetch_one(&pool).await.unwrap();
        assert_eq!(
            before, after,
            "new observations keep cooldown; a stale worker cannot extend it"
        );
    }

    async fn concurrent_pool(max_connections: u32) -> (tempfile::TempDir, DbPool) {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("scan-refresh-concurrency.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(false)
            .busy_timeout(Duration::from_secs(10));
        let migration_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .expect("connect sqlite migration pool");
        sqlx::migrate!("../../migrations")
            .run(&migration_pool)
            .await
            .expect("migrate concurrent sqlite pool");
        migration_pool.close().await;
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .expect("reopen concurrent sqlite pool");
        (directory, pool)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_resolution_starts_allocate_every_generation_once() {
        const WRITER_COUNT: u32 = 12;

        let (_directory, pool) = concurrent_pool(WRITER_COUNT).await;
        let barrier = Arc::new(Barrier::new(
            usize::try_from(WRITER_COUNT).expect("writer count fits usize"),
        ));
        let mut tasks = Vec::new();
        for _ in 0..WRITER_COUNT {
            let pool = pool.clone();
            let barrier = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                begin_resolution(
                    &pool,
                    10,
                    ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
                )
                .await
            }));
        }

        let mut generations = Vec::new();
        for task in tasks {
            generations.push(
                task.await
                    .expect("join resolution writer")
                    .expect("allocate resolution generation"),
            );
        }
        generations.sort_unstable();

        assert_eq!(
            generations,
            (1..=i64::from(WRITER_COUNT)).collect::<Vec<_>>()
        );
        let persisted_generation: i64 = sqlx::query_scalar(
            r#"SELECT "Generation" FROM "ScanRefreshResolution" WHERE "LibraryId" = 10"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load final resolution generation");
        assert_eq!(persisted_generation, i64::from(WRITER_COUNT));
        pool.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_intent_writers_merge_without_lost_updates() {
        const WRITER_COUNT: u32 = 12;

        let (_directory, pool) = concurrent_pool(WRITER_COUNT).await;
        let mut tx = begin_write(&pool).await.expect("begin seed intent");
        persist_refresh_intents(
            &mut tx,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
            &[20],
        )
        .await
        .expect("seed intent");
        tx.commit().await.expect("commit seed intent");
        let seed_generation = mark_refresh_building(&pool, 20)
            .await
            .expect("mark seed intent building")
            .expect("seed generation");
        fail_refresh(&pool, 20, seed_generation, "bounded-test-diagnostic")
            .await
            .expect("mark seed intent failed");
        let created_at: String = sqlx::query_scalar(
            r#"SELECT "CreatedAt" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load seed creation time");

        let barrier = Arc::new(Barrier::new(
            usize::try_from(WRITER_COUNT).expect("writer count fits usize"),
        ));
        let mut tasks = Vec::new();
        for writer in 0..WRITER_COUNT {
            let pool = pool.clone();
            let barrier = Arc::clone(&barrier);
            tasks.push(tokio::spawn(async move {
                let library_id = if writer % 3 == 0 { 11 } else { 10 };
                let reason = if writer % 2 == 0 {
                    ScanPlayoutRefreshReason::RemoteStreamMembershipAdded
                } else {
                    ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved
                };
                barrier.wait().await;
                let mut tx = begin_write(&pool).await?;
                persist_refresh_intents(&mut tx, library_id, reason, &[20]).await?;
                tx.commit().await
            }));
        }
        for task in tasks {
            task.await
                .expect("join intent writer")
                .expect("persist concurrent intent");
        }

        let intent: (Option<i64>, i64, i64, String, Option<String>, String) = sqlx::query_as(
            r#"SELECT "LibraryId", "Generation", "State", "Reason", "Diagnostic", "CreatedAt"
               FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load merged intent");
        assert_eq!(intent.0, None);
        assert_eq!(intent.1, 1 + i64::from(WRITER_COUNT));
        assert_eq!(intent.2, i64::from(INTENT_STATE_PENDING));
        assert_eq!(
            intent.3,
            ScanPlayoutRefreshReason::RemoteStreamMembershipMixed.as_str()
        );
        assert_eq!(intent.4, None);
        assert_eq!(intent.5, created_at);
        pool.close().await;
    }

    #[tokio::test]
    async fn newer_scan_generation_survives_older_build_completion() {
        let pool = pool().await;
        let mut tx = begin_write(&pool).await.expect("begin intent");
        persist_refresh_intents(
            &mut tx,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
            &[20],
        )
        .await
        .expect("persist first intent");
        tx.commit().await.expect("commit first intent");
        let first_generation = mark_refresh_building(&pool, 20)
            .await
            .expect("mark first building")
            .expect("first generation");
        assert!(pending_refreshes(&pool)
            .await
            .expect("normal retry sweep")
            .is_empty());
        assert_eq!(
            recoverable_refreshes(&pool)
                .await
                .expect("startup recovery includes interrupted build"),
            [DurableScanPlayoutRefresh {
                playout_id: 20,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
            }]
        );

        let mut tx = begin_write(&pool).await.expect("begin newer intent");
        persist_refresh_intents(
            &mut tx,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipRemoved,
            &[20],
        )
        .await
        .expect("persist newer intent");
        tx.commit().await.expect("commit newer intent");
        complete_refresh(&pool, 20, first_generation)
            .await
            .expect("complete older generation");

        assert_eq!(
            recoverable_refreshes(&pool).await.expect("recover latest"),
            [DurableScanPlayoutRefresh {
                playout_id: 20,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipMixed,
            }]
        );
    }

    #[tokio::test]
    async fn failed_build_is_retained_but_not_automatically_retried() {
        let pool = pool().await;
        let resolution_generation = begin_resolution(
            &pool,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
        )
        .await
        .expect("begin failed resolution");
        fail_resolution(&pool, 10, resolution_generation, "C:\\private\\provider-id")
            .await
            .expect("retain resolution failure");
        let mut tx = begin_write(&pool).await.expect("begin intent");
        persist_refresh_intents(
            &mut tx,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
            &[20],
        )
        .await
        .expect("persist intent");
        tx.commit().await.expect("commit intent");
        let generation = mark_refresh_building(&pool, 20)
            .await
            .expect("mark building")
            .expect("generation");
        fail_refresh(&pool, 20, generation, "C:\\private\\provider-id")
            .await
            .expect("retain failure");

        assert!(recoverable_refreshes(&pool)
            .await
            .expect("recoverable")
            .is_empty());
        let diagnostic: String = sqlx::query_scalar(
            r#"SELECT "Diagnostic" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 20"#,
        )
        .fetch_one(&pool)
        .await
        .expect("diagnostic");
        assert_eq!(diagnostic, REFRESH_FAILED_DIAGNOSTIC);
        assert!(!diagnostic.contains("private"));
        let resolution_diagnostic: String = sqlx::query_scalar(
            r#"SELECT "Diagnostic" FROM "ScanRefreshResolution" WHERE "LibraryId" = 10"#,
        )
        .fetch_one(&pool)
        .await
        .expect("resolution diagnostic");
        assert_eq!(resolution_diagnostic, RESOLUTION_FAILED_DIAGNOSTIC);
        assert!(!resolution_diagnostic.contains("private"));
    }

    #[tokio::test]
    async fn committed_membership_boundary_survives_restart_before_queue_admission() {
        let directory = tempfile::tempdir().expect("temporary database directory");
        let path = directory.path().join("scan-refresh.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .expect("connect file-backed sqlite");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate file-backed sqlite");
        pool.execute("PRAGMA foreign_keys = OFF")
            .await
            .expect("disable fixture foreign keys");

        let reason = ScanPlayoutRefreshReason::RemoteStreamMembershipAdded;
        let generation = begin_resolution(&pool, 10, reason)
            .await
            .expect("begin resolution");
        let mut tx = begin_write(&pool).await.expect("begin membership boundary");
        persist_refresh_intents(&mut tx, 10, reason, &[20])
            .await
            .expect("persist durable intent");
        complete_resolution_in_tx(&mut tx, 10, generation, reason)
            .await
            .expect("complete resolution atomically");
        tx.commit().await.expect("commit membership boundary");
        pool.close().await;

        let reopened = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("reopen file-backed sqlite");
        let resolution_state: i64 = sqlx::query_scalar(
            r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = 10"#,
        )
        .fetch_one(&reopened)
        .await
        .expect("load resolution state");
        assert_eq!(resolution_state, i64::from(RESOLUTION_STATE_RESOLVED));
        assert_eq!(
            recoverable_refreshes(&reopened)
                .await
                .expect("recover after restart"),
            [DurableScanPlayoutRefresh {
                playout_id: 20,
                reason,
            }]
        );
    }

    #[tokio::test]
    async fn interrupted_resolution_is_a_generation_guarded_replay_marker() {
        let pool = pool().await;
        let generation = begin_resolution(
            &pool,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
        )
        .await
        .expect("begin interrupted resolution");
        assert_eq!(
            interrupted_resolutions(&pool)
                .await
                .expect("load interrupted resolution"),
            [InterruptedScanResolution {
                library_id: 10,
                generation,
            }]
        );

        let newer_generation = begin_resolution(
            &pool,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
        )
        .await
        .expect("begin replay generation");
        fail_resolution(&pool, 10, generation, "old replay failed")
            .await
            .expect("older failure is ignored");
        assert_eq!(
            interrupted_resolutions(&pool)
                .await
                .expect("load current replay generation"),
            [InterruptedScanResolution {
                library_id: 10,
                generation: newer_generation,
            }]
        );
        complete_interrupted_replay(&pool, 10, generation)
            .await
            .expect("older replay completion is ignored");
        assert_eq!(
            interrupted_resolutions(&pool)
                .await
                .expect("newer generation remains interrupted"),
            [InterruptedScanResolution {
                library_id: 10,
                generation: newer_generation,
            }]
        );
        complete_interrupted_replay(&pool, 10, newer_generation)
            .await
            .expect("complete current replay marker");
        assert!(interrupted_resolutions(&pool)
            .await
            .expect("completed replay is not interrupted")
            .is_empty());
    }

    #[tokio::test]
    async fn interrupted_replay_failure_leaves_newer_success_untouched() {
        let pool = pool().await;
        let interrupted_generation = begin_resolution(
            &pool,
            7,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        )
        .await
        .expect("begin interrupted generation");
        let replay_generation = begin_resolution(
            &pool,
            7,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
        )
        .await
        .expect("advance generation during replay");
        complete_interrupted_replay(&pool, 7, replay_generation)
            .await
            .expect("complete successful part of replay");

        fail_interrupted_replay(&pool, 7, interrupted_generation)
            .await
            .expect("ignore stale aggregate replay failure");

        let state: (i64, i64, String, Option<String>) = sqlx::query_as(
            r#"SELECT "Generation", "State", "Reason", "Diagnostic"
               FROM "ScanRefreshResolution" WHERE "LibraryId" = 7"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load replay marker");
        assert_eq!(
            state,
            (
                replay_generation,
                i64::from(RESOLUTION_STATE_RESOLVED),
                ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced
                    .as_str()
                    .to_owned(),
                None
            )
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn interrupted_replay_failure_is_guarded_by_generation_and_state() {
        let pool = pool().await;
        let active_generation = begin_resolution(
            &pool,
            7,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        )
        .await
        .expect("begin active replay marker");
        fail_interrupted_replay(&pool, 7, active_generation)
            .await
            .expect("fail active replay marker");
        fail_interrupted_replay(&pool, 7, active_generation)
            .await
            .expect("repeat active replay failure idempotently");
        let failed: (i64, i64, Option<String>) = sqlx::query_as(
            r#"SELECT "Generation", "State", "Diagnostic"
               FROM "ScanRefreshResolution" WHERE "LibraryId" = 7"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load failed replay marker");
        assert_eq!(
            failed,
            (
                active_generation,
                i64::from(RESOLUTION_STATE_FAILED),
                Some(RESOLUTION_FAILED_DIAGNOSTIC.to_owned())
            )
        );

        let completed_generation = begin_resolution(
            &pool,
            8,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
        )
        .await
        .expect("begin replay marker to complete");
        complete_interrupted_replay(&pool, 8, completed_generation)
            .await
            .expect("complete replay marker");
        fail_interrupted_replay(&pool, 8, completed_generation)
            .await
            .expect("ignore completed replay failure");
        let completed: (i64, i64, Option<String>) = sqlx::query_as(
            r#"SELECT "Generation", "State", "Diagnostic"
               FROM "ScanRefreshResolution" WHERE "LibraryId" = 8"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load completed replay marker");
        assert_eq!(
            completed,
            (
                completed_generation,
                i64::from(RESOLUTION_STATE_RESOLVED),
                None
            )
        );

        let stale_generation = begin_resolution(
            &pool,
            9,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        )
        .await
        .expect("begin stale replay marker");
        let newer_generation = begin_resolution(
            &pool,
            9,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
        )
        .await
        .expect("begin unrelated newer work");
        fail_interrupted_replay(&pool, 9, stale_generation)
            .await
            .expect("ignore stale failure against newer work");
        let newer: (i64, i64, Option<String>) = sqlx::query_as(
            r#"SELECT "Generation", "State", "Diagnostic"
               FROM "ScanRefreshResolution" WHERE "LibraryId" = 9"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load newer active marker");
        assert_eq!(
            newer,
            (
                newer_generation,
                i64::from(RESOLUTION_STATE_RESOLVING),
                None
            )
        );

        let stale_generation = begin_resolution(
            &pool,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        )
        .await
        .expect("begin stale marker before newer failed work");
        let newer_generation = begin_resolution(
            &pool,
            10,
            ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
        )
        .await
        .expect("begin newer work that will fail");
        fail_resolution(&pool, 10, newer_generation, "unbounded caller detail")
            .await
            .expect("fail newer work with bounded diagnostic");
        fail_interrupted_replay(&pool, 10, stale_generation)
            .await
            .expect("ignore stale failure against newer failed work");
        let newer_failed: (i64, i64, String, Option<String>) = sqlx::query_as(
            r#"SELECT "Generation", "State", "Reason", "Diagnostic"
               FROM "ScanRefreshResolution" WHERE "LibraryId" = 10"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load newer failed marker");
        assert_eq!(
            newer_failed,
            (
                newer_generation,
                i64::from(RESOLUTION_STATE_FAILED),
                ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced
                    .as_str()
                    .to_owned(),
                Some(RESOLUTION_FAILED_DIAGNOSTIC.to_owned())
            )
        );
    }
}
