//! Durable association/CAS proof; native compilation is a separate app contract.

use super::super::{
    teletext_publications::tests::{open, Fixture},
    SqliteTeletextDesignRepository, SqliteTeletextProfileRepository,
};
use super::*;
use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, TeletextDesignMutation, TeletextDesignRepository as _,
    TeletextDesignSave, TeletextProfileRepository as _, TeletextPublicationMutation,
    TeletextPublicationRepository as _,
};
use sqlx::Executor as _;

const MAX_DOCUMENT: usize = 4096;

#[tokio::test]
async fn active_scripted_override_fences_base_output_without_replacing_accepted_history() {
    use crate::repositories::common::encode_datetime;
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let generation = accepted(
        repository
            .accept(&save(&fixture).await, MAX_DOCUMENT)
            .await
            .unwrap(),
    );
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, ?, 0), (2, 2, ?, 0)"#,
    )
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::PlayoutScheduleKind::Scripted,
    ))
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::PlayoutScheduleKind::Scripted,
    ))
    .execute(&repository.0)
    .await
    .unwrap();
    let now = chrono::Utc::now();
    let past = encode_datetime(now - chrono::Duration::hours(1));
    let future = encode_datetime(now + chrono::Duration::hours(1));
    let addon = &fixture.selection.bindings[0].addon_id;
    sqlx::query(r#"INSERT INTO "PlayoutTeletextInterval" ("PlayoutId", "AddonId", "Start", "Finish", "State") VALUES (2, ?, ?, ?, 0)"#)
        .bind(addon).bind(&past).bind(&future).execute(&repository.0).await.unwrap();
    assert_eq!(
        repository
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Accepted(())
    );
    sqlx::query(r#"INSERT INTO "PlayoutTeletextInterval" ("PlayoutId", "AddonId", "Start", "Finish", "State") VALUES (1, ?, ?, ?, 0)"#)
        .bind(addon).bind(&past).bind(&future).execute(&repository.0).await.unwrap();
    let reopened = SqliteTeletextGenerationRepository(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    assert_eq!(reopened.current(1).await.unwrap(), Some(generation.clone()));
    assert_eq!(
        reopened
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::SelectionChanged))
    );
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish" = ? WHERE "PlayoutId" = 1"#)
        .bind(encode_datetime(now - chrono::Duration::minutes(1)))
        .execute(&repository.0)
        .await
        .unwrap();
    assert_eq!(
        reopened
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Accepted(())
    );
    sqlx::query(
        r#"UPDATE "PlayoutTeletextInterval" SET "Start" = ?, "Finish" = ? WHERE "PlayoutId" = 1"#,
    )
    .bind(&future)
    .bind(encode_datetime(now + chrono::Duration::hours(2)))
    .execute(&repository.0)
    .await
    .unwrap();
    assert_eq!(
        reopened
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Accepted(())
    );
    assert_eq!(count(&reopened).await, 1);
}

#[tokio::test]
async fn corrupt_active_scripted_snapshot_is_unavailable_until_original_expiry() {
    use crate::repositories::common::encode_datetime;
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let generation = accepted(
        repository
            .accept(&save(&fixture).await, MAX_DOCUMENT)
            .await
            .unwrap(),
    );
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, ?, 0)"#,
    )
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::PlayoutScheduleKind::Scripted,
    ))
    .execute(&repository.0)
    .await
    .unwrap();
    let now = chrono::Utc::now();
    sqlx::query(r#"INSERT INTO "PlayoutTeletextInterval" ("PlayoutId", "AddonId", "Start", "Finish", "State", "CandidateSnapshot") VALUES (1, ?, ?, ?, 1, '{}')"#)
        .bind(&fixture.selection.bindings[0].addon_id)
        .bind(encode_datetime(now - chrono::Duration::hours(1)))
        .bind(encode_datetime(now + chrono::Duration::hours(1)))
        .execute(&repository.0).await.unwrap();
    assert!(repository
        .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
        .await
        .is_err());
    assert_eq!(
        repository.current(1).await.unwrap(),
        Some(generation.clone())
    );
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish" = ?"#)
        .bind(encode_datetime(now - chrono::Duration::minutes(1)))
        .execute(&repository.0)
        .await
        .unwrap();
    assert_eq!(
        repository
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Accepted(())
    );
}

