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
  FFmpeg 9.x override/pre-placed managed runtime for offline work. Startup
  validates the managed `ErsatzRS-ffmpeg` runtime for `drawvg`, `zscale`, and
  required codecs and FFmpeg 9 output/NVENC options before serving. `run.sh`
  exports local `~/.local/ffmpeg-9.0/` overrides when that developer build
  exists.
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
| `./run.sh test-closeout` | One workspace nextest pass, then Cargo doctests. |
| `./run.sh build` | Build the scanner companion, then build and run the web host. |
| `./run.sh serve` | Build the scanner companion, then rebuild and serve once.   |
| `./run.sh seed`  | Apply `tests/fixtures/seed_admin.sql` to the SQLite db.     |
| `./run.sh log`   | `tail -F ersatzrs.log`.                                     |
| `cargo xtask affected-checks` | Recommend the narrow focused loop for the current diff. |
| `cargo xtask runtime-audit --db <path> --redact` | Audit runtime SQLite state without writing it. |

## Select checks from the diff

`affected-checks` turns repository-relative changed paths into recommendations;
it never executes them. With no paths it combines tracked and untracked working
tree changes. Pass paths to plan a smaller slice, or `--base` to include
committed branch changes:

```bash
cargo xtask affected-checks
cargo xtask affected-checks --base origin/main
cargo xtask affected-checks xtask/src/main.rs skills/rust-inner-loop.md
```

The output uses `./run.sh` on POSIX and `.\run.cmd` on Windows, keeps changed
packages in one Cargo graph, adds migration/hydration/architecture/progress or
playback gates only when their paths require them, and labels broad checks as a
one-time closeout. It is a deterministic starting point; public-contract or
behavior changes may still require consumer tests from the owning milestone.

## Audit a runtime SQLite database read-only

Use `runtime-audit` during incident diagnosis or Windows dogfood acceptance:

```bash
cargo xtask runtime-audit --db <ersatzrs-database-path> --redact
```

The command refuses to create or migrate a database and opens the supplied
SQLite file with read-only flags. It reports aggregate integrity, successful
migration version, Classic dated/undated anchor state, exact/overlapping playout
slots, failed build statuses, and episode/artwork join cardinality. `--redact`
hides the database path and replaces playout IDs with local ordinals; titles,
URLs, media paths, and channel names are never queried. Findings are diagnostic
and keep exit status zero; invalid arguments, unreadable/corrupt databases, or
missing ErsatzRS schema return failure.

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
build/test work and warn when it exceeds the 60 GiB `cargo clean` threshold.
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
share the local runner's Cargo cache. Hosted native builds omit test-profile
debug information and use LLVM's `lld` linker. All test execution jobs consume
the artifacts built by the selected pool's shared producer, rather than linking
separate hosted shards. Cargo tools are prepared in the build job; execution
jobs use its verified nextest/helper executables. The full test selection remains
intact. Hosted execution remains an explicit fallback rather than the default.

