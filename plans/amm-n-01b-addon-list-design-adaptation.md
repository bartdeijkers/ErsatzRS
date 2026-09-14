# AMM-N-01B: Adapt the locked design to production Add-on Lists

The maintainer locked the artifact on 2026-09-09 and requested this implementation
slice. [ADR 0091](../docs/adr/0091-lock-addon-list-task-design.md) owns the decision;
[LOCK.md](admin-ux-organize/LOCK.md) and its manifest own the reference. Milestone
`amm-n-01b` in [milestones.toml](../docs/progress/milestones.toml) owns status,
acceptance and evidence. This plan owns source mapping and implementation order.

Implementation, item-form correction and live publication restoration are locally complete.
This source/capability map remains linked by the frozen reference; milestone
`amm-n-01b` owns current acceptance and evidence. The completed focused correction
plan has been pruned. No publication or N-02/N-03 completion is implied.

## Outcome and scope

Replace the production add-on-list editor with the locked task layout, backed by
real data and complete operations in SSR/no-JavaScript and hydrated workflows.
Work after AMM-N-01, before AMM-N-02; include the result in N-03 closeout.

Include the `/media/addon-lists` overview and add entry points, list editor, item
review, artwork, provider options, numbering/bulk tasks, readiness, source/revision
operations and recovery. The artifact specifies the editor most closely: retain
overview/create functionality and align its controls and navigation without
inventing a dashboard or wizard.

Exclude global shell redesign, new frameworks, shipping Python, production
fixtures, provider-protocol or playback-algorithm changes, tool rollback,
HLS Direct expansion and the withdrawn retention/eviction/admission/concurrency
program. Preserve native yt-dlp, frozen publication, human subtitle selection and
Health signals. This plan does not authorize push, CI publication or release.

## Source and visual map

Read the lock first. The [preservation inventory](admin-ux-organize/inventory.md)
records capability traceability; older layout descriptions are subordinate to
locked HTML. It does not prove that every production field appears in a scenario.

Inspected upstream references are
`$ERSATZTV_SRC/ErsatzTV/Pages/RemoteStreamList.razor` and
`$ERSATZTV_SRC/ErsatzTV/Pages/CollectionItems.razor` for browsing and selection.
There is no upstream repository-installed add-on draft equivalent. Existing
ErsatzRS ADRs and application contracts govern this extension.

| Surface | Existing owner | Adaptation |
| --- | --- | --- |
| Routes | `crates/ersatzrs-web/src/admin/routes.rs`, `crates/ersatzrs-web/src/host/router.rs` | Preserve overview/add/list/item URLs; register task routes and native POST adapters. |
| Overview/create | `admin/pages/media/addon_lists.rs`: `AddonMediaListsPage`, `AddonMediaListsIsland`, `AddAddonMediaListPage`, `AddAddonMediaListPageIsland` | Preserve search/paging, eligibility, import, match, deletion and global disk warning; align controls. |
| List editor | `admin/pages/media/addon_lists/workspace.rs` and `workspace/views.rs`; existing `load_addon_media_list_editor_initial` | `AddonListWorkspacePage` renders the four real task views from authoritative data/readiness. |
| Item review | `admin/pages/media/addon_lists/workspace/views/detail.rs` | Group full metadata, chapters, inclusion, classification, options and fixed navigation around Save item. |
| Commands/native forms | `admin/server_fns/addon_media_lists.rs`, `api/addon_media_lists.rs`; `crates/ersatzrs-app/src/lib.rs`, `commands.rs` | Native forms and islands invoke the same typed commands, validation and concurrency fences. |
| Provider options | `admin/pages/media/addon_list_options.rs`, `admin/server_fns/addon_list_options.rs`, `api/addon_list_options.rs`; `crates/ersatzrs-app/src/addon_media_list_options.rs` | Embed schema-driven controls in the owning task; retain preview, mixed values, inheritance, activation and undo. |
| Persistence | `crates/ersatzrs-infra/src/repositories/addon_media_list.rs`, `addon_media_list_lifecycle.rs`, `addon_media_list_reset.rs`; `crates/ersatzrs-infra/src/addon_media_list*.rs` | Reuse drafts/revisions, sidecars and durable coordinators; add only grouped-write/selection capabilities needed by the locked flow. |
| Validation | Application commands, core repository contracts, `crates/ersatzrs-core/src/media_chapters.rs` | Preserve NFO ETags, revision/generation checks, paired numbering, chapter bounds, derived-item restrictions and provider schema validation. |
| Full media information | `admin/pages/media/remote_stream_detail.rs` | Keep the canonical detail destination with video/audio/subtitle tracks, languages, versions and chapters. |
| Presentation/text | `admin/ui/primitives/`, `admin/layout/`, page CSS modules, `admin/languages/`, `admin/i18n.rs` | Reuse production shell/icons/theme and primitives with narrowly scoped variants; localize every culture. |

