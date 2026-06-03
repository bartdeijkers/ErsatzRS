# ADR 0005: Use drawvg for in-stream graphics

## Status

Accepted

## Context

ErsatzTV uses an external graphics-renderer pipeline for scripted graphics.
FFmpeg 8.1 introduces the `drawvg` filter, which can render vector graphics
directly inside the filter graph.

ErsatzTV stores the canonical graphics element shape in YAML files referenced
from `GraphicsElement.Path`. Its database row keeps only identity and kind
metadata, while render-time loaders deserialize kind-specific YAML into image,
text, subtitle, motion, and script element classes.

ErsatzRS originally added `0002_drawvg_graphics_engine.sql` as a narrow
drawvg-oriented stopgap (`X`, `Y`, `Width`, `Height`, `EnableExpression`,
`Text`, plus `ZIndex`). That shape could not represent ErsatzTV's image,
subtitle, motion, script, styling, opacity, location, scaling, EPG, or
per-kind configuration behavior.

## Decision

Use FFmpeg 8.1 `drawvg` with VGS draw-script templates in `assets/` as the
ErsatzRS graphics engine. Source SVG files may exist as authoring/reference
fixtures, but FFmpeg `drawvg` receives generated VGS scripts, not raw SVG/XML.
Do not reintroduce an external graphics process.

Graphics element storage follows Decision C from the graphics parity slice:
ErsatzRS treats the database as canonical, and an ErsatzTV-style YAML file at
`GraphicsElement.Path` is parsed to upsert a fully populated DB element. Shared
fields are typed columns; kind-specific extras live in a JSON `Config` column.
`Config = NULL` is interpreted as the default config for the row kind.

Migration `0003_graphics_element_fields.sql` retires the half-built `0002`
geometry/text columns (`X`, `Y`, `Width`, `Height`, `EnableExpression`, `Text`)
and keeps `ZIndex` as the shared stacking column. This makes the schema match
the render model rather than preserving the earlier drawvg-only approximation.

Rendering is split by element kind:

| Kind | Renderer |
| --- | --- |
| `Image` | Native FFmpeg image input plus `overlay` |
| `Text` | Generated VGS draw script plus `drawvg` |
| `Subtitle` | Generated `.ass` artifact plus libass/subtitles overlay input |
| `Motion` | Native FFmpeg video input plus `overlay`, with loop/hold/disappear behavior |
| `Script` | In-process Lua that returns a VGS draw script plus `drawvg` |

All rendered elements are composed by ascending `ZIndex`. Script elements import
the ErsatzTV `command`/`args` fields for fidelity, but they must not spawn an
external graphics process.

## Consequences

Graphics become part of the FFmpeg pipeline, making command generation and
snapshot testing central. FFmpeg 8.1 becomes a hard runtime requirement.

The live streaming paths must carry generated graphics artifacts and native
graphics inputs through the shared FFmpeg pipeline for IPTV MPEG-TS, HLS
segmenter output, and the dev MPEG-TS stream. Render-time templating remains
late-bound so media, EPG, time, and per-item variables can be resolved for the
active playout item.

The project now owns a durable SVG-to-VGS distinction: SVG assets can be kept as
source/reference material, but checked-in templates that feed `drawvg` must be
VGS draw scripts.

## Alternatives Considered

- Recreate ErsatzTV's external renderer: rejected because `drawvg` is the main
  technical reason for the rewrite.
- Render overlays in application code: rejected because it increases process
  complexity and data movement.
- Keep the `0002` geometry/text columns as the graphics engine schema: rejected
  because they cannot represent ErsatzTV's per-kind graphics model.

## References

- [Deliberate deviations](../explanation/deviations.md)
- [FFmpeg drawvg reference](https://ffmpeg.org/drawvg-reference.html)
