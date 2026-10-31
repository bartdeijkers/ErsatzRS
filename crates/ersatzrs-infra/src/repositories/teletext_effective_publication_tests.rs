//! Real bounded SQL scopes and independent role pointers; not native proof.
//! Interval rows are explicit test-only publication fixtures, without Lua replay.

use super::*;
use crate::repositories::{
    teletext_publications::tests::{open, Fixture as BaseFixture},
    SqliteTeletextDesignRepository, SqliteTeletextGenerationRepository,
    SqliteTeletextProfileRepository,
};
use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, TeletextDesignMutation, TeletextDesignRepository as _,
    TeletextDesignSave, TeletextEffectiveGenerationSave, TeletextGenerationMutation as G,
    TeletextGenerationRejection as GR, TeletextGenerationRepository as _, TeletextGenerationSave,
    TeletextProfileRepository as _, TeletextPublicationRejection as P,
    TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION, TELETEXT_GENERATION_ARTIFACT_VERSION,
    TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{
    TeletextAcceptedGeneration, TeletextProfileCandidateSnapshot, TeletextProfileValue,
};
use std::time::Duration;

const FIRST: &str = "org.example.publication.first";
const SECOND: &str = "org.example.publication.second";
const DOCUMENT_BUDGET: usize = 16_384;

