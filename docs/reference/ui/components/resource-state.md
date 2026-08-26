# Resource state

**Status:** Implemented
**Adoption:** Broad

## Sources

- [`resource_state.rs`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/resource_state.rs)
- [`resource_state.module.css`](../../../../crates/ersatzrs-web/src/admin/ui/primitives/resource_state.module.css)

## Surfaces

| Surface | Purpose |
|---|---|
| `AdminLoadState` | Loading, Ready, Empty, and Failed presentation state |
| `AdminResourceState` | Stable `<section>` wrapper with state attributes |
| `AdminLoadingState` | Localized status message |
| `AdminErrorState` | Alert containing a localized heading and error text |
| `admin_loading_resource_view` | Loading wrapper helper |
| `admin_result_view` | Ready or failed result wrapper helper |

## Current behavior

- The wrapper remains a `<section data-admin-resource>` for every state.
- `data-state` is `loading`, `ready`, `empty`, or `error`.
- `aria-busy` is true only while loading.
- Loading uses `role="status"`; failure uses `role="alert"`.
- A focused unit test verifies the stable wrapper and state attribute values.

## Current evidence and limitations

- Resource-state symbols appear across 36 Rust files including the primitive
  definition.
- Empty is part of the state enum and wrapper contract, but there is no shared
  empty-message component.
- `admin_result_view` maps success to Ready; callers own the distinction between
  a ready value and an empty collection.
- Error content accepts a display string from the caller. The shared component
  localizes the heading but does not classify or recover from the error.
