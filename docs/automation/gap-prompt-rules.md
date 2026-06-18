# Gap goal-prompt rules

This is the **contract** for the three goal-prompts written into
[`docs/top-three-gaps-to-fill-prompts.md`](../top-three-gaps-to-fill-prompts.md)
by the [daily progress routine](daily-progress-routine.md). A goal-prompt is a
self-contained instruction that a fresh agent can paste and run to close exactly
one ErsatzTV parity gap or ADR-backed parity-critical runtime hardening gap
end-to-end. [`prompt.md`](../../prompt.md) at the repo root is the canonical
worked example; every generated prompt must read like it.

## What a goal-prompt is

- **One gap, one prompt.** Each prompt targets a single `docs/progress` parity
  slug (or one route within it), or one ADR-backed runtime hardening gap tied to
  a tracked streaming/runtime slug. Never bundle two gaps into one prompt.
- **No-stopgap.** For ErsatzTV parity prompts, the stated goal is *fully
  functional ErsatzTV parity without stopgaps*. For ADR-backed FFmpeg/HLS
  hardening prompts, the stated goal is the fully validated runtime behavior
  promised by the ADR/plan. In both cases, the prompt must forbid hiding
  incomplete behavior in prose and require any shortfall to be recorded as
  visible `ParityDebt` or ADR implementation debt.
- **Self-contained but thin.** The prompt delegates process to the repo's own
  contracts rather than restating them. It must reference `AGENTS.md` and the
  canonical `parity-slice` skill (`skills/parity-slice.md`) for ErsatzTV parity
  work; ADR-backed FFmpeg/HLS hardening prompts must instead point at the
  relevant ADR/plan and not force the parity-slice workflow where there is no
  pinned ErsatzTV behavior to port.
- **Agnostic verification.** Required verification steps must be plain commands
  any agent can run, such as `cargo xtask ...`, `npx playwright test ...`, or
  `tsp ...`. MCP browser sessions may be listed as a convenience, never as the
  only documented path.

## Required structure (in order)

Each generated prompt MUST contain these sections, mirroring `prompt.md`:

1. **Title line** — `Fix the next ErsatzTV parity gap in ErsatzRS: <feature> (slug `<slug>`)`,
   naming the route(s) when the gap is UI; or `Fix the next ErsatzRS
   parity-infrastructure gap: <ADR/runtime target>` for ADR-backed FFmpeg/HLS
   hardening.
2. **Workflow pointer** — instruct use of the `AGENTS.md` workflow plus
   `skills/parity-slice.md` for ErsatzTV parity prompts; for ADR-backed
   FFmpeg/HLS hardening, point at the relevant ADR/plan instead. State the
   no-stopgap goal.
3. **Before editing** — for ErsatzTV parity prompts, read ErsatzTV source under
   `$ERSATZTV_SRC/`; for ADR-backed hardening prompts, read the relevant
   ADR/plan and only compare ErsatzTV where it informs the runtime contract.
   Inspect the current ErsatzRS implementation/routes/docs/tests, build a
   **Source Map**, and for admin-UI routes a **Visual Source Map**, then identify
   `ParityDebt` or ADR implementation debt.
4. **Implement the slice** — match ErsatzTV fields, actions, labels, validation,
   routes, persistence, side effects, and (for UI) visible layout; preserve
   SSR/browser-safe fallbacks; follow `TEST.md` hydration rules; add focused
   tests; update parity/status docs; decide whether any intentional deviation
   from ErsatzTV needs a new ADR.
5. **Visual verification** — UI routes only: serve via `./run.sh serve` after
   rebuilding Leptos assets, capture same-viewport screenshots vs the ErsatzTV
   reference, compare geometry not just DOM presence.
6. **Verification gates** — the narrow useful gates (normally
   `cargo xtask parity-check web` for UI; focused crate tests otherwise), any
   runnable Playwright spec added or updated for UI behavior
   (`npx playwright test ...`), plus `cargo xtask progress check`.
7. **Reporting** — report Source Map, Visual Source Map (UI), behavior
   implemented, remaining `ParityDebt`, verification run, and changed files.

UI vs non-UI: include sections 4–5's visual material **only** when the gap has an
admin route or an ErsatzTV Razor/MudBlazor reference. For pure backend/pipeline
gaps (e.g. FFmpeg command generation), drop the visual sections and lean on
snapshot tests (`insta`) as the regression gate instead.

## Selection rules (which three gaps)

The routine picks the three gaps **auto-prioritized from `docs/progress`**:

1. Source the candidate set from `cargo xtask progress gaps` (everything not
   `Ported`), plus ADR-backed FFmpeg/HLS runtime hardening items that are tied to
   tracked progress slugs or explicit plans.
