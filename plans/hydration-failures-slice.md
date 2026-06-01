# Admin UI Hydration Failures Slice

This is a hardening slice, not a new-page port. The goal is to bring every
admin page that still fails Leptos island hydration to a state where its
interactive handlers reliably attach and the round-trip server fn fires, with
browser-proven evidence. There is no new ErsatzTV C# surface to port; the
authoritative contract is the repo's own hydration rules.

## Goal

Each affected route is `✅` in **both** the STATUS.md *Admin UI Hydration
Parity* table and the TEST.md *Per-page E2E index*, the two trackers agree, and
`cargo xtask hydration-lint -p ersatzrs-web` is clean for the touched files.

## Critical First Step — Ground-Truth Reconciliation

The two trackers contradict each other and one is stale. They cannot both be
right; reconcile before writing any fix.

- **STATUS.md** *Admin UI Hydration Parity* table (snapshot of the 2026-05-04
  Playwright migration) marks ❌: `LibrariesPage`, `SearchPage`,
  `ChannelsListPage`, `FfmpegProfilesListPage`, `FfmpegProfileEditorPage`,
  `LocalLibrariesPage`.
- **TEST.md** *Per-page E2E index* marks the same routes ✅:
  `/media/libraries`, `/search`, `/channels`, `/ffmpeg/profiles`,
  `/ffmpeg/profiles/:id`, `/media/sources/local`.

Reconciliation steps:

1. Run `cargo xtask hydration-lint -p ersatzrs-web --no-baseline` to see the
   full structural debt and which files it implicates.
2. Start the server (`./run.sh dev`) and run a Playwright MCP probe against each
   disputed route: navigate, capture console, exercise exactly one interactive
   handler that calls a server fn. Record `failed_to_cast_marker_node` or any
   hydration error as the regression target.
3. Produce a ground-truth table (route → actually broken? → failure class) and
   update whichever tracker is stale so they agree. Scope the rest of the slice
   to only the routes that genuinely fail.

## Source Map

This slice is governed by the repo's hydration contract rather than ErsatzTV
source files.

- Hydration rules + containment playbook: `TEST.md:40-109`.
- Hydration lint: `cargo xtask hydration-lint -p ersatzrs-web`; known debt in
  `.hydration-lint-baseline`.
- Proven reference pages (correct patterns to mirror):
  - `crates/ersatzrs-web/src/admin/pages/libraries/mod.rs` — `#[component]`
    wrapper + `#[island]` body (`:18-24`), stable DOM shape, wasm32-gated
    polling `Effect` (`:39-56`).
  - `crates/ersatzrs-web/src/admin/pages/media_sources/plex.rs` —
    `PlexSignInIsland` all-arms always rendered.
  - `FfmpegProfileEditorIsland` — island owns a `Resource::new_blocking`, no
    `Suspense` wrapper, plain id passed from the page wrapper.
- ErsatzRS target modules (touched as the reconciliation proves necessary):
  - `crates/ersatzrs-web/src/admin/pages/libraries/`
  - `crates/ersatzrs-web/src/admin/pages/search.rs`
  - `crates/ersatzrs-web/src/admin/pages/channels/`
  - `crates/ersatzrs-web/src/admin/pages/ffmpeg_profiles/`
  - `crates/ersatzrs-web/src/admin/pages/media_sources/` (local)
  - `STATUS.md`, `TEST.md`, `docs/reference/parity-matrix.md`
- Intentional deviations:
  - `ChannelEditorPage` is intentionally an SSR multipart form (TEST.md
    hydration rule 3). Do not islandify it; do not use it as an island
    reference.

## Fix Patterns (from TEST.md "Hydration rules")

1. Page wrapper stays `#[component]`; interactive body is `#[island]`. Only
   handlers declared inside `#[island]` are attached by `hydrate_islands()`.
2. Stable DOM shape. Replace `cond.then(|| view!{…})` and `<Show when=…>` with
   always-render + `style:display=move || if cond {""} else {"none"}` and
   `disabled=move || !cond`. **Never `<Show>` inside `#[island]`**, including
   `#[component]`s called inside islands.
3. No prop-bearing islands across `Suspense`. Pass only a plain id; the island
   owns a `Resource::new_blocking` and seeds signals from `data.get_untracked()`.
4. `DeleteDialog` always rendered; control visibility via its
   `open: RwSignal<bool>`; derive `entity_name` via `Signal::derive(...)`.
5. Server-state polling via the wasm32-gated `Effect` +
   `gloo_timers::future::TimeoutFuture` pattern at `libraries/mod.rs:39-56`.

Follow the containment playbook: classify the failure, write one browser probe
before editing, patch the narrowest stable boundary, and **do not touch shared
UI** (`letter_bar`, pagers, shared modals, `HydrationScripts`,
`hydrate_islands`, router, Cargo deps) unless a probe proves that boundary is
the source. If a shared-primitive root cause emerges, stop and split it into a
dedicated hydration-hardening task rather than widening this slice. For Leptos
0.8 API questions use Context7, not generic examples.

## Acceptance

- [ ] Ground-truth reconciliation table produced; STATUS.md and TEST.md agree
      on which routes are broken.
- [ ] Each genuinely-broken route's interactive handler fires and the
      round-trip server fn completes, proven by a Playwright MCP probe.
- [ ] No `failed_to_cast_marker_node` or other hydration error in the console
      for each touched route on a clean rebuilt run.
- [ ] `cargo xtask hydration-lint -p ersatzrs-web` clean for touched files;
      each fixed line removed from `.hydration-lint-baseline`.
- [ ] No shared UI primitive changed without a browser probe proving it is the
      source.
- [ ] STATUS.md *Admin UI Hydration Parity* table and TEST.md *Per-page E2E
      index* rows flipped to ✅ and consistent with each other.
- [ ] Parity docs, status, and this plan file updated after final verification.

## ParityDebt

- Open until the slice lands. Any shared-primitive root cause discovered but
  deliberately deferred is recorded here and split into its own
  hydration-hardening task, not silently widened into this slice.

## Verification

- [ ] `cargo build -p ersatzrs-web`
- [ ] `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`
- [ ] `cargo xtask hydration-lint -p ersatzrs-web` (zero new findings)
- [ ] `cargo xtask parity-check web`
- [ ] Per route: Playwright MCP re-probe shows the handler fires and the console
      is free of hydration errors.
- [ ] Human-side gates before commit: `cargo fmt --all -- --check`,
      `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
      `cargo test --workspace --all-features`, `cargo leptos build` from
      `crates/ersatzrs-web`, and Playwright MCP smoke per the playbook for each
      touched page.
