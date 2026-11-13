# ADR 0134: Preserve owned finite programme continuations

## Status

Proposed

## Implementation

Partial — candidate implementation in PR #155; 17 focused continuity and
ownership tests pass. Two actual joined trials select the next row at the
completed predecessor's finish and reach every original 8 MiB capture cap.
A broader completion-receipt candidate now addresses the independently reproduced
ordinary legacy delivery gap. Its 38 focused completion, command-seal, admission and ownership tests and
strict FFmpeg/web Clippy pass. Matched candidate b7b6eb20 passes playback smoke
and all 93 unchanged manual four-route checks; legacy delivery is 102percent
against the required 95percent. Actual completion reports match forwarded bytes,
and the selected rows advance 1/2/3 without repeating the expiring tail.
The current joined caption trial reaches all five original capture caps and
advances at the predecessor finish without a delivery-deadline failure. Its four
non-boundary windows pass 80 strict checks. Owner-bound expiry/PMT qualification
now proves lawful withdrawal/reannouncement and confirmed coalescing without
changing the expiry fence. The joined cue, refresh and page-isolation assertions
pass; that checkpoint retains the original red boundary subtitle qualification
(19/21). After the maintainer's2026-11-13 source-criterion approval, the fresh
headed joined case passes all five original captures and102 strict checks,
including native-effective cue timing within8ms with raw flags retained.
Local part6 proof is complete; required exact-head ready-PR CI/merge and the
separate native Windows part7 prerequisite remain. The existing
`teletext-admin-recovery-flow` debt owns acceptance and its finite remaining list.

## Context

ADR 0121 gives each native legacy wrapper an exclusive item permit and separate
encoder admission. Internal requests previously selected an item at wall time.
An early request can therefore select the preceding item again. Preparation
then consumes most of that tiny remaining interval before the producer opens.
The next real programme needs another preparation, and final output fails its
unchanged delivery deadline.

The retained ordinary-bitrate trial demonstrates this path across an actual
scheduled finish. Complete-source I/O batching reduces measured preparation
time, and sharing the wrapper's pacer avoids a second input clock, but those
changes alone do not remove the repeated tail.

At the accepted upstream baseline `c41969b3`, the HLS worker supplies its
produced-until timeline to the next item
query and uses the selected item's start when appropriate. The native owned
legacy protocol has no exact C# counterpart. Apply that continuity principle
within its existing ownership fence, without borrowing a clock from an HTTP
parameter or a different wrapper.

## Decision

- Record a server-selected predecessor only for an owned, finite ordinary native
  producer whose requested nominal interval reaches its scheduled finish. Exclude
  retained, remote-stream, external-JSON and song producers. A shortened interval
  grants no continuation clock.
- Use FFmpeg9's existing progress capability on stderr, separate from media
  stdout. The generated command must carry the exact sealed stderr-progress step;
  opaque commands, stdout progress and duplicate progress options do not qualify.
- Publish the predecessor only after successful body forwarding and actual child
  cleanup. Require the last complete terminal report, consistent microsecond and
  display timestamps, an exact match between reported size and forwarded bytes,
  and produced duration covering the nominal interval. Successful early EOF is
  not nominal completion. Missing/malformed reports and retries after prior media
  clear the clock; failures, cancellation and uncertain cleanup retain the fence.
- Use the final owned wrapper's pacer and bounded backpressure for these finite
  producers. Direct, unowned and retained input pacing remains unchanged.
- The next item may select at the recorded finish. Before doing so, query the
  fresh schedule at the predecessor's original selection time and require its
  exact row identity, start and finish. A removed, replaced or restamped row
  invalidates the cursor; ordinary wall-time selection then applies.
- Keep the cursor inside the existing exclusive wrapper/item ownership. Never
  persist it, expose a caller-selected time or share it between wrappers.
- Requalify the next source normally. The cursor carries no source bytes,
  decoder result, caption artifact or authority to bypass source validation.
- Preserve capacity, work/storage quotas, source discovery limits, absolute
  startup deadlines, output holding/lateness and transport checks. Ordinary
  unowned, retained and Next routes retain their existing clock selection.

## Consequences

A correctly admitted continuation cannot waste another source preparation on
the already completed tail. Fresh schedule validation preserves rebuild/Reset
safety without changing persisted scheduling state. The extra metadata lookup
uses the same absolute request deadline. Completion uses the existing bounded
stderr reader and actual byte accounting; there is no new worker, queue, cache,
provider request or persisted state. A missing report grants no continuation
clock and introduces no new optional runtime/dependency requirement. The mechanism grants neither remote
EOF completeness nor platform/client acceptance.

## Alternatives Considered

- Increase output lateness or holding: rejected; it hides the producer gap.
- Reduce, densify or retime the source fixture: rejected; it changes the required
  viewing scenario.
- Apply pacing changes alone: retained negative evidence shows the unresolved
  repeated-tail preparation.
- Trust an old row or a caller-supplied timestamp: rejected; schedule changes
  and foreign requests cannot inherit continuation authority.
- Add a global caption cache: unnecessary for this demonstrated sequencing gap.

## References

- [ADR 0121](0121-teletext-pages-transport-and-addon-contract.md)
- [ADR 0133](0133-qualify-native-bitmap-caption-clock-rescaling.md)
- [Teletext parity](../progress/parity/teletext-support.toml)
- `crates/ersatzrs-web/src/transport/iptv/legacy_wrapper_admission.rs`
- `crates/ersatzrs-web/src/transport/iptv/stream.rs`
- `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/HlsSessionWorker.cs`
- `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/Queries/GetPlayoutItemProcessByChannelNumberHandler.cs`

- [FFmpeg progress reporting](https://ffmpeg.org/ffmpeg.html#toc-Main-options)