2. Rank by:
   - **Phase focus first** — gaps named in `phase.toml` `current_focus` /
     `next_steps`.
   - then **pinned ErsatzTV parity gaps** before ADR-backed hardening while there
     are at least three strong pinned-baseline candidates,
   - then **ADR-backed FFmpeg/HLS hardening** may fill remaining slots when the
     pinned-baseline parity pool has fewer than three actionable candidates or
     the remaining baseline gaps are mostly proof/coverage while streaming
     runtime debt blocks release confidence,
   - then **`In progress`** before **`Not started`** (finish-what-is-started),
   - then **enabling/foundational** features before leaf UI (a gap many other
     gaps depend on outranks a standalone page),
   - then **slug alphabetical** as a stable tiebreaker.
3. Take the top three. If fewer than three gaps remain, emit only that many and
   say so.
4. **Completed scope is always replaced, never kept.** Before any carry-over,
   check each prior prompt's scope against the reconciled parity files: if the
   feature flipped to `Ported`/`Deferred`/`Dropped`, or every debt id the
   prompt targeted is now closed, that prompt is dead. Remove it in the same
   run — never keep it with a "completed" banner or a "for reference" note —
   and refill the slot from the ranking: either a fresh prompt against the
   same slug's *remaining open* debts (if the slug still ranks top three) or
   the next-ranked gap. The published file must always contain three
   actionable prompts (fewer only when fewer open gaps exist), because daily
   agents work directly from this file.
5. Carry-over applies only to prompts whose scope is still open. If a prior
   pick saw **no commits** since, keep it but note it is carried over; if it
   *was* worked on and its targeted debts are partly closed, re-rank normally
   and rewrite the prompt against the current debt state (it may drop off as
   it nears `Ported`).

## Refinement rules (within a selected gap)

After selection, shape each prompt by the feature's structured `[[debt]]`
items, not only its summary prose:

- **Blocking runtime debt first.** A debt item with confirmed-blocking runtime
  evidence (a user-visible failure observed in real use: a save that cannot
  complete, a channel that dies, a resource limit violated) outranks
  coverage/proof/i18n debt *within the same feature*. The prompt must lead
  with the blocking debt as its primary scope; large mechanical coverage work
  (full i18n sweeps, multi-language browser proof) moves to a follow-up slice
  and stays tracked as visible `ParityDebt`.
- **No "if in scope" hedges.** Every named debt item is either inside the
  slice or explicitly deferred with its debt entry kept open. A prompt must
  never leave a runtime-evidenced debt behind a conditional clause.
- **Diagnosis-first on repeat failures.** If a debt item already received a
  landed fix and later runtime evidence shows the same failure signature, the
  prompt must require diagnosing why the fix is bypassed at runtime
  (instrument the live spawn/state, compare it to what tests exercise) before
  prescribing another iteration of the same approach. Re-tightening a gate
  that already failed once, on snapshot proof alone, is not an acceptable
  slice.
- **Fold hardware-bound verification.** Verification-only debt that needs
  specific hardware (e.g. a live NVENC/CUDA re-verification) is never a
  standalone prompt or prompt half when it is only proof collection. Fold it
  into the verification run of another slice that needs the same hardware
  session, name the debt id it closes, and follow the
  [runtime triage routine](runtime-triage.md) for capturing the proof. If the
  work also changes ADR-backed FFmpeg/HLS runtime behavior or release packaging,
  it may be selected as a hardening prompt, with the hardware proof listed as one
  validation gate rather than the whole scope.

## Grounding rules (no invention)

- **Parity baseline:** only treat behavior present in ErsatzTV at the pinned
  commit in [`parity-baseline.md`](parity-baseline.md) as an in-scope gap. Never
  write a prompt for ErsatzTV work added upstream after the pin. Each prompt must
  instruct the implementing agent to read ErsatzTV source **at that commit** when
  the prompt is a parity-porting prompt. ADR-backed FFmpeg/HLS hardening prompts
  must not pretend to be pinned-baseline ErsatzTV gaps; they must cite the
  governing ADR/plan and explain why the hardening is parity-critical.
- Every slug, route, status, and `etv_source` path in a prompt MUST come from
  the actual `docs/progress` files or repo — never invent a path or feature.
- Quote the gap's current `summary` / remaining-work text so the agent starts
  from the real ParityDebt, not a guess.
- If `$ERSATZTV_SRC/` paths are referenced, use the ones recorded in the
  feature's `parity/<slug>.toml` `etv_source` field.

## House style

- Imperative voice, second person, terse. No preamble, no marketing.
- Keep each prompt to roughly the length of `prompt.md` (~40–60 lines).
- English only (repository language contract).
