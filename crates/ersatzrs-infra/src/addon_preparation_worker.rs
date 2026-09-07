//! Private preparation-worker lifetime and exact-attempt cleanup evidence (ADR 0086).

use std::{fs::File, path::Path, time::Duration};

use ersatzrs_core::repositories::{RepositoryError, Result};
use serde::{Deserialize, Serialize};

use crate::{
    addon_media_list_artifacts::AddonMediaListArtifactStore,
    addon_media_list_item_coordinator::AddonMediaListItemCoordinator,
    addon_media_list_item_jobs::{AddonMediaListItemJobQueue, ClaimedItemOperation},
    db::DbPool,
    repositories::SqlxResultExt as _,
};

const CHECKPOINT_KEY: &str = "addon.preparation.worker";
const UNAVAILABLE_KEY: &str = "addon.preparation.worker-unavailable";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttemptIdentity {
    pub(crate) job_id: i64,
    pub(crate) generation: i64,
    pub(crate) lease_owner: String,
}

impl From<&ClaimedItemOperation> for AttemptIdentity {
    fn from(job: &ClaimedItemOperation) -> Self {
        Self {
            job_id: job.id,
            generation: job.generation,
            lease_owner: job.lease_owner.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Phase {
    Idle,
    Running,
    CleanupConfirmed,
    Quarantined,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    version: u8,
    epoch: String,
    phase: Phase,
    attempt: Option<AttemptIdentity>,
}

pub(crate) struct WorkerSession {
    pool: DbPool,
    epoch: String,
    _lock: File,
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "preparation worker",
        "preparation process recovery is required",
    )
}

async fn read_checkpoint<'e>(
    executor: impl sqlx::Executor<'e, Database = crate::db::Db>,
) -> Result<Option<Checkpoint>> {
    let value: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"=?"#)
            .bind(CHECKPOINT_KEY)
            .fetch_optional(executor)
            .await
            .repository()?;
    value
        .map(|value| {
            if value.len() > 2048 {
                return Err(unavailable());
            }
            let checkpoint: Checkpoint = serde_json::from_str(&value).map_err(|_| unavailable())?;
            if checkpoint.version != 1
                || uuid::Uuid::parse_str(&checkpoint.epoch).is_err()
                || checkpoint.attempt.as_ref().is_some_and(|attempt| {
                    attempt.job_id <= 0
                        || attempt.generation <= 0
                        || uuid::Uuid::parse_str(&attempt.lease_owner).is_err()
                })
                || (checkpoint.phase == Phase::Idle) != checkpoint.attempt.is_none()
            {
                return Err(unavailable());
            }
            Ok(checkpoint)
        })
        .transpose()
}

async fn write_checkpoint(pool: &DbPool, checkpoint: &Checkpoint) -> Result<()> {
    let value = serde_json::to_string(checkpoint).map_err(|_| unavailable())?;
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,?)
        ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
    )
    .bind(CHECKPOINT_KEY)
    .bind(value)
    .execute(pool)
    .await
    .repository()?;
    Ok(())
}

fn try_lock(root: &Path) -> Result<Option<File>> {
    std::fs::create_dir_all(root).map_err(|_| unavailable())?;
    let path = root.join(".preparation-worker.lock");
    if std::fs::symlink_metadata(&path)
        .is_ok_and(|metadata| metadata.is_symlink() || !metadata.is_file())
    {
        return Err(unavailable());
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|_| unavailable())?;
    match fs4::FileExt::try_lock(&file) {
        Ok(()) => Ok(Some(file)),
        Err(fs4::TryLockError::WouldBlock) => Ok(None),
        Err(_) => Err(unavailable()),
    }
}

