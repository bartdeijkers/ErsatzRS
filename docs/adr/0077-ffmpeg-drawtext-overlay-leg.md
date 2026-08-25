# ADR 0077: Add an FFmpeg drawtext overlay leg

## Status

Accepted (2026-08-25)

## Implementation

Implemented — milestones `dtx-a-01` and `dtx-a-02` are complete. Text graphics
default to drawvg for backward compatibility and may opt into drawtext through
the localized graphics-element editor. The minijinja controller renders before
FFmpeg starts, atomically refreshes bounded UTF-8 text files, preserves the
last good output on refresh failure, and stops before working-file cleanup.

Focused builder/pipeline snapshots, controller and health tests, Chromium and
Firefox editor persistence/no-JavaScript coverage, and same-process decoded
reload gates with packaged FFmpeg 9 on Windows and WSL/Linux verify the complete
decision. ErsatzRS-ffmpeg commit `7cdef93` supplies the first-party drawtext and
text-shaping package gates.

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

The current `text_drawvg` path also exposes a narrower text-specific control
surface than FFmpeg's dedicated filter. The additive leg should make
FFmpeg-native font selection and sizing, text/y alignment, line spacing and tab
size, shaping, border/shadow/box styling, alpha, and expression-driven values
available without pretending that `drawtext` replaces SVG for arbitrary vector
composition. FFmpeg 9 does not expose a `fontsdir`, letter-spacing, weight,
italic, or wrapping option on this filter; the contract must not invent them.

## Decision

Add a `drawtext` overlay leg to `ersatzrs-ffmpeg` and wire it through the app in
two bounded milestones:

- `dtx-a-01` adds a typed filter builder under `src/filter/`. Text input is an
  enum, so inline `text=` and file-backed `textfile=` are mutually exclusive.
  Font resolution is likewise explicit: either a Fontconfig `font=` family
  when the runtime has a configured font database, or an exact `fontfile=`
  path. Cross-platform managed graphics use packaged font files rather than
  assuming host family resolution. File-backed text may set `reload` to a positive
  frame interval; inline text cannot. Expansion is the supported `none` or
  `normal` mode (`strftime` is deprecated). The builder covers font size,
  `text_align`, `y_align`, `line_spacing`, `tabsize`, x/y expressions,
  foreground colour/alpha, and optional border/shadow/box styling.
- Filter values pass through both required escaping layers: the drawtext option
  parser and the surrounding FFmpeg filtergraph. File paths normalize
  separators and escape filter separators including a Windows drive colon.
  Inline text and expressions preserve FFmpeg `%{...}` expansion while safely
  escaping literal special characters.
- Drawtext joins the same ascending-z graphics overlay plan as drawvg and
  native image overlays. Because drawtext is a software-frame filter, hardware
  pipelines download before the first software graphics leg and upload again
  before a hardware encoder; existing drawvg, libass, and native-overlay order
  remains unchanged.
- `drawtext` becomes a required FFmpeg 9 runtime filter. First-party packages
  must prove the filter plus `fontconfig`, `libfreetype`, `libharfbuzz`, and
  `libfribidi`; shaping is part of the supported contract rather than an
  optional host accident.
- `dtx-a-02` adds a defaulted renderer selector to text graphics. Existing
  elements remain drawvg. Selecting drawtext opts into a minijinja controller
  that atomically writes an initial UTF-8 text file before FFmpeg starts and
  refreshes it while the same process remains alive. The localized UI and
  controller lifecycle/failure contract are owned by that milestone, not by
  the filter builder.

## Consequences

- New capability: live, file-backed text overlays that update without
  re-rendering the whole graphic.
- A second text path: the graphics config and admin UI must let authors choose
  `drawvg` (static, vector-rich) versus `drawtext` (dynamic, file-backed) and
  surface the trade-off — `drawtext` exposes dedicated font/text controls but
  has no arbitrary shapes, gradients or path effects.
- More FFmpeg surface to test: filtergraph snapshots, two-layer escaping (the
  Windows drive-colon case as in `subtitles`), family/file font resolution,
  hardware download/upload placement, reload intervals, and shaping package
  dependencies.
- ADR 0076's diagram becomes literally true — `drawtext` is a real leg rather
  than a conceptual one.

## Alternatives Considered

- Keep everything on `drawvg` and regenerate the VGS on each change: rejected —
  it cannot update per frame without re-invoking the pipeline, wasteful for a
  ticking clock.
- Use libass/`.ass` for dynamic text: rejected — an `.ass` card is also fixed
  at generation time; `drawtext`'s `textfile`+`reload` is purpose-built for
  live updates.
- Expose every syntactically possible drawtext option as free-form text:
  rejected — mutually exclusive input/font choices, positive reload intervals,
  and typed expansion/alignment values keep invalid combinations out of the
  generated graph while preserving expressions where FFmpeg defines them.

## References

- [ADR 0005](0005-drawvg-graphics-engine.md) — the drawvg leg this one sits
  beside.
- [ADR 0076](0076-minijinja-music-video-credits-templates.md) — the pipeline
  diagram whose `drawtext` branch this ADR realizes.
- `crates/ersatzrs-ffmpeg/src/graphics/mod.rs`, `src/pipeline/mod.rs`,
  `src/filter/complex.rs` — the drawvg leg to mirror.
- `crates/ersatzrs-ffmpeg/src/filter/simple.rs` — `subtitles_filter`, the
  filter-path escaping precedent.
- FFmpeg 9 `drawtext` filter — `font`/`fontfile`,
  `text`/`textfile`/interval `reload`, shaping, and `%{…}` expansion.
