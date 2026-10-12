//! Accepted external timelines shared by playback and guide readers (ADR 0110).
//!
//! Absence preserves legacy direct-file reading. Once accepted, a document is
//! authoritative until another guarded publication; file edits alone cannot
//! bypass programme preservation. Never fall back from a corrupt snapshot.

use std::{io::Write, path::Path, time::Instant};

use chrono::{DateTime, Utc};
use serde::{
    de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor},
    Deserialize, Serialize, Serializer,
};
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

/// Trusted remaining limits for one accepted document, before any filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedSnapshotBudget {
    pub max_document_bytes: usize,
    pub max_path_bytes: usize,
    pub max_total_bytes: usize,
    pub max_programs: usize,
}

/// Exact source UTF-8 byte counts read from storage and all raw programme records.
/// These are input costs, not retained projection or cloned-content totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedSnapshotUsage {
    pub document_bytes: usize,
    pub path_bytes: usize,
    pub programs: usize,
}

/// A bounded accepted revision; diagnostics never expose paths or programmes.
pub struct BoundedSnapshot {
    snapshot: Snapshot,
    usage: AcceptedSnapshotUsage,
}

impl BoundedSnapshot {
    #[must_use]
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    #[must_use]
    pub const fn usage(&self) -> AcceptedSnapshotUsage {
        self.usage
    }

    #[must_use]
    pub fn into_snapshot(self) -> Snapshot {
        self.snapshot
    }
}

impl std::fmt::Debug for BoundedSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BoundedSnapshot")
            .field("revision", &self.snapshot.revision)
            .field("usage", &self.usage)
            .finish_non_exhaustive()
    }
}

/// Numeric/static diagnostics only; never include stored input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AcceptedSnapshotError {
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error("external JSON accepted {category} count {count} exceeds limit {maximum}")]
    Budget {
        category: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("external JSON accepted planning size arithmetic failed")]
    Size,
}

/// Read only the accepted revision through the caller's connection/transaction.
///
/// Both stored strings are withheld by SQL unless their scalar types, revision,
/// schema and separate/combined byte limits pass. Raw programmes are counted
/// before the existing schedule parser allocates programme records. This neither
/// accepts a mutable file nor claims complete guide admission.
///
/// # Errors
/// Rejects corrupt storage, unsupported revisions/schema, byte/record limits,
/// size arithmetic overflow and database failures with redacted diagnostics.
pub async fn accepted_on(
    connection: &mut sqlx::SqliteConnection,
    playout_id: i64,
    budget: AcceptedSnapshotBudget,
) -> Result<Option<BoundedSnapshot>, AcceptedSnapshotError> {
    let document_limit =
        i64::try_from(MAX_SCHEDULE_BYTES).map_err(|_| AcceptedSnapshotError::Size)?;
    let row = sqlx::query(
        r#"WITH measured AS (
          SELECT *, typeof("Revision") AS revision_type,
            typeof("SchemaVersion") AS version_type,
            typeof("Document") AS document_type,
            typeof("ScheduleFile") AS path_type,
            length(CAST("Document" AS BLOB)) AS document_bytes,
            length(CAST("ScheduleFile" AS BLOB)) AS path_bytes
          FROM "PlayoutExternalJsonSnapshot" WHERE "PlayoutId"=?
        ), guarded AS (
          SELECT *, revision_type='integer' AND "Revision">0
            AND version_type='integer' AND "SchemaVersion"=1
            AND document_type='text' AND path_type='text'
            AND document_bytes<=? AND path_bytes<=?
            AND document_bytes+path_bytes<=? AS permitted FROM measured
        ) SELECT revision_type, version_type, document_type, path_type,
          CASE WHEN revision_type='integer' THEN "Revision" END AS revision,
          CASE WHEN version_type='integer' THEN "SchemaVersion" END AS version,
          document_bytes, path_bytes,
          CASE WHEN permitted THEN "Document" END AS document,
          CASE WHEN permitted THEN "ScheduleFile" END AS path FROM guarded"#,
    )
    .bind(playout_id)
    .bind(sql_limit(budget.max_document_bytes).min(document_limit))
    .bind(sql_limit(budget.max_path_bytes))
    .bind(sql_limit(budget.max_total_bytes))
    .fetch_optional(connection)
    .await
    .map_err(|_| SnapshotError::Database)?;
    let Some(row) = row else { return Ok(None) };
    let (revision, mut usage) = checked_measurements(&row, budget)?;
    let document: String = row
        .try_get("document")
        .map_err(|_| SnapshotError::Invalid)?;
    let schedule_file: String = row.try_get("path").map_err(|_| SnapshotError::Invalid)?;
    usage.programs = count_programs(document.as_bytes(), budget.max_programs)?;
    usage
        .programs
        .checked_mul(std::mem::size_of::<ExternalJsonItem>())
        .filter(|bytes| isize::try_from(*bytes).is_ok())
        .ok_or(AcceptedSnapshotError::Size)?;
    let schedule = super::parse(document.as_bytes())
        .map_err(|_| SnapshotError::Invalid)?
        .ok_or(SnapshotError::Invalid)?;
    Ok(Some(BoundedSnapshot {
        snapshot: Snapshot {
            revision,
            schedule_file,
            schedule,
        },
        usage,
    }))
}

