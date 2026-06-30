# Configuration Reference

ErsatzRS configuration will be layered through `figment` or an equivalent
structured configuration crate.

## Layer Order

Later layers override earlier layers:

1. Built-in defaults.
2. `config.toml`.
3. Environment variables.
4. CLI flags.

## Environment Prefix

Use the `ErsatzRS_` prefix for environment variables.

Examples:

```text
ErsatzRS_DATABASE__URL=sqlite:$HOME/.ersatzrs/ErsatzRS.db
ErsatzRS_HTTP__BIND=0.0.0.0:8080
ErsatzRS_FFMPEG__PATH=/usr/local/bin/ffmpeg
ErsatzRS_FFPROBE__PATH=/usr/local/bin/ffprobe
```

Startup database URL resolution now goes through the startup configuration
helper. `ErsatzRS_DATABASE__URL` is preferred, `ERSATZRS_DATABASE__URL` is
accepted for shell/orchestrator environments, and legacy `DATABASE_URL` remains
accepted so Docker keeps using `DATABASE_URL=sqlite:///data/.ersatzrs/ErsatzRS.db`.
When none of those is set, ErsatzRS defaults SQLite to
`~/.ersatzrs/ErsatzRS.db` and creates the `~/.ersatzrs` directory on startup.

## FFmpeg runtime

ErsatzRS checks both FFmpeg and ffprobe at web startup. Both tools must be
version 8.1 or newer and FFmpeg must expose the required capabilities described
by [ADR 0008](../adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md). The
normal path is the managed runtime under the ErsatzRS state directory
(`~/.ersatzrs/ffmpeg` by default). Startup downloads the matching
[`ErsatzRS-ffmpeg`](https://github.com/bartdeijkers/ErsatzRS-ffmpeg) release
when no managed runtime exists, and updates a managed runtime when a newer
release is available. Release archives and Docker images do not bundle
FFmpeg/ffprobe; both follow the same managed-runtime model.

The startup resolution order is:

1. Explicit path override from environment or persisted config.
2. Managed runtime under the state directory, auto-updated at startup.
3. Managed download from the latest or pinned `ErsatzRS-ffmpeg` release.
4. `PATH` fallback when managed download is disabled or fails.

In particular FFmpeg must expose:

- `drawvg` (built with `--enable-cairo`) for the graphics engine.
- `zscale` (built with `--enable-libzimg`) for the HDR/tonemap color pipeline.

Startup fails fast when the selected FFmpeg binary is missing a required
capability, for example:

```text
FFmpeg runtime is missing required capabilities: [RequiredFilter { name: "zscale" }]
```

This is a runtime packaging gap, not an application bug. Diagnose the configured
binary with:

```bash
ffmpeg -hide_banner -buildconf | grep -- --enable-libzimg
ffmpeg -hide_banner -filters | grep -w zscale
ffmpeg -hide_banner -buildconf | grep -- --enable-cairo
ffmpeg -hide_banner -filters | grep -w drawvg
```

If `zscale` (or `drawvg`) is absent, replace it with a compliant FFmpeg 8.1
binary or let the managed runtime download the first-party ErsatzRS-ffmpeg
package. When multiple FFmpeg installs exist, point ErsatzRS at the compliant
one with the path override variables below rather than weakening the runtime
check.

Accepted path override variables:

```text
ErsatzRS_FFMPEG__PATH=/opt/ffmpeg/bin/ffmpeg
ErsatzRS_FFPROBE__PATH=/opt/ffmpeg/bin/ffprobe
ERSATZRS_FFMPEG__PATH=/opt/ffmpeg/bin/ffmpeg
ERSATZRS_FFPROBE__PATH=/opt/ffmpeg/bin/ffprobe
FFMPEG_PATH=/opt/ffmpeg/bin/ffmpeg
FFPROBE_PATH=/opt/ffmpeg/bin/ffprobe
```

`ERSATZRS_FFPROBE` remains accepted for older scanner test workflows.

Managed runtime controls:

```text
ERSATZRS_FFMPEG_RELEASE_TAG=v1.0.1
ERSATZRS_FFMPEG_DOWNLOAD=0
```

`ERSATZRS_FFMPEG_RELEASE_TAG` pins managed acquisition/update to a specific
`ErsatzRS-ffmpeg` release tag. Without it, startup uses the latest published
release and falls back to the built-in known-good tag when the latest lookup is
unavailable during first install. `ERSATZRS_FFMPEG_DOWNLOAD=0` disables managed
downloads; use it only when an explicit override, a pre-placed managed runtime,
or a compliant `PATH` runtime is available.

Managed download requires a release asset for the current RID. If the selected
`ErsatzRS-ffmpeg` release does not publish that asset, startup falls back to an
explicit override, a pre-placed managed runtime, or `PATH` when one is usable;
otherwise startup fails with the missing asset error.

## Trakt

Trakt list add/refresh operations require a Trakt API client id. Keep the
actual value in the process environment or a deployment secret manager.

Accepted environment variable names:

```text
TRAKT__CLIENTID=<client-id>
ErsatzRS_TRAKT__CLIENTID=<client-id>
```

`TRAKT__CLIENTID` matches ErsatzTV's documented configuration shape. Existing
Trakt list match, settings update, generated playlist cleanup, and delete
operations are DB-only and do not require this value.

## Secrets

Secrets belong in environment variables or a deployment secret manager, not in
committed files.

Treat these as secrets:

- Database passwords.
- Plex, Jellyfin, Emby, and Trakt tokens.
- Session keys.
- API keys.
- Cookie signing material.

## Logging

Configuration debug output must redact secrets before logging.
