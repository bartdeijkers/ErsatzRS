# Add-on list design artifact

Status: locked for AMM-N-01B on 2026-09-09; see [LOCK.md](LOCK.md).
The locked pages take precedence over historical design notes below and the
[whole-artifact review](review.md).
The draft [admin-ux-organize skill](skill/admin-ux-organize/SKILL.md) now groups
visible Apply actions around user tasks and evaluates the complete editing flow.
It is not installed as a repository-wide skill.

## Open the artifact

Run from the repository root:

```bash
python3 plans/admin-ux-organize/preview_server.py --port 8412
```

Open `http://127.0.0.1:8412/index.html`. Start at [Items & review](index.html),
[List settings](settings.html), [Playback & access](options.html), or
[Activity & recovery](activity.html). [Design scenarios](states.html) links the
long mixed list, numbering, conflict, replacement, access and recovery examples.

The HTML files also open as static documents. Native disclosure, checkboxes,
radio choices and command popovers work without scripting. POST actions require
the local preview server. There is no JavaScript, TypeScript, WASM or hydration.

The preview server is standard-library Python, bound to loopback. Each browser
gets isolated in-memory synthetic drafts. Restarting the server clears them;
the session pool is bounded to 32. It neither reads the ErsatzRS database nor
calls providers, publishes media or writes user changes to disk. Uploads are
bounded to 2 MB and represented by their filename; bytes are not retained and
are not used as production artwork. Request values are not logged.

This small renderer exists to demonstrate the draft-retention gap found by the
review. It is disposable design tooling, not proposed application infrastructure
or a replacement for the Rust application use cases.

## Layout and tasks

The content redesign is scoped to `.design-main`. The synthetic shell also fixes
the reviewed mobile obstruction: CSS selects between an initially open desktop
disclosure and an initially closed mobile disclosure. Only one is displayed or
keyboard-reachable at a time, and their IDs are distinct. Page navigation starts
the mobile drawer closed again. This HTML-only adapter does not change the
production shell or introduce client scripting.

| Task | Layout and editing boundary |
| --- | --- |
| Inspect and number media | Title, date/gap, source runtime and adjacent S/E fields. One compact Apply/Discard bar stays reachable while scrolling. Explicit bulk scope for algorithms. |
| Configure the list | Compact source/refresh controls followed by identity and metadata. One Apply changes; source-test/reconcile entry points sit beside the saved source. |
| Configure playback | Delivery, cookies, SponsorBlock and the storage warning together. Checkbox-led option rows, pending package-default resets, one Apply changes. |
| Review an item | Metadata, inclusion, numbering and inherited overrides together; previous/next and return-to-row navigation retain drafts. Both navigation slots remain visible, disabled at their respective boundaries. |
| Choose artwork | One artwork editing home, per-role source choices and visible synthetic candidates. Metadata drafts survive the transition. |
| Investigate activity | Outcome and timestamp first, expandable events, named snapshot/revision choices and concrete impact review before final confirmation. |

Applied settings and published media remain separate concepts. Apply commits the
current editing task in the synthetic session. Publication/reset/restore/provider
operations show explicit examples; they do not perform those operations.

## Shared layout alignment

[alignment.css](alignment.css) owns content geometry: shared gaps, form columns,
control tracks, wrapping action groups and width-dependent stacking. Related
playback rows share Grid/subgrid tracks for labels, applied values and Help;
delivery choices share label and description columns. List headings and records
use identical tracks. Bulk scope, numbering, metadata/options and their Apply
controls occupy explicit groups. Artwork uses compact role/preview/action rows.

