//! Advisory file-content usage, independent of preparation admission/reservations.
//! Counts owned files and uploaded artwork payloads, not `SQLite` page/WAL overhead,
//! filesystem allocation overhead, or anonymous temporary files with no list owner.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository as _, ADDON_LIST_DISK_WARNING_GB_BYTES,
};
use serde::Deserialize;

use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};
use crate::{
    file_system_layout::{addon_data_folder, addon_processed_media_folder},
    repositories::SqliteAddonMediaListRepository,
};

const TITLE: &str = "Add-on list disk usage";
const UNAVAILABLE: &str = "Add-on list disk usage could not be measured completely. Check storage access and retry the health check.";
const MAX_ENTRIES: usize = 100_000;

pub(crate) struct AddonListDiskUsageHealthCheck {
    pool: Pool<Db>,
}

impl AddonListDiskUsageHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for AddonListDiskUsageHealthCheck {
    fn title(&self) -> &'static str {
        TITLE
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let root = addon_processed_media_folder();
        let measurement = tokio::time::timeout(Duration::from_secs(10), measure(&self.pool, &root))
            .await
            .unwrap_or_else(|_| Err(unavailable()));
        Ok(usage_result(measurement))
    }
}

fn usage_result(measurement: Result<Vec<String>>) -> HealthCheckResult {
    match measurement {
        Ok(lines) if lines.is_empty() => {
            HealthCheckResult::new(TITLE, HealthCheckStatus::Pass, "", "")
        }
        Ok(lines) => HealthCheckResult::new(
            TITLE,
            HealthCheckStatus::Warning,
            lines.join("\n"),
            "Add-on list disk usage warning",
        )
        .with_link("/media/addon-lists"),
        Err(_) => HealthCheckResult::new(
            TITLE,
            HealthCheckStatus::Warning,
            UNAVAILABLE,
            "Add-on list disk usage could not be measured",
        )
        .with_link("/media/addon-lists"),
    }
}

fn unavailable() -> HealthCheckError {
    HealthCheckError::Database(UNAVAILABLE.to_owned())
}

async fn measure(pool: &Pool<Db>, artifact_root: &Path) -> Result<Vec<String>> {
    let repository = SqliteAddonMediaListRepository(pool.clone());
    let combined_gb = repository
        .disk_warning_gb(None)
        .await
        .map_err(|_| unavailable())?;
    let lists = sqlx::query(
        r#"SELECT "Id", "DiskUsageWarningGb", "PublicationJournalJson"
           FROM "AddonMediaList" ORDER BY "Id" LIMIT 10001"#,
    )
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    if lists.len() > 10_000 {
        return Err(unavailable());
    }
    if lists.is_empty()
        || (combined_gb == 0
            && lists.iter().all(|row| {
                row.try_get::<i64, _>("DiskUsageWarningGb")
                    .is_ok_and(|value| value == 0)
            }))
    {
        return Ok(Vec::new());
    }
    let (revision_parent, owner) = repository
        .revision_disk_storage()
        .await
        .map_err(|_| unavailable())?;
    let mut files = MeasuredFiles::default();
    let mut lines = Vec::new();
    let mut combined = 0_u64;
    for list in lists {
        let id: i64 = list.try_get("Id").map_err(sqlx_error)?;
        let warning_gb = u32::try_from(
            list.try_get::<i64, _>("DiskUsageWarningGb")
                .map_err(sqlx_error)?,
        )
        .map_err(|_| unavailable())?;
        if id <= 0 {
            return Err(unavailable());
        }
        if combined_gb == 0 && warning_gb == 0 {
            continue;
        }
        let journal: Option<String> = list.try_get("PublicationJournalJson").map_err(sqlx_error)?;
        let mut roots = owned_paths(pool, id, artifact_root, journal.as_deref()).await?;
        roots.push(OwnedPath {
            base: revision_parent.clone(),
            relative: Path::new(".ersatzrs-revision-blobs")
                .join(&owner)
                .join(id.to_string()),
        });
        let upload_bytes: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM(length("Bytes")), 0)
               FROM "AddonMediaListArtworkUpload" WHERE "AddonMediaListId"=?"#,
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(sqlx_error)?;
        let mut bytes = u64::try_from(upload_bytes).map_err(|_| unavailable())?;
        for root in roots {
            bytes = bytes
                .checked_add(files.measure(id, root).await?)
                .ok_or_else(unavailable)?;
        }
        combined = combined.checked_add(bytes).ok_or_else(unavailable)?;
        if let Some(line) = warning_line(&format!("List #{id}"), bytes, warning_gb) {
            lines.push(line);
        }
    }
    if let Some(line) = warning_line("Combined", combined, combined_gb) {
        lines.push(line);
    }
    Ok(lines)
}

