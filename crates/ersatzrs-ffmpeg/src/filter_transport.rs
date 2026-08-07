//! Long filter-graph transport (ADR 0003, milestone `p8c-01`).
//!
//! Complex graphs grow quickly once a channel combines subtitles, watermarks,
//! opacity expressions, native graphics, scaling, and HDR processing. Inline
//! `-filter_complex` eventually approaches the native command-line limit and the
//! channel simply stops spawning — most visibly on Windows, whose
//! `CreateProcess` command line is capped at 32767 characters.
//!
//! `FFmpeg` 9 removed `-filter_complex_script`; its replacement is the generic
//! file-option form `-/filter_complex <path>`. This module owns the pure half:
//! deciding whether a command needs file transport and rewriting its argv. The
//! temporary file's lifetime belongs to [`crate::runtime`].
//!
//! Verified against `FFmpeg` 9.0 that `-/filter_complex` accepts paths
//! containing spaces and non-ASCII characters.

/// Maximum projected complete command length, in bytes, that stays inline.
///
/// Windows caps a `CreateProcess` command line at 32767 characters. The
/// budget here leaves generous headroom for the executable path, per-argument
/// quoting that the OS layer adds, and the fact that this projection counts
/// bytes while Windows counts UTF-16 code units.
///
/// The same threshold applies on every platform. ADR 0003 rejects a
/// Windows-only branch: one tested rule that occasionally uses a file on Linux
/// is better than an untested path that only ever runs on one OS.
pub const MAX_INLINE_COMMAND_BYTES: usize = 24_000;

/// Maximum single filter-graph argument, in bytes, that stays inline.
///
/// Linux caps one argument at `MAX_ARG_STRLEN` (128 KiB), far above this, but a
/// graph this large is already worth moving out of the process arguments where
/// it is easier to inspect and cannot interact with any argv limit.
pub const MAX_INLINE_GRAPH_BYTES: usize = 8_192;

/// The argv flag ErsatzRS emits for an inline graph.
pub const INLINE_FILTER_FLAG: &str = "-filter_complex";
/// The `FFmpeg` 9 file-option flag for a graph read from a path.
///
/// Note the leading `-/`: this is `FFmpeg`'s generic "read this option's value
/// from a file" syntax, not a removed bespoke option.
pub const FILE_FILTER_FLAG: &str = "-/filter_complex";

/// How a command's complex filter graph should reach `FFmpeg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterGraphTransport {
    /// Pass the graph as one structured `-filter_complex` argument.
    Inline,
    /// Write the graph to a private per-spawn file and pass `-/filter_complex`.
    File,
}

/// Locate the inline complex-filter argument pair.
///
/// Returns the index of the flag and the graph value. `None` when the command
/// has no complex filter, or already uses file transport.
#[must_use]
pub fn find_inline_filter_graph(arguments: &[String]) -> Option<(usize, &str)> {
    arguments
        .iter()
        .position(|argument| argument == INLINE_FILTER_FLAG)
        .and_then(|index| {
            arguments
                .get(index + 1)
                .map(|graph| (index, graph.as_str()))
        })
}

/// Projected complete command length in bytes, counting the executable and one
/// separator per argument the way a flattened command line would.
///
/// This is a projection, not a shell command: ErsatzRS never joins arguments
/// into a shell string, and ADR 0003 forbids doing so.
#[must_use]
pub fn projected_command_bytes(executable: &str, arguments: &[String]) -> usize {
    arguments.iter().fold(executable.len(), |total, argument| {
        total + argument.len() + 1
    })
}

/// Decide how a command's graph should reach `FFmpeg`.
///
/// `force_file` exists so tests and troubleshooting can exercise the file path
/// without synthesising a 24 KB graph.
#[must_use]
pub fn select_filter_graph_transport(
    executable: &str,
    arguments: &[String],
    force_file: bool,
) -> FilterGraphTransport {
    let Some((_, graph)) = find_inline_filter_graph(arguments) else {
        // Nothing to move. Even an oversized command without a complex filter
        // stays as it is: this milestone owns graph transport, not a general
        // argv rewriter.
        return FilterGraphTransport::Inline;
    };

    if force_file
        || graph.len() > MAX_INLINE_GRAPH_BYTES
        || projected_command_bytes(executable, arguments) > MAX_INLINE_COMMAND_BYTES
    {
        return FilterGraphTransport::File;
    }

    FilterGraphTransport::Inline
}

/// Rewrite argv to read the graph from `path`.
///
/// The path is passed as its own argument, so spaces and non-ASCII characters
/// need no quoting and cannot be re-split.
#[must_use]
pub fn rewrite_for_file_transport(arguments: &[String], path: &str) -> Vec<String> {
    let Some((index, _)) = find_inline_filter_graph(arguments) else {
        return arguments.to_vec();
    };

    let mut rewritten = arguments.to_vec();
    FILE_FILTER_FLAG.clone_into(&mut rewritten[index]);
    path.clone_into(&mut rewritten[index + 1]);
    rewritten
}

