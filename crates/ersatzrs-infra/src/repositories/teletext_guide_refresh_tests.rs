//! Bounded durable-refresh ownership; native editorial proof remains a host gate.

use super::*;
use chrono::Duration as ChronoDuration;
use ersatzrs_core::repositories::{
    TeletextGuideRefreshCandidate, TeletextGuideRefreshClaim, TeletextGuideRefreshCurrent,
    TeletextGuideRefreshFailure, TeletextGuideRefreshOutputCheck as OutputCheck,
    TeletextGuideRefreshOutputConfirmation as OutputConfirmation, TeletextGuideRefreshRepository,
    TeletextGuideRefreshRole, TeletextGuideRefreshSelection, TeletextGuideRefreshStatus,
    TeletextGuideRuntimeSnapshotSave,
};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

async fn base_current(port: &dyn TeletextGuideRefreshRepository) -> TeletextGuideRefreshCurrent {
    port.current(1, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .unwrap()
}

async fn claim_at(
    port: &dyn TeletextGuideRefreshRepository,
    seed_id: i64,
    at: chrono::DateTime<Utc>,
) -> TeletextGuideRefreshClaim {
    claimed(
        port.claim(
            seed_id,
            at,
            at + ChronoDuration::seconds(5),
            &check(),
            MAXIMUM,
        )
        .await
        .unwrap(),
    )
}

async fn seeded() -> (
    Fixture,
    SqliteTeletextGenerationRepository,
    TeletextGuideGenerationSave,
) {
    let fixture = Fixture::new().await;
    let repo = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let save = base_save(&fixture).await;
    assert!(repo
        .guide_refresh()
        .unwrap()
        .current(1, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .is_none());
    accepted(repo.accept_guide(&save, &check(), MAXIMUM).await.unwrap());
    (fixture, repo, save)
}

fn claimed(value: M<Option<TeletextGuideRefreshClaim>>) -> TeletextGuideRefreshClaim {
    match value {
        M::Accepted(Some(claim)) => claim,
        other => panic!("claim rejected: {other:?}"),
    }
}

async fn candidate_at(
    fixture: &Fixture,
    at: chrono::DateTime<Utc>,
) -> TeletextGuideRefreshCandidate {
    let document = b"synthetic opaque validated native snapshot envelope".to_vec();
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(&document) {
        write!(digest, "{byte:02x}").unwrap();
    }
    TeletextGuideRefreshCandidate {
        snapshot: TeletextGuideRuntimeSnapshotSave {
            version: 1,
            document_version: 1,
            document,
            document_digest: digest,
            artifact_digest: "c".repeat(64),
            qualification: qualification_at(fixture, at).await,
        },
        selection: TeletextGuideRefreshSelection::Base(fixture.selection.clone()),
    }
}

async fn unchanged_manual(repo: &SqliteTeletextGenerationRepository) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for sql in [
        r#"SELECT json_array("Id","Name","Revision","IsDeleted") FROM "TeletextDesign" ORDER BY "Id""#,
        r#"SELECT json_array("DesignId","Revision","Digest",hex("Document")) FROM "TeletextDesignRevision" ORDER BY "DesignId","Revision""#,
        r#"SELECT json_array("Id","ChannelId","DesignId","Revision","Digest") FROM "TeletextPublicationRequest" ORDER BY "Id""#,
        r#"SELECT json_array("ChannelId","RequestId") FROM "TeletextPublicationRequestCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("Id","ChannelId","RequestId","ArtifactVersion","ArtifactDigest") FROM "TeletextAcceptedGeneration" ORDER BY "Id""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextAcceptedGenerationCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextEffectiveAcceptedGenerationCurrent" ORDER BY "ChannelId""#,
    ] {
        rows.push(sqlx::query_scalar(sql).fetch_all(&repo.0).await.unwrap());
    }
    rows
}

async fn check_restarted_lease(
    fixture: &Fixture,
    first: &TeletextGuideRefreshClaim,
    now: chrono::DateTime<Utc>,
    first_loss: chrono::DateTime<Utc>,
) {
    let reopened = SqliteTeletextGenerationRepository(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    let again = reopened.guide_refresh().unwrap();
    let renewed = now + ChronoDuration::seconds(2);
    assert_eq!(
        again
            .heartbeat(first, renewed, now + ChronoDuration::seconds(7), &check())
            .await
            .unwrap(),
        M::Accepted(())
    );
    assert_eq!(
        again
            .claim(
                first.seed.id,
                now + ChronoDuration::seconds(6),
                now + ChronoDuration::seconds(8),
                &check(),
                MAXIMUM
            )
            .await
            .unwrap(),
        M::Accepted(None)
    );
    let later = now + ChronoDuration::seconds(8);
    let second = claimed(
        again
            .claim(
                first.seed.id,
                later,
                later + ChronoDuration::seconds(5),
                &check(),
                MAXIMUM,
            )
            .await
            .unwrap(),
    );
    assert_ne!(first.lease_owner, second.lease_owner);
    assert_eq!(
        again
            .heartbeat(first, later, later + ChronoDuration::seconds(5), &check())
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    assert!(again
        .current(2, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .is_none());
    let recovered = base_current(again).await;
    assert_eq!(recovered.first_failure_at, Some(first_loss));
    assert_eq!(
        recovered.failure,
        Some(TeletextGuideRefreshFailure::SourceUnavailable)
    );
    assert_eq!(
        again
            .fail(
                first,
                later,
                later + ChronoDuration::minutes(1),
                TeletextGuideRefreshFailure::SourceUnavailable
            )
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
}

#[tokio::test]
async fn refresh_seed_cadence_coalescing_and_restart_lease_are_durable() {
    let (fixture, repo, save) = seeded().await;
    let port = repo.guide_refresh().unwrap();
    let current = base_current(port).await;
    let capture = save.qualification.source.captured_at;
    assert_eq!(
        port.scan_current(None, 1).await.unwrap(),
        vec![current.seed.clone()]
    );
    assert!(port
        .scan_current(Some(current.seed.id), 1)
        .await
        .unwrap()
        .is_empty());
    assert!(port.scan_current(Some(0), 1).await.is_err());
    assert!(port.scan_current(None, 257).await.is_err());
    // A near role edge wins over both the debounce and normal cadence.
    assert_eq!(
        super::super::super::refresh::changed_due(
            capture + ChronoDuration::seconds(1),
            capture,
            capture + ChronoDuration::seconds(30)
        )
        .unwrap(),
        capture + ChronoDuration::seconds(30)
    );
    assert_eq!(
        current.seed.generation,
        ersatzrs_core::repositories::TeletextGenerationRepository::current(&repo, 1)
            .await
            .unwrap()
            .unwrap()
    );
    assert_eq!(
        current.next_due_at,
        (capture + ChronoDuration::minutes(5)).min(save.qualification.valid_until)
    );
    assert!(port.due(capture, 1).await.unwrap().is_empty());
    assert!(port.due(capture, 0).await.is_err());
    assert!(port.due(capture, 257).await.is_err());
    assert_eq!(
        port.changed(current.seed.id, capture + ChronoDuration::seconds(10))
            .await
            .unwrap(),
        M::Accepted(())
    );
    let changed = base_current(port).await;
    assert_eq!(changed.next_due_at, capture + ChronoDuration::seconds(70));
    let first_loss = capture + ChronoDuration::seconds(10);
    assert_eq!(
        changed.status(first_loss),
        TeletextGuideRefreshStatus::Stale
    );
    assert_eq!(changed.first_failure_at, Some(first_loss));
    let dirty_revision = changed.revision;
    assert_eq!(
        port.changed(current.seed.id, capture + ChronoDuration::seconds(280))
            .await
            .unwrap(),
        M::Accepted(())
    );
    let changed = base_current(port).await;
    assert_eq!(changed.next_due_at, capture + ChronoDuration::seconds(70));
    assert_eq!(changed.first_failure_at, Some(first_loss));
    assert_eq!(changed.revision, dirty_revision);
    let now = changed.next_due_at;
    assert_eq!(port.due(now, 1).await.unwrap(), vec![current.seed.clone()]);
    let first = claim_at(port, current.seed.id, now).await;
    assert_eq!(
        port.claim(
            current.seed.id,
            now,
            now + ChronoDuration::seconds(5),
            &check(),
            MAXIMUM
        )
        .await
        .unwrap(),
        M::Accepted(None)
    );
    check_restarted_lease(&fixture, &first, now, first_loss).await;
}

async fn check_immutable_history(
    repo: &SqliteTeletextGenerationRepository,
    seed_id: i64,
    snapshot_id: i64,
) {
    for statement in [
        r#"UPDATE "TeletextGuideRefreshSeed" SET "ExpectedDesignHead"=99 WHERE "Id"=?"#,
        r#"DELETE FROM "TeletextGuideRefreshSeed" WHERE "Id"=?"#,
    ] {
        assert!(sqlx::query(statement)
            .bind(seed_id)
            .execute(&repo.0)
            .await
            .is_err());
    }
    for statement in [
        r#"UPDATE "TeletextGuideRuntimeSnapshot" SET "DocumentVersion"=1 WHERE "Id"=?"#,
        r#"DELETE FROM "TeletextGuideRuntimeSnapshot" WHERE "Id"=?"#,
    ] {
        assert!(sqlx::query(statement)
            .bind(snapshot_id)
            .execute(&repo.0)
            .await
            .is_err());
    }
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&repo.0)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn refresh_success_after_seed_role_expiry_retains_manual_bytes_and_reopens_runtime() {
    let (fixture, repo, save) = seeded().await;
    let port = repo.guide_refresh().unwrap();
    let seed = base_current(port).await.seed;
    let now = save.qualification.valid_until + ChronoDuration::seconds(1);
    let claim = claim_at(port, seed.id, now).await;
    let candidate = candidate_at(&fixture, now).await;
    assert!(candidate.snapshot.qualification.valid_until > seed.qualification.valid_until);
    let before = unchanged_manual(&repo).await;
    let M::Accepted(snapshot) = port
        .complete(&claim, &candidate, now, &check(), MAXIMUM)
        .await
        .unwrap()
    else {
        panic!("fresh runtime rejected")
    };
    assert_eq!(unchanged_manual(&repo).await, before);
    assert_eq!(snapshot.seed_id, seed.id);
    assert_eq!(snapshot.saved, candidate.snapshot);
    assert_eq!(
        port.complete(&claim, &candidate, now, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    let snapshot = check_runtime_retention(&repo, &fixture, snapshot).await;
    assert_eq!(unchanged_manual(&repo).await, before);
    let now = snapshot.saved.qualification.source.captured_at;
    let reopened = SqliteTeletextGenerationRepository(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    let current = base_current(reopened.guide_refresh().unwrap()).await;
    assert_eq!(current.runtime, Some(snapshot.clone()));
    let metadata = port
        .observe_current(1, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.seed, current.seed);
    let identity = metadata.runtime.as_ref().unwrap();
    assert_eq!(
        (identity.id, identity.seed_id),
        (snapshot.id, snapshot.seed_id)
    );
    assert_eq!(identity.document_digest, snapshot.saved.document_digest);
    assert_eq!(identity.qualification, snapshot.saved.qualification);
    assert_eq!(metadata.status(now), current.status(now));
    assert!(port
        .observe_current(1, TeletextGuideRefreshRole::Base, 1)
        .await
        .is_err());
    assert_eq!(current.status(now), TeletextGuideRefreshStatus::Fresh);
    check_fresh_output(&repo, &fixture, &metadata, current.last_success_at).await;
    // A failed successor retains this snapshot's finite boundary, never the
    // expired seed role edge and never a boundary renewed from the retry clock.
    let failure_at = current.next_due_at;
    let retry = claim_at(port, seed.id, failure_at).await;
    assert_eq!(
        port.fail(
            &retry,
            failure_at,
            failure_at + ChronoDuration::minutes(1),
            TeletextGuideRefreshFailure::PreparationFailed
        )
        .await
        .unwrap(),
        M::Accepted(())
    );
    let stale = base_current(port).await;
    assert_eq!(stale.status(failure_at), TeletextGuideRefreshStatus::Stale);
    check_stale_output(&repo, &fixture, failure_at).await;
    assert_eq!(
        stale.available_until(),
        snapshot
            .saved
            .qualification
            .valid_until
            .min(failure_at + ChronoDuration::minutes(15))
    );
    assert!(reopened
        .guide_refresh()
        .unwrap()
        .current(1, TeletextGuideRefreshRole::Base, 1)
        .await
        .is_err());
    check_immutable_history(&repo, seed.id, snapshot.id).await;
}

#[tokio::test]
async fn refresh_failed_retries_never_renew_stale_deadline_and_review_or_revoke_withdraws() {
    let (fixture, repo, save) = seeded().await;
    let port = repo.guide_refresh().unwrap();
    let seed = base_current(port).await.seed;
    let now = save.qualification.source.captured_at + ChronoDuration::minutes(5);
    let claim = claim_at(port, seed.id, now).await;
    let failed_at = now + ChronoDuration::seconds(6);
    let working = base_current(port).await;
    assert!(working.refreshing);
    assert_ne!(working.status(now), TeletextGuideRefreshStatus::Fresh);
    assert_eq!(working.first_failure_at, None);
    assert_eq!(working.failure, None);
    let next = now + ChronoDuration::minutes(1);
    assert_eq!(
        port.fail(
            &claim,
            failed_at,
            next,
            TeletextGuideRefreshFailure::SourceUnavailable
        )
        .await
        .unwrap(),
        M::Accepted(())
    );
    let current = base_current(port).await;
    assert_eq!(current.first_failure_at, Some(failed_at));
    assert_eq!(
        current.available_until(),
        (failed_at + ChronoDuration::minutes(15)).min(seed.qualification.valid_until)
    );
    assert_eq!(
        current.status(failed_at),
        if failed_at < seed.qualification.valid_until {
            TeletextGuideRefreshStatus::Stale
        } else {
            TeletextGuideRefreshStatus::Unavailable
        }
    );
    let second = claim_at(port, seed.id, next).await;
    assert_eq!(
        port.fail(
            &second,
            next,
            next + ChronoDuration::minutes(1),
            TeletextGuideRefreshFailure::PreparationFailed
        )
        .await
        .unwrap(),
        M::Accepted(())
    );
    let current = base_current(port).await;
    assert_eq!(current.first_failure_at, Some(failed_at));
    assert_eq!(
        current.status(current.available_until()),
        TeletextGuideRefreshStatus::Unavailable
    );
    let third_at = next + ChronoDuration::minutes(1);
    let third = claim_at(port, seed.id, third_at).await;
    assert_eq!(
        port.fail(
            &third,
            third_at,
            third_at + ChronoDuration::minutes(1),
            TeletextGuideRefreshFailure::NeedsReview
        )
        .await
        .unwrap(),
        M::Accepted(())
    );
    assert!(port
        .due(third_at + ChronoDuration::minutes(5), 1)
        .await
        .unwrap()
        .is_empty());
    let review = base_current(port).await;
    assert_eq!(
        review.failure,
        Some(TeletextGuideRefreshFailure::NeedsReview)
    );
    assert_eq!(
        review.status(third_at),
        if third_at < review.available_until() {
            TeletextGuideRefreshStatus::Stale
        } else {
            TeletextGuideRefreshStatus::Unavailable
        }
    );
    assert_eq!(
        port.revoke(seed.id, third_at).await.unwrap(),
        M::Accepted(())
    );
    assert_eq!(
        base_current(port).await.status(third_at),
        TeletextGuideRefreshStatus::Revoked
    );
    assert!(port
        .current(2, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .is_none());
    drop(fixture);
}

#[tokio::test]
async fn refresh_stale_owner_source_selection_budget_and_manual_head_cannot_promote() {
    let (fixture, repo, save) = seeded().await;
    let port = repo.guide_refresh().unwrap();
    let seed = base_current(port).await.seed;
    let now = save.qualification.source.captured_at + ChronoDuration::minutes(5);
    let claim = claim_at(port, seed.id, now).await;
    let candidate = candidate_at(&fixture, now).await;
    let before = unchanged_manual(&repo).await;
    let mut invalid = candidate.clone();
    invalid.snapshot.document_digest = "d".repeat(64);
    assert_eq!(
        port.complete(&claim, &invalid, now, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::DigestConflict))
    );
    let mut wrong = candidate.clone();
    let TeletextGuideRefreshSelection::Base(ref mut selection) = wrong.selection else {
        unreachable!()
    };
    selection.bindings.clear();
    assert_eq!(
        port.complete(&claim, &wrong, now, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::SelectionChanged))
    );
    let mut limits = check();
    limits.limits.max_records = 0;
    assert!(matches!(
        port.complete(&claim, &candidate, now, &limits, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(_)
    ));
    let mut expired = check();
    expired.deadline = Instant::now();
    assert_eq!(
        port.complete(&claim, &candidate, now, &expired, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::GuideTimedOut))
    );
    sqlx::query("UPDATE Channel SET Name='Source changed' WHERE Id=2")
        .execute(&repo.0)
        .await
        .unwrap();
    assert_eq!(
        port.complete(&claim, &candidate, now, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::GuideChanged))
    );
    assert_eq!(unchanged_manual(&repo).await, before);
    assert_eq!(port.changed(seed.id, now).await.unwrap(), M::Accepted(()));
    assert_eq!(
        port.complete(&claim, &candidate, now, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    let next = now + ChronoDuration::minutes(1);
    let second = claim_at(port, seed.id, next).await;
    let candidate = candidate_at(&fixture, next).await;
    sqlx::query(r#"UPDATE "TeletextDesign" SET "Revision"="Revision"+1 WHERE "Id"=?"#)
        .bind(save.original.request.design_id)
        .execute(&repo.0)
        .await
        .unwrap();
    assert_eq!(
        port.complete(&second, &candidate, next, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    assert!(base_current(port).await.runtime.is_none());
    check_new_pending_preserves_old_read(
        &repo,
        &fixture,
        &second,
        &candidate,
        next,
        TeletextGuideRefreshStatus::Stale,
    )
    .await;
    check_draft_alone_keeps_fresh_until_source_loss().await;
}

#[tokio::test]
async fn refresh_effective_scope_end_cannot_be_extended_by_new_source_or_runtime() {
    use crate::repositories::teletext_publications::effective::tests as scoped;
    let fixture = scoped::Fixture::new(true).await;
    let repo = SqliteTeletextGenerationRepository(fixture.base.repo.0.clone());
    let mut qualification = qualification(&fixture.base).await;
    let scope_until = fixture.save.selection.valid_until.unwrap();
    qualification.valid_until = qualification.valid_until.min(scope_until);
    let document = serde_json::to_vec(
        &serde_json::json!({"format_version":2,"guide_source":qualification.source}),
    )
    .unwrap();
    let request = scoped::design(&fixture.base, fixture.save.selection.clone(), &document).await;
    let TeletextPublicationMutation::Requested(request) = fixture
        .base
        .repo
        .request_effective_with_source_held(&request, &qualification.source, &check(), MAXIMUM)
        .await
        .unwrap()
    else {
        panic!("scope request rejected")
    };
    let save = TeletextEffectiveGuideGenerationSave {
        original: TeletextEffectiveGenerationSave {
            request,
            expected_design_head: 1,
            expected_generation_id: None,
            selection: fixture.save.selection.clone(),
            artifact_version: TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
            artifact_digest: "e".repeat(64),
        },
        qualification,
    };
    accepted(
        repo.accept_effective_guide(&save, &check(), MAXIMUM)
            .await
            .unwrap(),
    );
    let port = repo.guide_refresh().unwrap();
    assert!(port
        .current(1, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .is_none());
    let current = port
        .current(1, TeletextGuideRefreshRole::Effective, MAXIMUM)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.seed.role, TeletextGuideRefreshRole::Effective);
    let now = current.next_due_at;
    let claim = claim_at(port, current.seed.id, now).await;
    let mut valid = candidate_at(&fixture.base, now).await;
    valid.selection = TeletextGuideRefreshSelection::Effective(fixture.save.selection.clone());
    valid.snapshot.qualification.valid_until =
        valid.snapshot.qualification.valid_until.min(scope_until);
    let before = unchanged_manual(&repo).await;
    let M::Accepted(runtime) = port
        .complete(&claim, &valid, now, &check(), MAXIMUM)
        .await
        .unwrap()
    else {
        panic!("current scope runtime rejected")
    };
    assert!(runtime.saved.qualification.valid_until <= scope_until);
    assert_eq!(unchanged_manual(&repo).await, before);
    let later = scope_until + ChronoDuration::seconds(1);
    let second = claim_at(port, current.seed.id, later).await;
    let mut expired = candidate_at(&fixture.base, later).await;
    expired.selection = TeletextGuideRefreshSelection::Effective(fixture.save.selection.clone());
    assert_eq!(
        port.complete(&second, &expired, later, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::SelectionChanged))
    );
    let retained = port
        .current(1, TeletextGuideRefreshRole::Effective, MAXIMUM)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retained.runtime, Some(runtime));
    assert_eq!(
        retained.status(scope_until),
        TeletextGuideRefreshStatus::Unavailable
    );
    assert_eq!(
        ersatzrs_core::repositories::TeletextGenerationRepository::current(&repo, 1)
            .await
            .unwrap(),
        Some(fixture.base_generation)
    );
}

fn output_confirmation(
    current: &ersatzrs_core::repositories::TeletextGuideRefreshCurrentMetadata,
    fixture: &Fixture,
    now: chrono::DateTime<Utc>,
) -> OutputConfirmation {
    OutputConfirmation {
        seed: current.seed.clone(),
        current_revision: current.revision,
        runtime: current.runtime.clone(),
        selection: TeletextGuideRefreshSelection::Base(fixture.selection.clone()),
        qualification: current.runtime.as_ref().map_or_else(
            || current.seed.qualification.clone(),
            |runtime| runtime.qualification.clone(),
        ),
        status: current.status(now),
        available_until: current.available_until(),
    }
}

async fn check_fresh_output(
    repo: &SqliteTeletextGenerationRepository,
    fixture: &Fixture,
    metadata: &ersatzrs_core::repositories::TeletextGuideRefreshCurrentMetadata,
    now: chrono::DateTime<Utc>,
) {
    let port = repo.guide_refresh().unwrap();
    let expected = output_confirmation(metadata, fixture, now);
    assert_eq!(
        port.confirm_output(&expected, now, &OutputCheck::Fresh(check()), MAXIMUM)
            .await
            .unwrap(),
        M::Accepted(())
    );
    let stale = OutputCheck::Stale {
        deadline: check().deadline,
    };
    assert_eq!(
        port.confirm_output(&expected, now, &stale, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GuideFreshnessRequired)
    );
    let original: String = sqlx::query_scalar("SELECT Name FROM Channel WHERE Id=2")
        .fetch_one(&repo.0)
        .await
        .unwrap();
    sqlx::query("UPDATE Channel SET Name='Changed output source' WHERE Id=2")
        .execute(&repo.0)
        .await
        .unwrap();
    let rejected = port
        .confirm_output(&expected, now, &OutputCheck::Fresh(check()), MAXIMUM)
        .await
        .unwrap();
    sqlx::query("UPDATE Channel SET Name=? WHERE Id=2")
        .bind(original)
        .execute(&repo.0)
        .await
        .unwrap();
    assert_eq!(rejected, M::Rejected(R::Publication(P::GuideChanged)));
    let mut wrong = expected.clone();
    wrong.current_revision += 1;
    assert_eq!(
        port.confirm_output(&wrong, now, &OutputCheck::Fresh(check()), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
}

async fn check_stale_output(
    repo: &SqliteTeletextGenerationRepository,
    fixture: &Fixture,
    now: chrono::DateTime<Utc>,
) {
    let port = repo.guide_refresh().unwrap();
    let metadata = port
        .observe_current(1, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .unwrap();
    let expected = output_confirmation(&metadata, fixture, now);
    assert_eq!(expected.status, TeletextGuideRefreshStatus::Stale);
    sqlx::query("UPDATE Channel SET Name='Changed stale output source' WHERE Id=2")
        .execute(&repo.0)
        .await
        .unwrap();
    let check = OutputCheck::Stale {
        deadline: check().deadline,
    };
    // Explicit stale authority requires no raw-source equality or work admission.
    assert_eq!(
        port.confirm_output(&expected, now, &check, MAXIMUM)
            .await
            .unwrap(),
        M::Accepted(())
    );
    let mut wrong = expected.clone();
    wrong.available_until += ChronoDuration::seconds(1);
    assert_eq!(
        port.confirm_output(&wrong, now, &check, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    let mut wrong = expected.clone();
    wrong.runtime.as_mut().unwrap().artifact_digest = "f".repeat(64);
    assert_eq!(
        port.confirm_output(&wrong, now, &check, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    assert_eq!(
        port.confirm_output(&expected, expected.available_until, &check, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
}

async fn check_new_pending_preserves_old_read(
    repo: &SqliteTeletextGenerationRepository,
    fixture: &Fixture,
    claim: &TeletextGuideRefreshClaim,
    candidate: &TeletextGuideRefreshCandidate,
    now: chrono::DateTime<Utc>,
    status: TeletextGuideRefreshStatus,
) {
    let source = qualification(fixture).await.source;
    let designs = SqliteTeletextDesignRepository(repo.0.clone());
    let TeletextDesignMutation::Saved(head) = designs
        .save(
            &TeletextDesignSave {
                name: "New pending guide concept".into(),
                document_version: 1,
                document: serde_json::to_vec(&serde_json::json!({"guide_source": source})).unwrap(),
                existing: Some((
                    claim.seed.generation.request.design_id,
                    claim.seed.expected_design_head + 1,
                )),
            },
            MAXIMUM,
        )
        .await
        .unwrap()
    else {
        panic!("new concept refused");
    };
    let revision = designs
        .revision(head.id, head.revision, MAXIMUM)
        .await
        .unwrap()
        .unwrap();
    let request = ersatzrs_core::repositories::TeletextPublicationRequestSave {
        design_id: head.id,
        revision: head.revision,
        digest: revision.digest,
        expected_design_head: head.revision,
        expected_request_id: Some(claim.seed.generation.request.id),
        selection: fixture.selection.clone(),
    };
    assert!(matches!(
        fixture
            .repo
            .request_with_source_held(&request, &source, &check(), MAXIMUM)
            .await
            .unwrap(),
        TeletextPublicationMutation::Requested(_)
    ));
    let port = repo.guide_refresh().unwrap();
    assert_eq!(
        port.complete(claim, candidate, now, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    let metadata = port
        .observe_current(1, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .unwrap();
    let expected = output_confirmation(&metadata, fixture, now);
    assert_eq!(expected.status, status);
    let output_check = if status == TeletextGuideRefreshStatus::Fresh {
        OutputCheck::Fresh(check())
    } else {
        OutputCheck::Stale {
            deadline: check().deadline,
        }
    };
    assert_eq!(
        port.confirm_output(&expected, now, &output_check, MAXIMUM)
            .await
            .unwrap(),
        M::Accepted(())
    );
    assert_eq!(
        ersatzrs_core::repositories::TeletextGenerationRepository::current(repo, 1)
            .await
            .unwrap(),
        Some(claim.seed.generation.clone())
    );
}

async fn runtime_ids(repo: &SqliteTeletextGenerationRepository, seed_id: i64) -> Vec<i64> {
    sqlx::query_scalar(
        r#"SELECT "Id" FROM "TeletextGuideRuntimeSnapshot" WHERE "SeedId"=? ORDER BY "Id""#,
    )
    .bind(seed_id)
    .fetch_all(&repo.0)
    .await
    .unwrap()
}

async fn check_runtime_retention(
    repo: &SqliteTeletextGenerationRepository,
    fixture: &Fixture,
    mut snapshot: ersatzrs_core::repositories::TeletextGuideRuntimeSnapshot,
) -> ersatzrs_core::repositories::TeletextGuideRuntimeSnapshot {
    let port = repo.guide_refresh().unwrap();
    let oldest = snapshot.id;
    for _ in 0..2 {
        let current = base_current(port).await;
        let now = current.next_due_at;
        let claim = claim_at(port, current.seed.id, now).await;
        let candidate = candidate_at(fixture, now).await;
        let M::Accepted(next) = port
            .complete(&claim, &candidate, now, &check(), MAXIMUM)
            .await
            .unwrap()
        else {
            panic!("rolling runtime rejected");
        };
        assert_eq!(
            port.complete(&claim, &candidate, now, &check(), MAXIMUM)
                .await
                .unwrap(),
            M::Rejected(R::GenerationConflict)
        );
        let expected = vec![snapshot.id, next.id];
        assert_eq!(runtime_ids(repo, snapshot.seed_id).await, expected);
        snapshot = next;
    }
    let ids = runtime_ids(repo, snapshot.seed_id).await;
    assert_eq!(ids.len(), 2);
    assert!(!ids.contains(&oldest));
    assert!(sqlx::query(
        r#"UPDATE "TeletextGuideRuntimeSnapshot" SET "DocumentVersion"=1 WHERE "Id"=?"#
    )
    .bind(ids[0])
    .execute(&repo.0)
    .await
    .is_err());
    snapshot
}

async fn check_draft_alone_keeps_fresh_until_source_loss() {
    let (fixture, repo, save) = seeded().await;
    let port = repo.guide_refresh().unwrap();
    let initial = base_current(port).await;
    let now = initial.next_due_at;
    let claim = claim_at(port, initial.seed.id, now).await;
    let candidate = candidate_at(&fixture, now).await;
    let designs = SqliteTeletextDesignRepository(repo.0.clone());
    let mut document = designs
        .revision(
            save.original.request.design_id,
            save.original.request.revision,
            MAXIMUM,
        )
        .await
        .unwrap()
        .unwrap()
        .document;
    document.push(b'\n');
    assert!(matches!(
        designs
            .save(
                &TeletextDesignSave {
                    name: "Unpublished guide draft".into(),
                    document_version: 1,
                    document,
                    existing: Some((
                        save.original.request.design_id,
                        save.original.expected_design_head
                    )),
                },
                MAXIMUM
            )
            .await
            .unwrap(),
        TeletextDesignMutation::Saved(_)
    ));
    assert_eq!(
        port.complete(&claim, &candidate, now, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    let review = base_current(port).await;
    assert_eq!(
        review.failure,
        Some(TeletextGuideRefreshFailure::NeedsReview)
    );
    assert_eq!(review.first_failure_at, None);
    assert_eq!(review.status(now), TeletextGuideRefreshStatus::Fresh);
    assert_eq!(review.available_until(), initial.available_until());
    check_new_pending_preserves_old_read(
        &repo,
        &fixture,
        &claim,
        &candidate,
        now,
        TeletextGuideRefreshStatus::Fresh,
    )
    .await;
    check_source_loss_during_review(
        &repo,
        &fixture,
        initial.seed.id,
        now + ChronoDuration::seconds(1),
    )
    .await;
}

async fn check_source_loss_during_review(
    repo: &SqliteTeletextGenerationRepository,
    fixture: &Fixture,
    seed_id: i64,
    loss: chrono::DateTime<Utc>,
) {
    let port = repo.guide_refresh().unwrap();
    sqlx::query("UPDATE Channel SET Name='Source changed during manual review' WHERE Id=2")
        .execute(&repo.0)
        .await
        .unwrap();

    assert_eq!(port.changed(seed_id, loss).await.unwrap(), M::Accepted(()));
    let changed = base_current(port).await;
    assert_eq!(
        changed.failure,
        Some(TeletextGuideRefreshFailure::NeedsReview)
    );
    assert_eq!(changed.first_failure_at, Some(loss));
    assert_eq!(changed.status(loss), TeletextGuideRefreshStatus::Stale);
    assert!(port
        .due(loss + ChronoDuration::minutes(5), 1)
        .await
        .unwrap()
        .is_empty());
    let metadata = port
        .observe_current(1, TeletextGuideRefreshRole::Base, MAXIMUM)
        .await
        .unwrap()
        .unwrap();
    let expected = output_confirmation(&metadata, fixture, loss);
    assert_eq!(
        port.confirm_output(
            &expected,
            loss,
            &OutputCheck::Stale {
                deadline: check().deadline
            },
            MAXIMUM
        )
        .await
        .unwrap(),
        M::Accepted(())
    );
}

#[path = "teletext_guide_refresh_health_tests.rs"]
mod health;
