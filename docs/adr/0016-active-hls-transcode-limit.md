# ADR 0016: HLS transcode capacity and admission control

## Status

Accepted (2026-06-17; revised — see Revision history).

## Context

ErsatzTV exposes `ffmpeg.segmenter.work_ahead_limit`, which limits how many HLS
segmenters may transcode *ahead* of realtime at maximum speed. That protects
against channels racing far ahead, but it is not a total cap on active FFmpeg
transcode processes. On smaller systems, many active transcodes (especially
after rapid channel switching) cause CPU oversubscription, fan noise, heat, and
stutter on the channel the viewer actually settles on.

The pinned ErsatzTV baseline has no `ConfigElement` for a total active transcode
limit, so adding one is an intentional ErsatzRS runtime-hardening decision
rather than a direct port.

The first cut of this ADR added `ffmpeg.segmenter.active_transcode_limit`
(default `2`) as a soft cap with a "priority bypass": a session that could not
reserve a slot would still run if it was recently accessed. Runtime review found
this does not deliver a real ceiling:

- The bypass (`force_reserve_active_transcode`) increments the counter **without
  any upper bound** whenever a session is "recently accessed" (idle `< 30 s`).
- Every playlist poll and every segment fetch calls `touch()`
  ([`crates/ersatzrs-web/src/transport/iptv/hls.rs`](../../crates/ersatzrs-web/src/transport/iptv/hls.rs)),
  so a channel with a live viewer is **permanently** "recently accessed." With K
  live viewers on K channels and limit N, the first N take real slots and the
  remaining K−N all force-reserve → K concurrent transcodes regardless of N.
- A single 30 s constant did double duty as both "is anyone watching?" and "how
  long a priority slot lives," coupling two unrelated decisions.
- So the cap held only for channels nobody was watching; for the real
  multi-viewer case it was effectively absent, and the (N+1)th viewer degraded
  **all** active streams instead of shedding one.

The underlying mistake was treating all transcode work as one fungible pool and
then poking an unbounded hole in it. The three conflated resources have very
different costs:

| Resource | Cost | Cap without hurting a viewer? |
| --- | --- | --- |
| A running transcode (FFmpeg) | Expensive (CPU/GPU, heat) | Only by refusing/stopping a stream |
| Segments retained on disk | Cheap (disk + cleanup timer) | Yes, freely |
| Work-ahead burst (racing past realtime) | Most expensive per stream | Yes, freely (realtime still serves) |

The target deployment that motivates the capacity number is **two stable
1080p60 streams (AC-3 6ch) over a 100 Mbps LAN** (see
[`docs/hls-segmenter-ffmpeg-profile-recommendations.md`](../hls-segmenter-ffmpeg-profile-recommendations.md)):
~19 Mbps muxed per stream, ~38 Mbps for two, well under the network ceiling,
which makes `N = 2` the correct default for that hardware class. The ceiling is
primarily CPU/GPU-bound; the network/segmenter profile is a complementary
concern handled by the FFmpeg profile, not by this ADR.

## Decision

`ffmpeg.segmenter.active_transcode_limit` is a **hard capacity ceiling plus
admission control**, built on the principle that *the watched set is sacred and
everything else is opportunistic and reclaimable.*

1. **Hard capacity ceiling.** `active_transcode_limit` is a true ceiling `N` on
   concurrent content transcodes — no path increments the counter past `N`. (HLS
   segmenter generations first; the MPEG-TS pipeline is folded into the same
   counter as a follow-up so the number means "this box's total transcode
   capacity.") It is independent of `work_ahead_limit`, which continues to
   control only maximum-speed work-ahead.

2. **Two distinct timers** replace the overloaded single constant:
   - **Watch window `W` (~30 s):** a session is "watched" if touched within `W`.
     Kept around the player's buffer depth so a deep-buffered but actively
     watching viewer is not misclassified as idle.
   - **Retention window `R` (= segmenter idle timeout, ~60 s):** segments stay on
     disk for `R` after the last touch; the session is torn down after `R`.
     Retention is decoupled from holding a transcode slot, so zap-back starts
     playback instantly from buffered segments even when the slot was reclaimed.

3. **Admission / preemption ladder.** When a watched session needs to (re)spawn
   FFmpeg and the pool is full, it reclaims capacity in cost order instead of
   force-reserving:
   1. Stop a **work-ahead** burst.
   2. Reclaim a **cooling** session's slot (`W ≤ idle < R`; it should not hold
      one anyway).
   3. Preempt the **stalest watched** session, but only if its idle is far
      enough toward `W` to be almost certainly an abandoned zap rather than a
      deep-buffered viewer.
   4. Otherwise **refuse the newcomer with a short "all streams in use" slate**
      and keep retrying. The slate is a trivial looping clip and does not consume
      a real transcode slot; the session flips to real content the moment a slot
      frees.

   The ladder never preempts an established active stream to admit a brand-new
   cold start — when genuinely full of real viewers, the newcomer waits on the
   slate. During zapping, the abandoned previous channel ages toward `W` and
   self-selects as the stalest preemption target, so a single zapper collapses to
   ~one active transcode while the channel they settle on stays responsive.