Grid preserves two-dimensional relationships; Flexbox handles action groups that
need wrapping. Tables remain semantic tables. Container queries respond to the
space left by the original navigation drawer. Native form ownership, disclosure
behavior and all inputs remain intact. The shared-track rationale follows
[MDN's subgrid guide](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Grid_layout/Subgrid).

## Current visual hierarchy and editing vocabulary

Editing actions now use **Save settings**, **Save item**, or **Save numbering**.
**Publish saved changes** is reserved for updating the published list. The primary
publication summary describes the next task; snapshot and revision identifiers
remain available in its details. Saving and publication remain separate operations.
Publishing submits current row inputs to retain unsaved drafts, while publishing
only the saved revision. Feedback appears beneath the page heading.

Bulk checkboxes explicitly select rows for editing. The selection summary shows
the total and how many are on other pages. Publication inclusion remains an item
setting; changing the edit selection does not alter inclusion.

Browser access and SponsorBlock each have one native select: use the add-on's
default or set a value for this list. There are no competing enable/reset
checkboxes. Saved summaries identify whether the value is inherited or explicit,
including an explicit value equal to the default. The synthetic adapter validates
both choices and storage limits before saving. Production must populate choices
and default labels from the installed add-on schema, retaining reset semantics.
Item override choices read **Use list setting**, keeping their existing values.

The content styling uses clearer headings, quieter secondary controls and
borders, neutral preview chrome, consistent input corners and bounded form widths.
Access settings retain their vertical layout and 48rem group width on 4K screens.
Optional disclosures remain limited to the established task groups. The new-video
cutoff explanation appears when that mode is selected or a cutoff was saved.
These changes improve the design rationale; user-study results are still needed
to measure learnability and task completion.

## Current disclosure policy

The bulk editor has one entry disclosure, **Edit multiple items**. Its shared
scope and selection summary sit above two visible task groups: Episode numbering
and Media and playback. Season/start inputs, classification and provider-option
selects are directly accessible; only optional runtime limits remain expandable.
Each task keeps its own existing preview/apply controls. Wide screens place the
tasks side by side; narrower screens stack them without hiding controls.

List settings shows genres, tags and source actions directly. Only the longer
Credits editor remains collapsed. Managed location is a plain reference block.
Access and segments stacks its settings vertically at every viewport, including
4K. A 48rem maximum keeps the group readable, with supporting values and controls
aligned beneath each label.

Playback and access uses ordinary sections for delivery, browser access,
SponsorBlock and storage. Explanations and inherited/default choices sit
beside their controls; storage is no longer an accordion. This supersedes the
broader disclosure adoption described in earlier revision notes below.

The rule is task-based: a disclosure can defer an occasional task or substantial
supporting detail. It should not require extra clicks to reach ordinary controls
inside a task the visitor has already opened.

## Native interaction and state

Forms submit an explicit intent together with their owning draft. Section links
that leave a primary editor are form-associated navigation buttons, allowing the
server to retain inputs before returning the next document. This is why the
preview needs real HTTP round trips even though its UI is entirely HTML/CSS.

Local Cancel closes a numbering/range surface and retains other row drafts and
checked IDs. Discard explicitly restores the task's applied values. Filtering and
pagination retain drafts; checkbox IDs omitted because their rows are off-page
are not interpreted as deselection. The bulk summary identifies retained off-page
selection. It does not pretend to calculate a live checked count with CSS.

Pagination owns one Items per page selector with 10, 25, 50, 100 and All items.
Apply page size submits the current draft and returns to page one. Previous/Next
and size changes preserve pending fields and checked IDs, including off-page
selection. The filter toolbar does not duplicate this selector.

Publication status and Publish have a stable aligned row. Optional import and
publication diagnostics expand below a full-width plus/minus disclosure control. Opening
them does not reflow the status/action row.

The bulk editor is a native `details.disclosure` named Edit multiple items,
directly above the records. It opens after a round trip with checked items or a
broader bulk scope, and never covers the evidence. Redundant jump links are removed.
The former 489px sticky panel is removed. Native range commands remain immediately
available through compact row popovers. Compact outlined expanders reveal optional rules. Help uses a familiar question
symbol and visible label; help containing reset choices is labeled Help & defaults.

Section disclosures group publication/import details, the bulk workspace,
optional numbering limits, credits and movie classification. Routine bulk
metadata/provider fields, storage thresholds and the managed location are visible. Core editing fields, publication status, language guidance,
errors and Apply/Discard remain visible. This is progressive disclosure of
optional tasks, not an accordion that permits only one open section.

Mobile records use three compact rows: identity, date/runtime evidence, and
numbering/actions. Number inputs remain at least 44px high. Numbering validation
focuses a named error summary, marks and describes the affected fields, and links
directly to them. Off-page correction buttons retain drafts and selection while
opening the relevant page; they clear the filter to reveal the conflicting row.
The native HTML behavior does not require browser-default triangle-only styling.

## Action hierarchy reassessment

The previous revision reduced visual emphasis by styling commands as small text
links, but that hid the difference between instructions and actions. Repeated
Group actions, Help, Back to first row and Apply/Discard entry points also asked
the operator to recognize multiple routes to the same task.

- One compact sticky numbering bar contains Apply numbering and Discard edits.
  It remains below the application header; focused fields scroll clear of it.
- Edit multiple items opens the actual bulk editor in place. Short numbering
  guidance is visible beside the task rather than behind a separate Help jump.
- Commands use recognizable primary or outlined secondary buttons. Row Actions
  is a normal labelled control; item titles retain their media-navigation role.
- Undo appears only when an applied numbering or bulk change is available.
  Publication status remains visible, and Publish appears when changes can be
  published. Previous/Next retain disabled boundary slots as requested.
- Source tools groups saved-source testing, reconciliation and add-on changes.
  Item navigation groups Previous, Next, Test applied access and Back to items.

These changes remove duplicate choices and clarify action scope. Browser checks
prove behavior and target geometry, not a measured reduction in human cognitive
load; an observed editing task remains the useful next design evaluation.

Numbering previews are read-only comparisons with one Apply action and Back to
editing. The server demonstrates min/max source-length filtering, explicit unknown-
length inclusion, range endpoints, missing-only behavior, occupied-slot skipping,
season-only edits, numbering undo and validation. It is a synthetic demonstration;
production must reuse the authoritative Rust algorithm and its full eligibility,
revision and concurrency guarantees. See [episode-numbering.md](episode-numbering.md).

## Review improvements applied

| Review finding | Revised evidence |
| --- | --- |
| 01: Cancel loses unrelated work | Native Cancel intent retains the whole numbering draft and selected IDs. Browser regression exercises the original item-7 case. |
| 02: Bulk tools obstruct evidence | Tools open in place above the list; only the compact Apply/Discard bar is sticky. |
| 03: Competing preview mutations | Numbering review contains no editable row inputs and exactly one Apply action. |
| 04: Missing concrete result paths | Media-detail, replacement, source-check, conflict, comparison, reconciliation, access and impact examples now have dedicated views. |
| 05: Per-setting saves | Playback and item overrides use coherent Apply tasks; package reset is a pending choice. |
| 06: First viewport contains no rows | Current viewport and row measurements are recorded in the UX verification reports; earlier row counts are superseded. |
| 07: Oversized settings furniture | Removed task cards/source diagram; delivery choices are compact radio rows. |
| 08: Split causal settings | Delivery moved to Playback & access; source operation entry points moved beside source configuration. |
| 09: Repeated row chrome | Removed per-row Save, duplicate Details links and routine inclusion/liveness labels; kept title/date/runtime/S/E evidence. Applied values remain explicit when editing. |
| 10: Inconsistent bulk scope | Numbering, metadata and provider operations use the same explicit checked/page/filter/all selector. |
| 11: Numbering field order | First episode precedes optional length bounds; bounds have Unbounded placeholders and concise units. |
| 12: Extras look like errors | Unnumbered is a neutral filter; healthy list has zero review items. Long scenario distinguishes excluded, unavailable, live and conflicted records. |
| 13: Bare revision numbers | Snapshot, underlying list revision, date and change summary identify history choices. |
| 14: Large success journey | Removed the illustrated journey; full event detail remains accessible. |
| 15: Duplicated artwork controls | URLs/upload/provider choice live only in Artwork, with explicit per-role radio sources and visible candidates. |
| 16: Item-review friction | Compact S/E group, movie-only disclosure, shared item Apply and retained previous/next/row navigation. |
| 17: Confirmation before impact | Read-only reset/restore impact opens first; acknowledgment and typed title are on the final example form. |
| 18: Insufficient scenarios | Coherent running/paused/recovery, invalid/empty/stale numbering, conflict, access and 36-item paging examples. |
| 19: Moving help trigger | Native help retains its grid position while content opens beneath it. |
| 20: Instructional slogans | Removed page preambles; standardized Apply, Preview and Filter labels and corrected gap pluralization. |

## Replacement-readiness corrections

Item review preserves editable Summary (`plot`), Genres (`genres`), Tags (`tags`)
and Chapters (`chapter_input`). Two native disclosures keep optional editors
inside the owning item form and above its one Apply/Discard pair. Changed
sections reopen when returning from another page. Chapter errors reveal the
section, focus an error summary and identify the field; a failed chapter check
prevents the synthetic renderer from applying the other item fields. Blank
chapter input represents provider fallback; production must retain the existing
provider-restore and derived-item editability contracts.

[Recovery in the list](recovery-list.html) exposes the current blocker and a
Review recovery options button outside publication diagnostics. The synthetic
session connects that summary to [recovery options](recovery.html), an eligible
retry, running status, and an explicitly simulated successful completion.
Generation checks reject a repeated action from an old form. The previous
published snapshot stays distinct from the pending snapshot; completion uses the
revision captured when retry began. Pending numbering drafts survive the flow.
Restore is explicitly unavailable in this scenario, rather than offered without
an eligible snapshot. Existing restore examples remain separate scenarios.

[The 165-item scenario](large-list.html) uses invented long titles and unknown
dates and runtimes. All rows remain reachable through filtering, pagination and
item navigation. Items beyond the existing 36 static item documents use the same
item template in the preview server. Page-size options remain 10/25/50/100/All;
changing page or filter retains pending numbers and off-page selection. Unknown
dates do not generate season-gap evidence, and unknown runtime respects the
explicit numbering-filter choice.

Implementation mapping: use the existing item review handler and NFO metadata
fields, `parse_chapter_input` validation, publication status/recovery DTOs and
job/generation fences. Native details need no island. The production adapter must
preserve each task draft across navigation and handle failed or partial commands
truthfully; the disposable Python session is not that adapter. Translate labels,
help and errors through the supported language catalogs. Verify the real editor
with and without hydration, including conflicts, recovery eligibility and
large-list performance, before claiming production replacement acceptance.

## Preservation and implementation boundary

The [preservation inventory](inventory.md) remains the capability checklist.
No production Rust, database, release, route registration or language catalog was
changed. This is not admin parity acceptance. The active milestone is unchanged.

Production implementation must reuse existing schema definitions and full option
commands, validate grouped changes before reporting success, preserve item
inclusion/classification/replacements, use genuine draft/revision fencing, and
translate every new string. CSS hiding does not disable inactive fields: the
native server adapter must ignore/validate branches according to the chosen mode.
File uploads need the existing multipart/storage path. Full media details retain
video/audio/subtitle tracks, chapters, source facets, ownership and playback health;
the new synthetic view follows the current Remote Stream detail groups.

Fixed result examples are labeled as examples. The preview's simplified bulk
recording, synthetic track data, source/access results and confirmation screens
are not evidence of actual provider preparation, metadata reconciliation,
publication, recovery or file upload processing.

## Verification

The [follow-up closure review](review.md#follow-up-closure-review) records the
subsequent P1/P2 corrections and skill contradiction review.

See [verification.md](verification.md) and
[revision-verification.json](revision-verification.json) for the latest browser
results, layout measurements and screenshots. Browser checks establish native
interaction and layout behavior, not observed human ease or WCAG conformance.

Source references:

- [Current list editor](../../crates/ersatzrs-web/src/admin/pages/media/addon_lists.rs)
- [Provider option forms](../../crates/ersatzrs-web/src/admin/pages/media/addon_list_options.rs)
- [Native option handlers](../../crates/ersatzrs-web/src/api/addon_list_options.rs)
- [Remote Stream information groups](../../crates/ersatzrs-web/src/admin/pages/media/remote_stream_detail.rs)
- [Original review and research](review.md)
