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
ErsatzRS_DATABASE__URL=sqlite://ErsatzRS.db
ErsatzRS_HTTP__BIND=0.0.0.0:8080
ErsatzRS_FFMPEG__PATH=/usr/local/bin/ffmpeg
```

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
