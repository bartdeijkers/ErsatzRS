# Web Server and WASM Hydration Crate Split

> **Type:** Structural refactor plan.
> **Status:** Proposed. Do only after the release Docker hotfix has proven
> stable, because this touches Leptos package metadata and release packaging.
> Coordinate with `plans/application-layer-and-video-jukebox.md`: application,
> core-port, infrastructure, and scheduler code stays native-only, while the
> hydration crate may share protocol DTOs but not concrete application services.

## Problem

`ersatzrs-web` currently serves two build roles:

- native SSR/admin/API server binary
- WASM hydration library for the Leptos admin UI

Because the package library is configured for both native and WASM consumers, a
native release build of the web binary can still involve the web library target
as both `rlib` and `cdylib`. That `cdylib` work is unnecessary for runtime
Docker images and release archives, but it is expensive on constrained runners
and especially expensive under QEMU.

The Docker hotfix can avoid the unused native `cdylib` work in the release image
path, but the crate shape still makes this easy to regress.

## Goal

Separate the native server runtime from the WASM hydration artifact so native
release builds compile only native code, while Leptos frontend builds compile
only the hydration library needed for the browser bundle.

The public executable name must remain `ersatzrs-web`.

## Non-goals

- Do not rewrite the admin UI.
- Do not change routes, server function endpoint names, localization behavior,
  IPTV endpoints, HDHomeRun endpoints, or scanner behavior.
- Do not change the release archive contents except for removing unnecessary
  native `cdylib` work.
- Do not weaken the hydration build consistency rules.

## Source Map

- `crates/ersatzrs-web/Cargo.toml`
  - Leptos package metadata and current library/bin target configuration.
- `crates/ersatzrs-web/src/main.rs`
  - native binary entry point.
- `crates/ersatzrs-web/src/lib.rs`
  - shared web crate entry points and hydration/server exports.
- `crates/ersatzrs-web/src/admin/`
  - SSR and hydrated admin UI modules.
- `.github/workflows/release.yml`
  - `web-assets`, `binaries`, and `docker` release jobs.
- `Dockerfile.release`
  - currently patched to avoid unused web `cdylib` work in Docker release
    builds.
- `scripts/release/verify-web-bundle.sh`
  - release gate for SSR/WASM asset consistency.

## Proposed Shape

Split by build role, not by route ownership:

- Keep the user-facing binary as `ersatzrs-web`.
- Move WASM hydration entry points and Leptos frontend package metadata to a
  dedicated hydration package.
- Keep native server bootstrap, transport routes, API routes, background tasks,
  and release binary behavior in a native package that only needs an `rlib` for
  its own binary.
- Keep shared UI/server-function code in modules where Leptos can still compile
  both SSR and hydrate configurations without endpoint renames.

The exact package names should be chosen during implementation after a source
map pass. A likely shape is:

- `crates/ersatzrs-web` remains the package that owns the `ersatzrs-web` binary.
- A new package such as `crates/ersatzrs-web-hydrate` owns the WASM `cdylib`
  target and Leptos frontend build metadata.
- Shared admin UI modules either stay in `ersatzrs-web` behind cfg boundaries or
  move to a third shared package if that is the smaller, cleaner boundary.

## Implementation Plan

### Phase 1 - Source Map and Build Graph Proof

- Record the current native build graph for:
  - `cargo build --release --bin ersatzrs-web`
  - `cargo leptos build --release --frontend-only --lib-features hydrate`
- Identify which modules are needed by native SSR only, hydrate only, and both.
- Identify which target currently causes native `cdylib` work.

### Phase 2 - Introduce Hydration Package

- Add the new hydration package with the minimum Leptos metadata needed for the
  frontend bundle.
- Move only the WASM entry point first.
- Keep generated asset names compatible with the release web-assets contract,
  especially `LEPTOS_OUTPUT_NAME=ersatzrs-web`.
- Verify `cargo leptos build --release --frontend-only --lib-features hydrate`
  still produces `public/bundle.css` and `target/site/pkg`.

### Phase 3 - Native Package Cleanup

- Remove `cdylib` from the native server package once the hydration package owns
  the browser artifact.
- Keep the `ersatzrs-web` executable name and server bootstrap unchanged.
- Keep server function names stable so generated endpoint URLs do not change.
- Update release archive and Docker build commands only where package names or
  Leptos paths require it.

### Phase 4 - Release Wiring and Regression Gates

- Update `.github/workflows/release.yml`, `Dockerfile.release`, and
  `docker-bake.hcl` only as needed for the new package layout.
- Remove any Docker manifest patching that exists only to suppress native
  `cdylib` work.
- Add a focused check that fails if native release Docker or archive builds
  invoke a web `cdylib` target.

## Acceptance Checks

- `cargo build --release --bin ersatzrs-web` does not build a native web
  `cdylib`.
- `cargo leptos build --release --frontend-only --lib-features hydrate` still
  produces the expected Leptos assets.
- `scripts/release/verify-web-bundle.sh` passes for the assembled Linux bundle.
- Docker release builds do not need to edit `crates/ersatzrs-web/Cargo.toml`.
- Admin hydration smoke coverage still proves interactive islands work after
  the split.
- Server function endpoint names and admin routes remain stable.

## Risks and Open Questions

- Leptos package metadata may assume the binary and hydration library live in
  the same package; verify before moving more than the entry point.
- Server functions cross SSR and hydrate compilation boundaries. Endpoint names
  must stay stable.
- Generated asset paths and output names are release-critical because archive
  binaries and web-assets are built in separate jobs.
- A third shared UI package might reduce cfg noise, but it should be introduced
  only if the source map shows real duplication or circular dependency pressure.
