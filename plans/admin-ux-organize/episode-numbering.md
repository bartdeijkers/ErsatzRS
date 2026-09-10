# Episode numbering while reading a mixed list

Status: revised design and implementation proposal. The HTML/CSS study is
functional through the optional local preview server, which retains synthetic
drafts, calculates example numbering and selects ranges in memory. No production
media is changed. The former wizard has been replaced.

Open [the mixed list](numbering.html),
[numbering at item 6](numbering-6.html#episode-6), or
[a fixed in-list preview](numbering-preview.html).
The same editor is embedded in [the overview](index.html#items).

## Keep the evidence beside the decision

People read neighboring titles, dates and runtimes to decide which entries
belong together. A list can contain episodes, trailers, previews, bloopers,
holiday specials, duplicates and incomplete metadata. Choosing membership comes
before numbering. Keep the list visible throughout this work.

The sample includes trailers at 1 and 5, a season-zero holiday special at 4,
bloopers at 8, and unknown runtime at 9. Checking 6, 7 and 9 allows them to become
season 2 episodes 1–3 while the bloopers remain unnumbered. Selection is separate
from publication inclusion, content kind, collection role and playback behavior.
Making an extra unnumbered must not exclude it from publication. Season zero
is available when the operator wants to number specials; it is not an automatic
classification for short videos.

Aligned date and runtime columns support scanning. On narrow screens they stay
in each row with explicit labels. The sample's 312-day gap before a special and
84-day gap before an episode demonstrate why a threshold cannot establish season
boundaries. Unknown duration is not zero; live/unknown liveness is not a short
finite episode. No keyword or runtime heuristic silently selects or renumbers
anything. The 30-day visual gap accent is separate from the existing automatic
90-day default and declares no season boundary.

## Controls where the decision happens

| Job | Interaction | Effect |
| --- | --- | --- |
| Correct one item | Adjacent S/E inputs and Apply numbering edits | Apply the edited pairs together. Blank/blank is unnumbered; season 0 supports specials. |
| Move episodes to another season | Check rows; enter season; Set season · keep episodes | Preserve episode numbers. Reject invalid pairs or collisions beside the affected rows. |
| Renumber a non-contiguous group | Check episodes; Renumber checked rows; choose first episode | Preview checked eligible items in provider order, skipping slots reserved by untouched items. |
| Clear numbers from extras | Row Actions → Make this item unnumbered, or Make unnumbered for checked rows | Clear the pair without changing inclusion, kind, role or playback behavior. |
| Select a long run | First row's Actions; inclusive endpoint; Select this range | Server returns checked rows. Uncheck trailers, bloopers and specials before numbering. No media mutation. |
| Number a clean bounded run | Row Actions → Number from this item | Existing season, first episode, count, replacement and date-gap options, scoped to an explicit endpoint. |

Every row's Actions is immediately available. No central enable-numbering step
is required. A compact command popover opens beside the row; the chosen command
returns its focused in-list editing surface. See the [pattern research](action-pattern-research.md). The default endpoint is that
row, not silently the rest of the list. Selected-row tools appear when a checkbox
is checked and remain reachable through short links above and below the list. The full group
editor stays in normal flow at every width so it cannot obscure the evidence.

Moving season and renumbering episodes are distinct operations. Preserve existing
undo. Single reversible edits need no separate confirmation; bulk calculations
keep their impact review, displayed beside affected rows in the same list.

## What HTML/CSS can do

Native checkboxes provide independent selection and keyboard Space support.
`:has()` highlights rows and reveals contextual tools. Native numbers retain
typing and keyboard stepping. Native popovers provide the command list; `details`/`summary` reveal optional
numbering rules within the editing surface. Container
queries adapt to the width left after the existing drawer; viewport width alone
does not describe that space. CSS anchor positioning places command popovers beside their row, with a
centered labeled popover fallback on browsers without anchor positioning.

Portable drag selection and Shift-click checking of arbitrary checkbox rows
require scripting. A native multiple select has different range-selection
behavior but loses this rich evidence/editing layout. The proposed range helper
uses one normal server round trip, then native checkboxes refine its result.
Selecting range 6–9 includes the bloopers at 8 until the operator unchecks it.

The command popover contains no settings fields. Choosing a command opens
only its required in-list controls. It uses ordinary buttons in a named group,
not an ARIA menu with unimplemented arrow-key navigation. The prototype does not fake a
live selection count or calculated result: its example review is explicitly
fixed. The server must provide exact selected/changed/unchanged/skipped/off-page
counts in a real preview.

## Native server form contract

### Optional runtime filter for bulk numbering

Both checked-item renumbering and row-anchored range numbering offer independent
minimum and maximum source lengths, entered as minutes:seconds. For example,
`2:30` means 150 seconds and `120:00` means two hours. Empty minimum or maximum
means that side is unbounded; both empty preserve existing numbering behavior.
Bounds are inclusive. Reject malformed durations, overflow and minimum greater
than maximum without changing items, and retain the submitted text.

When either bound is present, an unknown runtime is skipped unless the operator
checks Also include items with unknown runtime. With no bounds, unknown values
remain eligible as before. Filtering uses `duration_seconds` from the row DTO,
not a fresh provider call or the duration after SponsorBlock processing.

Resolve the selected/range candidate set first, then apply length bounds, then
legacy missing-only filtering and count limits, then calculate assignments.
Filtered-out numbered items still reserve their slots. Air-date detection sees
the remaining eligible sequence and must expose its proposed season boundaries.
Preview includes reasons such as shorter than minimum, longer than maximum and
unknown runtime. An empty result disables Apply and explains why.

The filter affects bulk renumbering only. Selection, publication inclusion,
season-only moves, and clear-number commands keep their explicit scopes. The
UI labels it inside the renumber editor so it does not imply a table filter.
It is an aid to selection, not a trailer/blooper classifier: a short episode
may be filtered out, and a long special can still pass. No values are changed
for skipped items. Native UI checks do not prove these backend rules.

### Submission and retained drafts

Use one form for the visible item editor and its filtering/navigation intents,
separate from list settings and publication. Each submit button identifies its operation and anchor, such as
`apply-numbers`, `cancel-context`, or `preview-selected-numbering`. All visible numbering drafts and
checked IDs travel with it. Sending drafts does not mean saving them all.

1. Validate authorization, list revision, operation and item membership. Resolve
   deduplicated IDs against stable provider order. Reject stale/missing/reversed
   range endpoints instead of falling back to the first item.
2. Validate only the operation's relevant inputs. `novalidate` prevents an
   unrelated/hidden invalid field from trapping the native submission. The
   server validates bounds, overflow, pairing and collisions. Empty/empty and
   legacy 0/0 remain unnumbered; numbered pairs need season >= 0, episode >= 1.
3. Merge only target season/episode changes into authoritative current snapshots.
   Preserve inclusion, content kind, role, liveness and metadata. For selected
   season-only edits, a missing episode needs a visible error, not an invented 1.
4. Apply the edited pairs as one task or calculate the scoped preview. Reload saved state, overlay
   untouched submitted drafts, retain checked IDs, open the originating/error
   row, and return to its fragment. Label saved, draft and proposed values
   distinctly. Another row's invalid draft must survive this round trip.
5. Bind bulk Apply to the exact preview/revision and reject stale results.
   Concurrent imports must not expand a reviewed selection. Reuse undo snapshots.

Separate isolated row forms would discard unsaved edits elsewhere whenever a
save returned a new document. A shared editor envelope with explicit intents
preserves those drafts while retaining explicit scoped operations. HTML form associations
cannot submit one input to two forms. Do not use form reset to clear selection:
it would reset numeric drafts too. Uncheck rows individually or provide a server
clear-selection intent that preserves drafts.

Direct checking updates native checkboxes; the renderer retains checked IDs across
pages. Cross-page server-selected ranges need explicit
scope/count and retained validated state. Do not infer selections from absent
checkboxes, CSS classes or a count. Bound rows/payloads under existing form
limits, and settle draft/selection retention on filtering and pagination before
implementing them. The prototype uses form-associated navigation intents to
retain the owning draft; plain unrelated links alone cannot establish persistence.

## Existing capabilities and implementation gap

The current `AddonMediaListItemCard` already has an inline S/E pair and save
action. Its Start here button is conditional on the central numbering panel
being open. Preserve the quick-edit model and remove that dependency.

`AddonMediaListItemRow` already exposes `first_aired`, `duration_seconds`, rank,
title, numbering, inclusion, parent ID and liveness. Displaying source runtime
needs no new provider fetch. Distinguish it from prepared duration after edits
such as SponsorBlock; missing values stay unknown.

Reuse/extract `numbering_candidates`, `episode_numbering_preview_assignments`
and `auto_detect_episode_assignments` for server use. Preserve included/top-level
scope, provider order, occupied-number skipping, missing-only count behavior and
date-gap rules. Do not duplicate their algorithm in CSS or a second client path.

Explicit selected IDs and an inclusive endpoint extend the current start/count
mode. Define a target-set input without changing legacy behavior. Selected-set
renumbering includes checked numbered items; legacy missing-only still preserves
them. Untouched numbered entries reserve their slots. Automatic gap detection
renumbers its candidates, starts each season at episode 1, and retains undated
items in provider order; it cannot identify extras.

`apply_addon_media_list_item_review_edits` already forwards a vector to the
atomic `ApplyAddonMediaListItemReviewEdits` application command. Reuse it. Its
server-function signature does not establish a native form endpoint or expected
revision check; add the smallest adapter/concurrency guard needed after auditing
that boundary. No production implementation is authorized by this artifact.

## Acceptance examples

- Apply item 6 as S02E01 beside its title/date/runtime, and another edited pair together. Reject the entire manual Apply if any
  submitted pair is invalid; retain both edits for correction.
- Check 6, 7 and 9; preview S02E01–03. Trailer, special and blooper persisted
  values remain unchanged. Range 6–9 initially includes 8 until deselected.
- Set only season on already-correct episodes; episode numbers stay unchanged.
- Handle a stale preview, collision, missing date, unknown duration, excluded
  item and child item without guessing a value or expanding scope.
- Verify keyboard operation, open/closed menus, selection visibility, both
  themes, long titles and 320-pixel reflow with scripting disabled.

Sources: [current editor](../../crates/ersatzrs-web/src/admin/pages/media/addon_lists.rs),
[row DTO and atomic adapter](../../crates/ersatzrs-web/src/admin/server_fns/addon_media_lists.rs),
[MDN form ownership](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Attributes/form),
[MDN :has()](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:has),
[MDN details content](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::details-content),
[MDN container queries](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Containment/Container_queries).
