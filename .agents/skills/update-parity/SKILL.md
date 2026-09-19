---
name: update-parity
description: Update or reopen an ErsatzRS parity record. Use when a port starts, progresses, completes, is deferred/dropped, or runtime evidence disproves a previously Ported or Implemented claim.
---

# Update Parity

Parity is tracked as one file per feature under `docs/progress/parity/`. The
old monolithic `docs/reference/parity-matrix.md` table is now a stub; do not
edit it for parity data.

## Milestone agreement and evidence

Use the existing milestone record:

- `title`: the viewer or operator outcome.
- `acceptance`: concise scope and exclusions, followed by observable success
  conditions and any mandated proof method.
- `evidence`: current results mapped to those conditions, with references,
  relevant revision/environment, failures and remaining gaps.
- `plan`: optional pointer to detailed future implementation work under
  `plans/`. Omit it for any status when the plan fits in the milestone TOML;
  a supplied pointer must resolve. Link to acceptance and evidence rather than
  copying them into Markdown.

Detailed current milestone evidence belongs in that milestone's `evidence`.
Parity files retain feature status and owning debt; debt entries link to
milestone evidence where shared. Sessions summarize checkpoints. ADRs own
decisions. Other documents link or summarize briefly.

Keep history in the owning TOML. When pruning a completed or superseded plan,
transfer unique results, revision/test references and remaining limitations
before removing it. Do not change agreed acceptance or infer completion from
implementation notes. ADRs retain decisions; reusable design references live
outside `plans/`.

## Workflow

1. Find the feature file: `cargo xtask progress parity` lists slugs, or look in
   `docs/progress/parity/<slug>.toml`.
2. For a new feature, create `docs/progress/parity/<slug>.toml` with this shape:

   ```toml
   feature    = 'Human-readable feature name'
   etv_source = '$ERSATZTV_SRC/path/to/Source.cs'
   rs_target  = 'crates/ersatzrs-<crate>'
   status     = 'In progress'
   adr        = ['0003']          # ADR numbers, or [] if none
   summary    = '''
   What is ported, what remains. Mirror the prose style of existing files.
   '''
   ```

3. Set `status` to one of: `Not started`, `In progress`, `Ported`,
   `Deferred`, `Dropped`.
4. List ADR numbers in `adr` for any deviation or dropped behavior (they must
   resolve to a file under `docs/adr/`).
5. If a route or hydration state changed, also update
   `docs/progress/routes.toml` / `docs/progress/hydration.toml`.
6. If an active plan milestone started, completed, blocked, or was deferred,
   update `docs/progress/milestones.toml`; a `Complete` milestone requires
   concrete verification evidence.
7. Validate: `cargo xtask progress check`.

## Reopen a regression

When runtime evidence disproves a completed claim:

1. Preserve a redacted, reproducible observation and separate confirmed facts
   from hypotheses. Do not commit library names, paths, URLs, or titles.
2. Set each affected parity feature to `In progress` and add one focused open
   `[[debt]]` with `discovered`, evidence, `resolved = false`, and the owning
   ADR when applicable.
3. If a linked ADR begins its Implementation section with `Implemented`, change
   that implementation token to `Partial` and explain the reopened gap. Do not
   change an accepted decision's Status unless the decision itself changed.
4. Reuse the owning milestone and live plan when they cover the correction.
   Reopen a completed milestone if its acceptance is disproven; retain or
   restore a live plan only when future detail needs Markdown. Add a new
   milestone only for separately agreed delivery scope; a missing plan does
   not require a new milestone.
   Put release-critical corrective work in `phase.toml` next steps.
5. Update `[Unreleased]` and run `cargo xtask progress check`. Use the
   existing Session Handoff mechanism in `AGENTS.md` at handoff.

When closing the regression, record the resolution and concrete test/runtime
evidence. Complete the owning milestone only when all its acceptance is proven;
restore `Ported`/`Implemented` only when no linked debt remains. Prune a plan
only after its substance is preserved in structured progress.

## Rule

Every change that completes, intentionally changes, or reopens parity updates
the relevant feature file and passes `cargo xtask progress check`.
