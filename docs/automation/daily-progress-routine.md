# Daily progress routine (cloud loop)

This is the prompt run by the scheduled remote agent ("cloud loop") once per day
against `bartdeijkers/ErsatzRS` `main`. It reconciles `docs/progress/` with what
actually merged since the last run and regenerates the three gap goal-prompts.
Edit this file to change the routine's behavior; the schedule points at this
contract.

- **Delivery:** commits doc-only changes **directly to `main`**.
- **Selection:** auto-prioritized from `docs/progress` (see
  [gap-prompt-rules.md](gap-prompt-rules.md)).
- **State:** the last processed commit is stored in the front-matter of
  [`docs/top-three-gaps-to-fill-prompts.md`](../top-three-gaps-to-fill-prompts.md)
  (`last_processed_commit`). That is the "since last check" marker.
- **Parity baseline:** gaps are measured **only** against the pinned ErsatzTV
  commit in [`parity-baseline.md`](parity-baseline.md). Upstream ErsatzTV work
  after that commit is out of scope until the pin is advanced (which the routine
  never does on its own).
- **Schedule owner:** this routine is intended for the maintainer-owned
  scheduled remote agent against `bartdeijkers/ErsatzRS` `main`. The repository
  contract lives here; the external scheduler/account must point at this file
  and commit with the `daily-progress-routine` agent name.

---

## Prompt

You are maintaining ErsatzRS porting progress. Work only in this repo's docs;
do **not** change Rust/source code. Follow `AGENTS.md`.

> **Hard guardrail — no-op when nothing changed.** If `main` has **no new
> commits** since `last_processed_commit`, make **zero** edits, write **nothing**,
> create **no** commit, report "no changes since `<baseline>`", and stop. The
> routine must never produce an empty/no-op commit or rewrite the gap prompts
> when the change window is empty.

### 1. Determine the change window

1. Ensure you are on `main` and up to date (`git fetch origin && git checkout
   main && git pull --ff-only`).
2. Read `last_processed_commit` from the front-matter of
   `docs/top-three-gaps-to-fill-prompts.md`. If the file or field is missing,
   use the commit from 7 days ago (`git rev-list -1 --before='7 days ago' main`)
   as the baseline.
3. Compute the change set: `git log --no-merges --stat <baseline>..HEAD`. If
   there are **no** new commits, make no edits, report "no changes since
   `<baseline>`", and stop without committing.

### 1b. Pin the ErsatzTV parity baseline

Before evaluating any gap, honor [`parity-baseline.md`](parity-baseline.md):

- Ensure the `$ERSATZTV_SRC` checkout is at the pinned commit
  (`git -C "$ERSATZTV_SRC" rev-parse HEAD` must equal the pin); fetch and check
  it out if not. If it cannot be pinned, report that and make **no** parity edits.
- Treat as a gap **only** behavior present in ErsatzTV at the pinned commit.
  Anything upstream added after the pin is out of scope; never flag it, never
  write a goal prompt for it, and never regress a `Ported` feature because
  upstream moved past the pin.
- **Never advance the pin** — bumping the baseline is a separate, reviewed action.

### 2. Reconcile `docs/progress/`

For each merged change in the window, update the source-of-truth text files
(never `progress.db`):

- **Parity:** if a change advanced, completed, deferred, or dropped a feature,
  edit the matching `docs/progress/parity/<slug>.toml` — update `status` (`Not
  started` / `In progress` / `Ported` / `Deferred` / `Dropped`), refresh
  `summary` to reflect what is now ported vs. remaining, add structured
  `[[debt]]` entries for new open gaps, and add `adr` numbers for any deviation.
  Follow the canonical `skills/update-parity.md` skill. Do not overstate: only
  mark `Ported` when the change clearly closes the feature and no open
  structured debt remains (`cargo xtask progress check` fails that
  combination); otherwise keep `In progress` and record remaining work as
  structured `[[debt]]`.
  For streaming/runtime features, a passing `cargo xtask smoke` run is the
  preferred runtime evidence before a `Ported` flip.
  If the change deviates from ErsatzTV instead of porting it, explicitly decide
  whether an ADR is required before updating status.
