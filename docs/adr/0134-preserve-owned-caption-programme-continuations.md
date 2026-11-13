# ADR 0134: Preserve owned caption programme continuations

## Status

Proposed

## Implementation

Partial — candidate implementation in PR #155; 17 focused continuity and
ownership tests pass. Two actual joined trials select the next row at the
completed predecessor's finish and reach every original 8 MiB capture cap.
Complete joined acceptance remains open: the guide expiry/PMT transition and
strict subtitle PTS/PCR gate still fail. Required final gates remain in progress. The existing
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

- Record a server-selected predecessor only for an owned, ordinary native
  source-caption producer whose qualified nominal interval reaches its scheduled
  finish. A shortened interval grants no continuation clock.
- Publish that predecessor only after successful body completion and actual
  producer cleanup. Source-caption rejection or fallback clears the pending
  clock. Failure, cancellation and uncertain cleanup retain the existing fence.
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
uses the same absolute request deadline. The mechanism grants neither remote
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
