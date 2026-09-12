# ADR 0087: Stream SponsorBlock intervals without playback underruns

## Status

Accepted 2026-09-05 — the maintainer approved a streaming-first proof on the
condition that SponsorBlock cuts do not cause user-visible buffering.
Amended 2026-09-06 — the maintainer permits a small, bounded initial startup
delay to establish the playback reserve instead of risking interruptions at cuts.
The maintainer subsequently specified 10-15 seconds: target first playback within
10 seconds and enforce a 15-second maximum, including source opening and buffer
establishment. This replaces the experiment's provisional five-second cap.
Amended 2026-09-06 after native-transfer startup measurements — the maintainer
approved a 20-second absolute maximum, retaining the 10-second target and the
uninterrupted-cut requirement. This supersedes the earlier 15-second maximum.
Supersedes ADR 0085's mandatory processed-video preparation decision and ADR
0086's preparation-worker requirement for this playback path.
Amended 2026-09-06 — split delivery into AMM-M-01A streaming and AMM-M-01B
optional Download ahead, selected at list level as described below.
Amended 2026-09-06 — the maintainer approved a bounded playback-owned HTTP
Range adapter inside the existing server to preserve required source headers.
Amended 2026-09-06 — that adapter decision is superseded by
[ADR 0088](0088-prefer-native-yt-dlp-transfer-and-capabilities.md). The maintainer
requires its removal and yt-dlp-owned transfer. Earlier adapter evidence below
is historical, not the active implementation or an approved fallback.

## Implementation

Partial — M-01A streaming is complete under the native-transfer amendment in
ADR 0088. Its owning milestone records options, frozen intervals, real scheduled
MPEG-TS/HLS cut continuity and cancellation acceptance. M-01B implements the
generic optional Download ahead modes, including the visible renewed cutoff
and frozen mixed publication. Its 2026-09-12 first-default-TV publication
regression is corrected: native/browser tests and independent real-provider
publication now pass. The owning milestone retains earlier playback evidence
and the scan-binding/failure-feedback corrections; M-01A is not reopened.
Static startup-stage diagnostics remain separate visible debt. Current evidence
belongs to the milestones, not the
superseded adapter checkpoints below.

### Historical implementation checkpoints

Partial — shared validated millisecond timeline, retained-range ffconcat
generation, digest-covered revision/journal plans, edited scanner durations and
ordinary-host metadata jobs have focused tests. Normal publication now admits
and selects saved timing-bound interval plans; automatic refresh waits for their
results without reopening terminal attempts. The official yt-dlp adapter now
implements metadata-only interval discovery with offline preprocessing proof.
The host now has a typed ephemeral seekable resolver and cancellable bounded
invocation. The official provider now implements finalized muxed/separate HTTP
resolution with offline selection proof. The in-host Range adapter has five
focused tests covering header/range fidelity, unsupported responses, admission,
HEAD/body cleanup and cancellation under backpressure. Independent review fixed
cancellation that previously depended on downstream polling. A synthetic installed
FFmpeg 9 run confirms per-file capability headers across source opens and a later
exact byte seek; an overly strict assertion requiring a network seek for an
already-buffered early range failed, then the corrected assertion passed on
rerun. Two host-route tests also pass for GET/HEAD fidelity and capability/range
rejection. This is not channel-continuity evidence.
The actual listener address now supplies the private playback origin. Retained
concat files and registrations have one encoder owner, with immediate revocation
on cancellation and file release after awaited exit. The opt-in FFmpeg input
policy preserves the shared concat clock and cut selectors without changing
ordinary pipeline snapshots. Publication selection now returns immutable interval
intent without invoking a provider; fresh resolution uses the saved options and
fingerprint and includes installation lookup in the remaining startup budget.
Ordinary inputs and probes reject unresolved interval intent rather than serve
uncut media. The bounded probe runner now accepts retained input options and
cancellation, shares its deadline through child exit and output collection, and
redacts both failed-process and malformed-JSON diagnostics. Pure in-memory
converters preserve fresh stream indexes, geometry, color/Dolby Vision and audio
metadata without changing the frozen programme duration. The shared startup path
now composes resolution, registration and one muxed or two sequential separate
probes under one deadline, returning the owned files and actual-index inputs.
Both encoder supervisors now consume this owned result, select fresh audio/video
metadata and recreate retained inputs for hardware retries. Capacity waits remain
source-free; cancellation signals and awaits preparation cleanup. Typed hardware
failure evidence is extracted before private input diagnostics are redacted.
Selected source-clock subtitles are explicitly refused pending edited-clock
handling, rather than silently omitted. Forty-six focused encoder/audio/retry
tests pass. Cross-review fixed an exhausted-plan retry loop and default-audio
language labeling in HLS; independent rereview confirms both corrections.
Retained inputs now use an eight-second initial burst on both audio/video legs.
HLS waits for eight seconds of completed media and the configured segment floor;
MPEG-TS uses a paced encoded-output reserve capped at 32 MiB. Short complete
remainders require successful encoder exit. Failed attempts that already emitted
media cannot restart the same content into the reserve. Graphics and watermark
startup reuse the cancellable owned probe runner; initial channel discovery also
shares the request deadline. Focused buffer, probe and lifecycle tests pass.
Measured playable reserve, strict end-to-end startup timing and actual channel
cancellation/continuity proof remain open. Direct MP4 reserve support is not yet
implemented. Remaining capacity-slate and post-preparation awaits still need
runtime cancellation scrutiny; a deadline check before spawn is not sufficient.
AMM-M-01A remains reserved
and incomplete. AMM-M-01B is not
started; existing uncommitted preparation code is preserved as its groundwork,
not scheduled for deletion. Neither mode selection nor its new-video boundary
is implemented merely because this decision is accepted.

