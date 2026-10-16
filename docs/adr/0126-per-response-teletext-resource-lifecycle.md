# ADR 0126: Authorize bounded teletext resource lifecycle headers per response

## Status

Proposed

## Implementation

Partial — bounded native authorization, new artifact roles, compatible storage
and per-response delivery integration are implemented. P9A-04 owns50 unique
low-level/storage,2 fingerprint and32 native successes, plus real three-viewer
ordinary warm transport with source AV/clock, TSDuck and one retained independent
bitmap decoder per response. Actual warm-client rendering/navigation and complete
engine acceptance remain required; prior cold-client proof retains its scope.

## Context

ADR 0123 requires authored resource updates without damaging ordinary playback.
A genuine QTeletextMaker bitmap concept retains its immutable original resource
transmissions. A cold stream accepts it, but a response that completed the old
resource correctly rejects changed bytes with the old counter. P9A-04 records the
native reproduction and independently proven cold ordinary/client flow.

The accepted artifact currently fingerprints exact native units. Silently
rewriting its header after acceptance would violate that identity. Advancing a
global counter from the previous accepted generation also fails: accepted states
can be coalesced before delivery, and two responses can have different actual
completed resource histories. Neither pending publication nor prepared output
proves receiver acquisition. Existing safe boundaries and acknowledgement rules
must remain authoritative.

## Decision

Keep immutable original documents, concepts, preview semantics and encoded page
bodies. Introduce separately versioned artifact roles: base version 3 and
scoped/effective version 4. Preserve version 1/base and version 2/effective exact
fingerprints, strict lifecycle validation and recovery behavior. No existing row
or historical artifact is reinterpreted or upgraded implicitly.

A new artifact binds the exact baseline service plus a finite, checked resource
header family. For each native full resource, in physical service order, generate
eight S2 values times full Changed/Unchanged first transmission through the
existing checked metadata and native header encoder. All sixteen 46-byte headers
are fingerprinted with resource/page ordering. Non-lifecycle controls, native
identity/S1, actual body extent, transmission mode and every body unit stay exact.
No Cartesian product or complete-service cloning is needed. Current resource and
page bounds plus the original deadline bound construction and hashing.

Construct this authorization inside the native encoded service before hashing
and acceptance. Its private payload cannot be attached from untrusted detached
headers. The same shared immutable service carries the checked authorization
through output selection, monitor updates, bindings and control actions. Baseline
unit getters remain exact for preview, export and diagnostics. An unqualified
encoded service and recovered version 1/2 artifacts have no such authorization.

At the existing acknowledged complete-cycle boundary, each carousel chooses only
authorized headers against its own actual completed history. An equal live body
with Unchanged intent retains its completed S2. Changed bytes/kind/extent,
explicit Changed intent, or a known retired owner require the next modulo-eight
S2 and Changed first transmission. A cold unknown owner uses its exact authored
seed. Pending, discarded and skipped generations never advance history.

Keep the selected metadata alongside the shared baseline. First headers and
repeated headers use that selected counter; repeats retain existing C4/C8 clearing
rules. Completed history records the actually selected counter, never the
baseline seed. No page-body or visual-model mutation is permitted. Existing
acquisition order, timing/capacity, M29/MOT retirement protection, failure
atomicity, source-clock and sink-acknowledgement fences remain unchanged.

Recovery rebuilds the same authorization and verifies its complete versioned
fingerprint before confirming the current pointer. A compatibility migration
widens accepted artifact roles while preserving immutable histories, both pointer
families, IDs, sequence state, constraints and triggers. Missing or invalid
artifacts still follow the existing optional-teletext failure policy; scripts
never acquire package-install authority.

## Consequences

Streams may emit distinct authorized lifecycle headers for the same validated
page content. The selected counter is transport state, not a change to the saved
design or shared browser preview. Existing exact-artifact APIs and historical
rows retain their previous meaning. The bounded sidecar adds sixteen resource
headers per authorized resource; display and resource bodies remain shared.

Required proof includes independent streams with different completed histories,
skipped and discarded candidates, unchanged MOT, deliberate equal-body refresh,
wraparound, retirement/reintroduction, selected-counter retention on repeats,
legacy strict rejection, failure atomicity and real ordinary warm delivery.
TSDuck, independent decoding and suitable-client display/navigation retain their
separate evidence scopes. This ADR accepts no restriction on full Level 2.5,
fallback, QTeletextMaker interchange, metadata/zones, Lua/restart, localized admin
or playback/platform acceptance. Production guide content still waits for
independent generic engine acceptance.

## Alternatives Considered

- Increment from the previous accepted generation: cannot represent per-response
  completed history or accepted generations that were never transmitted.
- Relax the strict receiver-history check: would permit stale counters and hide
  the demonstrated defect.
- Rewrite accepted version 1/2 units: breaks their exact fingerprint contract.
- Clone/recompile a complete service per stream and counter combination: repeats
  validation and copies immutable content without resolving acceptance identity.

## References

- [ADR 0123: authored Level 2.5 and external editors](0123-level-2-5-teletext-and-external-editor-workflows.md)
- [ADR 0124: scoped publication](0124-effective-teletext-originals-and-scoped-publication.md)
- [P9A-04 acceptance and evidence](../progress/milestones.toml)
- [Teletext interchange reference](../reference/teletext-design-interchange.md)
