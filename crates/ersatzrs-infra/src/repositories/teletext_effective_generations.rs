//! Scoped accepted metadata and fresh whole-scope confirmation, never delivery.

use super::super::teletext_publications::effective as publication;
use super::*;
use ersatzrs_core::repositories::{
    TeletextEffectiveGenerationSave, TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{
    TeletextEffectiveChannelSelectionSnapshot as Selection,
    TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
};
use std::time::Instant;

fn invalid() -> R {
    R::Publication(P::InvalidValues)
}
fn timeout() -> R {
    R::Publication(P::GuideTimedOut)
}
fn changed() -> R {
    R::Publication(P::SelectionChanged)
}

fn valid_target(
    request: &TeletextPublicationRequest,
    selection: &Selection,
    version: u16,
    digest: &str,
    maximum: usize,
) -> bool {
    valid_request(request)
        && publication::valid_scope(selection)
        && selection.channel_id == request.channel_id
        && valid_artifact_role(version, TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION)
        && valid_digest(digest)
        && maximum > 0
        && isize::try_from(maximum).is_ok()
}

pub(super) async fn accept(
    pool: &DbPool,
    save: &TeletextEffectiveGenerationSave,
    maximum: usize,
    deadline: Instant,
) -> Result<M<TeletextAcceptedGeneration>> {
    if !valid_target(
        &save.request,
        &save.selection,
        save.artifact_version,
        &save.artifact_digest,
        maximum,
    ) || save.expected_design_head <= 0
        || save.expected_generation_id.is_some_and(|id| id <= 0)
    {
        return Ok(M::Rejected(invalid()));
    }
    if Instant::now() >= deadline {
        return Ok(M::Rejected(timeout()));
    }
    if publication::expired(&save.selection) {
        return Ok(M::Rejected(changed()));
    }
    let Some(deadline) = publication::scope_deadline(&save.selection, deadline) else {
        return Ok(M::Rejected(changed()));
    };
    let tx = match tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        admit(pool, save, maximum, deadline),
    )
    .await
    {
        Err(_) => return Ok(M::Rejected(timeout())),
        Ok(Err(error)) => return Err(error),
        Ok(Ok(Err(reason))) => return Ok(M::Rejected(reason)),
        Ok(Ok(Ok(tx))) => tx,
    };
    insert(tx, save, deadline).await
}

async fn admit(
    pool: &DbPool,
    save: &TeletextEffectiveGenerationSave,
    maximum: usize,
    deadline: Instant,
) -> Result<std::result::Result<Transaction<'static, Db>, R>> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    if publication::current_in_transaction(&mut tx, save.request.channel_id)
        .await?
        .as_ref()
        != Some(&save.request)
    {
        return Ok(Err(R::Publication(P::RequestConflict)));
    }
    let current = current_in_transaction(&mut tx, save.request.channel_id).await?;
    let already: bool = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "TeletextAcceptedGeneration" WHERE "ChannelId"=? AND "RequestId"=?)"#)
        .bind(save.request.channel_id).bind(save.request.id).fetch_one(&mut *tx).await.repository()?;
    if already {
        return Ok(Err(R::AlreadyAccepted));
    }
    if current.as_ref().map(|generation| generation.id) != save.expected_generation_id {
        return Ok(Err(R::GenerationConflict));
    }
    let Some(head) = head_in_transaction(&mut tx, save.request.design_id).await? else {
        return Ok(Err(R::Publication(P::MissingDesign)));
    };
    if head.deleted {
        return Ok(Err(R::Publication(P::DeletedDesign)));
    }
    if head.revision != save.expected_design_head {
        return Ok(Err(R::Publication(P::DesignConflict)));
    }
    if let Err(reason) = publication::check_scope(&mut tx, &save.selection).await? {
        return Ok(Err(R::Publication(reason)));
    }
    if let Err(reason) = check_revision_for_version(
        &mut tx,
        &save.request,
        maximum,
        TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
        Some(&save.selection),
    )
    .await?
    {
        return Ok(Err(reason));
    }
    if Instant::now() >= deadline {
        return Ok(Err(timeout()));
    }
    if publication::expired(&save.selection) {
        return Ok(Err(changed()));
    }
    Ok(Ok(tx))
}

pub(super) async fn insert(
    tx: Transaction<'_, Db>,
    save: &TeletextEffectiveGenerationSave,
    deadline: Instant,
) -> Result<M<TeletextAcceptedGeneration>> {
    insert_seeded(tx, save, deadline, None).await
}

