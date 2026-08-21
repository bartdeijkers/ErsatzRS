# FFmpeg drawtext overlay leg plan

## Status

Active and not implemented. Slots after the Add-on metadata management (AMM)
program and before Phase 9A teletext, per
[`docs/progress/phase.toml`](../docs/progress/phase.toml). It starts with the
blocking `dtx-a-01` decision that accepts ADR 0077. The authoritative execution
state lives in [`docs/progress/milestones.toml`](../docs/progress/milestones.toml)
and [`docs/progress/parity/drawtext-overlay-leg.toml`](../docs/progress/parity/drawtext-overlay-leg.toml);
this plan owns future sequencing and acceptance boundaries.

## Goal

Add an FFmpeg `drawtext` overlay leg alongside the existing `drawvg` graphics
engine, so ErsatzRS can render live, file-backed text — a running clock, a
countdown, a now/next line that changes while the overlay stays on screen —
that `drawvg`'s static VGS cannot update mid-stream. `drawvg` remains the
default for static, richly-styled text; `drawtext` is the dynamic,
`textfile`+`reload` path the minijinja Rust controller writes into. See
[ADR 0077](../docs/adr/0077-ffmpeg-drawtext-overlay-leg.md) and the pipeline in
[ADR 0076](../docs/adr/0076-minijinja-music-video-credits-templates.md).

## Current evidence

- `drawtext` occurs zero times across `crates/`. Text graphics render as static
  VGS through `text_drawvg` in `crates/ersatzrs-ffmpeg/src/graphics/mod.rs`,
  baked at generation time; there is no reload/live-text path.
- `crates/ersatzrs-ffmpeg/src/filter/simple.rs::subtitles_filter` is the
  precedent for filter-path escaping (colons, Windows drive letters) that a
  `drawtext` builder must mirror for `fontfile`/`textfile` paths.
- The graphics-element system (`GraphicsElementKind::Text`) has no backend
  selector; adding `drawtext` needs a "dynamic / file-backed" text mode and the
  controller plumbing to write and refresh the reloaded text file.

## Scope

- `dtx-a-01` — accept ADR 0077 and land the `ersatzrs-ffmpeg` `drawtext` filter
  builder + pipeline leg (fontfile/fontsdir, `text=`/`textfile=`+`reload=1`,
  position/colour/box, escaping, filtergraph snapshots).
- `dtx-a-02` — wire the `drawtext` backend into the app: a dynamic/file-backed
  text graphics mode, controller textfile write/refresh, admin UI backend
  choice, and cross-platform (Linux/WSL/Windows) tests.

## Non-goals before the ADR is accepted

- No schema, admin UI, or pipeline wiring before `dtx-a-01` accepts ADR 0077.
- `drawtext` does not replace `drawvg`; static, vector-rich text stays on
  `drawvg`. This leg is additive.