4. **Configuration mapped to the admin's mental model (tuners).** Expose at most
   two numbers plus a live readout; keep the ladder and timers as fixed internal
   behavior:
   - **"Maximum simultaneous transcodes"** — the ceiling `N`. Help text frames it
     as the tuner count: "the most channels your server will transcode at once;
     when all are busy a new viewer briefly sees an 'all streams in use' message;
     channels already playing are never interrupted; direct-play streams that
     need no transcoding do not count."
   - **"Work-ahead transcodes"** (advanced, `≤ N`) — the existing
     `ffmpeg.segmenter.work_ahead_limit`, reframed: how many slots may race ahead
     of live to build buffer; buffering always yields to a new viewer.
   - A **live "Streams in use: X / N (peak Y)"** readout on the FFmpeg settings
     and troubleshooting pages, so the abstract number is observable and the
     admin knows when to raise it or upgrade hardware.

5. **Hardware-aware suggested default.** Keep `N = 2` as the shipped default, but
   surface a suggested value next to the field from the existing capability probe
   (`ersatzrs-hwprobe` / `ersatzrs-*::health`): higher when NVENC/QSV/VAAPI is
   present (HW encode is nearly free), modest for software x264. The suggestion is
   shown and explained, not auto-applied.

## Consequences

- The configured limit is truthful: a real ceiling on concurrent transcodes, so
  small machines get genuine back-pressure and the oversubscription death-spiral
  (every stream stuttering when one too many joins) is replaced by a single
  graceful "all streams in use" slate for the marginal viewer.
- Zapping stays responsive and gets *cheaper*: instant zap-back from retained
  on-disk segments (cheap disk) without pinning a transcode slot (expensive CPU),
  and the abandoned channel during a zap is the natural preemption target.
- The admin reasons in tuners with a live usage readout instead of an opaque
  internal scheduling parameter, and the shipped default adapts (via a
  suggestion) to detected hardware.
- It remains process-local, matching the in-memory HLS session registry; it is
  not a cluster-wide scheduler.
- New surface area to build and test: the preemption ladder, the slate-on-full
  admission path, the live counter, and the eventual pipeline-agnostic counting.
  Preempt-stalest is a heuristic; its idle threshold must be tuned against real
  zap/multi-viewer captures and is the main behavioral risk to validate.

## Alternatives Considered

- **Soft cap with unbounded priority bypass (this ADR's first cut).** Rejected:
  it does not bound the actively-watched case at all, and the label/help/"hard
  back-pressure" claim misrepresented runtime behavior.
- **Single flat hard cap with no priority.** Rejected: a new viewer (or a
  zapper's own current channel) would be refused while an abandoned previous
  session still held a slot, making zapping feel broken.
- **Preempt the oldest session unconditionally on every admission.** Rejected:
  two genuine viewers would ping-pong a slot and both stutter; preemption must be
  conservative and prefer refusing a new cold start over breaking an established
  stream.
- **Couple slot lifetime to the watch window (the original shared 30 s).**
  Rejected: retention is cheap and wants a generous window for instant zap-back,
  while slots are expensive and want tight admission — they must be separate.
- **Reuse `work_ahead_limit` as the total cap.** Rejected: it conflates
  max-speed buffering with total active load and would make the existing
  ErsatzTV-compatible setting misleading.
- **Rely only on the idle timeout.** Rejected: idle timeout bounds lifetime, not
  concurrency; many channel switches inside one timeout window still create too
  many simultaneous transcodes.

## References

- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0014: Runtime-learned hardware capability fallback](0014-runtime-learned-hardware-capability-fallback.md)
- [ADR 0015: HLS / MPEG-TS continuity normalization](0015-hls-mpeg-ts-continuity-normalization.md)
- [Implementation plan](../../plans/hls-transcode-capacity-and-admission.md)
- [HLS segmenter FFmpeg profile recommendations](../hls-segmenter-ffmpeg-profile-recommendations.md)
- [`crates/ersatzrs-infra/src/hls_worker.rs`](../../crates/ersatzrs-infra/src/hls_worker.rs)
- [`docs/progress/parity/hls-variant-playlists-segmenter.toml`](../progress/parity/hls-variant-playlists-segmenter.toml)

## Revision history

- 2026-06-16: Original decision — `active_transcode_limit` (default 2) as a soft
  cap with a recently-accessed priority bypass and a 30 s grace.
- 2026-06-17: Revised after runtime review found the bypass unbounded (every live
  viewer bypasses, so the cap was not a real ceiling). Replaced with the hard
  capacity ceiling + admission/preemption ladder, decoupled watch/retention
  windows, slate-on-full, the tuner-model configuration, a live usage readout,
  and a hardware-aware suggested default.
