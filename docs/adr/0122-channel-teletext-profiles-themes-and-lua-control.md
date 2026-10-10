# ADR 0122: Channel teletext profiles, add-on themes and Lua control

## Status

Accepted (2026-10-10). The maintainer explicitly added channel-scoped add-on
bindings/profiles, add-on-owned themes/custom zones and shared admin/Lua control
to Phase 9. This supplements ADRs 0120/0121 without changing their delivery order.

## Implementation

Partially implemented: neutral profile/binding types and an additive SQLite
repository provide isolated, revision-fenced configuration. Shared application
controls and profile-scoped invocation prepare structurally validated unpublished
drafts through the native host adapter. Lua intervals, localized administration,
automatic service activation, encoding and publication remain open. P9A-04 owns
independent synthetic acceptance; P9B-01/02 own the sole production TV-guide
add-on and integrated proof.

## Context

One installed add-on must serve several channels with different settings and
presentation simultaneously. Operators need multiple add-on bindings on a
channel, each with its own enable state and selected named profile. Scripted
playouts need the same validated controls without package installation or
activation side effects.

Existing `AddonSetting` rows belong to an installation, keyed by add-on and
setting. Saving them disables that installation and changes its StateRevision.
They cannot serve as mutable per-channel theme state. At decision time no named
add-on profile abstraction existed. Independent ConfigRepository get/set calls
also cannot provide atomic profile/binding/revision updates.

Existing Scripted Lua evaluates candidate schedules before publication.
Watermark/graphics controls affect subsequently generated items. Publication
uses a single owned transaction for the candidate timeline and attachments.
Teletext must preserve this rollback and restart boundary, including mirrored
channels that share programme data but require independent presentation.

## Decision

### Profiles and channel isolation

Introduce a small named teletext profile referencing an existing installed
add-on and a profile-scoped settings bag. Reuse manifest setting definitions,
value validation and existing setting-reference handling. A profile resolves a
complete settings snapshot for its invocation; selecting or editing it never
rewrites global AddonSetting rows or implicitly installs/enables a package.
Any inherited installation setting belongs to the fenced snapshot and invalidates
dependent generations when its revision changes. No second package manager or
general settings framework is introduced.

Persist explicit `(channel, add-on)` bindings with enabled state and selected
profile identity. One channel can enable several distinct add-ons. Each binding
selects one profile; several channels can use the same add-on with different
profiles simultaneously. Profiles can be reused intentionally, but editing one
invalidates only its dependent bindings. Channel-specific variation uses distinct
profiles rather than silently mutating a shared profile.

Use a narrow transactional repository and existing application command pattern
for profile/binding updates and revision checks. Core exposes the necessary
control port; it does not depend on the application crate. Admin handlers and
an injected Lua adapter invoke the same application validation and state-change
functions. Callers cannot bypass channel/add-on ownership, profile/package
compatibility, installed-package enablement or current revision checks.
A channel binding currently Off can be turned On through those controls.

Profile persistence uses owned setting rows and a composite profile/add-on
reference from each channel binding. Deleting a channel removes its bindings;
deleting a profile through the application retains a tombstone and its bindings
so the unavailable selection remains explainable. Physical profile deletion is
restricted while referenced. Package uninstall retains this configuration and
does not gain an implicit foreign-key blocker. Turning an existing binding Off
requires its current revision, but does not require a usable package or profile.

Complete override bags are bounded to 256 settings, 128 UTF-8 bytes per key,
16 KiB per literal or unresolved reference, and 64 KiB in aggregate. Profile
names contain 1–256 UTF-8 bytes without control characters. These host resource
limits do not define themes or zone semantics. Missing overrides inherit the
captured installation settings and then manifest defaults; every effective value
is validated under the same complete-bag bounds. Secret values remain unresolved
references at this boundary.

Writes compare the installation's state revision, version, package/content hashes
and manifest identity within the same write transaction as profile/binding
revision checks. Editing or deleting a shared profile advances only its dependent
binding revisions. A changed installation identity makes existing configuration
stale until explicitly revalidated. Configuration validity is not package-byte,
process, page-generation or publication readiness.

Profile invocation captures the enabled output binding, selected profile,
installation identity and complete effective settings in one read transaction.
The resolver compares that snapshot before native work and immediately before
launch. Scoped settings enter the existing environment and executable-tool
resolution together, so a profile cannot replace an executable after its lease
identity has been selected. Application preparation checks the receipt's exact
snapshot and request generation, rechecks the selection after the producer
finishes, and validates the complete response before returning an unpublished
draft. A later publication transaction must check these identities again.

The request's guide channels are source data; they are not the output binding.
Several outputs may intentionally share identical multi-channel guide context.
Output identity remains host-side, including for mirrors sharing playout IDs.
Supplied generation tokens remain opaque correlation values. A snapshot and
echoed token do not replace trusted generation ownership or publication fences.

Missing, deleted, wrong-add-on, disabled, incompatible or invalid profiles have
typed visible outcomes. A rejected command changes no selection. If an existing
selection becomes invalid, report that binding unavailable and follow ADR 0121's
bounded publication policy while preserving ordinary playback and other
channels. Do not choose another profile, enable a package or invent defaults.

### Add-on presentation and custom zones

The single production TV-guide add-on declares named colour/layout themes.
A profile selects a theme and its permitted overrides. The add-on defines named
page zones, their location, accepted custom values and bounds; profiles can fill
those defined zones. Theme names, zone semantics and editorial composition stay
add-on-owned. ErsatzRS does not add TV-guide layouts or a theme-specific renderer.

