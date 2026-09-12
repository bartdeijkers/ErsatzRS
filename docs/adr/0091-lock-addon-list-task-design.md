# ADR 0091: Lock the add-on list task design for production adaptation

## Status

Accepted (2026-09-09) — the maintainer approved the current artifact and requested
its strict adaptation under AMM-N-01B.

## Implementation

Partial — reopened on 2026-09-12 for contradictory preparation readiness,
hidden preparation failures and misleading post-create failure navigation.
Those A02/A03/A05 defects and the subsequently authorized A06-A10 guidance,
validation-label and accessibility corrections are now verified locally.
The focused joint walkthrough also fixes stale import landing, bulk inclusion
and recovered-failure feedback. Narrow filter readability, retained-versus-saved
bulk feedback and detail-progress labelling remain recorded UX follow-ups;
full responsive/accessibility closeout is not inferred from the focused pass.
The accepted design remains unchanged; the owning parity debts track repair.
Previously, live publication readiness was restored under AMM-N-01B
without reloading or replacing unsaved inputs. Production task routes, native draft retention,
transactional saves and immutable artwork retain their functional evidence. The
2026-09-10 item-form correction restores the locked editing/tools hierarchy,
explicit numbering, concise option guidance and restrained disclosure boundaries.
Milestone `amm-n-01b` owns current validation; N-02/N-03 remain separate and open.
The accepted design and its frozen reference are unchanged.

## Context

The add-on list editor combines configuration, item review, numbering, provider
options, publication and recovery in one large view. Repeated visual and
no-JavaScript reviews produced the approved artifact in `plans/admin-ux-organize/`.
It clarifies Save versus Publish, keeps routine settings visible, limits optional
disclosures and stacks access settings on wide displays. Design iteration is over.

ErsatzTV's `ErsatzTV/Pages/RemoteStreamList.razor` and
`ErsatzTV/Pages/CollectionItems.razor` provide related browsing and selection
behavior, but no repository-installed add-on draft editor. ADRs 0075, 0078,
0079 and 0085 own that ErsatzRS extension. This decision changes its presentation
and editing flow, not provider, publication or playback semantics.

## Decision

1. Freeze the artifact through the SHA-256 manifest described in
   [the design lock](../../plans/admin-ux-organize/LOCK.md). Its rendered content,
   task order, responsive geometry and interaction states are the reference.
2. Implement Items & review, List settings, Playback & access, Activity & recovery,
   item review and artwork in the real `/media/addon-lists` route family.
   Preserve existing list/add/item URLs and every existing capability, including
   conditional metadata, provider options, full media details and recovery.
3. Retain the real admin shell, localization, icons, authorization, application
   services and durable jobs. The prototype shell and Python adapter are
   demonstration tools. ADR 0089's Support navigation remains authoritative.
4. Keep task forms usable without JavaScript. Save persists the named task;
   Publish uses the exact saved revision. Navigation, paging, validation and
   publishing must not silently discard newer unsaved input. Bulk edit selection
   remains distinct from inclusion in publication.
5. Adapt fixtures to authoritative DTOs, schema-defined options and localized
   text. Existing controls absent from a scenario belong in their matching task
   group using its locked patterns. Map and verify those additions; do not
   restore the monolithic layout or hide ordinary settings in more disclosures.
6. Further design changes require an explicit maintainer decision and a new
   lock revision. Never resolve failed visual checks by replacing the reference.

### Artwork handles

Artwork uploads use immutable content handles in the existing upload table.
The unique key includes the content hash as well as list, item scope and role;
existing IDs, image bytes and foreign-key ownership remain valid. Re-uploading
identical content reuses its handle. Upload staging does not change metadata or
the saved revision. The native draft carries the handle through navigation;
Save validates ownership and commits all selected artwork under the task fence.
Legacy upload callers still select an image atomically, using the same immutable
storage. This prevents an upload from replacing bytes referenced by another
open task. Stored bytes remain included in existing disk-usage accounting and
are removed by the existing list/item cascade; no media eviction policy is added.

## Consequences

Production has an objective reference but needs real draft retention, coherent
grouped saves and native adapters. Existing item saves write metadata and chapters
separately; their consistency must be proven before completing Save item.

AMM-N-01B runs after N-01 and before N-02. This decision did not change N-02 or
N-03 acceptance. The later 2026-09-11 amendment in ADR 0085 replaces N-02's
rollback scope with a failed-and-stale tool warning; it does not change this
locked UI design. N-03 retains its remaining integrated acceptance. The owning parity
debt stays open until production acceptance. Prototype reports do not prove
persistence, hydration, localization or runtime parity.

## Alternatives Considered

- Continue redesigning during implementation: rejected by the explicit lock.
- Restyle the single editor without changing task boundaries: retains the
  demonstrated navigation and mental-load problems.
- Ship the Python preview or its simulated operations: existing Rust use cases
  and durable state remain authoritative.

## References

- [AMM-N-01B execution plan](../../plans/amm-n-01b-addon-list-design-adaptation.md)
- [Canonical acceptance and evidence](../progress/milestones.toml)
- [Owning parity record](../progress/parity/addon-metadata-management.toml)
- [Add-on lifecycle contract](0085-broaden-addon-list-lifecycle-and-provider-options.md)
- [Support navigation](0089-collapse-support-drawer-group-into-system-route-tabs.md)