- **Summary hygiene:** a `summary` describes the *current* state only —
  slice-by-slice history belongs in `sessions.ndjson` and git log. Keep every
  summary you touch under the 4,000-character budget that
  `cargo xtask progress check` warns on; when reconciling a feature whose
  summary is over budget, trim it in the same pass.
- **Runtime triage:** if the change window or maintainer report includes live
  log evidence, follow [runtime-triage.md](runtime-triage.md): cluster
  WARN/ERROR lines, map each cluster to a parity slug, add dated structured
  debt with the shortest useful evidence signature, and downgrade `Ported` when
  the evidence contradicts it.
- **Routes / hydration:** if a route became routed or a page gained/lost island
  hydration, update `docs/progress/routes.toml` / `hydration.toml`.
- **Phase:** if focus or remaining work shifted, update `phase.toml`
  `current_focus` / `next_steps` and tick any newly-complete checklist item.
- **Session log:** append one line via
  `cargo xtask progress add-session --agent "daily-progress-routine" --summary
  "<one line covering the window>"`. A session entry is required whenever a
  parity status changes so the status flip has a dated rationale.

Be conservative: when a commit is ambiguous, leave status unchanged and note the
uncertainty in the feature `summary` rather than guessing. Never invent a slug,
path, or feature.

### 3. Regenerate the three gap goal-prompts

Rewrite `docs/top-three-gaps-to-fill-prompts.md` following
`docs/automation/gap-prompt-rules.md` exactly:

1. Get candidates: `cargo xtask progress gaps`. Candidates include individual
   structured `[[debt]]` items, not only whole-feature slugs.
2. Rank and pick the top three per the selection rules in the rules file.
   Apply the completed-scope rule first: a prior prompt whose feature is now
   `Ported`/`Deferred`/`Dropped`, or whose targeted debt ids are all closed,
   is **removed and its slot refilled** — never kept as a "completed" /
   "for reference" entry. Every published prompt must be actionable today.
3. For each pick, read its `parity/<slug>.toml` (and `cargo xtask progress
   parity --feature <slug> --full`) to ground the prompt in the real
   `etv_source`, status, structured `[[debt]]` items, and remaining-work text.
4. Shape each prompt per the **refinement rules** in the rules file: lead with
   confirmed-blocking runtime debt, no "if in scope" hedges, diagnosis-first
   when a landed fix's failure signature reappears, and fold hardware-bound
   verification debt into another slice's verification run.
5. Write three goal-prompts that each mirror `prompt.md`'s structure (workflow
   pointer → before editing → implement → visual verification for UI →
   verification gates → reporting). Drop the visual sections for pure
   backend/pipeline gaps.
6. Update the front-matter: set `generated` to today's date and
   `last_processed_commit` to `HEAD` (`git rev-parse HEAD`).

### 4. Validate and commit

1. `cargo xtask progress check` — must pass. Fix any validation error you
   introduced before committing.
2. Stage only `docs/` changes. Commit directly to `main`:
   `docs(status): daily reconcile + refresh gap prompts (<baseline>..<head>)`.
   The scope must come from the `scope-enum` in `commitlint.config.cjs`
   (`progress` and `automation` are **not** valid scopes).
   End the commit message with the standard `Co-Authored-By` trailer.
3. Validate the commit message: `npx commitlint --from HEAD~1` — must pass.
   Amend the message if it does not.
4. Push to `main`.
5. Report: the change window, which parity files changed and how, the three
   gaps selected (with the ranking reason), and the new `last_processed_commit`.

If `cargo xtask progress check` cannot be made to pass from docs-only edits,
do **not** push — report the failure instead.
