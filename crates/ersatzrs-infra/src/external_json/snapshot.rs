//! Accepted external timelines shared by playback and guide readers (ADR 0110).
//!
//! Absence preserves legacy direct-file reading. Once accepted, a document is
//! authoritative until another guarded publication; file edits alone cannot
//! bypass programme preservation. Never fall back from a corrupt snapshot.

use std::{io::Write, path::Path, time::Instant};

use chrono::{DateTime, Utc};
use serde::{Serialize, Serializer};
use sqlx::Row;

use super::{ExternalJsonError, ExternalJsonItem, ExternalJsonSchedule, MAX_SCHEDULE_BYTES};
use crate::db::DbPool;

const SCHEMA_VERSION: i64 = 1;

/// Diagnostics intentionally omit document contents and source paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    #[error("external JSON accepted planning database access failed")]
    Database,
    #[error("external JSON accepted planning is invalid or unsupported")]
    Invalid,
    #[error("external JSON accepted planning exceeds the 32 MiB limit")]
    TooLarge,
    #[error("external JSON playout or source changed during preparation")]
    ChangedSource,
    #[error("external JSON accepted planning changed during preparation")]
    Superseded,
    #[error("external JSON publication missed its safe programme boundary")]
    Expired,
}

/// A persisted revision; paths and programme bindings are deliberately not Debug.
pub struct Snapshot {
    pub revision: i64,
    pub schedule_file: String,
    pub schedule: ExternalJsonSchedule,
}

/// Optimistic publication fence captured before preparing an external candidate.
pub struct Publication<'a> {
    pub playout_id: i64,
    pub expected_revision: Option<i64>,
    pub schedule_file: &'a str,
    pub published_at: DateTime<Utc>,
    pub effective_from: Option<DateTime<Utc>>,
    pub deadline: Option<Instant>,
}

/// Read only the optimistic revision for an explicit non-preserving repair.
pub(crate) async fn revision(pool: &DbPool, playout_id: i64) -> Result<Option<i64>, SnapshotError> {
    sqlx::query_scalar(
        r#"SELECT "Revision" FROM "PlayoutExternalJsonSnapshot" WHERE "PlayoutId"=?"#,
    )
    .bind(playout_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| SnapshotError::Database)
}

/// Read an accepted revision without consulting the mutable source file.
///
/// # Errors
/// Returns redacted database, size and version/document validation failures.
pub async fn accepted(pool: &DbPool, playout_id: i64) -> Result<Option<Snapshot>, SnapshotError> {
    let row = sqlx::query(
        r#"SELECT "Revision", "SchemaVersion", "ScheduleFile", "Document"
           FROM "PlayoutExternalJsonSnapshot" WHERE "PlayoutId"=?"#,
    )
    .bind(playout_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| SnapshotError::Database)?;
    let Some(row) = row else { return Ok(None) };
    let revision: i64 = row
        .try_get("Revision")
        .map_err(|_| SnapshotError::Database)?;
    let version: i64 = row
        .try_get("SchemaVersion")
        .map_err(|_| SnapshotError::Database)?;
    if version != SCHEMA_VERSION || revision <= 0 {
        return Err(SnapshotError::Invalid);
    }
    let document: String = row
        .try_get("Document")
        .map_err(|_| SnapshotError::Database)?;
    if document.len() as u64 > MAX_SCHEDULE_BYTES {
        return Err(SnapshotError::TooLarge);
    }
    let schedule = super::parse(document.as_bytes())
        .map_err(|_| SnapshotError::Invalid)?
        .ok_or(SnapshotError::Invalid)?;
    Ok(Some(Snapshot {
        revision,
        schedule_file: row
            .try_get("ScheduleFile")
            .map_err(|_| SnapshotError::Database)?,
        schedule,
    }))
}

/// Use the accepted document, or the legacy live file when none was accepted.
///
/// # Errors
/// A damaged accepted document fails closed rather than adopting a different
/// live source. Legacy readers retain the bounded file reader's diagnostics.
pub async fn load_playout(
    pool: &DbPool,
    playout_id: i64,
    live_path: &Path,
) -> Result<Option<ExternalJsonSchedule>, ExternalJsonError> {
    if let Some(snapshot) = accepted(pool, playout_id)
        .await
        .map_err(|_| ExternalJsonError::Snapshot)?
    {
        return Ok(Some(snapshot.schedule));
    }
    if live_path.as_os_str().is_empty() {
        return Ok(None);
    }
    super::load(live_path).await
}

