# ADR 0059: Platform-owned automatic startup

## Status

Accepted (2026-08-02)

## Implementation

Implemented (2026-08-02). Windows release users can opt in from **Settings** ->
**UI**. ErsatzRS registers the packaged tray launcher for the current user in
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; the live registry value,
not a database flag, determines the displayed state. Linux archives document a
dedicated `ersatzrs` account and a systemd unit enabled with `systemctl`.

## Context

ErsatzTV exposes a Windows startup preference backed by the current user's
Windows startup registration. Its Linux packages use an operating-system
service definition instead: systemd starts the server at boot under a
dedicated account. Those mechanisms have different ownership and privilege
boundaries.

ErsatzRS ships a Windows tray launcher named `ErsatzRS.exe` beside
`ersatzrs-web.exe`. Registering the server executable directly would bypass the
tray lifecycle, including its graceful **Exit** behavior. Persisting a second
boolean in the database would also drift when a user, installer, or system tool
changes the registry independently.

Linux portable archives do not currently install system files and the web
process should not write `/etc/systemd/system`, create service users, or invoke
privileged service-management commands.

## Decision

On Windows, automatic startup is an opt-in, per-user login behavior. The admin
UI reads and writes only the `ErsatzRS` value in the current user's `Run` key.
The value is a quoted absolute path to the sibling `ErsatzRS.exe` launcher.
Turning the setting off deletes only that value. A missing value is disabled;
an equivalent value is enabled; a stale or different value is shown as needing
repair. Registry failures or a missing packaged launcher are visible as an
unavailable state rather than being reported as success.

The setting is exposed through the application settings-runtime port so the
web layer does not own registry operations. Non-Windows implementations return
an explicit unsupported status and the server-rendered UI omits the control.
No startup value is stored in the database.

On Linux, automatic startup remains an operator-owned systemd concern. The
portable archive documentation provides a unit running `ersatzrs-web` as a
dedicated user, enabled under `multi-user.target`. ErsatzRS does not add a
Linux toggle, privileged helper, service installer, or package-manager scripts
in this slice.

## Consequences

The Windows checkbox reflects the machine's actual state and follows an
extracted release when the user applies the repair state from the new folder.
Each Windows user controls their own login startup without elevation. Moving or
deleting an enabled release folder is detected as a repair/unavailable state on
the next successful server start.

Linux startup works before any user signs in and follows conventional service
ownership, logging, restart, and permission controls. It requires an
administrator to install and enable the unit. Docker and macOS behavior are
unchanged.

## Alternatives considered

- Store one cross-platform database boolean. Rejected because a database may
  move between machines and cannot prove the registry or service-manager state.
- Register `ersatzrs-web.exe` directly. Rejected because Windows releases use
  the tray launcher as their user-facing lifecycle owner.
- Let the Linux web process install or enable a systemd unit. Rejected because
  a network-facing application should not gain root-level service-management
  authority and portable archives do not own `/etc`.
- Add a scheduled task or Windows service. Rejected for this user-facing mode:
  HKCU login startup matches the tray launcher and does not require elevation.

## References

- [Install on Windows](../how-to/install-on-windows.md)
- [Install on Linux with systemd](../how-to/install-on-linux.md)
- `crates/ersatzrs-web/src/host/settings.rs`
- `crates/ersatzrs-launcher/src/main.rs`
