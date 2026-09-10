# Row actions and mixed-list editing: pattern research

Reviewed 2026-09-09. This research revises the previous Actions disclosure.
Design-system guidance is evidence for choosing a pattern, not proof that the
result has been usability-tested with ErsatzRS operators.

## Findings and application

| Source | Relevant guidance | Application to ErsatzRS |
| --- | --- | --- |
| [Carbon data tables](https://carbondesignsystem.com/components/data-table/usage/) | Separate inline row actions from selected-item batch actions; keep a small action set visible where possible. | S/E editing and Save stay beside each item. Checked-row actions have their own clearly scoped region. |
| [Carbon overflow menus](https://carbondesignsystem.com/components/overflow-menu/usage/) | Overflow handles additional commands under space constraints, with direct labels and clear grouping. | Three commands, no settings, paragraphs or nested disclosures in the menu. |
| [Cloudscape inline editing](https://cloudscape.design/patterns/resource-management/edit/inline-edit/) | Frequent property changes belong in context, with explicit confirmation/discard and nearby validation. Avoid disorienting changes to filtered/sorted rows. | Keep title, air date and runtime beside numbers; retain row position and unfinished drafts after saves. |
| [PatternFly bulk selection](https://www.patternfly.org/patterns/bulk-selection/) | Selection and actions on selected items are separate concepts; scope and selection state must be clear. | Checking an episode never toggles publication inclusion. Range selection only checks candidates, which can then be refined. |
| [WAI menu button](https://www.w3.org/WAI/ARIA/apg/patterns/menu-button/) and [toolbar](https://www.w3.org/WAI/ARIA/apg/patterns/toolbar/) | ARIA menu/toolbar roles carry specific focus and arrow-key behavior. | Do not label ordinary HTML controls as an ARIA menu/toolbar without implementing that behavior. Use native buttons in a labeled popover group and normal Tab order. |

Carbon also recommends disabling individual row actions during batch mode. That
is a useful scope-separation principle, but its scripted behavior is not copied
unchanged here. The current static study keeps explicit row and checked-row
labels. Before production, verify that operators distinguish these scopes; if
introducing a locked batch mode, implement it through server/native state rather
than CSS that merely makes an active button look disabled.

## Correction to the previous artifact

The earlier disclosure combined three jobs: an action on one item, choosing a
range, and configuring an algorithm. Opening it expanded the table and revealed
fields unrelated to the command the user wanted. The label Actions did not tell
the operator that a substantial form was inside. This is our diagnosis of the
artifact, not a claim made by the external sources.

The revised command popover contains only:

- **Number from here** — open the numbering editor anchored to this item.
- **Select range from here** — open a range selector anchored to this item.
- **Clear these numbers** — clear this item's S/E pair; preserve inclusion.

Numbering opens a compact in-list surface containing endpoint, season and first
episode. Less frequent automatic/count settings live under Numbering rules in
that editor. Range selection opens only the endpoint and Select this range.
These are separate editing states, not nested menu levels or a wizard. The
sample pages demonstrate an ordinary server-rendered response after choosing a
command; they do not calculate from submitted input. The original form remains
intact in its tab while the demonstration state opens separately.

Optional minimum/maximum source-length filters live inside the bulk renumber
editor. They have explicit unknown-runtime handling and do not filter table
visibility or publication inclusion.

The toolbar for checked rows retains separate season-only and renumber commands.
The operator can select episodes 6, 7 and 9 while leaving bloopers at 8 alone.
Neither a runtime threshold nor an air-date gap automatically classifies extras.

## HTML/CSS implementation choice

The command list uses native `popover="auto"` and `popovertarget` buttons.
Escape/outside-click dismissal and replacing the previously opened popover are
native browser behavior. CSS anchor positioning places it beside its row and
offers alternate positions at viewport edges. An unsupported anchor-positioning
browser uses the native centered popover with the item identity still visible.
No JavaScript or TypeScript is shipped.

The popover uses `role="group"` with an item-specific name. Buttons retain
normal Tab/Shift+Tab/Enter behavior. It does not claim the arrow-key interaction
of an ARIA menu. Backend commands remain ordinary form submissions. The native
mechanisms are documented by [MDN popovers](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Global_attributes/popover)
and [CSS anchor positioning](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Anchor_positioning/Using).

The production form/draft/atomic-update requirements remain in
[episode-numbering.md](episode-numbering.md). In particular, selecting a command
must not save unrelated row drafts or lose them when returning the list.

## Review the result

- [Mixed list and compact commands](numbering.html)
- [Numbering state at item 6](numbering-6.html#episode-6)
- [Range-selection state at item 6](selection-6.html#episode-6)
- [Fixed example with bloopers skipped](numbering-preview.html)

Verify whether these labels are understood without instruction, whether the
operator retains the visual position of the starting item, and whether the
season-only/renumber distinction is clear. Browser checks establish native
operation and layout, not measured ease of use.
