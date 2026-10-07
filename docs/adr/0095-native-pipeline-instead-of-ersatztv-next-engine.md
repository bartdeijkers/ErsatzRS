# ADR 0095: Integrate ErsatzTV Next with automatic runtime availability

## Status

Accepted 2026-09-22

The maintainer rejected the native-only proposal and selected Next as an
optional second engine. The existing filename is retained for stable links;
its original native-only wording is superseded by this decision.

Amended and accepted 2026-10-07: Next may become the default for new channels
and standalone playback previews only when ErsatzRS always bundles the accepted
engine or downloads and verifies it automatically. Existing channel selections
and the explicit native option remain unchanged. This conditional decision
supersedes the earlier new-channel native default; acceptance of the decision
does not prove its implementation or runtime availability.

Amended and accepted 2026-10-07: distribute the exact accepted Next packages
through the public, package-only `bartdeijkers/ErsatzRS-next` repository. This
destination allows automatic acquisition without GitHub credentials while the
application repository retains its visibility. Preserve every accepted source,
patch, version and package hash when changing the distribution pin.

## Implementation

Partial — Next is integrated through persisted selection, managed package
discovery, supervised processes, authenticated provider/remote inputs, selected
tracks, fallback, graphics, troubleshooting and health. Row200's conditional
Next default and automatic runtime acquisition are accepted with exact
six-platform staging and affected lint. The public package-only repository
contains all19 unchanged accepted assets; six anonymous archive downloads match
their pinned checksums. Frozen source08 proves public cold acquisition before
listening and decoded Linux/native Windows TS/HLS with selected Dutch audio and
captions, reconnect, preview and idle cleanup. Bundled playback and all six
read-only/offline/override/failure startup cases pass on available platforms.
Packages and ACLs remain unchanged; owned hosts exit. Matched Chromium31 and
Firefox30 pass, with one unchanged older skip and all changed no-JS preservation
and diagnostics cases executed. This establishes row200's availability condition.
Final UCU-09 window gates through v26.11.1 and approved platform acceptance debt
remain open; the owning [catch-up milestone](../progress/milestones.toml) records
exact canonical proof, independent reviews and the remaining gates.
Historical Linux and native Windows application playback tests passed
with Next `6b49dae` and published ErsatzRS-ffmpeg v2.0.4, including selected
subtitles/audio and graphics. Direct RGB image opacity is now proven; the
older limitation described below is superseded. Accepted live embedded-text
limitations and approved QSV/AMF/native-Linux-NVIDIA hardware debt remain
explicit in [catch-up parity](../progress/parity/upstream-baseline-catch-up.toml).
These results do not establish full application acceptance on all six targets.
On 2026-10-01 the maintainer approved retaining Linux ARMv7/ARM64 and both macOS
targets as explicit unverified application acceptance debt at UCU-09 closeout.
Windows proof includes production-adapter Next HLS, JavaScript-selected audio,
subtitle retention, normal/preview isolation and MPEG-2 output. The owning debt
is `ucu-arm-macos-application-acceptance`; declared platform support is unchanged.

### Historical implementation checkpoints

The following checkpoints explain the integration sequence. Their references
to then-open work are historical; current gate status belongs to the owning
milestone and parity debt records.

The UCU-02 row 13 infrastructure checkpoint adds executable discovery, typed
JSON exchange and atomic complete-schedule publication. Profile conversion and
a managed process adapter add bounded startup/readiness, heartbeat and cleanup.
The host now connects local-video HLS startup, shared capacity and session
registration/cleanup. Rebuild notifications refresh through the same publisher;
busy source locks are retried until publication or session cancellation.
At that row-13 checkpoint, authenticated HTTP delivery and platform acceptance
remained open; the later row-20 delivery proof is recorded below. Full platform
acceptance remains open.
Item track projection reuses native database selectors and subtitle-path
resolution, with decoded Linux proof for alternate audio/subtitles and explicit
silence/subtitles-off. Locked schedule reads preserve mirror offsets and bind
one media file per item. Per-channel publication now serializes reads and writes,
including cancellation during filesystem work. A Next HLS session adapter now
handles readiness, heartbeat, idle expiry, refresh notification and shutdown.
The row-20 work adds provider-scoped local paths and authenticated
Plex/Jellyfin/Emby HTTP sources. The initial direct-HTTP checkpoint has synthetic
server decoded Linux proof. The current integration publishes a capability for
an internal route on the existing web host instead of provider credentials.
That route validates the persisted media binding and resolves current provider
configuration at open time, using bounded identity-verified Plex failover and
streaming byte-range responses. This avoids a second server process and keeps
provider credentials out of Next playout JSON. The runtime playout files retain
restrictive temporary-file permissions on Unix and inherit the data directory
ACL on Windows. Focused credential-rotation, Range and web response tests pass,
as does decoded Linux Next playback through the production media-open adapter.
Decoded Plex failover, credential-isolating redirects and real-host HTTP/preview
smoke also pass; row 20 is Port.
Single permanent or intermittent watermarks now use shared selection in ordinary and
Dynamic Burn item projection. The official Next build's RGB-image opacity defect
is an explicitly accepted upstream limitation pending its fix; no host workaround
or patched engine is introduced. UCU-02 owns the evidence and remaining gates.
Other graphics projection and full playback acceptance remain open.

