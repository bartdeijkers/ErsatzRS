---
name: durable-background-work
description: Design or change restart-safe bounded background work for scans, add-on imports, provider enrichment, retries, or external processes. Use when network/provider work risks request timeouts, rate-limit/fair-use violations, lost progress, duplicate execution, or unbounded cancellation.
---

# Durable Background Work

Keep HTTP/admin actions bounded: validate and persist intent, then return useful
state. Long enumeration, detail lookup, probing, and external-process work runs
through a durable coordinator rather than inside the request.

## Contract

- Persist generation, cursor/checkpoint, item identity, policy, attempts,
  diagnostic, lease owner/expiry, heartbeat, next eligibility, and outcome.
- Claim bounded work transactionally and make retries idempotent. A crash after
  external success but before persistence must not duplicate published state.
- Separate cheap overview discovery from optional per-item detail work when the
  provider supports it. Do not block publication on nonessential enrichment.
- Share one provider-host limiter across automatic and manual work. Apply
  positive configurable jitter, `Retry-After`, and backoff without shortening a
  persisted cooldown. Manual priority does not bypass fair use.
- Bound stdout/stderr, runtime, item/page counts, and concurrency. Cancellation
  must terminate the complete Unix process group or Windows Job Object.
- Preserve the last complete publication on failure and expose queued, active,
  waiting, retrying, stale, failed, paused, and complete states through progress
  and health.

## Verification

Use paused/virtual time rather than real multi-second sleeps. Cover work beyond
the old request deadline, page/item crash checkpoints, lease expiry, concurrent
claims, retry extension, configuration changes without restart, cancel, and
process-tree cleanup on Windows and POSIX. Prove bounded logs contain no URLs,
tokens, cookies, or private paths.
