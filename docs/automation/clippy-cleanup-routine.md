# Clippy exemption cleanup routine (cloud loop)

This is the prompt run by the scheduled remote agent ("cloud loop") once per
night against `bartdeijkers/ErsatzRS` `main`. It refreshes the
[Clippy Exemption Ledger](https://claude.ai/code/artifact/3fca4e2b-b674-4163-965c-d65bca3d4799)
artifact and, when there's a safe target, fixes exactly one
`#[allow(clippy::...)]` exemption per night. Edit this file to change the
routine's behavior; the schedule points at this contract.

- **Delivery:** commits code changes **directly to `main`**, at most **one
  commit per run**.
- **Selection:** fully deterministic, encoded in
  [`xtask/src/clippy_ledger.rs`](../../xtask/src/clippy_ledger.rs), not
  re-derived from prose each night. `cargo xtask clippy-ledger --next` prints
  exactly one target:
  - **Tier 1** — narrow the next file still carrying the copy-pasted
    module-level `#![allow(clippy::too_many_lines, dead_code)]` header down to
    precise, per-item allows.
  - **Tier 2** (once Tier 1 is exhausted) — fix the next single-instance,
    non-structural lint site from a fixed priority list.
  - **`tier=none`** once both are exhausted — the routine's stop signal.
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
- **State:** there is **no separate progress-tracking file**. The ledger is
  regenerated live from a fresh workspace scan every run — a fixed site
  simply stops appearing next time, which is both the state store and the
  completion signal. Do not invent a tracking file for this.
- **Budget guardrails** (why this routine is scoped the way it is): a full
  nightly session must stay short — one small, mechanical, verification-gated
  fix, never an open-ended cleanup pass — to avoid running long enough to
  threaten the 5-hour usage window, and to guarantee the session always ends
  with either a pushed commit or an explicit no-op report, never silently
  mid-work. `.github/workflows/ci.yml` runs the full CI gate on every push to
  `main` that touches anything outside `docs/**`; since these fixes touch
  `.rs` files, **exactly one push per night** keeps this to one CI run per
  night instead of a flood.
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
> run either ends with exactly one pushed commit, or with zero code changes
> and a clear "no-op: `<reason>`" report. Never leave the working tree dirty,
> never leave a partially-applied fix, never push more than once.

### 0. Pre-flight: confirm `main`'s CI is green

1. `git fetch origin && git checkout main && git pull --ff-only`.
2. Check the most recent CI run on `main` (e.g. `gh run list --branch main
   --limit 1 --json status,conclusion,headSha`). If it did not succeed, do
   **not** add a new commit on top of a red `main` tonight — report which run
   failed and why (if determinable), and stop. No-op.

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
   for step 2.

### 2. Get tonight's target

Run `cargo xtask clippy-ledger --next`.

- `tier=none` → the artifact is already updated. Report "no safe candidates
  remain" with the current ledger totals, and stop. No code changes, no
  commit.
- `tier=1` → a file path, its current blanket allow line, and its LOC.
- `tier=2` → a lint name, file, and line.

### 3. Apply exactly that one fix

- **Tier 1:** open the file, remove the module-level
  `#![allow(clippy::too_many_lines, dead_code)]` line, then run the crate's
  clippy check (see step 4) to see exactly what it now flags:
  - For every `too_many_lines` hit, add a scoped `#[allow(clippy::too_many_lines)]`
    directly on that function only.
  - For any `dead_code` hit: if the flagged item is genuinely unused, delete
    it (this is the one case where the fix is a real behavior change — see
    the commit/changelog rule in step 5). If it's a false positive you can't
    safely resolve by deletion, add a narrowly-scoped `#[allow(dead_code)]`
    directly on that item with a one-line reason comment.
  - Touch nothing else in the file. Do not restructure functions to satisfy
    `too_many_lines` — narrowing the allow's scope is the fix, not making the
    function shorter.
- **Tier 2:** apply the specific, non-structural fix for that lint at that
  site only (e.g. add `#[must_use]`, drop redundant raw-string hashes, rename
  a field, merge duplicate match arms, add the missing `# Errors`/`# Panics`
  doc section). Touch nothing else.

If, once you're in the code, the "safe" fix turns out to need judgment calls
beyond a mechanical change (e.g. the Tier-1 file's `too_many_lines` sites are
tangled with other pedantic debt, or the Tier-2 site's fix isn't actually
trivial), stop, revert, and treat it as a verification failure (step 4) —
don't improvise a larger change.

### 4. Verify (focused, not the full closeout gate)

Run, scoped to the touched crate only:

- `cargo fmt --all -- --check`
- `cargo clippy -p <touched crate> --all-targets -- -D warnings`, plus the
  `--features mysql` pass too if the touched crate is feature-gated by it
  (currently only `ersatzrs-infra`)
- `cargo xtask hydration-lint -p ersatzrs-web` if the touched file is an
  admin page
- A focused `./run.sh test -p <crate> <module filter>` if the touched
  file/module has existing test coverage

Full CI (`ci.yml`) remains the authoritative gate after push — this local
pass is a fast, cheap pre-check, not a substitute for it.

**If verification fails:** revert the change (`git checkout -- <file>` or
equivalent — leave the working tree clean), do not commit, do not push.
Report what was attempted and why it was deferred. Because the ledger is
regenerated live, the same target is simply retried on a future run; no
progress is lost.

### 5. Commit

**If verification passes**, one commit covering only tonight's single fix:

- Scope from `commitlint.config.cjs`'s `scope-enum` (`web`, `infra`,
  `scanner`, `core`, `app`, `ffmpeg`, `domain`, ...).
- `refactor(<scope>): narrow blanket clippy allow in <file>` for
  allow-narrowing/relocation with **no behavior change** — no `CHANGELOG.md`
  entry (internal lint hygiene; a nightly entry for every single-site fix
  would itself become noise the Changelog Contract doesn't ask for).
- `fix(<scope>): ...` instead, **with** a one-line `CHANGELOG.md` entry under
  `Removed`, only when real dead code was actually deleted (a genuine,
  if invisible, behavior change).
- `npx commitlint --from HEAD~1` — must pass; amend the message if not.
- Push directly to `main`.

### 6. Report

State: the pre-flight CI check result, whether the artifact was republished,
tonight's target and tier, the verification results, the commit (or the
no-op reason), and the fresh ledger totals (sites remaining, Tier-1 files
remaining, Tier-2 candidates remaining) so the maintainer can see the trend
without opening the artifact.
