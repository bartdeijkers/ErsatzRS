//! Additive source-aware accepted roles; static generation paths stay closed.

use super::{
    check_selection, current_in_transaction, current_request, effective, head_in_transaction,
    insert_generation, revision_in_transaction, stored_fence, valid_digest, valid_request,
    valid_selection,
};
use crate::db::{Db, DbPool};
use crate::repositories::SqlxResultExt as _;
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{
    Result, TeletextEffectiveGuideGenerationSave, TeletextGenerationMutation as M,
    TeletextGenerationRejection as R, TeletextGuideGenerationCheck,
    TeletextGuideGenerationQualification, TeletextGuideGenerationSave,
    TeletextPublicationRejection as P, TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{
    EntityId, TeletextAcceptedGeneration, TeletextChannelSelectionSnapshot,
    TeletextEffectiveChannelSelectionSnapshot, TeletextPublicationRequest,
    TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
};
use sqlx::Transaction;
use std::time::Instant;

use super::super::teletext_publications::{
    check_current_source, effective as publication, effective_scope,
};

#[derive(Clone, Copy)]
enum Scope<'a> {
    Base(&'a TeletextChannelSelectionSnapshot),
    Effective(&'a TeletextEffectiveChannelSelectionSnapshot),
}

impl Scope<'_> {
    fn artifact(self) -> u16 {
        match self {
            Self::Base(_) => TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
            Self::Effective(_) => TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
        }
    }
    fn valid(self, channel: EntityId) -> bool {
        match self {
            Self::Base(selection) => valid_selection(selection, channel),
            Self::Effective(selection) => {
                selection.channel_id == channel && publication::valid_scope(selection)
            }
        }
    }
    fn valid_until(self) -> Option<DateTime<Utc>> {
        match self {
            Self::Base(_) => None,
            Self::Effective(scope) => scope.valid_until,
        }
    }
    async fn current(
        self,
        tx: &mut Transaction<'_, Db>,
        channel: EntityId,
    ) -> Result<Option<TeletextAcceptedGeneration>> {
        match self {
            Self::Base(_) => current_in_transaction(tx, channel).await,
            Self::Effective(_) => effective::current_in_transaction(tx, channel).await,
        }
    }
    async fn check(self, tx: &mut Transaction<'_, Db>) -> Result<std::result::Result<(), R>> {
        match self {
            Self::Base(selection) => check_selection(tx, selection).await,
            Self::Effective(selection) => publication::check_scope(tx, selection)
                .await
                .map(|result| result.map_err(R::Publication)),
        }
    }
}

struct Target<'a> {
    request: &'a TeletextPublicationRequest,
    expected_head: i64,
    expected_generation: Option<EntityId>,
    scope: Scope<'a>,
    artifact_version: u16,
    artifact_digest: &'a str,
}

pub(super) async fn accept_base(
    pool: &DbPool,
    save: &TeletextGuideGenerationSave,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
) -> Result<M<TeletextAcceptedGeneration>> {
    let deadline = check.deadline;
    let original = &save.original;
    let target = Target {
        request: &original.request,
        expected_head: original.expected_design_head,
        expected_generation: original.expected_generation_id,
        scope: Scope::Base(&original.selection),
        artifact_version: original.artifact_version,
        artifact_digest: &original.artifact_digest,
    };
    match admit_timed(pool, &target, &save.qualification, check, maximum, deadline).await? {
        Err(reason) => Ok(M::Rejected(reason)),
        Ok(tx) => {
            if let Err(reason) = check_time(&save.qualification, target.scope, deadline) {
                return Ok(M::Rejected(reason));
            }
            insert_generation(tx, original).await
        }
    }
}

pub(super) async fn accept_effective(
    pool: &DbPool,
    save: &TeletextEffectiveGuideGenerationSave,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
) -> Result<M<TeletextAcceptedGeneration>> {
    let deadline = check.deadline;
    let original = &save.original;
    let target = Target {
        request: &original.request,
        expected_head: original.expected_design_head,
        expected_generation: original.expected_generation_id,
        scope: Scope::Effective(&original.selection),
        artifact_version: original.artifact_version,
        artifact_digest: &original.artifact_digest,
    };
    match admit_timed(pool, &target, &save.qualification, check, maximum, deadline).await? {
        Err(reason) => Ok(M::Rejected(reason)),
        Ok(tx) => {
            if let Err(reason) = check_time(&save.qualification, target.scope, deadline) {
                return Ok(M::Rejected(reason));
            }
            effective::insert(tx, original, deadline).await
        }
    }
}

fn valid_target(target: &Target<'_>, maximum: usize) -> bool {
    valid_request(target.request)
        && target.expected_head > 0
        && target.expected_generation.is_none_or(|id| id > 0)
        && target.scope.valid(target.request.channel_id)
        && target.artifact_version == target.scope.artifact()
        && valid_digest(target.artifact_digest)
        && maximum > 0
        && isize::try_from(maximum).is_ok()
}