Two self-hosted runners share one physical machine. Each runner runs its jobs
serially in its own workspace, so sharing build output across that runner's
jobs loses no wall-clock parallelism; the two runners do run jobs at the same
time, which is why the Playwright suites are serialized. `ci.yml` sets a
workflow-level `CARGO_TARGET_DIR` pointing at `github.workspace`'s parent
directory, which
the runner creates once per repo and `actions/checkout`'s default `git clean
-ffdx` never touches (it only cleans inside the checkout itself). This lets
`target/` persist across every self-hosted job and run instead of being wiped
and restored through `Swatinem/rust-cache`'s remote cache service every time
— the `clippy` and artifact-build jobs on that path do not restore a remote
cache; execution jobs use the verified build artifacts instead; only the GitHub-hosted fallback still does, since its
VMs are ephemeral. If a self-hosted CI run ever looks inconsistent with its
diff (a false pass or fail that doesn't match the change), suspect a stale
persistent `target/` first: either dispatch the workflow with the
`reset_target_dir` input set to `true`, or manually remove the directory on
the runner. This is unrelated to the local interactive dev target-dir and its
stale-binary trap below — this mechanism only sets `CARGO_TARGET_DIR` inside
the GitHub Actions job environment and never touches `.cargo/config.toml`.

### Build once, execute tests in separate jobs

**Build CI artifacts** produces the application test/runtime inputs once for
all selected execution jobs on the selected Linux runner pool:

- A Rust artifact contains the complete workspace nextest archive, the pinned
  nextest executable, the ordinary smoke server and the CI helper executables.
- A web artifact contains one matched Leptos SSR server, WASM/JS, the complete
  site assets and the CI helper executables. All six Playwright jobs use it.

The producer retains the native/test and Leptos build graphs as distinct build
products. Hosted native linker flags never reach the WASM build. General Rust,
Next and teletext jobs execute nextest archives without compiling again. The
former hosted cold-link test shards are unnecessary because linking belongs to
the shared build job. Playwright's prebuilt mode verifies the supplied bundle
and skips Cargo, Leptos, CSS generation and source-asset overlays. Ordinary local
runs retain the existing build path.

Each product has a versioned manifest with the exact source SHA, platform/libc,
toolchain/build flags and per-file hashes, sizes and executable status. Consumers
verify the checkout, expected product, complete inventory and contents before
execution. Wrong, missing, corrupt or incompatible artifacts fail; they do not
trigger a replacement build. The tar envelope preserves executable permissions.
Artifact names include the producer run/attempt; consumers use its job outputs,
so a failed-job rerun can retain the original successful build while a newly
executed producer publishes a distinct artifact.

Self-hosted consumers first copy the exact producer archive through a shared local
cache. Its key includes the repository, source SHA, producer run and attempt;
its expected archive SHA256 comes from the producer's GitHub job outputs. A hit
is exposed only after an atomic copy matches that hash, then the normal
source/platform/file validation still runs. A missing or inaccessible local
cache uses the same GitHub artifact download; corruption is an explicit error.
GitHub-hosted jobs continue to use GitHub artifacts directly. Successful
failed-job reruns retain the original producer key and digest.

The default cache is the runner user's `.cache/ersatzrs/ci-artifacts` directory.
Same-host runners must use the same home filesystem, or the repository variable
`CI_LOCAL_ARTIFACT_CACHE` must name a shared dedicated directory accessible to
both. Different hosts without that storage safely fall back to download. Only
four producer groups are retained; pruning recognizes the helper's exact
ownership marker and preserves unrelated directories. Artifacts are still
uploaded to GitHub for fallback and diagnostics.

After publishing both immutable products, the self-hosted producer cleans its
inactive configured Cargo target when it reaches the repository's 60-GiB
threshold (or `ERSATZRS_TARGET_CLEAN_THRESHOLD_GIB`). This leaves room for
restoring and extracting tests; consumers use the preserved copies. The first
build-once trial grew this cache to 107 GiB on a 115-GiB runner volume and
required that cleanup before test extraction.

Every execution job retains its own database, runtime, port and diagnostics.
Runtime staging and all test selection, ignored/helper semantics and resource
limits remain in the execution jobs. Source fixtures use
`tests/support/manifest_dir.rs` to resolve the Cargo/nextest runtime package
directory before their original compiled fallback; relocated
web hosts use the execution checkout's manifest and the explicit site directory.
Format, Clippy, dependency and source-contract checks remain independent gates.
This Linux build-sharing arrangement is not native acceptance for other RIDs.

Focused dispatch builds only the required products: Rust for `test_only` or
`next_engine_only`, web for `browser_only`, and both for `teletext_only` or full
CI. The same-SHA failed-job rerun reuses its successful producer artifact; a new
revision needs a new verified build. Artifacts expire after seven days, after
which the producer must run again. No cross-revision artifact reuse is allowed.

The smoke runner accepts an explicitly selected server without rebuilding:

```bash
cargo xtask smoke --server-bin <verified-server> --next --keep-temp
```

A missing or invalid explicit server fails without falling back to Cargo.

### Next engine CI check

The existing CI workflow reports **Next engine** as a separate job alongside
Test and the Chromium/Firefox Playwright jobs. It uses the selected runner pool
and the same tested ErsatzRS commit. General Test jobs exclude `next_engine::`
and the teletext selectors. The Next job runs its non-teletext component and
opt-in real-engine tests; named teletext cases move to the teletext jobs below
with their original runtime requirements. The ignored subprocess fixture remains
helper-only.
Local `test-closeout` still runs the complete ordinary workspace selection.

The job stages the published Next channel binary with the release staging
script, retains its origin/digest metadata, and provisions FFmpeg through the
production runtime manager (`provision_runtime` example, pinned release tag).
It then runs Next HTTP and preview smoke, including MPEG-TS refusal and idle
cleanup. Each smoke invocation selects a free loopback port immediately before
starting, and a job-specific temporary directory isolates its files from
Playwright and the live server. Test output, JUnit timings, host logs, runtime
identity and the ErsatzRS SHA are uploaded as `next-engine-<run>-<attempt>`.
A Linux job is not acceptance for the other supported platforms.

### Teletext CI checks

CI reports **Teletext 1.5** and **Teletext 2.5** as separate Rust jobs.
Playwright has six serial jobs: **general**, **1.5**, and **2.5**, each in
Chromium and Firefox, with independent ports and diagnostic artifacts.
`scripts/ci-teletext-selection.py` owns the disjoint selectors. General Test
and Playwright jobs exclude the tests moved to these jobs. Rust integration
binary names are matched as well as test names, so unnamed tests in teletext
integration targets are retained on both runner pools.

Shared infrastructure and unclassified teletext tests default to the 1.5 job.
The 2.5 job owns the named enhanced object/resource/DRCS/palette/panel/context
families, including their Level 1/1.5 fallback assertions. This is CI ownership,
not a claim that either level has passed product acceptance. A change to shared
validation, transport, preview, or fallback can require both jobs. Existing
opt-in editor, decoder, client and native-platform acceptance remains opt-in;
this split does not turn unavailable prerequisites into CI successes.

Both Rust jobs preserve ordinary ignored-test behavior. Teletext tests moved
from the Next job retain that job's runtime staging and include-ignored
selection, while helper subprocess fixtures remain excluded. Each job retains
its source SHA, runtime identity, logs and separate workspace/Next JUnit reports.

To check a corrected revision without rerunning unrelated jobs:

```bash
gh workflow run ci.yml --ref <branch> -f runner=self-hosted -f teletext_only=1.5
gh workflow run ci.yml --ref <branch> -f runner=self-hosted -f teletext_only=2.5
# Run both levels and all four teletext browser jobs:
gh workflow run ci.yml --ref <branch> -f runner=self-hosted -f teletext_only=all
```

Each level selects its Rust job and both browser projects, after the shared
artifact producer. A job rerun tests
its original SHA; dispatch tests the new branch revision. The title
`CI (Teletext only)` and separate concurrency group prevent a focused run
from cancelling or standing in for full CI. Focused modes are mutually
exclusive. On the shared host, wait for an existing run using the same suite
ports to finish before dispatching recovery. Record the new SHA, the targeted
results and the earlier unaffected checks; do not claim fresh full CI from a
targeted dispatch. For the same SHA,
GitHub's **Re-run failed jobs** also preserves already successful jobs.

## Release-candidate CI gate

Before creating any alpha, beta, or normal release tag, push the exact
candidate commit and verify completed, successful CI evidence for that SHA.
Local closeout checks and focused Playwright regressions are useful before the
push, but they do not replace this pre-tag gate.

```bash
candidate=$(git rev-parse HEAD)
git push origin HEAD
# Every CI run for the exact SHA, any event. No --limit 1: the newest run is
# not always the one that executed the gate (see below).
gh run list --workflow ci.yml --commit "$candidate" \
  --json databaseId,event,headSha,status,conclusion,url
