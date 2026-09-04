use std::path::Path;

use ersatzrs_core::repositories::{
    AddonMediaListLifecycleJob, ManagedLibraryOperationClaim, RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListDetailState, AddonMediaListImportState,
    AddonMediaListLifecycleJobKind, AddonMediaListLifecyclePhase, AddonMediaListPresentationKind,
    AddonMediaListReviewState, EntityId,
};
use sqlx::Row as _;

use crate::addon_media_list_import::AddonMediaListImportCoordinator;
use crate::addon_media_list_sync::AddonMediaListSynchronizer;
use crate::db::DbPool;
use crate::repositories::addon_publication_fs::{
    self, PublicationFsIdentity, PublicationFsPaths, PublicationTreeRole,
};
use crate::repositories::common::{encode_datetime, encode_enum};
use crate::repositories::SqlxResultExt as _;

#[derive(Clone, Copy)]
struct ResetScratchIdentity {
    operation_id: EntityId,
    generation: i32,
    saved_published_revision: i32,
}

pub(crate) async fn apply_phase(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
    phase: AddonMediaListLifecyclePhase,
) -> Result<()> {
    validate_identity(claim, job, phase)?;
    match phase {
        AddonMediaListLifecyclePhase::Quiescing => quiesce(pool, claim, job).await,
        AddonMediaListLifecyclePhase::UnpublishingFiles => {
            replace_files_with_empty_candidate(pool, claim, job).await
        }
        AddonMediaListLifecyclePhase::UnpublishingProjection => {
            unpublish_projection(pool, claim, job).await
        }
        AddonMediaListLifecyclePhase::ClearingProviderState => {
            clear_provider_state(pool, claim, job).await
        }
        AddonMediaListLifecyclePhase::ClearingCheckpoint => {
            clear_checkpoints(pool, claim, job).await
        }
        AddonMediaListLifecyclePhase::VerifyingEmpty => verify_empty(pool, claim, job).await,
        _ => Err(RepositoryError::Conflict(
            "the reset phase has no storage effect".to_owned(),
        )),
    }
}

pub(crate) async fn queue_initial_import(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
) -> Result<EntityId> {
    validate_identity(
        claim,
        job,
        AddonMediaListLifecyclePhase::QueueingInitialImport,
    )?;
    ensure_current(
        pool,
        claim,
        job,
        AddonMediaListLifecyclePhase::QueueingInitialImport,
    )
    .await?;
    AddonMediaListImportCoordinator::new(pool.clone())
        .queue_reset_reload(
            job.list_id,
            claim.operation.id,
            &claim.operation.owner_token,
            &claim.lease_owner,
        )
        .await
}

fn validate_identity(
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
    phase: AddonMediaListLifecyclePhase,
) -> Result<()> {
    if job.operation_id != claim.operation.id
        || job.kind != AddonMediaListLifecycleJobKind::ResetReload
        || job.phase != phase
    {
        return Err(RepositoryError::Conflict(
            "the reset identity or phase changed".to_owned(),
        ));
    }
    Ok(())
}

async fn ensure_current(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
    phase: AddonMediaListLifecyclePhase,
) -> Result<()> {
    let now = encode_datetime(chrono::Utc::now());
    let current: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1 FROM "AddonMediaListLifecycleJob" AS "job"
               JOIN "ManagedLibraryLease" AS "lease"
                 ON "lease"."OperationId" = "job"."OperationId"
               JOIN "AddonMediaList" AS "list"
                 ON "list"."Id" = "job"."AddonMediaListId"
               WHERE "job"."Id" = ? AND "job"."OperationId" = ?
                 AND "job"."AddonMediaListId" = ? AND "job"."Kind" = ?
                 AND "job"."Phase" = ? AND "job"."SavedDraftRevision" = "list"."DraftRevision"
                 AND "job"."SavedPublishedRevision" = "list"."PublishedRevision"
                 AND "lease"."OwnerToken" = ? AND "lease"."LeaseOwner" = ?
                 AND "lease"."LeaseExpiresAt" > ?
           )"#,
    )
    .bind(job.id)
    .bind(claim.operation.id)
    .bind(job.list_id)
    .bind(encode_enum(AddonMediaListLifecycleJobKind::ResetReload))
    .bind(encode_enum(phase))
    .bind(&claim.operation.owner_token)
    .bind(&claim.lease_owner)
    .bind(&now)
    .fetch_one(pool)
    .await
    .repository()?;
    if !current {
        return Err(RepositoryError::Conflict(
            "the reset lease, phase, or revision fence changed".to_owned(),
        ));
    }
    Ok(())
}

