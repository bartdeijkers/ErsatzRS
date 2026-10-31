//! Transaction/source/expiry/CAS proof; native compilation is the host's gate.

use super::*;
use crate::repositories::{
    teletext_publications::tests::{open, Fixture},
    SqliteTeletextDesignRepository, SqliteTeletextGenerationRepository,
};
use crate::xmltv::{
    capture_guide_snapshot, GuideExpansionLimits, GuideReadBudget, GuideReadLimits,
    GuideSnapshotDigestLimits,
};
use ersatzrs_core::{
    locking::{EntityLocker, TeletextWorkGuard},
    programme::ProgrammeClock,
    repositories::{
        TeletextDesignMutation, TeletextDesignRepository as _, TeletextDesignSave,
        TeletextEffectiveGenerationSave, TeletextGenerationRepository as _, TeletextGenerationSave,
        TeletextGuideSourceCheckLimits, TeletextGuideSourceFence, TeletextPublicationMutation,
        TeletextPublicationRepository as _, TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
    },
};
use std::{num::NonZeroU8, time::Duration};

const MAXIMUM: usize = 16_384;

fn check() -> TeletextGuideGenerationCheck {
    let guard = TeletextWorkGuard::try_acquire(&EntityLocker::new(), 1).unwrap();
    TeletextGuideGenerationCheck {
        limits: TeletextGuideSourceCheckLimits {
            max_records: 1000,
            max_source_bytes: 1_048_576,
            max_model_nodes: 10_000,
            max_model_bytes: 16_777_216,
            max_digest_fields: 100_000,
            max_digest_bytes: 16_777_216,
        },
        deadline: Instant::now() + Duration::from_secs(10),
        hold: guard.hold_work(),
    }
}

async fn qualification(fixture: &Fixture) -> TeletextGuideGenerationQualification {
    qualification_at(fixture, Utc::now()).await
}

async fn qualification_at(
    fixture: &Fixture,
    at: chrono::DateTime<Utc>,
) -> TeletextGuideGenerationQualification {
    let check = check();
    let clock = ProgrammeClock::system().unwrap();
    let (start, finish) = clock
        .calendar_day_bounds(at, NonZeroU8::new(3).unwrap())
        .unwrap();
    let mut budget = GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: check.limits.max_records,
            max_source_bytes: check.limits.max_source_bytes,
        },
        GuideExpansionLimits {
            max_nodes: check.limits.max_model_nodes,
            max_bytes: check.limits.max_model_bytes,
        },
    );
    // A read-only transaction captures the qualification fixture source.
    let mut tx = fixture.repo.0.begin().await.unwrap();
    let snapshot = capture_guide_snapshot(&mut tx, &[2], start, finish, &mut budget)
        .await
        .unwrap();
    let source = TeletextGuideSourceFence {
        version: 1,
        context_digest_version: 2,
        context_digest: snapshot
            .context_digest(GuideSnapshotDigestLimits {
                max_fields: check.limits.max_digest_fields,
                max_bytes: check.limits.max_digest_bytes,
            })
            .unwrap(),
        channel_ids: vec![2],
        captured_at: at,
        start,
        finish,
        locale: "en-us".to_owned(),
        timezone: clock.iana_name().unwrap().to_owned(),
    };
    source
        .qualify_current(
            snapshot
                .channels()
                .iter()
                .flat_map(crate::xmltv::CapturedGuideChannel::occurrences),
            at,
        )
        .unwrap()
}

async fn base_save(fixture: &Fixture) -> TeletextGuideGenerationSave {
    let qualification = qualification(fixture).await;
    let designs = SqliteTeletextDesignRepository(fixture.repo.0.clone());
    let TeletextDesignMutation::Saved(head) = designs
        .save(
            &TeletextDesignSave {
                name: "Source-aware original".into(),
                document_version: 1,
                document: serde_json::to_vec(
                    &serde_json::json!({"guide_source":qualification.source}),
                )
                .unwrap(),
                existing: None,
            },
            MAXIMUM,
        )
        .await
        .unwrap()
    else {
        panic!("source design rejected")
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
        expected_request_id: None,
        selection: fixture.selection.clone(),
    };
    let check = check();
    let TeletextPublicationMutation::Requested(request) = fixture
        .repo
        .request_with_source_held(&request, &qualification.source, &check, MAXIMUM)
        .await
        .unwrap()
    else {
        panic!("source request rejected")
    };
    TeletextGuideGenerationSave {
        original: TeletextGenerationSave {
            request,
            expected_design_head: head.revision,
            expected_generation_id: None,
            selection: fixture.selection.clone(),
            artifact_version: TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
            artifact_digest: "a".repeat(64),
        },
        qualification,
    }
}

fn accepted(value: M<TeletextAcceptedGeneration>) -> TeletextAcceptedGeneration {
    match value {
        M::Accepted(value) => value,
        M::Rejected(reason) => panic!("unexpected {reason:?}"),
    }
}

