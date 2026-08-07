//! Private per-spawn filter-graph transport files (ADR 0003, milestone
//! `p8c-01`).
//!
//! The decision of *whether* a graph needs a file lives in
//! `ersatzrs_ffmpeg::filter_transport`. This module owns the file itself: where
//! it is written, that its bytes reach `FFmpeg` unmodified, and — the part that
//! actually matters — that it is removed on every exit path.
//!
//! ADR 0003 requires deterministic cleanup after a successful exit, a non-zero
//! exit, a spawn failure, cancellation, and supervision teardown. That is a lot
//! of paths to remember individually, so the file is an RAII guard instead:
//! dropping it deletes the file, and the guard is owned by the process handle.
//! Any path that stops the child necessarily drops the guard.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use ersatzrs_ffmpeg::{
    rewrite_for_file_transport, sanitized_transport_log, select_filter_graph_transport,
    FilterGraphTransport,
};

/// A filter graph written to disk for one spawn.
///
/// Deleting on drop is the whole point; see the module docs.
#[derive(Debug)]
pub struct FilterGraphFile {
    path: PathBuf,
}

impl FilterGraphFile {
    /// Write `graph` to a private file inside `directory`.
    ///
    /// The file name embeds a process id and a monotonic counter so concurrent
    /// channels never collide.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory cannot be created or the file cannot
    /// be written.
    pub fn write(directory: &Path, graph: &str) -> std::io::Result<Self> {
        std::fs::create_dir_all(directory)?;

        let path = directory.join(format!(
            "graph-{}-{}.txt",
            std::process::id(),
            next_sequence()
        ));

        let mut file = std::fs::File::create(&path)?;
        restrict_permissions(&file)?;
        // Write the exact graph bytes. No trailing newline, no shell quoting,
        // no expansion: FFmpeg reads the file content as the option value
        // verbatim.
        file.write_all(graph.as_bytes())?;
        file.sync_all()?;

        Ok(Self { path })
    }

    /// Path passed to `FFmpeg` as the `-/filter_complex` value.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for FilterGraphFile {
    fn drop(&mut self) {
        // Best effort: a leaked transport file is harmless next to failing the
        // teardown of a live stream, and the directory is under `cache/`.
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!(%error, "failed to remove filter graph transport file");
            }
        }
    }
}

/// Owner-only permissions where the platform supports them.
#[cfg(unix)]
fn restrict_permissions(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)
}

/// Windows has no mode bits; the file inherits the ErsatzRS data directory's
/// ACL, which is already user-scoped.
#[cfg(not(unix))]
fn restrict_permissions(_file: &std::fs::File) -> std::io::Result<()> {
    Ok(())
}

fn next_sequence() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

