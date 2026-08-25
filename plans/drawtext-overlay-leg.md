# FFmpeg drawtext overlay leg plan

## Status

In progress after the Add-on metadata management (AMM) program and before
Phase 9A teletext, per
[`docs/progress/phase.toml`](../docs/progress/phase.toml). It starts with the
blocking `dtx-a-01` decision that accepted ADR 0077 and delivered the typed
filter/pipeline leg. `dtx-a-02` is the remaining controller, persistence, UI,
health, and runtime-proof slice. The authoritative execution state lives in
[`docs/progress/milestones.toml`](../docs/progress/milestones.toml)
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

- `dtx-a-01` added `DrawtextFilterBuilder` and the ordered drawtext pipeline
  overlay, including typed inline/file text and family/file font choices,
  interval reload, FFmpeg 9 layout/styling controls, two-layer escaping, and
  software-frame hardware bridging.
- Drawtext is now a required runtime capability. First-party package gates
  require drawvg/drawtext plus Fontconfig, FreeType, HarfBuzz, and FriBidi and
  execute a file-backed render before publication.
- The graphics-element system (`GraphicsElementKind::Text`) has no backend
  selector; adding `drawtext` needs a "dynamic / file-backed" text mode and the
  controller plumbing to write and refresh the reloaded text file.

## Scope

- `dtx-a-01` — complete: accepted ADR 0077 and landed the
  `ersatzrs-ffmpeg` typed `drawtext` filter builder + pipeline leg (`font` or
  `fontfile`, inline `text` or `textfile` with a positive reload interval,
  layout/styling, expansion, escaping, snapshots, and package enforcement).
- `dtx-a-02` — wire the `drawtext` backend into the app: a dynamic/file-backed
  text graphics mode, controller textfile write/refresh, admin UI backend
  choice, and cross-platform (Linux/WSL/Windows) tests.

## Boundaries

- Persistence, controller, admin UI, and live refresh behavior belong to
  `dtx-a-02`; they are not hidden inside the low-level filter builder.
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
