# ADR 0105: Render Next graphics canvases with FFmpeg

## Status

Accepted 2026-09-29

## Implementation

Partial — the canvas layer/schema, strict timing/header validation and finite
FFV1/BGRA command builder are implemented. Packaged-runtime decoding proves
transparent backgrounds, image alpha, layer ordering, content-box placement and
duration. The managed producer now owns graphics files and live text controllers
through startup, transfer and confirmed process cleanup. Paced rendering admits
its first output without waiting for a media-probe prefix or EOF. Packaged FFmpeg
proof covers this supervised drawvg stream. Expiring source bindings now
authenticate the channel, occurrence, version/file and timeline while excluding
mutable graphics settings. Host-side lookup now validates the active session and
current persisted, mirror, fallback or preview occurrence, and retains the
session start clock. Transient bindings explicitly include their owning session.
Canvas timing separates the scheduled programme finish from bounded chunk
duration and selects current decorations at the requested programme offset.
The private callback now authenticates requests and prepares streams through
the shared renderer and session transfer owner, with a fresh occurrence check
before returning the reader. Decimal and rational rates retain their wire
spelling in the renderer. Canvas filter clocks preserve programme offsets while
output timestamps start at zero; motion and subtitle inputs resume at their
requested source phase, and timed fades use the programme clock. Canvas inputs
have explicit instance identities so shared assets retain independent seeks.
Local-file troubleshooting previews now use the same session admission and
cleanup as Remote Stream previews, enabling session-bound canvas callbacks.
Canvas projections now carry stable occurrence metadata separately from issued
credentials. Publication and demand-time paths issue the official canvas layer
with non-consuming FFV1/BGRA/NUT probe hints. Dynamic-item start rebasing is signed
into the binding and restored before selecting or rendering the chunk; selected
video indexes determine source-content geometry. Complete integrated graphics
proof, capability health coverage and decoded Next playback acceptance remain pending.
Focused tests include private admission rejection and public route isolation;
packaged FFmpeg decoding proves selected Lua/drawvg output and transparent
frames after selection removal through the session-owned renderer.

## Context

Upstream row146 (`28b2e00c`, graphics canvas) replaces Next's limited image
layers with a full-frame transparent graphics stream. Next asks its HTTP source
for an item offset, duration and frame rate, then composites the returned canvas
over the programme. This enables text, scripts, motion, subtitle graphics and
expressions that the existing ErsatzRS Next image projection cannot represent.

Upstream runs its graphics engine and feeds raw BGRA frames to an FFmpeg encoder.
ADR 0005 instead requires ErsatzRS graphics to use FFmpeg filters and in-process
Lua, and forbids restoring a separate graphics-rendering application. ADR 0095
requires the optional Next engine to preserve graphics behavior. The official
Next canvas protocol is the integration boundary; native playback continues to
render graphics in its existing programme filter graph.

## Decision

Use a bounded FFmpeg canvas producer for Next, using the managed, pinned
ErsatzRS-ffmpeg runtime and the existing shared graphics selectors, templates,
drawvg/libass/image/motion filters and in-process Lua. Its base input is a
transparent frame at the requested output resolution and rate. It emits finite
BGRA FFV1 video in NUT, matching the official Next canvas input contract.

This is a narrowly scoped clarification of ADR 0005: a supervised FFmpeg child
may generate the Next canvas. No Skia renderer, external script command, new
rendering application, daemon or public listener is introduced. Native playback
continues to compose graphics within its current FFmpeg process.

The existing private loopback listener serves the canvas. Reuse the host's
capability model with an expiry and exact channel, session and occurrence
binding. Validate the current item or preview binding before rendering; reject
cross-channel, expired and stale capabilities. Credentials and private source
paths must not enter published playout JSON or diagnostics.

The staged official Next revision `4cc93e4222fe334ed6aad2f694e5b3bcb611f2c9`
preserves configured HTTP source headers when it adds canvas request context.
Use that existing authenticated-source mechanism; a public or unauthenticated
canvas URL is unnecessary. This differs from libass subtitle fetching, where
the renderer does not forward the Next source headers.

Preserve the official `x-etv-offset-ms`, `x-etv-duration-ms`,
`x-etv-frame-rate` and optional `x-etv-channel` protocol headers. They are Next
wire fields, distinct from the machine API's `X-Ers-Api-Key`. Reject duplicate,
negative, non-finite, zero-rate and overflowing values before starting work.

Keep programme start/finish, chunk duration, seek and session start as distinct
values. Mirror offsets, crop/stretch/content-box geometry, selection precedence,
stable graphics ordering, opacity, dynamic text and preview bindings share native
implementations. A valid active binding whose graphics were removed returns
transparent frames for the requested chunk. A late offset follows upstream's
finite transparent/rendered chunk behavior rather than returning an empty body.

Each producer owns its FFmpeg process, generated artifacts and dynamic-text
tasks until completion. Reuse existing bounded admission, cancellation and
process supervision; client disconnect, session stop and startup failure must
stop and reap the producer and remove its artifacts. Do not buffer whole
programmes, detach cleanup responsibilities or introduce an unbounded cache.

Publish the official canvas kind and probe hints so probing cannot consume the
graphics stream. Reuse the already staged official Next and published FFmpeg
artifacts. Existing direct media-layer optimization can remain only where it
preserves the selected graphics behavior; row173 owns its later upstream audit.

## Consequences

Next gains the shared graphics feature set while FFmpeg remains the rendering
engine. Canvas generation adds a bounded FFmpeg child and local encoded-video
traffic to affected Next chunks. CPU and cancellation behavior need runtime
proof; this decision does not claim hardware or cross-platform acceptance.

Row146 remains pending until implementation and acceptance evidence are complete.
Required evidence includes protocol/header and binding rejection tests, exact
argv snapshots, decoded alpha/position/order/timing for all graphics kinds,
seek/chunk/mirror/preview coverage, transparent output after graphics removal,
and disconnect/stop cleanup. Exercise the actual official Next process as well
as the canvas endpoint. Native graphics, subtitles and alternate audio must
retain their existing runtime behavior.

## Alternatives considered

- Keep only current image layers: cannot represent the new upstream behavior.
- Restore upstream's separate graphics renderer: conflicts with ADR 0005 and
  duplicates ErsatzRS's existing filter-based renderer.
- Patch Next to accept arbitrary host filter graphs: introduces a maintained
  runtime fork where the official canvas protocol already supplies the boundary.
- Pre-render complete programme canvases: adds latency and storage and makes
  dynamic templates, edits and cancellation harder to preserve.

## References

- [ADR 0005](0005-drawvg-graphics-engine.md)
- [ADR 0095](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- [ADR 0102](0102-isolate-http-listener-route-surfaces.md)
- [Catch-up source map](../../plans/upstream-catch-up.md)
- Upstream `GetGraphicsCanvasStreamHandler`, `PlayoutItemConverter`,
  `GraphicsEngineContextFactory` and `InternalController` at `28b2e00c`.
