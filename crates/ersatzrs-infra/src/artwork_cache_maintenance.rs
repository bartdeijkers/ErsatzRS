//! Filesystem portion of upstream `DeleteOrphanedArtworkHandler` and `ImageCache`.
//!
//! Native scans register originals directly. Only separator-free compatibility
//! cache entries are repaired; originals and dedicated upload stores are never pruned.

use std::{
    collections::HashSet,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
    time::UNIX_EPOCH,
};

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use sha1::{Digest, Sha1};
use sqlx::FromRow;
use tokio::{fs, sync::Mutex};

use crate::{
    artwork_cleanup::{clean_orphaned_artwork, OrphanedArtworkCleanupReport},
    db::{self, DbPool},
};

#[derive(Debug, thiserror::Error)]
pub enum ArtworkCacheError {
    #[error("artwork cache database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("artwork cache filesystem operation failed")]
    FileSystem(#[from] std::io::Error),
}

/// Aggregate counters deliberately contain no source paths or library names.
#[derive(Debug, Default)]
pub struct ArtworkCacheReport {
    pub database: OrphanedArtworkCleanupReport,
    pub files_deleted: u64,
    pub bytes_deleted: u64,
    pub files_restored: u64,
    pub rows_repointed: u64,
    pub unrestorable: u64,
    pub file_failures: u64,
}

/// One host-owned service shared by scheduled and manually requested passes.
#[derive(Clone)]
pub struct ArtworkCacheMaintenance {
    pool: DbPool,
    data_root: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl ArtworkCacheMaintenance {
    /// Roots are explicit so tests and standalone database maintenance never
    /// silently operate on the user's artwork cache.
    #[must_use]
    pub fn new(pool: DbPool, data_root: PathBuf) -> Self {
        Self {
            pool,
            data_root,
            lock: Arc::default(),
        }
    }

    /// Prune unreferenced cache files, then repair missing local-image copies.
    /// The limit applies to orphan database rows, as in upstream.
    ///
    /// # Errors
    /// Database and directory-enumeration failures abort the pass for retry.
    /// Individual file failures are counted and retried on the next pass.
    pub async fn run(&self, limit: i32) -> Result<ArtworkCacheReport, ArtworkCacheError> {
        let _guard = self.lock.lock().await;
        let mut report = ArtworkCacheReport {
            database: clean_orphaned_artwork(&self.pool, limit).await?,
            ..ArtworkCacheReport::default()
        };
        self.prune(&mut report).await?;
        self.restore(&mut report).await?;
        if report.file_failures > 0 {
            tracing::warn!(
                failures = report.file_failures,
                "artwork cache file operations failed; next pass will retry"
            );
        }
        tracing::debug!(?report, "completed artwork cache maintenance");
        Ok(report)
    }

    async fn prune(&self, report: &mut ArtworkCacheReport) -> Result<(), ArtworkCacheError> {
        if !plain_directory(&self.data_root).await? {
            return Ok(());
        }
        let root = self.data_root.join("artwork");
        if !plain_directory(&root).await? {
            return Ok(());
        }
        let retained = self.retained_names().await?;
        let mut pending = vec![root.clone()];
        let mut directories = Vec::new();
        while let Some(directory) = pending.pop() {
            let mut entries = fs::read_dir(&directory).await?;
            while let Some(entry) = entries.next_entry().await? {
                let kind = entry.file_type().await?;
                if kind.is_dir() {
                    pending.push(entry.path());
                } else if kind.is_file()
                    && !entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| retained.contains(name))
                {
                    self.prune_file(&entry.path(), report).await?;
                }
                // Never traverse symlinks, including directory links/junctions.
            }
            directories.push(directory);
        }
        for directory in directories.into_iter().rev() {
            if directory != root && directory.parent() != Some(root.as_path()) {
                match fs::remove_dir(directory).await {
                    Ok(()) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
                        ) => {}
                    Err(_) => report.file_failures += 1,
                }
            }
        }
        Ok(())
    }

