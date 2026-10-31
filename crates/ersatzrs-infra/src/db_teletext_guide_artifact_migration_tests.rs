//! Guide roles preserve old immutable output, both pointer roles and advanced identities.
use super::{
    add_lifecycle_rows, assert_fences, assert_sql_safe, begin_write, child_schema, content,
    history_before, legacy as lifecycle_legacy, migrations_through, open, schema, scoped_pointers,
    seed, DbPool, LIFECYCLE_VERSION as PREVIOUS_VERSION, SQLITE_MIGRATOR,
};

const GUIDE_VERSION: i64 = 202_610_310_829;

async fn legacy() -> (tempfile::TempDir, DbPool) {
    let (directory, pool) = lifecycle_legacy().await;
    migrations_through(PREVIOUS_VERSION)
        .run(&pool)
        .await
        .unwrap();
    add_lifecycle_rows(&pool, 198).await;
    migrations_through(GUIDE_VERSION - 1)
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE sqlite_sequence SET seq=297 WHERE name='TeletextAcceptedGeneration'")
        .execute(&pool)
        .await
        .unwrap();
    (directory, pool)
}

async fn add_guide_rows(pool: &DbPool, expected_first_id: i64) {
    sqlx::raw_sql(r#"
        INSERT INTO "TeletextPublicationRequest" ("Id","ChannelId","DesignId","Revision","Digest")
            SELECT 37,107,11,1,"Digest" FROM "TeletextDesignRevision" WHERE "DesignId"=11 AND "Revision"=1;
        INSERT INTO "TeletextPublicationRequest" ("Id","ChannelId","DesignId","Revision","Digest")
            SELECT 38,108,12,1,"Digest" FROM "TeletextDesignRevision" WHERE "DesignId"=12 AND "Revision"=1;
    "#).execute(pool).await.unwrap();
    for (version, channel, request, expected) in [
        (5, 107, 37, expected_first_id),
        (6, 108, 38, expected_first_id + 1),
    ] {
        let result = sqlx::query(r#"INSERT INTO "TeletextAcceptedGeneration"
            ("ChannelId","RequestId","ArtifactVersion","ArtifactDigest") VALUES (?,?,?,printf('%064d',?))"#)
            .bind(channel).bind(request).bind(version).bind(version)
            .execute(pool).await.unwrap();
        assert_eq!(result.last_insert_rowid(), expected);
        // These new histories have no current pointer: the immutable triggers,
        // independently of child FKs, must still guard updates and deletion.
        for statement in [
            r#"UPDATE "TeletextAcceptedGeneration" SET "ArtifactDigest"=printf('%064d',8) WHERE "Id"=?"#,
            r#"DELETE FROM "TeletextAcceptedGeneration" WHERE "Id"=?"#,
        ] {
            assert!(sqlx::query(statement)
                .bind(expected)
                .execute(pool)
                .await
                .is_err());
        }
    }
}

async fn assert_invalid_versions_and_foreign_keys(pool: &DbPool) {
    for version in [0, 7, 65_535] {
        let failure = sqlx::query(r#"INSERT INTO "TeletextAcceptedGeneration"
            ("ChannelId","RequestId","ArtifactVersion","ArtifactDigest") VALUES (104,34,?,printf('%064d',5))"#)
            .bind(version).execute(pool).await.unwrap_err();
        assert!(failure
            .to_string()
            .contains("CK_TeletextAcceptedGeneration_ArtifactVersion"));
    }
    for statement in [
        r#"INSERT INTO "TeletextAcceptedGenerationCurrent" ("ChannelId","GenerationId") VALUES (999,41)"#,
        r#"INSERT INTO "TeletextEffectiveAcceptedGenerationCurrent" ("ChannelId","GenerationId") VALUES (999,98)"#,
        r#"INSERT INTO "TeletextAcceptedGeneration" ("ChannelId","RequestId","ArtifactVersion","ArtifactDigest") VALUES (999,999,5,printf('%064d',5))"#,
    ] {
        let failure = sqlx::query(statement).execute(pool).await.unwrap_err();
        assert!(failure.to_string().contains("FOREIGN KEY"));
    }
    assert_fences(pool).await;
}

#[tokio::test]
async fn guide_artifact_migration_preserves_populated_history_both_roles_and_reopens() {
    let (directory, pool) = legacy().await;
    let versions: Vec<i64> = sqlx::query_scalar(r#"SELECT DISTINCT "ArtifactVersion" FROM "TeletextAcceptedGeneration" ORDER BY "ArtifactVersion""#)
        .fetch_all(&pool).await.unwrap();
    assert_eq!(versions, vec![1, 2, 3, 4]);
    let rows = content(&pool).await;
    let pointers = scoped_pointers(&pool).await;
    let children = child_schema(&pool).await;
    let previous_migrations = history_before(&pool, GUIDE_VERSION).await;
    migrations_through(GUIDE_VERSION).run(&pool).await.unwrap();
    assert_eq!(content(&pool).await, rows);
    assert_eq!(scoped_pointers(&pool).await, pointers);
    assert_eq!(child_schema(&pool).await, children);
    assert_eq!(
        history_before(&pool, GUIDE_VERSION).await,
        previous_migrations
    );
    assert_invalid_versions_and_foreign_keys(&pool).await;
    add_guide_rows(&pool, 298).await;
    let upgraded = content(&pool).await;
    pool.close().await;
    let reopened = open(&directory.path().join("migration.sqlite")).await;
    migrations_through(GUIDE_VERSION)
        .run(&reopened)
        .await
        .unwrap();
    assert_eq!(content(&reopened).await, upgraded);
    assert_eq!(scoped_pointers(&reopened).await, pointers);
    assert_eq!(child_schema(&reopened).await, children);
    assert_eq!(
        history_before(&reopened, GUIDE_VERSION).await,
        previous_migrations
    );
    assert_fences(&reopened).await;
}

#[tokio::test]
async fn guide_artifact_migration_supports_fresh_schema_and_empty_advanced_sequence() {
    let directory = tempfile::tempdir().unwrap();
    let pool = open(&directory.path().join("fresh.sqlite")).await;
    SQLITE_MIGRATOR.run(&pool).await.unwrap();
    seed(&pool).await;
    add_guide_rows(&pool, 43).await;
    assert_fences(&pool).await;
    pool.close().await;

    let empty = open(&directory.path().join("empty.sqlite")).await;
    migrations_through(GUIDE_VERSION - 1)
        .run(&empty)
        .await
        .unwrap();
    // The preceding constraint rebuild creates the empty AUTOINCREMENT row at
    // zero. sqlite_sequence has no unique constraint on name: INSERT would add
    // a duplicate row, rather than advance this valid existing sequence.
    let updated =
        sqlx::query("UPDATE sqlite_sequence SET seq=297 WHERE name='TeletextAcceptedGeneration'")
            .execute(&empty)
            .await
            .unwrap();
    assert_eq!(updated.rows_affected(), 1);
    let before: Vec<i64> = sqlx::query_scalar(
        "SELECT seq FROM sqlite_sequence WHERE name='TeletextAcceptedGeneration'",
    )
    .fetch_all(&empty)
    .await
    .unwrap();
    assert_eq!(before, vec![297]);
    migrations_through(GUIDE_VERSION).run(&empty).await.unwrap();
    let sequence: i64 = sqlx::query_scalar(
        "SELECT seq FROM sqlite_sequence WHERE name='TeletextAcceptedGeneration'",
    )
    .fetch_one(&empty)
    .await
    .unwrap();
    assert_eq!(sequence, 297);
    let history: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextAcceptedGeneration""#)
        .fetch_one(&empty)
        .await
        .unwrap();
    assert_eq!(history, 0);
    assert_fences(&empty).await;
}

#[tokio::test]
async fn guide_artifact_migration_rolls_back_on_invalid_legacy_version() {
    let (_directory, pool) = legacy().await;
    let trigger: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE name='TR_TeletextAcceptedGeneration_ImmutableUpdate'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(r#"DROP TRIGGER "TR_TeletextAcceptedGeneration_ImmutableUpdate""#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextAcceptedGeneration" SET "ArtifactVersion"=7 WHERE "Id"=41"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=OFF")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(assert_sql_safe(trigger))
        .execute(&pool)
        .await
        .unwrap();
    let rows = content(&pool).await;
    let pointers = scoped_pointers(&pool).await;
    let before_schema = schema(&pool, true).await;
    let failure = migrations_through(GUIDE_VERSION)
        .run(&pool)
        .await
        .unwrap_err();
    assert!(failure
        .to_string()
        .contains("CK_TeletextAcceptedGeneration_ArtifactVersion"));
    assert_eq!(content(&pool).await, rows);
    assert_eq!(scoped_pointers(&pool).await, pointers);
    assert_eq!(schema(&pool, true).await, before_schema);
    assert_fences(&pool).await;
}

#[tokio::test]
async fn guide_artifact_migration_guard_rejects_dangling_effective_pointer() {
    let (_directory, pool) = legacy().await;
    let rows = content(&pool).await;
    let pointers = scoped_pointers(&pool).await;
    let mut tx = begin_write(&pool).await.unwrap();
    sqlx::query("PRAGMA defer_foreign_keys=ON")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "TeletextEffectiveAcceptedGenerationCurrent" ("ChannelId","GenerationId") VALUES (999,999)"#)
        .execute(&mut *tx).await.unwrap();
    let migration = SQLITE_MIGRATOR
        .iter()
        .find(|migration| migration.version == GUIDE_VERSION)
        .unwrap();
    assert!(!migration.no_tx);
    let failure = sqlx::raw_sql(migration.sql.clone())
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(failure
        .to_string()
        .contains("CK_TeletextAcceptedGenerationMigrationIntegrity_Zero"));
    tx.rollback().await.unwrap();
    assert_eq!(content(&pool).await, rows);
    assert_eq!(scoped_pointers(&pool).await, pointers);
    assert_fences(&pool).await;
}
