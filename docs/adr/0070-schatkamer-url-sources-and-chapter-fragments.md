# ADR 0070: Treat Schatkamer hyperlinks and chapters as durable media sources

## Status

Accepted (2026-08-16)

## Implementation

Implemented (working tree, 2026-08-16)

## Context

The official Beeld & Geluid add-on originally accepted series, public lists,
and saved searches through separate POSIX and Windows branches. Enumeration
stopped after 100 pages, list metadata was generic, individual episodes were
playable but could not be imported as a one-item source, and chapter markers
had no durable identity. Repeating URL recognition in the add dialog, managed
media-list importer, Remote Stream scanner, and two package runtimes would make
normalization and compatibility failures likely.

ErsatzTV has chapters attached to media versions, but it has no equivalent for
operator-authored chapters on a remotely scraped episode or independently
selectable chapter fragments. This is an ErsatzRS extension.
ErsatzRS's existing `MediaChapter` table mirrors probed container chapters: it
belongs to one media version, requires an end time, and has no child media-item
identity. It therefore cannot represent an open-ended final marker or the
full-item-to-selectable-fragment relationship without changing probe metadata
semantics.

## Decision

### Amendment: keep source grammar in the add-on (2026-08-17)

The package now owns Schatkamer URL recognition and normalization on both
platforms. ErsatzRS performs only its generic absolute HTTP(S), userinfo, and
declared-host validation before forwarding the canonical URL. This supersedes
the provider-specific host-adapter paragraph below and restores ADR 0068's
provider-neutral host boundary; the paragraph remains as decision history.

One provider-specific host adapter parses Schatkamer URLs before either add-on
list contract is invoked. It recognizes UUID list paths, numeric series or
programme paths, numeric episode paths, and search paths. It upgrades HTTP to
HTTPS, removes fragments, default ports, trailing slashes, and pagination
parameters, decodes supported search filters as Unicode, retains repeated
values, and records unknown parameters for diagnostics. The normalized URL and
source kind are passed to the official package. This is a narrow exception to
ADR 0068's provider-neutral host rule: the module contains only source grammar
and normalization, while HTTP requests, page traversal, metadata extraction,
classification, and playback remain package-owned.

The official package follows result pages until a page contributes no new
stable episode identifiers. It has no fixed page-count ceiling, retains first
occurrence order, logs each page, and treats an incomplete page request as a
failed synchronization instead of publishing a silently partial snapshot.
Series structured data supplies the list title, editorial introduction, and
lead image; episode structured data and embedded provider state supply item
metadata and artwork. POSIX and Windows entrypoints implement the same package
contract without browser automation.

`media-list.list.v3` and `remote-stream.list.v3` add duration, additional
images, parent provider identity, and whole-second fragment bounds. Earlier
versions remain frozen and reject those fields. The host keeps the full episode
item and derives chapter items atomically from validated YouTube-style text.
Each fragment has a stable provider ID based on its parent and start time, a
structured `AddonMediaListChapter` row, and a distinct Remote Stream media item
whose `ParentMediaItemId` points at the full episode. Refresh deletes obsolete
fragment rows and upserts surviving or new fragments by stable identity.
`MediaChapter` remains the canonical projection for chapters discovered inside
local or remote container metadata; the add-on-list record owns authored source
markers and their independently selectable child identity.

Fragment URLs carry `start` and optional `end` query parameters. Both package
runtimes remove those parameters before provider metadata and stream requests,
then apply the start and duration to FFmpeg. The full episode URL remains
unbounded. A final chapter omits `end` only when the episode duration is
unknown.

Provider-supplied markers use the provider-neutral
`remote-stream.item.v2` enrichment contract. The add-on converts its source
metadata to bounded timestamp/title text and whole seconds; the host neither
understands nor rounds provider objects. ErsatzRS stores the provider text
separately, gives a non-empty operator correction precedence, and offers an
explicit restore action. Expansion consumes the enrichment response and does
not invoke the add-on a second time. Chapter capability checks, fragment URLs,
provider IDs, and `addon-chapter://` GUIDs are add-on-neutral.

## Consequences

- Lists, programmes, episodes, and filtered searches share one normalization
  and identity policy across administration and background synchronization.
- Full episodes and chapter fragments are ordinary distinct media items, so
  collections and playlists can select either independently.
- Invalid chapter text cannot publish a partial or incorrectly bounded set.
- The host gains one deliberately isolated provider-specific source-grammar
  module; any broader provider protocol knowledge still belongs in the add-on.
- Adding another Schatkamer path shape requires coordinated host-contract and
  package tests rather than a package-only update.

## Alternatives Considered

- Parse every URL only inside the POSIX and Windows scripts: rejected because
  the two platform implementations and two host invocation paths could disagree
  before scraping begins.
- Store the chapter textarea or generated URLs only: rejected because edits
  could not reconcile fragment identity or parentage predictably.
- Split one episode into chapters instead of retaining the full item: rejected
  because it removes a valid source and prevents independent selection.
- Use browser automation: rejected because current pages expose structured
  metadata and ordinary HTTP pagination.

## References

- [ADR 0065: Managed Remote Stream playlists](0065-managed-remote-stream-playlists.md)
- [ADR 0067: Move provider lists into repository-installed add-ons](0067-move-provider-lists-into-addons.md)
- [ADR 0068: Keep add-on metadata provider-neutral in the host](0068-addon-metadata-sidecars-and-local-replacements.md)
- [Add-on media-list contract reference](../reference/addon-media-lists.md)
