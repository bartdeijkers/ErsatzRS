# Dialog

**Status:** Implemented
**Adoption:** Broad

## Sources

- [`dialog.rs`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/dialog.rs)
- [`dialog.module.css`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/dialog.module.css)

## Surfaces

| Surface | Purpose | Notable inputs |
|---|---|---|
| `Modal` | Present content followed by Cancel and Confirm actions | Reactive title and open state, labels, disabled confirmation, destructive and wide variants |
| `DeleteDialog` | Confirm deletion of a named entity | Reactive entity name and confirmation callback |

## Current behavior

- The backdrop and panel remain mounted and visibility changes through CSS, so
  the DOM shape stays stable across hydration.
- The overlay exposes `role="dialog"`, `aria-modal="true"`, and an accessible
  label derived from the title.
- Opening focuses the panel in the hydrated browser; closing restores the
  previously focused element when it is still available.
- Escape and an overlay click cancel. A click inside the panel does not close
  the dialog.
- The default and wide panels fit the viewport; the wide body becomes the
  internal overflow region.
- `DeleteDialog` uses the Danger confirmation variant and states that deletion
  cannot be undone.

## Current limitations

- Focus moves into and back out of the dialog, but the implementation does not
  currently contain Tab focus within the open modal.
- The visible heading and accessible label contain the same title but are not
  connected with `aria-labelledby`.
- Nine page CSS modules still contain modal or dialog selectors. They require
  semantic comparison before being treated as variants of this component.
