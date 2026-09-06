# ADR 0086: Isolate add-on preparation process ownership

## Status

Accepted for preparation; superseded by ADR 0087 as a streaming prerequisite.
The 2026-09-06 split retains this recovery decision for AMM-M-01B Download ahead,
not AMM-M-01A streaming. The worker is not required for interval streaming.

## Implementation

Partial — private worker dispatch, checkpoints and exact-attempt reconciliation
remain in the preserved uncommitted work. Further preparation implementation and
crash/restart acceptance await AMM-M-01B, not the current M-01A gate. See
[ADR 0087](0087-stream-sponsorblock-intervals-without-playback-underruns.md).

## Context

Finite preparation in ADR 0085 can run provider descendants and a final media
probe. Its database lease and artifact reservation survive a web-host crash,
but in-process cleanup does not. Lease expiry, reaping the direct child, and
releasing an operating-system file lock do not prove that descendants stopped.
Reclaiming storage or retrying on those signals can overlap the old attempt.
Persisting a PID and later signalling it risks targeting an unrelated process
after identifier reuse.

## Decision

Run the existing preparation coordinator in one private worker mode of the
same web executable. Do not introduce a separately installed service, new
crate, second preparation state machine, or per-provider-command IPC protocol.
The worker owns claims, heartbeats, provider children, final probes and artifact
admission. It opens the already-initialized database without starting HTTP,
unrelated background work, migrations or runtime downloads.

The web host retains a private liveness pipe. EOF requests cancellation; the
worker awaits managed cleanup and durable handoff before exiting. The worker
must survive termination of the web host alone. Normal shutdown uses the same
awaited path rather than killing the worker tree prematurely.

An operating-system lock under the processed-artifact root excludes another
preparation worker or recovery pass. Persist a worker epoch and exact active
job/generation/lease identity before executing its first subprocess, including
runtime-info. Record cleanup proof only after all managed process owners have
confirmed termination. The lock establishes exclusion; the durable checkpoint
establishes which attempt has cleanup proof. Neither replaces the other.

Recovery reconciles only the evidenced attempt's staging/reservation and
requeues an interrupted attempt through existing generation and saved-input
fences. It preserves manual cancellation, completed artifacts and provider
cooldowns. Unconfirmed process cleanup remains a durable quarantine visible to
provider admission in the web host as well as to Health; moving the worker
must not lose the existing process-local safeguard.

On Unix, observe leader exit without reaping, issue the single process-group
kill while the leader still pins its identity, then reap and confirm group
absence. Use the safe, already-resolved rustix process API for Linux and macOS;
the locked nix waitid API does not include macOS. On Windows, retain the
existing independently tracked Job Object and confirm empty membership.

## Consequences

The web executable gains one private runtime mode and one bounded worker
process. Existing coordinator, quota and publication boundaries remain intact.
Acceptance must kill a synthetic web host during real preparation, demonstrate
that its worker drains descendants, restart, and prove one safe resumed attempt
without double charging or raw playback fallback. Unix and Windows need native
process-lifetime evidence.

This decision covers forced web-host exit, not arbitrary simultaneous worker
death. If the worker itself dies, a power loss interrupts its checkpoint, or
cleanup cannot be proven, retain reservations and report recovery required.
Do not guess a reboot from timestamps or uptime. Broader automatic recovery
requires separately verified, platform-complete termination evidence.

## Alternatives Considered

- Lease expiry, an unlocked file, or a missing leader: insufficient descendant
  termination evidence.
- Persisted PID/PGID signalling: unsafe after identifier reuse.
- A per-attempt guardian with command-by-command RPC: duplicates orchestration
  and splits heartbeat ownership without helping this host-crash boundary.
- A new daemon or platform-specific service: unnecessary installation and
  lifecycle complexity for one serialized worker.
- A separate Unix group-anchor mode: unnecessary when non-reaping observation
  preserves the existing leader identity through termination.

## References

- [ADR 0085](0085-broaden-addon-list-lifecycle-and-provider-options.md)
- [Structured progress](../progress/README.md)
- [Architecture](../../ARCHITECTURE.md)
- [Preparation coordinator](../../crates/ersatzrs-infra/src/addon_media_list_preparation.rs)
- [Managed add-on processes](../../crates/ersatzrs-infra/src/addons.rs)