/// Run file-writing item work against an already-initialized database.
/// Sending or dropping `shutdown` requests an awaited process-tree drain.
///
/// # Errors
/// Returns a static recovery failure when ownership or cleanup cannot be proven.
pub async fn run_preparation_worker(
    pool: DbPool,
    root: &Path,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    let lock = loop {
        if let Some(lock) = try_lock(root)? {
            break lock;
        }
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            () = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    };
    let session = WorkerSession {
        pool: pool.clone(),
        epoch: uuid::Uuid::new_v4().to_string(),
        _lock: lock,
    };
    if let Some(mut checkpoint) = read_checkpoint(&pool).await? {
        match checkpoint.phase {
            Phase::Running => {
                checkpoint.phase = Phase::Quarantined;
                write_checkpoint(&pool, &checkpoint).await?;
                return Err(unavailable());
            }
            Phase::Quarantined => return Err(unavailable()),
            Phase::CleanupConfirmed => {
                session
                    .reconcile(root, checkpoint.attempt.as_ref().ok_or_else(unavailable)?)
                    .await?;
            }
            Phase::Idle => {}
        }
    }
    session.record(Phase::Idle, None).await?;
    set_worker_unavailable(&pool, false).await?;
    let coordinator = AddonMediaListItemCoordinator::new(pool);
    let mut subtitle_turn = false;
    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => return Ok(()),
            () = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
        let (cancel, cancellation) = tokio::sync::oneshot::channel();
        subtitle_turn = !subtitle_turn;
        let operation = async {
            if subtitle_turn {
                coordinator
                    .process_one_subtitles_supervised(root, cancellation, &session)
                    .await
            } else {
                coordinator
                    .process_one_preparation_supervised(root, cancellation, &session)
                    .await
            }
        };
        tokio::pin!(operation);
        tokio::select! {
            biased;
            _ = &mut shutdown => {
                let _ = cancel.send(());
                operation.await?;
                return Ok(());
            }
            result = &mut operation => {
                if result.is_err() {
                    if read_checkpoint(&session.pool).await?.is_some_and(|checkpoint| checkpoint.phase != Phase::Idle) {
                        return Err(unavailable());
                    }
                    tracing::warn!("add-on preparation worker iteration failed");
                }
            }
        }
    }
}

impl WorkerSession {
    async fn record(&self, phase: Phase, attempt: Option<AttemptIdentity>) -> Result<()> {
        write_checkpoint(
            &self.pool,
            &Checkpoint {
                version: 1,
                epoch: self.epoch.clone(),
                phase,
                attempt,
            },
        )
        .await
    }

    pub(crate) async fn begin(&self, job: &ClaimedItemOperation) -> Result<()> {
        self.record(Phase::Running, Some(job.into())).await
    }

    pub(crate) async fn finish(
        &self,
        root: &Path,
        job: &ClaimedItemOperation,
        cleanup_confirmed: bool,
    ) -> Result<()> {
        let attempt = AttemptIdentity::from(job);
        if !cleanup_confirmed {
            self.record(Phase::Quarantined, Some(attempt)).await?;
            return Err(unavailable());
        }
        // Persist proof before filesystem/SQL reconciliation; a crash can replay it.
        self.record(Phase::CleanupConfirmed, Some(attempt.clone()))
            .await?;
        self.reconcile(root, &attempt).await?;
        self.record(Phase::Idle, None).await
    }

    async fn reconcile(&self, root: &Path, attempt: &AttemptIdentity) -> Result<()> {
        AddonMediaListArtifactStore::new(self.pool.clone())
            .recover_exact_after_cleanup(root, attempt)
            .await?;
        AddonMediaListItemJobQueue::new(self.pool.clone())
            .recover_interrupted_after_cleanup(attempt, chrono::Utc::now())
            .await
    }
}

/// Record uncertain worker death only after observing exit. A newer worker's
/// held lock prevents an older parent observer from quarantining its work.
///
/// # Errors
/// Returns checkpoint/lock failures without inferring cleanup from absence.
pub async fn mark_worker_exit_uncertain(pool: &DbPool, root: &Path) -> Result<()> {
    let Some(_lock) = try_lock(root)? else {
        return Ok(());
    };
    if let Some(mut checkpoint) = read_checkpoint(pool).await? {
        if checkpoint.phase == Phase::Running {
            checkpoint.phase = Phase::Quarantined;
            write_checkpoint(pool, &checkpoint).await?;
        }
    }
    Ok(())
}

