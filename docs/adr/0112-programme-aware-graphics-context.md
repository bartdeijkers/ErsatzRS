# ADR 0112: Programme-aware graphics context and metadata refresh

## Status

Accepted — maintainer-authorized finding 04, 2026-10-02, including October 3–4 addenda.

## Implementation

Implemented and locally accepted under finding 04 on 2026-10-05. Shared occurrence
projection, explicit native render time, owned opt-in metadata refresh and
attachment propagation have repository, cancellation and fresh-process Continue
proof. Historical editor previews and decoded HLS/regular-IPTV programme
transitions pass in both browsers, with unchanged stored timelines. The owning
milestone records canonical evidence, final gates and platform limits.

## Context

Raw graphics `Epg` contains filler and individual film segments. Fixed indexes
cannot identify a full programme or a film continuing after a bumper. Drawtext
refreshes its clock using a startup snapshot; FFmpeg text-file reload does not
reload database variables. Native source seek includes the media InPoint, which
is distinct from the elapsed broadcast occurrence time.

Upstream `RefreshChannelDataHandler.WritePlayoutXml` groups Classic, Sequential
and Scripted rows, including pre-roll and trailing filler. `WriteBlockPlayoutXml`
applies the configured Block guide-window behavior. Upstream graphics read that
guide; ErsatzRS currently applies only the Block rule to XMLTV and exposes raw
playout rows to graphics.

## Decision

Share a read-only programme occurrence projection between XMLTV and native
graphics. Preserve channel/playout identity, actual programme segments and guide
boundaries. Apply the source scheduler's grouping rule; identify continuation
from the same media occurrence and source InPoint/OutPoint continuity, rather
than matching titles. Retain existing raw `Epg` variables for template compatibility.

Expose optional `CurrentProgramme`, `NextProgramme`, `ContinuingProgramme` and
bounded `LaterToday` values. Use the explicit broadcast render instant, including
historical previews, and the host civil timezone already used for channel
scheduling. Resolve the next local date independently across DST. Missing or
ambiguous metadata must not produce invented titles, times or a guessed next
programme. Provide bounded defaults using the existing template dependencies.

Carry explicit graphics overrides through schedule-to-playout materialization.
Make database metadata refresh an explicit opt-in for native Drawtext, separate
from clock refresh and FFmpeg file reload. Reuse the producer-owned controller,
bounded channel queries, cancellation, deadlines and last-good-file behavior;
do not introduce detached maintenance or write to the live timeline.

The editor preview must use the same context builder and expose effective refresh
semantics. Acceptance includes database refresh/override propagation, local/DST
and filler/split/boundary/historical cases, and a decoded served transition.

## Consequences

Classic guide grouping now matches its upstream branch. This corrects the
demonstrated projection gap; it does not diagnose or repair the separately
reported exported-guide overlap or any production timeline. Existing templates
and startup-snapshot behavior remain available. Query failure retains the last
good rendered file without claiming that metadata is current.

## Alternatives Considered

- Fixed raw Epg indexes: filler and split films invalidate the selected label.
- External metadata polling: bypasses the native occurrence and producer lifecycle.
- Treating every file reload as database refresh: obscures behavior and creates
  unbounded query load.

## References

- [Finding 04 acceptance](../progress/milestones.toml)
- [Graphics parity debt](../progress/parity/drawvg-graphics-engine.toml)
- [XMLTV projection](../../crates/ersatzrs-core/src/xmltv.rs)
- [Graphics variables](../../crates/ersatzrs-infra/src/graphics_template.rs)
- [Owned Drawtext controller](../../crates/ersatzrs-infra/src/dynamic_text.rs)
