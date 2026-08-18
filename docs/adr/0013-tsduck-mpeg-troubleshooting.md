# ADR 0013: Optional TSDuck MPEG-TS troubleshooting

## Status

Accepted

## Implementation

Implemented 2026-06-28. ErsatzRS now exposes `/system/troubleshooting/mpeg`
as an optional in-app Support tool, detects `tsp --version`, captures bounded
channel MPEG-TS samples, runs read-only `tsp`/`tsbitrate` analysis, writes
redacted `report.json` / `summary.json` archive metadata, deletes raw `.ts`
samples unless the user opts in, and reports TSDuck availability as a
non-blocking health row. The manual
`scripts/e2e/tsduck-playback-gate.sh` gate remains separate verification
tooling.

## Context

ErsatzRS already owns the FFmpeg streaming path, IPTV/HLS/HDHomeRun endpoints,
startup runtime checks, persisted health results, and the
`/system/troubleshooting` Support surface. FFmpeg logs and `ffprobe` metadata
are useful, but MPEG-TS playback failures often need packet-level visibility:
continuity counters, PAT/PMT/service/PID structure, PCR validity, PCR jitter,
and bitrate estimates.

TSDuck is an MPEG Transport Stream toolkit with command-line utilities and
`tsp` plugins for analyzing and manipulating transport streams. Its official
documentation describes `tsp` as a transport stream processor using a chain of
plugins, `tsanalyze` and the `analyze` plugin for stream structure analysis,
`continuity` for continuity-counter checks, `pcrverify` for PCR validation,
and `tsbitrate` for bitrate evaluation from PCR values. It also includes
repair or transformation utilities such as `tsfixcc`, `tsfclean`,
`pcradjust`, and `pcrcopy`.

ErsatzTV does not appear to use TSDuck. A local source search in
`$ERSATZTV_SRC` found no `TSDuck`, `tsduck`, `tsp`, `tsanalyze`, `tsbitrate`,
`pcrverify`, `tsfixcc`, or `tsfclean` usage. Using TSDuck in ErsatzRS is
therefore an intentional diagnostic extension, not parity behavior.

## Decision

Use TSDuck as an optional external CLI toolkit for explicit MPEG-TS
troubleshooting runs.

Keep FFmpeg as the only streaming and transcoding engine. Continue to follow
[ADR 0003](0003-ffmpeg-cli-spawn.md): spawn external tools with structured
arguments and no shell invocation. Do not link to TSDuck C++ libraries, write
custom TSDuck plugins, or make TSDuck part of the normal playback path.

TSDuck is not required for startup, `/healthz`, normal IPTV/HLS/HDHomeRun
playback, Docker images, or CI. Missing TSDuck is reported as unavailable or
not applicable in Support tooling and health detail rows; it is not a fatal
runtime condition.

The v1 diagnostic flow captures a bounded MPEG-TS sample from a selected
channel, then runs read-only analysis commands:

```bash
tsp --realtime=off \
  -I file <sample.ts> \
  -P continuity --json-line=ERSATZRS_CONTINUITY \
  -P pcrverify \
  -P analyze --json --output-file <analysis.json> --error-analysis \
  -O drop
```

```bash
tsbitrate --value-only <sample.ts>
```

The report may include TSDuck JSON, sanitized logs, continuity findings, PCR
findings, and bitrate estimates. It must not auto-repair production output or
replace an ErsatzRS stream. Repair/transformation tools such as `tsfixcc`,
`tsfclean`, `pcradjust`, and `pcrcopy` are out of scope for v1.

## Consequences

Operators get repeatable MPEG-TS diagnostics without changing playback
behavior or FFmpeg pipeline parity.

Support archives become more useful for TS-specific bugs because they can show
which PID, service, PCR, or continuity condition failed. Reports must be
bounded and redacted because request URLs can carry credentials and raw samples
can contain user media.

The deployment remains simple for users who do not need deep TS diagnostics.
Packaging TSDuck into release images can be evaluated later, after reviewing
license, size, and multi-architecture build cost. TSDuck must be installed
separately by contributors who need it; install steps live in
[Troubleshoot startup and playback](../how-to/troubleshoot-startup-and-playback.md).
The maintainer's dev box (WSL Ubuntu 24.04) runs TSDuck 3.44-4676.

The TSDuck runner adds process-management and parser code, but keeps the
complex MPEG-TS inspection logic in a mature external toolkit instead of
reimplementing it in Rust for v1.

## Alternatives Considered

- FFprobe-only troubleshooting: rejected because it does not provide the same
  packet-level continuity, PSI/SI, PCR, and bitrate diagnostics.
- Custom Rust MPEG-TS analyzer: rejected for v1 because it would recreate a
  mature toolkit before the troubleshooting workflow is proven.
- Link against TSDuck C++ APIs: rejected because it adds ABI and build
  complexity and weakens the single-static-binary deployment goal.
- Make TSDuck mandatory: rejected because normal streaming must not depend on
  a troubleshooting-only tool.
- Auto-repair streams with TSDuck: rejected for v1 because diagnostics should
  reveal FFmpeg or pipeline defects, not hide them.

## References

- [Troubleshoot startup and playback](../how-to/troubleshoot-startup-and-playback.md)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0008: FFmpeg 8.1 runtime and multi-arch images](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
- [Support parity matrix](../reference/parity-matrix.md)
- [TSDuck User Guide](https://tsduck.io/docs/tsduck.html)
- [TSDuck commands and plugins](https://tsduck.io/commands/)
