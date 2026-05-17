//! Folder-etag helper.
//!
//! Ports `ErsatzTV.Scanner/Core/Metadata/FolderEtag.cs` (and the
//! `ListFiles` / `ListSubdirectories` / `GetLastWriteTime` behavior of
//! `ErsatzTV.Core/Metadata/LocalFileSystem.cs`) so a season-folder etag
//! computed here matches one computed by ErsatzTV for the same paths and
//! modification times.
//!
//! Parity contract:
//!
//! - `ListFiles` lists top-level files only and excludes basenames starting
//!   with `._` (case-insensitive); an inaccessible folder yields nothing.
//! - `ListSubdirectories` lists direct subfolders only; an inaccessible
//!   folder yields nothing.
//! - Both callers sort by the full path string (`OrderBy(identity)`); .NET's
//!   `EnumerateFiles`/`EnumerateDirectories` return folder-prefixed paths, so
//!   we sort the same folder-prefixed strings we hash.
//! - The hashed value is the file/subfolder path followed by, for files, the
//!   UTC last-write time expressed in .NET ticks (100 ns since 0001-01-01).
//!   A metadata error maps to `DateTime.MinValue` whose `.Ticks` is `0`,
//!   matching ErsatzTV's `SystemTime.MinValueUtc` fallback.
//! - The digest is MD5, rendered as lowercase hex. `MediaFile.PathHash`
//!   stays SHA-256 and is unaffected.
//!
//! On metadata errors or inaccessible entries we log and keep scanning,
//! mirroring ErsatzTV's local-filesystem style of never failing a whole
//! library for one bad path.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use md5::{Digest, Md5};
use tracing::warn;

/// Ticks between .NET's `DateTime` epoch (0001-01-01) and the Unix epoch
/// (1970-01-01), in 100-ns units. Adding this to a Unix-based tick count
/// yields the same `.Ticks` value ErsatzTV's `DateTime.Ticks` produces.
const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;

/// .NET ticks are 100-nanosecond intervals.
const TICKS_PER_SECOND: i64 = 10_000_000;

/// Mirror `FolderEtag.Calculate`: hash the sorted top-level files of `folder`,
/// each as its path string followed by its UTC last-write time in .NET ticks.
///
/// Returns lowercase MD5 hex. An inaccessible folder hashes the empty string
/// (same as ErsatzTV, where `ListFiles` returns an empty sequence).
pub(crate) fn folder_etag(folder: &Path) -> String {
    let mut buf = String::new();
    append_files(folder, &mut buf);
    md5_lower_hex(&buf)
}

/// Mirror `FolderEtag.CalculateWithSubfolders`: the top-level file digest
/// input, then each sorted direct subfolder as its path string followed by
/// `folder_etag(subfolder)`.
pub(crate) fn folder_etag_with_subfolders(folder: &Path) -> String {
    let mut buf = String::new();
    append_files(folder, &mut buf);

    for subfolder in list_subdirectories(folder) {
        buf.push_str(&subfolder);
        buf.push_str(&folder_etag(Path::new(&subfolder)));
    }

    md5_lower_hex(&buf)
}

/// Append the sorted top-level files of `folder` as `path + ticks`, matching
/// the first loop of both `FolderEtag` methods.
fn append_files(folder: &Path, buf: &mut String) {
    for file in list_files(folder) {
        buf.push_str(&file);
        let _ = write!(buf, "{}", last_write_ticks(Path::new(&file)));
    }
}

/// Port of `LocalFileSystem.ListFiles(folder)`: top-level files only,
/// excluding basenames starting with `._` (case-insensitive), sorted by the
/// full path string. An inaccessible folder yields an empty list.
fn list_files(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.len() >= 2 && name[..2].eq_ignore_ascii_case("._") {
            continue;
        }
        files.push(entry.path().to_string_lossy().into_owned());
    }

    files.sort();
    files
}