pub(super) async fn insert_seeded(
    mut tx: Transaction<'_, Db>,
    save: &TeletextEffectiveGenerationSave,
    deadline: Instant,
    qualification: Option<&ersatzrs_core::repositories::TeletextGuideGenerationQualification>,
) -> Result<M<TeletextAcceptedGeneration>> {
    if Instant::now() >= deadline {
        return Ok(M::Rejected(timeout()));
    }
    if publication::expired(&save.selection) {
        return Ok(M::Rejected(changed()));
    }
    // Do not wrap admitted writes/commit in a timeout or report false failure.
    let result = sqlx::query(r#"INSERT INTO "TeletextAcceptedGeneration" ("ChannelId","RequestId","ArtifactVersion","ArtifactDigest") VALUES (?,?,?,?)"#)
        .bind(save.request.channel_id).bind(save.request.id).bind(i64::from(save.artifact_version))
        .bind(&save.artifact_digest).execute(&mut *tx).await.repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "TeletextEffectiveAcceptedGenerationCurrent" ("ChannelId","GenerationId") VALUES (?,?) ON CONFLICT("ChannelId") DO UPDATE SET "GenerationId"=excluded."GenerationId""#)
        .bind(save.request.channel_id).bind(id).execute(&mut *tx).await.repository()?;
    let generation = TeletextAcceptedGeneration {
        id,
        request: save.request.clone(),
        artifact_version: save.artifact_version,
        artifact_digest: save.artifact_digest.clone(),
    };
    if let Some(qualification) = qualification {
        super::refresh::register(
            &mut tx,
            &generation,
            save.expected_design_head,
            qualification,
            ersatzrs_core::repositories::TeletextGuideRefreshRole::Effective,
        )
        .await?;
    }
    tx.commit().await.repository()?;
    Ok(M::Accepted(generation))
}

pub(super) async fn confirm(
    pool: &DbPool,
    generation: &TeletextAcceptedGeneration,
    selection: &Selection,
    maximum: usize,
    deadline: Instant,
) -> Result<M<()>> {
    if generation.id <= 0
        || !valid_target(
            &generation.request,
            selection,
            generation.artifact_version,
            &generation.artifact_digest,
            maximum,
        )
    {
        return Ok(M::Rejected(invalid()));
    }
    if Instant::now() >= deadline {
        return Ok(M::Rejected(timeout()));
    }
    if publication::expired(selection) {
        return Ok(M::Rejected(changed()));
    }
    let Some(deadline) = publication::scope_deadline(selection, deadline) else {
        return Ok(M::Rejected(changed()));
    };
    match tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        confirm_before(pool, generation, selection, maximum, deadline),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Ok(M::Rejected(timeout())),
    }
}

async fn confirm_before(
    pool: &DbPool,
    generation: &TeletextAcceptedGeneration,
    selection: &Selection,
    maximum: usize,
    deadline: Instant,
) -> Result<M<()>> {
    // Read-only transaction: new heads/pending intents never replace accepted output.
    let mut tx = pool.begin().await.repository()?;
    if current_in_transaction(&mut tx, generation.request.channel_id)
        .await?
        .as_ref()
        != Some(generation)
    {
        return Ok(M::Rejected(R::GenerationConflict));
    }
    if let Err(reason) = publication::check_scope(&mut tx, selection).await? {
        return Ok(M::Rejected(R::Publication(reason)));
    }
    if let Err(reason) = check_revision_for_version(
        &mut tx,
        &generation.request,
        maximum,
        TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
        Some(selection),
    )
    .await?
    {
        return Ok(M::Rejected(reason));
    }
    if Instant::now() >= deadline {
        return Ok(M::Rejected(timeout()));
    }
    if publication::expired(selection) {
        return Ok(M::Rejected(changed()));
    }
    Ok(M::Accepted(()))
}

pub(super) async fn current_in_transaction(
    tx: &mut Transaction<'_, Db>,
    channel: EntityId,
) -> Result<Option<TeletextAcceptedGeneration>> {
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
        CASE WHEN TYPEOF(r."Digest")='text' AND LENGTH(CAST(r."Digest" AS BLOB))=64 THEN r."Digest" END AS "Digest",
        CASE WHEN TYPEOF(d."DocumentVersion")='integer' THEN d."DocumentVersion" END AS "DocumentVersion",
        CASE WHEN TYPEOF(d."Digest")='text' AND LENGTH(CAST(d."Digest" AS BLOB))=64 THEN d."Digest" END AS "DesignDigest"
        FROM "TeletextEffectiveAcceptedGenerationCurrent" c LEFT JOIN "TeletextAcceptedGeneration" g
            ON g."Id"=c."GenerationId" AND g."ChannelId"=c."ChannelId"
        LEFT JOIN "TeletextPublicationRequest" r ON r."Id"=g."RequestId" AND r."ChannelId"=g."ChannelId"
        LEFT JOIN "TeletextDesignRevision" d ON d."DesignId"=r."DesignId" AND d."Revision"=r."Revision"
        WHERE c."ChannelId"=?"#).bind(channel).fetch_optional(&mut **tx).await.repository()?;
    row.as_ref()
        .map(|row| {
            let generation = decode_generation_for_version(
                row,
                channel,
                TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
            )?;
            if row
                .try_get::<Option<i64>, _>("DocumentVersion")
                .repository()?
                != Some(i64::from(TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION))
                || row
                    .try_get::<Option<String>, _>("DesignDigest")
                    .repository()?
                    .as_ref()
                    != Some(&generation.request.digest)
            {
                return Err(corrupt("effective generation role"));
            }
            Ok(generation)
        })
        .transpose()
}
