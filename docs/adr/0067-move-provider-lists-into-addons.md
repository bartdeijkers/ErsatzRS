# ADR 0067: Move provider lists into repository-installed add-ons

## Status

Accepted (2026-08-14)

## Implementation

Implemented (working tree, 2026-08-14)

## Context

ErsatzRS inherited Trakt list ownership from ErsatzTV: a built-in HTTP client,
Trakt-specific domain types and tables, a process-wide `TRAKT__CLIENTID`
configuration value, and dedicated admin routes. That boundary conflicts with
ADR 0066. Trakt is an independently released provider integration with its own
network permission, configuration, readiness, and failure behavior, just like
the Remote Stream providers already moved into the official add-on repository.

Beeld & Geluid exposes several kinds of durable Schatkamer hyperlinks: series,
public shared lists, and saved searches. Those links select ordered programmes
but previously had a separate Remote Stream playlist editor. Retaining a
Trakt-shaped host feature for one provider while adding another provider-shaped
surface would duplicate lifecycle, matching, generated-playlist, and admin
behavior.

The legacy Trakt rows can contain external-provider snapshots and generated
system playlists. The maintainer explicitly requires a clean migration rather
than importing or retaining those rows under a generic name.

## Decision

The host owns provider-neutral `media-list.list.v1` and v2 capabilities. An enabled
add-on receives one source hyperlink and emits bounded NDJSON: exactly one list
record followed by ordered movie, show, season, episode, or Remote Stream records. ErsatzRS
validates size, count, identity, and rank bounds before it persists or matches
anything. Local metadata matching and optional generated-playlist maintenance
remain host responsibilities.

Trakt API access moves entirely into `org.ersatzrs.addon.trakt` in the official
add-on repository. Its client ID is a required `secret_reference` setting. A
schema-v3 manifest may suggest a safe environment-variable name or file path;
the settings UI may prefill that location, but the database continues to store
only reference metadata. The Trakt package suggests the established
`TRAKT__CLIENTID` environment-variable name. Neither the manifest nor ErsatzRS
stores the resolved client ID.

Beeld & Geluid implements the same capability alongside its Remote Stream
capabilities. Series, `/lijst/<uuid>`, and `/zoeken?...` Schatkamer links can
therefore be managed through the add-on list surface and can still be used as
Remote Stream playlist sources where applicable.

Schema v3 also permits a package to declare `media_list_storage`. The section
links one manifest-rendered path setting to a safe profile-relative default and
a stable local-source name. The host resolves that default below the ErsatzRS
profile (`beeldengeluid_media` or `yt-dlp_media` for the official packages),
creates the source idempotently when the add-on is enabled, and never asks the
add-on process to edit the database.

For a storage-backed provider, each imported list owns one filesystem-safe
subfolder whose name is also the local library name. ErsatzRS writes a managed
Remote Stream playlist manifest in that folder and registers it as a Remote
Streams local library under the add-on-owned media source. The established ADR
0065 scanner then enumerates, materializes, and refreshes ordinary YAML Remote
Stream definitions. Deleting an add-on list removes its database registration
but retains its files
for recovery. Disabling its add-on retains both registration and files; the
host never recursively deletes an operator-configurable media path.

The forward SQLite and MySQL migration is deliberately destructive. It deletes
legacy Trakt-generated system playlists and page-size configuration, drops
`TraktListItemGuid`, `TraktListItem`, and `TraktList`, and creates new
`AddonMediaList`, `AddonMediaListItem`, and `AddonMediaListItemGuid` tables.
No Trakt row, identifier, setting, or generated playlist is copied into the new
schema. Administrators install/configure the add-on and add desired links
again.

The canonical admin routes are `/media/addon-lists` and
`/media/addon-lists/{id}`. Search uses the provider-neutral `addon_list:<id>`
field. The old Trakt route and built-in HTTP client have no compatibility alias.
The navigation groups Add-on Lists under Media Sources immediately after Local
because storage-backed lists create local libraries.

## Consequences

- Trakt releases, network access, readiness, and client configuration are
  independently owned by its add-on package.
- The host retains one tested local matching and playlist lifecycle for every
  provider instead of duplicating it per integration.
- Administrators can manage Schatkamer programme selectors with the same list
  workflow as Trakt links.
- Beeld & Geluid and yt-dlp lists become separately scannable, file-backed
  Remote Streams libraries without Python or permanent video downloads.
- Upgrading irreversibly removes existing Trakt lists, items, GUIDs, page-size
  preference, and generated system playlists. Re-adding a list starts from a
  fresh provider snapshot.
- A required secret reference can have a safe suggested location without
  weakening the no-literal-secret persistence rule.
- Add-on list output is not a general provider API: it is a bounded one-shot
  enumeration contract and remains subject to add-on process permissions and
  lifecycle controls from ADR 0066.

## Alternatives Considered

- Keep Trakt built into ErsatzRS and add only Beeld & Geluid to the shared UI:
  rejected because provider configuration and HTTP behavior would still bypass
  the add-on lifecycle.
- Rename and import the existing Trakt tables: rejected because the requested
  migration explicitly requires that existing Trakt tables and data do not
  remain.
- Store the client ID as a normal add-on setting: rejected because database
  backups, admin reads, and diagnostics would then contain credential material.
- Read `TRAKT__CLIENTID` directly in the host and forward it implicitly:
  rejected because it preserves hidden process-wide provider coupling and
  bypasses visible add-on configuration.
- Add a second Beeld & Geluid-specific list manager: rejected because source
  links, matching state, refresh cadence, and generated playlists are the same
  host concepts.

## References

- [ADR 0065: Managed Remote Stream playlists](0065-managed-remote-stream-playlists.md)
- [ADR 0066: Repository-backed add-ons](0066-repository-backed-add-ons.md)
- [Add-on media-list contract reference](../reference/addon-media-lists.md)
- [Add-on package reference](../reference/addon-packages.md)
- [Trakt list parity record](../progress/parity/trakt-lists.toml)
