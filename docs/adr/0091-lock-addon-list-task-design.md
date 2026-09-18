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
Maintainer-authorized refinement (2026-09-18, phase 1 of
`plans/addon-list-workspace-ux-refinements.md`): on Items the publication
summary sits inside the heading beside the title, finished preparation and
successful publication history appear only on Activity & recovery, and the
Save button is the visible draft indicator (the status text remains for
screen readers only). Phase 2 adds fixed S/E row
symbols with localized accessible names, a visible unsaved-row marker, and a
separate new-tab link beside the title; the title itself remains the
draft-retaining submit button. The Items heading also edits the list title in
place as an Items field under the list metadata fence. Phase 3 splits the
numbering tools into Items to change (scope, range, length filter), Number
episodes and Detect seasons panels; season detection uses its own first season
and no longer reads the manual Count. A pending bulk proposal is reviewed
inside the tools with one Apply action, which is Save changes for the Items
task and therefore lists every value that save writes, and Cancel preview,
which restores the draft from before the proposal. Phase 4 adds an applied
filter summary with Clear filters, First/Last page and a page position, a
progressive-enhancement script that applies a pointer-picked filter category
or page size through the same native submit buttons (keyboard changes wait for
Enter or the button), a Save item action in the item navigation row and a
real Save item action beside a chosen replacement. The frozen reference is not edited; these are recorded
deviations from it.

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

## Addendum: Playback save conflicts (2026-09-13)

The accepted dogfood correction narrows the conflict fence only for the
Playback & access task. A submitted typed baseline must still match the saved
delivery mode, cutoff timestamp and identity, disk warning, bound option schema
and list values, and installed add-on identity. Metadata-only global revision
movement may then coexist with Save. Other tasks and older forms without the
baseline retain their strict revision and metadata checks.

Option edits are validated against the current revision. The repository compares
the complete baseline, checks the enabled installation and delivery capabilities,
and enforces publication ownership in the same write transaction as the grouped
save. A change after option validation causes a conflict; prepared values must
never be relabelled with an unvalidated revision. Save and Undo use the revision
actually committed. Save does not publish or alter the retained playback snapshot.

The editor retries a moving initial read once. Ancillary failures retain the form
and submitted input, explain the incomplete read, and disable affected actions
until the necessary information is available. Publication admission remains an
authoritative submission-time check. This changes neither provider pacing nor
the SponsorBlock default and introduces no migration or workflow framework.

## Addendum: Subtitle check reuse across saved revisions (2026-09-18)

Native Windows dogfood showed every metadata-only save, such as renumbering
episodes, re-running one provider subtitle check per available item (156 yt-dlp
calls, about 15 minutes) with byte-identical input and an identical outcome. The
maintainer approved reuse. Preparation stays exact-revision: each saved revision
still owns its own completed subtitle receipt. Automatic admission records an
earlier completed outcome as that receipt when the persisted operation input,
including source, add-on package identity, tool version, options and requested
languages, is identical and its subtitle artifacts still validate.

Reuse is unbounded in time. Only a manual list refresh requests a recheck: it
records `SubtitleRecheckRequestedAt`, and outcomes completed before that instant
are no longer reused. Scheduled refreshes reuse outcomes; items new to the list
have no earlier outcome and are always checked. Explicit manual admission of a
subtitle task still asks the provider.

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
