//! Real `SQLite` health projection; no script execution or native readiness claim.

use super::*;
use crate::repositories::{
    make_repositories, SqliteTeletextDesignRepository, SqliteTeletextGenerationRepository,
    SqliteTeletextProfileRepository, SqliteTeletextPublicationRepository,
};
use ersatzrs_addon_contract::{AddonManifest, ADDON_MANIFEST_VERSION};
use ersatzrs_core::repositories::{
    ChannelRepository as _, FfmpegProfileRepository as _, TeletextDesignMutation,
    TeletextDesignRepository as _, TeletextDesignSave, TeletextEffectiveGenerationSave,
    TeletextEffectivePublicationRequestSave, TeletextGenerationMutation, TeletextProfileMutation,
    TeletextProfileSave, TeletextPublicationMutation, TeletextPublicationRepository as _,
    TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{
    Channel, FFmpegProfile, TeletextAcceptedGeneration, TeletextEffectiveBindingSnapshot,
    TeletextEffectiveChannelSelectionSnapshot, TeletextProfileCandidateSnapshot,
    TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
};
use sqlx::sqlite::SqlitePoolOptions;

const ADDON: &str = "org.example.teletext.health";

fn saved<T>(result: TeletextProfileMutation<T>) -> T {
    match result {
        TeletextProfileMutation::Saved(value) => value,
        TeletextProfileMutation::Rejected(_) => panic!("synthetic candidate rejected"),
    }
}

async fn setup() -> (Pool<Db>, TeletextProfileCandidateSnapshot) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'fixture', 160, 90)"#).execute(&pool).await.unwrap();
    let repositories = make_repositories(pool.clone());
    repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Synthetic".to_owned(),
            resolution_id: 1,
            ..FFmpegProfile::default()
        })
        .await
        .unwrap();
    for id in 1..=2 {
        repositories
            .channel
            .upsert(&Channel {
                id,
                unique_id: uuid::Uuid::new_v4(),
                number: id.to_string(),
                name: format!("Synthetic {id}"),
                sort_number: f64::from(i32::try_from(id).unwrap()),
                ffmpeg_profile_id: 1,
                ..Channel::default()
            })
            .await
            .unwrap();
    }
    let manifest: AddonManifest = serde_json::from_value(serde_json::json!({
        "manifest_version":ADDON_MANIFEST_VERSION,"id":ADDON,"version":"1.0.0",
        "name":{"en-US":"Synthetic"},"summary":{"en-US":"Synthetic health configuration"},
        "license":"GPL-3.0-or-later","source_url":"https://example.test/source","host_version":"*",
        "capabilities":[{"id":"addon.check.v1"},{"id":"teletext.pages.v1"}],
        "entrypoints":{"any":{"path":"fixture.bin","kind":"native"}}
    }))
    .unwrap();
    manifest.validate().unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation"
        ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision")
        VALUES (?, '1.0.0', 1, 'synthetic-package', 'synthetic-content', ?, 'unused-fixture', 'now', 'now', 'synthetic-revision')"#)
        .bind(ADDON).bind(serde_json::to_string(&manifest).unwrap()).execute(&pool).await.unwrap();
    let profiles = SqliteTeletextProfileRepository(pool.clone());
    let token = profiles
        .installation_snapshot(ADDON)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    let profile = saved(
        profiles
            .save_profile(&TeletextProfileSave {
                addon_id: ADDON.to_owned(),
                name: "Synthetic profile".to_owned(),
                settings: Vec::new(),
                expected_installation: token,
                existing: None,
            })
            .await
            .unwrap(),
    )
    .profile;
    sqlx::query(r#"INSERT INTO "ChannelTeletextBinding" ("ChannelId", "AddonId", "ProfileId", "Enabled", "Revision") VALUES (1, ?, ?, 0, 1)"#)
        .bind(ADDON).bind(profile.id).execute(&pool).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, ?, 1)"#,
    )
    .bind(encode_enum(PlayoutScheduleKind::Scripted))
    .execute(&pool)
    .await
    .unwrap();
    let candidate = saved(
        profiles
            .candidate_snapshot(1, ADDON, profile.id)
            .await
            .unwrap(),
    );
    (pool, candidate)
}