async fn save(fixture: &Fixture) -> TeletextGenerationSave {
    TeletextGenerationSave {
        request: fixture.request(None).await,
        expected_design_head: 1,
        expected_generation_id: None,
        selection: fixture.selection.clone(),
        artifact_version: TELETEXT_GENERATION_ARTIFACT_VERSION,
        artifact_digest: "a".repeat(64),
    }
}

fn accepted(value: M<TeletextAcceptedGeneration>) -> TeletextAcceptedGeneration {
    match value {
        M::Accepted(value) => value,
        M::Rejected(reason) => panic!("unexpected generation rejection: {reason:?}"),
    }
}

async fn count(repository: &SqliteTeletextGenerationRepository) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#)
        .fetch_one(&repository.0)
        .await
        .unwrap()
}

#[tokio::test]
async fn pending_is_not_accepted_and_selected_identity_survives_reopen_without_activation() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let save = save(&fixture).await;
    assert!(repository.current(1).await.unwrap().is_none());
    let generation = accepted(repository.accept(&save, MAX_DOCUMENT).await.unwrap());
    assert_eq!(generation.request, save.request);
    assert_eq!(generation.artifact_digest, save.artifact_digest);
    assert_eq!(
        fixture.repo.current(1).await.unwrap(),
        Some(save.request.clone())
    );
    let reopened = SqliteTeletextGenerationRepository(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    assert_eq!(reopened.current(1).await.unwrap(), Some(generation.clone()));
    assert_eq!(
        reopened
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Accepted(())
    );
    let revision = SqliteTeletextDesignRepository(reopened.0.clone())
        .revision(fixture.design_id, 1, MAX_DOCUMENT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision.document, b"synthetic immutable originals");
    let debug = format!("{save:?} {generation:?}");
    assert!(!debug.contains(&fixture.digest));
    assert!(!debug.contains(&save.artifact_digest));
    assert_eq!(count(&reopened).await, 1);
}