gh run watch <run-id> --exit-status
gh run view <run-id> --json headSha,status,conclusion,url
# Per-job results: a Preflight-skipped run shows its gate jobs as skipped.
gh run view <run-id> --json jobs --jq '.jobs[] | "\(.name): \(.conclusion)"'
```

Create the release tag only when the evidence run reports the same full
`headSha`, `status: completed`, and `conclusion: success`, **and** its job
list shows `success` for `Format`, `Clippy`, `Build CI artifacts`, the `Test` job(s) for its runner
configuration, both `Teletext` levels, all six `Playwright E2E` suite/project
combinations, `Next engine`, `Cargo deny`, and
`Documentation links`. A green workflow conclusion alone is not enough,
because skipped jobs do not fail a run. A failed, cancelled, queued, or
in-progress run is not release evidence. Any commit or amend after the
successful run creates a new candidate SHA and requires new CI evidence before
tagging.

### Preflight-skipped runs are a pointer, not evidence

The `Preflight` job skips the gate when a pushed SHA already has a successful
CI run, so a direct fast-forward merge to `main` does not retest an identical
tree. Such a run still reports `conclusion: success` and is usually the newest
run for that SHA, so a recipe that takes the latest successful run can record
skipped jobs as newly passed tests.

When the automatic push run was skipped this way, record both runs: the push
run as provenance and the original full-gate run as the evidence. The push
run's `Preflight` job logs a notice listing every earlier successful run for
this exact SHA, which can include other skipped runs. Follow those links to
one that executed every required job above (often a `workflow_dispatch` run on
the feature branch) and verify it there. The exact-candidate full-gate
requirement is unchanged; only the run you cite moves.

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

`run.ps1` looks for FFmpeg 9 at `%USERPROFILE%\.local\ffmpeg-9.0\bin` and
sets `ErsatzRS_FFMPEG__PATH` / `ErsatzRS_FFPROBE__PATH` automatically when
that directory exists. If your FFmpeg lives elsewhere, set those two
environment variables yourself. Without an explicit override, startup uses the
managed `ErsatzRS-ffmpeg` download/update path and falls back to `PATH` only if
managed acquisition is disabled or fails.

`run.ps1 seed` shells out to the `sqlite3` CLI; `winget install
SQLite.SQLite` (or any other source of `sqlite3.exe` on `PATH`) covers the
prerequisite.

`cargo xtask smoke` always overrides the web listener to
`127.0.0.1:8409`. The ephemeral smoke executable therefore remains local and
does not require an interactive Windows Firewall approval.

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

### Rerun only Rust tests after a fixture repair

A GitHub job rerun uses the original revision. To check a new revision while
reusing unaffected checks, dispatch the existing workflow with `test_only`:

```bash
gh workflow run ci.yml --ref <branch> -f runner=self-hosted -f test_only=true
```

This runs the general `Test` job, including progress and architecture validation,
but excludes the separately owned teletext and Next tests. Unrelated jobs are
skipped. Both runner pools execute the same archived workspace selection.
The run is titled `CI (Test only)` and cannot suppress a later full CI run through
the duplicate-run check. It is scoped test evidence, not full CI acceptance.
When deliberately preserving earlier checks, `[skip ci]` on the repair commit
prevents the automatic full PR run; the manual dispatch still checks that commit.

### Rerun only Next engine checks after a fixture repair

For a correction confined to the Next engine gate, dispatch its existing job:

```bash
gh workflow run ci.yml --ref <branch> -f runner=self-hosted -f next_engine_only=true
```

The run is titled `CI (Next engine only)`. It runs the unchanged Next runtime
staging, regression tests and playback checks, while skipping unrelated jobs.
Its concurrency group is separate from normal CI, so it cannot cancel a full
run still executing on the same branch. Do not combine `next_engine_only` with
`test_only`: that dispatch fails before checkout or compilation.

A job rerun checks its original SHA; this dispatch checks the selected branch's
new SHA. Record both revisions when retaining unaffected earlier evidence, and
repeat any checks invalidated by the correction. This focused run cannot suppress
a later full run through Preflight and does not establish full green CI or
satisfy the exact-candidate release gate above.

### Retry the failed browser fixtures on a corrected revision

Use the bounded recovery mode to run all 11 cases per selected browser in the
Block preview, teletext authoring and editor specs. Run complete serial groups
so failures are rechecked with their predecessors and any followers that did not
run after the original failure, including both TCP publication cases.

```bash
gh workflow run ci.yml --ref <branch> -f runner=self-hosted -f browser_only=both
# Use browser_only=chromium or browser_only=firefox to recover just one project.
```

This runs the selected browser jobs with the existing matched Leptos build,
pinned runtime staging and isolated application runner. `both` runs 22 cases
through the existing matrix with `max-parallel: 1`, preserving serial execution
and avoiding resource contention between browser projects. Normal CI runs all
six general/1.5/2.5 browser jobs. The `CI (Browser recovery)` title identifies
partial evidence and cannot suppress full CI through Preflight. Its
browser-specific
concurrency group preserves other running gates. On a shared host, wait for an
existing job of the same project to release its port before starting recovery.
Combining this mode with `test_only`,
`next_engine_only` or `teletext_only` fails before checkout or building.

Record the repaired SHA and the earlier full run when reusing unaffected
results. This three-spec recovery is scoped evidence; it does not establish full
green CI or satisfy the exact-candidate release gate.

## Inner-loop strategy (don't recompile the source 5×)

A single task often runs five *different* compilation graphs — and because Cargo
keys its cache on `(features × targets × profile × check-vs-build)`, each one
re-runs `rustc` over everything that differs. This is expensive on any disk,
and was especially brutal during the period this project's `target-dir` was
overridden to the `/mnt/f` (9p) scratch mount (see "Performance baseline"
below) — no such override is committed today, so `target-dir` resolves to the
default `./target` unless you add one yourself:

1. `cargo run --bin ersatzrs-web` — native binary.
2. `cargo clippy --workspace --all-targets` — adds test/example targets.
3. `cargo leptos build` — the `wasm32` target, an entirely separate triple.

Keep the edit loop cheap by **not** flipping configurations while iterating:

- **Logic/crate work:** `./run.sh check -p <crate>` (skips codegen *and*
  linking — linking the big binary to 9p is a major cost). Use `./run.sh test
  -p <crate>` as the required focused/local runner; append a test filter
  when one regression is sufficient. The checked-in `.config/nextest.toml`
  keeps running after failures, retries nothing, and reports tests slower than
  60 seconds in the final summary. Large teletext carousel/output fixture suites
  share a single nextest test-group slot in local and CI profiles: each complete
  500-display service retains several GiB of semantic preview state. Other test
  suites keep their normal concurrency. The self-hosted Test job additionally
  limits compilation to one job and test execution to four slots. The dedicated
  Next engine job limits execution to four test slots and compilation to two jobs
  on the same self-hosted machine (eight on a hosted runner). Avoid overlapping
  local heavy builds with CI builds:
  multiple web-crate compilers can exhaust the shared machine's memory and swap.
- **UI work:** `./run.sh dev` (`cargo leptos watch`) — keeps the SSR binary and
  WASM bundle in lockstep; never pair a standalone `cargo leptos build` with a
  separate `cargo run` while iterating.
- Run the full gate (`cargo clippy`, `./run.sh test-closeout`, `cargo leptos
  build`, `cargo xtask parity-check`) **once at
  closeout**, not per edit.

The agent edit hook formats a single Rust file when it can identify one. Run
`cargo xtask affected-checks` after a coherent edit batch, then run strict
Clippy on the affected crate with `cargo clippy -p <crate> --all-targets -- -D warnings`
before closeout. Do not run Clippy from a per-edit hook: it compiles a crate and
can repeatedly check unfinished intermediate edits. After staging Rust
changes and before committing, run `python3 scripts/check_new_clippy_allows.py --base HEAD` to check
new Clippy exemptions in the working tree. The CI Format job applies the same
policy to the full push or pull request diff. New `#[allow(clippy::...)]` and
`#[expect(clippy::...)]` attributes must be item-scoped and carry an inline
`//` explanation or a `reason = "..."` argument. Broad lint groups and
`#[allow(warnings)]` are rejected. Existing exemptions remain outside this diff
check; `cargo xtask clippy-ledger` tracks the existing inventory.

