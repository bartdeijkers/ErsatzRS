# ADR 0124: Effective teletext originals and scoped publication

## Status

Proposed (2026-10-14). This records implementation choices within the maintainer's
authorized Phase 9 profile/Lua scope; ADRs 0122/0123 remain the accepted product
contract. It changes no engine-first, standards, editor or acceptance requirement.

## Implementation

Partial — distinct version-2 originals, bounded native save/reload, compatible
immutable-history migration and explicit base/editor role checks are implemented.
P9A-04 owns the focused verification. Scoped publication/current pointers,
same-route recovery and expiry remain open; no full engine acceptance is claimed.

## Context

Unpublished native preparation now keeps a complete effective observation: base
bindings, actual published Scripted origins, explicit Off and the earliest
scheduled transition. Legacy original envelopes and accepted pointers describe
base bindings only. Converting an interval into a base binding would lose its
owner, lifetime and authority. Replacing the only accepted base pointer would
also prevent correct restoration at interval expiry.

The immutable revision table currently permits document version 1 only, and
publication/export readers rely on that restriction. Supporting another opaque
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

Opaque immutable storage accepts exactly versions 1 and 2. Base publication,
generation admission and existing export receipts require version 1 explicitly.
Scoped publication uses separate typed request/current/accept/confirmation roles
on the existing repositories. Same-transaction scope, original, pointer and
expiry checks are required. The independently accepted base remains recoverable;
an expired scoped generation cannot replace or erase it. Guide-bearing originals
retain the existing current-role guide-freshness requirement.

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

Version-2 save/reload is an unpublished foundation. Scoped promotion/recovery,
pending/applied state, same-route expiry/restart and suitable-client proof remain
required. Retain the existing base guard until the scoped runtime is connected.
The parity record and P9A-04 own this remaining work and verification.

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
