//! Exact read-only origin fences, not invocation or native generation acceptance.

use super::*;
use ersatzrs_domain::{
    TeletextEffectiveBindingSnapshot as Binding, TeletextEffectiveChannelSelectionSnapshot,
    MAX_TELETEXT_OVERRIDE_COMMANDS,
};
use ersatzrs_time::{DateTime, Duration, TimeZone as _, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};

fn at(minute: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + Duration::minutes(minute)
}

async fn changes(fixture: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(&fixture.repo.0)
        .await
        .unwrap()
}

async fn playout(fixture: &Fixture, id: i64, channel: i64) {
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (?, ?, ?, 1)"#,
    )
    .bind(id)
    .bind(channel)
    .bind(crate::repositories::common::encode_enum(
        ersatzrs_domain::PlayoutScheduleKind::Scripted,
    ))
    .execute(&fixture.repo.0)
    .await
    .unwrap();
}

async fn setup() -> (Fixture, TeletextProfileCandidateSnapshot) {
    let fixture = Fixture::new().await;
    let candidate = populate(&fixture).await;
    (fixture, candidate)
}

async fn populate(fixture: &Fixture) -> TeletextProfileCandidateSnapshot {
    let base = fixture.profile("Base", "day").await;
    let target = fixture.profile("Target", "night").await;
    fixture.bind(1, &base).await;
    fixture.bind(2, &base).await;
    playout(fixture, 1, 1).await;
    saved(
        fixture
            .repo
            .candidate_snapshot(1, ADDON, target.id)
            .await
            .unwrap(),
    )
}

fn encode(candidate: &TeletextProfileCandidateSnapshot) -> String {
    let token = &candidate.installation;
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
        "installation":{"addon":token.addon_id,"revision":token.state_revision,
            "version":token.version,"package":token.package_sha256,
            "content":token.content_sha256,"manifest":token.manifest_sha256}})
    .to_string()
}

