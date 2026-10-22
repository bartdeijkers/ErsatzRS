//! Versioned bounded body/context child; captured data grants no playback lease.

use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{SourceTeletextObservedPages, SourceTeletextPageWindow};
use ersatzrs_ffmpeg::teletext::validate_source_page_observations;
use sqlx::Transaction;

use super::{column, invalid, storage};
use crate::db::Db;

pub(super) const MAX_PAGE_WINDOW_BYTES: usize = 2_097_152;
const SCHEMA_VERSION: i64 = 1;

pub(super) async fn replace(
    tx: &mut Transaction<'_, Db>,
    id: i64,
    observed: &SourceTeletextObservedPages,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "SourceTeletextPageWindow" WHERE "SourceTeletextMetadataId"=?"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|_| storage())?;
    let Some(pages) = observed.page_window() else {
        return Ok(());
    };
    let headers = observed
        .headers()
        .ok_or_else(|| invalid("page window requires observed headers"))?;
    validate_source_page_observations(pages, headers)
        .map_err(|_| invalid("page window structure"))?;
    let payload = serde_json::to_string(pages).map_err(|_| invalid("page window encoding"))?;
    if payload.len() > MAX_PAGE_WINDOW_BYTES {
        return Err(invalid("page window byte bound"));
    }
    sqlx::query(r#"INSERT INTO "SourceTeletextPageWindow" ("SourceTeletextMetadataId", "SchemaVersion", "Payload") VALUES (?, ?, ?)"#)
        .bind(id).bind(SCHEMA_VERSION).bind(payload)
        .execute(&mut **tx).await.map_err(|_| storage())?;
    Ok(())
}

pub(super) async fn load(
    tx: &mut Transaction<'_, Db>,
    id: i64,
    observed: SourceTeletextObservedPages,
) -> Result<SourceTeletextObservedPages> {
    // Enforce the byte/type bound in SQLite before moving untrusted JSON into
    // Rust. Corruption is an error, never an absent or ready page window.
    let row = sqlx::query(
        r#"SELECT "SchemaVersion",
        CASE WHEN TYPEOF("Payload")='text' AND LENGTH(CAST("Payload" AS BLOB)) BETWEEN 1 AND ?
        THEN "Payload" ELSE NULL END AS "Payload"
        FROM "SourceTeletextPageWindow" WHERE "SourceTeletextMetadataId"=?"#,
    )
    .bind(i64::try_from(MAX_PAGE_WINDOW_BYTES).map_err(|_| invalid("page window byte bound"))?)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| storage())?;
    let Some(row) = row else {
        return Ok(observed);
    };
    if column::<i64>(&row, "SchemaVersion")? != SCHEMA_VERSION {
        return Err(invalid("page window schema version"));
    }
    let SourceTeletextObservedPages::HeadersObserved(headers) = observed else {
        return Err(invalid("page window requires observed headers"));
    };
    let payload: Option<String> = column(&row, "Payload")?;
    let pages: SourceTeletextPageWindow =
        serde_json::from_str(&payload.ok_or_else(|| invalid("page window byte bound"))?)
            .map_err(|_| invalid("stored page window structure"))?;
    validate_source_page_observations(&pages, &headers)
        .map_err(|_| invalid("stored page window structure"))?;
    Ok(SourceTeletextObservedPages::PagesObserved {
        headers,
        pages: Box::new(pages),
    })
}
