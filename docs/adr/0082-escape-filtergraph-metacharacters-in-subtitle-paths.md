# ADR 0082: Escape filtergraph metacharacters in subtitle paths

## Status

Accepted (2026-08-29)

## Implementation

Implemented — `escape_subtitles_filter_common`
(`crates/ersatzrs-ffmpeg/src/filter/simple.rs`) now escapes `'`, `,` and `;` in
addition to the existing `[` and `]`. Both `subtitles_filter` and
`subtitle_graphics_lavfi_input` route through it, so the subtitle path and the
`fontsdir` argument are covered. Two focused regressions in
`crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs` pin the apostrophe and the
comma/semicolon cases.

## Context

A 2026-08-29 production channel died at an episode boundary with:

```
[AVFilterGraph] No option name near '/Users/Bart/AppData/Local/ErsatzRS/cache/streams/fonts[v];[1:0]scale=154:-1,format=yuva420p|...'
Error : Invalid argument
```

The emitted filterchain was:

```
...,subtitles=D\\:/Downloads/TV/South Park (1997)/Season 13/South Park (1997) - S13E09 - Butters' Bottom Bitch (1080p BluRay x265 RCVR).dut.srt:fontsdir=C\\:/Users/Bart/AppData/Local/ErsatzRS/cache/streams/fonts[v];...
```

The apostrophe in `Butters'` opened an ffmpeg quoted region. Because the quote
never closes, every subsequent filter separator (`,`), chain separator (`;`) and
pad label (`[`, `]`) is consumed as literal text, collapsing the remainder of the
graph into a single option blob. That also stripped the `\\:` drive-colon escape
from the following `fontsdir`, so the option parser split at `C:`, exhausted the
`subtitles` filter's shorthand option slots, and reported `No option name near`.

The failure is item-specific and fires at the start of a playout item.
`rebase_text_subtitle_with_format` (`crates/ersatzrs-infra/src/subtitles.rs`)
returns the original sidecar path when `seek_seconds <= 0`; a fresh item starts
at seek 0, so the raw filename reaches the graph. Mid-item resumes substitute an
MD5-named cache file and therefore never reproduced it.

Filter values cross two ffmpeg tokenizers — the filtergraph parser, then the
filter option parser — which is the same reason the existing Windows drive colon
is written `\\:`. Probing FFmpeg 9 for the minimal working escape per character
gives:

| char | escape | crosses |
|---|---|---|
| `\` | `\\\\` | both |
| `'` | `\\\'` | both |
| `:` | `\\:` | option parser |
| `,` `;` | `\,` `\;` | filtergraph parser |
| `[` `]` | `\[` `\]` | filtergraph parser |

## Decision

Escape `'`, `,` and `;` alongside `[` and `]` when interpolating a path into the
`subtitles` filter, using the escape depth each character actually needs.

This is a deliberate divergence from ErsatzTV.
`$ERSATZTV_SRC/ErsatzTV.FFmpeg/Filter/SubtitlesFilter.cs` escapes only `[`, `]`
and the Windows drive colon, so upstream carries the same defect; ErsatzRS
inherited it through a faithful port. Per PORTING.md the deviation is recorded
here so a future parity pass does not "correct" the escaping back to match the
C# source.

Backslashes are not separately escaped because `escape_subtitles_filter_path`
already normalizes `\` to `/` before this stage.

## Consequences

- Sidecar subtitles whose filenames contain apostrophes, commas or semicolons
  now play instead of killing the channel at the item boundary. Apostrophes are
  common in episode and film titles.
- The escape is applied at the single point where a filesystem path enters a
  filtergraph; `subtitles` is the only such filter in the codebase.
- If ErsatzTV later fixes this upstream, the two implementations converge and
  this ADR can be downgraded to a note.