fn warning_line(label: &str, bytes: u64, warning_gb: u32) -> Option<String> {
    (warning_gb > 0 && bytes >= u64::from(warning_gb) * ADDON_LIST_DISK_WARNING_GB_BYTES).then(
        || {
            // Format without floating-point rounding or loss of precision.
            let whole = bytes / ADDON_LIST_DISK_WARNING_GB_BYTES;
            let fraction = bytes % ADDON_LIST_DISK_WARNING_GB_BYTES / 1_000_000;
            format!("{label}: {whole}.{fraction:03} GB used; warning at {warning_gb} GB.")
        },
    )
}

#[derive(Deserialize)]
#[allow(clippy::struct_field_names)] // Preserve the existing publication journal's JSON fields.
struct JournalPaths {
    managed_folder: Option<PathBuf>,
    backup_folder: Option<PathBuf>,
    failed_folder: Option<PathBuf>,
}

struct OwnedPath {
    base: PathBuf,
    relative: PathBuf,
}

#[allow(clippy::too_many_lines)] // Enumerate the existing owned stores without introducing a second storage model.
async fn owned_paths(
    pool: &Pool<Db>,
    id: i64,
    artifact_root: &Path,
    journal: Option<&str>,
) -> Result<Vec<OwnedPath>> {
    let mut roots = Vec::new();
    let artifacts = sqlx::query(
        r#"SELECT "RelativePath", "PreparationJobId", "PreparationGeneration",
                  "PreparationLeaseOwner" FROM "AddonMediaListArtifact"
           WHERE "AddonMediaListId"=? LIMIT 100001"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    if artifacts.len() > MAX_ENTRIES {
        return Err(unavailable());
    }
    for artifact in artifacts {
        if let Some(relative) = artifact
            .try_get::<Option<String>, _>("RelativePath")
            .map_err(sqlx_error)?
        {
            roots.push(OwnedPath {
                base: artifact_root.to_path_buf(),
                relative: safe_relative(&relative)?.to_path_buf(),
            });
        }
        let job: Option<i64> = artifact.try_get("PreparationJobId").map_err(sqlx_error)?;
        let generation: Option<i64> = artifact
            .try_get("PreparationGeneration")
            .map_err(sqlx_error)?;
        let owner: Option<String> = artifact
            .try_get("PreparationLeaseOwner")
            .map_err(sqlx_error)?;
        if let (Some(job), Some(generation), Some(owner)) = (job, generation, owner) {
            roots.push(OwnedPath {
                base: artifact_root.to_path_buf(),
                relative: crate::addon_media_list_artifacts::stage_path(
                    Path::new(""),
                    job,
                    generation,
                    &owner,
                )
                .map_err(|_| unavailable())?,
            });
        }
    }
    let managed = sqlx::query(
        r#"SELECT "source"."RootPath", "library"."FolderName"
           FROM "AddonMediaListLibrary" AS "library"
           JOIN "AddonMediaList" AS "list" ON "list"."Id"="library"."AddonMediaListId"
           JOIN "AddonMediaSource" AS "source" ON "source"."AddonId"="list"."AddonId"
           WHERE "list"."Id"=?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(sqlx_error)?;
    if let Some(managed) = managed {
        let parent: String = managed.try_get("RootPath").map_err(sqlx_error)?;
        let folder: String = managed.try_get("FolderName").map_err(sqlx_error)?;
        let folder = safe_relative(&folder)?;
        if folder.components().count() != 1 {
            return Err(unavailable());
        }
        roots.push(OwnedPath {
            base: PathBuf::from(parent),
            relative: folder.to_path_buf(),
        });
    }
    if let Some(journal) = journal {
        append_journal_paths(&mut roots, id, journal)?;
    }
    let historical: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT "DataRoot" FROM "AddonMediaListImportCheckpoint"
           WHERE "AddonMediaListId"=? AND "DataRoot" IS NOT NULL LIMIT 100001"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    if historical.len() > MAX_ENTRIES {
        return Err(unavailable());
    }
    for root in historical {
        roots.push(OwnedPath {
            base: PathBuf::from(root),
            relative: Path::new("media-list-checkpoints").join(format!("list-{id}")),
        });
    }
    let installation = sqlx::query(
        r#"SELECT "installation"."DataNamespace", "list"."AddonId"
           FROM "AddonMediaList" AS "list"
           JOIN "AddonInstallation" AS "installation" ON "installation"."AddonId"="list"."AddonId"
           WHERE "list"."Id"=?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(sqlx_error)?;
    if let Some(installation) = installation {
        let namespace: String = installation.try_get("DataNamespace").map_err(sqlx_error)?;
        let addon: String = installation.try_get("AddonId").map_err(sqlx_error)?;
        if safe_relative(&namespace)?.components().count() != 1
            || safe_relative(&addon)?.components().count() != 1
        {
            return Err(unavailable());
        }
        roots.push(OwnedPath {
            base: addon_data_folder(&namespace, &addon),
            relative: Path::new("media-list-checkpoints").join(format!("list-{id}")),
        });
    }
    if roots.len() > MAX_ENTRIES {
        return Err(unavailable());
    }
    Ok(roots)
}

fn append_journal_paths(roots: &mut Vec<OwnedPath>, id: i64, journal: &str) -> Result<()> {
    let paths: JournalPaths = serde_json::from_str(journal).map_err(|_| unavailable())?;
    if let Some(managed) = paths.managed_folder {
        let parent = managed.parent().ok_or_else(unavailable)?;
        for (backup, prefix) in [
            (paths.backup_folder, ".ersatzrs-publication-"),
            (paths.failed_folder, ".ersatzrs-failed-publication-"),
        ] {
            if let Some(backup) = backup {
                if backup.parent() != Some(parent)
                    || !backup
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with(&format!("{prefix}{id}-")))
                {
                    return Err(unavailable());
                }
                if prefix == ".ersatzrs-publication-" {
                    let mut initializer =
                        backup.file_name().ok_or_else(unavailable)?.to_os_string();
                    initializer.push(".initializing");
                    roots.push(OwnedPath {
                        base: parent.to_path_buf(),
                        relative: PathBuf::from(initializer),
                    });
                }
                roots.push(OwnedPath {
                    base: parent.to_path_buf(),
                    relative: PathBuf::from(backup.file_name().ok_or_else(unavailable)?),
                });
            }
        }
        roots.push(OwnedPath {
            base: parent.to_path_buf(),
            relative: PathBuf::from(managed.file_name().ok_or_else(unavailable)?),
        });
    }
    Ok(())
}

