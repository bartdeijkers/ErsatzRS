# Configuration Reference

NasTv configuration will be layered through `figment` or an equivalent
structured configuration crate.

## Layer Order

Later layers override earlier layers:

1. Built-in defaults.
2. `config.toml`.
3. Environment variables.
4. CLI flags.

## Environment Prefix

Use the `NASTV_` prefix for environment variables.

Examples:

```text
NASTV_DATABASE__URL=sqlite://nastv.db
NASTV_HTTP__BIND=0.0.0.0:8080
NASTV_FFMPEG__PATH=/usr/local/bin/ffmpeg
```

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