Paths starting `admin/` or `api/` are under `crates/ersatzrs-web/src/`.
Keep presentation modules beside `addon_lists.rs`; split its monolithic island
by task instead of enlarging it. Scope CSS to these pages; no global overrides.

## Routes and task boundaries

| Production URL | Locked reference | Responsibility |
| --- | --- | --- |
| `/media/addon-lists` | Shared visual language | Overview, page size, add/match/delete and global disk warning. |
| `/media/addon-lists/add` | Shared form language | Existing import inputs, eligibility and errors; enter Items on completion. |
| `/media/addon-lists/{id}` | `index.html` | Items & review, filters, selection, numbering, readiness and Publish saved changes. |
| `/media/addon-lists/{id}/settings` | `settings.html` | Source/cadence, list identity/metadata and contextual source actions. |
| `/media/addon-lists/{id}/playback` | `options.html` | Delivery, access/segments, provider options and list disk warning. |
| `/media/addon-lists/{id}/activity` | `activity.html` and recovery states | Jobs, source/reconcile, diff/history, restore and diagnostics. |
| `/media/addon-lists/{id}/items/{item_id}` | `item-*.html` | Item review, fixed Previous/Next, tools and return-to-row context. |
| `/media/addon-lists/{id}/artwork` | `artwork.html` | List artwork; an optional validated `item_id` scopes item artwork and return context. |

Task subroutes are additions, not existing endpoints. Keep existing URLs and API
callers working; map known legacy query parameters and section anchors to the
owning task through compatible anchors/redirects. Accept only known local return
destinations. Search/filter/page/page-size are read-only URL state. Unsaved data,
private source values and credentials never belong in query strings.

## Editing and persistence contract

Represent unsaved task input, saved draft revision and published snapshot as
three distinct states. Use the locked Save/Publish vocabulary and summaries.

- Carry sparse edits keyed by real item ID, selection and originating list/task
  context through paging, tabs, item navigation, previews and artwork. Native
  forms use a bounded typed draft envelope, following the existing native
  option-command encoding pattern. Hidden fields are untrusted: validate every
  value, ownership, ID and fence. Do not copy Python's session pool or require
  localStorage for retention.
- Navigation from an editable native form submits that form using POST, even
  when the server last rendered it clean: without JavaScript, links cannot detect
  edits made since rendering. Merge submitted fields with the carried draft
  envelope and render the destination without saving. Read-only navigation uses
  GET. Keep POST actions at the destination route, preserve Back/reload behavior
  and redirect after a write only when remaining drafts will survive. Determine
  explicit measured
  payload limits; show an actionable size error without discarding input or
  silently capping All. Avoid hidden copies of every item's full metadata.
- Save validates the whole task before committing it coherently. Current item
  metadata/chapter writes are sequential; options and disk warnings have
  separate handlers. Add composite application commands and repository
  coordination where required. Chaining existing endpoints is not proof of
  atomicity. Preserve NFO ETags and DB revisions, stage file changes through
  existing ownership mechanisms and prove failure/rollback behavior.
- Distinguish unchanged fields, explicit clears and inherited defaults. Saving
  visible fields must not erase metadata absent from the scenario. Discard
  clears only that task's unsaved edits; Save leaves other task drafts intact.
- Publish calls the existing durable coordinator with the exact saved revision.
  It preserves newer unsaved values without saving/publishing them. Stale tabs,
  concurrent refresh and repeat submissions must not overwrite newer state or
  cause another unintended mutation. Keep leases, job identity, cancellation
  and retry eligibility authoritative.
