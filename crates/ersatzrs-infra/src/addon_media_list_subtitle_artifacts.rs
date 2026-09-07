//! Bounded manual sidecars, using the existing artifact reservation lifecycle.
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use ersatzrs_addon_contract::{
    subtitle_logical_language, validate_requested_subtitle_languages,
    AddonMediaListOptionOperation, MediaListSubtitleArtifact, MediaListSubtitleFormat,
    MediaListSubtitleProvenance, MediaListSubtitleTimeline, MediaListSubtitlesRequest,
};
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, RepositoryError, Result,
};
use ersatzrs_domain::{MediaPlaybackTimeline, RemoteStreamLiveness};
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use tokio::io::AsyncReadExt as _;

use crate::{
    addon_media_list_artifacts::{
        confined_file, measure_tree, owned, prepare_root, stage_path, AddonMediaListArtifactStore,
        ArtifactReservation,
    },
    addon_media_list_item_jobs::{validate_claim, ClaimedItemOperation, ItemOperationKind},
    db::DbPool,
    repositories::{common::encode_datetime, SqliteAddonMediaListRepository, SqlxResultExt as _},
};

pub(crate) const MAXIMUM_SUBTITLE_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAXIMUM_SUBTITLE_STAGING_BYTES: u64 = 2 * MAXIMUM_SUBTITLE_FILE_BYTES;
const GIB: u64 = 1024 * 1024 * 1024;

pub(crate) type SubtitleReservation = ArtifactReservation;

impl ArtifactReservation {
    pub(crate) fn subtitle_request(
        &self,
        job: &ClaimedItemOperation,
    ) -> Result<MediaListSubtitlesRequest> {
        self.matches(job)?;
        if job.kind != ItemOperationKind::Subtitles {
            return Err(invalid());
        }
        let resolved = job.input.resolved_options()?;
        let request = MediaListSubtitlesRequest {
            source_url: job.input.source_url.clone(),
            provider_id: job.input.provider_id.clone(),
            context_fingerprint: job.input.context_fingerprint.clone(),
            tool_version: job.input.tool_version.clone(),
            options: resolved
                .as_ref()
                .map(|options| {
                    options.for_operation(
                        AddonMediaListOptionOperation::Artifact,
                        RemoteStreamLiveness::Finite,
                    )
                })
                .transpose()?,
            requested_languages: job.input.requested_subtitle_languages.clone(),
            staging_directory: self
                .staging_directory
                .to_str()
                .ok_or_else(invalid)?
                .to_owned(),
            maximum_file_bytes: self.maximum_file_bytes,
            maximum_staging_bytes: self.maximum_staging_bytes,
            timeout_seconds: 120,
        };
        request
            .validate(job.input.definition.as_ref(), job.input.item_is_finite)
            .map_err(|_| invalid())?;
        Ok(request)
    }
}

pub(crate) struct SubtitleArtifactStore {
    pool: DbPool,
}

