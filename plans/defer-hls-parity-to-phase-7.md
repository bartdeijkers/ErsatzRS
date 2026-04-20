# Defer full HLS parity to Phase 7

## Context

The original master plan placed HLS video endpoints inside Phase 5 alongside
the IPTV playlist, XMLTV guide, HDHomeRun HTTP endpoints, and UDP discovery.
In implementation, ErsatzRS completed the Phase 5 MPEG-TS and HDHomeRun
surface first: `/iptv/channels.m3u`, `/iptv/xmltv.xml`, HDHomeRun discovery
and lineup routes, UDP discovery on port 65001, and MPEG-TS channel streaming.

Full HLS parity is materially larger than the rest of that protocol surface.
It includes variant playlist generation, segment production and caching,
discontinuity handling across playout boundaries, HLS-direct passthrough,
tokenized access, and client-specific compatibility details such as Kodi
`#KODIPROP` headers.

Treating those remaining HLS items as unfinished Phase 5 work leaves the
project plan and status docs out of sync with the actual staged delivery.

## Decision

Keep the completed MPEG-TS playlist/XMLTV/HDHomeRun surface in Phase 5.

Move full HLS parity into Phase 7. Phase 7 now starts with HLS completion,
covering:

- multi-variant playlists,
- segmenter worker with work-ahead,
- discontinuity tracking,
- hls-direct passthrough,
- access tokens,
- Kodi `#KODIPROP` headers.

This is a planning and sequencing change, not a product-scope reduction. HLS
remains required for ErsatzTV parity; it is intentionally scheduled later.

## Consequences

Phase 5 can be considered complete once the MPEG-TS IPTV playlist, XMLTV, and
HDHomeRun protocol surface are in place.

Phase 7 is no longer pure hardening; it begins with deferred protocol parity
work before continuing with additional hardware paths, MySQL support, Trakt,
multi-arch delivery, benchmarking, and soak testing.

Project documents that describe phase boundaries must reflect this sequencing so
readiness reviews do not report a false Phase 5 regression.

## Alternatives considered

- Keep HLS in Phase 5 and mark Phase 5 incomplete: rejected because the repo
  status and implementation already use a staged delivery where the core
  MPEG-TS/HDHomeRun surface shipped first and HLS remains a distinct body of
  work.
- Drop HLS parity: rejected because HLS remains part of ErsatzTV feature parity
  and is still required by the master plan.

## References

- [sequential-dancing-valiant.md](sequential-dancing-valiant.md)
- [../STATUS.md](../STATUS.md)