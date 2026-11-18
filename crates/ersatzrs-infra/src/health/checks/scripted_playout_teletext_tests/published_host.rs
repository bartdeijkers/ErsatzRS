//! Actual SQL publication/source acceptance for Health association only. Opaque
//! originals and artifact pins are fixtures, never native page or source-output proof.
use super::*;
use crate::xmltv::{
    capture_guide_snapshot, GuideExpansionLimits, GuideReadBudget, GuideReadLimits,
    GuideSnapshotDigestLimits,
};
use ersatzrs_core::{
    locking::{EntityLocker, TeletextWorkGuard},
    programme::ProgrammeClock,
    repositories::{
        TeletextEffectiveGuideGenerationSave, TeletextGuideGenerationCheck,
        TeletextGuideGenerationQualification, TeletextGuideSourceCheckLimits,
        TeletextGuideSourceFence,
    },
};
use serde_json::{json, Value};
use std::num::NonZeroU8;

async fn link(pool: &Pool<Db>) {
    let id: i64 = sqlx::query_scalar(r#"INSERT INTO "TeletextElement"
        ("Name","SourceFormat","SourceFileName","Document","PageNumber","SubpageCount","CycleSeconds","Bindings","Revision","UpdatedAt")
        VALUES ('Unpublished invalid source',1,'fixture.tti',X'41',336,1,10,'[]',1,'2026-11-15T00:00:00Z') RETURNING "Id""#)
        .fetch_one(pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "ChannelTeletextElement" ("ChannelId","TeletextElementId","IsEnabled") VALUES (1,?,1)"#)
        .bind(id).execute(pool).await.unwrap();
}

fn installation(token: &ersatzrs_domain::TeletextInstallationSnapshot) -> Value {
    json!({"addon_id":token.addon_id,"state_revision":token.state_revision,
        "version":token.version,"package_sha256":token.package_sha256,
        "content_sha256":token.content_sha256,"manifest_sha256":token.manifest_sha256})
}

fn original_scope(selection: &TeletextEffectiveChannelSelectionSnapshot) -> Value {
    let bindings: Vec<_> = selection.bindings.iter().map(|binding| match binding {
        TeletextEffectiveBindingSnapshot::Base(base) => json!({"origin":"base","scripted":null,
            "base":{"contract":base.contract.schema_version(),"channel_id":base.channel_id,
                "addon_id":base.addon_id,"binding_revision":base.binding_revision,
                "profile_id":base.profile_id,"profile_revision":base.profile_revision,
                "installation":installation(&base.installation),"effective_settings":[]}}),
        TeletextEffectiveBindingSnapshot::Scripted(published) => {
            let state = &published.interval.state;
            let candidate = match state {
                TeletextOverrideState::Off { .. } => Value::Null,
                TeletextOverrideState::Enabled(candidate) => json!({
                    "contract":candidate.contract.schema_version(),"channel_id":candidate.channel_id,
                    "addon_id":candidate.addon_id,"profile_id":candidate.profile_id,
                    "profile_revision":candidate.profile_revision,"effective_settings":[],
                    "installation":installation(&candidate.installation)}),
            };
            json!({"origin":"scripted","base":null,"scripted":{
                "playout_id":published.playout_id,"start":published.interval.start,
                "finish":published.interval.finish,"state":{
                    "kind":if binding.enabled() {"enabled"} else {"off"},
                    "channel_id":state.channel_id(),"addon_id":state.addon_id(),"candidate":candidate}}})
        }
    }).collect();
    json!({"format_version":TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
        "effective_selection":{"channel_id":selection.channel_id,
            "valid_until":selection.valid_until,"bindings":bindings}})
}

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

async fn qualification(pool: &Pool<Db>, at: DateTime<Utc>) -> TeletextGuideGenerationQualification {
    let clock = ProgrammeClock::system().unwrap();
    let (start, finish) = clock
        .calendar_day_bounds(at, NonZeroU8::new(3).unwrap())
        .unwrap();
    let limits = check().limits;
    let mut budget = GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: limits.max_records,
            max_source_bytes: limits.max_source_bytes,
        },
        GuideExpansionLimits {
            max_nodes: limits.max_model_nodes,
            max_bytes: limits.max_model_bytes,
        },
    );
    // Read-only transaction: capture fixture source qualification without mutation.
    let mut tx = pool.begin().await.unwrap();
    let snapshot = capture_guide_snapshot(&mut tx, &[2], start, finish, &mut budget)
        .await
        .unwrap();
    let source = TeletextGuideSourceFence {
        version: 1,
        context_digest_version: 2,
        context_digest: snapshot
            .context_digest(GuideSnapshotDigestLimits {
                max_fields: limits.max_digest_fields,
                max_bytes: limits.max_digest_bytes,
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

async fn request(
    pool: &Pool<Db>,
    selection: &TeletextEffectiveChannelSelectionSnapshot,
    qualification: Option<&TeletextGuideGenerationQualification>,
) -> ersatzrs_domain::TeletextPublicationRequest {
    let mut document = original_scope(selection);
    if let Some(qualification) = qualification {
        document["guide_source"] = json!(qualification.source);
    }
    let designs = SqliteTeletextDesignRepository(pool.clone());
    let TeletextDesignMutation::Saved(head) = designs
        .save(
            &TeletextDesignSave {
                name: "Scoped Health fixture".to_owned(),
                document_version: TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
                document: serde_json::to_vec(&document).unwrap(),
                existing: None,
            },
            4096,
        )
        .await
        .unwrap()
    else {
        panic!("design rejected")
    };
    let revision = designs
        .revision(head.id, head.revision, 4096)
        .await
        .unwrap()
        .unwrap();
    let target = TeletextEffectivePublicationRequestSave {
        design_id: head.id,
        revision: head.revision,
        digest: revision.digest,
        expected_design_head: head.revision,
        expected_request_id: None,
        selection: selection.clone(),
    };
    let repo = SqliteTeletextPublicationRepository(pool.clone());
    let result = match qualification {
        Some(qualification) => repo
            .request_effective_with_source_held(&target, &qualification.source, &check(), 4096)
            .await
            .unwrap(),
        None => repo
            .request_effective(&target, 4096, check().deadline)
            .await
            .unwrap(),
    };
    let TeletextPublicationMutation::Requested(request) = result else {
        panic!("request rejected")
    };
    request
}

pub(super) async fn accepted_variant(
    published_host: bool,
    on: bool,
    role: u16,
) -> (
    Pool<Db>,
    DateTime<Utc>,
    DateTime<Utc>,
    TeletextAcceptedGeneration,
) {
    let (pool, candidate) = setup().await;
    let now = Utc::now();
    let finish = now + ersatzrs_time::Duration::hours(1);
    insert(
        &pool,
        now - ersatzrs_time::Duration::hours(1),
        finish,
        on.then(|| snapshot(&candidate)),
    )
    .await;
    if published_host {
        link(&pool).await;
    }
    set_policy(&pool, &policy(4096).to_string()).await;
    let selection = saved(
        SqliteTeletextProfileRepository(pool.clone())
            .effective_channel_snapshot(1, now)
            .await
            .unwrap(),
    );
    let mut qualification = if role == 6 {
        Some(qualification(&pool, now).await)
    } else {
        None
    };
    if let Some(qualification) = &mut qualification {
        qualification.valid_until = qualification
            .valid_until
            .min(selection.valid_until.unwrap());
    }
    let request = request(&pool, &selection, qualification.as_ref()).await;
    let original = TeletextEffectiveGenerationSave {
        request,
        expected_design_head: 1,
        expected_generation_id: None,
        selection,
        artifact_version: role,
        artifact_digest: "a".repeat(64),
    };
    let repo = SqliteTeletextGenerationRepository(pool.clone());
    let result = match qualification {
        Some(qualification) => repo
            .accept_effective_guide(
                &TeletextEffectiveGuideGenerationSave {
                    original,
                    qualification,
                },
                &check(),
                4096,
            )
            .await
            .unwrap(),
        None => repo
            .accept_effective(&original, 4096, check().deadline)
            .await
            .unwrap(),
    };
    let TeletextGenerationMutation::Accepted(generation) = result else {
        panic!("generation rejected: {result:?}")
    };
    (pool, now, finish, generation)
}

#[tokio::test]
async fn effective_health_keeps_original_host_presence_and_absence_for_static_and_guide_roles() {
    for role in [2, 4, 6] {
        for published_host in [false, true] {
            let (pool, now, _, generation) = accepted_variant(published_host, true, role).await;
            let original = saved(
                SqliteTeletextProfileRepository(pool.clone())
                    .effective_channel_snapshot(1, now)
                    .await
                    .unwrap(),
            );
            if published_host {
                sqlx::query(r#"DELETE FROM "ChannelTeletextElement" WHERE "ChannelId"=1"#)
                    .execute(&pool)
                    .await
                    .unwrap();
            } else {
                link(&pool).await;
            }
            for version in [1, 2, 3, 4] {
                let mut runtime = policy(4096);
                runtime["version"] = json!(version);
                if version >= 3 {
                    runtime["guide_limits"] = json!({});
                }
                set_policy(&pool, &runtime.to_string()).await;
                let before = changes(&pool).await;
                let result = ScriptedPlayoutStateHealthCheck::new(pool.clone())
                    .check_at(now)
                    .await
                    .unwrap();
                assert_eq!(
                    result.status,
                    HealthCheckStatus::Pass,
                    "role={role},host={published_host},policy={version}: {}",
                    result.message
                );
                assert_eq!(changes(&pool).await, before);
            }
            let repo = SqliteTeletextGenerationRepository(pool);
            assert_eq!(
                repo.current_effective(1).await.unwrap(),
                Some(generation.clone())
            );
            if role == 6 {
                // A Health pass grants no fresh-source output authority. The generic
                // static confirmation keeps its existing guide-role rejection.
                assert_eq!(repo.confirm_current_effective(&generation,&original,4096,check().deadline).await.unwrap(),
                    TeletextGenerationMutation::Rejected(ersatzrs_core::repositories::TeletextGenerationRejection::Publication(ersatzrs_core::repositories::TeletextPublicationRejection::InvalidValues)));
            }
        }
    }
}

#[tokio::test]
async fn effective_health_does_not_hide_corrupt_published_host_pages_behind_all_off() {
    let (pool, now, _, _) = accepted_variant(true, false, 4).await;
    sqlx::query(r#"DELETE FROM "ChannelTeletextElement" WHERE "ChannelId"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    let check = ScriptedPlayoutStateHealthCheck::new(pool.clone());
    assert_eq!(
        check.check_at(now).await.unwrap().status,
        HealthCheckStatus::Pass
    );
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "Document"=X'41'"#)
        .execute(&pool)
        .await
        .unwrap();
    let before = changes(&pool).await;
    assert_eq!(
        check.check_at(now).await.unwrap().status,
        HealthCheckStatus::Fail
    );
    assert_eq!(changes(&pool).await, before);
}

#[tokio::test]
async fn effective_health_retains_original_scope_budget_and_deadline_controls_after_link_edits() {
    let (pool, now, finish, _) = accepted_variant(true, true, 4).await;
    sqlx::query(r#"DELETE FROM "ChannelTeletextElement" WHERE "ChannelId"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    let check = ScriptedPlayoutStateHealthCheck::new(pool.clone());
    assert_eq!(
        check.check_at(now).await.unwrap().status,
        HealthCheckStatus::Pass
    );
    assert!(check
        .teletext_channel_failure(1, now, Instant::now())
        .await
        .is_some());
    set_policy(&pool, &policy(1).to_string()).await;
    assert_eq!(
        check.check_at(now).await.unwrap().status,
        HealthCheckStatus::Fail
    );
    set_policy(&pool, &policy(4096).to_string()).await;
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish"=?"#)
        .bind(crate::repositories::common::encode_datetime(
            finish + ersatzrs_time::Duration::hours(1),
        ))
        .execute(&pool)
        .await
        .unwrap();
    let before = changes(&pool).await;
    assert_eq!(
        check.check_at(now).await.unwrap().status,
        HealthCheckStatus::Fail
    );
    assert_eq!(changes(&pool).await, before);
}