fn snapshot(candidate: &TeletextProfileCandidateSnapshot) -> String {
    let token = &candidate.installation;
    serde_json::json!({"version":1,"channel":candidate.channel_id,"addon":candidate.addon_id,
        "profile":candidate.profile_id,"revision":candidate.profile_revision,
        "contract":candidate.contract.schema_version(),"settings":[],
        "installation":{"addon":token.addon_id,"revision":token.state_revision,
            "version":token.version,"package":token.package_sha256,
            "content":token.content_sha256,"manifest":token.manifest_sha256}})
    .to_string()
}

async fn insert(
    pool: &Pool<Db>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    value: Option<String>,
) {
    sqlx::query(r#"INSERT INTO "PlayoutTeletextInterval"
        ("PlayoutId", "AddonId", "Start", "Finish", "State", "CandidateSnapshot") VALUES (1, ?, ?, ?, ?, ?)"#)
        .bind(ADDON).bind(crate::repositories::common::encode_datetime(start))
        .bind(crate::repositories::common::encode_datetime(finish))
        .bind(i32::from(value.is_some())).bind(value).execute(pool).await.unwrap();
}

async fn changes(pool: &Pool<Db>) -> i64 {
    sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(pool)
        .await
        .unwrap()
}

// Only the explicit document-limit projection is under test here; native output
// policy validation remains the host's separate readiness responsibility.
fn policy(maximum: usize) -> serde_json::Value {
    serde_json::json!({
        "version":1,"recovery_timeout_ms":1000,
        "design":{"max_serialized_bytes":maximum,"max_documents":16,
            "max_response_bytes":1024,"max_context_bytes":1024,
            "max_context_field_bytes":256,"max_resource_transmissions":16},
        "input":{},"validation":{},"composition":{},"encoding":{},"source":{}
    })
}

async fn set_policy(pool: &Pool<Db>, value: &str) {
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('teletext.output.policy', ?)
        ON CONFLICT("Key") DO UPDATE SET "Value" = excluded."Value""#,
    )
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

fn original_scope(selection: &TeletextEffectiveChannelSelectionSnapshot) -> Vec<u8> {
    let [TeletextEffectiveBindingSnapshot::Scripted(published)] = selection.bindings.as_slice()
    else {
        panic!("single Scripted fixture")
    };
    let TeletextOverrideState::Enabled(candidate) = &published.interval.state else {
        panic!("enabled fixture")
    };
    let install = &candidate.installation;
    let producer = serde_json::json!({
        "contract":candidate.contract.schema_version(),"channel_id":candidate.channel_id,
        "addon_id":candidate.addon_id,"profile_id":candidate.profile_id,"profile_revision":candidate.profile_revision,
        "effective_settings":[],"installation":{"addon_id":install.addon_id,"state_revision":install.state_revision,
            "version":install.version,"package_sha256":install.package_sha256,"content_sha256":install.content_sha256,
            "manifest_sha256":install.manifest_sha256}
    });
    serde_json::to_vec(&serde_json::json!({
        "format_version":TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
        "effective_selection":{"channel_id":selection.channel_id,"valid_until":selection.valid_until,
            "bindings":[{"origin":"scripted","base":null,"scripted":{
                "playout_id":published.playout_id,"start":published.interval.start,"finish":published.interval.finish,
                "state":{"kind":"enabled","channel_id":candidate.channel_id,"addon_id":candidate.addon_id,
                    "candidate":producer}
            }}]}
    })).unwrap()
}

// SQL ledger fixtures deliberately omit native payloads. They prove only the
// Health projection's durable association, never compiled pages or sink output.
async fn accepted_fixture() -> (
    Pool<Db>,
    DateTime<Utc>,
    DateTime<Utc>,
    TeletextAcceptedGeneration,
) {
    let (pool, candidate) = setup().await;
    let now = Utc::now();
    let finish = now + chrono::Duration::hours(1);
    insert(
        &pool,
        now - chrono::Duration::hours(1),
        finish,
        Some(snapshot(&candidate)),
    )
    .await;
    set_policy(&pool, &policy(4096).to_string()).await;
    let selection = saved(
        SqliteTeletextProfileRepository(pool.clone())
            .effective_channel_snapshot(1, now)
            .await
            .unwrap(),
    );
    let designs = SqliteTeletextDesignRepository(pool.clone());
    let TeletextDesignMutation::Saved(head) = designs
        .save(
            &TeletextDesignSave {
                name: "Synthetic scoped health originals".to_owned(),
                document_version: TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
                document: original_scope(&selection),
                existing: None,
            },
            4096,
        )
        .await
        .unwrap()
    else {
        panic!("design fixture rejected")
    };
    let revision = designs
        .revision(head.id, head.revision, 4096)
        .await
        .unwrap()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let TeletextPublicationMutation::Requested(request) =
        SqliteTeletextPublicationRepository(pool.clone())
            .request_effective(
                &TeletextEffectivePublicationRequestSave {
                    design_id: head.id,
                    revision: head.revision,
                    digest: revision.digest,
                    expected_design_head: head.revision,
                    expected_request_id: None,
                    selection: selection.clone(),
                },
                4096,
                deadline,
            )
            .await
            .unwrap()
    else {
        panic!("request fixture rejected")
    };
    let TeletextGenerationMutation::Accepted(generation) =
        SqliteTeletextGenerationRepository(pool.clone())
            .accept_effective(
                &TeletextEffectiveGenerationSave {
                    request,
                    expected_design_head: head.revision,
                    expected_generation_id: None,
                    selection,
                    artifact_version: TELETEXT_EFFECTIVE_GENERATION_ARTIFACT_VERSION,
                    artifact_digest: "a".repeat(64),
                },
                4096,
                deadline,
            )
            .await
            .unwrap()
    else {
        panic!("generation fixture rejected")
    };
    (pool, now, finish, generation)
}

#[tokio::test]
async fn document_bound_is_explicit_closed_and_independent_of_page_wire_limits() {
    let (pool, _) = setup().await;
    let check = ScriptedPlayoutStateHealthCheck::new(pool.clone());
    assert_eq!(check.teletext_document_limit().await.unwrap(), None);
    let valid = policy(16 * 1024 * 1024);
    set_policy(&pool, &valid.to_string()).await;
    assert_eq!(
        check.teletext_document_limit().await.unwrap(),
        Some(16 * 1024 * 1024)
    );
    let mut padded = valid.to_string();
    padded.extend(std::iter::repeat_n(
        ' ',
        usize::try_from(TELETEXT_POLICY_BYTES).unwrap() - padded.len(),
    ));
    set_policy(&pool, &padded).await;
    assert_eq!(
        check.teletext_document_limit().await.unwrap(),
        Some(16 * 1024 * 1024)
    );
    padded.push(' ');
    set_policy(&pool, &padded).await;
    assert_eq!(check.teletext_document_limit().await.unwrap(), None);
    let mut invalid = Vec::new();
    for value in [
        serde_json::Value::Null,
        serde_json::json!(0),
        serde_json::json!("1024"),
        serde_json::json!(u64::MAX),
    ] {
        let mut next = valid.clone();
        next["design"]["max_serialized_bytes"] = value;
        invalid.push(next.to_string());
    }
    let mut missing = valid.clone();
    missing["design"]
        .as_object_mut()
        .unwrap()
        .remove("max_serialized_bytes");
    invalid.push(missing.to_string());
    let mut unknown = valid.clone();
    unknown["design"]["private_unknown_key"] = serde_json::json!(1024);
    invalid.push(unknown.to_string());
    let mut unknown = valid.clone();
    unknown["private_unknown_key"] = serde_json::json!(1024);
    invalid.push(unknown.to_string());
    let mut array = valid.clone();
    array["design"] = serde_json::json!([1024, 16, 1024, 1024, 256, 16]);
    invalid.push(array.to_string());
    invalid.push(format!("{valid} {{}}"));
    invalid.push("null".to_owned());
    invalid.push(valid.to_string().replacen(
        "\"max_serialized_bytes\":",
        "\"max_serialized_bytes\":1024,\"max_serialized_bytes\":",
        1,
    ));
    for value in invalid {
        set_policy(&pool, &value).await;
        let before = changes(&pool).await;
        assert_eq!(check.teletext_document_limit().await.unwrap(), None);
        assert_eq!(changes(&pool).await, before);
    }
}

#[tokio::test]
async fn accepted_on_reports_only_readonly_selected_state_association() {
    let (pool, now, finish, _) = accepted_fixture().await;
    let check = ScriptedPlayoutStateHealthCheck::new(pool.clone());
    let before = changes(&pool).await;
    assert_eq!(
        check.check_at(now).await.unwrap().status,
        HealthCheckStatus::Pass
    );
    assert_eq!(
        check.check_at(finish).await.unwrap().status,
        HealthCheckStatus::Pass
    );
    assert_eq!(changes(&pool).await, before);
}

#[tokio::test]
async fn accepted_on_without_explicit_document_bound_remains_pending() {
    let (pool, now, _, _) = accepted_fixture().await;
    for value in [
        None,
        Some("null"),
        Some("{}"),
        Some("{\"design\":{\"max_serialized_bytes\":0}}"),
    ] {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = 'teletext.output.policy'"#)
            .execute(&pool)
            .await
            .unwrap();
        if let Some(value) = value {
            set_policy(&pool, value).await;
        }
        let before = changes(&pool).await;
        let result = ScriptedPlayoutStateHealthCheck::new(pool.clone())
            .check_at(now)
            .await
            .unwrap();
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result
            .message
            .contains("scoped native generation is not available"));
        assert_eq!(changes(&pool).await, before);
    }
}

