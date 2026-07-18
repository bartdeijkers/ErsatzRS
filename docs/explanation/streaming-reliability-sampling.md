# Avoiding the inspection paradox in streaming-reliability testing

Background and tradeoffs for how we sample streams when we test playback
reliability with TSDuck. This is an *explanation* page: it argues for a
sampling discipline, it does not replace the operational guide in
[`scripts/e2e/README.md`](../../scripts/e2e/README.md) or the layered view in
[test-strategy.md](test-strategy.md).

## The trap

The [inspection paradox](https://brooker.co.za/blog/2026/06/19/waiting.html)
(length-biased sampling, the "why is my bus wait longer than half the
headway" effect) says: **when you sample by picking a moment in time, you land
inside long intervals far more often than short ones.** A randomly chosen
instant is more likely to fall in a long gap than a short gap, so the intervals
you *observe* are biased longer than the intervals that actually *exist*. The
observed mean is inflated by the variance: the expected residual is
`E[X²] / (2·E[X])`, not `E[X]/2`.

Our TSDuck playback gate captures a fixed window of stream
(`CAPTURE_SECONDS`, default 20 s) and asks "did this window pass?". That is a
random-time sample, and it inherits the paradox directly. The result is a test
that is systematically blind to the failures users actually notice.

## How it bites a TSDuck gate

Streaming reliability failures come in two shapes, and a fixed window mishandles
both:

1. **Rare, *brief* events — undersampled.** A single continuity-counter
   discontinuity at a concat seam, a one-frame PTS/PCR leap on segment
   rollover, a momentary stall when the segmenter rotates. These occupy a tiny
   slice of wall-clock time, so a randomly placed 20 s window is unlikely to
   overlap one. These short events are exactly the stutters, skips, and
   A/V-sync breaks the gate exists to catch — and they are the ones a time-point
   sample hides.

2. **Rare, *long* events — oversampled.** A 10 s stall, a stuck segmenter, a
   wedged encoder. These fill a lot of time, so a random window is
   disproportionately likely to land inside one. They dominate any single
   capture and make the steady state look worse — or, when absent, make a single
   "pass" feel more reassuring than it is.

The net effect: **a single fixed-window capture over-weights long-lived
behavior and under-weights short-lived events.** "Did this window pass?"
conflates *the stream is healthy* with *this sample happened to miss the rare
event*. That is the wrong question.

A second, opposite trap is just as easy to fall into: running **many short
captures with gaps between them**. The gaps are unobserved time — any failure
that occurs there is invisible. You have traded length bias for blind intervals.

## The principle: sample events, not instants

The paradox is a property of sampling by *time point*. It disappears when you
sample by *event*, inspecting each structurally interesting moment exactly once,
independent of how long the surrounding interval is.

TSDuck already gives us the event-based primitive. A single **gapless,
continuous** pull counts each discontinuity exactly once regardless of its
duration:

```bash
# Event-based, paradox-immune: every packet inspected once over a long horizon.
tsp -I http 'http://127.0.0.1:8409/iptv/channel/1.ts' \
    -P continuity -P pcrverify -O drop
```

Compared to "capture 20 s and hope," this counts CC errors, PTS/DTS/PCR leaps,
and TEI/sync losses per unit of *streamed content*, not per arbitrary window.

## Concrete rules for our gate

These build on the existing knobs in
[`scripts/e2e/tsduck-playback-gate.sh`](../../scripts/e2e/tsduck-playback-gate.sh)
rather than replacing them.

1. **Report a rate, not a verdict.** Prefer *errors per hour of streamed
   content* (or mean-time-between-failures) over a per-window pass/fail. A rate
   is comparable across runs and is not biased by where the window landed. Keep
   the hard `MAX_CC_ERRORS=0` / `MAX_*_LEAPS=0` gates, but compute them over the
   whole continuous capture, not a snapshot.

2. **Make the horizon a function of the target failure rate, not a constant.**
   To detect a failure that occurs at rate λ per hour with ~95 % confidence you
   need roughly `3/λ` hours of *continuous* observation (≈ `-ln(0.05)/λ`). A
   1/hour fault needs ~3 hours of gapless capture — **not** 540 × 20 s windows
   with gaps between them, which leave most of the timeline unobserved. Pick
   `CAPTURE_SECONDS` from the rare event you are hunting, and capture
   continuously across it.

3. **Capture gaplessly; never stitch short windows.** One long `tsp` pull beats
   N short `tsanalyze` captures. If you must batch, the batches must abut with no
   dead time, or you reintroduce blind intervals.

4. **Anchor captures on events that matter.** Concat seams, HLS segment
   rollovers, and channel cold-starts are the failure-dense moments. Triggering
   a check on *every seam / every rollover / every start* converts time-point
   sampling into event-point sampling — each interesting transition inspected
   once, unbiased by interval length. The gate's `--item-seconds` flag is
   already this instinct: short playout items make a 20 s capture cross several
   concat seams instead of sitting inside one long item. Keep items short on
   purpose; a long item is a length-biased trap that hides its own seams.

5. **Treat startup and steady-state as separate regimes.** Cold start
   (time-to-first-segment, initial PAT/PMT, first keyframe) is short-lived per
   session but failure-dense. A single long capture is dominated by steady-state
   and undersamples startup. Characterize startup with **many independent
   session starts** — each its own sample — not one long stream. Steady-state and
   startup deserve separate metrics; averaging them together hides both.

6. **When you can only afford a window, place it deliberately.** A random window
   is the biased one. A window *triggered* by the seam or rollover you care about
   is not. If wall-clock budget forces a short capture, spend it on a known
   event boundary, and say so in the result.

## Rule of thumb

If a sampling method can answer "how often does this fail per hour of
streaming?" it is paradox-resistant. If it can only answer "did this 20 seconds
look OK?", it is biased — long faults inflate it, short faults escape it, and a
single green run means less than it appears.