/// Apply ADR 0003 filter-graph transport to a command about to be spawned.
///
/// Returns the arguments to spawn with, plus the transport file when one was
/// created. The caller must keep the file alive for as long as the child runs.
///
/// A write failure is deliberately **not** fatal: falling back to the inline
/// argument reproduces exactly the behavior that existed before this milestone,
/// which is better than refusing to play a channel because a cache directory is
/// read-only.
#[must_use]
pub fn apply_filter_graph_transport(
    executable: &str,
    arguments: &[String],
    directory: &Path,
    force_file: bool,
) -> (Vec<String>, Option<FilterGraphFile>) {
    if select_filter_graph_transport(executable, arguments, force_file)
        == FilterGraphTransport::Inline
    {
        return (arguments.to_vec(), None);
    }

    let Some((_, graph)) = ersatzrs_ffmpeg::find_inline_filter_graph(arguments) else {
        return (arguments.to_vec(), None);
    };
    let graph_bytes = graph.len();

    match FilterGraphFile::write(directory, graph) {
        Ok(file) => {
            let Some(path) = file.path().to_str() else {
                tracing::warn!("filter graph transport path is not valid UTF-8; staying inline");
                return (arguments.to_vec(), None);
            };
            tracing::info!("{}", sanitized_transport_log(file.path(), graph_bytes));
            let rewritten = rewrite_for_file_transport(arguments, path);
            (rewritten, Some(file))
        }
        Err(error) => {
            tracing::warn!(%error, "failed to write filter graph transport file; staying inline");
            (arguments.to_vec(), None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn the_file_holds_the_exact_graph_bytes() {
        let directory = tempfile::tempdir().expect("temp dir");
        let graph = "[0:0]scale=1920:1080,setparams=range=tv[v];[0:1]anull[a]";

        let file = FilterGraphFile::write(directory.path(), graph).expect("write graph");

        assert_eq!(
            std::fs::read_to_string(file.path()).expect("read back"),
            graph,
            "no trailing newline, no quoting, no expansion"
        );
    }

    #[test]
    fn dropping_the_guard_removes_the_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = FilterGraphFile::write(directory.path(), "[0:0]null[v]").expect("write graph");
        let path = file.path().to_path_buf();
        assert!(path.exists());

        drop(file);

        assert!(
            !path.exists(),
            "the transport file must not outlive its spawn"
        );
    }

    #[test]
    fn dropping_twice_over_an_already_deleted_file_is_silent() {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = FilterGraphFile::write(directory.path(), "[0:0]null[v]").expect("write graph");
        std::fs::remove_file(file.path()).expect("external removal");

        // Supervision teardown can race an operator clearing the cache; the
        // guard must not panic or log an error for a file that is already gone.
        drop(file);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp dir");
        let file = FilterGraphFile::write(directory.path(), "[0:0]null[v]").expect("write graph");

        let mode = std::fs::metadata(file.path())
            .expect("metadata")
            .permissions()
            .mode();

        assert_eq!(mode & 0o777, 0o600, "graphs embed private media paths");
    }

    #[test]
    fn concurrent_graphs_do_not_collide() {
        let directory = tempfile::tempdir().expect("temp dir");
        let first = FilterGraphFile::write(directory.path(), "[0:0]null[v]").expect("first");
        let second = FilterGraphFile::write(directory.path(), "[0:0]anull[a]").expect("second");

        assert_ne!(first.path(), second.path());
        assert_eq!(
            std::fs::read_to_string(first.path()).expect("first"),
            "[0:0]null[v]"
        );
    }

    #[test]
    fn a_short_graph_is_left_inline_and_creates_no_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let arguments = args(&["-i", "/media/movie.mkv", "-filter_complex", "[0:0]null[v]"]);

        let (spawned, file) =
            apply_filter_graph_transport("/usr/bin/ffmpeg", &arguments, directory.path(), false);

        assert_eq!(spawned, arguments);
        assert!(file.is_none());
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("read dir")
                .count(),
            0
        );
    }

    #[test]
    fn forced_transport_rewrites_argv_and_writes_the_graph() {
        let directory = tempfile::tempdir().expect("temp dir");
        let arguments = args(&["-i", "/media/movie.mkv", "-filter_complex", "[0:0]null[v]"]);

        let (spawned, file) =
            apply_filter_graph_transport("/usr/bin/ffmpeg", &arguments, directory.path(), true);

        let file = file.expect("transport file");
        assert_eq!(spawned[2], "-/filter_complex");
        assert_eq!(spawned[3], file.path().to_string_lossy());
        assert_eq!(
            std::fs::read_to_string(file.path()).expect("graph"),
            "[0:0]null[v]"
        );
        // The graph itself is gone from argv, so it cannot hit a length limit.
        assert!(!spawned.iter().any(|argument| argument.contains("null[v]")));
    }

    #[test]
    fn an_unwritable_directory_falls_back_to_inline_instead_of_failing_the_spawn() {
        let arguments = args(&["-filter_complex", "[0:0]null[v]"]);
        // A path under a regular file can never be created as a directory.
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let directory = file.path().join("not-a-directory");

        let (spawned, transport) =
            apply_filter_graph_transport("/usr/bin/ffmpeg", &arguments, &directory, true);

        assert!(transport.is_none());
        assert_eq!(
            spawned, arguments,
            "playback must not fail over a cache dir"
        );
    }

    /// Real-FFmpeg gate for the transport. Ignored by default because it needs
    /// an `ffmpeg` binary.
    ///
    /// Run with:
    /// `ERSATZRS_FFMPEG=<path to ffmpeg> ./run.sh test -p ersatzrs-infra \
    ///  --run-ignored ignored-only filter_graph`
    // Test-only opt-in path; the configuration layer is for the application.
    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    #[ignore = "requires a real ffmpeg binary"]
    async fn real_ffmpeg_accepts_the_rewritten_file_transport_argv() {
        let Some(ffmpeg) = std::env::var_os("ERSATZRS_FFMPEG") else {
            eprintln!("set ERSATZRS_FFMPEG to run this gate");
            return;
        };
        let executable = ffmpeg.to_string_lossy().into_owned();

        // A directory whose name has a space and a non-ASCII character, which
        // ADR 0003 requires the transport to survive.
        let root = tempfile::tempdir().expect("temp dir");
        let directory = root.path().join("fïlter graphs");

        let arguments = args(&[
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:duration=0.4:rate=5",
            "-filter_complex",
            "[0:0]scale=160:120,setparams=range=tv[v]",
            "-map",
            "[v]",
            "-frames:v",
            "2",
            "-f",
            "null",
            "-",
        ]);

        let (spawned, file) =
            apply_filter_graph_transport(&executable, &arguments, &directory, true);
        let file = file.expect("transport file");
        let path = file.path().to_path_buf();

        let status = tokio::process::Command::new(&executable)
            .args(&spawned)
            .stdin(std::process::Stdio::null())
            .status()
            .await
            .expect("spawn ffmpeg");

        assert!(
            status.success(),
            "ffmpeg rejected -/filter_complex {}: {status}",
            path.display()
        );
        assert!(path.exists(), "the file must survive until the child exits");

        drop(file);
        assert!(!path.exists(), "cleanup after a successful exit");
    }

    /// Cancellation and supervision teardown are the paths ADR 0003 calls out
    /// explicitly, and they are also the ones easiest to forget. `kill_on_drop`
    /// plus an owned guard makes them the same code path as any other drop;
    /// this proves that holds while the child is still running.
    // Test-only opt-in path; the configuration layer is for the application.
    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    #[ignore = "requires a real ffmpeg binary"]
    async fn killing_a_running_child_still_removes_the_transport_file() {
        let Some(ffmpeg) = std::env::var_os("ERSATZRS_FFMPEG") else {
            eprintln!("set ERSATZRS_FFMPEG to run this gate");
            return;
        };
        let executable = ffmpeg.to_string_lossy().into_owned();
        let directory = tempfile::tempdir().expect("temp dir");

        // A long-running encode so the child is definitely alive when killed.
        let arguments = args(&[
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=25",
            "-filter_complex",
            "[0:0]scale=160:120[v]",
            "-map",
            "[v]",
            "-f",
            "null",
            "-",
        ]);
        let (spawned, file) =
            apply_filter_graph_transport(&executable, &arguments, directory.path(), true);
        let file = file.expect("transport file");
        let path = file.path().to_path_buf();

        let mut child = tokio::process::Command::new(&executable)
            .args(&spawned)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn ffmpeg");

        assert!(path.exists(), "the file must exist while FFmpeg reads it");

        // Exactly what `RemoteStreamProcess` teardown does: cancel the child,
        // then drop the guard it owns.
        child.kill().await.expect("kill ffmpeg");
        drop(file);

        assert!(!path.exists(), "cleanup after cancellation");
    }

    #[test]
    fn paths_with_spaces_and_non_ascii_round_trip() {
        let root = tempfile::tempdir().expect("temp dir");
        let directory = root.path().join("fïlter graphs");
        let arguments = args(&["-filter_complex", "[0:0]null[v]"]);

        let (spawned, file) =
            apply_filter_graph_transport("/usr/bin/ffmpeg", &arguments, &directory, true);

        let file = file.expect("transport file");
        assert!(file.path().to_string_lossy().contains("fïlter graphs"));
        // The path is one argv entry, so it survives spaces without quoting.
        assert_eq!(spawned[1], file.path().to_string_lossy());
        assert!(!spawned[1].contains('"'));
    }
}
