# ADR 0108: Preview television folder discovery

## Status

Accepted (2026-10-02, maintainer findings solution 03)

## Implementation

Implemented (v111-findings-03, 2026-10-02). The owning milestone records local
scanner, native FFmpeg, API and browser acceptance. Integrated release closeout
remains separate.

## Context

Television discovery accepted only six extensions, silently omitting MPEG
episodes. An extra package directory above seasons was also silently ignored.
Operators need to understand discovery before saving a library without moving
files, publishing metadata or invoking decoders.

## Decision

Use the same discovery traversal and classification for television scans and a
read-only folder preview. Preserve root/show/season discovery and the existing
separation of extras. Do not promote arbitrary nested package folders to shows
or seasons. Report unsupported nesting, ignored extensions, empty shows,
unreadable or repeated directories and discovered episode/extra counts.

Restore all upstream video candidates, including MPEG program streams,
MPEG transport streams, Ogg video and AviSynth inputs, plus common equivalent
container suffixes.
An extension identifies a candidate; the existing ffprobe validation still
decides whether it contains usable media. Optional runtime support such as
AviSynth remains the responsibility of that existing FFmpeg/ffprobe runtime;
an unavailable decoder follows the normal probe-failure path. No script engine
or new decoder is introduced by discovery.

Bound preview work by entry count, depth, elapsed time and concurrent workers;
cap diagnostic samples and report truncation explicitly. Filesystem calls may
stall, so a timed-out blocking worker retains its concurrency slot until it
actually exits. Do not represent incomplete discovery as a complete inventory.
Preview makes no database or source-file changes. Preserve the submitted form
and offer the same preview without JavaScript. Normal scanning retains its
complete traversal and established identity, metadata and subtitle processing.
Revision the television season discovery cache once so existing MPEG files
cannot remain hidden behind an unchanged pre-upgrade folder etag. Subsequent
unchanged scans retain the fast path. Ignore hidden discovery entries and
detect repeated canonical directories to stop link cycles without duplicating
episodes.

## Consequences

Operators can see why a layout would be skipped before saving. One classifier
keeps preview and scanner behavior aligned. Preview cannot prove successful
decoding or predict filesystem changes after inspection. Existing scan errors
and health reporting remain applicable to actual media processing.

## Alternatives Considered

- Recursively treating every nested video as an episode risks classifying
  extras as programmes and duplicating seasons.
- A separate preview inventory would drift from actual discovery.
- Probing every file before saving would turn preview into a costly scan.

## References

- [Milestone v111-findings-03](../progress/milestones.toml)
- [Scanner parity](../progress/parity/scanner.toml)
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/LocalFolderScanner.cs`
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/TelevisionFolderScanner.cs`
