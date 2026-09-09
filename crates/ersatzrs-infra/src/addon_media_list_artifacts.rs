//! M01 bounded processed-media reservation and admission, without cache eviction.
use crate::{
    addon_media_list_item_jobs::{validate_claim, ClaimedItemOperation, ItemOperationKind},
    addons::{run_bounded_command_with_input_cancellable, AddonError, ResolvedAddonCommand},
    db::DbPool,
    repositories::{common::encode_datetime, SqliteAddonMediaListRepository, SqlxResultExt as _},
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_addon_contract::{
    AddonMediaListOptionOperation, MediaListPrepareRequest, MediaListPrepareResult,
};
use ersatzrs_core::repositories::{AddonMediaListLifecycleRepository, RepositoryError, Result};
use ersatzrs_domain::RemoteStreamLiveness;
use sha2::{Digest as _, Sha256};
use sqlx::Row;
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    time::Instant,
};
use tokio::io::AsyncReadExt as _;

const GIB: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_MAXIMUM_FILE_BYTES: u64 = 2 * GIB;
pub const DEFAULT_RETENTION_SECONDS: u64 = 7 * 24 * 3600;
const MAX_STAGING_ENTRIES: usize = 4096;

#[derive(Debug)]
pub enum ArtifactPromotionError {
    Repository(RepositoryError),
    TerminationUnconfirmed,
}

impl From<RepositoryError> for ArtifactPromotionError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedArtifact {
    pub id: i64,
    pub relative_path: String,
    pub byte_length: u64,
    pub duration_milliseconds: u64,
    pub selected_format: String,
    pub tool_version: String,
}

#[derive(Debug)]
pub enum ArtifactAdmission {
    Ready(AdmittedArtifact),
    Reserved(ArtifactReservation),
}