fn safe_relative(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains(['\\', ':'])
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(unavailable());
    }
    Ok(path)
}

struct MeasuredFiles {
    owners: BTreeMap<PathBuf, i64>,
    examined: usize,
    started: Instant,
}

impl Default for MeasuredFiles {
    fn default() -> Self {
        Self {
            owners: BTreeMap::new(),
            examined: 0,
            started: Instant::now(),
        }
    }
}

impl MeasuredFiles {
    async fn measure(&mut self, list_id: i64, owned: OwnedPath) -> Result<u64> {
        self.examined += 1;
        if self.examined > MAX_ENTRIES || self.started.elapsed() > Duration::from_secs(10) {
            return Err(unavailable());
        }
        if owned.relative.as_os_str().is_empty()
            || !owned
                .relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(unavailable());
        }
        // Writers accept symlinks in the configured storage boundary. Resolve
        // that boundary, then reject symlinks in every list-owned descendant.
        let mut root = match tokio::fs::canonicalize(owned.base).await {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(_) => return Err(unavailable()),
        };
        for component in owned.relative.components() {
            root.push(component);
            match tokio::fs::symlink_metadata(&root).await {
                Ok(metadata) if metadata.is_symlink() => return Err(unavailable()),
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
                Err(_) => return Err(unavailable()),
            }
        }
        let mut pending = vec![root];
        let mut bytes = 0_u64;
        while let Some(path) = pending.pop() {
            self.examined += 1;
            if self.examined > MAX_ENTRIES || self.started.elapsed() > Duration::from_secs(10) {
                return Err(unavailable());
            }
            let metadata = match tokio::fs::symlink_metadata(&path).await {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(unavailable()),
            };
            if metadata.is_symlink() {
                return Err(unavailable());
            }
            if let Some(previous) = self.owners.insert(path.clone(), list_id) {
                if previous != list_id {
                    return Err(unavailable());
                }
                continue;
            }
            if metadata.is_file() {
                bytes = bytes.checked_add(metadata.len()).ok_or_else(unavailable)?;
            } else if metadata.is_dir() {
                let mut entries = tokio::fs::read_dir(path).await.map_err(|_| unavailable())?;
                while let Some(entry) = entries.next_entry().await.map_err(|_| unavailable())? {
                    if pending.len() + self.examined >= MAX_ENTRIES
                        || self.started.elapsed() > Duration::from_secs(10)
                    {
                        return Err(unavailable());
                    }
                    pending.push(entry.path());
                }
            } else {
                return Err(unavailable());
            }
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::ADDON_LIST_TOTAL_DISK_WARNING_GB_KEY;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    async fn fixture() -> (tempfile::TempDir, PathBuf, Pool<Db>) {
        let temporary = tempfile::tempdir().expect("temporary storage");
        let root = std::fs::canonicalize(temporary.path()).expect("canonical temporary storage");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(root.join("fixture.db"))
                    .create_if_missing(true),
            )
            .await
            .expect("fixture pool");
        // Minimal read model: production migrations are covered by the migration
        // integration suite; these fixtures exercise file lengths and ownership.
        sqlx::raw_sql(r#"
            CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT);
            INSERT INTO "ConfigElement" VALUES ('addon_media_list.revision_blob_store_id', '0123456789abcdef0123456789abcdef');
            CREATE TABLE "AddonMediaList" ("Id" INTEGER PRIMARY KEY, "AddonId" TEXT, "DiskUsageWarningGb" INTEGER NOT NULL DEFAULT 25, "PublicationJournalJson" TEXT);
            INSERT INTO "AddonMediaList" ("Id", "AddonId", "DiskUsageWarningGb") VALUES (1, 'org.example.disk', 1), (2, 'org.example.disk', 1);
            CREATE TABLE "AddonMediaListArtifact" ("AddonMediaListId" INTEGER, "RelativePath" TEXT, "PreparationJobId" INTEGER, "PreparationGeneration" INTEGER, "PreparationLeaseOwner" TEXT, "ByteLength" INTEGER, "ReservedBytes" INTEGER);
            CREATE TABLE "AddonMediaListLibrary" ("AddonMediaListId" INTEGER, "FolderName" TEXT);
            CREATE TABLE "AddonMediaSource" ("AddonId" TEXT, "RootPath" TEXT);
            CREATE TABLE "AddonMediaListImportCheckpoint" ("AddonMediaListId" INTEGER, "DataRoot" TEXT);
            CREATE TABLE "AddonInstallation" ("AddonId" TEXT, "DataNamespace" TEXT);
            CREATE TABLE "AddonMediaListArtworkUpload" ("AddonMediaListId" INTEGER, "Bytes" BLOB);
            CREATE TABLE "AddonMediaListRevisionArtifact" ("AddonMediaListRevisionId" INTEGER, "AddonMediaListArtifactId" INTEGER);
            CREATE TABLE "AddonMediaListRevisionFile" ("AddonMediaListRevisionId" INTEGER, "AddonMediaListFileBlobId" INTEGER);
        "#).execute(&pool).await.expect("fixture schema");
        (temporary, root, pool)
    }