impl SubtitleArtifactStore {
    pub(crate) fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn reserve(
        &self,
        job: &ClaimedItemOperation,
        root: &Path,
        now: DateTime<Utc>,
    ) -> Result<SubtitleReservation> {
        if job.kind != ItemOperationKind::Subtitles || !job.input.item_is_finite {
            return Err(invalid());
        }
        validate_requested_subtitle_languages(&job.input.requested_subtitle_languages, false)
            .map_err(|_| invalid())?;
        let root = prepare_root(root).await?;
        let free = fs4::available_space(&root).map_err(|_| invalid())?;
        let policy = SqliteAddonMediaListRepository(self.pool.clone())
            .processed_artifact_policy()
            .await?;
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        validate_claim(&mut tx, job, now).await?;
        let unfinished: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaListArtifact"
            WHERE "Kind"=0 AND "State" IN (0,1) AND "PreparationJobId"=?)"#,
        )
        .bind(job.id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if unfinished {
            return Err(invalid());
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
        let usage = sqlx::query(r#"SELECT
            COALESCE(SUM(CASE WHEN "State"=2 THEN "ByteLength" ELSE "ReservedBytes" END),0) AS "GlobalUsed",
            COALESCE(SUM(CASE WHEN "AddonMediaListId"=? THEN CASE WHEN "State"=2 THEN "ByteLength" ELSE "ReservedBytes" END ELSE 0 END),0) AS "ListUsed",
            COALESCE(SUM(CASE WHEN "State"<>2 THEN "ReservedBytes" ELSE 0 END),0) AS "Reserved"
            FROM "AddonMediaListArtifact" WHERE "Kind" IN (0,1) AND "State" IN (0,1,2)"#)
            .bind(job.list_id).fetch_one(&mut *tx).await.repository()?;
        let used = |column| -> Result<u64> {
            u64::try_from(usage.try_get::<i64, _>(column).repository()?).map_err(|_| invalid())
        };
        if used("GlobalUsed")?.saturating_add(MAXIMUM_SUBTITLE_STAGING_BYTES)
            > policy.global_byte_limit.unwrap_or(20 * GIB)
            || used("ListUsed")?.saturating_add(MAXIMUM_SUBTITLE_STAGING_BYTES) > list_limit
            || free
                < policy
                    .minimum_free_bytes
                    .unwrap_or(2 * GIB)
                    .saturating_add(used("Reserved")?)
                    .saturating_add(MAXIMUM_SUBTITLE_STAGING_BYTES)
        {
            return Err(RepositoryError::Conflict(
                "processed artifact capacity is unavailable".into(),
            ));
        }
        let time = encode_datetime(now);
        let key = format!(
            "subtitle-reservation-{}-{}-{}",
            job.id, job.generation, job.lease_owner
        );
        let id = sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" (
            "AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","ReservedBytes",
            "PreparationJobId","PreparationGeneration","PreparationLeaseOwner","OperationContextFingerprint","CreatedAt","UpdatedAt")
            VALUES (?,?,?,0,0,?,?,?,?,?,?,?)"#)
            .bind(job.list_id).bind(job.item_id).bind(key)
            .bind(i64::try_from(MAXIMUM_SUBTITLE_STAGING_BYTES).map_err(|_| invalid())?)
            .bind(job.id).bind(job.generation).bind(&job.lease_owner).bind(&job.input.context_fingerprint)
            .bind(&time).bind(&time).execute(&mut *tx).await.repository()?.last_insert_rowid();
        let staging_directory = stage_path(&root, job.id, job.generation, &job.lease_owner)?;
        tokio::fs::create_dir(&staging_directory)
            .await
            .map_err(|_| invalid())?;
        let mut guard = NewFiles {
            directory: Some(staging_directory.clone()),
            files: Vec::new(),
        };
        tx.commit().await.repository()?;
        guard.directory = None;
        Ok(ArtifactReservation {
            id,
            staging_directory,
            maximum_file_bytes: MAXIMUM_SUBTITLE_FILE_BYTES,
            maximum_staging_bytes: MAXIMUM_SUBTITLE_STAGING_BYTES,
            root,
            generation: job.generation,
            job_id: job.id,
            lease_owner: job.lease_owner.clone(),
        })
    }

    pub(crate) async fn check_staging(&self, reservation: &SubtitleReservation) -> Result<u64> {
        measure_tree(
            &reservation.staging_directory,
            reservation.maximum_staging_bytes,
        )
        .await
    }

    /// Call only after the provider and all descendants have stopped writing.
    pub(crate) async fn promote(
        &self,
        job: &ClaimedItemOperation,
        reservation: &SubtitleReservation,
        artifacts: &[MediaListSubtitleArtifact],
        now: DateTime<Utc>,
    ) -> Result<Vec<i64>> {
        if job.kind != ItemOperationKind::Subtitles {
            return Err(invalid());
        }
        self.promote_inner(
            job,
            reservation,
            artifacts,
            MediaListSubtitleTimeline::Source,
            now,
        )
        .await
    }

    /// Uses the existing media reservation; call before media promotion removes scratch.
    pub(crate) async fn promote_prepared(
        &self,
        job: &ClaimedItemOperation,
        reservation: &ArtifactReservation,
        artifacts: &[MediaListSubtitleArtifact],
        now: DateTime<Utc>,
    ) -> Result<Vec<i64>> {
        if job.kind != ItemOperationKind::Preparation {
            return Err(invalid());
        }
        self.promote_inner(
            job,
            reservation,
            artifacts,
            MediaListSubtitleTimeline::Prepared,
            now,
        )
        .await
    }

    pub(crate) async fn release_after_cleanup(
        &self,
        job: &ClaimedItemOperation,
        reservation: &SubtitleReservation,
    ) -> Result<()> {
        AddonMediaListArtifactStore::new(self.pool.clone())
            .release_after_cleanup(job, reservation)
            .await
    }

    #[allow(clippy::too_many_lines)] // Keep file promotion and exact completion ownership together.
    async fn promote_inner(
        &self,
        job: &ClaimedItemOperation,
        reservation: &ArtifactReservation,
        artifacts: &[MediaListSubtitleArtifact],
        timeline: MediaListSubtitleTimeline,
        now: DateTime<Utc>,
    ) -> Result<Vec<i64>> {
        reservation.matches(job)?;
        validate_requested_subtitle_languages(&job.input.requested_subtitle_languages, true)
            .map_err(|_| invalid())?;
        if artifacts.len() > job.input.requested_subtitle_languages.len() {
            return Err(invalid());
        }
        self.check_staging(reservation).await?;
        let mut languages = std::collections::BTreeSet::new();
        let mut paths = std::collections::BTreeSet::new();
        let mut validated = Vec::new();
        for artifact in artifacts {
            let language = subtitle_logical_language(&artifact.language).map_err(|_| invalid())?;
            if !languages.insert(language.clone())
                || !paths.insert(&artifact.relative_path)
                || !job
                    .input
                    .requested_subtitle_languages
                    .iter()
                    .any(|requested| {
                        subtitle_logical_language(requested)
                            .is_ok_and(|requested| requested == language)
                    })
                || artifact.timeline != timeline
                || artifact.format != MediaListSubtitleFormat::Srt
                || artifact.provenance != MediaListSubtitleProvenance::Manual
                || artifact.byte_length == 0
                || artifact.byte_length > MAXIMUM_SUBTITLE_FILE_BYTES
            {
                return Err(invalid());
            }
            let path =
                confined_file(&reservation.staging_directory, &artifact.relative_path).await?;
            let file = tokio::fs::File::open(&path).await.map_err(|_| invalid())?;
            let mut bytes = Vec::new();
            file.take(MAXIMUM_SUBTITLE_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| invalid())?;
            let hash = hex_digest(&bytes);
            if u64::try_from(bytes.len()).map_err(|_| invalid())? != artifact.byte_length
                || hash != artifact.sha256
            {
                return Err(invalid());
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
            let identity =
                MediaPlaybackTimeline::from_exclusions(i64::MAX as u64, 0..i64::MAX as u64, &[])
                    .map_err(|_| invalid())?;
            let mapped = crate::subtitles::map_srt_through_timeline(
                text,
                &identity,
                usize::try_from(MAXIMUM_SUBTITLE_STAGING_BYTES).map_err(|_| invalid())?,
            )
            .map_err(|_| invalid())?;
            if mapped.is_empty() {
                return Err(invalid());
            }
            validated.push((artifact, path));
        }
        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        validate_claim(&mut tx, job, now).await?;
        owned(&mut tx, reservation).await?;
        let time = encode_datetime(now);
        let clock = match timeline {
            MediaListSubtitleTimeline::Source => 0,
            MediaListSubtitleTimeline::Prepared => 1,
        };
        let mut ids = Vec::new();
        let mut guard = NewFiles {
            directory: None,
            files: Vec::new(),
        };
        for (index, (artifact, source_path)) in validated.into_iter().enumerate() {
            let key = subtitle_cache_key(&job.input.context_fingerprint, &artifact.language);
            let existing = sqlx::query(
                r#"SELECT "Id","RelativePath","Sha256","ByteLength","SubtitleTimeline","State",
                "AddonMediaListItemId","OperationContextFingerprint","SubtitleLanguage"
                FROM "AddonMediaListArtifact" WHERE "AddonMediaListId"=? AND "CacheKey"=?"#,
            )
            .bind(job.list_id)
            .bind(&key)
            .fetch_optional(&mut *tx)
            .await
            .repository()?;
            if let Some(row) = existing {
                if row.try_get::<i32, _>("State").repository()? != 2
                    || row
                        .try_get::<Option<i64>, _>("AddonMediaListItemId")
                        .repository()?
                        != Some(job.item_id)
                    || row
                        .try_get::<String, _>("OperationContextFingerprint")
                        .repository()?
                        != job.input.context_fingerprint
                    || row.try_get::<String, _>("SubtitleLanguage").repository()?
                        != artifact.language
                    || row.try_get::<i32, _>("SubtitleTimeline").repository()? != clock
                    || row.try_get::<String, _>("Sha256").repository()? != artifact.sha256
                    || row.try_get::<i64, _>("ByteLength").repository()?
                        != i64::try_from(artifact.byte_length).map_err(|_| invalid())?
                {
                    return Err(invalid());
                }
                let path = confined_file(
                    &reservation.root,
                    &row.try_get::<String, _>("RelativePath").repository()?,
                )
                .await?;
                let actual = crate::addon_media_list_artifacts::inspect_file(
                    &path,
                    MAXIMUM_SUBTITLE_FILE_BYTES,
                )
                .await?;
                if actual != (artifact.byte_length, artifact.sha256.clone()) {
                    return Err(invalid());
                }
                ids.push(row.try_get("Id").repository()?);
                continue;
            }
            let relative = format!(
                "objects/{}-{}-{}-subtitle-{index}.srt",
                reservation.id, job.generation, job.lease_owner
            );
            let target = reservation.root.join(&relative);
            if tokio::fs::symlink_metadata(&target).await.is_ok() {
                return Err(invalid());
            }
            tokio::fs::rename(source_path, &target)
                .await
                .map_err(|_| invalid())?;
            guard.files.push(target);
            let id = sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" (
                "AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","Sha256",
                "OperationContextFingerprint","SubtitleLanguage","SubtitleTimeline","CreatedAt","UpdatedAt","CompletedAt","LastUsedAt")
                VALUES (?,?,?,0,2,?,?,?,?,?,?,?,?,?,?)"#)
                .bind(job.list_id).bind(job.item_id).bind(key).bind(relative)
                .bind(i64::try_from(artifact.byte_length).map_err(|_| invalid())?).bind(&artifact.sha256)
                .bind(&job.input.context_fingerprint).bind(&artifact.language).bind(clock)
                .bind(&time).bind(&time).bind(&time).bind(&time)
                .execute(&mut *tx).await.repository()?.last_insert_rowid();
            ids.push(id);
        }
        if timeline == MediaListSubtitleTimeline::Prepared {
            sqlx::query(
                r#"UPDATE "AddonMediaListArtifact" SET "PreparedSubtitleIdsJson"=? WHERE "Id"=?"#,
            )
            .bind(serde_json::to_string(&ids).map_err(|_| invalid())?)
            .bind(reservation.id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        tx.commit().await.repository()?;
        guard.files.clear();
        Ok(ids)
    }
}

pub(crate) fn subtitle_cache_key(context: &str, language: &str) -> String {
    let bytes = serde_json::to_vec(&("ersatzrs:subtitle-artifact:v1", context, language))
        .expect("string tuple serializes");
    hex_digest(&bytes)
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            write!(text, "{byte:02x}").expect("string write");
            text
        })
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("subtitle artifact ownership or validation failed".into())
}

