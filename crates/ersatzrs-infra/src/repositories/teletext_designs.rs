//! ADR 0123 opaque design storage; persistence grants no output authority.

use std::{fmt::Write as _, mem::size_of};

use async_trait::async_trait;
use ersatzrs_core::repositories::{
    RepositoryError, Result, TeletextDesignMutation as M, TeletextDesignRejection as R,
    TeletextDesignRepository, TeletextDesignSave,
};
use ersatzrs_domain::{
    EntityId, TeletextDesignHead, TeletextDesignRevision, MAX_TELETEXT_DESIGN_NAME_BYTES,
    TELETEXT_DESIGN_DOCUMENT_VERSION,
};
use sha2::{Digest as _, Sha256};
use sqlx::{Executor, Row as _, Transaction};

use super::SqlxResultExt as _;
use crate::db::{Db, DbPool, DbRow};

/// Atomic heads and exact immutable documents, independent of producer lifetime.
#[derive(Clone)]
pub struct SqliteTeletextDesignRepository(pub DbPool);

// All SQL structure is static; only bound values come from callers.
macro_rules! head_sql {
    ($suffix:literal) => {
        concat!(
            r#"SELECT "Id",
    CASE WHEN TYPEOF("Revision") = 'integer' THEN "Revision" ELSE NULL END AS "Revision",
    CASE WHEN TYPEOF("IsDeleted") = 'integer' THEN "IsDeleted" ELSE NULL END AS "IsDeleted",
    TYPEOF("Revision") AS "RevisionType", TYPEOF("IsDeleted") AS "DeletedType",
    CASE WHEN TYPEOF("Name") = 'text' AND LENGTH(CAST("Name" AS BLOB)) BETWEEN 1 AND 256
        THEN "Name" ELSE NULL END AS "BoundedName" FROM "TeletextDesign" "#,
            $suffix
        )
    };
}

#[async_trait]
impl TeletextDesignRepository for SqliteTeletextDesignRepository {
    async fn head(&self, id: EntityId) -> Result<Option<TeletextDesignHead>> {
        head_with_executor(&self.0, id).await
    }

    async fn revision(
        &self,
        id: EntityId,
        revision: i64,
        max_document_bytes: usize,
    ) -> Result<Option<TeletextDesignRevision>> {
        revision_with_executor(&self.0, id, revision, max_document_bytes).await
    }