#[tokio::test]
async fn changed_valid_scope_does_not_relabel_an_accepted_original_as_current() {
    let (pool, now, finish, _) = accepted_fixture().await;
    sqlx::query(r#"UPDATE "PlayoutTeletextInterval" SET "Finish" = ?"#)
        .bind(crate::repositories::common::encode_datetime(
            finish + chrono::Duration::hours(1),
        ))
        .execute(&pool)
        .await
        .unwrap();
    let before = changes(&pool).await;
    let result = ScriptedPlayoutStateHealthCheck::new(pool.clone())
        .check_at(now)
        .await
        .unwrap();
    assert_eq!(result.status, HealthCheckStatus::Fail);
    assert!(result
        .message
        .contains("scoped native generation is not available"));
    assert_eq!(changes(&pool).await, before);
}

#[tokio::test]
async fn missing_scoped_pointer_and_corrupt_original_do_not_report_accepted_state() {
    for corrupt in [false, true] {
        let (pool, now, _, _) = accepted_fixture().await;
        if corrupt {
            sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "Document" = CAST('{"format_version":2}' AS BLOB)"#)
                .execute(&pool).await.unwrap();
        } else {
            sqlx::query(r#"DELETE FROM "TeletextEffectiveAcceptedGenerationCurrent""#)
                .execute(&pool)
                .await
                .unwrap();
        }
        let before = changes(&pool).await;
        let result = ScriptedPlayoutStateHealthCheck::new(pool.clone())
            .check_at(now)
            .await
            .unwrap();
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert!(result.message.contains(if corrupt {
            "active override is unavailable"
        } else {
            "scoped native generation is not available"
        }));
        assert_eq!(changes(&pool).await, before);
    }
}

#[tokio::test]
async fn valid_off_does_not_require_healthy_package_or_profile() {
    let (pool, _) = setup().await;
    let now = Utc::now();
    insert(
        &pool,
        now - chrono::Duration::hours(1),
        now + chrono::Duration::hours(1),
        None,
    )
    .await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 0"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted" = 1"#)
        .execute(&pool)
        .await
        .unwrap();
    set_policy(&pool, "null").await;
    let before = changes(&pool).await;
    let result = ScriptedPlayoutStateHealthCheck::new(pool.clone())
        .check_at(now)
        .await
        .unwrap();
    assert_eq!(result.status, HealthCheckStatus::Pass);
    assert_eq!(changes(&pool).await, before);
}

#[tokio::test]
async fn valid_on_reports_missing_scoped_native_generation_until_expiry_without_writes() {
    let (pool, candidate) = setup().await;
    let now = Utc::now();
    let finish = now + chrono::Duration::hours(1);
    insert(
        &pool,
        now - chrono::Duration::hours(1),
        finish,
        Some(snapshot(&candidate)),
    )
    .await;
    set_policy(&pool, &policy(4096).to_string()).await;
    let check = ScriptedPlayoutStateHealthCheck::new(pool.clone());
    let before = changes(&pool).await;
    let result = check.check_at(now).await.unwrap();
    assert_eq!(result.status, HealthCheckStatus::Fail);
    assert_eq!(result.message, "Scripted teletext interval for channel 1: scoped native generation is not available; the published timeline is retained.");
    assert!(!result.message.contains(ADDON));
    assert!(!result.message.contains("synthetic-package"));
    assert_eq!(
        check.check_at(finish).await.unwrap().status,
        HealthCheckStatus::Pass
    );
    assert_eq!(changes(&pool).await, before);
}

#[tokio::test]
async fn stale_malformed_or_missing_membership_remains_unavailable_until_original_expiry() {
    for statement in [
        r#"UPDATE "TeletextProfile" SET "Revision" = "Revision" + 1"#,
        r#"UPDATE "AddonInstallation" SET "Enabled" = 0"#,
        r#"DELETE FROM "ChannelTeletextBinding""#,
        r#"UPDATE "PlayoutTeletextInterval" SET "CandidateSnapshot" = '{"synthetic-private-setting":"redacted fixture"}'"#,
    ] {
        let (pool, candidate) = setup().await;
        let now = Utc::now();
        let finish = now + chrono::Duration::hours(1);
        insert(
            &pool,
            now - chrono::Duration::hours(1),
            finish,
            Some(snapshot(&candidate)),
        )
        .await;
        sqlx::query(statement).execute(&pool).await.unwrap();
        let check = ScriptedPlayoutStateHealthCheck::new(pool.clone());
        let before = changes(&pool).await;
        let result = check.check_at(now).await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(result.message, "Scripted teletext interval for channel 1: active override is unavailable; the published timeline is retained.");
        assert!(!result.message.contains("redacted fixture"));
        assert_eq!(
            check.check_at(finish).await.unwrap().status,
            HealthCheckStatus::Pass
        );
        assert_eq!(changes(&pool).await, before);
    }
}

#[tokio::test]
async fn future_override_and_mirror_channel_do_not_report_active_source_state() {
    let (pool, candidate) = setup().await;
    let now = Utc::now();
    let start = now + chrono::Duration::hours(1);
    insert(
        &pool,
        start,
        start + chrono::Duration::hours(1),
        Some(snapshot(&candidate)),
    )
    .await;
    sqlx::query(r#"UPDATE "Channel" SET "MirrorSourceChannelId" = 1 WHERE "Id" = 2"#)
        .execute(&pool)
        .await
        .unwrap();
    let check = ScriptedPlayoutStateHealthCheck::new(pool);
    assert_eq!(
        check.check_at(now).await.unwrap().status,
        HealthCheckStatus::Pass
    );
    let result = check.check_at(start).await.unwrap();
    assert_eq!(result.status, HealthCheckStatus::Fail);
    assert!(result.message.contains("channel 1:"));
    assert!(!result.message.contains("channel 2:"));
}