// Only newly created, not-yet-committed paths belong to this bounded rollback guard.
struct NewFiles {
    directory: Option<PathBuf>,
    files: Vec<PathBuf>,
}
impl Drop for NewFiles {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = std::fs::remove_file(file);
        }
        if let Some(directory) = &self.directory {
            let _ = std::fs::remove_dir(directory);
        }
    }
}

// Reuses the installed POSIX fixture without executing its provider script.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::addon_media_list_item_jobs::{
        AddonMediaListItemJobQueue, ItemOperationCompletion, ItemOperationInput,
    };

    async fn fixture() -> (
        tempfile::TempDir,
        DbPool,
        ClaimedItemOperation,
        SubtitleReservation,
    ) {
        let temporary = tempfile::tempdir().unwrap();
        let pool =
            crate::addon_media_list_preparation::tests::fixture(&temporary.path().join("package"))
                .await;
        for (key, value) in [
            (
                ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE,
                "en",
            ),
            (
                ersatzrs_domain::CONFIG_KEY_FFMPEG_FALLBACK_SUBTITLE_LANGUAGE_CODE,
                "nl",
            ),
        ] {
            sqlx::query(
                r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,?)
                ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
            )
            .bind(key)
            .bind(value)
            .execute(&pool)
            .await
            .unwrap();
        }
        let options = crate::addon_media_list_options::load_draft_item_options(&pool, 1, 10)
            .await
            .unwrap()
            .unwrap();
        let runtime = crate::addon_media_list_item_runtime::InstalledItemRuntime::load(
            &pool,
            &options.schema.addon_id,
            None,
        )
        .await
        .unwrap();
        let mut input = ItemOperationInput {
            source_url: "https://example.test/item".into(),
            provider_id: "item".into(),
            context_fingerprint: String::new(),
            schema: Some(options.schema),
            definition: Some(options.definition),
            options: Some(options.values),
            addon_id: None,
            preparation_v2: None,
            package_identity: runtime.package_identity().into(),
            tool_version: "1.0".into(),
            item_is_finite: true,
            max_video_height: None,
            requested_timing: None,
            requested_subtitle_languages: vec!["en".into(), "nl".into()],
        };
        input.context_fingerprint = input.base_fingerprint().unwrap();
        let queue = AddonMediaListItemJobQueue::new(pool.clone());
        queue
            .enqueue(1, 10, 0, ItemOperationKind::Subtitles, input, Utc::now())
            .await
            .unwrap();
        let job = queue
            .claim_kind(Utc::now(), 90, ItemOperationKind::Subtitles)
            .await
            .unwrap()
            .unwrap();
        let reservation = SubtitleArtifactStore::new(pool.clone())
            .reserve(&job, &temporary.path().join("artifacts"), Utc::now())
            .await
            .unwrap();
        (temporary, pool, job, reservation)
    }

    async fn sidecar(
        reservation: &SubtitleReservation,
        language: &str,
    ) -> MediaListSubtitleArtifact {
        let bytes = b"1\n00:00:00,000 --> 00:00:01,000\nManual caption\n";
        let relative_path = format!("{language}.srt");
        tokio::fs::write(reservation.staging_directory.join(&relative_path), bytes)
            .await
            .unwrap();
        MediaListSubtitleArtifact {
            language: language.into(),
            format: MediaListSubtitleFormat::Srt,
            provenance: MediaListSubtitleProvenance::Manual,
            timeline: MediaListSubtitleTimeline::Source,
            relative_path,
            byte_length: bytes.len() as u64,
            sha256: hex_digest(bytes),
        }
    }

    #[tokio::test]
    async fn source_subtitle_artifacts_publish_two_owned_languages_and_release_scratch() {
        let (_temporary, pool, job, reservation) = fixture().await;
        let store = SubtitleArtifactStore::new(pool.clone());
        let artifacts = vec![
            sidecar(&reservation, "en-US").await,
            sidecar(&reservation, "nl").await,
        ];
        let ids = store
            .promote(&job, &reservation, &artifacts, Utc::now())
            .await
            .unwrap();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        let durations: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State"=2 AND "DurationMilliseconds" IS NOT NULL"#)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(durations, 0);
        store
            .release_after_cleanup(&job, &reservation)
            .await
            .unwrap();
        assert!(!reservation.staging_directory.exists());
        AddonMediaListItemJobQueue::new(pool)
            .complete(
                &job,
                ItemOperationCompletion::Subtitles { artifact_ids: ids },
                Utc::now(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn source_subtitle_artifacts_reject_invalid_second_file_atomically() {
        let (_temporary, pool, job, reservation) = fixture().await;
        let store = SubtitleArtifactStore::new(pool.clone());
        let first = sidecar(&reservation, "en").await;
        let mut second = sidecar(&reservation, "nl").await;
        second.sha256 = "0".repeat(64);
        assert!(store
            .promote(&job, &reservation, &[first, second], Utc::now())
            .await
            .is_err());
        let ready: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State"=2"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(ready, 0);
        store
            .release_after_cleanup(&job, &reservation)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn exact_subtitle_recovery_removes_only_uncommitted_attempt_targets() {
        let (_temporary, pool, job, reservation) = fixture().await;
        let store = SubtitleArtifactStore::new(pool.clone());
        let artifact = sidecar(&reservation, "en").await;
        let ids = store
            .promote(&job, &reservation, &[artifact], Utc::now())
            .await
            .unwrap();
        let ready_path: String = sqlx::query_scalar(
            r#"SELECT "RelativePath" FROM "AddonMediaListArtifact" WHERE "Id"=?"#,
        )
        .bind(ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
        let orphan = reservation.root.join(format!(
            "objects/{}-{}-{}-subtitle-1.srt",
            reservation.id, job.generation, job.lease_owner
        ));
        tokio::fs::write(&orphan, b"uncommitted rename")
            .await
            .unwrap();
        let unrelated = reservation.root.join("objects/unrelated.srt");
        tokio::fs::write(&unrelated, b"unrelated immutable file")
            .await
            .unwrap();
        AddonMediaListArtifactStore::new(pool.clone())
            .recover_exact_after_cleanup(&reservation.root, &(&job).into())
            .await
            .unwrap();
        assert!(!orphan.exists());
        assert!(!reservation.staging_directory.exists());
        assert!(reservation.root.join(ready_path).exists());
        assert!(unrelated.exists());
    }

    #[tokio::test]
    async fn source_subtitle_absence_and_changed_language_fence() {
        let (_temporary, pool, job, reservation) = fixture().await;
        let store = SubtitleArtifactStore::new(pool.clone());
        assert!(store
            .promote(&job, &reservation, &[], Utc::now())
            .await
            .unwrap()
            .is_empty());
        sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='de' WHERE "Key"=?"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_SUBTITLE_LANGUAGE_CODE)
            .execute(&pool)
            .await
            .unwrap();
        assert!(store
            .promote(&job, &reservation, &[], Utc::now())
            .await
            .is_err());
        store
            .release_after_cleanup(&job, &reservation)
            .await
            .unwrap();
    }
}