    async fn heads(
        &self,
        after_id: Option<EntityId>,
        limit: usize,
    ) -> Result<Vec<TeletextDesignHead>> {
        if let Some(id) = after_id {
            validate_identity(id)?;
        }
        let bytes = limit
            .checked_mul(size_of::<TeletextDesignHead>() + MAX_TELETEXT_DESIGN_NAME_BYTES)
            .ok_or_else(|| invalid("listing budget"))?;
        if isize::try_from(bytes).is_err() {
            return Err(invalid("listing budget"));
        }
        let limit = i64::try_from(limit).map_err(|_| invalid("listing budget"))?;
        let rows = sqlx::query(head_sql!(r#"WHERE "Id" > ? ORDER BY "Id" LIMIT ?"#))
            .bind(after_id.unwrap_or(0))
            .bind(limit)
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter().map(decode_head).collect()
    }

    async fn save(
        &self,
        save: &TeletextDesignSave,
        max_document_bytes: usize,
    ) -> Result<M<TeletextDesignHead>> {
        if !valid_save(save, max_document_bytes) {
            return Ok(M::Rejected(R::InvalidValues));
        }
        let digest = sha256_hex(&save.document);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let (id, revision) = if let Some((id, expected)) = save.existing {
            let Some(row) = sqlx::query(head_sql!(r#"WHERE "Id" = ?"#))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .repository()?
            else {
                return Ok(M::Rejected(R::MissingDesign));
            };
            let head = decode_head(&row)?;
            if let Some(reason) = mutation_rejection(&head, expected) {
                return Ok(M::Rejected(reason));
            }
            let Some(next) = head.revision.checked_add(1) else {
                return Ok(M::Rejected(R::RevisionLimit));
            };
            sqlx::query(r#"UPDATE "TeletextDesign" SET "Name" = ?, "Revision" = ? WHERE "Id" = ?"#)
                .bind(&save.name)
                .bind(next)
                .bind(id)
                .execute(&mut *tx)
                .await
                .repository()?;
            (id, next)
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "TeletextDesign" ("Name", "Revision") VALUES (?, 1)"#)
                    .bind(&save.name)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            (result.last_insert_rowid(), 1)
        };
        sqlx::query(r#"INSERT INTO "TeletextDesignRevision"
            ("DesignId", "Revision", "DocumentVersion", "Digest", "Document") VALUES (?, ?, ?, ?, ?)"#)
            .bind(id).bind(revision).bind(i64::from(save.document_version)).bind(digest)
            .bind(&save.document).execute(&mut *tx).await.repository()?;
        tx.commit().await.repository()?;
        Ok(M::Saved(TeletextDesignHead {
            id,
            name: save.name.clone(),
            revision,
            deleted: false,
        }))
    }

    async fn tombstone(
        &self,
        id: EntityId,
        expected_revision: i64,
    ) -> Result<M<TeletextDesignHead>> {
        if id <= 0 || expected_revision <= 0 {
            return Ok(M::Rejected(R::InvalidValues));
        }
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let Some(row) = sqlx::query(head_sql!(r#"WHERE "Id" = ?"#))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
        else {
            return Ok(M::Rejected(R::MissingDesign));
        };
        let mut head = decode_head(&row)?;
        if let Some(reason) = mutation_rejection(&head, expected_revision) {
            return Ok(M::Rejected(reason));
        }
        sqlx::query(r#"UPDATE "TeletextDesign" SET "IsDeleted" = 1 WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
        tx.commit().await.repository()?;
        head.deleted = true;
        Ok(M::Saved(head))
    }
}

// The caller owns the snapshot/write exclusion. Never open a pooled read here.
pub(super) async fn head_in_transaction(
    tx: &mut Transaction<'_, Db>,
    id: EntityId,
) -> Result<Option<TeletextDesignHead>> {
    head_with_executor(&mut **tx, id).await
}

async fn head_with_executor<'e>(
    executor: impl Executor<'e, Database = Db>,
    id: EntityId,
) -> Result<Option<TeletextDesignHead>> {
    validate_identity(id)?;
    sqlx::query(head_sql!(r#"WHERE "Id" = ?"#))
        .bind(id)
        .fetch_optional(executor)
        .await
        .repository()?
        .as_ref()
        .map(decode_head)
        .transpose()
}

pub(super) async fn revision_in_transaction(
    tx: &mut Transaction<'_, Db>,
    id: EntityId,
    revision: i64,
    max_document_bytes: usize,
) -> Result<Option<TeletextDesignRevision>> {
    revision_with_executor(&mut **tx, id, revision, max_document_bytes).await
}

async fn revision_with_executor<'e>(
    executor: impl Executor<'e, Database = Db>,
    id: EntityId,
    revision: i64,
    max_document_bytes: usize,
) -> Result<Option<TeletextDesignRevision>> {
    validate_identity(id)?;
    if revision <= 0 {
        return Err(invalid("revision"));
    }
    let maximum = i64::try_from(max_document_bytes).map_err(|_| invalid("document budget"))?;
    // CASE prevents SQLx from receiving oversized bodies or malformed metadata.
    let row = sqlx::query(
        r#"SELECT "DesignId", "Revision",
            CASE WHEN TYPEOF("DocumentVersion") = 'integer'
                THEN "DocumentVersion" ELSE NULL END AS "DocumentVersion",
            TYPEOF("DocumentVersion") AS "VersionType", TYPEOF("Document") AS "DocumentType",
            LENGTH("Document") AS "DocumentBytes",
            CASE WHEN TYPEOF("Digest") = 'text' AND LENGTH(CAST("Digest" AS BLOB)) = 64
                THEN "Digest" ELSE NULL END AS "BoundedDigest",
            CASE WHEN TYPEOF("Document") = 'blob' AND LENGTH("Document") BETWEEN 1 AND ?
                THEN "Document" ELSE NULL END AS "BoundedDocument"
            FROM "TeletextDesignRevision" WHERE "DesignId" = ? AND "Revision" = ?"#,
    )
    .bind(maximum)
    .bind(id)
    .bind(revision)
    .fetch_optional(executor)
    .await
    .repository()?;
    row.as_ref().map(decode_revision).transpose()
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= MAX_TELETEXT_DESIGN_NAME_BYTES && !name.contains('\0')
}

fn valid_save(save: &TeletextDesignSave, maximum: usize) -> bool {
    valid_name(&save.name)
        && save.document_version == TELETEXT_DESIGN_DOCUMENT_VERSION
        && !save.document.is_empty()
        && save.document.len() <= maximum
        && save
            .existing
            .is_none_or(|(id, revision)| id > 0 && revision > 0)
}

fn mutation_rejection(head: &TeletextDesignHead, expected: i64) -> Option<R> {
    if head.deleted {
        Some(R::DeletedDesign)
    } else if head.revision != expected {
        Some(R::Conflict)
    } else {
        None
    }
}

fn decode_head(row: &DbRow) -> Result<TeletextDesignHead> {
    let name: Option<String> = row.try_get("BoundedName").repository()?;
    let name = name
        .filter(|name| valid_name(name))
        .ok_or_else(|| corrupt("name"))?;
    let id: i64 = row.try_get("Id").repository()?;
    let revision: Option<i64> = row.try_get("Revision").repository()?;
    let revision = revision.ok_or_else(|| corrupt("head metadata"))?;
    let deleted: Option<i64> = row.try_get("IsDeleted").repository()?;
    let deleted = deleted.ok_or_else(|| corrupt("head metadata"))?;
    let revision_type: String = row.try_get("RevisionType").repository()?;
    let deleted_type: String = row.try_get("DeletedType").repository()?;
    if id <= 0
        || revision <= 0
        || revision_type != "integer"
        || deleted_type != "integer"
        || !matches!(deleted, 0 | 1)
    {
        return Err(corrupt("head metadata"));
    }
    Ok(TeletextDesignHead {
        id,
        name,
        revision,
        deleted: deleted == 1,
    })
}

fn decode_revision(row: &DbRow) -> Result<TeletextDesignRevision> {
    let version_type: String = row.try_get("VersionType").repository()?;
    let version: Option<i64> = row.try_get("DocumentVersion").repository()?;
    let version = version.ok_or_else(|| corrupt("revision metadata"))?;
    let document_type: String = row.try_get("DocumentType").repository()?;
    let stored_length: i64 = row.try_get("DocumentBytes").repository()?;
    let document: Option<Vec<u8>> = row.try_get("BoundedDocument").repository()?;
    let digest: Option<String> = row.try_get("BoundedDigest").repository()?;
    let document = document.ok_or_else(|| corrupt("document bounds or type"))?;
    let digest = digest.ok_or_else(|| corrupt("digest bounds or type"))?;
    if version_type != "integer"
        || version != i64::from(TELETEXT_DESIGN_DOCUMENT_VERSION)
        || document_type != "blob"
        || usize::try_from(stored_length).ok() != Some(document.len())
        || digest != sha256_hex(&document)
    {
        return Err(corrupt("revision integrity"));
    }
    Ok(TeletextDesignRevision {
        design_id: row.try_get("DesignId").repository()?,
        revision: row.try_get("Revision").repository()?,
        document_version: TELETEXT_DESIGN_DOCUMENT_VERSION,
        digest,
        document,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn validate_identity(id: EntityId) -> Result<()> {
    if id > 0 {
        Ok(())
    } else {
        Err(invalid("design identity"))
    }
}

fn invalid(field: &'static str) -> RepositoryError {
    RepositoryError::Validation {
        field,
        message: "invalid design storage request".to_owned(),
    }
}

fn corrupt(value: &'static str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "teletext design storage",
        value: value.to_owned(),
    }
}

#[cfg(test)]
#[path = "teletext_designs_tests.rs"]
mod tests;
