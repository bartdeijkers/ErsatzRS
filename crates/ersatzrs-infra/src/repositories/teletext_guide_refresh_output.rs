//! Read-only runtime authority: fresh source proof or explicit sealed last-good.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{
    Result, TeletextGenerationMutation as M, TeletextGenerationRejection as R,
    TeletextGuideRefreshOutputCheck as Check,
    TeletextGuideRefreshOutputConfirmation as Confirmation, TeletextGuideRefreshRole as Role,
    TeletextGuideRefreshSelection as Selection, TeletextGuideRefreshStatus as Status,
    TeletextPublicationRejection as P,
};
use sqlx::Transaction;
use std::time::Instant;

use super::{
    bounded, corrupt, current_in_transaction, effective, observe_in_tx, plus, selection_current,
    timed_out,
};
use crate::{
    db::{Db, DbPool},
    repositories::SqlxResultExt as _,
};

pub(super) async fn confirm(
    pool: &DbPool,
    expected: &Confirmation,
    at: DateTime<Utc>,
    check: &Check,
    maximum: usize,
) -> Result<M<()>> {
    if !bounded(maximum)
        || !matches!(
            (expected.status, check),
            (Status::Fresh, Check::Fresh(_)) | (Status::Stale, Check::Stale { .. })
        )
    {
        return Ok(M::Rejected(R::GuideFreshnessRequired));
    }
    if Instant::now() >= check.deadline() {
        return Ok(M::Rejected(timed_out()));
    }
    let started = Instant::now();
    let now = || {
        plus(
            at,
            Duration::from_std(started.elapsed()).map_err(|_| corrupt("refresh output clock"))?,
        )
    };
    let result = tokio::time::timeout_at(tokio::time::Instant::from_std(check.deadline()), async {
        // Read-only transaction: confirm existing output, with no publication or job writes.
        let mut tx = pool.begin().await.repository()?;
        if let Err(reason) = association(&mut tx, expected, now()?, maximum).await? {
            return Ok(M::Rejected(reason));
        }
        if let Check::Fresh(bounds) = check {
            let mut actual =
                match super::super::super::teletext_publications::check_current_source_at(
                    &mut tx,
                    &expected.qualification.source,
                    bounds,
                    now()?,
                )
                .await
                {
                    Ok(actual) => actual,
                    Err(reason) => return Ok(M::Rejected(reason)),
                };
            if let Selection::Effective(scope) = &expected.selection {
                if let Some(until) = scope.valid_until {
                    actual.valid_until = actual.valid_until.min(until);
                }
            }
            if actual != expected.qualification {
                return Ok(M::Rejected(R::GuideFreshnessRequired));
            }
        }
        // Stale never asserts old raw-source equality and never starts CPU work.
        if let Err(reason) = association(&mut tx, expected, now()?, maximum).await? {
            return Ok(M::Rejected(reason));
        }
        if Instant::now() >= check.deadline() {
            return Ok(M::Rejected(timed_out()));
        }
        Ok::<_, ersatzrs_core::repositories::RepositoryError>(M::Accepted(()))
    })
    .await;
    result.unwrap_or_else(|_| Ok(M::Rejected(timed_out())))
}

async fn association(
    tx: &mut Transaction<'_, Db>,
    expected: &Confirmation,
    now: DateTime<Utc>,
    maximum: usize,
) -> Result<std::result::Result<(), R>> {
    let Some(current) = observe_in_tx(tx, expected.seed.id, maximum).await? else {
        return Ok(Err(R::GenerationConflict));
    };
    if current.seed != expected.seed
        || current.revision != expected.current_revision
        || current.runtime != expected.runtime
        || current.status(now) != expected.status
        || current.available_until() != expected.available_until
    {
        return Ok(Err(R::GenerationConflict));
    }
    let qualification = current
        .runtime
        .as_ref()
        .map_or(&current.seed.qualification, |runtime| {
            &runtime.qualification
        });
    if qualification != &expected.qualification
        || qualification.validate_current(now).is_err()
        || now >= expected.available_until
    {
        return Ok(Err(R::GuideFreshnessRequired));
    }
    if !manual_origin_current(tx, expected).await? {
        return Ok(Err(R::GenerationConflict));
    }
    if !selection_current(tx, &expected.seed, &expected.selection, now).await? {
        return Ok(Err(R::Publication(P::SelectionChanged)));
    }
    Ok(Ok(()))
}

async fn manual_origin_current(
    tx: &mut Transaction<'_, Db>,
    expected: &Confirmation,
) -> Result<bool> {
    let seed = &expected.seed;
    let channel = seed.generation.request.channel_id;
    let generation = match seed.role {
        Role::Base => current_in_transaction(tx, channel).await?,
        Role::Effective => effective::current_in_transaction(tx, channel).await?,
    };
    if generation.as_ref() != Some(&seed.generation) {
        return Ok(false);
    }
    // New concepts and pending requests do not change already selected output.
    // Their head/pending CAS remains mandatory only for NEW promotion.
    // Metadata alone binds the immutable original. Never SELECT its Document.
    let revision: Option<(i64,String)> = sqlx::query_as(r#"SELECT "DocumentVersion",
        CASE WHEN TYPEOF("Digest")='text' AND LENGTH(CAST("Digest" AS BLOB))=64 THEN "Digest" END AS "Digest"
        FROM "TeletextDesignRevision" WHERE "DesignId"=? AND "Revision"=?"#)
        .bind(seed.generation.request.design_id).bind(seed.generation.request.revision)
        .fetch_optional(&mut **tx).await.repository()?;
    Ok(revision.is_some_and(|(version, digest)| {
        version == seed.role.code() && digest == seed.generation.request.digest
    }))
}
