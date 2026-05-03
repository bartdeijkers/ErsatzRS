# Porting the admin UI from ErsatzTV to ErsatzRS

The ErsatzTV UX/UI (while not perfect) is very useful and should be ported "as
is" to ErsatzRS. This document specifies the work that still needs to be done
to make ErsatzRS behave exactly the same as, or better than, ErsatzTV.

## UX/UI TODOs

### Slice 1 - closed 2026-05-03
- [x] Channel and watermarks pages now match the ErsatzTV list-page structure
  more closely: top action bar, constrained content area, divider, generated
  channel-logo image, disabled/hidden channel number markers, and icon row
  actions.
- [x] Add local library works for existing filesystem paths such as
  `/mnt/d/downloads/movies`: paths are normalized before validation,
  `file://` input is accepted, duplicate normalized paths collapse, and
  create/update writes are atomic.
- [x] Edit FFmpeg profile keeps saved hardware/profile settings visible by
  preserving the current stored hardware acceleration option even when it is
  not available in the current runtime capability report.
- [x] FFmpeg profile hardware acceleration options are populated from the
  startup report using both hardware accelerators and hardware encoder names,
  so encoder-backed NVENC/VAAPI/QSV-style options are selectable when exposed
  by the bundled FFmpeg. Vulkan remains a runtime/image capability used by
  FFmpeg filters and packaging, not an ErsatzTV `HardwareAccelerationKind`
  profile enum value.


### Slice 2

- [x] Library 'Scan' button now shows an indeterminate spinner while a scan
  is running, mirroring ErsatzTV's `IEntityLocker`-driven spinner. Server
  side, scans register the library id with `EntityLocker` for the lifetime
  of the spawned task; the libraries page polls `list_locked_libraries`
  every 1.5 s and swaps the row's action buttons for a spinning indicator
  while the lock is held.
- [x] Adding a Jellyfin API key now mirrors ErsatzTV's
  `SaveJellyfinSecretsHandler`: the connection is validated against
  `/System/Info` before any rows are written, the real `ServerName` and
  `OperatingSystem` are stored on `JellyfinMediaSource`, and a library list
  refresh + upsert (via `sync_jellyfin_libraries`) is triggered immediately
  on save so the libraries page is populated without a manual refresh.