#[tokio::test]
async fn independent_connections_accept_once_and_request_replay_cannot_duplicate_history() {
    let fixture = Fixture::new().await;
    let first = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let second = SqliteTeletextGenerationRepository(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    let save = save(&fixture).await;
    let (a, b) = tokio::join!(
        first.accept(&save, MAX_DOCUMENT),
        second.accept(&save, MAX_DOCUMENT)
    );
    let outcomes = [a.unwrap(), b.unwrap()];
    assert_eq!(
        outcomes
            .iter()
            .filter(|value| matches!(value, M::Accepted(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|value| **value == M::Rejected(R::AlreadyAccepted))
            .count(),
        1
    );
    assert_eq!(count(&first).await, 1);
    assert_eq!(
        first.current(1).await.unwrap(),
        second.current(1).await.unwrap()
    );
    assert_eq!(
        first.accept(&save, MAX_DOCUMENT).await.unwrap(),
        M::Rejected(R::AlreadyAccepted)
    );
}

#[tokio::test]
async fn generation_cas_and_current_pending_fences_preserve_previous_selection() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let original_save = save(&fixture).await;
    let original = accepted(
        repository
            .accept(&original_save, MAX_DOCUMENT)
            .await
            .unwrap(),
    );
    let mut next = original_save.clone();
    next.request = fixture.request(Some(original.request.id)).await;
    next.artifact_digest = "b".repeat(64);
    assert_eq!(
        repository
            .accept(&original_save, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Rejected(R::Publication(P::RequestConflict))
    );
    assert_eq!(
        repository.accept(&next, MAX_DOCUMENT).await.unwrap(),
        M::Rejected(R::GenerationConflict)
    );
    assert_eq!(repository.current(1).await.unwrap(), Some(original.clone()));
    assert_eq!(count(&repository).await, 1);
    next.expected_generation_id = Some(original.id);
    let selected = accepted(repository.accept(&next, MAX_DOCUMENT).await.unwrap());
    assert_ne!(original.id, selected.id);
    assert_eq!(selected.request, next.request);
    assert_eq!(count(&repository).await, 2);
    assert_eq!(
        repository
            .confirm_current(&original, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Rejected(R::GenerationConflict)
    );
}

#[tokio::test]
async fn new_drafts_pending_requests_and_tombstones_do_not_relabel_accepted_revisions() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let first_save = save(&fixture).await;
    let generation = accepted(repository.accept(&first_save, MAX_DOCUMENT).await.unwrap());
    let designs = SqliteTeletextDesignRepository(fixture.repo.0.clone());
    assert!(matches!(
        designs
            .save(
                &TeletextDesignSave {
                    name: "New draft".into(),
                    document_version: 1,
                    document: b"new unpublished originals".to_vec(),
                    existing: Some((fixture.design_id, 1)),
                },
                MAX_DOCUMENT
            )
            .await
            .unwrap(),
        TeletextDesignMutation::Saved(_)
    ));
    let mut pending = fixture.save(Some(generation.request.id));
    pending.expected_design_head = 2;
    let request = match fixture.repo.request(&pending, MAX_DOCUMENT).await.unwrap() {
        TeletextPublicationMutation::Requested(request) => request,
        other @ TeletextPublicationMutation::Rejected(_) => {
            panic!("valid original revision request rejected: {other:?}");
        }
    };
    assert_ne!(request.id, generation.request.id);
    assert_eq!(
        repository
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Accepted(())
    );
    designs.tombstone(fixture.design_id, 2).await.unwrap();
    assert_eq!(
        repository
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .unwrap(),
        M::Accepted(())
    );
    let next = TeletextGenerationSave {
        request,
        expected_design_head: 2,
        expected_generation_id: Some(generation.id),
        ..first_save
    };
    assert_eq!(
        repository.accept(&next, MAX_DOCUMENT).await.unwrap(),
        M::Rejected(R::Publication(P::DeletedDesign))
    );
    assert_eq!(repository.current(1).await.unwrap(), Some(generation));
}

#[tokio::test]
async fn whole_selection_and_installation_changes_reject_confirmation_and_promotion() {
    for sql in [
        r#"UPDATE "AddonSetting" SET "Value"='Changed' WHERE "Key"='ZONE'"#,
        r#"UPDATE "AddonInstallation" SET "Enabled"=0"#,
        r#"UPDATE "ChannelTeletextBinding" SET "Enabled"=0 WHERE "ChannelId"=1"#,
        r#"DELETE FROM "Channel" WHERE "Id"=1"#,
    ] {
        let fixture = Fixture::new().await;
        let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
        let first_save = save(&fixture).await;
        let generation = accepted(repository.accept(&first_save, MAX_DOCUMENT).await.unwrap());
        let next_request = fixture.request(Some(generation.request.id)).await;
        fixture.repo.0.execute(sql).await.unwrap();
        assert!(matches!(
            repository
                .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
                .await
                .unwrap(),
            M::Rejected(R::Publication(
                P::SelectionChanged | P::SelectionUnavailable(_)
            ))
        ));
        let next = TeletextGenerationSave {
            request: next_request,
            expected_generation_id: Some(generation.id),
            ..first_save
        };
        assert!(matches!(
            repository.accept(&next, MAX_DOCUMENT).await.unwrap(),
            M::Rejected(R::Publication(
                P::SelectionChanged | P::SelectionUnavailable(_)
            ))
        ));
        assert_eq!(repository.current(1).await.unwrap(), Some(generation));
        assert_eq!(count(&repository).await, 1);
    }
}

#[tokio::test]
async fn channel_identity_and_composite_foreign_keys_prevent_cross_channel_reuse() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let first_save = save(&fixture).await;
    let first = accepted(repository.accept(&first_save, MAX_DOCUMENT).await.unwrap());
    let binding = &fixture.selection.bindings[0];
    let profiles = SqliteTeletextProfileRepository(fixture.repo.0.clone());
    profiles
        .save_binding(&ChannelTeletextBindingSave {
            channel_id: 2,
            addon_id: binding.addon_id.clone(),
            profile_id: binding.profile_id,
            enabled: true,
            expected_installation: binding.installation.clone(),
            expected_profile_revision: binding.profile_revision,
            expected_binding_revision: None,
        })
        .await
        .unwrap();
    let TeletextProfileMutation::Saved(selection) =
        profiles.channel_invocation_snapshot(2).await.unwrap()
    else {
        panic!("valid second channel selection");
    };
    let mut request_save = fixture.save(None);
    request_save.selection = selection.clone();
    let TeletextPublicationMutation::Requested(request) = fixture
        .repo
        .request(&request_save, MAX_DOCUMENT)
        .await
        .unwrap()
    else {
        panic!("valid independent channel request");
    };
    let second = accepted(
        repository
            .accept(
                &TeletextGenerationSave {
                    request,
                    selection,
                    ..first_save
                },
                MAX_DOCUMENT,
            )
            .await
            .unwrap(),
    );
    assert_ne!(first.id, second.id);
    assert_eq!(repository.current(1).await.unwrap(), Some(first.clone()));
    assert_eq!(repository.current(2).await.unwrap(), Some(second));
    assert!(sqlx::query(
        r#"UPDATE "TeletextAcceptedGenerationCurrent" SET "GenerationId"=? WHERE "ChannelId"=2"#
    )
    .bind(first.id)
    .execute(&fixture.repo.0)
    .await
    .is_err());
}

#[tokio::test]
async fn validation_bounds_and_failed_pointer_write_roll_back_without_orphan_generation() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let valid = save(&fixture).await;
    for altered in [
        TeletextGenerationSave {
            artifact_version: 0,
            ..valid.clone()
        },
        TeletextGenerationSave {
            artifact_digest: "A".repeat(64),
            ..valid.clone()
        },
        TeletextGenerationSave {
            artifact_digest: "a".repeat(65),
            ..valid.clone()
        },
        TeletextGenerationSave {
            expected_generation_id: Some(0),
            ..valid.clone()
        },
    ] {
        assert_eq!(
            repository.accept(&altered, MAX_DOCUMENT).await.unwrap(),
            M::Rejected(R::Publication(P::InvalidValues))
        );
    }
    assert!(repository.accept(&valid, 0).await.is_err());
    let stale = TeletextGenerationSave {
        expected_design_head: 2,
        ..valid.clone()
    };
    assert_eq!(
        repository.accept(&stale, MAX_DOCUMENT).await.unwrap(),
        M::Rejected(R::Publication(P::DesignConflict))
    );
    fixture.repo.0.execute(r#"CREATE TRIGGER "FailGenerationPointer" BEFORE INSERT ON "TeletextAcceptedGenerationCurrent" BEGIN SELECT RAISE(ABORT,'synthetic pointer failure'); END"#).await.unwrap();
    assert!(repository.accept(&valid, MAX_DOCUMENT).await.is_err());
    assert_eq!(count(&repository).await, 0);
    assert!(repository.current(1).await.unwrap().is_none());
    fixture
        .repo
        .0
        .execute(r#"DROP TRIGGER "FailGenerationPointer""#)
        .await
        .unwrap();
    let generation = accepted(repository.accept(&valid, MAX_DOCUMENT).await.unwrap());
    assert_eq!(count(&repository).await, 1);
    assert_eq!(
        repository.current(1).await.unwrap(),
        Some(generation.clone())
    );
    let next = TeletextGenerationSave {
        request: fixture.request(Some(generation.request.id)).await,
        expected_generation_id: Some(generation.id),
        ..valid
    };
    fixture.repo.0.execute(r#"CREATE TRIGGER "FailGenerationReplacement" BEFORE UPDATE ON "TeletextAcceptedGenerationCurrent" BEGIN SELECT RAISE(ABORT,'synthetic pointer failure'); END"#).await.unwrap();
    assert!(repository.accept(&next, MAX_DOCUMENT).await.is_err());
    assert_eq!(count(&repository).await, 1);
    assert_eq!(repository.current(1).await.unwrap(), Some(generation));
}

#[tokio::test]
async fn immutable_history_and_bounded_corrupt_metadata_never_fall_back_to_pending() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let save = save(&fixture).await;
    let generation = accepted(repository.accept(&save, MAX_DOCUMENT).await.unwrap());
    assert!(fixture
        .repo
        .0
        .execute(r#"UPDATE "TeletextAcceptedGeneration" SET "Id"="Id""#)
        .await
        .is_err());
    assert!(fixture
        .repo
        .0
        .execute(r#"DELETE FROM "TeletextAcceptedGeneration""#)
        .await
        .is_err());
    fixture
        .repo
        .0
        .execute(r#"DROP TRIGGER "TR_TeletextAcceptedGeneration_ImmutableUpdate""#)
        .await
        .unwrap();
    fixture
        .repo
        .0
        .execute("PRAGMA ignore_check_constraints=ON")
        .await
        .unwrap();
    for sql in [
        r#"UPDATE "TeletextAcceptedGeneration" SET "ArtifactDigest"=zeroblob(131072)"#,
        r#"UPDATE "TeletextAcceptedGeneration" SET "ArtifactVersion"=zeroblob(131072)"#,
    ] {
        fixture.repo.0.execute(sql).await.unwrap();
        assert!(repository.current(1).await.is_err());
        assert!(repository
            .confirm_current(&generation, &fixture.selection, MAX_DOCUMENT)
            .await
            .is_err());
        sqlx::query(
            r#"UPDATE "TeletextAcceptedGeneration" SET "ArtifactVersion"=1,"ArtifactDigest"=?"#,
        )
        .bind(&save.artifact_digest)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    }
    fixture
        .repo
        .0
        .execute("PRAGMA foreign_keys=OFF")
        .await
        .unwrap();
    fixture
        .repo
        .0
        .execute(r#"UPDATE "TeletextAcceptedGenerationCurrent" SET "GenerationId"=9999"#)
        .await
        .unwrap();
    assert!(repository.current(1).await.is_err());
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(save.request));
}

#[tokio::test]
async fn historical_guide_source_receipt_cannot_supply_static_generation_authority() {
    let fixture = Fixture::new().await;
    let repository = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let document = serde_json::to_vec(&serde_json::json!({"guide_source": {
        "version": 1, "context_digest_version": 1, "context_digest": vec![0_u8; 32],
        "channel_ids": [], "captured_at": "2026-04-15T10:00:00Z",
        "start": "2026-04-15T00:00:00Z", "finish": "2026-04-18T00:00:00Z",
        "locale": "nl", "timezone": "UTC"
    }}))
    .unwrap();
    let designs = SqliteTeletextDesignRepository(fixture.repo.0.clone());
    assert!(matches!(
        designs
            .save(
                &TeletextDesignSave {
                    name: "Historical source receipt".into(),
                    document_version: 1,
                    document,
                    existing: Some((fixture.design_id, 1)),
                },
                MAX_DOCUMENT
            )
            .await
            .unwrap(),
        TeletextDesignMutation::Saved(_)
    ));
    let revision = designs
        .revision(fixture.design_id, 2, MAX_DOCUMENT)
        .await
        .unwrap()
        .unwrap();
    // Seed only pending metadata to exercise the defensive accepted-store fence;
    // this bypasses no output gate and supplies no native source-role authority.
    let result = sqlx::query(r#"INSERT INTO "TeletextPublicationRequest" ("ChannelId","DesignId","Revision","Digest") VALUES (1,?,2,?)"#)
        .bind(fixture.design_id).bind(&revision.digest).execute(&fixture.repo.0).await.unwrap();
    let request_id = crate::db::last_insert_id(result);
    sqlx::query(
        r#"INSERT INTO "TeletextPublicationRequestCurrent" ("ChannelId","RequestId") VALUES (1,?)"#,
    )
    .bind(request_id)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    let request = fixture.repo.current(1).await.unwrap().unwrap();
    let save = TeletextGenerationSave {
        request: request.clone(),
        expected_design_head: 2,
        expected_generation_id: None,
        selection: fixture.selection.clone(),
        artifact_version: 1,
        artifact_digest: "a".repeat(64),
    };
    assert_eq!(
        repository.accept(&save, MAX_DOCUMENT).await.unwrap(),
        M::Rejected(R::GuideFreshnessRequired)
    );
    assert_eq!(count(&repository).await, 0);
    assert!(repository.current(1).await.unwrap().is_none());
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(request));
}
