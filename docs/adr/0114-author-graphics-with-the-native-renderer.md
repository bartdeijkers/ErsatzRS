# ADR 0114: Author graphics with the native renderer

## Status

Accepted (2026-10-02)

## Implementation

Implemented — milestone `v111-findings-02` owns complete local native, browser,
transport and bundle acceptance. Findings 09/09b own their separate animation
and Motion placement/typography acceptance; both are locally accepted.

## Context

The authorized channel setup findings require the editor to expose the existing
Image, Text, Subtitle, Motion and Script model. ErsatzTV defines these properties
in YAML under `Core/Graphics/*GraphicsElement.cs`; its loader and refresh handler
read those files. ErsatzRS already persists the complete domain and has a native
canvas for every graphics kind, while its editor only creates Text.

## Decision

Use one native HTML form for hydrated and no-JS authoring. Extend the existing
application graphics port for YAML, preflight and an unsaved PNG canvas preview.
Reuse the native template context, playback overlay renderer and canvas command;
never alter a channel, attachment, timeline or live producer to preview a draft.
Channel/timestamp selection supplies actual historical programme context. Explicit
preview overrides augment the selected occurrence's saved variables without
changing them. The returned local timestamp includes the seek offset.

Validate geometry, bounds, discriminator/config agreement and required fields
before storage. Resolve templates and detect missing files/variables using the
native context. Use the existing strict YAML parser; export its flat format and
round-trip all shared domain fields, rejecting unknown fields and duplicate aliases.
Import is an explicit Save action. Ordinary omitted fields retain persisted values.

Resolve ASS contents with that same strict programme-variable parser before
passing the temporary resolved source to native libass. This corrects the
authoring preflight/runtime mismatch for spaced and nested placeholders.
An inactive timed element previews as a transparent canvas, retaining its timing
semantics rather than reporting an error merely because no overlay is active.

Explain capabilities instead of implying equivalent text renderers. Drawvg uses
built-in bitmap glyphs; configured real fonts do not affect it. Drawtext uses the
exact font (or the disclosed packaged default) at fixed size; its width/wrap/scale
properties do not constrain text. Preview long and non-ASCII text with the actual
font. The separately accepted findings 09 and 09b retain their full animation,
Motion alpha/timing/location and typography acceptance.

Finding 09b extends the existing absolute Text placement rule to Image/Motion.
When both X and Y are provided, they are output pixels and override the anchor,
percentage margins and source-content padding. Otherwise existing anchored
placement remains. This deliberately extends ErsatzTV Motion's anchor-only
placement through the already canonical shared fields; no new storage or renderer
is introduced. Preserve scale, decoded alpha and programme-relative timing.
The current VAAPI overlay path emits no position options, so positioned graphics
use the existing software composition and hardware upload path.

Keep previews finite: maximum geometry 8192 pixels per axis, aggregate authored
input 256 KiB, bounded output and a ten-second native process deadline. Reap a
failed/timed-out child, stop text controllers and remove temporary artifacts.
In-process Lua rendering has a 64 KiB source limit, 16 MiB heap and
one-million-instruction ceiling
across coroutine execution as well. Exclude Lua process, I/O, package and file
loaders to enforce ADR 0005's in-process drawing contract. The existing runtime
capability/health report also requires PNG encoding for preview output. No
external renderer or updater is introduced.

## Consequences

Existing canonical graphics and text-only compatibility callers remain readable.
All kinds become editable without direct database registration. Unsupported
shared geometry remains round-trippable and is explicitly described as unapplied
for ASS/Lua, whose native source owns their positioning and timing. Image/Motion
retain native anchors/margins unless both exact X/Y values are supplied.
Milestone `v111-findings-09b` owns pixel placement, alpha/timing/YAML and
independent real-font heading/title proof. Existing streams retain their startup
definitions until ordinary producer replacement.

## Alternatives Considered

- Another graphics renderer: rejected because it could differ from on-air output.
- Raw database edits or an external updater: rejected because authoring belongs
  in the product and must share its validation and serialization.

## References

- [Native graphics decision](0005-drawvg-graphics-engine.md)
- [Programme context](0112-programme-aware-graphics-context.md)
- Milestone `v111-findings-02` in [structured progress](../progress/README.md).
