//! Durable output selection; native compilation and sink delivery are separate.

use async_trait::async_trait;
use ersatzrs_core::repositories::{
    RepositoryError, Result, TeletextGenerationMutation as M, TeletextGenerationRejection as R,
    TeletextGenerationRepository, TeletextGenerationSave, TeletextProfileMutation,
    TeletextPublicationRejection as P, TELETEXT_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{
    EntityId, TeletextAcceptedGeneration, TeletextChannelSelectionSnapshot,
    TeletextPublicationRequest, MAX_TELETEXT_PAGE_RECORDS,
};
use sqlx::{Row as _, Transaction};

use super::{
    teletext_designs::{head_in_transaction, revision_in_transaction},
    teletext_profiles::channel_snapshot_in_transaction,
    teletext_publications::{current_in_transaction as current_request, stored_fence},
    SqlxResultExt as _,
};
use crate::db::{Db, DbPool, DbRow};

/// Transactional accepted history and per-channel pointer, without stream tasks.
#[derive(Clone)]
pub struct SqliteTeletextGenerationRepository(pub DbPool);

#[async_trait]
impl TeletextGenerationRepository for SqliteTeletextGenerationRepository {
    async fn accept(
        &self,
        save: &TeletextGenerationSave,
        max_document_bytes: usize,
    ) -> Result<M<TeletextAcceptedGeneration>> {
        if !valid_save(save) {
            return Ok(M::Rejected(R::Publication(P::InvalidValues)));
        }
        let mut transaction = crate::db::begin_write(&self.0).await.repository()?;
        if current_request(&mut transaction, save.request.channel_id)
            .await?
            .as_ref()
            != Some(&save.request)
        {
            return Ok(M::Rejected(R::Publication(P::RequestConflict)));
        }
        let current = current_in_transaction(&mut transaction, save.request.channel_id).await?;
        let previously_accepted: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "TeletextAcceptedGeneration" WHERE "ChannelId"=? AND "RequestId"=?)"#)
            .bind(save.request.channel_id).bind(save.request.id)
            .fetch_one(&mut *transaction).await.repository()?;
        if previously_accepted {
            return Ok(M::Rejected(R::AlreadyAccepted));
        }
        if current.as_ref().map(|current| current.id) != save.expected_generation_id {
            return Ok(M::Rejected(R::GenerationConflict));
        }
        let Some(head) = head_in_transaction(&mut transaction, save.request.design_id).await?
        else {
            return Ok(M::Rejected(R::Publication(P::MissingDesign)));
        };
        if head.deleted {
            return Ok(M::Rejected(R::Publication(P::DeletedDesign)));
        }
        if head.revision != save.expected_design_head {
            return Ok(M::Rejected(R::Publication(P::DesignConflict)));
        }
        if let Err(reason) = check_selection(&mut transaction, &save.selection).await? {
            return Ok(M::Rejected(reason));
        }
        if let Err(reason) =
            check_revision(&mut transaction, &save.request, max_document_bytes).await?
        {
            return Ok(M::Rejected(reason));
        }
        insert_generation(transaction, save).await
    }

    async fn current(&self, channel_id: EntityId) -> Result<Option<TeletextAcceptedGeneration>> {
        if channel_id <= 0 {
            return Err(corrupt("generation channel identity"));
        }
        // Read-only transaction: selected metadata is not runtime admission.
        let mut transaction = self.0.begin().await.repository()?;
        current_in_transaction(&mut transaction, channel_id).await
    }

    async fn confirm_current(
        &self,
        generation: &TeletextAcceptedGeneration,
        selection: &TeletextChannelSelectionSnapshot,
        max_document_bytes: usize,
    ) -> Result<M<()>> {
        if generation.id <= 0
            || !valid_request(&generation.request)
            || !valid_selection(selection, generation.request.channel_id)
            || !valid_artifact(generation.artifact_version, &generation.artifact_digest)
        {
            return Ok(M::Rejected(R::Publication(P::InvalidValues)));
        }
        // Read-only transaction: pending pointers and live draft heads are not
        // consulted, so a new draft/request cannot replace accepted output.
        let mut transaction = self.0.begin().await.repository()?;
        if current_in_transaction(&mut transaction, generation.request.channel_id)
            .await?
            .as_ref()
            != Some(generation)
        {
            return Ok(M::Rejected(R::GenerationConflict));
        }
        if let Err(reason) = check_selection(&mut transaction, selection).await? {
            return Ok(M::Rejected(reason));
        }
        if let Err(reason) =
            check_revision(&mut transaction, &generation.request, max_document_bytes).await?
        {
            return Ok(M::Rejected(reason));
        }
        Ok(M::Accepted(()))
    }
}

