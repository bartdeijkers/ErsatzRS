# BUILD

How to build, run, and iterate on ErsatzRS. The dev cycle is the bottleneck for
the port — keep this file accurate when you change profile flags, leptos
metadata, or `run.sh`.

For the test pyramid and Playwright playbooks, see [TEST.md](TEST.md).

## Prerequisites

- Rust toolchain pinned at MSRV `1.85` (workspace `Cargo.toml`).
- `cargo install cargo-leptos --locked` (the web crate is built through it,
  not plain cargo, because of the SSR + WASM dual target).
- Network access for the first managed FFmpeg runtime download, or an explicit
  FFmpeg 8.1+ override/pre-placed managed runtime for offline work. Startup
  validates the managed `ErsatzRS-ffmpeg` runtime for `drawvg`, `zscale`, and
  required codecs before serving. `run.sh` still exports local
  `~/.local/ffmpeg-8.1/` overrides when that developer build exists.
- `SQLX_OFFLINE=true` is already set in `.cargo/config.toml`. The repo ships
  the `.sqlx/` cache so query macros compile without a live database.
- `wasm-bindgen-cli` is installed transitively by `cargo-leptos` and **must**
  match the `wasm-bindgen` crate version in the workspace lockfile. If
  hydration breaks after a `cargo update`, re-run
  `cargo install cargo-leptos --locked` to refresh the matching CLI.

## Quick reference

| Command          | What it does                                                |
| ---------------- | ----------------------------------------------------------- |
| `./run.sh`       | Alias for `dev`.                                            |
| `./run.sh dev`   | `cargo leptos watch` — edit-save-refresh hot loop.          |
| `./run.sh build` | One-shot `cargo leptos build` + `cargo run`. Matches CI.    |
| `./run.sh serve` | Rebuild once, serve via cargo-leptos (no watch).            |
| `./run.sh seed`  | Apply `tests/fixtures/seed_admin.sql` to the SQLite db.     |
| `./run.sh log`   | `tail -F ersatzrs.log`.                                     |

On Windows, use [`run.cmd`](run.cmd) (or `pwsh -File run.ps1`) with the same
subcommands. See [Windows development](#windows-development) below.

For the longer commands (clippy, test, deny, snapshot review), see the
"Commands" block in [AGENTS.md](AGENTS.md).

`./run.sh dev`, `./run.sh build`, `./run.sh serve`, and
`cargo xtask parity-check` check `target/` size before starting expensive
build/test work and warn when it exceeds the 40 GiB `cargo clean` threshold.
When running broad cargo commands directly, check first with
`cargo xtask target-size`.

## Windows development

Windows contributors use [`run.ps1`](run.ps1) — a PowerShell 7+ port of
`run.sh` — invoked through the [`run.cmd`](run.cmd) shim so the same `dev` /
`build` / `serve` / `seed` / `log` subcommands work from any shell.

```powershell
.\run.cmd            # alias for `dev`
.\run.cmd dev        # cargo leptos watch
.\run.cmd build      # cargo leptos build + cargo run
.\run.cmd serve      # cargo leptos serve
.\run.cmd seed       # seed the SQLite db from tests/fixtures/seed_admin.sql
.\run.cmd log        # Get-Content -Wait equivalent of tail -F
```

`run.cmd` calls `pwsh` (PowerShell 7+); install it with `winget install
Microsoft.PowerShell` if it's not on PATH.

`run.ps1` looks for FFmpeg 8.1 at `%USERPROFILE%\.local\ffmpeg-8.1\bin` and
sets `ErsatzRS_FFMPEG__PATH` / `ErsatzRS_FFPROBE__PATH` automatically when
that directory exists. If your FFmpeg lives elsewhere, set those two
environment variables yourself. Without an explicit override, startup uses the
managed `ErsatzRS-ffmpeg` download/update path and falls back to `PATH` only if
managed acquisition is disabled or fails.

`run.ps1 seed` shells out to the `sqlite3` CLI; `winget install
SQLite.SQLite` (or any other source of `sqlite3.exe` on `PATH`) covers the
prerequisite.

## Dev loop (`run.sh dev`)

`cargo leptos watch` recompiles the SSR binary and the WASM bundle on every
save and triggers a browser LiveReload via the `reload-port = 3001` websocket
configured in `crates/ersatzrs-web/Cargo.toml` `[package.metadata.leptos]`.

- Edits inside `crates/ersatzrs-web/src/` rebuild in seconds.
- Edits inside `ersatzrs-infra`, `ersatzrs-core`, `ersatzrs-ffmpeg` trigger a
  full SSR binary relink — that takes longer (~20–60s warm).
- CSS edits go through `stylance` via `build.rs`. Save the `.module.css`,
  watch picks it up, the bundle regenerates into `public/bundle.css`.

If the browser does not auto-refresh, check that nothing else is bound to port
`3001` and that the page was loaded via `cargo leptos`'s server (not a stale
`cargo run --bin ersatzrs-web` from a previous session).