#[tokio::test]
async fn effective_artifact_versions_keep_legacy_and_lifecycle_roles_distinct() {
    for version in [
        TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
        TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
    ] {
        let fixture = Fixture::new(true).await;
        let request = fixture.request().await;
        let mut input = fixture.generation_save(request, None);
        input.artifact_version = version;
        for wrong in [
            0,
            TELETEXT_GENERATION_ARTIFACT_VERSION,
            TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
            5,
            u16::MAX,
        ] {
            let mut invalid = input.clone();
            invalid.artifact_version = wrong;
            assert_eq!(
                fixture
                    .generations
                    .accept_effective(&invalid, DOCUMENT_BUDGET, deadline())
                    .await
                    .unwrap(),
                G::Rejected(GR::Publication(P::InvalidValues))
            );
        }
        let generation = accepted(
            fixture
                .generations
                .accept_effective(&input, DOCUMENT_BUDGET, deadline())
                .await
                .unwrap(),
        );
        assert_eq!(generation.artifact_version, version);
        let reopened = SqliteTeletextGenerationRepository(
            open(&fixture.base.root.path().join("requests.sqlite")).await,
        );
        assert_eq!(
            reopened.current_effective(1).await.unwrap(),
            Some(generation.clone())
        );
        assert_eq!(
            reopened
                .confirm_current_effective(
                    &generation,
                    &fixture.save.selection,
                    DOCUMENT_BUDGET,
                    deadline()
                )
                .await
                .unwrap(),
            G::Accepted(())
        );
        for wrong in [
            TELETEXT_GENERATION_ARTIFACT_VERSION,
            TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
            5,
        ] {
            let mut invalid = generation.clone();
            invalid.artifact_version = wrong;
            assert_eq!(
                fixture.confirm(&invalid).await,
                G::Rejected(GR::Publication(P::InvalidValues))
            );
        }
        fixture.assert_base_unchanged().await;
        sqlx::query(r#"UPDATE "TeletextAcceptedGenerationCurrent" SET "GenerationId"=? WHERE "ChannelId"=1"#)
            .bind(generation.id).execute(&fixture.base.repo.0).await.unwrap();
        assert!(fixture.generations.current(1).await.is_err());
    }
}

fn deadline() -> Instant {
    Instant::now().checked_add(Duration::from_secs(10)).unwrap()
}

fn saved<T: std::fmt::Debug>(mutation: TeletextProfileMutation<T>) -> T {
    let TeletextProfileMutation::Saved(value) = mutation else {
        panic!("profile fixture rejected: {mutation:?}")
    };
    value
}

fn accepted<T: std::fmt::Debug>(mutation: G<T>) -> T {
    let G::Accepted(value) = mutation else {
        panic!("generation fixture rejected: {mutation:?}")
    };
    value
}

fn candidate_bytes(candidate: &TeletextProfileCandidateSnapshot) -> String {
    let install = &candidate.installation;
    let settings = candidate
        .effective_settings
        .iter()
        .map(|setting| {
            let TeletextProfileValue::Literal(value) = &setting.value else {
                panic!("literal fixture")
            };
            serde_json::json!({"key":setting.key,"kind":0,"value":value})
        })
        .collect::<Vec<_>>();
    serde_json::json!({"version":1,"channel":candidate.channel_id,"addon":candidate.addon_id,
        "profile":candidate.profile_id,"revision":candidate.profile_revision,
        "contract":candidate.contract.schema_version(),"settings":settings,
        "installation":{"addon":install.addon_id,"revision":install.state_revision,"version":install.version,
            "package":install.package_sha256,"content":install.content_sha256,"manifest":install.manifest_sha256}}).to_string()
}

async fn interval(
    base: &BaseFixture,
    channel: i64,
    addon: &str,
    candidate: Option<&TeletextProfileCandidateSnapshot>,
) {
    sqlx::query(r#"INSERT OR IGNORE INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (?,?,?,0)"#)
        .bind(200 + channel).bind(channel)
        .bind(crate::repositories::common::encode_enum(ersatzrs_domain::PlayoutScheduleKind::Scripted))
        .execute(&base.repo.0).await.unwrap();
    let now = Utc::now();
    sqlx::query(r#"INSERT INTO "PlayoutTeletextInterval" ("PlayoutId","AddonId","Start","Finish","State","CandidateSnapshot") VALUES (?,?,?,?,?,?)"#)
        .bind(200 + channel).bind(addon)
        .bind(crate::repositories::common::encode_datetime(now - chrono::Duration::minutes(1)))
        .bind(crate::repositories::common::encode_datetime(now + chrono::Duration::minutes(10)))
        .bind(i32::from(candidate.is_some())).bind(candidate.map(candidate_bytes))
        .execute(&base.repo.0).await.unwrap();
}

pub(in crate::repositories) async fn design(
    base: &BaseFixture,
    selection: Selection,
    document: &[u8],
) -> TeletextEffectivePublicationRequestSave {
    let mut document: serde_json::Value = serde_json::from_slice(document).unwrap();
    document
        .as_object_mut()
        .unwrap()
        .remove("synthetic_channel");
    document["effective_selection"] = super::super::effective_scope::test_scope(&selection);
    let document = serde_json::to_vec(&document).unwrap();
    let designs = SqliteTeletextDesignRepository(base.repo.0.clone());
    let TeletextDesignMutation::Saved(head) = designs
        .save(
            &TeletextDesignSave {
                name: "Synthetic scoped originals".to_owned(),
                document_version: TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
                document,
                existing: None,
            },
            DOCUMENT_BUDGET,
        )
        .await
        .unwrap()
    else {
        panic!("design fixture rejected")
    };
    let revision = designs
        .revision(head.id, head.revision, DOCUMENT_BUDGET)
        .await
        .unwrap()
        .unwrap();
    TeletextEffectivePublicationRequestSave {
        design_id: head.id,
        revision: head.revision,
        digest: revision.digest,
        expected_design_head: head.revision,
        expected_request_id: None,
        selection,
    }
}

pub(in crate::repositories) struct Fixture {
    pub(in crate::repositories) base: BaseFixture,
    profiles: SqliteTeletextProfileRepository,
    generations: SqliteTeletextGenerationRepository,
    base_request: TeletextPublicationRequest,
    pub(in crate::repositories) base_generation: TeletextAcceptedGeneration,
    pub(in crate::repositories) save: TeletextEffectivePublicationRequestSave,
}

impl Fixture {
    pub(in crate::repositories) async fn new(enabled: bool) -> Self {
        let base = BaseFixture::new().await;
        let profiles = SqliteTeletextProfileRepository(base.repo.0.clone());
        let generations = SqliteTeletextGenerationRepository(base.repo.0.clone());
        let base_request = base.request(None).await;
        let base_generation = accepted(
            generations
                .accept(
                    &TeletextGenerationSave {
                        request: base_request.clone(),
                        expected_design_head: 1,
                        expected_generation_id: None,
                        selection: base.selection.clone(),
                        artifact_version: 1,
                        artifact_digest: "1".repeat(64),
                    },
                    DOCUMENT_BUDGET,
                )
                .await
                .unwrap(),
        );
        let candidate = saved(
            profiles
                .candidate_snapshot(1, FIRST, base.selection.bindings[0].profile_id)
                .await
                .unwrap(),
        );
        interval(&base, 1, FIRST, enabled.then_some(&candidate)).await;
        interval(&base, 1, SECOND, None).await;
        let selection = saved(
            profiles
                .effective_channel_snapshot(1, Utc::now())
                .await
                .unwrap(),
        );
        let save = design(&base, selection, b"{\"format_version\":2}").await;
        Self {
            base,
            profiles,
            generations,
            base_request,
            base_generation,
            save,
        }
    }

    async fn request(&self) -> TeletextPublicationRequest {
        let M::Requested(request) = self
            .base
            .repo
            .request_effective(&self.save, DOCUMENT_BUDGET, deadline())
            .await
            .unwrap()
        else {
            panic!("scoped request rejected")
        };
        request
    }

    fn generation_save(
        &self,
        request: TeletextPublicationRequest,
        current: Option<i64>,
    ) -> TeletextEffectiveGenerationSave {
        TeletextEffectiveGenerationSave {
            request,
            expected_design_head: self.save.expected_design_head,
            expected_generation_id: current,
            selection: self.save.selection.clone(),
            artifact_version: TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
            artifact_digest: "2".repeat(64),
        }
    }

    async fn assert_base_unchanged(&self) {
        assert_eq!(
            self.base.repo.current(1).await.unwrap(),
            Some(self.base_request.clone())
        );
        assert_eq!(
            self.generations.current(1).await.unwrap(),
            Some(self.base_generation.clone())
        );
    }

    async fn confirm(&self, generation: &TeletextAcceptedGeneration) -> G<()> {
        self.generations
            .confirm_current_effective(
                generation,
                &self.save.selection,
                DOCUMENT_BUDGET,
                deadline(),
            )
            .await
            .unwrap()
    }
}

async fn counts(pool: &DbPool) -> (i64, i64) {
    (
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#)
            .fetch_one(pool)
            .await
            .unwrap(),
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#)
            .fetch_one(pool)
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn scoped_on_and_off_use_independent_cas_and_keep_base_history_and_accepted_output() {
    let fixture = Fixture::new(true).await;
    let first = fixture.request().await;
    fixture.assert_base_unchanged().await;
    let first_generation = accepted(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(first.clone(), None),
                DOCUMENT_BUDGET,
                deadline(),
            )
            .await
            .unwrap(),
    );
    assert_eq!(first_generation.artifact_version, 2);
    assert_eq!(fixture.confirm(&first_generation).await, G::Accepted(()));
    let mut next = fixture.save.clone();
    assert_eq!(
        fixture
            .base
            .repo
            .request_effective(&next, DOCUMENT_BUDGET, deadline())
            .await
            .unwrap(),
        M::Rejected(R::RequestConflict)
    );
    next.expected_request_id = Some(first.id);
    let M::Requested(second) = fixture
        .base
        .repo
        .request_effective(&next, DOCUMENT_BUDGET, deadline())
        .await
        .unwrap()
    else {
        panic!("next request rejected")
    };
    assert_eq!(
        fixture.generations.current_effective(1).await.unwrap(),
        Some(first_generation.clone())
    );
    assert_eq!(fixture.confirm(&first_generation).await, G::Accepted(()));
    assert_eq!(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(first, None),
                DOCUMENT_BUDGET,
                deadline()
            )
            .await
            .unwrap(),
        G::Rejected(GR::Publication(R::RequestConflict))
    );
    assert_eq!(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(second.clone(), None),
                DOCUMENT_BUDGET,
                deadline()
            )
            .await
            .unwrap(),
        G::Rejected(GR::GenerationConflict)
    );
    let latest = accepted(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(second.clone(), Some(first_generation.id)),
                DOCUMENT_BUDGET,
                deadline(),
            )
            .await
            .unwrap(),
    );
    assert_eq!(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(second, Some(latest.id)),
                DOCUMENT_BUDGET,
                deadline()
            )
            .await
            .unwrap(),
        G::Rejected(GR::AlreadyAccepted)
    );
    fixture.assert_base_unchanged().await;
}

