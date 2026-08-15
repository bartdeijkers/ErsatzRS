# ADR 0068: Keep add-on metadata provider-neutral in the host

## Status

Accepted (2026-08-15)

## Implementation

Partial — generic contracts, persistence, validated pair publication and
failure recovery, NFO fidelity, observable search/XMLTV classification,
replacement projection, provider tombstone recovery, and hydrated browser
acceptance exist in the working tree. Live MySQL migration proof and packaged
Windows add-on runtime evidence remain open.

## Context

Storage-backed add-on lists materialize Remote Stream definitions that are then
scanned like other Local media. Keeping descriptive metadata only in generated
YAML made those definitions unlike other Local media and tied metadata parsing
to the Remote Stream adapter. It also left no durable list position when a
provider retained an item but could no longer play it, even though an operator
might own an appropriate local replacement.

More add-ons will target providers, catalogues, and languages that ErsatzRS
does not know. Provider classification rules, translated package text, icons,
and other language-specific assets cannot become host fixtures or branches
without turning the add-on boundary back into built-in integrations.

Kodi, Jellyfin, and Emby share broadly compatible Kodi-style NFO roots and
fields. Plex does not provide an equivalent native, general-purpose NFO
contract, so Plex compatibility must not be claimed by inventing a Plex-only
sidecar dialect.

## Decision

`media-list.list.v2` and `remote-stream.list.v2` extend the generic NDJSON
contracts with stable provider identity, explicit availability, an optional
reason, content kind, and richer descriptive metadata. The contract contains
no provider names, language-specific keyword tables, or package assets.

The add-on owns source interpretation. It decides whether an item is a movie,
television episode, music video, or other video from provider metadata and
emits the generic content kind. If it cannot decide, it emits `auto`. An
operator may override the content kind for an imported list URL in the generic
Add-on Lists administration surface; that override wins for every generated
sidecar from the list. Classification keywords, provider ratings, localized
labels, icons, and provider fixtures remain in the add-on package and its own
repository tests.

The host owns files below managed list storage. It invokes the add-on, validates
the generic v2 records, and atomically writes a same-stem operational YAML file
and descriptive `.nfo` sidecar. YAML retains only playback and synchronization
data. NFO uses UTF-8, LF line endings, deterministic ordering, and these roots:

- `episodedetails` for television episodes;
- `musicvideo` for music videos; and
- `movie` for movies and other videos, because the interoperable Kodi-style
  vocabulary has no separate other-video root.

Standard Kodi-style elements carry titles, plot, dates, runtime, rating,
show/season/episode identity, genres, tags, studios, credits, actors, artists,
unique IDs, and artwork. Provider fields without a standard equivalent use the
namespaced `urn:ersatzrs:nfo:1` extension. Readers that do not understand the
extension can ignore it. This is the compatibility baseline for ErsatzRS,
Kodi, Jellyfin, and Emby; Plex interoperability depends on a Plex metadata
agent and is not part of this sidecar contract.

The versioned managed-state sidecar owns both generated files. Refresh, keep,
trash, recovery, and intact-cache checks treat YAML and NFO as one unit. The
scanner reads NFO as the descriptive authority and YAML as the operational
authority. Both files are encoded and the NFO is parsed before either is
published. A second-file publication failure restores the previous YAML, while
state-v2 content hashes reject and heal a mixed generation after interruption.
A v1 state is accepted and healed to v2 on the next scan.

An add-on list item records `available`, `unavailable`, or `unknown`.
`unknown` is deliberately not treated as unavailable. An explicitly
unavailable item remains visible in provider order and may have one
administrator-selected local replacement consisting of an exact media-item
and media-version pair. The mapping may be prepared while provider playback is
available, but it activates only for explicit `unavailable`. Only an available
Local video version with a real
non-attached video stream is selectable. Generated playlists use that pinned
local version and its actual local collection type only while the provider item
is unavailable. Available or unknown provider state never activates the
fallback, and provider recovery immediately restores the provider item.

Replacement target IDs deliberately have no foreign keys. A deleted or
unavailable local target remains visible as a stale mapping in health and the
admin UI instead of silently erasing operator intent. A Local scan rematches
all lists so repaired targets and provider recovery are reflected promptly.

A provider refresh that omits an existing item retains it as an unavailable
provider-neutral tombstone after the current snapshot, rather than deleting
it. This preserves its stable row and local replacement across incomplete
pagination, permission changes, and temporary catalogue gaps. Returning the
same provider ID updates that row in place, and provider playback wins again.
Deleting the imported list remains the explicit way to remove its tombstones
and replacement intent.

The host localizes only its provider-neutral administration labels in every
supported host culture. Add-on display names, descriptions, provider help,
classification resources, icons, and future language-specific assets remain
package-owned and are selected from the manifest locale map with the existing
fallback rules.

## Consequences

- New providers and languages can implement the same contracts without a host
  release or provider-specific host tests.
- Local scans and external media managers see the same descriptive NFO model
  as other Local media, while playback configuration remains separate.
- Missing provider entries retain identity and order and can be scheduled from
  an exact local file without pretending an ambiguous provider result is down.
- The host has more generic persistence, health, and administration code, and
  managed synchronization must keep two generated files consistent.
- Plex users need an appropriate metadata agent if they want Plex itself to
  consume these NFO files; ErsatzRS does not emit a misleading Plex dialect.

## Alternatives Considered

- Generate NFO files inside each add-on: rejected because add-ons do not own
  the host-managed storage transaction and would duplicate naming, atomicity,
  cleanup, and compatibility behavior.
- Put provider classification and localized assets in ErsatzRS: rejected
  because every new provider or language would require a host release.
- Keep rich metadata in YAML: rejected because it preserves a Remote
  Stream-only metadata path and does not match Local sidecar precedence.
- Use a replacement whenever availability is not `available`: rejected because
  `unknown` includes temporary or incomplete enumeration and must not hide a
  usable provider item.
- Cascade-delete replacement mappings: rejected because silent deletion would
  conceal stale operator configuration from health and administration.

## References

- [ADR 0065: Managed Remote Stream playlists](0065-managed-remote-stream-playlists.md)
- [ADR 0066: Repository-backed add-ons](0066-repository-backed-add-ons.md)
- [ADR 0067: Move provider lists into repository-installed add-ons](0067-move-provider-lists-into-addons.md)
- [Add-on media-list contract reference](../reference/addon-media-lists.md)
- [Remote Stream playlist manifest reference](../reference/remote-stream-playlist-manifests.md)