## Profile tuning

The workspace `Cargo.toml` has these dev profile blocks (added so the dev loop
is usable, not blazingly fast — it's still a Rust + WASM project):

```toml
[profile.dev]
opt-level = 0
debug = "line-tables-only"     # smaller debuginfo, faster link
split-debuginfo = "unpacked"   # Linux only
incremental = true

[profile.dev.package."*"]
opt-level = 1                  # all deps -O1: dev runtime ~2-5x faster

[profile.dev.package.mlua]
opt-level = 0                  # vendored Lua C build is slow; -O1 buys little

[profile.dev.package.sqlx-macros]
opt-level = 0
```

Trade-offs:

- `opt-level = 1` for all transitive deps adds **~30s to a clean build** but
  makes mlua/sqlx/leptos runtime much snappier in dev. Worth it for the
  watch loop.
- `split-debuginfo = "unpacked"` shrinks the resulting `target/` and speeds
  up linking on Linux. On other OSes, drop or change this line.
- `target/` will be larger than before — the per-package optimisation level
  generates more codegen artifacts. Run `cargo clean` when `target/` grows
  beyond 40 GiB; between cleanups, use
  `cargo install cargo-sweep && cargo sweep --time 30` to prune stale
  artifacts while preserving the warm cache.

## Cross-compiling

ErsatzRS targets the same six RIDs as ErsatzTV (`linux-x64`, `linux-arm64`,
`linux-arm`, `osx-x64`, `osx-arm64`, `win-x64`). Targets are installed on
demand: CI installs the matching target per matrix cell, and locally you add
them with `rustup target add`.

For the targets the host can't compile natively, use
[`cross`](https://github.com/cross-rs/cross). Configuration lives in
[`Cross.toml`](Cross.toml). `cross` requires a working Docker or Podman, and
its containers ship the target stdlib already so no `rustup target add` is
needed when cross-building.

```bash
cargo install cross --git https://github.com/cross-rs/cross --locked

# Linux ARM64 (linux-arm64)
cross build --workspace --target aarch64-unknown-linux-gnu

# Linux ARMv7 (linux-arm)
cross build --workspace --target armv7-unknown-linux-gnueabihf

# Windows x86-64 (win-x64) — uses MinGW for cross-from-Linux verification.
# Native Windows CI runners use the MSVC target instead.
cross build --workspace --target x86_64-pc-windows-gnu
```

macOS targets are **not** supported by `cross`. Build them on a real macOS
host (or `macos-latest` / `macos-13` GitHub runners):

```bash
cargo build --workspace --target aarch64-apple-darwin   # osx-arm64
cargo build --workspace --target x86_64-apple-darwin    # osx-x64
```

Notes:

- The workspace uses `reqwest` with `rustls-tls` only; no OpenSSL native libs
  are needed, so the default `cross` images work without `pre-build` apt
  packages.
- `SQLX_OFFLINE=true` is exported via [`.cargo/config.toml`](.cargo/config.toml)
  and passed through `Cross.toml`'s `passthrough` list. The shipped `.sqlx/`
  cache is required.
- `cargo-leptos` and the WASM hydration bundle are **only** built on the
  Linux host that ships the release. The other RIDs ship the SSR binary
  plus the prebuilt `target/site/` assets from that host build.

## Compile-cost notes

The two crates that dominate cold build time:

- **`mlua` with `vendored` (in `ersatzrs-core`)** — compiles Lua 5.4 from C.
  Required (controlled scripted-scheduling runtime). The C compile runs once
  per profile per target; switching profiles invalidates the cache.
- **`sqlx`** — declared once in `[workspace.dependencies]` so all four crates
  (`ersatzrs-infra`, `-web`, `-scanner`, `-scheduler`) share a single sqlx
  compilation with feature union `runtime-tokio,sqlite,migrate,chrono,uuid,macros`.
  `sqlx-macros` is held at `opt-level = 0` because optimisation buys nothing
  for proc-macro crates.

Common things that look like a "rebuild bug" but aren't:

- Touching anything in `ersatzrs-core` triggers an mlua relink path. This is
  expected — the Lua VM lives in `ersatzrs-core`.
- Editing a `.module.css` only regenerates the stylance bundle; if the page
  doesn't reflect the change, hard-reload the browser (cache).
- Adding a feature flag to any crate that pulls in `sqlx` recompiles sqlx
  with the new feature union. Keep features behind `[workspace.dependencies]
  sqlx` whenever possible.

## Common errors

| Symptom                                          | Likely cause                                            | Fix                                                                                       |
| ------------------------------------------------ | ------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `failed_to_cast_marker_node` in browser console  | Hydration DOM shape mismatch (conditional rendering).   | See [TEST.md](TEST.md) hydration rules — replace `cond.then(...)` with `style:display`.   |
| `wasm-bindgen` version mismatch                  | `wasm-bindgen-cli` and crate drifted after `cargo update`.| `cargo install cargo-leptos --locked`.                                                  |
| `cargo leptos: command not found`                | cargo-leptos not installed.                             | `cargo install cargo-leptos --locked`.                                                    |
| Server starts but admin buttons do nothing       | Page's interactive body is `#[component]`, not `#[island]`, OR the SSR binary and WASM bundle disagree on hashing (see "Hydration build consistency"). | See [TEST.md](TEST.md) hydration playbook; for the build-consistency case run `scripts/release/verify-web-bundle.sh`. |
| `failed to read hash file` panic / blank admin UI after release | "wasm checksum mismatch": server expects hashed `/pkg` names the bundle lacks. | See "Hydration build consistency" below.                                |
| `SQLX_OFFLINE`-related compile errors            | `.sqlx/` cache out of date after schema changes.        | `DATABASE_URL=sqlite://./tmp.db cargo sqlx prepare --workspace`.                          |
| FFmpeg missing / wrong version at startup        | Managed download disabled/failed and no compliant override or `PATH` runtime exists. | Re-enable downloads, pre-place the managed runtime, or set `ErsatzRS_FFMPEG__PATH` / `ErsatzRS_FFPROBE__PATH`. |

## Hydration build consistency (the "wasm checksum" trap)

The Leptos SSR server binary and the WASM hydration bundle **must come from a
consistent build**, or the admin UI silently fails to hydrate (buttons/dialogs
dead, lists non-interactive) even though the page renders. This has broken
several releases. Two distinct failure modes:

1. **Separate builds.** `cargo leptos build` produces a matched binary + WASM.
   Building the server separately (`cargo build --bin` / `cargo run --bin`
   after `cargo leptos build`) can yield a binary that is hydration-incompatible
   with that WASM. The dev recipe `cargo leptos build` then `cargo run --bin`
   works only because the binary defaults agree; prefer `./run.sh dev`
   (`cargo leptos watch`) or `./run.sh serve`, which keep them in lockstep.
2. **File-hashing mismatch ("wasm checksum mismatch").** cargo-leptos can hash
   `/pkg` asset filenames (writing `site/pkg/hash.txt`). If the WASM build and
   the running binary disagree about hashing, the server emits
   `/pkg/ersatzrs-web.<hash>.{js,wasm}` URLs the bundle does not contain (or
   panics with `failed to read hash file`). Hashing is pinned **off** in
   `crates/ersatzrs-web/Cargo.toml` (`[package.metadata.leptos] hash-files =
   false`) and in `.github/workflows/release.yml` (`LEPTOS_HASH_FILES: "false"`
   on **both** the web-assets and binaries jobs). Keep those in agreement.

**Structural gate:** `scripts/release/verify-web-bundle.sh` boots a built server
binary against a site-assets directory and fails if any `/pkg` asset the shell
HTML references does not resolve (HTTP 200). The release workflow runs it against
the real release artifacts (linux-x64) before publishing, so a checksum mismatch
fails the release instead of shipping. Run it locally against any bundle:

```bash
ERSATZRS_WEB_BIN=target/release/ersatzrs-web \
ERSATZRS_SITE_ROOT=target/site \
  scripts/release/verify-web-bundle.sh
```

For a deeper, browser-level check (every island actually hydrates), use
`scripts/e2e/hydration-smoke.sh` (see [TEST.md](TEST.md)). Normal CI runs that
browser smoke only for web, Leptos, E2E, workflow, Cargo, Node, and hydration
policy changes; release CI always keeps the structural web-bundle gate above.

## Performance baseline

Record measurements here when you change profile flags or `run.sh`. Format:
`<date> — <command> — <cold s> / <warm s>`.

### Linker investigation (negative result, 2026-05-17)

The reported "10+ minute" builds are the **cold-rebuild path forced by an
oversized `target/`** (observed at 102–136 GB), not a linker problem — on a
healthy `target/` the cold cost is ~2.5 min and warm rebuilds are seconds.
Keep `target/` pruned: run `cargo sweep --time 30` routinely (prunes only
artifacts older than 30 days, preserves the warm cache) and reserve the heavy
`cargo clean` for when `target/` exceeds 40 GiB or when reclaiming all space.
See the disk-space note in
[CLAUDE.md](CLAUDE.md).
