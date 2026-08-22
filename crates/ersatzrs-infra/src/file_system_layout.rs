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
/// Optional absolute override for the persistent ErsatzRS state root.
pub const APP_STATE_DIR_ENV_VAR: &str = "ERSATZRS_APP_STATE_DIR";
/// Optional absolute override for the ErsatzRS local-data/cache root.
pub const APP_DATA_DIR_ENV_VAR: &str = "ERSATZRS_APP_DATA_DIR";

fn configured_directory(name: &str) -> Option<PathBuf> {
    crate::startup_config::environment_variable(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Application state root, matching the default `SQLite` database directory
/// (`~/.ersatzrs`).
#[must_use]
pub fn app_state_folder() -> PathBuf {
    if let Some(path) = configured_directory(APP_STATE_DIR_ENV_VAR) {
        return path;
    }
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

/// Root for the managed ADR 0008 `FFmpeg` runtime (`~/.ersatzrs/ffmpeg`).
#[must_use]
pub fn ffmpeg_runtime_folder() -> PathBuf {
    app_state_folder().join("ffmpeg")
}

/// Root for repository-managed add-on packages.
#[must_use]
pub fn addon_packages_folder() -> PathBuf {
    app_state_folder().join("addons").join("packages")
}

/// Persistent private data root for one installed add-on.
#[must_use]
pub fn addon_data_folder(data_namespace: &str, addon_id: &str) -> PathBuf {
    data_path(Path::new("addons").join(data_namespace).join(addon_id))
}

/// Resolve a path below the persistent data root.
#[must_use]
pub fn data_path(path: impl AsRef<Path>) -> PathBuf {
    data_folder().join(path)
}

/// `<data>/scripts/channel-stream-selectors` — user-authored custom stream
/// selectors. Mirrors `FileSystemLayout.ChannelStreamSelectorsFolder`
/// (`ScriptsFolder/channel-stream-selectors`).
///
/// User-authored content folders that ErsatzTV roots at `<AppDataFolder>/…`
/// live below the persistent data root in ErsatzRS (the same mapping
/// `templates/music-video-credits` uses), so the ErsatzTV `scripts` grouping
/// is preserved here rather than resolved through [`scripts_folder`].
/// This is the single source of truth: the channel editor, playback, and
/// playback troubleshooting must all resolve selectors through this function,
/// or the read and write sides of the feature silently disagree.
#[must_use]
pub fn channel_stream_selectors_folder() -> PathBuf {
    data_path(Path::new("scripts").join("channel-stream-selectors"))
}

/// List the custom stream selectors available to channels and playback
/// troubleshooting, sorted by file name.
///
/// Mirrors `GetChannelStreamSelectorsHandler` on top of
/// `LocalFileSystem.ListFiles`: only files in the top directory are listed,
/// macOS resource-fork siblings (`._*`) are skipped, and an absent folder, a
/// permission error, or any other listing failure yields an empty list rather
/// than an error. An empty list is a first-class state — the playback
/// troubleshooting page disables its Stream Selector dropdown — so treating it
/// as a failure would take the whole page down on every install that has never
/// created the folder.
///
/// Both the channel editor and playback troubleshooting must call this so their
/// dropdowns can never disagree about which selectors exist.
pub async fn list_channel_stream_selectors() -> Vec<String> {
    list_channel_stream_selectors_in(&channel_stream_selectors_folder()).await
}

async fn list_channel_stream_selectors_in(folder: &Path) -> Vec<String> {
    let mut selectors = Vec::new();
    let mut entries = match tokio::fs::read_dir(folder).await {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    folder = %folder.display(),
                    %error,
                    "unable to list channel stream selectors"
                );
            }
            return selectors;
        }
    };

    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                if !entry.file_type().await.is_ok_and(|kind| kind.is_file()) {
                    continue;
                }
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if name.starts_with("._") {
                    continue;
                }
                selectors.push(name.to_owned());
            }
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(
                    folder = %folder.display(),
                    %error,
                    "unable to list channel stream selectors"
                );
                break;
            }
        }
    }

    selectors.sort();
    selectors
}

