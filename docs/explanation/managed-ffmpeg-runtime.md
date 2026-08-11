# Why ErsatzRS manages FFmpeg

FFmpeg is not just an optional transcoder in ErsatzRS. Its version and build
capabilities define whether graphics, subtitles, color conversion, codecs, and
hardware acceleration behave correctly. A system-provided `ffmpeg` command can
be new enough while still missing a required filter or library.

## Why version checks are not enough

Startup probes the selected FFmpeg and ffprobe pair and checks the capabilities
that ErsatzRS depends on. A version number alone cannot prove that a particular
build contains the required filters, codecs, or hardware integrations.

The server does not become ready when a required capability is missing. This
turns a later, channel-specific playback failure into an immediate health
result with the selected runtime path and missing capability. The
[system-requirements reference](../reference/system-requirements.md) owns the
supported version and capability contract.

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

## Why resolution has fallbacks

Operators need reproducible defaults without losing the ability to run offline
or use a specialized build. ErsatzRS therefore prefers explicit operator
choices and reusable managed state, can acquire a first-party runtime, and can
fall back to a compatible pair on `PATH`. Whichever source wins, FFmpeg and
ffprobe are validated together. The
[configuration reference](../reference/configuration.md) owns the exact
precedence and environment variables.

## The offline trade-off

Managed acquisition makes the common path predictable, but it cannot help an
isolated host. An offline deployment must make a compliant runtime available
before startup. Disabling downloads without another valid pair makes readiness
fail; this is intentional because starting without a usable media runtime
would only defer the failure until playback.
