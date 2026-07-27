# CI: persistent target-dir for the self-hosted runner

## Context

`.github/workflows/ci.yml` defaults every job to a **single self-hosted Linux
runner** (see the concurrency-group comment at `ci.yml:37-42`), with an opt-in
`workflow_dispatch` fallback to ephemeral GitHub-hosted `ubuntu-24.04` VMs.
That distinction matters:

- On the **GitHub-hosted** path, jobs run in parallel on separate disposable
  machines — job-per-concern splitting (`fmt`/`clippy`/`test`/`hydration` as
  separate jobs, `test_hosted` sharded 3 ways) is the correct playbook there.
- On the **self-hosted** path, there is only one physical machine, so jobs
  queue and run **serially** — splitting work into more jobs buys zero
  wall-clock parallelism. The workflow still pays the *ephemeral-VM tax* on
  that path anyway: every self-hosted job's `target/` lives inside the git
  checkout, `actions/checkout@v7`'s default `git clean -ffdx` wipes it at the
  start of every job, and `Swatinem/rust-cache@v2` (used with no
  `key:`/`shared-key:` override) then restores/saves it through GitHub's
  *remote* cache blob service — a full network round-trip every job, even
  though the previous job's compiled output is sitting untouched on the same
  local disk two directories up.

The fix keeps the job structure (good for PR check granularity and fail-fast
ordering) and removes the actual cost driver: a remote-cache-shaped solution
applied to a problem a local, persistent disk already solves for free.

A second, unrelated bug found during this work: `test_hosted`'s 3 matrix
shards (`workspace-minus-web`, `web-lib`, `web-bins`) all resolved to the
**same** `Swatinem/rust-cache` key (the action's default key includes the job
id but not the matrix value), so concurrent shards on the hosted path raced to
restore/save one cache entry instead of three.

A third, unrelated finding: `AGENTS.md` and `BUILD.md` asserted as present
fact that the build target-dir is "the 9p scratch disk (`/mnt/f`)". A direct
read of the committed `.cargo/config.toml` shows an **empty** `[build]`
section — that override was removed 2026-07-02 and never corrected in docs.

sccache is explicitly out of scope for this pass — the target-dir fix
captures most of the win on its own; sccache would add install-guard/
permissions/second-cache-dir complexity for an uncertain marginal gain on top.
Revisit only if a future measurement shows real compile time still
concentrated in identical dependency graphs across jobs.

## Approach

1. **Fix the `test_hosted` cache-key collision** — give each matrix shard its
   own `Swatinem/rust-cache` `key:`, mirroring `release.yml`'s `binaries`
   matrix (`key: release-${{ matrix.rid }}`).
2. **Give self-hosted jobs a persistent, checkout-external target dir** — add
   a workflow-level `CARGO_TARGET_DIR: ${{ github.workspace }}/../ci-target`.
   `scripts/e2e/playwright-e2e.sh` already resolves `CARGO_TARGET_DIR` first
   (its `resolve_target_dir()` helper), so `hydration` needs no script
   changes. Land as two commits: first add the env var and a warn-only
   `cargo xtask target-size` step alongside the existing rust-cache steps
   (observe a few runs); then remove/conditionalize the now-redundant
   rust-cache steps on the self-hosted-only paths.
3. **Small independent wins** — idempotency guards on `cargo install
   stylance-cli`/`cargo-deny` (mirroring the guard `hydration` and
   `release.yml`'s `web-assets` job already use), npm caching for `hydration`
   and `commitlint`.
4. **Documentation** — document the new persistent target-dir and its
   recovery step in BUILD.md's "CI runner selection" section; correct the
   stale `/mnt/f` target-dir claims in `AGENTS.md`/`BUILD.md`; add a
   CHANGELOG entry.

## Explicitly not done here

- Collapsing `clippy`/`test`/`hydration` into one job — job-per-concern is
  free once the remote-cache round-trip is removed; merging would only lose
  per-check PR status granularity.
- Switching CI to nextest or wiring in `cargo xtask parity-check` — BUILD.md
  already documents CI's existing `cargo test` selection as a deliberate,
  settled non-drift decision.
- Splitting `hydration`'s `cargo leptos build` into separate WASM/SSR jobs the
  way `release.yml`'s `web-assets`/`binaries` split works — that split exists
  in `release.yml` to amortize one WASM build across a 4-RID matrix; `ci.yml`
  has no matrix to amortize across, and splitting would reintroduce the
  "wasm checksum mismatch" risk `release.yml` has to explicitly guard against.
- sccache — deferred; revisit only if a future measurement warrants it.

## Verification

- Trigger `gh workflow run ci.yml --ref <branch> -f runner=github-hosted` and
  confirm the three `test_hosted` shards report distinct cache keys.
- Push two commits in a row on a scratch branch and compare self-hosted
  `clippy`/`test`/`hydration` durations between the cold and warm run.
- Confirm `clippy`/`hydration` still restore cache correctly when manually
  dispatched with `runner=github-hosted`, and that `test`'s self-hosted run
  has no `Swatinem/rust-cache` step in the job log.
- Run a normal PR through the modified workflow end-to-end; confirm all
  self-hosted-path jobs still go green, then dispatch once with
  `runner=github-hosted` to confirm the hosted fallback path also goes green.
