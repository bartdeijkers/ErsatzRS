# Release bundle reference

This page describes the contents and runtime environment of published release
archives. Use [Install on Windows](../how-to/install-on-windows.md) or
[Install with Docker](../how-to/install-with-docker.md) for end-user setup.

## Published artifacts

Normal releases publish Linux x86-64, Linux AArch64, Linux ARMv7, and Windows
x86-64 archives plus a multi-architecture Docker image. Windows beta releases
may publish only the Windows archive. The active release workflow does not
currently publish macOS archives.

## Windows archive layout

The Windows archive is named `ersatzrs-<tag>-win-x64.zip` and contains one
matching top-level directory.

| Path | Purpose |
|---|---|
| `ErsatzRS.exe` | Windows tray launcher and user-facing entry point |
| `ersatzrs-web.exe` | Server for the admin UI and streaming endpoints |
| `ersatzrs-scanner.exe` | One-shot scanner binary |
| `ersatzrs-healthcheck.exe` | Readiness probe for `/healthz` |
| `site/` | Leptos hydration assets plus CSS, fonts, images, and other public assets |
| `assets/` | Graphics and template assets |
| `LICENSE` | Project license |
| `README.md` | Project overview |

Keep the complete extracted directory together and make it writable by the
user who runs ErsatzRS. `ErsatzRS.exe` starts the co-located server and points
it at the matching browser assets. After managed acquisition, the Windows
server copies its selected FFmpeg runtime files into this directory for stable
launcher resolution.

## Linux archive layout

Linux archives are named `ersatzrs-<tag>-<rid>.tar.gz` and contain one matching
top-level directory.

| Path | Purpose |
|---|---|
| `ersatzrs-web` | Server for the admin UI and streaming endpoints |
| `ersatzrs-scanner` | One-shot scanner binary |
| `ersatzrs-healthcheck` | Readiness probe for `/healthz` |
| `site/` | Leptos hydration assets plus CSS, fonts, images, and other public assets |
| `assets/` | Graphics and template assets |
| `LICENSE` | Project license |
| `README.md` | Project overview |

Run `ersatzrs-web` from the extracted top-level directory unless the service
configuration supplies explicit asset locations. Keep `site/` and `assets/`
from the same archive as the server binary.

## FFmpeg contents

Official archives and images do not include FFmpeg or ffprobe. On first startup,
ErsatzRS downloads and validates the matching first-party runtime under the
application state directory, then reuses or updates that managed copy.

See [Why ErsatzRS manages FFmpeg](../explanation/managed-ffmpeg-runtime.md) and
the [configuration reference](configuration.md) for offline overrides.

## Windows launcher environment

When `ErsatzRS.exe` starts `ersatzrs-web.exe`, it sets these variables when the
corresponding co-located files or directories exist:

| Variable | Value | Purpose |
|---|---|---|
| `ERSATZRS_FFMPEG__PATH` | `<archive>\ffmpeg.exe` | Selects a manually co-located FFmpeg binary; official archives do not ship it |
| `ERSATZRS_FFPROBE__PATH` | `<archive>\ffprobe.exe` | Selects a manually co-located ffprobe binary; official archives do not ship it |
| `LEPTOS_SITE_ROOT` | `<archive>\site` | Selects the bundled Leptos site output |
| `LEPTOS_SITE_PKG_DIR` | `pkg` | Selects the hydration package directory |
| `LEPTOS_OUTPUT_NAME` | `ersatzrs-web` | Selects the JavaScript and WASM basename |

## Runtime state

The extracted archive primarily contains replaceable release assets. Durable
user data belongs under the application state directories. Windows may also
hold replaceable, co-located FFmpeg runtime files in the release directory.

| Platform | Durable state |
|---|---|
| Native Linux | `$HOME/.ersatzrs` |
| Windows | `%USERPROFILE%\.ersatzrs` |
| Docker | `/data`, backed by the configured volume |

Logs and caches use the platform local application data directory. They are
separate from the default SQLite state directory on native installations.

## Docker image layout

The Docker image runs `/usr/local/bin/ersatzrs-web` as a non-root service user,
publishes port `8409`, and declares `/data` as persistent storage. It includes
the web assets and `ersatzrs-healthcheck`, but acquires FFmpeg through the same
managed runtime path as native archives.

The image defaults to:

```text
HOME=/data
DATABASE_URL=sqlite:///data/.ersatzrs/ErsatzRS.db
LEPTOS_SITE_ROOT=/usr/local/bin/site
LEPTOS_SITE_PKG_DIR=pkg
LEPTOS_OUTPUT_NAME=ersatzrs-web
```

## Artifact consistency

The native server and hydration bundle must come from the same release. Mixing
an old server with a new `site/` directory, or the reverse, can render static
HTML while browser actions fail. Official release builds verify the assembled
bundle before publishing it.
