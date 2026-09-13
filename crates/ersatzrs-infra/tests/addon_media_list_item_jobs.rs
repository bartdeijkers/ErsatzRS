mod common;

use std::fmt::Write as _;

use chrono::Duration;
use ersatzrs_addon_contract::{AddonCapability, MediaListAccessCheckOutcome};
use ersatzrs_infra::{
    addon_media_list_item_jobs::*, addon_media_list_options::load_draft_item_options,
};
use sha2::{Digest as _, Sha256};
use sqlx::Executor as _;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}

async fn fixture() -> (
    sqlx::SqlitePool,
    AddonMediaListItemJobQueue,
    ItemOperationInput,
) {
    let pool = common::test_db().await;
    pool.execute(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt") VALUES ('org.example.jobs','1',1,'package','content','{}','addons/fixture','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z')"#).await.unwrap();
    let definition: AddonCapability = serde_json::from_str(r#"{"id":"media-list.options.v1","options":[{"key":"enabled","label":{"en-US":"Enabled"},"description":{"en-US":"Fixture option"},"scope":"both","operation_targets":["test_access","preparation"],"applicability":"finite_only","kind":"boolean","default":false}]}"#).unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListOptionSchema" ("Id","AddonId","CapabilityId","SchemaDigest","DefinitionJson","CreatedAt") VALUES (1,'org.example.jobs','media-list.options.v1',?,?,'2026-09-05T00:00:00Z')"#)
        .bind(sha256_hex(&serde_json::to_vec(&definition).unwrap())).bind(serde_json::to_string(&definition).unwrap()).execute(&pool).await.unwrap();
    pool.execute(r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","DraftOptionSchemaId") VALUES (1,'org.example.jobs','list','https://example.test/list','Fixture',1)"#).await.unwrap();
    pool.execute(r#"INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness") VALUES (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1)"#).await.unwrap();
    let resolved = load_draft_item_options(&pool, 1, 10)
        .await
        .unwrap()
        .unwrap();
    let package_identity = sha256_hex(
        &serde_json::to_vec(&("org.example.jobs", "1", "package", "content", "initial")).unwrap(),
    );
    let input = ItemOperationInput {
        source_url: "https://example.test/item".into(),
        provider_id: "item".into(),
        context_fingerprint: resolved
            .context_fingerprint(
                "https://example.test/item",
                "item",
                &package_identity,
                "1.0",
                Some(1080),
            )
            .unwrap(),
        schema: Some(resolved.schema),
        definition: Some(resolved.definition),
        options: Some(resolved.values),
        addon_id: None,
        preparation_v2: None,
        package_identity,
        tool_version: "1.0".into(),
        item_is_finite: true,
        max_video_height: Some(1080),
        requested_timing: None,
        requested_subtitle_languages: Vec::new(),
    };
    (pool.clone(), AddonMediaListItemJobQueue::new(pool), input)
}

fn success(now: chrono::DateTime<chrono::Utc>) -> ItemOperationCompletion {
    ItemOperationCompletion::Access {
        outcome: MediaListAccessCheckOutcome::Accessible,
        expires_at: now + Duration::minutes(10),
    }
}

fn interval_input(mut input: ItemOperationInput) -> ItemOperationInput {
    let timing = ItemOperationTiming {
        item_duration_seconds: None,
        fragment_start_seconds: None,
        fragment_end_seconds: None,
    };
    input.context_fingerprint = timing
        .bind_context_fingerprint(&input.context_fingerprint)
        .unwrap();
    input.requested_timing = Some(timing);
    input
}

#[allow(clippy::single_range_in_vec_init)] // The single element is an excluded source interval.
fn interval_completion(
    input: &ItemOperationInput,
    window: std::ops::Range<u64>,
) -> ItemOperationCompletion {
    ItemOperationCompletion::IntervalPlan {
        plan: ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan::new(
            10_000,
            window,
            &[2_000..3_000],
            input.context_fingerprint.clone(),
        )
        .unwrap(),
    }
}

#[tokio::test]
async fn interval_jobs_complete_without_artifacts_and_timing_changes_hide_evidence() {
    let (pool, queue, input) = fixture().await;
    let input = interval_input(input);
    let now = common::fixed_time();
    let id = queue
        .enqueue(
            1,
            10,
            0,
            ItemOperationKind::IntervalMetadata,
            input.clone(),
            now,
        )
        .await
        .unwrap();
    let job = queue
        .claim_kind(now, 30, ItemOperationKind::IntervalMetadata)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.kind, ItemOperationKind::IntervalMetadata);
    let completion = interval_completion(&input, 0..10_000);
    queue.complete(&job, completion.clone(), now).await.unwrap();
    assert_eq!(
        queue
            .status(id, now + Duration::days(30))
            .await
            .unwrap()
            .unwrap()
            .completion,
        Some(completion)
    );
    let artifacts: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtifact""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(artifacts, 0);
    let stored: (Option<i64>, Option<String>) = sqlx::query_as(
        r#"SELECT "ArtifactId", "ExpiresAt" FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, (None, None));
    for (update, reset) in [
        (
            r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=1 WHERE "Id"=10"#,
            r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=NULL WHERE "Id"=10"#,
        ),
        (
            r#"UPDATE "AddonMediaListItem" SET "FragmentStartSeconds"=1 WHERE "Id"=10"#,
            r#"UPDATE "AddonMediaListItem" SET "FragmentStartSeconds"=NULL WHERE "Id"=10"#,
        ),
        (
            r#"UPDATE "AddonMediaListItem" SET "FragmentEndSeconds"=1 WHERE "Id"=10"#,
            r#"UPDATE "AddonMediaListItem" SET "FragmentEndSeconds"=NULL WHERE "Id"=10"#,
        ),
    ] {
        sqlx::query(update).execute(&pool).await.unwrap();
        let status = queue.status(id, now).await.unwrap().unwrap();
        assert_eq!(status.state, 5);
        assert!(status.completion.is_none());
        sqlx::query(reset).execute(&pool).await.unwrap();
    }
}

#[tokio::test]
async fn interval_jobs_reject_wrong_mode_fingerprint_window_and_changed_claim() {
    let (pool, queue, base) = fixture().await;
    let now = common::fixed_time();
    assert!(queue
        .enqueue(
            1,
            10,
            0,
            ItemOperationKind::IntervalMetadata,
            base.clone(),
            now
        )
        .await
        .is_err());
    let input = interval_input(base);
    for kind in [ItemOperationKind::Access, ItemOperationKind::Preparation] {
        assert!(queue
            .enqueue(1, 10, 0, kind, input.clone(), now)
            .await
            .is_err());
    }
    let mut invalid = input.clone();
    invalid.context_fingerprint = "a".repeat(64);
    assert!(queue
        .enqueue(
            1,
            10,
            0,
            ItemOperationKind::IntervalMetadata,
            invalid.clone(),
            now
        )
        .await
        .is_err());
    invalid = input.clone();
    invalid.item_is_finite = false;
    assert!(queue
        .enqueue(1, 10, 0, ItemOperationKind::IntervalMetadata, invalid, now)
        .await
        .is_err());
    queue
        .enqueue(
            1,
            10,
            0,
            ItemOperationKind::IntervalMetadata,
            input.clone(),
            now,
        )
        .await
        .unwrap();
    let job = queue.claim(now, 30).await.unwrap().unwrap();
    assert!(queue
        .complete(&job, interval_completion(&input, 1_000..10_000), now)
        .await
        .is_err());
    let mut other_context = input.clone();
    other_context.context_fingerprint = "a".repeat(64);
    assert!(queue
        .complete(&job, interval_completion(&other_context, 0..10_000), now)
        .await
        .is_err());
    let mut forged = job.clone();
    forged.kind = ItemOperationKind::Access;
    forged.input.requested_timing = None;
    assert!(queue.complete(&forged, success(now), now).await.is_err());
    let mut forged = job.clone();
    forged.input.max_video_height = Some(720);
    assert!(queue
        .complete(&forged, interval_completion(&input, 0..10_000), now)
        .await
        .is_err());
    pool.execute(r#"UPDATE "AddonMediaListItem" SET "DurationSeconds"=12 WHERE "Id"=10"#)
        .await
        .unwrap();
    assert!(queue
        .complete(&job, interval_completion(&input, 0..10_000), now)
        .await
        .is_err());
    assert!(queue
        .claim(now + Duration::seconds(31), 30)
        .await
        .unwrap()
        .is_none());
    assert_eq!(queue.status(job.id, now).await.unwrap().unwrap().state, 5);
}

#[test]
fn interval_timing_checks_overflow_and_uses_probed_duration() {
    let mut timing = ItemOperationTiming {
        item_duration_seconds: Some(20),
        fragment_start_seconds: Some(2),
        fragment_end_seconds: None,
    };
    assert_eq!(
        timing.source_window_milliseconds(10_000).unwrap(),
        2_000..10_000
    );
    timing.fragment_end_seconds = Some(8);
    assert_eq!(
        timing.source_window_milliseconds(10_000).unwrap(),
        2_000..8_000
    );
    assert!(timing.source_window_milliseconds(7_000).is_err());
    timing.fragment_start_seconds = Some(u64::MAX);
    assert!(timing.source_window_milliseconds(10_000).is_err());
    assert!(timing.bind_context_fingerprint(&"a".repeat(64)).is_err());
}

#[tokio::test]
async fn legacy_input_bytes_and_interval_subtitle_migrations_preserve_jobs_and_indexes() {
    let (pool, queue, input) = fixture().await;
    let encoded = serde_json::to_string(&input).unwrap();
    assert!(!encoded.contains("requested_timing"));
    assert_eq!(
        serde_json::to_string(&serde_json::from_str::<ItemOperationInput>(&encoded).unwrap())
            .unwrap(),
        encoded
    );
    pool.execute(r#"DROP TABLE "AddonMediaListItemOperationJob""#)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../migrations/202609051900_addon_media_list_item_jobs.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let now = common::fixed_time();
    let id = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input.clone(), now)
        .await
        .unwrap();
    let job = queue.claim(now, 30).await.unwrap().unwrap();
    queue.complete(&job, success(now), now).await.unwrap();
    queue
        .enqueue(1, 10, 0, ItemOperationKind::Preparation, input, now)
        .await
        .unwrap();
    let preparation = queue.claim(now, 30).await.unwrap().unwrap();
    let query = r#"SELECT json_array("Id", "AddonMediaListId", "AddonMediaListItemId",
        "Kind", "Generation", "SavedDraftRevision", "InputJson", "ContextFingerprint",
        "State", "AttemptCount", "NextEligibleAt", "LeaseOwner", "LeaseExpiresAt",
        "HeartbeatAt", "ArtifactId", "OutcomeJson", "ExpiresAt", "CreatedAt",
        "UpdatedAt", "CompletedAt") FROM "AddonMediaListItemOperationJob" ORDER BY "Id""#;
    let before: Vec<String> = sqlx::query_scalar(query).fetch_all(&pool).await.unwrap();
    let index_query = r#"SELECT "name", "sql" FROM "sqlite_master" WHERE "type"='index' AND "tbl_name"='AddonMediaListItemOperationJob' ORDER BY "name""#;
    let indexes: Vec<(String, String)> =
        sqlx::query_as(index_query).fetch_all(&pool).await.unwrap();
    assert_eq!(indexes.len(), 3);
    pool.execute(
        r#"UPDATE "sqlite_sequence" SET "seq"=100 WHERE "name"='AddonMediaListItemOperationJob'"#,
    )
    .await
    .unwrap();
    for migration in [
        include_str!("../../../migrations/202609061030_addon_interval_metadata_jobs.sql"),
        include_str!("../../../migrations/202609071900_addon_subtitle_jobs.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
        let after: Vec<String> = sqlx::query_scalar(query).fetch_all(&pool).await.unwrap();
        assert_eq!(before, after);
        let after_indexes: Vec<(String, String)> =
            sqlx::query_as(index_query).fetch_all(&pool).await.unwrap();
        assert_eq!(indexes, after_indexes);
        let sequence: i64 = sqlx::query_scalar(
            r#"SELECT "seq" FROM "sqlite_sequence" WHERE "name"='AddonMediaListItemOperationJob'"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sequence, 100);
    }
    assert_eq!(queue.status(id, now).await.unwrap().unwrap().state, 2);
    assert_eq!(preparation.kind, ItemOperationKind::Preparation);
    queue.heartbeat(&preparation, now, 30).await.unwrap();
}

#[tokio::test]
async fn item_jobs_migration_idempotence_reclaim_and_stale_owner() {
    let (_pool, queue, input) = fixture().await;
    let now = common::fixed_time();
    let id = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input.clone(), now)
        .await
        .unwrap();
    assert_eq!(
        id,
        queue
            .enqueue(1, 10, 0, ItemOperationKind::Access, input, now)
            .await
            .unwrap()
    );
    let first = queue.claim(now, 30).await.unwrap().unwrap();
    assert!(queue.claim(now, 30).await.unwrap().is_none());
    let later = now + Duration::seconds(31);
    let second = queue.claim(later, 30).await.unwrap().unwrap();
    assert_eq!(second.generation, first.generation + 1);
    assert!(queue.complete(&first, success(later), later).await.is_err());
    assert!(queue.heartbeat(&first, later, 30).await.is_err());
    queue
        .complete(&second, success(later), later)
        .await
        .unwrap();
    assert_eq!(queue.status(id, now).await.unwrap().unwrap().state, 2);
}

#[tokio::test]
async fn item_jobs_cancel_and_revision_fence_preserve_newer_work() {
    let (pool, queue, input) = fixture().await;
    let now = common::fixed_time();
    let id = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input.clone(), now)
        .await
        .unwrap();
    let claimed = queue.claim(now, 30).await.unwrap().unwrap();
    queue.cancel(id, now).await.unwrap();
    assert!(queue.complete(&claimed, success(now), now).await.is_err());
    assert!(queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input.clone(), now)
        .await
        .is_err());
    let later = now + Duration::seconds(31);
    let second = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input.clone(), later)
        .await
        .unwrap();
    let claimed = queue.claim(later, 30).await.unwrap().unwrap();
    pool.execute(r#"UPDATE "AddonMediaList" SET "DraftRevision"=1 WHERE "Id"=1"#)
        .await
        .unwrap();
    assert!(queue
        .complete(&claimed, success(later), later)
        .await
        .is_err());
    assert!(queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input, later)
        .await
        .is_err());
    assert_eq!(queue.status(second, later).await.unwrap().unwrap().state, 1);
}

#[tokio::test]
async fn item_jobs_attempt_budget_and_source_ownership_are_bounded() {
    let (pool, queue, mut input) = fixture().await;
    let now = common::fixed_time();
    let id = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input.clone(), now)
        .await
        .unwrap();
    for attempt in 0..3 {
        assert!(queue
            .claim(now + Duration::seconds(attempt * 31), 30)
            .await
            .unwrap()
            .is_some());
    }
    assert!(queue
        .claim(now + Duration::seconds(93), 30)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        queue.status(id, now).await.unwrap().unwrap().completion,
        Some(ItemOperationCompletion::Failed(
            ItemOperationFailure::AttemptsExhausted
        ))
    );
    pool.execute(r#"UPDATE "AddonMediaListItem" SET "SourceUrl"='https://example.test/changed' WHERE "Id"=10"#).await.unwrap();
    assert!(queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input.clone(), now)
        .await
        .is_err());
    input.item_is_finite = false;
    assert!(queue
        .enqueue(1, 10, 0, ItemOperationKind::Preparation, input, now)
        .await
        .is_err());
}

#[tokio::test]
async fn item_jobs_admission_waits_do_not_exhaust_attempts_and_package_drift_is_fenced() {
    let (pool, queue, input) = fixture().await;
    let mut now = common::fixed_time();
    let id = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input, now)
        .await
        .unwrap();
    for _ in 0..5 {
        let job = queue.claim(now, 30).await.unwrap().unwrap();
        queue
            .defer_unstarted(&job, now, now + Duration::seconds(1))
            .await
            .unwrap();
        assert!(queue
            .defer_unstarted(&job, now, now + Duration::seconds(1))
            .await
            .is_err());
        now += Duration::seconds(1);
    }
    assert_eq!(
        queue.status(id, now).await.unwrap().unwrap().attempt_count,
        0
    );
    let job = queue.claim(now, 30).await.unwrap().unwrap();
    pool.execute(r#"UPDATE "AddonInstallation" SET "StateRevision"='changed' WHERE "AddonId"='org.example.jobs'"#).await.unwrap();
    assert!(queue.complete(&job, success(now), now).await.is_err());
}

#[tokio::test]
async fn item_jobs_changed_input_failure_still_requires_a_live_owned_attempt() {
    let (pool, queue, input) = fixture().await;
    let now = common::fixed_time();
    let failure = ItemOperationCompletion::Failed(ItemOperationFailure::InputChanged);
    let id = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input, now)
        .await
        .unwrap();
    let first = queue.claim(now, 30).await.unwrap().unwrap();
    let later = now + Duration::seconds(31);
    assert!(queue
        .complete(&first, failure.clone(), later)
        .await
        .is_err());
    let current = queue.claim(later, 30).await.unwrap().unwrap();
    assert!(queue
        .complete(&first, failure.clone(), later)
        .await
        .is_err());
    pool.execute(r#"UPDATE "AddonInstallation" SET "StateRevision"='changed' WHERE "AddonId"='org.example.jobs'"#)
        .await
        .unwrap();
    assert!(queue
        .complete(&current, success(later), later)
        .await
        .is_err());
    queue
        .complete(&current, failure.clone(), later)
        .await
        .unwrap();
    let status = queue.status(id, later).await.unwrap().unwrap();
    assert_eq!(status.state, 3);
    assert_eq!(status.completion, Some(failure.clone()));
    assert!(queue.complete(&current, failure, later).await.is_err());

    let (_pool, queue, input) = fixture().await;
    let id = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input, now)
        .await
        .unwrap();
    let cancelled = queue.claim(now, 30).await.unwrap().unwrap();
    queue.cancel(id, now).await.unwrap();
    assert!(queue
        .complete(
            &cancelled,
            ItemOperationCompletion::Failed(ItemOperationFailure::InputChanged),
            now,
        )
        .await
        .is_err());
    assert_eq!(queue.status(id, now).await.unwrap().unwrap().state, 4);
}