The row-48 music-video credits integration renders the existing native ASS
template on demand for a persisted playout binding. The optional Next process
uses an HTTP subtitle source. FFmpeg's libass opens that URL itself and does
not forward Next's source headers, so this one route uses a process-local,
binding-scoped ticket in the URL. The route also requires an active channel
session and rechecks the persisted media, in/out points, credits mode and
subtitle selection before rendering. Tickets are separate from the general
media-open capability and expire with the host process. This is the scoped
transport adaptation to upstream's unauthenticated loopback credits endpoint;
ordinary media remains header-authenticated. The URL may appear in local
FFmpeg reports, so it grants only generated credits for that exact binding.

The row-49 implementation adds authenticated Dynamic placeholders for internal
programme gaps. Each active Next session owns at most one selected ephemeral
fallback occurrence, revalidates its current gap, policy, media and track
selection, and discards it when the session ends. Callback bounds retain the
persisted schedule's 100-nanosecond precision. Filler never becomes a persisted
playout row. Embedded Burn preparation and ephemeral generated ASS credits use
the existing bounded subtitle cache; credits are keyed by rendered content and
validated against the selected occurrence before publication. Local gap playback
has decoded Linux evidence. Explicit persisted/fallback origins now carry the
correct current-occurrence validation into ready-artifact, ordinary-producer and
retained-interval preparation. Existing bounded probing, fresh track mapping,
one-shot readers and session cleanup retain ownership. Finite direct HTTP,
prepared, ordinary and retained fallback have decoded Burn/Convert handoff
proof. Direct and ordinary live external captions, authenticated Plex/Jellyfin/
Emby inputs, generated credits and explicit audio/subtitle-off selections also
have decoded Burn/Convert handoff evidence. Generated silence is distinguished
from source stream zero when retaining the published selection. Focused
regressions and routed HLS/Hybrid/preview smoke pass. Row 49 is reconciled;
window-wide and supported-platform acceptance remains owned by UCU-04.

Row 50 publishes Next schema `0.0.2` and supplies stored video, audio and
embedded-subtitle probe hints for local and provider inputs. Existing one-shot
sources keep fresh producer-owned hints. If persisted stream metadata cannot
describe every selectable track, normal Next probing remains available; sending
an unverified hint would hide a changed stream layout. Still-image hints carry
their image container identity when that metadata is complete. Leading gap
placeholders start at a 100-nanosecond publication boundary and are admitted
against the exact active process instance, so normal and preview callbacks
can use only exact intervals published to their shared channel manifest while
retaining separate session authentication. This is the scoped validation
adaptation to upstream's unvalidated stored hints and initial filler callbacks.

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

Integrate Next alongside the native streaming engine. Make Next the default
for newly created channels and standalone playback previews only after its
runtime is automatically available under the accepted condition below. Preserve
the native option, existing channel selections and native AV1 behavior under
ADR 0106. Installing or upgrading must not silently convert existing channels,
rewrite their profiles or choose another engine after a runtime failure.
Channel-bound playback previews retain their stored channel engine unless the
operator explicitly selects another one. Port the accepted behavior through
the ordered catch-up ledger.

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

### Accepted automatic availability condition