fn timeout() -> R {
    R::Publication(P::GuideTimedOut)
}

fn check_time(
    qualification: &TeletextGuideGenerationQualification,
    scope: Scope<'_>,
    deadline: Instant,
) -> std::result::Result<(), R> {
    if Instant::now() >= deadline {
        return Err(timeout());
    }
    let now = Utc::now();
    qualification
        .validate_current(now)
        .map_err(|_| R::GuideFreshnessRequired)?;
    if scope.valid_until().is_some_and(|until| now >= until) {
        return Err(R::Publication(P::SelectionChanged));
    }
    Ok(())
}

fn admission_deadline(
    qualification: &TeletextGuideGenerationQualification,
    scope: Scope<'_>,
    deadline: Instant,
) -> std::result::Result<Instant, R> {
    let monotonic = Instant::now();
    let observed = Utc::now();
    check_time(qualification, scope, deadline)?;
    let until = scope
        .valid_until()
        .map_or(qualification.valid_until, |until| {
            until.min(qualification.valid_until)
        });
    let remaining = until
        .signed_duration_since(observed)
        .to_std()
        .map_err(|_| R::GuideFreshnessRequired)?;
    let boundary = monotonic
        .checked_add(remaining)
        .ok_or(R::GuideFreshnessRequired)?;
    Ok(deadline.min(boundary))
}

async fn admit_timed(
    pool: &DbPool,
    target: &Target<'_>,
    qualification: &TeletextGuideGenerationQualification,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
    deadline: Instant,
) -> Result<std::result::Result<Transaction<'static, Db>, R>> {
    if !valid_target(target, maximum) {
        return Ok(Err(R::Publication(P::InvalidValues)));
    }
    let deadline = match admission_deadline(qualification, target.scope, deadline) {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    match tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        admit(pool, target, qualification, check, maximum, deadline),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Ok(Err(timeout())),
    }
}

async fn admit(
    pool: &DbPool,
    target: &Target<'_>,
    qualification: &TeletextGuideGenerationQualification,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
    deadline: Instant,
) -> Result<std::result::Result<Transaction<'static, Db>, R>> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    if let Err(reason) = check_time(qualification, target.scope, deadline) {
        return Ok(Err(reason));
    }
    if let Err(reason) = check_acceptance(&mut tx, target).await? {
        return Ok(Err(reason));
    }
    if let Err(reason) = target.scope.check(&mut tx).await? {
        return Ok(Err(reason));
    }
    if let Err(reason) = check_revision(
        &mut tx,
        target.request,
        target.scope,
        qualification,
        check,
        maximum,
        deadline,
    )
    .await?
    {
        return Ok(Err(reason));
    }
    if let Err(reason) = check_time(qualification, target.scope, deadline) {
        return Ok(Err(reason));
    }
    Ok(Ok(tx))
}

async fn check_acceptance(
    tx: &mut Transaction<'_, Db>,
    target: &Target<'_>,
) -> Result<std::result::Result<(), R>> {
    let pending = match target.scope {
        Scope::Base(_) => current_request(tx, target.request.channel_id).await?,
        Scope::Effective(_) => {
            publication::current_in_transaction(tx, target.request.channel_id).await?
        }
    };
    if pending.as_ref() != Some(target.request) {
        return Ok(Err(R::Publication(P::RequestConflict)));
    }
    let current = target.scope.current(tx, target.request.channel_id).await?;
    let already:bool=sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "TeletextAcceptedGeneration" WHERE "ChannelId"=? AND "RequestId"=?)"#)
        .bind(target.request.channel_id).bind(target.request.id).fetch_one(&mut **tx).await.repository()?;
    if already {
        return Ok(Err(R::AlreadyAccepted));
    }
    if current.as_ref().map(|generation| generation.id) != target.expected_generation {
        return Ok(Err(R::GenerationConflict));
    }
    let Some(head) = head_in_transaction(tx, target.request.design_id).await? else {
        return Ok(Err(R::Publication(P::MissingDesign)));
    };
    if head.deleted {
        return Ok(Err(R::Publication(P::DeletedDesign)));
    }
    if head.revision != target.expected_head {
        return Ok(Err(R::Publication(P::DesignConflict)));
    }
    Ok(Ok(()))
}

