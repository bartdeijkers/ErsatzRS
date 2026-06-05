//! `FFmpeg` / `ffprobe` executable resolution, ported from ErsatzTV's
//! `FFmpegLocator` (`ErsatzTV.Core/FFmpeg/FFmpegLocator.cs`).
//!
//! ErsatzTV resolves the tool path in this order, persisting the result back
//! into config so subsequent runs are stable:
//!
//! 1. configured DB value, if the file still exists (otherwise the stale
//!    config is dropped and the tool is re-located);
//! 2. next to the running executable (with a `.exe` suffix on Windows);
//! 3. on `PATH` (`which` on Unix / `where` on Windows).
//!
//! This module mirrors that behaviour and adds an explicit environment-variable
//! override at the front (kept for CI and operator overrides). The resolved
//! absolute paths are cached in a process-wide holder that every spawn site
//! reads, so a bare `"ffmpeg"` is no longer handed to `Command::new` (which only
//! searches `PATH`, never the directory of the running executable — the cause of
//! the Windows "`ffmpeg.exe` next to `ErsatzRS.exe` is not found" bug).

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::version::{
    DEFAULT_FFMPEG_PATH, DEFAULT_FFPROBE_PATH, FFMPEG_PATH_ENV_VARS, FFPROBE_PATH_ENV_VARS,
};

/// Where a resolved tool path came from. Mirrors the branches of ErsatzTV's
/// `FFmpegLocator`, plus the environment-variable override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathSource {
    /// An explicit environment-variable override (`*_PATH`). Never persisted.
    Env,
    /// An existing, valid DB config value. Already persisted; nothing to do.
    DbConfig,
    /// Located next to the running executable. Should be persisted.
    NextToExe,
    /// Located on `PATH`. Should be persisted.
    OnPath,
    /// Nothing was found; the bare tool name is returned as a last resort and
    /// resolution is left to the OS `PATH` at spawn time.
    Fallback,
}

/// A resolved tool path plus provenance, mirroring ErsatzTV's
/// `FFmpegLocator.ValidatePath` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedTool {
    /// The resolved path (absolute when located on disk, otherwise the bare
    /// tool name for the [`PathSource::Fallback`] case).
    pub path: String,
    /// How [`Self::path`] was resolved.
    pub source: PathSource,
    /// True when a DB config value was supplied but its file no longer exists,
    /// so the caller should drop the stale config (ErsatzTV deletes it).
    pub db_was_stale: bool,
}

impl LocatedTool {
    /// Whether the resolved path should be written back into DB config.
    ///
    /// Matches ErsatzTV, which upserts only paths it located on disk (next to
    /// the executable or on `PATH`); env overrides and an already-valid DB value
    /// are not re-persisted.
    #[must_use]
    pub fn should_persist(&self) -> bool {
        matches!(self.source, PathSource::NextToExe | PathSource::OnPath)
    }
}

/// Resolve the `ffmpeg` path, consulting an optional DB config value.
///
/// Order: env override → DB config (if the file exists) → next to the running
/// executable → `PATH` → bare `"ffmpeg"`.
#[must_use]
pub fn resolve_ffmpeg_path(db_config: Option<&str>) -> LocatedTool {
    resolve_tool(DEFAULT_FFMPEG_PATH, FFMPEG_PATH_ENV_VARS, db_config)
}

/// Resolve the `ffprobe` path, consulting an optional DB config value. See
/// [`resolve_ffmpeg_path`] for the resolution order.
#[must_use]
pub fn resolve_ffprobe_path(db_config: Option<&str>) -> LocatedTool {
    resolve_tool(DEFAULT_FFPROBE_PATH, FFPROBE_PATH_ENV_VARS, db_config)
}

static RESOLVED_FFMPEG_PATH: RwLock<Option<String>> = RwLock::new(None);
static RESOLVED_FFPROBE_PATH: RwLock<Option<String>> = RwLock::new(None);

/// Store the resolved absolute `ffmpeg` path for spawn sites to read. Called by
/// startup once the path is resolved from DB config, and by the settings save
/// when the operator changes it (so a restart is not required).
pub fn set_resolved_ffmpeg_path(path: impl Into<String>) {
    if let Ok(mut guard) = RESOLVED_FFMPEG_PATH.write() {
        *guard = Some(path.into());
    }
}

/// Store the resolved absolute `ffprobe` path. See [`set_resolved_ffmpeg_path`].
pub fn set_resolved_ffprobe_path(path: impl Into<String>) {
    if let Ok(mut guard) = RESOLVED_FFPROBE_PATH.write() {
        *guard = Some(path.into());
    }
}