Official application archives and Docker images must carry the exact accepted
Next package. A standalone or source-built host must acquire that same package
automatically before composing application state or accepting requests. Reuse
the existing six-platform mapping and one pinned release manifest for acquisition
and package staging; no new engine build or mutable release tag is implied.
Verify archive and payload SHA-256, source/patch provenance, licenses and platform
companions before executing an automatic runtime. Report the accepted runtime's
version and acquisition source through the existing diagnostics and health check.

Prefer a verified shipped copy, then a verified persistent installation; both
must work without network access. Otherwise download the exact immutable package
with bounded time, size and installation locking, validate it in owned staging,
and publish it atomically. Keep managed state separate from read-only package
files. A failed or invalid acquisition is an actionable startup failure, not an
implicit native fallback. Do not discard unrelated caches or running processes.

An explicit `ETV_NEXT_FOLDER` remains an operator-owned override with strict
precedence: an invalid override must fail rather than trigger a different runtime.
The existing bounded version check establishes launch/output for custom builds,
not equivalence to the accepted distribution or its channel schema. Mark that
source as unverified against the published pin; do not present it as package
acceptance. Automatic copies always require the exact published identity.

Acceptance requires actual Linux x64 and native Windows cold acquisition,
offline bundle/cache reuse, failure and concurrent-install safety, retained
selected audio/subtitles, and the real hydrated/no-JS creation and preview flows.
Every supported RID needs exact archive/payload integrity coverage. The existing
approved ARM/macOS application and hardware debt remains explicit and unchanged.
The final UCU-09 integrated gates and exact-SHA CI still own release acceptance.

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

### Playout publication implementation

Use one stable JSON filename covering years 1 through 9999 in a channel-owned
directory. Replace the complete file through a same-directory `tempfile`
publication, retaining the previous complete file on failure. The Next loader
reopens the matching file on each item selection; individual item timestamps
remain authoritative for coverage. Empty schedules explicitly withdraw old
items. Temporary filenames cannot match the loader's filename parser.

This implements the complete-generation visibility intended by upstream
`SyncNextPlayoutHandler` without Unix symlink or Windows junction switches.
Upstream row 18 adds junction replacement on Windows; the accepted atomic-file
strategy remains unchanged.
It does not create another scheduling authority: SQLite remains authoritative,
and the integration must regenerate this derived file after restart. The caller
must serialize schedule reads and publication to prevent stale generations
from overwriting newer ones. UCU-02 row 18 proves native Windows/NTFS complete
replacement, concurrent reads and preservation/recovery after sharing denial.
This verifies publication only; full native Windows Next playback remains open.

### Profile and process boundary implementation

Map profile dimensions, codecs, bit depth, bitrates, kHz-to-Hz audio rate,
loudness, scale/crop/stretch, deinterlacing and supported hardware selections
to the packaged Next API. Its current output contract supports H.264, HEVC and MPEG-2 with
AAC/AC3 SDR normalization. MPEG-2 requires a positive video bitrate and forces
eight-bit output, matching the official Next configuration contract. Reject explicit unsupported codecs, HDR output,
V4L2M2M and Nouveau selections instead of inheriting the original integration's
silent codec or acceleration substitutions. User-facing validation and health
for those failures remain part of integration acceptance. Native encoding
policy fields without Next API equivalents still require reconciliation before
claiming equal output behavior.

Reuse the existing managed Unix process group and Windows Job Object owner.
Send configuration on stdin, remove stale readiness before spawn, share one
startup deadline across configuration delivery and readiness, and await bounded
whole-tree cleanup on startup failure or normal stop. Raw child stdout/stderr
must not be emitted or retained directly because they can contain private source
URLs. Upstream row 94 adds diagnostic levels and targets: process-owned readers
discard oversized lines, sanitize complete messages with the existing Remote
Stream redactor plus exact configured path/session redactions, and retain only
a bounded tail. Reader completion is bounded after managed tree cleanup; dropping
the owner aborts readers. Troubleshooting additionally opts into bounded stream
selection decisions without making scheduled conversion verbose. Adapter errors
still contain static operation labels, error kinds and exit codes. The host
session owns admission, idle policy, shutdown and heartbeat scheduling.

Track projection uses explicit video/audio/subtitle sources and omits the
general item source, so an absent track cannot inherit it. Exact pins are
validated against the resolved version and stream metadata. Since Next requires
an audio input, pinned silence uses generated silent audio with a finite probe
hint, following Next's own synthetic-input contract; it does not restore source
audio. Sidecar and extracted subtitle paths use the existing native resolver,
and item in/out points remain on the original media timeline. Prepared provider
intervals use the shared version-bound preparation paths; graphics and
watermarks use the publication rules below.

