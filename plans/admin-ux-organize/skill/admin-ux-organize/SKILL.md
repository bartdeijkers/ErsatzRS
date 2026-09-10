---
name: admin-ux-organize
description: Reduce human mental load in dense admin interfaces through compact, intuitive organization while preserving every setting, action, and information item. Use for settings overload, weak hierarchy, excessive cards or navigation, and comparison-heavy editing, including HTML/CSS prototypes and no-JavaScript admin flows.
---

# Organize admin UX

Act as an admin interaction designer specializing in human mental load. Make
information easier to find, compare and act on with less reading, remembering,
interpretation and navigation. Preserve capability and useful information.
Compact, practical controls are the default; visual treatment must help the task.

Respect the requested deliverable: a prototype does not authorize production
changes. Preserve excluded surfaces such as the header and navigation.

## Establish the task and information contract

Inspect the rendered page, forms, handlers and repository contracts. A supplied
screenshot or saved page is appearance evidence; source alone is insufficient.
Open saved pages without running their scripts. Use synthetic data in artifacts.

Inventory every setting, action, status, warning, explanation and conditional
branch. Record scope, dependencies, save boundary, inherited/effective value and
proposed destination. Preserve provider-defined fields, mixed and off-page
values, language/track information, disabled reasons and recovery actions. Give
each editable value one clear home. Trace input encoding and save/reset semantics
before replacing a control; simplification must not change its meaning.

Name the user's immediate job, what they change together, and the evidence needed
to do it before choosing components or tabs. Group by those
relationships, not database structure or implementation history. Separate jobs
only when doing so reduces effort; avoid preset tab counts and default wizards.

## Remove mental overhead before adding visual treatment

Walk through the task and identify concrete sources of effort:

| Effort imposed on the person | Design response |
| --- | --- |
| Hunting for the control or its current value | Use a stable label–value–action order, short task headings and familiar native controls. |
| Remembering information from another view | Keep comparison evidence and editing controls adjacent; retain context when opening an action. |
| Decoding unfamiliar icons, labels or status | Use domain terms, explicit units and readable state; reserve icons for familiar or useful visual cues. |
| Sorting repeated explanations and decoration from useful facts | Remove duplication, oversized summaries and ornamental containers; keep concise help at its point of use. |
| Deciding among actions unrelated to the current task | Make the frequent action direct; disclose optional depth under a specific label. |
| Guessing scope, inheritance or whether an edit was saved | Put that information beside the control or action; distinguish the saved value from the editable value. |

Read [mental-load design patterns](references/mental-load.md) for worked examples,
source guidance and a walkthrough rubric. Explain each substantial layout choice
by the effort it removes. Every prominent metric, image, heading or paragraph
needs a concrete decision to support; remove duplicate representations. Do not claim zero cognitive load or measured usability
without observed user evidence.

## Choose the smallest useful control arrangement

Start a boolean setting with one checkbox-led row. Put a related choice beside
it when needed. Use labeled rows for ordinary settings and aligned columns for
repeated records. A card needs a reason: a bounded object or meaningful group
with several related values/actions. A single toggle does not need an illustrated
card, large state heading, category badge and separate Change page.

Use proximity, alignment, restrained headings and consistent control placement
to establish hierarchy. Align repeated labels, controls, saved values and actions
on shared grid tracks; independently sized flex rows drift as their text changes.
Use Grid/subgrid for two-dimensional relationships and wrapping Flexbox for action
groups. Share gaps and control heights, and define how tracks stack when the drawer
or translated text reduces available width. Check alignment with help expanded
and mixed-length values; do not force unrelated tasks into identical columns.
Keep related rows compact while leaving clear separation
between groups. Compactness does not mean putting unrelated roles on one line:
status, a consequential action and diagnostic disclosure need distinct visual
positions. A disclosure is a control, not punctuation inside a status sentence.
Give expanded content a predictable region below its trigger; opening it must
not rearrange peer actions. Preserve readable text, visible focus, keyboard order, touch
targets and room for translated labels; reducing font size is not organization.

First-glance recognition should come from the task's working shape: adjacent
season/episode fields, aligned runtime/date columns, useful media thumbnails,
or chronological history. Use artwork or diagrams only when they convey evidence
or a relationship more efficiently than the controls themselves. Do not vary
composition merely to make every section look unique. Avoid decorative left-edge
accent bars and replacing them with equally unnecessary colored panels.

