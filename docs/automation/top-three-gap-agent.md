# Top-three gap agent routine

This is the contract for a coordinator agent that closes the numbered goal
prompts in [`docs/top-three-gaps-to-fill-prompts.md`](../top-three-gaps-to-fill-prompts.md).
It is an implementation routine, not the daily docs-only progress routine.

- **Delivery:** scheduled remote Codex job against `bartdeijkers/ErsatzRS`.
- **Cadence:** run once per day after
  [`daily-progress-routine.md`](daily-progress-routine.md) has had time to
  refresh the prompt list.
- **Agent:** `top-three-gap-orchestrator` from
  [`.codex/agents/top-three-gap-orchestrator.toml`](../../.codex/agents/top-three-gap-orchestrator.toml).
- **Stop condition:** do not start new implementation work when all numbered
  sections are already completed and their PRs are mergeable.
- **State:** open PRs, branch names, and completed markers in
  `docs/top-three-gaps-to-fill-prompts.md`.
- **Scheduler owner:** the external scheduler/account must point at this file
  and run the `top-three-gap-orchestrator` agent. The repo-local contract lives
  here; GitHub Actions does not run this workflow.

## Role

The coordinator runs the current top-three gap list as three independent goal
slots:

1. One isolated implementation worker workspace per numbered gap section.
2. One review loop per gap.
3. One pull request per gap after review passes.
4. One final readiness pass that makes the PRs mergeable in gap order: 1, 2, 3.

Use `gpt-5.5` with `medium` reasoning effort for implementation workers unless
the maintainer explicitly changes the model.

## Inputs

- `AGENTS.md`
- `skills/parity-slice.md`
- `docs/top-three-gaps-to-fill-prompts.md`
- `docs/automation/gap-prompt-rules.md`
- The relevant `docs/progress/parity/<slug>.toml` files

Treat the numbered sections in `docs/top-three-gaps-to-fill-prompts.md` as the
queue. A section already marked `COMPLETED` still occupies its slot, but the
worker task becomes verification of completion state instead of reimplementing
the gap.

## Coordinator checklist

Track this state locally while the routine runs:

| Gap | Slug | Branch | Workspace | Worker | Review | PR | CI |
|---|---|---|---|---|---|---|---|
| 1 |  |  |  |  |  |  |  |
| 2 |  |  |  |  |  |  |  |
| 3 |  |  |  |  |  |  |  |

Do not use a shared implementation branch for multiple gaps. Prefer
`gap/<number>-<slug>`.

## Workspace isolation

Parallel implementation workers must not edit the same working directory.

Use this order:

1. Prefer Codex-provided forked workspaces for workers. Record each worker's
   workspace identity in the coordinator checklist.
2. If forked workspaces are unavailable, create one local git worktree per gap
   before spawning workers:
   - `../ErsatzRS-gap-1` on branch `gap/1-<slug>`
   - `../ErsatzRS-gap-2` on branch `gap/2-<slug>`
   - `../ErsatzRS-gap-3` on branch `gap/3-<slug>`
3. Use one shared checkout only when work is intentionally sequential, never
   for the three-worker fan-out.

Review workers inspect the matching worker branch/workspace. The final
readiness worker operates on PR branches in order 1, 2, 3 after the review
loops are closed.

## Worker prompt template

Assign each worker exactly one numbered section:

```text
/goal
You own gap <number> only: <section title> from
docs/top-three-gaps-to-fill-prompts.md.

Use AGENTS.md plus skills/parity-slice.md. The goal is fully functional
ErsatzTV parity without stopgaps for this gap only.

Work on branch gap/<number>-<slug> in your assigned isolated workspace. Other
agents may be editing related files in their own workspaces; do not revert their
changes. Keep your write scope to this gap's modules, tests, parity docs,
active plan files, and your branch/PR metadata.

Before editing, build the Source Map requested by the gap prompt, and a Visual
Source Map if the gap touches admin UI. Preserve visible ParityDebt for anything
not closed.

When done, report:
- Source Map and Visual Source Map, if applicable
- behavior implemented
- remaining ParityDebt
- tests and runtime/browser proof run
- changed files
- whether the branch is ready for review
```

For a section already marked completed, replace "behavior implemented" with:

```text
Verify the completion marker, commit/PR evidence, progress docs, and required
verification. Do not reimplement closed work unless the marker is wrong.
```

## Review loop

After a worker says the gap is done, run a review worker for that same gap.
The review worker must inspect the diff and evidence against:

- the original numbered prompt
- `AGENTS.md`
- `skills/parity-slice.md`
- relevant progress/parity TOML debt
- tests, smoke/browser proof, and docs updates

If review finds required changes, convert the findings into a new goal for the
same implementation worker:

```text
/goal
Address review feedback for gap <number> (<slug>) only.

Required changes:
<actionable review findings>

Keep the same branch. Do not expand scope beyond these findings unless needed
to preserve parity. Report changed files and verification.
```

Repeat worker -> review -> feedback until the review worker has no required
changes. Review comments that are optional must not block PR creation.

## PR routine

When a gap passes review:

1. Run the gap prompt's required focused checks.
2. Run `cargo xtask progress check`.
3. Mark the matching section in
   `docs/top-three-gaps-to-fill-prompts.md` as completed, with the date,
   branch/PR, and commit if known.
4. Open exactly one PR for that gap.
5. Include the Source Map summary, behavior implemented, remaining ParityDebt,
   and verification commands in the PR body.

Do not mark the gap completed before review passes and the progress check is
green.

## Final readiness worker

After all three gaps are completed or verified complete, spawn a final worker:

```text
/goal
Make the three top-gap PRs ready and easy to merge in order 1, 2, 3 from
docs/top-three-gaps-to-fill-prompts.md.

Check each PR's CI and mergeability in order. Fix CI errors on the matching PR
branch, rebase or update branches as needed, preserve one PR per gap, and keep
later PRs stacked or rebased after earlier ones when dependencies require it.

Report final PR order, CI status, merge blockers, and any commands run.
```

The final worker must not combine PRs, close unresolved required review
feedback, or claim CI success without checking the current PR status.