## Context

The user wants server-side streaming with SponsorBlock hard cuts, not a complete
download and post-processing step before playback. The previous file-based
design introduced storage admission, revision pins and recovery infrastructure
that are unnecessary for this goal. Replacing it with sequential network seeks
alone is insufficient: opening the next range at the cut can starve playback.

## Decision

Delivery mode is a generic provider-neutral list option, independent of
SponsorBlock. Use one list-level native HTML select with three localized choices: **Stream**
(default), **Download ahead**, and **Download ahead only for new videos**.
On 2026-09-07 the maintainer confirmed that entering or re-enabling the
new-videos-only mode establishes a fresh cutoff. Saving that same mode preserves
the cutoff. Display its persisted date/time beside the list delivery selector.
Already-known videos stream after the next publication; current published
playback remains unchanged until publication succeeds.
This is not a checkbox or a per-item delivery override. The last choice means
videos first imported after enabling that mode; videos already in the list
continue streaming. Preserve this distinction across refreshes and restarts.
The existing SponsorBlock removal option remains separate; downloading ahead
works even when removal is disabled or unavailable. When enabled, removal must
be honored in every mode: interval skipping for streamed items, file processing
for downloaded items, and both paths in new-videos-only mode. Unsupported or
failed removal must be explicit, never silently treated as successful uncut
playback. Eligibility depends on
declared delivery capabilities, not add-on identity or SponsorBlock support.
This does not promise full-download support for every provider or live source;
yt-dlp is the first integration, not the owner of the generic setting.
In Download ahead paths, download eligible finite videos in full and remove requested intervals
before publishing their prepared artifacts. Do not silently switch modes.
Freeze effective item delivery with the publication so a saved list-mode change
does not alter current published playback. The following streaming constraints
apply to Stream, including the existing-item cohort of the new-videos mode.

Retrieve bounded interval metadata through the add-on, without downloading the
media. Preserve effective browser options, typed access checks, fair-use limits
and immutable publication semantics. Freeze normalized source intervals and
effective duration with each publication; map programme seeks onto that same
source timeline. The host remains provider-neutral.

Resolve temporary seekable HTTP inputs through a separate
`remote-stream.resolve.v1` capability, preserving existing stdout playback.
Fresh transport resolution must not rewrite or rehash a published cut plan.
Bind its response to the frozen identity and independently observed source
duration, without requiring a preliminary tool-version probe. URLs and required
headers are invocation-local transport data, never revision metadata; reject
unsupported transport instead of omitting authentication or requested cuts.

