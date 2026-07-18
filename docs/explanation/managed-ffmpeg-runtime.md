# Why ErsatzRS manages FFmpeg

FFmpeg is not just an optional transcoder in ErsatzRS. Its version and build
capabilities define whether graphics, subtitles, color conversion, codecs, and
hardware acceleration behave correctly. A system-provided `ffmpeg` command can
be new enough while still missing a required filter or library.

## The runtime contract

ErsatzRS requires FFmpeg and ffprobe 8.1 or newer. Startup probes the selected
binaries and checks required capabilities, including `drawvg` for in-process
graphics and `zscale` for color and HDR processing.

The server does not become ready when a required capability is missing. This
turns a later, channel-specific playback failure into an immediate health
result with the selected runtime path and missing capability.

## Why releases do not bundle FFmpeg

Official application archives and Docker images follow the same acquisition
model: they ship ErsatzRS, browser assets, and graphics assets, but not FFmpeg.
At startup, ErsatzRS can download the matching first-party `ErsatzRS-ffmpeg`
release for the current platform and keep it in persistent application state.

This has several benefits:

- application packages remain separate from large platform-specific runtime
  archives;
- the runtime can be updated without replacing user data;
- every installation uses the same capability validation; and
- operators can still supply a compliant custom build for offline or
  specialized deployments.

## Resolution order

At startup, ErsatzRS resolves the runtime in this order:

1. explicit FFmpeg and ffprobe path overrides or persisted paths;
2. an existing managed runtime in application state;
3. a managed download from the selected first-party release;
4. compatible binaries on `PATH` when download is unavailable or disabled.

The chosen pair is validated together. On subsequent starts, a managed runtime
can be checked for updates, and the admin health page reports update state.

## Offline deployments

An offline deployment must prepare the runtime before startup. It can:

- pre-place a managed runtime in the state directory;
- set explicit FFmpeg and ffprobe paths; or
- make a compliant pair available on `PATH` and disable managed download.

Disabling downloads without another valid pair makes startup fail. Pinning
`ERSATZRS_FFMPEG_RELEASE_TAG` is useful for controlled environments that allow
downloads but require an immutable runtime version.

See the [configuration reference](../reference/configuration.md) for exact
variables and [Troubleshoot startup and playback](../how-to/troubleshoot-startup-and-playback.md)
for failure diagnosis.
