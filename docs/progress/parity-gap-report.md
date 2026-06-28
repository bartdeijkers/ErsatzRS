# Human parity gap report

Generated: 2026-06-28T21:47:18+02:00

This is a human-readable snapshot. The source of truth remains the structured
progress files in this directory and the `cargo xtask progress ...` queries.

## Headline

ErsatzRS has **zero tracked parity gaps left** in `cargo xtask progress gaps`.

That means the feature-complete ErsatzTV parity tracker currently has no
features marked `In progress`, `Not started`, `Deferred`, or `Dropped`, and no
unresolved `[[debt]]` items in the parity files.

## Counts

| Area | Count | Human meaning |
| --- | ---: | --- |
| Tracked parity features | 37 | Feature records under `docs/progress/parity/`. |
| Ported parity features | 37 | Features currently marked `Ported`. |
| In-progress parity features | 0 | Features currently returned by `cargo xtask progress gaps`. |
| Open parity debt items | 0 | `[[debt]]` records with `resolved = false`. |
| Advertised-but-404 admin routes | 0 | `cargo xtask progress routes --unrouted` returned no rows. |
| Pending hydration-proof rows | 0 | `hydration.toml` currently has no non-green rows. |

## Remaining tracked parity gaps

None.

## Recently closed tracked parity gap

### Schedules / Templates / Decorations / Deco templates

- Slug: `schedules-templates-decorations-deco-templates`
- Status: `Ported`
- Closed debt: `schedule-items-fallback-vector-form-deserialization`
- Discovered: 2026-06-27 during MTV dogfood setup
- Resolved: 2026-06-28

The hydrated/generated schedule-items save path can persist a populated
schedule. The raw HTML fallback endpoint at `/api/schedules/items/save` now
parses the raw URL-encoded body before reconstructing schedule item rows, so it
accepts repeated `item_index=0` style fields instead of failing before the
fallback persistence code runs.

The regression proof covers repeated scalar row fields, checkbox index fields,
percent/plus decoding, filler IDs, watermark IDs, graphics element IDs,
language selector fields, playback fields, and add-item fallback fields.

Human impact: no-JS fallback saves, stale-hydration fallback submits, and direct
HTML form submissions preserve multiple schedule rows instead of rejecting or
dropping the schedule.

Closeout proof:

- `ScheduleItemsSaveForm::from_urlencoded_bytes` accepts repeated row fields.
- Focused regression tests cover repeated scalar row fields and preserved
  schedule item data.
- `cargo xtask progress check` passes.

## Adjacent work not counted as this parity gap

The phase notes still identify broad verification and release-hardening work,
but those items are not returned by `cargo xtask progress gaps` today:

- Full closeout verification for the newly implemented ADR 0013 TSDuck
  troubleshooting tool.
- Full closeout verification for the startup config cleanup and infra health
  module split.
- Follow-up module-split work for the media-browser and YAML scheduling files.

Those are hardening, spec, or proof chores. The current ErsatzTV parity tracker
has no remaining feature/debt gaps.

## Commands used

```bash
cargo xtask progress gaps
cargo xtask progress next
cargo xtask progress parity --feature schedules-templates-decorations-deco-templates --full
cargo xtask progress routes --unrouted
cargo xtask progress hydration --pending
```
