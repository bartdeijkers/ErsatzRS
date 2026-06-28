# Human parity gap report

Generated: 2026-06-28T14:11:23+02:00

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
| Pending hydration-proof rows | 16 | Coverage/proof TODOs in `hydration.toml`; these are not the current feature parity blocker. |

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

The daily prompt file and phase notes still identify useful follow-up work, but
those items are not returned by `cargo xtask progress gaps` today:

- ADR 0013 in-app TSDuck MPEG-TS troubleshooting.
- Startup configuration cleanup around direct `DATABASE_URL` environment access.
- The first behavior-preserving module-split survey slice.
- Pending hydration proof rows for selected admin UI pages.

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
