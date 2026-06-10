# Release Bundle Reference

This page describes the contents and runtime environment of release archives.
Use [Build and publish a local Windows release](../how-to/release-locally.md)
for the local publishing procedure.

## Windows archive layout

The Windows archive is named `ersatzrs-<tag>-win-x64.zip` and contains one top
level folder named `ersatzrs-<tag>-win-x64/`.

| Path | Source | Notes |
|---|---|---|
| `ErsatzRS.exe` | `ersatzrs-launcher.exe` | Windows tray launcher and user-facing entry point. |
| `ersatzrs-web.exe` | `ersatzrs-web` release binary | Server for the admin UI, IPTV endpoints, and HDHomeRun endpoints. |
| `ersatzrs-scanner.exe` | `ersatzrs-scanner` release binary | One-shot scanner binary. |
| `ffmpeg.exe` | Bundled FFmpeg 8.1+ Windows build | Must expose the `drawvg` and `zscale` filters required by startup checks. |
| `ffprobe.exe` | Same FFmpeg bundle | Used by runtime media probing and startup checks. |
| `site/` | `cargo leptos build --release --frontend-only --lib-features hydrate` output | Leptos hydration assets. Must include `site/pkg/`. |
| `public/` | `crates/ersatzrs-web/public` | Static public assets, including the generated CSS bundle. |
| `assets/` | Repository `assets/` | Runtime graphics and template assets. |
| `LICENSE` | Repository root | Project license. |
| `README.md` | Repository root | Project overview. |

The GitHub Actions workflow stages the same layout in
`.github/workflows/release.yml`.

See [Configuration Reference](configuration.md) for the full FFmpeg runtime
capability contract.

## Launcher environment

When `ErsatzRS.exe` starts `ersatzrs-web.exe`, it sets these environment
variables when the corresponding bundled files or directories exist:

| Variable | Value | Purpose |
|---|---|---|
| `ERSATZRS_FFMPEG__PATH` | `<archive>\ffmpeg.exe` | Selects the bundled FFmpeg binary. |
| `ERSATZRS_FFPROBE__PATH` | `<archive>\ffprobe.exe` | Selects the bundled ffprobe binary. |
| `LEPTOS_SITE_ROOT` | `<archive>\site` | Selects the bundled Leptos site output. |
| `LEPTOS_SITE_PKG_DIR` | `pkg` | Selects the hydration package directory under `site/`. |
| `LEPTOS_OUTPUT_NAME` | `ersatzrs-web` | Selects the hydration JS/WASM basename. |

`LEPTOS_OUTPUT_NAME` is set only when the parent process did not already define
it. The release server binary is built with plain `cargo build`, so the launcher
must provide this value for the hydration scripts to be emitted reliably.

## Runtime state

The archive folder is for immutable release assets. User data and mutable
runtime files belong under the normal application state directory, not next to
the extracted zip contents. On Windows this keeps logs, the default SQLite
database, and mutable data under the user's local application data paths instead
of the release extraction folder.
