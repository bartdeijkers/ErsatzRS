# Whole-artifact review: reduce human mental load

Reviewed 2026-09-09. This is a prioritized improvement list, not an implementation
or approval of the current draft skill. The prototype and skill were left unchanged during that review. The subsequent
authorized revision is mapped in [design.md](design.md#review-improvements-applied).

The main problem is that the artifact still makes people manage the interface:
large introductions precede the work, related controls live in different places,
and multiple saving/previewing paths require users to remember their scope.
The next revision should make the working list and coherent editing tasks primary.

## Scope and evidence

Reviewed all 38 HTML documents, their shared styles, preservation inventory,
numbering proposal, action-pattern research, verification records and draft skill.
This includes all nine item reviews, nine numbering contexts and nine range
selection contexts, plus settings, options, activity, artwork and example states.
The fixed top header and collapsible left navigation are excluded from redesign.
This is a separate design task; it does not change the active Phase 9A milestone
or the unrelated AMM-N implementation work.

Fresh browser evidence is in [review-evidence.json](review-evidence.json):
Chromium 151.0.7922.34, JavaScript disabled, all 38 documents opened, and 84 layout
observations across 14 representative views, two themes and 320/768/1440px widths.
Viewport height was 900px; the drawer was closed at 320px and open at wider sizes.
No document overflow was observed, and the header remained 56px tall. These are
layout observations, not 84 usability tests. Earlier Firefox/native-control
checks remain in [verification.md](verification.md); they were not all repeated.

The review applies the distinction between necessary task reasoning and effort
introduced by the interface. Keep the former visible and reduce the latter.
See [NN/g's cognitive-load guidance](https://www.nngroup.com/articles/minimize-cognitive-load/).
Recognition, consistent actions, clear state and recovery provide the supporting
heuristics; the recommendations below are this artifact's design assessment,
not measured user-study results. See [NN/g's usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/).

Severity describes the expected effect if the proposed interaction is adopted:
**high** threatens task completion or retained work; **medium** creates recurring
friction or ambiguity; **low** is localized friction. **Proven** describes an
observed artifact property; inferred human impact is stated as likely or a
hypothesis. No production data-loss or persistence claim is made.

## Improvements, ordered by severity

### 01 — Cancel the local action without discarding other edits

**High · Proven behavior · Numbering and selection contexts.**
In [numbering-6.html](numbering-6.html#episode-6), change item 7's season to 4,
then choose Cancel in item 6's numbering surface. The link returns to
[numbering.html](numbering.html), where item 7 is back at season 1. Cancel
therefore abandons unrelated work as well as the current action.

**Improve:** make cancellation a native form intent that closes only the current
surface and returns the other drafts, selection and position. Filtering,
pagination and item-detail navigation need the same explicit retention contract.
Keep full-task Discard changes distinct from local Cancel.
**Accept when:** canceling a range/numbering action retains another row's valid
or invalid draft and all checked IDs; only an explicit discard removes edits.

### 02 — Keep bulk controls from hiding the evidence

**High · Proven layout · Numbering with selected rows.**
Select item 6, open Renumber checked rows and Filter by media length, then scroll.
The sticky `.selection-tools` panel is approximately **489px tall** in a 900px
viewport. It covers much of the list people need to inspect. At 320px, selecting
item 9 instead leaves the group's actions more than 1,300px above the viewport.
See the `expandedSelection` and `mobileSelection` observations.

**Improve:** keep only a short scope/action strip sticky; let the full editor
flow beside the affected rows. Provide a nearby native link to group actions
at the list's end on narrow screens. Retain an anchor back to the inspected row.
**Accept when:** several neighboring records remain readable during numbering,
and the group action is reachable without hunting back through the whole list.
Check expanded controls at short desktop heights as well as mobile widths.

### 03 — Make the numbering preview a single, unambiguous decision

**High · Proven structure; likely scope mistakes · Bulk numbering preview.**
[numbering-preview.html](numbering-preview.html) offers Apply 3 numbering changes
above a second, fully editable form with row Save buttons and more bulk actions.
The proposed values are small annotations alongside existing inputs. Editing those
inputs does not change the separate fixed Apply form.

**Improve:** show a review state with clear current → proposed values, reasons
for unchanged/skipped items, one Apply numbering changes action and Back to edit.
Do not offer a second mutation path inside that review. Keep neighboring evidence
visible and return editing to the retained original draft.
**Accept when:** a person can identify exactly which values Apply will use without
reading implementation notes; selected, changed, skipped and off-page scope agree.
The static preview must remain labeled as a fixed example.

### 04 — Complete the important paths currently represented only by promises

**High · Proven design gap · Metadata, access and recovery review.**
In [item-6.html](item-6.html), Open media details leads to a paragraph in
[states.html](states.html#media-detail), not an example of the information layout.
Replacement search has no result/selected/clear state. The [inventory](inventory.md)
also explicitly defers conflict resolution, restore/reset impact and other results.
Recording a destination preserves intent, but does not yet show that the redesign
preserves useful access to the information.

**Improve:** add representative synthetic result states for these paths. Reuse
the existing complete media-detail layout rather than designing a reduced one.
Show audio/subtitle languages, tracks and relevant provenance where the user makes
those decisions. Add a concrete unavailable/retry case, not only a healthy list.
**Accept when:** each inventory entry has an inspectable destination or a clearly
identified existing surface whose navigation can be reviewed. Keep backend work
explicitly separate from this artifact acceptance.

### 05 — Replace per-setting saves with task-level Apply changes

**Medium · Proven structure; likely repeated work · Playback and item overrides.**
[options.html](options.html) still has separate Save buttons for cookies and
SponsorBlock. Every item review repeats separate override Save/Reset pairs.
The user has explicitly chosen a shared **Apply changes** interaction for the
playback group, but that choice is not yet reflected in the artifact or skill.

**Improve:** let users adjust the group's controls, including a pending inheritance
reset, then apply them together. Scope the button to the group; do not blindly
merge unrelated commands or the whole admin page. Preserve the difference between
applying list settings and publishing media. Test access remains an immediate,
separate command with clear use of applied settings.
**Accept when:** changing both playback options requires one Apply action,
validation retains the whole draft, and a failure cannot masquerade as complete
success. Backend command/revision guarantees must be preserved without dictating
one visible button per endpoint. The new grouped failure behavior needs an explicit
server contract before implementation.

### 06 — Bring the media list into the first viewport

**Medium · Proven layout · Opening Items & review.**
At 1440×900, the first row starts around **y=1,004px**; no complete media row is
initially visible. At 320px it starts around **y=1,532px**. The page repeats Published
in the heading, readiness panel and bottom publication section, interspersed with
metrics, guidance, filters and another editor heading.

**Improve:** use a compact list identity/status line, a concise exception summary
when needed, and the filter row immediately above the records. Put diagnostic IDs
and healthy detail counts in a single status disclosure. Preserve all information
and the prototype notice, but remove repeated representations.
**Accept when:** at 1440×900 the first view contains multiple useful records;
at 320×900 the first record is reachable in the initial view after the drawer closes.
Measure visible task evidence, not just total page height.

### 07 — Remove the remaining oversized settings furniture

**Medium · Proven layout · Editing refresh, delivery or metadata.**
[settings.html](settings.html) repeats three section summaries as illustrated
navigation cards, a source-flow diagram, the actual fields, and a saved-appearance
block. The first editable control starts around y=878px on desktop and y=1,277px
at 320px. Delivery uses three large illustrated radio cards for a small choice.

**Improve:** replace the task cards with compact section links if the page needs
them. Remove the source diagram when the source/provider labels already express
that relationship. Use compact labeled radio rows for delivery, with short
consequences beside each choice. Keep one useful saved-artwork preview.
**Accept when:** opening settings reveals actual controls, and no repeated summary
must be scanned to discover the same value again below. Retain readable text and
hit areas rather than shrinking everything.

### 08 — Put causally related settings in the same place

**Medium · Proven separation; layout recommendation is a hypothesis.**
Delivery mode lives in [List settings](settings.html#delivery), while cookies,
SponsorBlock and disk usage live in [Playback & access](options.html). The delivery
section mentions SponsorBlock but requires another tab to change it. Source
configuration and Test saved source are likewise split from their action context.

**Improve:** group delivery, access and preparation behavior in Playback & access;
keep source/refresh and list identity under List settings. Place source-test and
reconcile entry points beside the saved source, with detailed history in Activity.
Keep metadata language distinct from playback audio/subtitle preferences.
**Accept when:** configuring download-ahead with access/segment settings can be
done within one task surface, and testing a source does not require remembering
where a maintenance command was moved. Validate this task map before changing tabs.

### 09 — Reduce repeated row chrome while preserving scan evidence

**Medium · Proven repetition · Manual episode correction.**
Every row repeats Included, Finite media, Details, Source, an air-date-gap sentence,
a saved-number line, Save and Actions. Title and Details both open the same item.
There are nine per-row Save buttons; an unchanged pair is displayed twice.
At 320px, the first overview row is about 238px tall.

**Improve:** align title, air date, source runtime and S/E as the main reading
sequence. Give routine shared state one clear home while keeping exceptions
explicit in affected rows. Use concise gap notation with a meaningful accessible
label. Use real thumbnail evidence when available and restrained missing-art
placeholders. Evaluate one Apply numbering edits action for a manual editing pass;
retain per-row apply only if the workflow actually benefits from it.
**Accept when:** the same mixed list is easier to compare without losing titles,
dates, runtimes, specials or unknown values. Applying edited pairs must have a
clear scope distinct from checked-row algorithms. Do not fake client-side dirty
counts or hide authoritative saved/draft differences that the server must render.

### 10 — Give bulk work one recognizable scope model

**Medium · Proven structure · Editing several items.**
Selected-row numbering tools live above the list. Media-kind/liveness bulk tools
live below it and use All/page/needs-review selectors. Provider options appear
both below the list and on Playback & access. These controls use different scope
models without a single place to establish what is affected.

**Improve:** organize bulk actions around one visible scope summary: checked
items, this page, current filter or all items, as appropriate to the operation.
Offer only relevant commands, keep publication inclusion separate, and provide
Clear selection without resetting numbering drafts. Keep off-page scope explicit.
**Accept when:** the selected group does not silently become all items when the
operator changes action, and returning from an action retains the intended scope.
No JavaScript-based range selection is required; native checkboxes and the server
range helper remain valid.

### 11 — Put numbering choices before optional filtering detail

**Medium · Proven order · Renumbering a selected group or range.**
In the checked-row editor, the nested length filter precedes First episode and
Preview numbering. Expanding it introduces several explanatory paragraphs before
the primary numbering inputs. Range numbering has a separate arrangement of the
same concepts. This makes one operation feel like two different tools.

**Improve:** use the same order in both contexts: scope, season/first episode,
replacement policy where applicable, optional length bounds, advanced date/count
rules, Preview. Keep minimum and maximum together, with units and a short unknown-
runtime choice. Use explicit Unbounded empty states rather than example values
that can look like active filters. Keep the detailed algorithm explanation in
help, and show actual skip reasons in the server preview.
**Accept when:** blank bounds clearly mean no filter, a short episode can still be
chosen deliberately, and trailers/bloopers are never classified from duration alone.
Both contexts must preserve inclusive bounds, unknown-runtime and occupied-slot rules.

### 12 — Distinguish valid extras from items that need correction

**Medium · Proven inconsistent sample · Finding problems.**
The overview shows three unnumbered entries and Needs review (3), while the bulk
scope says Items needing review (0). All items are otherwise included and healthy;
the unnumbered entries are the two previews/trailers and the bloopers.
The three-item review count has no disclosed reason.

**Improve:** derive review categories from explicit problems and show their reasons.
Offer Unnumbered as a neutral filter, not an implied error. Make all counts and
scope labels agree, including empty categories.
**Accept when:** a deliberately unnumbered trailer or blooper does not appear broken,
and Needs review leads to an actual explanation on every affected row.

### 13 — Use stable, informative revision identities

**Medium · Proven ambiguity · Comparing or restoring history.**
[activity.html](activity.html) identifies published snapshot 3 and saved revision 29,
then presents bare 3/2/1 options labeled Compare revision and Revision to restore.
The operator must infer which identifier namespace the controls use.

**Improve:** label snapshot and list-revision identities consistently, with timestamp
and short change summary beside each history entry. Choose comparison/restore from
those entries or use descriptive native options. Keep protected/unavailable targets
visible with reasons rather than offering them as ordinary choices.
**Accept when:** the selected restore target is recognizable without memorizing
numbers, and its draft/publication consequences are explicit in the impact review.

### 14 — Make activity a readable history rather than a success display

**Medium · Proven repetition · Checking the latest outcome.**
The latest publication panel repeats success in three journey stages, the result,
a Completed badge and the page badge. It occupies much of the first screen before
revision history. The final outcome/time is more useful here than the illustrated
path through three already-complete stages.

**Improve:** lead with a compact chronological entry showing action, outcome, time
and affected snapshot; expand an entry for phases/events and diagnostic associations.
Show active phase/progress when work is running, and promote recovery only when
there is an actual problem. Preserve every event and truncation indicator.
**Accept when:** the latest outcome and several prior events are easy to scan,
and a failure points to its cause and available next step without opening unrelated
sections.

### 15 — Give artwork one editing home and visible choices

**Medium · Proven duplication/ambiguity · Changing an image.**
Artwork URLs exist in [settings.html](settings.html) and [artwork.html](artwork.html).
The latter repeats three large forms with URL, file and provider selection active
at once, but supplies no visible precedence when more than one is populated.
Provider candidate 1 is not enough information for a visual decision.

**Improve:** use one compact artwork area with the current image and role. Opening
Change exposes a native source choice—URL, upload or provider—with only its relevant
fields emphasized. Show candidate thumbnails and role/aspect information. Retain
independent artwork roles and make Apply's draft scope explicit.
**Accept when:** the same URL has one clear editing home, the selected source is
unambiguous, and choosing artwork preserves other metadata drafts. No raw upload
or provider choice is silently ignored because another field happened to contain data.

### 16 — Keep item review compact and support moving through the list

**Medium · Proven structure · Reviewing neighboring items.**
All nine item pages repeat five forms and large review/tools/override sections.
Season/episode editing occupies a large fieldset, Movie classification is offered
for an episode, and the user must return to the list to reach the next item.
The list-return link targets the general Items section rather than that item's row.

**Improve:** keep core metadata in compact rows, S/E adjacent, and inclusion explicit.
Reveal movie-only classification when applicable; preserve it correctly in the
server payload. Group override edits under one coherent Apply task. Provide
Previous/Next within the retained filter/order and Return to this row.
**Accept when:** reviewing a sequence does not require repeatedly reconstructing
position, and navigation safely retains or explicitly resolves pending edits.
Do not turn this into a wizard or move the inline numbering task out of the list.

### 17 — Confirm destructive operations after showing their impact

**Medium · Proven friction · Reset/restore investigation.**
In [activity.html](activity.html), Review reset impact is behind a required typed
list name. Restore asks for acknowledgment before Preview restore. The user is
being asked to confirm before seeing the concrete affected data.

**Improve:** allow read-only impact review first. Put any required acknowledgment
or typed confirmation at the final destructive action, next to the exact consequences.
Do not add extra previews to ordinary reversible edits merely for uniformity.
**Accept when:** the user can inspect impact without committing, and the final
operation retains the existing protection, exact target and concurrency checks.

### 18 — Demonstrate complete states and longer lists before calling the design ready

**Medium · Proven evidence gap · Whole artifact.**
The current example has nine healthy, included, finite items and one page.
[states.html](states.html) presents exceptions in a gallery, including a Published
page badge while discussing recovery and multiple mutually exclusive work actions
side by side. The fixed numbering preview does not demonstrate real filtering.

**Improve:** provide coherent whole-view examples for a long mixed list, off-page
selection, pending edits, conflict, unknown/live/unavailable items, running work and
recovery. Distinguish currently published media from a pending publication failure.
Keep prototype-only banners and response tabs identified as review scaffolding;
do not copy them into the proposed production interaction.
**Accept when:** a reviewer can walk the full task through each consequential state,
not infer it from a paragraph. Include zero-match length filtering, invalid bounds,
stale preview, and a deliberately excluded special. No persistence claim is made
until the native server implementation is separately verified.

### 19 — Make conditional help stable and concise

**Low · Proven layout movement · Playback options.**
At 320px, opening cookies Details moves its summary from approximately x=252,y=617
to x=42,y=663. The trigger changes position while being used. Default origin and
reset explanations are also repeated in the open content.

**Improve:** keep the disclosure trigger in its original place and reveal content
below it. Prefer a specific label such as Cookie settings and help. Keep a concise
origin summary near the checkbox; stage Use package default as part of the same
Apply task rather than an unexpected immediate second mutation.
**Accept when:** pointer and keyboard users can open/close help without reacquiring
its trigger, and reset scope is clear without rereading repeated paragraphs.

### 20 — Replace instructional slogans with useful information

**Low · Proven copy · All main views.**
Examples include A clear view of what is ready to play, Find the outcome quickly,
and Read the list. Put each item in its place. The interface repeatedly explains
how to scan instead of giving that space to the information being scanned.
The row gap text also contains the visible grammar error 1 days.

**Improve:** use short task names, actual state and specific consequences. Keep
necessary guidance at its relevant control, remove duplicate preambles, and use
consistent Apply changes / Preview changes / Publish saved changes terminology
according to the real operation. Use Filter items for a filter submit so Apply
alone does not refer to both filtering and committing edits.
**Accept when:** removing the preambles does not remove a necessary fact, and
button labels predict their immediate effect. Correct pluralization and support
long localized labels in the next artifact states.

## What the future skill needs to change

The current draft is not yet the finished skill. These are required corrections
to its decision-making method before using it to drive the next redesign:

1. **Replace the independent-saves rule.** `SKILL.md` currently says to preserve
   independent saves, and `references/mental-load.md` explicitly instructs one
   Save per playback option. Preserve data guarantees while grouping visible
   commits around the user's task. Incorporate the agreed Apply changes wording.
2. **Review task boundaries before choosing components.** Map what users need to
   compare, change together and recognize before choosing tabs, cards or disclosures.
3. **Require evidence for every prominent element.** Identify what decision a
   metric, image, heading, repeated status or paragraph helps. Remove duplication;
   retain unique information in its appropriate, reachable context.
4. **Evaluate the complete editing lifecycle.** Include opening, changing several
   controls, applying, canceling, changing scope, navigating and recovering. A native
   control that works in isolation does not establish a low-effort workflow.
5. **Treat visible comparison evidence as an acceptance criterion.** Measure useful
   rows in the first viewport and while tools are expanded. A no-overflow result
   alone can coexist with an unusably tall or obstructed working area.
6. **Keep classification under human control.** Mixed lists are normal. Preserve
   dates, runtimes, titles and unknown states without equating short/unnumbered
   with unwanted or broken. Numbering, selection and publication are separate.
7. **Test several domains before generalizing the skill.** Apply the rubric to
   playback options, mixed-list numbering, artwork choice and recovery. Do not
   produce a universal card ban or a giant shared Apply form from one correction.

## Friction and deceptive-pattern result

The artifact clearly labels synthetic data and non-saving responses. No observed
commercial pressure, disguised advertising, forced consent, or false urgency was
found in the inspected main content. This is not a legal/comprehensive compliance
assessment. The relevant risks here are unnecessary confirmation, ambiguous scope,
misleading state combinations, repeated actions and lost draft context (findings
01, 03, 05, 12, 17 and 18). No live mutation or destructive operation was performed.

## Responsive and accessibility coverage

| Area | Evidence | Remaining work |
| --- | --- | --- |
| 320/768/1440px; dark/light | 84 fresh Chromium layouts, no horizontal document overflow | First-view density and active-tool obstruction need correction. |
| Native operation with scripts disabled | All 38 pages opened; focused selection, disclosure and Cancel paths exercised | Real server responses, validation retention and grouped apply remain unimplemented. |
| Keyboard | Current cookie disclosure exercised; earlier native checkbox/popover/keyboard evidence retained | Full task focus order, returned error focus and cancellation focus need scenario checks. |
| Screen readers | Existing labels inspected; no assistive-technology session in this review | Check repeated row names, list/column relationships and returned status announcements. |
| Contrast, targets, zoom | Both themes visually inspected; earlier baseline checks available | No complete measured contrast, spacing-exception, 200% zoom or forced-colors audit in this pass. |
| Localization | Artifact is English; skill requires localized production UI | Long translations and locale-specific date/duration presentation need representative states. |
| State coverage | States gallery and fixed preview inspected | Full contextual examples described in findings 04 and 18. |

No WCAG conformance or measured reduction in human mental effort is claimed.

## Repeated patterns to consolidate after the interaction decisions

These are **candidate** patterns, not newly implemented catalog entries:

| Pattern | Consumers | Shared contract to establish |
| --- | --- | --- |
| Compact option group | Playback defaults; item overrides; future provider options | Label, effective value/origin, conditional fields, pending reset, one scoped Apply. |
| Evidence row with inline numbering | Overview, numbering, selection and preview documents | Stable identity, comparison columns, selection, edited/saved/proposed state. |
| Contextual bulk editor | Selected numbering and row-anchored numbering | Same field order, explicit scope, bounded sticky actions and retained drafts. |
| Operation/history row | Activity and recovery states | Outcome/time first; expandable causes, events and safe recovery actions. |
| Artwork role editor | Poster, fanart, thumbnail | Visible current/candidate art, explicit source selection and draft ownership. |

Use the existing button, resource-state, pagination and data-table contracts
where they fit. Do not abstract the current repetition before correcting its
interaction model. No shared component catalog was changed by this review.

## Suggested order for the next revision

First settle Apply/Publish, draft retention and scope (01, 03, 05, 10). Then redesign
the first viewport and active numbering workspace (02, 06, 09, 11). Reorganize
settings and secondary tasks (07, 08, 13–17, 19–20), while correcting misleading review
counts (12). Finish with the representative states and preservation walkthrough
(04, 18). Use the seven skill corrections to drive all of those decisions.

The list-first editor, aligned air dates/runtime, native checkboxes and popovers,
explicit unnumbered state, and optional min/max numbering filters are worth
retaining. The next task is to turn this review into a coherent revised prototype;
no UI implementation, production change, skill installation or commit is included
in this review.

## Follow-up closure review

The maintainer requested another rework through closure of demonstrated P1/P2
issues. This pass covers the revised 85-document artifact and draft skill, not
production acceptance. P1 means a blocked task, loss of work or major access
barrier; P2 means credible wrong-scope behavior, misleading state or substantial
recurring friction. Visual judgment accompanies the browser checks.

| Finding | Severity | Correction and acceptance evidence |
| --- | --- | --- |
| Publication disclosure interrupts the status line and reflows peer actions | P2 | Status/Publish occupy one aligned region; diagnostics expand in their own full-width region below it. Keyboard opening at 320/768/1440 in both themes leaves peer coordinates unchanged. |
| Bulk labeling says checked rows even when scope is filter/page/all | P2 | Scope-neutral renumber label; checked-selection retention is identified separately. A filtered numbering review contains exactly the matching item. |
| Unavailable/conflicted filters advertise records but return none | P2 | Native filter results return the corresponding records in the long mixed-list scenario. |
| Undo replaces the entire numbering draft and loses unrelated edits | P1 | Undo restores applied pairs while preserving unrelated draft pairs and selection. Browser reproduces a separate pending item-7 edit. |
| Item review and list numbering disagree or permit duplicate assignments | P1 | Both views share pending pairs; Apply checks occupied assignments and retains rejected edits. Native navigation/duplicate-pair cases exercise the full return path. |
| Invalid or inactive artwork fields can trap navigation or Apply | P2 | Draft navigation bypasses native validation; conditional Apply validates the selected branch on the server. Invalid URL/keep-current and invalid playback cases verified. |
| Length format and help references disappeared during compaction | P2 | Restored visible minutes:seconds guidance, inclusive/unbounded semantics and reachable numbering help. Expanded-page reference and local-link checks cover all documents. |
| Manual Apply follows unrelated bulk controls; bulk settings have competing previews | P2 | Manual Apply directly follows the records. Metadata and provider-option fields share one explicit preview action within the bulk region. |
| Replacement search repeats a field whose text is not submitted and loses item context | P2 | One replacement editor; originating item and return destination are retained. Item-7 navigation is exercised. |
| Recovery/maintenance views highlight Playback and fixed examples resemble session state | P2 | Correct task-section highlights; fixed scenario banners identify their scope. Publication and applied state are demonstrated separately. |

The disabled “Details complete” pseudo-button is now status text. Shared section
navigation uses consistent labels, order and active styling. Original header and
left-drawer markup, and shell CSS, are byte-identical to the pre-revision artifact.

The draft skill now resolves compactness versus hierarchy, preserving save
semantics versus endpoint-specific buttons, mandatory versus optional previews,
and static examples versus actual synthetic validation. Its worked publication
example explains the failed grouping instead of prescribing a universal card
ban or banning all inline controls. Research:
[NN/g visual-design principles](https://www.nngroup.com/articles/principles-visual-design/),
[progressive disclosure](https://www.nngroup.com/articles/progressive-disclosure/).

See [verification.md](verification.md) for final evidence and coverage limits.
No demonstrated P1/P2 remains open in this artifact review. This is a scoped
review conclusion, not proof that no user could encounter a design flaw; full
assistive-technology conformance and observed usability remain unmeasured.


## Live-comparison correction scope

A read-only comparison with an available production list exposed four missing
item editors: Summary, Genres, Tags and Chapters. The artifact now gives those
fields editable native disclosures inside the item Apply/Discard boundary.
The preservation inventory has been corrected; the read-only media-details
example never established editing parity.

The follow-up also connects a synthetic recovery-required state to the main
review flow, preserving the published/pending distinction, eligible actions and
numbering drafts. A separate 165-item scenario exercises long titles, absent
metadata and all page sizes. Focused evidence is recorded in
[verification.md](verification.md#replacement-readiness-corrections).
These close the demonstrated artifact gaps, not the production integration gate.