fn sql_limit(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

fn checked_measurements(
    row: &sqlx::sqlite::SqliteRow,
    budget: AcceptedSnapshotBudget,
) -> Result<(i64, AcceptedSnapshotUsage), AcceptedSnapshotError> {
    for (column, expected) in [
        ("revision_type", "integer"),
        ("version_type", "integer"),
        ("document_type", "text"),
        ("path_type", "text"),
    ] {
        let actual: &str = row.try_get(column).map_err(|_| SnapshotError::Invalid)?;
        if actual != expected {
            return Err(SnapshotError::Invalid.into());
        }
    }
    let revision: i64 = row
        .try_get("revision")
        .map_err(|_| SnapshotError::Invalid)?;
    let version: i64 = row.try_get("version").map_err(|_| SnapshotError::Invalid)?;
    if revision <= 0 || version != SCHEMA_VERSION {
        return Err(SnapshotError::Invalid.into());
    }
    let document_bytes = byte_count(row, "document_bytes")?;
    let path_bytes = byte_count(row, "path_bytes")?;
    if document_bytes as u64 > MAX_SCHEDULE_BYTES {
        return Err(SnapshotError::TooLarge.into());
    }
    check_limit("document bytes", document_bytes, budget.max_document_bytes)?;
    check_limit("path bytes", path_bytes, budget.max_path_bytes)?;
    let total = document_bytes
        .checked_add(path_bytes)
        .ok_or(AcceptedSnapshotError::Size)?;
    check_limit("total bytes", total, budget.max_total_bytes)?;
    Ok((
        revision,
        AcceptedSnapshotUsage {
            document_bytes,
            path_bytes,
            programs: 0,
        },
    ))
}

fn byte_count(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<usize, AcceptedSnapshotError> {
    let count: i64 = row.try_get(column).map_err(|_| SnapshotError::Invalid)?;
    usize::try_from(count).map_err(|_| AcceptedSnapshotError::Size)
}

fn check_limit(
    category: &'static str,
    count: usize,
    maximum: usize,
) -> Result<(), AcceptedSnapshotError> {
    if count > maximum {
        return Err(AcceptedSnapshotError::Budget {
            category,
            count,
            maximum,
        });
    }
    Ok(())
}

struct ProgramCount<'a> {
    count: &'a mut usize,
    maximum: usize,
}
impl<'de> DeserializeSeed<'de> for ProgramCount<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for ProgramCount<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an external JSON document map")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        let mut seen = false;
        while let Some(field) = map.next_key::<CountField>()? {
            match field {
                CountField::Programs => {
                    if seen {
                        return Err(serde::de::Error::custom("duplicate programs"));
                    }
                    seen = true;
                    map.next_value_seed(ProgramArray {
                        count: self.count,
                        maximum: self.maximum,
                    })?;
                }
                CountField::Other => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        if !seen {
            return Err(serde::de::Error::custom("missing programs"));
        }
        Ok(())
    }
}

enum CountField {
    Programs,
    Other,
}
impl<'de> Deserialize<'de> for CountField {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldVisitor;
        impl Visitor<'_> for FieldVisitor {
            type Value = CountField;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a document field")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<CountField, E> {
                Ok(if value == "programs" {
                    CountField::Programs
                } else {
                    CountField::Other
                })
            }
        }
        deserializer.deserialize_identifier(FieldVisitor)
    }
}

struct ProgramArray<'a> {
    count: &'a mut usize,
    maximum: usize,
}
impl<'de> DeserializeSeed<'de> for ProgramArray<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for ProgramArray<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a programs array")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> Result<(), S::Error> {
        while sequence
            .next_element_seed(ProgramRecord {
                count: self.count,
                maximum: self.maximum,
            })?
            .is_some()
        {}
        Ok(())
    }
}
struct ProgramRecord<'a> {
    count: &'a mut usize,
    maximum: usize,
}
impl<'de> DeserializeSeed<'de> for ProgramRecord<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        *self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| serde::de::Error::custom("program count overflow"))?;
        if *self.count > self.maximum {
            return Err(serde::de::Error::custom("program limit"));
        }
        IgnoredAny::deserialize(deserializer).map(|_| ())
    }
}

fn count_programs(bytes: &[u8], maximum: usize) -> Result<usize, AcceptedSnapshotError> {
    let mut count = 0;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let result = ProgramCount {
        count: &mut count,
        maximum,
    }
    .deserialize(&mut deserializer);
    check_limit("programs", count, maximum)?;
    result.map_err(|_| SnapshotError::Invalid)?;
    deserializer.end().map_err(|_| SnapshotError::Invalid)?;
    Ok(count)
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
