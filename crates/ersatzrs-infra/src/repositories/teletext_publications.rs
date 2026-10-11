//! Immutable pending requests; this adapter never activates teletext output.

use async_trait::async_trait;
use ersatzrs_core::repositories::{
    RepositoryError, Result, TeletextProfileMutation, TeletextPublicationMutation as M,
    TeletextPublicationRejection as R, TeletextPublicationRepository,
    TeletextPublicationRequestSave,
};
use ersatzrs_domain::{EntityId, TeletextPublicationRequest, MAX_TELETEXT_PAGE_RECORDS};
use sqlx::{Row as _, Transaction};

use super::{
    teletext_designs::{head_in_transaction, revision_in_transaction},
    teletext_profiles::channel_snapshot_in_transaction,
    SqlxResultExt as _,
};
use crate::db::{Db, DbPool, DbRow};

/// Transactional pending request history and exact channel current-pointer CAS.
/// Stored intent carries no checked-page or active-publication authority.
#[derive(Clone)]
pub struct SqliteTeletextPublicationRepository(pub DbPool);

#[async_trait]
impl TeletextPublicationRepository for SqliteTeletextPublicationRepository {
    async fn request(
        &self,
        save: &TeletextPublicationRequestSave,
        max_document_bytes: usize,
    ) -> Result<M<TeletextPublicationRequest>> {
        if !valid_save(save) {
            return Ok(M::Rejected(R::InvalidValues));
        }
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let Some(head) = head_in_transaction(&mut tx, save.design_id).await? else {
            return Ok(M::Rejected(R::MissingDesign));
        };
        if head.deleted {
            return Ok(M::Rejected(R::DeletedDesign));
        }
        if head.revision != save.expected_design_head {
            return Ok(M::Rejected(R::DesignConflict));
        }
        let Some(revision) =
            revision_in_transaction(&mut tx, save.design_id, save.revision, max_document_bytes)
                .await?
        else {
            return Ok(M::Rejected(R::MissingRevision));
        };
        if revision.digest != save.digest {
            return Ok(M::Rejected(R::DigestConflict));
        }
        drop(revision);
        match channel_snapshot_in_transaction(&mut tx, save.selection.channel_id).await? {
            TeletextProfileMutation::Rejected(reason) => {
                return Ok(M::Rejected(R::SelectionUnavailable(reason)));
            }
            TeletextProfileMutation::Saved(current) if current != save.selection => {
                return Ok(M::Rejected(R::SelectionChanged));
            }
            TeletextProfileMutation::Saved(_) => {}
        }
        let current = current_in_transaction(&mut tx, save.selection.channel_id).await?;
        if current.as_ref().map(|request| request.id) != save.expected_request_id {
            return Ok(M::Rejected(R::RequestConflict));
        }
        let result = sqlx::query(
            r#"INSERT INTO "TeletextPublicationRequest"
            ("ChannelId", "DesignId", "Revision", "Digest") VALUES (?, ?, ?, ?)"#,
        )
        .bind(save.selection.channel_id)
        .bind(save.design_id)
        .bind(save.revision)
        .bind(&save.digest)
        .execute(&mut *tx)
        .await
        .repository()?;
        let id = result.last_insert_rowid();
        sqlx::query(
            r#"INSERT INTO "TeletextPublicationRequestCurrent" ("ChannelId", "RequestId")
            VALUES (?, ?) ON CONFLICT("ChannelId") DO UPDATE SET "RequestId" = excluded."RequestId""#,
        )
        .bind(save.selection.channel_id)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(M::Requested(TeletextPublicationRequest {
            id,
            channel_id: save.selection.channel_id,
            design_id: save.design_id,
            revision: save.revision,
            digest: save.digest.clone(),
        }))
    }

    async fn current(&self, channel_id: EntityId) -> Result<Option<TeletextPublicationRequest>> {
        if channel_id <= 0 {
            return Err(invalid_identity());
        }
        // Read-only transaction: pending pointer projection never promotes output.
        let mut tx = self.0.begin().await.repository()?;
        current_in_transaction(&mut tx, channel_id).await
    }
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn valid_save(save: &TeletextPublicationRequestSave) -> bool {
    save.design_id > 0
        && save.revision > 0
        && save.expected_design_head > 0
        && save.expected_request_id.is_none_or(|id| id > 0)
        && save.selection.channel_id > 0
        && !save.selection.bindings.is_empty()
        && save.selection.bindings.len() <= MAX_TELETEXT_PAGE_RECORDS
        && valid_digest(&save.digest)
}

async fn current_in_transaction(
    tx: &mut Transaction<'_, Db>,
    channel_id: EntityId,
) -> Result<Option<TeletextPublicationRequest>> {
    // A dangling or malformed pointer is an error, not a missing request.
    // Numeric CASE guards also exclude oversized corrupt TEXT/BLOB metadata.
    let row = sqlx::query(
        r#"SELECT
        CASE WHEN TYPEOF(p."RequestId") = 'integer' THEN p."RequestId" ELSE NULL END AS "Pointer",
        CASE WHEN TYPEOF(r."Id") = 'integer' THEN r."Id" ELSE NULL END AS "Id",
        CASE WHEN TYPEOF(r."ChannelId") = 'integer' THEN r."ChannelId" ELSE NULL END AS "ChannelId",
        CASE WHEN TYPEOF(r."DesignId") = 'integer' THEN r."DesignId" ELSE NULL END AS "DesignId",
        CASE WHEN TYPEOF(r."Revision") = 'integer' THEN r."Revision" ELSE NULL END AS "Revision",
        CASE WHEN TYPEOF(r."Digest") = 'text' AND LENGTH(CAST(r."Digest" AS BLOB)) = 64
            THEN r."Digest" ELSE NULL END AS "Digest"
        FROM "TeletextPublicationRequestCurrent" p LEFT JOIN "TeletextPublicationRequest" r
            ON r."Id" = p."RequestId" AND r."ChannelId" = p."ChannelId"
        WHERE p."ChannelId" = ?"#,
    )
    .bind(channel_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?;
    row.as_ref()
        .map(|row| decode_request(row, channel_id))
        .transpose()
}

fn decode_request(row: &DbRow, channel_id: EntityId) -> Result<TeletextPublicationRequest> {
    let number = |column: &str| -> Result<i64> {
        row.try_get::<Option<i64>, _>(column)
            .repository()?
            .filter(|value| *value > 0)
            .ok_or_else(corrupt)
    };
    let id = number("Id")?;
    let owner = number("ChannelId")?;
    let digest: Option<String> = row.try_get("Digest").repository()?;
    let digest = digest
        .filter(|digest| valid_digest(digest))
        .ok_or_else(corrupt)?;
    if number("Pointer")? != id || owner != channel_id {
        return Err(corrupt());
    }
    Ok(TeletextPublicationRequest {
        id,
        channel_id: owner,
        design_id: number("DesignId")?,
        revision: number("Revision")?,
        digest,
    })
}

fn invalid_identity() -> RepositoryError {
    RepositoryError::Validation {
        field: "channel identity",
        message: "invalid teletext request identity".to_owned(),
    }
}

fn corrupt() -> RepositoryError {
    RepositoryError::Decode {
        kind: "teletext publication request",
        value: "invalid stored pending request".to_owned(),
    }
}

#[cfg(test)]
#[path = "teletext_publications_tests.rs"]
mod tests;
