# Add-on runtime reference

The host launches an installed package only when it is enabled, its manifest
supports the requested capability, its host version and RID match, required
dependencies are ready, and its extracted content still matches the recorded
hash.

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
| `FFMPEG_BIN`, `FFPROBE_BIN` | Managed tool paths for generic operations when `managed_ffmpeg` is declared. |
| `ERSATZRS_REMOTE_STREAM_URL` | URL for `remote-stream.play.v1`. |
| `ERSATZRS_REMOTE_STREAM_SEEK` | Current source offset for play. |
| `ERSATZRS_REMOTE_STREAM_PLAYLIST_URL` | URL for `remote-stream.list.v1`. |

The Remote Stream producer integration currently supplies `FFMPEG_BIN` and an
available `FFPROBE_BIN` as playback environment values even when the package
does not declare `managed_ffmpeg`. This is an implementation gap, not an
operating-system permission boundary.

## Operations

The first positional argument is `check`, `list`, or `play`.
`addon.check.v1` has a 30-second limit and a 64 KiB output limit. It emits one
JSON object with `status` (`ready`, `warning`, or `unavailable`), a stable
lowercase `code`, and a safe message.

The list and play payloads, environment, output bounds, and NDJSON fields are
defined by the [Remote Stream playlist reference](remote-stream-playlist-manifests.md).
Unknown capability versions are rejected.

Processes run as the ErsatzRS operating-system user. Signing proves provenance
and integrity, not confinement, and v1 does not install a portable OS sandbox.

## Persistence and lifecycle

`AddonRepository`, `AddonInstallation`, and `AddonSetting` own repository,
active package, and configuration state. `RemoteStream.AddonId` is a restricting
foreign key to the active installation. Packages are installed in immutable
version/hash/RID paths; data and cache use separate per-add-on roots.

New installs and updates are disabled by default. Enable validates settings,
required dependency versions, RID and host compatibility, package integrity,
and readiness. An `unavailable` result blocks enablement. A package cannot be
disabled while an enabled package requires it, and uninstall is blocked by
installed required dependents or active Remote Stream references.

Uninstall removes package executables but retains persistent add-on data.
Removing a repository retains installed packages while clearing their stored
repository association. Changing a repository URL or public key clears its
catalog, refresh sequence, and refresh state.

There is no stable public add-on HTTP or event API in v1. Leptos server
functions under the admin route are internal UI adapters, not an add-on ingress
contract.
