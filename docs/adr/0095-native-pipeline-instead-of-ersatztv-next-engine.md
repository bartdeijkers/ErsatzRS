# ADR 0095: Add ErsatzTV Next as an optional second streaming engine

## Status

Accepted 2026-09-22

The maintainer rejected the native-only proposal and selected Next as an
optional second engine. The existing filename is retained for stable links;
its original native-only wording is superseded by this decision.

## Implementation

Partial — release and Docker packaging stage the upstream artifacts with
checksum, license and provenance (UCU-01 row 4). Engine selection, runtime
supervision and playback acceptance remain open.

## Context

The upstream catch-up backlog introduces ErsatzTV Next, a separately
packaged `ersatztv-channel` process fed channel configuration and playout JSON.
ErsatzRS already owns scheduling, FFmpeg command generation and process
supervision. The maintainer requires Next alongside that native pipeline,
rather than excluding its integration or replacing the existing engine.

Subtitle, alternate-audio, graphics and playback changes inside Next commits
require individual behavioral review. Existing native capabilities do not
prove that the optional Next path provides those capabilities.

## Decision

Add Next as an optional second streaming engine. Preserve the native engine
and existing channels' behavior; installing or upgrading must not silently
switch them to Next. Port upstream engine selection and its persisted contract
through the ordered catch-up ledger, with an explicit operator choice to use
Next.

Next integration is in scope: package/runtime discovery, child-process
supervision, channel configuration and playout JSON exchange, engine/version
visibility, health checks and troubleshooting. Reconcile each upstream change
against the selected engine and record concrete implementation and test evidence.
Do not classify Next-specific plumbing or user interfaces as N/A merely
because the native engine already has an alternative implementation.

Preserve subtitle and alternate-audio selection, media-server and remote inputs,
watermarks/graphics, scheduling continuity and output behavior for the selected
engine. Missing Next behavior remains visible parity debt, even where the native
path works. No silent fallback to another engine may conceal a failed operator
selection.

Keep application use cases and scheduling independent of concrete process
adapters, following ADR 0051. Infrastructure owns external-process/runtime
integration; the web host composes it through the existing application/core
boundaries. Derive the exact exchange and lifecycle contracts from upstream
before implementation rather than introducing a second independent scheduler.

### Existing architecture contracts

The native engine retains direct FFmpeg invocation (ADR 0003), in-process
FFmpeg drawvg/drawtext graphics (ADRs 0005 and 0077), and its FFmpeg 9 contract
(ADR 0063). This decision adds a documented external streaming-engine boundary
for Next; it does not reintroduce an external graphics renderer into the native
pipeline or authorize downgrading its runtime.

SQLite (ADR 0080), the application boundary (ADR 0051), and all six supported
native platforms remain required. Inspect Next's actual runtime, licensing,
packaging and FFmpeg compatibility before claiming those contracts supported.
Any necessary change to an existing runtime requirement beyond this optional
engine boundary needs an explicit recorded decision; an unavailable artifact
or unsupported platform is an open gap, not an implicit platform exemption.

## Consequences

ErsatzRS must operate and verify two selectable streaming paths. Packaging,
configuration migration, process cancellation/restart, health, troubleshooting
and browser controls must accurately distinguish the selected engine.
The native pipeline remains available while Next is implemented; that does
not satisfy acceptance of the optional engine.

The catch-up milestones retain their per-row review and window acceptance
requirements. Prove configuration roundtrips, retained native defaults,
operator selection, actual Next process execution and served playback, including
subtitle/audio handling and recovery. Reuse existing verification tools where
they cover these requirements; record any missing evidence in the owning parity
record. Acceptance of this ADR is not implementation or release evidence.

## Alternatives considered

- Native pipeline only, adapting Next behavior: rejected by the maintainer.
  It would omit the requested second engine and its operator-facing integration.
- Next replacing the native pipeline: not selected; the maintainer requested
  an optional second engine, preserving the native path.
- Next alongside the native pipeline: selected. It carries additional runtime
  and acceptance work that must remain explicit in catch-up progress.

## Decision history

- Initial proposal: retain the native pipeline and exclude Next integration,
  while adapting relevant user-visible behavior per upstream commit.
- 2026-09-22: maintainer rejected that proposal, requested revision, and then
  directed: add Next as an optional second engine. No native-only exclusions
  were accepted or used to land an upstream row.

## References

- [Catch-up ledger](../../plans/upstream-catch-up.md)
- [Catch-up progress](../progress/parity/upstream-baseline-catch-up.toml)
- [Architecture](../../ARCHITECTURE.md)
- [ADR 0003](0003-ffmpeg-cli-spawn.md)
- [ADR 0005](0005-drawvg-graphics-engine.md)
