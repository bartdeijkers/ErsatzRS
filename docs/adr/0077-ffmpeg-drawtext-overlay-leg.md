# ADR 0077: Add an FFmpeg drawtext overlay leg

## Status

Proposed (2026-08-21)

## Implementation

Not implemented — recorded so the pipeline in
[ADR 0076](0076-minijinja-music-video-credits-templates.md) describes a real
target, not just a conceptual branch. The `drawtext` filter appears nowhere in
the codebase today.

## Context

[ADR 0005](0005-drawvg-graphics-engine.md) made `drawvg` the in-stream graphics
engine, and ADR 0076's pipeline diagram shows a `Text files → FFmpeg drawtext`
leg beside the `VGS script → drawvg` one. That leg does not exist yet: text
graphics render as static VGS through `text_drawvg`
(`crates/ersatzrs-ffmpeg/src/graphics/mod.rs`), baked at generation time, and
`drawtext` occurs zero times across `crates/`.

Baking text into VGS is right for static labels, but `drawvg` cannot cheaply
change text mid-stream — any edit re-runs the render/pipeline. Live text (a
running clock, a countdown, a now/next line that changes while the same overlay
stays on screen) wants FFmpeg's `drawtext`, which reads text inline or from a
`textfile` and can `reload` it per frame, and offers native time expansion
(`%{pts}`, `%{localtime}`, `%{eif}`). ADR 0076's diagram already anticipates
this: the Rust controller writes the text files a `drawtext` leg consumes.

## Decision

Add a `drawtext` overlay leg to `ersatzrs-ffmpeg` and wire it through the app
(`ersatzrs`):

- In `ersatzrs-ffmpeg`, a filter builder (under `src/filter/`) emits
  `drawtext=…` with `fontfile`/`fontsdir`, `text=` or `textfile=`+`reload=1`,
  position, colour, optional box, and correct escaping of colons, Windows drive
  letters and special characters — mirroring the escaping care already in
  `subtitles_filter`. It plugs into the graphics/pipeline exactly where the
  `drawvg` leg does (`graphics/mod.rs`, `pipeline/mod.rs`, `filter/complex.rs`).
- In the app, the graphics-element system gains a way to select the `drawtext`
  backend for text — a "dynamic / file-backed" text mode — with the minijinja
  controller writing and refreshing the text file the filter reloads. `drawvg`
  stays the default for static, richly-styled text.

## Consequences

- New capability: live, file-backed text overlays that update without
  re-rendering the whole graphic.
- A second text path: the graphics config and admin UI must let authors choose
  `drawvg` (static, vector-rich) versus `drawtext` (dynamic, file-backed) and
  surface the trade-off — `drawtext` has no arbitrary shapes, gradients or path
  effects.
- More FFmpeg surface to test: filtergraph snapshots, escaping (the Windows
  drive-colon case as in `subtitles`), `fontfile`/`fontsdir` resolution, and
  reload timing.
- ADR 0076's diagram becomes literally true — `drawtext` is a real leg rather
  than a conceptual one.

## Alternatives Considered

- Keep everything on `drawvg` and regenerate the VGS on each change: rejected —
  it cannot update per frame without re-invoking the pipeline, wasteful for a
  ticking clock.
- Use libass/`.ass` for dynamic text: rejected — an `.ass` card is also fixed
  at generation time; `drawtext`'s `textfile`+`reload` is purpose-built for
  live updates.

## References

- [ADR 0005](0005-drawvg-graphics-engine.md) — the drawvg leg this one sits
  beside.
- [ADR 0076](0076-minijinja-music-video-credits-templates.md) — the pipeline
  diagram whose `drawtext` branch this ADR realizes.
- `crates/ersatzrs-ffmpeg/src/graphics/mod.rs`, `src/pipeline/mod.rs`,
  `src/filter/complex.rs` — the drawvg leg to mirror.
- `crates/ersatzrs-ffmpeg/src/filter/simple.rs` — `subtitles_filter`, the
  filter-path escaping precedent.
- FFmpeg `drawtext` filter — `text`/`textfile`/`reload` and `%{…}` expansion.