#[tokio::test]
async fn item_jobs_decode_failure_does_not_permanently_stale_job() {
    let (pool, queue, input) = fixture().await;
    let now = common::fixed_time();
    let id = queue
        .enqueue(1, 10, 0, ItemOperationKind::Access, input, now)
        .await
        .unwrap();
    pool.execute(r#"UPDATE "AddonMediaListItem" SET "ProviderLiveness"='corrupt' WHERE "Id"=10"#)
        .await
        .unwrap();
    assert!(queue.claim(now, 30).await.is_err());
    assert_eq!(queue.status(id, now).await.unwrap().unwrap().state, 0);
}

#[tokio::test]
async fn item_jobs_status_expires_access_and_hides_changed_context_without_history_writes() {
    for mutation in [
        r#"UPDATE "AddonMediaList" SET "DraftRevision"=1 WHERE "Id"=1"#,
        r#"UPDATE "AddonMediaListItem" SET "SourceUrl"='https://example.test/changed' WHERE "Id"=10"#,
        r#"UPDATE "AddonInstallation" SET "StateRevision"='changed' WHERE "AddonId"='org.example.jobs'"#,
    ] {
        let (pool, queue, input) = fixture().await;
        let now = common::fixed_time();
        let id = queue
            .enqueue(1, 10, 0, ItemOperationKind::Access, input.clone(), now)
            .await
            .unwrap();
        let job = queue.claim(now, 30).await.unwrap().unwrap();
        queue.complete(&job, success(now), now).await.unwrap();
        let valid = queue.status(id, now).await.unwrap().unwrap();
        assert_eq!(valid.state, 2);
        assert_eq!(valid.context_fingerprint, input.context_fingerprint);
        assert_eq!(valid.tool_version, input.tool_version);
        let expired = queue
            .status(id, now + Duration::minutes(10))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(expired.state, 5);
        assert!(expired.completion.is_none());
        pool.execute(mutation).await.unwrap();
        let stale = queue.status(id, now).await.unwrap().unwrap();
        assert_eq!(stale.state, 5);
        assert!(stale.completion.is_none());
        let (persisted_state, json): (i32, String) = sqlx::query_as(
            r#"SELECT "State","OutcomeJson" FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(persisted_state, 2);
        assert_eq!(
            serde_json::from_str::<ItemOperationCompletion>(&json).unwrap(),
            success(now)
        );
    }
}
