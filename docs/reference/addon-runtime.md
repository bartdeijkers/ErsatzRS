# Add-on runtime reference

Normal capability, list, and play invocations require an enabled package whose
manifest supports the requested capability, whose host version and RID match,
whose required dependencies are ready, and whose extracted content still
matches the recorded hash. `addon.check.v1` is also allowed while disabled so
readiness can be established before enablement.

## Process environment

The inherited environment is cleared. ErsatzRS retains only a small
platform-specific startup allowlist such as `PATH`, locale and temporary
directory variables, and corresponding Windows system variables. Host and
setting values use these names:

| Variable | Meaning |
|---|---|
| `ERSATZRS_ADDON_ID`, `ERSATZRS_ADDON_VERSION` | Active package identity. |
| `ERSATZRS_ADDON_DATA_DIR`, `ERSATZRS_ADDON_CACHE_DIR` | Per-add-on writable roots. |
| `ERSATZRS_ADDON_SETTING_<KEY>` | Resolved non-secret setting. |
| `ERSATZRS_ADDON_SECRET_<KEY>` | Secret resolved from a named environment reference. |
| `ERSATZRS_ADDON_SECRET_FILE_<KEY>` | Configured secret-file path. |
| `FFMPEG_BIN`, `FFPROBE_BIN` | Managed tool paths when `managed_ffmpeg` is declared. |
| `ERSATZRS_REMOTE_STREAM_URL` | URL for `remote-stream.play.v1` or v2. |
| `ERSATZRS_REMOTE_STREAM_SEEK` | Current source offset for play. |
| `ERSATZRS_REMOTE_STREAM_MAX_VIDEO_HEIGHT` | Host-selected maximum video height for `remote-stream.play.v2`; decimal pixels from 144 through 4320. |
| `ERSATZRS_REMOTE_STREAM_PLAYLIST_URL` | URL for Remote Stream list v1 or v2. |
| `ERSATZRS_MEDIA_LIST_URL` | Source hyperlink for media-list list v1 or v2. |

## Operations

The first positional argument selects the versioned capability's operation,
including `check`, `list`, `play`, or the permission-reviewed `update`.
`addon.check.v1` has a 30-second limit and a 64 KiB output limit. It emits one
JSON object with `status` (`ready`, `warning`, or `unavailable`), a stable
lowercase `code`, and a safe message.

The list and play payloads, environment, output bounds, and NDJSON fields are
defined by the [Remote Stream playlist reference](remote-stream-playlist-manifests.md).
Unknown capability versions are rejected.

`media-list.options.v1` is interpreted by the host as a declarative schema and
cannot be invoked as an executable operation. Draft list defaults and sparse
item overrides are stored independently from the currently published option
snapshot. Effective values resolve from item override to list default to the
frozen schema default. Existing import-v1 and Remote Stream play-v1/v2
requests remain unchanged; later operation versions must opt into carrying
published effective values explicitly.

The provider-neutral list payload is defined by the
[add-on media-list reference](addon-media-lists.md).

### Native-tool self-update

`tool.update.v1` invokes `update` with one JSON request on stdin:

```json
{"schema":"tool.update.v1","tool_key":"YT_DLP_BIN"}
```

The request is limited to 1 KiB and must name an approved executable setting.
The result is one JSON object limited to 2 KiB with `schema`, `tool_key`, nullable
`before_version` and `after_version`, `outcome`, and nullable `diagnostic_code`.
Outcomes are `updated`, `already_current`, `manual_update_required`, or `failed`.
Diagnostics are `unsupported_installation`, `update_failed`,
`version_probe_failed`, or `tool_unavailable`; subprocess prose is not forwarded.

The official yt-dlp package runs configured native `yt-dlp -U` between bounded
version probes. The host does not download replacement bytes, run a package
manager, or force a release channel. The total invocation deadline is 90 seconds.
Normal operations retain `--no-update`.

Startup and daily triggers coalesce in the existing durable maintenance queue.
Daily work becomes eligible 24 hours after the previous completed attempt.
An update acquires exclusive access only when its executable is idle; busy work
is deferred without stopping playback or making new readers wait for a pending
writer. Shared use covers discovery, readiness, item operations, preparation,
and streaming. Ownership lasts through confirmed process-tree cleanup.
Latest status appears in Add-ons, with actionable failures also shown in Health.
Replacement rollback and the manual update/history interface belong to AMM-N-02.

### Offline tool cleanup recovery

The packaged server supports one explicit offline action before starting HTTP,
background workers, the database runtime, or FFmpeg:

```text
ersatzrs-web --recover-addon-tool --tool-path <absolute-executable-path> --confirm-all-processes-stopped
```