async fn check_selection(
    transaction: &mut Transaction<'_, Db>,
    selection: &TeletextChannelSelectionSnapshot,
) -> Result<std::result::Result<(), R>> {
    Ok(
        match channel_snapshot_in_transaction(transaction, selection.channel_id).await? {
            TeletextProfileMutation::Rejected(reason) => {
                Err(R::Publication(P::SelectionUnavailable(reason)))
            }
            TeletextProfileMutation::Saved(current) if current != *selection => {
                Err(R::Publication(P::SelectionChanged))
            }
            TeletextProfileMutation::Saved(_) => Ok(()),
        },
    )
}

async fn check_revision(
    transaction: &mut Transaction<'_, Db>,
    request: &TeletextPublicationRequest,
    maximum: usize,
) -> Result<std::result::Result<(), R>> {
    let Some(revision) =
        revision_in_transaction(transaction, request.design_id, request.revision, maximum).await?
    else {
        return Ok(Err(R::Publication(P::MissingRevision)));
    };
    if revision.digest != request.digest {
        return Ok(Err(R::Publication(P::DigestConflict)));
    }
    match stored_fence(&revision.document) {
        Ok(Some(_)) => Ok(Err(R::GuideFreshnessRequired)),
        Err(()) => Ok(Err(R::Publication(P::GuideUnavailable))),
        Ok(None) => Ok(Ok(())),
    }
    // Canonical guide_inputs channels=[] qualification belongs to the trusted
    // native validator, bound to this exact immutable digest. This adapter does
    // not infer static authority from missing source metadata or parse templates.
}