Nextest does not run doctests. `./run.sh test-closeout` therefore runs nextest
for unit/integration/binary tests and then uses Cargo only for workspace
doctests. Direct agent `cargo test` commands are blocked by the checked-in
Codex/Claude hooks; use the wrappers so focused testing cannot drift back to
Cargo. CI runs the same nextest selection through the `ci` profile
(`cargo nextest run --profile ci --workspace --lib --bins --tests`, ADR 0090),
which additionally writes a JUnit report so per-test durations are available as
a build artifact. Neither the local nor the CI selection runs doctests; only
`./run.sh test-closeout` adds them.

The closeout deliberately uses the sole supported SQLite graph. Historical
dual-backend feature combinations are not part of current development or
release acceptance.

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
- To also cache the manual closeout gate (clippy and `./run.sh test-closeout`),
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

[profile.test]
debug = 0                      # keeps the Windows web-test PDB below its limit

[profile.dev.package."*"]
opt-level = 1                  # all deps -O1: dev runtime ~2-5x faster

[profile.dev.package.mlua]
opt-level = 0                  # vendored Lua C build is slow; -O1 buys little

[profile.dev.package.sqlx-macros]
opt-level = 0

[profile.dev.package.ersatzrs-web]
debug = 0                      # keeps the native web rlib below MSVC's 4-GiB limit
```

Trade-offs:

- `opt-level = 1` for all transitive deps adds **~30s to a clean build** but
  makes mlua/sqlx/leptos runtime much snappier in dev. Worth it for the
  watch loop.
- `split-debuginfo = "unpacked"` shrinks the resulting `target/` and speeds
  up linking on Linux. On other OSes, drop or change this line.
- Test binaries omit debug information because it is not needed to determine
  outcomes and the large `ersatzrs-web` harness otherwise exceeds MSVC's PDB
  size limit on Windows. Panic messages and nextest diagnostics remain intact.
- The `ersatzrs-web` workspace package also omits debug information in dev
  builds. Its native RLIB exceeds MSVC's 4-GiB archive limit even with line
  tables only; dependencies retain their normal dev line tables.
- `target/` will be larger than before — the per-package optimisation level
  generates more codegen artifacts. Run `cargo clean` when `target/` grows
  beyond 60 GiB; between cleanups, use
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
fails the release instead of shipping. The gate binds only to loopback, defaults
to port 18479, and refuses an occupied port before starting a candidate. Set
`ERSATZRS_VERIFY_PORT` to another free port when needed. Windows cleanup resolves
the Git Bash process to its native PID and stops that candidate's process tree.
Run the offline port regressions with
`python3 scripts/release/test-verify-web-bundle.py`; on native Windows, use
Python with `BUNDLE_TEST_BASH` pointing to Git Bash's `bash.exe`. Under
Git Bash/MSYS it keeps POSIX paths for shell cleanup but converts every path
passed to the native Windows server into drive-letter form, so its temporary
SQLite database and assembled site are usable without a Firewall prompt. Run it
locally against any bundle:

```bash
ERSATZRS_WEB_BIN=target/release/ersatzrs-web \
ERSATZRS_SITE_ROOT=target/site \
  scripts/release/verify-web-bundle.sh