#[tokio::test]
async fn all_off_metadata_is_valid_over_unhealthy_base_and_reopens_without_replay() {
    let fixture = Fixture::new(false).await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled"=0"#)
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted"=1"#)
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    let request = fixture.request().await;
    let generation = accepted(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(request.clone(), None),
                DOCUMENT_BUDGET,
                deadline(),
            )
            .await
            .unwrap(),
    );
    assert!(fixture
        .save
        .selection
        .bindings
        .iter()
        .all(|binding| !binding.enabled()));
    assert_eq!(
        fixture
            .generations
            .confirm_current_effective(
                &generation,
                &fixture.save.selection,
                DOCUMENT_BUDGET,
                deadline()
            )
            .await
            .unwrap(),
        G::Accepted(())
    );
    fixture.assert_base_unchanged().await;
    fixture.base.repo.0.close().await;
    let pool = open(&fixture.base.root.path().join("requests.sqlite")).await;
    let publication = SqliteTeletextPublicationRepository(pool.clone());
    let generations = SqliteTeletextGenerationRepository(pool.clone());
    assert_eq!(
        publication.current_effective(1).await.unwrap(),
        Some(request)
    );
    assert_eq!(
        generations.current_effective(1).await.unwrap(),
        Some(generation.clone())
    );
    assert_eq!(
        generations
            .confirm_current_effective(
                &generation,
                &fixture.save.selection,
                DOCUMENT_BUDGET,
                deadline()
            )
            .await
            .unwrap(),
        G::Accepted(())
    );
    assert_eq!(counts(&pool).await, (2, 2));
}