/// The `ffmpeg` path every spawn site should use.
///
/// Returns the process-wide resolved path when startup (or a settings change)
/// has set it; otherwise resolves on demand from env / next-to-exe / `PATH`
/// without DB config. This is the replacement for the old bare
/// `ffmpeg_path_from_env()` at spawn sites.
#[must_use]
pub fn ffmpeg_path() -> String {
    resolved_or(&RESOLVED_FFMPEG_PATH, || resolve_ffmpeg_path(None).path)
}

/// The `ffprobe` path every spawn site should use. See [`ffmpeg_path`].
#[must_use]
pub fn ffprobe_path() -> String {
    resolved_or(&RESOLVED_FFPROBE_PATH, || resolve_ffprobe_path(None).path)
}

fn resolved_or(slot: &RwLock<Option<String>>, fallback: impl FnOnce() -> String) -> String {
    if let Some(path) = slot.read().ok().and_then(|guard| guard.clone()) {
        return path;
    }
    fallback()
}

fn resolve_tool(base: &str, env_keys: &[&str], db_config: Option<&str>) -> LocatedTool {
    let env_override = first_non_empty_env(env_keys);
    let exe_dir = current_exe_dir();
    let path_var = path_env_var();
    locate_tool_in(
        base,
        env_override.as_deref(),
        db_config,
        exe_dir.as_deref(),
        path_var.as_deref(),
    )
}

/// Pure resolution used by [`resolve_tool`] and by tests. `exe_dir` and
/// `path_var` are injected so the directory-search and `PATH`-search branches
/// can be exercised against real temp files without faking `current_exe`.
fn locate_tool_in(
    base: &str,
    env_override: Option<&str>,
    db_config: Option<&str>,
    exe_dir: Option<&Path>,
    path_var: Option<&str>,
) -> LocatedTool {
    // (a) explicit environment override wins, unconditionally.
    if let Some(value) = env_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return LocatedTool {
            path: value.to_owned(),
            source: PathSource::Env,
            db_was_stale: false,
        };
    }

    let executable = executable_name(base);

    // (b) configured DB value, if the file still exists.
    let mut db_was_stale = false;
    if let Some(value) = db_config.map(str::trim).filter(|value| !value.is_empty()) {
        if Path::new(value).is_file() {
            return LocatedTool {
                path: value.to_owned(),
                source: PathSource::DbConfig,
                db_was_stale: false,
            };
        }
        // Configured path was incorrect; ErsatzTV drops it and re-locates.
        db_was_stale = true;
    }

    // (c) next to the running executable.
    if let Some(dir) = exe_dir {
        let candidate = dir.join(&executable);
        if candidate.is_file() {
            return LocatedTool {
                path: candidate.to_string_lossy().into_owned(),
                source: PathSource::NextToExe,
                db_was_stale,
            };
        }
    }

    // (d) on PATH (the platform-neutral equivalent of `which` / `where`). Like
    // `which`, an entry on PATH must be an executable file, not just any file.
    if let Some(path_var) = path_var {
        for dir in std::env::split_paths(path_var) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            let candidate = dir.join(&executable);
            if is_executable_file(&candidate) {
                return LocatedTool {
                    path: candidate.to_string_lossy().into_owned(),
                    source: PathSource::OnPath,
                    db_was_stale,
                };
            }
        }
    }

    // (e) last resort: hand the bare name to the OS at spawn time.
    LocatedTool {
        path: base.to_owned(),
        source: PathSource::Fallback,
        db_was_stale,
    }
}

/// The on-disk file name for a tool base name, adding `.exe` on Windows.
#[cfg(windows)]
fn executable_name(base: &str) -> String {
    if base.to_ascii_lowercase().ends_with(".exe") {
        base.to_owned()
    } else {
        format!("{base}.exe")
    }
}

#[cfg(not(windows))]
fn executable_name(base: &str) -> String {
    base.to_owned()
}

/// Whether `path` is a file that can be executed. On Unix this checks the
/// owner/group/other execute bits (matching `which`); on other platforms a
/// regular file is sufficient (Windows executability is determined by the
/// `.exe` extension already applied by [`executable_name`]).
#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn current_exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

#[allow(clippy::disallowed_methods)]
fn path_env_var() -> Option<String> {
    std::env::var("PATH").ok().filter(|value| !value.is_empty())
}