    fn file(path: &Path, bytes: u64) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
        // Sparse logical lengths exercise decimal-GB thresholds without writing
        // a gigabyte of contents or relying on filesystem allocation semantics.
        std::fs::File::create(path)
            .expect("file")
            .set_len(bytes)
            .expect("file length");
    }

    async fn combined_threshold(pool: &Pool<Db>, value: u32) {
        sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(ADDON_LIST_TOTAL_DISK_WARNING_GB_KEY)
            .bind(value.to_string())
            .execute(pool)
            .await
            .expect("combined threshold");
    }

    #[tokio::test]
    async fn thresholds_use_actual_lengths_and_zero_disables_independently() {
        let (_temporary, root, pool) = fixture().await;
        let artifacts = root.join("artifacts");
        file(&artifacts.join("objects/first.bin"), 600_000_000);
        file(&artifacts.join("objects/second.bin"), 600_000_000);
        sqlx::raw_sql(r#"
            INSERT INTO "AddonMediaListArtifact" ("AddonMediaListId", "RelativePath", "ByteLength", "ReservedBytes") VALUES (1, 'objects/first.bin', 999000000000, 999000000000), (2, 'objects/second.bin', 999000000000, 999000000000);
            INSERT INTO "AddonMediaListRevisionArtifact" VALUES (1, 1), (2, 1), (3, 1);
        "#).execute(&pool).await.expect("artifacts with repeated pins and stale size metadata");
        combined_threshold(&pool, 1).await;
        assert_eq!(
            measure(&pool, &artifacts).await.unwrap(),
            vec!["Combined: 1.200 GB used; warning at 1 GB."]
        );
        file(&artifacts.join("objects/first.bin"), 1_000_000_000);
        assert_eq!(
            measure(&pool, &artifacts).await.unwrap(),
            vec![
                "List #1: 1.000 GB used; warning at 1 GB.",
                "Combined: 1.600 GB used; warning at 1 GB."
            ]
        );
        combined_threshold(&pool, 0).await;
        assert_eq!(
            measure(&pool, &artifacts).await.unwrap(),
            vec!["List #1: 1.000 GB used; warning at 1 GB."]
        );
        sqlx::query(r#"UPDATE "AddonMediaList" SET "DiskUsageWarningGb"=0"#)
            .execute(&pool)
            .await
            .unwrap();
        combined_threshold(&pool, 1).await;
        assert_eq!(
            measure(&pool, &artifacts).await.unwrap(),
            vec!["Combined: 1.600 GB used; warning at 1 GB."]
        );
        combined_threshold(&pool, 0).await;
        // Prove all-disabled returns before touching corrupt storage metadata.
        sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "RelativePath"='../outside'"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(measure(&pool, &artifacts).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn removed_files_clear_warning_and_reservations_never_count_as_usage() {
        let (_temporary, root, pool) = fixture().await;
        let artifact = root.join("objects/item.bin");
        file(&artifact, 1_000_000_000);
        sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("AddonMediaListId", "RelativePath", "ByteLength", "ReservedBytes") VALUES (1, 'objects/item.bin', 1000000000, 999000000000)"#).execute(&pool).await.unwrap();
        combined_threshold(&pool, 1).await;
        assert_eq!(measure(&pool, &root).await.unwrap().len(), 2);
        std::fs::remove_file(artifact).expect("remove fixture file");
        let result = usage_result(measure(&pool, &root).await);
        assert_eq!(result.status, HealthCheckStatus::Pass);
        assert!(result.message.is_empty());
    }

    #[tokio::test]
    async fn uploads_count_once_alongside_files_and_equal_threshold_warns() {
        let (_temporary, root, pool) = fixture().await;
        file(&root.join("objects/item.bin"), 999_999_995);
        sqlx::raw_sql(r#"
            INSERT INTO "AddonMediaListArtifact" ("AddonMediaListId", "RelativePath") VALUES (1, 'objects/item.bin');
            INSERT INTO "AddonMediaListArtworkUpload" VALUES (1, X'0102030405');
        "#).execute(&pool).await.unwrap();
        combined_threshold(&pool, 1).await;
        assert_eq!(
            measure(&pool, &root).await.unwrap(),
            vec![
                "List #1: 1.000 GB used; warning at 1 GB.",
                "Combined: 1.000 GB used; warning at 1 GB."
            ]
        );
    }

    #[tokio::test]
    async fn owned_stores_include_staging_copies_and_deduplicate_paths() {
        let (_temporary, root, pool) = fixture().await;
        let artifacts = root.join("artifacts");
        let attempt = "01234567-89ab-cdef-0123-456789abcdef";
        let stage =
            crate::addon_media_list_artifacts::stage_path(&artifacts, 7, 1, attempt).unwrap();
        let managed = root.join("list");
        let backup = root.join(".ersatzrs-publication-1-fixture");
        let failed = root.join(".ersatzrs-failed-publication-1-fixture");
        let initializer = root.join(".ersatzrs-publication-1-fixture.initializing");
        let checkpoint = root.join("provider/media-list-checkpoints/list-1");
        let blob = root.join(".ersatzrs-revision-blobs/0123456789abcdef0123456789abcdef/1");
        for path in [
            artifacts.join("objects/subtitle.srt"),
            stage.join("download.part"),
            managed.join("artwork.jpg"),
            backup.join("item.bin"),
            failed.join("item.bin"),
            initializer.join("item.bin"),
            checkpoint.join(".staging/attempt/archive.txt"),
            checkpoint.join("generation-1/archive.txt"),
            blob.join("ab/blob"),
            blob.join("temporary-blob"),
        ] {
            file(&path, 100_000_000);
        }
        sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("AddonMediaListId", "RelativePath", "PreparationJobId", "PreparationGeneration", "PreparationLeaseOwner") VALUES (1, 'objects/subtitle.srt', 7, 1, ?)"#).bind(attempt).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaSource" VALUES ('org.example.disk', ?)"#)
            .bind(root.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListLibrary" VALUES (1, 'list')"#)
            .execute(&pool)
            .await
            .unwrap();
        let journal = serde_json::json!({"managed_folder": managed, "backup_folder": backup, "failed_folder": failed}).to_string();
        sqlx::query(r#"UPDATE "AddonMediaList" SET "PublicationJournalJson"=? WHERE "Id"=1"#)
            .bind(journal)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListImportCheckpoint" VALUES (1, ?)"#)
            .bind(root.join("provider").to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(r#"INSERT INTO "AddonMediaListRevisionFile" VALUES (1, 1), (2, 1), (3, 1)"#)
            .execute(&pool)
            .await
            .unwrap();
        combined_threshold(&pool, 1).await;
        assert_eq!(
            measure(&pool, &artifacts).await.unwrap(),
            vec![
                "List #1: 1.000 GB used; warning at 1 GB.",
                "Combined: 1.000 GB used; warning at 1 GB."
            ]
        );
    }

    #[test]
    fn decimal_threshold_boundaries_and_unavailable_are_advisory() {
        assert!(warning_line("List #1", 24_999_999_999, 25).is_none());
        assert_eq!(
            warning_line("List #1", 25_000_000_000, 25).as_deref(),
            Some("List #1: 25.000 GB used; warning at 25 GB.")
        );
        assert_eq!(
            warning_line("Combined", 250_001_000_000, 250).as_deref(),
            Some("Combined: 250.001 GB used; warning at 250 GB.")
        );
        assert!(warning_line("Combined", u64::MAX, 0).is_none());
        let result = usage_result(Err(unavailable()));
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(result.message, UNAVAILABLE);
    }

    #[tokio::test]
    async fn traversal_bound_reports_incomplete_and_does_not_pass_with_zero() {
        let (_temporary, root, _pool) = fixture().await;
        file(&root.join("owned/file.bin"), 12);
        let mut files = MeasuredFiles {
            examined: MAX_ENTRIES,
            ..MeasuredFiles::default()
        };
        let result = files
            .measure(
                1,
                OwnedPath {
                    base: root.clone(),
                    relative: PathBuf::from("owned"),
                },
            )
            .await;
        assert!(result.is_err());
        let result = files
            .measure(
                1,
                OwnedPath {
                    base: root,
                    relative: PathBuf::from("removed-stage"),
                },
            )
            .await;
        assert!(result.is_err());
    }

    // Symlink creation on Windows requires privileges or Developer Mode; the
    // platform-neutral traversal itself rejects symlinks on every target.
    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_files_and_ancestors_are_not_followed() {
        let (_temporary, root, _pool) = fixture().await;
        file(&root.join("outside/file.bin"), 12);
        std::os::unix::fs::symlink(root.join("outside"), root.join("linked")).unwrap();
        for relative in ["linked", "linked/file.bin"] {
            assert!(MeasuredFiles::default()
                .measure(
                    1,
                    OwnedPath {
                        base: root.clone(),
                        relative: PathBuf::from(relative)
                    }
                )
                .await
                .is_err());
        }
        assert_eq!(
            MeasuredFiles::default()
                .measure(
                    1,
                    OwnedPath {
                        base: root.join("linked"),
                        relative: PathBuf::from("file.bin")
                    }
                )
                .await
                .unwrap(),
            12
        );
        std::fs::create_dir(root.join("owned")).unwrap();
        std::os::unix::fs::symlink(root.join("outside/file.bin"), root.join("owned/file.bin"))
            .unwrap();
        assert!(MeasuredFiles::default()
            .measure(
                1,
                OwnedPath {
                    base: root,
                    relative: PathBuf::from("owned")
                }
            )
            .await
            .is_err());
    }
}