Keep current values, scope, blockers and action consequences visible. Keep short
hints inline when that is clearer than another interaction. Optional help can use
a compact, recognizable Help control with a visible boundary and focus state;
controls that also contain settings need an honest label such as Help & defaults.
Use distinct treatment for expandable settings and section details. Native
`details` provides behavior, not a requirement to repeat bare text-and-triangle
triggers everywhere. Preserve keyboard operation and perceivable expanded state.
Put longer optional explanations in accessible disclosure; hiding essential information adds recall
and navigation work. Repeated help can have one contextual home if it stays
reachable wherever needed. Do not collapse all settings to make a screenshot
look clean, or add another page just to edit a boolean.

## Preserve context and honest interaction

For comparison-led editing, keep the list and its evidence visible. Episode
classification needs neighboring titles, dates and runtimes, including trailers,
bloopers, specials and unknown data. Selection, numbering and publication
inclusion are separate states. Prefer direct editing and explicit selection;
do not require a wizard or central enable-editing step before working on a row.

Keep overflow menus short command lists. Show the chosen action's fields in a
focused editing surface, not nested forms inside every menu. Separate single-item
and checked-item scope. Research established task patterns when uncertain.

Group visible commits around a coherent user task, using one Apply changes action
where settings are edited together. Backend operations do not dictate one visible
button per endpoint. Preserve reset meaning, revision checks and complete payloads;
define grouped failure behavior explicitly. Keep Publish distinct from applying
settings. Do not silently default omitted fields. Native links cannot preserve
arbitrary unsaved inputs: design retained drafts before navigation. Local Cancel
closes only that action; Discard changes explicitly abandons the editing task.
A saved summary describes saved data, not a live preview of unsaved inputs.

## Work without JavaScript

Use semantic HTML/CSS and ordinary HTTP forms. Required interaction must not
depend on JavaScript, TypeScript, WASM, hydration or client event handlers. Read
[native interaction patterns](references/native-patterns.md) for mechanisms,
fallbacks, form ownership and conditional validation.

CSS can reveal controls and reflect checked state. It cannot disable inputs,
calculate authoritative impact, validate conditional payloads or persist edits.
Specify necessary server adapters and error responses; never substitute mock
success. Unknown provider fields still need their complete schema/save contract.

## Deliver and check the reduction in effort

Provide a task outline, preservation inventory and native/server interaction map.
Check the whole task before and after: open, compare, edit several values, apply,
cancel, change scope, navigate away and recover. Count context switches and competing
visual groups. Measure useful rows in the first viewport and while tools are open;
passing a no-overflow check alone does not establish a usable working area. Record
remaining friction. A smaller pixel height alone is not usability evidence.

Exercise the method on different tasks such as playback settings, mixed-list
numbering, artwork and recovery before treating it as a general rule. Avoid deriving
a universal card ban or one giant Apply form from a single correction.

Audit the instructions and examples for contradictions. Preserve capability,
accessibility and truthful state first; organize by the user's task and visual
relationships next; optimize density within those constraints. Preserving save
semantics does not preserve one button per endpoint. Removing duplicate prose
does not remove unique information. First-glance recognition comes from useful
task evidence, not decorative variety. Routine reversible edits need no extra
preview; use impact review for consequential or algorithmic bulk changes.

Review both collapsed and expanded states and the full edit/apply/return loop.
Record P1 findings for blocked tasks, lost work or major accessibility barriers;
P2 findings for credible wrong-scope actions, misleading state or substantial
recurring friction. Fix and recheck demonstrated findings, including shared
variants. Report remaining findings and coverage limits instead of claiming
that passing automated checks proves the absence of design flaws.

Verify with scripting disabled, keyboard operation, narrow and desktop widths,
both themes and long/localized labels. Check that essential evidence remains
visible during editing and that scope, reset and saved state are unambiguous.
Follow repository accessibility/localization contracts. Scope CSS so excluded
shell surfaces remain unchanged.

Label fixed previews as examples. If a synthetic server demonstrates edits,
state its actual validation and persistence boundaries; preserve inputs when
displaying responses. Production acceptance additionally needs
real HTTP submissions, retained invalid inputs, stale-revision rejection and all
inventoried capabilities. Screenshots prove appearance, not persistence or ease.
