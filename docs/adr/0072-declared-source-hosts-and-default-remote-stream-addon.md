# ADR 0072: Route operator source URLs to add-ons by declaration

## Status

Accepted (2026-08-19)

## Implementation

Partial — the host contract, resolver, administration surface, and health check
are implemented (working tree, 2026-08-19). The official Beeld & Geluid package
does not declare `default_source_hosts` yet, so Schatkamer URLs currently
resolve through the configured default remote stream processor. The channel
package that motivates the resolver is deliberate future work.

## Context

Adding an add-on media list required choosing an add-on before the source URL
was even typed. The choice was mandatory, the dialog defaulted to whichever
provider sorted first, and the URL — the only thing that determines which
add-on can process the source — came second. An operator who picked the wrong
add-on learned about it only when enumeration failed.

The host already knows enough to decide. Every installed package stores its
manifest, and `validate_operator_source_url` (ADR 0068) already compares a URL
against `permissions.network_hosts` before enumeration. What was missing is a
statement of intent: `permissions.network_hosts` records which hosts a package
is *permitted to contact* — for Beeld & Geluid that includes the media CDN
`sk-video.cdn.beeldengeluid.nl` — and a package may legally declare `*`.
Permission is not a claim to handle a source.

This matters beyond one dialog. A planned channel package uploads a `.zip`
holding artwork, graphics elements, a channel name, and a feed URL whose items
are Beeld & Geluid, YouTube, or other add-on sources. Nobody fills in a form for
those items, so routing has to be a host capability rather than a dialog
convenience.

## Decision

### Packages declare the hosts they handle

Manifest schema v4 adds an optional `default_source_hosts` list. It is a
narrower statement than `permissions.network_hosts`, and the contract enforces
that: every entry must be covered by the permission declaration, no entry may be
`*` or a whole top-level domain, and the field is rejected outright by earlier
schema versions. A package that claimed a host it may not contact would win
auto-selection and then be refused at enumeration.

`match_declared_source_host` ranks candidates by specificity: an exact host
outranks a wildcard, a longer wildcard suffix outranks a shorter one, and a
genuine tie returns `Ambiguous`. The host does not guess between equal claims;
the operator picks.

### The host resolves, provider-neutrally

`ersatzrs_infra::addon_source_routing::resolve_source_addon` compares one URL
against installed, enabled packages that declare a required capability family.
It reads manifests through `list_installed_addons` — one query, no process
launch, no content hashing — so it is cheap enough to run while the operator is
still editing a URL. It contains no provider protocol knowledge, preserving the
ADR 0068 boundary.

### An administrator-configured fallback

`addons.default_remote_stream_addon` names the package used when nothing claims
a host. It applies only to bounded HTTP(S) URLs, and only when the chosen
package's own `permissions.network_hosts` admit the host. It is surfaced under
Settings > Add-ons > Sources, and the add-on health check reports a configured
default that is missing, disabled, or no longer able to play remote streams.

### Resolution happens when the list is written

The add-on selection in the add dialog is optional and sits below the URL. When
it is left empty, the same web-layer seam that both the interactive dialog and
the plain HTML form post reach resolves it and stores the result. The choice is
visible before saving and always overridable.

The stored add-on is not final. `change_addon` moves a saved list to a different
package: it enumerates through the new package first, refuses a move that would
collide with an existing `(AddonId, ProviderId)` identity, re-points the row, and
then re-imports. Because item identity is provider-scoped, items are re-derived
rather than migrated, and the operator confirms that before the move runs.

### Playback is deliberately untouched

`resolve_remote_stream_source` remains the single place where a Remote Stream
definition without `addon:` means "direct URL". Applying the default there would
replace the URL input with `pipe:0`, discard the `-reconnect*` tuning that
`remote_url_input_options` adds for HTTP sources, and hand `rtsp` sources to a
package that cannot fetch them. Every existing direct-URL definition keeps its
current behavior.

## Consequences

- A package states which sources it handles, separately from which hosts it may
  contact, and the host can route without provider knowledge.
- Schema v4 manifests are refused by hosts that still cap at v3, so the host
  change must ship before the packages that use the field.
- Operators can add a claimed URL without naming an add-on, and can move a list
  to a different add-on later at the documented cost of re-derived item metadata.
- The future channel package can reuse one tested resolver for feed items rather
  than growing its own provider table; metadata gaps stay the job of the existing
  `remote-stream.item.v2` enrichment contract from ADR 0070.
- Two packages that claim the same host equally block auto-selection instead of
  silently racing on installation order.

## Alternatives Considered

- Reuse `permissions.network_hosts` for auto-selection: rejected because it
  conflates permission with intent, would let a package declaring `*` claim every
  source, and would have matched the Beeld & Geluid CDN host as a list source.
- Resolve at scan or playback instead of at write: rejected because it changes
  the behavior of existing direct-URL Remote Streams and moves a decision the
  operator should be able to see and override into the playback path.
- Apply the configured default to every add-on-less definition: rejected for the
  same reason; it would need an `addon: none` opt-out to avoid regressing sources
  that work today.
- Implement the change-add-on action as delete-and-re-add: rejected because it
  discards the list identity, its generated playlist, and its managed library
  registration for what the operator experiences as an edit.

## References

- [ADR 0066: Repository-backed add-ons](0066-repository-backed-add-ons.md)
- [ADR 0067: Move provider lists into repository-installed add-ons](0067-move-provider-lists-into-addons.md)
- [ADR 0068: Keep add-on metadata provider-neutral in the host](0068-addon-metadata-sidecars-and-local-replacements.md)
- [ADR 0070: Schatkamer URL sources and chapter fragments](0070-schatkamer-url-sources-and-chapter-fragments.md)
- [Add-on package reference](../reference/addon-packages.md)
