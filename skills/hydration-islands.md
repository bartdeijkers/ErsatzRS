---
name: hydration-islands
description: Work on ErsatzRS admin HTML, Leptos 0.8 WASM hydration islands, SSR/no-JS fallback forms, marker-node hydration failures, dead buttons/dialogs, browser hydration smoke, and hydration build-consistency bugs. Use when changing crates/ersatzrs-web admin UI components, shared UI primitives, Leptos server functions used from islands, cargo-leptos/WASM assets, or any task that mentions hydration, islands, wasm, failed_to_cast_marker_node, or interactive admin controls.
---

# Hydration Islands

Use this for admin UI implementation and debugging where SSR HTML, hydrated
Leptos islands, and browser behavior must all agree. For ErsatzTV parity work,
use this together with `parity-slice`: build the source/visual map first, then
apply the hydration rules here while implementing the page.

## Required Context

Read these before editing non-trivial web UI:

- `TEST.md` hydration rules and the page's E2E row.
- `BUILD.md` hydration build consistency section when assets, cargo-leptos,
  release packaging, `run.sh`, or dead controls after release are involved.
- `STYLE.md`, `ARCHITECTURE.md`, and `PORTING.md` when behavior or parity changes.
- The current page/module plus nearby working examples in `crates/ersatzrs-web/src/admin/`.
- Current Leptos 0.8 docs through Context7 when API syntax or framework behavior
  is uncertain; do not apply generic Leptos examples blindly.

## Source Map

Before editing an admin route, write a short map in the handoff, plan, or final
response:

```md
## Hydration Source Map

- Route/page:
- SSR wrapper:
- Island component(s):
- Data boundary:
- Server functions / API endpoints:
- Fallback HTML / no-JS path:
- Shared primitives touched:
- Browser proof target:
```

For parity UI routes, also include the visual source map from `parity-slice`.

## Boundary Rules

1. Keep the page wrapper as `#[component]`; move only interactive bodies into
   `#[island]`. `hydrate_islands()` attaches handlers declared inside islands.
2. Keep DOM shape stable inside islands. Do not use `<Show>` inside an island
   or inside a component called by an island. Avoid `cond.then(|| view! { ... })`
   for real subtrees. Prefer always-rendered nodes controlled by
   `style:display=move || if cond { "" } else { "none" }`, `hidden`, or
   `disabled=move || !cond`.
3. Do not render a prop-bearing island across an unresolved parent `Suspense`
   boundary. Either resolve the data before the island is emitted, or pass only
   stable scalar props such as an `i64` id and let the island own
   `Resource::new_blocking` plus its internal `Suspense`.
4. Keep dialogs and modals mounted. Control visibility with signals. For
   `DeleteDialog`, derive `entity_name: Signal<String>` and keep the dialog in
   the DOM instead of conditionally constructing it from an option.
5. For island-local navigation, prefer a small `set_browser_location` helper
   using `web_sys::window().location().set_href(...)` when router context is not
   guaranteed. Do not assume `use_navigate()` is available inside every island.
6. For server-driven state like scan locks, use a wasm32-gated `Effect` that
   spawns a `gloo_timers::future::TimeoutFuture` poll loop. Keep non-wasm code
   compiling with `#[cfg(target_arch = "wasm32")]` guards.
7. Do not move an entire SSR page into an island to attach one action. Prefer a
   narrow island or explicit JSON/POST endpoint and preserve the same
   persistence and side effects.

## HTML And Fallbacks

- Critical save/delete/copy actions need either a hydrated browser proof or an
  SSR/no-JS fallback. If hydration is intentionally required, say so and prove
  the hydrated path.
- Keep no-JS forms populated from server data, not from a client-only `Effect`.
- Avoid Leptos streaming marker surprises inside `<noscript>`. For no-JS
  fallback markup, use a proven plain form path or inspect the raw SSR HTML.
- User-facing text must go through `TextKey` and the language files in
  `crates/ersatzrs-web/src/admin/languages/`.
- Reuse existing CSS-module and primitive patterns. If styling changes, prove
  the running browser is serving the rebuilt CSS.

## Debugging Playbook

1. Classify the failure as SSR HTML shape, island boundary/runtime, server
   function/API, CSS/layout, data/query behavior, or bundle consistency.
2. Reproduce before editing. Use an existing Playwright spec when practical;
   otherwise use a browser probe to load the route, click one failing control,
   and capture console/page errors.
3. Patch the narrowest boundary that explains the evidence. Avoid shared
   primitive, router, `HydrationScripts`, or dependency changes until a browser
   probe points there.
4. Re-run the same browser probe after each meaningful change. Stop expanding
   the patch when the failing behavior is fixed.
5. If route or hydration tracker status changes, update
   `docs/progress/hydration.toml`, route/progress files, active plans, and the
   page's E2E row as appropriate.

## Verification Ladder

Use the smallest useful checks while iterating, then broaden for shared or
release-sensitive changes:

```bash
cargo check -p ersatzrs-web --target wasm32-unknown-unknown --no-default-features --features hydrate
cargo xtask hydration-lint -p ersatzrs-web
cargo test -p ersatzrs-web --all-targets <route_or_module_filter>
cargo clippy -p ersatzrs-web --all-targets -- -D warnings
```

For browser proof:

```bash
./run.sh serve
E2E_BASE_URL=http://127.0.0.1:8409 npx playwright test tests/e2e/<spec>.ts
```

For broad or release-sensitive hydration work:

```bash
scripts/e2e/hydration-smoke.sh
scripts/release/verify-web-bundle.sh
```

Browser proof should check:

- `window.__ersatzrs_hydrated === true` for island routes.
- No `failed_to_cast_marker_node`, uncaught page errors, or hydration-signature
  console errors.
- At least one real user interaction attaches and causes the expected UI,
  server-function/API, and persistence side effect.
- The route did not fall through to `[data-not-found]`.

## Report Shape

```text
Hydration source map
Boundary chosen and why
HTML/fallback behavior
Browser proof
Commands run
Tracker/docs updates
Remaining risk or ParityDebt
```
