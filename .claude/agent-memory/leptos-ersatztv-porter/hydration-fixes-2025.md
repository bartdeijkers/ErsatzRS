---
name: hydration-fixes-2025
description: Concrete remedies for the codebase-wide failed_to_cast_marker_node panic — focus on Suspense+prop:value and modal backdrop click handling.
metadata:
  type: feedback
---

When porting MudBlazor dialogs / forms with server-loaded selects, never put
`<Suspense>` directly around a `<select prop:value=...>` (or any element with
a `prop:` binding) inside an `#[island]`. SSR resolves the resource and emits
the populated element; the client's first hydration tick sees the Suspense
fallback shape instead, and the marker-node cast panics. Hydration aborts —
every `on:click` after the panic point silently fails to attach. Symptoms:
buttons look enabled but do nothing; sometimes only some buttons hydrate
depending on DOM order.

**Why:** documented in [[movies-page-port]] note 2; re-triggered on
2026-05-13 by the movies multi-select modals, breaking the Cancel/Confirm
buttons inside Add-to-Collection and Add-to-Playlist plus the
"Add To Playlist" header button.

**How to apply:**

1. Use `Resource::new_blocking` for any data backing a `prop:value` element
   inside an island.
2. Drop the inner `<Suspense>` wrapper. Render the element directly with a
   stable shape and source its dynamic children from
   `resource.get().unwrap_or_default()`. SSR and the first client tick will
   agree because the resource resolves blocking server-side and the
   serialized value is on the page for the client.
3. If you absolutely need a loading state, put it *outside* the
   `prop:value` element (e.g. a disabled flag driven by a separate signal)
   — never as a Suspense fallback that swaps the element shape.

**Modal backdrop click-to-close pattern** (same file:
`crates/ersatzrs-web/src/admin/ui/primitives/dialog.rs`):

The `.backdrop` (z-index 40) and `.overlay` (z-index 50, `inset: 0`) both
cover the viewport. The overlay always intercepts pointer events, so any
`on:click` placed on the backdrop is unreachable. Put the dismiss handler
on the overlay element and keep `on:click=|e| e.stop_propagation()` on the
inner panel.

Files involved when last hit:
- `crates/ersatzrs-web/src/admin/ui/primitives/dialog.rs`
- `crates/ersatzrs-web/src/admin/pages/media/movies.rs`
  (AddToCollectionModal / AddToPlaylistModal)
