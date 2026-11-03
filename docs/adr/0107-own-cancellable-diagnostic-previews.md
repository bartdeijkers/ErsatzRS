# ADR 0107: Own cancellable diagnostic previews

## Status

Accepted 2026-10-02 — the maintainer authorized the full finding-08 solution
for v1.1.1, including ownership, explicit status, Stop and programme-relative seek.

## Implementation

Implemented and accepted locally under `v111-findings-08`. Its evidence records
native/Next cancellation, cleanup, capacity, stale-ID, UTC/seek and SSR/enhanced
browser proofs. Integrated v1.1.1 web parity and release acceptance remain separate.

## Context

ErsatzTV's playback troubleshooting page uses an exclusive diagnostic lock and
a finite output directory. ErsatzRS preserves that isolation, but destroying
the browser player does not stop the server producer. A second diagnostic
request therefore fails until the process completes or reaches its deadline.
A response can also arrive after the user navigates away, and static diagnostic
URLs do not identify which operation owns their contents.

## Decision

Expose an ephemeral owned diagnostic ID through the application runtime port.
Queue admission returns immediately; bounded retained records report queued,
running, completed, failed, cancelling and cancelled states, output readiness,
a safe error code and UTC RFC 3339 selection/update timestamps. IDs are revoked
at host restart. Diagnostic work has no publication intent or durable playout
mutation, so it does not need a database job or automatic restart/retry.

Stop addresses one ID only. The existing exclusive preview lock remains held
through process-tree reaping, controller shutdown and diagnostic publication.
Both native FFmpeg and Next reuse the existing managed Unix process-group /
Windows-job owner. Both use global transcode admission independently of live
channel identity. Cancellation never invokes a live channel's stop controller.
Unconfirmed cleanup retains the lock's quarantine and capacity reservation;
only a server restart clears that condition.

An owned Stop also retires completed output when the finite producer finishes
before the request arrives. If cleanup is already confirmed, transition directly
to cancelled, revoke output readiness and retain the completed diagnostics. This
retires playback; it does not claim to have interrupted the completed producer.
Active cancellation still waits for actual cleanup, and failed or unconfirmed
cleanup remains fenced. A repeated or stale Stop cannot affect a replacement.

Serve owned HLS members behind a generation fence, with bounded file reads,
validated member names and no-store responses. An earlier ID cannot read a
replacement's output or cancel a later operation. Keep the legacy diagnostic
GET/playlist and archive endpoints compatible. Preserve the two-minute encoding
deadline, Next's bounded preparation, capped diagnostics and finite previews.

The SSR page keeps native POST start/Stop forms and a refresh-status link.
Enhanced submissions poll the same owned operation and retain Stop during
preparation and encoding. Channel seeking is relative to the selected programme
row, clamps inside that row and preserves source in-points. Show explicit UTC
input and selection timestamps, including for historical previews. No channel,
playout or scheduling history is changed by these controls.

## Consequences

This intentionally improves on ErsatzTV's diagnostic controls. Retained ID
history is bounded; only current output remains available after replacement.
Cancellation is acknowledged before cleanup, and a new preview is admitted
only after the existing owner releases its resources. The visible cancelling
state distinguishes that interval from completed cleanup.

## Alternatives Considered

- Cancel the browser player only: leaves FFmpeg and its reservation alive.
- Stop the live channel worker: violates diagnostic isolation.
- Persist and restart diagnostic jobs: adds publication-style coordination to
  an explicitly temporary operation and surprises operators after restart.
- Keep unowned static URLs for enhanced playback: permits stale clients to
  observe replacement output.

## References

- [Application boundaries](../../ARCHITECTURE.md)
- [Progress acceptance](../progress/milestones.toml)
- [Next graphics process ownership](0105-render-next-graphics-canvases-with-ffmpeg.md)
- `$ERSATZTV_SRC/ErsatzTV/Pages/Troubleshooting/PlaybackTroubleshooting.razor`
- [Tokio child supervision](https://docs.rs/tokio/latest/tokio/process/struct.Child.html)
