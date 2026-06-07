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

When `DATABASE_URL` / `ErsatzRS_DATABASE__URL` is not set, ErsatzRS currently
defaults SQLite to `~/.ersatzrs/ErsatzRS.db` and creates the `~/.ersatzrs`
directory on startup.

## FFmpeg runtime

ErsatzRS checks both FFmpeg and ffprobe at web startup. Both tools must be
version 8.1 or newer and FFmpeg must expose the required capabilities described
by [ADR 0008](../adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md). In
particular FFmpeg must expose:

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

If `zscale` (or `drawvg`) is absent, rebuild FFmpeg with the corresponding flag
(`--enable-libzimg` for zscale, `--enable-cairo` for drawvg) or replace it with
a compliant FFmpeg 8.1 binary. On the local WSL/NVIDIA dev box,
`scripts/rebuild-ffmpeg-nvenc.sh` produces a compliant build (NVENC + drawvg +
zscale). When multiple FFmpeg installs exist, point ErsatzRS at the compliant
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
