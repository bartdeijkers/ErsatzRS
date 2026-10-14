//! Opaque immutable export storage, using the design repository and transaction.

use ersatzrs_core::repositories::TeletextDesignExportSave;
use ersatzrs_domain::{
    TeletextDesignExportMetadata, TeletextDesignExportReceipt,
    TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
};
use sqlx::Row as _;

use super::*;

pub(super) async fn create(
    pool: &DbPool,
    save: &TeletextDesignExportSave,
    max_receipt_bytes: usize,
    max_document_bytes: usize,
) -> Result<M<TeletextDesignExportMetadata>> {
    if !save.is_valid(max_receipt_bytes, max_document_bytes) {
        return Ok(M::Rejected(R::InvalidValues));
    }
    let digest = sha256_hex(&save.receipt);
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let Some(head) = head_in_transaction(&mut tx, save.design_id).await? else {
        return Ok(M::Rejected(R::MissingDesign));
    };
    if let Some(reason) = mutation_rejection(&head, save.expected_design_head) {
        return Ok(M::Rejected(reason));
    }
    let Some(original) =
        revision_in_transaction(&mut tx, save.design_id, save.revision, max_document_bytes).await?
    else {
        return Ok(M::Rejected(R::MissingDesign));
    };
    if original.document_version != TELETEXT_DESIGN_DOCUMENT_VERSION {
        return Ok(M::Rejected(R::InvalidValues));
    }
    if original.digest != save.design_digest {
        return Ok(M::Rejected(R::Conflict));
    }
    drop(original);
    if identity_exhausted(&mut tx).await? {
        return Ok(M::Rejected(R::RevisionLimit));
    }
    let result = sqlx::query(
        r#"INSERT INTO "TeletextDesignExportReceipt"
        ("DesignId", "Revision", "DesignDigest", "ExpectedDesignHead", "ReceiptVersion", "Digest", "Receipt")
        VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(save.design_id)
    .bind(save.revision)
    .bind(&save.design_digest)
    .bind(save.expected_design_head)
    .bind(i64::from(save.receipt_version))
    .bind(&digest)
    .bind(&save.receipt)
    .execute(&mut *tx)
    .await
    .repository()?;
    let metadata = TeletextDesignExportMetadata {
        id: result.last_insert_rowid(),
        design_id: save.design_id,
        revision: save.revision,
        design_digest: save.design_digest.clone(),
        expected_design_head: save.expected_design_head,
        receipt_version: TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
        digest,
    };
    tx.commit().await.repository()?;
    Ok(M::Saved(metadata))
}

pub(super) async fn read(
    pool: &DbPool,
    id: EntityId,
    max_receipt_bytes: usize,
    max_document_bytes: usize,
) -> Result<Option<TeletextDesignExportReceipt>> {
    validate_identity(id)?;
    let maximum = bounded_length(max_receipt_bytes).ok_or_else(|| invalid("receipt budget"))?;
    bounded_length(max_document_bytes).ok_or_else(|| invalid("document budget"))?;
    // This read-only transaction binds receipt and original integrity checks
    // to one SQLite snapshot without taking write ownership.
    let mut tx = pool.begin().await.repository()?;
    let row = sqlx::query(
        r#"SELECT "Id",
        CASE WHEN TYPEOF("DesignId") = 'integer' THEN "DesignId" ELSE NULL END AS "DesignId",
        CASE WHEN TYPEOF("Revision") = 'integer' THEN "Revision" ELSE NULL END AS "Revision",
        CASE WHEN TYPEOF("ExpectedDesignHead") = 'integer' THEN "ExpectedDesignHead" ELSE NULL END AS "ExpectedDesignHead",
        CASE WHEN TYPEOF("ReceiptVersion") = 'integer' THEN "ReceiptVersion" ELSE NULL END AS "ReceiptVersion",
        TYPEOF("Receipt") AS "ReceiptType", LENGTH("Receipt") AS "ReceiptBytes",
        CASE WHEN TYPEOF("DesignDigest") = 'text' AND LENGTH(CAST("DesignDigest" AS BLOB)) = 64
            THEN "DesignDigest" ELSE NULL END AS "BoundedDesignDigest",
        CASE WHEN TYPEOF("Digest") = 'text' AND LENGTH(CAST("Digest" AS BLOB)) = 64
            THEN "Digest" ELSE NULL END AS "BoundedDigest",
        CASE WHEN TYPEOF("Receipt") = 'blob' AND LENGTH("Receipt") BETWEEN 1 AND ?
            THEN "Receipt" ELSE NULL END AS "BoundedReceipt"
        FROM "TeletextDesignExportReceipt" WHERE "Id" = ?"#,
    )
    .bind(maximum)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let receipt = decode(&row)?;
    let Some(original) = revision_in_transaction(
        &mut tx,
        receipt.metadata.design_id,
        receipt.metadata.revision,
        max_document_bytes,
    )
    .await?
    else {
        return Err(corrupt("export original revision"));
    };
    if original.document_version != TELETEXT_DESIGN_DOCUMENT_VERSION {
        return Err(corrupt("export original version"));
    }
    if original.digest != receipt.metadata.design_digest {
        return Err(corrupt("export original digest"));
    }
    tx.commit().await.repository()?;
    Ok(Some(receipt))
}

fn bounded_length(maximum: usize) -> Option<i64> {
    isize::try_from(maximum).ok()?;
    i64::try_from(maximum).ok()
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

async fn identity_exhausted(tx: &mut Transaction<'_, Db>) -> Result<bool> {
    let row = sqlx::query(
        r#"SELECT CASE WHEN TYPEOF("seq") = 'integer' THEN "seq" ELSE NULL END AS "Sequence"
        FROM "sqlite_sequence" WHERE "name" = 'TeletextDesignExportReceipt'"#,
    )
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    let sequence = if let Some(row) = row {
        row.try_get::<Option<i64>, _>("Sequence")
            .repository()?
            .filter(|value| *value >= 0)
            .ok_or_else(|| corrupt("export identity counter"))?
    } else {
        0
    };
    let maximum: Option<i64> =
        sqlx::query_scalar(r#"SELECT MAX("Id") FROM "TeletextDesignExportReceipt""#)
            .fetch_one(&mut **tx)
            .await
            .repository()?;
    Ok(sequence.max(maximum.unwrap_or(0)) == i64::MAX)
}

fn decode(row: &DbRow) -> Result<TeletextDesignExportReceipt> {
    let integer = |field: &'static str| -> Result<i64> {
        row.try_get::<Option<i64>, _>(field)
            .repository()?
            .filter(|value| *value > 0)
            .ok_or_else(|| corrupt("export metadata"))
    };
    let version = integer("ReceiptVersion")?;
    let metadata = TeletextDesignExportMetadata {
        id: integer("Id")?,
        design_id: integer("DesignId")?,
        revision: integer("Revision")?,
        design_digest: row
            .try_get::<Option<String>, _>("BoundedDesignDigest")
            .repository()?
            .ok_or_else(|| corrupt("export design digest bounds or type"))?,
        expected_design_head: integer("ExpectedDesignHead")?,
        receipt_version: TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION,
        digest: row
            .try_get::<Option<String>, _>("BoundedDigest")
            .repository()?
            .ok_or_else(|| corrupt("export digest bounds or type"))?,
    };
    let receipt: Option<Vec<u8>> = row.try_get("BoundedReceipt").repository()?;
    let receipt = receipt.ok_or_else(|| corrupt("export receipt bounds or type"))?;
    let kind: String = row.try_get("ReceiptType").repository()?;
    let length: Option<i64> = row.try_get("ReceiptBytes").repository()?;
    if version != i64::from(TELETEXT_DESIGN_EXPORT_RECEIPT_VERSION)
        || metadata.revision > metadata.expected_design_head
        || !valid_digest(&metadata.design_digest)
        || !valid_digest(&metadata.digest)
        || metadata.digest != sha256_hex(&receipt)
        || kind != "blob"
        || length.and_then(|length| usize::try_from(length).ok()) != Some(receipt.len())
    {
        return Err(corrupt("export receipt integrity"));
    }
    Ok(TeletextDesignExportReceipt { metadata, receipt })
}

#[cfg(test)]
#[path = "teletext_design_exports_tests.rs"]
mod tests;