The existing web host owns an invocation-local HTTP Range adapter for these
resolved inputs. Stock FFmpeg 9 concat cannot carry arbitrary multi-line headers;
the adapter forwards them while FFmpeg sees a fixed local URL and one opaque
capability header, avoiding capability values in ordinary URI request logs. Stream
bytes with backpressure, bounded admission and request lifetimes, and revoke
locators/cancel requests with playback. Do not expose an arbitrary-URL proxy or
persist transport credentials. This is not a new listener/service, cache,
prefetcher, full downloader or transcoding layer, and requires no custom FFmpeg
build. Validate exact Range responses and redact local capability locators too.
Real-provider acceptance exposed CDN redirects when later retained ranges open.
Follow at most three HTTP redirects within the original request deadline,
revalidating every destination against the installed add-on's declared hosts.
Preserve GET/HEAD, byte ranges and required headers; reject HTTPS downgrades and
cross-origin Authorization instead of dropping required authentication. Do not
cache redirected signed URLs. Retained inputs use FFmpeg's error-exit policy so
a failed later range cannot masquerade as successful shortened EOF.

Stream retained source ranges through the existing channel pipeline. Reuse
existing bounded playback buffering where it actually covers range transitions;
do not assume an HLS work-ahead target proves next-range readiness or protects
legacy MPEG-TS. The next retained range must be ready early enough that ordinary
source-open latency and tested network jitter do not drain playable output.
Hard cuts are acceptable; a spinner, frozen frame, audio gap or timeline reset
introduced by the cut is not. Correct final-file timestamps alone are not proof.
Startup may establish a small safety reserve, but must have a finite deadline;
do not wait indefinitely or grow the reserve toward a whole-item download.
Use the approved 20-second startup maximum and report the actual startup time and
reserve before claiming acceptance. The reserve size remains experimental; it
must not be confused with the wall-clock startup allowance.

Begin with one synthetic runtime proof, not wholesale production integration.
Measure startup, range-open delay, playable buffer around each cut, audio/video
continuity, media bytes fetched and cancellation. Include adjacent/short retained
ranges and joining after a cut, then verify actual MPEG-TS and HLS playback.
Record the tested network conditions; no finite buffer can guarantee continued
playback through an arbitrary outage or sustained insufficient bandwidth.

Do not introduce mandatory whole-item downloads, an unbounded look-ahead cache,
a second transcoding layer or a new buffering service to satisfy this condition.
If the proof needs substantial additional machinery, stop for reassessment.
Do not silently omit a requested skip, freeze the output or change the published
duration as a fallback. Live playback stays unchanged. Unsupported source shapes
must be explicit rather than silently reverting to full-file preparation.

## Consequences

M-01A retains shared settings, access and publication work. M-01B reuses the
preserved processed-file admission, worker recovery and playback binding;
ADR 0086 remains applicable to preparation, not a streaming prerequisite.
Existing files, migrations, reservations and uncommitted work are not deleted.
Generic subtitle support remains M-02; broader storage administration remains
M-03. This decision does not start those later slices.

## Alternatives Considered

- Mandatory full-file SponsorBlock post-processing: rejected for streaming;
  retained as the explicit Download ahead alternative in M-01B.
- Opening each range only when playback reaches its boundary: exposes network
  latency at the cut and does not meet the continuity requirement.
- Discarding intervals from a source-realtime pipe: still transfers/decodes the
  removed content and can starve output while doing so.
- Masking underruns with repeated frames, silence or uncut content: rejected.

## References

- [ADR 0085](0085-broaden-addon-list-lifecycle-and-provider-options.md)
- [ADR 0086](0086-isolate-addon-preparation-process-ownership.md)
- [M-01A streaming plan](../../plans/addon-list-lifecycle-broadening.md#amm-m-01a--streaming-with-sponsorblock-intervals-removed)
- [M-01B Download ahead plan](../../plans/addon-list-lifecycle-broadening.md#amm-m-01b--list-level-download-ahead-modes)
- [Structured progress](../progress/README.md)
