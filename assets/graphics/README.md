# Graphics templates

These files are source templates for generated graphics artifacts.

- `lower-third.vgs` is a VGS draw script template for FFmpeg `drawvg`.
- `song-progress.vgs` is a VGS draw script template for progress-style overlays.
- `subtitle-template.ass` is a libass subtitle graphics template.
- `lower-third.svg` is retained as a source/reference fixture. Do not pass raw
  SVG/XML to FFmpeg `drawvg`; generated drawvg artifacts must be VGS scripts.

Template variables use ErsatzRS render-time `{{name}}` replacement. Common
variables include media fields such as `MediaItem_Title`, `MediaItem_Duration`,
EPG fields such as `Guide_Start`, `Guide_Stop`, and per-item variables from
schedule YAML/scripted graphics joins. VGS templates are rendered without XML
escaping because drawvg expects script tokens. ASS templates are also rendered
raw so libass override tags and dialogue text remain intact.