/// A reservation is bound to an attempt, never transferable to a newer claimant.
#[derive(Debug, Clone)]
pub struct ArtifactReservation {
    pub id: i64,
    pub staging_directory: PathBuf,
    pub maximum_file_bytes: u64,
    pub maximum_staging_bytes: u64,
    pub(crate) root: PathBuf,
    pub(crate) generation: i64,
    pub(crate) job_id: i64,
    pub(crate) lease_owner: String,
}
impl ArtifactReservation {
    /// Build the bounded provider request for this exact reservation owner.
    ///
    /// # Errors
    /// Returns an error for a different attempt, invalid options or request limits,
    /// or a staging path that cannot be represented in the provider contract.
    pub fn prepare_request(&self, job: &ClaimedItemOperation) -> Result<MediaListPrepareRequest> {
        self.matches(job)?;
        let resolved = job.input.resolved_options()?;
        let request = MediaListPrepareRequest {
            source_url: job.input.source_url.clone(),
            provider_id: job.input.provider_id.clone(),
            context_fingerprint: job.input.context_fingerprint.clone(),
            tool_version: job.input.tool_version.clone(),
            options: resolved
                .as_ref()
                .map(|resolved| {
                    resolved.for_operation(
                        AddonMediaListOptionOperation::Preparation,
                        RemoteStreamLiveness::Finite,
                    )
                })
                .transpose()?,
            staging_directory: self
                .staging_directory
                .to_str()
                .ok_or_else(invalid)?
                .to_owned(),
            maximum_file_bytes: self.maximum_file_bytes,
            maximum_staging_bytes: self.maximum_staging_bytes,
            timeout_seconds: 3600,
            max_video_height: job.input.max_video_height,
            requested_subtitle_languages: job.input.requested_subtitle_languages.clone(),
        };
        if job.input.preparation_v2 == Some(true) {
            request.validate_generic(job.input.definition.as_ref(), job.input.item_is_finite)
        } else {
            request.validate(
                job.input.definition.as_ref().ok_or_else(invalid)?,
                job.input.item_is_finite,
            )
        }
        .map_err(|_| invalid())?;
        Ok(request)
    }
    pub(crate) fn matches(&self, job: &ClaimedItemOperation) -> Result<()> {
        if self.job_id != job.id
            || self.generation != job.generation
            || self.lease_owner != job.lease_owner
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn final_relative_path(&self) -> String {
        format!(
            "objects/{}-{}-{}.media",
            self.id, self.generation, self.lease_owner
        )
    }
}

#[derive(Clone)]
pub struct AddonMediaListArtifactStore {
    pool: DbPool,
}
impl AddonMediaListArtifactStore {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Atomically count existing files and reservations before admitting one attempt.
    ///
    /// # Errors
    /// Returns an error for invalid or stale ownership, exhausted storage capacity,
    /// inconsistent cached files, or filesystem/database failures.
    #[allow(clippy::too_many_lines)] // Ownership and capacity admission share one transaction.
    pub async fn reserve(
        &self,
        job: &ClaimedItemOperation,
        root: &Path,
        now: DateTime<Utc>,
    ) -> Result<ArtifactAdmission> {
        if job.kind != ItemOperationKind::Preparation || !job.input.item_is_finite {
            return Err(invalid());
        }
        let root = prepare_root(root).await?;
        let free = fs4::available_space(&root).map_err(|_| invalid())?;
        let repository = SqliteAddonMediaListRepository(self.pool.clone());
        let policy = repository.processed_artifact_policy().await?;
        let global_limit = policy.global_byte_limit.unwrap_or(20 * GIB);
        let minimum_free = policy.minimum_free_bytes.unwrap_or(2 * GIB);
        let concurrency = policy.max_preparation_concurrency.unwrap_or(1);
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        validate_claim(&mut tx, job, now).await?;
        let existing = sqlx::query(
            r#"
            SELECT "Id", "AddonMediaListItemId", "State", "RelativePath", "ByteLength", "DurationMilliseconds",
                   "SelectedFormat", "ToolVersion"
            FROM "AddonMediaListArtifact" WHERE "AddonMediaListId"=? AND "CacheKey"=?
        "#,
        )
        .bind(job.list_id)
        .bind(&job.input.context_fingerprint)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        if let Some(row) = existing.as_ref() {
            let owner: Option<i64> = row.try_get("AddonMediaListItemId").repository()?;
            if owner.is_some_and(|owner| owner != job.item_id) {
                return Err(invalid());
            }
            if owner.is_none() {
                // Confirmed removal leaves cached files unowned. Reappearance
                // may reuse the exact provider/source/options key, without
                // changing the immutable file or its retained revision pins.
                sqlx::query(
                    r#"UPDATE "AddonMediaListArtifact" SET "AddonMediaListItemId"=? WHERE "Id"=?"#,
                )
                .bind(job.item_id)
                .bind(row.try_get::<i64, _>("Id").repository()?)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            if row.try_get::<i32, _>("State").repository()? == 2 {
                let artifact = admitted(row)?;
                let path = confined_file(&root, &artifact.relative_path).await?;
                if tokio::fs::metadata(path)
                    .await
                    .map_err(|_| invalid())?
                    .len()
                    != artifact.byte_length
                    || artifact.tool_version != job.input.tool_version
                {
                    return Err(invalid());
                }
                tx.commit().await.repository()?;
                return Ok(ArtifactAdmission::Ready(artifact));
            }
            // Explicit recovery owns cleanup. A fresh claim never takes over a file silently.
            if row.try_get::<i32, _>("State").repository()? != 3 {
                return Err(invalid());
            }
        }
        let list_limit: Option<i64> = sqlx::query_scalar(
            r#"SELECT "ProcessedArtifactByteLimit" FROM "AddonMediaList" WHERE "Id"=?"#,
        )
        .bind(job.list_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        let list_limit = list_limit
            .map(u64::try_from)
            .transpose()
            .map_err(|_| invalid())?
            .unwrap_or(10 * GIB);
        let row = sqlx::query(
            r#"
            SELECT COALESCE(SUM(CASE WHEN "State"=2 THEN "ByteLength" ELSE "ReservedBytes" END),0) AS "GlobalUsed",
                   COALESCE(SUM(CASE WHEN "AddonMediaListId"=? THEN CASE WHEN "State"=2 THEN "ByteLength" ELSE "ReservedBytes" END ELSE 0 END),0) AS "ListUsed",
                   COALESCE(SUM(CASE WHEN "State"<>2 THEN "ReservedBytes" ELSE 0 END),0) AS "Reserved",
                   COALESCE(SUM(CASE WHEN "Kind"=1 AND "State"<>2 THEN 1 ELSE 0 END),0) AS "Running"
            FROM "AddonMediaListArtifact" WHERE "Kind" IN (0,1) AND "State" IN (0,1,2)
        "#,
        )
        .bind(job.list_id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        let global_used = u64::try_from(row.try_get::<i64, _>("GlobalUsed").repository()?)
            .map_err(|_| invalid())?;
        let list_used = u64::try_from(row.try_get::<i64, _>("ListUsed").repository()?)
            .map_err(|_| invalid())?;
        let reserved = u64::try_from(row.try_get::<i64, _>("Reserved").repository()?)
            .map_err(|_| invalid())?;
        let running =
            u32::try_from(row.try_get::<i64, _>("Running").repository()?).map_err(|_| invalid())?;
        let staging_bytes = DEFAULT_MAXIMUM_FILE_BYTES;
        if running >= concurrency
            || global_used.saturating_add(staging_bytes) > global_limit
            || list_used.saturating_add(staging_bytes) > list_limit
            || free
                < minimum_free
                    .saturating_add(reserved)
                    .saturating_add(staging_bytes)
        {
            return Err(RepositoryError::Conflict(
                "processed artifact capacity is unavailable".into(),
            ));
        }
        let time = encode_datetime(now);
        let id = if let Some(row) = existing {
            let id: i64 = row.try_get("Id").repository()?;
            sqlx::query(r#"
                UPDATE "AddonMediaListArtifact" SET "State"=0,"ReservedBytes"=?,
                    "PreparationJobId"=?,"PreparationGeneration"=?,"PreparationLeaseOwner"=?,
                    "RelativePath"=NULL,"ByteLength"=NULL,"Sha256"=NULL,"CompletedAt"=NULL,"UpdatedAt"=?
                WHERE "Id"=? AND "State"=3
            "#).bind(i64::try_from(staging_bytes).map_err(|_| invalid())?).bind(job.id).bind(job.generation)
                .bind(&job.lease_owner).bind(&time).bind(id).execute(&mut *tx).await.repository()?;
            id
        } else {
            sqlx::query(r#"
                INSERT INTO "AddonMediaListArtifact" ("AddonMediaListId","AddonMediaListItemId","CacheKey",
                    "Kind","State","ReservedBytes","PreparationJobId","PreparationGeneration",
                    "PreparationLeaseOwner","CreatedAt","UpdatedAt") VALUES (?,?,?,1,0,?,?,?,?,?,?)
            "#).bind(job.list_id).bind(job.item_id).bind(&job.input.context_fingerprint).bind(i64::try_from(staging_bytes).map_err(|_| invalid())?)
                .bind(job.id).bind(job.generation).bind(&job.lease_owner).bind(&time).bind(&time)
                .execute(&mut *tx).await.repository()?.last_insert_rowid()
        };
        let staging_directory = stage_path(&root, job.id, job.generation, &job.lease_owner)?;
        tokio::fs::create_dir(&staging_directory)
            .await
            .map_err(|_| invalid())?;
        let mut stage_guard = NewStageGuard(Some(staging_directory.clone()));
        let reservation = ArtifactReservation {
            id,
            staging_directory,
            maximum_file_bytes: DEFAULT_MAXIMUM_FILE_BYTES,
            maximum_staging_bytes: staging_bytes,
            root,
            generation: job.generation,
            job_id: job.id,
            lease_owner: job.lease_owner.clone(),
        };
        tx.commit().await.repository()?;
        stage_guard.0 = None;
        Ok(ArtifactAdmission::Reserved(reservation))
    }

    /// Call periodically while the child is running; exceeding the bound requires cancellation.
    ///
    /// # Errors
    /// Returns an error if staging exceeds its size/entry limits, contains unsafe
    /// entries, or cannot be inspected.
    pub async fn check_staging(&self, reservation: &ArtifactReservation) -> Result<u64> {
        measure_tree(
            &reservation.staging_directory,
            reservation.maximum_staging_bytes,
        )
        .await
    }

    /// Invoke only after provider descendants have exited and can no longer mutate staging.
    ///
    /// # Errors
    /// Returns an error for stale ownership, invalid or mismatched media output,
    /// exceeded limits, or probe/filesystem/database failures.
    pub async fn promote(
        &self,
        job: &ClaimedItemOperation,
        reservation: &ArtifactReservation,
        result: &MediaListPrepareResult,
        ffprobe_path: &str,
        now: DateTime<Utc>,
    ) -> Result<AdmittedArtifact> {
        let (_sender, cancellation) = tokio::sync::oneshot::channel();
        self.promote_cancellable(job, reservation, result, ffprobe_path, now, cancellation)
            .await
            .map_err(|error| match error {
                ArtifactPromotionError::Repository(error) => error,
                ArtifactPromotionError::TerminationUnconfirmed => RepositoryError::unavailable(
                    "artifact promotion",
                    "probe process termination could not be confirmed",
                ),
            })
    }

    /// Promote only after provider cleanup, cancelling and awaiting the final probe.
    ///
    /// # Errors
    /// Unconfirmed probe termination requires retaining the reservation and files.
    #[allow(clippy::too_many_arguments)] // Keep the existing promotion inputs plus explicit cancellation.
    pub async fn promote_cancellable(
        &self,
        job: &ClaimedItemOperation,
        reservation: &ArtifactReservation,
        result: &MediaListPrepareResult,
        ffprobe_path: &str,
        now: DateTime<Utc>,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> std::result::Result<AdmittedArtifact, ArtifactPromotionError> {
        let started = Instant::now();
        reservation.matches(job)?;
        prepare_root(&reservation.root).await?;
        result
            .validate(&reservation.prepare_request(job)?)
            .map_err(|_| invalid())?;
        self.check_staging(reservation).await?;
        let file = confined_file(&reservation.staging_directory, &result.relative_path).await?;
        let (size, hash) = inspect_file(&file, reservation.maximum_file_bytes).await?;
        if size != result.byte_length || hash != result.sha256 {
            return Err(invalid().into());
        }
        let command = ResolvedAddonCommand {
            tool_paths: Vec::new(),
            addon_id: job.input.addon_id()?.to_owned(),
            program: ffprobe_path.into(),
            args: crate::ffprobe::ffprobe_args(&file),
            working_directory: reservation.staging_directory.clone(),
            environment: BTreeMap::new(),
            state_revision: job.lease_owner.clone(),
        };
        let output = run_bounded_command_with_input_cancellable(
            &command,
            std::time::Duration::from_secs(30),
            1024 * 1024,
            b"",
            &[],
            cancellation,
        )
        .await
        .map_err(|error| match error {
            AddonError::TerminationUnconfirmed => ArtifactPromotionError::TerminationUnconfirmed,
            _ => ArtifactPromotionError::Repository(invalid()),
        })?;
        let probe = crate::ffprobe::parse_ffprobe_json(&file, &output).map_err(|_| invalid())?;
        let duration =
            u64::try_from(probe.version.duration.num_milliseconds()).map_err(|_| invalid())?;
        if duration == 0
            || duration.abs_diff(result.duration_milliseconds) > 1000
            || probe.streams.is_empty()
        {
            return Err(invalid().into());
        }
        let artifact = AdmittedArtifact {
            id: reservation.id,
            relative_path: reservation.final_relative_path(),
            byte_length: size,
            duration_milliseconds: duration,
            selected_format: result.selected_format.clone(),
            tool_version: result.tool_version.clone(),
        };
        let completed_at = now + Duration::from_std(started.elapsed()).map_err(|_| invalid())?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        validate_claim(&mut tx, job, completed_at).await?;
        owned(&mut tx, reservation).await?;
        let target = reservation.root.join(&artifact.relative_path);
        if tokio::fs::symlink_metadata(&target).await.is_ok() {
            return Err(invalid().into());
        }
        tokio::fs::rename(&file, &target)
            .await
            .map_err(|_| invalid())?;
        // Do not release the staging reservation while intermediate files remain.
        tokio::fs::remove_dir_all(&reservation.staging_directory)
            .await
            .map_err(|_| invalid())?;
        let time = encode_datetime(completed_at);
        sqlx::query(r#"
            UPDATE "AddonMediaListArtifact" SET "State"=2,"RelativePath"=?,"ByteLength"=?,"Sha256"=?,
                "DurationMilliseconds"=?,"SelectedFormat"=?,"ToolVersion"=?,"ReservedBytes"=0,
                "CompletedAt"=?,"UpdatedAt"=?,"LastUsedAt"=? WHERE "Id"=?
        "#).bind(&artifact.relative_path).bind(i64::try_from(size).map_err(|_| invalid())?).bind(&hash).bind(i64::try_from(duration).map_err(|_| invalid())?)
            .bind(&result.selected_format).bind(&result.tool_version).bind(&time).bind(&time).bind(&time)
            .bind(reservation.id).execute(&mut *tx).await.repository()?;
        tx.commit().await.repository()?;
        Ok(artifact)
    }

    /// Caller must first stop all attempt descendants. Cancellation may have
    /// invalidated the queue lease; artifact ownership still fences cleanup.
    ///
    /// # Errors
    /// Returns an error for mismatched ownership or filesystem/database failures.
    pub async fn release_after_cleanup(
        &self,
        job: &ClaimedItemOperation,
        reservation: &ArtifactReservation,
    ) -> Result<()> {
        reservation.matches(job)?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        owned(&mut tx, reservation).await?;
        let same_generation: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(SELECT 1 FROM "AddonMediaListItemOperationJob"
                WHERE "Id"=? AND "Generation"=? AND "LeaseOwner"=?)
        "#,
        )
        .bind(job.id)
        .bind(job.generation)
        .bind(&job.lease_owner)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if !same_generation {
            return Err(invalid());
        }
        discard_owned_files(&mut tx, reservation).await?;
        release_reservation(&mut tx, reservation.id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Reconcile only the checkpoint's exact attempt while the worker root lock
    /// is held and its durable receipt proves process cleanup. Never touch Ready.
    pub(crate) async fn recover_exact_after_cleanup(
        &self,
        root: &Path,
        attempt: &crate::addon_preparation_worker::AttemptIdentity,
    ) -> Result<()> {
        let root = prepare_root(root).await?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        let ids: Vec<i64> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "AddonMediaListArtifact"
            WHERE "Kind" IN (0,1) AND "State" IN (0,1) AND "PreparationJobId"=?
              AND "PreparationGeneration"=? AND "PreparationLeaseOwner"=? ORDER BY "Id" LIMIT 33"#,
        )
        .bind(attempt.job_id)
        .bind(attempt.generation)
        .bind(&attempt.lease_owner)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        if ids.len() > 32 {
            return Err(invalid());
        }
        for id in ids {
            let reservation = ArtifactReservation {
                id,
                staging_directory: stage_path(
                    &root,
                    attempt.job_id,
                    attempt.generation,
                    &attempt.lease_owner,
                )?,
                root: root.clone(),
                maximum_file_bytes: DEFAULT_MAXIMUM_FILE_BYTES,
                maximum_staging_bytes: DEFAULT_MAXIMUM_FILE_BYTES,
                job_id: attempt.job_id,
                generation: attempt.generation,
                lease_owner: attempt.lease_owner.clone(),
            };
            owned(&mut tx, &reservation).await?;
            discard_owned_files(&mut tx, &reservation).await?;
            release_reservation(&mut tx, id).await?;
        }
        tx.commit().await.repository()?;
        Ok(())
    }

    /// Reconcile an expired predecessor after the coordinator confirms its process
    /// tree is gone. Ready artifacts are immutable and are never reclaimed here.
    ///
    /// # Errors
    /// Returns an error for stale claims, live predecessors, unsafe or excessive
    /// staging entries, or filesystem/database failures.
    pub async fn recover_stale_after_process_cleanup(
        &self,
        job: &ClaimedItemOperation,
        root: &Path,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let root = prepare_root(root).await?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        validate_claim(&mut tx, job, now).await?;
        let rows=sqlx::query(r#"
            SELECT "Id","PreparationJobId","PreparationGeneration","PreparationLeaseOwner"
            FROM "AddonMediaListArtifact" WHERE "AddonMediaListId"=? AND "AddonMediaListItemId"=? AND "State" IN (0,1)
            ORDER BY "Id" LIMIT 33
        "#).bind(job.list_id).bind(job.item_id).fetch_all(&mut *tx).await.repository()?;
        if rows.len() > 32 {
            return Err(invalid());
        }
        for row in rows {
            let old_job: i64 = row.try_get("PreparationJobId").repository()?;
            let generation: i64 = row.try_get("PreparationGeneration").repository()?;
            let owner: String = row.try_get("PreparationLeaseOwner").repository()?;
            if old_job == job.id && generation == job.generation {
                return Err(invalid());
            }
            let live: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS(SELECT 1 FROM "AddonMediaListItemOperationJob"
                    WHERE "Id"=? AND "Generation"=? AND "LeaseOwner"=? AND "LeaseExpiresAt">?)
            "#,
            )
            .bind(old_job)
            .bind(generation)
            .bind(&owner)
            .bind(encode_datetime(now))
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if live {
                return Err(invalid());
            }
            let reservation = ArtifactReservation {
                id: row.try_get("Id").repository()?,
                staging_directory: stage_path(&root, old_job, generation, &owner)?,
                root: root.clone(),
                maximum_file_bytes: DEFAULT_MAXIMUM_FILE_BYTES,
                maximum_staging_bytes: DEFAULT_MAXIMUM_FILE_BYTES,
                job_id: old_job,
                generation,
                lease_owner: owner,
            };
            owned(&mut tx, &reservation).await?;
            discard_owned_files(&mut tx, &reservation).await?;
            release_reservation(&mut tx, reservation.id).await?;
        }
        recover_orphan_stages(&mut tx, job, &root).await?;
        tx.commit().await.repository()?;
        Ok(())
    }
}

/// No child can have started before reserve returns. Compensate errors and
/// dropped futures with an empty-directory removal, never recursive deletion.
struct NewStageGuard(Option<PathBuf>);
impl Drop for NewStageGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_dir(path);
        }
    }
}

async fn recover_orphan_stages(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    job: &ClaimedItemOperation,
    root: &Path,
) -> Result<()> {
    let mut entries = tokio::fs::read_dir(root.join("staging"))
        .await
        .map_err(|_| invalid())?;
    let mut count = 0usize;
    while let Some(entry) = entries.next_entry().await.map_err(|_| invalid())? {
        count += 1;
        if count > MAX_STAGING_ENTRIES {
            return Err(invalid());
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let mut parts = name.splitn(3, '-');
        let Some(id) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let Some(generation) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let Some(owner) = parts.next() else {
            continue;
        };
        if id != job.id
            || generation <= 0
            || generation >= job.generation
            || uuid::Uuid::parse_str(owner).is_err()
        {
            continue;
        }
        let recorded: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(SELECT 1 FROM "AddonMediaListArtifact" WHERE "PreparationJobId"=?
                AND "PreparationGeneration"=? AND "PreparationLeaseOwner"=?)
        "#,
        )
        .bind(id)
        .bind(generation)
        .bind(owner)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
        if recorded {
            continue;
        }
        // Only older generations of this exact reclaimed job are eligible.
        let path = stage_path(root, id, generation, owner)?;
        measure_tree(&path, DEFAULT_MAXIMUM_FILE_BYTES).await?;
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|_| invalid())?;
    }
    Ok(())
}

async fn discard_owned_files(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    reservation: &ArtifactReservation,
) -> Result<()> {
    // A hard crash can interrupt subtitle rename before its Ready transaction.
    // These are the only two extra targets this exact reservation can create;
    // committed immutable sidecars must survive normal release and recovery.
    for index in 0..2 {
        let relative = format!(
            "objects/{}-{}-{}-subtitle-{index}.srt",
            reservation.id, reservation.generation, reservation.lease_owner
        );
        let ready: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListArtifact"
            WHERE "State"=2 AND "RelativePath"=?)"#,
        )
        .bind(&relative)
        .fetch_one(&mut **tx)
        .await
        .repository()?;
        if ready {
            continue;
        }
        match tokio::fs::symlink_metadata(reservation.root.join(&relative)).await {
            Ok(metadata) if metadata.is_file() && !metadata.is_symlink() => {
                tokio::fs::remove_file(reservation.root.join(relative))
                    .await
                    .map_err(|_| invalid())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(invalid()),
        }
    }
    let stage = &reservation.staging_directory;
    match tokio::fs::symlink_metadata(stage).await {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {
            tokio::fs::remove_dir_all(stage)
                .await
                .map_err(|_| invalid())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(invalid()),
    }
    let target = reservation.root.join(reservation.final_relative_path());
    match tokio::fs::symlink_metadata(&target).await {
        Ok(metadata) if metadata.is_file() && !metadata.is_symlink() => {
            tokio::fs::remove_file(target)
                .await
                .map_err(|_| invalid())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err(invalid()),
    }
    Ok(())
}
async fn release_reservation(tx: &mut sqlx::Transaction<'_, crate::db::Db>, id: i64) -> Result<()> {
    sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "State"=3,"ReservedBytes"=0 WHERE "Id"=?"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
    Ok(())
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("processed artifact ownership or validation failed".into())
}
pub(crate) async fn prepare_root(root: &Path) -> Result<PathBuf> {
    tokio::fs::create_dir_all(root)
        .await
        .map_err(|_| invalid())?;
    let root = tokio::fs::canonicalize(root).await.map_err(|_| invalid())?;
    for child in ["staging", "objects"] {
        let path = root.join(child);
        if !tokio::fs::try_exists(&path).await.map_err(|_| invalid())? {
            tokio::fs::create_dir(&path).await.map_err(|_| invalid())?;
        }
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|_| invalid())?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(invalid());
        }
    }
    Ok(root)
}
pub(crate) fn stage_path(root: &Path, job: i64, generation: i64, owner: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(owner).map_err(|_| invalid())?;
    if job <= 0 || generation <= 0 {
        return Err(invalid());
    }
    Ok(root
        .join("staging")
        .join(format!("{job}-{generation}-{owner}")))
}
pub(crate) async fn confined_file(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative.contains(['\\', ':']) || relative.starts_with('/') {
        return Err(invalid());
    }
    let mut current = root.to_path_buf();
    let parts = relative.split('/').collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || *part == "." || *part == ".." {
            return Err(invalid());
        }
        current.push(part);
        let metadata = tokio::fs::symlink_metadata(&current)
            .await
            .map_err(|_| invalid())?;
        if metadata.is_symlink()
            || (index + 1 == parts.len() && !metadata.is_file())
            || (index + 1 < parts.len() && !metadata.is_dir())
        {
            return Err(invalid());
        }
    }
    Ok(current)
}
pub(crate) async fn measure_tree(root: &Path, limit: u64) -> Result<u64> {
    let mut pending = vec![root.to_path_buf()];
    let (mut count, mut bytes) = (0usize, 0u64);
    while let Some(path) = pending.pop() {
        count += 1;
        if count > MAX_STAGING_ENTRIES {
            return Err(invalid());
        }
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|_| invalid())?;
        if metadata.is_symlink() {
            return Err(invalid());
        }
        if metadata.is_dir() {
            let mut entries = tokio::fs::read_dir(path).await.map_err(|_| invalid())?;
            while let Some(entry) = entries.next_entry().await.map_err(|_| invalid())? {
                if pending.len() + count >= MAX_STAGING_ENTRIES {
                    return Err(invalid());
                }
                pending.push(entry.path());
            }
        } else if metadata.is_file() {
            bytes = bytes.checked_add(metadata.len()).ok_or_else(invalid)?;
        } else {
            return Err(invalid());
        }
        if bytes > limit {
            return Err(RepositoryError::Conflict(
                "processed artifact staging limit exceeded".into(),
            ));
        }
    }
    Ok(bytes)
}
pub(crate) async fn inspect_file(path: &Path, limit: u64) -> Result<(u64, String)> {
    let mut file = tokio::fs::File::open(path).await.map_err(|_| invalid())?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut bytes = 0u64;
    let mut hash = Sha256::new();
    loop {
        let count = file.read(&mut buffer).await.map_err(|_| invalid())?;
        if count == 0 {
            break;
        }
        bytes = bytes.checked_add(count as u64).ok_or_else(invalid)?;
        if bytes > limit {
            return Err(invalid());
        }
        hash.update(&buffer[..count]);
    }
    let mut digest = String::with_capacity(64);
    for byte in &hash.finalize() {
        let _ = write!(digest, "{byte:02x}");
    }
    Ok((bytes, digest))
}
pub(crate) async fn owned(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    reservation: &ArtifactReservation,
) -> Result<()> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(SELECT 1 FROM "AddonMediaListArtifact" WHERE "Id"=? AND "State" IN (0,1)
            AND "PreparationJobId"=? AND "PreparationGeneration"=? AND "PreparationLeaseOwner"=?)
    "#,
    )
    .bind(reservation.id)
    .bind(reservation.job_id)
    .bind(reservation.generation)
    .bind(&reservation.lease_owner)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if !exists {
        return Err(invalid());
    }
    Ok(())
}
fn admitted(row: &crate::db::DbRow) -> Result<AdmittedArtifact> {
    Ok(AdmittedArtifact {
        id: row.try_get("Id").repository()?,
        relative_path: row.try_get("RelativePath").repository()?,
        byte_length: u64::try_from(row.try_get::<i64, _>("ByteLength").repository()?)
            .map_err(|_| invalid())?,
        duration_milliseconds: u64::try_from(
            row.try_get::<i64, _>("DurationMilliseconds").repository()?,
        )
        .map_err(|_| invalid())?,
        selected_format: row.try_get("SelectedFormat").repository()?,
        tool_version: row.try_get("ToolVersion").repository()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artifact_failed_reservation_guard_removes_only_new_empty_stage() {
        let root = tempfile::tempdir().unwrap();
        let empty = root.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        drop(NewStageGuard(Some(empty.clone())));
        assert!(!empty.exists());
        let occupied = root.path().join("occupied");
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("unrelated"), b"preserve").unwrap();
        drop(NewStageGuard(Some(occupied.clone())));
        assert!(occupied.join("unrelated").exists());
    }
}