/// Port of `LocalFileSystem.ListSubdirectories(folder)`: direct subfolders
/// only, sorted by the full path string. An inaccessible folder yields an
/// empty list.
fn list_subdirectories(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            dirs.push(entry.path().to_string_lossy().into_owned());
        }
    }

    dirs.sort();
    dirs
}

/// Port of `LocalFileSystem.GetLastWriteTime(path).Ticks`: the file's UTC
/// last-write time as .NET ticks. On any metadata error this returns `0`
/// (`DateTime.MinValue.Ticks`), the same fallback ErsatzTV uses.
fn last_write_ticks(path: &Path) -> i64 {
    let modified = match fs::metadata(path).and_then(|m| m.modified()) {
        Ok(modified) => modified,
        Err(error) => {
            warn!(path = %path.display(), %error, "unable to read last-write time; using zero ticks");
            return 0;
        }
    };

    // Compute in i128 so the seconds-to-ticks multiply cannot wrap, then
    // saturate into the i64 range .NET's DateTime.Ticks occupies.
    let ticks: i128 = match modified.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(delta) => {
            let secs = i128::from(delta.as_secs());
            // 100-ns fraction within the second.
            let sub_ticks = i128::from(delta.subsec_nanos() / 100);
            secs * i128::from(TICKS_PER_SECOND) + sub_ticks + i128::from(UNIX_EPOCH_TICKS)
        }
        Err(before_epoch) => {
            // Pre-1970 mtime: subtract the negative offset from the epoch.
            let delta = before_epoch.duration();
            let secs = i128::from(delta.as_secs());
            let sub_ticks = i128::from(delta.subsec_nanos() / 100);
            i128::from(UNIX_EPOCH_TICKS) - (secs * i128::from(TICKS_PER_SECOND) + sub_ticks)
        }
    };

    i64::try_from(ticks).unwrap_or(i64::MAX)
}

