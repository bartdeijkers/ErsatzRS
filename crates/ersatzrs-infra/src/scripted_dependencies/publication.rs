//! Revalidate candidate selections before taking the timeline write lock.
use super::{resolve, validate_dependency};
use crate::{
    db::{Db, DbPool},
    repositories::{common::encode_enum, SqliteSchedulingContentRepository, SqlxResultExt as _},
};
use ersatzrs_core::{
    repositories::{RepositoryError, Result},
    scheduling::ScriptedSourceDependency,
};
use ersatzrs_domain::ManagedLibraryOperationState;
use sqlx::{SqliteConnection, Transaction};

const REVISIONS: &str = r#"SELECT "LibraryId", "Revision", "UpdatedAt"
    FROM "ScriptedLibraryRevision" ORDER BY "LibraryId""#;
pub(crate) const PUBLICATION_OPERATION: &str = "scripted source publication";

pub(crate) struct PublicationFence {
    revisions: Vec<(i64, i64, String)>,
    scans: Vec<(i64, i64, i64)>,
}

async fn scan_activity(connection: &mut SqliteConnection) -> Result<Vec<(i64, i64, i64)>> {
    // Attempt counts also change when recovery reclaims an existing operation.
    // Completion/failure between validation and publication cannot hide that a
    // scan started, even when no successful-scan receipt was emitted.
    let rows: Vec<(i64, i64, i64, bool)> = sqlx::query_as(
        r#"SELECT "LibraryId", MAX("Id"), SUM("AttemptCount"),
        MAX(CASE WHEN "State" IN (?, ?) THEN 1 ELSE 0 END)
        FROM "ManagedLibraryOperation" WHERE "AttemptCount" > 0
        GROUP BY "LibraryId" ORDER BY "LibraryId""#,
    )
    .bind(encode_enum(ManagedLibraryOperationState::Running))
    .bind(encode_enum(ManagedLibraryOperationState::WaitingToRetry))
    .fetch_all(connection)
    .await
    .repository()?;
    if rows.iter().any(|row| row.3) {
        return Err(RepositoryError::unavailable(
            PUBLICATION_OPERATION,
            "A library scan or publication is still running or awaiting replay; the published Scripted timeline was not changed. Retry after that work completes",
        ));
    }
    Ok(rows
        .into_iter()
        .map(|(library, id, attempts, _)| (library, id, attempts))
        .collect())
}

fn superseded() -> RepositoryError {
    RepositoryError::unavailable(
        PUBLICATION_OPERATION,
        "Scripted source selection changed before publication; the published timeline was not changed. Retry using current sources",
    )
}

impl PublicationFence {
    pub(crate) async fn prepare(
        pool: &DbPool,
        dependencies: &[ScriptedSourceDependency],
    ) -> Result<Self> {
        // Include empty/dynamic selections: a new library may populate them.
        // Capture before the reads so any completed scan during validation is
        // detected under the write lock, even before it creates a Refresh intent.
        let (revisions, scans) = {
            let mut connection = pool.acquire().await.repository()?;
            let revisions = sqlx::query_as(REVISIONS)
                .fetch_all(&mut *connection)
                .await
                .repository()?;
            let scans = scan_activity(&mut connection).await?;
            (revisions, scans)
        };
        let repository = SqliteSchedulingContentRepository::new(pool.clone());
        for dependency in dependencies {
            validate_dependency(dependency)?;
            let (_, current) =
                resolve(&repository, &dependency.source, dependency.resolution).await?;
            if current != *dependency {
                return Err(superseded());
            }
        }
        Ok(Self { revisions, scans })
    }

    pub(crate) async fn validate(&self, tx: &mut Transaction<'_, Db>) -> Result<()> {
        let current: Vec<(i64, i64, String)> = sqlx::query_as(REVISIONS)
            .fetch_all(&mut **tx)
            .await
            .repository()?;
        if current != self.revisions || scan_activity(tx).await? != self.scans {
            return Err(superseded());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::ManagedLibraryOperationKind;
    use sqlx::Executor as _;

    #[tokio::test]
    async fn completed_scan_during_validation_is_rejected_under_write_lock() {
        let pool = DbPool::connect("sqlite::memory:").await.unwrap();
        pool.execute(r#"CREATE TABLE "ScriptedLibraryRevision" ("LibraryId" INTEGER, "Revision" INTEGER, "UpdatedAt" TEXT)"#).await.unwrap();
        pool.execute(r#"CREATE TABLE "ManagedLibraryOperation" ("Id" INTEGER, "LibraryId" INTEGER, "Kind" INTEGER, "State" INTEGER, "AttemptCount" INTEGER)"#).await.unwrap();
        let fence = PublicationFence::prepare(&pool, &[]).await.unwrap();
        pool.execute(r#"INSERT INTO "ScriptedLibraryRevision" VALUES (1, 1, 'first')"#)
            .await
            .unwrap();
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        assert!(fence.validate(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let current = PublicationFence::prepare(&pool, &[]).await.unwrap();
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        current.validate(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        // Deleting/recreating a library with the same numeric revision does
        // not recreate the old receipt identity.
        pool.execute(r#"UPDATE "ScriptedLibraryRevision" SET "UpdatedAt" = 'recreated'"#)
            .await
            .unwrap();
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        assert!(current.validate(&mut tx).await.is_err());
    }

    #[tokio::test]
    async fn failed_scan_between_validation_and_publication_is_detected() {
        let pool = DbPool::connect("sqlite::memory:").await.unwrap();
        pool.execute(r#"CREATE TABLE "ScriptedLibraryRevision" ("LibraryId" INTEGER, "Revision" INTEGER, "UpdatedAt" TEXT)"#).await.unwrap();
        pool.execute(r#"CREATE TABLE "ManagedLibraryOperation" ("Id" INTEGER, "LibraryId" INTEGER, "Kind" INTEGER, "State" INTEGER, "AttemptCount" INTEGER)"#).await.unwrap();
        let fence = PublicationFence::prepare(&pool, &[]).await.unwrap();
        sqlx::query(r#"INSERT INTO "ManagedLibraryOperation" VALUES (1, 1, ?, ?, 1)"#)
            .bind(encode_enum(ManagedLibraryOperationKind::ManualScan))
            .bind(encode_enum(ManagedLibraryOperationState::Failed))
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        assert!(fence.validate(&mut tx).await.is_err());
    }
}