async fn insert_generation(
    mut transaction: Transaction<'_, Db>,
    save: &TeletextGenerationSave,
) -> Result<M<TeletextAcceptedGeneration>> {
    let result = sqlx::query(r#"INSERT INTO "TeletextAcceptedGeneration" ("ChannelId","RequestId","ArtifactVersion","ArtifactDigest") VALUES (?,?,?,?)"#)
        .bind(save.request.channel_id).bind(save.request.id)
        .bind(i64::from(save.artifact_version)).bind(&save.artifact_digest)
        .execute(&mut *transaction).await.repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "TeletextAcceptedGenerationCurrent" ("ChannelId","GenerationId") VALUES (?,?) ON CONFLICT("ChannelId") DO UPDATE SET "GenerationId"=excluded."GenerationId""#)
        .bind(save.request.channel_id).bind(id).execute(&mut *transaction).await.repository()?;
    transaction.commit().await.repository()?;
    Ok(M::Accepted(TeletextAcceptedGeneration {
        id,
        request: save.request.clone(),
        artifact_version: save.artifact_version,
        artifact_digest: save.artifact_digest.clone(),
    }))
}

async fn current_in_transaction(
    transaction: &mut Transaction<'_, Db>,
    channel_id: EntityId,
) -> Result<Option<TeletextAcceptedGeneration>> {
    // CASE fences numeric/text corruption before driver payload allocation.
    // LEFT joins distinguish dangling/wrong-channel pointers from true absence.
    let row = sqlx::query(r#"SELECT
        CASE WHEN TYPEOF(c."GenerationId")='integer' THEN c."GenerationId" END AS "Pointer",
        CASE WHEN TYPEOF(g."Id")='integer' THEN g."Id" END AS "GenerationId",
        CASE WHEN TYPEOF(g."ChannelId")='integer' THEN g."ChannelId" END AS "GenerationChannelId",
        CASE WHEN TYPEOF(g."ArtifactVersion")='integer' THEN g."ArtifactVersion" END AS "ArtifactVersion",
        CASE WHEN TYPEOF(g."ArtifactDigest")='text' AND LENGTH(CAST(g."ArtifactDigest" AS BLOB))=64 THEN g."ArtifactDigest" END AS "ArtifactDigest",
        CASE WHEN TYPEOF(r."Id")='integer' THEN r."Id" END AS "RequestId",
        CASE WHEN TYPEOF(r."ChannelId")='integer' THEN r."ChannelId" END AS "ChannelId",
        CASE WHEN TYPEOF(r."DesignId")='integer' THEN r."DesignId" END AS "DesignId",
        CASE WHEN TYPEOF(r."Revision")='integer' THEN r."Revision" END AS "Revision",
        CASE WHEN TYPEOF(r."Digest")='text' AND LENGTH(CAST(r."Digest" AS BLOB))=64 THEN r."Digest" END AS "Digest"
        FROM "TeletextAcceptedGenerationCurrent" c LEFT JOIN "TeletextAcceptedGeneration" g
        ON g."Id"=c."GenerationId" AND g."ChannelId"=c."ChannelId"
        LEFT JOIN "TeletextPublicationRequest" r ON r."Id"=g."RequestId" AND r."ChannelId"=g."ChannelId"
        WHERE c."ChannelId"=?"#)
        .bind(channel_id).fetch_optional(&mut **transaction).await.repository()?;
    row.as_ref()
        .map(|row| decode_generation(row, channel_id))
        .transpose()
}

fn decode_generation(row: &DbRow, channel_id: EntityId) -> Result<TeletextAcceptedGeneration> {
    let number = |column: &str| -> Result<i64> {
        row.try_get::<Option<i64>, _>(column)
            .repository()?
            .filter(|value| *value > 0)
            .ok_or_else(|| corrupt("generation metadata"))
    };
    let id = number("GenerationId")?;
    if number("Pointer")? != id
        || number("GenerationChannelId")? != channel_id
        || number("ChannelId")? != channel_id
    {
        return Err(corrupt("generation association"));
    }
    let artifact_version =
        u16::try_from(number("ArtifactVersion")?).map_err(|_| corrupt("artifact version"))?;
    let artifact_digest: Option<String> = row.try_get("ArtifactDigest").repository()?;
    let artifact_digest = artifact_digest.ok_or_else(|| corrupt("artifact digest"))?;
    if !valid_artifact(artifact_version, &artifact_digest) {
        return Err(corrupt("artifact metadata"));
    }
    let digest: Option<String> = row.try_get("Digest").repository()?;
    let digest = digest
        .filter(|digest| valid_digest(digest))
        .ok_or_else(|| corrupt("revision digest"))?;
    Ok(TeletextAcceptedGeneration {
        id,
        request: TeletextPublicationRequest {
            id: number("RequestId")?,
            channel_id,
            design_id: number("DesignId")?,
            revision: number("Revision")?,
            digest,
        },
        artifact_version,
        artifact_digest,
    })
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}
fn valid_artifact(version: u16, digest: &str) -> bool {
    version == TELETEXT_GENERATION_ARTIFACT_VERSION && valid_digest(digest)
}
fn valid_request(request: &TeletextPublicationRequest) -> bool {
    request.id > 0
        && request.channel_id > 0
        && request.design_id > 0
        && request.revision > 0
        && valid_digest(&request.digest)
}
fn valid_selection(selection: &TeletextChannelSelectionSnapshot, channel_id: EntityId) -> bool {
    selection.channel_id == channel_id
        && channel_id > 0
        && !selection.bindings.is_empty()
        && selection.bindings.len() <= MAX_TELETEXT_PAGE_RECORDS
}
fn valid_save(save: &TeletextGenerationSave) -> bool {
    valid_request(&save.request)
        && save.expected_design_head > 0
        && save.expected_generation_id.is_none_or(|id| id > 0)
        && valid_selection(&save.selection, save.request.channel_id)
        && valid_artifact(save.artifact_version, &save.artifact_digest)
}
fn corrupt(kind: &'static str) -> RepositoryError {
    RepositoryError::Decode {
        kind,
        value: "invalid stored teletext generation".to_owned(),
    }
}

#[cfg(test)]
#[path = "teletext_generations_tests.rs"]
mod tests;