#[tokio::test]
async fn output_channels_and_role_pointers_remain_independent() {
    let fixture = Fixture::new(true).await;
    let base = &fixture.base.selection.bindings[0];
    saved(
        fixture
            .profiles
            .save_binding(&ChannelTeletextBindingSave {
                channel_id: 2,
                addon_id: FIRST.to_owned(),
                profile_id: base.profile_id,
                enabled: false,
                expected_installation: base.installation.clone(),
                expected_profile_revision: base.profile_revision,
                expected_binding_revision: None,
            })
            .await
            .unwrap(),
    );
    let candidate = saved(
        fixture
            .profiles
            .candidate_snapshot(2, FIRST, base.profile_id)
            .await
            .unwrap(),
    );
    interval(&fixture.base, 2, FIRST, Some(&candidate)).await;
    let scope = saved(
        fixture
            .profiles
            .effective_channel_snapshot(2, Utc::now())
            .await
            .unwrap(),
    );
    let save = design(
        &fixture.base,
        scope,
        b"{\"format_version\":2,\"synthetic_channel\":2}",
    )
    .await;
    let first = fixture.request().await;
    let M::Requested(second) = fixture
        .base
        .repo
        .request_effective(&save, DOCUMENT_BUDGET, deadline())
        .await
        .unwrap()
    else {
        panic!("channel2 rejected")
    };
    let mut promotion = fixture.generation_save(second.clone(), None);
    promotion.selection = save.selection;
    let generation = accepted(
        fixture
            .generations
            .accept_effective(&promotion, DOCUMENT_BUDGET, deadline())
            .await
            .unwrap(),
    );
    assert_eq!(
        fixture.base.repo.current_effective(1).await.unwrap(),
        Some(first)
    );
    assert_eq!(
        fixture.base.repo.current_effective(2).await.unwrap(),
        Some(second)
    );
    assert!(fixture
        .generations
        .current_effective(1)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        fixture.generations.current_effective(2).await.unwrap(),
        Some(generation)
    );
    fixture.assert_base_unchanged().await;
}

#[tokio::test]
async fn scope_and_profile_drift_or_original_expiry_never_fall_back_to_base_or_renew() {
    for mutation in [
        r#"UPDATE "TeletextProfile" SET "Revision"="Revision"+1 WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "AddonInstallation" SET "Enabled"=0 WHERE "AddonId"='org.example.publication.first'"#,
        r#"DELETE FROM "ChannelTeletextBinding" WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "PlayoutTeletextInterval" SET "Start"='2026-01-01T00:00:00.0000000Z',"Finish"='2026-01-01T00:01:00.0000000Z'"#,
    ] {
        let fixture = Fixture::new(true).await;
        let request = fixture.request().await;
        sqlx::query(mutation)
            .execute(&fixture.base.repo.0)
            .await
            .unwrap();
        let mut next = fixture.save.clone();
        next.expected_request_id = Some(request.id);
        assert!(!matches!(
            fixture
                .base
                .repo
                .request_effective(&next, DOCUMENT_BUDGET, deadline())
                .await,
            Ok(M::Requested(_))
        ));
        assert!(!matches!(
            fixture
                .generations
                .accept_effective(
                    &fixture.generation_save(request, None),
                    DOCUMENT_BUDGET,
                    deadline()
                )
                .await,
            Ok(G::Accepted(_))
        ));
        assert!(fixture
            .generations
            .current_effective(1)
            .await
            .unwrap()
            .is_none());
        assert_eq!(counts(&fixture.base.repo.0).await, (2, 1));
        fixture.assert_base_unchanged().await;
    }
}