/// Atomically replace an accepted document after source/revision/deadline checks.
///
/// This writes no library or materialized programme rows. The caller owns the
/// shared playout build lock and producer notification/acknowledgement lifecycle.
/// A first explicit replacement may create the baseline; preserving callers must
/// have reconciled against the exact `expected_revision` before this call.
///
/// # Errors
/// Refuses invalid/oversized documents, stale ownership/revision, expired
/// publication or database failure, leaving the previous snapshot intact.
pub async fn publish(
    pool: &DbPool,
    request: Publication<'_>,
    schedule: &ExternalJsonSchedule,
) -> Result<i64, SnapshotError> {
    check_deadline(request.deadline)?;
    let document = encode(schedule)?;
    let revision = request
        .expected_revision
        .unwrap_or(0)
        .checked_add(1)
        .filter(|revision| *revision > 0)
        .ok_or(SnapshotError::Invalid)?;
    let mut tx = crate::db::begin_write(pool)
        .await
        .map_err(|_| SnapshotError::Database)?;
    // The writer is reserved before reading ownership or the accepted revision.
    // This conditional no-op update also fences an intervening source/kind edit.
    let owned = sqlx::query(
        r#"UPDATE "Playout" SET "Id"="Id" WHERE "Id"=?
           AND "ScheduleKind" IN (20, 'ExternalJson') AND "ScheduleFile"=?"#,
    )
    .bind(request.playout_id)
    .bind(request.schedule_file)
    .execute(&mut *tx)
    .await
    .map_err(|_| SnapshotError::Database)?
    .rows_affected();
    check_deadline(request.deadline)?;
    if owned != 1 {
        return Err(SnapshotError::ChangedSource);
    }
    let current: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Revision" FROM "PlayoutExternalJsonSnapshot" WHERE "PlayoutId"=?"#,
    )
    .bind(request.playout_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| SnapshotError::Database)?;
    if current != request.expected_revision {
        return Err(SnapshotError::Superseded);
    }
    sqlx::query(
        r#"INSERT INTO "PlayoutExternalJsonSnapshot"
           ("PlayoutId", "Revision", "SchemaVersion", "ScheduleFile", "Document", "PublishedAt", "EffectiveFrom")
           VALUES (?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT("PlayoutId") DO UPDATE SET "Revision"=excluded."Revision",
           "SchemaVersion"=excluded."SchemaVersion", "ScheduleFile"=excluded."ScheduleFile",
           "Document"=excluded."Document", "PublishedAt"=excluded."PublishedAt",
           "EffectiveFrom"=excluded."EffectiveFrom""#,
    ).bind(request.playout_id).bind(revision).bind(SCHEMA_VERSION)
        .bind(request.schedule_file).bind(document).bind(request.published_at.to_rfc3339())
        .bind(request.effective_from.map(|time| time.to_rfc3339()))
        .execute(&mut *tx).await.map_err(|_| SnapshotError::Database)?;
    check_deadline(request.deadline)?;
    tx.commit().await.map_err(|_| SnapshotError::Database)?;
    Ok(revision)
}

fn check_deadline(deadline: Option<Instant>) -> Result<(), SnapshotError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        Err(SnapshotError::Expired)
    } else {
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Document<'a> {
    start_time: String,
    guide_minimum_duration_seconds: i32,
    programs: Programs<'a>,
}

struct Programs<'a>(&'a [ExternalJsonItem]);
impl Serialize for Programs<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(|item| &item.program))
    }
}

fn encode(schedule: &ExternalJsonSchedule) -> Result<String, SnapshotError> {
    super::adaptation::validate(schedule).map_err(|_| SnapshotError::Invalid)?;
    let document = Document {
        start_time: schedule.items.first().map_or_else(
            || "1970-01-01T00:00:00Z".into(),
            |item| item.start.to_rfc3339(),
        ),
        guide_minimum_duration_seconds: schedule.guide_minimum_duration_seconds,
        programs: Programs(&schedule.items),
    };
    let mut writer = BoundedDocument {
        bytes: Vec::new(),
        exceeded: false,
    };
    if serde_json::to_writer(&mut writer, &document).is_err() {
        return Err(if writer.exceeded {
            SnapshotError::TooLarge
        } else {
            SnapshotError::Invalid
        });
    }
    String::from_utf8(writer.bytes).map_err(|_| SnapshotError::Invalid)
}

struct BoundedDocument {
    bytes: Vec<u8>,
    exceeded: bool,
}
impl Write for BoundedDocument {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) as u64 > MAX_SCHEDULE_BYTES {
            self.exceeded = true;
            return Err(std::io::Error::other("external JSON snapshot size limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
