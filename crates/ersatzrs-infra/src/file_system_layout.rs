//! Cache-folder layout, ported from `ErsatzTV.Core/FileSystemLayout.cs`.
//!
//! ErsatzTV roots its cache under the app-data folder
//! (`<AppDataFolder>/cache/streams/{subtitles,fonts}`). ErsatzRS uses the same
//! `data_local_dir()/ErsatzRS` base that `logging::default_log_dir` already uses
//! (see `logging.rs`), so the two stay consistent. Only the subtitle and font
//! folders required by embedded-subtitle extraction are exposed here.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

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
}
