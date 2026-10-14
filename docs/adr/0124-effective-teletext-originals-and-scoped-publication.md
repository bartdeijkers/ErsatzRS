# ADR 0124: Effective teletext originals and scoped publication

## Status

Proposed (2026-10-14). This records implementation choices within the maintainer's
authorized Phase 9 profile/Lua scope; ADRs 0122/0123 remain the accepted product
contract. It changes no engine-first, standards, editor or acceptance requirement.

## Implementation

Partial — distinct version-2 originals, bounded native save/reload, compatible
immutable-history migrations and explicit base/editor role checks are implemented.
Separate scoped request/accepted pointers, original-scope confirmation, native
promotion/recovery and version-2 live direct-route selection are connected.
The channel admin now observes effective scope and dispatches concept preparation,
immutable preview and explicit request/acceptance by stored revision role,
including absence without invented content. A representative headed all-culture
admin/Lua flow now reaches ordinary output and independently decoded, visibly
navigable two-theme captured replay. Effective originals now connect native editor
export/import-as-concept and authenticated overlay restoration through the same
compiler. Public project schema2 records real producing origins and nullable saved
expiry; schema1 and outer receipt/sidecar schemas remain compatible. Historical
editing preserves expired scope while publication retains fresh authority. A
representative actual pinned-editor save now passes through concept import,
preview, explicit publication, ordinary stream capture and independently decoded,
visually inspected two-theme client replay. Repeated concept imports now retain
one authenticated original and cumulative rows; the latest export supplies each
new edit's authority. Native correction tests and representative repeated-editor
headed Chromium/Firefox flows now pass, with independent cumulative Q/R decoding
and visibly navigable isolated client replay. Actual pinned-editor GUI/capture
evidence is composed with complete supplementary browser cases; the interrupted
long GUI/capture case is not a complete pass. A new role2 cold real-Router fixture
now connects cumulative concepts to actual ordinary-URL client attachment and
inspected two-theme navigation, preserving original expiry and reopened state.
Complete editor coverage, warm transitions and whole-host restart remain open.
P9A-04 owns verification and remaining userflow gaps. This does not establish
same-URL suitable-client transitions or full engine acceptance.

## Context

Unpublished native preparation now keeps a complete effective observation: base
bindings, actual published Scripted origins, explicit Off and the earliest
scheduled transition. Legacy original envelopes and accepted pointers describe
base bindings only. Converting an interval into a base binding would lose its
owner, lifetime and authority. Replacing the only accepted base pointer would
also prevent correct restoration at interval expiry.

The original immutable revision schema permitted document version 1 only, and
publication/export readers relied on that restriction. Supporting another opaque
version requires explicit role checks before widening storage. Existing byte,
digest, type, CAS, tombstone and immutable-history guarantees must remain intact.

## Decision

Keep the version-1 codec and exact legacy bytes. Add a separate version-2
effective-original envelope with complete tagged origins, owning playout and
interval bounds, `valid_until`, explicit producer-to-member indices and original
contract/response/request, entry, region and resource lifecycle data. Preserve
V3 template/theme/zone/dynamic-field metadata in its validated original response.
Off has no response but remains in the complete association. Bound all retained
context and source allocations before structural/content restoration. Historical
restoration and preview remain possible after expiry without producer/Lua replay;
they grant no current publication or template/editor authority.

Opaque immutable storage accepts exactly versions 1 and 2. Base publication and
base generation admission require version1 explicitly. Historical native exports
retain either known original role, using exact stored-version/digest/head checks
and original-byte/receipt-byte confirmation. Effective public project schema2
records actual Base/Scripted producing origins and complete-list indices; it never
fabricates a base revision or producer for Off. Nullable UTC expiry preserves pure
base absence and actual Scripted boundaries. Version1 project bytes and outer
receipt/sidecar schemas1 remain compatible. Imports regenerate the same role's
retained baseline and validate captured edit boundaries before concept save;
metadata is descriptive and cannot install packages, modify templates or publish.
Repeated imports authenticate both the latest retained export and exactly one
overlay-free original receipt. Regenerate latest files before reconciling edits;
an unchanged latest export remains a no-op. Merge physical rows by exact page,
subcode and row, keeping previous edits, replacements and explicit original-equal
undo rows in one cumulative overlay. Validate the complete overlay against the
original, then validate only new effects against the latest export's permissions.
Earlier edits cannot grant new permissions. The original document bytes, role,
scope and expiry remain unchanged beneath that one overlay.

A later export subset may edit an owner outside the first subset. When needed,
retain a bounded native-generated original export covering the cumulative owner
union through the existing export confirmation and current-head CAS. Requalify
the concept against that actual receipt before saving. Use the same admitted work
hold and original operation deadline throughout; do not acquire a nested work
slot or replay producers/Lua. A later save conflict may leave an unused immutable
export receipt, but changes no concept head, publication pointer or package.

