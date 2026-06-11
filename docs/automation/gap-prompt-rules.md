# Gap goal-prompt rules

This is the **contract** for the three goal-prompts written into
[`docs/top-three-gaps-to-fill-prompts.md`](../top-three-gaps-to-fill-prompts.md)
by the [daily progress routine](daily-progress-routine.md). A goal-prompt is a
self-contained instruction that a fresh agent can paste and run to close exactly
one ErsatzTV parity gap end-to-end. [`prompt.md`](../../prompt.md) at the repo
root is the canonical worked example; every generated prompt must read like it.

## What a goal-prompt is

- **One gap, one prompt.** Each prompt targets a single `docs/progress` parity
  slug (or one route within it). Never bundle two gaps into one prompt.
- **No-stopgap.** The stated goal is always *fully functional ErsatzTV parity
  without stopgaps*. The prompt must forbid hiding incomplete behavior in prose
  and require any shortfall to be recorded as visible `ParityDebt`.
- **Self-contained but thin.** The prompt delegates process to the repo's own
  contracts rather than restating them. It must reference `AGENTS.md` and the
  canonical `parity-slice` skill (`skills/parity-slice.md`), not re-explain
  them.
- **Agnostic verification.** Required verification steps must be plain commands
  any agent can run, such as `cargo xtask ...`, `npx playwright test ...`, or
  `tsp ...`. MCP browser sessions may be listed as a convenience, never as the
  only documented path.

## Required structure (in order)

Each generated prompt MUST contain these sections, mirroring `prompt.md`:

1. **Title line** — `Fix the next ErsatzTV parity gap in ErsatzRS: <feature> (slug `<slug>`)`,
   naming the route(s) when the gap is UI.
2. **Workflow pointer** — instruct use of the `AGENTS.md` workflow plus
   `skills/parity-slice.md`; state the no-stopgap goal.
3. **Before editing** — read ErsatzTV source under `$ERSATZTV_SRC/`, inspect the
   current ErsatzRS implementation/routes/docs/tests, build a **Source Map**, and
   for admin-UI routes a **Visual Source Map**, then identify `ParityDebt`.
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
   `Ported`).
2. Rank by:
   - **Phase focus first** — gaps named in `phase.toml` `current_focus` /
     `next_steps`.
   - then **`In progress`** before **`Not started`** (finish-what-is-started),
   - then **enabling/foundational** features before leaf UI (a gap many other
     gaps depend on outranks a standalone page),
   - then **slug alphabetical** as a stable tiebreaker.
3. Take the top three. If fewer than three gaps remain, emit only that many and
   say so. If a gap was already the subject of a prompt in the previous run and
   saw **no commits** since, keep it but note it is carried over; if it *was*
   worked on, re-rank normally (it may drop off as it nears `Ported`).

## Grounding rules (no invention)

- **Parity baseline:** only treat behavior present in ErsatzTV at the pinned
  commit in [`parity-baseline.md`](parity-baseline.md) as an in-scope gap. Never
  write a prompt for ErsatzTV work added upstream after the pin. Each prompt must
  instruct the implementing agent to read ErsatzTV source **at that commit**.
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
