//! One-time owned text-caption repair and sanitation before extraction readiness.
//! Upstream row250: `SubtitleNulBytes` and `CacheCleanerService` at fb0e2bc1.

use std::{fs::File, io, path::Path};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};

use crate::{db::DbPool, repositories::SqlxResultExt as _};

pub(crate) const REPAIR_DONE_KEY: &str = "ffmpeg.subtitle_cache_nul_stripped";
pub(crate) const REPAIR_PENDING_KEY: &str = "ffmpeg.subtitle_cache_nul_repair_pending";

/// Outcome of repairing the owned, regenerable text-caption cache.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RepairReport {
    /// Files whose NUL characters were removed atomically.
    pub repaired: usize,
    /// Failed outputs removed so ordinary extraction can regenerate them.
    pub removed: usize,
    /// Unresolved filesystem operations; no private paths are retained.
    pub failed: usize,
    /// All eligible files were handled, or this repair was already complete.
    pub complete: bool,
}

/// Repair old cache files once, retrying interrupted/failed attempts safely.
/// Only ASS/SRT/VTT regular files below the owned root are considered.
///
/// # Errors
/// Returns database errors; filesystem failures leave a pending health warning.
pub async fn repair_once(
    pool: &DbPool,
    root: &Path,
) -> ersatzrs_core::repositories::Result<RepairReport> {
    let done: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "ConfigElement" WHERE "Key"=? AND LOWER("Value") IN ('true','1'))"#,
    ).bind(REPAIR_DONE_KEY).fetch_one(pool).await.repository()?;
    if done {
        return Ok(RepairReport {
            complete: true,
            ..Default::default()
        });
    }
    let mut report = RepairReport::default();
    match fs::symlink_metadata(root).await {
        Err(error) if error.kind() == io::ErrorKind::NotFound => report.complete = true,
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {
            match cache_lease(root, true) {
                Ok(Some(_lease)) => {
                    repair_tree(root, &mut report).await;
                    report.complete = report.failed == 0;
                }
                Ok(None) => {} // An extraction owns the cache; retry without waiting.
                Err(_) => report.failed += 1,
            }
        }
        _ => report.failed += 1,
    }
    record_state(pool, report.complete).await?;
    tracing::info!(?report, "subtitle cache NUL repair pass complete");
    Ok(report)
}

async fn record_state(pool: &DbPool, complete: bool) -> ersatzrs_core::repositories::Result<()> {
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    // A stale busy/failed attempt must not clear a concurrent completed repair.
    let already_complete: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "ConfigElement" WHERE "Key"=? AND LOWER("Value") IN ('true','1'))"#,
    ).bind(REPAIR_DONE_KEY).fetch_one(&mut *tx).await.repository()?;
    if complete || !already_complete {
        sqlx::query(
            r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,?),(?,?)
        ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
        )
        .bind(REPAIR_DONE_KEY)
        .bind(complete.to_string())
        .bind(REPAIR_PENDING_KEY)
        .bind((!complete).to_string())
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    tx.commit().await.repository()?;
    Ok(())
}

/// Share admission during extraction; repair requires exclusive admission.
/// The cache is a trusted application root; entries below it must be ordinary.
pub(crate) fn cache_lease(root: &Path, repair: bool) -> io::Result<Option<File>> {
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.is_symlink() {
        return Err(invalid());
    }
    let path = root.join(".nul-repair.lock");
    if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.is_symlink()) {
        return Err(invalid());
    }
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let result = if repair {
        fs4::FileExt::try_lock(&file)
    } else {
        fs4::FileExt::try_lock_shared(&file)
    };
    match result {
        Ok(()) => Ok(Some(file)),
        Err(fs4::TryLockError::WouldBlock) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn repair_tree(root: &Path, report: &mut RepairReport) {
    let Ok(first) = fs::read_dir(root).await else {
        report.failed += 1;
        return;
    };
    // Depth-first iterators bound memory by directory depth, not file count.
    let mut directories = vec![first];
    while let Some(entries) = directories.last_mut() {
        match entries.next_entry().await {
            Ok(Some(entry)) => match entry.file_type().await {
                Ok(kind) if kind.is_dir() => match fs::read_dir(entry.path()).await {
                    Ok(child) => directories.push(child),
                    Err(_) => report.failed += 1,
                },
                Ok(kind) if kind.is_file() && text_extension(&entry.path()) => {
                    match strip_or_delete(&entry.path()).await {
                        StripResult::Unchanged => {}
                        StripResult::Stripped => report.repaired += 1,
                        StripResult::Removed => report.removed += 1,
                        StripResult::Failed => report.failed += 1,
                    }
                }
                Ok(_) => {} // Never follow links or inspect binary caption/font data.
                Err(_) => report.failed += 1,
            },
            Ok(None) => {
                directories.pop();
            }
            Err(_) => {
                report.failed += 1;
                directories.pop();
            }
        }
    }
}

fn text_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ass" | "srt" | "vtt"))
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StripResult {
    Unchanged,
    Stripped,
    Removed,
    Failed,
}

/// Failure never publishes extraction readiness. Only an owned generated path
/// is eligible for the upstream delete-to-regenerate fallback.
pub(crate) async fn strip_or_delete(path: &Path) -> StripResult {
    match strip(path).await {
        Ok(false) => StripResult::Unchanged,
        Ok(true) => StripResult::Stripped,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::PermissionDenied
            ) =>
        {
            StripResult::Failed
        }
        Err(error) => {
            tracing::warn!(kind=?error.kind(), "subtitle cache text repair failed");
            match fs::symlink_metadata(path).await {
                Err(error) if error.kind() == io::ErrorKind::NotFound => StripResult::Removed,
                Ok(m) if m.is_file() && !m.is_symlink() => match fs::remove_file(path).await {
                    Ok(()) => StripResult::Removed,
                    Err(_) => StripResult::Failed,
                },
                _ => StripResult::Failed,
            }
        }
    }
}

