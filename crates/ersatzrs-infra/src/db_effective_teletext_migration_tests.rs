//! Exact upgrade/rollback proofs for the distinct opaque original format.

use super::*;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

const VERSION: i64 = 202_610_140_746;
const ORIGINAL: &[u8] = b"synthetic original bytes\r\n";

fn sha256_hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(result, "{byte:02x}").unwrap();
    }
    result
}

async fn open(path: &Path) -> DbPool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Wal),
        )
        .await
        .unwrap()
}

async fn fixture() -> (tempfile::TempDir, DbPool) {
    let directory = tempfile::tempdir().unwrap();
    let pool = open(&directory.path().join("migration.sqlite")).await;
    Migrator::with_migrations(
        SQLITE_MIGRATOR
            .iter()
            .filter(|migration| migration.version < VERSION)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    seed(&pool).await;
    assert_fences(&pool).await;
    (directory, pool)
}

async fn seed(pool: &DbPool) {
    sqlx::raw_sql(
        r#"
        INSERT INTO "TeletextDesign" ("Id","Name","Revision","IsDeleted") VALUES
            (11,'Retained tombstone',2,1),(12,'Retained active',1,0);
    "#,
    )
    .execute(pool)
    .await
    .unwrap();
    for (rowid, design, revision, bytes) in [
        (73, 11, 1, ORIGINAL),
        (89, 11, 2, b"synthetic edited original\n".as_slice()),
        (155, 12, 1, b"synthetic other channel".as_slice()),
    ] {
        sqlx::query(r#"INSERT INTO "TeletextDesignRevision" (rowid,"DesignId","Revision","DocumentVersion","Digest","Document") VALUES (?,?,?,1,?,?)"#)
            .bind(rowid).bind(design).bind(revision).bind(sha256_hex(bytes))
            .bind(bytes).execute(pool).await.unwrap();
    }
    sqlx::raw_sql(r#"
        INSERT INTO "TeletextPublicationRequest" ("Id","ChannelId","DesignId","Revision","Digest")
            SELECT 31,101,11,1,"Digest" FROM "TeletextDesignRevision" WHERE "DesignId"=11 AND "Revision"=1;
        INSERT INTO "TeletextPublicationRequest" ("Id","ChannelId","DesignId","Revision","Digest")
            SELECT 32,102,12,1,"Digest" FROM "TeletextDesignRevision" WHERE "DesignId"=12 AND "Revision"=1;
        INSERT INTO "TeletextPublicationRequestCurrent" ("ChannelId","RequestId") VALUES (101,31),(102,32);
        INSERT INTO "TeletextAcceptedGeneration" ("Id","ChannelId","RequestId","ArtifactVersion","ArtifactDigest")
            VALUES (41,101,31,1,printf('%064d',1)),(42,102,32,1,printf('%064d',2));
        INSERT INTO "TeletextAcceptedGenerationCurrent" ("ChannelId","GenerationId") VALUES (101,41),(102,42);
    "#).execute(pool).await.unwrap();
    let receipt = b"ABC\0\xff";
    sqlx::query(r#"INSERT INTO "TeletextDesignExportReceipt" ("Id","DesignId","Revision","DesignDigest","ExpectedDesignHead","ReceiptVersion","Digest","Receipt")
        SELECT 51,11,2,"Digest",2,1,?,? FROM "TeletextDesignRevision" WHERE "DesignId"=11 AND "Revision"=2"#)
        .bind(sha256_hex(receipt)).bind(receipt.as_slice())
        .execute(pool).await.unwrap();
}

async fn content(pool: &DbPool) -> Vec<Vec<String>> {
    // Explicit deterministic projections retain identity, framing and binary bytes.
    let queries = [
        r#"SELECT json_array("Id","Name","Revision","IsDeleted") FROM "TeletextDesign" ORDER BY "Id""#,
        r#"SELECT json_array(rowid,"DesignId","Revision","DocumentVersion","Digest",hex("Document")) FROM "TeletextDesignRevision" ORDER BY rowid"#,
        r#"SELECT json_array("Id","ChannelId","DesignId","Revision","Digest") FROM "TeletextPublicationRequest" ORDER BY "Id""#,
        r#"SELECT json_array("ChannelId","RequestId") FROM "TeletextPublicationRequestCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("Id","ChannelId","RequestId","ArtifactVersion","ArtifactDigest") FROM "TeletextAcceptedGeneration" ORDER BY "Id""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextAcceptedGenerationCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("Id","DesignId","Revision","DesignDigest","ExpectedDesignHead","ReceiptVersion","Digest",hex("Receipt")) FROM "TeletextDesignExportReceipt" ORDER BY "Id""#,
        r#"SELECT json_array("name","seq") FROM sqlite_sequence WHERE "name" LIKE 'Teletext%' ORDER BY "name""#,
    ];
    let mut rows = Vec::new();
    for query in queries {
        rows.push(sqlx::query_scalar(query).fetch_all(pool).await.unwrap());
    }
    rows
}

async fn history(pool: &DbPool) -> Vec<(i64, String, bool, Vec<u8>)> {
    sqlx::query_as("SELECT version,description,success,checksum FROM _sqlx_migrations WHERE version<? ORDER BY version")
        .bind(VERSION).fetch_all(pool).await.unwrap()
}

async fn schema(pool: &DbPool, include_revision: bool) -> Vec<(String, String)> {
    sqlx::query_as(r#"SELECT "name","sql" FROM sqlite_master WHERE "sql" IS NOT NULL AND
        ("tbl_name" IN ('TeletextDesign','TeletextPublicationRequest','TeletextPublicationRequestCurrent',
            'TeletextAcceptedGeneration','TeletextAcceptedGenerationCurrent','TeletextDesignExportReceipt')
        OR (? AND "tbl_name"='TeletextDesignRevision')) ORDER BY "name""#)
        .bind(include_revision).fetch_all(pool).await.unwrap()
}

async fn assert_fences(pool: &DbPool) {
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA ignore_check_constraints")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA defer_foreign_keys")
            .fetch_one(pool)
            .await
            .unwrap(),
        0
    );
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .unwrap()
        .is_empty());
    let temporary: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_temp_master WHERE name LIKE 'TeletextDesignRevisionMigration%'")
        .fetch_one(pool).await.unwrap();
    assert_eq!(temporary, 0);
}

#[tokio::test]
async fn previous_sqlx_database_preserves_exact_history_and_foreign_references_on_upgrade_reopen() {
    let (directory, pool) = fixture().await;
    let original = content(&pool).await;
    let old_history = history(&pool).await;
    let original_children = schema(&pool, false).await;
    SQLITE_MIGRATOR.run(&pool).await.unwrap();
    assert_eq!(content(&pool).await, original);
    assert_eq!(history(&pool).await, old_history);
    assert_eq!(schema(&pool, false).await, original_children);
    let (success, checksum): (bool, Vec<u8>) =
        sqlx::query_as("SELECT success,checksum FROM _sqlx_migrations WHERE version=?")
            .bind(VERSION)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(success);
    assert_eq!(
        checksum,
        SQLITE_MIGRATOR
            .iter()
            .find(|migration| migration.version == VERSION)
            .unwrap()
            .checksum
            .as_ref()
    );
    for statement in [
        r#"UPDATE "TeletextDesignRevision" SET "Document"=X'00' WHERE rowid=73"#,
        r#"DELETE FROM "TeletextDesignRevision" WHERE rowid=73"#,
        r#"DELETE FROM "TeletextDesign" WHERE "Id"=11"#,
    ] {
        assert!(sqlx::query(statement).execute(&pool).await.is_err());
    }
    assert_eq!(content(&pool).await, original);
    sqlx::query(r#"INSERT INTO "TeletextDesignRevision" (rowid,"DesignId","Revision","DocumentVersion","Digest","Document") VALUES (201,12,2,2,printf('%064d',4),X'00FF')"#)
        .execute(&pool).await.unwrap();
    // This revision has no child reference: only the restored immutable trigger
    // can prevent deletion, rather than an existing foreign-key restriction.
    assert!(
        sqlx::query(r#"DELETE FROM "TeletextDesignRevision" WHERE rowid=201"#)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query(r#"INSERT INTO "TeletextDesignRevision" (rowid,"DesignId","Revision","DocumentVersion","Digest","Document") VALUES (202,12,3,3,printf('%064d',5),X'00FF')"#)
        .execute(&pool).await.is_err());
    assert_fences(&pool).await;
    let upgraded = content(&pool).await;
    pool.close().await;
    let reopened = open(&directory.path().join("migration.sqlite")).await;
    SQLITE_MIGRATOR.run(&reopened).await.unwrap();
    assert_eq!(content(&reopened).await, upgraded);
    assert_eq!(history(&reopened).await, old_history);
    assert_fences(&reopened).await;
}

#[tokio::test]
async fn malformed_old_version_aborts_upgrade_and_preserves_original_schema_rows_and_metadata() {
    let (_directory, pool) = fixture().await;
    let trigger: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE name='TR_TeletextDesignRevision_ImmutableUpdate'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // Deliberately corrupt only the negative fixture, then restore every fence.
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "DocumentVersion"=3 WHERE rowid=73"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=OFF")
        .execute(&pool)
        .await
        .unwrap();
    // The captured DDL is the immutable trigger from this owned fixture schema.
    sqlx::raw_sql(assert_sql_safe(trigger))
        .execute(&pool)
        .await
        .unwrap();
    let original = content(&pool).await;
    let old_history = history(&pool).await;
    let old_schema = schema(&pool, true).await;
    let failure = SQLITE_MIGRATOR.run(&pool).await.unwrap_err();
    assert!(failure
        .to_string()
        .contains("CK_TeletextDesignRevision_DocumentVersion"));
    assert_eq!(content(&pool).await, original);
    assert_eq!(schema(&pool, true).await, old_schema);
    assert_eq!(history(&pool).await, old_history);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version>=?")
            .bind(VERSION)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert!(
        sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "Document"=X'00' WHERE rowid=89"#)
            .execute(&pool)
            .await
            .is_err()
    );
    assert_fences(&pool).await;
}

#[tokio::test]
async fn affected_dangling_child_is_rejected_by_the_migration_guard_and_whole_transaction_rolls_back(
) {
    let (_directory, pool) = fixture().await;
    let original = content(&pool).await;
    let old_schema = schema(&pool, true).await;
    let old_history = history(&pool).await;
    let mut tx = begin_write(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        1
    );
    sqlx::query("PRAGMA defer_foreign_keys=ON")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "TeletextPublicationRequest" ("Id","ChannelId","DesignId","Revision","Digest") VALUES (99,999,999,1,printf('%064d',9))"#)
        .execute(&mut *tx).await.unwrap();
    let migration = SQLITE_MIGRATOR
        .iter()
        .find(|migration| migration.version == VERSION)
        .unwrap();
    assert!(!migration.no_tx);
    let failure = sqlx::raw_sql(migration.sql.clone())
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(failure
        .to_string()
        .contains("CK_TeletextDesignRevisionMigrationIntegrity_Zero"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        1
    );
    tx.rollback().await.unwrap();
    assert_eq!(content(&pool).await, original);
    assert_eq!(schema(&pool, true).await, old_schema);
    assert_eq!(history(&pool).await, old_history);
    assert_fences(&pool).await;
}
