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

`MediaItem_SeekSeconds` is the numeric source position at rendering or preview
join, including the item's InPoint. `MediaItem_StreamSeek.TotalSeconds` retains the duration-shaped
version of the same value. A troubleshooting media seek also moves
`MediaItem_Start` backwards, while `MediaItem_Stop` remains the bounded preview
finish; graphics therefore keep their programme anchor after a seek.

## Programme context

Native graphics and XMLTV share scheduler-specific guide grouping. The following
values use the explicit broadcast instant, independently of source seek:

| Variable | Meaning |
| --- | --- |
| `CurrentProgramme` | The complete guide occurrence at the render instant, including its pre/post-roll and split content. |
| `NextProgramme` | The first subsequent complete guide occurrence. |
| `ContinuingProgramme` | The current film during an inserted junction before a proven continuation of the same media and source position. Pre/post-roll is excluded. |
| `LaterToday` | At most 32 future occurrences before the next local midnight. |
| `Programme_RenderTime` | The broadcast instant in UTC, including historical previews. |
| `Programme_LocalTime` | The same instant in the host scheduling timezone, with its UTC offset. |
| `Programme_ContextAmbiguous` | Whether overlapping occurrences prevent an unambiguous selection. |

Programme objects contain `Title`, `SubTitle`, `Description`, UTC `Start`/`Stop`,
offset-bearing `LocalStart`/`LocalStop`, `PlayoutId` and `FirstItemId`. Missing
metadata, invalid bounds or unavailable roles yield null rather than invented
labels. Local-day boundaries use the host civil timezone already used for
scheduling; the next date is resolved separately across DST. A per-channel
timezone setting is not introduced.

Use bounded scalar defaults to suppress unavailable labels:

```jinja
{{ CurrentProgramme.Title | default("", true) }}
{{ NextProgramme.Title | default("", true) }}
{{ ContinuingProgramme.Title | default("", true) }}
{{ LaterToday[0].LocalStart | default("", true) }}
```

Drawvg accepts this scalar default syntax alongside its existing path and numeric
expressions. Drawtext retains its existing template engine. Raw `Epg` remains a
list of individual playout rows, including filler, anchored at the source item's
start; its indexing does not become programme-aware. Explicit per-item variables
are applied last. Classic and Block item editors preserve these overrides through
copying, materialization, Continue and restart; existing published items retain
their stored values until an authorized rebuild changes them.

The text editor can resolve draft text for a selected channel and an ISO 8601
instant with a UTC offset. Historical previews read the stored occurrence and
its overrides without changing the timeline. The no-JavaScript preview resolves
saved text. These are text/context previews; font metrics and placement require
the actual renderer.

## Drawtext metadata refresh

Metadata refresh defaults to a startup snapshot (`0`). An explicit interval of
5–60 seconds rereads programme metadata and the running occurrence's database
overrides. The producer owns one bounded read at a time, with a two-second
deadline and cancellation on stop. Failed reads, invalid variables or a changed
occurrence retain the last good rendered file. Nonpersisted attachments retain
their startup overrides.

This setting is separate from Drawtext's clock refresh and FFmpeg's text-file
reload interval. Drawvg uses its startup snapshot. Saving a refresh setting does
not restart a running producer.