/// Preserve clean files and all non-NUL code units. A unique sibling remains
/// owned across awaits, so cancellation leaves the original intact.
async fn strip(path: &Path) -> io::Result<bool> {
    let metadata = fs::symlink_metadata(path).await?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(invalid());
    }
    let length = metadata.len();
    let mut source = fs::File::open(path).await?;
    let mut prefix = [0_u8; 4];
    let initial = usize::try_from(length.min(4)).map_err(|_| invalid())?;
    source.read_exact(&mut prefix[..initial]).await?;
    let unit = if prefix == [0xff, 0xfe, 0, 0] || prefix == [0, 0, 0xfe, 0xff] {
        4
    } else if prefix.starts_with(&[0xff, 0xfe]) || prefix.starts_with(&[0xfe, 0xff]) {
        2
    } else {
        1
    };
    if !length.is_multiple_of(unit) {
        return Err(invalid());
    }
    source.rewind().await?;
    if !transfer(
        &mut source,
        None,
        length,
        usize::try_from(unit).map_err(|_| invalid())?,
    )
    .await?
    {
        return Ok(false);
    }
    if metadata.permissions().readonly() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    source.rewind().await?;
    replace_file(path, &metadata, async move |output| {
        let result = transfer(
            &mut source,
            Some(output),
            length,
            usize::try_from(unit).map_err(|_| invalid())?,
        )
        .await;
        // Windows cannot replace an existing file while its reader is open.
        // Await pending I/O and close it before publishing the sibling output.
        drop(source.into_std().await);
        result.map(|_| ())
    })
    .await?;
    Ok(true)
}

async fn replace_file(
    path: &Path,
    metadata: &std::fs::Metadata,
    write: impl std::ops::AsyncFnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path.parent().ok_or_else(invalid)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut output = fs::File::from_std(temporary.reopen()?);
    write(&mut output).await?;
    output.flush().await?;
    output.sync_all().await?;
    drop(output);
    // Reject common concurrent replacement/truncation, retaining the original.
    let current = fs::symlink_metadata(path).await?;
    if !current.is_file()
        || current.is_symlink()
        || current.len() != metadata.len()
        || current.modified()? != metadata.modified()?
        || current.permissions().readonly() != metadata.permissions().readonly()
    {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "subtitle changed during repair",
        ));
    }
    fs::set_permissions(temporary.path(), metadata.permissions()).await?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

async fn transfer(
    source: &mut fs::File,
    mut destination: Option<&mut fs::File>,
    length: u64,
    unit: usize,
) -> io::Result<bool> {
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut left = length;
    let mut removed = false;
    while left > 0 {
        let count = usize::try_from(left.min(64 * 1024)).map_err(|_| invalid())?;
        source.read_exact(&mut buffer[..count]).await?;
        let mut kept = 0;
        for offset in (0..count).step_by(unit) {
            if buffer[offset..offset + unit].iter().all(|byte| *byte == 0) {
                removed = true;
            } else {
                buffer.copy_within(offset..offset + unit, kept);
                kept += unit;
            }
        }
        if let Some(output) = destination.as_mut() {
            output.write_all(&buffer[..kept]).await?;
        }
        left -= u64::try_from(count).map_err(|_| invalid())?;
    }
    Ok(removed)
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid owned text caption")
}

#[cfg(test)]
#[path = "subtitle_cache_nul_tests.rs"]
mod tests;