async fn quiesce(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
) -> Result<()> {
    let now = encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_current_tx(
        &mut tx,
        claim,
        job,
        AddonMediaListLifecyclePhase::Quiescing,
        &now,
    )
    .await?;
    sqlx::query(
        r#"UPDATE "AddonMediaListImportJob"
           SET "State" = ?, "CancellationRequested" = 1, "LeaseOwner" = NULL,
               "LeaseExpiresAt" = NULL, "UpdatedAt" = ?
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(encode_enum(AddonMediaListImportState::Superseded))
    .bind(&now)
    .bind(job.list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    sqlx::query(
        r#"UPDATE "AddonMediaListDetailJob"
           SET "State" = ?, "ResultRecordJson" = NULL, "Outcome" = NULL,
               "NextEligibleAt" = NULL, "LeaseOwner" = NULL, "LeaseExpiresAt" = NULL,
               "Diagnostic" = 'reset_quiesced', "UpdatedAt" = ?
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(encode_enum(AddonMediaListDetailState::Failed))
    .bind(&now)
    .bind(job.list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()
}

async fn replace_files_with_empty_candidate(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
) -> Result<()> {
    ensure_current(
        pool,
        claim,
        job,
        AddonMediaListLifecyclePhase::UnpublishingFiles,
    )
    .await?;
    let (paths, identity) = reset_paths(pool, job).await?;
    let prepare_paths = paths.clone();
    tokio::task::spawn_blocking(move || {
        addon_publication_fs::prepare_empty_managed_candidate(&prepare_paths, identity)
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "empty managed add-on media-list files",
            format!("reset file task failed: {error}"),
        )
    })?
    .map_err(|error| reset_fs_error("prepare managed add-on media-list reset", &error))?;
    ensure_current(
        pool,
        claim,
        job,
        AddonMediaListLifecyclePhase::UnpublishingFiles,
    )
    .await?;
    tokio::task::spawn_blocking(move || {
        addon_publication_fs::empty_prepared_managed_candidate(&paths, identity)
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "empty managed add-on media-list files",
            format!("reset file task failed: {error}"),
        )
    })?
    .map_err(|error| reset_fs_error("empty managed add-on media-list files", &error))?;
    ensure_current(
        pool,
        claim,
        job,
        AddonMediaListLifecyclePhase::UnpublishingFiles,
    )
    .await
}

async fn unpublish_projection(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
) -> Result<()> {
    ensure_current(
        pool,
        claim,
        job,
        AddonMediaListLifecyclePhase::UnpublishingProjection,
    )
    .await?;
    AddonMediaListSynchronizer::new(pool.clone())
        .empty_generated_playlist_for_reset(job.list_id)
        .await?;
    let now = encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_current_tx(
        &mut tx,
        claim,
        job,
        AddonMediaListLifecyclePhase::UnpublishingProjection,
        &now,
    )
    .await?;
    sqlx::query(
        r#"DELETE FROM "MediaItem" WHERE "LibraryPathId" IN (
               SELECT "path"."Id" FROM "LibraryPath" AS "path"
               JOIN "AddonMediaListLibrary" AS "binding"
                 ON "binding"."LibraryId" = "path"."LibraryId"
               WHERE "binding"."AddonMediaListId" = ?
           )"#,
    )
    .bind(job.list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()
}

async fn clear_provider_state(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
) -> Result<()> {
    let now = encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_current_tx(
        &mut tx,
        claim,
        job,
        AddonMediaListLifecyclePhase::ClearingProviderState,
        &now,
    )
    .await?;
    for table in [
        "AddonMediaListArtworkMaterialization",
        "AddonMediaListArtworkUpload",
        "AddonMediaListTelevisionShow",
        "AddonMediaListArtist",
    ] {
        let sql = format!(r#"DELETE FROM "{table}" WHERE "AddonMediaListId" = ?"#);
        sqlx::query(crate::db::assert_sql_safe(&sql))
            .bind(job.list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?"#)
        .bind(job.list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "Description" = NULL, "ItemCount" = 0, "LastUpdate" = NULL,
               "LastMatch" = NULL, "LastError" = NULL, "ContentKindOverride" = ?,
               "ChapterInput" = NULL, "ProviderMetadataJson" = '{}',
               "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL,
               "PresentationKind" = ?, "RequestedPresentationKind" = ?,
               "PresentationError" = NULL, "ReviewState" = ?, "ReviewError" = NULL,
               "DetailImportAuthorizedAt" = NULL, "DetailProjectionPendingAt" = NULL,
               "PublicationJournalJson" = NULL, "PublicationStartedAt" = NULL,
               "NextRefreshAt" = NULL, "RefreshRetryAt" = NULL,
               "LastRefreshSucceededAt" = NULL
           WHERE "Id" = ? AND "DraftRevision" = ? AND "PublishedRevision" = ?"#,
    )
    .bind(encode_enum(AddonMediaListContentKind::Auto))
    .bind(encode_enum(AddonMediaListPresentationKind::RemoteStreams))
    .bind(encode_enum(AddonMediaListPresentationKind::Auto))
    .bind(encode_enum(AddonMediaListReviewState::Draft))
    .bind(job.list_id)
    .bind(job.saved_draft_revision)
    .bind(job.saved_published_revision)
    .execute(&mut *tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(RepositoryError::Conflict(
            "the add-on media-list revision changed during reset".to_owned(),
        ));
    }
    tx.commit().await.repository()
}

#[allow(clippy::too_many_lines)] // Keep the restart-safe DB/file/DB cleanup sequence together.
async fn clear_checkpoints(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
) -> Result<()> {
    let now = encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_current_tx(
        &mut tx,
        claim,
        job,
        AddonMediaListLifecyclePhase::ClearingCheckpoint,
        &now,
    )
    .await?;
    let older_reset_scratch = sqlx::query(
        r#"SELECT "OperationId", "Generation", "SavedPublishedRevision"
           FROM "AddonMediaListLifecycleJob"
           WHERE "AddonMediaListId" = ? AND "Kind" = ? AND "OperationId" <> ?
           ORDER BY "Generation", "Id""#,
    )
    .bind(job.list_id)
    .bind(encode_enum(AddonMediaListLifecycleJobKind::ResetReload))
    .bind(claim.operation.id)
    .fetch_all(&mut *tx)
    .await
    .repository()?
    .iter()
    .map(|row| {
        Ok(ResetScratchIdentity {
            operation_id: row.try_get("OperationId").repository()?,
            generation: row.try_get("Generation").repository()?,
            saved_published_revision: row.try_get("SavedPublishedRevision").repository()?,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    sqlx::query(
        r#"DELETE FROM "AddonMediaListRevisionArtifact" WHERE "AddonMediaListRevisionId" IN (
               SELECT "Id" FROM "AddonMediaListRevision" WHERE "AddonMediaListId" = ?
           )"#,
    )
    .bind(job.list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    for table in ["AddonMediaListRevision", "AddonMediaListArtifact"] {
        let sql = format!(r#"DELETE FROM "{table}" WHERE "AddonMediaListId" = ?"#);
        sqlx::query(crate::db::assert_sql_safe(&sql))
            .bind(job.list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(
        r#"UPDATE "AddonMediaListImportJob" SET "CheckpointId" = NULL
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(job.list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    sqlx::query(r#"DELETE FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ?"#)
        .bind(job.list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    sqlx::query(r#"DELETE FROM "AddonMediaListImportCheckpoint" WHERE "AddonMediaListId" = ?"#)
        .bind(job.list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    sqlx::query(
        r#"DELETE FROM "ManagedLibraryOperation" WHERE "Id" IN (
               SELECT "OperationId" FROM "AddonMediaListPublicationJob"
               WHERE "AddonMediaListId" = ?
           ) AND "Id" <> ?"#,
    )
    .bind(job.list_id)
    .bind(claim.operation.id)
    .execute(&mut *tx)
    .await
    .repository()?;
    sqlx::query(r#"DELETE FROM "ManagedLibraryOperation" WHERE "ParentOperationId" = ?"#)
        .bind(claim.operation.id)
        .execute(&mut *tx)
        .await
        .repository()?;
    sqlx::query(
        r#"DELETE FROM "ScanRefreshResolution" WHERE "LibraryId" = (
               SELECT "LibraryId" FROM "AddonMediaListLibrary" WHERE "AddonMediaListId" = ?
           )"#,
    )
    .bind(job.list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;

    let (paths, identity) = reset_paths(pool, job).await?;
    let managed = paths.managed.clone();
    tokio::task::spawn_blocking(move || {
        addon_publication_fs::cleanup_after_verification(
            &paths,
            identity,
            PublicationTreeRole::Candidate,
        )?;
        for scratch in older_reset_scratch {
            let identity = PublicationFsIdentity {
                list_id: identity.list_id,
                operation_id: Some(scratch.operation_id),
                generation: Some(scratch.generation),
                previous_revision: scratch.saved_published_revision,
                target_revision: scratch.saved_published_revision.saturating_add(1),
            };
            let paths = addon_publication_fs::publication_paths(
                managed.clone(),
                identity.list_id,
                identity.operation_id,
                identity.generation,
            )?;
            addon_publication_fs::cleanup_reset_scratch(&paths, identity)?;
        }
        Ok::<_, addon_publication_fs::PublicationFsError>(())
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "finalize add-on media-list reset files",
            format!("reset cleanup task failed: {error}"),
        )
    })?
    .map_err(|error| reset_fs_error("finalize add-on media-list reset files", &error))?;

    let now = encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    ensure_current_tx(
        &mut tx,
        claim,
        job,
        AddonMediaListLifecyclePhase::ClearingCheckpoint,
        &now,
    )
    .await?;
    sqlx::query(
        r#"DELETE FROM "ManagedLibraryOperation" WHERE "Id" IN (
               SELECT "OperationId" FROM "AddonMediaListLifecycleJob"
               WHERE "AddonMediaListId" = ? AND "OperationId" <> ?
           )"#,
    )
    .bind(job.list_id)
    .bind(claim.operation.id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()
}

async fn verify_empty(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
) -> Result<()> {
    ensure_current(
        pool,
        claim,
        job,
        AddonMediaListLifecyclePhase::VerifyingEmpty,
    )
    .await?;
    let row = sqlx::query(
        r#"SELECT "list"."DraftRevision", "list"."PublishedRevision",
                  CASE WHEN "list"."PlaylistId" IS NULL THEN 1
                       ELSE EXISTS (SELECT 1 FROM "Playlist" WHERE "Id" = "list"."PlaylistId") END
                       AS "PlaylistRetained",
                  (SELECT COUNT(*) FROM "PlaylistItem"
                   WHERE "PlaylistId" = "list"."PlaylistId") AS "PlaylistItems",
                  (SELECT COUNT(*) FROM "AddonMediaListItem"
                   WHERE "AddonMediaListId" = "list"."Id") AS "Items",
                  (SELECT COUNT(*) FROM "AddonMediaListRevision"
                   WHERE "AddonMediaListId" = "list"."Id") AS "Revisions",
                  (SELECT COUNT(*) FROM "AddonMediaListArtifact"
                   WHERE "AddonMediaListId" = "list"."Id") AS "Artifacts",
                  (SELECT COUNT(*) FROM "AddonMediaListImportCheckpoint"
                   WHERE "AddonMediaListId" = "list"."Id") AS "Checkpoints",
                  (SELECT COUNT(*) FROM "AddonMediaListImportJob"
                   WHERE "AddonMediaListId" = "list"."Id") AS "Imports",
                  (SELECT COUNT(*) FROM "MediaItem" AS "media"
                   JOIN "LibraryPath" AS "path" ON "path"."Id" = "media"."LibraryPathId"
                   JOIN "AddonMediaListLibrary" AS "binding"
                     ON "binding"."LibraryId" = "path"."LibraryId"
                   WHERE "binding"."AddonMediaListId" = "list"."Id") AS "ProjectedMedia"
           FROM "AddonMediaList" AS "list" WHERE "list"."Id" = ?"#,
    )
    .bind(job.list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: job.list_id,
    })?;
    let revision_matches = row.try_get::<i32, _>("DraftRevision").repository()?
        == job.saved_draft_revision
        && row.try_get::<i32, _>("PublishedRevision").repository()? == job.saved_published_revision;
    let retained = row.try_get::<bool, _>("PlaylistRetained").repository()?;
    let counts = [
        "PlaylistItems",
        "Items",
        "Revisions",
        "Artifacts",
        "Checkpoints",
        "Imports",
        "ProjectedMedia",
    ];
    if !revision_matches
        || !retained
        || counts.iter().any(|column| {
            row.try_get::<i64, _>(*column)
                .map_or(true, |count| count != 0)
        })
    {
        return Err(RepositoryError::Conflict(
            "the add-on media list is not empty after reset".to_owned(),
        ));
    }
    let (paths, identity) = reset_paths(pool, job).await?;
    tokio::task::spawn_blocking(move || {
        addon_publication_fs::verify_empty_reset_candidate(&paths, identity)
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "verify add-on media-list reset files",
            format!("reset verification task failed: {error}"),
        )
    })?
    .map_err(|error| reset_fs_error("verify add-on media-list reset files", &error))
}

async fn ensure_current_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    claim: &ManagedLibraryOperationClaim,
    job: &AddonMediaListLifecycleJob,
    phase: AddonMediaListLifecyclePhase,
    now: &str,
) -> Result<()> {
    let current: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
               SELECT 1 FROM "AddonMediaListLifecycleJob" AS "job"
               JOIN "ManagedLibraryLease" AS "lease"
                 ON "lease"."OperationId" = "job"."OperationId"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "job"."AddonMediaListId"
               WHERE "job"."Id" = ? AND "job"."OperationId" = ?
                 AND "job"."AddonMediaListId" = ? AND "job"."Phase" = ?
                 AND "job"."SavedDraftRevision" = "list"."DraftRevision"
                 AND "job"."SavedPublishedRevision" = "list"."PublishedRevision"
                 AND "lease"."OwnerToken" = ? AND "lease"."LeaseOwner" = ?
                 AND "lease"."LeaseExpiresAt" > ?
           )"#,
    )
    .bind(job.id)
    .bind(claim.operation.id)
    .bind(job.list_id)
    .bind(encode_enum(phase))
    .bind(&claim.operation.owner_token)
    .bind(&claim.lease_owner)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if !current {
        return Err(RepositoryError::Conflict(
            "the reset lease, phase, or revision fence changed".to_owned(),
        ));
    }
    Ok(())
}

async fn reset_paths(
    pool: &DbPool,
    job: &AddonMediaListLifecycleJob,
) -> Result<(PublicationFsPaths, PublicationFsIdentity)> {
    let manifest: String = sqlx::query_scalar(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(job.list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Conflict("managed list storage is unavailable".to_owned()))?;
    let identity = PublicationFsIdentity {
        list_id: job.list_id,
        operation_id: Some(job.operation_id),
        generation: Some(job.generation),
        previous_revision: job.saved_published_revision,
        target_revision: job.saved_published_revision.saturating_add(1),
    };
    let paths = tokio::task::spawn_blocking(move || {
        let manifest_path = Path::new(&manifest);
        let managed_path = if manifest_path.is_dir() {
            manifest_path
        } else {
            manifest_path
                .parent()
                .ok_or(addon_publication_fs::PublicationFsError::UnsafeState(
                    "the managed manifest has no parent folder",
                ))?
        };
        let managed = managed_path.canonicalize().map_err(|source| {
            addon_publication_fs::PublicationFsError::Io {
                action: "canonicalize managed reset folder",
                source,
            }
        })?;
        addon_publication_fs::publication_paths(
            managed,
            identity.list_id,
            identity.operation_id,
            identity.generation,
        )
    })
    .await
    .map_err(|error| {
        RepositoryError::unavailable(
            "resolve add-on media-list reset files",
            format!("reset path task failed: {error}"),
        )
    })?
    .map_err(|error| reset_fs_error("resolve add-on media-list reset files", &error))?;
    Ok((paths, identity))
}

fn reset_fs_error(
    operation: &'static str,
    error: &addon_publication_fs::PublicationFsError,
) -> RepositoryError {
    RepositoryError::unavailable(operation, error.to_string())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use ersatzrs_core::repositories::{
        AddonMediaListLifecycleJob, ManagedLibraryOperation, ManagedLibraryOperationClaim,
        ManagedLibraryOperationRepository as _,
    };
    use ersatzrs_domain::{
        AddonMediaListLifecycleJobKind, AddonMediaListLifecyclePhase, ManagedLibraryOperationKind,
        ManagedLibraryOperationState,
    };

    use super::*;

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn reset_phases_are_restart_safe_and_preserve_identity_policy_and_revisions() {
        let pool = crate::db::init("sqlite::memory:")
            .await
            .expect("migrate reset database");
        let temporary = tempfile::tempdir().expect("create managed root");
        let managed = temporary.path().join("managed-list");
        std::fs::create_dir(&managed).expect("create managed list folder");
        let manifest = managed.join("playlist.json");
        std::fs::write(&manifest, b"derived").expect("seed managed definition");
        seed_reset_state(&pool, manifest.to_string_lossy().as_ref()).await;
        let claim = reset_claim();
        let mut job = reset_job(AddonMediaListLifecyclePhase::Quiescing);

        for (phase, next) in [
            (
                AddonMediaListLifecyclePhase::Quiescing,
                AddonMediaListLifecyclePhase::UnpublishingFiles,
            ),
            (
                AddonMediaListLifecyclePhase::UnpublishingFiles,
                AddonMediaListLifecyclePhase::UnpublishingProjection,
            ),
            (
                AddonMediaListLifecyclePhase::UnpublishingProjection,
                AddonMediaListLifecyclePhase::ClearingProviderState,
            ),
            (
                AddonMediaListLifecyclePhase::ClearingProviderState,
                AddonMediaListLifecyclePhase::ClearingCheckpoint,
            ),
            (
                AddonMediaListLifecyclePhase::ClearingCheckpoint,
                AddonMediaListLifecyclePhase::VerifyingEmpty,
            ),
        ] {
            apply_phase(&pool, &claim, &job, phase)
                .await
                .expect("apply reset phase");
            apply_phase(&pool, &claim, &job, phase)
                .await
                .expect("repeat reset phase");
            sqlx::query(r#"UPDATE "AddonMediaListLifecycleJob" SET "Phase" = ? WHERE "Id" = 1"#)
                .bind(encode_enum(next))
                .execute(&pool)
                .await
                .expect("advance test phase");
            job.phase = next;
        }
        apply_phase(
            &pool,
            &claim,
            &job,
            AddonMediaListLifecyclePhase::VerifyingEmpty,
        )
        .await
        .expect("verify empty reset");

        let retained = sqlx::query(
            r#"SELECT "Name", "AddonId", "ProviderId", "SourceUrl", "AutoRefresh",
                      "GeneratePlaylist", "DetailImportPolicy", "RefreshCadence",
                      "RefreshPublicationPolicy", "ProcessedArtifactByteLimit",
                      "DraftOptionValuesJson", "DraftRevision", "PublishedRevision",
                      "ItemCount", "Description", "MetadataOverrideJson",
                      "ContentKindOverride", "PresentationKind",
                      "RequestedPresentationKind", "ChapterInput", "NextRefreshAt",
                      "RefreshRetryAt", "LastRefreshSucceededAt"
               FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load retained reset state");
        assert_eq!(
            retained.try_get::<String, _>("Name").unwrap(),
            "Current Name"
        );
        assert_eq!(
            retained.try_get::<String, _>("AddonId").unwrap(),
            "org.example.reset"
        );
        assert_eq!(
            retained.try_get::<String, _>("ProviderId").unwrap(),
            "provider-list"
        );
        assert_eq!(
            retained.try_get::<String, _>("SourceUrl").unwrap(),
            "https://example.invalid/list"
        );
        assert!(retained.try_get::<bool, _>("AutoRefresh").unwrap());
        assert!(retained.try_get::<bool, _>("GeneratePlaylist").unwrap());
        assert_eq!(retained.try_get::<i32, _>("DetailImportPolicy").unwrap(), 1);
        assert_eq!(retained.try_get::<i32, _>("RefreshCadence").unwrap(), 0);
        assert_eq!(
            retained
                .try_get::<i32, _>("RefreshPublicationPolicy")
                .unwrap(),
            1
        );
        assert_eq!(
            retained
                .try_get::<i64, _>("ProcessedArtifactByteLimit")
                .unwrap(),
            4096
        );
        assert_eq!(
            retained
                .try_get::<String, _>("DraftOptionValuesJson")
                .unwrap(),
            r#"{"quality":"best"}"#
        );
        assert_eq!(retained.try_get::<i32, _>("DraftRevision").unwrap(), 7);
        assert_eq!(retained.try_get::<i32, _>("PublishedRevision").unwrap(), 4);
        assert_eq!(retained.try_get::<i32, _>("ItemCount").unwrap(), 0);
        assert!(retained
            .try_get::<Option<String>, _>("Description")
            .unwrap()
            .is_none());
        assert_eq!(
            retained
                .try_get::<String, _>("MetadataOverrideJson")
                .unwrap(),
            "{}"
        );
        assert_eq!(
            retained.try_get::<i32, _>("ContentKindOverride").unwrap(),
            0
        );
        assert_eq!(retained.try_get::<i32, _>("PresentationKind").unwrap(), 0);
        assert_eq!(
            retained
                .try_get::<i32, _>("RequestedPresentationKind")
                .unwrap(),
            2
        );
        for column in [
            "ChapterInput",
            "NextRefreshAt",
            "RefreshRetryAt",
            "LastRefreshSucceededAt",
        ] {
            assert!(retained
                .try_get::<Option<String>, _>(column)
                .unwrap()
                .is_none());
        }

        for table in [
            "AddonMediaListItem",
            "AddonMediaListImportJob",
            "AddonMediaListImportCheckpoint",
            "AddonMediaListRevision",
            "AddonMediaListArtifact",
            "MediaItem",
        ] {
            let sql = format!(r#"SELECT COUNT(*) FROM "{table}""#);
            let count: i64 = sqlx::query_scalar(crate::db::assert_sql_safe(&sql))
                .fetch_one(&pool)
                .await
                .expect("count cleared reset state");
            assert_eq!(count, 0, "{table} must be empty");
        }
        assert!(managed
            .join(addon_publication_fs::PUBLICATION_MARKER_FILE)
            .is_file());
        assert_eq!(std::fs::read_dir(&managed).unwrap().count(), 1);
        let active_job_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListLifecycleJob" WHERE "Id" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("count active reset job");
        assert_eq!(active_job_count, 1);

        let too_early = crate::addon_media_list_import::persist_reset_reload_import(
            &pool,
            1,
            1,
            "reset-owner",
            "reset-lease",
            "media.list.record.v1",
        )
        .await;
        assert!(matches!(too_early, Err(RepositoryError::Conflict(_))));
        sqlx::query(r#"UPDATE "AddonMediaListLifecycleJob" SET "Phase" = ? WHERE "Id" = 1"#)
            .bind(encode_enum(
                AddonMediaListLifecyclePhase::QueueingInitialImport,
            ))
            .execute(&pool)
            .await
            .expect("enter queueing phase");
        let queued = crate::addon_media_list_import::persist_reset_reload_import(
            &pool,
            1,
            1,
            "reset-owner",
            "reset-lease",
            "media.list.record.v1",
        )
        .await
        .expect("queue reset import after empty commit");
        let duplicate = crate::addon_media_list_import::persist_reset_reload_import(
            &pool,
            1,
            1,
            "reset-owner",
            "reset-lease",
            "media.list.record.v1",
        )
        .await
        .expect("reuse queued reset import");
        assert_eq!(queued, duplicate);
        let lifecycle: (i32, i32, i32) = sqlx::query_as(
            r#"SELECT "Generation", "Purpose", "Mode" FROM "AddonMediaListImportJob"
               WHERE "Id" = ?"#,
        )
        .bind(queued)
        .fetch_one(&pool)
        .await
        .expect("load reset import lifecycle");
        assert_eq!(lifecycle, (1, 2, 0));
        let repository = crate::repositories::SqliteManagedLibraryOperationRepository(pool.clone());
        assert!(repository
            .complete_reset_reload(&claim, queued)
            .await
            .expect("complete reset operation"));
        let terminal: (i32, Option<EntityId>, i32) = sqlx::query_as(
            r#"SELECT "job"."Phase", "job"."CandidateImportJobId", "operation"."State"
               FROM "AddonMediaListLifecycleJob" AS "job"
               JOIN "ManagedLibraryOperation" AS "operation"
                 ON "operation"."Id" = "job"."OperationId"
               WHERE "job"."Id" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("load terminal reset state");
        assert_eq!(terminal, (10, Some(queued), 3));
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn successful_reset_removes_only_persisted_older_reset_scratch() {
        let pool = crate::db::init("sqlite::memory:")
            .await
            .expect("migrate reset database");
        let temporary = tempfile::tempdir().expect("create managed root");
        let managed = temporary.path().join("managed-list");
        std::fs::create_dir(&managed).expect("create managed list folder");
        let manifest = managed.join("playlist.json");
        std::fs::write(&manifest, b"derived").expect("seed managed definition");
        seed_reset_state(&pool, manifest.to_string_lossy().as_ref()).await;
        let claim = reset_claim();
        let mut job = reset_job(AddonMediaListLifecyclePhase::UnpublishingFiles);
        sqlx::query(r#"UPDATE "AddonMediaListLifecycleJob" SET "Phase" = ? WHERE "Id" = 1"#)
            .bind(encode_enum(AddonMediaListLifecyclePhase::UnpublishingFiles))
            .execute(&pool)
            .await
            .expect("enter file reset phase");
        apply_phase(
            &pool,
            &claim,
            &job,
            AddonMediaListLifecyclePhase::UnpublishingFiles,
        )
        .await
        .expect("prepare current reset files");

        sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                   "Id", "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (2, 1, 'older-reset', 7, 4,
                         '2026-09-03T00:00:00Z', '2026-09-03T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed older reset operation");
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLifecycleJob" (
                   "Id", "OperationId", "AddonMediaListId", "Generation", "Kind", "Phase",
                   "SavedDraftRevision", "SavedPublishedRevision", "CreatedAt", "UpdatedAt",
                   "CompletedAt"
               ) VALUES (2, 2, 1, 0, 0, 10, 7, 4,
                         '2026-09-03T00:00:00Z', '2026-09-03T00:00:00Z',
                         '2026-09-03T00:00:00Z')"#,
        )
        .execute(&pool)
        .await
        .expect("seed older reset lifecycle");
        let old_identity = PublicationFsIdentity {
            list_id: 1,
            operation_id: Some(2),
            generation: Some(0),
            previous_revision: 4,
            target_revision: 5,
        };
        let old_paths = addon_publication_fs::publication_paths(
            managed.canonicalize().expect("canonical managed folder"),
            old_identity.list_id,
            old_identity.operation_id,
            old_identity.generation,
        )
        .expect("older reset paths");
        std::fs::create_dir(&old_paths.backup).expect("create older reset backup");
        std::fs::write(old_paths.backup.join("archive.yml"), b"old reset archive")
            .expect("seed older reset archive");
        std::fs::write(
            old_paths
                .backup
                .join(addon_publication_fs::PUBLICATION_MARKER_FILE),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "list_id": 1,
                "operation_id": 2,
                "generation": 0,
                "role": "previous",
                "revision": 4,
                "state": "complete",
                "sha256": null,
                "file_count": 1,
                "byte_count": 17
            }))
            .expect("serialize older reset marker"),
        )
        .expect("seed older reset marker");
        let unrelated = temporary.path().join(".ersatzrs-publication-unrelated");
        std::fs::create_dir(&unrelated).expect("create unrelated scratch");
        std::fs::write(unrelated.join("keep.txt"), b"keep").expect("seed unrelated scratch");

        job.phase = AddonMediaListLifecyclePhase::ClearingCheckpoint;
        sqlx::query(r#"UPDATE "AddonMediaListLifecycleJob" SET "Phase" = ? WHERE "Id" = 1"#)
            .bind(encode_enum(
                AddonMediaListLifecyclePhase::ClearingCheckpoint,
            ))
            .execute(&pool)
            .await
            .expect("enter checkpoint reset phase");
        apply_phase(
            &pool,
            &claim,
            &job,
            AddonMediaListLifecyclePhase::ClearingCheckpoint,
        )
        .await
        .expect("clear current and older reset scratch");

        assert!(!old_paths.backup.exists());
        assert!(unrelated.join("keep.txt").is_file());
        let older_rows: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ManagedLibraryOperation" WHERE "Id" = 2"#)
                .fetch_one(&pool)
                .await
                .expect("count older reset operation");
        assert_eq!(older_rows, 0);
    }

    #[allow(clippy::too_many_lines)]
    async fn seed_reset_state(pool: &DbPool, manifest: &str) {
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
               VALUES (1, 1, 1, 'Managed')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (1, ?, 1)"#)
            .bind(manifest)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES ('org.example.reset', '1.0.0', 1, 'package', 'content', '{}',
                         'addons/reset', '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
               VALUES (1, 'System', 1)"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
               VALUES (1, 1, 'Current Name', 1)"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaList" (
                   "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "Description",
                   "ItemCount", "AutoRefresh", "GeneratePlaylist", "DetailImportPolicy",
                   "RefreshCadence", "RefreshPublicationPolicy", "ProcessedArtifactByteLimit",
                   "DraftOptionValuesJson", "DraftRevision", "PublishedRevision",
                   "ProviderMetadataJson", "MetadataOverrideJson", "PlaylistId", "ContentKindOverride",
                   "RequestedPresentationKind", "ChapterInput", "NextRefreshAt",
                   "RefreshRetryAt", "LastRefreshSucceededAt"
               ) VALUES (1, 'org.example.reset', 'provider-list',
                         'https://example.invalid/list', 'Current Name', 'derived', 1, 1, 1,
                         1, 0, 1, 4096, '{"quality":"best"}', 7, 4, '{"title":"old"}',
                         '{"title":"operator"}', 1, 3, 3, '00:01:00',
                         '2026-09-05T00:00:00Z', '2026-09-04T01:00:00Z',
                         '2026-09-03T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLibrary" (
                   "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
               ) VALUES (1, 1, 'managed-list', ?)"#,
        )
        .bind(manifest)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (10, 1)"#)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                   "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind",
                   "MediaItemId", "Rank", "Title"
               ) VALUES (20, 1, 'item', 'Item', 0, 10, 0, 'Item')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "PlaylistItem" (
                   "Index", "PlaylistId", "CollectionType", "CollectionId",
                   "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                   "IncludeInProgramGuide", "PlaybackOrder", "PlayAll",
                   "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
                   "SubtitleStreamIndex"
               ) VALUES (0, 1, 70, NULL, 10, NULL, NULL, 1, 1, 0,
                         NULL, NULL, NULL, NULL)"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportCheckpoint" (
                   "Id", "AddonMediaListId", "RelativePath", "Generation", "Sha256",
                   "EntryCount", "PromotedAt"
               ) VALUES (1, 1, 'archive.txt', 3, 'sha', 1, '2026-09-04T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListImportJob" (
                   "Id", "AddonMediaListId", "Generation", "State", "RecordCapability",
                   "CheckpointId", "CreatedAt", "UpdatedAt"
               ) VALUES (1, 1, 3, 3, 'media.list.record.v1', 1,
                         '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListDetailJob" (
                   "AddonMediaListItemId", "AddonMediaListId", "ProviderHost", "State",
                   "OverviewFingerprint", "OverviewRecordJson", "LeaseOwner", "LeaseExpiresAt",
                   "UpdatedAt"
               ) VALUES (20, 1, 'example.invalid', 3, 'fingerprint', '{}', 'stale-detail',
                         '2999-01-01T00:00:00Z', '2026-09-04T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevision" (
                   "Id", "AddonMediaListId", "Revision", "OptionValuesJson"
               ) VALUES (1, 1, 4, '{}')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtifact" (
                   "Id", "AddonMediaListId", "CacheKey", "Kind", "State", "CreatedAt", "UpdatedAt"
               ) VALUES (1, 1, 'cache', 0, 2, '2026-09-04T00:00:00Z',
                         '2026-09-04T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListRevisionArtifact" (
                   "AddonMediaListRevisionId", "AddonMediaListArtifactId"
               ) VALUES (1, 1)"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryOperation" (
                   "Id", "LibraryId", "OwnerToken", "Kind", "State", "LeaseOwner",
                   "LeaseExpiresAt", "CreatedAt", "UpdatedAt"
               ) VALUES (1, 1, 'reset-owner', 7, 1, 'reset-lease', '2999-01-01T00:00:00Z',
                         '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "ManagedLibraryLease" (
                   "LibraryId", "OperationId", "OwnerToken", "LeaseOwner", "LeaseExpiresAt",
                   "HeartbeatAt", "UpdatedAt"
               ) VALUES (1, 1, 'reset-owner', 'reset-lease', '2999-01-01T00:00:00Z',
                         '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListLifecycleJob" (
                   "Id", "OperationId", "AddonMediaListId", "Generation", "Kind", "Phase",
                   "SavedDraftRevision", "SavedPublishedRevision", "CreatedAt", "UpdatedAt"
               ) VALUES (1, 1, 1, 1, 0, 1, 7, 4,
                         '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z')"#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    fn reset_claim() -> ManagedLibraryOperationClaim {
        ManagedLibraryOperationClaim {
            operation: ManagedLibraryOperation {
                id: 1,
                library_id: 1,
                parent_operation_id: None,
                owner_token: "reset-owner".to_owned(),
                kind: ManagedLibraryOperationKind::Reset,
                state: ManagedLibraryOperationState::Running,
                scan: None,
                attempt_count: 1,
                next_eligible_at: None,
            },
            lease_owner: "reset-lease".to_owned(),
        }
    }

    fn reset_job(phase: AddonMediaListLifecyclePhase) -> AddonMediaListLifecycleJob {
        AddonMediaListLifecycleJob {
            id: 1,
            operation_id: 1,
            list_id: 1,
            generation: 1,
            kind: AddonMediaListLifecycleJobKind::ResetReload,
            phase,
            saved_draft_revision: 7,
            saved_published_revision: 4,
            candidate_import_job_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        }
    }
}