```

For a deeper, browser-level check, normal CI runs the full Playwright admin E2E
suite by default through `scripts/e2e/playwright-e2e.sh`, as one job per
Playwright project (see [TEST.md](TEST.md)). The wrapper first regenerates `public/bundle.css` with the
pinned Stylance CLI, because a clean checkout paired with a persistent Cargo
target can reuse the web build-script result without recreating that generated
file. After `cargo leptos build`, it synchronizes the complete authoritative
`crates/ersatzrs-web/public/` tree into the discovered site root and verifies
the CSS, preview bridge, and vendored Hls.js assets. This also keeps a
persistent Cargo target from reusing `site/pkg` without the configured
`assets-dir`. For a narrower island-only local smoke, use
`scripts/e2e/hydration-smoke.sh`. Release CI always keeps the structural web
bundle gate above.

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
`cargo clean` for when `target/` exceeds 60 GiB or when reclaiming all space.
See the disk-space note in
[CLAUDE.md](CLAUDE.md).

## Optional Next engine artifacts

Release archive staging requires Python 3.12 and access to the exact public
Next distribution selected by the canonical shared manifest,
`scripts/release/next-patches/manifest.json`. The local UCU-11 integration now
pins [Next v0.2.2](https://github.com/bartdeijkers/ErsatzRS-next/releases/tag/next-4a0b59a-ersatzrs.1)
in `bartdeijkers/ErsatzRS-next`; integrated Linux/Windows adoption and required
local acceptance pass. The six verified source builds retain both temporary corrections
under ADR 0117. Each archive records source, lockfile, patch and payload hashes.
The dependency assets are publicly downloadable without credentials. Package
staging verifies the pinned checksums and provenance; it never rebuilds Next.
`scripts/release/stage-next.py --rid <rid> --output <stage>/next` stages the
`ersatztv-channel` executable, its MIT license and `origin.json`. Archive
packaging then moves the executable (with `.exe` on Windows) to the package
root beside ErsatzRS, retaining only license/provenance in `next/`.
Windows packages also require `libvpl.dll` and `libvpl-LICENSE.txt` from the
same verified Next archive beside `ersatztv-channel.exe` at the package root.
Missing or invalid companions fail staging before the destination is published.
It checks archive and license SHA-256 values pinned in source and records
the selected asset, full pinned source revision and license hash. Staging rejects
an artifact whose revision differs from the canonical manifest's
`upstream_revision`, even when upstream replaces the mutable develop release.
Review upstream changes, validate the selected runtime and update the pin.
Application releases still require their own acceptance and green exact-SHA CI;
dependency publication needs its explicit maintainer authorization. Missing artifacts, invalid checksums or missing
licenses fail packaging. A new accepted snapshot requires new pinned hashes.
All six supported RIDs use the same reviewed source and two patches. Source and
lockfile hashes are pinned alongside the archive hashes; altered provenance
fails staging. Reproduction instructions are below. The former unmodified
ARMv7 source helper remains available for future patch retirement, but refuses
the active patched pin. Ordinary application packaging downloads existing
artifacts and never rebuilds Next. Docker runs from the host without mounting
its socket in a container.
Run `python3 scripts/release/test-stage-next.py` for offline
archive, provenance and failure-path regressions.

Docker builds use a separate build-host Python stage and copy the channel
executable into `/usr/local/bin`, retaining license/provenance under
`/usr/local/bin/next`; Python is not added to the runtime image. Bake
passes `VERSION` as `NEXT_RELEASE_REFRESH` for explicit cache refreshes;
archive hashes remain fixed. Before a Docker build, stage the snapshot on the
host using the exact current tag (not a mutable upstream release):

```sh
gh release download next-4a0b59a-ersatzrs.1 \
  --repo bartdeijkers/ErsatzRS-next --dir .next-snapshot
