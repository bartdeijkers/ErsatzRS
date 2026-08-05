# ADR 0063: FFmpeg 9 runtime and profile options

## Status

Accepted (2026-08-05).

## Implementation

Implemented 2026-08-05 across runtime acquisition and validation, FFmpeg
command generation, SQLite/MySQL profile persistence, the localized profile
editor, health checks, and Remote Stream producer documentation. Existing
profiles are migrated without requiring an administrator to recreate them.

This ADR supersedes the FFmpeg version and package-line decision in
[ADR 0008](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md). ADR 0008 remains
the owner of the separate first-party runtime repository, checksum validation,
and package architecture.

## Context

ErsatzTV and the original ErsatzRS port emit the legacy output option
`-vsync cfr` and store NVENC low-latency presets as combined aliases such as
`llhp` and `llhq`. FFmpeg 9 rejects `-vsync`, and its NVENC interface models
performance presets and encoding tunes as separate options. The first-party
`ErsatzRS-ffmpeg` v2 package line follows that interface.

Keeping the old arguments caused every HLS transcode to fail before output was
opened. This also affected Remote Streams produced by the Beeld & Geluid
extractor: the extractor itself is a stream-copy producer, but its stdout is
consumed by the same shared transcode pipeline.

Accepting any version newer than the historical minimum would hide future
incompatible command-line changes. Persisting downloaded paths as ordinary
operator overrides would also pin an old managed runtime and prevent the
runtime manager from selecting an update on the next startup.

## Decision

ErsatzRS supports FFmpeg and ffprobe **9.x**. Startup rejects versions below 9
and unverified future major versions. The startup capability gate executes a
small `-fps_mode:v cfr` smoke command in addition to checking required filters,
encoders, decoders, protocols, and formats. NVIDIA encoder probing records the
advertised `preset` and `tune` choices; first-party FFmpeg 9 runtimes must
advertise presets `p1` through `p7` and tunes `hq`, `ll`, `ull`, and
`lossless` for each available NVENC encoder.

Generated transcode commands use `-fps_mode:v cfr` instead of `-vsync cfr`.
Diagnostic stream-copy commands use `-fps_mode:v passthrough` where an explicit
sync mode is needed. The Beeld & Geluid scripts remain stream-copy producers
and do not perform their own frame-rate normalization.

Add nullable `VideoTune` to `FFmpegProfile` in SQLite and MySQL. The admin
editor presents NVENC preset and tune as separate localized fields. The
supported preset values are `p1` through `p7`; tune values are `hq`, `ll`,
`ull`, and `lossless`. Fresh NVENC defaults use `p7` with tune `ll`.

The additive migration and the command-generation boundary translate legacy
values case-insensitively:

| Legacy preset | FFmpeg 9 preset | FFmpeg 9 tune |
| --- | --- | --- |
| `llhp` | `p1` | `ll` |
| `llhq` | `p7` | `ll` |

The boundary conversion protects databases that have not yet run the migration
and imported profiles that still carry the ErsatzTV value.

Managed acquisition defaults to `ErsatzRS-ffmpeg` v2 and never falls back to a
v1 package. A persisted ownership marker distinguishes paths selected by the
runtime manager from paths saved through the admin settings page. Managed
paths may be replaced during startup; administrator-saved paths remain explicit
operator overrides. When the first-party release has no asset for the current
RID, startup may accept an operator-provided FFmpeg 9 runtime from `PATH`, but
it never accepts FFmpeg 8.

## Consequences

- FFmpeg 9 packages play existing channels without the removed `-vsync`
  option or invalid NVENC aliases.
- Existing `llhp` and `llhq` profiles retain their intended low-latency
  performance/quality bias after migration.
- FFmpeg 8 installations must be upgraded before ErsatzRS starts. This is an
  intentional ErsatzTV divergence; rollback to FFmpeg 8 is not supported.
- A future FFmpeg major requires an explicit compatibility review and code
  change instead of being accepted optimistically.
- The profile schema and editor gain one field. Software and non-NVIDIA
  encoders continue to omit `VideoTune`.
- Beeld & Geluid and other direct producer scripts inherit the shared FFmpeg 9
  transcode contract without duplicating output-rate flags.

## Alternatives Considered

- **Roll back to FFmpeg 8.1.2.** Rejected: the first-party package line and
  project direction are FFmpeg 9, and retaining removed options only postpones
  the compatibility work.
- **Keep `llhp` and `llhq` and translate them only in the UI.** Rejected:
  scheduled playback and imported databases can bypass the editor, so the
  persistence migration and command boundary both need compatibility guards.
- **Treat preset and tune as one free-form string.** Rejected: FFmpeg 9 exposes
  independent option domains, and a combined string cannot be validated or
  round-tripped reliably.
- **Accept every FFmpeg version at or above 9.** Rejected: unverified future
  majors may remove or reinterpret command-line options just as FFmpeg 9 did.
- **Add FFmpeg flags to each Remote Stream extractor.** Rejected: extractors
  own acquisition and stream copy; the shared pipeline owns transcoding and
  synchronization.

## References

- [FFmpeg command-line documentation](https://ffmpeg.org/ffmpeg.html)
- [FFmpeg NVENC encoder option definitions](https://git.ffmpeg.org/gitweb/ffmpeg.git/blob/HEAD:/libavcodec/nvenc.c)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0008: FFmpeg 8.1 runtime and multi-arch images](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
- [ADR 0017: Default FFmpeg profiles for multi-resolution streaming](0017-default-ffmpeg-profile-1080p60.md)
