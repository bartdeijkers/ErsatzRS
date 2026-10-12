use super::{common, document, open, request, seed};
use ersatzrs_infra::{
    db::DbPool,
    external_json::{
        self,
        snapshot::{
            self, AcceptedSnapshotBudget, AcceptedSnapshotError, AcceptedSnapshotUsage,
            SnapshotError,
        },
    },
};
use sqlx::Executor;
use std::path::Path;

fn budget() -> AcceptedSnapshotBudget {
    AcceptedSnapshotBudget {
        max_document_bytes: 32 * 1024 * 1024,
        max_path_bytes: 4096,
        max_total_bytes: 32 * 1024 * 1024 + 4096,
        max_programs: 1024,
    }
}

async fn stored(bytes: &[u8], path: &str) -> DbPool {
    let pool = common::test_db().await;
    seed(&pool, Path::new(path)).await;
    sqlx::query(
        r#"INSERT INTO "PlayoutExternalJsonSnapshot"
        ("PlayoutId","Revision","SchemaVersion","ScheduleFile","Document","PublishedAt")
        VALUES (1,1,1,?,?, '2026-04-15T10:00:00Z')"#,
    )
    .bind(path)
    .bind(std::str::from_utf8(bytes).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn read(
    pool: &DbPool,
    limits: AcceptedSnapshotBudget,
) -> Result<Option<snapshot::BoundedSnapshot>, AcceptedSnapshotError> {
    let mut connection = pool.acquire().await.unwrap();
    snapshot::accepted_on(&mut connection, 1, limits).await
}

#[tokio::test]
async fn separate_and_combined_utf8_byte_limits_are_inclusive_and_redacted() {
    let bytes = document(&["Programme é"]);
    let path = "sources/é.json";
    let pool = stored(&bytes, path).await;
    let limits = AcceptedSnapshotBudget {
        max_document_bytes: bytes.len(),
        max_path_bytes: path.len(),
        max_total_bytes: bytes.len() + path.len(),
        max_programs: 1,
    };
    let result = read(&pool, limits).await.unwrap().unwrap();
    assert_eq!(
        result.usage(),
        AcceptedSnapshotUsage {
            document_bytes: bytes.len(),
            path_bytes: path.len(),
            programs: 1,
        }
    );
    assert_eq!(result.snapshot().revision, 1);
    assert_eq!(result.snapshot().schedule_file, path);
    assert_eq!(
        result.snapshot().schedule.items[0].program.title.as_deref(),
        Some("Programme é")
    );
    let debug = format!("{result:?}");
    assert!(!debug.contains(path));
    assert!(!debug.contains("Programme"));
    assert!(path.len() > path.chars().count());
    for (category, actual, changed) in [
        (
            "document bytes",
            bytes.len(),
            AcceptedSnapshotBudget {
                max_document_bytes: bytes.len() - 1,
                ..limits
            },
        ),
        (
            "path bytes",
            path.len(),
            AcceptedSnapshotBudget {
                max_path_bytes: path.len() - 1,
                ..limits
            },
        ),
        (
            "total bytes",
            bytes.len() + path.len(),
            AcceptedSnapshotBudget {
                max_total_bytes: bytes.len() + path.len() - 1,
                ..limits
            },
        ),
    ] {
        assert_eq!(
            read(&pool, changed).await.unwrap_err(),
            AcceptedSnapshotError::Budget {
                category,
                count: actual,
                maximum: actual - 1,
            }
        );
    }
    let retained = read(&pool, limits).await.unwrap().unwrap().into_snapshot();
    assert_eq!(retained.revision, 1);
    assert_eq!(retained.schedule_file, path);
}

#[tokio::test]
async fn raw_programs_are_charged_even_when_shorter_than_guide_minimum() {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "startTime":"2026-04-15T10:00:00Z", "guideMinimumDurationSeconds":3600,
        "programs":(0..1000).map(|_| serde_json::json!({"duration":1})).collect::<Vec<_>>()
    }))
    .unwrap();
    let pool = stored(&bytes, "tiny.json").await;
    assert_eq!(
        read(
            &pool,
            AcceptedSnapshotBudget {
                max_programs: 64,
                ..budget()
            }
        )
        .await
        .unwrap_err(),
        AcceptedSnapshotError::Budget {
            category: "programs",
            count: 65,
            maximum: 64
        }
    );
    let result = read(
        &pool,
        AcceptedSnapshotBudget {
            max_programs: 1000,
            ..budget()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.usage().programs, 1000);
    assert_eq!(result.snapshot().schedule.items.len(), 1000);
    assert_eq!(
        result.snapshot().schedule.guide_minimum_duration_seconds,
        3600
    );
    assert_eq!(
        read(
            &pool,
            AcceptedSnapshotBudget {
                max_programs: 0,
                ..budget()
            }
        )
        .await
        .unwrap_err(),
        AcceptedSnapshotError::Budget {
            category: "programs",
            count: 1,
            maximum: 0
        }
    );
    let empty = document(&[]);
    sqlx::query(r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Document"=? WHERE "PlayoutId"=1"#)
        .bind(std::str::from_utf8(&empty).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let accepted = read(
        &pool,
        AcceptedSnapshotBudget {
            max_programs: 0,
            ..budget()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(accepted.usage().programs, 0);
    assert!(accepted.snapshot().schedule.items.is_empty());
}

#[tokio::test]
async fn counting_rejects_ambiguous_shapes_and_full_parser_remains_authority() {
    let pool = stored(&document(&["Accepted"]), "accepted.json").await;
    for invalid in [
        r#"{"startTime":"2026-04-15T10:00:00Z","programs":[],"programs":[]}"#,
        r#"{"startTime":"2026-04-15T10:00:00Z","programs":{}}"#,
        r#"{"startTime":"2026-04-15T10:00:00Z"}"#,
        r#"{"startTime":"invalid","programs":[]}"#,
        r#"{"startTime":"2026-04-15T10:00:00Z","programs":[{"duration":0}]}"#,
        r#"{"startTime":"2026-04-15T10:00:00Z","programs":[]} trailing"#,
        "null",
        "42",
        "{broken",
    ] {
        sqlx::query(r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Document"=? WHERE "PlayoutId"=1"#)
            .bind(invalid)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            read(&pool, budget()).await.unwrap_err(),
            AcceptedSnapshotError::Snapshot(SnapshotError::Invalid)
        );
        let unchanged: String = sqlx::query_scalar(
            r#"SELECT "Document" FROM "PlayoutExternalJsonSnapshot" WHERE "PlayoutId"=1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(unchanged, invalid);
    }
}

#[tokio::test]
async fn stored_scalar_types_and_versions_fail_closed_without_conversion() {
    let bytes = document(&["Accepted"]);
    let pool = stored(&bytes, "accepted.json").await;
    for mutation in [
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Revision"='broken'"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Revision"=CAST('1' AS BLOB)"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Revision"=1.5"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "SchemaVersion"=CAST('1' AS BLOB)"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "SchemaVersion"='broken'"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "SchemaVersion"=2"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Document"=CAST("Document" AS BLOB)"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Document"=42"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "ScheduleFile"=CAST("ScheduleFile" AS BLOB)"#,
    ] {
        pool.execute(mutation).await.unwrap();
        assert_eq!(
            read(&pool, budget()).await.unwrap_err(),
            AcceptedSnapshotError::Snapshot(SnapshotError::Invalid)
        );
        sqlx::query(r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Revision"=1,"SchemaVersion"=1,"Document"=?,"ScheduleFile"='accepted.json'"#)
            .bind(std::str::from_utf8(&bytes).unwrap()).execute(&pool).await.unwrap();
    }
    assert_eq!(
        read(&pool, budget())
            .await
            .unwrap()
            .unwrap()
            .snapshot()
            .revision,
        1
    );
}

#[tokio::test]
async fn hard_document_cap_survives_larger_caller_allowance() {
    let pool = stored(&document(&["Accepted"]), "accepted.json").await;
    // Test-only corruption bypasses the schema guard to exercise the reader's own fence.
    pool.execute("PRAGMA ignore_check_constraints=ON")
        .await
        .unwrap();
    pool.execute(
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Document"=printf('%.*c',33554433,'x')"#,
    )
    .await
    .unwrap();
    let limits = AcceptedSnapshotBudget {
        max_document_bytes: usize::MAX,
        max_path_bytes: usize::MAX,
        max_total_bytes: usize::MAX,
        max_programs: usize::MAX,
    };
    assert_eq!(
        read(&pool, limits).await.unwrap_err(),
        AcceptedSnapshotError::Snapshot(SnapshotError::TooLarge)
    );
    let size: i64 = sqlx::query_scalar(
        r#"SELECT length(CAST("Document" AS BLOB)) FROM "PlayoutExternalJsonSnapshot""#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(size, 33_554_433);
}

#[tokio::test]
async fn absence_does_not_accept_or_consult_a_live_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("live.json");
    let pool = common::test_db().await;
    seed(&pool, &path).await;
    std::fs::write(&path, document(&["Live"])).unwrap();
    assert!(read(
        &pool,
        AcceptedSnapshotBudget {
            max_document_bytes: 0,
            max_path_bytes: 0,
            max_total_bytes: 0,
            max_programs: 0,
        }
    )
    .await
    .unwrap()
    .is_none());
    let schedule = external_json::load(&path).await.unwrap().unwrap();
    snapshot::publish(&pool, request(&path, None), &schedule)
        .await
        .unwrap();
    std::fs::write(&path, document(&["Changed"])).unwrap();
    std::fs::remove_file(&path).unwrap();
    let result = read(&pool, budget()).await.unwrap().unwrap();
    assert_eq!(
        result.snapshot().schedule.items[0].program.title.as_deref(),
        Some("Live")
    );
    let revisions: i64 =
        sqlx::query_scalar(r#"SELECT count(*) FROM "PlayoutExternalJsonSnapshot""#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(revisions, 1);
    assert_eq!(result.snapshot().revision, 1);
}

#[tokio::test]
async fn borrowed_read_transaction_keeps_accepted_revision_during_pool_publication() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("schedule.json");
    let pool = open(&root.path().join("state.db")).await;
    pool.execute("PRAGMA journal_mode=WAL").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    seed(&pool, &path).await;
    std::fs::write(&path, document(&["First"])).unwrap();
    let first = external_json::load(&path).await.unwrap().unwrap();
    snapshot::publish(&pool, request(&path, None), &first)
        .await
        .unwrap();
    let mut transaction = pool.begin().await.unwrap();
    let captured = snapshot::accepted_on(&mut transaction, 1, budget())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(captured.snapshot().revision, 1);
    std::fs::write(&path, document(&["Second"])).unwrap();
    let second = external_json::load(&path).await.unwrap().unwrap();
    assert_eq!(
        snapshot::publish(&pool, request(&path, Some(1)), &second)
            .await
            .unwrap(),
        2
    );
    let pinned = snapshot::accepted_on(&mut transaction, 1, budget())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pinned.snapshot().revision, 1);
    assert!(pinned.snapshot().schedule == first);
    transaction.rollback().await.unwrap();
    let current = read(&pool, budget()).await.unwrap().unwrap();
    assert_eq!(current.snapshot().revision, 2);
    assert!(current.snapshot().schedule == second);
}
