//! Connection-owned image staging for atomic receiving list reconstruction.

use std::path::Path;

use ersatzrs_core::repositories::{AddonMediaListArtworkRole, RepositoryError, Result};
use ersatzrs_domain::EntityId;
use sqlx::{pool::PoolConnection, Connection as _, Transaction};

use super::files::validate_reference;
use super::model::SavedFile;
use crate::addon_artwork::MAX_ARTWORK_BYTES;
use crate::db::{Db, DbPool};

/// Holds verified images in `SQLite` TEMP storage, never placeholder list rows.
///
/// Stage one file at a time before opening the main write transaction. TEMP is
/// connection-local, so the final projection must use [`Self::begin_projection`]
/// on this same connection. It is closed on drop rather than returned to the
/// pool, including failed/cancelled admission; no persistent recovery marker or
/// TEMP cleanup job is needed. File-backed TEMP may still use `SQLite`'s page cache.
///
/// Caller owns enabled/source validation and the managed-tree/operation fence.
/// Acquire that fence before staging and before beginning the main transaction;
/// recheck the live claim inside the transaction before changing receiving rows.
/// This type does not create owners, map metadata, commit, or activate playback.
/// The final transaction still writes all receiving image BLOBs; its duration
/// scales with their size. Only source reads/validation are kept outside it.
pub struct ArtworkStaging {
    connection: PoolConnection<Db>,
}

impl ArtworkStaging {
    /// Acquire an operation-owned connection with file-backed TEMP storage.
    ///
    /// # Errors
    /// Refuses unavailable connection or local temporary storage without creating owners.
    pub async fn new(pool: &DbPool) -> Result<Self> {
        let mut connection = pool.acquire().await.map_err(|_| unavailable())?;
        connection.close_on_drop();
        sqlx::query("PRAGMA temp_store=FILE")
            .execute(&mut *connection)
            .await
            .map_err(|_| unavailable())?;
        sqlx::query(
            r#"CREATE TEMP TABLE "SavedListArtwork" (
            "Path" TEXT NOT NULL PRIMARY KEY,
            "Sha256" TEXT NOT NULL,
            "ContentType" TEXT NOT NULL,
            "Bytes" BLOB NOT NULL)"#,
        )
        .execute(&mut *connection)
        .await
        .map_err(|_| unavailable())?;
        Ok(Self { connection })
    }

    /// Read one exact verified image into TEMP without taking main write ownership.
    /// Identical references reuse staged bytes; a changed identity is a conflict.
    ///
    /// # Errors
    /// Refuses invalid/conflicting references, bad image bytes or temporary storage failure.
    pub async fn stage_file(&mut self, root: &Path, file: &SavedFile) -> Result<()> {
        validate_image_reference(file)?;
        let existing: Option<(String, i64)> = sqlx::query_as(
            r#"SELECT "Sha256", length("Bytes") FROM temp."SavedListArtwork" WHERE "Path"=?"#,
        )
        .bind(&file.relative_path)
        .fetch_optional(&mut *self.connection)
        .await
        .map_err(|_| unavailable())?;
        if let Some((hash, length)) = existing {
            return if hash == file.sha256 && u64::try_from(length).ok() == Some(file.byte_length) {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        let (bytes, content_type) = super::receiving::read_artwork(root, file)
            .await
            .map_err(|_| invalid())?;
        sqlx::query(
            r#"INSERT INTO temp."SavedListArtwork" ("Path", "Sha256", "ContentType", "Bytes")
            VALUES (?,?,?,?)"#,
        )
        .bind(&file.relative_path)
        .bind(&file.sha256)
        .bind(content_type)
        .bind(bytes)
        .execute(&mut *self.connection)
        .await
        .map_err(|_| unavailable())?;
        Ok(())
    }

    /// Begin the receiving projection on the connection that owns the staged images.
    ///
    /// # Errors
    /// Returns an error if main write ownership cannot be acquired. Never wait for
    /// a managed-tree fence or read source files after this transaction begins.
    pub async fn begin_projection(&mut self) -> Result<Transaction<'_, Db>> {
        self.connection
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| unavailable())
    }
}

/// Allocate/reuse a receiving-owned handle from this transaction's TEMP images.
///
/// Use the existing editor staging implementation for role/scope ownership,
/// image validation and deduplication. Reads only locally staged bytes, never
/// the copied folder. Only one bounded image is loaded at a time. The caller
/// preserves typed roles and uses Thumb for untyped `artwork_urls`; person metadata
/// remains literal. Receiving list/item rows must exist in this transaction.
///
/// # Errors
/// Refuses missing/different staged content, invalid roles or missing/wrong owners.
pub async fn bind_artwork_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    item_id: Option<EntityId>,
    role: AddonMediaListArtworkRole,
    file: &SavedFile,
) -> Result<String> {
    validate_image_reference(file)?;
    let (content_type, bytes): (String, Vec<u8>) = sqlx::query_as(
        r#"SELECT "ContentType", "Bytes" FROM temp."SavedListArtwork"
        WHERE "Path"=? AND "Sha256"=? AND length("Bytes")=? AND length("Bytes") BETWEEN 1 AND ?"#,
    )
    .bind(&file.relative_path)
    .bind(&file.sha256)
    .bind(i64::try_from(file.byte_length).map_err(|_| invalid())?)
    .bind(i64::try_from(MAX_ARTWORK_BYTES).map_err(|_| invalid())?)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(invalid)?;
    crate::repositories::stage_editor_artwork_in_tx(
        tx,
        list_id,
        item_id,
        role,
        &content_type,
        &bytes,
    )
    .await
}

fn validate_image_reference(file: &SavedFile) -> Result<()> {
    validate_reference(file).map_err(|_| invalid())?;
    if file.byte_length == 0 || file.byte_length > MAX_ARTWORK_BYTES as u64 {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("the saved artwork content is unavailable or changed".into())
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "stage saved artwork",
        "local temporary artwork storage or database access is unavailable",
    )
}

#[cfg(test)]
mod tests;

mod metadata;
pub use metadata::{bind_map_in_tx, bind_metadata_in_tx};
