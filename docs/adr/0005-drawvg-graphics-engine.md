# ADR 0005: Use drawvg for in-stream graphics

## Status

Accepted

## Context

ErsatzTV uses an external graphics-renderer pipeline for scripted graphics.
FFmpeg 8.1 introduces the `drawvg` filter, which can render vector graphics
directly inside the filter graph.

## Decision

Use FFmpeg 8.1 `drawvg` with SVG templates in `assets/` as the NasTv graphics
engine. Do not reintroduce an external graphics process.

## Consequences

Graphics become part of the FFmpeg pipeline, making command generation and
snapshot testing central. FFmpeg 8.1 becomes a hard runtime requirement.

## Alternatives Considered

- Recreate ErsatzTV's external renderer: rejected because `drawvg` is the main
  technical reason for the rewrite.
- Render overlays in application code: rejected because it increases process
  complexity and data movement.

## References

- [Deliberate deviations](../explanation/deviations.md)
