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
| `ERSATZRS_REMOTE_STREAM_URL` | URL for `remote-stream.play.v1`. |
| `ERSATZRS_REMOTE_STREAM_SEEK` | Current source offset for play. |
| `ERSATZRS_REMOTE_STREAM_PLAYLIST_URL` | URL for `remote-stream.list.v1`. |
| `ERSATZRS_MEDIA_LIST_URL` | Source hyperlink for `media-list.list.v1`. |

## Operations

The first positional argument is `check`, `list`, or `play`.
`addon.check.v1` has a 30-second limit and a 64 KiB output limit. It emits one
JSON object with `status` (`ready`, `warning`, or `unavailable`), a stable
lowercase `code`, and a safe message.

The list and play payloads, environment, output bounds, and NDJSON fields are
defined by the [Remote Stream playlist reference](remote-stream-playlist-manifests.md).
Unknown capability versions are rejected.

The provider-neutral list payload is defined by the
[add-on media-list reference](addon-media-lists.md).

Every invocation runs in an owned Unix process group or Windows Job Object.
Timeout, cancellation, output-drain failure, and invocation-owner teardown
terminate every member that remains in that managed group or job. Control,
list, and check output plus diagnostic stderr are byte- and time-bounded.
`remote-stream.play.v1` stdout is instead a session-owned media stream; session
cancellation owns its lifetime.

Processes still run as the ErsatzRS operating-system user. Signing proves
provenance and integrity, not confinement. The small startup allowlist still
includes `PATH`, and v1 does not install a portable OS sandbox.

## Persistence and lifecycle

`AddonRepository`, `AddonInstallation`, and `AddonSetting` own repository,
active package, and configuration state. `RemoteStream.AddonId` is a restricting
foreign key to the active installation. Packages are installed in immutable
version/hash/RID paths and their content hash is revalidated before each
invocation. On Unix, extraction normalizes directories to `0755` and files to
`0444` or `0555`, stripping group/world write access. Data and cache use
separate repository-scoped per-add-on roots, so an add-on with the same ID from
a different configured repository ID cannot inherit retained private state.

New installs and updates are disabled by default. Enable validates settings,
required dependency versions, RID and host compatibility, package integrity,
and readiness. An `unavailable` result blocks enablement. A package cannot be
disabled while an enabled package requires it, and uninstall is blocked by
installed required dependents, active Remote Stream references, or owned
add-on media lists through the normal administration flow.

Forced uninstall detaches Remote Stream references and removes owned media-list
snapshots plus their generated system playlists. Uninstall removes package
executables and disposable cache but retains the installation's
repository-scoped persistent add-on data.
Removing a repository retains installed packages while clearing their stored
repository association; their stable data namespace remains retained. Changing
a repository URL or public key clears its catalog, refresh sequence,
validators, and refresh state.

Environment-backed secret values and operation URLs are exactly redacted from
add-on-controlled output. When a file-backed secret is configured, ErsatzRS
does not read that file and therefore suppresses add-on-controlled diagnostic
text instead of risking disclosure of a value the add-on read itself.

There is no stable public add-on HTTP or event API in v1. Leptos server
functions under the admin route are internal UI adapters, not an add-on ingress
contract.
