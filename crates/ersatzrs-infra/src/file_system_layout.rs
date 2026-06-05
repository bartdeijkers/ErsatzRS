//! Cache-folder layout, ported from `ErsatzTV.Core/FileSystemLayout.cs`.
//!
//! ErsatzTV roots its cache under the app-data folder
//! (`<AppDataFolder>/cache/streams/{subtitles,fonts}`). ErsatzRS uses the same
//! `data_local_dir()/ErsatzRS` base that `logging::default_log_dir` already uses
//! (see `logging.rs`), so the two stay consistent. Only the subtitle and font
//! folders required by embedded-subtitle extraction and troubleshooting
//! playback are exposed here.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

const DEFAULT_SQLITE_DIR_NAME: &str = ".ersatzrs";

/// Application state root, matching the default `SQLite` database directory
/// (`~/.ersatzrs`).
#[must_use]
pub fn app_state_folder() -> PathBuf {
    BaseDirs::new().map_or_else(
        || Path::new(".").to_path_buf(),
        |base_dirs| base_dirs.home_dir().join(DEFAULT_SQLITE_DIR_NAME),
    )
}

/// Root for persistent ErsatzRS data files (`~/.ersatzrs/data` by default).
#[must_use]
pub fn data_folder() -> PathBuf {
    app_state_folder().join("data")
}

/// Resolve a path below the persistent data root.
#[must_use]
pub fn data_path(path: impl AsRef<Path>) -> PathBuf {
    data_folder().join(path)
}

/// Application data root, matching `logging::default_log_dir`'s base
/// (`data_local_dir()/ErsatzRS`), falling back to `./` when no home directory
/// is available.
fn app_data_folder() -> PathBuf {
    BaseDirs::new().map_or_else(
        || Path::new(".").to_path_buf(),
        |base_dirs| base_dirs.data_local_dir().join("ErsatzRS"),
    )
}

/// `<AppDataFolder>/cache/streams` — root of the per-stream extraction cache.
/// Mirrors `FileSystemLayout.StreamsCacheFolder`.
#[must_use]
pub fn streams_cache_folder() -> PathBuf {
    app_data_folder().join("cache").join("streams")
}

/// `<AppDataFolder>/cache/transcode` — root of HLS transcode sessions.
/// Mirrors `FileSystemLayout.TranscodeFolder`.
#[must_use]
pub fn transcode_folder() -> PathBuf {
    app_data_folder().join("cache").join("transcode")
}

/// `.troubleshooting` — the reserved HLS channel number used by ErsatzTV for
/// one-off troubleshooting playback.
pub const TRANSCODE_TROUBLESHOOTING_CHANNEL: &str = ".troubleshooting";

/// `<TranscodeFolder>/.troubleshooting` — one-off playback troubleshooting
/// artifacts and HLS output. Mirrors `FileSystemLayout.TranscodeTroubleshootingFolder`.
#[must_use]
pub fn transcode_troubleshooting_folder() -> PathBuf {
    transcode_folder().join(TRANSCODE_TROUBLESHOOTING_CHANNEL)
}

/// `<AppDataFolder>/cache/streams/subtitles` — extracted text subtitles.
/// Mirrors `FileSystemLayout.SubtitleCacheFolder`.
#[must_use]
pub fn subtitle_cache_folder() -> PathBuf {
    streams_cache_folder().join("subtitles")
}

/// `<AppDataFolder>/cache/streams/subtitles/seek` — playback-time re-based
/// subtitle files for non-zero seeks.
#[must_use]
pub fn subtitle_seek_cache_folder() -> PathBuf {
    subtitle_cache_folder().join("seek")
}

/// `<AppDataFolder>/cache/streams/fonts` — extracted font attachments.
/// Mirrors `FileSystemLayout.FontsCacheFolder`.
#[must_use]
pub fn fonts_cache_folder() -> PathBuf {
    streams_cache_folder().join("fonts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_cache_folder_ends_with_streams_subtitles() {
        let path = subtitle_cache_folder();
        assert!(
            path.ends_with(Path::new("cache").join("streams").join("subtitles")),
            "unexpected subtitle cache path: {}",
            path.display()
        );
    }

    #[test]
    fn subtitle_seek_cache_folder_extends_subtitle_cache() {
        let path = subtitle_seek_cache_folder();
        assert!(
            path.ends_with(
                Path::new("cache")
                    .join("streams")
                    .join("subtitles")
                    .join("seek")
            ),
            "unexpected subtitle seek cache path: {}",
            path.display()
        );
    }

    #[test]
    fn fonts_cache_folder_ends_with_streams_fonts() {
        let path = fonts_cache_folder();
        assert!(
            path.ends_with(Path::new("cache").join("streams").join("fonts")),
            "unexpected fonts cache path: {}",
            path.display()
        );
    }

    #[test]
    fn transcode_troubleshooting_folder_matches_ersatztv_shape() {
        let path = transcode_troubleshooting_folder();
        assert!(
            path.ends_with(
                Path::new("cache")
                    .join("transcode")
                    .join(TRANSCODE_TROUBLESHOOTING_CHANNEL)
            ),
            "unexpected troubleshooting transcode path: {}",
            path.display()
        );
    }

    #[test]
    fn persistent_data_folder_lives_under_ersatzrs_state_root() {
        let path = data_folder();
        assert!(
            path.ends_with(Path::new(".ersatzrs").join("data")),
            "unexpected data path: {}",
            path.display()
        );
    }
}