- Return submitted values, a focusable error summary and linked field errors.
  Reveal affected optional sections. A stale task must show a conflict and a
  review/reload path, never false success.
- Draft envelopes contain only existing `SecretReference` values, not raw
  secrets/cookies. Artwork uses the real bounded multipart/upload-handle
  contract, not filename-only prototype behavior.

These are required outcomes, not existing implementation claims. If a concrete
gap requires architecture beyond existing boundaries, record it before expanding
scope; do not weaken coherent-save or draft-retention acceptance.

## Capability preservation

| Task | Required behaviors, including conditional branches |
| --- | --- |
| Items/numbering | Stable provider order; unknown metadata; unavailable, excluded, conflicted, pending-removal/reappeared states; paired S/E validation, Unnumbered, legacy 0/0 and season-zero specials; existing algorithms, occupied-number skipping, missing-only counts and date-gap evidence. Add the artifact's exact selected-ID, bounded range and season-only modes without touching unselected extras. |
| Paging/bulk | 10/25/50/100/All on overview and item controls, existing saved preferences where applicable and 10 when unset; filter-relative totals, page clamping, no duplicate/omitted rows, off-page drafts and selection. All means every matching item. Kind/liveness/provider bulk preserves scope, mixed values, preview, fences and undo. Selection is not inclusion. |
| List settings | Full title/original/sort title/year/summary, genres/tags/languages, studios/directors/writers/artists/people; all presentation kinds and migration impact; read-only source/folder, compatible add-on change, cadences including unsupported Adaptive, detail policy, playlist generation and safe-additions policy. |
| Playback/access | Three delivery modes and saved cutoff, frozen published playback, schema-driven package/list/item precedence, explicit equal-to-default overrides, reset and activation undo; access target/test/cancel/retry and capability guidance; segments in every supported mode and advisory disk warnings without new retention/admission behavior. |
| Item/artwork | Inclusion/numbering/kind/movie role, unknown/finite/live and conflicts; full metadata including original title/year/release date/credits/languages, every artwork role/candidate/upload and provenance; provider chapter fallback/restore and derived-item restrictions; details request/retry, full media link, local replacement search/select/clear and version validity. |
| Activity/source | Saved/published identifiers and change categories; job phase/counters/timestamps/truncation; redacted diagnostics and eligible Retry/Restore with stale-generation refusal; source probe, reset/provider-change impact, non-destructive reconcile/two-observation removal; revision compare, retained/pinned eligibility and acknowledged restore. |

Map every existing command, disabled reason and validation branch to a Rust
handler and browser scenario before removing its old rendering. Add fields
absent from a fixture to the matching locked group. No synthetic omission grants
permission to delete a capability, conceal it or replace it with mocked success.

## Hydration and visual adaptation

The routed task wrapper is SSR `#[component]` `AddonListWorkspacePage`. Its
populated native forms work in both interaction modes; the existing application
shell hydrates. The previous editor/item islands are no longer route owners.
Task interactivity does not depend on client-only effects; resolve data before prop-bearing islands cross Suspense,
keep island DOM stable and dialogs mounted, and follow TEST.md. Both interaction
modes call the same application commands.

Port locked structure and geometry into scoped CSS: headings/statuses, tabs,
item columns/mobile records, action hierarchy, spacing, sticky controls and
disclosure placement. Use matching production theme tokens/icons. Access and
segments stay vertical at at most 48rem even at 3840 pixels. Previous/Next slots
remain fixed. Do not copy the synthetic shell or its mobile adapter.

Compare matching fixtures, viewport, theme, fonts and UI state. Record permitted
differences for real data/localization/shell; unexplained content-layout drift is
a defect. Production captures cannot replace locked screenshots. Render states
without screenshots from locked HTML in a disposable copy for comparison.

## Implementation checkpoints

1. **Complete mapping:** verify the lock; capture real-host fixture behavior;
   map every preservation row and native/hydrated command; identify concrete
   grouped-save and draft-transfer gaps before touching the reference.
2. **Routes/layout:** add task wrappers/navigation and scoped styling in the
   real shell; move read-only content and preserve deep links. Verify SSR,
   mobile/4K geometry and missing/not-found states.
