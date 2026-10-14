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

## Teletext candidate controls

These functions address an existing add-on binding on the script's own output
channel. They share application profile validation with admin controls and never
install, activate or invoke packages, or rewrite the admin base selection.

| Function | Arguments | Effect |
|---|---|---|
| `teletext_profile(addon_id, profile_id)` | Add-on string and positive Lua integer | Select a validated existing profile; preserve enabled state |
| `teletext_enabled(addon_id, enabled)` | Add-on string and Lua boolean | Turn the candidate binding On or Off |

Each returns `{ok=true}` or `{ok=false, code="..."}`. Arguments are strict:
strings/floats are not coerced into profile IDs, and non-booleans are rejected.
Profile selection validates even while Off. On revalidates the last successfully
selected profile, or the observed admin base profile when none was selected.
Off requires only current channel/add-on membership, so an unavailable profile
or package cannot prevent disabling teletext.

Handled rejections preserve the preceding candidate state. Static codes include
`invalid_argument`, `invalid_cursor`, `unsupported`, `unavailable`,
`invalid_response`, `command_limit`, `snapshot_limit`, and profile-specific codes
such as `missing_binding`, `missing_profile`, `wrong_addon`, `deleted_profile`,
`installation_unavailable`, `invalid_settings` and `conflict`. An uncaught Lua
error rejects the complete scheduling result.

Successful commands create half-open intervals from `current_time()` until the
next successful command for that add-on or the actual last accepted item finish.
At an equal cursor the last successful command wins. Commands at/after the
actual finish and builds with no accepted items publish no override. A new
Continue evaluation starts from the admin base. Refresh/Reset retain or replace
intervals at the same programme seam as items and checkpoints.

Intervals and exact unresolved profile/package/settings snapshots are stored
atomically with the schedule. Reopening reads the published intervals without
replaying Lua; expiry returns no override. Missing, stale or malformed active
snapshots remain unavailable until their original expiry. Mirrors read their
own output channel's intervals rather than inheriting source presentation.

The worker uses one original 30-second teletext lookup deadline per build and
existing cancellation checks. Provisional work bounds are 1,024 command attempts
and 4 MiB of retained candidate material; database framing also bounds reads
before allocating snapshot payloads. These are host admission bounds, not
permanent authored-page limitations.

Runtime activation remains incomplete: current base artifacts do not carry an
interval identity and are refused during an active override. This preparatory
guard suppresses the complete base authored generation; preserving unaffected
bindings requires effective whole-channel selection and scoped composition.
The existing Scripted Health check reports unavailable active selections and
profiles whose pages are not yet ready, while valid Off is configuration-valid.
It does not prove stream application. Complete scoped
generation preparation/promotion, per-binding stream application, requested versus
applied state and live restart/expiry proof remain required. A successful Lua
result or persisted interval alone does not establish effective On playback.
