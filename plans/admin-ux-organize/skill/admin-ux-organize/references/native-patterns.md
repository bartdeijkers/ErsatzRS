# Native interaction choices

Read this when a dense settings task needs local interaction without scripts.

| Need | HTML/CSS mechanism | Boundary |
| --- | --- | --- |
| Mutually exclusive behavior | Labeled radio inputs in a fieldset; style their labels with `:has(input:checked)` | Radios must represent real submitted choices, not hidden navigation switches. Keep their keyboard behavior and focus visible. |
| Applicable settings | Scoped `:has()` plus `@supports selector(:has(*))` | CSS hiding neither disables fields nor removes them from submission or validation. Server must ignore inactive branches. |
| Optional explanation/history | `details` with a meaningful `summary` | Give the control a stable position and its content a region below it. Do not splice it into status prose. Essential choices, blockers and error fields stay visible. |
| Related numbers | Visible labeled `input type="number"` controls, grouped with `fieldset`/`legend` and meaningful notation | Native steppers are not an authoritative numbering algorithm. Support typing, keyboard stepping, and domain-specific bounds. |
| Responsive relationships | Grid/flex, intrinsic sizing, wrapping, scoped media/container queries | Preserve DOM order and semantic reading order. A collapsed navigation drawer changes available content width. |
| Apply an editing task | Ordinary HTTP form and server-rendered response | Validate authoritative state, preserve inputs/errors and commit the coherent task. Ordinary reversible edits need no extra preview. |
| Review consequential or algorithmic changes | Server-rendered impact comparison, followed by an explicit Apply | Show exact scope and before/after values; bind Apply to the reviewed revision. |

Keep unsupported-CSS fallbacks useful: show applicable alternatives with notes
instead of hiding required controls by default. Avoid fake buttons, CSS counters
masquerading as live calculations, or an always-changing status animation.

Choose disclosure presentation by purpose. A brief hint may need no disclosure.
Optional help can use a small outlined summary with a familiar help symbol and
visible label; larger groups can use a full-width summary with a plus/minus
indicator. Avoid giving every hint, setting group and history section the same
bare triangle-and-text treatment. Hide the native marker only when supplying a
clear replacement affordance. Keep `summary` semantics and keyboard behavior;
do not nest a button inside it or hard-code an `aria-expanded` value that cannot
update without scripts. Decorative symbols should be hidden from assistive tools.

Conditional validation deserves explicit design. A hidden invalid number input
can block a native submit and cannot receive focus. CSS cannot toggle `disabled`
or `required`. Either use separate server-rendered forms for each mode, or use
`novalidate` for the conditional form and perform complete server validation of
the selected mode. The latter needs a returned error summary, retained values,
and visible affected fields. Also keep Cancel, Discard and draft-retaining
navigation usable when fields are invalid. Disclose which validation a synthetic
renderer demonstrates and which states are fixed examples; neither proves
production validation.

For episode numbering, retain the source algorithm: season zero may mean
specials; unnumbered can be a valid state; existing numbers may reserve slots;
excluded items, children, undated entries, and off-page candidates affect scope.
Do not show a consecutive strip as a computed result when those rules have not
run. Use it as a labeled conceptual example and provide a real server preview
in production.

Primary references:

- [MDN: :has()](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/:has)
- [MDN: number inputs](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/input/number)
- [WAI: grouping form controls](https://www.w3.org/WAI/tutorials/forms/grouping/)
