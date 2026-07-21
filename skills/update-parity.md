---
name: update-parity
description: Update the ErsatzRS ErsatzTV parity record. Use when a porting task starts, progresses, completes, is deferred, or is deliberately dropped.
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

## Rule

Every PR that completes or intentionally changes parity updates the relevant
feature file and passes `cargo xtask progress check`.
