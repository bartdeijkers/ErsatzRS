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
