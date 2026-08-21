# FFmpeg drawtext overlay leg plan

## Status

Queued and not implemented. Slots after the Add-on metadata management (AMM)
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
that `drawvg`'s static VGS cannot update mid-stream, while exposing the
dedicated filter's broader font selection, sizing, alignment/spacing,
border/shadow/box, alpha, and expression controls. `drawvg` remains the default
for arbitrary vector composition; `drawtext` is the text-specialized dynamic,
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
  font size and layout, position/colour/alpha, border/shadow/box styling,
  expression expansion, escaping, and filtergraph snapshots).
- `dtx-a-02` — wire the `drawtext` backend into the app: a dynamic/file-backed
  text graphics mode, controller textfile write/refresh, admin UI backend
  choice, and cross-platform (Linux/WSL/Windows) tests.

## Non-goals before the ADR is accepted

- No schema, admin UI, or pipeline wiring before `dtx-a-01` accepts ADR 0077.
- `drawtext` does not replace `drawvg`; static, vector-rich text stays on
  `drawvg`. This leg is additive.

## Verification

- Unit and filtergraph snapshots cover inline and file-backed text, expression
  escaping, and Windows drive-letter paths without weakening existing drawvg
  or libass snapshots.
- Decoded playback proves that an atomically replaced text file changes a live
  overlay without rebuilding the FFmpeg graph and that restart/failure behavior
  remains bounded.
- The localized editor and serialized contract preserve existing text elements
  as drawvg by default and pass focused hydration/browser coverage.
- Linux, WSL, and Windows exercise packaged font resolution and the same
  controller behavior before `dtx-a-02` closes.
