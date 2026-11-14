//! Durable refresh lineage, worker leases and separate immutable runtime output.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{
    RepositoryError, Result, TeletextGenerationMutation as M, TeletextGenerationRejection as R,
    TeletextGuideGenerationCheck, TeletextGuideGenerationQualification,
    TeletextGuideRefreshCandidate, TeletextGuideRefreshClaim, TeletextGuideRefreshCurrent,
    TeletextGuideRefreshCurrentMetadata, TeletextGuideRefreshFailure as Failure,
    TeletextGuideRefreshOutputCheck, TeletextGuideRefreshOutputConfirmation,
    TeletextGuideRefreshRepository, TeletextGuideRefreshRole as Role, TeletextGuideRefreshSeed,
    TeletextGuideRefreshSelection, TeletextGuideRuntimeSnapshot,
    TeletextGuideRuntimeSnapshotMetadata, TeletextGuideRuntimeSnapshotSave,
    TeletextGuideSourceFence, TeletextProfileMutation, TeletextPublicationRejection as P,
    MAX_TELETEXT_GUIDE_REFRESH_BATCH, MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES,
    TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION,
};
use ersatzrs_domain::{EntityId, TeletextAcceptedGeneration, TeletextPublicationRequest};
use sha2::{Digest as _, Sha256};
use sqlx::{Row as _, Transaction};
use std::{fmt::Write as _, time::Instant};
use uuid::Uuid;

use super::{
    corrupt, current_in_transaction, current_request, effective, head_in_transaction,
    revision_in_transaction, valid_digest, SqliteTeletextGenerationRepository,
};
use crate::{
    db::{Db, DbRow},
    repositories::SqlxResultExt as _,
};

#[path = "teletext_guide_refresh_output.rs"]
mod output;

fn invalid() -> R {
    R::Publication(P::InvalidValues)
}
fn conflict<T>() -> M<T> {
    M::Rejected(R::GenerationConflict)
}
fn timed_out() -> R {
    R::Publication(P::GuideTimedOut)
}
fn bounded(maximum: usize) -> bool {
    maximum > 0 && i64::try_from(maximum).is_ok()
}
fn limit(maximum: usize) -> Result<i64> {
    i64::try_from(maximum).map_err(|_| corrupt("refresh byte limit"))
}
fn plus(at: DateTime<Utc>, duration: Duration) -> Result<DateTime<Utc>> {
    at.checked_add_signed(duration)
        .ok_or_else(|| corrupt("refresh clock boundary"))
}
fn lease_cap(
    now: DateTime<Utc>,
    requested: DateTime<Utc>,
    check: &TeletextGuideGenerationCheck,
) -> Option<DateTime<Utc>> {
    let remaining = check.deadline.checked_duration_since(Instant::now())?;
    let bound = now.checked_add_signed(Duration::from_std(remaining).ok()?)?;
    let until = requested.min(bound);
    (until > now).then_some(until)
}

pub(super) fn changed_due(
    now: DateTime<Utc>,
    success: DateTime<Utc>,
    role_until: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    Ok(plus(now, Duration::minutes(1))?
        .min(plus(success, Duration::minutes(5))?)
        .min(role_until))
}

fn failure_code(value: Failure) -> i64 {
    match value {
        Failure::SourceUnavailable => 1,
        Failure::PreparationFailed => 2,
        Failure::NeedsReview => 3,
        Failure::Disabled => 4,
    }
}
fn failure(value: i64) -> Result<Failure> {
    match value {
        1 => Ok(Failure::SourceUnavailable),
        2 => Ok(Failure::PreparationFailed),
        3 => Ok(Failure::NeedsReview),
        4 => Ok(Failure::Disabled),
        _ => Err(corrupt("refresh failure")),
    }
}
fn role(value: i64) -> Result<Role> {
    match value {
        1 => Ok(Role::Base),
        2 => Ok(Role::Effective),
        _ => Err(corrupt("refresh role")),
    }
}
fn source_bytes(source: &TeletextGuideSourceFence) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(source).map_err(|_| corrupt("refresh source"))?;
    if bytes.len() > MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES {
        return Err(corrupt("refresh source size"));
    }
    Ok(bytes)
}
fn qualification(row: &DbRow) -> Result<TeletextGuideGenerationQualification> {
    let bytes: Option<Vec<u8>> = row.try_get("Source").repository()?;
    let source: TeletextGuideSourceFence =
        serde_json::from_slice(&bytes.ok_or_else(|| corrupt("refresh source size"))?)
            .map_err(|_| corrupt("refresh source"))?;
    let value = TeletextGuideGenerationQualification {
        source,
        valid_until: row.try_get("ValidUntil").repository()?,
    };
    value
        .validate_current(value.source.captured_at)
        .map_err(|_| corrupt("refresh validity"))?;
    Ok(value)
}

async fn seed_in_transaction(
    tx: &mut Transaction<'_, Db>,
    id: EntityId,
) -> Result<Option<TeletextGuideRefreshSeed>> {
    seed_bounded_in_transaction(tx, id, MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES).await
}

