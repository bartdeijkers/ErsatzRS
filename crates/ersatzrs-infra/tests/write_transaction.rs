//! Regression coverage for `SQLite`'s deferred-transaction upgrade failure.
//!
//! A transaction started with a plain `begin()` is deferred: it takes a read
//! snapshot on its first statement and only tries to take the write lock when
//! it first writes. If another connection commits in between, that upgrade
//! fails immediately with `SQLITE_BUSY`, and `busy_timeout` deliberately does
//! not apply, because waiting could never resolve it. Almost every write path
//! reads before it writes, so the default `begin()` is a latent 500 under
//! concurrency. `ersatzrs_infra::db::begin_write` issues `BEGIN IMMEDIATE`
//! instead, which takes the write lock up front where `busy_timeout` does
//! apply, so concurrent writers queue rather than fail.

use std::time::Duration;

use ersatzrs_infra::db::{begin_write, init, DbPool};
use sqlx::Executor;
use tempfile::TempDir;

/// A file-backed database, so several pooled connections really contend.
///
/// `sqlite::memory:` gives each connection its own database, which cannot
/// reproduce cross-connection lock contention.
async fn contended_db() -> (TempDir, DbPool) {
    let directory = tempfile::tempdir().expect("write transaction temp directory");
    let database = directory.path().join("ErsatzRS.db");
    let pool = init(&format!("sqlite://{}", database.display()))
        .await
        .expect("open contended database");
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .expect("seed media source");
    pool.execute(
        r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
           VALUES (1, 0, 1, 'Fixture')"#,
    )
    .await
    .expect("seed library");
    (directory, pool)
}

async fn insert_library_path(executor: impl Executor<'_, Database = sqlx::Sqlite>, path: &str) {
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Path", "LibraryId") VALUES (?, 1)"#)
        .bind(path)
        .execute(executor)
        .await
        .expect("insert library path");
}

async fn count_library_paths(pool: &DbPool) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryPath""#)
        .fetch_one(pool)
        .await
        .expect("count library paths")
}

/// The control: this is the failure the whole change exists to remove.
#[tokio::test]
async fn deferred_transaction_cannot_upgrade_after_a_concurrent_commit() {
    let (_directory, pool) = contended_db().await;

    let mut tx = pool.begin().await.expect("begin deferred transaction");
    // Take the read snapshot, exactly as every read-then-write path does.
    let _existing: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryPath""#)
        .fetch_one(&mut *tx)
        .await
        .expect("read snapshot");

    // A second connection commits while the snapshot is held.
    insert_library_path(&pool, "/media/concurrent").await;

    // The deferred transaction can no longer take the write lock. This must
    // fail immediately rather than wait out `busy_timeout`.
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Path", "LibraryId") VALUES ('/media/tx', 1)"#)
            .execute(&mut *tx),
    )
    .await
    .expect("the deferred upgrade must fail immediately, not wait for busy_timeout")
    .expect_err("a deferred transaction cannot upgrade over a concurrent commit");

    assert!(
        error.to_string().contains("database is locked"),
        "expected a SQLITE_BUSY upgrade failure, got: {error}"
    );
}

/// The fix: the same sequence survives, because the write lock is already held.
#[tokio::test]
async fn begin_write_survives_a_concurrent_writer() {
    let (_directory, pool) = contended_db().await;

    let mut tx = begin_write(&pool).await.expect("begin write transaction");
    let existing: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryPath""#)
        .fetch_one(&mut *tx)
        .await
        .expect("read snapshot");
    assert_eq!(existing, 0);

    // A second connection wants to write while the transaction holds the write
    // lock. `BEGIN IMMEDIATE` is where `busy_timeout` applies, so this queues
    // instead of failing.
    let concurrent = tokio::spawn({
        let pool = pool.clone();
        async move { insert_library_path(&pool, "/media/concurrent").await }
    });

    sqlx::query(r#"INSERT INTO "LibraryPath" ("Path", "LibraryId") VALUES ('/media/tx', 1)"#)
        .execute(&mut *tx)
        .await
        .expect("a write transaction that already holds the lock can write");
    tx.commit().await.expect("commit write transaction");

    tokio::time::timeout(Duration::from_secs(30), concurrent)
        .await
        .expect("the queued writer must complete once the lock is released")
        .expect("queued writer task");

    assert_eq!(count_library_paths(&pool).await, 2);
}

/// Two overlapping write transactions both commit; neither is abandoned.
#[tokio::test]
async fn concurrent_begin_write_transactions_serialise_instead_of_failing() {
    let (_directory, pool) = contended_db().await;

    let first = tokio::spawn({
        let pool = pool.clone();
        async move {
            let mut tx = begin_write(&pool).await.expect("begin first transaction");
            let _existing: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryPath""#)
                .fetch_one(&mut *tx)
                .await
                .expect("first read snapshot");
            insert_library_path(&mut *tx, "/media/first").await;
            tx.commit().await.expect("commit first transaction");
        }
    });
    let second = tokio::spawn({
        let pool = pool.clone();
        async move {
            let mut tx = begin_write(&pool).await.expect("begin second transaction");
            let _existing: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryPath""#)
                .fetch_one(&mut *tx)
                .await
                .expect("second read snapshot");
            insert_library_path(&mut *tx, "/media/second").await;
            tx.commit().await.expect("commit second transaction");
        }
    });

    tokio::time::timeout(Duration::from_secs(30), async {
        first.await.expect("first task");
        second.await.expect("second task");
    })
    .await
    .expect("both write transactions must complete");

    assert_eq!(count_library_paths(&pool).await, 2);
}
