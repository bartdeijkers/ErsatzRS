# Acquire FFmpeg 8.1 at runtime from ErsatzRS-ffmpeg releases

> Revised 2026-06-19. This plan previously bundled a self-built FFmpeg into every
> release archive. Per [ADR 0008](../docs/adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
> (revised), ErsatzRS now ships **without** FFmpeg/ffprobe and acquires the
> runtime at startup from the first-party
> [`bartdeijkers/ErsatzRS-ffmpeg`](https://github.com/bartdeijkers/ErsatzRS-ffmpeg)
> GitHub Releases, with an update-availability indicator on `/system/health`.

## Context

ErsatzRS startup requires both `ffmpeg` and `ffprobe` to report a parseable
version `>= 8.1`, the `drawvg` filter (via `--enable-cairo`), and the `zscale`
filter (via `--enable-libzimg`) for the HDR/tonemap color pipeline. Public
BtbN/Gyan-style builds either miss those capabilities or do not produce
version/configuration output the runtime checker accepts.

Bundling a self-built FFmpeg into every release archive met the capability
requirement but produced large six-RID artifacts and coupled the FFmpeg lifecycle
to the ErsatzRS release cadence. Instead:

- First-party FFmpeg 8.1 builds are produced and published by the separate
  `ErsatzRS-ffmpeg` repository as **per-RID GitHub Release packages**.
- ErsatzRS application artifacts ship without FFmpeg/ffprobe.
- A startup runtime manager resolves an existing runtime, downloads the matching
  `ErsatzRS-ffmpeg` package when none is usable, and checks for newer releases to
  surface on `/system/health`.

## Goals

- Ship ErsatzRS release archives/images **without** `ffmpeg`/`ffprobe`.
- Resolve a usable FFmpeg/ffprobe runtime at startup in a deterministic order
  (env override → managed runtime dir → `PATH`).
- When no resolved runtime satisfies the capability contract, download the
  `ErsatzRS-ffmpeg` release package matching the current RID, verify its published
  checksum, install it into a managed runtime directory, and record the installed
  release tag.
- Run the existing capability checks against the resolved/downloaded runtime
  before it is accepted: parseable `ffmpeg -version` / `ffprobe -version`,
  `--enable-cairo`, `drawvg`, `--enable-libzimg`, `zscale`, required codecs, a
  real `drawvg` smoke transcode, and a real `zscale`/`tonemap` smoke transcode.
- Check the latest `ErsatzRS-ffmpeg` release at startup and expose an
  "update available" indicator (installed tag → latest tag) in the health report
  and on `/system/health`.
- Remove all FFmpeg bundling and BtbN/Gyan download assumptions from
  `.github/workflows/release.yml` and release docs.
- Keep environment overrides higher priority than the managed runtime, and keep a
  documented offline/air-gapped path.

## Non-Goals

- Do not change the graphics-engine runtime requirements.
- Do not lower the required FFmpeg version below 8.1.
- Do not silently auto-replace a working runtime; updates are notify + explicit
  apply.
- Do not own the FFmpeg build here — configure flags, the source pin/SHA-256, and
  per-RID compilation live in `ErsatzRS-ffmpeg`.

## ErsatzRS-ffmpeg release contract

The `ErsatzRS-ffmpeg` repository owns the build and publishes, per supported RID,
a redistributable package containing relocatable `ffmpeg`/`ffprobe` plus their
library closure. ErsatzRS depends on this contract:

- One asset per RID: `linux-x64`, `linux-arm64`, `linux-arm`, `osx-x64`,
  `osx-arm64`, `win-x64`. Stable, RID-keyed asset names.
- A published checksum (e.g. `*.sha256`) per asset that ErsatzRS verifies before
  install.
- Each asset passes, on the relocated payload, the same capability gate ErsatzRS
  enforces (see Required Validation) so a drawvg-only / zimg-less build cannot be
  published.
- Release tags are orderable so ErsatzRS can compare installed vs latest.
- Packages are redistributable: no `--enable-nonfree`, CUDA toolkit, or libnpp in
  the default published assets.

`ErsatzRS-ffmpeg` keeps its own platform build details (Linux Docker export with
`-Wl,-rpath,$ORIGIN/../lib`, Windows MSYS2 UCRT64/MINGW64 + DLL closure, macOS
Homebrew + `@loader_path/../lib` dylib relocation). They are no longer carried in
this repository.

## Required Validation (applied to the resolved/downloaded runtime)

Before a runtime is accepted (whether resolved from `PATH`, the managed dir, or a
fresh download), it must pass:

```bash
<ffmpeg> -version
<ffprobe> -version
<ffmpeg> -hide_banner -buildconf | grep -- --enable-cairo
<ffmpeg> -hide_banner -filters | grep -w drawvg
<ffmpeg> -hide_banner -buildconf | grep -- --enable-libzimg
<ffmpeg> -hide_banner -filters | grep -w zscale
<ffmpeg> -hide_banner \
  -f lavfi -i color=c=black:s=32x32:d=0.1 \
  -vf "drawvg=script='rect 4 4 16 16 fill',format=yuv420p" \
  -frames:v 1 -f null -
<ffmpeg> -hide_banner -loglevel error \
  -f lavfi -i "color=c=black:s=64x64:d=0.1,format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc" \
  -vf "zscale=t=linear:npl=100,tonemap=hable,zscale=p=bt709:t=bt709:m=bt709,format=yuv420p" \
  -frames:v 1 -f null -
```

The HDR smoke source must be tagged with HDR color metadata
(`setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc`);
otherwise `zscale` reports "no path between colorspaces" even on a correct zimg
build and the check fails spuriously.

## Runtime Manager

Add an FFmpeg runtime manager that runs during startup capability detection.

### Resolution order

1. Explicit env/config override (operator-provided `ffmpeg`/`ffprobe` path) —
   always wins, never auto-downloaded.
2. Managed runtime directory under the ErsatzRS data dir, e.g.
   `<data_dir>/ffmpeg/<tag>/bin/{ffmpeg,ffprobe}[.exe]`, with a stable
   `current` pointer.
3. `PATH` (`ffmpeg` / `ffprobe`) for developer installs.

### Download / install

- If steps 1–3 yield no runtime that passes Required Validation, select the
  `ErsatzRS-ffmpeg` release asset for the current RID (pinned minimum/known-good
  tag by default), download it, verify the published checksum, extract into the
  managed runtime directory, run Required Validation, and only then promote it via
  the `current` pointer and record the installed tag.
- A download or validation failure aborts startup with an explicit, actionable
  error (which capability/version failed, and the manual-install path).
- Offline/air-gapped: an operator can pre-place a managed runtime directory or set
  the env override; the manager must detect this and skip the network entirely.

### Update check

- Record the installed `ErsatzRS-ffmpeg` tag. At startup query the latest release
  tag (best-effort, network-failure-tolerant).
- A *missing* runtime is auto-downloaded. An *outdated but valid* runtime is left
  in place and an "update available" flag (installed → latest) is set.
- Applying an update is an explicit action; never swap a runtime mid-operation.

### Code touch points

- `crates/ersatzrs-ffmpeg/src/version.rs` — resolver + capability gate + manager
  entry, replacing any bundled-`ffmpeg/bin`-sibling assumption.
- `crates/ersatzrs-scanner/src/main.rs` — scanner `ffprobe` discovery uses the
  same resolver/managed path.
- `ersatzrs-core::health` / `ersatzrs-infra::health` — add an FFmpeg-runtime
  health check (resolved source, installed version, update-available) per the
  Health Check Parity contract.

Add resolver tests proving: env override wins; a pre-placed managed dir is
discovered without network; `PATH` fallback works; an outdated managed runtime
sets the update flag; checksum mismatch is rejected.

## Health surfacing

`/system/health` reports the resolved FFmpeg source (override / managed / PATH),
installed FFmpeg+ffprobe version, and whether a newer `ErsatzRS-ffmpeg` release is
available. The indicator must localize per the language contract and not spawn
`ffmpeg` per request — it reads the captured startup report.

## Release workflow changes

In `.github/workflows/release.yml`:

- Delete the `Bundle FFmpeg 8.1 with drawvg (win-x64)` download/staging step and
  any `ffmpeg/bin` staging across archives.
- Do **not** add a `build-ffmpeg` job here; that build lives in `ErsatzRS-ffmpeg`.
- Release archives contain the ErsatzRS binaries (and Windows launcher/site) only;
  no `ffmpeg/` directory.
- Container images may, at build time, fetch and pre-place the matching
  `ErsatzRS-ffmpeg` package at the managed runtime path so first run needs no
  network; document the pinned tag.

Expected archive shape:

```text
ersatzrs-vX.Y.Z-<rid>/
  ersatzrs-web[.exe]
  ersatzrs-scanner[.exe]
  ErsatzRS.exe                # Windows only
  site/                       # Windows only for launcher UI
  assets/
  LICENSE
  README.md
  # no ffmpeg/ — fetched into the data dir on first run
```

## Documentation Updates

- `RELEASING.md`: describe that archives no longer bundle FFmpeg, the startup
  download/update from `ErsatzRS-ffmpeg` releases, the resolution order, the
  managed runtime directory, and the offline/air-gapped install path.
- `CHANGELOG.md`: add an Unreleased entry once implementation starts (release
  artifacts no longer bundle FFmpeg; runtime is downloaded/updated from
  `ErsatzRS-ffmpeg`).

## Acceptance Criteria

- Release workflow no longer references BtbN/Gyan URLs or stages `ffmpeg/bin`.
- ErsatzRS release archives contain **no** `ffmpeg/ffprobe`.
- On a machine without system FFmpeg, a fresh ErsatzRS start downloads the matching
  `ErsatzRS-ffmpeg` release, validates it, and serves playback.
- Env override and a pre-placed managed runtime both bypass the download (offline
  path proven).
- `--enable-cairo`, `drawvg`, `--enable-libzimg`, `zscale`, and the
  `drawvg`/`zscale`+`tonemap` smoke transcodes are validated against the resolved
  runtime before it is accepted; a drawvg-only (zimg-less) runtime is rejected.
- `/system/health` shows resolved source, installed version, and update
  availability; publishing a newer `ErsatzRS-ffmpeg` release flips the indicator
  without re-releasing ErsatzRS.
- Checksum verification rejects a tampered/mismatched download.

## Suggested Implementation Order

1. Define/confirm the `ErsatzRS-ffmpeg` per-RID release asset + checksum contract.
2. Add the runtime resolver (env → managed → PATH) and capability gate with tests.
3. Add download + checksum-verify + managed-install for one RID (linux-x64) and
   prove a from-empty start.
4. Add the update check and the `/system/health` indicator + health check.
5. Extend asset selection to all six RIDs.
6. Remove FFmpeg bundling from `.github/workflows/release.yml`.
7. Pre-place the managed runtime in container images at build time.
8. Update release docs (`RELEASING.md`, `CHANGELOG.md`).
9. Run a throwaway tag release and a clean-machine first-run install.