#[allow(clippy::disallowed_methods)]
pub(crate) fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| std::env::var(key).ok())
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{locate_tool_in, LocatedTool, PathSource};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ersatzrs-locator-{name}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"#!/bin/sh\n").expect("write fake tool");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("make fake tool executable");
        }
        path
    }

    #[test]
    fn env_override_wins_over_everything() {
        let dir = temp_dir("env-wins");
        let on_disk = touch(&dir, super::executable_name("ffmpeg").as_str());
        let located = locate_tool_in(
            "ffmpeg",
            Some("/custom/ffmpeg"),
            on_disk.to_str(),
            Some(&dir),
            dir.to_str(),
        );
        assert_eq!(
            located,
            LocatedTool {
                path: "/custom/ffmpeg".to_owned(),
                source: PathSource::Env,
                db_was_stale: false,
            }
        );
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn db_config_used_when_file_exists() {
        let dir = temp_dir("db-exists");
        let on_disk = touch(&dir, super::executable_name("ffmpeg").as_str());
        let located = locate_tool_in("ffmpeg", None, on_disk.to_str(), None, None);
        assert_eq!(located.source, PathSource::DbConfig);
        assert_eq!(located.path, on_disk.to_string_lossy());
        assert!(!located.should_persist());
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn db_config_ignored_when_file_missing_and_marked_stale() {
        // No exe dir, no PATH: falls through to the bare fallback, but the stale
        // DB value is flagged so the caller can drop it.
        let located = locate_tool_in("ffmpeg", None, Some("/gone/ffmpeg"), None, None);
        assert_eq!(located.source, PathSource::Fallback);
        assert_eq!(located.path, "ffmpeg");
        assert!(located.db_was_stale);
    }

    #[test]
    fn locates_next_to_executable() {
        let dir = temp_dir("next-to-exe");
        touch(&dir, super::executable_name("ffmpeg").as_str());
        let located = locate_tool_in("ffmpeg", None, None, Some(&dir), None);
        assert_eq!(located.source, PathSource::NextToExe);
        assert_eq!(
            located.path,
            dir.join(super::executable_name("ffmpeg")).to_string_lossy()
        );
        assert!(located.should_persist());
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn next_to_executable_preferred_over_path() {
        let exe_dir = temp_dir("prefer-exe-dir");
        let path_dir = temp_dir("prefer-path-dir");
        touch(&exe_dir, super::executable_name("ffmpeg").as_str());
        touch(&path_dir, super::executable_name("ffmpeg").as_str());
        let located = locate_tool_in("ffmpeg", None, None, Some(&exe_dir), path_dir.to_str());
        assert_eq!(located.source, PathSource::NextToExe);
        assert!(located.path.starts_with(exe_dir.to_str().expect("utf-8")));
        fs::remove_dir_all(exe_dir).expect("remove temp dir");
        fs::remove_dir_all(path_dir).expect("remove temp dir");
    }

    #[test]
    fn locates_on_path() {
        let path_dir = temp_dir("on-path");
        touch(&path_dir, super::executable_name("ffprobe").as_str());
        let located = locate_tool_in("ffprobe", None, None, None, path_dir.to_str());
        assert_eq!(located.source, PathSource::OnPath);
        assert_eq!(
            located.path,
            path_dir
                .join(super::executable_name("ffprobe"))
                .to_string_lossy()
        );
        assert!(located.should_persist());
        fs::remove_dir_all(path_dir).expect("remove temp dir");
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_file_on_path_is_skipped() {
        let path_dir = temp_dir("non-exec-on-path");
        // A plain, non-executable file named like the tool must not be selected
        // (matches `which`, which only returns executables).
        let path = path_dir.join(super::executable_name("ffmpeg"));
        fs::write(&path, b"not executable\n").expect("write non-exec file");
        let located = locate_tool_in("ffmpeg", None, None, None, path_dir.to_str());
        assert_eq!(located.source, PathSource::Fallback);
        fs::remove_dir_all(path_dir).expect("remove temp dir");
    }

    #[test]
    fn falls_back_to_bare_name() {
        let located = locate_tool_in("ffmpeg", None, None, None, None);
        assert_eq!(
            located,
            LocatedTool {
                path: "ffmpeg".to_owned(),
                source: PathSource::Fallback,
                db_was_stale: false,
            }
        );
    }

    #[test]
    fn empty_env_override_is_ignored() {
        let located = locate_tool_in("ffmpeg", Some("   "), None, None, None);
        assert_eq!(located.source, PathSource::Fallback);
    }

    #[cfg(unix)]
    #[test]
    fn executable_name_is_bare_on_unix() {
        assert_eq!(super::executable_name("ffmpeg"), "ffmpeg");
    }

    #[cfg(windows)]
    #[test]
    fn executable_name_adds_exe_on_windows() {
        assert_eq!(super::executable_name("ffmpeg"), "ffmpeg.exe");
        assert_eq!(super::executable_name("ffmpeg.exe"), "ffmpeg.exe");
    }
}
