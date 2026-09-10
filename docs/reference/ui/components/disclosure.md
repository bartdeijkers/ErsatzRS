# Section disclosure

**Status:** Candidate
**Adoption:** Add-on list task pages and their locked prototype; no shared Rust primitive claimed

## Sources

- `plans/admin-ux-organize/{index,settings,options,item-1,activity}.html`
- `plans/admin-ux-organize/{styles,organize,alignment}.css`
- `plans/admin-ux-organize/verify-ux-fixes.cjs`
- `crates/ersatzrs-web/src/admin/pages/media/addon_lists/workspace/`
- `tests/e2e/addon-list-workspace.spec.ts`
- `tests/e2e/addon-list-workspace-visual.spec.ts`
- `tests/e2e/addon-list-item-form.spec.ts`

## Contract

A native `details.disclosure` groups one optional task or supporting detail.
Its `summary` names the content and may include a short description. A full-width
summary, visible focus and plus/minus indicator expose the state; the indicator
is decorative because native details provides the accessible expanded state.
Opening one section does not close another. Use one decorative toggle; expanded
metadata keeps its credits in a plain subsection, without a nested task accordion.

Keep current outcomes, errors, primary editing fields and Save/Discard outside
optional disclosures. Preserve original form ownership inside them. Validation
must reveal invalid controls and make the error reachable, including with no
JavaScript. Long summaries wrap at 320px without hiding the toggle.

## Variants and limits

This section pattern is distinct from a compact contextual Help disclosure and
from a row-command popover. Similar borders do not make them interchangeable.
The prototype uses sections for the bulk workspace, publication details,
optional numbering limits and longer metadata/credits editors. Routine bulk
fields, playback settings, storage thresholds, managed locations and source
actions stay visible. Do not nest task disclosures inside an already opened
workspace merely to give every group the same styling. Summaries open the task
in place instead of adding separate jump links to the same disclosure. Production
native forms retain task drafts across submissions and use all supported cultures.
The pattern remains a shared-component candidate: its scoped production markup
and styling currently belong to the add-on list task pages. AMM-N-01B records the completed local visual and capability acceptance.
