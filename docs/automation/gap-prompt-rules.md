# Gap goal-prompt rules

This is the **contract** for the three goal-prompts written into
[`docs/top-three-gaps-to-fill-prompts.md`](../top-three-gaps-to-fill-prompts.md)
by the [daily progress routine](daily-progress-routine.md). A goal-prompt is a
self-contained instruction that a fresh agent can paste and run to close exactly
one **chore** end-to-end, where a chore is one of:

- an **ErsatzTV parity gap** (a feature not yet `Ported`, or one open `[[debt]]`
  item inside any feature — even a `Ported` one), or
- an **ADR-backed parity-critical runtime hardening gap**, or
- a **spec-backed chore**: the next implementation slice of a `Proposed` /
  `Not implemented` ADR, an active `plans/*.md` goal that is not yet built, or
  an open `phase.toml` `next_steps` / `known_todo` item.

The three prompts always describe the **top three most important chores the
specs say still need doing**, ranked by the selection rules below. The file is
empty of actionable prompts **only** when every one of those spec sources is
exhausted — not merely when `cargo xtask progress gaps` is clear (it goes clear
long before the specs are fully implemented, because it only inspects
whole-feature parity status). [`prompt.md`](../../prompt.md) at the repo root is
the canonical worked example; every generated prompt must read like it.

## What a goal-prompt is

- **One chore, one prompt.** Each prompt targets a single `docs/progress` parity
  slug (or one route within it), one open `[[debt]]` item, one ADR-backed
  runtime hardening gap tied to a tracked streaming/runtime slug, or **one
  implementation slice of a single spec-backed chore** (a `Proposed` ADR, an
  active plan goal, or a phase TODO). Never bundle two chores into one prompt; a
  large spec (e.g. a multi-slice ADR) is scoped down to its next coherent slice,
  not its whole roadmap.
- **No-stopgap.** For ErsatzTV parity prompts, the stated goal is *fully
  functional ErsatzTV parity without stopgaps*. For ADR-backed FFmpeg/HLS
  hardening prompts, the stated goal is the fully validated runtime behavior
  promised by the ADR/plan. For spec-backed chores, the stated goal is the
  complete, working behavior the ADR/plan/TODO describes for the slice in scope.
  In every case, the prompt must forbid hiding incomplete behavior in prose and
  require any shortfall to be recorded as visible `ParityDebt` or ADR/plan
  implementation debt.
- **Self-contained but thin.** The prompt delegates process to the repo's own
  contracts rather than restating them. It must reference `AGENTS.md` and the
  canonical `parity-slice` skill (`skills/parity-slice.md`) for ErsatzTV parity
  work; ADR-backed hardening and spec-backed chore prompts must instead point at
  the relevant ADR/plan and not force the parity-slice workflow where there is no
  pinned ErsatzTV behavior to port. A spec-backed chore that *creates* a new
  parity slug (e.g. ADR 0018 → `content-variants`) must say so and instruct the
  agent to register the slug under `docs/progress/parity/` in the same slice.
- **Agnostic verification.** Required verification steps must be plain commands
  any agent can run, such as `cargo xtask ...`, `npx playwright test ...`, or
  `tsp ...`. MCP browser sessions may be listed as a convenience, never as the
  only documented path.

## Required structure (in order)

Each generated prompt MUST contain these sections, mirroring `prompt.md`:

1. **Title line** — `Fix the next ErsatzTV parity gap in ErsatzRS: <feature> (slug `<slug>`)`,
   naming the route(s) when the gap is UI; or `Fix the next ErsatzRS
   parity-infrastructure gap: <ADR/runtime target>` for ADR-backed FFmpeg/HLS
   hardening; or `Build the next ErsatzRS spec chore: <ADR/plan/TODO target>
   (slug `<slug>`)` for a spec-backed chore (cite the new slug it will create
   when the chore has none yet).
2. **Workflow pointer** — instruct use of the `AGENTS.md` workflow plus
   `skills/parity-slice.md` for ErsatzTV parity prompts; for ADR-backed
   FFmpeg/HLS hardening and spec-backed chores, point at the governing ADR/plan
   instead. State the no-stopgap goal.
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

