//! Versioned bounded child for actual header observations; parent status stays zero.

use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{SourceTeletextHeaderWindow, SourceTeletextObservedPages};
use sqlx::Transaction;

use super::{column, invalid, storage};
use crate::db::Db;

/// Resource policy applied before JSON parsing and before database writes.
pub(super) const MAX_HEADER_WINDOW_BYTES: usize = 1_048_576;
const SCHEMA_VERSION: i64 = 1;

pub(super) async fn replace(
    tx: &mut Transaction<'_, Db>,
    id: i64,
    observed: &SourceTeletextObservedPages,
) -> Result<()> {
    // Absence explicitly clears the previous bounded read on every replacement.
    sqlx::query(r#"DELETE FROM "SourceTeletextHeaderWindow" WHERE "SourceTeletextMetadataId"=?"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|_| storage())?;
    if let SourceTeletextObservedPages::HeadersObserved(window) = observed {
        window
            .validate()
            .map_err(|_| invalid("header window structure"))?;
        let payload =
            serde_json::to_string(window).map_err(|_| invalid("header window encoding"))?;
        if payload.len() > MAX_HEADER_WINDOW_BYTES {
            return Err(invalid("header window byte bound"));
        }
        sqlx::query(r#"INSERT INTO "SourceTeletextHeaderWindow" ("SourceTeletextMetadataId", "SchemaVersion", "Payload") VALUES (?, ?, ?)"#)
            .bind(id).bind(SCHEMA_VERSION).bind(payload)
            .execute(&mut **tx).await.map_err(|_| storage())?;
    }
    Ok(())
}

pub(super) async fn load(
    tx: &mut Transaction<'_, Db>,
    id: i64,
) -> Result<SourceTeletextObservedPages> {
    // SQL gates the untrusted text before SQLite transfers it into Rust. A
    // corrupt/oversized child is an error, never silently treated as absent.
    let row = sqlx::query(
        r#"SELECT "SchemaVersion",
        CASE WHEN TYPEOF("Payload")='text' AND LENGTH(CAST("Payload" AS BLOB)) BETWEEN 1 AND ?
        THEN "Payload" ELSE NULL END AS "Payload"
        FROM "SourceTeletextHeaderWindow" WHERE "SourceTeletextMetadataId"=?"#,
    )
    .bind(i64::try_from(MAX_HEADER_WINDOW_BYTES).map_err(|_| invalid("header window byte bound"))?)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| storage())?;
    let Some(row) = row else {
        return Ok(SourceTeletextObservedPages::NotAcquired);
    };
    if column::<i64>(&row, "SchemaVersion")? != SCHEMA_VERSION {
        return Err(invalid("header window schema version"));
    }
    let payload: Option<String> = column(&row, "Payload")?;
    let payload = payload.ok_or_else(|| invalid("header window byte bound"))?;
    let window: SourceTeletextHeaderWindow =
        serde_json::from_str(&payload).map_err(|_| invalid("stored header window structure"))?;
    window
        .validate()
        .map_err(|_| invalid("stored header window structure"))?;
    Ok(SourceTeletextObservedPages::HeadersObserved(Box::new(
        window,
    )))
}
