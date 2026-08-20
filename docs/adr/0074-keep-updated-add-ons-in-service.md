# ADR 0074: Keep an updated add-on in service

## Status

Accepted (2026-08-19)

## Implementation

Implemented (working tree, 2026-08-19)

## Context

ADR 0066 gave installed add-ons an explicit install → configure → check →
enable lifecycle and required a fresh enable check after every update. The
implementation expressed that by writing `Enabled = 0` and clearing the stored
readiness result whenever a package was replaced, and by leaving the rest of
the lifecycle to the operator.

For a first install that is right: a new package is unconfigured, its
permissions have not been reviewed, and nothing depends on it yet. For an
update it is not. A working, enabled add-on stops serving the moment its
package is replaced, and stays stopped until someone notices and clicks
**Check** and **Enable** again. Every scheduled Remote Stream, media list
refresh, and playback path that relies on that add-on fails in the meantime,
for no reason the operator chose. Updating an add-on that works is expected to
keep it working; the maintainer reported the current behavior as undesirable
after updating an add-on in normal use.

The readiness check itself is not the burden — it is fast, it is exactly what
`enable_addon` already runs, and the host can run it without help. What
genuinely needs a human is a change in what the package is allowed to do.
Permissions are declared in the manifest, shown before installation, and
accepted by enabling; a new version that reaches new hosts, reads media it
could not read before, or runs new external commands is asking for something
the operator has not agreed to.

## Decision

An update finishes the lifecycle it interrupted. After a package update
replaces an add-on that was enabled, the host runs the same validation
`enable_addon` runs — required settings, dependencies, RID and host
compatibility, package integrity, and `addon.check.v1` — and re-enables the
add-on when it passes. A failing check leaves the add-on disabled with the
failure recorded and operator-visible; the update itself still succeeds.

The one change the host will not accept on the operator's behalf is a
privilege increase. When the new manifest declares a network host, an external
command, media read access, or managed FFmpeg access that the previous version
did not, the add-on stays disabled until someone reviews the new permissions
and enables it. Narrowed or unchanged permissions are not a review reason.

First installs are unaffected and still start disabled. Manual installation and
update remain manual: nothing here makes updates automatic, and this decision
does not change ADR 0066's rejection of silent background updates.

## Consequences

- A routine update of a working add-on no longer takes it out of service.
- An update that fails its readiness check behaves exactly as before: disabled,
  with the recorded check result explaining why.
- An update that widens permissions is held for review, so the permission
  screen keeps its meaning as the point where privileges are granted.
- The update path now runs the add-on's `check` entrypoint, so a package that
  is slow to answer makes the update request slower.
- The permission comparison is a host-side manifest diff, not a sandbox. It
  records what a package declares, exactly like the rest of ADR 0066's
  permission model.

## Alternatives Considered

- Keep the previous `Enabled` value without re-checking: rejected because the
  code behind the add-on has been replaced, and an enabled add-on with a stale
  readiness result is exactly what ADR 0066's fresh check exists to prevent.
- Re-enable unconditionally, permissions included: rejected because it would
  let a new version grant itself privileges the operator never reviewed.
- Leave the behavior and explain it in the UI: rejected because the problem is
  the silent outage, not its discoverability.
- Ask the operator to confirm re-enabling in a dialog after each update:
  rejected because the answer is "yes" in every case where the check passes and
  the permissions are unchanged, which is the case this decision addresses.

## References

- [ADR 0066: Repository-backed add-ons](0066-repository-backed-add-ons.md)
- [Install and manage add-ons](../how-to/manage-addons.md)
- [Add-on platform parity record](../progress/parity/addon-platform.toml)