Scoped publication uses separate typed request/current/accept/confirmation roles
on the existing repositories. Same-transaction scope, original, pointer and
expiry checks are required. The independently accepted base remains recoverable;
an expired scoped generation cannot replace or erase it. Guide-bearing originals
retain the existing current-role guide-freshness requirement.

Retain immutable request and accepted-generation histories. Add separate effective
current-pointer tables with the same channel/composite-reference constraints.
Scoped native artifact framing version 2 includes explicit presence/absence and
the complete service fingerprint; version-1 base framing stays byte-identical.
All-Off is an accepted absence after complete native/transactional confirmation,
never a substitute for invalid or pending On. Widen the accepted-history artifact
check to versions 1 and 2 through the same compatible recreation/reinsertion
procedure, retaining row identities, references, immutable triggers and the exact
autoincrement sequence. Current reads/confirmation retain explicit base/scoped
roles. Metadata selection alone is still not a stream acknowledgement.

Confirm the supplied whole scope against the immutable original envelope as well
as current database state. A fresh valid interval/profile/base observation must
not retarget an older original. Compare the bounded scope structurally without
loading the complete untrusted document tree; unknown, missing and duplicate
fields reject. Equivalent UTC timestamp spelling does not alter literal settings.

The existing version-2 live policy selects the applicable base/scoped role before
native recovery. Active pending, invalid or expired scoped output cannot fall
through to base. All-Off retains a confirmed cached absence. Preserve the
earliest observed transition as a conservative monotonic deadline; withdraw at
that boundary before a potentially blocked reread. Recover the independently
accepted base only after current authority and native checks permit it. Response
ownership and actual worker admission survive cancellation as before.

Actual source EOF closes new activation in that response: reject an uninstalled
replacement before it can announce a service with no complete future cycle. Keep
requested withdrawal, installed-action acknowledgements, physical AV, deadlines
and strict EOF/full-cycle checks. Independently accepted output remains available
for the next response. Do not undo an already installed incomplete cycle.

Rebuild the revision table in the migration-owned transaction to widen its check
constraint while keeping all rows, rowids, bytes, digests, composite identities,
foreign keys and immutable triggers. Use transaction-local foreign-key deferral
with foreign-key enforcement still enabled. Copy rows to a temporary table,
recreate the original table under the original name, and reinsert those rows
before commit. Reinsert under that name so deferred parent-removal counters are
settled. Never disable deferral before commit or edit `sqlite_schema` directly.
Validate all affected foreign keys and restore immutable triggers before commit.
Any failure rolls back the whole migration and keeps the old schema/history.

## Consequences

One temporary database copy is needed during the compatible table rebuild.
Disk/constraint failures retain the existing migration failure and schema-health
behavior. No production database is modified as part of development verification.
Upgrade proof must include existing pending/accepted/export references, exact
legacy content, foreign-key enforcement and atomic rollback.

The scoped native promotion path currently qualifies frozen synthetic originals;
guide-bearing originals still require current-role guide freshness and reject
instead of emitting stale output. This is an integration gap, not an accepted
authoring restriction. Version-1 startup-only delivery keeps its base-only guard.
Automatic preparation/publication following schedule controls, headed localized
scoped administration and per-output applied acknowledgement, the complete
delivery matrix, whole-host restart
and same-URL suitable-client proof remain required. The parity record and P9A-04
own verification and the next concrete connection.

## Alternatives Considered

- Store an interval as a base snapshot: loses authority and original expiry.
- Overwrite the accepted base pointer: loses independent expiry restoration.
- A second package, configuration or storage framework: existing repositories,
  opaque revisions and native admission already supply the required mechanisms.
- Disable foreign keys or patch schema text: bypasses supported integrity checks.
- Copy into a renamed replacement before dropping the old table: a targeted
  SQLite 3.51.3 probe has no reported final orphan yet fails commit because the
  deferred removal counter remains unsettled. Reinserting after recreation
  commits with both enforcement and reference integrity retained. Native SQLx
  upgrade/rollback tests must verify the actual application path as well.

## References

- [Channel profiles and Lua](0122-channel-teletext-profiles-themes-and-lua-control.md)
- [Level 2.5 and editor contracts](0123-level-2-5-teletext-and-external-editor-workflows.md)
- [Structured progress](../progress/README.md)
- [SQLite foreign-key deferral](https://www.sqlite.org/pragma.html#pragma_defer_foreign_keys)
- [SQLite table changes](https://www.sqlite.org/lang_altertable.html)