3. **Editing:** implement typed draft transfer and coherent task saves, then
   settings, playback/access, item metadata and artwork. Prove error atomicity,
   inheritance and adjacent-item retention with and without JavaScript.
4. **Items/bulk:** port rows, selection, paging, sticky Save/Discard, numbering
   scopes, preview and undo. Prove off-page retention and large-list behavior.
5. **Lifecycle:** bind readiness, saved-revision Publish, source/impact/history
   and recovery to durable state. Preserve drafts and real eligibility.
6. **Closeout:** complete production acceptance, fix actionable review findings,
   then remove duplicate controls/dead styles. Update route/hydration evidence,
   component docs, changelog and milestone/debt. Partial routes are not completion.

## Evidence collection

The milestone owns required acceptance. Run builds/media checks serially with
bounded concurrency and the configured target-dir. Prototype reports in
`admin-ux-organize/verification.md` are only design evidence.

Start with `cargo xtask affected-checks`, focused web/application/repository
tests, affected-crate Clippy, localization and hydration lint. Extend
`tests/e2e/addon-metadata-editor.spec.ts`, `addon-list-options.spec.ts` and
`addon-list-disk-warnings.spec.ts`; add a focused task-navigation spec where
useful. Assert real persisted state and revisions, not just visible labels.

Build a matched SSR/WASM/CSS bundle using BUILD.md's supported workflow. Run
Chromium/Firefox hydrated and JavaScript-disabled cases on temporary database
and provider fixtures. Keep screenshots, browser errors, state assertions and
bundle identity under `test-results/amm-n-01b/`. Cover:

- Empty/importing/ready/dirty/published/blocked/unavailable/recovery states;
  localized long titles, missing metadata, 165-item and larger generated lists.
- Both themes at 320, 390, 720, 1024, 1440, 1920 and 3840 CSS pixels; principal
  tasks at mobile/desktop with keyboard and 200% zoom. Check overflow, focus,
  disabled navigation, 44px principal controls, names/errors and contrast.
- Save/Discard, task/artwork/item navigation, chapters, every page size,
  off-page selection, numbering scopes, undo and inheritance. Publish must
  retain newer unsaved input. Inject invalid/stale/write-failure cases.
- Actual source/revision/recovery eligibility and durable outcomes, repeated
  submissions and concurrent refresh; access to full audio/subtitle information.

Run `cargo xtask parity-check web` once for final route/shared-UI acceptance.
Update route/hydration trackers and TEST.md coverage as applicable. Run playback
smoke and applicable TSDuck checks if commands/persistence affect publication
or playback; do not call those changes CSS-only. N-03 retains integrated six-RID,
Windows/restart and real-channel gates. Do not substitute prototype evidence.

## Production comparison and retained capabilities

The production comparison uses the unchanged locked HTML, including current
`data-disclosure-kind="section"` markup; historical screenshots of older Apply
labels and tiny group-action links are not the comparison baseline. Disposable
captures live with production captures under `test-results/amm-n-01b/`.

Permitted differences are the real shell and its Support navigation, application
font/theme tokens, translated command labels, actual image availability and
provider-driven fields. Production omits the synthetic preview banner. Its
breadcrumbs and title therefore begin higher. Actual counts, readiness,
durations, dates and job outcomes replace examples. Package options appear only
when the installed add-on declares them; absence is explained in place. Real
cutoff, capability, revision and validation guidance may increase section height.

The content retains four task tabs, readiness before filters, one sticky
numbering Save/Discard bar, aligned row columns/mobile cards, restrained section
disclosures, fixed adjacent-item slots and vertical access controls bounded at
48rem. List settings use the reference 64rem reading width. Storage input keeps
the reference compact width. Artwork uses validated image handles and previews
instead of prototype filenames. Expanded credits, source operations and Activity
contain additional real fields within their owning groups.

Browser capability ownership:

