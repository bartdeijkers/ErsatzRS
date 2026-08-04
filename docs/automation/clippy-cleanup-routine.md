# Clippy exemption cleanup routine (cloud loop)

This is the prompt run by the scheduled remote agent ("cloud loop") once per
night against `bartdeijkers/ErsatzRS` `main`. It refreshes the
[Clippy Exemption Ledger](https://claude.ai/code/artifact/3fca4e2b-b674-4163-965c-d65bca3d4799)
artifact and, when there are safe targets, fixes a **batch** of
`#[allow(clippy::...)]` exemptions per night — up to a self-tuning cap,
each one individually verification-gated — behind a single push. Edit this
file to change the routine's behavior; the schedule points at this contract.

- **Delivery:** commits code changes **directly to `main`**. Each verified
  fix within a run gets its own commit (readable history, meaningful
  `commitlint --from HEAD~1` checks), but the whole run does **at most one
  `git push`**. An all-skipped run or a run that reaches the time-box before
  its first success uses one empty state commit so the tuner can observe that
  outcome — see "Budget guardrails" for why one push is the real constraint,
  not one fix.
- **Selection:** fully deterministic, encoded in
  [`xtask/src/clippy_ledger.rs`](../../xtask/src/clippy_ledger.rs), not
  re-derived from prose each night. `cargo xtask clippy-ledger --next
  [--skip <file>[,<file>...]]` prints exactly one target, excluding any
  files passed to `--skip`:
  - **Tier 1** — narrow the next file still carrying the copy-pasted
    module-level `#![allow(clippy::too_many_lines, dead_code)]` header down to
    precise, per-item allows.
  - **Tier 2** (once Tier 1 is exhausted) — fix the next single-instance,
    non-structural lint site from a fixed priority list.
  - **`tier=none`** once both are exhausted (after accounting for anything
    skipped this run) — the loop's stop signal.
  - Structural / judgment-call lints (`too_many_lines`, `too_many_arguments`,
    `struct_excessive_bools`, `disallowed_methods`, cast lints,
    `needless_pass_by_value`, `unused_async`, standalone `dead_code`) are
    **never** auto-selected — see the constants and comments at the top of
    `clippy_ledger.rs` for the current, authoritative list and the reasoning.
    Don't touch those categories from this routine; if the maintainer wants
    one addressed, that's a separate, explicitly-requested slice.
  - `clippy::misnamed_getters` is **never** auto-picked even though it's
    single-instance — it's a correctness lint, not a style one, and may
    indicate a real bug. It's surfaced in the ledger's `flagged` list for the
    nightly report only; a human decides what to do with it.
- **Batch cap:** how many fixes this run attempts before stopping (subject
  to the fixed time-box below and to `tier=none`), from `cargo xtask
  clippy-ledger --tune-batch-cap`. The cap **self-tunes** run to run by
  reading this routine's own past `Clippy-cleanup-run:` commit trailers out
  of `git log` — no separate tracking file, the same "derive it live" design
  as the ledger itself, just applied to git history instead of the working
  tree. See [`xtask/src/clippy_ledger.rs`](../../xtask/src/clippy_ledger.rs)
  for the exact clamp rules (`MIN_BATCH_CAP`/`MAX_BATCH_CAP`/`BATCH_CAP_STEP`/
  `TUNE_LOOKBACK_RUNS`). It only ever moves the *count* of fixes attempted —
  never the time-box, never which lints are eligible.
- **State:** there is **no separate progress-tracking file**. The ledger is
  regenerated live from a fresh workspace scan every run — a fixed site
  simply stops appearing next time, which is both the state store and the
  completion signal. The batch-cap tuner's only "memory" is the
  `Clippy-cleanup-run:` trailer written on the final fix commit, or on an
  empty `chore(ci)` state commit when every target was skipped or the time-box
  elapsed before the first success (see step 4). Do not invent a tracking
  file for either purpose.
- **Budget guardrails** (why this routine is scoped the way it is): a full
  nightly session must stay short — a bounded batch of small, mechanical,
  verification-gated fixes, never an open-ended cleanup pass — to avoid
  running long enough to threaten the 5-hour usage window, and to guarantee
  the session always ends with either a pushed commit or an explicit no-op
  report, never silently mid-work. The **time-box is fixed, not tunable**:
  stop the loop after `TIME_BOX_SECONDS` (2400s / 40 minutes, from
  `xtask/src/clippy_ledger.rs`) of active work regardless of the batch cap —
  that's the actual backstop protecting the usage window. `.github/workflows/
  ci.yml` runs the full CI gate on every push to `main` that touches anything
  outside `docs/**`; since these fixes touch `.rs` files, **exactly one push
  per night** keeps this to one CI run per night instead of a flood. GitHub's
  `push` trigger fires once per `git push` regardless of how many commits it
  carries, so batching several verified commits behind one push doesn't cost
  any extra CI runs — that's what makes batching free from a CI-flood
  perspective, and is why the batch cap can safely be more than one.
- **Schedule owner:** this routine is intended for the maintainer-owned
  scheduled remote agent against `bartdeijkers/ErsatzRS` `main`. The
  repository contract lives here; the external scheduler/account must point
  at this file and commit with the `clippy-cleanup-routine` agent name.

---

## Prompt

You are running the ErsatzRS nightly clippy-exemption cleanup. Work only in
this repo; follow `AGENTS.md`. You start cold — the full contract is this
file.

> **Hard guardrail — never end without a commit or an explicit no-op.** Every
> run either ends with exactly one `git push` (carrying one or more fix commits
> or one empty state commit from tonight), or with zero commits and a clear
> "no-op: `<reason>`" report when pre-flight blocks the run or the first
> selection is already `tier=none`. Never leave the working tree dirty, never
> leave a partially-applied fix, never push more than once.

### 0. Pre-flight: confirm `main`'s CI is green

1. `git fetch origin && git checkout main && git pull --ff-only`.
2. Check the most recent CI run on `main` (e.g. `gh run list --branch main
   --limit 1 --json status,conclusion,headSha`). If it did not succeed, do
   **not** add a new commit on top of a red `main` tonight — report which run
   failed and why (if determinable), and stop. No-op.
3. Set `start_epoch=$(date -u +%s)` and `TIME_BOX_SECONDS=2400` — you'll need
   both at the end of the loop to compute `duration_s` for the run trailer and
   before every potentially long verification command to enforce the
   time-box.

### 1. Regenerate and republish the ledger artifact

1. `cargo xtask clippy-ledger` — regenerates `plans/clippy-exemption-ledger.html`
   from the checked-in template (the file itself is gitignored; this always
   works from a fresh clone).
2. Publish it with the `Artifact` tool, `file_path:
   plans/clippy-exemption-ledger.html`, and `url:
   https://claude.ai/code/artifact/3fca4e2b-b674-4163-965c-d65bca3d4799` so it
   updates in place at the same link. Keep the favicon and title stable. If
   the `Artifact` tool is unavailable in this session, skip publishing, note
   it in the report, and continue — regenerating the local file still matters
   for step 2. (This first publish reflects last night's ending state; step 7
   republishes again after tonight's batch so the artifact doesn't lag by a
   full day.)

### 2. Get tonight's batch cap

Run `cargo xtask clippy-ledger --tune-batch-cap`. It prints `batch_cap=<N>`
and `reason=<why>`. Use `<N>` as the loop bound in step 3. Note the reason —
it goes in the report (step 7) so the maintainer can see why the cap moved
without digging into `git log` themselves.

### 3. The fix loop

Maintain a running `skip` list (file paths already attempted this run, so a
failed target isn't retried in an infinite loop) and a `succeeded` list (one
entry per committed fix, for the final report and the run trailer). Also keep
a `stop_reason`, including whether the time-box ended the loop. Repeat the
following until **any** of: `succeeded.len() == batch_cap`, elapsed time since
step 0.3 reaches `TIME_BOX_SECONDS` (2400s), or a `--next` call returns
`tier=none`. Check elapsed time before selecting or editing another target; if
the deadline is already reached, set `stop_reason=time-box` and break:

1. Run `cargo xtask clippy-ledger --next --skip <comma-separated skip list>`
   (omit `--skip` on the first iteration).
   - `tier=none` → break out of the loop (nothing left, even after
     accounting for this run's skips).
   - `tier=1` → a file path, its current blanket allow line, and its LOC.
   - `tier=2` → a lint name, file, and line.
2. **Apply exactly that one fix:**
   - **Tier 1:** open the file, remove the module-level
     `#![allow(clippy::too_many_lines, dead_code)]` line, then run the
     crate's clippy check (see step 3.3) to see exactly what it now flags:
     - For every `too_many_lines` hit, add a scoped
       `#[allow(clippy::too_many_lines)]` directly on that function only.
     - For any `dead_code` hit: if the flagged item is genuinely unused,
       delete it (this is the one case where the fix is a real behavior
       change — see the commit/changelog rule in step 3.5). If it's a false
       positive you can't safely resolve by deletion, add a narrowly-scoped
       `#[allow(dead_code)]` directly on that item with a one-line reason
       comment.
     - Touch nothing else in the file. Do not restructure functions to
       satisfy `too_many_lines` — narrowing the allow's scope is the fix,
       not making the function shorter.
   - **Tier 2:** apply the specific, non-structural fix for that lint at
     that site only (e.g. add `#[must_use]`, drop redundant raw-string
     hashes, rename a field, merge duplicate match arms, add the missing
     `# Errors`/`# Panics` doc section). Touch nothing else.
   - If, once you're in the code, the "safe" fix turns out to need judgment
     calls beyond a mechanical change (e.g. the Tier-1 file's
     `too_many_lines` sites are tangled with other pedantic debt, or the
     Tier-2 site's fix isn't actually trivial), treat it as a verification
     failure (next bullet) — don't improvise a larger change.
3. **Verify** (focused, not the full closeout gate), scoped to the touched
   crate only:
   - `cargo fmt --all -- --check`
   - `cargo clippy -p <touched crate> --all-targets -- -D warnings`, plus the
     `--features mysql` pass too if the touched crate is feature-gated by it
     (currently only `ersatzrs-infra`)
   - `cargo xtask hydration-lint -p ersatzrs-web` if the touched file is an
     admin page
   - A focused `./run.sh test -p <crate> <module filter>` if the touched
     file/module has existing test coverage

   The 2,400-second limit is a hard process deadline, not merely a check
   between commands. Immediately before **each** command above, recompute:

   ```bash
   remaining_s=$((TIME_BOX_SECONDS - ($(date -u +%s) - start_epoch)))
   ```

   If `remaining_s <= 0`, restore the active target (and any uncommitted
   `CHANGELOG.md` edit) to `HEAD`, add the target to `skip`, set
   `stop_reason=time-box`, and break out of the fix loop. Otherwise run the
   command with the remaining budget, for example:

   ```bash
   timeout --signal=KILL "${remaining_s}s" cargo clippy -p <touched crate> --all-targets -- -D warnings
   ```

   Apply that wrapper separately to formatting, every Clippy pass,
   hydration lint, and the focused test. Recompute `remaining_s` before each
   one; never reuse an earlier value. Using `KILL` ensures the command and its
   process group cannot continue beyond the remaining budget. Exit status 124
   or 137 is a time-box stop: restore the active target and any uncommitted
   changelog edit to `HEAD`, add the target to `skip`, record it as
   skipped-with-reason `verification timed out`, set
   `stop_reason=time-box`, and break out of the fix loop. Do not start another
   target after a timeout.

   Full CI (`ci.yml`) remains the authoritative gate after push — this local
   pass is a fast, cheap pre-check, not a substitute for it. Fixes that land
   consecutively in the same crate (all 14 current Tier-1 files are in
   `ersatzrs-web`, for example) reuse the incremental build, so batching
   several fixes is cheap in practice, not just in commit count.
4. **On a non-timeout verification failure:** restore the active target and
   any uncommitted changelog edit to `HEAD` (leave the working tree clean),
   add the file to the `skip` list, note it in the report as
   skipped-with-reason, and go to the next loop iteration — **do not** end
   the run. Because the ledger is regenerated live, a skipped target is
   simply retried on a future run; no progress is lost.
5. **On verification success:** commit it —
   - Scope from `commitlint.config.cjs`'s `scope-enum` (`web`, `infra`,
     `scanner`, `core`, `app`, `ffmpeg`, `domain`, ...).
   - `refactor(<scope>): narrow blanket clippy allow in <file>` for
     allow-narrowing/relocation with **no behavior change** — no
     `CHANGELOG.md` entry (internal lint hygiene; a nightly entry for every
     single-site fix would itself become noise the Changelog Contract
     doesn't ask for).
   - `fix(<scope>): ...` instead, **with** a one-line `CHANGELOG.md` entry
     under `Removed`, only when real dead code was actually deleted (a
     genuine, if invisible, behavior change).
   - `npx commitlint --from HEAD~1` — must pass; amend the message if not.
   - Add the file to `succeeded` and continue the loop. **Do not push yet.**

### 4. Persist the run outcome for tuning

Compute `duration_s` (now minus `start_epoch`) and construct this trailer:

```
Clippy-cleanup-run: fixes=<succeeded.len()> skipped=<skip.len()> duration_s=<N> cap=<batch_cap> timebox_s=2400
```

This is what step 2's `--tune-batch-cap` reads back on future runs — it's
the tuner's only state.

- If `succeeded` is non-empty, amend the **last** commit made in step 3 to
  append the trailer to its message body (keep the rest of the message as
  committed).
- If `succeeded` is empty and either `skip` is non-empty or
  `stop_reason=time-box`, persist the no-op outcome in an empty commit:

  ```bash
  git commit --allow-empty -m "chore(ci): record clippy cleanup no-op" -m "Clippy-cleanup-run: fixes=0 skipped=<skip.len()> duration_s=<N> cap=<batch_cap> timebox_s=2400"
  ```

  This state-only commit is required even though the working tree is clean:
  without it, the next run cannot lower the batch cap after an all-skipped or
  time-boxed run and will retry the same workload unchanged.
- If `succeeded` and `skip` are both empty and the first selection returned
  `tier=none`, make no commit; this is a true completed-ledger no-op with no
  tuning signal to preserve.

Run `npx commitlint --from HEAD~1` after the amend or empty commit; it must
pass.

### 5. Push

If step 4 amended a fix commit or created an empty state commit, `git push`
once, carrying every commit made this run. If the first `--next` call returned
`tier=none` without skips or a time-box stop: no-op, no push, working tree
clean.

### 6. Re-regenerate and republish the ledger artifact

Repeat step 1 (regenerate + publish) so the artifact reflects tonight's
fixes immediately rather than lagging until tomorrow's step 1. Skip this if
`succeeded` was empty — nothing changed since the first publish.

### 7. Report

State: the pre-flight CI check result, whether the artifact was published
(both times), tonight's batch cap and the tuner's reason, each target
attempted this run with its outcome (committed / skipped-with-reason), the
final fix or state commit and push (or the true no-op reason), whether the
time-box stopped a verification command, and the fresh ledger totals (sites
remaining, Tier-1 files remaining, Tier-2 candidates remaining) so the
maintainer can see the trend without opening the artifact.