async fn check_revision(
    tx: &mut Transaction<'_, Db>,
    request: &TeletextPublicationRequest,
    scope: Scope<'_>,
    qualification: &TeletextGuideGenerationQualification,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
    deadline: Instant,
) -> Result<std::result::Result<(), R>> {
    let Some(revision) =
        revision_in_transaction(tx, request.design_id, request.revision, maximum).await?
    else {
        return Ok(Err(R::Publication(P::MissingRevision)));
    };
    let expected_version = match scope {
        Scope::Base(_) => ersatzrs_domain::TELETEXT_DESIGN_DOCUMENT_VERSION,
        Scope::Effective(_) => TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
    };
    if revision.document_version != expected_version {
        return Ok(Err(R::Publication(P::InvalidValues)));
    }
    if revision.digest != request.digest {
        return Ok(Err(R::Publication(P::DigestConflict)));
    }
    if let Scope::Effective(selection) = scope {
        if !effective_scope::matches(&revision.document, selection) {
            return Ok(Err(R::Publication(P::SelectionChanged)));
        }
    }
    let source = match stored_fence(&revision.document) {
        Ok(Some(source)) => source,
        Ok(None) => return Ok(Err(R::Publication(P::GuideFenceRequired))),
        Err(()) => return Ok(Err(R::Publication(P::GuideUnavailable))),
    };
    if source != qualification.source {
        return Ok(Err(R::Publication(P::GuideFenceMismatch)));
    }
    let bounded = TeletextGuideGenerationCheck {
        limits: check.limits,
        deadline,
        hold: check.hold.clone(),
    };
    let mut actual = match check_current_source(tx, &source, &bounded).await {
        Ok(actual) => actual,
        Err(reason) => return Ok(Err(reason)),
    };
    if let Some(scope_until) = scope.valid_until() {
        actual.valid_until = actual.valid_until.min(scope_until);
    }
    if actual != *qualification {
        return Ok(Err(R::GuideFreshnessRequired));
    }
    if let Err(reason) = check_time(qualification, scope, deadline) {
        return Ok(Err(reason));
    }
    Ok(Ok(()))
}

pub(super) async fn confirm_base(
    pool: &DbPool,
    generation: &TeletextAcceptedGeneration,
    selection: &TeletextChannelSelectionSnapshot,
    qualification: &TeletextGuideGenerationQualification,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
) -> Result<M<()>> {
    let deadline = check.deadline;
    confirm(
        pool,
        generation,
        Scope::Base(selection),
        qualification,
        check,
        maximum,
        deadline,
    )
    .await
}

pub(super) async fn confirm_effective(
    pool: &DbPool,
    generation: &TeletextAcceptedGeneration,
    selection: &TeletextEffectiveChannelSelectionSnapshot,
    qualification: &TeletextGuideGenerationQualification,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
) -> Result<M<()>> {
    let deadline = check.deadline;
    confirm(
        pool,
        generation,
        Scope::Effective(selection),
        qualification,
        check,
        maximum,
        deadline,
    )
    .await
}

async fn confirm(
    pool: &DbPool,
    generation: &TeletextAcceptedGeneration,
    scope: Scope<'_>,
    qualification: &TeletextGuideGenerationQualification,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
    deadline: Instant,
) -> Result<M<()>> {
    if generation.id <= 0
        || !valid_request(&generation.request)
        || !scope.valid(generation.request.channel_id)
        || generation.artifact_version != scope.artifact()
        || !valid_digest(&generation.artifact_digest)
        || maximum == 0
        || isize::try_from(maximum).is_err()
    {
        return Ok(M::Rejected(R::Publication(P::InvalidValues)));
    }
    let deadline = match admission_deadline(qualification, scope, deadline) {
        Ok(value) => value,
        Err(reason) => return Ok(M::Rejected(reason)),
    };
    match tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        confirm_before(
            pool,
            generation,
            scope,
            qualification,
            check,
            maximum,
            deadline,
        ),
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
    scope: Scope<'_>,
    qualification: &TeletextGuideGenerationQualification,
    check: &TeletextGuideGenerationCheck,
    maximum: usize,
    deadline: Instant,
) -> Result<M<()>> {
    // This read-only transaction confirms one consistent accepted/source snapshot.
    // It never changes requests, generations, selection or publication pointers.
    let mut tx = pool.begin().await.repository()?;
    if let Err(reason) = check_time(qualification, scope, deadline) {
        return Ok(M::Rejected(reason));
    }
    if scope
        .current(&mut tx, generation.request.channel_id)
        .await?
        .as_ref()
        != Some(generation)
    {
        return Ok(M::Rejected(R::GenerationConflict));
    }
    if matches!(scope, Scope::Base(_))
        && !super::super::playout::teletext_intervals::active_in_transaction(
            &mut tx,
            generation.request.channel_id,
            Utc::now(),
        )
        .await?
        .is_empty()
    {
        return Ok(M::Rejected(R::Publication(P::SelectionChanged)));
    }
    if let Err(reason) = scope.check(&mut tx).await? {
        return Ok(M::Rejected(reason));
    }
    if let Err(reason) = check_revision(
        &mut tx,
        &generation.request,
        scope,
        qualification,
        check,
        maximum,
        deadline,
    )
    .await?
    {
        return Ok(M::Rejected(reason));
    }
    if let Err(reason) = check_time(qualification, scope, deadline) {
        return Ok(M::Rejected(reason));
    }
    Ok(M::Accepted(()))
}

#[cfg(test)]
#[path = "teletext_guide_generations_tests.rs"]
mod tests;
