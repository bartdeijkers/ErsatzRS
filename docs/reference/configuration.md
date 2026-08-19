# Configuration reference

ErsatzRS has two configuration boundaries:

- startup settings that must be known before the database opens come from
  environment variables; and
- day-to-day media server settings are persisted in the database and edited
  through the admin **Settings** pages.

There is currently no general `config.toml` or CLI-flag layer. Do not rely on
the aspirational configuration examples found in older releases.

## Startup database URL

The first non-empty variable wins:

| Priority | Variable | Notes |
|---:|---|---|
| 1 | `ErsatzRS_DATABASE__URL` | Preferred repository spelling |
| 2 | `ERSATZRS_DATABASE__URL` | Uppercase shell and orchestrator spelling |
| 3 | `DATABASE_URL` | Compatibility spelling used by Docker and SQLx tooling |

When none is set, ErsatzRS creates and opens:

```text
$HOME/.ersatzrs/ErsatzRS.db
```

The Docker image sets:

```text
DATABASE_URL=sqlite:///data/.ersatzrs/ErsatzRS.db
```

SQLite connections always enable foreign keys, use a 30-second busy timeout,
select WAL journaling, and use NORMAL synchronization. WAL mode persists in the
database file; the connection-local settings are reapplied to pooled and lazy
connections.

MySQL and MariaDB URLs require an application binary built with the `mysql`
feature:

```text
DATABASE_URL=mysql://user:password@database-host/ersatzrs
```

Treat URLs containing credentials as secrets.

## Server listen address

Leptos provides the native server listen setting:

| Variable | Default | Purpose |
|---|---|---|
| `LEPTOS_SITE_ADDR` | `0.0.0.0:8409` | Admin, API, IPTV, XMLTV, and HDHomeRun HTTP listener |

For Docker, keep the container address on `0.0.0.0` and change the host-side
port publication when needed, for example `--publish 8410:8409`.

## Development-only routes

Normal builds do not register the component gallery at `/dev/components` or
the Big Buck Bunny sample stream at `/dev/stream/bbb.ts`. Local development
requires both gates:

1. compile `ersatzrs-web` with the non-default `development-routes` feature;
2. set `ERSATZRS_ENABLE_DEVELOPMENT_ROUTES=true` at runtime.

For example:

```bash
ERSATZRS_ENABLE_DEVELOPMENT_ROUTES=true \
  cargo run -p ersatzrs-web --features development-routes --bin ersatzrs-web
```

The opt-in accepts `1`, `true`, `yes`, or `on`, case-insensitively. Do not
expose a development-route build to untrusted clients: the sample-stream route
launches FFmpeg and exists only for local pipeline verification.

## Browser asset settings

Official archives and images set or normalize these values. Operators normally
should not override them.

| Variable | Release value | Purpose |
|---|---|---|
| `LEPTOS_SITE_ROOT` | archive/image `site` directory | Root containing the hydration package |
| `LEPTOS_SITE_PKG_DIR` | `pkg` | Package directory below the site root |
| `LEPTOS_OUTPUT_NAME` | `ersatzrs-web` | JavaScript and WASM basename |

The native server binary and browser assets must come from the same release.
Mismatched values or mixed artifacts can render HTML while leaving buttons and
dialogs unresponsive.

## FFmpeg and ffprobe

ErsatzRS validates both tools at web startup. They must be on the supported
FFmpeg 9.x runtime line and expose the capabilities listed in the
[system requirements](system-requirements.md), including `drawvg` and `zscale`.

Runtime resolution order:

1. explicit environment or persisted path;
2. an existing managed runtime under `$HOME/.ersatzrs/ffmpeg`;
3. a managed download from `bartdeijkers/ErsatzRS-ffmpeg`;
4. a compatible `PATH` pair when managed acquisition cannot be used.

### Path overrides

The first available value in each tool's resolution chain is used. Set both
paths to keep FFmpeg and ffprobe on the same build.

| Tool | Accepted variables |
|---|---|
| FFmpeg | `ErsatzRS_FFMPEG__PATH`, `ERSATZRS_FFMPEG__PATH`, `FFMPEG_PATH` |
| ffprobe | `ErsatzRS_FFPROBE__PATH`, `ERSATZRS_FFPROBE__PATH`, `FFPROBE_PATH` |

`ERSATZRS_FFPROBE` remains accepted by older scanner test workflows but is not
the preferred server setting.

Example:

