# Admin UI design artifacts

Retained design references and synthetic prototypes support comparisons with
production. They do not establish product acceptance. Current results and
historical review outcomes belong to `docs/progress/milestones.toml` and the
owning parity TOMLs. The milestone and ADR determine which reference is active.

- [Locked add-on workspace](admin-ux-organize/LOCK.md)
- [Locked add-on Sources v0.1](addon-sources/LOCK.md) — accepted OAD reference; first run, ZIP installation, state review and light theme. Use the [review guide](addon-sources/README.md) for new revisions before post-jukebox implementation.
- [Workspace design reference](admin-ux-organize/design.md)
- [Production source and capability map](admin-ux-organize/production-source-map.md)
- [Other layout artifacts](artifacts/README.md)
- [Captured upstream playout structure](upstream-snapshots/playouts-1280x800-snapshot.md)
- [Captured upstream schedule structure](upstream-snapshots/schedules-snapshot-2026-05-25.md)

The add-on workspace lock preserves its original 139 files byte-for-byte.
Historical paths in pinned source comments remain part of that frozen baseline;
current verification and preview commands are documented in LOCK.md and design.md.
Run browser scripts only on disposable copies because they overwrite reports
and screenshots. Prototype reports prove synthetic behavior only.