#[tokio::test]
async fn original_half_open_expiry_remains_a_fence_for_retained_accepted_metadata() {
    let mut fixture = Fixture::new(false).await;
    let finish = Utc::now() + chrono::Duration::seconds(1);
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish"=?"#)
        .bind(crate::repositories::common::encode_datetime(finish))
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    let selection = saved(
        fixture
            .profiles
            .effective_channel_snapshot(1, Utc::now())
            .await
            .unwrap(),
    );
    fixture.save = design(&fixture.base, selection, b"{\"format_version\":2}").await;
    let original_until = fixture.save.selection.valid_until.unwrap();
    let generation = accepted(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(fixture.request().await, None),
                DOCUMENT_BUDGET,
                deadline(),
            )
            .await
            .unwrap(),
    );
    tokio::time::sleep(
        (original_until - Utc::now()).to_std().unwrap_or_default() + Duration::from_millis(10),
    )
    .await;
    assert_eq!(
        fixture
            .generations
            .confirm_current_effective(
                &generation,
                &fixture.save.selection,
                DOCUMENT_BUDGET,
                deadline()
            )
            .await
            .unwrap(),
        G::Rejected(GR::Publication(R::SelectionChanged))
    );
    assert_eq!(
        fixture.generations.current_effective(1).await.unwrap(),
        Some(generation)
    );
    assert_eq!(fixture.save.selection.valid_until, Some(original_until));
    fixture.assert_base_unchanged().await;
}

#[tokio::test]
async fn admission_deadline_bounds_external_writer_wait_and_never_leaves_partial_history() {
    let fixture = Fixture::new(true).await;
    let request = fixture.request().await;
    let other = open(&fixture.base.root.path().join("requests.sqlite")).await;
    let writer = crate::db::begin_write(&other).await.unwrap();
    let mut next = fixture.save.clone();
    next.expected_request_id = Some(request.id);
    assert_eq!(
        fixture
            .base
            .repo
            .request_effective(
                &next,
                DOCUMENT_BUDGET,
                Instant::now() + Duration::from_millis(20)
            )
            .await
            .unwrap(),
        M::Rejected(R::GuideTimedOut)
    );
    assert_eq!(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(request.clone(), None),
                DOCUMENT_BUDGET,
                Instant::now() + Duration::from_millis(20)
            )
            .await
            .unwrap(),
        G::Rejected(GR::Publication(R::GuideTimedOut))
    );
    writer.rollback().await.unwrap();
    assert_eq!(
        fixture.base.repo.current_effective(1).await.unwrap(),
        Some(request)
    );
    assert!(fixture
        .generations
        .current_effective(1)
        .await
        .unwrap()
        .is_none());
    assert_eq!(counts(&fixture.base.repo.0).await, (2, 1));
    fixture.assert_base_unchanged().await;
}

#[tokio::test]
async fn pointer_write_failures_roll_back_history_and_both_roles_remain_intact() {
    for generation in [false, true] {
        let fixture = Fixture::new(true).await;
        let request = if generation {
            Some(fixture.request().await)
        } else {
            None
        };
        let trigger = if generation {
            r#"CREATE TRIGGER "SyntheticScopedFailure" BEFORE INSERT ON "TeletextEffectiveAcceptedGenerationCurrent" BEGIN SELECT RAISE(ABORT,'synthetic pointer failure'); END"#
        } else {
            r#"CREATE TRIGGER "SyntheticScopedFailure" BEFORE INSERT ON "TeletextEffectivePublicationRequestCurrent" BEGIN SELECT RAISE(ABORT,'synthetic pointer failure'); END"#
        };
        sqlx::query(trigger)
            .execute(&fixture.base.repo.0)
            .await
            .unwrap();
        if let Some(request) = request {
            assert!(fixture
                .generations
                .accept_effective(
                    &fixture.generation_save(request, None),
                    DOCUMENT_BUDGET,
                    deadline()
                )
                .await
                .is_err());
        } else {
            assert!(fixture
                .base
                .repo
                .request_effective(&fixture.save, DOCUMENT_BUDGET, deadline())
                .await
                .is_err());
        }
        assert!(fixture
            .generations
            .current_effective(1)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            counts(&fixture.base.repo.0).await,
            (if generation { 2 } else { 1 }, 1)
        );
        fixture.assert_base_unchanged().await;
    }
}