/// Report worker availability separately from unconfirmed process cleanup.
/// Only the initialized lock-owning worker clears this flag.
///
/// # Errors
/// Returns a database failure when availability cannot be persisted.
pub async fn set_worker_unavailable(pool: &DbPool, unavailable: bool) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,?)
        ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
    )
    .bind(UNAVAILABLE_KEY)
    .bind(if unavailable { "true" } else { "false" })
    .execute(pool)
    .await
    .repository()?;
    Ok(())
}

pub(crate) async fn worker_unavailable(pool: &DbPool) -> Result<bool> {
    let value: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"=?"#)
            .bind(UNAVAILABLE_KEY)
            .fetch_optional(pool)
            .await
            .repository()?;
    Ok(value.is_some_and(|value| value != "false"))
}

pub(crate) async fn recovery_required(pool: &DbPool) -> Result<bool> {
    let mut connection = pool.acquire().await.repository()?;
    recovery_required_on(&mut connection).await
}

pub(crate) async fn recovery_required_in_transaction(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
) -> Result<bool> {
    recovery_required_on(tx).await
}

async fn recovery_required_on(connection: &mut sqlx::SqliteConnection) -> Result<bool> {
    let interval_cleanup_unconfirmed: bool =
        sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "ConfigElement" WHERE "Key"=?)"#)
            .bind(crate::addon_media_list_interval_metadata::PROCESS_RECOVERY_KEY)
            .fetch_one(&mut *connection)
            .await
            .repository()?;
    if interval_cleanup_unconfirmed {
        return Ok(true);
    }
    // Malformed evidence and stale Running records fail closed even before a
    // replacement worker or parent observer has acquired the OS lock.
    let checkpoint = match read_checkpoint(&mut *connection).await {
        Ok(Some(checkpoint)) => checkpoint,
        Ok(None) => return Ok(false),
        Err(_) => return Ok(true),
    };
    match checkpoint.phase {
        Phase::Quarantined => Ok(true),
        Phase::Idle | Phase::CleanupConfirmed => Ok(false),
        Phase::Running => {
            let attempt = checkpoint.attempt.ok_or_else(unavailable)?;
            let live: bool = sqlx::query_scalar(
                r#"SELECT EXISTS(SELECT 1
                FROM "AddonMediaListItemOperationJob" WHERE "Id"=? AND "Kind" IN (1,3)
                AND "State"=1 AND "Generation"=? AND "LeaseOwner"=? AND "LeaseExpiresAt">?)"#,
            )
            .bind(attempt.job_id)
            .bind(attempt.generation)
            .bind(&attempt.lease_owner)
            .bind(crate::repositories::common::encode_datetime(
                chrono::Utc::now(),
            ))
            .fetch_one(connection)
            .await
            .repository()?;
            Ok(!live)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn checkpoint_pool() -> DbPool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            r#"CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY,"Value" TEXT NOT NULL);
               CREATE TABLE "AddonMediaListItemOperationJob" (
                   "Id" INTEGER PRIMARY KEY,"Kind" INTEGER,"State" INTEGER,
                   "Generation" INTEGER,"LeaseOwner" TEXT,"LeaseExpiresAt" TEXT);"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn worker_checkpoint_distinguishes_active_quarantine_and_availability() {
        let pool = checkpoint_pool().await;
        let root = tempfile::tempdir().unwrap();
        let mut checkpoint = Checkpoint {
            version: 1,
            epoch: uuid::Uuid::new_v4().to_string(),
            phase: Phase::Running,
            attempt: Some(AttemptIdentity {
                job_id: 1,
                generation: 1,
                lease_owner: uuid::Uuid::new_v4().to_string(),
            }),
        };
        sqlx::query(r#"INSERT INTO "AddonMediaListItemOperationJob" VALUES (1,1,1,1,?,'2999-01-01 00:00:00')"#)
            .bind(&checkpoint.attempt.as_ref().unwrap().lease_owner).execute(&pool).await.unwrap();
        write_checkpoint(&pool, &checkpoint).await.unwrap();
        assert!(!recovery_required(&pool).await.unwrap());
        let lock = try_lock(root.path()).unwrap().unwrap();
        mark_worker_exit_uncertain(&pool, root.path())
            .await
            .unwrap();
        assert!(
            !recovery_required(&pool).await.unwrap(),
            "old observer cannot quarantine a lock-owning worker"
        );
        drop(lock);
        mark_worker_exit_uncertain(&pool, root.path())
            .await
            .unwrap();
        assert!(recovery_required(&pool).await.unwrap());
        checkpoint.phase = Phase::CleanupConfirmed;
        write_checkpoint(&pool, &checkpoint).await.unwrap();
        mark_worker_exit_uncertain(&pool, root.path())
            .await
            .unwrap();
        assert_eq!(
            read_checkpoint(&pool).await.unwrap().unwrap().phase,
            Phase::CleanupConfirmed
        );
        set_worker_unavailable(&pool, true).await.unwrap();
        assert!(worker_unavailable(&pool).await.unwrap());
        assert!(
            !recovery_required(&pool).await.unwrap(),
            "availability is not cleanup quarantine"
        );
    }

    #[tokio::test]
    async fn worker_checkpoint_rejects_malformed_evidence_without_disclosing_it() {
        let pool = checkpoint_pool().await;
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,?)"#)
            .bind(CHECKPOINT_KEY)
            .bind("private-invalid-provider-input")
            .execute(&pool)
            .await
            .unwrap();
        assert!(recovery_required(&pool).await.unwrap());
        assert!(!read_checkpoint(&pool)
            .await
            .unwrap_err()
            .to_string()
            .contains("private-invalid"));
    }

    #[tokio::test]
    async fn worker_checkpoint_restart_never_treats_an_unlocked_running_record_as_cleanup() {
        let pool = checkpoint_pool().await;
        let root = tempfile::tempdir().unwrap();
        let retained = root.path().join("retained.part");
        std::fs::write(&retained, b"unproved predecessor").unwrap();
        write_checkpoint(
            &pool,
            &Checkpoint {
                version: 1,
                epoch: uuid::Uuid::new_v4().to_string(),
                phase: Phase::Running,
                attempt: Some(AttemptIdentity {
                    job_id: 1,
                    generation: 1,
                    lease_owner: uuid::Uuid::new_v4().to_string(),
                }),
            },
        )
        .await
        .unwrap();
        let (_sender, cancellation) = tokio::sync::oneshot::channel();
        assert!(
            run_preparation_worker(pool.clone(), root.path(), cancellation)
                .await
                .is_err()
        );
        assert_eq!(
            read_checkpoint(&pool).await.unwrap().unwrap().phase,
            Phase::Quarantined
        );
        assert_eq!(std::fs::read(retained).unwrap(), b"unproved predecessor");
    }

    #[tokio::test]
    async fn worker_checkpoint_running_requires_exact_live_ownership_before_admission() {
        let pool = checkpoint_pool().await;
        let checkpoint = Checkpoint {
            version: 1,
            epoch: uuid::Uuid::new_v4().to_string(),
            phase: Phase::Running,
            attempt: Some(AttemptIdentity {
                job_id: 1,
                generation: 1,
                lease_owner: uuid::Uuid::new_v4().to_string(),
            }),
        };
        write_checkpoint(&pool, &checkpoint).await.unwrap();
        assert!(
            recovery_required(&pool).await.unwrap(),
            "missing claim blocks without a parent observer"
        );
        sqlx::query(r#"INSERT INTO "AddonMediaListItemOperationJob" VALUES (1,1,1,1,?,'2999-01-01 00:00:00')"#)
            .bind(&checkpoint.attempt.as_ref().unwrap().lease_owner).execute(&pool).await.unwrap();
        assert!(!recovery_required(&pool).await.unwrap());
        for update in [
            r#"UPDATE "AddonMediaListItemOperationJob" SET "Generation"=2"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "Generation"=1,"State"=4"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=1,"LeaseExpiresAt"='2000-01-01 00:00:00'"#,
            r#"UPDATE "AddonMediaListItemOperationJob" SET "LeaseExpiresAt"='2999-01-01 00:00:00',"LeaseOwner"='other'"#,
        ] {
            sqlx::query(update).execute(&pool).await.unwrap();
            let mut tx = crate::db::begin_write(&pool).await.unwrap();
            assert!(recovery_required_in_transaction(&mut tx).await.unwrap());
            tx.rollback().await.unwrap();
            assert_eq!(
                read_checkpoint(&pool).await.unwrap().unwrap().phase,
                Phase::Running,
                "expiry is an admission fence, never cleanup proof"
            );
        }
    }

    #[cfg(unix)]
    async fn claimed_fixture(root: &Path) -> (DbPool, ClaimedItemOperation) {
        let pool = crate::addon_media_list_preparation::tests::fixture(&root.join("package")).await;
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        coordinator.queue_preparation(1, 10, 0, None).await.unwrap();
        let job = coordinator
            .queue
            .claim_kind(
                chrono::Utc::now(),
                90,
                crate::addon_media_list_item_jobs::ItemOperationKind::Preparation,
            )
            .await
            .unwrap()
            .unwrap();
        (pool, job)
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // One receipt replay fixture proves exact filesystem and SQL ownership.
    async fn worker_checkpoint_receipt_replays_exact_cleanup_after_database_completion_failure() {
        let temporary = tempfile::tempdir().unwrap();
        let (pool, job) = claimed_fixture(temporary.path()).await;
        let root = temporary.path().join("artifacts");
        let session = WorkerSession {
            pool: pool.clone(),
            epoch: uuid::Uuid::new_v4().to_string(),
            _lock: try_lock(&root).unwrap().unwrap(),
        };
        let attempt = AttemptIdentity::from(&job);
        let stage = root
            .join("staging")
            .join(format!("{}-{}-{}", job.id, job.generation, job.lease_owner));
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("unfinished.part"), b"owned partial bytes").unwrap();
        std::fs::create_dir_all(root.join("objects")).unwrap();
        let ready = root.join("objects/ready.bin");
        std::fs::write(&ready, b"immutable ready bytes").unwrap();
        let other_owner = uuid::Uuid::new_v4().to_string();
        let other = root
            .join("staging")
            .join(format!("{}-{}-{other_owner}", job.id, job.generation));
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("other.part"), b"unproved other bytes").unwrap();
        for (key, state, owner) in [
            ("unfinished", 1, &job.lease_owner),
            ("ready", 2, &job.lease_owner),
            ("other", 1, &other_owner),
        ] {
            sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" (
                "AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","ReservedBytes",
                "PreparationJobId","PreparationGeneration","PreparationLeaseOwner","CreatedAt","UpdatedAt")
                VALUES (1,10,?,1,?,4096,?,?,?,'2026-09-05 00:00:00','2026-09-05 00:00:00')"#)
                .bind(key).bind(state).bind(job.id).bind(job.generation).bind(owner).execute(&pool).await.unwrap();
        }
        session.begin(&job).await.unwrap();
        // No child was spawned in this fixture, so cleanup proof is available.
        sqlx::raw_sql(r#"CREATE TRIGGER "FailWorkerCompletion" BEFORE UPDATE ON "AddonMediaListItemOperationJob"
            BEGIN SELECT RAISE(ABORT,'synthetic completion failure'); END;"#).execute(&pool).await.unwrap();
        assert!(session.finish(&root, &job, true).await.is_err());
        let receipt = read_checkpoint(&pool).await.unwrap().unwrap();
        assert_eq!(receipt.phase, Phase::CleanupConfirmed);
        assert_eq!(receipt.epoch, session.epoch);
        assert!(!stage.exists());
        assert!(other.join("other.part").exists());
        assert_eq!(std::fs::read(&ready).unwrap(), b"immutable ready bytes");
        sqlx::query(r#"DROP TRIGGER "FailWorkerCompletion""#)
            .execute(&pool)
            .await
            .unwrap();
        drop(session);
        let replacement = WorkerSession {
            pool: pool.clone(),
            epoch: uuid::Uuid::new_v4().to_string(),
            _lock: try_lock(&root).unwrap().unwrap(),
        };
        replacement.reconcile(&root, &attempt).await.unwrap();
        replacement.reconcile(&root, &attempt).await.unwrap();
        let queue = AddonMediaListItemJobQueue::new(pool.clone());
        let status = queue
            .status(job.id, chrono::Utc::now())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.state, 0);
        assert_eq!(
            status.attempt_count, 0,
            "replaying proof must restore budget only once"
        );
        let rows: Vec<(String, i32, i64)> = sqlx::query_as(
            r#"SELECT "CacheKey","State","ReservedBytes"
            FROM "AddonMediaListArtifact" ORDER BY "CacheKey""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                ("other".into(), 1, 4096),
                ("ready".into(), 2, 4096),
                ("unfinished".into(), 3, 0)
            ]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn worker_checkpoint_requeue_preserves_manual_cancel_stale_backoff_and_retry_budget() {
        let temporary = tempfile::tempdir().unwrap();
        let (pool, job) = claimed_fixture(temporary.path()).await;
        let queue = AddonMediaListItemJobQueue::new(pool.clone());
        let attempt = AttemptIdentity::from(&job);
        for state in [0, 2, 3, 4, 5] {
            sqlx::query(
                r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=?,"LeaseOwner"=?,
                "AttemptCount"=3,"NextEligibleAt"='2999-01-01 00:00:00' WHERE "Id"=?"#,
            )
            .bind(state)
            .bind(&job.lease_owner)
            .bind(job.id)
            .execute(&pool)
            .await
            .unwrap();
            queue
                .recover_interrupted_after_cleanup(&attempt, chrono::Utc::now())
                .await
                .unwrap();
            let row: (i32, i32, String) = sqlx::query_as(
                r#"SELECT "State","AttemptCount","NextEligibleAt"
                FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
            )
            .bind(job.id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(row, (state, 3, "2999-01-01 00:00:00".into()));
        }
        sqlx::query(r#"UPDATE "AddonMediaListItemOperationJob" SET "State"=1 WHERE "Id"=?"#)
            .bind(job.id)
            .execute(&pool)
            .await
            .unwrap();
        queue
            .recover_interrupted_after_cleanup(&attempt, chrono::Utc::now())
            .await
            .unwrap();
        let row: (i32, i32, String) = sqlx::query_as(
            r#"SELECT "State","AttemptCount","NextEligibleAt"
            FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
        )
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row, (0, 2, "2999-01-01 00:00:00".into()));
    }

    // Synthetic shell fixture is Unix-only; Windows job/drain evidence is native.
    #[cfg(unix)]
    #[test]
    #[ignore = "private subprocess fixture invoked by worker_checkpoint_eof_drains_and_resumes"]
    #[allow(clippy::disallowed_methods)] // Test-only parent/child handoff, not production configuration.
    fn private_worker_process() {
        use std::io::Read as _;
        let database = std::env::var("ERSATZRS_TEST_PREPARATION_DB").unwrap();
        let root =
            std::path::PathBuf::from(std::env::var_os("ERSATZRS_TEST_PREPARATION_ROOT").unwrap());
        let (cancel, cancellation) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let mut byte = [0_u8; 1];
            while std::io::stdin()
                .read(&mut byte)
                .is_ok_and(|count| count != 0)
            {}
            let _ = cancel.send(());
        });
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let pool = crate::db::lazy_sqlite_pool(&database).unwrap();
                run_preparation_worker(pool, &root, cancellation)
                    .await
                    .unwrap();
            });
    }

    #[cfg(unix)]
    fn spawn_fixture_worker(root: &Path) -> tokio::process::Child {
        use std::process::Stdio;
        tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "addon_preparation_worker::tests::private_worker_process",
                "--ignored",
                "--nocapture",
            ])
            .env(
                "ERSATZRS_TEST_PREPARATION_DB",
                format!("sqlite:{}", root.join("addons.db").display()),
            )
            .env("ERSATZRS_TEST_PREPARATION_ROOT", root.join("artifacts"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[cfg(unix)]
    async fn wait_fixture_provider(root: &Path, expected_invocations: usize) -> Vec<i32> {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let count = tokio::fs::read_to_string(root.join("worker-invocations"))
                    .await
                    .unwrap_or_default()
                    .lines()
                    .count();
                if count == expected_invocations {
                    if let Ok(pids) =
                        tokio::fs::read_to_string(root.join("worker-provider-pids")).await
                    {
                        let pids = pids
                            .split_whitespace()
                            .map(str::parse)
                            .collect::<std::result::Result<Vec<i32>, _>>()
                            .unwrap();
                        if pids.len() == 2 {
                            return pids;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn worker_checkpoint_eof_drains_and_resumes_without_double_charging() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let pool = crate::addon_media_list_preparation::tests::fixture_with_script(&root.join("package"),
            "#!/bin/sh\nset -eu\nrequest=$(cat)\ncase \"$1\" in\n runtime-info) printf '%s\\n' '{\"tool_version\":\"1.0\"}' ;;\n prepare) sleep 30 & descendant=$!; echo \"$$ $descendant\" > ../worker-provider-pids; echo prepare >> ../worker-invocations; wait \"$descendant\" ;;\n *) exit 1 ;;\nesac\n").await;
        for key in [
            ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MINIMUM_SECONDS,
            ersatzrs_domain::CONFIG_KEY_ADDON_DETAIL_WAIT_MAXIMUM_SECONDS,
        ] {
            sqlx::query(
                r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,'1')
                ON CONFLICT("Key") DO UPDATE SET "Value"='1'"#,
            )
            .bind(key)
            .execute(&pool)
            .await
            .unwrap();
        }
        let coordinator = AddonMediaListItemCoordinator::new(pool.clone());
        let job_id = coordinator.queue_preparation(1, 10, 0, None).await.unwrap();
        for invocation in 1..=2 {
            if invocation == 2 {
                // Preserve the real cross-process provider cooldown, not a reset fixture.
                tokio::time::sleep(Duration::from_millis(1200)).await;
            }
            let mut worker = spawn_fixture_worker(root);
            let pids = wait_fixture_provider(root, invocation).await;
            assert!(
                try_lock(&root.join("artifacts")).unwrap().is_none(),
                "worker owns production root lock"
            );
            assert_eq!(
                read_checkpoint(&pool).await.unwrap().unwrap().phase,
                Phase::Running
            );
            // This is the host-death signal: no kill or provider command is sent.
            drop(worker.stdin.take());
            assert!(tokio::time::timeout(Duration::from_secs(15), worker.wait())
                .await
                .unwrap()
                .unwrap()
                .success());
            for pid in pids {
                assert!(
                    matches!(
                        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
                        Err(nix::errno::Errno::ESRCH)
                    ),
                    "cleanup receipt precedes neither leader nor descendant exit"
                );
            }
            assert_eq!(
                read_checkpoint(&pool).await.unwrap().unwrap().phase,
                Phase::Idle
            );
            let status = coordinator
                .queue
                .status(job_id, chrono::Utc::now())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(status.state, 0);
            assert_eq!(status.attempt_count, 0);
            let reserved: i64 = sqlx::query_scalar(
                r#"SELECT COALESCE(SUM("ReservedBytes"),0) FROM "AddonMediaListArtifact""#,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(reserved, 0);
        }
        let generation: i64 = sqlx::query_scalar(
            r#"SELECT "Generation" FROM "AddonMediaListItemOperationJob" WHERE "Id"=?"#,
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(generation, 2);
    }
}
