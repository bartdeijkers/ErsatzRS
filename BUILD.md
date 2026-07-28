# BUILD

How to build, run, and iterate on ErsatzRS. The dev cycle is the bottleneck for
the port — keep this file accurate when you change profile flags, leptos
metadata, or `run.sh`.

For the test pyramid and Playwright playbooks, see [TEST.md](TEST.md).

## Prerequisites

- Rust `1.97.1` exactly, pinned in `rust-toolchain.toml` and required by the
  workspace `Cargo.toml`. CI, release jobs, and Docker builders use the same
  version.
- `cargo-nextest` for the required focused/local Rust test loop. Install a
  prebuilt release using the
  [official instructions](https://nexte.st/docs/installation/pre-built-binaries/).
  It is optional only for CI; the local closeout wrapper uses it before running
  Cargo doctests.
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
| `./run.sh dev`   | Build the scanner companion, then run the Leptos hot loop.  |
| `./run.sh check` | Fast inner-loop `cargo check` (no codegen/link); scope with `-p <crate>`. |
| `./run.sh test`  | Focused/local nextest run; requires explicit crate/filter arguments. |
| `./run.sh test-closeout` | Workspace nextest run followed by Cargo doctests. |
| `./run.sh build` | Build the scanner companion, then build and run the web host. |
| `./run.sh serve` | Build the scanner companion, then rebuild and serve once.   |
| `./run.sh seed`  | Apply `tests/fixtures/seed_admin.sql` to the SQLite db.     |
| `./run.sh log`   | `tail -F ersatzrs.log`.                                     |

Run `cargo xtask architecture-lint` after changing crate dependencies,
`ersatzrs-app`, or production admin data loaders. It enforces ADR 0051 and
requires the exact debt counts in `.architecture-lint-baseline.toml` to shrink
with migrated vertical slices.

On Windows, use [`run.cmd`](run.cmd) (or `pwsh -File run.ps1`) with the same
subcommands. See [Windows development](#windows-development) below.

For the longer commands (clippy, test, deny, snapshot review), see the
"Commands" block in [AGENTS.md](AGENTS.md).

`./run.sh dev`, `./run.sh build`, `./run.sh serve`, and
`cargo xtask parity-check` check `target/` size before starting expensive
build/test work and warn when it exceeds the 40 GiB `cargo clean` threshold.
When running broad cargo commands directly, check first with
`cargo xtask target-size`.

## CI runner selection

Pull requests and pushes to `main` use the repository's self-hosted Linux
runner by default. To run any branch manually on GitHub-hosted Ubuntu after
this workflow is present on `main`, dispatch it with:

```bash
gh workflow run ci.yml --ref <branch> -f runner=github-hosted
```

The Actions UI exposes the same choice under **CI → Run workflow**. Select the
target branch and choose `github-hosted`. Manually dispatched CI uses a
runner-pool-specific branch concurrency group, so hosted fallback can proceed
independently from automatic self-hosted CI. Routine pull-request labels do not
trigger CI; this prevents dependency automation from starting duplicate
full-suite runs while applying labels. GitHub-hosted jobs start cold and do not
share the local runner's Cargo cache. Hosted Test work is split into non-web,
web-library, and web-binary shards, uses the runner's native Cargo parallelism,
omits test-profile debug info, and uses LLVM's `lld` linker so cold all-features
linking stays within the hosted job lifetime. Hosted runs cache their pinned
Cargo tools; self-hosted runs leave the maintainer's existing tool binaries
untouched. The full test selection remains intact, and self-hosted CI retains
the single canonical workspace Test job. This path remains an explicit
fallback rather than the default.

Because the self-hosted runner is a single physical machine, every job in
`ci.yml` queues and runs serially on it — there is no wall-clock parallelism
to lose by sharing build output across jobs. `ci.yml` sets a workflow-level
`CARGO_TARGET_DIR` pointing at `github.workspace`'s parent directory, which
the runner creates once per repo and `actions/checkout`'s default `git clean
-ffdx` never touches (it only cleans inside the checkout itself). This lets
`target/` persist across every self-hosted job and run instead of being wiped
and restored through `Swatinem/rust-cache`'s remote cache service every time
— the `clippy`, `test`, and `hydration` jobs on that path no longer restore a
remote cache at all; only the GitHub-hosted fallback still does, since its
VMs are ephemeral. If a self-hosted CI run ever looks inconsistent with its
diff (a false pass or fail that doesn't match the change), suspect a stale
persistent `target/` first: either dispatch the workflow with the
`reset_target_dir` input set to `true`, or manually remove the directory on
the runner. This is unrelated to the local interactive dev target-dir and its
stale-binary trap below — this mechanism only sets `CARGO_TARGET_DIR` inside
the GitHub Actions job environment and never touches `.cargo/config.toml`.

## Windows development

Windows contributors use [`run.ps1`](run.ps1) — a PowerShell 7+ port of
`run.sh` — invoked through the [`run.cmd`](run.cmd) shim so the same `dev` /
`build` / `serve` / `seed` / `log` subcommands work from any shell.

```powershell
.\run.cmd            # alias for `dev`
.\run.cmd dev        # build scanner companion + cargo leptos watch
.\run.cmd check -p ersatzrs-core
.\run.cmd test -p ersatzrs-core scheduling
.\run.cmd test-closeout
.\run.cmd build      # build scanner companion + web bundle, then run
.\run.cmd serve      # build scanner companion + cargo leptos serve
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

The wrapper first runs an incremental build of the `ersatzrs-scanner` binary in
the same Cargo target directory as the web host. The maintenance scheduler can
then launch that sibling binary for automatic library refreshes instead of
skipping due scans. `cargo leptos watch` subsequently recompiles the SSR binary
and the WASM bundle on every save and triggers a browser LiveReload via the
`reload-port = 3001` websocket configured in
`crates/ersatzrs-web/Cargo.toml` `[package.metadata.leptos]`.

- Edits inside `crates/ersatzrs-web/src/` rebuild in seconds.
- Edits inside `ersatzrs-infra`, `ersatzrs-core`, `ersatzrs-ffmpeg` trigger a
  full SSR binary relink — that takes longer (~20–60s warm).
- CSS edits go through `stylance` via `build.rs`. Save the `.module.css`,
  watch picks it up, the bundle regenerates into `public/bundle.css`.

If the browser does not auto-refresh, check that nothing else is bound to port
`3001` and that the page was loaded via `cargo leptos`'s server (not a stale
`cargo run --bin ersatzrs-web` from a previous session).

## Inner-loop strategy (don't recompile the source 5×)

A single task often runs five *different* compilation graphs — and because Cargo
keys its cache on `(features × targets × profile × check-vs-build)`, each one
re-runs `rustc` over everything that differs. This is expensive on any disk,
and was especially brutal during the period this project's `target-dir` was
overridden to the `/mnt/f` (9p) scratch mount (see "Performance baseline"
below) — no such override is committed today, so `target-dir` resolves to the
default `./target` unless you add one yourself:

1. `cargo run --bin ersatzrs-web` — native, default features, binary.
2. `cargo clippy --workspace --all-targets` — adds test/example targets.
3. `cargo clippy … --features mysql` — different feature union → recompiles
   `sqlx` and every downstream crate.
4. `cargo test --workspace --all-features` — yet another feature union + test
   harness codegen.
5. `cargo leptos build` — the `wasm32` target, an entirely separate triple.

Keep the edit loop cheap by **not** flipping configurations while iterating:

- **Logic/crate work:** `./run.sh check -p <crate>` (skips codegen *and*
  linking — linking the big binary to 9p is a major cost). Use `./run.sh test
  -p <crate>` as the required focused/local runner; append a test filter
  when one regression is sufficient. The checked-in `.config/nextest.toml`
  keeps running after failures, retries nothing, and reports tests slower than
  60 seconds in the final summary.
- **UI work:** `./run.sh dev` (`cargo leptos watch`) — keeps the SSR binary and
  WASM bundle in lockstep; never pair a standalone `cargo leptos build` with a
  separate `cargo run` while iterating.
- Run the full multi-config gate (the two clippy passes, `./run.sh
  test-closeout`, `cargo leptos build`, `cargo xtask parity-check`) **once at
  closeout**, not per edit.

Nextest does not run doctests. `./run.sh test-closeout` therefore runs `cargo
nextest run --workspace --all-features` for unit/integration/binary tests and
then uses Cargo only for workspace doctests. Direct agent `cargo test` commands
are blocked by the checked-in Codex/Claude hooks; use the wrappers so focused
testing cannot drift back to Cargo. CI intentionally remains on its existing
`cargo test --workspace --all-features --lib --bins --tests` selection.

### Optional: sccache (recommended if your target-dir sits on a slow disk)

[`sccache`](https://github.com/mozilla/sccache) caches *dependency*
compilations on a fast disk, so the unavoidable config switches above read
cached objects instead of recompiling `sqlx`/`leptos`/`mlua`. This matters most
if your own `target-dir` override points at a slow mount (as `/mnt/f` (9p) did
during the period referenced above) — with the current default `./target` on
ordinary local disk, the win is smaller but still real for the config-switch
cost described above. Local workspace crates keep incremental compilation;
sccache only caches the cacheable (dependency) units.

It is **opt-in and machine-local** — deliberately *not* added to the repo's
committed `.cargo/config.toml`, so CI runners and other contributors are never
forced to install it.

Do **not** force it via a machine-global `[build] rustc-wrapper`: that applies
the wrapper to *every* cargo process by every tool/user/sandbox on the machine,
including agents (e.g. Codex) whose environment cannot reach the cache owner's
`0700` cache dir — their builds then fail with permission errors. Scope it to
where it is usable instead:

- `./run.sh` enables sccache for its subcommands automatically when `sccache` is
  on `PATH` (it runs as the cache owner).
- To also cache the manual closeout gate (the two clippy passes and
  `./run.sh test-closeout`),
  `export RUSTC_WRAPPER=sccache` in your own interactive shell (`~/.bashrc`).
- Keep only the cache location in your global `~/.cargo/config.toml` `[env]`
  (inert unless `RUSTC_WRAPPER=sccache` is set), pointed at fast ext4, not the
  9p scratch disk:

  ```toml
  [env]
  SCCACHE_DIR = "/path/on/fast/ext4/sccache"
  SCCACHE_CACHE_SIZE = "30G"
  ```

Check it is working with `sccache --show-stats` (compile requests climb; cache
hit rate rises once the same dependency/feature union is built a second time).

The wrappers apply the conventional local FFmpeg path overrides only to
`dev`, `build`, and `serve`. Test and check commands do not inject those
variables, so managed-runtime tests observe the same environment as direct
Cargo/CI unless the caller explicitly supplied an override.

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

## Docker image builds

The Dockerfile uses BuildKit cache mounts and builds the local runtime image in
three build steps:

1. Use the current stable Rust slim image, matching the CI/release toolchain,
   then install the `wasm32-unknown-unknown` target and the pinned web asset
   tools. Copy only workspace manifests, `Cargo.lock`, `rust-toolchain.toml`,
   and `.cargo/config.toml`, then create temporary placeholder crate sources
   inside the image and run the release build for `ersatzrs-web` and
   `ersatzrs-healthcheck`. This fills Docker-managed Cargo registry/git and
   target caches keyed by manifests and lockfiles, then discards the placeholder
   workspace crate artifacts and fingerprints.
2. Copy the real workspace source and run the same release build again. Source
   edits then reuse the Docker-managed target cache, rebuild only the affected
   workspace crate graph, and copy the two shipped binaries out of the cache
   mount into `/workspace/target/release` for the runtime image.
3. Build the Leptos frontend with the same pinned toolchain used by the release
   workflow (`stylance-cli 0.8.2`, `cargo-leptos 0.3.6`,
   `wasm32-unknown-unknown`, `cargo leptos build --release --frontend-only
   --lib-features hydrate`) and verify that `public/bundle.css` plus
   `target/site/pkg` exist before the runtime stage is assembled.

Do not commit host-local Cargo cache paths or sccache settings into the
Dockerfile. The image build keeps `SQLX_OFFLINE=true` through
`.cargo/config.toml`, preserves the release profile, and copies
`ersatzrs-web`, `ersatzrs-healthcheck`, `site/`, and `assets/` into the runtime
stage. `cargo-leptos` already copies the `assets-dir` (`public/`) contents into
the top level of the site root alongside `pkg/` (`[package.metadata.leptos]
assets-dir = "public"` in `crates/ersatzrs-web/Cargo.toml`), so `site/` is
already the single merged admin-UI directory — the image does not ship a
separate `public/` copy. The Leptos runtime environment is set to
`LEPTOS_SITE_ROOT=/usr/local/bin/site`, `LEPTOS_SITE_PKG_DIR=pkg`, and
`LEPTOS_OUTPUT_NAME=ersatzrs-web` so the SSR binary references the bundled
hydration scripts. Managed FFmpeg is not bundled in the image; startup uses the
same state-directory download/update path documented in
[Configuration Reference](docs/reference/configuration.md).

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

For a deeper, browser-level check, normal CI runs the full Playwright admin E2E
suite by default through `scripts/e2e/playwright-e2e.sh` (see
[TEST.md](TEST.md)). For a narrower island-only local smoke, use
`scripts/e2e/hydration-smoke.sh`. Release CI always keeps the structural
web-bundle gate above.

## Performance baseline

Record measurements here when you change profile flags or `run.sh`. Format:
`<date> — <command> — <cold s> / <warm s>`.

### Focused test-runner pilot (2026-07-18)

With the all-features test binaries already warm (`cargo test --workspace
--all-features --lib --bins --tests --no-run` took 0.77s), two passing runs of
each runner produced:

| Runner | Run 1 | Run 2 | Mean |
| --- | ---: | ---: | ---: |
| `cargo test --workspace --all-features --lib --bins --tests --no-fail-fast` | 16.20s | 18.10s | 17.15s |
| `cargo nextest run --workspace --all-features --lib --bins --tests --no-fail-fast` | 6.23s | 4.91s | 5.57s |

Nextest 0.9.140 ran 914 tests across 51 binaries with 11 skipped. It was about
3.1x faster for this warm execution slice, but saved only about 11.6 seconds in
absolute terms. That supports the focused/local workflow while leaving CI and
the doctest-capable closeout on `cargo test`.

### Docker dependency layer cache (2026-06-26)

The 2026-06-25 healthcheck closeout measured
`docker build --target runtime` spending `24m02s` in the single in-container
Rust release build after the full source tree was copied. The Dockerfile now
primes Docker-managed Cargo registry/git and target caches before copying
source.

2026-06-26 — `docker build --target runtime --progress=plain -t
ersatzrs-docker-cache-proof .` — `1458.13s` / `7.52s` after a source-only
`ersatzrs-healthcheck` edit. The source-only rebuild reused the manifest prime
step and rebuilt only `ersatzrs-healthcheck` (`2.55s` Cargo time).

### Linker investigation (negative result, 2026-05-17)

> These numbers were measured when `target-dir` was the default `./target` on
> ext4. `target-dir` was later moved to the `/mnt/f` (9p) scratch disk, where
> small-file I/O is ~20–100x slower, so cold/warm costs are now dominated by
> disk I/O and config-switch recompiles — see "Inner-loop strategy" above for
> the current mitigation (sccache + one-config loop). The linker conclusion
> still holds; the baseline figures below do not.

The reported "10+ minute" builds are the **cold-rebuild path forced by an
oversized `target/`** (observed at 102–136 GB), not a linker problem — on a
healthy ext4 `target/` the cold cost is ~2.5 min and warm rebuilds are seconds.
Keep `target/` pruned: run `cargo sweep --time 30` routinely (prunes only
artifacts older than 30 days, preserves the warm cache) and reserve the heavy
`cargo clean` for when `target/` exceeds 40 GiB or when reclaiming all space.
See the disk-space note in
[CLAUDE.md](CLAUDE.md).