## Selection rules (which three chores)

The routine picks the three chores **auto-prioritized from the specs**. Build
the candidate set by sweeping **all** of these sources every run — never stop at
the first one that is empty:

1. **Parity gaps:** `cargo xtask progress gaps` (every feature not `Ported`).
2. **Open parity debt:** every `[[debt]]` item in any
   `docs/progress/parity/<slug>.toml` whose `resolved` is not `true` — including
   debts inside features whose overall status is already `Ported`. These do
   **not** appear in `progress gaps`.
3. **ADR-backed runtime hardening:** FFmpeg/HLS hardening items tied to tracked
   progress slugs or explicit plans.
4. **Unimplemented ADRs:** every ADR whose Status is `Proposed` **or** whose
   Implementation is `Not implemented` (`cargo xtask progress adr list` — the
   `Proposed` / `Not implemented` rows). Scope each to its next coherent
   implementation slice and, when it has no slug yet, name the slug it will
   create (e.g. ADR 0018 → `content-variants`).
5. **Active plan goals:** `plans/*.md` files whose status is active/proposed and
   whose goal is not yet built, plus the `plans/INDEX.md` "Deferred Contract
   Work" list once the thing it was waiting on now exists.
6. **Open phase TODOs:** `docs/progress/phase.toml` `next_steps` items and
   `[[known_todo]]` notes that are still open.

Then rank the combined set by:

- **Phase focus first** — chores named in `phase.toml` `current_focus` /
  `next_steps`.
- then **blocking runtime debt** — an open `[[debt]]` item with confirmed
  user-visible runtime evidence outranks everything except phase focus.
- then **pinned ErsatzTV parity gaps** before ADR-backed hardening while there
  are at least three strong pinned-baseline candidates,
- then **ADR-backed FFmpeg/HLS hardening**, which may fill remaining slots when
  the pinned-baseline parity pool has fewer than three actionable candidates or
  the remaining baseline gaps are mostly proof/coverage while streaming runtime
  debt blocks release confidence,
- then **spec-backed chores** — unimplemented ADRs, active plan goals, and phase
  TODOs — ranked among themselves by maintainer-requested capability first (an
  ADR/plan that records an explicit maintainer request), then release-blocking
  hardening, then developer-experience/cleanup TODOs,
- then **`In progress`** before **`Not started`** (finish-what-is-started),
- then **enabling/foundational** work before leaf UI (a chore many other chores
  depend on outranks a standalone page),
- then **slug / ADR number / plan filename alphabetical** as a stable tiebreaker.

Take the top three. **Only emit fewer than three — or an empty file — when every
source 1–6 above is genuinely exhausted** (full parity *and* every Proposed ADR,
active plan, and open phase TODO implemented). A clear `cargo xtask progress
gaps` alone is never sufficient justification for an empty file; if sources 2–6
still hold work, the file must carry three actionable chores. When you do emit
fewer than three, name which sources you swept and why each is empty.
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
  commit in [`parity-baseline.md`](parity-baseline.md) as an in-scope **parity**
  gap. Never write a parity-porting prompt for ErsatzTV work added upstream after
  the pin. Each parity-porting prompt must instruct the implementing agent to
  read ErsatzTV source **at that commit**.
- **Spec chores are not parity gaps.** ADR-backed hardening and spec-backed
  chores (Proposed ADRs, active plans, phase TODOs) are deliberate ErsatzRS
  decisions that may have **no** ErsatzTV equivalent — that is allowed and is the
  whole point of those sources. They must cite the governing ADR/plan/TODO,
  explain why the chore matters now, and must **not** pretend to be
  pinned-baseline ErsatzTV gaps or invent an `etv_source` they do not have. When
  an ADR/plan does reference ErsatzTV behavior to reverse or extend (e.g. ADR
  0018's scanner discard lists), point at the exact `$ERSATZTV_SRC` paths it
  names.
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
