# ADR 0075: Review add-on metadata before library publication

## Status

Accepted (2026-08-21)

## Implementation

Implemented (c8d877b5, 2026-08-22)

AMM-A through AMM-E, including rollback, artwork, refresh conflicts, provider
parity, and the cross-platform/browser closeout, are complete. The separate
AMM-F live-source follow-up is owned by ADR 0078 and does not reopen this
draft/publication decision.

## Context

Storage-backed add-on lists currently become managed libraries before an
operator can inspect the provider snapshot. Television auto-presentation then
requires `tvshow.nfo`, even though the list header already contains the title
and description needed to generate it. Creating an empty file by hand only
changes the failure from missing metadata to invalid XML.

The Beeld & Geluid and yt-dlp packages also expose more descriptive metadata
than the narrow add-on-list editor can show. Provider-specific editors would
duplicate validation, artwork handling, NFO serialization, refresh ownership,
and localization while weakening the add-on boundary accepted by ADR 0068.

## Decision

An initial add-on-list import creates a provider-neutral draft. Enumeration and
metadata enrichment may write draft state, but YAML/NFO publication, library
projection, and playlist membership require explicit operator publication.
All items start included. Unknown, unavailable, and incomplete items are
flagged and may be excluded individually or in bulk before publication.
Exclusion is durable by provider ID and does not discard metadata or overrides.

`ersatzrs-addon-contract` owns one shared metadata document used for list-level
and item-level provider data. The official Beeld & Geluid and yt-dlp packages
populate that document; they continue to own provider parsing, classification,
and fixtures. ErsatzRS owns validation, localized administration, artwork
materialization, NFO generation, relational projection, and health.

For YouTube-compatible sources, the frequently updated `yt-dlp` executable
performs all provider-sensitive discovery and extraction. The package's shared
Deno helper only validates bounds and translates complete yt-dlp JSON into the
provider-neutral document. It does not scrape YouTube or reimplement yt-dlp,
and ErsatzRS contains no YouTube-specific branch.

The effective document is a field-level merge of the latest provider snapshot
and an operator override patch. Overrides win. A provider change beneath an
override becomes a visible review conflict without replacing the operator
value. New refresh items default to included but remain unpublished until
review. Removed items retain the existing provider tombstone behavior.

Managed NFO files are publication output, not an input prerequisite. ErsatzRS
generates parent metadata from a list header before a parented projection. A
valid external NFO edit is adopted as operator metadata; invalid user XML is
reported and preserved. An empty, untracked placeholder has no metadata to
preserve and may be replaced by the generated document.

One schema-driven metadata editor serves list and item scopes for every add-on.
It supports provider artwork candidates, poster/fanart/thumb roles, uploads,
and manually supplied HTTPS image URLs. Cropping and focal points are deferred.

The provider contract and host behavior are platform-neutral. Official package
acceptance covers POSIX execution on native Linux and WSL plus PowerShell on
Windows. Rust code never selects behavior from an unguarded OS assumption.

## Consequences

- A source URL can be inspected and cleaned before it changes a library.
- The host has one editor and one metadata/NFO pipeline for every provider.
- Refresh can improve untouched fields without destroying hand edits.
- Draft and publication state require additive SQLite/MySQL schema and health
  coverage.
- Older add-ons remain importable through compatibility adapters but may show
  incomplete-metadata warnings.

## References

- [ADR 0068: Keep add-on metadata provider-neutral in the host](0068-addon-metadata-sidecars-and-local-replacements.md)
- [ADR 0073: Classify add-on list entries per item](0073-per-item-classification-and-library-presentations.md)
- [Add-on media-list contract reference](../reference/addon-media-lists.md)
