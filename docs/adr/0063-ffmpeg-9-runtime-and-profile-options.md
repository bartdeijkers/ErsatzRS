# ADR 0063: FFmpeg 9 runtime and profile options

## Status

Accepted (2026-08-05; revised — see Revision history).

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
add-on: its producer process uses stream copy, but its stdout is consumed by
the same shared transcode pipeline.

Accepting any version newer than the historical minimum would hide future
incompatible command-line changes. Persisting downloaded paths as ordinary
operator overrides would also pin an old managed runtime and prevent the
runtime manager from selecting an update on the next startup.

The Windows tray launcher follows that same ownership boundary: colocated tools
that exactly match both executables in the tagged managed cache are generated
copies, not new operator overrides. Leave their selection to the runtime manager
on restart so update checks and installed-version Health remain active. Compare
complete regular-file contents with bounded buffers; names, timestamps or a
matching prefix do not prove identity. Missing, incomplete or different managed
copies retain manual colocation behavior. Explicit environment choices keep
precedence, including when a colocated companion completes a partial choice.
Use the existing state-root override/default layout and normal runtime validation;
introduce no new marker store, download policy or runtime wire contract.

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
sync mode is needed. The Beeld & Geluid add-on entrypoints remain stream-copy
producers and do not perform their own frame-rate normalization.

Add nullable `VideoTune` to `FFmpegProfile` in SQLite and MySQL. The admin
editor presents NVENC preset and tune as separate localized fields. The
supported preset values are `p1` through `p7`. Tune values are `hq`, `ll`,
`ull`, and `lossless`, plus `uhq` on the encoders that advertise it — FFmpeg 9
exposes `uhq` on `hevc_nvenc` and `av1_nvenc` but not on `h264_nvenc`, whose
tune range is 1-4, so the option is gated by codec rather than offered
uniformly.

Fresh NVENC seed defaults are owned by
[ADR 0017](0017-default-ffmpeg-profile-1080p60.md), not by this ADR. The
migration table below deliberately does **not** set them: it exists to preserve
already-saved profiles bit-exactly, and the `p7` + `ll` it produces is the
slowest preset paired with the low-latency tune. ADR 0017 v5 seeds `p5` + `hq`
instead.

The additive migration and the command-generation boundary translate legacy
values case-insensitively:

| Legacy preset | FFmpeg 9 preset | FFmpeg 9 tune |
| --- | --- | --- |
| `llhp` | `p1` | `ll` |
| `llhq` | `p7` | `ll` |

The boundary conversion protects databases that have not yet run the migration
and imported profiles that still carry the ErsatzTV value.

### Why this table follows FFmpeg, not NVIDIA

Two different mappings exist for the deprecated low-latency presets, and they
disagree. NVIDIA's own preset migration guide recommends architecture-dependent
equivalents (for H.264 on Ampere/Turing, `LowLatencyDefault` maps to P4 and
`LowLatencyHP` to P3). FFmpeg 8.1's `libavcodec/nvenc.c` instead resolved
`PRESET_LOW_LATENCY_HQ` to `NV_ENC_PRESET_P7_GUID` and `PRESET_LOW_LATENCY_HP`
to `NV_ENC_PRESET_P1_GUID`, both with `NVENC_LOWLATENCY` tuning.

ErsatzRS deliberately follows **FFmpeg's** mapping. The values ErsatzRS
migrates are the values FFmpeg 8.1 was already translating them into, so an
administrator's existing NVENC profile keeps producing the exact stream it
produced before the upgrade. Adopting NVIDIA's table instead would have
silently retuned every migrated profile at the same moment the runtime changed,
making any playback regression impossible to attribute.

This equivalence is empirically verified rather than assumed — see the
`nvenc-legacy-preset-migration-equivalence` entry in
`docs/progress/parity/ffmpeg-command-generation.toml`.

### H.264 NVENC profile default

FFmpeg 9 changed the `h264_nvenc` default `profile` from `main` to `high`
(`hevc_nvenc` is unchanged at `main`). ErsatzRS emits `-profile:v` only when the
FFmpeg profile record carries a video profile, so an NVENC H.264 profile that
leaves it unset inherits the encoder default and changed behavior at the
runtime upgrade.

ErsatzRS does not pin an explicit default. ErsatzTV's `EncoderH264Nvenc` applies
the same optional emission (`foreach` over an `Option<string>` video profile), so
ErsatzTV running on FFmpeg 9 would behave identically. This is upstream behavior
observed at exact ErsatzTV parity, not a port gap, and pinning `main` would
invent a divergence ahead of evidence.

The residual compatibility risk — High profile can be rejected by legacy or
constrained IPTV and HDHomeRun clients — is recorded with its evidence in the
`ffmpeg-9-differential-review` entry in
`docs/progress/parity/ffmpeg-command-generation.toml`, to be revisited if a
client-compatibility report justifies pinning `main`.

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
  performance/quality bias after migration. The 2026-08-07 review proved this
  is bit-exact, not approximate.
- NVENC H.264 profiles with no explicit video profile now encode as High rather
  than Main. This is an accepted, recorded behavior change, not an oversight.
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
- **Adopt NVIDIA's preset migration table.** Rejected: it disagrees with the
  mapping FFmpeg 8.1 actually applied, so it would have retuned every migrated
  NVENC profile during the runtime upgrade instead of preserving it.
- **Pin `-profile:v main` for NVENC H.264 to preserve the FFmpeg 8 default.**
  Rejected: ErsatzTV emits `-profile:v` with the same optional semantics and has
  no such pin, so pinning would create an ErsatzTV divergence to compensate for
  an upstream default both projects inherit. Recorded with evidence instead.

## Revision history

- **2026-08-09 (v3).** Expose FFmpeg 9's `uhq` NVENC tune, gated by codec
  because `h264_nvenc` does not advertise it. Move ownership of the fresh
  NVENC seed defaults to ADR 0017, which retunes them from `p7` + `ll` to
  `p5` + `hq`; the legacy `llhp`/`llhq` migration table is unchanged and still
  preserves saved profiles bit-exactly.
- **2026-08-07 (v2 — documentation only).** Recorded the evidence and rationale
  behind the v1 migration table after an 8.1.2-vs-9.0 differential review:
  the `llhp`/`llhq` mapping follows FFmpeg's internal resolution rather than
  NVIDIA's migration guide, and is proven bit-exact. Documented the
  `h264_nvenc` default profile change from `main` to `high` as an accepted
  upstream behavior change observed at exact ErsatzTV parity. No code or schema
  change.
- **2026-08-05 (v1 — original).** Adopted FFmpeg 9.x as the supported runtime
  line, replaced `-vsync` with `-fps_mode:v`, split NVENC preset and tune into
  independent persisted fields with legacy `llhp`/`llhq` translation, and made
  managed runtime paths updateable while preserving operator overrides.

## References

- [FFmpeg command-line documentation](https://ffmpeg.org/ffmpeg.html)
- [FFmpeg NVENC encoder option definitions](https://git.ffmpeg.org/gitweb/ffmpeg.git/blob/HEAD:/libavcodec/nvenc.c)
- [FFmpeg 9.0 Changelog](https://git.ffmpeg.org/gitweb/ffmpeg.git/blob/refs/heads/release/9.0:/Changelog)
- [NVIDIA NVENC preset migration guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvenc-preset-migration-guide/index.html)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0008: FFmpeg 8.1 runtime and multi-arch images](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
- [ADR 0017: Default FFmpeg profiles for multi-resolution streaming](0017-default-ffmpeg-profile-1080p60.md)