#[tokio::test]
async fn source_guide_base_accepts_reopens_and_confirms_without_static_authority() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let save = base_save(&fixture).await;
    let mut static_save = save.original.clone();
    static_save.artifact_version = TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION;
    assert_eq!(
        repository.accept(&static_save, MAXIMUM).await.unwrap(),
        M::Rejected(R::GuideFreshnessRequired)
    );
    assert_eq!(
        repository.accept(&save.original, MAXIMUM).await.unwrap(),
        M::Rejected(R::Publication(P::InvalidValues))
    );
    let generation = accepted(
        repository
            .accept_guide(&save, &check(), MAXIMUM)
            .await
            .unwrap(),
    );
    let reopened = SqliteTeletextGenerationRepository(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    assert_eq!(reopened.current(1).await.unwrap(), Some(generation.clone()));
    assert_eq!(
        reopened
            .confirm_current_guide(
                &generation,
                &save.original.selection,
                &save.qualification,
                &check(),
                MAXIMUM
            )
            .await
            .unwrap(),
        M::Accepted(())
    );
    assert_eq!(
        reopened
            .confirm_current(&generation, &save.original.selection, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::InvalidValues))
    );
    assert_eq!(
        repository
            .accept_guide(&save, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::AlreadyAccepted)
    );
    sqlx::query("UPDATE Channel SET Name='Changed source' WHERE Id=2")
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        reopened
            .confirm_current_guide(
                &generation,
                &save.original.selection,
                &save.qualification,
                &check(),
                MAXIMUM
            )
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::GuideChanged))
    );
    assert_eq!(reopened.current(1).await.unwrap(), Some(generation));
}

#[tokio::test]
async fn source_guide_rejects_forged_expiry_source_limits_deadline_and_cas_without_writes() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let save = base_save(&fixture).await;
    let mut forged = save.clone();
    forged.qualification.valid_until =
        save.qualification.source.captured_at + chrono::Duration::nanoseconds(1);
    assert_eq!(
        repository
            .accept_guide(&forged, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GuideFreshnessRequired)
    );
    let mut shortened = save.clone();
    shortened.qualification.valid_until = save.qualification.source.captured_at
        + (save.qualification.valid_until - save.qualification.source.captured_at) / 2;
    assert_eq!(
        repository
            .accept_guide(&shortened, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::GuideFreshnessRequired)
    );
    let mut changed = save.clone();
    changed.qualification.source.context_digest = [0; 32];
    assert_eq!(
        repository
            .accept_guide(&changed, &check(), MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::GuideFenceMismatch))
    );
    let mut constrained = check();
    constrained.limits.max_records = 0;
    assert_eq!(
        repository
            .accept_guide(&save, &constrained, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::GuideUnavailable))
    );
    let mut expired = check();
    expired.deadline = Instant::now();
    assert_eq!(
        repository
            .accept_guide(&save, &expired, MAXIMUM)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::GuideTimedOut))
    );
    for (head, pointer, expected) in [
        (2, None, R::Publication(P::DesignConflict)),
        (1, Some(999), R::GenerationConflict),
    ] {
        let mut stale = save.clone();
        stale.original.expected_design_head = head;
        stale.original.expected_generation_id = pointer;
        assert_eq!(
            repository
                .accept_guide(&stale, &check(), MAXIMUM)
                .await
                .unwrap(),
            M::Rejected(expected)
        );
    }
    assert!(repository.current(1).await.unwrap().is_none());
    assert_eq!(
        fixture.repo.current(1).await.unwrap(),
        Some(save.original.request)
    );
}

#[tokio::test]
async fn source_guide_effective_keeps_base_and_exact_original_scope_expiry() {
    use crate::repositories::teletext_publications::effective::tests as scoped;
    let fixture = scoped::Fixture::new(true).await;
    let repository = SqliteTeletextGenerationRepository(fixture.base.repo.0.clone());
    let mut qualification = qualification(&fixture.base).await;
    qualification.valid_until = qualification
        .valid_until
        .min(fixture.save.selection.valid_until.unwrap());
    let document = serde_json::to_vec(
        &serde_json::json!({"format_version":2,"guide_source":qualification.source}),
    )
    .unwrap();
    let request = scoped::design(&fixture.base, fixture.save.selection.clone(), &document).await;
    let check = check();
    let TeletextPublicationMutation::Requested(request) = fixture
        .base
        .repo
        .request_effective_with_source_held(&request, &qualification.source, &check, MAXIMUM)
        .await
        .unwrap()
    else {
        panic!("scoped source request rejected")
    };
    let save = TeletextEffectiveGuideGenerationSave {
        original: TeletextEffectiveGenerationSave {
            request,
            expected_design_head: 1,
            expected_generation_id: None,
            selection: fixture.save.selection.clone(),
            artifact_version: TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
            artifact_digest: "b".repeat(64),
        },
        qualification,
    };
    let generation = accepted(
        repository
            .accept_effective_guide(&save, &check, MAXIMUM)
            .await
            .unwrap(),
    );
    assert_eq!(
        repository.current_effective(1).await.unwrap(),
        Some(generation.clone())
    );
    assert_eq!(
        repository.current(1).await.unwrap(),
        Some(fixture.base_generation)
    );
    assert_eq!(
        repository
            .confirm_current_effective_guide(
                &generation,
                &save.original.selection,
                &save.qualification,
                &check,
                MAXIMUM
            )
            .await
            .unwrap(),
        M::Accepted(())
    );
    let mut wrong = save.qualification.clone();
    wrong.valid_until =
        wrong.source.captured_at + (wrong.valid_until - wrong.source.captured_at) / 2;
    assert_eq!(
        repository
            .confirm_current_effective_guide(
                &generation,
                &save.original.selection,
                &wrong,
                &check,
                MAXIMUM
            )
            .await
            .unwrap(),
        M::Rejected(R::GuideFreshnessRequired)
    );
    assert_eq!(
        repository
            .accept_effective(&save.original, MAXIMUM, check.deadline)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::InvalidValues))
    );
}

#[path = "teletext_guide_refresh_tests.rs"]
mod refresh;
