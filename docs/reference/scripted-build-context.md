# Scripted build context

The in-process Lua scheduling API exposes these zero-argument functions:

| Function | Result |
|---|---|
| `build_mode()` | `"continue"`, `"refresh"`, or `"reset"` |
| `start_time()` | Immutable effective build start, as an RFC 3339 UTC string |
| `current_time()` | Mutable scheduling cursor, as an RFC 3339 UTC string |
| `finish_time()` | Requested horizon, as an RFC 3339 UTC string |
| `is_done()` | Whether the cursor has reached the requested horizon |

Continue restores the published last-finish time and uses it when restoring
series and playlist history. If no published rows remain, the anchor supplies
the continuation time. An expired timeline resumes at the request clock.
Conflicting future tail/anchor values and malformed or unsupported anchor
contexts are rejected. Existing user-supplied `arg` positions are unchanged.

```lua
add_collection("main", "Programmes", "chronological")
while not is_done() do
    add_count("main", 1)
end
```

Scripts may intentionally wait forward, leaving a gap. Emitting an item before
the effective start or preceding item's finish, or with a non-positive duration,
rejects the complete candidate. Publication also validates the retained database
timeline inside the write transaction. Rejection preserves published items,
history, anchors, overlays and other playouts.

The playout page's Activity section lists the latest failed builds on the current
page. Select a failure to inspect its diagnostic and retained published timeline.
The `Scripted Playout State` Health check reports failed Scripted builds and
published boundaries with missing or incompatible resume checkpoints. A later
successful build clears the current build failure; this is not an audit-history log.
Activity refreshes on observed build completion and at least every 30 seconds;
without JavaScript, reload the page to refresh the failure list.

Scripted Refresh retains the programme currently published at the request clock,
including a contiguous locked guide group and its attachments. Its effective
start is the preserved programme's end. It restores source positions, shuffled
ordering and Smart Shuffle state from that boundary, then replaces the future.
Membership reconciliation keeps surviving unconsumed media ahead of new and
already consumed members. Empty replacement futures, missing checkpoints and
incompatible state reject the candidate. An explicit Reset creates fresh state
for legacy timelines without checkpoints. Continue can still resume legacy
history when no checkpoint exists.

Checkpoints and the actual SQL-backed source lookups are stored atomically with
items, history and the anchor. Dependency capture includes empty dynamic searches
and library-scoped semantic signatures. Continue keeps existing dependency
baselines while adding sources used by its extension; replacing builds publish
the new set. A changed durable refresh-intent revision invalidates a candidate
before publication.
Automatic source dependency admission, explicit rebuild controls and running-
producer handover acceptance remain open under `v111-findings-06`.