```

The gitignored directory is copied only into the staging layer; runtime
images receive the selected executable, license and provenance. For repeated manual builds of the same
version, use `--no-cache-filter next-official` with buildx, or change
`--build-arg NEXT_RELEASE_REFRESH=<build-id>`.

Engine selection and process supervision follow
[ADR 0095](docs/adr/0095-native-pipeline-instead-of-ersatztv-next-engine.md).
Current playback acceptance and approved platform debt are recorded in the
[structured progress records](docs/progress/README.md); a successful artifact
staging test does not prove playback acceptance.

### Temporary Next source corrections (ADRs 0116 and 0117)

The reviewed dependency retains the live RTSP buffering and scheduled-image
lifetime corrections. Each patch has independent upstream retirement criteria;
application milestone and release gates remain separate. Prepare its exact
reviewed source with:

```sh
python3 scripts/release/prepare-next-source.py \
  --source-archive tmp/next-source.tar.gz --output tmp/next-reviewed-source
python3 scripts/release/test-prepare-next-source.py
python3 scripts/release/test-package-next-source.py
```

The source archive must match `scripts/release/next-patches/manifest.json`.
Preparation checks the source archive, Cargo.lock and each patch SHA-256,
applies patches without fuzzy context fallback, and writes
`ersatzrs-source.json`. Build all platform packages from this same source with
the recorded distribution version; preserve source/patch provenance, licenses
and Windows companions. A prepared source tree alone does not prove package
or playback acceptance.

For UCU-11's v0.2.2 source, the versioned review manifest is
`scripts/release/next-patches/v0.2.2/manifest.json`. The existing preparation,
packaging and macOS source workflow apply. Both explicitly authorized macOS
jobs pass in source-candidate run 38542065540. The maintainer separately approved
publication of the exact six-RID `next-4a0b59a-ersatzrs.1` dependency release;
all 20 assets pass anonymous download/checksum checks. The canonical shared
manifest selects these published packages. Integrated Linux/Windows acquisition,
managed restart, bundled startup, decoded playback and required local UCU-11
gates pass. Remote push and associated CI follow acceptance, with failed specific
CI jobs retried individually. Application release remains separate;
approved hardware/ARM/macOS application debt remains explicitly unverified.

For UCU-10's v0.2.1 successor, the reviewed source manifest is
`scripts/release/next-patches/v0.2.1/manifest.json`. Pass it with `--manifest`
to both source preparation and packaging. Its exact Windows companion hashes
apply only to that reviewed source. UCU-10 adopted all six verified packages;
that delivered main integration remains distinct from local UCU-11 development.
The separately authorized `next-7f75012-ersatzrs.1` dependency release remains
public; all20 assets pass anonymous download/checksum acceptance and real
Linux/Windows cold acquisition. Use
`stage-next.py --snapshot-dir` with the verified package directory for offline
staging. Future dependency publication still requires maintainer instruction.
The existing macOS candidate workflow can select this manifest through its
`review_manifest` input. Dispatch, push and publication still require the
maintainer's instruction; choosing a source manifest does not run CI.

Package the built executable and license directory with
`scripts/release/package-next-source.py`; supply the generated source provenance,
RID and build's exact `rustc --version`. It verifies licenses, patch provenance
and Windows companions, records each payload hash, and refuses to overwrite an
existing archive. The candidate macOS workflow builds both supported targets
and uploads artifacts without publishing a release. macOS binaries receive an
ad-hoc signature for loader integrity; rebuilt executables do not inherit the
upstream publisher's signing/notarization identity.

Reproduce a Linux payload with host Docker and a directory containing the
checksum-pinned `source.tar.gz`:

```sh
docker buildx build --file docker/next-source.Dockerfile \
  --build-context next-source=tmp/next-source-input \
  --build-arg NEXT_TARGET=armv7-unknown-linux-gnueabihf \
  --output type=local,dest=tmp/next-source-output .