| Spec | Production behaviors |
| --- | --- |
| `addon-list-workspace.spec.ts` | Task/item/artwork retention, exact off-page selection, 165/1,001-item paging, coherent validation/write rollback, stale task review, unsaved input during publication, immutable uploads, chapter save/restore/derived restrictions. |
| `addon-metadata-editor.spec.ts` | Overview/create, source probes/discovery, conditional metadata and liveness, numbering scopes/Undo, replacement, publication/reinclude, presentation migration, provider confirmation, reset/recovery and revision compare/restore. |
| `addon-list-options.spec.ts` | Native/hydrated option precedence, explicit default overrides, preview/activation Undo, access testing/cancel/retry using saved settings. |
| `addon-list-disk-warnings.spec.ts` | Combined/per-list warning persistence, independent zero-disable and unchanged publication revisions. |
| `addon-list-item-form.spec.ts` | Populated item hierarchy, native numbered/unnumbered validation, credits and date preservation, option sources, exact replacement save/discard, inner-scroll captures and field contrast. |
| `addon-list-workspace-visual.spec.ts` | Six task views in both application themes independently of OS theme, seven widths, reflow, principal target size, contrast and 200% zoom. |

Core/app/infra editor regressions cover ownership, row/revision/ETag fences,
foreign artwork handles, immutable repeated uploads, injected grouped-write
failures and chapter reconciliation admission. Queued reconciliation follows
later draft revisions; already running reconciliation rejects a competing save
without partially changing metadata.

Revision comparison is shown only when two retained revisions exist, with distinct
source/target labels and different initial selections. Empty and single-revision
states retain their existing history explanation without offering an empty action.
Final implementation status and all acceptance evidence are owned by milestone
`amm-n-01b`; N-02/N-03 remain separate.

The item correction keeps `AddonListWorkspacePage` server-rendered, with only
the existing shell islands. `workspace/views/detail.rs` composes editing and
saved-item tools; `settings.rs` splits schema fields/trigger/status without a
second schema load. `workspace/actions.rs` normalizes the genuine numbering
radio while retaining sparse drafts and review fences. Canonical First aired
values use a native date input; existing timestamps/partial dates stay in text
inputs so browsers cannot silently clear them. Replacement selection remains
an unsaved edit until Save item, with its own pending notice and Save anchor.
Provider descriptions stay visible because the schema cannot classify security
prerequisites separately from optional prose. No new island or shared primitive.

### Live publication readiness boundary

The subsequent publication correction keeps `AddonListWorkspacePage` as the SSR
wrapper. Its resolved workspace data seeds `PublicationOverviewIsland` in
`workspace/publication.rs` before island emission; no unresolved `Suspense`
boundary carries these props. Only the Items task publication heading, counts,
blocker messages and action update from the existing
`get_addon_media_list_publication_readiness` server function. The native
`addon-list-workspace` form and `intent=publish` submission still invoke the same
saved-revision command, including without JavaScript. Inputs outside the island
stay mounted and retain unsaved values. No shared primitive changes.

`addon-prepared-playback.spec.ts` owns the real background Prepare-to-Publish
proof without a reload and with unsubmitted input preserved. Workspace native,
hydrated and visual suites cover both browsers; playback/restart/caption suites
retain their persistence and decoded-media guarantees through the new task
controls. Milestone `amm-n-01b` owns the results and remaining acceptance.

## Dogfood corrections (2026-09-13)

The accepted correction keeps O-01 withdrawn and uses existing AMM-N ownership.
Deliver reliable saves/editor loading (D11/D12), installation/setup accessibility
(D01-D03/D09), blockers/progress (D04-D07), focused presentation/provider fixes
(D08/D10/D13), then integrated verification. Do not touch the live profile or
infer publication authorization. Each finding remains separately tracked in its
owning parity record until evidence supports closure.

### D11/D12 source map

- Routes: `/media/addon-lists/{id}` and its task routes, particularly `/playback`.
- SSR wrapper: `AddonListWorkspacePage` and the initial editor loader in
  `admin/pages/media/addon_lists.rs`; task views remain SSR components.
- Interactive boundary: existing publication readiness island and polling.
- Native fallback: the existing workspace POST adapter carries `Draft`; only Save
  commits the task through the application service and repository transaction.
- Persistence: core `AddonMediaListEditorSave` and the SQLite editor repository;
  retained schemas and publication ownership use the existing lifecycle helpers.
