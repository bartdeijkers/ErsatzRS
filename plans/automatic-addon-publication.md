# Automatic add-on publication implementation

Accepted implementation plan for AMM-P-01. Decisions are owned by
[ADR 0094](../docs/adr/0094-automatically-publish-addon-list-updates.md);
acceptance and evidence are owned by `amm-p-01` in the structured progress index.

1. Migrate saved policies and pending revisions; extend the existing durable
   publication worker to all saved changes and bounded enrichment batches.
2. Preserve overrides and source-removed identities, local artifacts and
   replacements; distinguish source status from effective playability.
3. Complete yt-dlp metadata mapping, provider-neutral parent derivation,
   NFO transfer and episode ContentRating persistence.
4. Replace review/publication controls with automatic progress and localized
   technical failure/recovery guidance, including no-JS behavior.
5. Verify metadata, publication, migration, concurrency, restart and playback
   regressions on Linux, then package a Windows-only beta for maintainer testing.

Use isolated fixtures before the live-profile acceptance scenario. The active
checkout is watched by a development server; do not expose partial migrations
or code through it. Windows acceptance stays open until the maintainer reports
the manual beta results. No unresolved product decisions remain.

## Hydration source map

- Route/page: `/media/addon-lists/{id}`, the add-on list workspace.
- SSR wrapper: `AddonListWorkspacePage`; tabs use native forms.
- Island: `PublicationOverviewIsland` in `workspace/publication.rs`.
- Data boundary: saved workspace/readiness DTOs; unpublished browser edits remain local.
- Server boundary: workspace actions and add-on list readiness server functions.
- Fallback: native settings/save forms and explicit failure-recovery actions.
- Shared primitives: existing save bar, buttons and localized text keys.
- Browser proof: save metadata and observe background publication without a publish
  button, with hydrated polling and native navigation; no hydration errors.