    async fn retained_names(&self) -> Result<HashSet<String>, sqlx::Error> {
        let mut retained = HashSet::new();
        let watermarks: Vec<String> = sqlx::query_scalar(
            r#"SELECT "Image" FROM "ChannelWatermark" WHERE "Image" IS NOT NULL"#,
        )
        .fetch_all(&self.pool)
        .await?;
        for path in watermarks {
            retain_path(&mut retained, &path);
        }
        let mut last_id = 0_i64;
        loop {
            let rows: Vec<ReferencedEntry> = sqlx::query_as(
                r#"SELECT "Id" AS id,
                "Path" AS path, "SourcePath" AS source, "BlurHash43" AS blur43,
                "BlurHash54" AS blur54, "BlurHash64" AS blur64 FROM "Artwork"
                WHERE "Id" > ? ORDER BY "Id" LIMIT 1000"#,
            )
            .bind(last_id)
            .fetch_all(&self.pool)
            .await?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                last_id = row.id;
                for path in [row.path, row.source].into_iter().flatten() {
                    retain_path(&mut retained, &path);
                }
                for blur in [row.blur43, row.blur54, row.blur64].into_iter().flatten() {
                    if !blur.trim().is_empty() {
                        retained.insert(
                            STANDARD_NO_PAD
                                .encode(blur)
                                .replace('+', "_")
                                .replace('/', "-"),
                        );
                    }
                }
            }
        }
        Ok(retained)
    }

    async fn prune_file(
        &self,
        path: &Path,
        report: &mut ArtworkCacheReport,
    ) -> Result<(), ArtworkCacheError> {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Ok(());
        };
        let blur = STANDARD_NO_PAD
            .decode(name.replace('_', "+").replace('-', "/"))
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok());
        // Keep the write transaction through unlink: a concurrent owner cannot
        // publish a reference between the final reference check and deletion.
        let mut tx = db::begin_write(&self.pool).await?;
        let referenced: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(SELECT 1 FROM "Artwork" WHERE
                "Path" = ? OR "Path" = ? OR "SourcePath" = ? OR
                "BlurHash43" = ? OR "BlurHash54" = ? OR "BlurHash64" = ?)
                OR EXISTS(SELECT 1 FROM "ChannelWatermark" WHERE "Image" = ? OR "Image" = ?)
        "#,
        )
        .bind(name)
        .bind(path.to_string_lossy().as_ref())
        .bind(path.to_string_lossy().as_ref())
        .bind(&blur)
        .bind(&blur)
        .bind(&blur)
        .bind(name)
        .bind(path.to_string_lossy().as_ref())
        .fetch_one(&mut *tx)
        .await?;
        if !referenced {
            // Recheck the entry type rather than following a replaced link.
            match fs::symlink_metadata(path).await {
                // Unlink is a single local filesystem operation. Keep it
                // synchronous here: a cancelled Tokio filesystem future can
                // otherwise unlink later, after this transaction is dropped.
                Ok(metadata) if metadata.is_file() => match std::fs::remove_file(path) {
                    Ok(()) => {
                        report.files_deleted += 1;
                        report.bytes_deleted += metadata.len();
                    }
                    Err(_) => report.file_failures += 1,
                },
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => report.file_failures += 1,
            }
        }
        tx.commit().await?;
        Ok(())
    }

    async fn restore(&self, report: &mut ArtworkCacheReport) -> Result<(), ArtworkCacheError> {
        let mut last_id = 0_i64;
        let mut restored = HashSet::new();
        loop {
            let rows: Vec<CacheEntry> = sqlx::query_as(
                r#"SELECT "Id" AS id, COALESCE("Path", '') AS path,
                "SourcePath" AS source, "ArtworkKind" AS kind FROM "Artwork"
                WHERE "Id" > ? ORDER BY "Id" LIMIT 1000"#,
            )
            .bind(last_id)
            .fetch_all(&self.pool)
            .await?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                last_id = row.id;
                if !cache_name(&row.path) || restored.contains(&row.path) {
                    continue;
                }
                if self.cached(&row.path, row.kind).await? {
                    continue;
                }
                let Some(source) = row.source.as_deref().filter(|source| local_image(source))
                else {
                    report.unrestorable += 1;
                    continue;
                };
                match self.restore_entry(&row, source).await {
                    Ok(repointed) => {
                        report.files_restored += 1;
                        report.rows_repointed += repointed;
                        restored.insert(row.path);
                    }
                    Err(ArtworkCacheError::Database(error)) => return Err(error.into()),
                    Err(_) => report.file_failures += 1,
                }
            }
        }
        Ok(())
    }

    fn folder(&self, kind: i64) -> PathBuf {
        self.data_root.join(match kind {
            0 => "artwork/posters",
            1 => "artwork/thumbnails",
            2 => "iptv/logos",
            3 => "artwork/fanart",
            4 => "watermarks",
            _ => "artwork/legacy",
        })
    }

    async fn cached(&self, name: &str, kind: i64) -> Result<bool, std::io::Error> {
        let folder = self.folder(kind);
        for path in [folder.join(name), folder.join(&name[..2]).join(name)] {
            match fs::metadata(path).await {
                Ok(metadata) if metadata.is_file() => return Ok(true),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }

    async fn restore_entry(
        &self,
        row: &CacheEntry,
        source: &str,
    ) -> Result<u64, ArtworkCacheError> {
        let metadata = fs::metadata(source).await?;
        if !metadata.is_file() {
            return Err(std::io::Error::other("artwork source is not a file").into());
        }
        let modified = metadata.modified()?;
        let name = cache_key(source, modified);
        // The same stale path can be shared by different owners and kinds.
        // Populate every destination before atomically repointing all owners.
        let kinds: Vec<i64> =
            sqlx::query_scalar(r#"SELECT DISTINCT "ArtworkKind" FROM "Artwork" WHERE "Path" = ?"#)
                .bind(&row.path)
                .fetch_all(&self.pool)
                .await?;
        for kind in kinds {
            let folder = self.folder(kind);
            let folder = if matches!(kind, 2 | 4) {
                folder
            } else {
                folder.join(&name[..2])
            };
            self.ensure_directory(&folder).await?;
            let temp = tempfile::NamedTempFile::new_in(&folder)?;
            fs::copy(source, temp.path()).await?;
            if fs::metadata(source).await?.modified()? != modified {
                return Err(
                    std::io::Error::other("artwork source changed during restoration").into(),
                );
            }
            match temp.persist_noclobber(folder.join(&name)) {
                Ok(_) => {}
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.error.into()),
            }
        }
        Ok(
            sqlx::query(r#"UPDATE "Artwork" SET "Path" = ? WHERE "Path" = ?"#)
                .bind(name)
                .bind(&row.path)
                .execute(&self.pool)
                .await?
                .rows_affected(),
        )
    }

    async fn ensure_directory(&self, path: &Path) -> Result<(), std::io::Error> {
        let mut current = self.data_root.clone();
        if !plain_directory(&current).await? {
            fs::create_dir_all(&current).await?;
        }
        for part in path
            .strip_prefix(&self.data_root)
            .map_err(std::io::Error::other)?
            .components()
        {
            current.push(part);
            if !plain_directory(&current).await? {
                fs::create_dir(&current).await?;
            }
        }
        Ok(())
    }
}

#[derive(FromRow)]
struct CacheEntry {
    id: i64,
    path: String,
    source: Option<String>,
    kind: i64,
}

fn cache_name(name: &str) -> bool {
    name.len() >= 2
        && name.is_ascii()
        && !name.trim().is_empty()
        && !name.contains(['/', '\\', ':'])
        && name != ".."
}

fn local_image(source: &str) -> bool {
    let path = Path::new(source);
    path.is_absolute()
        && path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| {
                ["jpg", "jpeg", "png", "gif", "tbn", "webp"]
                    .iter()
                    .any(|allowed| extension.eq_ignore_ascii_case(allowed))
            })
}

async fn plain_directory(path: &Path) -> Result<bool, std::io::Error> {
    match fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => Ok(true),
        Ok(_) => Err(std::io::Error::other(
            "artwork cache directory is not a real directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[derive(FromRow)]
struct ReferencedEntry {
    id: i64,
    path: Option<String>,
    source: Option<String>,
    blur43: Option<String>,
    blur54: Option<String>,
    blur64: Option<String>,
}

fn retain_path(retained: &mut HashSet<String>, path: &str) {
    if cache_name(path) {
        retained.insert(path.to_owned());
    } else if Path::new(path).is_absolute() {
        if let Some(name) = Path::new(path).file_name().and_then(|name| name.to_str()) {
            retained.insert(name.to_owned());
        }
    }
}

fn cache_key(source: &str, modified: std::time::SystemTime) -> String {
    let epoch_ticks = 116_444_736_000_000_000_i128;
    let duration_ticks = |duration: std::time::Duration| {
        i128::from(duration.as_secs()) * 10_000_000 + i128::from(duration.subsec_nanos() / 100)
    };
    let ticks = match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => epoch_ticks + duration_ticks(duration),
        Err(error) => epoch_ticks - duration_ticks(error.duration()),
    };
    let mut name = String::with_capacity(40);
    for byte in Sha1::digest(format!("{source}:{ticks}")) {
        write!(name, "{byte:02X}").expect("writing to a string");
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reference_published_after_snapshot_prevents_unlink() {
        use sqlx::Executor as _;
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(
            r#"CREATE TABLE "Artwork" ("Id" INTEGER PRIMARY KEY, "Path" TEXT,
            "SourcePath" TEXT, "BlurHash43" TEXT, "BlurHash54" TEXT, "BlurHash64" TEXT);
            CREATE TABLE "ChannelWatermark" ("Image" TEXT)"#,
        )
        .await
        .unwrap();
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("AA11");
        fs::write(&file, b"retained").await.unwrap();
        let service = ArtworkCacheMaintenance::new(pool.clone(), root.path().to_owned());
        assert!(!service.retained_names().await.unwrap().contains("AA11"));
        pool.execute(r#"INSERT INTO "Artwork" ("Id", "Path") VALUES (1, 'AA11')"#)
            .await
            .unwrap();
        let mut report = ArtworkCacheReport::default();
        service.prune_file(&file, &mut report).await.unwrap();
        assert_eq!(report.files_deleted, 0);
        assert_eq!(fs::read(file).await.unwrap(), b"retained");
    }

    #[test]
    fn cache_key_matches_upstream_filetime_sha1_fixture() {
        let modified = UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 123_456_700);
        assert_eq!(
            cache_key("fixture/cover.png", modified),
            "087A7B6069C574B5E15BD23CD8C8BEF50CB0CA07"
        );
    }
}