On Windows the executable is `ersatzrs-web.exe`, not the tray launcher
`ErsatzRS.exe`. An optional `--state-dir <absolute-profile-directory>` selects
the profile whose markers are being recovered; without it, the normal
`ERSATZRS_APP_STATE_DIR`/default state root applies.

| Condition | Behavior |
|---|---|
| Missing confirmation or invalid arguments | Refuses recovery without starting the server. |
| A related process still holds a tool lease | Refuses recovery. |
| Valid owned markers for the selected executable | Retires only those markers under an exclusive lease. |
| No markers remain | Successful no-op. |
| Executable temporarily absent after replacement | Resolves the identity from its existing canonical parent and filename. |
| Unsafe or unexpected marker storage | Refuses recovery without deleting unrelated entries. |

The confirmation asserts that the server, scanner, and all related provider and
tool processes have stopped. Exclusive lock acquisition alone does not prove
that orphaned processes stopped. A machine reboot with automatic server startup
disabled can establish the offline condition when process cleanup is uncertain.
Recovery does not repair or replace the executable, delete media, remove job
history, or clear another tool's markers. Restarting the server without recovery
does not clear uncertain-cleanup markers.

### Operation diagnostics

A failed `list` or `play` operation exits non-zero and writes one diagnostic
JSON object as the final non-empty stderr line:

```json
{"code":"provider-unreachable","message":"The provider request failed."}
```

`code` is 1–128 lowercase ASCII letters, digits, hyphens, underscores, or
periods. `message` is diagnostic detail limited to 1,024 UTF-8 bytes. Earlier
stderr lines may contain bounded process diagnostics, but only the final line
is interpreted as the operation diagnostic. Stdout remains reserved for list
NDJSON or play media bytes.

The host recognizes these provider-neutral codes:

| Code | Condition |
|---|---|
| `missing-setting` | A required non-secret setting is absent. |
| `missing-secret` | A required secret reference is absent or unavailable. |
| `provider-unreachable` | The add-on could not reach its external provider. |
| `provider-rejected` | The provider rejected the request or response contract. |
| `provider-rate-limited` | The provider temporarily limited requests. |
| `not-found` | The requested provider item does not exist. |
| `unsupported-url` | The supplied URL is not supported by the add-on. |

Other valid codes are allowed. The admin UI renders a generic localized
failure line for an unknown code and appends the bounded add-on message. A
missing or invalid final JSON object is treated as `operation-failed` for
compatibility with older packages.

Every invocation runs in an owned Unix process group or Windows Job Object.
Timeout, cancellation, output-drain failure, and invocation-owner teardown
terminate every member that remains in that managed group or job. Control,
list, and check output plus diagnostic stderr are byte- and time-bounded.
`remote-stream.play.v1` and v2 stdout are instead session-owned media streams;
session cancellation owns their lifetime. Version 2 must honor the supplied
maximum video height without treating it as an exact required rendition.

Processes still run as the ErsatzRS operating-system user. Signing proves
provenance and integrity, not confinement. The small startup allowlist still
includes `PATH`, and v1 does not install a portable OS sandbox.

## Persistence and lifecycle

`AddonSource`, `AddonSourceOffer`, `AddonInstallation`, and `AddonSetting` own
local source, active package, and configuration state. `RemoteStream.AddonId` is a restricting
foreign key to the active installation. Packages are installed in immutable
version/hash/RID paths and their content hash is revalidated before each
invocation. On Unix, extraction normalizes directories to `0755` and files to
`0444` or `0555`, stripping group/world write access. Data and cache use
separate source-scoped per-add-on roots, so an add-on with the same ID from a
different local source cannot inherit retained private state. Explicit legacy
adoption preserves the historical namespace instead of silently switching it.

New installs and updates are disabled by default. Enable validates settings,
required dependency versions, RID and host compatibility, package integrity,
and readiness. An `unavailable` result blocks enablement. A package cannot be
disabled while an enabled package requires it, and uninstall is blocked by
installed required dependents, active Remote Stream references, or owned
add-on media lists through the normal administration flow.

Forced uninstall detaches Remote Stream references and removes owned media-list
snapshots plus their generated system playlists. Uninstall removes package
executables and disposable cache but retains the installation's
source-scoped persistent add-on data. Removing a checkout withdraws its offers
and records the missing source while retaining installed immutable packages and
their stable data namespace. Replacing an offline bundle atomically changes
only its validated offers; running packages change only after an explicit
per-add-on update.

Environment-backed secret values and operation URLs are exactly redacted from
add-on-controlled output. When a file-backed secret is configured, ErsatzRS
does not read that file and therefore suppresses add-on-controlled diagnostic
text instead of risking disclosure of a value the add-on read itself.

There is no stable public add-on HTTP or event API in v1. Leptos server
functions under the admin route are internal UI adapters, not an add-on ingress
contract.
