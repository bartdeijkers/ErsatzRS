# ADR 0003: Invoke FFmpeg through the CLI

## Status

Accepted

## Implementation

Partial — every pipeline spawns structured FFmpeg argv through
`tokio::process::Command`, but long complex filter graphs are still passed
inline and do not yet use the FFmpeg 9 file-option transport decided below.

## Context

ErsatzTV builds FFmpeg command lines and runs FFmpeg as a child process. ErsatzRS
needs to preserve those pipeline patterns while using its supported FFmpeg CLI
features.

Inline `-filter_complex` values can grow substantially when schedules combine
subtitles, watermarks, opacity expressions, native graphics, scaling, and HDR
processing. They eventually approach the native process command-line limit,
most visibly on Windows. FFmpeg 9 removed `-filter_complex_script` and replaces
it with the generic `-/filter_complex <path>` file-option syntax. ErsatzRS did
not emit the removed option, but it needs the replacement as a safe long-graph
transport before complex channels become unspawnable.

## Decision

Use `tokio::process::Command` to spawn FFmpeg. Do not use libav bindings and do
not invoke FFmpeg through a shell.

Keep short filter graphs inline as one structured `-filter_complex` argument.
When the encoded graph or projected complete argv exceeds a conservative,
tested threshold, write the graph to a private per-spawn file and pass it as
the structured pair `-/filter_complex <path>`. Apply the same selection rule on
all native platforms instead of embedding an unguarded Windows assumption in
Rust source.

The file is a transport artifact, not configuration or cache state. Its owner
must:

- create it in an ErsatzRS-controlled temporary directory with restrictive
  permissions where the platform supports them;
- preserve the exact graph bytes without shell quoting or expansion;
- retain it until FFmpeg has finished using it, including cancellation and
  abnormal exit;
- remove it deterministically after child-process cleanup; and
- log the transport choice and sanitized path, not duplicate the complete
  potentially sensitive graph in ordinary logs.

Command-generation tests must cover both inline and forced-file transport.
Runtime tests must cover spaces and non-ASCII paths, spawn failure,
cancellation, and cleanup.

## Consequences

ErsatzRS can mirror ErsatzTV's argv and filter-graph structure closely and can move
between FFmpeg builds without binding churn. Process supervision, logging, and
argument escaping must be handled carefully.

The long-graph path avoids native command-line length failures without changing
filter semantics or creating an OS-specific pipeline. It introduces temporary
artifact lifecycle and privacy responsibilities, so process supervisors must
own cleanup rather than leaving it to best-effort startup maintenance.

## Alternatives Considered

- libav bindings: rejected because they increase coupling and diverge from
  ErsatzTV's proven process model.
- Shell invocation: rejected because arguments must be structured and safe.
- Always write every filter graph to a file: rejected because short inline
  argv is easier to inspect and does not need artifact lifecycle management.
- Add a Windows-only graph path: rejected because a portable threshold is
  simpler to test and avoids platform-specific command-generation semantics.

## References

- [../../PORTING.md](../../PORTING.md)
- [ADR 0063: FFmpeg 9 runtime and profile options](0063-ffmpeg-9-runtime-and-profile-options.md)
- [FFmpeg 9 performance and support progress](../progress/parity/ffmpeg-9-performance-and-support.toml)