/// A log-safe description of the transport choice.
///
/// ADR 0003 requires logging the choice and a sanitized path, never the whole
/// graph: a filter graph embeds media paths, subtitle file names, and watermark
/// locations, which are exactly the private details `CONTRIBUTING.md` keeps out
/// of logs.
#[must_use]
pub fn sanitized_transport_log(path: &std::path::Path, graph_bytes: usize) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("filter-graph");
    format!("filter graph moved to file transport ({graph_bytes} bytes, {name})")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn short_graphs_stay_inline() {
        let arguments = args(&[
            "-i",
            "/media/movie.mkv",
            INLINE_FILTER_FLAG,
            "[0:0]scale=1920:1080[v]",
            "pipe:1",
        ]);

        assert_eq!(
            select_filter_graph_transport("/usr/bin/ffmpeg", &arguments, false),
            FilterGraphTransport::Inline
        );
    }

    #[test]
    fn a_command_without_a_complex_filter_is_never_rewritten() {
        let arguments = args(&["-i", "/media/movie.mkv", "-c", "copy", "pipe:1"]);

        assert_eq!(
            select_filter_graph_transport("/usr/bin/ffmpeg", &arguments, true),
            FilterGraphTransport::Inline
        );
        assert_eq!(
            rewrite_for_file_transport(&arguments, "/tmp/graph.txt"),
            arguments
        );
    }

    #[test]
    fn an_oversized_graph_moves_to_a_file() {
        let graph = format!("[0:0]{}[v]", "scale=1920:1080,".repeat(1_000));
        assert!(graph.len() > MAX_INLINE_GRAPH_BYTES);
        let arguments = args(&[
            "-i",
            "/media/movie.mkv",
            INLINE_FILTER_FLAG,
            &graph,
            "pipe:1",
        ]);

        assert_eq!(
            select_filter_graph_transport("/usr/bin/ffmpeg", &arguments, false),
            FilterGraphTransport::File
        );
    }

    #[test]
    fn an_oversized_complete_command_moves_the_graph_even_when_the_graph_is_small() {
        // Many long input paths can blow the command-line budget while every
        // individual argument stays small. The graph is the only part that can
        // be moved, so it is what moves.
        let mut arguments = Vec::new();
        for index in 0..400 {
            arguments.push("-i".to_owned());
            arguments.push(format!("/media/library/{}/{index}.mkv", "x".repeat(50)));
        }
        arguments.push(INLINE_FILTER_FLAG.to_owned());
        arguments.push("[0:0]scale=1920:1080[v]".to_owned());

        assert!(projected_command_bytes("/usr/bin/ffmpeg", &arguments) > MAX_INLINE_COMMAND_BYTES);
        assert_eq!(
            select_filter_graph_transport("/usr/bin/ffmpeg", &arguments, false),
            FilterGraphTransport::File
        );
    }

    #[test]
    fn forcing_file_transport_overrides_the_threshold() {
        let arguments = args(&[INLINE_FILTER_FLAG, "[0:0]null[v]"]);

        assert_eq!(
            select_filter_graph_transport("/usr/bin/ffmpeg", &arguments, true),
            FilterGraphTransport::File
        );
    }

    #[test]
    fn rewriting_replaces_only_the_flag_and_value() {
        let arguments = args(&[
            "-nostdin",
            "-i",
            "/media/movie.mkv",
            INLINE_FILTER_FLAG,
            "[0:0]scale=1920:1080[v]",
            "-map",
            "[v]",
            "pipe:1",
        ]);

        let rewritten = rewrite_for_file_transport(&arguments, "/tmp/ersatzrs/graph .txt");

        assert_eq!(
            rewritten,
            args(&[
                "-nostdin",
                "-i",
                "/media/movie.mkv",
                FILE_FILTER_FLAG,
                "/tmp/ersatzrs/graph .txt",
                "-map",
                "[v]",
                "pipe:1",
            ])
        );
        // The path is its own argv entry, so spaces need no quoting.
        assert!(!rewritten.iter().any(|argument| argument.contains('"')));
    }

    #[test]
    fn the_removed_ffmpeg_8_option_is_never_emitted() {
        // FFmpeg 9 deleted `-filter_complex_script`; emitting it would fail at
        // spawn on the only runtime line ErsatzRS supports.
        let arguments = args(&[INLINE_FILTER_FLAG, "[0:0]null[v]"]);
        let rewritten = rewrite_for_file_transport(&arguments, "/tmp/graph.txt");

        assert!(
            !rewritten
                .iter()
                .any(|argument| argument.contains("filter_complex_script")),
            "{rewritten:?}"
        );
        assert_eq!(rewritten[0], "-/filter_complex");
    }

    #[test]
    fn logs_carry_the_choice_and_file_name_but_not_the_graph() {
        let log = sanitized_transport_log(
            std::path::Path::new("/var/lib/ersatzrs/tmp/graph-42.txt"),
            9_000,
        );

        assert!(log.contains("9000 bytes"), "{log}");
        assert!(log.contains("graph-42.txt"), "{log}");
        // No containing directory path, no graph contents.
        assert!(!log.contains("/var/lib"), "{log}");
        assert!(!log.contains("scale="), "{log}");
    }
}