async fn seed_bounded_in_transaction(
    tx: &mut Transaction<'_, Db>,
    id: EntityId,
    maximum: usize,
) -> Result<Option<TeletextGuideRefreshSeed>> {
    let row=sqlx::query(r#"SELECT s."Id",s."Role",s."ExpectedDesignHead",s."ValidUntil",
        CASE WHEN TYPEOF(s."Source")='blob' AND LENGTH(s."Source")<=? THEN s."Source" END AS "Source",
        g."Id" AS "GenerationId",g."ArtifactVersion",g."ArtifactDigest",
        r."Id" AS "RequestId",r."ChannelId",r."DesignId",r."Revision",r."Digest"
        FROM "TeletextGuideRefreshSeed" s JOIN "TeletextAcceptedGeneration" g
        ON g."Id"=s."GenerationId" AND g."ChannelId"=s."ChannelId"
        JOIN "TeletextPublicationRequest" r ON r."Id"=g."RequestId" AND r."ChannelId"=g."ChannelId"
        WHERE s."Id"=?"#)
        .bind(limit(maximum.min(MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES))?).bind(id).fetch_optional(&mut **tx).await.repository()?;
    row.as_ref()
        .map(|row| {
            let role = role(row.try_get("Role").repository()?)?;
            let artifact_version =
                u16::try_from(row.try_get::<i64, _>("ArtifactVersion").repository()?)
                    .map_err(|_| corrupt("refresh artifact role"))?;
            if artifact_version
                != match role {
                    Role::Base => 5,
                    Role::Effective => 6,
                }
            {
                return Err(corrupt("refresh artifact role"));
            }
            let artifact_digest: String = row.try_get("ArtifactDigest").repository()?;
            let digest: String = row.try_get("Digest").repository()?;
            if !valid_digest(&artifact_digest) || !valid_digest(&digest) {
                return Err(corrupt("refresh origin digest"));
            }
            Ok(TeletextGuideRefreshSeed {
                id: row.try_get("Id").repository()?,
                role,
                expected_design_head: row.try_get("ExpectedDesignHead").repository()?,
                qualification: qualification(row)?,
                generation: TeletextAcceptedGeneration {
                    id: row.try_get("GenerationId").repository()?,
                    artifact_version,
                    artifact_digest,
                    request: TeletextPublicationRequest {
                        id: row.try_get("RequestId").repository()?,
                        channel_id: row.try_get("ChannelId").repository()?,
                        design_id: row.try_get("DesignId").repository()?,
                        revision: row.try_get("Revision").repository()?,
                        digest,
                    },
                },
            })
        })
        .transpose()
}

async fn runtime_in_transaction(
    tx: &mut Transaction<'_, Db>,
    id: EntityId,
    seed_id: EntityId,
    maximum: usize,
) -> Result<TeletextGuideRuntimeSnapshot> {
    let row=sqlx::query(r#"SELECT "Id","SeedId","Version","DocumentVersion","DocumentDigest","ArtifactDigest","ValidUntil",
        CASE WHEN TYPEOF("Document")='blob' AND LENGTH("Document")<=? THEN "Document" END AS "Document",
        CASE WHEN TYPEOF("Source")='blob' AND LENGTH("Source")<=? THEN "Source" END AS "Source"
        FROM "TeletextGuideRuntimeSnapshot" WHERE "Id"=? AND "SeedId"=?"#)
        .bind(limit(maximum)?).bind(limit(MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES)?).bind(id).bind(seed_id)
        .fetch_optional(&mut **tx).await.repository()?.ok_or_else(||corrupt("refresh runtime pointer"))?;
    let version: u16 = u16::try_from(row.try_get::<i64, _>("Version").repository()?)
        .map_err(|_| corrupt("refresh runtime version"))?;
    let document_version: u16 =
        u16::try_from(row.try_get::<i64, _>("DocumentVersion").repository()?)
            .map_err(|_| corrupt("refresh document version"))?;
    let document: Option<Vec<u8>> = row.try_get("Document").repository()?;
    let document = document.ok_or_else(|| corrupt("refresh document size"))?;
    let document_digest: String = row.try_get("DocumentDigest").repository()?;
    let artifact_digest: String = row.try_get("ArtifactDigest").repository()?;
    if version != TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION
        || document_version != 1
        || document.is_empty()
        || !valid_digest(&document_digest)
        || !valid_digest(&artifact_digest)
    {
        return Err(corrupt("refresh runtime document"));
    }
    Ok(TeletextGuideRuntimeSnapshot {
        id,
        seed_id,
        saved: TeletextGuideRuntimeSnapshotSave {
            version,
            document_version,
            document,
            document_digest,
            artifact_digest,
            qualification: qualification(&row)?,
        },
    })
}

async fn current_in_tx(
    tx: &mut Transaction<'_, Db>,
    seed_id: EntityId,
    maximum: usize,
) -> Result<Option<TeletextGuideRefreshCurrent>> {
    let Some(metadata) = observe_in_tx(tx, seed_id, MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES).await?
    else {
        return Ok(None);
    };
    let runtime = match metadata.runtime {
        Some(runtime) => Some(runtime_in_transaction(tx, runtime.id, seed_id, maximum).await?),
        None => None,
    };
    Ok(Some(TeletextGuideRefreshCurrent {
        seed: metadata.seed,
        revision: metadata.revision,
        runtime,
        next_due_at: metadata.next_due_at,
        last_success_at: metadata.last_success_at,
        first_failure_at: metadata.first_failure_at,
        failure: metadata.failure,
        revoked: metadata.revoked,
        refreshing: metadata.refreshing,
    }))
}

async fn runtime_metadata_in_transaction(
    tx: &mut Transaction<'_, Db>,
    id: EntityId,
    seed_id: EntityId,
    maximum: usize,
) -> Result<TeletextGuideRuntimeSnapshotMetadata> {
    // The document BLOB is deliberately absent from this cheap observation.
    let row = sqlx::query(
        r#"SELECT "Version","DocumentVersion","DocumentDigest","ArtifactDigest","ValidUntil",
        CASE WHEN TYPEOF("Source")='blob' AND LENGTH("Source")<=? THEN "Source" END AS "Source"
        FROM "TeletextGuideRuntimeSnapshot" WHERE "Id"=? AND "SeedId"=?"#,
    )
    .bind(limit(maximum.min(MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES))?)
    .bind(id)
    .bind(seed_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(|| corrupt("refresh runtime metadata"))?;
    let version = u16::try_from(row.try_get::<i64, _>("Version").repository()?)
        .map_err(|_| corrupt("refresh runtime version"))?;
    let document_version = u16::try_from(row.try_get::<i64, _>("DocumentVersion").repository()?)
        .map_err(|_| corrupt("refresh document version"))?;
    let document_digest: String = row.try_get("DocumentDigest").repository()?;
    let artifact_digest: String = row.try_get("ArtifactDigest").repository()?;
    if version != TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION
        || document_version != 1
        || !valid_digest(&document_digest)
        || !valid_digest(&artifact_digest)
    {
        return Err(corrupt("refresh runtime metadata"));
    }
    Ok(TeletextGuideRuntimeSnapshotMetadata {
        id,
        seed_id,
        version,
        document_version,
        document_digest,
        artifact_digest,
        qualification: qualification(&row)?,
    })
}

async fn observe_in_tx(
    tx: &mut Transaction<'_, Db>,
    seed_id: EntityId,
    maximum: usize,
) -> Result<Option<TeletextGuideRefreshCurrentMetadata>> {
    let row=sqlx::query(r#"SELECT "Revision","RuntimeId","NextDueAt","LastSuccessAt","FirstFailureAt","Failure","Revoked",
        CASE WHEN "LeaseOwner" IS NOT NULL THEN 1 ELSE 0 END AS "Refreshing"
        FROM "TeletextGuideRefreshCurrent" WHERE "SeedId"=?"#).bind(seed_id).fetch_optional(&mut **tx).await.repository()?;
    let Some(row) = row else { return Ok(None) };
    let seed = seed_bounded_in_transaction(tx, seed_id, maximum)
        .await?
        .ok_or_else(|| corrupt("refresh current seed"))?;
    let runtime_id: Option<i64> = row.try_get("RuntimeId").repository()?;
    let runtime = match runtime_id {
        Some(id) => Some(runtime_metadata_in_transaction(tx, id, seed_id, maximum).await?),
        None => None,
    };
    let revision = row.try_get("Revision").repository()?;
    let revoked: i64 = row.try_get("Revoked").repository()?;
    if revision <= 0 || !matches!(revoked, 0 | 1) {
        return Err(corrupt("refresh current values"));
    }
    Ok(Some(TeletextGuideRefreshCurrentMetadata {
        seed,
        runtime,
        revision,
        revoked: revoked == 1,
        refreshing: row.try_get::<i64, _>("Refreshing").repository()? == 1,
        next_due_at: row.try_get("NextDueAt").repository()?,
        last_success_at: row.try_get("LastSuccessAt").repository()?,
        first_failure_at: row.try_get("FirstFailureAt").repository()?,
        failure: row
            .try_get::<Option<i64>, _>("Failure")
            .repository()?
            .map(failure)
            .transpose()?,
    }))
}

/// Called inside explicit accepted5/6 insertion, before its single commit.
pub(super) async fn register(
    tx: &mut Transaction<'_, Db>,
    generation: &TeletextAcceptedGeneration,
    expected_head: i64,
    qualification: &TeletextGuideGenerationQualification,
    role: Role,
) -> Result<()> {
    let captured = qualification.source.captured_at;
    let next_due = plus(captured, Duration::minutes(5))?.min(qualification.valid_until);
    let result=sqlx::query(r#"INSERT INTO "TeletextGuideRefreshSeed"
        ("ChannelId","Role","GenerationId","ExpectedDesignHead","Source","ValidUntil") VALUES (?,?,?,?,?,?)"#)
        .bind(generation.request.channel_id).bind(role.code()).bind(generation.id).bind(expected_head)
        .bind(source_bytes(&qualification.source)?).bind(qualification.valid_until).execute(&mut **tx).await.repository()?;
    let id = crate::db::last_insert_id(result);
    sqlx::query(r#"INSERT INTO "TeletextGuideRefreshCurrent"
        ("ChannelId","Role","SeedId","RuntimeId","Revision","NextDueAt","LastSuccessAt","Revoked") VALUES (?,?,?,NULL,1,?,?,0)
        ON CONFLICT("ChannelId","Role") DO UPDATE SET "SeedId"=excluded."SeedId","RuntimeId"=NULL,
        "Revision"="TeletextGuideRefreshCurrent"."Revision"+1,"NextDueAt"=excluded."NextDueAt",
        "LastSuccessAt"=excluded."LastSuccessAt","ChangedAt"=NULL,"FirstFailureAt"=NULL,"Failure"=NULL,
        "Revoked"=0,"LeaseOwner"=NULL,"LeaseUntil"=NULL"#)
        .bind(generation.request.channel_id).bind(role.code()).bind(id).bind(next_due).bind(captured)
        .execute(&mut **tx).await.repository()?;
    Ok(())
}

#[async_trait]
impl TeletextGuideRefreshRepository for SqliteTeletextGenerationRepository {
    async fn confirm_output(
        &self,
        confirmation: &TeletextGuideRefreshOutputConfirmation,
        now: DateTime<Utc>,
        check: &TeletextGuideRefreshOutputCheck,
        max_metadata_bytes: usize,
    ) -> Result<M<()>> {
        output::confirm(&self.0, confirmation, now, check, max_metadata_bytes).await
    }

    async fn scan_current(
        &self,
        after_seed_id: Option<EntityId>,
        count: usize,
    ) -> Result<Vec<TeletextGuideRefreshSeed>> {
        if count == 0
            || count > MAX_TELETEXT_GUIDE_REFRESH_BATCH
            || after_seed_id.is_some_and(|id| id <= 0)
        {
            return Err(corrupt("refresh scan bounds"));
        }
        // Read-only transaction: bounded observations never mutate guide jobs or output.
        let mut tx = self.0.begin().await.repository()?;
        let ids: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "SeedId" FROM "TeletextGuideRefreshCurrent"
                WHERE "SeedId">? ORDER BY "SeedId" LIMIT ?"#,
        )
        .bind(after_seed_id.unwrap_or(0))
        .bind(limit(count)?)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        let mut seeds = Vec::with_capacity(ids.len());
        for id in ids {
            seeds.push(
                seed_in_transaction(&mut tx, id)
                    .await?
                    .ok_or_else(|| corrupt("refresh scan seed"))?,
            );
        }
        tx.rollback().await.repository()?;
        Ok(seeds)
    }

    async fn observe_current(
        &self,
        channel_id: EntityId,
        role: Role,
        maximum: usize,
    ) -> Result<Option<TeletextGuideRefreshCurrentMetadata>> {
        if channel_id <= 0 || !bounded(maximum) {
            return Err(corrupt("refresh metadata arguments"));
        }
        // Read-only transaction: bounded observations never mutate guide jobs or output.
        let mut tx = self.0.begin().await.repository()?;
        let id: Option<i64> = sqlx::query_scalar(r#"SELECT "SeedId" FROM "TeletextGuideRefreshCurrent" WHERE "ChannelId"=? AND "Role"=?"#)
            .bind(channel_id).bind(role.code()).fetch_optional(&mut *tx).await.repository()?;
        let value = match id {
            Some(id) => observe_in_tx(&mut tx, id, maximum).await?,
            None => None,
        };
        tx.rollback().await.repository()?;
        Ok(value)
    }
    async fn current(
        &self,
        channel_id: EntityId,
        role: Role,
        maximum: usize,
    ) -> Result<Option<TeletextGuideRefreshCurrent>> {
        if channel_id <= 0 || !bounded(maximum) {
            return Err(corrupt("refresh read arguments"));
        }
        // Read-only transaction: bounded observations never mutate guide jobs or output.
        let mut tx = self.0.begin().await.repository()?;
        let id:Option<i64>=sqlx::query_scalar(r#"SELECT "SeedId" FROM "TeletextGuideRefreshCurrent" WHERE "ChannelId"=? AND "Role"=?"#)
            .bind(channel_id).bind(role.code()).fetch_optional(&mut *tx).await.repository()?;
        let value = match id {
            Some(id) => current_in_tx(&mut tx, id, maximum).await?,
            None => None,
        };
        tx.rollback().await.repository()?;
        Ok(value)
    }
    async fn due(&self, now: DateTime<Utc>, count: usize) -> Result<Vec<TeletextGuideRefreshSeed>> {
        if count == 0 || count > MAX_TELETEXT_GUIDE_REFRESH_BATCH {
            return Err(corrupt("refresh discovery limit"));
        }
        // Read-only transaction: bounded observations never mutate guide jobs or output.
        let mut tx = self.0.begin().await.repository()?;
        let ids:Vec<i64>=sqlx::query_scalar(r#"SELECT "SeedId" FROM "TeletextGuideRefreshCurrent"
            WHERE "Revoked"=0 AND ("Failure" IS NULL OR "Failure" IN (1,2)) AND "NextDueAt"<=?
            AND ("LeaseOwner" IS NULL OR "LeaseUntil"<=?) ORDER BY "NextDueAt","ChannelId","Role" LIMIT ?"#)
            .bind(now).bind(now).bind(limit(count)?).fetch_all(&mut *tx).await.repository()?;
        let mut values = Vec::with_capacity(ids.len());
        for id in ids {
            values.push(
                seed_in_transaction(&mut tx, id)
                    .await?
                    .ok_or_else(|| corrupt("refresh due seed"))?,
            );
        }
        tx.rollback().await.repository()?;
        Ok(values)
    }
    async fn changed(&self, seed_id: EntityId, now: DateTime<Utc>) -> Result<M<()>> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let row = sqlx::query(
            r#"SELECT c."LastSuccessAt",c."NextDueAt",c."ChangedAt",COALESCE(r."ValidUntil",s."ValidUntil") AS "RoleUntil"
            FROM "TeletextGuideRefreshCurrent" c JOIN "TeletextGuideRefreshSeed" s ON s."Id"=c."SeedId"
            LEFT JOIN "TeletextGuideRuntimeSnapshot" r ON r."Id"=c."RuntimeId" AND r."SeedId"=c."SeedId"
            WHERE c."SeedId"=? AND c."Revoked"=0 AND (c."Failure" IS NULL OR c."Failure" IN (1,2,3))"#,
        )
        .bind(seed_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(conflict());
        };
        let success: DateTime<Utc> = row.try_get("LastSuccessAt").repository()?;
        if now < success {
            return Ok(M::Rejected(invalid()));
        }
        // Reobserving the same pending loss cannot renew its stale ceiling,
        // invalidate its worker repeatedly or postpone already eligible work.
        if row
            .try_get::<Option<DateTime<Utc>>, _>("ChangedAt")
            .repository()?
            .is_some()
        {
            return Ok(M::Accepted(()));
        }
        let previous_due: DateTime<Utc> = row.try_get("NextDueAt").repository()?;
        let due =
            changed_due(now, success, row.try_get("RoleUntil").repository()?)?.min(previous_due);
        sqlx::query(r#"UPDATE "TeletextGuideRefreshCurrent" SET "ChangedAt"=?,"NextDueAt"=?,"Revision"="Revision"+1,
            "FirstFailureAt"=COALESCE("FirstFailureAt",?),"Failure"=COALESCE("Failure",1)
            WHERE "SeedId"=?"#).bind(now).bind(due).bind(now).bind(seed_id).execute(&mut *tx).await.repository()?;
        tx.commit().await.repository()?;
        Ok(M::Accepted(()))
    }
    async fn claim(
        &self,
        seed_id: EntityId,
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
        check: &TeletextGuideGenerationCheck,
        maximum: usize,
    ) -> Result<M<Option<TeletextGuideRefreshClaim>>> {
        if seed_id <= 0 || lease_until <= now || !bounded(maximum) {
            return Ok(M::Rejected(invalid()));
        }
        let Some(lease_until) = lease_cap(now, lease_until, check) else {
            return Ok(M::Rejected(timed_out()));
        };
        let admitted =
            tokio::time::timeout_at(tokio::time::Instant::from_std(check.deadline), async {
                let mut tx = crate::db::begin_write(&self.0).await.repository()?;
                let Some(current) = current_in_tx(&mut tx, seed_id, maximum).await? else {
                    return Ok(Err(R::GenerationConflict));
                };
                if current.revoked
                    || matches!(
                        current.failure,
                        Some(Failure::NeedsReview | Failure::Disabled)
                    )
                    || now < current.last_success_at
                {
                    return Ok(Err(R::GenerationConflict));
                }
                let lease = sqlx::query(
                    r#"SELECT "LeaseOwner","LeaseUntil" FROM "TeletextGuideRefreshCurrent"
                    WHERE "SeedId"=? AND "NextDueAt"<=? AND ("LeaseOwner" IS NULL OR "LeaseUntil"<=?)"#)
                    .bind(seed_id).bind(now).bind(now).fetch_optional(&mut *tx).await.repository()?;
                let Some(lease) = lease else { return Ok(Ok(None)); };
                let abandoned = lease.try_get::<Option<String>,_>("LeaseOwner").repository()?
                    .map(|_| lease.try_get::<DateTime<Utc>,_>("LeaseUntil").repository()).transpose()?;
                if Instant::now() >= check.deadline {
                    return Ok(Err(timed_out()));
                }
                Ok::<_, RepositoryError>(Ok(Some((tx, current, abandoned))))
            })
            .await;
        let (mut tx, current, abandoned) = match admitted {
            Err(_) => return Ok(M::Rejected(timed_out())),
            Ok(Err(error)) => return Err(error),
            Ok(Ok(Err(reason))) => return Ok(M::Rejected(reason)),
            Ok(Ok(Ok(None))) => return Ok(M::Accepted(None)),
            Ok(Ok(Ok(Some(value)))) => value,
        };
        let lease_owner = Uuid::new_v4().to_string();
        let revision = current
            .revision
            .checked_add(i64::from(abandoned.is_some()))
            .ok_or_else(|| corrupt("refresh revision"))?;
        sqlx::query(r#"UPDATE "TeletextGuideRefreshCurrent" SET "LeaseOwner"=?,"LeaseUntil"=?,"Revision"=?,
            "FirstFailureAt"=COALESCE("FirstFailureAt",?),
            "Failure"=CASE WHEN "Failure" IS NULL AND ? IS NOT NULL THEN 2 ELSE "Failure" END WHERE "SeedId"=?"#)
            .bind(&lease_owner).bind(lease_until).bind(revision).bind(abandoned).bind(abandoned)
            .bind(seed_id).execute(&mut *tx).await.repository()?;
        tx.commit().await.repository()?;
        Ok(M::Accepted(Some(TeletextGuideRefreshClaim {
            seed: current.seed,
            revision,
            expected_runtime_id: current.runtime.map(|snapshot| snapshot.id),
            lease_owner,
            lease_until,
        })))
    }
    async fn heartbeat(
        &self,
        claim: &TeletextGuideRefreshClaim,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        check: &TeletextGuideGenerationCheck,
    ) -> Result<M<()>> {
        if until <= now || now < claim.seed.qualification.source.captured_at {
            return Ok(M::Rejected(invalid()));
        }
        let Some(until) = lease_cap(now, until, check) else {
            return Ok(M::Rejected(timed_out()));
        };
        let admitted =
            tokio::time::timeout_at(tokio::time::Instant::from_std(check.deadline), async {
                let tx = crate::db::begin_write(&self.0).await.repository()?;
                if Instant::now() >= check.deadline {
                    return Ok::<_, RepositoryError>(None);
                }
                Ok(Some(tx))
            })
            .await;
        let mut tx = match admitted {
            Err(_) | Ok(Ok(None)) => return Ok(M::Rejected(timed_out())),
            Ok(Err(error)) => return Err(error),
            Ok(Ok(Some(tx))) => tx,
        };
        let result=sqlx::query(r#"UPDATE "TeletextGuideRefreshCurrent" SET "LeaseUntil"=MAX("LeaseUntil",?)
            WHERE "SeedId"=? AND "Revision"=? AND "LeaseOwner"=? AND "LeaseUntil">? AND "Revoked"=0"#)
            .bind(until).bind(claim.seed.id).bind(claim.revision).bind(&claim.lease_owner).bind(now)
            .execute(&mut *tx).await.repository()?;
        if result.rows_affected() != 1 {
            return Ok(conflict());
        }
        tx.commit().await.repository()?;
        Ok(M::Accepted(()))
    }
    async fn complete(
        &self,
        claim: &TeletextGuideRefreshClaim,
        candidate: &TeletextGuideRefreshCandidate,
        now: DateTime<Utc>,
        check: &TeletextGuideGenerationCheck,
        maximum: usize,
    ) -> Result<M<TeletextGuideRuntimeSnapshot>> {
        complete(self, claim, candidate, now, check, maximum).await
    }
    async fn fail(
        &self,
        claim: &TeletextGuideRefreshClaim,
        now: DateTime<Utc>,
        next: DateTime<Utc>,
        failure: Failure,
    ) -> Result<M<()>> {
        if next <= now || next > plus(now, Duration::minutes(5))? {
            return Ok(M::Rejected(invalid()));
        }
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result=sqlx::query(r#"UPDATE "TeletextGuideRefreshCurrent" SET "FirstFailureAt"=COALESCE("FirstFailureAt",?),
            "Failure"=?,"NextDueAt"=?,"Revoked"=?,"LeaseOwner"=NULL,"LeaseUntil"=NULL,"Revision"="Revision"+1
            WHERE "SeedId"=? AND "Revision"=? AND "RuntimeId" IS ? AND "LeaseOwner"=? AND "Revoked"=0"#)
            .bind(now).bind(failure_code(failure)).bind(next).bind(i64::from(failure==Failure::Disabled))
            .bind(claim.seed.id).bind(claim.revision).bind(claim.expected_runtime_id).bind(&claim.lease_owner)
            .execute(&mut *tx).await.repository()?;
        if result.rows_affected() != 1 {
            return Ok(conflict());
        }
        tx.commit().await.repository()?;
        Ok(M::Accepted(()))
    }
    async fn revoke(&self, seed_id: EntityId, _now: DateTime<Utc>) -> Result<M<()>> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"UPDATE "TeletextGuideRefreshCurrent" SET "Revoked"=1,"LeaseOwner"=NULL,
            "LeaseUntil"=NULL,"Revision"="Revision"+1 WHERE "SeedId"=?"#,
        )
        .bind(seed_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if result.rows_affected() != 1 {
            return Ok(conflict());
        }
        tx.commit().await.repository()?;
        Ok(M::Accepted(()))
    }
}

async fn owns(
    tx: &mut Transaction<'_, Db>,
    claim: &TeletextGuideRefreshClaim,
    now: DateTime<Utc>,
) -> Result<bool> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "TeletextGuideRefreshCurrent"
        WHERE "SeedId"=? AND "Revision"=? AND "RuntimeId" IS ? AND "LeaseOwner"=? AND "LeaseUntil">?
        AND "Revoked"=0 AND ("Failure" IS NULL OR "Failure" IN (1,2)))"#,
    )
    .bind(claim.seed.id)
    .bind(claim.revision)
    .bind(claim.expected_runtime_id)
    .bind(&claim.lease_owner)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .repository()
}

async fn origin_current(
    tx: &mut Transaction<'_, Db>,
    seed: &TeletextGuideRefreshSeed,
    maximum: usize,
) -> Result<bool> {
    let generation = match seed.role {
        Role::Base => current_in_transaction(tx, seed.generation.request.channel_id).await?,
        Role::Effective => {
            effective::current_in_transaction(tx, seed.generation.request.channel_id).await?
        }
    };
    if generation.as_ref() != Some(&seed.generation) {
        return Ok(false);
    }
    let request = match seed.role {
        Role::Base => current_request(tx, seed.generation.request.channel_id).await?,
        Role::Effective => {
            super::super::teletext_publications::effective::current_in_transaction(
                tx,
                seed.generation.request.channel_id,
            )
            .await?
        }
    };
    if request.as_ref() != Some(&seed.generation.request) {
        return Ok(false);
    }
    let head = head_in_transaction(tx, seed.generation.request.design_id).await?;
    if head.is_none_or(|head| head.deleted || head.revision != seed.expected_design_head) {
        return Ok(false);
    }
    let revision = revision_in_transaction(
        tx,
        seed.generation.request.design_id,
        seed.generation.request.revision,
        maximum,
    )
    .await?;
    Ok(revision.is_some_and(|revision| {
        revision.digest == seed.generation.request.digest
            && super::super::teletext_publications::stored_fence(&revision.document)
                .ok()
                .flatten()
                .as_ref()
                == Some(&seed.qualification.source)
    }))
}

async fn selection_current(
    tx: &mut Transaction<'_, Db>,
    seed: &TeletextGuideRefreshSeed,
    selection: &TeletextGuideRefreshSelection,
    now: DateTime<Utc>,
) -> Result<bool> {
    match (seed.role, selection) {
        (Role::Base, TeletextGuideRefreshSelection::Base(selection)) => {
            if selection.channel_id != seed.generation.request.channel_id
                || !super::valid_selection(selection, selection.channel_id)
            {
                return Ok(false);
            }
            if !super::super::playout::teletext_intervals::active_in_transaction(
                tx,
                selection.channel_id,
                now,
            )
            .await?
            .is_empty()
            {
                return Ok(false);
            }
            Ok(super::check_published_selection(tx, selection)
                .await?
                .is_ok())
        }
        (Role::Effective, TeletextGuideRefreshSelection::Effective(selection)) => {
            if selection.channel_id != seed.generation.request.channel_id {
                return Ok(false);
            }
            Ok(
                matches!(super::super::teletext_profiles::recheck_effective_in_transaction(tx,selection,now).await?,TeletextProfileMutation::Saved(actual) if actual==*selection),
            )
        }
        _ => Ok(false),
    }
}

fn matches_origin_source(
    origin: &TeletextGuideSourceFence,
    source: &TeletextGuideSourceFence,
) -> bool {
    source.channel_ids == origin.channel_ids
        && source.locale == origin.locale
        && source.timezone == origin.timezone
        && source.captured_at >= origin.captured_at
}

fn document_digest_matches(document: &[u8], expected: &str) -> Result<bool> {
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(document) {
        write!(digest, "{byte:02x}").map_err(|_| corrupt("refresh digest"))?;
    }
    Ok(digest == expected)
}

async fn complete(
    repo: &SqliteTeletextGenerationRepository,
    claim: &TeletextGuideRefreshClaim,
    candidate: &TeletextGuideRefreshCandidate,
    now: DateTime<Utc>,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
) -> Result<M<TeletextGuideRuntimeSnapshot>> {
    let started = Instant::now();
    let observed_now = || -> Result<DateTime<Utc>> {
        plus(
            now,
            Duration::from_std(started.elapsed()).map_err(|_| corrupt("refresh elapsed clock"))?,
        )
    };
    let saved = &candidate.snapshot;
    if !bounded(maximum)
        || saved.document.is_empty()
        || saved.document.len() > maximum
        || saved.version != TELETEXT_GUIDE_RUNTIME_SNAPSHOT_VERSION
        || saved.document_version != 1
        || !valid_digest(&saved.document_digest)
        || !valid_digest(&saved.artifact_digest)
        || saved.qualification.validate_current(now).is_err()
    {
        return Ok(M::Rejected(invalid()));
    }
    let origin = &claim.seed.qualification.source;
    let source = &saved.qualification.source;
    if !matches_origin_source(origin, source) {
        return Ok(M::Rejected(R::GuideFreshnessRequired));
    }
    if !document_digest_matches(&saved.document, &saved.document_digest)? {
        return Ok(M::Rejected(R::Publication(P::DigestConflict)));
    }
    if Instant::now() >= check.deadline {
        return Ok(M::Rejected(timed_out()));
    }
    let admitted = tokio::time::timeout_at(tokio::time::Instant::from_std(check.deadline), async {
        let mut tx = crate::db::begin_write(&repo.0).await.repository()?;
        let now = observed_now()?;
        if !owns(&mut tx, claim, now).await?
            || seed_in_transaction(&mut tx, claim.seed.id).await?.as_ref() != Some(&claim.seed)
        {
            return Ok(Err(R::GenerationConflict));
        }
        if !origin_current(&mut tx, &claim.seed, maximum).await? {
            if Instant::now() >= check.deadline {
                return Ok(Err(timed_out()));
            }
            return Ok(Ok((tx, true)));
        }
        if !selection_current(&mut tx, &claim.seed, &candidate.selection, now).await? {
            return Ok(Err(R::Publication(P::SelectionChanged)));
        }
        let mut actual = match super::super::teletext_publications::check_current_source_at(
            &mut tx, source, check, now,
        )
        .await
        {
            Ok(actual) => actual,
            Err(reason) => return Ok(Err(reason)),
        };
        if let TeletextGuideRefreshSelection::Effective(selection) = &candidate.selection {
            if let Some(until) = selection.valid_until {
                actual.valid_until = actual.valid_until.min(until);
            }
        }
        if actual != saved.qualification {
            return Ok(Err(R::GuideFreshnessRequired));
        }
        if let Some(current) = current_in_tx(&mut tx, claim.seed.id, maximum).await? {
            let previous = current
                .runtime
                .as_ref()
                .map_or(origin.captured_at, |runtime| {
                    runtime.saved.qualification.source.captured_at
                });
            if source.captured_at < previous {
                return Ok(Err(R::GuideFreshnessRequired));
            }
        }
        let now = observed_now()?;
        if saved.qualification.validate_current(now).is_err() {
            return Ok(Err(R::GuideFreshnessRequired));
        }
        if !selection_current(&mut tx, &claim.seed, &candidate.selection, now).await? {
            return Ok(Err(R::Publication(P::SelectionChanged)));
        }
        if !owns(&mut tx, claim, now).await? || Instant::now() >= check.deadline {
            return Ok(Err(timed_out()));
        }
        Ok::<_, RepositoryError>(Ok((tx, false)))
    })
    .await;
    let (tx, needs_review) = match admitted {
        Err(_) => return Ok(M::Rejected(timed_out())),
        Ok(Err(error)) => return Err(error),
        Ok(Ok(Err(reason))) => return Ok(M::Rejected(reason)),
        Ok(Ok(Ok(value))) => value,
    };
    if needs_review {
        return stop_for_review(tx, claim).await;
    }
    insert_runtime(tx, claim, saved, observed_now()?).await
}

async fn stop_for_review(
    mut tx: Transaction<'_, Db>,
    claim: &TeletextGuideRefreshClaim,
) -> Result<M<TeletextGuideRuntimeSnapshot>> {
    // A newer manual intent blocks NEW promotion, not already selected output.
    // No source loss is invented and no runtime snapshot or pointer changes.
    sqlx::query(
        r#"UPDATE "TeletextGuideRefreshCurrent" SET "Failure"=3,
        "LeaseOwner"=NULL,"LeaseUntil"=NULL,"Revision"="Revision"+1 WHERE "SeedId"=?"#,
    )
    .bind(claim.seed.id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(M::Rejected(R::GenerationConflict))
}

async fn insert_runtime(
    mut tx: Transaction<'_, Db>,
    claim: &TeletextGuideRefreshClaim,
    saved: &TeletextGuideRuntimeSnapshotSave,
    now: DateTime<Utc>,
) -> Result<M<TeletextGuideRuntimeSnapshot>> {
    let source = &saved.qualification.source;
    let inserted=sqlx::query(r#"INSERT INTO "TeletextGuideRuntimeSnapshot"
        ("SeedId","Version","DocumentVersion","DocumentDigest","Document","Source","ValidUntil","ArtifactDigest") VALUES (?,?,?,?,?,?,?,?)"#)
        .bind(claim.seed.id).bind(i64::from(saved.version)).bind(i64::from(saved.document_version))
        .bind(&saved.document_digest).bind(&saved.document).bind(source_bytes(source)?).bind(saved.qualification.valid_until)
        .bind(&saved.artifact_digest).execute(&mut *tx).await.repository()?;
    let id = crate::db::last_insert_id(inserted);
    let next = plus(now, Duration::minutes(5))?.min(saved.qualification.valid_until);
    sqlx::query(r#"UPDATE "TeletextGuideRefreshCurrent" SET "RuntimeId"=?,"Revision"="Revision"+1,
        "NextDueAt"=?,"LastSuccessAt"=?,"ChangedAt"=NULL,"FirstFailureAt"=NULL,"Failure"=NULL,"LeaseOwner"=NULL,"LeaseUntil"=NULL
        WHERE "SeedId"=?"#).bind(id).bind(next).bind(now).bind(claim.seed.id).execute(&mut *tx).await.repository()?;
    // Bound automatic history by successful generations, not background uptime.
    // Manual origins and the current plus previous complete snapshots survive.
    sqlx::query(
        r#"DELETE FROM "TeletextGuideRuntimeSnapshot" WHERE "SeedId"=?
        AND "Id" NOT IN (SELECT "Id" FROM "TeletextGuideRuntimeSnapshot"
            WHERE "SeedId"=? ORDER BY "Id" DESC LIMIT 2)"#,
    )
    .bind(claim.seed.id)
    .bind(claim.seed.id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(M::Accepted(TeletextGuideRuntimeSnapshot {
        id,
        seed_id: claim.seed.id,
        saved: saved.clone(),
    }))
}