python3 scripts/release/package-next-source.py \
  --payload tmp/next-source-output \
  --source-provenance tmp/next-source-output/ersatzrs-source.json \
  --rid linux-arm --output tmp/next-packages \
  --rustc "$(cat tmp/next-source-output/rustc-version.txt)"
```

The same recipe accepts `x86_64-unknown-linux-gnu` and
`aarch64-unknown-linux-gnu` with their corresponding package RIDs. It builds
only the Next channel executable, keeps the upstream lockfile, uses the
reviewed Rust/Bookworm base and checks the GLIBC 2.36 ceiling. Windows uses
the prepared source with Rust 1.97.1 and MSVC:
`cargo +1.97.1 build --locked --release --all-features --target x86_64-pc-windows-msvc --bin ersatztv-channel`.
Use the explicit compiler selection for source builds and provenance
(`rustc +1.97.1 --version`): an upstream `rust-toolchain.toml` may select a
different compiler whose cross-target standard libraries were not installed.
Set `ETV_VERSION` to the prepared provenance's `distribution_version`, and
package the resulting executable and source license with the exact verified
`libvpl.dll` and `libvpl-LICENSE.txt` companion hashes enforced by the packager.
The previous unmodified ARMv7 helper refuses a patched pin to prevent
accidentally shipping an unpatched executable under the new distribution.

For an unpublished reviewed candidate, add
`--build-arg NEXT_REVIEW_MANIFEST=v0.2.2/manifest-sourceout-candidate.json`
to the Docker invocation and pass the same manifest under
`scripts/release/next-patches/` to the packager's `--manifest` option. The default
still selects the accepted distribution. Candidate packaging does not change
managed runtime selection or authorize publication.
The existing macOS candidate workflow also exposes this reviewed manifest as
a manual input. An authorized run using it builds both macOS candidate
artifacts; workflow dispatch, remote source transfer and publication still need
explicit maintainer authorization. Adding the choice does not run a job.

For every Next revision update, first prepare the comparison with
`--unpatched` and run the existing
`real_next_direct_http_and_rtsp_preserve_audio_and_source_clock` and
`real_next_rtsp_handoff_preserves_tracks_and_stops_padded_audio_at_video_eof`
application regressions. Review the upstream diff and run compatibility
acceptance. When an equivalent upstream correction passes, remove the patch
from the build and manifest, return to verified official artifacts where
available, and resolve ADR 0116's retirement debt. Retain the regression tests.
If upstream still fails, record the new unpatched evidence and explicitly
review the patch for that revision; changing the source pin alone is rejected.

For package cleanup, retain the accepted snapshot, current candidate and any
artifacts required by release recovery or the chosen rollback. Check references
and active processes first; verify duplicate payload hashes before deleting
their local copies. Remove a published dependency snapshot only after no
retained consumer requires it. Test logs and provenance remain useful even
after obsolete binaries are removed.

## Intel container userspace verification

The Docker `intel-media` stage builds the matched, checksum-pinned sources in
`docker/intel-media-sources.json` on Debian Bookworm. It is independent of the
Rust/WASM graph and uses two build jobs by default. Only `linux/amd64` receives
Intel libraries; the ARM stages produce no Intel binaries. Shared libraries
are stripped before copying them into the runtime. Build tools remain in the
intermediate image. Native installs still use their host GPU drivers.

After changing this stack, verify its loading in the production runtime base:

```bash
docker build --platform linux/amd64 --target intel-media -t ersatzrs-intel-build:test .
docker build --platform linux/amd64 --target runtime-base -t ersatzrs-runtime:test .
docker build --platform linux/amd64 -f docker/verify-intel-media.Dockerfile .
```

Repeat `runtime-base` builds for the release Dockerfile and the supported ARM
platforms. For the alternate `docker/Dockerfile`, use the probe's
`--target loader-check`: its existing Mesa Vulkan packages depend on Python,
whereas the default probe additionally requires the minimal runtime to omit it.
The loader probe checks the shipped driver, VAAPI, Gmmlib, dispatcher
and QSV runtime without adding build dependencies to the runtime image. This
proves packaging and symbol resolution; hardware acceptance additionally needs
an exposed render device, successful application capability/health checks and
a decoded hardware transcode. Source device IDs alone do not prove playback
on a particular GPU. The full application image retains the existing matched
SSR/WASM and managed FFmpeg verification requirements.