async fn interval(
    fixture: &Fixture,
    owner: i64,
    addon: &str,
    start: i64,
    end: i64,
    candidate: Option<&TeletextProfileCandidateSnapshot>,
) {
    sqlx::query(r#"INSERT INTO "PlayoutTeletextInterval" ("PlayoutId", "AddonId", "Start", "Finish", "State", "CandidateSnapshot") VALUES (?, ?, ?, ?, ?, ?)"#)
        .bind(owner).bind(addon).bind(crate::repositories::common::encode_datetime(at(start)))
        .bind(crate::repositories::common::encode_datetime(at(end)))
        .bind(i32::from(candidate.is_some())).bind(candidate.map(encode))
        .execute(&fixture.repo.0).await.unwrap();
}

async fn read(
    fixture: &Fixture,
    channel: i64,
    minute: i64,
) -> TeletextEffectiveChannelSelectionSnapshot {
    saved(
        fixture
            .repo
            .effective_channel_snapshot(channel, at(minute))
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn explicit_on_and_off_override_unhealthy_or_disabled_base_before_validation() {
    for mode in 0..3 {
        let (fixture, candidate) = setup().await;
        interval(&fixture, 1, ADDON, 0, 10, (mode != 1).then_some(&candidate)).await;
        sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted" = 1 WHERE "Name" = 'Base'"#)
            .execute(&fixture.repo.0)
            .await
            .unwrap();
        if mode == 1 {
            sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 0"#)
                .execute(&fixture.repo.0)
                .await
                .unwrap();
        } else if mode == 2 {
            sqlx::query(
                r#"UPDATE "ChannelTeletextBinding" SET "Enabled" = 0 WHERE "ChannelId" = 1"#,
            )
            .execute(&fixture.repo.0)
            .await
            .unwrap();
        }
        let before = changes(&fixture).await;
        let effective = read(&fixture, 1, 1).await;
        assert_eq!(effective.bindings.len(), 1);
        let Binding::Scripted(published) = &effective.bindings[0] else {
            panic!("base substituted")
        };
        assert_eq!(published.playout_id, 1);
        assert_eq!(published.interval.state.channel_id(), 1);
        assert_eq!(effective.bindings[0].enabled(), mode != 1);
        assert_eq!(effective.valid_until, Some(at(10)));
        assert_eq!(
            saved(
                fixture
                    .repo
                    .recheck_effective_channel_snapshot(&effective, at(2))
                    .await
                    .unwrap()
            ),
            effective
        );
        assert_eq!(changes(&fixture).await, before);
    }
}

#[tokio::test]
async fn unoverridden_base_members_remain_canonical_and_fully_validated() {
    let (fixture, candidate) = setup().await;
    let other = add_channel_producer(&fixture, "org.example.a-first").await;
    interval(&fixture, 1, ADDON, 0, 10, Some(&candidate)).await;
    let effective = read(&fixture, 1, 1).await;
    assert_eq!(
        effective
            .bindings
            .iter()
            .map(Binding::addon_id)
            .collect::<Vec<_>>(),
        ["org.example.a-first", ADDON]
    );
    assert!(matches!(effective.bindings[0], Binding::Base(_)));
    assert!(matches!(effective.bindings[1], Binding::Scripted(_)));
    sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted" = 1 WHERE "Id" = ?"#)
        .bind(other.profile_id)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .effective_channel_snapshot(1, at(1))
            .await
            .unwrap(),
        M::Rejected(R::DeletedProfile)
    );
    assert_eq!(
        fixture
            .repo
            .recheck_effective_channel_snapshot(&effective, at(1))
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
}

#[tokio::test]
async fn invalid_active_scope_never_falls_back_to_healthy_operator_base() {
    for statement in [
        r#"UPDATE "TeletextProfile" SET "Revision" = "Revision" + 1 WHERE "Name" = 'Target'"#,
        r#"UPDATE "AddonInstallation" SET "Enabled" = 0"#,
        r#"DELETE FROM "ChannelTeletextBinding" WHERE "ChannelId" = 1"#,
        r#"UPDATE "PlayoutTeletextInterval" SET "CandidateSnapshot" = '{"version":99}'"#,
    ] {
        let (fixture, candidate) = setup().await;
        interval(&fixture, 1, ADDON, 0, 10, Some(&candidate)).await;
        sqlx::query(statement)
            .execute(&fixture.repo.0)
            .await
            .unwrap();
        let before = changes(&fixture).await;
        assert!(fixture
            .repo
            .effective_channel_snapshot(1, at(1))
            .await
            .is_err());
        assert_eq!(changes(&fixture).await, before);
    }
}

#[tokio::test]
async fn base_changes_are_independent_but_target_package_and_published_scope_drift_conflict() {
    for mode in 0..4 {
        let (fixture, candidate) = setup().await;
        interval(&fixture, 1, ADDON, 0, 10, Some(&candidate)).await;
        let initial = read(&fixture, 1, 1).await;
        sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled" = 0, "ProfileId" = ?, "Revision" = "Revision" + 1 WHERE "ChannelId" = 1"#).bind(candidate.profile_id).execute(&fixture.repo.0).await.unwrap();
        assert_eq!(
            saved(
                fixture
                    .repo
                    .recheck_effective_channel_snapshot(&initial, at(2))
                    .await
                    .unwrap()
            ),
            initial
        );
        match mode {
            0 => {
                sqlx::query(r#"UPDATE "TeletextProfile" SET "Revision" = "Revision" + 1 WHERE "Name" = 'Target'"#).execute(&fixture.repo.0).await.unwrap();
            }
            1 => {
                sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256" = 'changed'"#)
                    .execute(&fixture.repo.0)
                    .await
                    .unwrap();
            }
            2 => {
                sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish" = ?"#)
                    .bind(crate::repositories::common::encode_datetime(at(5)))
                    .execute(&fixture.repo.0)
                    .await
                    .unwrap();
            }
            _ => {
                playout(&fixture, 3, 1).await;
                sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "PlayoutId" = 3"#)
                    .execute(&fixture.repo.0)
                    .await
                    .unwrap();
            }
        }
        assert_eq!(
            fixture
                .repo
                .recheck_effective_channel_snapshot(&initial, at(2))
                .await
                .unwrap(),
            M::Rejected(R::Conflict)
        );
    }
}

#[tokio::test]
async fn future_boundary_and_missed_interval_are_exact_clock_fences_without_observed_at() {
    let (fixture, candidate) = setup().await;
    interval(&fixture, 1, ADDON, 2, 4, Some(&candidate)).await;
    let before = read(&fixture, 1, 0).await;
    assert!(matches!(before.bindings[0], Binding::Base(_)));
    assert_eq!(before.valid_until, Some(at(2)));
    assert_eq!(
        saved(
            fixture
                .repo
                .recheck_effective_channel_snapshot(&before, at(1))
                .await
                .unwrap()
        ),
        before
    );
    let active = read(&fixture, 1, 2).await;
    assert!(matches!(active.bindings[0], Binding::Scripted(_)));
    assert_eq!(active.valid_until, Some(at(4)));
    let expired = read(&fixture, 1, 4).await;
    assert_eq!(expired.bindings, before.bindings);
    assert_eq!(expired.valid_until, None);
    assert_eq!(
        fixture
            .repo
            .recheck_effective_channel_snapshot(&before, at(5))
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    assert_eq!(
        fixture
            .repo
            .recheck_effective_channel_snapshot(&active, at(4))
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    interval(&fixture, 1, ADDON, 8, 10, None).await;
    let next = read(&fixture, 1, 5).await;
    assert_eq!(next.valid_until, Some(at(8)));
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Start" = ? WHERE "State" = 0"#)
        .bind(crate::repositories::common::encode_datetime(at(7)))
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .recheck_effective_channel_snapshot(&next, at(6))
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
}

#[tokio::test]
async fn output_channel_mirror_and_own_published_playout_origins_stay_isolated() {
    let (fixture, candidate) = setup().await;
    interval(&fixture, 1, ADDON, 0, 10, Some(&candidate)).await;
    sqlx::query(r#"UPDATE "Channel" SET "MirrorSourceChannelId" = 1 WHERE "Id" = 2"#)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    let source = read(&fixture, 1, 1).await;
    let mirror = read(&fixture, 2, 1).await;
    assert!(matches!(source.bindings[0], Binding::Scripted(_)));
    assert!(matches!(mirror.bindings[0], Binding::Base(_)));
    assert_eq!(mirror.bindings[0].channel_id(), 2);
    assert_eq!(mirror.valid_until, None);
    playout(&fixture, 2, 2).await;
    interval(&fixture, 2, ADDON, 0, 10, None).await;
    let own = read(&fixture, 2, 1).await;
    let Binding::Scripted(published) = &own.bindings[0] else {
        panic!("missing own Off")
    };
    assert_eq!(published.playout_id, 2);
    assert!(!own.bindings[0].enabled());
    assert_eq!(read(&fixture, 1, 1).await, source);
    assert_eq!(
        fixture
            .repo
            .effective_channel_snapshot(99, at(1))
            .await
            .unwrap(),
        M::Rejected(R::MissingChannel)
    );
}

async fn off_member(fixture: &Fixture, index: usize, start: i64, end: i64) {
    let addon = format!("org.example.off-{index:04}");
    let inserted = sqlx::query(r#"INSERT INTO "TeletextProfile" ("AddonId", "Name", "InstallationStateRevision", "InstallationVersion", "InstallationPackageSha256", "InstallationContentSha256", "InstallationManifestSha256") VALUES (?, 'Off', 'unused', 'unused', 'unused', 'unused', 'unused')"#)
        .bind(&addon).execute(&fixture.repo.0).await.unwrap();
    sqlx::query(r#"INSERT INTO "ChannelTeletextBinding" ("ChannelId", "AddonId", "ProfileId", "Enabled", "Revision") VALUES (1, ?, ?, 0, 1)"#)
        .bind(&addon).bind(inserted.last_insert_rowid()).execute(&fixture.repo.0).await.unwrap();
    interval(fixture, 1, &addon, start, end, None).await;
}

#[tokio::test]
async fn off_markers_do_not_consume_producer_budget_and_independent_row_bounds_precede_validation()
{
    let fixture = Fixture::new().await;
    let base = add_channel_producer(&fixture, "org.example.a-enabled").await;
    playout(&fixture, 1, 1).await;
    for index in 0..MAX_TELETEXT_OVERRIDE_COMMANDS {
        off_member(&fixture, index, 0, 5).await;
    }
    let effective = read(&fixture, 1, 1).await;
    assert_eq!(effective.bindings.len(), MAX_TELETEXT_OVERRIDE_COMMANDS + 1);
    assert_eq!(
        effective
            .bindings
            .iter()
            .filter(|binding| binding.enabled())
            .count(),
        1
    );
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled" = 1"#)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .effective_channel_snapshot(1, at(6))
            .await
            .unwrap(),
        M::Rejected(R::BindingLimit)
    );
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled" = 0 WHERE "AddonId" = ?"#)
        .bind(&base.addon_id)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    let on = saved(
        fixture
            .repo
            .candidate_snapshot(1, &base.addon_id, base.profile_id)
            .await
            .unwrap(),
    );
    interval(&fixture, 1, &base.addon_id, 6, 7, Some(&on)).await;
    assert_eq!(
        fixture
            .repo
            .effective_channel_snapshot(1, at(6))
            .await
            .unwrap(),
        M::Rejected(R::BindingLimit)
    );
    off_member(&fixture, MAX_TELETEXT_OVERRIDE_COMMANDS, 0, 5).await;
    assert!(fixture
        .repo
        .effective_channel_snapshot(1, at(1))
        .await
        .is_err());
}

#[tokio::test]
async fn exact_effective_read_and_recheck_survive_reopen_without_writes_or_replay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("effective.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let fixture = Fixture::from_pool(
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .unwrap(),
    )
    .await;
    let candidate = populate(&fixture).await;
    interval(&fixture, 1, ADDON, 0, 10, Some(&candidate)).await;
    let before = changes(&fixture).await;
    let effective = read(&fixture, 1, 1).await;
    assert_eq!(
        saved(
            fixture
                .repo
                .recheck_effective_channel_snapshot(&effective, at(2))
                .await
                .unwrap()
        ),
        effective
    );
    assert_eq!(changes(&fixture).await, before);
    fixture.repo.0.close().await;
    let repo = SqliteTeletextProfileRepository(
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap(),
    );
    assert_eq!(
        saved(repo.effective_channel_snapshot(1, at(2)).await.unwrap()),
        effective
    );
    assert_eq!(
        saved(
            repo.recheck_effective_channel_snapshot(&effective, at(9))
                .await
                .unwrap()
        ),
        effective
    );
    assert_eq!(
        repo.recheck_effective_channel_snapshot(&effective, at(10))
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    let writes: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(&repo.0)
        .await
        .unwrap();
    assert_eq!(writes, 0);
    repo.0.close().await;
}

#[tokio::test]
async fn host_custom_pages_remain_base_members_during_scripted_on_and_off() {
    for enabled in [false, true] {
        let (fixture, candidate) = setup().await;
        interval(&fixture, 1, ADDON, 0, 10, enabled.then_some(&candidate)).await;
        sqlx::query(r#"INSERT INTO "TeletextElement"
            ("Id", "Name", "SourceFormat", "SourceFileName", "Document", "PageNumber",
             "SubpageCount", "CycleSeconds", "Bindings", "Revision", "UpdatedAt")
            VALUES (1, 'Fixture custom page', 1, 'fixture.tti', X'20', 336, 1, 10, '[]', 1, 'fixture')"#)
            .execute(&fixture.repo.0).await.unwrap();
        sqlx::query(
            r#"INSERT INTO "ChannelTeletextElement" ("ChannelId", "TeletextElementId", "IsEnabled")
            VALUES (1, 1, 1)"#,
        )
        .execute(&fixture.repo.0)
        .await
        .unwrap();
        let before = changes(&fixture).await;
        let selection = read(&fixture, 1, 1).await;
        assert_eq!(selection.bindings.len(), 2);
        assert_eq!(
            selection.bindings[0],
            Binding::Base(ersatzrs_domain::teletext_custom_pages_selection(1))
        );
        assert!(matches!(selection.bindings[1], Binding::Scripted(_)));
        assert_eq!(selection.bindings[1].enabled(), enabled);
        assert_eq!(selection.valid_until, Some(at(10)));
        assert_eq!(
            saved(
                fixture
                    .repo
                    .recheck_effective_channel_snapshot(&selection, at(2))
                    .await
                    .unwrap()
            ),
            selection
        );
        assert_eq!(changes(&fixture).await, before);
        assert!(!read(&fixture, 2, 1)
            .await
            .bindings
            .iter()
            .any(|binding| ersatzrs_domain::is_teletext_custom_pages_producer(binding.addon_id())));
        // New Check/publication must observe link changes, unlike later output
        // confirmation of an already published immutable original.
        sqlx::query(r#"UPDATE "ChannelTeletextElement" SET "IsEnabled" = 0"#)
            .execute(&fixture.repo.0)
            .await
            .unwrap();
        assert_eq!(
            fixture
                .repo
                .recheck_effective_channel_snapshot(&selection, at(2))
                .await
                .unwrap(),
            M::Rejected(R::Conflict)
        );
        assert_eq!(read(&fixture, 1, 2).await.bindings.len(), 1);
        assert_eq!(
            fixture
                .repo
                .recheck_effective_channel_snapshot(&selection, at(10))
                .await
                .unwrap(),
            M::Rejected(R::Conflict)
        );
    }
}

async fn producer_limit_fixture() -> Fixture {
    let (fixture, _) = setup().await;
    interval(&fixture, 1, ADDON, 0, 10, None).await;
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled"=0 WHERE "ChannelId"=1"#)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    for index in 0..MAX_TELETEXT_PAGE_RECORDS {
        let mut manifest = fixture.manifest.clone();
        manifest.id = format!("org.example.capacity.p{index:04}");
        insert_installation(&fixture.repo.0, &manifest).await;
        let token = fixture
            .repo
            .installation_snapshot(&manifest.id)
            .await
            .unwrap()
            .snapshot
            .unwrap();
        let profile = saved(
            fixture
                .repo
                .save_profile(&TeletextProfileSave {
                    addon_id: manifest.id,
                    name: "Capacity".to_owned(),
                    settings: vec![literal("THEME", "day")],
                    expected_installation: token,
                    existing: None,
                })
                .await
                .unwrap(),
        )
        .profile;
        fixture.bind(1, &profile).await;
    }
    fixture
}

// Real durable Base confirmation at the bound. Its opaque body/artifact is a
// storage fixture; native composition feasibility is proved independently.
async fn base_generation_at_limit(
    fixture: &Fixture,
    selection: &TeletextChannelSelectionSnapshot,
) -> ersatzrs_domain::TeletextAcceptedGeneration {
    use ersatzrs_core::repositories::{
        TeletextDesignMutation, TeletextDesignRepository as _, TeletextGenerationMutation,
        TeletextGenerationRepository as _, TeletextPublicationMutation,
        TeletextPublicationRepository as _,
    };
    let designs = crate::repositories::SqliteTeletextDesignRepository(fixture.repo.0.clone());
    let TeletextDesignMutation::Saved(head) = designs
        .save(
            &ersatzrs_core::repositories::TeletextDesignSave {
                name: "Capacity Base fixture".to_owned(),
                document_version: 1,
                document: br#"{"format_version":1}"#.to_vec(),
                existing: None,
            },
            1024,
        )
        .await
        .unwrap()
    else {
        panic!("design rejected")
    };
    let revision = designs
        .revision(head.id, head.revision, 1024)
        .await
        .unwrap()
        .unwrap();
    let publications =
        crate::repositories::SqliteTeletextPublicationRepository(fixture.repo.0.clone());
    let TeletextPublicationMutation::Requested(request) = publications
        .request(
            &ersatzrs_core::repositories::TeletextPublicationRequestSave {
                design_id: head.id,
                revision: head.revision,
                digest: revision.digest,
                expected_design_head: head.revision,
                expected_request_id: None,
                selection: selection.clone(),
            },
            1024,
        )
        .await
        .unwrap()
    else {
        panic!("request rejected")
    };
    let generations =
        crate::repositories::SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let TeletextGenerationMutation::Accepted(generation) = generations
        .accept(
            &ersatzrs_core::repositories::TeletextGenerationSave {
                request,
                expected_design_head: head.revision,
                expected_generation_id: None,
                selection: selection.clone(),
                artifact_version: 1,
                artifact_digest: "a".repeat(64),
            },
            1024,
        )
        .await
        .unwrap()
    else {
        panic!("generation rejected")
    };
    generation
}

#[tokio::test]
async fn published_host_absence_at_producer_limit_survives_unpublished_link_addition() {
    let fixture = producer_limit_fixture().await;
    let original = read(&fixture, 1, 1).await;
    let base = saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap());
    let base_generation = base_generation_at_limit(&fixture, &base).await;
    assert_eq!(
        original
            .bindings
            .iter()
            .filter(|binding| binding.enabled())
            .count(),
        MAX_TELETEXT_PAGE_RECORDS
    );
    sqlx::query(r#"INSERT INTO "TeletextElement"
        ("Id","Name","SourceFormat","SourceFileName","Document","PageNumber","SubpageCount","CycleSeconds","Bindings","Revision","UpdatedAt")
        VALUES (1,'New unpublished page',1,'fixture.tti',X'20',336,1,10,'[]',1,'fixture')"#)
        .execute(&fixture.repo.0).await.unwrap();
    sqlx::query(r#"INSERT INTO "ChannelTeletextElement" ("ChannelId","TeletextElementId","IsEnabled") VALUES (1,1,1)"#)
        .execute(&fixture.repo.0).await.unwrap();
    // The next Check must reject this new producer; published output retains absence.
    assert_eq!(
        fixture
            .repo
            .effective_channel_snapshot(1, at(2))
            .await
            .unwrap(),
        M::Rejected(R::BindingLimit)
    );
    assert_eq!(
        fixture.repo.channel_invocation_snapshot(1).await.unwrap(),
        M::Rejected(R::BindingLimit)
    );
    assert_eq!(
        saved(
            fixture
                .repo
                .published_channel_dependencies(1)
                .await
                .unwrap()
        ),
        base
    );
    assert_eq!(
        saved(
            fixture
                .repo
                .published_effective_channel_dependencies(1, at(2))
                .await
                .unwrap()
        ),
        original
    );
    {
        use ersatzrs_core::repositories::TeletextGenerationRepository as _;
        let generations =
            crate::repositories::SqliteTeletextGenerationRepository(fixture.repo.0.clone());
        assert_eq!(
            generations
                .confirm_current(&base_generation, &base, 1024)
                .await
                .unwrap(),
            ersatzrs_core::repositories::TeletextGenerationMutation::Accepted(())
        );
    }
    let before = changes(&fixture).await;
    // Read-only transaction: recheck dependencies; never accept or publish a generation.
    let mut tx = fixture.repo.0.begin().await.unwrap();
    let retained = recheck_published_effective_in_transaction(&mut tx, &original, at(2))
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(retained, M::Saved(original.clone()));
    assert_eq!(changes(&fixture).await, before);
    let mut over_budget = original.clone();
    over_budget.bindings.insert(
        0,
        Binding::Base(ersatzrs_domain::teletext_custom_pages_selection(1)),
    );
    // Read-only transaction: recheck dependencies; never accept or publish a generation.
    let mut tx = fixture.repo.0.begin().await.unwrap();
    assert_eq!(
        recheck_published_effective_in_transaction(&mut tx, &over_budget, at(2))
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    tx.rollback().await.unwrap();
    sqlx::query(r#"UPDATE "TeletextProfile" SET "Revision"="Revision"+1 WHERE "AddonId"='org.example.capacity.p0000'"#)
        .execute(&fixture.repo.0).await.unwrap();
    // Read-only transaction: recheck dependencies; never accept or publish a generation.
    let mut tx = fixture.repo.0.begin().await.unwrap();
    assert_eq!(
        recheck_published_effective_in_transaction(&mut tx, &original, at(2))
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    tx.rollback().await.unwrap();
}