- Shared UI: no new draft manager; incomplete task data disables its fieldset.
- Upstream relationship: ADR 0091's related RemoteStreamList/CollectionItems
  source map still applies; repository-installed add-on tasks are an extension.
- Browser target: Chromium/Firefox, JavaScript enabled/disabled, synthetic fixture
  metadata movement and two-operator saves; matched SSR/WASM/CSS assets required.

D11/D12 acceptance combines repository transaction tests, controlled moving-read
tests and native/hydrated browser workflows. The owning milestone records their
results; this correction does not imply full AMM-N completion.

### Dogfood correction component boundaries

- The shared `Modal` owns viewport bounds and body scrolling. Its existing focus
  trap, Escape handling and focus restoration still own keyboard behavior.
- Repository uploads return the committed source identity, display name and
  offer count. The existing Browse route accepts a repository filter; uploading,
  installing and enabling remain separate operations.
- Publication blockers carry optional stable item identities. Both direct native
  editor navigation and the Publication blockers filter use those identities
  from the authoritative readiness classification. Unknown availability has its
  own draft category; publication restrictions are unchanged.
- `AddonMediaListPreparationSummary` is an additive read model. It counts unique
  item/operation requirements and projects the existing durable jobs for the
  saved revision. Required interval, download and subtitle tasks are separate
  from affected items. Heartbeats are activity; completed required tasks are
  progress. There is no byte estimate or ETA. Legacy aggregate fields remain.
- The native form still owns all drafts and submissions. The small
  `public/workspace-draft-status.js` asset only compares rendered form defaults
  for display and exposes the full selected filter label on narrow screens.
  It never stores, reconstructs or submits a draft. Without JavaScript, the
  server reports retained edits after each native submission.
- The provider duration correction is in the sibling add-ons repository as
  yt-dlp patch 0.11.2. Unknown input is omitted before numeric conversion;
  genuine numeric zero is preserved. No protocol version changes.

Missing media preparation continues through the existing typed
`Preparing`/`PreparingMedia` publication outcomes. It is not a reason to reject
an otherwise valid Playback & access Save. Revision conflicts, invalid option
values and storage failures have distinct typed paths and localized Save copy.

### Reviewable checkpoints

| Checkpoint | Source responsibility | Regression evidence |
|---|---|---|
| 1 — D11/D12 | Typed task baseline, application option validation, transactional editor save, resilient SSR reads and ADR 0091 addendum. | Repository editor/lifecycle suites, option-service tests, controlled editor-read tests, native/hydrated metadata-save and Undo scenarios. |
| 2 — D01-D03/D09 | Shared Modal, repository import receipt, existing add-on manager and localized permission descriptions. | `addons.spec.ts`: short/narrow dialog keyboard behavior and repository upload, browse, install and explicit enablement. |
| 3 — D04-D07 | Readiness item identities, Unknown reporting, closed access outcomes and current-revision durable preparation projection. | Readiness/restriction repository tests, synthetic 166-entry task projection, blocker navigation and Activity browser scenarios. |
| 4 — D08/D10/D13 | Responsive native controls, thumbnail fallback, cutoff/save feedback, ignored build metadata and sibling yt-dlp 0.11.2 duration correction. | Both-theme viewport/zoom matrix, Dutch/German narrow labels, large-list form regression and add-ons unknown-duration regression. |
| 5 — Integrated verification | Owning parity debts, hydration records, changelogs and milestone evidence. | Affected native tests/Clippy, final web parity gate, matched Chromium/Firefox build, smoke and applicable TSDuck acceptance. |

Source review corrected typed error erasure, the public asset route, a quadratic
form comparison on All pages, the Stylance cutoff selector and incomplete-read
status classification. The display-only comparison indexes native baseline names
once per event, preserving the existing form as the sole draft owner. Fixture
review also restored the distinction between an inherited option and explicit
false, and used the subtitle provider fixture's supported source route.

CI subsequently exposed Save after option Undo retaining the pre-Undo baseline.
After successful Undo, the native adapter applies only the receipt's restored
schema and list-option values to that baseline. Delivery, cutoff, installation
and warning fences remain unchanged; a legacy form without a baseline keeps
the strict path. Both option workflows now test another metadata update between
Undo and Save, while retaining published-option isolation.
