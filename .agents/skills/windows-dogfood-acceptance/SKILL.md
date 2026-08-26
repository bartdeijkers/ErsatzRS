---
name: windows-dogfood-acceptance
description: Validate an exact ErsatzRS Windows candidate or investigate a Windows dogfood installation through the packaged launcher and admin UI. Use for pre-beta acceptance, package/runtime identity, SSR-WASM consistency, live UI scenarios, or Windows-only regressions; it does not authorize publication or mutation of a production database.
---

# Windows Dogfood Acceptance

Read [BUILD.md](../BUILD.md),
[the local Windows release guide](../docs/how-to/release-locally.md), the active
Windows hardening plan, and the scenario's parity/milestone acceptance.

## Establish candidate identity

1. Use an independently extracted candidate archive in a fresh temporary
   directory. Record its full source SHA and archive/executable digests.
2. Verify ZIP integrity, the four executables, assets, and exclusions. Confirm
   `ErsatzRS.exe` launches the packaged server rather than a stale build-tree
   binary.
3. Run the structural web-bundle consistency gate against the exact packaged
   server and `site` assets before interpreting any admin button result.
4. Set isolation variables in the shell that starts the process and require the
   first bootstrap evidence to identify the disposable database. Never rely on
   a parent WSL/Git-Bash environment implicitly reaching a native process.

## Exercise the product

- Use a disposable or explicitly approved copied database. Do not edit the
  operator's production database as an acceptance shortcut.
- Complete the requested workflow through the hydrated admin UI, including
  restart/persistence behavior and no-JS fallback where the contract requires
  it. Capture zero console errors and relevant network/server-function results.
- Verify health, IPTV/HLS output, managed FFmpeg identity, and bounded process
  cleanup where playback is in scope.
- On an existing SQLite installation, use
  `cargo xtask runtime-audit --db <database> --redact` for read-only aggregate
  evidence before proposing repair.

## Handoff and authority

Report candidate SHA/digests, isolation, scenarios, results, logs retained,
failures, and cleanup. Redact URLs, titles, media paths, credentials, and local
machine paths. Tagging, pushing, GitHub publication, live database repair, and
service restarts remain separate actions that require the user's request and
the applicable release/destructive-action checks.