#[tokio::test]
async fn cross_role_or_corrupt_current_pointers_are_errors_not_absence() {
    let fixture = Fixture::new(true).await;
    let request = fixture.request().await;
    let generation = accepted(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(request.clone(), None),
                DOCUMENT_BUDGET,
                deadline(),
            )
            .await
            .unwrap(),
    );
    sqlx::query(r#"UPDATE "TeletextEffectivePublicationRequestCurrent" SET "RequestId"=?"#)
        .bind(fixture.base_request.id)
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    assert!(fixture.base.repo.current_effective(1).await.is_err());
    sqlx::query(r#"UPDATE "TeletextEffectivePublicationRequestCurrent" SET "RequestId"=?"#)
        .bind(request.id)
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextEffectiveAcceptedGenerationCurrent" SET "GenerationId"=?"#)
        .bind(fixture.base_generation.id)
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    assert!(fixture.generations.current_effective(1).await.is_err());
    sqlx::query(r#"UPDATE "TeletextEffectiveAcceptedGenerationCurrent" SET "GenerationId"=?"#)
        .bind(generation.id)
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextAcceptedGenerationCurrent" SET "GenerationId"=?"#)
        .bind(generation.id)
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    assert!(fixture.generations.current(1).await.is_err());
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    // A non-integer pointer necessarily violates its parent FK. Deliberately
    // simulate offline corruption only in this owned fixture, then restore every
    // fence before reading. Production FK and pointer-read guards stay enabled.
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextEffectiveAcceptedGenerationCurrent" SET "GenerationId"=zeroblob(131072)"#)
        .execute(&fixture.base.repo.0).await.unwrap();
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=OFF")
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&fixture.base.repo.0)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA ignore_check_constraints")
            .fetch_one(&fixture.base.repo.0)
            .await
            .unwrap(),
        0
    );
    assert!(fixture.generations.current_effective(1).await.is_err());
}

async fn guide_fence(
    fixture: &Fixture,
) -> (TeletextGuideSourceFence, TeletextGuideSourceCheckLimits) {
    let clock = ProgrammeClock::system().unwrap();
    let captured_at = Utc::now();
    let (start, finish) = clock
        .calendar_day_bounds(captured_at, NonZeroU8::new(3).unwrap())
        .unwrap();
    let limits = TeletextGuideSourceCheckLimits {
        max_records: 1000,
        max_source_bytes: 1024 * 1024,
        max_model_nodes: 100_000,
        max_model_bytes: 16 * 1024 * 1024,
        max_digest_fields: 100_000,
        max_digest_bytes: 16 * 1024 * 1024,
    };
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
    // Read-only transaction: preparatory guide capture/digest share one snapshot;
    // this transaction performs no publication, configuration or timeline writes.
    let mut tx = fixture.base.repo.0.begin().await.unwrap();
    let snapshot = capture_guide_snapshot(&mut tx, &[1], start, finish, &mut budget)
        .await
        .unwrap();
    let context_digest = snapshot
        .context_digest_for_version(
            2,
            GuideSnapshotDigestLimits {
                max_fields: limits.max_digest_fields,
                max_bytes: limits.max_digest_bytes,
            },
        )
        .unwrap();
    tx.rollback().await.unwrap();
    let fence = TeletextGuideSourceFence {
        version: 1,
        context_digest_version: 2,
        context_digest,
        channel_ids: vec![1],
        captured_at,
        start,
        finish,
        locale: "nl".to_owned(),
        timezone: clock.iana_name().unwrap().to_owned(),
    };
    (fence, limits)
}

