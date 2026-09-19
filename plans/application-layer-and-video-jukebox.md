# Video-jukebox implementation plan

Queued for Phase 10, after teletext milestone `p9b-02`. Milestones `p10a-01`
through `p10b-02` in [the tracker](../docs/progress/milestones.toml) own
acceptance and evidence. Phase 7 application-boundary history remains in its
completed milestone records and ADR 0051.

Build a durable music-video request queue through the existing application
boundary: transport adapters call `ersatzrs-app`, core ports define policy and
storage, infrastructure implements persistence, and the scheduler consumes work.
Do not use an in-memory queue as the source of truth or introduce a native
plugin ABI. Keep SQL in infrastructure. The
[web/WASM split](web-server-wasm-crate-split.md) must keep application services
native-only; shared hydration DTOs must not depend on concrete services.

## Phase 10A — Durable video-jukebox domain

### P10A-01 — Product and architecture decisions

Before schema or API work, accept an ADR covering:

- channel scope and eligible music-video sources;
- requester identity, moderation, authorization, and privacy;
- FIFO/priority policy, duplicate and artist/video cooldowns, expiry, and
  per-source rate limits;
- current-item behavior, safe insertion boundaries, cancellation, retries,
  filler interaction, and failure recovery;
- stable add-on transport/versioning and whether adapters run out of process or
  are compiled into the binary.

### P10A-02 — Durable request aggregate and ports

- Add request identity, channel/media identity, source/requester identity,
  idempotency key, queue position, timestamps, and rejection/failure detail.
- Use explicit states: Pending, Accepted, Rejected, Queued, Playing, Played,
  Cancelled, Expired, and Failed.
- Add storage-neutral repository/catalog/policy/clock/event ports and a SQLite
  implementation.
- Make submit, idempotency, claim, and state transition operations atomic.

### P10A-03 — Application use cases and scheduler integration

- Add catalog search, submit, list/status, cancel/moderate, claim, start,
  complete, fail, and recovery use cases.
- Let scheduling claim an accepted request only at the ADR-defined safe
  boundary, attach its request id to the materialized playout item, and preserve
  the currently playing item during refresh.
- Recover accepted/playing work after restart without duplicate scheduling.

## Phase 10B — Add-on ingress and runtime proof

### P10B-01 — Stable add-on transport

- Expose a versioned authenticated HTTP command/query API and an SSE or
  equivalent status/event feed.
- Apply authorization, rate limits, idempotency, and audit logging at the
  application boundary.
- Keep adapter SDK/examples protocol-only; they must not expose SQL, Leptos
  server-function URLs, or scheduler internals.

### P10B-02 — Operations and end-to-end acceptance

- Add health checks for stalled claims, invalid requested media, failed
  scheduler handoff, and add-on configuration.
- Add request/queue/state-transition metrics without logging tokens or private
  media paths.
- Prove duplicate-idempotency handling, rejection policy, restart survival,
  exactly-once scheduling, playing/played transitions, and multiple independent
  add-on sources in a real end-to-end run.

## Vertical-slice completion rule

Every implementation milestone must close with:

- source-vs-port comparison when ErsatzTV behavior exists;
- application unit tests with fake ports;
- infrastructure database tests for SQLite schema and concurrency-sensitive shapes;
- focused web/scheduler tests for adapter and side-effect behavior;
- updated parity debt and milestone evidence;
- `cargo xtask progress check`;
- the narrow wrapper-based Rust test command for changed crates;
- the relevant hydration, Playwright, smoke, or TSDuck gate when the slice
  changes admin interaction or playback behavior.

Run the full multi-configuration repository gate once at the closeout of a
phase, not once per milestone.