```text
ERSATZRS_FFMPEG__PATH=/opt/ffmpeg/bin/ffmpeg
ERSATZRS_FFPROBE__PATH=/opt/ffmpeg/bin/ffprobe
```

### Managed runtime controls

| Variable | Default | Purpose |
|---|---|---|
| `ERSATZRS_FFMPEG_RELEASE_TAG` | latest compatible release, with a built-in known-good fallback | Pin acquisition and update checks to one runtime tag |
| `ERSATZRS_FFMPEG_DOWNLOAD` | enabled | Set to `0` to disable managed downloads |

Disable downloads only when an explicit, pre-placed, or `PATH` runtime already
satisfies startup validation.

## Logging

| Variable | Default | Purpose |
|---|---|---|
| `RUST_LOG` | `info` | Base `tracing` filter |

The admin **Settings** -> **Logging** page persists the default minimum level
and per-subsystem overrides for scanning, scheduling, searching, streaming, and
HTTP. Persisted directives are applied after the base `RUST_LOG` filter and can
be reloaded without replacing application files. Exact keys, accepted levels,
and file locations are in the [logging reference](logging.md).

## IPTV JWT protection

| Variable | Purpose |
|---|---|
| `JWT__IssuerSigningKey` | Enables HS256 JWT validation for IPTV routes |
| `JWT:IssuerSigningKey` | Compatibility spelling where the environment supports colons |

When neither variable contains a non-empty value, IPTV routes are open. When a
key is configured, clients must supply a valid HS256 JWT with an `exp` claim as
a bearer token or an `access_token` query value.

Treat the signing key and generated tokens as secrets. See the
[HTTP endpoint reference](http-endpoints.md) for request forms.

## Provider add-on credentials

ErsatzRS has no process-wide Trakt configuration. Provider credentials are
declared by installed add-ons and configured as environment-variable or file
references in **Settings > Add-ons**. The database stores only the reference
kind and reference name/path. See the [add-on package setting
contract](addon-packages.md#settings-and-panels) and [runtime environment
reference](addon-runtime.md#process-environment).

## Healthcheck binary

Release bundles contain `ersatzrs-healthcheck` (or
`ersatzrs-healthcheck.exe`). It checks the server readiness URL.

| Variable | Default | Purpose |
|---|---|---|
| `ERSATZRS_HEALTHCHECK_URL` | `http://127.0.0.1:8409/healthz` | Override the readiness target |

The Docker image uses this binary for its container health check.

## Trusted add-on repositories

New databases seed an official ErsatzRS add-on source row. In the current
development tree its public key is intentionally empty, so it cannot refresh
until a release pins the production key. To replace the complete list for an
immutable deployment, set:

| Variable | Purpose |
|---|---|
| `ERSATZRS_ADDONS__TRUSTED_REPOSITORIES_FILE` | Filesystem path to the authoritative repository allowlist TOML file |

While this variable is set, database-only sources are not trusted and the
repository editor is read-only. The exact file schema, path behavior, and
replacement semantics are in the [add-on repository reference](addon-repositories.md).

## Database-backed settings pages

Use the admin UI for settings that can be loaded after database startup:

| Page | Examples |
|---|---|
| **Settings** -> **FFmpeg** | runtime paths, segmenter behavior, subtitle extraction, transcode limits |
| **Settings** -> **Logging** | global and subsystem log levels |
| **Settings** -> **HDHomeRun** | tuner and discovery settings |
| **Settings** -> **Scanner** | library refresh behavior |
| **Settings** -> **Playout** | playout build horizon and scheduling behavior |
| **Settings** -> **UI** | language, theme, and per-user Windows login startup |
| **Settings** -> **XMLTV** | guide days, time zone, and block behavior |
| **Settings** -> **Add-ons** | trusted repositories, install/update state, readiness, manifest-declared settings, and the default remote stream processor |

These values live in the database and move with a database backup.

The Windows startup checkbox is the exception: it reflects
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` on the current machine and
is not persisted in the database. Linux automatic startup is configured by an
administrator through a service manager such as systemd.

## Secret handling

Treat these values as secrets:

- database passwords;
- Plex, Jellyfin, and Emby tokens;
- JWT signing keys and access tokens; and
- add-on values resolved through environment or secret-file references;
- reverse-proxy or session credentials.

Do not commit them, paste them into issue reports, or include them in
screenshots. The troubleshooting export redacts recognized secret-bearing
configuration, but review diagnostics before sharing them.
