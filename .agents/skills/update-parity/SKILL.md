---
name: update-parity
description: Update or reopen an ErsatzRS parity record. Use when a port starts, progresses, completes, is deferred/dropped, or runtime evidence disproves a previously Ported or Implemented claim.
---

# Update Parity

Parity is tracked as one file per feature under `docs/progress/parity/`. The
old monolithic `docs/reference/parity-matrix.md` table is now a stub; do not
edit it for parity data.

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
4. Add an open milestone with measurable acceptance and a live plan. Put
   release-critical corrective work in `phase.toml` next steps.
5. Update `[Unreleased]`, prepend a session entry, and run
   `cargo xtask progress check`.

When closing the regression, record the resolution and concrete test/runtime
evidence, complete the milestone, restore `Ported`/`Implemented` only when no
linked debt remains, and prune a plan only after its substance is preserved in
structured progress.

## Rule

Every change that completes, intentionally changes, or reopens parity updates
the relevant feature file and passes `cargo xtask progress check`.