/// Render the MD5 of `input` as lowercase hex, matching `FolderEtag`'s
/// `ToString("x2")` loop.
fn md5_lower_hex(input: &str) -> String {
    let digest = Md5::digest(input.as_bytes());
    let mut hash = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut hash, "{byte:02x}").expect("writing to String should not fail");
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write as _;
    use std::time::Duration;

    use super::*;

    fn unix_time(unix_secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(unix_secs)
    }

    fn set_mtime(path: &Path, unix_secs: u64) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(unix_time(unix_secs))
            .unwrap();
    }

    fn touch(path: &Path, unix_secs: u64) {
        File::create(path).unwrap().write_all(b"x").unwrap();
        set_mtime(path, unix_secs);
    }

    #[test]
    fn ticks_match_dotnet_for_known_unix_time() {
        // 2021-01-01T00:00:00Z. .NET `new DateTimeOffset(2021,1,1,0,0,0,
        // TimeSpan.Zero).UtcDateTime.Ticks` == 637_450_560_000_000_000
        // (verified: 1_609_459_200 * 10_000_000 + 621_355_968_000_000_000).
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("e01.mkv");
        touch(&file, 1_609_459_200);
        assert_eq!(last_write_ticks(&file), 637_450_560_000_000_000);
    }

    #[test]
    fn missing_file_uses_zero_ticks() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(last_write_ticks(&dir.path().join("nope.mkv")), 0);
    }

    #[test]
    fn etag_is_lowercase_md5_hex_of_sorted_path_plus_ticks() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mkv");
        let b = dir.path().join("b.mkv");
        // Create out of order; the etag must not depend on creation order.
        touch(&b, 1_609_459_200);
        touch(&a, 1_609_459_200);

        let expected = {
            let mut s = String::new();
            s.push_str(&a.to_string_lossy());
            s.push_str("637450560000000000");
            s.push_str(&b.to_string_lossy());
            s.push_str("637450560000000000");
            md5_lower_hex(&s)
        };

        let etag = folder_etag(dir.path());
        assert_eq!(etag, expected);
        assert_eq!(etag.len(), 32);
        assert!(etag
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn appledouble_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("a.mkv"), 1_609_459_200);
        let with_dot = folder_etag(dir.path());

        // Adding a `._a.mkv` sidecar must not change the etag.
        touch(&dir.path().join("._a.mkv"), 1_700_000_000);
        touch(&dir.path().join("._B.MKV"), 1_700_000_000);
        assert_eq!(folder_etag(dir.path()), with_dot);
    }

    #[test]
    fn changing_mtime_changes_etag() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.mkv");
        touch(&file, 1_609_459_200);
        let before = folder_etag(dir.path());

        set_mtime(&file, 1_609_459_260);
        assert_ne!(folder_etag(dir.path()), before);
    }

    #[test]
    fn inaccessible_folder_hashes_empty_string() {
        let missing = Path::new("/definitely/not/a/real/folder/etag-test");
        assert_eq!(folder_etag(missing), md5_lower_hex(""));
        assert_eq!(folder_etag_with_subfolders(missing), md5_lower_hex(""));
    }

    #[test]
    fn with_subfolders_appends_sorted_subfolders_and_their_etags() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("top.mkv"), 1_609_459_200);

        let s1 = dir.path().join("Season 01");
        let s2 = dir.path().join("Season 02");
        std::fs::create_dir(&s1).unwrap();
        std::fs::create_dir(&s2).unwrap();
        touch(&s2.join("e01.mkv"), 1_609_459_200);
        touch(&s1.join("e01.mkv"), 1_609_459_200);

        let expected = {
            let mut s = String::new();
            s.push_str(&dir.path().join("top.mkv").to_string_lossy());
            s.push_str("637450560000000000");
            // Subfolders sorted by full path string: "Season 01" < "Season 02".
            s.push_str(&s1.to_string_lossy());
            s.push_str(&folder_etag(&s1));
            s.push_str(&s2.to_string_lossy());
            s.push_str(&folder_etag(&s2));
            md5_lower_hex(&s)
        };

        assert_eq!(folder_etag_with_subfolders(dir.path()), expected);
    }

    #[test]
    fn subfolder_file_change_changes_with_subfolders_etag() {
        let dir = tempfile::tempdir().unwrap();
        let season = dir.path().join("Season 01");
        std::fs::create_dir(&season).unwrap();
        let ep = season.join("e01.mkv");
        touch(&ep, 1_609_459_200);
        let before = folder_etag_with_subfolders(dir.path());

        // Adding an episode under the season changes the parent etag.
        touch(&season.join("e02.mkv"), 1_609_459_200);
        assert_ne!(folder_etag_with_subfolders(dir.path()), before);

        // Touching an existing episode also changes it.
        let touched = folder_etag_with_subfolders(dir.path());
        set_mtime(&ep, 1_700_000_000);
        assert_ne!(folder_etag_with_subfolders(dir.path()), touched);
    }

    #[test]
    fn subdirectory_only_contents_do_not_count_as_top_level_files() {
        // A nested file must not be treated as a top-level file by
        // `folder_etag` (TopDirectoryOnly parity).
        let dir = tempfile::tempdir().unwrap();
        let empty = folder_etag(dir.path());
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        touch(&sub.join("deep.mkv"), 1_609_459_200);
        assert_eq!(folder_etag(dir.path()), empty);
    }

    /// ErsatzTV-derived vector: with a fixed path and tick value, the MD5 is
    /// deterministic and reproducible from the documented input shape, so an
    /// imported ErsatzTV etag for the same path+mtime will match.
    #[test]
    fn known_input_vector_is_stable() {
        // "/library/Show/Season 01/e01.mkv" + 2021-01-01 ticks
        // (637450560000000000). Hash verified with `md5sum`.
        let input = "/library/Show/Season 01/e01.mkv637450560000000000";
        assert_eq!(md5_lower_hex(input), "d9b8e57eda0199410a27163a980733fa");
    }
}