#[tokio::test]
async fn source_aware_scoped_pending_recaptures_guide_but_does_not_claim_current_role_qualification(
) {
    let mut fixture = Fixture::new(true).await;
    let (fence, limits) = guide_fence(&fixture).await;
    let document =
        serde_json::to_vec(&serde_json::json!({"format_version":2,"guide_source":fence})).unwrap();
    fixture.save = design(&fixture.base, fixture.save.selection.clone(), &document).await;
    assert_eq!(
        fixture
            .base
            .repo
            .request_effective(&fixture.save, DOCUMENT_BUDGET, deadline())
            .await
            .unwrap(),
        M::Rejected(R::GuideFenceRequired)
    );
    let M::Requested(request) = fixture
        .base
        .repo
        .request_effective_with_source(&fixture.save, &fence, limits, DOCUMENT_BUDGET, deadline())
        .await
        .unwrap()
    else {
        panic!("fresh scoped guide pending rejected")
    };
    assert_eq!(
        fixture
            .generations
            .accept_effective(
                &fixture.generation_save(request.clone(), None),
                DOCUMENT_BUDGET,
                deadline()
            )
            .await
            .unwrap(),
        G::Rejected(GR::GuideFreshnessRequired)
    );
    assert_eq!(
        fixture.base.repo.current_effective(1).await.unwrap(),
        Some(request)
    );
    assert!(fixture
        .generations
        .current_effective(1)
        .await
        .unwrap()
        .is_none());
    fixture.assert_base_unchanged().await;
}

