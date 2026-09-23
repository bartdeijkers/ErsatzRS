# ADR 0099: Keep converted Next subtitles on HLS when wrapping MPEG-TS

## Status

Accepted 2026-09-23

## Implementation

Partial — the media-playlist wrapper, localized help and focused/runtime/browser
proof are complete. UCU-02 window gates remain open in the linked parity debt.

## Context

UCU-02 row 30 (`d8955134`) enables Next MPEG-TS Hybrid. The existing wrapper
copies all streams from an HLS master playlist. With Next Burn this works and
preserves selected audio and burned text. With Convert, that master also
advertises a live WebVTT rendition. FFmpeg 9 then buffers without emitting
MPEG-TS, making the channel unwatchable through its configured output.

The failure is reproduced with generated Dutch audio and text through the real
web host, and independently with the same three video/subtitle segment pairs.
A live video media playlist produces MPEG-TS immediately; the corresponding
master with WebVTT times out with zero output bytes. Adding an end marker lets
the master finish, but copies no subtitle packets. The exact upstream master
shape, which omits the subtitle group association, also stalls. Breaking HLS
subtitle association therefore neither fixes this failure nor preserves HLS.

No applicable correction exists in the reviewed legacy history through
`ee038ba4` or the official Next source at `946aa528`. The previously accepted
RGB-opacity exception does not authorize ignoring this playback failure.
UCU-02 owns the complete evidence and required acceptance.

## Decision

For Next MPEG-TS Hybrid only, feed the existing copy wrapper the authenticated
session media playlist (`/iptv/session/{channel}/hls.m3u8`) after the normal
Next startup/readiness gate. Do not feed the wrapper the HLS master containing
WebVTT. Retain token propagation, capacity, process ownership and cleanup.
Native wrappers keep their existing input and stream-copy behavior.

Keep Convert subtitles available in the normal HLS master and its WebVTT
rendition, including previews. Do not change stored subtitle preferences or
silently switch engine or subtitle mode. MPEG-TS carries the selected video and
audio; converted WebVTT remains HLS-only. Burn continues to provide visible text
in MPEG-TS. Add localized editor help making this distinction explicit, so an
operator needing subtitles through MPEG-TS can select Burn.

This deliberately differs from upstream's master-playlist wrapper input to
restore playable Hybrid output. It does not claim portable WebVTT in MPEG-TS,
add a second conversion pipeline, or patch the official alpha Next engine.

## Consequences

- Next Hybrid remains playable when Convert is configured, with selected audio.
- HLS retains selectable converted text; MPEG-TS users need Burn for subtitles.
- Existing native playback and official Next artifacts remain unchanged.
- Required proof: authenticated URL routing, real Burn/Convert MPEG-TS decoding,
  positive Dutch HLS WebVTT cues, preview and disconnect/idle lifecycle, localized
  JS/no-JS editor persistence in Chromium/Firefox, and the UCU window gates.
- Do not advance row 30 or mark the linked debt resolved before the maintainer
  accepts this decision and the implementation passes its checks.

## Alternatives Considered

- Keep the master input: demonstrated zero-byte live playback failure.
- Remove the HLS subtitle association: reproduces the failure and harms clients.
- Silently force Burn: overrides the configured mode and loses selectable HLS text.
- A separate Burn engine/session for MPEG-TS clients: adds process capacity and
  lifecycle policy beyond this compatibility correction; not selected here.
- Wait for an upstream fix: none was found; requires an explicit deferral decision.

## References

- [Optional Next engine](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- [Catch-up source map and procedure](../../plans/upstream-catch-up.md)
- [Canonical milestone evidence](../progress/milestones.toml)
- [Parity debt](../progress/parity/upstream-baseline-catch-up.toml)
