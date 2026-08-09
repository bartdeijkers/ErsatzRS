# Logging reference

ErsatzRS uses `tracing` for runtime diagnostics. Operators configure levels in
**Settings** -> **Logging** or with the base `RUST_LOG` environment filter.

## Persisted settings

Runtime binaries load ErsatzTV-compatible `ConfigElement` levels from the
database.

| Key | Purpose |
|---|---|
| `log.minimum_level` | Default minimum level |
| `log.minimum_level.scanning` | Scanner, ffprobe, and provider imports |
| `log.minimum_level.scheduling` | Scheduling and maintenance |
| `log.minimum_level.searching` | Search parsing and admin search |
| `log.minimum_level.streaming` | FFmpeg, HLS workers, and IPTV transport |
| `log.minimum_level.http` | Axum and HTTP middleware |

Accepted level names include `trace`/`verbose`, `debug`, `info`/`information`,
`warn`/`warning`, `error`/`fatal`/`critical`, and `off`. Persisted directives are
appended after `RUST_LOG`.

## Default log file

The default file is `ErsatzRS.log` in the platform local application data
`ErsatzRS/logs` directory. The Windows launcher's **Show Logs** action opens
that directory. The **Support** -> **Logging** page displays the active log in the
admin UI.

## Remote Stream producer failures

A failed script-backed Remote Stream records one structured warning with these
safe correlation fields:

| Field | Meaning |
|---|---|
| `surface` | `hls`, `mpeg-ts`, or `probe` |
| `channel_number` | The playback channel, or `n/a` for a probe |
| `producer` | Executable file name only, never its full path |
| `seek` | Effective source-side seek, or `not-configured` |
| `failure_kind` | `producer-exit` or `producer-wait` |
| `status` / `error` | Process exit status or wait error |
| `stderr_bytes` | Size of the captured rolling stderr tail before sanitation |
| `stderr_excerpt` | Sanitized tail, bounded to 2 KiB |

Before the excerpt reaches the logger, ErsatzRS removes URL paths and query
parameters, authentication and cookie values, common token assignments, control
characters, and local filesystem paths. The excerpt is intended to retain the
extractor's error class, not its complete command or response. Cancellation and
a consumer closing a healthy producer pipe remain debug-level lifecycle events.
