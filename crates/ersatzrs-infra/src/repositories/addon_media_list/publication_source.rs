//! Resolve retained publication input without substituting mutable draft state.

use ersatzrs_core::repositories::AddonMediaListRevisionSnapshot;
use sqlx::Transaction;

use super::{
    EntityId, PublicationFence, PublicationJournal, RepositoryError, Result,
    SqliteAddonMediaListRepository,
};
use crate::db::Db;
use crate::repositories::{load_valid_revision_snapshot_in_tx, SqlxResultExt as _};

impl SqliteAddonMediaListRepository {
    /// Read the exact active publication and its validated snapshot in one transaction.
    ///
    /// No publication, or a legacy publication without a modern snapshot marker
    /// or playback artifact bindings, returns `None`. Newer draft and retained revisions are
    /// never substituted for the list's active published revision.
    ///
    /// # Errors
    /// Refuses missing lists, corrupt snapshots, and absent snapshots with
    /// prepared media, timeline, or subtitle bindings that require exact evidence.
    pub async fn active_publication_snapshot(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListRevisionSnapshot>> {
        let mut tx = self.0.begin().await.repository()?;
        let revision: i32 =
            sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
                .bind(list_id)
                .fetch_optional(&mut *tx)
                .await
                .repository()?
                .ok_or(RepositoryError::NotFound {
                    kind: "add-on media list",
                    id: list_id,
                })?;
        if revision <= 0 {
            tx.commit().await.repository()?;
            return Ok(None);
        }
        let snapshot = load_valid_revision_snapshot_in_tx(&mut tx, list_id, revision).await?;
        if snapshot.is_none() {
            let modern: bool = sqlx::query_scalar(
                r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListRevision"
                   WHERE "AddonMediaListId"=? AND "Revision"=? AND "SnapshotVersion" IS NOT NULL)"#,
            )
            .bind(list_id)
            .bind(revision)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if modern {
                return Err(invalid());
            }
            let bound: bool = sqlx::query_scalar(
                r#"SELECT EXISTS(
                    SELECT 1 FROM "AddonMediaListRevisionItem" AS "item"
                    JOIN "AddonMediaListRevision" AS "revision"
                      ON "revision"."Id"="item"."AddonMediaListRevisionId"
                    WHERE "revision"."AddonMediaListId"=? AND "revision"."Revision"=?
                      AND ("item"."PreparedArtifactId" IS NOT NULL
                           OR "item"."PlaybackTimelineJson" IS NOT NULL
                           OR "item"."SubtitleArtifactIdsJson" <> '[]'))"#,
            )
            .bind(list_id)
            .bind(revision)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if bound {
                return Err(invalid());
            }
        }
        tx.commit().await.repository()?;
        Ok(snapshot)
    }

    /// Read the digest-validated source snapshot of the active publication.
    ///
    /// Ordinary draft publications and completely unfenced legacy journals have
    /// no retained source. Journal, exact job scope and snapshot are read in one
    /// transaction. This does not materialize the source as the current draft.
    ///
    /// # Errors
    /// Refuses malformed/partial active journals, missing or mismatched jobs,
    /// and missing or corrupt referenced snapshots; never falls back to draft.
    pub async fn publication_source_snapshot(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListRevisionSnapshot>> {
        let mut tx = self.0.begin().await.repository()?;
        let journal: Option<Option<String>> = sqlx::query_scalar(
            r#"SELECT "PublicationJournalJson" FROM "AddonMediaList"
               WHERE "Id"=? AND "ReviewState"=3"#,
        )
        .bind(list_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let Some(journal) = journal else {
            tx.commit().await.repository()?;
            return Ok(None);
        };
        let journal: PublicationJournal =
            serde_json::from_str(&journal.ok_or_else(invalid)?).map_err(|_| invalid())?;
        let snapshot = match (
            journal.operation_id,
            journal.generation,
            journal.saved_revision,
        ) {
            (None, None, None) => None,
            (Some(operation_id), Some(generation), Some(saved_revision)) => {
                source_in_tx(&mut tx, list_id, operation_id, generation, saved_revision).await?
            }
            _ => return Err(invalid()),
        };
        tx.commit().await.repository()?;
        Ok(snapshot)
    }

    /// Resolve the exact job source before its journal has been written.
    /// The staging caller has already validated the live lease and draft fence.
    pub(super) async fn source_snapshot_for_fence(
        &self,
        list_id: EntityId,
        fence: PublicationFence<'_>,
    ) -> Result<Option<AddonMediaListRevisionSnapshot>> {
        let mut tx = self.0.begin().await.repository()?;
        let snapshot = source_in_tx(
            &mut tx,
            list_id,
            fence.operation_id,
            fence.generation,
            fence.saved_revision,
        )
        .await?;
        tx.commit().await.repository()?;
        Ok(snapshot)
    }
}

async fn source_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    operation_id: EntityId,
    generation: i32,
    saved_revision: i32,
) -> Result<Option<AddonMediaListRevisionSnapshot>> {
    if list_id <= 0 || operation_id <= 0 || generation < 0 || saved_revision < 0 {
        return Err(invalid());
    }
    let source: Option<i32> = sqlx::query_scalar(
        r#"SELECT "SourcePublishedRevision" FROM "AddonMediaListPublicationJob"
           WHERE "AddonMediaListId"=? AND "OperationId"=? AND "Generation"=? AND "SavedRevision"=?"#,
    )
    .bind(list_id)
    .bind(operation_id)
    .bind(generation)
    .bind(saved_revision)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(invalid)?;
    let Some(revision) = source else {
        return Ok(None);
    };
    if revision <= 0 {
        return Err(invalid());
    }
    load_valid_revision_snapshot_in_tx(tx, list_id, revision)
        .await?
        .map(Some)
        .ok_or_else(invalid)
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict(
        "the retained publication source is unavailable or inconsistent".into(),
    )
}

#[cfg(test)]
mod tests;
