# Designing for less mental effort

Use this reference when organizing a settings page or reviewing a proposed layout.
The core aim is to remove work introduced by the interface while preserving the
information needed for the user's own judgment.

## Grounding

Visibility and consistent controls help people recognize choices without recalling
them from another screen. Remove material that competes with the current task;
keep necessary information available where it is used. These are established
heuristics, not proof that a particular redesign has reduced mental effort.
See [NN/g's usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/)
and [cognitive-load guidance](https://www.nngroup.com/articles/minimize-cognitive-load/).

The following arrangements are applications of that guidance to admin settings.

## Compactness must preserve visual relationships

Proximity makes neighboring elements look related, and differences in placement,
size and spacing express hierarchy. See [NN/g's visual-design principles](https://www.nngroup.com/articles/principles-visual-design/).
Secondary detail can be deferred while the primary task stays visible; see
[progressive disclosure](https://www.nngroup.com/articles/progressive-disclosure/).
Apply these principles to both collapsed and expanded layouts.

The publication prototype failed this check: status prose, a disclosure trigger,
Publish and another status message shared one flex row. The trigger read like
an interruption in a sentence, relationships were unclear, and opening it
reflowed peer controls. Fewer rows had increased interpretation effort.

Use a status/action row with a clear alignment, then a separate full-width
diagnostic disclosure beneath it. Its content opens below a stable trigger;
the status and Publish stay in place. A checkbox, its value and a Help column
can share a row because their relationship is consistent and local. This is
not a ban on inline controls or a requirement for another card.

## Two settings, two rows

For cookies and SponsorBlock, start with:

```text
[ ] Use browser cookies             Applied: None · package default   [? Help & defaults]
[ ] Remove SponsorBlock segments    Applied: Off · package default    [? Help & defaults]

[Apply changes]  [Discard changes]
```

Checking cookies reveals a browser selector on the same row. Show the saved
browser name when an override exists. Both settings belong to one editing task.
The Help & defaults trigger has a compact button-shaped boundary and familiar
help symbol; native summary semantics handle its keyboard and expanded state.
It contains optional explanation and a pending reset-to-package choice; Apply
commits the group's edits and resets together. Define failure behavior without
allowing an incomplete operation to look successful.
Both settings stay in the same section; no separate editor navigation is needed.
At narrow widths, wrap status/actions predictably beneath the label rather than
shrinking text or clipping controls. The complete label is a checkbox target.

Checkboxes represent enabled state, not inheritance. An unchecked saved override
and an unchecked inherited default are different states; the adjacent origin and
explicit reset preserve that distinction. Applying an unchanged inherited option
must not turn it into an override. If the provider's enum has no explicit
off value, keep its native selector instead of inventing a boolean interpretation.
Conditional fields need server validation; CSS hiding is not disabling.

The superseded arrangements used per-option Save buttons, two illustrated cards,
large Off/None summaries,
category labels, paragraphs and separate Change links. Those elements increased
scanning distance and navigation without improving this two-setting decision.

## Comparison work is different from isolated configuration

For numbering a mixed media list, titles, dates, runtimes and current season/episode
values are decision evidence. Keep them aligned and visible together. A nearby
date-gap cue may help scanning; it must not silently classify a holiday special
or turn trailers and bloopers into episodes. Keep selection distinct from
inclusion and offer a focused action for the selected scope.

Adding a wizard would make users carry this evidence in memory between steps.
Removing columns to make the list visually sparse would have the same problem.
Here useful density reduces overhead.

## Review rubric

Walk through an actual user task and record a short before/after assessment:

| Question | Evidence to inspect |
| --- | --- |
| Can I find the next control without reading every paragraph? | Label order, visible values, action placement and competing decorations. |
| Can I compare what matters without changing views? | Simultaneously visible evidence, number of context switches, lost selection or draft state. |
| Can I tell what this action changes? | Scope, units, defaults, saved versus edited state and visible consequences. |
| Is optional depth still easy to retrieve? | Specific disclosure labels, no nested help maze, required/error fields exposed. |
| Does compactness hold under real conditions? | Long labels, narrow widths, keyboard focus, zoom/reflow and actionable hit areas. |
| Did anything disappear or become ambiguous? | Preservation inventory, reset behavior, mixed/unknown values and original save contracts. |

Use the assessment to change the design, not just describe it. Report measured
steps or browser behavior as such; call expected ease a hypothesis until people
have tried the task. Avoid arbitrary claims about attention spans, fixed limits
on the number of settings or quantified cognitive-load reductions.
