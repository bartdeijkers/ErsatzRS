# FFmpeg zscale Runtime Remediation

## Context

`./run.sh serve` starts the web host and initializes the SQLite database, but
startup fails during the FFmpeg runtime check:

```text
FFmpeg runtime is missing required capabilities: [RequiredFilter { name: "zscale" }]
```

This is a runtime packaging/configuration gap, not an application startup bug.
ErsatzRS intentionally fails fast when the selected FFmpeg binary does not
match the runtime contract. The `zscale` filter is required for HDR and tonemap
filter graphs, and FFmpeg only exposes it when built with zimg support
(`--enable-libzimg`).

Do not weaken or remove the runtime check. The correct fix is to select or
build an FFmpeg 8.1 runtime that includes both the graphics requirements
(`drawvg` via cairo) and the color pipeline requirements (`zscale` via zimg).

## Goals

- Make the local self-built FFmpeg rebuild path produce a runtime with
  `zscale`.
- Document the exact diagnosis and remediation path for future startup
  failures.
- Keep release/runtime plans aligned with the actual startup capability checks.
- Preserve the existing fail-fast startup behavior.

## Non-Goals

- Do not make `zscale` optional.
- Do not downgrade the FFmpeg version requirement below 8.1.
- Do not add an alternate tonemap implementation.
- Do not change database startup, server binding, or Leptos serving behavior.

## Implementation Plan

1. Update `scripts/rebuild-ffmpeg-nvenc.sh`.
   - Add `libzimg-dev` / `pkg-config zimg` to the prerequisite list.
   - Add a preflight check that fails clearly when zimg development headers are
     unavailable.
   - Add `--enable-libzimg` to the FFmpeg configure flags.
   - Extend post-build verification to check:
     - `ffmpeg -hide_banner -buildconf` contains `--enable-libzimg`.
     - `ffmpeg -hide_banner -filters` contains `zscale`.
     - A short `zscale,tonemap,zscale` smoke transcode succeeds.
   - Keep the existing NVENC and `drawvg` checks.

2. Update `plans/release-self-built-ffmpeg.md`.
   - Treat `--enable-libzimg` and `zscale` as required release capabilities
     alongside `--enable-cairo` and `drawvg`.
   - Add zimg to macOS dependency guidance if it is not already present.
   - Add required validation commands for `--enable-libzimg`, `zscale`, and a
     tonemap smoke transcode.
   - Update acceptance criteria so release archives cannot pass with a
     drawvg-only FFmpeg build.

3. Update `docs/reference/configuration.md`.
   - State explicitly that FFmpeg must expose `drawvg` and `zscale`.
   - Add quick diagnostic commands:

     ```bash
     ffmpeg -hide_banner -buildconf | grep -- --enable-libzimg
     ffmpeg -hide_banner -filters | grep -w zscale
     ```

   - Explain that a missing `zscale` filter means the configured FFmpeg binary
     should be rebuilt with zimg support or replaced by a compliant binary.
   - Point users at the accepted FFmpeg/ffprobe path override variables when
     multiple FFmpeg installs exist.

4. Optionally add this plan to `plans/INDEX.md` if it should remain visible as
   an active remediation contract.

## Local Closeout

After implementing the documentation and script changes, verify without running
a full FFmpeg rebuild:

```bash
bash -n scripts/rebuild-ffmpeg-nvenc.sh
git diff --check
```

If a compliant FFmpeg binary is already available locally, also verify:

```bash
ffmpeg -hide_banner -buildconf | grep -- --enable-libzimg
ffmpeg -hide_banner -filters | grep -w zscale
ffmpeg -hide_banner -loglevel error \
  -f lavfi -i color=c=black:s=64x64:d=0.1 \
  -vf "zscale=transfer=linear,tonemap=hable,zscale=transfer=bt709,format=yuv420p" \
  -frames:v 1 -f null -
```

Then restart `./run.sh serve` with `PATH`, `FFMPEG_PATH`, or
`ERSATZRS_FFMPEG__PATH` pointing at the compliant binary.

## Acceptance Criteria

- The local rebuild script configures FFmpeg with `--enable-libzimg`.
- The rebuild script fails clearly before install if `zscale` is still absent.
- The release FFmpeg plan requires `--enable-libzimg` and `zscale`.
- The configuration reference documents how to diagnose and fix a missing
  `zscale` filter.
- ErsatzRS startup still fails fast when the selected FFmpeg runtime is missing
  required capabilities.
