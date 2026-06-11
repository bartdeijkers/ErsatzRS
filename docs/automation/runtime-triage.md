# Runtime triage routine

Use this when live runtime evidence contradicts the progress index: playback
fails, scanner output regresses, a channel cannot start, or user-observed logs
show behavior that a `Ported` feature should already cover. The goal is to turn
runtime failures into explicit parity work instead of leaving them as chat-only
diagnostics.

## Inputs

- The current `ErsatzRS.log` or a dated log excerpt.
- The affected route, channel, library, provider, or command when known.
- The pinned ErsatzTV baseline from [parity-baseline.md](parity-baseline.md).

## Procedure

1. Capture the evidence date and the exact log source. Do not rewrite a stale
   diagnostic as current proof.
2. Cluster WARN/ERROR lines by failure class, not by repeated line count:
   FFmpeg startup/exit, filter-graph conversion, decoder fallback, HLS segment
   timeout, scanner parse failure, database lock, provider availability,
   search mismatch, schedule/build overlap, or UI/runtime save failure.
3. Map every cluster to one `docs/progress/parity/<slug>.toml` file. If no slug
   exists, add a conservative note to `docs/progress/phase.toml` `next_steps`
   rather than inventing a feature.
4. Confirm the behavior is in scope for the pinned ErsatzTV commit. If the
   failure is caused by post-baseline upstream behavior, do not create parity
   debt; note it as out of scope.
5. For each in-scope cluster, add a dated `[[debt]]` item:

   ```toml
   [[debt]]
   id          = 'short-stable-slug'
   description = 'What behavior is missing or contradicted.'
   discovered  = 'YYYY-MM-DD'
   evidence    = 'ErsatzRS.log: short exact failure signature.'
   ```

   Keep `resolved = true` only for a debt entry retained as historical context.
   Open debt is the default.
6. Keep or refresh the narrative `summary` so older tooling still shows the
   current state, but do not bury new evidence only in prose.
7. Downgrade `status = 'Ported'` to `In progress` when the evidence contradicts
   the ported claim. Do not downgrade for weak or out-of-scope evidence.
8. Add a session entry with `cargo xtask progress add-session --agent
   "runtime-triage" --summary "<one line>"` whenever a status changes or new
   structured debt is added.
9. Validate with `cargo xtask progress check`, then inspect `cargo xtask
   progress gaps` to ensure the new debt is queryable.
10. For streaming-path fixes, run `cargo xtask smoke` before closing the debt
    or marking the feature `Ported`. Preserve the command output or log path in
    the session summary when it is the runtime proof.

## Evidence rules

- Quote only the shortest useful failure signature in `evidence`.
- Prefer concrete runtime facts over interpretations: exit code, route,
  channel id, library id, FFmpeg stderr signature, SQL error code, or query
  name.
- If one failure affects several features, add one debt item to the feature
  that owns the fix and cross-reference other summaries only when needed.
- A snapshot or unit test can close a debt only when it covers the runtime
  failure class. Streaming-path debt generally needs live HLS/IPTV proof or an
  explicit note that runtime proof remains missing.