Unknown themes/zones, invalid overrides and values exceeding a zone's supported
extent fail visibly. No silent truncation, page overwrite or hidden substitution
is allowed. The host validates the complete resulting typed page set, including
references, geometry, spacing controls and supported teletext encoding. Custom
zones do not permit arbitrary transport bytes, HTML or renderer commands.

Several active bindings must yield a valid bounded channel page map. Duplicate
page/subpage ownership or incompatible composition rejects the candidate;
never use silent last-writer-wins or renumber add-on pages. Preserve channel-wide
resource limits: at most 1,024 page/subpage records in the aggregate channel
generation, not 1,024 per active binding. Retain one in-flight invocation per
configured channel service; multiple bindings compose serially, with at most
one invocation for any `(channel, add-on)` binding. Channel admission and
invalidation coalescing stay bounded.
Stream and browser consume exactly the same immutable validated channel
generation and page content. Neither reconstructs a separate theme or zone view.

### Lua timing, lifetime and errors

Admin changes persist the channel's base enabled/profile selection. Lua exposes
profile selection and on/off controls for existing profiles and bindings on the
script's channel. Scripts do not expose package install/enable operations.

During schedule generation a Lua command produces a candidate override at the
validated scheduling cursor (`current_time()`), rather than mutating live
configuration. Each `(channel, add-on)` override applies on the half-open
published interval from that cursor until the next successful command for that
binding or the end of the candidate timeline, ending at its final accepted published item finish. Commands
at/after that finish, or candidates with no published interval, create no
override. At expiry, use the current admin
base. A later Continue evaluation starts from base unless its script selects an
override again; no implicit persistent Lua globals are added.

Persist accepted override intervals atomically with their scheduling result.
Refresh/Reset retain or replace them at the same preserved programme boundary.
Mirrors resolve by output channel identity, not only shared playout/item IDs;
source-channel Lua overrides are not inherited automatically.
Admin changes affect the base and intervals outside an active explicit Lua
override; they do not silently rewrite that override. Profile edits invalidate
every affected snapshot, including active overrides. Their original expiry
does not extend while a replacement is prepared.

An enable/profile transition requests a new complete channel generation. It
becomes visible at a safe carousel/output boundary after all validation and
atomic promotion. If it cannot be ready at the requested interval boundary,
show pending/unavailable and never keep presenting another profile as current.
Disable withdraws that channel's authored binding at the defined safe output
boundary, without interrupting ordinary audio/video playback. Report requested
and applied state so preparation delay is visible. ADR 0121's validity and
stale-expiry rules remain in force; expired intervals cannot publish late output.

Lua receives a typed rejection for missing/invalid selections and can handle it.
An uncaught compilation/runtime error rejects the whole candidate transaction;
no partial teletext command or timeline escapes. Previously published schedules
remain intact. Restart restores only current published intervals and exact
validated generations; it does not replay Lua for immediate effects or resurrect
expired overrides. Deleted profiles, changed revisions and unavailable packages
remain visible failures, with no automatic package activation.

### Acceptance and sequence

P9A-04 first proves these generic controls with synthetic fixture producers:
two simultaneous channels using one package with different profiles/presentation;
several add-ons on one channel; isolated toggles and profile edits; explicit
shared-profile invalidation; missing/wrong/deleted profiles; page collisions;
Lua cursor transitions, expiry, handled rejection and failed-script rollback;
restart inside/after an interval; Refresh/Reset boundaries and mirror isolation;
identical browser/stream generation; localized admin in every supported culture;
and continued ordinary playback on failures.

Only after independent engine acceptance does P9B-01 implement exactly one
production TV-guide add-on in ErsatzRS-Addons. Its sole NOS reference remains
TV-guide-related pages 200–299. It supplies named themes, overrides and defined
custom zones. P9B-02 repeats the accepted scenarios with the exact installed
production package, actual guide content and supported clients. Existing source
subtitle, alternate-audio, six-RID, native/Next, browser and TSDuck gates remain.

## Consequences

Profiles and timeline overrides require bounded, transactionally consistent
persisted relationships and generation identity. Existing installation, settings,
Lua and schedule-publication mechanisms remain their foundations. This decision
adds acceptance requirements; it does not establish implemented behavior or
close a milestone. Ordinary playback must remain independent of optional
authored teletext availability.

## Alternatives Considered

- Mutate global add-on settings per channel: breaks isolation and disables the
  shared installation.
- Let Lua immediately mutate live configuration while building: leaks partial
  effects from rejected schedules and makes restart behavior unpredictable.
- Render browser themes separately: permits divergence from validated DVB pages.
- Deliver production themes before engine acceptance: violates ADR 0120.

## References

- [ADR 0120: Engine/content order](0120-separate-teletext-engine-from-addon-content.md)
- [ADR 0121: Teletext transport and lifecycle](0121-teletext-pages-transport-and-addon-contract.md)
- [Phase 9 plan](../../plans/teletext-support.md)
- [Milestone acceptance](../progress/milestones.toml)
- [Teletext parity/debt](../progress/parity/teletext-support.toml)
- [Existing add-on settings](../../crates/ersatzrs-infra/src/addons.rs)
- [Scripted evaluation](../../crates/ersatzrs-core/src/scheduling/scripted/mod.rs)
- [Scripted resolver port](../../crates/ersatzrs-core/src/scheduling/scripted/resolver.rs)
- [Atomic schedule publication](../../crates/ersatzrs-infra/src/repositories/playout/publication.rs)
