use super::{decode, invalid, resolve, PublicationFence};
use crate::{
    db::{begin_write, DbPool},
    repositories::{
        common::encode_datetime, SqliteSchedulingContentRepository, SqlxResultExt as _,
    },
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{EntityId, PlayoutScheduleKind, ScanPlayoutRefreshReason};
use sqlx::Row;
use std::collections::BTreeMap;

/// Persist a successful scan's revision and coalesced comparison work atomically.
/// Source lookups run later through the existing refresh sweep, not in the scan.
///
/// # Errors
/// Returns an error if the successful-scan receipt cannot be committed.
pub async fn record_successful_scan(pool: &DbPool, library_id: EntityId) -> Result<()> {
    let mut tx = begin_write(pool).await.repository()?;
    let now = encode_datetime(Utc::now());
    sqlx::query(r#"INSERT INTO "ScriptedLibraryRevision" ("LibraryId", "Revision", "UpdatedAt") VALUES (?, 1, ?)
        ON CONFLICT("LibraryId") DO UPDATE SET "Revision" = "Revision" + 1, "UpdatedAt" = excluded."UpdatedAt""#)
        .bind(library_id).bind(&now).execute(&mut *tx).await.repository()?;
    sqlx::query(r#"INSERT INTO "ScriptedSourceRefreshWork" ("LibraryId", "PlayoutId", "Revision", "NextAttempt")
        SELECT r."LibraryId", p."Id", r."Revision", ? FROM "ScriptedLibraryRevision" r CROSS JOIN "Playout" p
        WHERE r."LibraryId" = ? AND p."ScheduleKind" IN (?, 'Scripted')
          AND EXISTS (SELECT 1 FROM "PlayoutScriptedSource" s WHERE s."PlayoutId" = p."Id")
        ON CONFLICT("LibraryId", "PlayoutId") DO UPDATE SET "Revision" = excluded."Revision""#)
        .bind(&now).bind(library_id).bind(PlayoutScheduleKind::Scripted as i32)
        .execute(&mut *tx).await.repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

#[derive(Clone, Copy)]
struct Work {
    library_id: EntityId,
    playout_id: EntityId,
    revision: i64,
    attempts: i64,
}

struct Observation {
    key: String,
    context: String,
    fingerprint: Option<String>,
}

async fn observe(pool: &DbPool, work: Work) -> Result<Vec<Observation>> {
    let active: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "Playout" WHERE "Id" = ? AND "ScheduleKind" IN (?, 'Scripted'))"#)
        .bind(work.playout_id).bind(PlayoutScheduleKind::Scripted as i32).fetch_one(pool).await.repository()?;
    if !active {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT "SourceKey", "Context" FROM "PlayoutScriptedSource" WHERE "PlayoutId" = ? ORDER BY "SourceKey""#)
        .bind(work.playout_id).fetch_all(pool).await.repository()?;
    let repository = SqliteSchedulingContentRepository::new(pool.clone());
    let mut result = Vec::with_capacity(rows.len());
    for (key, context) in rows {
        let dependency = decode(&key, &context)?;
        let (_, current) = resolve(&repository, &dependency.source, dependency.resolution).await?;
        result.push(Observation {
            key,
            context,
            fingerprint: current.library_fingerprints.get(&work.library_id).cloned(),
        });
    }
    Ok(result)
}

async fn publish_observation(
    pool: &DbPool,
    work: Work,
    observations: &[Observation],
    fence: &PublicationFence,
) -> Result<bool> {
    let mut tx = begin_write(pool).await.repository()?;
    let revision: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Revision" FROM "ScriptedSourceRefreshWork" WHERE "LibraryId" = ? AND "PlayoutId" = ?"#)
        .bind(work.library_id).bind(work.playout_id).fetch_optional(&mut *tx).await.repository()?;
    if revision != Some(work.revision) {
        return Ok(false);
    }
    fence.validate(&mut tx).await?;
    let active: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "Playout" WHERE "Id" = ? AND "ScheduleKind" IN (?, 'Scripted'))"#)
        .bind(work.playout_id).bind(PlayoutScheduleKind::Scripted as i32).fetch_one(&mut *tx).await.repository()?;
    if !active {
        delete_work(&mut tx, work).await?;
        tx.commit().await.repository()?;
        return Ok(true);
    }
    let rows = sqlx::query(
        r#"SELECT "SourceKey", "Context", "ObservedLibraries" FROM "PlayoutScriptedSource" WHERE "PlayoutId" = ? ORDER BY "SourceKey""#)
        .bind(work.playout_id).fetch_all(&mut *tx).await.repository()?;
    if rows.len() != observations.len() {
        return Err(invalid());
    }
    let mut changed = false;
    for (row, observation) in rows.into_iter().zip(observations) {
        if row.try_get::<&str, _>("SourceKey").repository()? != observation.key
            || row.try_get::<&str, _>("Context").repository()? != observation.context
        {
            return Err(invalid());
        }
        let mut observed: BTreeMap<EntityId, String> =
            serde_json::from_str(row.try_get("ObservedLibraries").repository()?)
                .map_err(|_| invalid())?;
        if observed.get(&work.library_id) == observation.fingerprint.as_ref() {
            continue;
        }
        changed = true;
        if let Some(fingerprint) = &observation.fingerprint {
            observed.insert(work.library_id, fingerprint.clone());
        } else {
            observed.remove(&work.library_id);
        }
        sqlx::query(r#"UPDATE "PlayoutScriptedSource" SET "ObservedLibraries" = ? WHERE "PlayoutId" = ? AND "SourceKey" = ?"#)
            .bind(serde_json::json!(observed).to_string()).bind(work.playout_id).bind(&observation.key)
            .execute(&mut *tx).await.repository()?;
    }
    if changed {
        sqlx::query(r#"UPDATE "Playout" SET "ScriptedSourceRevision" = "ScriptedSourceRevision" + 1 WHERE "Id" = ?"#)
            .bind(work.playout_id).execute(&mut *tx).await.repository()?;
        crate::scan_refresh::persist_refresh_intents(
            &mut tx,
            work.library_id,
            ScanPlayoutRefreshReason::ScriptedSourceChanged,
            &[work.playout_id],
        )
        .await
        .repository()?;
    }
    delete_work(&mut tx, work).await?;
    tx.commit().await.repository()?;
    Ok(true)
}

async fn delete_work(tx: &mut sqlx::Transaction<'_, crate::db::Db>, work: Work) -> Result<()> {
    sqlx::query(r#"DELETE FROM "ScriptedSourceRefreshWork" WHERE "LibraryId" = ? AND "PlayoutId" = ? AND "Revision" = ?"#)
        .bind(work.library_id).bind(work.playout_id).bind(work.revision).execute(&mut **tx).await.repository()?;
    Ok(())
}

async fn retry(
    pool: &DbPool,
    work: Work,
    now: DateTime<Utc>,
    error: &RepositoryError,
) -> Result<()> {
    let delay = 30_i64
        .saturating_mul(1_i64 << work.attempts.clamp(0, 4))
        .min(300);
    let diagnostic = if matches!(
        error,
        RepositoryError::Unavailable {
            operation: super::PUBLICATION_OPERATION,
            ..
        }
    ) {
        // Waiting for an active scan is normal pending work, not a failed source.
        None
    } else {
        Some("scripted-source-comparison-failed")
    };
    sqlx::query(
        r#"UPDATE "ScriptedSourceRefreshWork" SET "Attempts" = "Attempts" + 1,
        "NextAttempt" = ?, "Diagnostic" = ?
        WHERE "LibraryId" = ? AND "PlayoutId" = ? AND "Revision" = ? AND "Attempts" = ?"#,
    )
    .bind(encode_datetime(now + Duration::seconds(delay)))
    .bind(diagnostic)
    .bind(work.library_id)
    .bind(work.playout_id)
    .bind(work.revision)
    .bind(work.attempts)
    .execute(pool)
    .await
    .repository()?;
    Ok(())
}

/// Compare at most sixteen queued library/playout pairs within thirty seconds.
/// Durable revisions, observations and intents make concurrent/restarted sweeps
/// idempotent. A failing source backs off without blocking other queued playouts.
///
/// # Errors
/// Returns a storage error if the queue or its retry state cannot be accessed.
pub async fn resolve_pending(pool: &DbPool, now: DateTime<Utc>) -> Result<usize> {
    let rows = sqlx::query(
        r#"SELECT "LibraryId", "PlayoutId", "Revision", "Attempts" FROM "ScriptedSourceRefreshWork"
        WHERE "NextAttempt" <= ? ORDER BY "NextAttempt", "LibraryId", "PlayoutId" LIMIT 16"#,
    )
    .bind(encode_datetime(now))
    .fetch_all(pool)
    .await
    .repository()?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut completed = 0;
    for row in rows {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        let work = Work {
            library_id: row.try_get("LibraryId").repository()?,
            playout_id: row.try_get("PlayoutId").repository()?,
            revision: row.try_get("Revision").repository()?,
            attempts: row.try_get("Attempts").repository()?,
        };
        let result = tokio::time::timeout_at(deadline, async {
            let fence = PublicationFence::prepare(pool, &[]).await?;
            let observation = observe(pool, work).await?;
            publish_observation(pool, work, &observation, &fence).await
        })
        .await
        .unwrap_or_else(|_| {
            Err(RepositoryError::unavailable(
                "scripted source",
                "comparison timed out",
            ))
        });
        match result {
            Ok(true) => completed += 1,
            Ok(false) => {}
            Err(error) => retry(pool, work, now, &error).await?,
        }
    }
    Ok(completed)
}