Schedule loading reuses the canonical playout repository and shared scheduler
locks, retaining one logical row per item and choosing its pinned version or the
first version/file by ID. Mirror offsets apply to query, playback and guide times.
Busy reads retain the previous publication; empty successful reads can withdraw
it. Missing media fails the complete read. The per-channel publisher retains these locks
through publication and serializes channel reads, projection and replacement.
Its blocking filesystem worker owns both locks until completion, even if the
awaiting refresh is cancelled. The host must share one publisher per channel
across startup and rebuild triggers, retaining its stable channel-ID folder. These are
scheduler build locks, not a transaction snapshot across metadata or admin edits.
Remote bindings still require provider resolution before publication.

The session adapter implements the existing HLS handle: requests renew demand,
stop wakes supervision, and rebuild notifications coalesce while publication is
in flight. Heartbeat and cancellation remain responsive during refresh work.
Startup has one deadline; terminal paths hide the playlist, including task abort.
Playlist reads are bounded and use Next's `live.m3u8`. The shared host now owns registry
reservation/removal, transcode admission, output-folder cleanup and publication.
A Next reservation uses the native HLS capacity identity and lasts for the
whole engine session; native per-programme reclamation cannot pause this engine.
Next previews reuse the native browser-compatible profile override without
persisting it. Their output folder, registry and capacity key are independent
from normal playback. Both sessions share the channel publisher; rebuild
notifications reach previews even when normal playback is absent. Continue
builds notify dependent Next channels and mirrors after persistence; native
sessions retain their existing continuation behavior without restart.
HTTP master playlists preserve existing token propagation. Next Hybrid channels
use the existing HLS-to-MPEG-TS wrapper; explicit segmenter requests remain HLS
to avoid redirect loops. At initial UCU-02 acceptance, explicit legacy-concat
requests were rejected for Next to prevent native fallback. UCU-09 row198 ports
upstream #3062: cached legacy TS requests now use the existing Next wrapper,
preserving the selected engine and stored settings. Native concat entrypoints
continue to reject Next; implicit Hybrid redirects retain the accepted contract.
Row198's available platform verification and remaining gates live in UCU-09.
Accepted ADR 0099 selects the session media playlist for the Next wrapper,
keeping converted text on HLS and Burn text visible in MPEG-TS. Decoded
Burn/Convert and lifecycle checks pass; UCU-02 window gates remain open.
The health engine checks the required Next runtime's executable and accepted
identity even when no enabled channel selects it. Profile and overlay checks
apply only to enabled Next channels. These checks are not playback acceptance.
Local-library sources reuse native path replacement and track selectors. Provider
inputs use the authenticated media-open integration described above. Permanent
and intermittent watermarks and image graphics share stable ZIndex ordering and
independent source timelines. Intermittent timing uses wall-clock cycles and
Next's default fades. Images reuse native selection and template variables,
including per-item overrides and source geometry, for scheduled and preview
playback. Plain permanent/intermittent watermarks use direct media layers. Mixed or
expression graphics use the authenticated FFmpeg canvas accepted in ADR 0105,
including image, text, script, motion and subtitle graphics. Current official
Next RGB opacity has decoded Linux and Windows proof; the production HTTP
canvas has decoded Linux application proof. This is implementation evidence, not a waiver of the remaining platform
acceptance.

On-demand Next sessions shift the stored timeline before manifest publication,
then checkpoint viewer touches every five minutes and on exit. The saved point
retains the configured segmenter safety interval and cannot move backward or
before the first item. Startup shifts and bounded writes share the playout
rebuild lock; heartbeat/idle time and preview sessions do not advance normal
playback. UCU-06 row 107 owns restart and decoded resume acceptance.

## Consequences

ErsatzRS must operate and verify two selectable streaming paths. Packaging,
configuration migration, process cancellation/restart, health, troubleshooting
and browser controls must accurately distinguish the selected engine.
The native pipeline remains available while Next is implemented; that does
not satisfy acceptance of the optional engine.

The catch-up milestones retain their per-row review and window acceptance
requirements. Prove configuration roundtrips, retained native channel choices,
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