/// Application data root, matching `logging::default_log_dir`'s base
/// (`data_local_dir()/ErsatzRS`), falling back to `./` when no home directory
/// is available.
fn app_data_folder() -> PathBuf {
    if let Some(path) = configured_directory(APP_DATA_DIR_ENV_VAR) {
        return path;
    }
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

/// `<AppDataFolder>/cache/filter-graphs` — private per-spawn filter-graph
/// transport files (ADR 0003).
///
/// Under `cache/` because the contents are disposable transport artifacts, not
/// configuration: every file is written immediately before a spawn and removed
/// once that child has exited.
#[must_use]
pub fn filter_graph_folder() -> PathBuf {
    app_data_folder().join("cache").join("filter-graphs")
}

/// Disposable cache root for one installed add-on.
#[must_use]
pub fn addon_cache_folder(data_namespace: &str, addon_id: &str) -> PathBuf {
    app_data_folder()
        .join("cache")
        .join("addons")
        .join(data_namespace)
        .join(addon_id)
}

/// `<AppDataFolder>/scripts` — root for user-editable scripts.
/// Mirrors `FileSystemLayout.ScriptsFolder`.
#[must_use]
pub fn scripts_folder() -> PathBuf {
    app_data_folder().join("scripts")
}

/// `<AppDataFolder>/scripts/mpegts` — MPEG-TS wrapper script folders.
/// Mirrors `FileSystemLayout.MpegTsScriptsFolder`.
#[must_use]
pub fn mpegts_scripts_folder() -> PathBuf {
    scripts_folder().join("mpegts")
}

/// `<AppDataFolder>/scripts/mpegts/default` — bundled default MPEG-TS wrapper.
/// Mirrors `FileSystemLayout.DefaultMpegTsScriptFolder`.
#[must_use]
pub fn default_mpegts_script_folder() -> PathBuf {
    mpegts_scripts_folder().join("default")
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

/// `<AppDataFolder>/cache/streams/subtitles/seek-v2` — playback-time
/// re-based subtitle files for non-zero seeks. Versioned (`v2`) because the
/// cache key is a content offset, not wall-clock time, so a looping channel
/// would otherwise keep re-serving pre-ADR-0061 cached files that silently
/// dropped straddling cues; the old unversioned `seek/` folder is simply
/// orphaned, not actively cleaned up.
#[must_use]
pub fn subtitle_seek_cache_folder() -> PathBuf {
    subtitle_cache_folder().join("seek-v2")
}

/// `<AppDataFolder>/cache/streams/fonts` — extracted font attachments.
/// Mirrors `FileSystemLayout.FontsCacheFolder`.
#[must_use]
pub fn fonts_cache_folder() -> PathBuf {
    streams_cache_folder().join("fonts")
}

/// `<AppDataFolder>/cache/resources` — bundled resources extracted at startup
/// (song progress overlays, song backgrounds). Mirrors
/// `FileSystemLayout.ResourcesCacheFolder`; `ChannelWatermarkImageSource::Resource`
/// watermark images resolve against this folder.
#[must_use]
pub fn resources_cache_folder() -> PathBuf {
    app_data_folder().join("cache").join("resources")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_stream_selectors_folder_keeps_the_ersatztv_scripts_grouping() {
        let path = channel_stream_selectors_folder();
        assert!(
            path.ends_with(Path::new("scripts").join("channel-stream-selectors")),
            "unexpected channel stream selector path: {}",
            path.display()
        );
    }

    #[tokio::test]
    async fn listing_channel_stream_selectors_treats_a_missing_folder_as_empty() {
        let root = tempfile::tempdir().expect("temp dir");
        let missing = root.path().join("never-created");

        assert!(list_channel_stream_selectors_in(&missing).await.is_empty());
    }

    #[tokio::test]
    async fn listing_channel_stream_selectors_keeps_every_file_but_resource_forks() {
        let root = tempfile::tempdir().expect("temp dir");
        let folder = root.path();
        for name in ["b.yml", "a.yaml", "plain.txt", "._a.yaml"] {
            tokio::fs::write(folder.join(name), "")
                .await
                .expect("write");
        }
        tokio::fs::create_dir(folder.join("nested"))
            .await
            .expect("nested dir");

        assert_eq!(
            list_channel_stream_selectors_in(folder).await,
            vec![
                "a.yaml".to_owned(),
                "b.yml".to_owned(),
                "plain.txt".to_owned()
            ]
        );
    }

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
                    .join("seek-v2")
            ),
            "unexpected subtitle seek cache path: {}",
            path.display()
        );
    }

    #[test]
    fn resources_cache_folder_ends_with_cache_resources() {
        let path = resources_cache_folder();
        assert!(
            path.ends_with(Path::new("cache").join("resources")),
            "unexpected resources cache path: {}",
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

    #[test]
    fn ffmpeg_runtime_folder_lives_under_ersatzrs_state_root() {
        let path = ffmpeg_runtime_folder();
        assert!(
            path.ends_with(Path::new(".ersatzrs").join("ffmpeg")),
            "unexpected ffmpeg runtime path: {}",
            path.display()
        );
    }
}
