# Add-on workspace source and capability map

Historical source mapping retained from AMM-N-01B. These tables identify
implementation owners and preserved capabilities; verify current source before
using an old symbol name. This is reference material, not outstanding work.

## Source and visual map

Read the lock first. The [preservation inventory](inventory.md)
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