async fn with_base_member() -> Fixture {
    let mut fixture = Fixture::new(true).await;
    sqlx::query(r#"DELETE FROM "PlayoutTeletextInterval" WHERE "AddonId"=?"#)
        .bind(SECOND)
        .execute(&fixture.base.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled"=1,"Revision"="Revision"+1 WHERE "AddonId"=?"#)
        .bind(SECOND).execute(&fixture.base.repo.0).await.unwrap();
    let scope = saved(
        fixture
            .profiles
            .effective_channel_snapshot(1, Utc::now())
            .await
            .unwrap(),
    );
    assert!(matches!(scope.bindings[1], Binding::Base(_)));
    fixture.save = design(&fixture.base, scope, b"{\"format_version\":2}").await;
    fixture
}

async fn change_valid_scope(fixture: &Fixture, mode: u8) {
    match mode {
        0 => {
            sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish"=?"#)
                .bind(crate::repositories::common::encode_datetime(
                    Utc::now() + chrono::Duration::minutes(15),
                ))
                .execute(&fixture.base.repo.0)
                .await
                .unwrap();
        }
        1 => {
            sqlx::query(
                r#"UPDATE "TeletextProfile" SET "Revision"="Revision"+1 WHERE "AddonId"=?"#,
            )
            .bind(FIRST)
            .execute(&fixture.base.repo.0)
            .await
            .unwrap();
            let Binding::Scripted(original) = &fixture.save.selection.bindings[0] else {
                panic!("scripted fixture")
            };
            let ersatzrs_domain::TeletextOverrideState::Enabled(original) =
                &original.interval.state
            else {
                panic!("enabled fixture")
            };
            let fresh = saved(
                fixture
                    .profiles
                    .candidate_snapshot(1, FIRST, original.profile_id)
                    .await
                    .unwrap(),
            );
            sqlx::query(
                r#"UPDATE "PlayoutTeletextInterval" SET "CandidateSnapshot"=? WHERE "AddonId"=?"#,
            )
            .bind(candidate_bytes(&fresh))
            .bind(FIRST)
            .execute(&fixture.base.repo.0)
            .await
            .unwrap();
        }
        _ => {
            sqlx::query(
                r#"UPDATE "ChannelTeletextBinding" SET "Revision"="Revision"+1 WHERE "AddonId"=?"#,
            )
            .bind(SECOND)
            .execute(&fixture.base.repo.0)
            .await
            .unwrap();
        }
    }
}

#[tokio::test]
async fn fresh_valid_finish_profile_or_base_scope_cannot_relabel_immutable_old_originals() {
    for mode in 0..3 {
        let fixture = with_base_member().await;
        let first = fixture.request().await;
        let generation = accepted(
            fixture
                .generations
                .accept_effective(
                    &fixture.generation_save(first.clone(), None),
                    DOCUMENT_BUDGET,
                    deadline(),
                )
                .await
                .unwrap(),
        );
        let mut next = fixture.save.clone();
        next.expected_request_id = Some(first.id);
        let M::Requested(pending) = fixture
            .base
            .repo
            .request_effective(&next, DOCUMENT_BUDGET, deadline())
            .await
            .unwrap()
        else {
            panic!("pending fixture")
        };
        change_valid_scope(&fixture, mode).await;
        let fresh = saved(
            fixture
                .profiles
                .effective_channel_snapshot(1, Utc::now())
                .await
                .unwrap(),
        );
        assert_ne!(fresh, fixture.save.selection);
        next.expected_request_id = Some(pending.id);
        next.selection = fresh.clone();
        assert_eq!(
            fixture
                .base
                .repo
                .request_effective(&next, DOCUMENT_BUDGET, deadline())
                .await
                .unwrap(),
            M::Rejected(R::SelectionChanged)
        );
        let mut save = fixture.generation_save(pending.clone(), Some(generation.id));
        save.selection = fresh.clone();
        assert_eq!(
            fixture
                .generations
                .accept_effective(&save, DOCUMENT_BUDGET, deadline())
                .await
                .unwrap(),
            G::Rejected(GR::Publication(R::SelectionChanged))
        );
        assert_eq!(
            fixture
                .generations
                .confirm_current_effective(&generation, &fresh, DOCUMENT_BUDGET, deadline())
                .await
                .unwrap(),
            G::Rejected(GR::Publication(R::SelectionChanged))
        );
        assert_eq!(
            fixture.base.repo.current_effective(1).await.unwrap(),
            Some(pending)
        );
        assert_eq!(
            fixture.generations.current_effective(1).await.unwrap(),
            Some(generation)
        );
        assert_eq!(counts(&fixture.base.repo.0).await, (3, 2));
        fixture.assert_base_unchanged().await;
    }
}

#[tokio::test]
async fn original_scope_streaming_comparison_rejects_loss_duplicates_and_wrong_shapes() {
    use super::super::effective_scope::{matches, test_scope};
    let fixture = Fixture::new(true).await;
    let scope = &fixture.save.selection;
    let original = serde_json::json!({"format_version":2,"effective_selection":test_scope(scope)});
    assert!(matches(&serde_json::to_vec(&original).unwrap(), scope));
    for mode in 0..12 {
        let mut value = original.clone();
        let context = &mut value["effective_selection"];
        match mode {
            0 => {
                context.as_object_mut().unwrap().remove("valid_until");
            }
            1 => {
                context["valid_until"] = serde_json::Value::Null;
            }
            2 => {
                context["unrecognized"] =
                    serde_json::json!({"oversized": "X".repeat(DOCUMENT_BUDGET)});
            }
            3 => {
                context["bindings"] = serde_json::json!([]);
            }
            4 => {
                context["bindings"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!(null));
            }
            5 => {
                context["bindings"][0]["scripted"]["state"]["candidate"]["installation"]
                    ["package_sha256"] = serde_json::json!("different");
            }
            6 => {
                context["bindings"][0]["scripted"]["state"]["candidate"]
                    .as_object_mut()
                    .unwrap()
                    .remove("effective_settings");
            }
            7 => {
                context["bindings"][1]["scripted"]["state"]["candidate"] = serde_json::json!({});
            }
            8 => {
                context["bindings"] = serde_json::json!({});
            }
            9 => {
                context["bindings"][0]["base"] = serde_json::json!({});
            }
            10 => {
                value.as_object_mut().unwrap().remove("format_version");
            }
            _ => {
                value["format_version"] = serde_json::json!(3);
            }
        }
        assert!(!matches(&serde_json::to_vec(&value).unwrap(), scope));
    }
    let serialized = serde_json::to_string(&original).unwrap();
    let duplicate = serialized.replacen("\"channel_id\":1", "\"channel_id\":1,\"channel_id\":1", 1);
    assert_ne!(duplicate, serialized);
    assert!(!matches(duplicate.as_bytes(), scope));
    let duplicate = format!(
        "{{\"format_version\":2,\"effective_selection\":{},\"effective_selection\":{}}}",
        test_scope(scope),
        test_scope(scope)
    );
    assert!(!matches(duplicate.as_bytes(), scope));
    let offset = chrono::FixedOffset::east_opt(3600).unwrap();
    let mut alias = original.clone();
    alias["effective_selection"]["valid_until"] =
        serde_json::json!(scope.valid_until.unwrap().with_timezone(&offset));
    assert!(matches(&serde_json::to_vec(&alias).unwrap(), scope));
    // A huge unknown value is rejected at its key, without requiring its parse.
    let unknown = format!(
        "{{\"format_version\":2,\"effective_selection\":{},\"unknown\":{{",
        test_scope(scope)
    );
    assert!(!matches(unknown.as_bytes(), scope));
}
