# Upstream ErsatzTV catch-up

## Accepted release boundary

On 2026-10-06 the maintainer fixed UCU-09 at upstream v26.11.1:
`46118d43313b7828780fbfcfb46e980390ea8b0d`, ledger row208 (local and remote tag
verified). Rows1-208 are accepted; there are no remaining in-scope
rows. Rows209 onward remain recorded as later work and do not block or reopen
UCU-09. Existing row acceptance, final local gates, push and terminal green
exact-SHA CI remain required. ADR 0117 is accepted for necessary temporary
Next corrections and their verified packages, with mandatory upstream retirement.

## Row 200 source map and acceptance

Accepted slice: `a3bbf89d`, default Next playback and native-only profile help.
Adapt under accepted ADR 0095: Next is the default for new channels and
standalone previews only with the exact engine bundled or automatically
downloaded and verified before startup. Preserve stored engines, explicit
Native/AV1 and shared profile values; omit upstream's bulk engine migration.

The public package-only ErsatzRS-next repository supplies the unchanged accepted
Next036 packages without GitHub credentials. All six anonymous archive hashes
and staged payload/provenance checks pass. Linux and native Windows prove cold
acquisition before listening, decoded TS/HLS with selected Dutch audio/subtitles,
reconnect, preview and idle cleanup. Native bundled playback and all six actual
read-only startup cases per platform pass: downloaded-cache offline reuse,
offline bundle, strict absent override, usable custom override and failed
download without a cache. Packages/ACLs remain unchanged and owned hosts exit.

Focused default/health/repository/lifecycle checks, affected strict lint and
matched Chromium/Firefox checks pass. The two demonstrated browser defects are
corrected: narrow no-JS help overflow and a failed unspawned preview borrowing
an earlier producer's log. Both engines retain the original diagnostic checks.
UCU-09 owns the exact receipts and independent reviews, including retained
historical failed attempts. Row201 complete-window troubleshooting is next;
final integrated/window CI and approved hardware/ARM/macOS debt remain separate.

## Row 199 source map and acceptance

Accepted slice: `df6ae228`, Next output frame-rate normalization. Port:
channel schema0.1.1 has an optional target derived from the selected scheduled
versions and requested profile, including mirrors. Integer/rational/NTSC
conversion matches upstream; unknown0/0 probe hints preserve startup without
publishing an unusable target. Disabled normalization avoids unnecessary reads.

Eleven focused cases on each Linux and native Windows platform, strict infra
all-target lint and three actual playback cases per platform pass. Decoded
integer normalization, original-rate passthrough and rational mirror output
retain selected Dutch audio, Convert/Burn captions, exact pins and full persisted
state. Matching coded-video slices copy unchanged; mismatched sources transcode.
Independent reviews recompute every-frame cadence, measured source-clock
precision, cumulative phase, audio/pixels, payload identity and owned cleanup.
The original failed disabled-only oracle is preserved; its correction accounts
for original Matroska timestamp precision and still rejects drift/missing frames.

UCU-09 owns exact frozen-source and canonical acceptance proof. No runtime
package build, browser, hardware, other-platform or full-CI claim is introduced.
Final integrated/window/release gates remain open. Continue at row200, whose
Next default is maintainer-approved only with bundled or verified automatic
runtime acquisition; existing channel selections and explicit Native remain.

## Row 198 source map and acceptance

Accepted slice: `897a6ef9`, cached Next streaming-mode compatibility. Port:
resolved legacy MPEG-TS URLs now use the existing Next hybrid wrapper. Existing
Next-first HLS dispatch already coerces old playlist modes to the segmenter;
expanded regressions preserve tokens, optional IPTV authentication and native
entry-point isolation without rewriting stored channel/profile settings.

Five portable routing/authentication tests and strict web/xtask all-target lint
pass. Native Windows runs those five plus all 21 current artwork tests. Actual
Linux and Windows host playback decodes Dutch 880Hz audio and burned captions
from ordinary TS, cached legacy TS, reconnect and a real cached-HLS segment.
Real master/media/live manifests, preview, disconnect, persisted-state equality,
positive cached-wrapper restart and owned idle/process cleanup pass; independent
reviews verify raw PCM/RGB/TS and exact published FFmpeg v2.0.6/Next036 payloads.
Restart is an HTTP/lifecycle assertion, without a decoded-restart claim.

UCU-09 owns exact frozen-source and canonical acceptance receipts. A reviewed
metadata-only repair adds required upstream credits to unpublished rows194-197;
all commit trees, working files and index are unchanged, and historic source
receipts retain their original identities. No package rebuild, browser, hardware,
other-platform or full-CI claim is introduced. Final integrated/window/release
gates remain open. Continue at row199, output frame-rate normalization.

## Row 197 source map and acceptance

Accepted slice: `39c1811f`, custom Next embedded-text eligibility. Covered:
`ErsatzTV.Core/FFmpeg/CustomStreamSelector.cs` removes the Next exemption from
unextracted embedded text outside HLS Direct; the new upstream test selects the
eligible extracted English candidate. The shared Rust predicate already runs
before custom YAML rules for both engines, preserving Direct, bitmap, selected
version and exact-pin/troubleshooting boundaries.

The owning SQL/YAML fixture now explicitly sets Next and HLS Segmenter. It
rejects the first unextracted English candidate, selects extracted subtitle4
at stream5 and preserves audio1 through the shared selector and Next wire
projection. Four focused cases include the existing two-engine/settings/path,
bitmap/version/pin and automatic/exact matrix. Strict infrastructure Clippy,
formatting, source guards and substantive review pass. UCU-09 owns exact proof.
The synthetic extracted path proves selection metadata; no new physical
sidecar decoding or native Windows execution is claimed. Production policy,
generated FFmpeg output and runtime packages are unchanged. Final integrated
Linux/Windows and window/release gates remain required. Continue at row198.

## Row 196 source review and acceptance

Accepted slice: `ffa4743e`, upstream Windows artifact size. N/A: the complete
change removes .NET native PDB publish assets and narrows EFCore.BulkExtensions
to its Core package. Neither dependency tree is used by Rust/SQLx.

The release profile strips symbols; both Windows packaging paths copy four
named application executables instead of the build-directory contents. The
optional pinned Next executable and required companions are staged separately
from exact permitted archive members. The existing published Windows Next
archive passed checksum and payload-provenance verification; its selected
members contain no PDB. UCU-09 owns exact source/archive hashes and review.
No source correction or new build/release is required. This does not replace
final release-archive execution, integrated platform gates or exact-SHA CI.
Continue at row197, custom subtitle eligibility.

## Row 195 source map and acceptance

Accepted slice: `007b3415`, Local deep scan. The commit covers Image, Movie,
MusicVideo, OtherVideo, RemoteStream, Song and Television. Only native television
has an unchanged-season shortcut requiring a deep bypass; the other families
already reprocess their source metadata. CLI and durable native runtime preserve
the requested flag, retryability, identity and international tracks.

- Always-mounted localized confirmation covers library and external-collection
  deep actions. No-JavaScript renders current rows with disabled scan controls
  and a clear requirement; it does not imply a scan was dispatched.
- Local original artwork is database-gated and reread with no-cache. A stable
  URL policy marker bypasses the previous one-hour browser cache. Provider and
  compatibility-hash routes retain their existing policy.
- Known finite/live RemoteStreams still use fresh bounded probes. Managed
  definitions and Unknown liveness preserve existing publication authority;
  local deep scan does not trigger managed provider rediscovery.
- Linux and Windows scanner/durable-restart proof, eight Libraries browser
  cases, four real-cache browser cases, eighteen artwork regressions, strict
  lint and independent review pass. UCU-09 owns exact receipts and explicit
  source deltas for reuse. Historical failures remain retained.
- Final integrated Windows artwork execution is still required at window
  close; no new Windows source06 execution or full CI/publication is claimed.
  Existing approved platform/hardware debt is unchanged. Continue at row196.

## Row 194 source map and acceptance

Accepted slice: `fb75ce8c`, content-bound local-library duration. The existing
parser at the ErsatzRS prerequisite `2fdd126c` implements upstream video
selection and complete-audio fallback. ADR 0117 additionally guards the
verified FFmpeg9 Matroska numeric/container20-second estimate when a valid
content tag and video packets end at one second. Row194 is accepted as Adapt.

- Upstream: `ErsatzTV.Infrastructure/Metadata/LocalStatisticsProvider.cs`
  and `ErsatzTV.Infrastructure.Tests/Metadata/LocalStatisticsProviderTests.cs`.
  Choose highest-bitrate non-attached video, retain first bitrate tie, use
  usable stream duration/tag, and require all audio durations for audio-only
  maximum before container fallback.
- Rust projection: `ersatzrs-infra/src/ffprobe.rs` and its extractor tests.
  Preserve independent numeric durations, other-container precedence,
  missing/malformed-tag fallback, subtitle language and selected stream data.
- Persistence: public `scan_local_library`, normal insert/update and rescan
  of `MediaVersion.Duration`; do not seed its duration manually.
- Runtime acceptance: build the normal persisted schedule from scanned IDs,
  then decode real native HLS and published Next output across the one-second
  blue-to-red programme seam, with six-second finite Next ENDLIST and owned
  stop/reaping/capacity restoration.
- Existing parser/projection/lint evidence is reusable with its unchanged
  source identity. The exact v2.0.6 probe reconfirms the numeric20/tag1/packet1
  conflict. The old retained clip has no language metadata; a new synthetic
  blue/Dutch-subtitle fixture proves persistence and track preservation on
  Linux and native Windows; normal scheduling and actual decoded native/Next
  handoff, six-second finite Next ENDLIST and owned cleanup pass.
- Exclusions: no production-parser change is currently demonstrated, no
  FFmpeg/Next build/release, browser/layout/schema change or later row.
  UCU-09 owns exact evidence; platform/hardware debt and final gates remain.
- The first truly video-only fixture exposed a separate existing native audio
  graph defect. A subsequent non-row checkpoint resolves
  ffmpeg-command-generation/native-video-only-null-audio-fallback using shared
  generated silence only after complete source metadata confirms absence.
  Explicit Disabled, pinned/selected tracks, unknown pipe inputs, fresh retained
  metadata and source seek remain preserved. Genuine video-only native HLS and
  finite direct/HLS Direct decode silent audio and content with owned Stop/cleanup
  on Linux/Windows; UCU-09 owns exact evidence and final lint/transport gates.
  The original failure and independent source05 audio-bearing duration proof
  remain historical. Baseline194 is unchanged; continue at row195.

## Row 193 source map and acceptance

Accepted slice: `432f107e`, troubleshooting first-frame seek correctness.
Port: serve generated `live.m3u8` for both engines; preserve
the owned job ID, stale-read refusal, process reaping, cancellation, capacity
and native/Next cleanup quarantine. No argv, runtime-package, schema, layout
or row201 preview-window change is included.

- Upstream controller: `Controllers/Api/TroubleshootController.cs` selects
  the generated playlist, waits for Next `.ready`, and requires two Legacy
  segments while running.
- Upstream player: `Pages/Troubleshooting/PlaybackTroubleshooting.razor` passes
  start position zero; shared `_Host.cshtml` retains default minus one for live
  channel previews. Existing page geometry and controls are unchanged.
- Rust API/identity: `api/troubleshooting.rs` and
  `host/troubleshooting_lifecycle.rs` expose `live.m3u8` on the existing GET
  redirect and owned job URL. Preserve registry read guards, filename bounds,
  no-store status, stop/replacement and SSR/no-JS form routes.
- Rust supervision: `host/troubleshooting_next.rs` requires the fresh `.ready`
  and generated playlist while running. The published Next marker needs four
  segments and is not emitted by `finish()`: successful reaped short previews
  retain a separate final-ENDLIST/referenced-segment fallback. Failed or
  cancelled previews never gain readiness from that fallback. Native running
  readiness requires two segments; successful finite EOF retains one.
- JS boundary: existing `public/channel-preview.js` accepts optional
  `startPosition = -1`; `public/troubleshooting-preview.js` alone passes zero.
  Preserve async player identity, subtitle synchronization, autoplay, error
  reporting and navigation teardown; no Leptos DOM or localization change.
- Acceptance passes: readiness/EOF, API/job URL, stale-file/ownership checks,
  real published Next selected-track/callback/short-EOF proof on Linux and
  native Windows, and first-presented sought frames in both browsers with an
  ordinary live-edge control and unchanged encoded segments. UCU-09 owns exact
  source identities, reviewed reuse and results. Final window gates and
  exact-SHA CI remain separate; approved platform debt is unchanged.
  Continue at row194 content-duration application acceptance.

## Row 192 source review and acceptance

Accepted slice: `20c81e63`, upstream unsigned macOS CI. N/A after reviewing
the complete ten-file delta: pass `CODE_SIGNING_ALLOWED=NO` through the Xcode
bundle script for unsigned builds, run packaging for affected pull requests,
and trim upstream workflow/script comments. ErsatzRS does not invoke that
Xcode project or bundle/sign script and retains its own CI/release workflow.
Independent substantive review confirms there is no broken Rust counterpart.
UCU-09 owns exact diff/workflow hashes. Approved macOS application/runtime
debt remains explicit; no native proof, new package build or release is claimed.
Continue at row193, troubleshooting playback seek/readiness.

## Row 191 source review and acceptance

Accepted slice: `fdf3b13c`, upstream manual develop publication. The complete
six-file diff extracts upstream prerelease tagging, separates unsigned CI
artifact/image builds from signed publication, and adds a manually dispatched
develop release workflow. N/A: ErsatzRS already has separate verification and
tag-triggered release workflows, with explicit stable/beta publication gates.
There is no matching develop-tag script, .NET artifact workflow or signing
environment failure to correct. Independent substantive review confirms the
classification; no product, packaging or publication policy is changed.
UCU-09 owns exact source/workflow hashes. Continue at row192.

## Row 190 source review and acceptance

Accepted slice: `c13ef9c5`, upstream macOS bundled FFmpeg8.1.2-4,
one-time saved-path adoption, bundled-series health warnings and package/CI
changes. Adapt under ADRs 0008/0063/0117: retain separate managed FFmpeg9,
explicit operator overrides, checksum/capability validation before promotion,
and validated PATH9 fallback with visible acquisition warnings when no managed
RID asset is available. Do not adopt the upstream FFmpeg8 downgrade or replace
saved operator paths. All15 upstream package patches are already carried,
equivalent or inapplicable under current dependency pins; reuse v2.0.6.

Exact source hashes remain unchanged at the accepted row189 checkpoint;
16 runtime-policy regressions and independent substantive review pass.
UCU-09 owns exact diff/source receipts and verification. macOS application
and runtime proof remain approved explicit unverified debt; no first-party
macOS FFmpeg asset, native acceptance, new build or publication is claimed.
Continue at row191; final window gates and exact-SHA CI remain open.

## Row 189 source map and acceptance

Accepted slice: `481840fb`, songs through Next and deterministic song backgrounds,
adapted under existing ADRs 0005/0102/0105. Preserve selected audio/subtitles, programme timing and
target-channel resolution, including mirrors. Render song metadata and artwork
through the existing native FFmpeg boundary; no additional graphics service,
FFmpeg package build, schedule reset or new platform deferral is included.

- Upstream conversion: `ErsatzTV.Infrastructure/Scheduling/PlayoutItemConverter.cs`
  and its tests; Song versions participate in the existing query extensions.
  Audio-only inputs receive generated song pictures or finite black video;
  embedded cover art is not an actual video programme. Unknown stream metadata
  retains ordinary source probing and its silence fallback.
- Upstream rendering: `ErsatzTV.Core/FFmpeg/SongVideoGenerator.cs`, ordinary
  playback and troubleshooting handlers. Ordinary visuals use the playout-item
  seed; the requested playback channel supplies the profile for mirror output.
  Preserve background/layout choices, text, cover-art and BlurHash treatment.
- Upstream callback: `GetSongVideoBackgroundByPlayoutItemIdHandler` and
  `InternalController`. Adapt transport authentication to the existing private
  listener, process capability and exact occurrence/session admission.
- Rust selection: `next_engine/selection.rs`, protocol probe hints and graphics
  content geometry. Bind generated-picture publication after ordinary track
  selection and retain the original source for audio and subtitles.
- Rust rendering: shared infra song preparation, native playback adapters and
  the existing FFmpeg progress filter. Decode BlurHash with the maintained Rust
  parser in infra; FFmpeg continues to compose and encode the picture.
- Rust lifecycle: Next song callback, existing session transfers and supervised
  prepared inputs. Validate occurrence, target geometry and session before and
  after preparation; cancellation must reap processes before removing assets.
- Intentional deviations: existing private-listener/canvas ownership decisions
  apply. Upstream Next omits the progress bar for WithProgress songs; native
  playback must preserve the bar rather than inherit that Next limitation.

Passing evidence: stream/track-pin and attached-picture regressions; stable
native/Next song visuals, metadata/artwork and mirror geometry; decoded available
Linux/Windows song playback, seeks and handoff; private-only callback admission,
transient preview isolation, cancellation and owned asset/process cleanup.
Linux and native Windows runtime and actual private HTTP checks pass with the
published, hash-verified FFmpeg v2.0.6 and Next0362692 packages. UCU-09 owns
the frozen source identities, test runs, independent review and lint evidence.
Continue at row190. Final window playback, full local gates and exact-SHA CI
remain separate gates; existing approved platform/hardware debt is unchanged.

## Row 188 source review and acceptance

Accepted slice: `26755a16`, one upstream changelog line announcing a Next QSV
loudness/canvas memory correction. The application delta contains no code,
dependency, packaging or protocol change; the row is N/A. Published and pinned
Next0362692 already includes upstream separate-audio-input and bounded-shortest
fixes. Exact source/patch/archive provenance and existing QSV argument tests
prove inclusion; Intel QSV execution remains approved unverified hardware debt.
UCU-09 owns the commit identities, reused check results and independent review.
Row189 songs/Next integration is next; final window gates and CI remain open.

## Row 187 source map and acceptance

Accepted slice: `e18b4a07`, block scheduling/history correctness. The outcome is
continuous media rotation when rebuilding changed block collections, without
stale future cursor rows causing skips or repeats. Preserve already consumed
history and the retained current programme under ADR 0110. No schema, UI,
FFmpeg/Next runtime, health capability or new scheduling policy is included.

- Upstream: `ErsatzTV.Core/Scheduling/BlockScheduling/BlockPlayoutBuilder.cs`,
  `BlockPlayoutChangeDetection.cs`, and the production-builder regressions in
  `ErsatzTV.Core.Tests/Scheduling/BlockScheduling/BlockPlayoutRebuildHistoryTests.cs`.
- Rust: `crates/ersatzrs-core/src/scheduling/block.rs` removes orphaned future
  history before `block_sources.rs` restores cursors. Existing set-based removal
  already covers all rows at deleted item starts, and source/deco restoration
  already filters history queued for removal.
- Persistence: the existing build result and scheduling repository transaction
  remove exact history IDs. SQLite reopen/Continue/Refresh tests belong in
  `crates/ersatzrs-infra/tests/scheduling_repository/block_continuation.rs`.
- Boundary: identify orphaned future history against the original requested
  clock before programme-preservation state is restored, then repeat against
  retained items after change detection. Keep pre-request consumed history
  even when its played item has been pruned. Retain the current programme and
  its matched checkpoint; the protected seam remains governed by ADR 0110.
  Publish exact removed history IDs even when preserving Refresh clears only
  the future beyond that seam. Preroll shifts remove every old-start duplicate.
- Intentional deviations: retain existing ADR 0110 programme preservation and
  persisted deterministic enumerator checkpoints; no new deviation is proposed.

Acceptance: mixed-duration repeated Shuffle references and collection additions
retain consecutive cursor indexes; orphaned future and duplicate removed-start
history are deleted; past consumed history remains. Persist production-generated
state, inject stale future history, reopen SQLite and compare complete future
media/timestamps/context/settings against the clean oracle for Continue/Refresh.
Reuse existing five-order, membership, current-programme and health regressions.
All33 core Block checks and10 selected SQLite/publication checks pass,
including both new paired-control cases after correcting a fixture ID collision.
Strict affected lint and independent substantive review pass. Row187 is Port;
UCU-09 owns exact evidence. Row188 is next. Final UCU-09 playback, closeout
and window CI remain separate required gates.

## Row 186 source map and acceptance

Accepted slice: `b4a7efe2`, image/remote-stream playout duration and Next input
hints. Use image metadata duration (15 seconds when unset), otherwise the
selected/head media version duration, with RemoteStream.Duration as the fallback
only for a zero version duration. Preserve fractional image duration, explicit
track pins, finite/live source policy and ordinary video behavior. No schedule
reset, data migration or later ledger row is included. The accepted ADR 0117
now authorizes the necessary Next correction and verified packages; its accepted
FFmpeg extension supplies the published checksum-verified v2.0.6 runtime.
The reviewed six-platform next-0362692-ersatzrs.1 snapshot is now published and
pinned, with successful Linux/Windows optimized executable compatibility and
both macOS package jobs. The independent unpatched comparison still requires
both local corrections. Production HTTP smoke now passes with the independently
downloaded release executable, selected Dutch audio and rendered captions,
normal/preview playback, reconnect and idle cleanup.

- Upstream: `MediaItemExtensions`, troubleshooting preparation, chapterless
  filler conversion in `PlayoutBuilder`, `DynamicPlayoutItemService`, and
  `PlayoutItemConverter` plus its image/AVI hint tests.
- Rust: scheduling repository candidate duration feeds both chapterless fillers
  and demand-time fallback. Troubleshooting preview reads and persisted Next
  probe hints need the same duration rule. Retained/one-shot hints continue to
  use their fresh source metadata and independent duration policy.
- Next image formats: PNG, JPEG, BMP, TIFF, WebP and GIF; AVI extension is
  case-insensitive. Unknown image codecs retain ordinary probing.
- SQL invariant: one logical media/version row; image metadata and media-file
  selection use indexed, deterministic first-row lookups, never multiplying
  candidates. Existing persisted playout items are not rewritten.

Acceptance: duration matrices (image default/fractional/zero; remote fallback
versus nonzero version; multiple versions/metadata), scheduling and fallback
bounds, troubleshooting seek/duration, all image/AVI wire hints, actual decoded
image and AVI playback using the published runtime, strict affected lint and
production smoke. Row186 is accepted as Adapt under ADR 0117. UCU-09 owns
exact test and runtime evidence; the baseline advances with this checkpoint.
Next action: row187 block scheduling/history correctness. Rows194 and 207
remain pending despite their independently reviewed implementation prerequisites.

## Row 185 source map and acceptance

Active slice: `e1d2cac0`, copy-mode cleanup. The outcome is faithful copied
video without unwanted graphics/transcoding or incorrect fixed resolution,
with fresh Next projections after relevant profile edits. No scheduler reset,
new settings, runtime rebuild or later ledger row is included.

- Upstream handlers: `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Commands/UpdateChannelHandler.cs`
  and `FFmpegProfiles/Commands/UpdateFFmpegProfileHandler.cs`.
- Upstream playlist: `Streaming/Commands/StartFFmpegNextSessionHandler.cs`.
- Upstream projection/tests: `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Scheduling/PlayoutItemConverter.cs`
  and `ErsatzTV.Infrastructure.Tests/Scheduling/PlayoutItemConverterTests.cs`.
- Rust channel saves already notify normal/preview Next consumers without
  rebuilding persisted playouts (`admin/server_fns/channels.rs`). Prove a
  profile reassignment through that existing route.
- Rust profile commands in `ersatzrs-app` must notify through the existing
  channel runtime port after a copy-video or resolution change; the native
  host selects only channels referencing the saved profile. Both editor and
  machine API paths require the same behavior.
- Rust `next_engine/graphics.rs` must suppress overlays using the effective
  playback profile across host, dynamic, fallback, remote and troubleshooting
  projection. Preserve subtitle/audio selection and non-copy rendering.
- Rust `transport/iptv/variant.rs` must omit fixed RESOLUTION for copied video.

Acceptance: focused save/notification and unchanged-schedule tests, complete
projection-path coverage, normal/preview playlist tests, decoded published
Next copy output with configured graphics, strict affected lint and production
HTTP smoke. Reuse row184 artifact and compatibility evidence where unchanged.
Row185 is accepted as Port. Focused notification/projection/playlist checks,
decoded copy and normal-graphics playback, strict affected lint and production
Convert HTTP smoke pass. UCU-09 owns the evidence in `docs/progress/milestones.toml`.
The baseline advances to `e1d2cac0`; row186 is next.

## Row 184 source map and acceptance preparation

The 2026-10-06 Codex review starts with upstream `aad7bdc4` and preserves
all accepted rows through `960421e8`. Row 184 is now accepted at `aad7bdc4`.

- Upstream conversion: `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/ChannelConfigConverter.cs`.
- Upstream wire schema: `$ERSATZTV_SRC/ErsatzTV.Core/Next/Config/ChannelConfig.cs`.
- Upstream regressions: `$ERSATZTV_SRC/ErsatzTV.Tests/Streaming/ChannelConfigConverterTests.cs`.
- Rust wire and mapping: `crates/ersatzrs-infra/src/next_engine/protocol.rs`,
  `config.rs` and `config_tests.rs` in the same directory.
- Runtime distribution: `scripts/release/stage-next.py`; its previously pinned
  `4ad282f` source predates Next copy-mode/schema support. Next introduced the
  typed copy policy, per-item blockers and channel configuration in commits
  `0ec1ac0`, `38d035e` and `09d8289`. Upstream currently publishes `e88b2d7`
  packages for five supported architectures, but no ARMv7 package.
- Runtime/health and callers must be audited before changing the pin. A newer
  executable is not accepted merely because it parses the configuration.

Required behavior: emit the channel schema version; select copy/transcode
independently for audio and video; retain AAC/H.264 fallback configuration for
copy profiles; suppress loudness normalization for audio copy, as upstream
explicitly does. The converter does not populate copy_formats, so engine
codec defaults apply. Unsupported source codecs and copy blockers still need
transcoding. Preserve the existing supported-codec refusals outside Copy.

Acceptance must include serialization and mixed-mode conversion regressions,
decoded compatible-source copy, incompatible-source fallback, graphics and
subtitle burn fallback, selected alternate audio and captions, and normal
transcode behavior through the production Next path. Runtime pin, package
checksums, platform provenance and existing approved hardware/platform debt
must remain explicit. This row does not change the default engine, process
later ledger rows, or advance the baseline before acceptance.

### Candidate runtime regression (2026-10-06)

The copy-mode and fallback matrices pass on Linux; the exact Windows candidate
also preserves native copy geometry, sample rate, channel count and selected
880 Hz audio. The subsequent native Windows Rust application run passes all
six tests for exact copy payloads, codec fallback, graphics, mixed modes,
burned captions, JavaScript-selected audio and ordinary HLS
(`54c42a27-43d6-4aa8-8e8e-845fe3939640`;
`tmp/ucu09-row184/windows-application.log`). The full Linux Next compatibility
selection ran 328 tests: 322 passed initially. Three failures were missing fixture FFmpeg resolver configuration;
two were old unversioned overlay fixtures. Those five pass after fixture fixes,
and the added version-boundary health checks pass.

One real regression remains: the RTSP source-clock/alternate-audio test fails
its unchanged 30-second startup limit with candidate `e88b2d7`. The same input
and published FFmpeg payload pass with accepted Next `4ad282f` (27.1–27.3 seconds
for the complete HTTP/RTSP matrix). Upstream `4138a67` added unconditional
`-shortest` to audio-transcoding pipelines. Diagnostic-only wrappers establish
that removing that option restores the test (27.5 seconds), and retaining it
with `-shortest_buf_duration 0.5` also passes (31.6 seconds for the full matrix;
the RTSP startup itself remains within 30 seconds). This does not constitute
acceptance of a wrapper or a patched distribution. Test timeouts, source GOP,
product behavior and the accepted runtime pin remain unchanged.

Evidence: `tmp/ucu09-row184/next-compatibility.log`,
`compatibility-focused.log`, `compatibility-diagnostic.log`,
`rtsp-old-runtime-comparison.log`, `rtsp-old-diagnostic.log`,
`rtsp-shortest-diagnostic.log` and `rtsp-small-buffer-diagnostic.log` in that
same directory. The RTSP fixture now retains its synthetic logs on failure.
The versioned overlays require an explicit upgrade because legacy
`format: null` copy semantics changed; health and configuration documentation
explain the boundary.

### Approved temporary correction (2026-10-06)

The maintainer accepted ADR 0116: a source patch bounds shortest buffering to
0.5 seconds only for live RTSP video inputs. Existing QSV and audio-copy behavior
remain intact. The patch must be removed when equivalent upstream behavior
passes the retained startup and EOF/handoff regressions. The source preparation
manifest rejects a new revision without a fresh unpatched regression review.

All 220 upstream pipeline unit tests pass. The actual patched Linux executable
(`0.1.0-e88b2d7-ersatzrs.1`, SHA256
`b06c89ad83ab19043a9543ba3a2110da3858c05d27647ecf8c48eeb78c67e517`)
passes all 328 Next compatibility checks, including the original RTSP case at
27.764 seconds. The additional decoded RTSP-to-file handoff/EOF regression
passes: selected 880 Hz live audio changes to 440 Hz file audio, followed by
silence padded only through video EOF; programme timestamps remain increasing
and output terminates at the expected duration. Run
`34e72437-2621-42cb-9870-d4305a66f776`; logs
`tmp/ucu09-row184/patched-next-compatibility.log`, `patched-next-endings.log`
and `next-patch-unit.log`. Strict infra test-target Clippy passes.

The Windows patched binary also passes all six native application checks
(`4b14a195-3591-45da-a120-72235c5dd4a5`,
`tmp/ucu09-row184/patched-windows-application.log`); executable SHA256
`039b26a1503bf32c226289ad84d58ce45efcd467265f72475791b883a52518c2`.
Linux x64, ARMv7, ARM64 and Windows candidate archives are built and their
archive/payload hashes verified under `tmp/ucu09-row184/patched-packages/`.
All three Linux executables require GLIBC 2.34. Native ARM playback remains
unverified. Source preparation and packaging have eight passing offline
identity, retirement, companion and archive regression tests. Twenty staging
regressions also pass, including all six RIDs, rejection of altered source/patch
provenance and refusal of unpatched staging paths while a patched pin is active.
The checked-in Linux reproduction recipe reproduces the accepted ARMv7 payload
byte-for-byte and enforces the GLIBC baseline. Packaging
checkpoint `bfa2c5be7ecda379aaa16ffc1739cf0f7d5a30e1` is pushed only to
`build/next-rtsp-candidate`. Its candidate-only macOS
[run 37503854054](https://github.com/bartdeijkers/ErsatzRS/actions/runs/37503854054)
is terminal success for both architectures, including native version startup,
ad-hoc signature verification and packaging. Independently downloaded macOS
archives, source identity, patch identity and payload checksums match. This is
not native macOS application playback acceptance.

The dependency snapshot [next-e88b2d7-ersatzrs.1](https://github.com/bartdeijkers/ErsatzRS/releases/tag/next-e88b2d7-ersatzrs.1)
is published (release ID `404996006`), with all twelve asset hashes and sizes
verified after publication. All six packages stage through the production CLI;
the production online downloader also verifies the published Linux package.
Twenty-one staging regressions pass, including the manifest/pin agreement.
The rebuilt Windows executable is unsigned; macOS binaries have ad-hoc loader
signatures, with no upstream publisher identity or Apple notarization claimed.

Production Next Burn and Convert HTTP/HLS lifecycle smoke passes: selected
Dutch audio, burned captions/WebVTT, preview, wrapper disconnect, idle cleanup
and restart. The overlay health message passes all nine languages and strict
web-library Clippy. Next copy transport acceptance passes 83 TSDuck checks
across programme transitions, with zero unmarked continuity/timestamp errors
and a maximum decoded audio-pip deviation of 20 ms. The gate now registers its
synthetic local library and retains HLS discontinuity boundaries under RFC 8216
section 3. Five checker regressions prove that unmarked resets and packet loss
inside a declared group still fail. Raw segment bytes and thresholds are unchanged.
Evidence under `tmp/ucu09-row184/`: `patched-next-http-smoke.log`,
`patched-next-convert-smoke.log`, `overlay-localization.log`,
`clippy-web-localization.log`, `patched-next-copy-tsduck-groups.log`,
`staging-final.log`, and `published-release-metadata.json`.

Source and failure-path review confirms independent stream modes, retained
codec fallback, copy-audio loudness suppression, explicit overlay upgrade,
exact archive/source/patch/payload identity and refusal of unpatched staging
under the patched pin. Row 184 is accepted as Adapt (ADR 0116); baseline and
upstream checkout advance to `aad7bdc4`. Row 185 is next. Patch retirement
remains an explicit maintainer-approved obligation. Application window CI and
stable-release acceptance remain separate and are not claimed by this checkpoint.
Release/recovery staging derives the dependency tag from the checked-out source;
older recovery sources continue to require their accepted snapshot.

The authorized package cleanup removed 12 duplicate archives and 23 obsolete
or duplicate extracted payloads (257 MiB total), preserving provenance and
test logs. The accepted six-platform `4ad282f` snapshot, release recovery
packages and new candidate remain. GitHub retains the still-used published
`next-4ad282f` dependency release alongside the new accepted snapshot; it cannot be removed until its consumers and
recovery requirements are resolved. Cleanup manifests are
`tmp/ucu09-row184/next-package-cleanup.json` and
`next-extracted-cleanup.json` in the same directory. After publication, another
133.9 MiB of verified duplicate download/staging files was removed (about
391 MiB total); source archives, accepted payloads and copied staging provenance
remain. See `post-publication-cleanup.json`.

## Latest upstream target and state

- Initial pin: `0d301df5e8512fdfccf3a611188d27ab1690ee31` (row 0; no upstream commit processed).
- Latest fetched target (`origin/main`, 2026-10-06): `46118d43313b7828780fbfcfb46e980390ea8b0d`
  — docs: prep for release v26.11.1.
- Ledger: **208 commits**, **0 pending**. The original plan covered 152 through
  `495e3799`; the maintainer expanded the target to the latest upstream commit
  on 2026-09-19, so the ten newer commits are also pending in this ledger. The 2026-09-22
  refresh adds six more commits; a later refresh adds Next engine rotation.
  A subsequent refresh adds the AMD VAAPI default-driver fix. The subtitle
  correction audit appends graphics-subtitle performance and Jellyfin 12 collection
  handling as two pending tail rows; neither is processed by the row-39 audit.
  The row-80 refresh appends watermark improvements as pending row 173.
  The 2026-09-26 refresh appends Next engine subtitles as pending row 174.
  A subsequent refresh appends MPEG-TS script line endings as pending row 175;
  all 174 existing SHA/subject pairs remain an exact prefix.
  The latest refresh appends Windows libvpl packaging as pending row 176.
  The row-130 refresh appends ARMv7/FFmpeg packaging and Docker quoting as
  rows 177-178; all 176 existing SHA/subject pairs remain an exact prefix.
  The 2026-10-04 refresh appends Next copy mode, its cleanup, image and remote
  stream durations, block playout skips and repeats, an upstream changelog line
  and Next songs as pending rows 184-189; all 183 existing SHA/subject pairs
  remain an exact prefix.
  The 2026-10-05 refresh appends macOS bundled FFmpeg, two upstream workflow
  commits, the troubleshooting seek point, local library duration and the local
  deep scan as pending rows 190-195; all 189 existing SHA/subject pairs remain
  an exact prefix.
  A later 2026-10-05 refresh appends the Windows artifact size build change,
  unextracted text subtitles on Next and streaming-mode coercion as pending rows
  196-198; all 195 existing SHA/subject pairs remain an exact prefix.
  The 2026-10-06 refresh appends the Next output frame rate, Next by default,
  Next channel troubleshooting, tag-triggered releases, upstream release
  v26.11.0, Windows Next playout folders and two changelog commits as pending
  rows 199-206; all 198 existing SHA/subject pairs remain an exact prefix.
  A later 2026-10-06 refresh appends an upstream changelog update and release
  v26.11.1 as pending rows 207-208; all 206 existing SHA/subject pairs remain an
  exact prefix.
  The 2026-10-06 Codex audit corrected reversed pending rows 193/194 to
  match upstream history; all 183 accepted rows remain unchanged. All 206
  SHA/subject pairs now follow the fetched upstream sequence.
- Refresh upstream refs and verify the fixed v26.11.1 release target. Newly
  observed post-release commits remain outside UCU-09. A bootstrap still
  processes no upstream commit.
- Ledger order comes from
  `git -C "$ERSATZTV_SRC" log --reverse --abbrev=8 --format='%h|%s' <initial-pin>..<target>`.
- Processed rows: **208**; in-scope total: **208**. Empty verdict/owner/ErsatzRS-commit cells mean pending.
- UCU-03 is Complete with terminal green CI at `8f722099`. UCU-04 is Complete:
  rows 35-67, local window gates and terminal green exact-SHA CI run
  [36140800461](https://github.com/bartdeijkers/ErsatzRS/actions/runs/36140800461)
  at `a14ada80` are recorded in its milestone. The earlier Clippy-policy and
  Firefox failures were corrected without changing product logic. UCU-05 has
  completed rows 68-90, local window gates and terminal green exact-SHA CI
  [36190815587](https://github.com/bartdeijkers/ErsatzRS/actions/runs/36190815587)
  at `81bcd649`. UCU-06 is Complete: rows 91-109 are reconciled, local window
  gates pass, and correction `0e482f66` has terminal green exact-SHA CI
  [36274614736](https://github.com/bartdeijkers/ErsatzRS/actions/runs/36274614736).
  Its milestone owns the runtime publication, browser correction and decoded
  audio/subtitle evidence. UCU-07 row 110 is Adapt under accepted ADR 0102;
  row 111 is Port (Plex reauthentication and retained missing sources); row 112
  is Adapt under accepted ADR 0103 (machine API key and browser route separation).
  Row 113 is Covered (Next mirror build notifications); row 114 is Adapt
  (internal media authentication under ADRs 0095/0102); row 115 is N/A release
  bookkeeping. UCU-07 is Complete: local window gates and terminal green CI
  [36321494841](https://github.com/bartdeijkers/ErsatzRS/actions/runs/36321494841)
  at `b4d659eb` satisfy acceptance. UCU-08 row 116 is N/A release-note wording;
  row 117 is Port (safe block cleanup cutoff); row 118 is N/A (empty build trigger).
  Row 119 is N/A (upstream Next release note); row 120 is Port (persisted YAML
  shuffle order). Row 121 is Covered (subtitle query/seek separation).
  Rows 122-124 and 126-127 are Port; row 125 is Adapt (ADR 0103).
  Row 128 is N/A (upstream NUnit fixture deadline); row 129 is Adapt (accepted ADR 0104); row 130 is Port (startup ownership/deadlines); row 131 is Port (multi-day timing and bounded interlace sampling); row 132 is Port (identity-safe concurrent startup); row 133 is Port (retry failed local folders); row 134 is Port (provider season repair and artwork fallback); row 135 is N/A (upstream release bookkeeping). UCU-08 is Complete: all local gates and terminal green exact-SHA CI [36453480304](https://github.com/bartdeijkers/ErsatzRS/actions/runs/36453480304) at `dfeaf6d0` pass. UCU-09 row 136 is Port (shared bounded health runs and cached/explicit refresh). Row 137 is Port (artwork-cache pruning and healing). Row 138 is Covered (call-local hashing). Row 139 is Adapt (native/Next sessions API under ADRs 0095/0103). Row 140 is Port (published v2.0.3 and approved hardware debt). Row 141 is N/A (release notes). Row 142 is N/A (empty build trigger). Row 143 is Port (HDR refresh and Media Info). Row 144 is N/A (Next optimization release notes). Row 145 is Port (external JSON playback and validation). Row 146 is Adapt (accepted ADR 0105 graphics canvas). Row 147 is Port (persisted YAML roll/graphics state). Row 148 is Port (provider subtitles and Next source availability). Row 149 is Port (Plex Other Video folder tags). Row 150 is Port (Next fallback error cards). Row 151 is Port (both-engine music-video credits troubleshooting). Row 152 is N/A (upstream Next AMF release notes). Row 153 is N/A (.NET dependency bumps; AAC surround already covered). Row 154 is Adapt (ADRs 0005/0105; motion boundaries and preview clocks). Row 155 is Port (optional Next AMF adapter protocol; verified runtime reused). Row 156 is Port (typed machine CRUD and safe profile deletion; localized island callbacks). Row 157 is Port (native completion, bounded sample cleanup, URI redaction and smart conflicts). Row 158 is Covered (atomic operation admission, fenced Plex identity and retired Trakt). Row 159 is Port (isolated maintenance failures and cancellation). Row 160 is Port (movie ordering, save recovery and scoped legacy repair). Row 161 is Covered (FFmpeg font ownership; no shared mapper). Row 162 is Port (fresh MPEG-TS script choices, selection recovery and no-JS settings). Row 163 is Port (shared script validation/execution and Lua arguments). Row 164 is Port (dependent selection clearing, saved display and stable template hydration; four browser cases). Row 165 is Covered (credits fallback, typed HLS errors and graphics/process ownership; 18 checks). Row 166 is Covered (case-insensitive YAML discovery and scanner metadata/subtitle proof). Row 167 is Port (native path validation, retained media identity and localized correction forms; eleven production, two Windows and eight browser checks). Row 168 is N/A (release heading/date/links only). Row 169 is Port (provider rotation, Media Info, Next 0.0.5 and corrected orientation health; decoded/runtime/browser proof). Row 170 is Port (Default VAAPI driver omission, explicit choices retained; 14 checks and strict lint). Row 171 is Covered (ADRs 0005/0105; native libass and encoded Next canvas). Row 172 is Port (Jellyfin query/type guards and retained-identity season repair proof; 47 checks and strict lint). Row 173 is Port (direct plain watermark layers, verified RGB opacity and Next smoke). Row 174 is Port (both-engine automatic subtitle eligibility, decoded sidecar fallback and Next smoke). Row 175 is Port (CRLF batch checkout, LF shell scripts; isolated Git checkout proof). Row 176 is Port (Windows oneVPL companions; six-RID regressions and native package/startup proof). Row 177 is Adapt (ADRs 0008/0063; published v2.0.3, verified hashes and acquisition checks; ARMv7 retained). Row 178 is N/A (upstream-only Docker workflow quote repair). Row 179 is N/A (upstream develop-release publication policy and tooling). Row 180 is Port (development notes from this build; exact tagged notes retained; browser proof). Row 181 is Port (published v2.0.4 QSV/oneVPL corrections). Row 182 is Port (Next MPEG-2 output). Row 183 is Adapt (accepted ADR 0106; native AV1 fMP4 and transport safety). Continue at row 184. All local window gates pass; authorized push and exact-SHA CI remain. Existing debt
  and platform limitations remain unchanged.
- [Pin](../docs/automation/parity-baseline.md) is authoritative, followed by
  this ledger, followed by upstream checkout HEAD. All must agree before work.
  The [parity record](../docs/progress/parity/upstream-baseline-catch-up.toml)
  mirrors the target and backlog; a mismatch there is also a disagreement.
- [Milestones](../docs/progress/milestones.toml) own acceptance and evidence.
  AMM-N-03 remains a prerequisite; setup does not waive native acceptance.
- [ADR 0095](../docs/adr/0095-native-pipeline-instead-of-ersatztv-next-engine.md)
  is Accepted for Next as an optional second engine alongside the native
  pipeline. The rejected native-only proposal cannot justify exclusions.


## Row 183 source map: AV1 profile removal

- Upstream `960421e8`: FFmpegEditor removes the AV1 select item; validator
  removes AV1 from QSV/NVENC/VAAPI choices; create/update handlers coerce AV1
  to HEVC; both database migrations rewrite VideoFormat 4 to 2. Generated EF
  designer files are schema snapshots. Changelog explains the loss of fMP4.
- Rust counterparts: admin FFmpeg profile editor and server functions,
  persisted profile format, native pipeline, HLS routing/worker, Next config,
  codec signaling and output health. Native `segmenter-fmp4` remains available;
  Next rejects AV1 explicitly. Existing persisted profiles are retained.
- Existing editor geometry and localized codec choices are the visual baseline.
  If choices or validation change, verify hydrated and no-JS profile behavior.
- Accepted ADR 0106 (2026-10-01) retains compatible native AV1 with transport
  checks and decoded application proof. Row acceptance is complete; the pin
  and accepted reference checkout advance to this row. UCU-09 owns exact proof.
- Health UI source map: `/system/health`, SSR HealthPage and HealthIsland;
  persisted health results via load_health_snapshot/RefreshHealthSnapshot,
  existing no-JS refresh form. New text uses TextKey and every culture file;
  table/card geometry and primitives remain unchanged. Browser proof targets
  actionable English/Dutch guidance with JavaScript enabled and disabled.
- Native transcoded HLS selects fMP4 for AV1. TS routes and the final command
  builder reject AV1 MPEG-TS output, including copied AV1; AV1 input decoding
  to H.264 remains supported. Next keeps its explicit AV1 rejection. AV1
  playlist CODECS is omitted until its actual level/tier/bit depth is known.

## Row 182 source map: Next MPEG-2 output

- Upstream `b6666882`: `ChannelConfigConverter` maps MPEG-2 and forces 8-bit;
  `Core/Next/Config/ChannelConfig` adds the `mpeg2video` wire value;
  `StartFFmpegNextSessionHandler` emits CODECS only when both codecs are known.
- Rust targets: `next_engine/config.rs`, `protocol.rs` and their regressions;
  shared `transport/iptv/variant.rs` playlist codec projection. Audit callers,
  health and validation before implementation so an accepted profile does not
  get rejected elsewhere.
- Preserve explicit rejection of other unsupported codecs, HDR policy, selected
  audio/subtitles, and existing H.264/HEVC bit depth. Verify actual decoded
  software MPEG-2 output with the published runtime and official Next 6b49dae.
- No UI layout change or database migration is indicated by the source delta.
  Hardware claims require evidence; existing approved debt is not expanded.
- Ordered implementation and local row acceptance are complete. Source diff,
  focused/runtime proof and history checks are in `tmp/ucu09-row182/`; the
  milestone owns final window and platform acceptance.

## Row 181 source map: legacy QSV corrections

- `76884783` adopts ErsatzTV-ffmpeg `8.1.2-2`, updates bundled revision health,
  Windows checksum and Docker digest, and documents new runtime/Next fixes.
- Exact release comparison adds FFmpeg patches 0013/0014/0015: retain fixed
  decoder pools on compatible stream-parameter changes; initialize individual
  D3D11 texture indices without overwriting single-handle VAAPI/D3D9 callers;
  transfer D3D11-backed QSV frames through the child context.
- All three patches apply to exact FFmpeg n9.0 without fuzz or offsets. Existing
  ErsatzRS QSV timestamp correction remains compatible (ten-line offset only).
- The accompanying oneVPL dispatcher fix covers non-primary Intel adapters and
  missing runtime device IDs. It applies without fuzz/offsets to the published
  v2.0.3 dependency revision `d77f9195`; v2.0.4 now includes the patch.
- Implementation closure is recorded as `ucu09-row181-legacy-qsv-runtime`.
  Previously approved hardware-validation debt does not waive these corrections.
- Reuse existing Linux/Windows SVG dependency images by exact image ID; add the
  small dispatcher layer and rebuild only necessary runtime packages. Do not
  rebuild/publish the unrelated multi-platform Docker runtime. Preserve all
  published artifacts; publication of a replacement is a separate gate.
- Runtime v2.0.4 is now published from `657482cc` with exact tested draft/public
  archive checksums; the app's managed pin selects it locally. Required available
  runtime checks pass. Official Next 6b49dae passes scaling, audio/subtitles,
  direct watermarks and the production HTTP canvas preview. The stale infra-only
  image fixture is migrated to passing scheduled production-route coverage.
  Strict lint and Next HTTP smoke pass. Native Windows Next CUDA/NVENC, alpha,
  subtitles and selected audio pass. Milestone evidence owns detailed results;
  row181 is accepted and the baseline advances to `76884783`.

## Row 180 source and visual map: development release notes

- Full `8bda4411` audit: release tooling moves develop artifacts to a separate
  repository, changes version/tag calculation and release-note lookup; Docker
  health stops treating ARM suffixes as deprecated. Those repository/tag policies
  have no equivalent in ErsatzRS's current release process.
- Product gap: upstream `Index.razor` displays notes for the development build.
  ErsatzRS `admin/release_notes.rs` embeds its exact build's CHANGELOG, but its
  untagged selection chooses the latest released section. Select Unreleased for
  development builds; keep exact version selection for tagged releases.
- Routes `/` and `/system/health` reuse ReleaseNotesCard, SSR Resource/server
  function `load_release_notes`, existing sanitized Markdown and localized
  heading/full-changelog links. No database, validation or side effects.
- Visual map: retain the existing card header/body, link row, responsive CSS
  and SSR/no-JS structure. Only selected Markdown changes; no new strings or
  island primitives. Browser proof checks both routes and 320px width.
- Verify development/tagged selection and sanitizer regressions, focused browser
  output, strict web lint. Existing embedded changelog strategy stays in place;
  no GitHub client, new repository or deprecated Docker tag system is introduced.

## Row 179 source map: upstream immutable develop releases

- All seven files in `5d65c231` reviewed: release-note extraction, exact uploaded
  draft assets, retention pruning, serialized SHA-tagged develop publication,
  relative reusable workflows, branch protection for the develop image tag,
  signing environment, stable-tag selection and historical changelog link.
- These scripts operate on ErsatzTV's own releases, build matrix, signing and
  Docker channel. ErsatzRS does not publish a develop release on every main push.
  Its CI remains a verification gate; `.github/workflows/release.yml` owns
  tag-triggered product releases and beta drafts. `stage-beta-draft.ps1` refuses
  to replace published beta assets; exact candidate/native acceptance and
  maintainer publication authorization remain separate gates.
- No application API, playback, UI, data or health behavior changes. N/A;
  enabling new immutable-release policy or automatic retention/publication here
  would be new release infrastructure, not required product parity.
- No repository setting, remote release, tag, workflow dispatch or secret changed.

## Row 178 source map: Docker workflow quoting

- Upstream `f8410286` fixes two missing closing quotes in `.github/workflows/docker.yml`
  around per-architecture digest lists introduced by row177.
- ErsatzRS `.github/workflows/release.yml` uses `docker/bake-action` with
  `docker-bake.hcl` for one multi-platform image and quotes both arguments to
  `imagetools create`. It has no corresponding digest-list assignments.
- No product/runtime, UI, repository or platform behavior changes. N/A;
  row176's actionlint result covers the unchanged ErsatzRS workflow.

## Row 177 source map: immutable FFmpeg packages and platform policy

- Upstream `b1ed47db` changes nine files: native/PR/Docker platform matrices,
  Windows archive checksum, Docker image digest, FFmpeg version health and
  changelog. The adopted FFmpeg package is `8.1.2-1`.
- ErsatzRS retains all six supported targets, including ARMv7. Upstream's
  platform removal and libfdk_aac Docker distribution policy do not change
  ErsatzRS's accepted platform/runtime contracts.
- ADRs 0008/0063 own the deliberate counterpart: separate managed FFmpeg 9
  packages, SHA-256 verified before promotion, capability/version checks and
  runtime-acquisition health. Application Docker images do not bundle FFmpeg.
- `ersatzrs-ffmpeg::runtime` pins published v2.0.3; `version` enforces major 9;
  infra `health/checks/ffmpeg_version.rs` checks both tools. Full matching of
  upstream's `8.1.2-etv.N` version strings would reject the accepted runtime.
- The earlier approved expanded package work audited the adopted release's
  twelve patches. The published v2.0.3 payload already contains applicable
  hardware-frame, D3D11, QSV, AMF, pacing/export and librsvg corrections;
  row140 owns public hashes, decoded software/Windows NVIDIA acceptance and
  the approved Intel QSV, AMD AMF and native Linux NVIDIA debt.
- No UI, database, command generation, new dependency, build or publication.
  Reuse that runtime evidence, recheck package hashes and focused acquisition
  regressions. Source-only patch coverage does not close hardware debt.

## Row 176 source map: Windows Next oneVPL companions

- Upstream `6f0449c2` changes `.github/workflows/artifacts.yml` to move
  `libvpl.dll` and `libvpl-LICENSE.txt` from the same Next archive beside the
  Windows executable. No UI, handlers, database or health contract changes.
- ErsatzRS `scripts/release/stage-next.py` validates archive origin/digest and
  reads exact members; extend the Windows member set with both companions.
- Both Windows staging branches in `.github/workflows/release.yml` must move
  these files beside `ersatztv-channel.exe`. BUILD.md documents the layout.
- Verification: six-platform offline staging, missing/empty/duplicate/link
  companion rejection and exact official archive digest/member proof. Existing
  approved Intel QSV hardware acceptance debt remains; packaging proves no GPU
  behavior. No new FFmpeg build or publication is needed.
- Intentional deviations: none.

## Row 175 source map: MPEG-TS script line endings

- Upstream `04f49b0a` changes only `.gitattributes`: shell scripts use LF and
  batch scripts use CRLF. No UI, validators, handlers or repository queries change.
- ErsatzRS `.gitattributes` already supplies LF through its global rule. Add the
  batch-file override so clean checkouts provide CRLF on every build host.
- `host/bootstrap.rs` embeds `assets/Scripts/MpegTs/run.bat` as bytes; release
  workflows check out the repository before building and copying assets.
- Scope: checkout bytes only; script commands, PowerShell and CMD launcher policy,
  custom script preservation, health capabilities and platform support stay as-is.
- Verify effective attributes and a real isolated Git checkout with both
  `core.autocrlf=false` and `true`, including unchanged normalized repository blobs.
  No Rust or browser behavior changes require a new build for this row.
- Intentional deviations: none.

## FFmpeg runtime look-ahead audit (2026-09-28; no row acceptance)

**Published follow-up:** expanded Linux/Windows v2.0.3 packages, including
librsvg, are published with independently verified public archive checksums.
UCU-09 owns exact source, release and runtime evidence. SVG alpha metadata and
Linux Vulkan export coverage are corrected; existing codecs/filters remain.
The maintainer explicitly approved Intel QSV, AMD AMF and native Linux NVIDIA
interop hardware acceptance debt on 2026-09-28. Those paths remain unverified.
Row140 integration verification passes; the baseline advances with its Port verdict.
The inventory below records the original v2.0.2 audit baseline.

Requested by the maintainer before considering another runtime build. Refreshed
legacy origin/main remains `8bda4411`; reviewed remaining rows140-180 and the
12-patch inventory of the exact `ErsatzTV-ffmpeg` release `8.1.2-1` adopted by
row177. This audit does not advance the ledger. The maintainer subsequently
approved the expanded Linux/Windows runtime build, explicitly including
librsvg; publication remains a separate gate. Only NVIDIA hardware is available
on the build/test PC, so Intel QSV and AMD AMF hardware acceptance remain open.

- Row140: QSV padding needs patches0008-0010 (timestamp, padding, colour/chroma).
- Row155: AMF MPEG-2 decoding needs patch0011 adapted to FFmpeg9 registration
  context. The v2.0.2 Linux decoder list lacks mpeg2_amf/vc1_amf. Exact Next
  `4cc93e4` selects MPEG-2 only when available and deliberately excludes VC-1
  because its AMF timestamps reorder incorrectly; do not enable VC-1 selection.
- Row177's adopted package also contains missing hardware frame-size handling
  (0001's hardware half; FFmpeg9 already fixes its software-pool half) and
  Windows D3D11/QSV texture/pool fixes0003-0004. The former requires a small
  FFmpeg9 adaptation; the latter apply with line offsets and zero fuzz. Hardware
  runtime acceptance remains required, including the D3D11 trade-off described
  by upstream; source applicability alone does not establish safety/parity.
- Already covered: slowest-stream pacing0006 in FFmpeg9, sparse-stream pacing0007
  in v2.0.2, and Vulkan/CUDA export0005 in the Windows package patch. The native
  Linux package and Docker do not install that Windows-specific patch; review
  their export behavior separately rather than claim universal coverage.
- Conditional rather than rebuild prerequisites: VAAPI alignment0002 bypasses
  an older-header guard, but the pinned native toolchain's libva commit
  `28388a09` exposes VA-API1.25 (guard threshold1.21). OpenAPV fix0012 targets
  APISET>=1; the cached toolchain pins `9f6fd2a7`, APISET0. Retaining dependencies
  does not require either fix for those documented compatibility problems.
- Row176's libvpl.dll/license is Next application packaging, not an FFmpeg patch.
  stage-next.py currently copies only ersatztv-channel; acceptance is still due.
- Release8.1.2-1 newly enables librsvg; v2.0.2 has no SVG decoder. This is a
  separate capability/dependency decision, not required to compile the above
  patches. Including it changes the dependency image and requires revisiting
  the cached-build estimate. ARMv7 removal is upstream policy, not authorized
  for ErsatzRS. Existing platform support remains.

Comparison used the affected FFmpeg n9.0 source files plus the exact v2.0.2
patches in disposable files, source manifests and local runtime capability
queries. No new hardware-output proof is claimed. The upstream AMF manifest
also reports limited unsupported-GPU validation; preserve fail-safe capability
selection and test actual MPEG-2 output before acceptance. The earlier 30-60
minute estimate covered package compilation/basic checks using cached
dependencies, not adapting this complete set or GPU acceptance.

## Row 174 source map — automatic subtitle eligibility (Port)

Complete `d7b265e07cbcfcf67dca69724f69349491b67ea9` changes the shared
FFmpegStreamSelector, four two-engine regression cases and changelog. Automatic
selection must respect Use Embedded Subtitles for both engines. Embedded text
requires extracted metadata plus a nonblank path unless using HLS Direct;
embedded images remain eligible. Apply filtering before preference selection
so an unusable embedded default cannot hide a usable sidecar.

Targets: persisted selection in `playback.rs` and shared eligibility/transient
selection in `playback_transient.rs`. Custom selectors use the shared candidate
filter. Keep explicitly pinned Next demand-time extraction and ADR 0100 refusal
semantics distinct from automatic preference fallback; the upstream change does
not alter explicit troubleshooting/pinned contracts. Existing native transient
preparation is caller-owned and remains unchanged. Both-engine setting, path,
bitmap, HLS Direct and sidecar-precedence checks pass. Decoded Burn/Convert
proves usable sidecars outrank an unextracted embedded default; explicit-pin
preparation retains seek/audio/off and authenticated/direct-HTTP behavior.
UCU-09 owns 308 affected checks (including corrected identity fixtures), runtime
results, strict lint and passing production Next HTTP smoke.

## Row 173 source map — direct plain watermark layers (Port)

Complete `c0184f000095c9801dcf9c238010d9f13ed0c104` changes the playout
converter, three selector regression cases and changelog. When graphics are
absent and all selected watermarks are Permanent/Intermittent, publish stable
Z-ordered Media layers; preserve image source/stream, position, source-content
placement, actual/scaled size, opacity and wall-clock periodic timing. Mixed
graphics or opacity-expression selection still uses one canvas.

The provisional Covered candidate was false: production always selected a canvas,
while `watermark.rs` was test-only. Shared projection now publishes explicit
Media layers for plain selections across host, dynamic/remote, fallback and
troubleshooting. Other selections keep ADR 0105 canvas ownership. Dynamic
fingerprints include the direct layers, and prepared/retained projection carries
them forward; no credentials or graphic lifetime responsibilities are discarded.

The exact accepted Next e7997fa ensures alpha before opacity and places image
format conversion before the loop used by intermittent fades. All 249 affected
Next regressions and three decoded runtime cases pass. The RGB opacity debt is
resolved by a bounded 50-percent red-over-blue assertion; permanent/dynamic,
ordered layers, disabled selection, periodic fades, audio and subtitles pass.
Strict infra lint and architecture/progress checks pass. UCU-09 owns detailed
proof; Next HTTP smoke passes. No runtime build/release or new hardware
acceptance claim is involved. Required final window gates remain.

## Row 172 source map — Jellyfin collection member identity (Port)

Complete `27f9d2174f377e27baa553872c73fcac863a52d7` changes all Jellyfin sorted
item queries, movie/show/season/episode/collection type projections, existing
season ShowId reconciliation and operator repair notes. Emby is unchanged.
Targets: `jellyfin/client.rs` three query builders and `projection.rs` five
projections now set collapseBoxSetItems=false and require exact expected Type.
`sync.rs` already upserts each season regardless of ETag; `persistence.rs`
updates its existing ShowId transactionally by provider item ID within library.
The production scan regression proves retained season/episode identity while
repairing the parent with unchanged ETag. Collection discovery counts, paging,
media tracks and Emby remain covered.
No migration or bulk live-library mutation is required. Tests use actual provider Type values and cover wrong/missing types and all
query paths. UCU-09 owns 47 passing checks, final repair proof and strict lint.

## Row 171 source map — subtitle compositor optimization (Covered)

Complete `62b7a5e9550e0f574efc3ca3007d8bd800988649` changes only upstream
SubtitleElement and its changelog: bounded multi-frame Pipe buffering,
premultiplied SKBitmap alpha, visible-region extraction, empty-frame omission,
and consumed/examined reader advancement. These operations belong to Skia's
managed pixel compositor.

ErsatzRS `ersatzrs-ffmpeg/src/graphics/mod.rs` renders the subtitle template to
ASS and composes its libass input inside FFmpeg. Native output retains that
filter path. Next `canvas_render.rs` reuses it through `build_canvas_command`:
a paced finite FFV1/BGRA/NUT canvas, with session-owned transfer and artifacts.
Rust relays encoded bytes; it does not allocate, copy, scan or crop subtitle
pixel frames. Contrary to the provisional triage, Next does receive a full
encoded output-sized canvas under accepted ADR 0105. ADRs 0005/0105 cover the
existing renderer adaptation; row171 introduces no new rendering architecture.
Existing rows146/154 own decoded alpha, subtitle, timing and cleanup evidence.
The subtitle-inclusive production canvas publication regression passes; UCU-09
owns review and reused decoded/runtime evidence.

## Row 170 source map — default VAAPI driver (Port)

Full `e8b7bb638831548e85a78992ba1b3b0ebf982aea` changes the Next config converter,
nullable-field documentation and changelog. ErsatzRS now maps Auto to None,
preserving explicit iHD/i965/RadeonSI, the selected device and existing
unsupported-Nouveau rejection. The wire type omits absent optional drivers.
The production converter regression proves absence, explicit choices, device
retention and JSON round-trip; accelerator coverage remains.

The exact accepted Next e7997fa consumer passes the optional driver into VAAPI
probing; its VAAPI implementation sets LIBVA_DRIVER_NAME only for Some. Existing
VAAPI health already recognizes Default. This correction changes no hardware
filter or runtime artifact and makes no AMD hardware-output claim. UCU-09 owns
14 passing config/protocol checks, strict lint and substantive review. Existing
approved hardware debt and required final window gates remain unchanged.

## Row 169 source and UI map — Next source rotation (Port)

The full `ee038ba460a237e277c2ed2fbb413bfac13795f8` diff covers provider/local
metadata, persistence, Media Info, Next probe hints and schema version 0.0.5.
Generated SQLite/MySQL model snapshots add only nullable MediaStream.Rotation;
ErsatzRS already has that column and the local ffprobe normalization path.

- Plex orientation values 8/3/6 map to counter-clockwise 270/180/90 degrees;
  absent/other values map to zero for video. Add the omitted response/projection
  fields without changing audio or subtitle projection.
- Jellyfin/Emby Rotation is normalized modulo 360 for video. Their shared stream
  persistence and Plex persistence already save the domain rotation field.
- Existing local metadata and Next retained/persisted probe hints carry rotation.
  Publish the shared 0.0.5 version for normal and troubleshooting playouts. The
  accepted e7997fa Next source contains this schema; reuse its verified package.
- Media Info: extend the existing repository DTO and serialized Streams report
  with Rotation; the existing dialog renders this report. Preserve other metadata,
  null omission, localization and subtitle/audio fields. No new control or layout.
- Health: Source Orientation incorrectly selects persisted audio kind 2 instead
  of video kind 1, and its test repeats the incorrect value. Correct this
  demonstrated warning gap and verify producer-written video plus excluded audio.
- Native FFmpeg keeps its accepted autorotation model (p8c-02); Next owns explicit
  rotation. Do not add a second native transpose or fabricate retained rotation.

Required proof: provider response mapping and SQLite round-trip, local signed
rotation normalization, published probe hints/schema, Media Info projection, and
real packaged Next decoded orientation/geometry. Existing hardware acceptance debt
remains unchanged. No new migration, runtime build or release is required. All 163 regressions,
four-angle decoded official-Next proof, two Media Info browser cases, the
producer-backed health regression and Next HTTP smoke pass. UCU-09 owns exact
evidence, review and inspected screenshot; final window gates remain.

## Row 168 source map — release preparation (N/A)

The full `558855083c7602b6f50ed63675ff61d1d325f98d` diff changes only upstream
CHANGELOG.md: the 26.10.0 heading/date and comparison links. It changes no source,
dependency, workflow or runtime artifact. No ErsatzRS release is implied; reuse
prior acceptance evidence. UCU-09 owns the audit and pin advancement.

## Row 167 source and UI map — local library paths (Port)

Upstream `5eb9102c33abff262207f829fdaf72329e84ba86` adds shared native path
qualification/normalization/conflict rules, create/update checks and path-specific
editor errors. Its full seven-file diff and platform test matrix are audited.

- Core `LocalLibraryPaths`: map to a small core helper using native platform
  path semantics, lexical dot-segment normalization and case-insensitive keys.
  Preserve literal percent characters; reject relative paths, URI schemes and
  invisible/quoted prefixes. Windows fully qualified drive/UNC paths and Unix
  roots have their native counterparts. Malformed stored paths must not panic.
- Create/update handlers: application validates before writes; infra checks
  overlaps against other local libraries inside the write transaction, retaining
  the Images/Other Videos exception. The previously accepted relative
  paths/file URIs and percent-decoding are removed; overlap checks now serialize
  with persistence.
- Update reconciliation: preserve matched LibraryPath identities and scanned
  children; delete only removed paths. The admin repository now preserves
  existing parent IDs, matching the already-safe general library repository.
  Production database tests prove idempotence, rollback and rejection safety.
- UI: `/media/sources/local/add` and `/{id}/edit` use existing label/control rows,
  paths textarea and top Save toolbar. Keep that geometry and CSS. The previous native
  HTML POST hid errors behind an add-page redirect; now show localized
  offending-path feedback and preserve correction inputs for JS and no-JS use.
  Resolve both editor routes with in-order SSR so the form is present without
  JavaScript; use the existing form component for escaped correction responses.
  Update every supported culture, preserve escaping and keep private paths out
  of logs. The server-function route must use the same application validation.

Required proof: native platform rule fixtures, production app/SQLite rejection
without mutation, overlap boundaries/Images exception/legacy recovery, unchanged
media/path identity across repeated saves, and Chromium/Firefox UI validation and
successful correction. Reuse pinned ICU4X simple case mapping already present in
the workspace; no new external package or runtime is needed. All eleven production
checks, two native Windows probes and eight Chromium/Firefox JS/no-JS cases pass,
along with strict affected lint. UCU-09 owns exact evidence. Review and screenshot
inspection confirm the path correction flow and retained media identity.

## Row 166 source map — remote-stream YAML extensions (Covered)

Upstream `3d31bfdd4b724b5122e0793904fbe13538c6ce87` changes the remote-stream
extension set from yml to yaml/yml (case insensitive) and its changelog.
`ersatzrs-scanner::collect_remote_stream_files` already normalizes extension
case and accepts both, preserving recursive discovery and manifest-first order.
Existing real SQLite scan coverage persists `.yaml` metadata and continues after
an invalid `.yml`; sidecar reconciliation preserves subtitle identity. Extend
existing discovery fixtures with mixed-case extensions and excluded non-YAML
files. All three focused checks and strict scanner lint pass; UCU-09 owns the
evidence and review. No production/UI/runtime change is needed.

## Row 165 source map — worker errors, credits and graphics ownership (Covered)

Upstream `3054e5d0569ed8696ffe8c2c99480df99557b61a` changes seven files;
the complete diff is audited independently.

- `HlsSessionWorker`: Rust `infra::hls_worker` handles fallible operations at
  their call sites with channel-scoped diagnostics and explicit process outcomes.
  Cancellation is an owned Notify signal, followed by process cleanup and
  identity-checked registry removal; no unobserved C# exception continuation.
- Both credits query/process handlers: `music_video_credits` trims and rejects
  blank template names before extension handling, then uses the embedded default.
  Native playback, Next and troubleshooting share that renderer. Prove null,
  empty and whitespace names against an explicit built-in template with real
  database metadata; retain existing missing-template and track-selection proof.
- `StreamingControllerBase`: accepted ADRs 0005/0105 replace the external
  graphics pipe/task with FFmpeg-owned filters and prepared artifacts. The
  stream supervisor kills/reaps the process on body drop, observes stderr and
  stops dynamic-text controllers and cleans owned graphics directories on
  completion/refusal. No parallel graphics
  engine needs cancellation. Existing process-backed cleanup tests apply.
- Template/DecoTemplate editors: native start-time options are freshly collected
  from the bounded 0..1440 five-minute range, not appended to a retained list.
- Playouts notification handling: native owned resources and activity polling
  return typed errors with one visible failure report; there is no C# event
  callback/table reference to catch. Existing UI remains unchanged.

No UI geometry, control, API, schema, runtime package or health capability change
is indicated. Reuse row164 browser acceptance and earlier decoded graphics proof;
All 16 focused credits/lifecycle checks and both explicit real-FFmpeg cleanup
cases pass, with strict affected lint. UCU-09 owns detailed evidence and review;
no production correction or new runtime build is required. Continue row166.

## Row 164 source and UI map — dependent selections and lifecycle (Port)

Upstream `3f4255bedb69ab8b94a88675f09988d5e019c774` changes 14 Razor files.
The full diff is audited; the remaining native gaps are stale child IDs in the
Deco break-content playlist selector and Playout Templates' template/deco-template
selectors after a group change. Correct the form model, not just visible options.

- BlockPlayoutEditor: `playout_kind_editor.rs` already clears default deco ID on
  group change. TemplateEditor and DecoTemplateEditor: `templates.rs` and
  `decos.rs` already clear selection and guard Add without a selected child.
- DecoEditor: `decos.rs::DecoBreakSelectionRows` filters options but retains the
  saved playlist ID. Clear the selected break row's playlist on group change.
- PlayoutTemplatesEditor: `playout_nested_editor.rs::template_editor` filters
  options but retains child IDs/names. Clear those fields and retain the chosen
  group so Save cannot persist an invisible prior selection.
  Initial browser verification also found both browsers showing Select for a saved
  template despite its matching option being present. Render selected option state
  with the dynamic child options so initial display matches the persisted model.
  The next browser run exposed a hydration text-node mismatch at the island's
  asynchronous not-found branch. Resolve the form in the server wrapper using
  Suspend, pass concrete data to the island, and use in-order SSR for this route,
  following the established Health page boundary. Keep the failure and persistence
  assertions; no retry, timeout relaxation or console-error filtering is introduced.
- FillerPresetEditor already reconciles the selected playlist against its group.
  ScheduleItemsEditor uses an owned flat playlist option list and fresh item
  forms; it has no separately filtered group selection to retain.
- AddToPlaylistDialog maps to media and search modals: both clear playlist ID
  on group change, replace loaded lists and fence responses by current group.
  AddToScheduleModal initializes its options to an empty vector.
- Libraries reads owned signal snapshots with optional progress lookup, without
  a shared mutable dictionary/indexing race. Playouts polls initialized resource
  handles rather than dereferencing a possibly absent table component.
- LocalLibraryEditor uses a native paths textarea submitted on Save; deleting
  an unsaved line makes no media-count request. MoveLocalLibraryPathDialog has no
  nullable-list modal in that flow. Trakt's event subscription is N/A under ADR 0069.
- Repeat loads use fresh form/options or resource replacement rather than AddRange.

Hydration/visual map: existing SSR wrappers and islands for `/decos/{id}` and
`/playouts/{id}/templates`; current server functions and application save commands
remain the data boundary. Preserve upstream label/select rows, editor/table/action
placement, native CSS modules, localization, modal mounting and existing fallback
forms. No geometry or new control is planned. Browser proof must change groups,
confirm child selection is cleared, choose a valid replacement and verify Save/
reload persistence in Chromium and Firefox, including retained date-filter fields.
Reuse unchanged existing source guards with focused regressions and lint; retain
the full UCU-09 final gates. No FFmpeg/runtime-package change is indicated.

Accepted: all four matched Chromium/Firefox cases and strict web lint pass.
UCU-09 owns the two causal failures, final persistence/hydration evidence and
review. The demonstrated dependent selection debt is resolved; continue row165.

## Row 163 source map — scripted schedule validation (Port)

Upstream `db5a81e44dd5980dcc34e67899c0b1afc1c21bb2` guards empty
command tokens in create/update/build, rejects invalid wait times, and requires
nonempty file input in the shared creation form. Sources: CreateScriptedPlayoutHandler,
UpdateScriptedPlayoutHandler, ScriptedPlayoutBuilder, ScriptedScheduleController,
and PlayoutEditViewModelValidator. PlayoutEditor uses that validator for creation;
the native Sequential update field contains inline YAML and must retain that contract.

Native targets: application create/save commands, the Lua scheduling engine,
maintenance reference-data loading, and existing playout form/server-function adapters.
Empty values already fail safely, but production validation accepts quoted paths
which both file readers treat as literal command strings; directories also pass
the metadata check. Unify script command parsing and reject non-files. Preserve
the existing in-process Lua engine, exposing supplied script arguments through its
standard `arg` table; no external process, HTTP callback bridge or shell is added.
The existing Lua wait API already returns an error before mutating the clock.

Acceptance: exercise real application commands against SQLite, verify rejected
create/update leaves persistence and reset queues untouched, execute quoted files
with arguments, and prove invalid waits do not alter scheduling time. No layout,
translation, schema, FFmpeg command or runtime package change is intended.
Accepted: three reproduced defects corrected; 46 focused scripted checks, strict
lint and structural checks pass. UCU-09 records the test runs and review. Pin
advances to db5a81e4; 163/181 accepted, 18 pending. Continue row164.

## Row 162 source and UI map — MPEG-TS script reliability (Port)

Upstream `4e951c1724d7f42324462a0ae70db3589b599938` changes six files:
GetMpegTsScriptsHandler, IMpegTsScriptService, MpegTsScriptService,
FFmpegLibraryProcessService, FFmpegSettings.razor and SchedulerService.
Refresh now serializes/cache-reloads definitions, removes missing folders and
accepts cancellation. Execution resolves the folder from identity, rejects missing
platform script names/files and logs built-in fallback. Settings chooses Default
(case-insensitive), then the first sorted script when the saved ID disappeared.

Rust reads definitions on demand without a shared cache or refresh scheduler;
request cancellation drops the async discovery future. Existing execution uses
owned folder identity, cfg-guarded platform scripts, temporary script ownership,
Unicode-safe environment values and logged fallback. The remaining gap is settings'
hard-coded Default-only catalog. Share discovery with playback so edited/new/removed
scripts appear on the next load and invalid saved selections recover in the form.
Do not mutate persisted selection until the existing Save action runs.

- Route: `/settings/ffmpeg`; existing SSR `FfmpegSettingsPage` and SettingsForm.
  Keep normal form POST/no-JS behavior, translations and current island boundaries.
  Browser proof exposed the existing out-of-order SSR fallback remaining on Loading
  without JavaScript; use the established in-order SSR route pattern for this page.
  Preserve custom names on the default script; localize only its built-in label.
- Visual reference: upstream responsive MudStack label/select rows within the
  settings form, followed by Custom Resolutions. Reuse SettingsSection, Field,
  Select and existing CSS; no geometry or action-bar change.
- Data: shared filesystem catalog, settings server function, current application
  settings save and IPTV command preparation. No schema/capability change.
- Acceptance: fresh/edited/deleted definitions, malformed sibling isolation,
  repeated/concurrent discovery, platform missing-file fallback, invalid/default/
  first/empty selection, retained audio/subtitle settings, browser save/reload
  with JavaScript and no-JS, focused lint and runtime smoke. Reuse unchanged
  generated-command snapshots; no FFmpeg build or release is indicated.
- Accepted evidence: 19 focused checks, six matched Chromium/Firefox cases,
  final strict affected lint and native MPEG-TS/HLS smoke pass. UCU-09 records
  the causal label/no-JS failures and bounded Firefox readiness correction.
  Pin advances to 4e951c17; 162/181 accepted, 19 pending. Continue row163.

## Row 161 source map — font-cache concurrency (Covered)

Upstream `810c1dbd0273163f2a5582cb55683ea3ee7a0235` changes only
`CustomFontMapper.cs` and `GraphicsEngineFonts.cs`. It replaces a shared mutable
font dictionary/list with concurrent copy-on-write updates, makes the loaded-file
set instance-owned and locked, records a file only after successful loading, and
logs individual failures so later files and retries remain possible.

ErsatzRS has no Skia/RichTextKit mapper or process-wide loaded-typeface set.
Under accepted ADRs 0005/0077/0105, native graphics and the Next canvas share the
FFmpeg renderer. `graphics_element_drawtext_overlay` builds an owned font-file
or family selector per request; the packaged default is Roboto. Drawvg text
writes per-session vector commands without a font cache. FFmpeg owns drawtext
and libass font loading inside each child. The bundled font extraction is awaited
in host startup before workers/listeners, overwrites stale files and has no
successful-load registry that can become poisoned after a failed read.
The official pinned Next e7997fa source passes owned `fonts_folder` settings to
FFmpeg `fontsdir`; it has no copy of either changed C# class.

The row concerns concurrency and failed-load bookkeeping in the retired renderer;
it does not change the accepted text-rendering contract or require a new cache,
lock, graphics service, dependency, runtime build or release. Existing graphics
and subtitle runtime evidence remains applicable. Nine existing font extraction,
editor and drawtext checks pass; UCU-09 owns exact evidence. No production change.
Pin advances to 810c1dbd; 161/181 accepted, 20 pending. Continue row162.

## Row 160 source and hydration map — movie collection ordering (Port)

Upstream `b403012f2233a2d76e13925e30b68a8c9d34457a` replaces title-only
jQuery sorting with whole-card Sortable, serializes saving, restores failed
orders, validates membership before persistence and returns NotFound for a
missing collection. Both database migrations disable legacy custom playback
order only for collections containing non-movies. Generated EF models and the
removed vendored Sortable implementation do not add another schema contract.

Rust already provides whole-card drag plus keyboard move controls, all-kind
movie-only eligibility and localized feedback. Keep ADR 0048 row identities,
including duplicate movies with distinct audio/subtitle overrides. Fix remaining
save/selection admission and visual ordering; validate stale/foreign rows within
the existing write transaction, preserving rollback and dependent Refresh.
Port the scoped compatibility repair without clearing indices, memberships,
playback overrides, subtitles or movie-only/empty collection settings.

- Route: `/media/collections/:id`; SSR wrapper `CollectionDetailPage`, existing
  `CollectionDetailIsland` and resolved detail view; no new island boundary.
- Data boundary: application collection commands and SQL collection repository.
- Actions: existing custom-order toggle/reorder server functions and remove
  actions; keep protected `/admin-api` and current fallback behavior.
- Visual map: title/section links, movie poster grid, custom-order toggle,
  selection bar, drag and accessible Move Up/Down controls. Keep stable mounted
  controls and signal-based disabled state while saving or selecting.
- Browser proof: Chromium/Firefox poster drag, repeated keyboard moves, pending
  save exclusion, failure restoration, reload persistence and selected-row
  removal. Existing localization is retained; translate any new validation
  feedback for all cultures. No FFmpeg/runtime rebuild is indicated.
- Required database proof: missing collection, foreign/stale row rollback,
  duplicate-row identity and preserved overrides; pre-migration mixed/media-kind
  fixtures, fresh database and repeat startup. All 13 focused checks, strict
  affected lint and eight unique Chromium/Firefox cases pass; UCU-09 records
  causal failures and the Firefox post-reload hydration wait correction.
  Pin advances to b403012f; 160/181 accepted, 21 pending. Continue row161.

## Row 159 source map — scheduler failure isolation (Port)

Upstream `94c8c0149cdfb8f9d823c4e0fc917acc305c6695` changes only
SchedulerService. All dispatch methods now catch operational failures separately
from cancellation, so one failed job no longer skips the rest of DoWork.
Startup Next synchronization and capability refresh also log their own failures.

Rust already owns build/scan jobs, guards, provider connectivity timeouts,
per-playout failure status, and the repeating scheduler tick. However, the
maintenance pass propagates loop-level errors before later loops run. Correct
that demonstrated gap using the existing sequential pass: isolate each loop's
due check, work and successful timestamp; retain its first operational error for
the caller after independent jobs and health have run. Failed loops remain due
for retry. Cancellation must stop before another loop starts and must not be
treated as an ordinary retryable failure.

Required evidence: injected SQLite failures in early cleanup and cadence
persistence do not block later work; fixing the fault permits immediate retry;
successful later jobs retain their cadence; cancellation skips subsequent jobs.
Reuse existing Classic continuation, owned scan admission, Next mirror, cached
health and playback proof where unchanged. Native Trakt remains deliberately
retired. Script/graphics loading and memory ownership remain process-local;
there is no new queue, dependency, schema, runtime package or UI change.

Source counterparts: scheduled library requests use the existing owned
application queue; rejected requests do not stop other library admissions.
Provider poll failures retain Plex inventory. Add-on history/revision/admission/
matching maintenance already isolates errors. Next mirrors are per-channel,
with atomic publication and session-owned refresh; there is no global startup
queue whose failure can starve native maintenance. MPEG-TS scripts are read at
stream creation and SQL-backed graphics references at build/projection, so no
stale scheduler-owned script/graphics cache needs a refresh job. Rust owns
memory lifetimes without the upstream release-memory queue.

The causal regression fails before the fix: subtitle cleanup skips artwork.
The final 24 maintenance checks pass, including SQLite trigger failures,
independent later cleanup/search/health, first-error retention, immediate retry,
cadence preservation, cancellation, Classic state, dependent Next notification,
and guard release. Strict infra/scheduler Clippy, architecture and formatting
pass. Boxing the large playout future keeps the async state within the lint
contract; no suppression is added. UCU-09 owns detailed evidence. No generated
FFmpeg/transport or browser change: reuse prior runtime/browser proof and run
required window gates at closeout. Pin advances to `94c8c014`; 159/181 accepted,
22 pending. Continue row160; no intermediate push or CI.

## Row 158 source map — atomic operation admission (Covered)

Full `f6bd089f5b392b6753f6cacbeeecf55964cb85a2` diff changes only
EntityLocker: Plex, Trakt, Plex/Jellyfin/Emby collections and troubleshooting
flags use CompareExchange for acquire/release and Volatile.Read for observation.
Inspect the Razor admission paths as well as handlers; the six flags cannot be
judged from the playout/library lock sets alone.

| Upstream surface | Existing Rust counterpart | Acceptance |
| --- | --- | --- |
| Three provider collection flags and status notifications | Application OperationCoordinator keyed by provider/source; Mutex-protected insertion/status; owned guard retained by NativeMediaSourceRuntime worker (ADR 0051) | Simultaneous attempts through cloned coordinators admit exactly one owner; rejection cannot release it; independent keys remain usable and drop permits retry |
| Plex sign-in/sign-out/discovery flag | Persisted pending-flow claim and credential-generation checks in Plex authentication transactions | Existing superseded completion, sign-out, late rejection, rollback and inventory tests; no stale write over a replacement identity |
| Troubleshooting flag | Shared Tokio owned preview guard for both engines (ADR 0095) | Reuse row157 lock/preparation/process/diagnostic completion and browser disconnection evidence |
| Trakt flag | Native Trakt deliberately Dropped under ADRs 0066/0067/0069 | No retired global lock or provider implementation reintroduced; provider-neutral add-on list ownership remains unchanged |

No production change is indicated. Add a real contested admission regression
for all three external provider keys rather than a second flag implementation.
No UI, schema, health requirement, dependency, runtime artifact or generated
FFmpeg change is implied. All 22 operation/Plex/preview checks, strict affected
lint, architecture, formatting and progress validation pass. Eight simultaneous
cloned callers admit exactly one owner for each provider; rejection and releasing
a different source preserve that owner, and dropping it permits a retry.
Reuse unchanged row157 browser/runtime proof. UCU-09 owns detailed evidence.
Pin advances to `f6bd089f`; 158/181 accepted, 23 pending. Continue row159;
window closeout, push and exact-SHA CI remain. No intermediate push or CI.

## Row 157 source map — troubleshooting completion and cleanup (Port)

Audit of `2bbfbd10e10de2f49978c0efbe8b82e9d082dc43` covers all 13 files,
including the smart-collection status correction alongside troubleshooting.
The accepted ADR 0095 process ownership model already replaces the upstream
global lock/notifier handoff with owned guards and bounded native/Next workers.
Rejected admission never releases another owner's guard; Next preparation and
process completion already wake the caller, with cleanup retained by the worker.
Do not reintroduce global notifications or change that architecture.

| Requirement | Target | Acceptance evidence |
| --- | --- | --- |
| Duplicate smart names return 409; mixed known validation/not-found/conflict errors remain 400 | Existing application commands, transactional repository and machine OpenAPI | CRUD/error matrix, duplicate race, preserved browser validation and scheduling refresh |
| Native readiness observes failed or completed processes immediately | Owned native preview worker and existing readiness wait | Early-exit regression, successful output, cancellation/disconnected caller, stable diagnostics and released ownership |
| Captured sample is removed after success, capture failure, read/archive failure and caller cancellation | Existing sample capture and in-memory ZIP | Owned temporary directory, bounded process termination and cleanup tests; real FFmpeg copy preserves mapped tracks |
| Canonical settings, snapshot logs and credential-safe environment diagnostics | Existing report queries and bounded log reads | Synthetic URI redaction regression; existing settings/log/ownership tests |
| No-report archives and UI teardown do not leak files/subscriptions | Existing in-memory archive and preview lifecycle | Archive regression and matched Chromium/Firefox preview completion/navigation checks |

Confirmed gaps are native readiness polling after child exit, unowned sample
files after read/cancellation failures, and credentials within allowed URI-valued
environment variables. The existing TSDuck text helper redacts query tokens but
does not remove URI user information; use parsed URLs for the latter. Preserve
ordinary paths/settings and never inspect real credentials for the regression.
Sample capture must retain its temporary directory until the child is reaped,
including after a disconnected request, and keep the existing two-minute bound.
The archive remains an in-memory ZIP; no temporary ZIP or new dependency is needed.

Exclusions: no schema, global notifier, new background infrastructure, generated
FFmpeg argv, runtime rebuild/release or hardware acceptance expansion. Reuse the
verified v2.0.3 runtime and official Next e7997fa. All 53 focused checks, strict
affected lint, architecture/hydration/progress checks and 24 Chromium/Firefox
cases pass. Real sample ZIPs retain video, English/Dutch audio and Dutch subtitles;
disconnected browser and sample workers complete their owned cleanup.
The initial Firefox Next startup exceeded the unchanged five-second browser
observation; its unchanged serial selection passes all 12 cases on rerun.
No test timeout/assertion changes or Next production changes. UCU-09 owns the
full evidence and review. Streaming/transport proof is unchanged and reused.
Pin advances to `2bbfbd10`; 157/181 accepted, 24 pending. Continue row158;
window closeout, push and exact-SHA CI remain. No intermediate push or CI.

## Row 156 source map — machine mutations and typed HTTP errors (Port)

Complete `a4642d95bbd1715033a48df12306aedfd6169216` audit includes the
profile and smart-collection handlers/controllers, shared validators,
BaseError.Join, concat lookup, maintenance empty-trash and removal of an unused
scan-error type. The existing Rust repository/application error enums already
carry validation, not-found, conflict and operational failures. No new error
framework, schema, dependency or ADR is needed; ADR 0103 owns machine access.

| Requirement | Target | Acceptance evidence |
| --- | --- | --- |
| Profile create/update: invalid name/thread/resolution 400, duplicate 409, missing update 404; known mixed categories 400, unknown 500 | Application API validation, existing profile repository and machine adapters | Owned SQLite HTTP matrix, mixed errors, no rejected write, success round trip, concurrent duplicate admission |
| Profile delete: missing 404, in-use 409, default 400; preserve channels/playouts | Existing profile deletion transaction | HTTP/repository guards and unchanged dependent graph; default plus in-use remains 400 |
| Smart create/update: invalid/duplicate 400, missing update/delete 404 | Existing collection commands/repository and machine adapters | HTTP CRUD/query matrix; scheduling/search refresh regression |
| Known HTTP error categories and safe unknown 500 | Machine response adapter, empty-trash endpoint | Error-body/status checks and closed-storage failure without disclosure |
| Missing concat channel 404 | Existing IPTV concat handler | Owned router test with private listener initialized; public callback policy retained |
| Upstream-compatible machine JSON, normal browser behavior and security | Existing application boundary; authenticated production router/OpenAPI | Enum/name/null/result projection, create/list/update/delete, wrong/missing key before side effects, streaming denial; browser save/errors unchanged |

The machine CRUD paths are missing despite existing browser forms and application
operations. Add the affected routes using those operations, including list
projections for reviewable round trips; do not redirect machine clients into
browser forms. Upstream uses camelCase, string enums (also accepts numeric enum
values), omits nulls and special-cases ffmpegProfileId. Accepted ADRs 0049/0050
keep persisted TranscodeAudio/Video while the machine adapter accepts upstream
NormalizeAudio/Video. Preserve ErsatzRS-only HDR/tune fields on API updates.
The current profile repository delete can cascade channels and does not reject
missing rows; guard it transactionally before exposing the machine operation.
Show localized browser feedback for default/in-use deletion failures, preserving
the profile row and existing dialog/toast flow; verify English and Dutch cases.
Hydration map: `/ffmpeg` uses the component wrapper FfmpegProfilesListPage and
FfmpegProfilesListIsland, existing list/count/delete server functions and the
mounted PagedTable/DeleteDialog/toast primitives. No shared primitive or DOM
shape changes. Browser evidence found row callbacks losing their language
context after async loading. Initialize the island's own language context
(SSR ancestry does not survive client island creation), then capture it for
those callbacks and deletion feedback. Existing editor/no-JS checks remain required.
Acceptance: all 33 focused checks, strict affected lint, matched Leptos, 15
browser passes (one unchanged Firefox no-JS skip) and native smoke pass.
UCU-09 owns full evidence and substantive review. The initial Dutch failures
were fixed at the island context/callback boundary; assertions remain intact.
Pin advances to `a4642d95`; 156/181 accepted, 25 pending. Continue row157;
window closeout, push and exact-SHA CI remain. No intermediate push or CI.

## Row 155 source map — AMF runtime and optional adapter (Port)

Full `d1c2ab3abcd7da9191b40b4cbc252ea973f4a9ec` audit: upstream Windows
artifact URL changes its FFmpeg8 build suffix, AMF release notes expand, and
Next Video gains nullable `amf_device`. No upstream caller sets that new field.
Exact official Next e7997fa consumes an optional uint32 adapter index, with
absence selecting Auto; Windows chooses a discrete AMD adapter, and non-Windows
ignores explicit indexes. ErsatzRS emits AMF acceleration but lacks the optional
wire field. Port that typed field while preserving None in profile mapping;
no UI/profile selector or native acceleration expansion is required by this row.

| Requirement | Target | Evidence before acceptance |
| --- | --- | --- |
| Optional adapter field and automatic default | Next protocol/config | Missing/null round trip; zero/nonzero indexes; reject negative/out-of-range; existing hardware/profile mappings |
| Expanded AMF runtime, including MPEG-2 | Published FFmpeg9 v2.0.3 plus official Next e7997fa | Reverify exact archives/executables and existing patch/package evidence; no duplicate build |
| Preserve playback/health and platforms | Existing unchanged runtime integration | Reuse row154 native/Next smoke and available hardware proof; maintain approved unverified AMD/Intel/native-Linux hardware debt |

Existing accepted row140 and the runtime look-ahead audit own publication and
patch integration. Upstream's FFmpeg8 filename does not replace ErsatzRS's
pinned, checksum-verified FFmpeg9 package. Runtime capability registration is
not AMD output acceptance.

The new regression reproduced silent loss of adapter zero before the fix
(`de702681-92d7-4e1b-b185-f549e594d03f`). All 14 Next protocol/config checks
pass (`c61b962d-935d-432f-8bdd-b6d5ba1ff9c4`), covering absent/null values,
zero/nonzero/max uint32 indexes, invalid negative/overflow/fraction/string
values and existing hardware, subtitle, track, timing and canvas contracts.
Strict affected FFmpeg/infra/web all-target Clippy, architecture lint and
formatting pass. Default generated profile output omits the field, preserving
automatic device selection and existing runtime/health behavior.
Both published Linux/Windows v2.0.3 archives and all four executables match
the accepted candidate checksums; exact official Next e7997fa matches its
accepted digest. The Linux decoder list advertises all six AMF wrappers,
including MPEG-2/VC-1. Exact Next's guarded decoder selection intentionally
keeps VC-1 in software to avoid reordered timestamps; no native policy changes.
Reuse row154 native/Next smoke and browser evidence: generated profile output,
media/canvas handling and runtime artifacts are unchanged. Approved hardware
debt remains open; no AMF hardware success is claimed. No new dependency,
package, build, publication or architecture deviation. Pin advances to
`d1c2ab3a`; 155/181 accepted, 26 pending. Continue row156; no push or CI.

## Row 154 source map — motion timing and preview graphics anchors (Adapt)

Upstream `908fa961782618aed2621ca8fa6d02d9b658102f` changes MotionElement,
its decoded-frame tests and troubleshooting preparation. Adapt under
accepted ADRs 0005/0105: retain native FFmpeg overlays and the shared canvas;
no Skia renderer, runtime rebuild or new dependency is required.

| Upstream requirement | ErsatzRS target | Required evidence |
| --- | --- | --- |
| Recover the last decoded Hold frame near/after EOF; reject inclusive expiry, including zero hold | FFmpeg graphics motion renderer and canvas timing tests | Published v2.0.3 decoded red/blue/alpha boundary checks |
| Preserve complete subsequent loops after a partial initial loop | Motion source phase preparation | Decode multiple loop cycles after seeks; preserve fractional-first-frame coverage |
| Stop at the finite stream finish and skip starts at/after that finish | Graphics render context and bounded canvas/native output | Exact frame count and no motion input when start is outside the stream |
| Anchor media troubleshooting at now minus seek and expose numeric MediaItem_SeekSeconds | Native troubleshooting, Next selection/configuration, graphics template variables | Template/anchor regressions and both-engine playback proof |
| Ignore seek for live previews; preserve finite-source seeking | Shared preview seek selection and existing liveness read model | Live/finite regression and remote/prepared/producer Next playback |
| Keep subtitles, alternate audio, credits, owned artifacts and schedule isolation | Existing native/Next preparation and credits adapter | Existing regressions plus actual seeked playback/archives |

Confirmed source gaps: Hold uses a strict greater-than expiry test and seeks
near EOF without frame recovery; Next media preview puts seek only in source
in_point while its canvas starts at zero. Native preview content_start equals
now. The numeric MediaItem_SeekSeconds alias is absent. Multi-cycle decoded testing passes unchanged with the published FFmpeg 9
input loop/seek behavior; preserve that existing path. All validation uses
the already published exact runtime and official Next engine.

Acceptance: 75 focused checks pass (Nextest
`46b12aef-2f24-4fb2-be86-30cdffd8d4c7`), including NTSC first-frame accuracy,
EOF/expiry/loop boundaries, actual official Next canvas playback, Dutch audio,
subtitles/credits and artifact/session cleanup. The reviewed complete native
argv snapshot passes (`bb7f86f8-1263-4299-a163-3286b7f2fac2`): media seek,
bounded last-frame recovery, exclusive Hold expiry, programme-clock frame
filters and finite output are wired together without local paths.
Matched Leptos SSR/WASM and all 18 Chromium/Firefox cases pass in
`test-results/ucu09-row154-browser/`: both engines decode the expected held
blue motion and timed green script, then black after expiry, while retaining
existing playback/credits/archive/no-JS checks. Strict affected all-target
Clippy, architecture/hydration lint, formatting and progress validation pass.
Native and Next smoke pass, including Dutch audio/subtitles, preview,
IPTV/HLS, private callback isolation, wrapper restart and idle cleanup.
Row146's 87 TSDuck checks remain valid transport evidence because transport
and mux arguments are unchanged; final UCU-09 pipeline-wide TSDuck remains
required. Existing reviewed UI geometry is unchanged. Published v2.0.3 and
Next e7997fa checksums are reverified. No new runtime build, dependency or
publication. Pin advances to `908fa961`; continue row155 without push or CI.

## Row 153 source map — .NET dependencies and AAC surround notes (N/A)

- Full `0e1b355f` audit: twelve .csproj files update Microsoft framework/test,
  NCalc, SkiaSharp, System.CommandLine, Dapper, Jint, NUnit analyzers, MudBlazor
  and Scalar packages. These NuGet versions have no Rust-manifest counterpart;
  no ErsatzTV handler, repository, schema, FFmpeg builder or runtime pin changes.
- One CHANGELOG line announces the Next AAC-above-stereo fix. Native
  `emit_audio_channels` already retains `-ac` for AAC/AAC-LATM above two channels,
  even when the input and output counts match. Exact Next e7997fa's optimizer
  likewise retains AudioChannels for AAC surround. Existing profile-wire proof
  maps six channels without truncation; reuse unchanged row150 configuration proof.
- The existing native six-channel regression now snapshots the complete final
  argv. Six-channel AAC, unchanged stereo omission and AC3 downmix tests all pass
  (`e9353b71-dd2e-4631-9f7e-23dc7f98e681`); strict FFmpeg all-target Clippy passes.
  Published v2.0.3 executes that argv with an owned three-second source/boundary:
  output is H.264 plus six-channel AAC 5.1; strict decode emits no error bytes.
  The initial standalone harness omitted the programme boundary required by apad;
  the corrected finite capture keeps codec/filter arguments unchanged.
- Verdict N/A for upstream dependency/release bookkeeping; the announced AAC
  behavior is already covered. No production code, Cargo dependency, runtime
  package or ADR changes. Preserve row151 browser/host and existing transport proof.

## Row 152 source map — upstream Next AMF release notes (N/A)

- The complete `495e3799` diff adds one CHANGELOG line announcing Next AMF
  capability detection and optional hardware scaling on AMD Windows systems.
  No source, protocol, dependency, build pin or runtime configuration changes.
- ErsatzRS runtime integration remains owned by accepted row140/ADR 0095, using
  the published v2.0.3 packages and exact Next e7997fa artifact. Native AMF
  selection and the Next acceleration mapping already exist; this note does
  not provide hardware acceptance evidence. Approved AMD AMF debt stays unverified.
- Verdict N/A: release bookkeeping in the upstream project. Preserve the runtime
  and hardware contracts; no Rust edit, new package, dependency or ADR is needed.
  Progress/diff checks pass; reuse row151's unchanged code and runtime evidence.

## Row 151 source map — troubleshooting music-video credits (Port)

- Upstream `0840e0fb` adds the music-only optional template dropdown to
  `PlaybackTroubleshooting.razor`, forwards `musicVideoCreditsTemplate` through
  the controller and Prepare/Start handlers, and archives the template and ASS.
  Generated credits replace the chosen subtitle while audio selection remains.
- Native preparation renders ASS at the selected seek; Next uses the transient
  troubleshooting item rather than creating a database playout. Archive filtering
  includes `music-video-credits` files. Channel previews retain channel settings.
- Rust targets the application preview request/read model and runtime port,
  SSR playback form/context, query adapter, both host preparation paths, and the
  existing `music_video_credits` renderer. No schema, scheduler or new capability.
- ADR 0076 retains the existing minijinja/Scriban-compatible renderer; ADR 0095
  retains the optional engine and existing owned ephemeral artifact contract.
- Visual map: upstream 64px download toolbar, extra-large content container,
  responsive label/select rows, music-only None/template field between subtitles
  and watermarks. Reuse `playbackField`, existing select styling and translations;
  the SSR form and existing preview JavaScript preserve the no-JS GET path.
  No shared island primitive or CSS changes are planned.
- Acceptance: template listing/query, music-only/None gating, audio/subtitle
  precedence, seek timing and unchanged persisted channel/playout state pass
  focused regressions. All 762 web tests, strict boundary/web lint, architecture,
  hydration and web parity gates pass. Chromium/Firefox pass all 14 scenarios:
  both engines decode credits after seeking and archive ASS/template. Two no-JS
  mobile cases also pass after capturing the visible credits field; the reviewed
  320px geometry matches the Razor-derived field order and existing form styling.
  Native and official Next host smoke pass. Health capabilities and transport mux
  are unchanged; UCU-09 still owns the full window closeout and TSDuck gates.
- Review corrected a missing music-video summary projection, retained native
  channel credits settings, portable snapshot redaction and the stale sessions
  test after ADR 0105's startedAt addition. No new deviation or runtime build.

## Row 150 source map — visible Next fallback reasons (Port)

- Upstream `599dc5ea` adds optional `fallback.show_error` to channel configuration
  and enables it in `ChannelConfigConverter`. There is no new UI, validator,
  persistence or scheduling policy.
- Rust targets `next_engine::protocol::ChannelConfig` and `config::channel_config`.
  Keep older omitted/null configuration valid and preserve explicit false values;
  generated configurations enable the error card for Burn and Convert alike.
- Official Next `e7997fa` already renders a fallback ASS error card and burns it
  regardless of ordinary subtitle conversion. Use that existing runtime behavior.
- Accepted ADR 0095 continues to own the optional engine integration; no new
  deviation or FFmpeg package is needed. Existing overlays can disable the card.
- Acceptance: generated/default and nullable wire contracts, overlay precedence,
  and decoded official-engine gap/failed-media video in Burn/Convert with explicit
  off. Finite troubleshooting retains upstream failure reporting.
  All checks, host smoke and strict lint pass; UCU-09 owns detailed evidence.
  The mpeg-ts-channel-streaming / ucu09-row150 debt is resolved.

## Row 149 source map — Plex Other Video folder tags (Port)

- Upstream `4fffa955` changes `PlexServerApiClient.GetFolderTags` and resets
  Plex Other Video ETags and library scan times in a SQLite migration.
  No UI, validator, playback or FFmpeg contract changes are involved.
- Rust `plex::PlexLibraryResponse` now retains server `Location` paths.
  Other Video scans read current locations once through the existing client,
  before publication, and reuse them across all pages. The item fingerprint
  includes derived tags so changed roots also update cached items.
- Derive tags from server paths independently of host OS: mixed separators,
  drive/UNC/POSIX roots, case-insensitive folder boundaries and the first
  matching library root. Include the library folder, exclude roots and filenames.
- Persist tags with existing metadata ownership, separately from Plex labels,
  collection/network tags and other providers (ADR 0104). Repeated scans and
  restart must preserve retained identities and remove only stale folder tags.
- The native SQLite refresh migration preserves media/metadata and makes existing
  Other Videos eligible for ordinary rescan. Existing migration health checks
  continue to verify schema readiness. Owning debt: scanner / ucu09-row149.
- UCU-09 records passing upstream path cases, paged provider/SQLite integration,
  scoped migration, failure preservation, repeat/restart, search visibility,
  subtitle/owner retention, duplicate convergence and strict lint acceptance.

## Row 148 source map — subtitle conversion (Port)

- Upstream `c673082e` changes `PlayoutItemConverter`, Next metadata loading,
  subtitle-path ownership and native/troubleshooting provider classification.
  Rust maps these to `next_engine::selection`, `remote_projection`, shared
  `playback` selectors and `provider_subtitles` preparation.
- Plex movie, episode and Other Video sidecars resolve current owner identity
  and header credentials before bounded preparation. Provider eia_608 exclusion
  is shared across native, Next and troubleshooting; stored metadata is retained.
- Automatic selection omits unusable sidecars; exact pins fail explicitly.
  Prepared-media file checks occur against the materialized artifact. HTTP(S)
  sources include unpersisted and loopback URLs and normalize scheme case.
- Existing image input identity, music-video metadata subtitles and generated
  credit timing retain their source/seek semantics. Embedded Burn preparation
  validates the exact version rather than trusting old extraction metadata.
- Accepted ADRs 0095/0102 keep exact-item private callbacks and owned local
  artifacts; no additional subtitle proxy or architecture deviation is required.
- UCU-09 records provider/caption regressions, 245 Next checks, three decoded
  runtime checks, four final availability checks, strict lint and native smoke.
  It distinguishes final URI checks from reused unchanged transport evidence.
  No hardware acceptance or milestone-wide closeout is claimed.

## Row 147 source map — YAML ambient state (Port)

- Upstream `b3f7f3fe` adds post-roll, mid-roll and graphics settings to
  `YamlPlayoutContext.State`, serialization and restoration. Empty or missing
  fields clear stale values. Tests cover default/named contexts, graphics
  variables, explicit clears, old anchors and repeated Continue builds.
- Rust `SerializedYamlContext` omitted the same three fields. It now saves and
  restores them, accepting the new upstream field shapes while retaining the
  existing Rust anchor format. Legacy missing/null values remain empty; no
  inferred replay or change to Refresh/Reset semantics is introduced.
- Production SQLite restart proof exposed a second gap: YAML reference loading
  never populated graphics path IDs. It now reuses the existing SQL graphics
  lookup and the engine's complete definition/import loader, covering reset,
  default and named instruction lists plus imported sequences. Missing paths
  retain the existing no-op behavior.
- Three failing core regressions precede the fix. The 78-test YAML selection and
  six final focused checks pass. Real builder/repository/reopen cycles cross
  midnight and match uninterrupted items, including breaks, graphics variables,
  scheduling context and audio/subtitle preferences. Import/reset/named lookups,
  legacy defaults and explicit clears pass. Strict affected lint and native
  smoke pass; UCU-09 owns the evidence and existing continuity regressions.
- Substantive Codex review checked every changed upstream state field and test
  scenario against serialization, restoration and production SQL wiring. There
  is no new architecture deviation or row-specific deferral.

## Row 146 source map — graphics canvas (Adapt)

- Upstream `28b2e00c` introduces `GraphicsLayerKind.Canvas`, signed private
  `InternalController.GetGraphicsCanvas` delivery and strict offset/duration/rate
  headers; `PlayoutItemConverter.SelectGraphics` emits FFV1/BGRA/NUT hints.
- `GetGraphicsCanvasStreamHandler` reloads channel/item/decorations, applies
  mirror offsets, derives session time, clips the chunk to the programme and
  supplies transparent frames if selected elements disappear. A stale offset
  still produces a finite requested chunk.
- `GraphicsEngineContextFactory` shares crop/stretch/content-box geometry;
  `GraphicsElementLoader` now binds `MediaItem_Stop` to programme finish rather
  than chunk duration. Native `FFmpegLibraryProcessService` uses this factory.
- `SyncNextPlayoutHandler` shares `IncludeForNextPlayout` loading with canvas
  lookup. Preview preparation/start retains a transient channel/item in
  `ITroubleshootingPlayoutItemStore`; normal/Next session models expose start time.
- Tests cover invariant scheduled stop across chunk offsets and invalid HTTP
  headers before rendering. Startup registration and native pipeline tests wire
  the extracted context factory. No admin page or database schema change.
- Rust counterparts are `next_engine::{graphics,protocol,media,troubleshooting}`,
  the existing private host router, `graphics_template`, shared playback
  selectors/artifact ownership and FFmpeg graphics filters. The initial Next
  projection rejected non-image kinds/expressions. The independent `MediaItem_Stop`
  correction now passes scheduled finish through shared/native/Next/preview
  contexts; UCU-09 owns the focused chunk/mirror regression evidence.
- Accepted ADR 0105 specifies a supervised FFmpeg canvas using the shared native renderer,
  preserving ADR 0005's ban on a separate graphics-rendering application.
  The ADR owns architecture and acceptance details; UCU-09 owns evidence.
  The official Next runtime and existing v2.0.3 FFmpeg artifacts are reused.
- Completed: authenticated finite canvas delivery, shared render context,
  programme/chunk/session clocks, transient previews, stable `startedAt` reports
  and session-owned producer cleanup. All five graphics kinds and both text
  renderers pass decoded HTTP and official Next proof; native smoke and all
  87 TSDuck checks pass. UCU-09 owns the detailed evidence and exact artifacts.
- Substantive Codex review mapped each changed upstream behavior to production
  wiring and existing/focused proof. Final corrections advance historical preview
  decoration selection and share the reported session clock with graphics.
- Later row173 changes watermark-only projection; it remains an ordered future
  audit and does not justify skipping row146's full graphics behavior.

## Row 145 source map — playout validation and diagnostics (Port)

- Complete upstream `4c3683ae` changes four behavior paths: explicit invalid
  external-JSON start-time handling during channel refresh and item lookup;
  distinct missing Plex server/active connection/token diagnostics; rejection
  of empty alternate schedules; and a warning on marathon-to-random fallback.
- Existing coverage: `ersatzrs-app::commands::save_playout_alternate_schedules`
  rejects empty items before repository mutation/queue work. The repository
  also requires a default schedule inside its write transaction.
- Initial source audit identified a wider external-JSON gap: the editor saved
  `ScheduleFile` and `ExternalJsonSchedulingEngine` deliberately does no work,
  but no external channel/program JSON parser or equivalent upstream
  `ExternalJsonPlayoutItemProvider` consumer was present in the Rust tree.
  Native `load_active_media_path` and XMLTV `load_guide_items` selected persisted
  items. Next `schedule::read_locked` also does so, but the later source audit
  confirmed that this matches upstream and is not an external-JSON parity gap.
  The bounded runtime observation and its limitations are recorded in UCU-09
  milestone evidence.
- Upstream `MarathonHelper.GetEnumerator` returns None for an unsupported
  grouping mode, not for an empty valid group. The local fix normalizes only
  `MarathonGroupBy::None` to Random in the build-local schedule copy and logs the
  upstream warning. Valid marathon grouping is unchanged.
- The shared `ersatzrs-infra::external_json` reader and XMLTV consumer are now
  implemented. The transient resolver now preserves mirror timing, local-file
  preference, server-scoped replacements, fresh stream metadata and distinct
  redacted Plex failures. Its remote metadata client rejects redirects and
  enforces bounded reads/deadlines. Milestone evidence owns focused results.
- Native `load_active_media_path` and HLS `load_active_hls_item` consume these
  transient bindings. Reuse existing probe-to-source helpers and protected loopback media
  routing; never insert temporary library/media/playout rows or publish Plex
  credentials. Preserve custom/default audio and subtitle selection and
  troubleshooting metadata. UCU-09 owns the successful native HTTP/HLS, guide,
  invalid-schedule, smoke and TSDuck acceptance evidence.
- Scope correction: at accepted source `b30d9670` and fetched target `8bda4411`,
  only `GetPlayoutItemProcessByChannelNumberHandler` calls the external JSON
  provider. `SyncNextPlayoutHandler` exports persisted items; the Next callback
  uses fallback filler. Next support is not required to port row145 and would
  require a separately accepted ADR as a deliberate extension. Milestone
  evidence records this correction to the earlier assessment.
- The [official Plex PMS schema](https://developer.plex.tv/pms/) confirms nested
  `Media`/`Part`/`Stream` names. Shared decoding now accepts these plus existing
  lowercase aliases; metadata and scanner fixture evidence belongs to UCU-09.
- Next action: trace and complete external schedule consumption through the
  existing playback/guide boundaries, including bounded file reads, invalid
  start-time handling and safe Plex precondition diagnostics; preserve provider
  credentials and subtitle/audio selection. Reuse the existing probe-to-source
  metadata converters rather than creating persisted media for temporary JSON
  programmes. Require successful and invalid-schedule
  runtime proof, empty-alternate mutation protection and focused regressions.
- No row verdict or pin advance yet. The external runtime gap is recorded in
  MPEG-TS channel streaming debt and must be accepted before this row lands.

## Row 144 source map — Next optimization release notes (N/A)

- Complete upstream `b30d9670` diff adds only two CHANGELOG lines: downscale
  before tonemapping when possible, and merge consecutive QSV filters.
- Exact packaged Next `4cc93e4`, `pipeline.rs`, keeps tonemapping first for
  Vulkan decode or no downscale and otherwise applies geometry first.
  `filter_chain.rs` fuses consecutive QSV filters through `VppQsv::fuse`;
  `qsv.rs` retains incompatible padding/tonemapping/deinterlacing guards.
- No legacy source or build pin changes occur. Existing packaged Next already
  carries the described behavior; row140 integration evidence remains valid.
  No duplicate build or new hardware/performance claim is required. Explicit
  approved hardware acceptance debt remains unverified.

## Row 143 source map — Dolby Vision/HDR10 probing (Port)

- Upstream `cbfea01c`: local statistics parse DOVI configuration and HDR10
  mastering/content-light metadata, with one-frame PQ fallback. Repository
  refresh persists both fields; Next hints and Media Info JSON expose them.
  Additive migrations also reset local PQ/HLG version timestamps and folder
  ETags so unchanged HDR files receive a one-time metadata refresh.
- ErsatzRS already stores richer mastering/content-light/Dolby Vision fields.
  `ersatzrs-infra/src/ffprobe.rs` reads bounded frame side data, scopes it to
  the owning video stream and populates those fields; scanner persistence and
  `next_engine/probe_hint.rs` retain them and derive `has_hdr10_metadata`.
- Demonstrated gaps: `MediaItemInfoStreamJson` lacks `DvProfile` and
  `HasHdr10Metadata`; movie/local and remote projections therefore omit them.
  The existing HDR/DV migrations add fields but leave prior local HDR scan
  timestamps and folder ETags unchanged. An unchanged file can keep missing
  metadata until an operator forces a scan.
- Next action: follow the migration contract for a scoped one-time local
  PQ/HLG refresh, confirming ErsatzRS scanner gates first; expose nullable
  video metadata through the existing Media Info JSON projection. Preserve
  non-video absence, provider isolation, file/subtitle identities and playout
  state. Verify synthetic migration isolation, probe/projection regressions
  and real unchanged-file rescan behavior before assigning a verdict.
- Implemented the one-time local PQ/HLG migration and video-only nullable
  Media Info fields through the admin repository and JSON projection. The
  migration suite passes 34 tests; the projection regression passes. A real
  synthetic HDR episode test confirms an unchanged season skips probing before
  migration, repairs mastering/MaxCLL/MaxFALL after it, preserves episode/version
  and Dutch subtitle identities and file mtime, then skips again after success.
  The fixture explicitly sets frame colour tags and asserts PQ before migration.
- Browser boundary: `/media/music/videos/:id`, existing faceted-detail island
  and Media Info dialog; application repository to server-function JSON to
  existing `pre` viewer. No HTML, styles, localization keys or island structure
  change. Browser proof extends the existing facet-detail scenario with exact
  HDR/DV JSON fields and omission on audio; matched SSR/WASM is required.
- Acceptance: production JSON regression passes; matched Chromium/Firefox
  each pass the extended Media Info scenario (2 expected, no skipped,
  unexpected, flaky or report errors). Strict all-target Clippy for core,
  infra, scanner and web passes; hydration lint reports no new findings.
  Review confirms scoped scan-gate invalidation, existing state/identity
  preservation and agreement between Media Info and Next metadata presence.
- No architecture deviation proposed. Intel QSV tonemapping output remains
  covered only by the maintainer-approved unverified hardware debt, not by
  metadata presence or software playback evidence.

## Row 142 source map — upstream build trigger (N/A)

- Upstream `00db622d` is an empty commit: its tree equals its first parent's
  tree. The full diff contains no source, workflow, configuration or notes.
- This triggers upstream packaging only. No ErsatzRS implementation or
  runtime change is required; reuse row140 integration proof. Do not launch
  duplicate runtime builds for this bookkeeping commit.

## Row 141 source map — Next release notes (N/A)

- Complete upstream `8cc34a1e` diff only changes CHANGELOG.md: QSV padding
  availability, QSV AV1/VC1/VP8/VP9 decoding and default `send_frame`
  deinterlacing with channel overrides. No application or build pin changes.
- Exact packaged Next `4cc93e4` maps all four codec strings through QSV
  capability checks. Its `video_filter.rs` preserves configured YADIF/BWDIF/
  W3FDIF modes and defaults each to `0`; the checksum-verified v2.0.3 BWDIF
  option help identifies `0` as `send_frame` and `1` as `send_field`.
- Row140 owns the QSV runtime patch adoption. Actual QSV output remains in
  explicitly approved hardware debt. Row140 integration now passes; row141 is
  N/A release bookkeeping. No new runtime or pipeline change is required.

## Row 140 source map — QSV hardware padding (Port)

- Upstream `6e9693e9`: the complete two-file diff changes the Windows custom
  FFmpeg artifact from g3c0520f9 to g14f231eb and advertises Next QSV hardware
  padding on Windows/Linux. There is no application implementation in this diff.
- Accepted ADRs 0008/0063 keep runtime production in ErsatzRS-ffmpeg and select
  FFmpeg 9. That packaging separation does not prove this capability exists.
- Exact packaged Next revision `4cc93e4`, `crates/ffpipeline/src/accel/qsv.rs`,
  selects hardware padding only when `vpp_qsv` advertises `pad_w`; its emitted
  options are pad_w/pad_h/pad_x/pad_y/pad_color. Otherwise it keeps software Pad.
  This is an option on vpp_qsv, not a separate pad_qsv filter.
- The checksum-verified published Linux x64 v2.0.2 runtime advertises vpp_qsv
  but none of those padding options. Native QSV also downloads for software
  padding. Successful existing playback therefore cannot establish hardware
  padding acceptance. Windows package capability has not been newly checked.
- Owning debt: ffmpeg-9-performance-and-support, row-140-qsv-hardware-padding.
  UCU-09 owns artifact/source evidence. Required follow-up is the separate
  runtime patch/package contract plus functional Intel QSV proof, including
  centered geometry and preserved subtitle/audio paths. Keep the existing
  software fallback for runtimes without the capability.
- Read-only follow-up identified the three-patch upstream series at build-repo
  commit `60e545dc00f3ea52fe393c02a9ab43ab16034a1f`: 0008 seeds QSV composite
  output timestamps, 0009 adds vpp_qsv padding, and 0010 fixes colour/range,
  chroma-grid alignment and rejection of incompatible combined operations.
  All three apply sequentially to disposable FFmpeg n9.0 source with zero fuzz,
  no offsets and no rejected hunks. This is source applicability, not build or
  runtime acceptance; existing checkouts and binaries were not changed.
- Proposed runtime follow-up: carry the complete series in the separate runtime
  repository, build versioned Linux/Windows packages only after reconciling the
  artifact constraint, verify checksums and option exposure, then run the
  upstream padding harness on real Intel QSV. Cover NV12/P010, centered and
  odd-offset geometry, coloured borders, cropped decoder surfaces, monotonic
  timestamps at async_depth=4, and incompatible-operation rejection. Verify
  Next chooses the hardware path while subtitles and alternate audio survive.
  The current published package is the missing-option negative control.
- Subsequent explicit approval authorized the expanded build and publication.
  ErsatzRS-ffmpeg v2.0.3 now carries the complete series; exact public archives
  match the tested bytes, and the managed default is updated locally. Both
  package capability checks expose all five padding options. Focused manager
  tests (13), strict crate Clippy and native/Next host playback smoke pass.
  Next decoded proof retains Dutch 880 Hz audio and burned subtitles, preview,
  wrapper restart, HLS after wrapper disconnect and idle session cleanup.
- Intel QSV hardware acceptance remains unverified and is explicitly approved
  debt (2026-09-28); retain the above geometry, colour, timestamp and media
  checks for a suitable machine. Approval permits catch-up after integration,
  without claiming hardware-output proof. Row140 is Port under ADRs 0008/0063
  with approved hardware debt; the provisional N/A suggestion is not accepted.

## Row 139 source map — Next sessions API (Adapt)

- Upstream `2989faeb`: full six-file diff and SessionController/worker context
  reviewed. GET `/api/sessions` projects registered workers as channelNumber,
  state, transcodedUntil and lastAccess. Next reports state `next`, a null
  transcodedUntil and its last touch; native workers report their live cursor.
- Next legacy trim calls now return None with a warning rather than throwing.
  Native NextSession already returns an optional owned playlist without throwing;
  its existing HLS delivery bridge must remain functional (ADR 0095).
- OpenAPI marks transcodedUntil nullable. The two scripted scheduling schemas
  also correct the existing director grouping description; native scripted
  marathon validation already lists all five grouping modes, including director.
- Closed gap: typed worker/registry reports and the application query now serve
  GET sessions through the authenticated machine router and OpenAPI.
- Targets: core HLS report/registry port, native and Next worker snapshots,
  application channel-runtime query, host runtime adapter, authenticated machine
  route and its OpenAPI schema. Preserve preview isolation and direct-response
  registration semantics; report the normal playlist-worker registry only.
- Acceptance: mixed native/Next JSON reports stable fields with Next null cursor;
  lastAccess changes on touch, listing is observational, removal is reflected,
  empty registry returns an empty array, and the API-key/listener boundaries
  remain. Native cursor reporting follows actual progress without changing
  playback scheduling. No new UI or authentication design is proposed.

## Row 138 source map — static hash functions (Covered)

- Upstream `7e141a13`: full seven-file diff reviewed. Hash inputs, algorithms
  and hex casing stay unchanged; hashing uses .NET static helpers and image
  file hashing delegates to the existing local filesystem helper. The shared
  SHA-1/MD5 objects in ImageCache/FolderEtag are removed.
- UI, validation, persistence shape and FFmpeg arguments: no upstream change.
- `ImageCache` / `LocalFileSystem.GetHash`: native artwork cache identity uses
  one-shot `Sha1::digest` in `artwork_cache_maintenance::cache_key`; native
  uploads retain their existing IDs. Native file/content hash helpers construct
  their digest locally. There is no shared mutable digest instance to replace.
- `FolderEtag`: scanner `folder_etag::md5_lower_hex` uses one-shot `Md5::digest`;
  its sorted paths/timestamps, subfolder traversal and lowercase encoding remain.
- `CollectionEtag`: `core::scheduling::block::collection_etag_for_items` owns its
  rolling fingerprint state within each call, including audio/subtitle overrides.
  `PlexEtag`: provider ETags and `plex::sha256_hex` have no shared mutable hash
  state. This row preserves those existing native fingerprints; the upstream
  refactor introduces no new byte-format requirement.
- `InternalUrlSigner`: Next `MediaRelay::credits_ticket` constructs a fresh
  SHA-256 digest per call over its immutable process capability and binding.
  Existing capability/listener decisions remain under ADRs 0095/0102; no new
  authentication decision is introduced by this hash-allocation refactor.
- `TranscodingTests`: native pipeline acceptance uses textual insta snapshots;
  the upstream test-only SHA-256 helper rewrite changes no generated command.
- Verification: three existing hash/ticket regressions pass; reuse row137's
  shared-pass evidence and unchanged UCU-08 window acceptance. Source review
  confirms no shared mutable digest in these paths. No runtime edit, new
  regression test or FFmpeg rebuild is needed. UCU-09 owns the exact evidence.

## Row 137 source map — artwork-cache healing (Port)

- Upstream `96cef081`: full five-file diff, complete cleanup/cache methods and
  `LocalFolderScanner.RefreshArtwork` audited and reconciled. UCU-09 owns acceptance evidence.
- UI/components/validators: no new screen or control. Existing scheduled and
  queued manual artwork cleanup own the operation; health must describe it
  accurately and never claim all cached files are preserved after pruning.
- Handler: delete orphaned database rows, collect live cache filenames from
  Artwork paths, all three blur hashes and ChannelWatermark images, prune unused
  files recursively, retain root/direct cache folders, then restore missing
  cached images from local jpg/jpeg/png/gif/tbn/webp sources. Empty/remote keys
  and embedded-media sources are not copied. Shared stale paths are repointed
  together when the source path/write-time key produces a different cache name.
- Cache/scanner: source path plus Windows-file-time timestamp is SHA-1 keyed;
  kind selects the cache directory and hash prefix shard. Upstream scanner
  forces refresh and bypasses cloning when an existing cache entry is missing.
- Rust: preserve the existing bounded transactional `artwork_cleanup` database
  helper. `artwork_cache_maintenance` now provides explicit-root filesystem
  pruning/healing shared by the manual worker and scheduler. Candidate unlink
  rechecks references inside a write transaction; paged snapshots avoid per-file
  database lookups for retained images. Temporary copies publish before shared
  path repointing, and local failures remain retryable.
- Cache layout follows existing HTTP readers: posters/fanart/thumbnails stay
  under `data_path("artwork")`; logo/watermark restoration uses the dedicated
  native upload locations, which are never pruned. Root/direct cache folders,
  originals, symlinks and referenced blur variants are retained.
- Native local scanners store absolute source-image paths through
  `metadata::replace_artwork`, so there is no stale copied-cache fast path.
  The local-artwork HTTP regression removes and restores an original with its
  mtime unchanged, proving immediate visibility without creating a scanner cache.
- Targets: infra artwork/cache helper and maintenance integration, host manual
  worker/scheduler composition, owning health check/text, and focused SQL/file/
  HTTP regressions. Preserve existing direct-image serving, provider routes,
  artwork cardinality, watermark/logo assets and audio/subtitle state.
- Safety/acceptance: injected disposable cache roots; no traversal or symlink
  escape; no deletion of original files; preserve referenced hashes and blur
  variants; deterministic shared-path repointing; failed restoration remains
  retryable; repeated cleanup is idempotent; manual/hourly paths share behavior.
  Existing cache capabilities must be used where present. No FFmpeg build,
  release or live-library mutation. UCU-09 owns the row acceptance.

## Row 136 source map — shared health runs (Port)

- Upstream `3c41959f`: complete eight-file diff reviewed and ported. UCU-09 owns test/browser evidence and the resolved health debt.
- UI/visual contract: `ErsatzTV/Pages/Index.razor` keeps the two-column health
  table and adds a small refresh action beside the heading. Reads use cached
  results; explicit refresh bypasses completed cache and joins an in-flight run.
  ErsatzRS keeps its accepted Support navigation (ADR 0089), localized heading,
  existing table, and an accessible refresh action with SSR fallback.
- Handler/service: `GetAllHealthCheckResults` carries Refresh;
  `GetAllHealthCheckResultsHandler` filters NotApplicable and contains caller
  cancellation/disposal. Singleton `HealthCheckService` owns one run independent
  of page cancellation, caches successful results for five minutes, and limits
  execution to fifteen minutes. Timed-out results are not cached/published;
  notification failure cannot stop the host.
- Repository/side effects: health checks read the library and may start FFmpeg;
  the result set and summary must remain coherent. Scoped upstream checks are
  created within the service-owned run. Orphan cleanup logging changes from
  Information to Debug; Rust cleanup already returns a report without those
  repeated information logs.
- Rust targets: `ersatzrs-core/src/health.rs`, `ersatzrs-infra/src/health/mod.rs`,
  `ersatzrs-infra/src/maintenance.rs`, web `host/health.rs`, `host/tasks.rs`,
  `host/state.rs`, app health port/use cases, `admin/server_fns/layout.rs`,
  `admin/pages/system/health.rs`, and every supported language as needed.
- Audited pre-port gap: `load_health_snapshot` started a fresh run; SSR and
  hydration each invoked it, and maintenance constructed a separate runner.
  Five-minute caching, shared ownership, a deadline and manual refresh were absent.
- Preserve existing media-revision refresh, check order/failure rows,
  persisted summary badges, troubleshooting, audio/subtitle checks and native
  platform support. No FFmpeg release/build or runtime-package change.
- Hydration source map: `/system/health` keeps `HealthPage` as its SSR wrapper;
  `Suspend` resolves `load_health_snapshot` before emitting `HealthIsland`
  with its finished snapshot. No unresolved Suspense boundary wraps the island
  or its table. `load_health_snapshot` reads the shared cache; `refresh_health_snapshot`
  forces a run for the refresh button and subsequent media-revision changes.
  The ordinary POST form `/admin-api/system/health/refresh` redirects back to
  the health page without JavaScript. Existing table, toast and Support layout
  primitives remain in use; the native submit button reuses the existing button
  class helper without changing primitive behavior. Browser proof covers Chromium and Firefox with
  hydrated and no-JS refresh and navigation away.
- Required proof: deterministic concurrent/cache/expiry/forced refresh and
  waiter-cancellation tests; timeout and publication-failure recovery;
  persisted result/summary consistency; localized hydrated/no-JS refresh,
  navigation-away behavior, and strict affected lint. UCU-09 owns acceptance;
  health parity debt `ucu-shared-health-run-cache` is resolved; UCU-09 owns the evidence.


## UCU-08 row 135 source map: release preparation (N/A)

- Reviewed the complete single-file `5bb3ddfc` diff. It inserts ErsatzTV
  26.9.0's release heading/date and updates two comparison links. No source,
  package, schema, dependency or runtime behavior changes.
- ErsatzRS maintains its own release versions and Unreleased notes. Copying an
  upstream release heading would incorrectly imply an ErsatzRS release.
  Verdict: N/A. Existing implementation evidence through row134 is unchanged.
- Pin and ledger advance together, then the clean reference is aligned.
  UCU-08 remains In progress until its window-wide gates, authorized push and
  terminal exact-SHA CI pass. UCU-09 starts only after that acceptance.

## UCU-08 row 134 source map: provider season scanning (Port)

- Reviewed all 12 upstream `188274e8` files. Emby episode responses with a
  non-null different SeasonId are excluded; Plex/Jellyfin/Emby update an
  existing episode's season even when its etag is unchanged. Season artwork
  falls back to the show poster; media content layers stay above fanart.
- Backend targets: `emby/{models,client,sync}.rs`, `jellyfin/sync.rs`,
  `plex.rs`, shared provider helpers, and `repositories/television.rs`.
  Confirmed all three episode etag branches skip parent repair. Emby response
  lacks SeasonId and filters only through project_episode. Its scan loop also
  stops on an empty projected page: filtering specials must not discard later
  valid pages. Scope parent repair by provider identity and library; preserve
  metadata, files, stream selection and etag fast paths.
- Confirmed season-detail projection selects only SeasonPoster; no show-poster
  fallback. It already rejects blank artwork paths. Extend existing projected
  artwork identity handling, preserving provider URL/auth resolution.
- Visual map: upstream Movie/Artist/TelevisionSeasonList/TelevisionEpisodeList
  add relative content layers above absolute fanart. Rust season shell already
  has fanart z-index 0 and content z-index 2 enclosing header and episode stack.
  Shared movie shell likewise has content z-index 2. Finish auditing show/movie/
  artist descendants and prove missing-season-artwork behavior in the browser;
  do not add redundant styling without a demonstrated gap.
- Required evidence: provider HTTP/database regressions for unchanged-etag
  parent repair and library scoping; Emby mismatched/null SeasonId and filtered
  page continuation; season poster precedence/blank fallback plus browser
  visibility; focused lint and substantive review. No schema, new provider
  infrastructure or runtime release. Pin remains row133; implementation next.
- Three provider HTTP regressions reproduce unchanged-etag parent failures
  (`65a1691f`). Shared library-scoped parent repair now runs before successful
  etag skips; all three pass (`9a43b23c`) without extra playback lookup or file
  changes. A database regression proves invalid/cross-library parents are
  ignored and repeated repair is a no-op.
- Emby specials filtering and blank/missing season-poster fallback reproduce
  (`6f756666`). Filtering retains provider totals and scans past fully filtered
  pages. Artwork falls back through the existing projected-artwork path.
  All ten focused tests pass (`2abae677`): the three provider scans, Emby
  response filtering/pagination, parent isolation/idempotency, and all five
  television repository tests. EXPLAIN uses primary-key and existing
  ShowMetadata/SeasonMetadata/Artwork owner indexes, with no artwork table scan.
- Hydration map: `/media/tv/seasons/:id`, `SeasonPage` and `SeasonDetailIsland` use the unchanged server-function/repository data boundary.
  Only artwork projection changes; no island props, routes, localization or
  shared styles change. Movie/show/artist content and episode stacks already
  sit inside relative z-index 2 containers above fanart at z-index 0.
  The focused cardinality spec now checks missing/blank poster fallback,
  loaded fanart and title hit testing with and without a show poster.
  Strict infra library/test Clippy passes after factoring a test fixture and
  documenting test-helper panics; no suppressions were added. Substantive
  backend review passes. Browser verification is running via matched Leptos
  build and isolated port18474; no row verdict/pin move.

- Final acceptance: matched Leptos SSR/WASM build passes. Chromium and Firefox
  pass all four focused cardinality/artwork cases in 20 seconds, with no retries
  or changed timeouts (`test-results/ucu08-row134/playwright-results.json`).
  The server log confirms published v2.0.2 FFmpeg/ffprobe 9.0.0 overrides and no
  download. Both absent and blank season posters fall back; without any poster,
  title hit testing still passes above loaded fanart. Existing 27-episode
  uniqueness and reload tests remain green. Backend/source review and strict
  infra library/test lint pass. Verdict: Port. Refresh confirms upstream target
  unchanged at 8bda4411. Commit pin/ledger together and then align reference.
  Row135 and milestone-wide gates remain; no intermediate push/CI.

## UCU-08 row 133 source map: local folder etags (Port)

- Reviewed all 19 files in upstream `4a2aa157`: movie/season etags publish
  only after all file processing and indexing succeeds; image/music-video/
  other-video scanners also count indexing failures. Cancellation must not
  publish a partially processed folder. .NET dependency and solution updates
  have no direct Rust package equivalent.
- Rust target: scanner `local_television.rs`. Confirmed unconditional season
  etag publication after episode errors; malformed episode NFO is swallowed
  as fallback and also permits publication. Other local media kinds have no
  folder-etag skip and retry every scan; search reads committed SQLite metadata
  directly, so no external indexing acknowledgement exists here.
- Outcome: a failed local episode or malformed NFO remains retryable without a
  file touch, while successful folders retain their fast path. Preserve
  fallback playback, sibling processing, metadata/track ownership and IDs.
  Exclusions: no new cache, search service, dependency upgrade or schema change;
  row134 season scanning remains separate.
- Required evidence: real scan with mixed success/failure, unchanged retry,
  malformed-NFO repair with preserved mtime, successful skip, focused scanner
  tests/lint and source review. Pin remains row132.
- Regression `f8cf9940` reproduces erroneous etag publication after a database
  write failure. The correction counts failed episode/extra writes and retains
  metadata completeness separately from playable fallback success. Publication
  remains after all awaited writes; cancellation cannot reach it midway.
- All seven real local-TV scan tests pass (`0490d38f`), including mixed sibling
  success/error, unchanged retries, malformed NFO repair with preserved mtime,
  healthy fast paths, metadata/child rows, remake repair, trash restoration and
  decoded subtitle-language restoration. Published v2.0.2 FFmpeg/ffprobe used.
  Ten focused local-TV unit tests pass (`c4ec5adb`). Strict scanner lint is
  running. Source review confirms other media kinds rescan without folder etags;
  SQLite-backed search needs no external index acknowledgement. No new health
  capability or state is introduced; existing identity/availability checks apply.

- Final acceptance: strict scanner library/test Clippy and substantive source
  review pass. Verdict: Port. Existing fallback playback is preserved while
  failed metadata remains retryable. No schema, dependency or browser change.
  Commit pin/ledger together, then align the reference checkout. Row134 and
  milestone-wide gates remain; no intermediate push/CI.

## UCU-08 row 132 source map: concurrent channel startup (Port)

- Upstream `d5f113159f76120adee51b2c7c91c6a5ed583c01`: all 11 changed
  files reviewed. Native/Next handlers share SessionStartCoordinator; readiness
  waits occur outside admission locking, existing-worker termination permits
  one recovery, cancellation/deadline do not restart healthy workers, and
  readiness belongs to the exact registered session. HlsSessionWorker requires
  at least one segment and delegates its deadline to the caller.
- Targets: web `transport/iptv/hls.rs` admission/readiness/preview routes;
  infra `hls.rs`, `hls_worker.rs`, and Next host/session; core HLS handles.
  Atomic real-worker registration already avoids duplicate starts and losing
  constructors own no process. HTTP disconnect drops only its waiter (row130).
- Confirmed gaps: playlist I/O can complete with stale readiness after worker
  removal, and variant tune-in has no bounded existing-worker recovery path.
  Ordinary native sessions also re-poll playlists on every tune-in instead of
  retaining session readiness. Audit exact identity, shared readiness and
  replacement behavior before verdict.
  Upstream ReleaseMemoryHandler only corrects a .NET GC admission condition;
  Rust has deterministic ownership and no equivalent managed GC operation.
- Required evidence: simultaneous waiters, cancellation/retry independence,
  old/replacement identity, one recovery only, no timeout-triggered restart,
  playable segment floor, focused lint and affected native/Next runtime checks.
  Preserve track selection and capacity behavior. Pin remains row131.

- The stale-playlist race reproduces (`cfe9e3de-bfe7-4445-b9e4-7154098f3ec6`):
  a removed worker returned Ready after blocked playlist I/O resumed. Registry
  entries now own readiness; publication checks exact worker identity under
  the write lock. Waiters remember the initial worker, cache completed
  readiness and require at least one segment. New tests cover stale publication,
  established-session reuse, replacement isolation and a zero configured count.
  Focused web tests are running. Bounded recovery and final acceptance remain.

- Initial readiness changes pass 23/24 focused web tests (`71c9abbf`),
  including stale-result rejection and replacement/cache isolation. The sole
  failure is the existing HTTP-disconnect test's denied sandbox loopback socket.
  Shared admission/readiness coordination is now wired into variant, preview
  and hybrid paths (native and Next). It permits one existing-worker recovery,
  preserves the original deadline and never retries timeout/capacity denial.
  Pending playlist reads observe registry removal without waiting for the full
  startup deadline. Deterministic replacement-success, replacement-ending and
  no-timeout-restart regressions are added; the authorized loopback-capable
  focused test run is compiling. Full concurrency/cancellation coverage and
  final lint/runtime review remain; no row verdict or pin advance.

- The authorized focused run passes all 26 HLS tests
  (`e96d6e7f-a52b-42d8-b019-7a0ba1815180`), including real HTTP disconnect,
  bounded pending-read removal/replacement, no repeated recovery, no timeout
  restart, and ready-cache isolation. Strict core/infra/web lint is running.
  Concurrent-viewer/recovery interactions and native/Next runtime acceptance
  remain before verdict.

- Resumed verification passes all 28 HLS tests
  (`90673701-4aca-414b-98b0-dc32924aa356`), including simultaneous waiters,
  one-viewer cancellation and adoption of another viewer's replacement.
  Strict core/infra/web library/test Clippy passes with the final test set.
  Three real routed Next lifecycle tests pass
  (`37382233-ff7b-4b63-90b0-9cd8dd8af1d4`): authentication/cleanup,
  disconnect reaping, and independent normal/preview producer lifetimes.
  Temporary artifacts had disappeared across the pause; published FFmpeg
  v2.0.2 was restored and its archive SHA-256 again matches
  `ef797ab1ad7c54340b2b92c208d1ecd7223c0941a68d831e1154aea16ba33e59`.
  The official Next artifact now carries revision `4cc93e4`, archive SHA-256
  `dbfb525bf6875cc28e53c7e4b95b169808671a022831a5f3dda7631706b5e31b`;
  repository staging verified origin and digest. No runtime was rebuilt.
  Native lifecycle smoke remains pending after interruption of its harness;
  its surviving Cargo build is being allowed to finish before rerunning.
- The refreshed upstream target is `8bda4411`; the existing 178 ordered
  SHA/subject pairs remain an exact prefix. Rows 179-180 are appended as
  pending UCU-09 build-workflow audits and are not processed here.

- Final acceptance supersedes interim pending notes above: native lifecycle
  smoke passes with playlist/segment delivery, idle cleanup, MPEG-TS wrapper
  restart and final cleanup. Artifact: system temporary directory
  `ersatzrs-smoke-9846-1790608956/ersatzrs-web-smoke.log`.
  All 28 HLS tests, three real Next lifecycle tests, affected strict lint and
  substantive source review pass. Row131 TSDuck 87/87 remains applicable to
  unchanged media generation; this row changes session coordination only.
  Verdict: Port. Registry-owned readiness and one bounded existing-worker
  recovery cover native/Next variant, preview and hybrid paths. No timeout,
  cancellation, capacity or selected-track acceptance was weakened.
  Pin and ledger advance together; align the clean reference after commit.
  Row133 and UCU-08 window gates remain; no intermediate push/CI.

## UCU-08 row 131 source map: multi-day seeking (Port)

- Upstream `6ebcaf800d05082ac22bcff7394d6e1d592d5c59` changes eight files:
  FFmpegFormatter, input/output seek and time-limit options, pipeline snapshots,
  LocalStatisticsProvider idet sampling, MotionElement timing and changelog.
- Native input/output seek and time limits use options::format_time_limit:
  total hours (`as_secs() / 3600`), never a day-prefixed C# TimeSpan string.
  Add focused boundary/command regressions and real FFmpeg duration parsing
  evidence before calling that portion Covered. Keep current submillisecond
  precision; this row does not require changing valid argument syntax.
- Native motion graphics use an in-process overlay/filter graph under ADR 0005,
  with numeric-second setpts/enable expressions and no external renderer pipe.
  Verify multi-day motion timing alongside the command formatting audit.
- Confirmed missing path: settings persists ffmpeg.probe_for_interlaced_frames,
  but no native playback consumer reads it or invokes idet. Upstream native
  playback, when profile deinterlacing is enabled and container metadata is not
  already interlaced, uses a cached ratio or samples 200 frames at duration/3
  for local files of at least three seconds; ratios above 0.05 are interlaced.
  Remote/missing/AviSynth-unavailable inputs are skipped. Scanner reprobe clears
  cached ratios. This pre-existing gap is relevant to the changed multi-day
  sampling path and must be implemented and verified before this row lands.
- Targets: ffmpeg options/command regression tests; infra playback and bounded
  local process probing; native MPEG-TS/HLS and troubleshooting consumers.
  Preserve selected version/stream identity, cancellation and cached-state
  invalidation. Do not probe unrelated streams or remote provider URLs.
- Focused duration and command-order tests pass
  (`762ba3e0-ab3a-4a54-a6aa-b0bc641e259f`): 24/25/48/100 hours, fractional
  precision and input/output argument placement. A synthetic 53-hour FFV1
  Matroska clip (one uniquely shaded frame per hour) proves both input-side and
  output-side 48-hour seeks with a 100-hour limit decode the exact frame 48.
  Pinned v2.0.2 FFmpeg output SHA-256 for both and the select-filter oracle:
  `7d2f96a1df0d5d799a21b83063dca880b68738a5c5bef4d01760939fecddbbc8`.
  The source fixture is in system temporary directory ersatzrs-ucu08-row131.
- Playback integration target: enrich_source_video_stream_for_playback is
  shared by native MPEG-TS, HLS and both troubleshooting paths. It currently
  only probes QSV H.264 profile count. The new idet path needs the selected
  version, stream, resolved local file and profile deinterlace setting; reuse
  this runtime boundary without adding subprocess work to metadata-only reads.
  Upstream Next schedule projection uses container field order independently;
  do not silently impose native-only probing on that projection.
- Initial implementation now connects the setting to the shared native
  enrichment boundary and all four consumers. Sampling maps only the selected
  video stream, seeks at duration/3 using the total-hour formatter, and captures
  bounded idet statistics through the existing managed process-group/job owner.
  A ten-second deadline includes admission, reads, sampling and publication;
  a process-local gate serializes optional probes and rechecks cached state.
  Only the sole original file and primary video stream can use/publish the
  version ratio. Alternate streams and replacements are sampled without
  poisoning that shared cache. Conditional publication guards the scan timestamp,
  duration, file ownership and primary stream; filesystem size/mtime are rechecked.
  Existing FFmpeg Capabilities health reports missing idet only when enabled.
  Initial infra/web type-check passes. Three cache/parser/health tests pass
  (`ba141abc-5297-44bd-870a-a2b73468bfb3`); three multi-day argument/snapshot
  tests pass (`de4a74ff-420a-4e24-b531-1b0ee239bdda`). Real verified-v2.0.2
  idet sampling passes (`0b1cd87a-8f1d-4942-8756-fccafc2d94b9`): two selected
  streams are classified separately and alternate sampling preserves the
  primary cached ratio. The first synthetic control (testsrc2) was itself
  classified interlaced by standalone idet; testsrc supplies a verified
  progressive control without weakening the 0.05 threshold. Race/cancellation,
  motion timing and integrated runtime acceptance remain; this is not a
  completion claim. Strict FFmpeg/infra/web library/test Clippy passes. HLS now selects cancellation around enrichment so
  Stop drops the owned probe future and follows normal worker cleanup instead
  of waiting for the optional sampling budget.
- Conditional cache publication now has a SQLite race regression covering
  scanner timestamp/duration changes, replacement/additional files, primary
  stream removal and another cache publisher. Three focused tests pass
  (`c714a3cb-ae02-4be2-8377-d0df3d50caf0`). A Unix process fixture verifies
  nonzero exit, unbounded stderr and stalled execution return bounded errors
  (`d04e333a-6af4-4469-af57-24962ec86a57`). These checks do not yet prove
  dropped-future process-tree cleanup or integrated HLS Stop behavior.
- Dropping the actual probe future terminates its delayed descendant in the
  Unix process fixture (`b988efe0-68f0-4c07-a195-b6e7b18add07`). Windows uses
  the existing managed-job owner; this run does not claim Windows runtime proof.
  Native motion rendering preserves total seconds for a 48-hour fractional
  start, 25-hour source duration and 24-hour fractional hold; the inline
  snapshot passes (`97a1c33e-2f8a-437d-8cdf-1d74c40f10ee`). This follows
  ADR 0005's native filter path, with no external motion renderer added.
  Integrated HLS Stop and playback acceptance remain open.
- Added `cargo xtask smoke --interlace-probe --keep-temp`: generated interlaced
  content marked progressive, enabled native detection, and cache reset/assertion
  for each exercised MPEG-TS/HLS path. Strict xtask lint passes. The v2.0.2
  package checksum was reverified before launch; the first runtime run is
  still building the web host after target cleanup, so no runtime pass is claimed.
- Integrated HLS Stop during sampling passes
  (`766b8bb0-95e5-4ebc-a4ca-3af2ca275ba2`): real worker/SQLite/registry,
  observed probe admission, Stop, prompt output-directory cleanup, absent
  registry entry, no cached ratio publication, preserved source file and no
  surviving delayed descendant. The initial fixture used RFC3339 timestamps
  rather than the repository SQL formatter; correcting that fixture admits
  the item without weakening assertions or deadlines.
  The smoke build passed, but sandbox loopback access was denied before health
  verification. The authorized rerun with local networking is pending.
- First authorized HTTP probe run served both MPEG-TS modes and published
  ratio 0.9950248756218906. Its later HLS cache assertion failed because hybrid
  MPEG-TS had already started the shared HLS worker. The harness now requires
  separate `--skip-hls` and `--skip-ts` invocations with disposable hosts;
  cache assertions remain unchanged. This fixes test isolation, not playback.
  Terminal smoke acceptance is still pending those separate runs.
- Both isolated native HTTP smoke runs pass with the checksum-verified v2.0.2
  package: MPEG-TS legacy/hybrid (`ersatzrs-smoke-1102236-1790546796`) and
  HLS variant/session playlists (`ersatzrs-smoke-1106284-1790546856`). Each
  sampled from an empty ratio cache and published 0.9950248756218906. Logs are
  `ersatzrs-web-smoke.log` in those system-temporary directories; no known
  runtime failure signatures. Strict infra/xtask library/test lint passes.
  TSDuck transport acceptance is running; final row review/verdict remain.
- Final acceptance supersedes interim pending notes above: TSDuck passes all
  87 checks with no failures (`tsduck-gate.vRQ8xg` in the system temporary
  directory). Source review confirms scanner ffprobe projection initializes
  `InterlacedRatio` to None and the transactional update overwrites the old
  ratio while advancing DateUpdated. Playback probes do not change audio or
  subtitle selection; the existing source projection and caller order remain.
  No new dependency, schema or graphics renderer is introduced. Existing
  managed process ownership supplies Unix groups and Windows jobs. Verdict
  Port: valid total-hour seek formatting is retained, native motion follows
  ADR 0005, and the missing configured probe is implemented and verified.
  Pin advances with this row commit; align the clean reference checkout after
  commit. Row 132 and milestone-wide gates remain. No intermediate push/CI.

## UCU-08 row 130 source map: channel startup (Port)

- Upstream `1681d1ad3882ceb37bb215fc3e2e1cd85fa6e68b` changes both
  StartFFmpegSession handlers, FFmpegSegmenterService, its interface, new
  SessionStartWait/StartLockReleaser, two test suites, IptvController,
  request-cancellation middleware and release notes.
- Required behavior: one worker per channel, no null reservation after failed
  validation, safe folder preparation, identity-checked cleanup, bounded playlist
  readiness that ends with the worker, and cancellable request waits.
- Native targets: core HLS registry/handle contracts, infra registry and
  hls_worker, Next host/session, web transport/iptv/hls and request lifecycle.
  Rust admission already atomically registers a real handle before spawning or
  preparing folders; concurrent losers touch the winning handle. Further audit
  must verify readiness and client cancellation across both engines.
- Confirmed gap: native and Next cleanup remove by channel number. An old
  worker can remove a replacement registration. Add atomic identity-checked
  removal and regressions for stale, repeated and current-owner cleanup.
- Identity-checked cleanup now covers all native exit branches and the Next
  host cleanup path. All ten registry cases pass
  (`1102cea4-0021-4e86-8ab1-f005e257ef30`), including replacement protection,
  repeated cleanup and independent direct viewers.
- Stalled playlist I/O reproduced an unbounded readiness wait
  (`bc301f54-7f53-4deb-aa0b-92577f8c82b1`): a 20 ms startup deadline still hung
  past the regression's one-second outer guard. Registry/playlist reads and
  polling now share one deadline. The same correction covers the capacity-slate
  wait. All 21 HLS route tests pass (`e685b45b-f37a-4abd-bfc9-7d55dce7ea5e`),
  including both stalled-read paths, capacity, ready/failed/missing handles,
  token preservation and Next routing. Nine native/Next lifecycle tests pass
  (`d82fa5fe-34af-438a-9998-19d4933919ae`) for terminal deadlines, cancellation,
  early exit, idle cleanup, stale playlist suppression and folder preparation.
  Strict core/infra/web library/test lint passed before the capacity extension;
  final web library/test lint also passes after it. The later HTTP regression
  and runtime results below complete the request-disconnect and playback checks.
- A real loopback HTTP disconnect before response headers drops the blocked
  readiness future without cancelling or unregistering the shared worker
  (`c3786ef8-bd77-4d81-a832-192b4239c0c6`). This is the Rust counterpart to
  request-token propagation and upstream cancellation-exception handling: the
  request future is dropped, with no detached polling task or error response.
- The real Next HLS fixture passes (`d0e92f99-cc5f-497e-ad7d-43b07c7ac116`):
  capacity precedes process/output creation, duplicate starts reuse the worker,
  stop removes files/registry/capacity, previews remain isolated, and decoded
  selected audio and external subtitles survive. It uses the existing official
  Next df41692 executable and checksum-verified ErsatzRS-ffmpeg v2.0.2 package.
- Substantive source review: TryAddWorker/null-placeholder changes map to
  atomic real-handle admission before spawning in both native and Next paths;
  failed preflight therefore cannot strand a null reservation. Rust owned
  handles/lock guards replace StartLockReleaser disposal. Identity removal is
  atomic under the registry write lock. SessionStartWait maps to bounded inline
  readiness polling and terminal Next/native startup state; dropping a wait
  leaves the shared session running. Folder preparation occurs only for the
  winning registered worker. Interface changes and both upstream test suites
  are covered by the registry, lifecycle, route and HTTP/runtime evidence above.
  Changelog formatting has no additional executable behavior. No new deviation.
- Native real-host lifecycle smoke passes on disposable data: HLS master,
  variant and nonempty segment; idle cleanup; MPEG-TS wrapper restart and idle
  cleanup; no known runtime failure signatures. Canonical local artifact:
  `ersatzrs-smoke-1037398-1790543273/ersatzrs-web-smoke.log (system temporary directory)`.
  Bootstrap confirms both FFmpeg and ffprobe 9.0.0 came from the pinned v2.0.2
  package with no runtime download. Final core/infra/web library/test Clippy
  passes including the HTTP regression; window-wide acceptance stays separate.
- Upstream refreshed to `f8410286`; the 176 previous ledger SHA/subject pairs
  remain an exact prefix. Rows 177-178 are appended without processing them.
- Exclusions: no new streaming engine, FFmpeg build, authentication change or
  browser UI. Verdict: Port, with no new deviation. The row source review,
  focused checks and native/Next runtime proof pass. Advance the pin with this
  row, then align the clean reference checkout; row 131 is next.

## UCU-08 row 129 source map: scanner tag ownership (Adapt; accepted ADR 0104)

- Upstream `9067cde8` changes 31 files. Two large generated EF designer snapshots
  are model bookkeeping; substantive changes include typed Tag owners, ordered
  legacy backfill, all tag insert paths, three collection repositories, scoped
  Plex network removal/insertion, Plex label projection/ownership, local NFO
  country comparison, UI/search filtering and eight ownership tests. The full
  non-generated diff still needs final per-file review before implementation.
- User outcome: library, collection and network refreshes preserve one another's
  tags; renamed/deleted labels and collections disappear only from their owned
  rows. Networks and NFO countries remain distinct from ordinary searchable
  tags, while typed labels/collections continue matching tag-based schedules.
- Confirmed native gaps: `infra::metadata::replace_strings` deletes every Tag
  for the metadata owner; `plex::replace_plex_metadata_facets` uses it. Plex
  projection keeps collection names as untyped tags, maps Label to music labels,
  and `PlexTagResponse` discards IDs. Collection persistence in Plex and Jellyfin
  currently replaces CollectionItem membership without typed Tag projection.
  `admin_media` exposes only untyped show tags; scheduling's generic tag reader
  does not distinguish owner types. Network reads use type 319, but the provider
  network producer/refresh needs a full audit. NFO parsing and native search lack
  country handling in the inspected modules. Emby and all other media writes
  still require mapping before choosing the shared implementation boundary.
- Invariants: one metadata-owner/type/external-ID/name identity per tag;
  repeated scans are idempotent; removing one producer's rows preserves other
  owners even with equal names or colliding provider IDs. Scope provider media
  lookups to the owning source/library. Keep real collection membership,
  scheduling checkpoints, local playable media and audio/subtitle state intact.
- Required acceptance: synthetic provider projection and real SQLite mixed-owner
  refresh/rename/removal regressions across Plex/Jellyfin/Emby and metadata kinds;
  repeated writes and restart; legacy backfill preserving existing network/NFO
  tags; same-name distinct-owner cases; source/library collision isolation;
  NFO country and tag/network/country search/UI projection; membership-driven
  refresh continuity; relevant health/schema readiness and strict affected lint.
  Browser proof is needed for changed visible search/detail behavior. Do not
  use live provider data or credentials, create runtime artifacts, weaken tests,
  or advance the row verdict/pin before acceptance.
- First regression reproduced destructive ordinary replacement: all five typed
  or externally identified rows disappeared (Nextest
  `2830c2bb-0af8-4282-92ed-db35b82d3036`). The shared writer now deletes only
  rows with both ExternalTypeId and ExternalCollectionId NULL. This preserves
  other producers while replacing/deduplicating ordinary tags, including empty
  refreshes. All 19 selected metadata tests pass (Nextest
  `6e6f9980-6dca-48d2-8510-4c415ce38c17`). Strict infra library/test Clippy passes.
- Initial mapping found PlexShowItem/PlexSeasonItem discarded labels, and show
  upsert did not persist ShowMetadata facets. No native network-scan producer was
  found. Plex/Jellyfin/Emby collection sync replaces membership but has no
  collection-tag removal/reconciliation path. These are required producer gaps,
  not grounds for dropping ownership acceptance. Tag columns already exist;
  the upstream migration is an ordered data backfill, not a schema addition.
- Plex movie/episode/other-video projection now retains label IDs (numeric and
  string JSON IDs canonicalize to the same string). Library-owned tags use the
  new `plex/tag_ownership.rs` reconciliation, including legacy untyped labels.
  Existing same-ID/name labels retain row IDs; duplicate incoming identities
  collapse, renamed/deleted labels are removed, other owner types are untouched.
  Existing MusicLabel projection is retained. Collection names are no longer
  misrepresented as library-owned tags; proper collection projection remains
  part of this open row and must land before row acceptance.
- All 15 Plex service tests pass (Nextest
  `e22b5f63-3e2d-4fc9-aa56-4d6059d79123`). The new HTTP/SQLite regression checks
  two identical scans per phase, equal numeric/string label IDs, rename and
  removal, preservation of five other owners sharing a name/external ID, stable
  label row IDs, database close/reopen and unchanged media-stream count. Initial
  lint found one unused mutable fixture binding; corrected strict infra library
  and Plex service lint passes. Formatting and diff checks also pass.
- Show and season projection now retains label IDs and provider timestamps.
  The shared container writer creates missing metadata and updates title/time
  without clearing unrelated fields or facets, then reconciles only labels.
  The new mixed-owner provider test proves one metadata row per container,
  stable label IDs across repeated scans, stale legacy label removal, and
  retained network/collection tags. All 16 Plex service tests pass (Nextest
  `4b246622-6c02-4224-97c6-9eeb5919ffbf`). Fixture corrections use the existing
  SQL safety wrapper and matching synthetic HTTP token; strict infra library and
  Plex service lint passes.
- Network source map: upstream PlexNetworkScanner pages `/library/tags?type=319`,
  parses each Directory tag Filter, and pages `/library/sections/{key}/all`
  with that network filter. Per-library show membership determines owned type
  319 insertions/removals; changed shows and episodes are reindexed. Native
  implementation must preserve request/source confinement and durable refresh.
- Collection membership replacement now snapshots prior members and reconciles
  typed tags before committing for Plex/Jellyfin/Emby. The new collection_tags
  module uses provider type plus external collection ID on affected metadata;
  unchanged rows retain IDs, membership removal deletes only owned tags, and
  existing names update in place. Ordinary labels/networks remain independent.
  This covers membership refresh only: deleted collection discovery and source
  collision handling still require implementation before row acceptance.
- All 35 provider service tests pass (Nextest
  `38030e5f-ed31-4351-a87b-be25c4a329c3`). Expanded collection fixtures for all
  three providers assert typed external IDs, stable repeated-sync IDs, removal
  on empty membership and preservation of an equal-name/equal-ID network tag.
  SQLite EXPLAIN on the initial schema confirms metadata-owner and Tag FK index
  searches for all five metadata kinds; membership probes use the CollectionItem
  primary index, with only the supplied JSON member list scanned. Strict lint
  passes after documenting the shared fixture helper's panic behavior and
  extracting the enlarged Plex assertion sequence into a focused helper.
- Provider member collision regressions first reproduced wrong-source membership
  in all three providers. Collection member lookup now joins through Library and
  requires the requested MediaSourceId. All 35 provider tests pass (Nextest
  `0ef871b0-c382-4e77-acc8-a8552bd58023`). Collection container identity still
  needs source isolation; this result covers member lookup only. Strict infra
  library/test lint passes after extracting a shared assertion helper; the final
  35-test rerun passes (`aa0a32a2-c618-4216-bc5b-129bc22f501b`). Tests require
  loopback socket permission for synthetic HTTP servers.
- ADR 0104 proposes source-owned collection identities, conservative legacy
  adoption and discovery-root-scoped removal without deleting scheduling
  references. Decision is Proposed; schema implementation awaits acceptance.
- NFO audit: upstream MovieNfoReader retains repeated country elements and
  LocalMetadataProvider projects them as nfo/country. Native movie parsing now
  retains them; a scoped writer preserves unchanged IDs and other producers.
  The new real SQLite regression failed with no country rows before the fix;
  all ten selected movie tests now pass, including replacement, sidecar removal,
  equal-name ordinary tags and transaction rollback. Country/search acceptance
  is still in progress.
- Search/detail source map: `/search` uses SearchPage and SearchIsland,
  with SSR query form and the search_results server function; scheduling's
  SearchDocument projection owns the database field reads. ShowPage at
  `/media/tv/shows/:id` loads get_show_details into ShowDetailIsland;
  SqliteAdminMediaRepository owns the production tag/network projection.
  Existing fact-row labels remain localized. No island shape, shared primitive
  or interaction changes are required. Browser proof must show typed collection
  and label tags, separate networks and country search with hydrated and no-JS
  routes; the legacy test-only SQL projection must agree with production.
- Search parser and evaluator now expose country. SQLite hydration reads only
  nfo/country for that field and excludes nfo/country/319 from ordinary tags,
  including inherited show tags. Production and legacy test-only detail readers
  retain typed labels/collections. All 35 scheduling repository tests pass
  (`bde8da88-00dd-4d6a-a06f-cecb00368855`), including movie/show details and
  country/tag/network separation; 18 core search tests pass. Strict core, infra
  and scanner library/test lint passes. Web compilation/browser proof remains.
- Plex network discovery now pages `/library/tags?type=319` and filtered show
  memberships, with per-request deadlines and complete, consistent page totals
  required before writes. Reconciliation scopes metadata to the current library
  and owns only each returned network's name/type-319 pair, matching upstream;
  other network names and producers remain independent. Unknown show keys do
  not create metadata. Unchanged tag IDs survive. Tag writes and LastNetworksScan
  commit together. TV-library dispatch runs network refresh after show metadata.
- All 25 Plex service/source-reconciliation tests pass
  (`2ac1d91c-bbc8-412a-b4e5-0b2a2484fc9f`). Network regressions cover colliding
  keys on another server, other network/label/collection preservation, repeated
  sync, unknown shows, 101-member pagination, HTTP failure, truncated membership,
  timestamp-write rollback and normal library dispatch. Scheduling search sees
  network membership additions/removals directly from SQLite without a separate
  index rebuild. Live Plex-server acceptance is not claimed by these fixtures.
- Web consumer checks pass all 40 media-browser tests
  (`7fbc56e0-93a8-4520-a5fe-9efebc117210`). The first four browser cases passed
  hydrated Chromium/Firefox but reproduced a persistent Loading state without
  JavaScript on `/search`. Route-local InOrder SSR now supplies resolved search
  results in the response, following the existing settings/remote-detail pattern.
  All four cases now pass in `test-results/ucu08-row129-tags-final`, with the
  same assertions and timeouts. The runner used a matched Leptos bundle and the
  checksum-reverified v2.0.2 FFmpeg package on an isolated database.
- ADR 0104 remains Proposed. Its local identity implementation adds nullable
  source ownership to all three provider collection tables and unique claimed
  identities. Legacy ownership is backfilled only from complete, single-source
  membership of the matching provider, with duplicate candidate identities left
  unclaimed. Collection IDs, membership rows and audio/subtitle overrides survive.
  Three SQLite migration tests pass (`37dc812d-779d-40d6-b06e-5f12b295c522`),
  covering fresh schema, upgrade ambiguity, uniqueness, provider foreign keys,
  transaction rollback, restart and source deletion without collection deletion.
- Shared source-scoped upserts preserve unclaimed collections and distinguish
  colliding display names while retaining provider names for tags. Two focused
  unit tests pass (`297bb23f-0e2e-4b49-80f7-61984a0c6b03`), including rename
  isolation, stable IDs and failed-write rollback.
- Public sync regressions for Plex/Jellyfin/Emby use two source identities with
  equal external IDs and names. Retained foreign-source membership exposed tag
  deletion in all three (`75fcb242-50b7-4b77-9704-1e46f0e6c1f0`): metadata owner
  plus provider/key did not prevent a previous foreign member from losing tags.
  Tag reconciliation now also joins the collection's owning media source.
  All eight focused collection cases pass
  (`1ae13418-4a25-4608-af2b-36b93e00457d`) with exact tag-ID preservation.
- Legacy tag backfill now follows collection-first classification within the
  metadata owner's source. It covers all ten metadata kinds, preserves existing
  types and ordinary tags, and limits Plex-label fallback to a uniquely
  identified Plex source without an ambiguous unclaimed collection match.
  This deliberate source-isolation adaptation is described in Proposed ADR 0104.
  Orphan/multiple-owner tags, empty external IDs and ambiguous source subtypes
  remain unchanged. The migration does not modify IDs, names or owner links.
- Six focused migration tests pass (`40c2ea4a-a22c-43c5-b165-0c133e03ce61`),
  including the combined upgrade from before both new migrations: source-owned
  collection backfill precedes tag classification, with duplicated legacy
  collections still unclaimed/untyped. The tag matrix covers all metadata kinds
  and providers, colliding keys, local rows, preserved country/network/custom
  types, failed-update rollback and persisted restart behavior. The missing
  migration first failed both tag regressions (`0ea1ccf1-8e70-4f13-8b82-a257261c074d`).
- Complete-root discovery now gathers provider pages before a shared write
  transaction and records per-provider/source/root claims. Raw page counts must
  match a stable total; malformed collection/member identities, truncated pages
  and bounded HTTP failures preserve published state. A complete empty root
  removes membership/tags only when no other recorded root retains the collection.
  Base collection IDs and dependent references survive; rediscovery reuses them.
- Repeated discovery exposed destructive member replacement in all three providers
  (`c9e56e99-5a97-428b-90cb-d98cb9daaf11`): row IDs and explicit audio/subtitle
  choices were lost despite unchanged membership. Shared reconciliation now
  reuses retained member rows and updates only provider order.
- All 17 focused collection cases pass (`ec50cfad-b961-4d39-95c4-5c4a0025ca1e`).
  Nine new lifecycle cases cover three providers, persisted database restart,
  HTTP/SQL failure rollback, malformed/truncated responses, 101 raw-member pages
  with unsupported-kind projection, changing totals, multiple roots, removal
  and rediscovery. They retain rerun-collection references, unrelated tags,
  audio/subtitle streams and unchanged membership playback choices. The prior
  Unicode/mixed-member fixture was renamed accurately; its snapshot data is
  unchanged (it never exercised pagination despite the old name/comment).
- Cancellation now passes for all three providers
  (`dc12c153-8612-4c2d-954c-404e7f68c7c1`): each fixture cancels while a
  member response is pending and verifies unchanged publication before and
  after reopening the database.
- A Classic media-dependency snapshot now shares default/alternate schedule
  graph discovery with the existing scan-impact resolver. It retains resolved
  media dependencies across publication, including nested collections,
  playlists, reruns, fillers and global fallback. The focused regression
  (`16b3f537-e614-430c-9f93-55de3af1d358`) proves membership removal and a
  metadata rename cannot erase captured old matches; unrelated library
  playouts remain excluded unless the global fallback actually depends on the
  changed media.
- Provider publication now compares semantic membership/tag projections inside
  its write transaction and records changed playable media plus old dependency
  matches atomically. Startup and periodic refresh recovery resolve new matches
  and transfer both sets to existing durable playout intents. The 19 focused
  collection cases pass (`5ba6bc0f-948b-44ae-9cb1-5177e69901fd`), including
  unchanged-scan suppression and the handoff's file-backed restart, publication
  rollback, transfer rollback and idempotent completion. The numeric/textual
  schedule-kind follow-up passes (`2181e0a4-47fa-40d6-9e70-09feb45be665`).
- The 64 selected refresh regressions pass
  (`70d577f4-49cd-43e9-86da-7345f9423ffc`), including mixed native/remote
  reasons, concurrent existing intent writers, replay generation guards and
  aggregate health for stalled provider publication. The preceding sandbox run
  failed only four HTTP fixtures because loopback binds were denied; the
  permission-correct run passes all selected tests without assertion changes.
- Strict domain/infra/web library, test and binary Clippy passes after extracting
  fixture setup and coalescing assertions into named helpers; no lint allowances
  or acceptance assertions were weakened.
- Integrated provider-to-builder continuity passes for Plex, Jellyfin and Emby
  across Shuffle, Random and RandomRotation (`9c1e7ef8-4501-4712-944a-8c18db828f11`):
  production builds generate two-day checkpoints; publication adds/removes/reorders
  members; a restart before resolution produces the same items, anchors and
  checkpoints as uninterrupted Refresh beyond the day seam. A second provider
  server reuses collection and media keys; its separately scheduled state is
  unchanged. Repeating an unchanged provider scan produces no work.
- Show/season descendant invalidation and inherited `show_tag` search changes
  pass through real sync for all three providers
  (`fefbf783-0d49-4d90-8262-5752d113c12c`). Adding only a show member refreshes
  the direct collection and newly matching episode search; replacing it with a
  season member retains the old search dependency for invalidation. The fixture
  initially misplaced EpisodeNumber and used `tag` instead of the documented
  inherited `show_tag` field; both fixture mistakes were corrected without
  changing the expected affected playouts.
- The publication-boundary failure regression passes for all three providers
  (`16f6472e-2e0f-43b0-9895-8c13b3fac1c9`): a trigger rejects the durable
  handoff insert after membership/tag/discovery writes, and the complete prior
  state is preserved. Strict continuity-test Clippy passes.
- Final source-content review covered all 29 non-generated changed files:
  Tag constants/search semantics and Television Mapper map to SQL-backed search
  and admin readers; the three CollectionRepository changes map to shared
  source-scoped collection tag reconciliation; Emby/Jellyfin Movie/Television
  writers map to the shared untyped-only facet replacement; Image/Movie/
  MusicVideo/OtherVideo/RemoteStream/Song/Television AddTag changes map to typed
  producer inserts and ordinary replacement preserving typed rows. Native
  scoped selection and deletion occur within one write transaction, covering
  MetadataRepository's ownership guard without a separate stale entity delete.
  PlexServerApiClient, three library scanners and PlexTagOwnership map to
  ID/name label projection/reconciliation; PlexTelevisionRepository network
  changes map to library-scoped network writes. ElasticSearchIndex maps to
  immediate SQL search projection (including distinct show_tag and country).
  LocalMetadataProvider maps to separate NFO country reconciliation. Both
  database backfills map to the conservative SQLite ownership migration;
  ErsatzRS remains SQLite-only. The upstream ownership tests map to the mixed
  owner, retained-ID, rename/removal and search regressions recorded above.
  Changelog behavior is represented in Unreleased. Two generated EF designers
  are bookkeeping and introduce no independent runtime behavior.
- This review found network invalidation missing from the new durable path.
  Plex network publication now records changed shows/episodes in the same
  transaction, with a distinct provider-network-changed reason; recovery
  preserves old and new show_network query matches. Its regression proves
  unrelated-library exclusion, unchanged suppression and rollback if handoff
  insertion fails. The handoff schema now stores its bounded reason explicitly.
- The review also reproduced stranded collection tags outside current/previous
  membership (`3554622a-62ca-4e3c-aa1e-885f51079dbd`). Reconciliation now includes
  all matching owned tags within the source, preserving equal-name other-owner
  rows. All 39 combined collection/network/continuity checks pass after these
  corrections (`d047f85d-b7b0-4f5b-b9f5-3078da042a0a`).
- Strict domain/infra/web library, test and binary Clippy passes after both
  audit corrections.
- Explicit admin deletion reproduced orphaned provider tags
  (`bff19c03-5908-4120-95c7-ca123e149031`). It now removes confidently owned
  projections, deletes the collection and records affected refresh work in one
  transaction; ambiguous unclaimed legacy tags are not assigned an owner.
  Source ownership is rechecked inside the write transaction before cleanup.
  All 46 selected migration, collection, network and continuity cases pass
  (`657deb15-c037-46b0-9470-2c7f4010d8a9`), including deletion rollback/restart,
  retained media/tracks and the ordinary manual-collection delete regression.
- Row source review and final strict infra library/test lint are complete.
  Maintainer accepted ADR 0104 on 2026-09-27. Verdict: Adapt, preserving
  upstream tag behavior with accepted source-owned collection identities.
  The pin advances with row 129; align the clean reference checkout after
  landing. Row 130 is next. UCU-08 milestone-wide acceptance remains open.

## UCU-08 row 128 source map: upstream test deadline

- Full upstream `165c39ec04e814c8fc5bf6c84c867d49b25eeb15` changes only
  `ErsatzTV.Core.Tests/Scheduling/ClassicScheduling/MultiDayBuildTests.cs`.
  Its four added lines override the base fixture's 10-second cancellation
  source with 30 seconds. No test scenarios/assertions or runtime code change.
- Verdict N/A: the Rust Classic tests are synchronous scheduling calls and do
  not inherit the NUnit fixture cancellation token. Existing multiday/hourly
  continuation tests remain; `.config/nextest.toml` reports slow tests at 60
  seconds without terminating them and specifies zero retries. No timeout,
  assertion, production or runtime-artifact changes are warranted.
- Exact diff, base fixture and native test/runner contracts were reviewed.
  Structured progress and diff validation are the appropriate row checks;
  existing scheduler evidence remains valid. Pin/ledger advance in the local
  documentation commit, then align the clean reference checkout. Row 129 and
  full milestone gates/push/CI remain outstanding.

## UCU-08 row 127 source map: HDHomeRun with IPTV JWT protection

- Upstream `463a4c808aaaccb925036b0236ff88c14c516a78` removes HdhrController
  when JWT is enabled. HDHRSettings disables Save and Tuner Count and explains
  that the endpoints are disabled and those settings have no effect. Its
  Startup convention and changelog complete the four-file change.
- Native build_app and build_streaming_app currently register discover.json,
  device.xml, lineup_status.json and lineup.json unconditionally. The existing
  immutable IptvAccessTokenVerifier is the runtime policy source. Registration
  must omit HDHR routes under JWT on both listeners, independent of a supplied
  JWT or machine key, while preserving the unprotected protocol responses.
  Audit UDP task startup as well so discovery does not advertise disabled HTTP
  endpoints; preserve the existing UDP packet format and shutdown behavior.
- Hydration/visual source map: `/settings/hdhr` uses the HdhrSettingsPage SSR
  wrapper, blocking Resource/Suspense and render_hdhr, with an ordinary native
  SettingsForm POST. There is no new island. Load runtime availability through
  the existing settings server function; disable the shared form's save control
  via a default-false prop and disable only this page's Tuner Count input. Add
  the upstream warning through TextKey and every supported culture. Preserve
  UUID display, labels, layout, stored settings and ordinary no-JS submission.
  Native `/admin-api/settings/hdhr/save` and generated save_hdhr_settings share
  save_hdhr_settings_with_state; reject disabled saves there using trusted state,
  never a client-submitted availability flag.
- Acceptance: production router GET/HEAD tests for all four endpoints on both
  listeners with JWT enabled/disabled; no disabled lineup DB access; enabled
  discovery payload unchanged. Verify settings load/disabled-save behavior,
  localized SSR/hydrated/no-JS browser behavior with a disposable JWT-enabled
  host and the ordinary editable case. Check UDP startup policy, affected strict
  lint and required web parity. No existing health check depends on HDHR; no
  FFmpeg artifact/build or media data change is needed. Row 127 is Port;
  do not push before full UCU-08 acceptance.
- Working implementation omits the HDHR route group on both listeners, gates
  the UDP task before spawning, carries trusted runtime availability in separate
  settings page data, disables the native form/input and rejects saves in the
  shared server handler. All nine cultures include the warning, and native-form
  rejection maps back to the selected language. All 15 initial focused HDHR
  tests pass (Nextest `356ddabc-5b5e-4979-b6fa-6cd026f1cdd1`), including both
  listener modes and existing protocol/shutdown tests. The full web parity gate
  passes against the subsequently strengthened valid-credential fixture,
  no-runtime UDP no-spawn test and final localization handling: 742 tests pass,
  20 existing skips, strict web Clippy, formatting, architecture, progress and
  hydration checks pass (Nextest `55c43114-bff6-408e-8763-e972d2d73b86`).
  `tests/e2e/hdhr-jwt.spec.ts` discovers four cases covering Chromium/Firefox and
  hydrated/no-JS behavior; run it against isolated JWT-enabled and disabled hosts.
  It checks English/Dutch notices, crafted-save rejection without persistence,
  and editable tuner settings with live discovery updates when JWT is disabled.
- The first browser run exposed an existing no-JS rendering defect: UI settings
  remained at the Suspense loading fallback. HDHomeRun and UI settings now use
  the existing InOrder SSR route mode so native forms arrive without a script
  replacement. The Firefox trace also showed navigation aborting UI hydration;
  the fixture now waits for hydration before leaving each hydrated settings
  page. Assertions and timeouts remain unchanged.
- Final evidence: all four Chromium/Firefox hydrated/no-JS cases pass against
  each isolated JWT-disabled and JWT-enabled host (eight cases total).
  `test-results/ucu08-row127-open-final/playwright-results.json` and
  `test-results/ucu08-row127-jwt/playwright-results.json` record the runs.
  Desktop and fresh-mobile warning layouts are visually checked; the focused
  Chromium capture also passes (`test-results/ucu08-row127-jwt-visual`).
  Final web parity passes 742 tests with 20 existing skips and strict Clippy
  after the SSR route correction (Nextest
  `682f6459-b482-4699-ab3f-8bf287752dd8`). Pin and ledger advance together
  in the coherent local row commit, followed by clean reference-checkout
  alignment. Row 128 and milestone gates/push/CI remain.

## UCU-08 row 126 source map: fallback duration, history and guide groups

- Upstream `fd2171ade1d76e84f1496cea85e40871b4855c51` changes the scripted
  SchedulingEngine and YamlPlayoutDurationHandler fallback branches: set the out
  point from the gap duration, advance the guide group and generate history from
  the fallback media item instead of the rejected main item. Added upstream
  tests exercise 10-minute and 90-second gaps with longer fallback media.
- Native scripted fallback already delegates to add_item_with_opts with the
  remaining duration and selected fallback identity. Both engines' shared item
  constructors generate matching start/finish and in/out points, history from
  the selected item and guide-group advancement that respects locking.
  YAML schedule_fallback_item instead clamps duration to the fallback length,
  then jumps the cursor to the target, leaving an uncovered interval when the
  fallback is shorter. Align that argument with upstream and native scripted
  behavior; no persisted-state format or restore policy changes are needed.
- A two-engine matrix covers 90/600-second gaps, 30/2700/3600-second fallback
  lengths, locked/unlocked guide groups, exact interval continuity, fallback
  history identity/times, retained rejected main content and audio/subtitle
  preferences. Before correction, scripted passes and YAML fails with a
  30-second item in a 90-second gap. The YAML duration argument now uses the
  complete remaining interval. All 96 YAML/scripted tests pass (Nextest
  `2339dee6-2003-4045-bc59-42efc6787041`); strict core all-targets Clippy passes.
- Substantive review confirms the one-argument correction matches the upstream
  target-time interval and native scripted path. Both engines construct fallback
  history and advance guide groups through their existing helpers; those parts
  of the upstream change are covered, including group locking. Acceptance passes
  with the two-engine suites and strict affected lint.
  Existing row 122/123 SQL history restart evidence remains applicable because
  history construction/storage does not change. No FFmpeg argv, platform,
  schema or browser change is introduced. Verdict Port: commit code, tests,
  evidence, ledger and pin together, then align the clean reference checkout.
  Row 127 and window acceptance remain, with no intermediate push/CI.

## UCU-08 row 125 source map: artwork authorization surfaces

- Upstream `19b783d7e996c406d1e1bc1366b5f340685391f4` changes only
  ArtworkController. It separates IPTV handlers from UI handlers and applies
  ConditionalUiAuthorizeFilter to UI redirects, cache images and provider
  proxies. The filter requires the management cookie only when upstream OIDC
  is enabled; it never requires the machine API key for browser image requests.
- Native policy is already explicitly adapted by accepted ADR 0103: management
  stays on a trusted network or behind an authenticated reverse proxy with
  direct access prevented. No built-in OIDC/session claim is made. `/artwork/*`
  belongs to the management listener; `/iptv/artwork/*` is shared with the
  streaming listener and uses the existing optional IPTV JWT policy. Machine
  keys cannot substitute for either management deployment protection or IPTV
  JWTs. Adding a second internal login implementation is outside this row.
- Acceptance: production-router tests must prove management artwork is absent
  from the streaming listener even with forged forwarding headers and a machine
  key; GET/HEAD IPTV image requests must require valid JWTs when enabled on both
  listeners and remain open when disabled. UI artwork must remain independent
  of the machine key. Reuse row 124's actual provider/cache image and browser
  loading evidence; no image-serving or browser implementation changes occur.
- The initial four boundary tests passed (Nextest
  `5b1d33bc-35a7-48e3-99d8-98cd6a39a712`). Review strengthened the new fixture
  to configure the supplied machine key as valid, proving a real machine key
  cannot bypass IPTV JWT admission. The strengthened four-test selection passes
  (Nextest `6b726a14-06c4-4340-a815-8cb2027b80b3`), and final strict web
  all-targets Clippy passes.
  Configuration docs now explicitly include `/artwork/*` in the management
  proxy boundary. The complete single-file upstream diff and conditional filter
  are reviewed; no runtime, schema, health, platform or browser change is needed.
  Verdict Adapt (ADR 0103): preserve the accepted native management deployment
  boundary and existing separate IPTV routes. Advance pin/ledger in the local
  row commit, then align the clean reference checkout. Row 126 and window
  acceptance remain; no intermediate push/CI.

## UCU-08 row 124 source map: artwork identity and episode images

- Upstream `b3a351167a1f051ea9093b65530dfda7435d4a31` centralizes artwork
  mapping and replaces provider paths/query strings in public artwork URLs with
  persisted artwork IDs. The controller resolves the stored row, verifies the
  provider prefix and derives provider paths and resize settings from that row.
  Local redirects require hex cache names; numeric artwork IDs and hex route
  constraints eliminate ambiguous catch-all matches. Provider helpers, media
  cards, movie/TV details and channel-guide projections use those references.
- Routing/controller sources: ArtworkController, IptvController logo routes,
  Startup's artwork branch, HexConstraint and StringExtensions; account, sort
  and internal route constraints are included in the final source audit.
  Presentation sources are ArtworkMapper and the media-card/movie/television
  mappers. Existing image geometry and localized controls remain unchanged;
  browser acceptance must prove the rendered image URLs load successfully.
- XMLTV source: `_episode.sbntxt` removes the quote appended to the poster image
  URL. Rust's ADR 0058 omitted that element as a workaround; retire that omission
  and restore the valid poster image alongside the show icon and episode still.
- Rust targets: artwork repository/application projection, admin media/search
  artwork references, web artwork/router handlers and guide projections,
  bundled `_episode.jinja` and XMLTV regression coverage. Current projections
  lose artwork IDs and expose paths; provider handlers accept request-selected
  paths and query parameters with stored provider credentials. Those are actual
  gaps against the new ID-only route contract, not a covered cleanup.
- Acceptance: persisted-ID resolution for every provider and affected admin/EPG
  projection; reject unknown/mismatched IDs, arbitrary provider paths/query
  overrides and invalid cache names; preserve local registered artwork, routing
  policies, provider source ownership, fixed resize behavior and credential
  isolation. Verify valid episode poster/still output and custom-template
  precedence, focused SQL/router tests, affected browser proof and strict lint.
  No live provider credentials or library data are needed. Row 124 is Port;
  pin and ledger advance together, then align the clean reference checkout.
- Ancillary audit: AccountController only changes C# collection syntax; the
  internal remote-stream and collection-sort endpoints gain numeric constraints.
  Native collection ordering uses `update_collection_custom_order_impl` with an
  `i64` collection ID. The native internal media callback is
  `/ffmpeg/next-media/{item}/{version}/{file}` with a `Path<(i64, i64, i64)>`
  extractor and capability validation, served only by the loopback router
  (ADR 0102); there is no arbitrary-string remote-stream callback to constrain.
  Native uploaded channel logos and watermarks use UUID plus
  image-extension filenames (or `.bin` for unknown upload MIME types), so retain
  that stored format alongside upstream hex names when constraining those routes.
  Arbitrary filenames and decoded path separators must be rejected before
  filesystem access.
- ID propagation targets confirmed by source: `AdminMediaCard`, detail/actor
  projections and `AdminSearchEnrichment`; SQL lives in `admin_media.rs`,
  `television.rs`, `metadata_query.rs` and search enrichment in `scheduling.rs`.
  Guide artwork is separately batched in `xmltv_metadata.rs`; preserve its
  fallback order and batch loading when retaining IDs. Search/cards and XMLTV
  now retain typed references, and provider routes resolve IDs before creating
  outbound requests. Detail/actor and collection projections also retain IDs,
  resolving selected rows in bounded batches. Legacy path mapping is test-only;
  current production projection uses typed artwork references.
- Hydration source map for the admin conversion: existing `/search` and
  `/media/*` SSR wrappers and islands stay in place. Server-side
  `search::build_card`, `media_browser::browser_card_artwork_url` and detail
  projections consume typed repository artwork references and emit image URLs;
  island props, forms, localization and DOM structure are unchanged. Existing
  `search-see-all.spec.ts` and `media-browser-actions.spec.ts` cover navigation,
  hydration and responsive behavior; update provider URL expectations to IDs
  and add real provider-route evidence rather than relying solely on mocked
  browser images. No shared visual primitives are changed.
- Focused evidence so far: artwork repository identity/error test passed
  (Nextest `44f6c75f-b029-43a6-a684-22858ececf3f`); four XMLTV metadata tests
  passed with persisted poster/still IDs
  (`da7fc1e7-c0c6-4cd0-901c-87e9c3e041c6`); SQL search fallback test passed
  with exact winning IDs despite duplicate paths
  (`07d9cd10-a336-4208-b5bc-2a65c8333a30`). Combined web selection passed
  46 tests (`4bbc9402-c755-4cf6-b998-10b63fc4870c`); its one stale guide-URL
  assertion was updated to the IPTV artwork route, then that rendering test
  passed (`b8cedb5f-48d4-4e08-910a-8e6cd6e74b61`). These are partial proofs,
  not row or milestone acceptance. Detail conversion subsequently passed the
  remote-stream SQL test (`837de8ee-340b-4ed1-94d9-69e6a3a97c9f`) and 24
  detail/search web tests (`3f9bd83d-c9c0-4397-8a9b-1a15b0661218`). The initial
  three-provider HTTP fixture passed (`02bd784e-4570-497f-aeca-cbe8cd1e763d`);
  its added decoy Plex source and local-cache redirect cases subsequently
  passed (`5dc939c2-be61-44bf-9744-75995bd1f3c9`). Strict Clippy passed for
  infrastructure and web all-targets. A two-episode regression then reproduced
  the existing XMLTV shared-show map-consumption defect (second episode lost
  its poster); the loader now clones shared show artwork, genres and guide
  identifiers. Ten final projection tests passed
  (`1c0ec21a-feed-47fd-8383-694ae8e58a6d`), and strict infrastructure lint was
  rerun successfully for the correction. The initial matched SSR/WASM browser
  run passed all six media-browser cases. Search failed in both projects because
  the new decoded-image assertion inspected an offscreen lazy image; Chromium's
  trace confirms no thumbnail request was made. The test now scrolls each image
  into view before the unchanged decode assertion. The focused rerun passed in
  both projects (two tests, 15.5 seconds) under
  `test-results/ucu08-row124-search-final/`; the six earlier media-browser passes
  remain applicable.
  Final source review also caught disabled provider redirects in the pending
  proxy change. Bounded redirects now preserve same-origin headers and permanently
  drop provider credentials after crossing origins, with a synthetic two-origin
  regression covering the return hop, loop limit and invalid scheme.
  All 20 artwork tests now pass (Nextest
  `f65eaf7b-6fb6-4fb1-b4da-8c9ae18227b8`), including those redirects, all three
  provider ID routes and case-insensitive native/imported Plex prefixes.
  Final strict web all-targets Clippy, formatting and progress validation pass.
  The existing v2.0.2 Linux package passed
  its published checksum again before use. Substantive source review confirms
  all 19 upstream files are accounted for: ID projections and provider helpers,
  image constraints and routing, guide templates and the ancillary numeric-route
  changes. No FFmpeg argv, schema, platform assumption, health requirement or
  audio/subtitle selection changes are introduced. The required web parity gate
  passed architecture/hydration checks and all 738 web tests (20 existing skips;
  Nextest `08d4dc97-c456-4980-a746-487269efca95`). Its final strict Clippy step
  also passed. Row 124 is Port; commit the implementation, evidence, ledger and
  pin together, then align the clean reference checkout. Row 125 and the
  milestone window gates/push/CI remain; no intermediate push is requested.

## UCU-08 row 123 source map: shuffle cycles and scheduling safety

- Upstream `b0f313eec89774fecc7e88cb1fc0cdea29c507a4` adds nullable parent
  and child history seeds; restores later shuffled cycles; keeps child metadata
  aligned with the shuffled order; preserves the current repeated collection
  slot and resets per-child counters when changing slots. It also clamps stale
  indices, fixes history positioning after removed items/specials, deduplicates
  marathon sources, validates grouping and tolerates absent album metadata.
- Scheduling safety changes bound equal-entry shuffle behavior, handle empty
  playlists, reject nonpositive filler durations and preserve Skip Missing Items
  filtering across playlist/multi-collection reloads. API validation rejects
  invalid marathon grouping. SQLite adds `PlayoutHistory.Seed` compatibly; the
  MySQL migration maps to the existing SQLite-only deployment contract.
- Rust targets: shared media/playlist enumerators, YAML snapshot/history and
  scripted/block history writers, domain/repository history persistence,
  marathon construction/validation, Classic filler and source filtering.
  Existing YAML snapshots already preserve full playlist cycles; existing Rust
  shuffle has no retry loop and media indices wrap safely. These must be proved
  alongside the gaps, not used to skip the broader row audit.
- Demonstrated source gaps: ordinary playlist history has no seed; scripted
  marathon group shuffle is a one-time permutation outside the enumerator;
  history lookup chooses the first repeated collection slot. Shared media
  enumerators tag Shuffle but do not reshuffle children when they wrap.
- Acceptance: later-cycle and uneven-child continuation through real SQLite
  restart, legacy null-seed history, repeated children, removed/special media,
  equal/empty playlists, invalid grouping, duplicate sources and missing-item
  filtering; preserve audio/subtitle settings and existing cancellation. Follow
  the additive migration contract.

- Working evidence: the later-cycle regression fails before correction and passes
  for chronological and shuffled uneven children with saved seeds. Independent
  child wraps also require preserving consumed media in Rust's explicit parent
  cycle state; legacy history retains the earlier replay fallback. YAML's existing
  full snapshots/checkpoints retain child seeds compatibly. Migration acceptance
  preserves existing rows with null seeds and stored seed values on rerun
  (`66eb5b6e-71f7-4472-a912-c74cf96ab4db`). Final 228 scheduling tests pass
  (`ec549a1a-2de1-405a-b4ca-66bde7d70b4c`), including repeated-slot batches,
  equal/empty playlists, cycle-boundary repeat avoidance, invalid groups,
  overlapping marathon sources, removed/special media and nonpositive filler.
  Classic playlist child shuffles retain seeds in their existing checkpoints;
  the Continue oracle covers shuffled and chronological children/groups.
- Real SQLite acceptance: all 55 affected repository tests pass
  (`c9e9fba2-31fe-4e2b-b00c-e5cb08315421`). An uneven shuffled marathon resumes
  after 41 items across closed/reopened SQLite and matches all 100 uninterrupted
  media/time tuples. Assertions require multiple parent and child seeds. Both
  history write paths preserve seeds; existing audio/subtitle restart proofs and
  extended multi-collection/structured-playlist missing-item checks pass. Strict
  affected core/infra Clippy passes.
- Substantive review: no duplicate shuffle list exists in Rust; owned child order
  is always authoritative. Equal/empty cases are bounded, and duplicate media IDs
  cannot trap the child boundary-repeat guard. Existing Classic duration guards
  already cover tail/fallback hangs; the regression proves progress. Repository
  filtering applies before grouped ordering and structured playlist generation,
  so missing files are not reintroduced. Missing album metadata was already
  represented safely with Option. SQLite migration is additive, legacy seeds stay
  null, existing YAML checkpoints remain readable, and current schema health
  checks remain applicable. No browser, new dependency, FFmpeg or platform-specific
  change. Verdict: Port. Advance the pin with the coherent local commit, then
  align the reference checkout. Row 124 and window gates/push/CI remain.

## UCU-08 row 122 source map: playlist continuation state

- Upstream `34b0d954324395e9b6472bc9eae8ad2c99673687` changes playlist
  history restoration in `SchedulingEngine`, `YamlPlayoutApplyHistoryHandler`
  and `BlockPlayoutEnumerator`: restore the parent cycle position once, retain
  each child's position, then advance once. Scripted/YAML history must record
  the cycle index rather than the current child slot. Tests cover marathons,
  scripted playlists, all child positions and block deco playlist filler.
- Rust targets: shared `PlaylistEnumerator`, scripted history collection and
  writing, YAML checkpoint/history restoration, block deco scheduling and the
  scheduling repository's playlist reference-data loader. YAML already stores
  complete sparse snapshots and anchor checkpoints. The scripted writer only
  saved the active child at the latest timestamp; restoration did not replay
  the parent and could advance a non-chronological active child twice.
  Block deco playlists are flattened into media lists and restarted from the
  playout seed, with no playlist child-state restoration.
- Acceptance: production-builder uninterrupted versus Continue/restart proof
  across these consumers, distinct child positions and parent cycle index,
  preserved count/play-all semantics and audio/subtitle settings, focused
  regressions and strict lint. Preserve existing safe handling of invalidated
  state; no UI/schema change is proposed.
- Working correction: scripted history now includes every child; shared
  restoration replays the parent cycle before restoring children and advancing
  once. The production Lua split-build test failed before correction at seven
  items, and passes after correction at splits 1/7/11/15. All 218 scheduling
  tests pass (Nextest `52dc0f7b-6173-4d5c-8713-0ae4b0d47667`), including
  chronological/unordered counted and play-all child restoration. Strict core
  Clippy passes. Block/scripted parity records are reopened with explicit debt.
- Block loading now retains structured playlist children. Reset-time block-start
  filler restores the latest complete parent/child history and shares the
  scripted history writer. The existing upstream Reset-only admission remains
  unchanged. The SQL loader regression passes (Nextest
  `f2e8b93f-945e-4e01-8395-0bd1b1f94164`), and SQLite close/reopen tests pass
  for block filler and scripted playlists/marathons (Nextest
  `10bd4431-7cce-4d12-9d1f-b60ce99f3595`), preserving audio/subtitle settings.
  The scripted loader now supplies persisted items/history, previously absent.
- Production source resolution now uses the existing scheduling repository via
  a fallible core port, injected through the existing playout builder into its
  awaited blocking worker. No source-name parsing or preloaded membership is
  required. Named playlists retain structured children; all media source types,
  watermark names and graphics paths resolve from SQLite. Repository failures,
  cancellation and a bounded lookup timeout fail the build without persisting
  partial output. Runtime-loaded sources retain Remote Stream duration filtering
  and the non-Classic live-source restriction.
- Real-source acceptance: three SQLite tests pass (Nextest
  `2b771d24-2770-4ebb-bbb0-38f59e038717`). Named playlists, inline playlists and
  marathons match uninterrupted media/times and audio/subtitle settings after
  closing and reopening the database; registrations use no injected membership.
  Dynamic names, every source kind, repeated playlist registration, overlays,
  closed-pool failure and cancellation are covered. All 220 core scheduling tests
  pass (`26fab6b4-6159-447b-a968-3298231da8ac`), including runtime-source
  Remote Stream safety. Strict affected core/infra Clippy, all seven shared
  builder tests and both SQLite history restart tests also pass (restart run
  `8e86b59d-3145-43bc-8f5f-1600c84884d2`).
- Substantive review: parent replay and the single final advance match upstream;
  all child histories share the emitted item's timestamp. Count/play-all and
  chronological media identity are preserved. Production resolution stays behind
  the existing core/infra repository boundary and awaited worker; SQL errors are
  redacted and cannot yield a partial successful build. Existing Reset-only block
  filler admission, cancellation ownership, health prerequisites and playback
  pipeline remain unchanged. No new dependency, schema, platform assumption or
  browser surface. Verdict: Port. Row 123 and window acceptance remain pending;
  no intermediate push or CI.

## UCU-08 row 121 source map: text subtitle query preservation

- Upstream `06ee45ff06203988acb68339850c73f30167104d` changes one
  character in `FFmpegLibraryProcessService`: append `&seekToMs` rather than
  `?seekToMs` after the subtitle callback's existing `exp` and `sig` query.
  Its only other change is a CHANGELOG line.
- ErsatzRS does not construct that subtitle callback. `playback.rs` preserves
  HTTP(S) subtitle paths, provider preparation produces a local artifact, and
  `subtitles.rs` rebases text into the seek cache under accepted ADR 0061.
  `transport/iptv/stream.rs` passes the seek separately to
  `rebase_burned_text_subtitle_input`; FFmpeg fallback uses a separate `-ss`
  argument. There is no `seekToMs` query concatenation to repair.
- Strengthened existing regressions require the original `exp`/`sig` query
  at the HTTP fixture, then verify the straddling cue rebases correctly; the
  extensionless ASS fallback must preserve its query-bearing source argument
  and keep seek in a separate argv entry. All 50 selected subtitle tests pass
  (Nextest `7093b16e-9a91-40f8-8c9b-cad05a2ce5b0`), including existing Next
  subtitle coverage; strict infra library/test Clippy passes.
- Covered under the existing seek-cache design (ADR 0061). No production
  behavior, generated FFmpeg arguments, URL/authentication policy, track
  selection, schema or health requirements change. The row adds focused proof
  rather than importing an unused callback; full UCU-08 gates remain due.

## UCU-08 row 120 source map: persisted YAML sequence order

- Upstream `722f1b93` changes SequentialPlayoutBuilder sequence flattening,
  YamlPlayoutInstruction sequence identity, YamlPlayoutShuffleSequenceHandler
  placement, and YamlPlayoutContext serialization/restoration. Independent
  clones, declaration indices and sequence/list fingerprints preserve shuffled
  order across builds and reset only invalidated list positions.
- Upstream regressions cover nested/non-contiguous positions, bounded shuffle
  retries, continuation, all named lists, changed content/layout, partial or
  malformed orders, missing fingerprints, legacy anchors, unnamed and duplicate
  schedules. Complete-order validation precedes mutation of a restored list.
- Rust targets: `crates/ersatzrs-core/src/scheduling/yaml/{mod,sequences}.rs`
  and focused tests; persisted anchors pass through the existing playout
  repository transaction. Current Rust cloning and bounded shuffle already
  exist, but contiguous splice can overwrite nested instructions, and anchors
  retain list indices without retaining the shuffled instruction order.
- Preserve current anchor fields, media enumerator/history state, selected
  audio/subtitle overrides and schedule switching. Add backward-compatible
  order/fingerprint state and reject partial restoration without partially
  mutating a list. No schema or UI changes are required.
- Acceptance: focused sequence placement and restore-validation regressions,
  uninterrupted-versus-split/reloaded builder proof, strict affected lint and
  persistence evidence. All pass: 64 YAML tests (Nextest
  `bc67e989-b5c0-49ac-aba4-3488e859ec44`), production-builder SQLite close/reopen
  and uninterrupted-permutation oracle comparison (Nextest
  `79a39c33-bfbf-4459-97a7-eafb676e6295`), plus strict core/infra lint.
- Review confirms whole-list validation before mutation, ignored ephemeral
  invocation IDs, independent named/default state, legacy-anchor compatibility,
  and preserved media history/settings. SHA-256 uses the existing workspace
  dependency version; no schema, platform, UI or FFmpeg argument change.
  Row 120 is Port; milestone closeout and publication remain pending.

## UCU-08 row 119 source map: external Next QSV release note

- Upstream `2cf8e56ae3cc8d84598d7b36e26234d23c12747b` adds two lines
  only to `CHANGELOG.md`, describing Next QSV detection on older Intel devices.
- N/A: no host source, dependency, engine pin or package input changes. This
  upstream note is not evidence of hardware acceptance for ErsatzRS. Preserve
  existing Next/native capability checks and hardware limitations; no new claim.
- Full diff reviewed; structured progress validation is the applicable check.

## UCU-08 row 118 source map: empty build trigger

- Upstream `fcd593e08d9ae1501f25af55337432745aea0338` has the same tree
  (`4fa5596444fbfd29145cf8bdbb712d43973d0315`) as its parent, row 117.
- N/A: commit metadata triggers an upstream build without changing source,
  configuration, package inputs or runtime pins. No Rust behavior to port.
- Existing runtime selections and acceptance evidence remain authoritative.
  No additional runtime test, dependency change or release is warranted.

## UCU-08 row 117 source map: block retention cutoff

- Upstream `b7aaea3d46bc1eaf2e239ca0f534190a90dac633` changes
  `ErsatzTV.Core/Scheduling/BlockScheduling/BlockPlayoutBuilder.cs`: selecting
  the earliest effective block may move the build boundary earlier, never later.
  Its regression builds after today's block and requires RemoveBefore <= now.
- Rust `crates/ersatzrs-core/src/scheduling/block.rs` currently assigns the
  first effective block start directly to `SchedulingResult.remove_before`.
  `crates/ersatzrs-infra/src/repositories/playout.rs::apply_build_result`
  deletes items finishing before this boundary minus the delayed-channel offset.
  A future block can therefore delete current or future persisted items.
- Clamp the cutoff to the request start while preserving an earlier block's
  boundary. Exercise future, active, exact-boundary and absent blocks, then
  prove retained items through the production builder/persistence path.
- No UI, schema, playback selection or intentional deviation changes. Preserve
  persisted subtitle/audio overrides and delayed-channel retention.

## UCU-08 row 116 source map: upstream security-release wording

- Upstream `39e39ef6ebb939f0b069a100589926d78f4535d7` changes only
  `CHANGELOG.md`: credential-rotation emphasis and ordering, Plex action
  capitalization, an ErsatzTV cache/restart instruction, Jellyfin scope
  clarification, and relocation of the retained-missing-server release note.
- No UI, handler, validator, repository, runtime, dependency or package changes.
- N/A: upstream release guidance is not an ErsatzRS runtime contract or proof
  of vulnerability. Existing UCU-07 security, provider-retention and API-key
  implementation/evidence remains authoritative; no new deviation is needed.
- Review covers the full 14-line diff. Validate structured records and pin/ledger
  agreement; unchanged executable behavior requires no new runtime test.

## UCU-07 row 115 source map: upstream release bookkeeping

- Upstream: `49b68314299ea9961c1914ca41b3016a62f2ed73` changes only
  `CHANGELOG.md`: it creates the 26.8.1 heading, narrows the upstream security
  upgrade recommendation to Plex/Emby users and updates comparison links.
- N/A: no executable source, configuration, dependency or package changes.
  ErsatzRS retains its own Unreleased security notes and release lifecycle;
  this upstream release does not establish an ErsatzRS release or CI result.
- All UCU-07 rows, local closeout gates, push and terminal exact-SHA green CI
  are complete; the milestone owns the final evidence. UCU-08 is eligible.

## UCU-07 closeout correction: Next executable path resolution

- Runtime evidence: bare `ffprobe` passes host startup but Next emits
  black/silence after probe failure. Supplying the same executable by absolute
  path restores decoded Dutch audio and Convert subtitles with unchanged smoke
  assertions. The milestone owns both runs and the open acceptance condition.
- Upstream Next: `ersatztv-channel/src/config.rs` applies
  `resolve_relative_paths` to `PATH_FIELDS`, including both tool paths, before
  deserializing configuration. `ffpipeline/src/probe.rs` spawns that result.
- Rust: `next_engine/config.rs::runtime_path` currently serializes paths without
  executable resolution; `host.rs::configuration` supplies the startup tool
  report. The shared runtime manager's explicit candidate retains bare names
  even after validating them. Resolve those candidates through the existing
  locator before validation, preserving the selected absolute paths for all
  consumers and leaving missing tools to the existing failure report. Reuse
  `ersatzrs-ffmpeg::locator` platform-aware discovery rather than
  introducing another process runner or relying on a verification-only override.
- Correction must preserve absolute and explicit relative executable choices,
  resolve PATH commands to absolute executables, and retain malformed/missing
  tool refusal. Add focused coverage and repeat the original bare-path runtime
  case before resolving the owning parity debt. ADR 0095 remains Partial.
- Corrected in the shared runtime candidate path: 23 locator/runtime tests and
  strict crate lint pass. The original bare-FFprobe smoke now passes decoded
  Dutch audio, Convert subtitles and session lifecycle checks without an
  absolute-path verification override. The owning debt is resolved; refresh
  affected window gates before final UCU-07 acceptance.

## UCU-07 row 114 source map: internal media request authentication

- Upstream: `ef5094aa164666460fdc30d9ed0926ea04718386`
  (`GHSA-h3r4-r2f2-qf59`) adds process-local HMAC signatures and expiration
  checks before remote-stream, Plex, Jellyfin, Emby, subtitle and fallback
  callback handling. Plex paths are appended to the configured address rather
  than allowed to replace its authority. Invalid or out-of-range expiration
  input is rejected without throwing.
- Producers: subtitle extraction and lookup, troubleshooting, statistics,
  FFmpeg subtitle arguments, dynamic items, Next projection and external JSON
  playouts generate signed requests. Scheduler startup republishes Next
  playouts after database readiness; troubleshooting sample filenames strip
  URL queries from extensions. Plex connection view-model and artwork changes
  accommodate the address representation.
- Rust callback boundary: `web/host/router.rs` and `host/listeners.rs` expose
  callbacks only on the private listener with actual loopback-peer admission
  (accepted ADR 0102). `transport/iptv/next_media.rs` authenticates Next media,
  dynamic items, prepared transfers, fallback and credits before their work.
  `infra/next_engine/media.rs` creates process-local capabilities and credits
  tickets; URLs carry persisted binding IDs rather than provider credentials.
- Provider confinement: `next_engine/source.rs::resolve_http` rejects Plex
  file keys that change the configured origin or introduce user information.
  `media.rs::open_http` permanently stops forwarding credential headers after
  a cross-origin redirect. `provider_subtitles.rs` encodes provider identities
  as path segments, refuses redirects, and prepares local subtitle artifacts.
- Restart path: `next_engine/host.rs::start` refreshes publication before
  `NextProcess::start_with_session`. The existing media-publication regression
  rejects a prior relay's capability; the Plex source regression rejects an
  absolute file key on another origin without exposing the URL or token.
  Native `subtitles.rs::resolve_input_path` reads stored file paths with path
  replacements and invokes extraction directly, without upstream-style public
  provider callback URLs.
- Corrections identified: use the existing `subtle` constant-time comparison
  for capabilities and credits tickets; strip HTTP(S) query/fragment suffixes
  before deriving troubleshooting sample extensions. Regression coverage checks
  altered tickets, every credits binding field, process restart, and URL queries
  containing dots or slashes. All seven focused media tests pass (Nextest
  `762b9ffb-52fb-4613-bb61-1c6c0ff4f9d8`), including real localhost redirect,
  range, credential-rotation and Plex failover checks. The first sandboxed run
  denied mock-server socket binds; the unchanged set passed with loopback
  access. Sample-filename and binding/session verification also pass; the
  milestone owns all 24 focused test results. Strict all-target infra/web lint passes.
- Scope review: native extraction and sample capture use resolved stored paths,
  while Next troubleshooting uses the same session-owned preparation machinery.
  There is no upstream-shaped arbitrary provider-path callback or external-JSON
  callback URL generator in the Rust transport. The upstream Plex view-model
  and artwork edits only accommodate its address-type change; Rust already
  stores the connection address as a string. The existing accepted ADR 0095
  capability/session lifetime and ADR 0102 listener boundary remain the
  adaptation; no new public callback or signature-query API is introduced.
- Adapt under accepted ADRs 0095/0102: the scoped fixes, 24 focused tests and
  strict lint pass. The pin advances with this row; milestone gates and push/CI
  remain open after the release bookkeeping row.

## UCU-07 row 113 source map: Next mirror synchronization

- Upstream: `cefd715c28e31d003fe5ccd3a113f93672486b41` moves mirror lookup
  outside guide-cache invalidation and enqueues `SyncNextPlayout` for every
  mirror after a successful source build. Its only other change is a changelog
  entry; no UI, schema, track-selection or FFmpeg argument changes are present.
- Rust: `infra/maintenance.rs` notifies source and mirror channels after
  persisted scheduling changes. Continue filters this set to Next channels,
  independently of the source engine, preserving native playback continuity.
  The shared HLS registry also forwards notifications to preview sessions.
- `next_engine/session.rs` consumes each notification through its refresh loop;
  `host.rs` binds that loop to the requested channel publisher. The coordinator
  locks source playouts through atomic publication, while `schedule.rs` reads
  source items with the mirror offset. Refresh is event-driven, not periodic.
- Existing behavior covers the upstream defect. Extend the production scheduler
  regression to multiple Next mirrors, a native mirror and an unrelated Next
  channel; verify the dependent set and preservation of already scheduled rows.
  Existing session and mirror-offset regressions cover the downstream handoff.
- Covered: the production scheduler regression and seven downstream checks pass.
  No production behavior, track-selection or scheduling-state change is needed.
  The pin advances with row 113; row 114 and window acceptance remain pending.

## UCU-07 row 112 source map: mandatory machine API keys

- Upstream: `c27a8d5031c30bb3557a959db55a959ab2aa2aae` (#2991).
  Security handlers/options, startup routing, run-once key creation,
  `ApiSecrets`, scanner proxy, scripted-process environment, Python entrypoint,
  troubleshooting authorization and all three OpenAPI diffs are audited.
- Upstream outcome: machine `/api` requests require `X-Etv-Api-Key`, persisted in
  `api-secrets.json` as `ApiKey`; missing or failed key setup never allows
  anonymous machine calls. Existing valid keys survive restart. Upstream uses
  32 random bytes, unpadded base64url and fixed-time comparison.
- Upstream browser exception: exactly the `/api/troubleshoot` path segment uses
  management authorization. Its cookie is required only when OIDC is configured.
  Ordinary API paths cannot use management cookies as machine credentials.
- Confirmed Rust inventory: `host/router.rs` mixes native browser forms,
  uploads, browser JSON handlers and generated Leptos functions under `/api`.
  `/api/maintenance/clean_artwork` is an existing machine endpoint. No API-key
  guard, API secret store, OpenAPI document or OIDC/session implementation was
  found. `AppState` currently holds only the separate IPTV JWT verifier.
- Accepted adaptation: [ADR 0103](../docs/adr/0103-separate-machine-api-from-browser-actions.md)
  moves browser actions/functions to `/admin-api`, preserves the troubleshooting
  exception and ADR 0102 listener isolation, and uses `X-Ers-Api-Key` instead of
  upstream's header. The maintainer accepted trusted-network or authenticated-
  proxy management without built-in OIDC on 2026-09-27, explicitly choosing the
  ErsatzRS header name. Implementation and verification are recorded in UCU-07.
- Rust consumers: scanner CLI coordinates through SQLite managed operations,
  not `ScannerProxy` HTTP. Scripted scheduling executes Lua in-process through
  `core/scheduling/scripted`, not an external Python HTTP client. These existing
  transports need no key injection; do not add an unused HTTP dependency.
- OpenAPI diff also refreshes artwork-cleanup and FFmpeg profile schemas
  (normalization, pad mode and target loudness). Any new Rust description must
  describe implemented endpoints truthfully, not advertise absent upstream APIs.
  Removed Bugsnag configuration and IDE namespace settings have no Rust runtime
  counterpart. The Plex changelog note changes operator guidance, not code.
- Target modules: infra durable state/key persistence; web startup/AppState,
  router middleware and machine API description; admin server-function prefixes,
  form actions, static JavaScript and corresponding route/browser contracts.
- Evidence required: key creation/reuse/failure and concurrency; malformed,
  duplicate, missing and incorrect header rejection before side effects;
  no cookie/query/forwarded-header bypass; exact exception paths; denial on the
  streaming listener even with a key; matched hydrated and no-JS actions;
  unchanged IPTV/private callback authorization. Use synthetic fixtures only.
- Row 112 is Adapt under accepted ADR 0103 with local verification recorded in
  UCU-07. Rows 113-115 and window gates remain separate work; no intermediate
  push or CI.

### Row 112 hydration source map

- Scope: all existing management pages that submit native forms, upload files,
  fetch browser JSON or call generated server functions. Existing page wrappers
  and islands retain their ownership and DOM shape; this slice changes request
  paths, not page geometry, dialogs or control labels.
- Boundary: 355 server-function declarations across 22 current source files
  require an explicit management prefix. Handwritten route registrations,
  form actions, static JavaScript and browser interception patterns must agree.
  Machine artwork cleanup and browser troubleshooting retain their `/api` paths.
- No-JS fallback: migrate actual form actions alongside hydrated callers;
  never replace a native form with a JavaScript-only key-injection step.
- Browser proof: matched SSR/WASM bundle, generated server-function calls,
  native saves, multipart uploads and image retrieval, JSON actions and
  troubleshooting. Existing full browser coverage is required after this broad
  request-path migration. Retain listener-denial and IPTV/callback proof.
- Shared components/styles: no structural or visual change is intended.
  Audit route inventories and hydration contracts for embedded path assumptions.

## UCU-07 row 111 source map: Plex reauthentication and missing servers

- Upstream: `a593738637bbc1b186f40395697882969bb4664e` (#2990). Reviewed the
  commands, Plex API/secret store, domain/view model, repository, additive
  `MissingSince` migration, scheduler, page/dialog and changelog diff. The
  generated EF designers/snapshots repeat the nullable timestamp schema.
- Outcome: obtain a new Plex device token without deleting synchronized
  content; retain temporarily unclaimed servers, identify them visibly, resume
  when they return, and remove them only through an explicit destructive action.
- Authentication: a forced flow uses a new client identifier, committed only
  after authorization. Abandonment, provider errors and the two-minute polling
  deadline release flow state. Use HTTP status 401 to invalidate credentials;
  absence of credentials or a failed discovery must not mean an empty server
  inventory. Include product/version identity on Plex authentication calls.
- Reconciliation: keep source/library/media IDs and path overrides. Mark the
  first successful-inventory absence with `MissingSince`, retain its original
  timestamp on repeats, clear it on return, and skip missing sources in scheduled
  and requested scans. Explicit server removal includes search cleanup.
- Upstream explicit `SignOutOfPlexHandler` still deletes all Plex sources and
  search entries. Preserve that distinct, confirmed action; reauthentication
  and an upstream 401 must never use the destructive sign-out operation.
- Rust map: `infra/plex.rs`, media-source domain/repository and scan selection,
  `app` media-source ports/handlers, web host adapter, media-source server
  functions and `admin/pages/media_sources/plex.rs`. Health must report missing
  source/authentication state without exposing credential values.
- Confirmed gaps: polling has no deadline; the account/client ID is reused;
  pending PIN state is not bound to a flow; authorization checks a cached server
  token; no missing-server timestamp, disabled refresh or explicit missing-server
  removal exists. Existing resource upserts already retain absent source rows,
  but lack the state needed to pause their scans or explain their absence.
- Visual source map: upstream toolbar keeps sign-out and reauthentication
  alongside the source table; signed-out/missing alerts precede it. Rows retain
  name/address, library/path actions, warning status and conditional removal.
  Removal uses a small confirmation dialog naming the source and destructive
  consequences. Reuse the existing Rust shared media-source styles/dialogs;
  keep narrow layouts usable and localize every new label/message.
- Required evidence: synthetic PIN/provider tests for new-device identity,
  abandoned/error/expired flows and successful credential replacement; SQLite
  missing/return/repeat/rollback tests retaining media and overrides; scan and
  health tests; hydrated browser reauthentication, warning/removal confirmation,
  provider failures and narrow layout. No real credentials enter fixtures or logs.
- Exclusions: rows 112-115 and broader admin API-key policy. No new service or
  dependency is proposed. Row 111 remains pending until all layers are proven.

### Row 111 hydration source map

- Route/wrapper: `/media/sources/plex`, `PlexSourcesPage`.
- Island: `PlexSourcesIsland`; `PlexSignInControls` stays a child component.
- Data boundary: existing media-source server functions through the app port;
  authorization returns a boolean, never an account token.
- Mutations: start/check PIN, explicit sign-out, missing-source removal with a
  transaction-time missing-state check. Search uses the live SQL projection.
- Fallback: this existing management route requires hydration; prove actual
  interactions in the matched browser bundle rather than adding a second flow.
- Shared primitives: reuse mounted `Modal`/`DeleteDialog`, toast and source CSS.
- Browser target: reauthentication request, provider failure/retry, missing-source
  warning, cancel/confirm removal, retained content, stale removal and narrow view.

## UCU-07 row 110 source map: listener and internal-route isolation

- Upstream commit: `14372575160a58672a44c9ff52034904b5ace156`.
- UI/components/validators: no form or visual change. `Program.cs` and
  `Core/SystemEnvironment.cs` retain independently configurable UI/streaming
  ports; equal ports share the default listener.
- Request boundary: `Startup.cs` removes the client-controlled `Host`
  localhost exception, rejects management requests on a separate streaming
  port, and checks actual loopback peers for internal requests before
  forwarded headers. `InternalController.cs` moves callbacks under `/internal`.
- Consumers: `ConcatPlaylist`, `FFmpegLibraryProcessService`, subtitle
  extraction/path queries, troubleshooting, `LocalStatisticsProvider`,
  `DynamicPlayoutItemService`, `PlayoutItemConverter`, and external JSON
  playout items update their internal callback URLs together. Provider media,
  external subtitles, fallback and generated credits remain playable.
- ErsatzRS targets: `host/bootstrap.rs`, `host/config.rs`, `host/router.rs`,
  `transport/iptv/{mod,stream,next_media}.rs`, FFmpeg concat generation and
  the existing Next media relay. Configuration and endpoint references own
  operator-facing listener documentation.
- Confirmed gaps: the current host registers management and streaming routes
  on one listener without a separate streaming-only configuration. Legacy
  `/ffmpeg/concat/*` and `/ffmpeg/stream/*` handlers have no peer or capability
  restriction. Next callbacks already validate scoped capabilities/tickets;
  those protections must remain in addition to network isolation.
- Required proof: crafted `Host`, `Forwarded` and `X-Forwarded-*` values cannot
  expose management on the streaming listener or internal callbacks to remote
  peers; UI access and public IPTV remain correct. Default/shared and separate
  listeners, IPv4/IPv6, configured interfaces, shutdown and startup failures
  need coverage. Generated internal URLs must use a trusted bound address,
  with decoded legacy/Next playback and selected audio/subtitles preserved.
- Exclusions: rows 111-115 remain pending. No new admin authentication or
  API-key policy is inferred from this row. No visual-layout change is planned.
- Existing decisions: ADRs 0057/0095 retain native direct producers and
  authenticated Next callbacks. Accepted ADR 0102 separates public/internal
  route surfaces within the same process, with a dynamically bound loopback
  callback listener and optional `ERSATZRS_STREAMING_ADDR`. The maintainer
  accepted the decision on 2026-09-27; UCU-07 owns implementation evidence.
- Troubleshooting consumer: `/system/troubleshooting/mpeg` uses the
  `MpegTroubleshootingPage` SSR wrapper and a native GET form. Its context
  loader calls `run_mpeg_troubleshooting`; private legacy samples must use
  the bound internal address and public samples the bound streaming address.
  There is no page island or changed form contract, shared primitive, or
  visual geometry. Verify URL selection with distinct IPv4/IPv6 sockets and
  preserve actual sample capture in the runtime/browser acceptance.
- Runtime follow-up: native legacy-to-Hybrid switching reproduced a direct
  response handle occupying the HLS worker slot until asynchronous cleanup.
  Upstream `IptvController` owns direct response processes separately from
  `FFmpegSegmenterService` workers. Keep that distinction in the existing Rust
  registry: HLS lookup/admission sees only segmenters; separately tracked
  direct handles remain visible to channel activity and Stop. Remove direct
  registrations by handle identity so one viewer cannot remove another.
  Regression proof must retain the immediate transition and check concurrent
  direct/HLS tracking, independent cleanup, and channel-wide cancellation.

## UCU-06 CI follow-up: troubleshooting containment

- Route: `/system/troubleshooting/playback`; SSR wrapper
  `PlaybackTroubleshootingPage` and `render_playback_form` in the system
  troubleshooting page module. The form uses the existing preview JavaScript;
  no island or server-function boundary changes.
- Data boundary: existing troubleshooting context/options; native GET fallback
  remains `/api/troubleshoot/playback.m3u8`. Shared primitives are unchanged.
- CI Chromium trace proves the first select ends at x=345 in a 320-pixel
  viewport. Mobile grid tracks used an intrinsic minimum; constrain them to
  `minmax(0, 1fr)` and allow native controls to shrink within their field.
- Browser proof target: the existing troubleshooting playback specification,
  including engine/graphics/watermark retention and the unchanged 320-pixel
  containment assertion. Four Chromium cases (6.6s) and four Firefox cases
  (12.2s) pass, with zero skips or retries, using regenerated served CSS.

## Row 108 source map: sparse-stream pacing

- Upstream `c0e4c4ac` replaces the Windows archive build `25d21b2c` with
  `3c0520f9`. The same-day ErsatzTV-ffmpeg change `32bf88a3` adds
  `0004-ffmpeg_demux-do-not-pace-readrate-on-sparse-streams.patch`.
- Unlike row 104, this change is absent from FFmpeg n9.0. It chooses the
  slowest active audio/video stream for readrate pacing and uses sparse
  streams only when no active audio/video stream exists. Finished, discarded
  and unstarted streams retain their existing exclusion rules.
- Target: the separate first-party runtime repository's package/container
  build recipes and focused pacing proof. No host UI, schema, track selection
  or generated FFmpeg argv change is needed. Keep v2.0.1 immutable; any new
  runtime candidate must pass package and decoded playback gates before use.
- Row is Port. Runtime source `6b84f17` passes continuous
  pacing, sparse-only source fallback, decoded bitmap/alternate-audio and package
  checks. Both published v2.0.1 controls fail the new pacing regression.
  The maintainer authorized v2.0.2 publication. Its public Linux/Windows downloads
  match the tested archives; the managed default and Docker smoke adopt v2.0.2.
  Existing row-104 total-duration proof does not establish this stronger rule.

## Row 107 source map: Next on-demand progress

- Upstream `b36e94cc`: `NextSessionWorker` looks up the channel's playout mode,
  shifts on-demand items before publishing the manifest, checkpoints the last
  viewer touch every five minutes and once on exit, and cancels its checkpoint
  loop with the session. Continuous channels retain their existing behavior.
  Its shared checkpoint command subtracts the configured segmenter timeout,
  never moves backward and clamps to the first item; preserve those semantics.
- Rust targets: `next_engine/host.rs`, `session.rs` and a bounded checkpoint
  helper, reusing native HLS time-shift/checkpoint SQL. Acquire the existing
  playout lock around mutations so rebuilds and startup cannot interleave.
  Publish shifted items before starting Next; heartbeat and idle-wait time must
  not advance the saved position. Preview sessions must not mutate normal state.
- Preserve generated item identities, in/out points, audio/subtitle settings,
  gaps and continuation anchors. No Reset, schema change, new scheduler or UI.
  ADRs 0010/0019/0020 govern continuity; this adds the missing Next session hook.
- Proof targets: production-generated schedule stop/restart continuity, consumed
  position and future identities, last-touch checkpoint/idle behavior, five-minute
  updates, cancellation and failed startup, continuous/preview isolation, lock
  contention and decoded official Next resume. Row 107 is Port; UCU-06 owns
  passing scoped evidence and remaining full window gates, push and exact-SHA CI.

## Row 106 source and visual map: Next image graphics

- Upstream `cd30ac4a`: `PlayoutItemConverter.SelectGraphics` selects image
  elements through the shared template/deco/item selector, renders their
  templates using media, timing, output/source geometry and EPG variables,
  and merges images with watermarks in stable ascending ZIndex order.
  Absolute HTTP/HTTPS image sources are remote; local paths stay local.
- `GraphicsElementLoader` caches imported YAML and threads cancellation;
  ErsatzRS already persists parsed definitions as canonical database state.
  The shared Rust template loader already avoids EPG queries for zero entries.
  Reuse it without introducing a second YAML cache or graphics process.
- Rust targets: shared playback template rendering and a Next image adapter;
  scheduled, dynamic, remote, retained/prepared, fallback and troubleshooting
  projections must carry the same ordered layers into their identity checks.
  Preserve media-version binding, subtitle/audio choices, disabled watermarks
  independent of graphics, and explicit errors for unsupported graphics.
- Upstream troubleshooting exposes the existing graphics multiselect for both
  engines. Keep the current SSR form, localized label and responsive control;
  remove engine-specific hiding/clearing and pass every selected ID. No new
  island, CSS, schema migration or scheduling reset is required.
- Required proof: template and selection precedence, mixed image/watermark
  ordering, local/HTTP sources, source geometry and unsupported definitions;
  decoded official Next output with audio/subtitles preserved; Chromium and
  Firefox selection, engine switching and no-JS preview. Row 106 is Port;
  UCU-06 owns passing scoped evidence and remaining window/publication gates.

## Row 105 source and visual map: multiple Next watermarks

- Upstream `3a02c39b`: `SyncNextPlayoutHandler` and troubleshooting publish
  schema 0.0.3. `Core/Next/Playout.cs` adds ordered `graphics` layers, with the
  legacy `watermark` below them. `PlayoutItemConverter` projects permanent and
  intermittent watermarks in ascending ZIndex. Opacity-expression support is
  unchanged; invalid/unsupported selections still fail explicitly in ErsatzRS.
- `PrepareTroubleshootingPlaybackHandler` carries all selected watermarks;
  `TroubleshootController` stops truncating the selection. The Razor page keeps
  its responsive label/select row and uses the plural label and multiselect
  for both engines. No schema, persisted selection or scheduling reset changes.
- Rust targets: `next_engine/protocol.rs`, `watermark.rs`, host, dynamic/remote
  projection, fallback and troubleshooting; preserve all layers during source
  replacement. Reuse shared watermark selection, paths, geometry and timing.
- Web targets: system troubleshooting SSR wrapper, existing enhancement script,
  host/API preview boundaries. Keep the mounted form and existing responsive
  controls; reuse localized Watermarks in every culture. No new island or CSS.
- Proof targets: stable ZIndex ordering, geometry/timing and unsupported modes,
  publication/dynamic/fallback preservation, all selected preview IDs, decoded
  official Next multilayer output and unchanged audio/subtitles, plus Chromium
  and Firefox engine switching and narrow-screen/no-JS controls. Full window
  gates remain required. Row 105 is Port; UCU-06 owns passing scoped evidence.

## Row 104 source map: Windows bitmap-subtitle readrate

- Upstream `080039be` changes only the Windows FFmpeg archive from build
  `20138c8f` to `25d21b2c` and release notes. The new fix is
  `ErsatzTV/ErsatzTV-ffmpeg` commit `f85014496b6697e8cc156a0785c25a7fa3a53424`,
  `patches/0003-ffmpeg_demux-only-throttle-readrate-on-the-slowest-stream.patch`.
- It skips finished/unstarted streams, moves resume/lag state to the demuxer,
  and sleeps once based on the slowest active stream. This avoids the old
  per-stream sleep after sparse bitmap subtitle timestamps jump.
- ErsatzRS's published v2.0.1 packages build FFmpeg `n9.0`, which already
  contains the complete fix: reverse dry-run applies all three patch hunks
  with zero fuzz (three-line offsets). Row 99 covers the retained Vulkan fix.
  No host UI, schema, handler, selection or FFmpeg argv change is required.
- Verdict: Covered. Synthetic sparse PGS playback passes with the released
  Windows and Linux executables: 16 seconds/400 frames, left/right bitmap
  transitions across an eight-second packet gap, selected 880 Hz alternate
  audio and error-free full decode. This proves this row's stall fix for the
  fixture; it does not claim every sparse-stream pacing edge case is fixed.
  UCU-06 owns exact evidence; later archive updates remain pending.

## Row 103 source map: Next release notes

- Upstream `5febb943` changes only `CHANGELOG.md`: it announces Next QSV
  overlays, an audio normalization/synchronization correction and anamorphic
  scaling. It changes no engine source, runtime pin, application configuration,
  schema, UI, handlers or media selection.
- ErsatzRS uses the optional external Next engine through the existing adapter;
  this documentation-only commit supplies no new host integration to port.
  Verdict: N/A (upstream release notes). These announcements are not runtime
  proof of QSV, audio synchronization or anamorphic rendering in ErsatzRS.
- Existing Next acceptance and hardware limitations remain in their owning
  milestone/parity records; no requirement or limitation is waived here.
  Full diff inspection and progress validation are the checks for this row.

## Row 102 source map: Vulkan in containers

- Upstream `33c33587` removes `ETV_DISABLE_VULKAN=1` from its application and
  FFmpeg-test images, enabling the normal native/Next selection paths. There
  are no UI, schema, media-selection or scheduling changes.
- ErsatzRS has no blanket Vulkan disable. Native playback uses functional
  graph proofs (`graph_probe.rs`, `hardware_graph.rs`, pipeline selection);
  Next inherits the container environment through `next_engine/process.rs`.
  Preserve these checks and their existing health/troubleshooting reporting.
  Include NVIDIA driver capabilities in persisted runtime identity so enabling
  graphics invalidates old denials even when the binary and GPU are unchanged.
- The root and release runtime bases omit the Vulkan loader and Mesa ICDs.
  Released FFmpeg n9.0 fails in the Debian base with "Unable to open the
  libvulkan library". The alternate runtime has these libraries but requests
  NVIDIA compute/video/utility without graphics. NVIDIA's container contract
  requires graphics to inject Vulkan driver components.
- Targets: all three image recipes, hardware smoke scripts and Docker guidance.
  Provide Vulkan runtime libraries on supported GPU architectures and request
  compute/video/graphics/utility for NVIDIA. Keep ARMv7 CPU-only. Use the
  current managed FFmpeg package in validation. No new engine or host-driver
  installation policy is introduced.
- Minimal images stage Mesa's libraries/manifests/settings and declared library
  dependencies without the unrelated Python overlay-control utility, preserving
  their no-Python/no-compiler contract. The alternate build's obsolete `mysql`
  feature is removed after Cargo rejects it; SQLite remains the sole backend.
- Package-layer builds pass for amd64/arm64/ARMv7 and the alternate amd64 image.
  Actual Vulkan upload/scale/readback and CUDA/NVENC control pass. On this WSL
  host, Vulkan derived from CUDA fails because no Vulkan device matches the
  NVIDIA UUID; this is not a positive NVIDIA interop result. Native selection
  remains gated by functional proof; Next receives the driver environment with
  no blanket filter disable. UCU-06 owns detailed evidence and remaining gates.

## Row 101 source map: .NET dependency patches

- Upstream `11789e8d` changes nine `.csproj` files plus the changelog, with no
  C# application code, media model, FFmpeg graph or container artifact changes.
  Microsoft Extensions/ASP.NET packages move 10.0.10 to 10.0.11,
  System.CommandLine 2.0.10 to 2.0.11, and test-only NSubstitute 6.1.0 to 6.2.0.
  The explicit Microsoft.OpenApi 2.7.5 transitive override is removed.
- ErsatzRS uses Tokio/Axum, reqwest, tracing, clap and jsonwebtoken rather than
  those .NET packages. The removed override is not present in its Cargo graph.
  No Rust dependency change follows from these upstream package numbers.
- Verdict: N/A (upstream tooling). The Docker Vulkan changelog addition does
  not prove implementation; row 102 owns container changes and acceptance.
  Source/diff and Cargo-manifest review plus progress validation cover this row.

## Row 100 source map: upstream dependency automation

- Upstream `24eab260` deletes only `.github/dependabot.yml`: daily NuGet and
  three Docker directory update jobs assigned to its maintainer.
- ErsatzRS uses a separate weekly Cargo/GitHub Actions rollup and has no NuGet
  dependency graph. Upstream's automation ownership does not govern this repo.
- Verdict: N/A (upstream tooling). Keep the Rust repository's existing update
  policy. No product, runtime, subtitle, alternate-audio or schema change;
  source/diff review and progress validation are sufficient for this row.

## Row 99 source map: Windows Vulkan/CUDA export

- Upstream `9f83c8d2` changes the Windows FFmpeg archive and changelog only.
  The corresponding ErsatzTV-ffmpeg patch is
  `patches/0002-vulkan-cuda-export-fix.patch`: query the actual image creation
  flags and every backing image format before enabling external-memory export.
- ErsatzRS uses first-party FFmpeg 9 (`runtime.rs`, ADRs 0008/0063).
  Its v2.0.0 Windows archive and pinned BtbN build do not contain this fix;
  FFmpeg n9.0 still queries only format zero with substituted alias flags.
- Target: adapt the upstream patch to the first-party Windows package build,
  preserving FFmpeg 9's DRM modifier diagnostics and dedicated-memory handling.
  Native validation also exposed a separate Rust capability-probe defect:
  `hwmap` is unsupported for Vulkan-to-CUDA in FFmpeg 9. Playback already uses
  `hwupload_cuda`, matching upstream `Filter/Cuda/TonemapCudaFilter.cs`.
  Reuse playback's filter in the probe, with a final-argv snapshot. Version the
  persisted probe cache so an old false denial cannot survive this correction.
  No UI, media-library, subtitle or alternate-audio changes are required.
- Acceptance: patched package identity, native Windows CUDA control and the
  existing `graph_probe.rs` Vulkan/libplacebo/CUDA/NVENC graph, plus decoded
  output evidence. Both old and patched packages fail the old `hwmap` probe
  and pass the upstream CUDA-upload path on the tested Windows GPU. Thus the
  -40 failure proves probe drift, not the runtime patch's effect. The patched
  candidate also passes a 25-frame BT.709 encode/decode check with the corrected
  graph. UCU-06 owns detailed evidence. The approved v2.0.1 release is public;
  both downloaded archives match the tested hashes. The managed default selects
  that release. Row 99 is Port, with no new behavior deviation.

## Row 98 source map: live sources and Dolby Vision probe hints

- Upstream `7c96203b` changes only `ErsatzTV.Core/Next/Playout.cs`: it moves
  existing `is_live` properties/documentation and adds optional `dv_profile`
  to `VideoHint`. There is no runtime download pin or pipeline code change.
- Rust already exposes HTTP `is_live` and video `dv_profile`; persisted
  `DolbyVisionProfile` and freshly probed producer metadata populate the hint.
  Live-source selection already removes source seek fields.
- Acceptance: explicit profile 5/7/8 and absent-profile metadata preservation,
  existing live-source selection/callback tests, and strict infra Clippy.
  All 15 focused tests and strict infra library/test Clippy pass; UCU-06 owns
  the evidence. No new tone-mapping, GPU or Dolby Vision rendering acceptance
  is claimed.

## Row 97 source map: provider client generation and dependency contracts

- Upstream `aae2529f` upgrades .NET packages and changes only Refit client
  construction/registration to `ForGenerated`/`AddRefitGeneratedClient` for
  Emby, Jellyfin, Plex, GitHub and Trakt. Routes, headers, timeout policies,
  serializers and projections are unchanged. Rust uses explicit reqwest clients.
- SQLitePCLRaw 3.0.5 selects SQLite 3.53.4. ErsatzRS's shared SQLx/rusqlite
  binding uses SQLite 3.51.3, which already includes the WAL-reset fix.
  SQLx 0.9 requires libsqlite3-sys below 0.38; no unsupported binding fork is
  justified by this patch's fixes for 3.53.x regressions. Extend the linked
  runtime guard to protect the existing WAL fix as well as CVE-2025-6965.
- NCalc 7.1 adds null helpers/coalescing and changes its .NET configuration
  API; ADR 0098 deliberately retains the tested deterministic 6.2 expression
  profile. Jint/Skia/search/UI/test package changes map to the existing
  QuickJS, FFmpeg graphics, SQL search and Leptos alternatives (ADRs 0005,
  0009, 0096). No new behavior or exact newer-package equivalence is claimed.
- Evidence sources: [SQLitePCLRaw 3.0.5](https://github.com/ericsink/SQLitePCL.raw/releases/tag/v3.0.5),
  [SQLite 3.51.3](https://sqlite.org/releaselog/3_51_3.html),
  [SQLite 3.53.4](https://sqlite.org/releaselog/3_53_4.html),
  [Refit 15](https://github.com/reactiveui/refit/releases/tag/v15.0.0),
  [NCalc 7.1](https://github.com/ncalc/ncalc/blob/v7.1.0/CHANGELOG.md), and
  [Jint 4.15.3](https://github.com/sebastienros/jint/releases/tag/v4.15.3).
- Acceptance: linked SQLite guard, retained expression corpus and focused
  provider-client regressions, plus strict affected-crate Clippy. All pass;
  UCU-06 owns the evidence. Verdict: Adapt (ADRs 0009/0096/0098).

## Row 96 source map: .NET package patches and changelog note

- Upstream `3b53b69e` changes eight `.csproj` files and `CHANGELOG.md` only.
  CliWrap moves 3.10.2 to 3.10.4, Testably 7.0.1 to 7.0.2, Scriban.Signed
  7.2.5 to 7.2.6, Heron.MudCalendar 4.0.0 to 4.0.1 and Scalar 2.16.16 to 2.16.18.
- ErsatzRS uses Tokio/process-wrap for process ownership, Rust test fixtures,
  minijinja under ADRs 0058/0076, and Leptos admin components. No corresponding
  package or changed application consumer requires a port.
- The Mesa 25.2.8-to-26.0.3 sentence is a release note; this commit modifies no
  Dockerfile, driver package or runtime artifact. This verdict makes no new
  Mesa-version or AMD hardware acceptance claim.
- Verdict: N/A (upstream tooling). Source/diff review and progress validation
  are the relevant checks; UCU-06 window acceptance remains separate.

## Row 95 source map: optional Next subtitle style model

- Upstream `7b43e2f7` adds nullable `force_style` to
  `ErsatzTV.Core/Next/Config/ChannelConfig.cs`; it adds no producer or UI use.
- Target: `next_engine::protocol::SubtitleNormalization` and the existing
  profile-to-config constructor. Preserve optional values through JSON, leaving
  generated configurations unset and existing Burn/Convert selection unchanged.
- Acceptance: populated, empty, null and absent style serialization coverage,
  existing config/protocol tests and strict infra Clippy. No UI, scheduler,
  native FFmpeg, health requirement or dependency change is needed.
- Null omission matches upstream `Converter.Settings.WhenWritingNull`. All 11
  config/protocol tests and strict infra/web library/test Clippy pass. Review
  confirms the only constructor leaves styling unset; UCU-06 owns evidence.

## Row 94 source map: Next troubleshooting selection and diagnostics

- Upstream `9088ae1a`: `NextLogger`, `NextSessionWorker`, Prepare/Start
  troubleshooting handlers, `PlayoutItemConverter`, `InMemoryLogService` and
  `PlaybackTroubleshooting.razor`.
- Outcome: custom stream selectors work with Next troubleshooting, explicit
  subtitle candidates reach selection, diagnostic levels/targets are retained,
  and completed Next previews report FFmpeg speed from immediate-child
  `ffreport.log` files. Normal scheduled conversion stays quiet.
- Existing coverage: row 93 already carries the Next engine identity and
  explicit subtitle/off choices through isolated local/provider/remote
  projection. Reuse those decoded fixtures while auditing selector precedence.
- Gaps found: the Next host clears the request's selector; the UI hides that
  field for Next; the process boundary discards stdout/stderr; completion logs
  contain only a summary and no parsed speed. Preserve managed process cleanup,
  bounded output and private-input redaction while connecting diagnostics.
- Targets: shared playback/custom selectors, Next selection/process boundary,
  troubleshooting host completion result, and existing preview/log UI.
- Visual/hydration boundary: keep the row-93 SSR GET form and progressive
  enhancement. Stream Selector remains mounted and available for both engines;
  graphics remain Legacy-only. Add localized speed alongside Logs without
  moving the page into an island or weakening no-JS submission.
- Required proof: custom audio/subtitle selection and explicit-off precedence,
  scheduled-selection regression, bounded log framing/level parsing and cleanup,
  real Next completion speed/report evidence, then the existing serialized
  Chromium/Firefox playback and no-JS cases with selector/speed assertions.
- Exclusions: no producer protocol changes, persistent playout mutation,
  dependency replacements or acceptance reductions. Row 94 is accepted; UCU-06 owns the evidence.
- Selection checkpoint: retain the submitted selector and mark media previews
  as Custom; without a selector or explicit subtitle, supply no subtitle
  candidates. Filter explicit candidates before YAML audio/subtitle rule
  evaluation, using temporary forced eligibility while returning unchanged
  metadata for retained-caption validation. Existing channel pins and scheduled
  projection keep their selection policy. All 76 focused troubleshooting and
  Next selection regressions pass (Nextest `a8e22597`), as does strict library
  Clippy for infra/web. That checkpoint left UI and diagnostics acceptance open.
- Completion-speed implementation: the existing log API returns a typed
  completed result (`text`, optional finite `speed`) under the same preview
  ownership. Native stdout progress and Next's immediate `ffreport.log`
  children supply the measured speed. The mounted Logs section shows the
  localized value; missing/malformed progress remains absent. Keep reads,
  directory traversal and process-pipe buffers bounded, and drain excess output.
  Serialized Chromium/Firefox cases now submit a synthetic custom selector
  with both engines and require real measured speed. The official Next fixture
  also decodes selected Dutch audio/subtitles over an English default. UCU-06
  owns this checkpoint's evidence. The diagnostic implementation below completes
  the remaining row acceptance.
- Diagnostic implementation: process-owned stdout/stderr readers use 8-KiB line
  and 256-KiB retained-tail limits, preserve upstream levels/targets, and sanitize
  before either tracing or retention. Oversized lines are discarded intact;
  readers are drained and bounded by managed cleanup. Opt-in preview selection
  logs retain effective language preferences, per-rule content/audio/subtitle
  matches and selected stream IDs, including deferred provider callbacks.
  Scheduled selection stays quiet. ADR 0095 clarifies that raw output is never
  retained directly. Focused process, redaction, selection, official runtime,
  Chromium/Firefox and the full web parity gate pass; UCU-06 owns the evidence.

## Row 93 source map: Next troubleshooting playback

- Upstream `ad9848cc`: `PlaybackTroubleshooting.razor`,
  `TroubleshootController`, Prepare/Start troubleshooting commands and handlers,
  `ChannelConfigConverter`, `ArchiveTroubleshootingResultsHandler`, and
  `FileSystemLayout`. Native behavior remains available as Legacy.
- Request contract: engine defaults to Legacy; the Next API retains the first
  supplied watermark. Stream selector and graphics controls are Legacy-only. Preserve
  media/channel selection, profile override, subtitles, audio and seek/start.
- Next preparation shares normal channel config conversion, sets whole-second
  virtual start, publishes one isolated preview item, supplies config on stdin,
  applies default overlays and starts `run --troubleshoot`. Reports belong to
  the preview directory; redirect to `ffmpeg.m3u8` instead of `live.m3u8`.
- Lifetime: upstream preparation rejects overlapping troubleshooting and the
  worker releases ownership after process completion and diagnostic writes.
  Share that ownership across Legacy and Next before clearing preview files;
  bound the process run to upstream's two-minute limit.
- Archive contract: inspect the root and immediate child directories; include
  existing native diagnostics plus names containing `ffreport` and `outcome`.
  An FFmpeg or ffreport file is required before returning an archive. Preserve
  upstream basename ZIP entries and exclude media segments.
- ErsatzRS targets: app `PlaybackPreviewRequest`, web API and host
  `troubleshooting`, infra Next config/process/session/projection. Existing
  `NextHost::start` handles channel previews but currently loads persisted
  channel/profile/playout state; an ad-hoc troubleshooting request must not
  mutate the normal channel playout or silently use its profile.
- Remote Stream previews own a transient selection in the existing authenticated
  callback registry. Preserve that selection through prepared-artifact and
  producer track remapping, and revalidate source identity after preparation.
  Pass the admitted session to the finite Next process, revoke callbacks after
  process termination, and await producer cleanup before releasing capacity.
- Exclusions: no provider scan changes, production channel mutation, alternate
  engine replacement, or acceptance reduction. No deliberate deviation planned.

### Row 93 visual and hydration source map

- Route/page: playback troubleshooting; SSR `PlaybackTroubleshootingPage` and
  `render_playback_form` in `admin/pages/system/troubleshooting.rs`.
- Upstream geometry: toolbar download actions above the settings panel;
  divided sections; labelled field rows that stack on narrow viewports; engine
  selector before profile; preview player and Play action below settings.
- Data: blocking troubleshooting context resource, existing server function,
  repository-backed profiles, watermarks, media and subtitle options.
- API/fallback: GET form `/api/troubleshoot/playback.m3u8`; archive GET remains
  usable without JavaScript. Preserve this fallback when adding engine controls.
- Current page has no form island. Reuse its CSS module, field helpers and
  existing preview handling; any hydrated visibility controls must keep DOM
  nodes mounted and avoid prop-bearing islands across unresolved Suspense.
- The form now uses a small delegated `troubleshooting-preview.js` enhancement:
  engine changes hide/disable mounted Legacy fields, constrain Next watermarks,
  and submit once before handing the redirected playlist to the shared player.
  The SSR GET form and archive link remain usable without JavaScript. No new
  resource/island boundary or provider request contract is introduced.
- Completion logs use the existing troubleshooting application port and a
  bounded GET read. Active preview ownership suppresses partial diagnostics;
  browser polling stops on completion, navigation, or the two-minute deadline.
- Browser proof: engine selection, Legacy-only fields, single Next watermark,
  serialized request and successful preview/report download; native regression,
  no-JS submission, narrow-screen geometry and no hydration errors.
- Row 93 is Port. UCU-06 owns backend, decoded runtime, browser/no-JS and
  web parity-gate evidence; row 94 owns subsequent upstream improvements.
- Next source audit (`$ERSATZTV_NEXT_SRC`, `f6350a0`): troubleshooting exits
  after its first transcode, writes a dossier on success as well as failure,
  and retains an ended FFmpeg playlist. The ordinary `NextProcess` readiness
  race treats exit-before-ready as failure, so its live-session startup must
  not be blindly reused for a short successful troubleshooting process.

## Row 92 source map: provider external subtitle recovery

- Upstream `f737de89` changes `DeleteOrphanedSubtitlesHandler`,
  `MetadataRepository.UpdateSubtitles`, its local and media-server callers,
  and the `Fix_MediaServerExternalSubtitles` data migration.
- Provider subtitle identity is owner, subtitle kind and stream index. Local
  sidecars retain case-insensitive filename identity. Repeated scans preserve
  row identities; changed sidecar paths update metadata without collapsing
  distinct language streams that share a provider path.
- Empty-path cleanup applies only to local Movie, Episode, MusicVideo and
  OtherVideo owners. Provider sidecars can be fetched by index without a path.
  Ownerless cleanup, bounded atomic batches and cached files remain protected.
- ErsatzRS targets: infra metadata persistence, the shared cleanup/health
  predicate, and a one-time SQLite repair invalidating Jellyfin/Emby movie and
  episode Etags only where external MediaStreams exist. Existing subtitle rows
  and cached files survive the repair; ordinary scans restore missing rows.
- Acceptance: distinct provider streams, stable rescans, cache invalidation,
  local/provider cleanup ownership, repair targeting and idempotence, ordinary
  provider scan recovery, and playback availability. No browser UI change or
  MySQL migration is required. Audit and implementation are in progress.
- Playback audit: upstream `GetSubtitlePathByIdHandler` resolves provider
  endpoints before input construction. Jellyfin uses item identity and stream
  index; Emby additionally needs the media-source identity stored in `Path`.
  ErsatzRS currently selects restored rows but `subtitle_input_for_stream`
  cannot construct text input from their absent paths. Emby stream projection
  also omits that source identity. Port the missing resolution through existing
  protected provider access, shared by native playback, troubleshooting and
  Next; do not expose credentials in stored subtitle paths or diagnostics.
- Index mapping: ErsatzTV adds `JellyfinStream.ExternalStreamOffset` (100,000)
  when scanning, then subtracts it for the provider URL. ErsatzRS stores the
  provider's raw index, so endpoint construction must use that raw index without
  subtraction. Emby's `EmbyApiClient` puts the media-source ID in external
  stream `FileName`; the matching local projection is now implemented and its
  HTTP rescan test passes. Shared provider subtitle bindings now resolve exact
  owning-server and item identities, encode endpoint path segments and preserve
  configured server base paths. Both provider HTTP tests prove language
  selection and endpoint mapping; SQLite uses primary-key lookups throughout.
  Authenticated retrieval now shares the existing owning-server credential
  selection with Next media opens. HTTP fixtures pass downloads, failed
  authentication, redirect refusal and redacted errors. Downloads have a
  15-second deadline and 32 MiB limit. Native/Next artifact integration and
  runtime availability remain open.
- Native playback now prepares an atomically published local artifact before
  constructing subtitle input, retaining the provider row/index in the database.
  Both provider HTTP tests prove artifact bytes and native input construction.
  Next projection now records pending provider preparation without network I/O;
  programme and fallback callbacks fetch into the existing bounded subtitle
  cache, then revalidate publication identity. The cache key includes downloaded
  content. Next checks and runtime proof remain pending, as does troubleshooting
  integration.
- Troubleshooting now prepares provider artifacts and propagates preparation
  failures. The web-host check passes. Official Next `802f2d8` (staged release
  archive SHA-256 `f4f06196f7ecb751ee973d6754cf3292f22ab1a7f92e85e836ee490056631808`)
  passes authenticated Jellyfin/Emby external subtitle Burn and Convert through
  real demand-time callbacks, with decoded glyph/WebVTT, seek and Dutch audio
  checks. Native decoded artifact proof and final row review are running.

Row 92 closeout: all 30 affected tests, native and official Next decoded
playback, strict infra/web all-target Clippy, format, progress and policy checks
pass. Review confirms source-scoped credentials, no redirect forwarding,
stale-selection rejection, atomic reusable artifacts, no network under schedule
locks and unchanged local subtitle identity. Verdict: Port. Pin, ledger and
upstream checkout advance together to `f737de89`; row 93 is next.

## Row 91 source map: Unicode MPEG-TS script inputs

- ErsatzTV service: `ErsatzTV.Infrastructure/FFmpeg/MpegTsScriptService.cs`
  at `5ab81525`, plus `ErsatzTV/Resources/Scripts/MpegTs/run.bat` and `run.sh`.
- Upstream behavior: pass `ETV_CHANNEL_NAME`, `ETV_HLS_URL` and
  `ETV_FFMPEG_PATH` through the process environment; remove double quotes from
  the channel name. Built-in scripts consume these variables; legacy custom
  template substitution remains available. The row also updates the Windows
  FFmpeg 8.1.2 archive, superseded by accepted ADR 0063's FFmpeg 9 runtime.
- ErsatzRS target: `assets/Scripts/MpegTs/run.*` and the MPEG-TS script command
  renderer in `crates/ersatzrs-web/src/transport/iptv/stream.rs`.
- Process boundary: `FfmpegCommand.environment` already reaches the managed
  child process. Both Windows batch and Unix bash commands must carry all three
  exact variables without putting Unicode values into the bundled script body.
- UI, database and track selection: unchanged. Preserve wrapper muxrate/PCR
  options, authenticated HLS URLs, all-stream copy, script cleanup and fallback.
- Acceptance: cross-platform command environment and template compatibility,
  real child-process Unicode value propagation, focused checks and review.
  Native Windows and Unix batch/shell playback both pass; UCU-06 owns evidence.

## Row 90 source map: upstream release metadata

Upstream `35b5a259` changes only CHANGELOG.md: a 26.7.1 heading dated
2026-07-31 and the Unreleased/26.7.1 comparison links. Its commit subject says
26.7.0, but the diff is 26.7.1. ErsatzRS retains its own version and release
history. Verdict: N/A (upstream release metadata); no runtime or package change.
Rows 68-90 are reconciled. UCU-05 remains In progress until its full window
acceptance and authorized push plus exact-SHA terminal green CI are recorded.

## Row 89 source map: playback archive engine versions

Upstream `b5fd035b` adds only `NextVersion` and `FFmpegVersion` to the anonymous
object written by `StartTroubleshootingPlaybackHandler`. Row 80 commit
`f6f076a2` already supplies both top-level keys in the native preview writer's
`troubleshooting_info.json`, using shared `TroubleshootingVersions::detected`.
The archive member filter includes JSON files, so these fields reach the
playback ZIP as well as General diagnostics. The Next version comes from the
startup cache and the FFmpeg version from the accepted runtime; absent optional
Next is `unknown`, retaining ADR 0095's accepted optional-engine contract.
Verdict: Covered. Reuse row 80's 26 passing library tests (including both Next
presence states) and current source inspection; no code change or duplicate
runtime gate is needed for this two-field addition.

## Row 88 source map: upstream Windows FFmpeg archive

Upstream `57a0c40d` changes only the Windows artifact download URL in
`.github/workflows/artifacts.yml` and its changelog. The archive remains FFmpeg
8.1.2 but changes build revision from `g82f576a8` to `gd3cc14b8` for QSV fixes.
No pipeline code, capability check or scheduling behavior changes in this row.
ErsatzRS acquires its own first-party runtime through `ersatzrs-ffmpeg::runtime`
under ADRs 0008/0063; its supported line is FFmpeg 9. Importing this 8.1.2 archive
would violate the accepted runtime contract. Verdict: N/A under ADR 0063 for
this upstream packaging pin. This source classification provides no new native
Windows/QSV execution evidence and does not substitute for runtime acceptance.

## Row 87 source map: build and on-demand gap consistency

Upstream `3d24c3f0` moves gap insertion into the locked build completion path,
after any time shift; its adjacent-item calculation rejects zero/negative gaps.
The on-demand shifter trims gaps finishing before the checkpoint and shifts all
retained gaps with the items. No layout or localization changes are involved.

Native `SqlitePlayoutRepository::apply_build_result` already commits items and
scheduler-provided gaps atomically, but Classic supplies no complete gap list
and Continue retains stale projections. Rebuild the adjacent-item projection
from the final retained/appended SQL item set in that transaction. Match the
upstream adjacent-item rule exactly. YAML gap hints still drive fallback filling
before persistence; the UI projection is based on the resulting items.
`hls_worker::time_shift_on_demand_playout` must trim/shift gaps in the existing
transaction alongside item timestamps. Unrelated playouts remain untouched;
any projection/shift failure rolls back all associated item changes.

UI source map: `/playouts`, SSR wrapper and `PlayoutsIsland` in
`admin/pages/scheduling/playouts.rs`; detail/count server functions delegate to
repository `list_detail_admin`/`count_detail_admin`. Existing localized negative
filler-kind rows display UNSCHEDULED; shared primitives and SSR/WASM structure
are unchanged. Focused browser target is the existing playout-detail workflow
in `tests/e2e/playouts-feedback.spec.ts`, paired with actual repository producer
and on-demand transaction tests. UCU-05 owns passing acceptance evidence; row 87 is Port.

## Row 86 source map: flexible fixed starts near midnight

Upstream `a7ecc5f9` changes `PlayoutModeSchedulerBase.GetStartTimeAfter`:
flexible starts wait for the next occurrence only when it is at most 12 hours
away; a peek from an active Flood still advances to tomorrow. Tomorrow's local
wall-clock offset is resolved independently to preserve DST boundaries.
The matrix covers midnight, early morning, recently missed starts on either
side of midnight, same-day future starts, all calendar dates and a real
three-item builder sequence around midnight.

Classic now carries an explicit civil clock in reference data. The SQL loader
resolves the host zone once per build; deterministic core fixtures use UTC or
an explicit named zone. Jiff 0.2.23 supplies real transition offsets, including
platform system-zone discovery. Chrono's `LocalResult::None` alone cannot expose
both sides of a missing civil time, and the existing YAML helper deliberately
retains the current offset. No new parser or OS-specific implementation is added.
Jiff's MIT license passes repository policy. UTC checkpoint dates and complete
Refresh state remain unchanged under ADR 0020.

The flexible rule and Flood peek resolve tomorrow independently; Strict retains
upstream's wall-time comparison and offset-preserving AddDays behavior. Initial
state no longer pre-advances a flexible late-night start before applying the
12-hour rule. Clock resolution errors propagate before publishing a build.
Tests cover the upstream clock matrix, the inclusive 12-hour bound, incomplete
modes, Flood peek, every 2026 date in four zones, and the real three-item builder.
A .NET runtime 10.0.12 oracle (SDK 10.0.112) checks gap/fold offsets in Amsterdam,
Dublin, Lord Howe and Casablanca, plus political clock changes in Apia/Moscow.
SQLite restart Continue and Refresh across both DST transitions preserve future
identities, UTC timestamps, track preferences and scheduling contexts. UCU-05
owns final evidence. Row 86 is Port; row 87 follows.

## Row 85 source map: complete Classic collection identities

Upstream `ecf6c2c1` groups Refresh anchors by every CollectionKey field and
selects the earliest same-day checkpoint. This repairs dropped playlist and
synthetic anchors. ErsatzRS already groups its versioned complete checkpoints
by serialized source keys (ADR 0020), including playlist and synthetic keys.
The audit found a missing RerunCollectionId in SchedulingCollectionKey, anchor
projection and matching; distinct rerun sources can share an enumerator.
A focused regression reproduces wrong media selection before the correction.
Targets: shared scheduling key/history serialization, Classic item/anchor
mapping, SQL key hydration, and production restart/Refresh tests. Preserve
legacy refusal and full key identity; no UI or rebuild-policy changes.
The correction includes the rerun ID in source/history keys, persisted anchors,
and matching. Non-rerun JSON is unchanged. Restart/Refresh and health checks
preserve populated timelines and refuse ambiguous legacy anchors. The 21-day
playlist fixture proves progress survives Refresh. Verdict: Adapt under ADR
0020 (complete checkpoints and explicit legacy refusal); UCU-05 owns evidence.

## Row 84 source map: channel-logo watermarks

Upstream `15db3c4c` changes the three precedence branches in
`ErsatzTV.Core/FFmpeg/WatermarkSelector.cs` to accept external/generated logo
URLs without filesystem existence checks. Its regression matrix covers external
URLs, generated logos, cached files and missing cached files. No editor or
validation contract changes.

Native targets are the shared `playback` watermark resolver, Next watermark
projection, and the existing generated-logo transport. The resolver initially
excluded HTTP logos and had no generated fallback. Explicit source typing now
preserves URLs for native and Next inputs, with focused and Next runtime proof
in UCU-05. Channels without artwork now receive a cached PNG rendered with
existing FFmpeg drawvg/drawtext support and the bundled font. The SVG endpoint
shares the abbreviation helper; its response is unchanged. Rendering is bounded
and only complete PNGs are atomically promoted. Explicit missing local artwork
still produces no watermark, matching upstream. Native and Next decoded output
checks pass. Verdict: Port; UCU-05 owns the evidence and remaining window gates.

## Row 82 source map: text subtitle playback

Upstream `4ffd775a` changes `UpdateChannelHandler`,
`StartFFmpegNextSessionHandler`, both embedded-subtitle extraction handlers,
`PlayoutItemConverter`, `InternalController`, and generated Next wire models.
It adds the font cache to subtitle normalization, extracted text sidecars with
source in/out points, republishing after extraction, channel-save invalidation
for subtitle/credits modes, disabled-credits handling, and valid empty subtitle
documents after seeking beyond the final cue. There is no UI layout change.

Native targets are `next_engine::{protocol,host,selection,credits}`, the shared
subtitle extraction/cache paths, channel save orchestration, and subtitle HTTP
transport. ADRs 0095/0100 retain official Next integration and explicit refusal
of unsupported live/one-shot embedded text. Existing version-bound demand-time
preparation must not be replaced with an unvalidated legacy cache path.

The initial audit found the `fonts_folder` wire field and host configuration
missing; the partial correction and red/green test are recorded in UCU-05.
The web save wrapper already notifies the shared registry, including Next and
preview sessions. Existing exact-version demand preparation validates the source
before publishing a sidecar, so Next does not depend on legacy background cache
completion. Playout builds also extract before notifying consumers. Native
extraction maps absolute stream indexes, preserving partially extracted sets.
The new runtime matrix proves continued video/audio after the final text cue in
embedded/external Burn/Convert; a separate isolated fixture proves cached Sen
font selection and decoded captions. Disabled credits leave the projected tracks
unchanged. Generated wire-model comparison adds only `fonts_folder` to the field
set; existing typed serialization retains the other names. Verdict: Adapt under
ADRs 0095/0100. UCU-05 owns test identities and remaining window acceptance.

## Row 80 source and hydration map

Upstream `9cc771be` adds detected Next/FFmpeg versions to General diagnostics
and the playback troubleshooting archive. Native targets are
`admin/server_fns/troubleshooting.rs` and `host/troubleshooting.rs`.
Reuse the startup-only Next probe and accepted FFmpeg runtime report; requests
must not spawn new probes. Share the startup Next value with the archive runtime.
Keep the existing `Version` key for ErsatzRS and add `NextVersion` and
`FFmpegVersion`; an unavailable optional engine is reported as unknown.

- Route/SSR wrapper: `/system/troubleshooting`, `TroubleshootingPage`.
- Islands: none changed; General is an SSR report with unchanged DOM shape.
- Boundary: `load_troubleshooting_report_with_state`, shared runtime versions.
- API/export: existing playback preview artifacts and archive endpoint.
- No-JS: the initial browser proof found the report stuck at Loading because
  the default streaming route inserts resolved content through JavaScript.
  Use the existing `SsrMode::InOrder` pattern on this route so General JSON
  arrives as ordinary HTML through the existing report block.
- Shared primitives: none; sidebar reads the same startup Next value.
- Browser proof: focused General JSON assertions with and without JavaScript,
  including agreement with the sidebar when Next is available.
- Localization: existing localized tabs and unavailable sidebar label remain;
  diagnostic JSON field names retain the machine-readable English contract.

Verdict: Adapt under ADR 0095: ErsatzRS retains its `Version` diagnostic key
instead of adopting the upstream Legacy label, and reports the detected version
of the optional Next executable. All 26 focused library tests and four General
browser checks pass in Chromium/Firefox with JavaScript enabled/disabled.
The populated/absent Next contract has unit coverage; the isolated browser
fixture reports Next as unknown. UCU-05 owns the complete evidence.

## Row 79 source map: Block build loop safety

Upstream `a2f54ef1` guards nonpositive default-filler durations, excludes
existing rows that persistence will expire, uses offset-aware finish times
during Reset, and observes cancellation while filling gaps. Its primary-item
fix removes reliance on an optional enumerator minimum duration. Rust's
primary loop is already bounded and stops on an oversized candidate. Local
checkpoint `ee8f7c34` added the duration guard and expired-row filter, matching
SQL's nonnegative mirror offset and inclusive retained boundary.
Reset now replaces filler and matching history at the UTC rebuild boundary,
retains primary/GuideMode items, and repositions retained primary/history pairs
when block-start filler changes length. Missing history refuses the build
before persistence. Break insertion is Reset-only, as upstream requires.
The scheduler's shutdown signal reaches Block's filler loop through the shared
build dispatcher. CPU work runs on an awaited blocking worker so the async
executor remains responsive; cancellation returns no partial build to persist.
Core, SQLite cancellation/retention, maintenance and scheduler evidence belongs
in UCU-05. Verdict: Port. Row 80 follows; window gates remain open.

## Row 78 source map: playlist enumeration consistency

Upstream `16b71569` changes `PlaylistEnumerator` and the shared playable-item
filter in `SeasonEpisodeMediaCollectionEnumerator`. `PlayoutBuilder` passes
the Classic item's `MarathonShuffleGroups` to playlists; `MarathonHelper`
constructs a playlist from grouped media. The regression contract covers
cycle completion with season-zero specials, deterministic reconstruction after
a shuffled cycle, and complete restoration after a rejected filler candidate.
No UI or schema changes occur in the upstream row. Rust targets are the shared
`scheduling::PlaylistEnumerator`, Classic construction/checkpoints, YAML
marathons and SQL-backed restart regressions. Existing audio/subtitle override
identities survive ordering and reconstruction. Production tests exposed
flattened Classic/YAML SQL loaders; both now supply structured children.
Classic Marathon honors all five group types, batches and shuffle flags.
YAML history stores versioned snapshots at build/cycle boundaries and candidate
swaps, replays the saved membership before reconciling current media, and keeps
the required baseline during cleanup. Incompatible history refuses an unsafe
build before clearing rows; explicit Reset establishes compatible state.
Verdict: Adapt under ADRs 0010/0020, using Rust identity checkpoints and the
existing safe-rebuild contract. Core, SQL restart/Refresh, failure/Reset and
scoped Clippy evidence is recorded in UCU-05; window gates remain open.

## Row 77 source map: shuffled collection index retention

Upstream `6ff44cf5` validates a saved shuffled index against the flattened
media-item count instead of the smaller multipart-group count. Its alternate
schedule regression preserves index 9 for ten episodes represented by nine
groups. ErsatzRS's Classic `MediaEnumerator` already stores a flat item vector
and bounds `set_index` and `current` against its full length. A new core test
proves index 9 of ten show items selects the expected item. Source comparison
also exposed a separate loss: Classic Continue omitted undated collection
anchors belonging to inactive alternate schedules when publishing the next
state. The corrected publication retains those anchors, and the runtime audit
checks the active source set while allowing dormant alternate state. A restarted
SQLite build preserves an inactive Sunday's saved seed and index through a
Wednesday Continue. The existing multipart grouping and schedule-flag gaps
remain explicit in `classic-scheduling.toml`; this row does not claim that
missing feature. Verdict: Adapt under ADR 0010; UCU-05 window gates remain open.

## Row 76 source map: daily Classic checkpoints

Upstream `f7bdcede` moves checkpoint creation from pass boundaries to the
moment scheduled items cross a local day seam, because a final partial pass
can cross midnight without leaving a checkpoint for that day. It also keeps
one checkpoint per collection and local date. ErsatzRS already captures
dated source anchors and full Classic context at pass boundaries, but its
final pass omitted a capture. A seven-day, shuffled, hourly Continue test
failed on the newly covered final day before the fix. The Rust scheduler now
captures after the block that crosses midnight, emits one generated checkpoint
per UTC schedule day, and reuses persisted Continue checkpoints for that day.
The first SQLite Refresh proof exposed a second loss: clearing the entire
playout discarded the item crossing midnight before the first checkpoint.
Refresh now retains the item, gap and history prefix through the selected
checkpoint, then replaces the future transactionally. This preserves primary
content under accepted ADR 0097. The full-window regression, 249 core tests,
and a restarted production SQLite Continue-to-Refresh proof pass. The health
check already audits the complete checkpoint before Refresh. Verdict: Port;
window gates remain open.

## Row 75 source map: FFmpeg green-line workaround removal

Upstream `7850ced6` deletes `AmdCropMetadataWorkaroundFilter` and
`NvidiaGreenLineWorkaroundFilter`, removes their insertion from VAAPI and
NVIDIA pipeline builders, and updates its Windows FFmpeg artifact. Its release
notes tie the old bottom-edge artifacts to FFmpeg and the new 8.1.2 runtime.
The tagged upstream package builds from the official 8.1.2 tarball; ErsatzRS
uses its first-party FFmpeg 9.0 build under ADR 0063. The Rust pipeline still
inserted the same `h264_metadata`/`hevc_metadata` bottom crop options. Removed
those options and their NVIDIA applicability gate. The two affected argv
snapshots assert no `-bsf:v`; 344 FFmpeg crate tests pass. A synthetic 1080p
NVENC overlay encoded by the shipped FFmpeg 9 payload without crop metadata
retained neutral bottom rows matching the source. AMD live hardware is not
available on this host; AMD command generation is covered by its snapshot.
Verdict: Port. UCU-05 window playback gates remain open.

## Row 74 source map: Jint dependency patch

Upstream `6d8666b8` changes one NuGet package reference in
`ErsatzTV.Infrastructure.csproj`: Jint 4.13.0 to 4.14.0. There is no C#
source, JavaScript callback, selector result, media metadata, UI, database,
health or playback change in the commit. ErsatzRS uses `rquickjs` under
accepted ADR 0096, with its own bounded interpreter and tests. No Rust crate
or source contract maps to this Jint-only version update. Verdict: N/A
(NuGet-only update under ADR 0096).

## Row 73 source map: FFmpeg 8.1.2 packaging and health

Upstream `5855deb8` changes release artifact acquisition, four Docker bases,
the FFmpeg version health check and changelog from the FFmpeg 7.1 line to
8.1.2. It does not change filter graphs, transcoding commands, media models,
playout, subtitles or alternate-audio selection. ErsatzRS's accepted ADR 0063
requires first-party FFmpeg 9 and rejects 8.1.2 at startup; its health check
reports the resolved runtime version against that supported line. ADR 0008
retains package provenance and architecture, not the superseded version pin.
Verdict: Adapt (ADR 0063); no Rust package or health change is appropriate.
The existing FFmpeg 9 runtime, package and health acceptance evidence remains
the relevant proof; UCU-05 window gates remain open.

## Row 72 source map: config-folder keyed singleton mutex

Upstream `16e341d9` changes only `ErsatzTV/Program.cs` and its changelog:
the Windows global singleton mutex name now hashes `AppDataFolder`, letting
different configuration folders run concurrently. ErsatzRS has no named
process-wide mutex in its server or launcher, so there is no equivalent Rust
lock to re-key. ADR 0080 explicitly leaves multiple active ErsatzRS server
instances outside the supported deployment contract. No model, schema, UI,
scanner, playout, subtitle or audio behavior changes in this row. Verdict:
N/A (upstream-only singleton mutex; ADR 0080 deployment boundary).

## Row 71 source map: repeat playlist offsets on every build

- ErsatzTV UI/model: no setting or schema change; the existing Classic Random
  Start Point flag remains the input.
- ErsatzTV `PlaylistEnumerator.Create` now reapplies the seeded child offsets
  on every build, even when the playlist state's Started flag is true.
  `PlayoutBuilder` passes the active schedule flag directly to playlists.
- The new upstream test builds three chronological playlist children, saves
  state halfway through, rebuilds from the same seed and proves its continuation
  matches the uninterrupted sequence.
- ErsatzRS target: Classic recreates child offsets whenever the flag is on,
  then replays a retained Continue index before scheduling. A production-engine
  regression compares a 6+6 item Continue build with an uninterrupted 12-item
  build, including media identity and timestamps. A Refresh checkpoint still
  restores its complete source state after initialization.
- Intentional deviations: Rust uses its existing deterministic seed mapping;
  no UI, transport or migration change is needed for this row. Later row 85
  still owns broader playlist progress repair beyond this random-start path.

## Row 70 source map: durable enumerator Started state

- ErsatzTV model and persistence: `CollectionEnumeratorState.Started` is added
  to schedule-items, fill-group and collection enumerator state tables. All
  enumerator constructors and ResetState paths copy it; MoveNext marks it true.
- ErsatzTV builder: passes the active Classic schedule setting each build and
  gates random initialization with `!state.Started`, including when Index wraps
  to zero. Chronological and SeasonEpisode bounds now include the last item;
  playlist children receive the same correction.
- ErsatzRS domain/SQLite: `CollectionEnumeratorState.started` defaults false
  for old JSON. An additive SQLite migration adds Started to all three tables.
  Anchor, source and fill-group repository reads/writes persist the flag.
- ErsatzRS Classic: schedule, media and fill-group advancement mark state
  started. New or retained-but-unplayed sources can receive the configured
  random initial choice; a played source at index zero does not restart. The
  existing seed modulo range already includes the final item.
- Tests: an old-schema migration preserves legacy state with a false default;
  repository round trips true for all three state tables; Classic covers fresh,
  unplayed anchored, played anchored and final-index playlist cases. Row 71
  subsequently adds the every-build playlist rule.
- Intentional deviations: Rust's deterministic seed-to-index choice remains
  different from .NET PRNG bytes; selection and persistence behavior match.

## Row 69 source map: Classic playlist random start

- ErsatzTV UI/model: the existing Classic schedule Random Start Point setting
  also applies to playlist children; no new field or route.
- ErsatzTV builder: `PlayoutBuilder.GetMediaCollectionEnumerator` passes the
  setting into `PlaylistEnumerator.Create`. The enumerator seeds its choice from
  the saved collection state and applies it only when the state index is zero.
- ErsatzTV orders: Chronological and SeasonEpisode children receive an initial
  media index; ShuffleInOrder receives the setting. Already randomized orders
  retain their existing selection. Other Block/YAML/Next call sites pass false.
- ErsatzRS target: `ersatzrs-core::scheduling::classic` initializes eligible
  playlist children with deterministic start indices only for a fresh source.
  Retained anchors and Refresh checkpoints bypass this new initialization;
  existing playlist continuation behavior remains owned by later row 85.
- The same upstream commit updates .NET package manifests and test null guards.
  These have no Rust dependency or behavior counterpart in this row.
- Intentional deviations: ErsatzRS uses its existing seed-to-index choice rather
  than reproducing the .NET PRNG. Row 70 adds explicit Started state and
  full-range selection; row 71 still owns the later playlist rule.

## Row 68 source map: Plex movie and other-video title refresh

- ErsatzTV UI/pages: no route or control changes; updated library titles flow to
  existing media listings and search.
- ErsatzTV response/model: Plex movie and Other Video records carry `Title` and
  `UpdatedAt`. `PlexEtag.ForMovie` now includes title in its fingerprint, so a
  title-only change reaches ordinary periodic scans and the first scan after
  upgrading refreshes the old fingerprint.
- ErsatzTV handlers/scanners: `PlexMovieLibraryScanner` and
  `PlexOtherVideoLibraryScanner` compare both `Title` and `SortTitle` with the
  existing metadata after a changed fingerprint.
- ErsatzTV repositories: `IMovieRepository` and `IOtherVideoRepository` write
  `Title` and `SortTitle` together; no schema change.
- Side effects: changed movie/Other Video titles become visible after sync;
  unchanged records retain the existing no-write fast path. No subtitle,
  alternate-audio, playout or transport behavior changes.
- ErsatzRS target: `ersatzrs-infra::plex` projection, Etag skip and metadata
  upserts; `tests/plex_service.rs` covers title-only ordinary rescans for both
  library kinds, metadata persistence and unchanged-scan idempotency.
- Intentional deviations: none. ErsatzRS uses a versioned title-aware Etag
  representation rather than byte-identical upstream hashing; its persisted
  skip contract and one-time old-Etag refresh are the behavior that matters.

## Row 67 source map: upstream 26.6.0 release preparation

Upstream `0b44ad47` changes only `$ERSATZTV_SRC/CHANGELOG.md`: it places a
dated `26.6.0` heading after `Unreleased`, advances the latter's comparison
link from `v26.5.1` to `v26.6.0`, and adds the `26.6.0` comparison link. It
adds no C# code, packages, migrations, assets, UI, runtime configuration,
subtitles, or audio behavior. The release notes under the new heading are
earlier upstream entries, not new changes in this commit.

ErsatzRS keeps an independent version and release history. Its changelog
contract creates a dated section only when performing an ErsatzRS release;
this upstream-only preparation does not perform one. The row therefore needs
no Rust, UI, or changelog edit. Verdict: N/A (upstream release metadata).
The upstream diff, ErsatzRS changelog rules, and progress validation own this
row; UCU-04 window acceptance remains separate.

## Row 66 source map: SkiaSharp 4 API migration

Upstream `95fa937c` (PR #2943) upgrades SkiaSharp and its Linux native assets
from 3.119.4 to 4.150.0 in two .NET manifests. The only C# consumer edits are
`ChannelLogoGenerator` and `GraphicsEngine`: one destination-rectangle logo
draw and both point-positioned prepared-image draws now pass
`SKSamplingOptions.Default` to the SkiaSharp 4 overloads. The upstream diff
does not change placement, opacity, Z-index, frame selection, serialization,
UI, database state, or FFmpeg arguments. There is no validator or repository
path to port.

ErsatzRS has no SkiaSharp or native-assets package. Generated IPTV logos come
from `transport/iptv/logos.rs` as SVG, while Image and Motion graphics enter
the native FFmpeg overlay chain through `ersatzrs-ffmpeg/src/graphics/mod.rs`
and `filter/complex.rs` under ADR 0005. These paths do not call a Skia bitmap
draw overload. The explicit upstream default-sampling argument is therefore
an API migration in the external renderer, with no matching Rust dependency or
source operation. Existing native rendering evidence is retained; this row
adds no new cross-version pixel-equivalence claim. Verdict: N/A (upstream
SkiaSharp API migration). Source/diff review and progress validation own this
documentation-only row; UCU-04 retains its window acceptance gates.

## Row 65 source map: release notes on home and health

- ErsatzTV UI/pages: `Index.razor` serves `/` and `/system/health`. Below the
  ready-state health table it renders a `MudCard` with a GitHub changelog link
  in the header and sanitized Markdown release content in the body.
- Components/view models: `MarkdownView` parses Markdown with Markdig and
  sanitizes the generated HTML. There is no input form or validator.
- Handlers/services: `Index.OnParametersSetAsync` caches a successful release
  body in memory. The GitHub API client fetches the installed version's tag,
  stripping a version suffix, or the latest release for `develop`.
- Repository/query paths and side effects: no database write; a read-only
  GitHub request for uncached notes, with failure leaving notes
  empty. The card links to the full changelog.
- ErsatzRS targets: shared SSR release-notes component and renderer in
  `crates/ersatzrs-web/src/admin/`, used by `home.rs` and
  `pages/system/health.rs`; localized labels, focused tests and browser proof.
  The release identity comes from the packaged build ref rather than the
  workspace package version (which is always `0.1.0`). The exact version's
  section of the bundled `CHANGELOG.md` is authoritative; development builds
  show its latest dated release, as upstream does. A local `/changelog.md`
  route serves the full bundled file.
  An unauthenticated request to the ErsatzRS GitHub API and release page
  returned 404 on 2026-09-25, so a GitHub-only fetch would leave the card empty.
  The optional GitHub link remains for users with repository access.
- Hydration: both route wrappers remain SSR components. The health table's
  existing island stays in place; release notes have no interactive controls
  and require no island. A regular link works with JavaScript disabled.
- Visual source map: upstream centers an extra-large Mud container; the
  ready-state two-column health table precedes a card with `mt-6` vertical
  separation. The card header contains the GitHub link and the content uses
  body typography for rendered Markdown. There are no new buttons, forms,
  pagers or dialogs. ErsatzRS should use its page width and card tokens, keep
  the card below health content on `/system/health`, and show it after the
  running-state content on `/` because its existing home route is separate.
- Browser proof target: both routes show one card with rendered notes and the
  local full-changelog link, with readable layout at desktop and 320px widths;
  raw release HTML cannot execute in the page.

## Row 62 source map: Next subtitle selection and extraction

Upstream `0036fc5c` (PR #2938) adds subtitle codec/index entries to Next
`ProbeHint` from the selected media version. Its custom stream selector lets
Next select an unextracted embedded text track, while native transcoding
continues to require extraction outside HLS Direct. The playout subtitle
extraction handler also admits channels with a custom stream selector even
when the channel subtitle mode is None; the converter suppresses duplicate
selector diagnostics during Next projection. There is no UI or schema change.
The channel-save trigger remains gated on the general subtitle mode upstream;
playout builds and settings changes still dispatch extraction.

ErsatzRS already projects version-scoped subtitle probe hints in
`crates/ersatzrs-infra/src/next_engine/probe_hint.rs` and separates Next/native
custom selection in `crates/ersatzrs-infra/src/playback.rs`. The Next selector
and decoded subtitle tests own these existing contracts. The remaining query
gap is in `crates/ersatzrs-infra/src/subtitles.rs`: its playout extraction
candidate filter checks channel and schedule subtitle modes but omits custom
stream selectors. Add that upstream branch and prove the eligibility matrix,
then run focused extraction/Next checks and playback runtime evidence. ADR
0100's live and one-shot embedded-text limitations remain separate; this
upstream row does not remove them.

## Row 61 source map: date-based YAML scheduled playouts

Upstream `42937ac3` (PR #2935) adds `schedules[]` with a name, date range,
priority and playout instruction list to the Sequential YAML model and schema.
`AlternateScheduleSelector` chooses the highest-priority matching annual or
specific-year range at each instruction boundary, keeping definition order on
ties. `SequentialPlayoutBuilder` flattens sequences in each schedule, switches
to the active list, and advances the guide group at a switch. The context saves
per-list instruction positions and ambient watermark, graphics, filler and
roll state within a build; the anchor serializes active schedule and list
indices for Continue. The shuffle-sequence handler acts on the active list.
Invalid dates do not select a schedule. No Razor layout or database schema
changes occur in this commit.

ErsatzRS targets are `crates/ersatzrs-core/src/scheduling/yaml/` for parsing,
date selection, list state and anchor continuity;
`assets/sequential-schedule.schema.json` and the served copy under
`crates/ersatzrs-web/resources/` for editor/server validation;
`crates/ersatzrs-infra/src/repositories/scheduling.rs` and the existing
playout repository path for runtime input and persistence proof. The raw-YAML
editor needs no new controls, but its validation must accept schedules and
reject malformed entries. Existing inline-YAML storage remains the documented
ErsatzRS deviation. Focused core, schema, SQLite restart and runtime smoke
tests own the acceptance before moving the ledger and pin.

## Row 60 source map: upstream OpenAPI package build override

Upstream `c8bd9766` (PR #2936) changes only
`$ERSATZTV_SRC/ErsatzTV/ErsatzTV.csproj`: it adds a direct
`Microsoft.OpenApi` 2.7.5 package reference to override the transitive 2.0.0
version from `Microsoft.AspNetCore.OpenApi`. The adjacent project reference and
`Startup.cs` OpenAPI registration are unchanged. No C# handler, endpoint,
schema, media or UI behavior changes in this commit.

ErsatzRS has neither a `Microsoft.OpenApi`/NuGet dependency nor a matching
OpenAPI generation dependency in `Cargo.lock`; its Rust web host uses Axum.
There is no Rust package substitution or application change for this build
override. This verdict makes no claim about broader OpenAPI endpoint parity.
The row requires source, package-graph and progress validation only.

## Row 59 source map: remove already-played playout items

Upstream `2b7531ca` (PR #2934) changes the scripted builder and the on-demand
time shifter. Continuous scripted builds set `RemoveBefore` to the build start
minus four hours; on-demand scripted builds leave it unset because the time
shifter removes items whose finish is strictly before the saved checkpoint.
The upstream playout builder also documents this ownership split. The change
does not alter scheduling history, anchors, media selection, or schema.

ErsatzRS owns scripted build results in
`crates/ersatzrs-core/src/scheduling/scripted/mod.rs` and persists their trim
cutoff through `crates/ersatzrs-infra/src/repositories/playout.rs`. The existing
repository subtraction of the maximum mirror offset remains necessary for
delayed channels. `crates/ersatzrs-infra/src/maintenance.rs` loads the channel
mode before building and supplies it to reference data. The on-demand segmenter
time shifter in `crates/ersatzrs-infra/src/hls_worker.rs` removes played rows
inside its existing shift transaction before moving surviving rows, history,
anchors, and checkpoint. Focused scheduler and SQLite tests own the strict
boundary, channel isolation, and continuity checks; native smoke covers routed
playback. This row does not delete history or reset enumerators.

## Row 58 source map: retain stale HLS segments during append

Upstream `0b8c5146` (PR #2933) changes only
`ErsatzTV.Core/FFmpeg/HlsPlaylistFilter.cs` and its focused test. With no
`maxSegments` limit, trimming now removes old segments only when at least one
segment starts at or after `filterBefore`; otherwise it keeps the existing
playlist. A second guard returns the current generation when the input has no
segment, avoiding an empty-list minimum error. No UI, validation, database or
FFmpeg command changes appear in this commit.

ErsatzRS owns the equivalent parser, selection, renderer and append path in
`crates/ersatzrs-infra/src/hls_worker.rs` (`trim_playlist_contents`,
`select_retained_segments`, `render_trimmed_playlist` and
`trim_playlist_for_append`). The current no-limit selection returns an empty
list when every segment is before the cutoff, so rendering returns `None` and
the append path cannot preserve a playable playlist. Bounded-window selection
already keeps a non-empty tail. The no-limit path now retains all source
segments only when none meets the cutoff; recent-segment trimming is unchanged.
Red-then-green MPEG-TS and fMP4 tests verify sequence, map, generation and
referenced-file cleanup. On empty input Rust already returns `None` before
calling a minimum operation; append callers continue into the next FFmpeg
generation, so the upstream empty-list exception has no Rust equivalent.
Native smoke and TSDuck playback own the runtime/transport check.

## Row 57 source map: dependency refresh and SQLite security

Upstream `3756e7fb` (PR #2927) changes twelve C# project files and
`OpacityExpressionHelper.cs`. Its NCalc 6.2 adapter replaces the event-argument
type and indexed parameter evaluation while preserving the four fade helper
arguments and arithmetic. The package refresh also pins a fixed SQLitePCLRaw
native bundle to address GHSA-2m69-gcr7-jv3q. No UI, route or persisted schema
changes appear in this commit.

ErsatzRS renders opacity expressions through
`crates/ersatzrs-ffmpeg/src/filter/simple.rs`; its existing custom-helper
regression covers the unchanged fade arity and arithmetic. The other C# package
updates use NuGet-only surfaces without a matching Rust dependency. The NCalc
update does affect Classic saved filler expressions: an exact NCalcSync 6.2.0
probe with upstream's comparer and parameter callback reproduced 119 oracle
outcomes, finding changed IEEE-754 NaN comparisons and signed-zero remainder.
The Rust evaluator, oracle corpus, expression reference and ADR 0098 follow
those changes without altering its accepted deterministic culture or limits.

The SQLite path is a bundled `libsqlite3-sys` shared by SQLx and `xtask`'s
`rusqlite`. Before this row the lock resolved 0.30.1 with SQLite 3.46.0,
below the advisory's 3.50.2 fix. `rusqlite` 0.39.0 permits one shared bundled
`libsqlite3-sys` 0.37.0 with SQLite 3.51.3. An actual SQLx connection asserts
the linked version at runtime. Focused watermark/opacity and expression probes,
license/advisory checks, the native web smoke test, and progress checks own
the row's acceptance. No schema, UI, media-track or release-package contract
changes are implied.

## Row 56 source map: Intel container driver stack

Upstream `6b74da3c` (PR #2926) changes only the changelog, announcing an Intel
driver update in its externally built FFmpeg base image for newer Battlemage
devices such as B70. The pinned upstream Dockerfile consumes that image; it
contains no driver source pin itself. ErsatzRS uses Debian runtime images and
the separate managed FFmpeg executable package under ADR 0008. Before this row, the active root
Dockerfiles installed no Intel media driver, and the managed Linux FFmpeg package
bundled no driver shared libraries. Passing a render device alone therefore
does not deliver the announced userspace capability. This is a packaging gap,
not an upstream-only documentation change.

The owning targets are the active Docker runtime stages, compatible Intel
userspace dependencies, container installation guidance and existing hardware
capability/health probes. Row 56 is Port: matched checksum-pinned Intel sources
are built separately from Rust/WASM, stripped and copied with licenses into
Linux x64 runtime images. Minimal and alternate runtime loader checks, actual
managed FFmpeg CPU encode/decode and VAAPI loading, and ARM64/ARMv7 build/start
checks pass. UCU-04 owns the detailed evidence and remaining window gates.
Physical Battlemage transcoding is not claimed by these packaging checks. Kernel drivers and render-device permissions remain host-owned under
ADR 0008. No Rust playback or UI behavior change is implied by the source diff.

## Row 55 source map: undefined sidecar language and title isolation

Upstream `32e54828` (PR #2925) changes LocalSubtitlesProvider to parse only
text after the media filename stem, removes forced/SDH/CC flags from that
suffix, and uses `und` when no language remains. Its regression matrix adds
bare and forced-only sidecars and a video title containing Forced. There are
no UI, schema, API or maintenance changes. Rust's existing `sidecar_suffix`,
`sidecar_tokens` and `sidecar_language` already implement these outcomes;
`locate_external_subtitles` preserves full-path/filename modes and codec flags.
The metadata integration matrix verifies persisted suffix-only flags and `und`
across rescans, with the existing undefined-language playback selection test.
No production change or new deviation is needed for this row.

## Row 54 source map: local subtitle identity and repair

Upstream `7032cfc8` (PR #2924) separates embedded subtitle identity (stream
index) from sidecar identity (case-insensitive filename), deduplicates both,
updates sidecar paths independently of extraction state, and preserves cached
embedded extractions when metadata is unchanged. Maintenance recognizes all
metadata owners and removes sidecars with null/empty paths. SQLite and MySQL
data migrations clear affected local folder etags so the next scan repairs
collisions. Generated EF model files add no schema change.

The shared metadata writer now reconciles local filenames deterministically;
provider inputs retain their existing complete URL identity so equal filenames
on distinct URLs are not merged. The data migration follows the upstream four
local-video-owner etag predicates without deleting subtitle rows. The existing
maintenance dispatcher runs database-only subtitle cleanup hourly, in bounded
atomic batches, preserving all ten owner types and cached files. Its aggregate
health warning is localized by the existing health-message formatter. Scanner
upgrade, language selection and decoded-caption regressions own runtime proof.
The migration, cleanup, health, scanner upgrade and decoded-caption regressions
pass, together with scoped all-target Clippy, hydration lint and native playback
smoke. The baseline advances to this row; UCU-04 window acceptance remains open.

## Row 53 source map: image graphics EPG context

Upstream `dbdf3dff` (PR #2923) adds `epg_entries` to ImageGraphicsElement,
makes its context implement ITemplateDataContext, and includes Image in the
loader's EPG count discovery. Unreadable source YAML is now caught during that
discovery. GraphicsEngine consumes the renamed image context; no schema or UI
change occurs. Under ADR 0005, ErsatzRS persists imported graphics definitions
and renders from the database without rereading their YAML source. Its common
EPG query and image path templating already use the stored `epg_entries` field,
but ImageYaml omits that field and rejects such imports. The importer and its
persisted image-to-playback regression own the correction. The existing
database-authoritative behavior covers unavailable source YAML at playback.

## Row 52 source map: motion graphics compositing

Upstream `dca93717` (PR #2922) changes `MotionElement.cs` and its changelog.
The external Skia renderer stops allocating and clearing a canvas-sized
bitmap for every motion frame. It returns the decoded, scaled motion bitmap
with its calculated point for normal, held and pending frames. No UI, schema,
protocol, timing or asset change occurs.

ADR 0005 already renders ErsatzRS motion elements as native FFmpeg video inputs
with `overlay` placement and optional source-sized scaling. The filter uses
the overlay's own `w`/`h` and location expression against the output `W`/`H`;
there is no application-owned full-frame bitmap or per-frame canvas copy.
Loop, Disappear and Hold snapshots and decoded top-right motion compositing
confirm the existing path. Verdict: Covered under ADR 0005; no Rust command
or behavior change is needed. The separately refused Next general graphics
projection remains visible under ADR 0095 and is not closed by this row.

## Row 51 source map: Refit package update

Upstream `679fb172` (PR #2921) updates Refit, Refit.Newtonsoft.Json,
Refit.Xml and Refit.HttpClientFactory from NuGet version `10.1.6` to
`10.2.0` in two C# project manifests. No C# source, serialized contract,
runtime setting or test changes occur. ErsatzRS uses its Rust HTTP clients
and does not depend on Refit or these NuGet packages. The certificate
revocation workaround therefore has no Rust dependency or behavior to port.
The row is N/A; existing Rust provider transport and certificate behavior are
unchanged. The UCU-04 window gates still own broad playback and platform proof.

## Row 50 source map: Next probe hints and initial gaps

Upstream `387da2b8` (PR #2920) changes three files. The schedule exporter
starts gap insertion at the current time, including a leading gap before the
first programme, and increments the playout schema to `0.0.2`. The converter
supplies video and audio probe hints from the selected head version and a
finite PCM hint for generated silence. Next's wire DTO adds optional hints for
local, HTTP, RTSP and lavfi sources; its dynamic callback accepts a resolved
item carrying the same hints. No UI, schema migration or dependency changes.

ErsatzRS already supplies fresh typed hints for one-shot ordinary/retained
sources and generated silence. Scheduled local and media-server sources still
require persisted stream hints. The Rust source resolver, wire schema,
publication, gap builder and callback validation own this row. Embedded
subtitle hints must remain available to Next's subtitle selection; invalid or
incomplete stored metadata must retain normal probing rather than inventing a
stream. Still images need their container identity to remain recognized. Source
fingerprints and playback tests must cover metadata changes, leading gaps and
one-shot producer ownership. ADR 0095 owns the native host adaptation.

## Row 49 source map: dynamic fallback filler in Next

Upstream `294a2ff1` (PR #2919) inserts Dynamic placeholders only between
persisted programmes in `SyncNextPlayoutHandler`. `PlayoutItemConverter`
extracts the existing media/track/watermark conversion and adds a Dynamic
source. `/media/fallback` reads the injected channel/time headers, invokes
`DynamicPlayoutItemService`, and converts one ephemeral fallback item to JSON.
The native process handler delegates its existing fallback/path logic to that
same service. There are no UI, validator, schema or dependency changes.

Fallback precedence is active template decoration, playout decoration, channel
preset, then global preset. Decoration Disable suppresses inheritance. A
nonempty configured collection supplies a random item, limited to its media
duration and the next programme. Empty selected collections do not cascade to
another preset. Existing source resolution, audio/subtitle selection, fallback
watermark policy and mirror source/target time conversion must be retained.

Rust targets are the scheduling collection loader and playback decoration
queries, plus Next schedule publication, authenticated session callbacks and
existing local/provider/Remote Stream projection/preparation. Ephemeral filler
must not be written into the persisted programme timeline or reset enumerators.
Callbacks must reject expired sessions, changed gaps and mismatched channel or
time headers. The current persisted-item callback cannot directly represent an
ephemeral filler item; row acceptance requires completing that boundary,
including subtitle preparation, credits, source lifecycle and real gap-to-next
programme playback. ADR 0095 owns the existing authenticated callback adaptation.

## Row 48 source map: generated music-video credits in Next

Upstream `14ee8063` (PR #2917) adds a playout-item query and internal ASS
endpoint, emits a generated subtitle URL for MusicVideo items, sends the
playout in-point to the renderer, and lets Next use that URL as an HTTP subtitle
source. The existing native credits generator remains the rendering authority.
Rust blocked every Next channel with credits enabled in host and callback
projection, although native playback already rendered the templates. The row
uses demand-time rendering and a binding-scoped URL ticket because libass does
not forward Next's source headers; ADR 0095 records the transport adaptation.
The route checks an active channel session and persisted item/media/in/out
binding. Explicit subtitle-off and pinned selections stay in control; no
publication-time temporary file is needed. Focused tests and decoded Next
Burn/Convert runtime prove in-point timing and selected alternate audio.
Generated credits remain distinct from unsupported general graphics elements.

## Row 47 source map: MPEG-TS script lookup

Upstream `aa639375` (PR #2916) changes three locations: FFmpeg settings and
troubleshooting default the configured script ID to lowercase `default`, and
`FFmpegLibraryProcessService.WrapSegmenter` selects a discovered script with
`StringComparison.OrdinalIgnoreCase`. The Rust settings/default seed already
use lowercase `default`; troubleshooting still falls back to `Default`.
`transport::iptv::stream::find_mpegts_script_in` compares the configured ID to
the folder name exactly, so a saved ID with different case misses the script.
Port the invariant, one-character-at-a-time case comparison for script IDs,
align the troubleshooting fallback, and prove mixed-case folder discovery,
accent distinction and quiet fallback for the built-in ID. No UI structure,
schema, generated FFmpeg argv or health policy changes are in this row.

## Row 46 source map: SkiaSharp and Scriban patch versions

Upstream `99a1ecee` (PR #2915) changes two project manifests only:
SkiaSharp and its Linux native assets move from 3.119.2 to 3.119.4;
Scriban.Signed moves from 7.2.0 to 7.2.1. Image/logo/graphics and template
consumers are unchanged. Cargo manifests/lock contain neither dependency.
ADRs 0005, 0058 and 0076 retain the native FFmpeg graphics and minijinja template
boundaries. No application, schema, asset or runtime-artifact change follows
from these .NET patch versions. Verdict: N/A (upstream tooling); this adds no
new image-format or graphics acceptance claim.

## Row 45 source map: Next in/out-point synchronization

Upstream `b1c7702a` (PR #2912) changes only `SyncNextPlayoutHandler`: copy
positive in-points and clipped out-points onto the source, then preserve that
complete source when splitting video from generated silence. No UI, schema,
validation, dependency or asset changes occur. Rust `selection::apply_item_seek`
already projects millisecond bounds before cloning concrete video/audio/subtitle
sources; the no-audio branch retains the original video source and transport
options. Next's `input_timing` defaults an omitted in-point to zero and an omitted
out-point to in-point plus scheduled duration. Rust's explicit full-interval
values therefore retain that timing for valid persisted intervals.
Verify zero/fractional/open-ended bounds, existing no-audio HTTP projection,
and existing decoded Burn/Convert seek/audio/off cases. Live and producer-owned
seek remain governed by the accepted row-39 contract. No new production change
is proposed; row-44 routed smoke remains applicable to unchanged playback code.

## Row 44 source map: explicitly scheduled images

Upstream `577983af` (PR #2911) changes only `SyncNextPlayoutHandler`: include
Image versions/files/streams/metadata and allow Image in the export loop.
The existing local source resolver and no-audio silence path then apply.
There are no UI, validators, schema, graphics-overlay or asset changes.
Rust already includes `ImageId` in `schedule::bind_media` and projects explicit
video plus finite silence in `selection`; Next identifies still images from
its own probe and holds them for the scheduled duration. Verify this existing
path through image-only persisted bindings, projection and real host playback
before assigning Covered. Acceptance needs decoded image colour after multiple
seconds, encoded silent audio, original schedule timing, and awaited cleanup.
This row does not introduce generated images, credits or graphics elements.
Existing Next availability health and native image behavior are unchanged.

## Row 43 source and hydration map: Next version

Upstream `9c5d86a1` (PR #2910) adds `GetNextVersion` and its handler,
extracts the executable locator into `NextChannelHandlerBase`, caches the
startup result in `NextVersion`, and displays it in `MainLayout.razor`.
The existing Rust `next_engine::host::channel_binary` already shares the
locator with playback. Probe that executable once with `--version`, with
bounded output, a deadline and managed process cleanup; cache the optional
result in host state. Missing or failed probes remain nonfatal.

- Route/page: every admin route using the navigation drawer.
- Visual source: upstream drawer footer, a small Next version label and value;
  retain the Rust drawer spacing and typography. No assets or validators change.
- SSR wrapper: `AppShell` / `NavDrawer`; static version footer component.
- Islands: existing drawer controls and badges unchanged.
- Data boundary: startup-cached `AppState` value, no per-request process.
- Server functions/API endpoints: none added.
- Fallback/no-JS: identical localized label/value, localized unavailable text.
- Shared primitives touched: navigation footer CSS only.
- Browser proof target: available/unavailable version, desktop/mobile drawer,
  no-JS rendering, existing drawer interaction and clean hydration diagnostics.
- Health: optional version discovery does not change engine availability or
  profile validation; existing Next runtime health checks retain that ownership.

## Row 42 source map: ImageSharp dependency constraint

Upstream `822c7547` (PR #2909) changes one `PackageReference` in
`ErsatzTV.Infrastructure.csproj`: ImageSharp `3.1.12` becomes `[3.1.12, 4)`.
This retains the lower bound and excludes the next major version. Its
`ImageElementBase` consumer, UI, validators, image assets, handlers and schema
are unchanged. Rust Cargo manifests/lockfile do not include ImageSharp;
ADR 0005 routes image graphics through native FFmpeg inputs and overlays.
Verdict: N/A (upstream tooling). This row neither changes that accepted
architecture nor claims new image-format or Next graphics support.

## Row 41 source map: automatic Next error reports

Upstream `cc9880c2` (PR #2907) always supplies `ReportsFolder` in
`StartFFmpegNextSessionHandler` and removes the unused `SaveReports` wire field.
Three additional project-file changes only update .NET dependency versions.
Next owns temporary report cleanup and failure dossier promotion; ErsatzRS
must preserve overlays and normal/preview routing. The Rust change belongs in
`next_engine::host::configuration` and its configuration/runtime report tests.
There are no schema, route, UI, or selection changes. Native report opt-in and
its disk-use health warning retain their existing meaning. Required evidence:
setting-independent configuration, real Next failure dossiers with reports off,
normal playback, scoped review/tests/Clippy, and documentation of retained data.
These checks pass; UCU-04 owns the evidence and the corrupt-input fixture finding.

## Row 40 source map: .NET dependency refresh

Upstream `2c02561f` (PR #2900) changes only `PackageReference` versions in
11 `.csproj` files. Microsoft extensions/AspNetCore, Hardware.Info, Jint,
BlazorSortable, Scalar, NUnit, Testably and coverlet are upstream dependencies.
There are no handlers, UI markup, validators, schemas, assets or runtime binary
changes. Cargo manifests and the lockfile contain none of these packages.

The Hardware.Info major version change leaves its CPU/video-controller call
sites unchanged. ErsatzRS troubleshooting uses `available_parallelism` for its
CPU summary and FFmpeg capability reports; this row does not claim improved
hardware inventory or platform acceptance. JavaScript uses the accepted
QuickJS boundary in ADR 0096. No Rust dependency update follows from these
.NET version numbers. Verdict: N/A (upstream tooling); UCU-04 owns review evidence.

## Row 39 source map: Next Remote Streams and reports

Upstream `7c4a5e3a` (PR #2899) changes four files: the Next playout exporter,
session configuration builder and their wire models. It adds RemoteStream
versions/files/subtitles, HTTP live/reconnect/keep-alive, RTSP, raw command argv,
and opt-in FFmpeg reports. No test, migration, UI or changelog file changes.

- Outcome: scheduled Remote Streams play through Next while preserving selected
  audio/subtitles, finite/live timing, source ownership and existing add-on
  preparation contracts. Enable the persisted report setting for Next too.
- Accepted adaptation: supported remote subtitle/lifecycle paths, substantive
  review and Next/native smoke pass. ADR 0100 owns the accepted embedded text limitations.
  Direct sources and prepared artifacts use authenticated callbacks; retained
  callbacks reuse native preparation and pass installed-producer playback proof.
  Retained runtime proof now applies requested source intervals and verifies
  mapped Burn/Convert captions across the cut. Ordinary producers now pass finite/live callback playback
  and finite/live absolute and relative external subtitle Burn/Convert proof with one producer and
  source-owned seek; UCU-04 owns the detailed evidence and remaining acceptance.
- Accepted adaptation: ADR 0066 removed raw `script:` playback and supersedes
  ADR 0057 on that point. Preserve installed add-on commands and their structured
  argv, environment, working directory, tool leases and cancellation ownership.
  Do not recreate upstream's raw command parser or flatten add-ons into Next's
  bare script source. No reversal of this accepted boundary is proposed.
- Reuse: `resolve_published_remote_stream` takes precedence over legacy direct
  resolution. Preserve prepared artifacts and frozen option envelopes. Retained
  intervals use `prepare_playback_cancellable`, fresh stream metadata and the
  producer-applied seek; do not seek the resulting bytes again.
- Transport integration: reuse authenticated demand-time item callbacks
  to keep credentials out of published JSON. The existing prepared-input
  transfer now supports an owned bounded byte sink, preserving prefix replay and
  process cleanup; expose unchanged bytes through scoped Next HTTP transport.
  Bind transfer ownership to the particular normal/preview session, interval
  and publication, including cancellation on body drop and awaited shutdown.
  This is not permission to add a generic broker or another transcoder.
- Session binding: shared channel publications use the Dynamic header environment
  templates supported by official Next f2a981b. A reserved child-only instance
  UUID identifies normal/preview independently without changing the
  channel number or overlay lookup. Validate that header against the exact
  active context; never fall back to a channel-only owner. Register pending
  preparation before external work, close admission before shutdown, then cancel
  and await all preparation/transfers before capacity release. Header expansion
  has official-runtime proof; independent contexts and stale callbacks have
  focused regressions. Bounded pending/active registration and awaited shutdown
  are implemented and regression-tested. Production-router runtime proof now
  verifies independent normal/preview producers, stopping preview while normal
  continues, and final process/output/capacity cleanup.
- Direct HTTP/RTSP resolution must retain finite/live semantics; RTSP is always
  live. Add-on preparation must occur outside scheduler locks. Unsupported or
  stale exact selections must fail visibly, not silently choose another track.
- Official Next f2a981b drops multiplexed live embedded text in Burn mode;
  Convert rejects RTSP and waits for HTTP EOF. Its global live flag also affects
  sidecar timing. Row 39 upstream forwarding does not resolve those limitations.
  The maintainer accepted ADR 0100 for live embedded text and ordinary one-shot
  embedded text without an independent source-bound caption artifact: keep the
  official engine and refuse the unavailable selection explicitly. Image
  subtitles, external sidecar timing and HTTP lifecycle remain required.
  UCU-04 evidence and visible parity debt track the exception; wire selection
  tests must not be represented as subtitle playback acceptance.
- Standalone live captions are rebased to the callback's source clock before
  Next clears live input seeks. Reuse the bounded Burn artifact cache, bind the
  output to publication, content and millisecond join, then revalidate publication
  and session identity. The adapter reads only independent finite sidecars;
  whole-source extraction of live embedded text remains refused.
- Reports: official Next f2a981b enables diagnostics by `reports_folder` presence;
  it has no operative `save_reports` field. Map the existing persisted opt-in to
  the application-data `ffmpeg-reports` directory, preserving overlay priority.
  Row 41 supersedes the opt-in rule with automatic Next error diagnostics.
- Acceptance evidence: source/protocol/timing regressions, finite/live HTTP,
  RTSP and managed-producer runtime proof, selected tracks and subtitles,
  disconnect/cancellation cleanup, reports, focused review and HTTP smoke.
  The one-shot non-seekable NUT prerequisite passes with official Next and fresh
  probe_hint metadata (UCU-04 evidence). The prepared-artifact callback now passes
  five real Next text/audio/seek scenarios. Retained production callbacks are
  connected and fresh-index/no-second-seek regressions pass. The installed
  retained producer now passes callback-to-decoder proof with one producer and
  selected audio; the expanded fixture also verifies actual requested cuts and
  mapped Burn/Convert captions. Ordinary finite/live external Burn/Convert and
  direct finite/live HTTP/RTSP sidecar playback now pass. Production routing
  proves finite producer authentication, single claims, independent normal/preview
  lifetimes and stop cleanup. A real HTTP body disconnect reaps its producer
  before session stop; claiming the consumed transfer again returns Gone.
  Final scoped review/checks and Next Burn/Convert plus native lifecycle smoke
  pass. Row 39 is Adapt under ADRs 0066/0100; full UCU-04 window gates and
  supported-platform acceptance remain open. The authorized local checkpoint
  preceded resumed catch-up on 2026-09-24.

## Row 36 source map: Next channel configuration overlays

Upstream `64296597` (PR #2891) adds operator overlay discovery in
`NextSessionWorker`, the application-data directory/bootstrap, nullable JSON
omission, and profile deinterlace/per-filter tonemap mapping. It changes no
schema, admin editor, validator or upstream tests.

- Outcome: apply generated profile, optional `default.json`, then optional
  `{channel_number}.json` from `next/channel-config-overlays`, for normal and
  preview Next sessions. Native-engine channels are unaffected.
- Mapping: `file_system_layout`, web bootstrap/state, and the infrastructure
  Next host/process adapter. Preserve the real channel number rather than the
  preview capacity key. Keep operator files outside cleared output folders.
- Existing coverage: all six tonemap mappings and deinterlacing are already
  represented in `next_engine/config.rs` and its focused tests.
- Merge ownership: pass JSON file arguments directly to Next; it resolves
  relative paths per file and merges in order. Do not duplicate its schema or
  merge algorithm. Omit unset generated optional values, matching upstream.
- Acceptance: argv order/optional files/decimal channels/path safety/deadline
  tests, actual process stdin/exit cleanup, official Next merge and invalid-JSON
  proof, decoded playback with selected audio/subtitles, truthful existing
  runtime health, focused infra/web checks and substantive review.
- Exclusions: no new admin controls, live file watcher, automatic restart,
  runtime dependency or graphics-opacity workaround. Window acceptance and
  exact-SHA CI remain owned by UCU-04; row acceptance is recorded in its evidence.

## Row 32 source map: orphan metadata maintenance

Upstream `a1036430` changes `DeleteOrphanedArtwork` and its handler/repository,
`MaintenanceController`, hourly `SchedulerService`, four Emby/Jellyfin movie/show
actor writers, Next playout logo loading, and the changelog. There are no UI,
validators, migrations or upstream tests. Read the complete diff and surrounding
handler, repository, controller and scheduler before implementation.

- Outcome: prevent detached actor growth and reclaim existing orphan database
  rows without deleting artwork files or referenced actor/channel images.
- Native producer mapping: provider movie/show actor replacement physically
  deletes owner-scoped rows in its transaction; focused repeat/shrink tests
  verify this existing behavior. Next's shared selector already resolves logo
  artwork from SQL; explicit projection coverage verifies the selected path.
- Cleanup mapping: a dedicated infrastructure database helper deletes actors
  with all ten metadata owners null, then metadata-orphan artwork without an
  actor reference. Keep upstream 5,000-row batches, 100-ID deletion chunks and
  per-kind limit checks between batches; nonpositive limits do nothing.
  Transactional batches retain referenced records under concurrent writes.
- Scheduler mapping: reuse persisted maintenance-loop cadence, startup plus
  hourly cleanup with default limit 100,000 and aggregate counts in reports.
- API mapping: `/api/maintenance/clean_artwork?limit=...` admits bounded
  background work through the application boundary; preserve HTTP 200/default
  100,000 and signed integer binding. Host cancellation owns the worker.
- Health mapping: an aggregate-only orphan check reuses the deletion predicates,
  with localized hourly/manual-cleanup guidance in all nine cultures. Existing
  actor-artwork and orphan-artwork indexes support the reference checks.
- Exclusions: filesystem cache deletion and VACUUM are not performed by this
  upstream handler; do not introduce either or change live media/track data.
- Acceptance: database survivor/batch/idempotency/rollback tests, real provider
  update regressions, queued route behavior and logo projection proof. UCU-03
  owns evidence, window gates and the first consolidated exact-SHA CI.
- Intentional deviations: none proposed. Existing SQLite/application boundaries
  apply; missing behavior stays visible until the row passes review and checks.

## One-commit procedure

The maintainer's 2026-09-20 decision makes this a Codex-only workflow:
Codex performs the substantive review directly; no Claude or replacement
review agents are required. Earlier review attempts remain historical evidence.
On 2026-09-22 the maintainer authorized the prepared first local commit and
subsequent individually reviewed local UCU steps. This satisfies step 8 local
commit authorization; push, CI dispatch and release tagging remain separate.
The maintainer amended the CI cadence on 2026-09-22: skip individual CI runs
for UCU-01 and UCU-02. Their local gates and ordered row reviews still apply;
continue to the next window once those are complete. Run the first consolidated
exact-SHA CI after UCU-03 smoke; that CI checkpoint is explicitly authorized.
This supersedes the separate-CI requirement for these two windows only.

The maintainer also authorizes pushing to remote `main` after completion of
each milestone from UCU-03 through UCU-09 inclusive (UCU-05 explicitly confirmed).
This is milestone-boundary push authorization, not permission to push each
intermediate row. The subsequent explicit goal also authorizes running and
finishing green exact-SHA CI at every boundary from UCU-03 onward. Complete the
local window gates, push its candidate to remote main, verify terminal green CI,
and only then claim the milestone complete. Release tagging/publication remains
a separate action; a push alone is not CI proof.

Before selecting or processing row 1, AMM-N-03 must be accepted and ucu-00
closed against its recorded criteria. A documentation checkpoint does not
satisfy that prerequisite or authorize a later implementation commit.

1. Print the pin, last processed ledger row (or initial pin at row 0), and
   `$ERSATZTV_SRC` HEAD. Stop on disagreement; do not repair by guessing.
2. Fetch `origin/main` without moving upstream HEAD. Verify the previous target
   is an ancestor of the fetched tip and the existing ledger remains an exact
   prefix of the generated log; stop on divergence or fetch failure. Append
   every new commit with empty verdict/owner/commit fields, refresh the target
   and backlog count in every record that states them — this plan, the pin
   document's Catch-up mode section, and the catch-up parity record's
   `etv_source` and summary — and update the open ucu-09 tail milestone. Other
   documents describe the bootstrap as of a stated date and must not repeat the
   live target or count. If that milestone is already Complete, reopen it with the
   new scope while preserving earlier evidence; do not claim current catch-up
   completion from older acceptance. Select exactly the first pending row. Read its stat, full diff, tests,
   CHANGELOG hunk and surrounding files with `git show <sha>:<path>`; leave
   upstream HEAD at the pin until landing.
3. Map behavior through [PORTING.md](../PORTING.md) and the owning parity
   record's `etv_source`/`rs_target`. Record in notes the problem solved,
   whether it exists in Rust (code/test evidence), the benefit of porting,
   and any conflict with an accepted ADR. Provisional triage is not evidence.
4. Choose exactly one verdict: `Port`, `Adapt (ADR nnnn)`, `Covered`,
   `N/A (ADR nnnn)`, `N/A (upstream tooling)`, or `Superseded by <sha>`.
   Supersession applies only to consecutive fixes inside the same window;
   implement the final shape at the cluster's last row. A new ADR, Deferred
   verdict or acceptance change requires a maintainer decision.
5. For Port/Adapt use [parity-slice](../skills/parity-slice.md), port upstream
   tests at the narrowest level, snapshot generated FFmpeg/YAML output, use
   sqlx SQLite migrations, preserve subtitle/audio handling, update health,
   localize UI in every culture, credit the upstream PR in the changelog and
   update the owning parity record. Accepted ErsatzRS ADRs take precedence.
6. Use `CARGO_BUILD_JOBS=8` and serial builds: `cargo xtask affected-checks`,
   focused `./run.sh test -p <crate> <filter>` and scoped Clippy. Add smoke for
   playback changes, relevant Playwright and hydration lint for browser changes,
   and `cargo xtask progress check`. Do not run a full gate for an ordinary row.
7. Codex reviews the full diff and affected behavior directly. Record concrete
   findings with failure path, impact and smallest correction; fix actionable
   serious defects and review the corrections. Record remaining limitations
   without claiming unproven acceptance.
8. Only with maintainer commit authorization, land the change, filled row and
   updated pin/subject/URL together; use Conventional Commits and body
   `Upstream: ErsatzTV <sha> (#PR)` (omit PR when absent). To avoid a self-hash,
   the landing row may identify its ErsatzRS commit by unique commit subject;
   resolve it with `git log --grep` when reading the ledger.
   Then `git -C "$ERSATZTV_SRC" checkout <sha>`. Stop after one row; no batching
   during this task, no push, CI dispatch or release tagging.
9. At a milestone boundary, run its acceptance gates with the CI cadence
   amendment above: UCU-01/UCU-02 close locally; the first CI follows UCU-03
   smoke. Push and exact-SHA CI are authorized for UCU-03 through UCU-09.
   After local window acceptance, push the candidate, run/monitor CI and resolve
   failures until that exact candidate is green before marking Complete. Record
   evidence and a progress session; never claim skipped or queued CI as proof.
10. Report upstream SHA/subject, verdict and rationale, changed files, evidence,
    old/new pin, row count, next commit and remaining blocker.

## Row 63 source map: deleted manual collections in search

- ErsatzTV UI, view models, validators: unchanged by PR #2939; the existing
  manual-collection delete action supplies the collection id.
- ErsatzTV handler: `ErsatzTV.Application/MediaCollections/Commands/DeleteCollectionHandler.cs`
  loads the collection's media item ids before deletion, deletes the collection,
  rebuilds those items in Lucene, commits the index, and refreshes search targets.
- ErsatzTV repository/query path: `IMediaCollectionRepository.GetItems` supplies
  the affected ids; `ISearchIndex.RebuildItems` removes the deleted membership
  from persisted search documents.
- ErsatzRS targets: `ersatzrs-app::commands::delete_collection` calls
  `SqliteCollectionRepository::delete_admin`; both scheduling search and admin
  `/search` use `SqliteSchedulingContentRepository::search_documents_page`.
  It loads documents from SQLite for each query, and `collection_values` joins
  `CollectionItem` to the extant `Collection` rows at query time.
- Side effect and deviation: ADR 0009 chooses SQL-backed search. SQLite
  foreign-key cascade removes collection membership on delete; there is no
  persisted Lucene document to rebuild and no intentional change to the
  user-visible result. A focused test verifies immediate search invalidation
  through the real delete and query repositories. No schema, UI, or translation
  change is needed.

## Row 64 source map: Plex subtitle titles

- ErsatzTV UI, view models, validators: unchanged. The existing media stream
  and subtitle metadata surfaces consume persisted stream titles.
- ErsatzTV service: `ErsatzTV.Infrastructure/Plex/PlexServerApiClient.cs`
  now copies `PlexStreamResponse.Title` into embedded and external subtitle
  `MediaStream` projections; audio titles were already copied.
- ErsatzTV repository/side effect: Plex scans persist projected `MediaStream`
  rows and derive metadata-backed `Subtitle` rows from them. The same commit
  updates .NET package references in seven project manifests.
- ErsatzRS targets: `crates/ersatzrs-infra/src/plex.rs` already deserializes
  Plex stream `title` and projects it for all stream kinds. `replace_streams`
  persists `MediaStream.Title`; `metadata::update_metadata_subtitles` copies
  that value to movie and episode `Subtitle.Title`, including updates.
- Intentional deviations: none for subtitle-title behavior. ADR 0098 retains a
  deterministic NCalcSync 6.2 saved-expression profile while this upstream row
  raises NCalcSync to 6.4; ADRs 0009 and 0096 own the existing SQL-search and
  QuickJS runtime choices behind its Lucene/Jint package updates. Movie and
  episode provider-sync fixtures now assert embedded/external stream and
  metadata subtitle titles.
- Dependency sources: the [NCalc changelog](https://github.com/ncalc/ncalc/blob/master/CHANGELOG.md)
  records 6.3.x membership coercion changes; the [Jint 4.11 release](https://github.com/sebastienros/jint/releases/tag/v4.11.0)
  documents its function-source retention default; [Refit 13 breaking changes](https://github.com/reactiveui/refit/blob/main/docs/breaking-changes.md)
  describe XML/JSON hardening; and the [Lucene beta18 status](https://github.com/apache/lucenenet/discussions/1401)
  identifies its vulnerability fixes. These upstream package changes do not
  alter the accepted Rust alternatives or introduce a matching Cargo upgrade.

## Row 20 source map: media-server inputs

Upstream `bb493670` changes `Application/Playouts/Commands/SyncNextPlayoutHandler.cs`
and `Core/Next/Playout.cs`; it has no UI, migration, test or changelog changes.
The handler exports media-server items, prefers an existing local file after
provider-specific path replacement, otherwise uses internal HTTP routes for
Plex files and Jellyfin/Emby movies or episodes. Missing audio produces a lavfi
silence track. Wire-model renames consolidate shared and per-track sources.

- Upstream services: `Core/Extensions/MediaItemExtensions.cs::GetLocalPath` and
  `Core/{Plex,Jellyfin,Emby}/*PathReplacementService.cs` select replacements for
  the item's library/source. `Controllers/InternalController.cs` resolves the
  internal media URLs through provider connection queries and redirects to the
  server. Plex uses the selected media file's key, not its filesystem path.
- ErsatzRS persistence: `next_engine/schedule.rs` selects one ordered version
  and file, respecting exact version pins. Retain the selected file ID with its
  version/path so HTTP resolution cannot select a different Plex file by path.
  `repositories/media_source.rs` already exposes per-source replacements and
  connection records. Provider item/file tables retain remote identities.
- ErsatzRS transport: `next_engine/protocol.rs::Source::Http` already models
  URI, headers, timeout, reconnect and seek options. `selection.rs` preserves
  complete sources for selected tracks and inserts finite silence without
  reconstructing the video source. The upstream no-audio branch copies only
  source type/path; do not lose the HTTP URI when applying that behavior.
- Integration: `next_engine/source.rs` resolves provider-scoped local paths;
  remote publication uses a process-capability-protected route in the existing
  web host. `next_engine/media.rs` validates the exact item/version/file binding,
  loads current provider credentials at media-open time, and streams HTTP through
  the web route with byte-range status and headers retained. Published Next JSON
  contains the internal capability rather than provider credentials. Local
  libraries retain the native replacement list. Scoped credentials take precedence;
  legacy global fallback is restricted to one configured server of that kind.
  `next_engine/connections.rs` supplies bounded identity-verified Plex selection
  and transactional activation outside schedule build locks. The media-open
  integration has type-check, focused credential/range/binding and web response
  proof, decoded Next playback including Plex failover, bounded redirect review
  and real-host HTTP/preview smoke. UCU-02 owns the row acceptance evidence.
- Side effects and evidence: keep complete atomic schedule publication,
  mirror timing, exact stream pins, redacted errors, session ownership and
  cancellation. Test overlapping paths across servers, selected file/version
  identity, local preference, HTTP fallback for all three providers, missing
  audio, seek and retained subtitle/audio sources, then decoded Next playback.
  Remote Stream/add-on lifetimes and later graphics rows are separate scope.
- Intentional deviations: the optional engine and atomic publication remain
  governed by ADR 0095. Row 20 is Port; no new deviation or Next exemption is
  introduced. Full-window and platform acceptance remain owned by UCU-02.

## Row 21 source map: audio selection during Next publication

Upstream `e6496bbc` (#2866) changes six files: the Next sync handler, default
stream-selector interface/implementation, native caller, selector tests and
scanner fake. It calls custom YAML selection before the default selector while
building Next items, retaining existing indexes and item/channel preferences.
The new logging flag suppresses routine selection diagnostics during publication.
There is no schema, UI, changelog or FFmpeg argument-builder change.

The default selector also invokes movie/episode JavaScript callbacks. Existing
Rust YAML/language selection alone therefore cannot cover this row. The source
map includes `FFmpegStreamSelector`, `ScriptEngine`, `StreamSelectorRepository`
and both upstream resource scripts. The new `infra/audio_script.rs` adapter
preserves callback metadata and uses the accepted ADR 0096 runtime constraints.
Native HLS/MPEG-TS, troubleshooting and Next call it only after stronger audio
choices, with ordinary selection as fallback. Native transport remaps selected
indexes by identity when an interval source changes its stream layout.
Next's existing explicit track source, seek and subtitle projection remain intact.

The row is an adaptation under ADR 0096; unsupported CLR interop and the
statement-counter replacement are accepted decisions. UCU-02 owns local proof;
full native execution on all six platforms remains an explicit program gate.

## Row 22 source map: Classic fallback intervals

Upstream `7e38297d` (#2867) changes `PlayoutModeSchedulerBase` and five scheduler
test files. Gap fallback and padding fallback must emit as many real media
intervals as needed, clamp the last interval, advance the filler enumerator for
each interval and preserve chapter offsets, parent identity and chapter title.
One, Multiple, Duration and Flood share this behavior. No UI, schema, FFmpeg
builder, changelog or source-transport changes occur upstream.

Rust targets are `core/scheduling/classic` (gap/pad generation and continuation)
and the scheduling repository (chapter-as-media filler hydration). Existing
Rust gap fallback stretches one item and clears its out-point; padding omits
fallback, and filler hydration ignores the persisted chapter option. These are
required parts of the row, not covered behavior. Tests must prove exact rows,
clipping, watermark/guide metadata, chapter boundaries and continued filler order.
Existing Refresh/Reset safety and audio/subtitle identity remain authoritative.
The first checkpoint implements real clip intervals for gaps and post-roll pad,
SQL chapter-as-media hydration, source offsets and serialized filler positions.
The Duration overflow path must retain the real cursor so tail fallback sees
its unfilled interval. A second checkpoint supplies source chapter boundaries,
places mid-roll before the following chapter and post-roll after the final one,
and rounds padding on wall-clock boundaries. Pinned source versions and filler
identities now survive Continue/Refresh and membership changes. ADR 0097 accepts
retaining primary content when padding selects no chapter points. ADR 0098 accepts
deterministic ICU-root comparison, invariant numbers/date formats and bounded
expression evaluation with atomic failure. The evaluator and focused SQL tests
implement those contracts; native six-platform acceptance remains explicit debt.
Final runtime verification also covers the corrected 100-nanosecond source-offset
persistence. See the owning classic-scheduling debt and UCU-02 evidence.

## Row 23 source map: embedded image subtitles for Next

Upstream `57f7dfa5` (#2868) changes Next playout synchronization, its subtitle
track DTO, and the shared selector logging flag plus two call-site test updates.
No UI, validator, migration or changelog changes occur in the upstream diff.
The handler loads movie/episode/other-video subtitle metadata, limits candidates
to local embedded image subtitles, applies item/channel language and mode,
retains custom selector precedence, and fills the subtitle index only when not
already assigned. Native playback keeps selection logs; Next builds suppress them.

Rust targets are `infra/next_engine/{selection,protocol,schedule}` and the shared
playback subtitle loaders/selectors. Existing projection already supports an
embedded image track, exact pins, item/channel/global language precedence and
explicit subtitles-off, plus external text/provider sources verified in earlier
rows. Preserve those capabilities; an upstream TODO is not a reason to remove
working native/Next behavior. No Rust per-choice logging exists to suppress.

Review found an actual source-identity gap: automatic/custom candidates aggregate
metadata for the entire item without validating embedded candidates against the
resolved media version. Candidates are now scoped before selection so a stale index or
conflicting old-version row cannot hide a valid track. The filter reuses pinned-stream
metadata matching rules and retains extracted/external behavior. This preserves the
existing selected-version invariant; no new architecture decision is proposed.
Focused projection regressions and actual generated bitmap-subtitle Next
playback proof pass, together with other Next consumers and HTTP/preview smoke.
UCU-02 owns evidence and remaining platform gates.

## Row 24 source map: Next text subtitle conversion

Upstream `1b5d6af7` (#2869) adds `NextEngineTextSubtitleMode` (Burn=0,
Convert=1) to Channel, create/update commands, mappings, editor and SQLite
schema. `SyncNextPlayoutHandler` projects embedded text indices and local
external text sources; `FFmpegStreamSelector` bypasses native embedded-enable
and extraction restrictions for Next. `StartFFmpegNextSessionHandler` maps
`normalization.subtitle.mode` and advertises the subtitle HLS rendition.
`ChannelPreviewDialog` adds a captions control. No new upstream validator is
introduced. Generated EF models and .NET-only dependency updates are reviewed
separately; SQLite remains the supported persistence backend.
Both new EF Designer method bodies equal their corresponding snapshots after
whitespace normalization; snapshot changes add only the new integer property.
The .NET package changes are Microsoft Extensions/ASP.NET 10.0.7, Test SDK
18.5.1, DependencyModel 10.0.7, Elasticsearch 9.3.6, System.CommandLine 2.0.7,
YamlDotNet 17.1.0, BlazorSortable 6.0.1, MudBlazor 9.4.0 and Scalar 2.14.6.
They do not change Rust dependencies. Config DTO additions for optional
`preferred_filters` and `deinterlace` have no producer in this commit;
no active setting is discarded. Remaining selector changes gate logging only;
Rust Next selection already omits per-choice logs. No upstream tests or
changelog hunk change in this commit.

Rust targets: domain/core/app channel contracts, infra channel repository and
an additive migration; Next config/selection and shared playback policy;
web channel forms/API, HLS master/session transport and channel preview.
Preserve resolved-version filtering, exact pins, subtitles-off, external/provider
text support, native playback policy and channel-edit invalidation. Required
evidence includes persistence/defaults, burn/convert configuration and selection,
real Next text conversion and image fallback, authenticated subtitle playlists
and segments, and hydrated/no-JS channel editing and preview captions.

Visual source map: `ChannelEditor.razor` inserts a Next-only responsive
`MudStack` label/select row between Streaming Engine and Streaming Mode, using
the existing `form-field-stack gap-md-8 mb-5` geometry. Options are Burn and
Convert If Possible; new channels default to Burn. Reuse the Rust editor's
form-row/select primitives and existing responsive CSS. Preview retains the
video/control geometry with captions available alongside playback controls.
No reference screenshot is available; browser comparison uses this Razor
contract, desktop/mobile overflow checks and actual captions interaction.

Hydration source map: `/channels/new` and `/channels/:id/edit` use the existing
SSR editor and ordinary POST form, channel server DTO and `/admin-api/channels`
save boundary. Keep the subtitle-mode row mounted and reuse the streaming-engine
dependency script for visibility. Localize every supported language. The preview
player remains inside its existing island with the shared channel-preview JS
adapter. Browser proof targets Chromium/Firefox save/reload with and without
JavaScript, plus converted-subtitle discovery and captions toggling.

Row 24 is Port with focused runtime, browser and native smoke acceptance.
The available Next runtime explicitly skips multi-stream embedded text in Burn
mode (`channel_session.rs`, `skip_embedded_text_subtitles`). Preserve extracted
sidecar Burn behavior and prepare missing sidecars before claiming this row.
Do not reuse item-wide native extraction blindly: it is gated by native settings,
does not select the resolved version, and lacks bounded cancellation. Conversion
must retain original media/index and its source timing and HTTP headers.
The implementation uses the packaged Next runtime's top-level Dynamic callback
for demand-time preparation. Re-read exact binding/selection around bounded
extraction outside playout locks; atomically promote only validated completion.
Advance all concrete track seeks by the callback join offset while preserving
original finish/out-points. Cache identity is version/file/stream/revision scoped,
with local size/mtime and bounded retained leases. The callback and real browser
Burn proofs pass; canonical evidence is in the UCU-02 milestone.
The HLS rendition uses a truthful language-neutral name because programme/item
language overrides can change within one linear channel. Link the rendition via
`SUBTITLES` on `EXT-X-STREAM-INF`, as required by RFC 8216 section 4.3.4.2.1;
upstream's fixed English label and unattached group cannot provide correct
multilingual preview behavior. Existing subtitle selection remains authoritative.

## Row 25 source map: reuse extracted embedded subtitles

Upstream `cd298820` changes only the extraction handler and changelog. Its
cache-presence check must resolve the generated relative subtitle path under
`SubtitleCacheFolder`, rather than the process working directory. Missing or
blank recorded paths still require repair. Rust `subtitles.rs::extract_subtitles`
already checks the rooted generated cache path, but only treats NULL recorded
paths as absent; complete the empty/whitespace case and prove candidate filtering
against a disposable cache and SQL fixture. Existing native extraction settings,
metadata ownership, image exclusions and post-success persistence remain intact.
There are no UI, schema, translation, or new health/runtime requirements.

The same commit changes absolute stream mapping to a subtitle ordinal computed
from only pending extraction candidates. That can select the wrong language when
an earlier embedded subtitle is cached or bitmap. Read-only later-source research
finds `4ffd775a` corrects the ordinal basis to all embedded streams. Rust scanner
metadata already carries absolute FFprobe indices, so preserve `0:<stream_index>`
and test partial extraction directly. This preserves selected-track correctness;
it does not process or supersede the later ledger row. Timing-only log changes
have no selection or persistence contract. Focused candidate/argv regressions and
scoped lint are required; unchanged decoded runtime evidence remains reusable.

## Row 26 source map: single permanent Next watermarks

Upstream `e04a834e` adds a watermark to the Next item wire contract, invokes
existing watermark selection at each item's original start time, and refreshes
Next publication after channel watermark/audio/subtitle preference changes.
Selection still owns item disabling, template/playout deco precedence, filler
eligibility, channel/global fallback and image resolution. Selector signature
changes only suppress publication-time logs; generated-model changes describe
source, absolute stream index, nine anchors, size, margins and opacity.
No schema, editor layout or translations change in this row.

Rust targets: `next_engine/protocol.rs`, a focused watermark projector, shared
`playback::select_playback_watermarks`, `NextHost::refresh`, and Dynamic Burn
reprojection. Preserve mirror source time for deco selection and target channel
identity for its logo. Channel save already notifies the normal registry, whose
forwarding adapter also notifies preview sessions. Both concrete and Dynamic
items must include the same watermark before descriptor hashing; joining an
item must not seek the independently looping watermark along with programme
media. Reuse local custom/logo/resource resolution and existing image-stream
index zero. Exactly one permanent watermark is this row's scope; intermittent,
multiple layers, expressions and graphics remain owned by later ordered rows.
Output-frame placement matches this upstream wire; later source-content placement
semantics are not implicitly enabled.
Source audit also exposes an existing shared-selector defect: Merge deco IDs are
discarded when item or channel/global fallback is selected. Upstream retains them,
including a single Merge-only watermark without fallback. Preserve the accumulated
IDs and port those upstream regression cases before relying on this selector for
Next. Channel-save forwarding is already present; watermark-definition/global
settings handlers are unchanged upstream, so notification redesign is excluded.

Required proof: stable serialized watermark fields, all anchors, actual/scaled
size, margins/opacity, existing selection/disable/mirror precedence and channel
refresh, plus decoded real Next overlay with retained audio/subtitles and Dynamic
Burn. Existing UI controls remain unchanged; runtime and relevant focused checks
own this integration's acceptance. Record gaps and evidence in UCU-02 before
advancing the pin.

Maintainer amendment (2026-09-23): the official Next RGB-image opacity defect
is accepted pending upstream correction. Keep the official engine, RGB fixture
and pixel diagnostic; do not require translucent RGB output for this row or add
a local patch/normalization layer. Geometry, disabled selection, audio/subtitle
proof and all other gates remain required. UCU-02 and the upstream-opacity debt
own the failure evidence and this explicit acceptance exception.

## Row 27 source map: intermittent Next watermarks

Upstream `91b029e4` changes only SyncNextPlayoutHandler and its generated Next
wire model. Exactly one intermittent watermark joins the existing permanent
case. Project periodic timing with the wall clock, minutes converted to milliseconds
and fully visible hold seconds converted to milliseconds. Omit phase offset,
appearance cap and fade duration; Next defaults these to zero, unbounded and
one second. Wall cycles use epoch boundaries, independent of item start, source
seek or Dynamic callback join time. Existing selection and image handling remain.

Rust targets: `next_engine/protocol.rs`, `watermark.rs`, focused projection and
Dynamic regressions, and decoded real-engine intermittent visibility proof.
Widen persisted i32 durations before multiplying. Reject negative or invalid
fade/hold/frequency combinations with redacted errors: current Next otherwise
returns no fade filters and displays a permanent overlay. Permanent watermarks
retain their omitted timing field and do not validate unused intermittent fields.
No schema, editor, translations or native FFmpeg argv builder changes are needed;
existing runtime/profile health requirements remain. Multiple layers and opacity
expressions belong to later rows. The explicitly accepted upstream RGB-opacity
limitation remains; intermittent visibility/fading is not exempted.

Required proof: exact wall-clock JSON/default omissions, large/invalid durations,
unchanged permanent output, unchanged periodic timing on programme joins and
publication identity changes, plus decoded visible/off behavior with retained
subtitle/audio selection. Reuse unchanged row-26 native smoke and selector proof;
record new runtime evidence and substantive review in UCU-02 before pin advance.

## Row 28 source map: configured Next scaling

Upstream `7869f165` maps the persisted scaling behavior to Next normalization:
Stretch, Crop and the ScaleAndPad default, with exact snake-case JSON names.
The remaining diff updates NUnit/analyzers, Scalar and Lennox.NvEncSharp packages;
review their actual runtime use rather than treating the mixed commit as tooling.

Rust already maps all three domain values in `next_engine/config.rs`; its existing
configuration regression asserts all three wire values. `NextHost::configuration`
loads the persisted profile and applies the shared browser-compatible override
before the same converter, retaining scaling selection. The default wire enum is
ScaleAndPad. No production change is indicated by this source comparison.

Add decoded real-Next evidence for a mismatched input/output aspect ratio so pad,
stretch and crop cannot pass as equivalent. Preserve selected audio and subtitle
behavior, and retain existing profile rejection/normalization coverage. No native
FFmpeg argv, schema, controls, translations or runtime-health requirement changes
are implied. Reuse prior unchanged native and host/preview proof; record the mixed
dependency review, focused tests and direct review in UCU-02 before pin advance.

Dependency review: NvEncSharp 2.1 updates NVENC definitions from API 9.1 to 12.2
and adds AV1/preset symbols, but this commit changes no capability caller/policy.
ErsatzTV already supplies the same AV1 GUIDs and uses device, codec/profile,
10-bit and B-frame queries. `ersatzrs-hwprobe` already implements that subset
with API 12.1, followed by FFmpeg capability enrichment/fallback; no numerical
ABI bump is justified. Its zero-initialized name buffer avoids the package's
Linux `kernel32.dll` regression. See the [package change](https://github.com/jlennox/NvEncSharp/commit/d8f91647bb56)
and [upstream report](https://github.com/jlennox/NvEncSharp/issues/19).
NUnit/analyzers remain test-project-only dependencies; Scalar only updates the
existing docs renderer dependency without changing configuration or API behavior.
These dependency implementations have no required Rust package substitution.

## Row 29 source map: provider subtitles, HTTP options and source-relative watermarks

Upstream `b1ce607c` changes SyncNextPlayoutHandler and generated Next wire models.
It admits provider subtitles while excluding eia_608 for the six Plex/Jellyfin/Emby
movie/episode types, independent of resolved local/HTTP path. It sets provider
HTTP keep_alive=false/reconnect=true, suppresses projection-time path logging,
and forwards PlaceWithinSourceContent for watermarks. Review all three behaviors.

Rust already admits provider candidates and projects selected embedded Convert
tracks from the exact media source/version. Burn uses bounded sidecar preparation.
Add provider-item eia_608 eligibility before built-in/custom ranking; exact pins
must fail explicitly rather than change streams. Preserve local-library and native
selection. Keep provider identity distinct from transport choice, so a locally
accessible provider file still receives the upstream exclusion.

Add optional keep_alive to the HTTP wire and explicitly disable it on published
provider relay/direct inputs, retaining reconnect and cloned track options. The
packaged Next default is already false; old JSON remains readable. Existing
redacted path handling/authenticated media-open routes remain, without credentials
in published playouts. Add within_source_content to Watermark and project the
existing domain setting; false/absent keeps frame-relative placement. Prove source
edges inside letterboxing and no placement effect for crop/stretch.

Targets: shared Next subtitle eligibility in playback.rs, Next protocol/source/
media/selection constructors, watermark projection, focused SQL/wire tests and
real-provider Convert/geometry regressions. Use all three authenticated HTTP
providers, selected language/audio and seek-relative WebVTT cues. Existing native,
provider relay and web subtitle transport evidence remains reusable when code is
unchanged. No schema, UI, translations or new runtime capability is introduced.
Keep the accepted RGB-opacity limitation separate; no provider/geometry exemption.
Record substantive review and new evidence in UCU-02 before pin advancement.

## Row 30 source map: Next acceleration and MPEG-TS hybrid

Upstream `d8955134` maps AMF/RKMPP acceleration and also admits Next channels
in MPEG-TS Hybrid mode. Read the full eight-file diff rather than limiting scope
to its title. Existing Rust configuration and wire tests already map both
accelerators. The shared hardware health check includes both and checks Next
channels; the Next runtime health check validates the same profile converter.
The upstream Scalar patch changes the existing documentation renderer package,
without changing API callers or settings. No Rust dependency change is implied.

CreateChannelHandler/UpdateChannelHandler preserve Hybrid or Segmenter for Next;
other modes normalize to Segmenter. Rust targets are app channel form normalization
and infra channel editor persistence. No domain enum or schema change is needed.
IptvController removes its blanket Next MPEG-TS rejection; trace the existing
hybrid HLS-to-TS wrapper and its ownership/authentication before changing dispatch.
Retain disabled-channel checks, selected audio/subtitles, admission and cleanup.

UI source: ChannelEditor.razor's existing MudSelect gains one MPEG-TS option in
the Next branch; layout, form columns and controls do not change. Rust's existing
channel editor/select styles and all localized mode labels can be reused. SSR
must retain saved Hybrid mode and expose only Hybrid/Segmenter for Next. The
island's initialization must preserve saved values; an actual engine change
retains upstream's Segmenter default. Both server-function and no-JS form paths
must preserve supported mode choices. Browser proof covers Chromium/Firefox,
save/reload, engine switches and unchanged subtitle settings; no shared CSS or
island DOM restructuring is needed. Runtime proof must exercise actual Next
MPEG-TS delivery and lifecycle, not infer it from working native Hybrid output.
No row acceptance or pin advancement until implementation, review and focused
checks pass. Window-wide gates remain separate.

Runtime review found a live WebVTT master causes the FFmpeg copy wrapper to
stall at zero MPEG-TS bytes; video-only input and Burn remain playable. Exact
upstream playlist formatting reproduces the failure, and no later applicable
fix was found. The maintainer accepted ADR 0099: Next wrappers use the existing
authenticated session media playlist, while HLS masters retain selectable text.
Localized editor help explains Burn for MPEG-TS subtitles. Native inputs and
stream-copy options remain unchanged. Row 30 requires Adapt (ADR 0099), with
corrected real Burn/Convert, command snapshot and JS/no-JS browser evidence
before acceptance. Canonical diagnosis and outcomes belong to UCU-02.

## Provisional triage and ledger

Seeds below come from [the original plan](ErsatzRS_catchup.md#provisional-triage-confirmed-per-row-nexttriage--extract-behavior-per-adr-0094).
The maintainer selected Next as an optional second engine on 2026-09-22.
Next-specific exclusions from the rejected native-only proposal are withdrawn;
review integration as required behavior under Accepted ADR 0095.
Its Next-engine references to 0094 mean the newly allocated **0095** here;
0094 already owns automatic add-on publication. Notes are hypotheses only.
Unlisted rows retain the plan's tooling hypothesis, or FFmpeg/tooling hypothesis
in ucu-06. The ten newly appended rows have no inherited triage. Every full diff still needs review, including mixed behavior/tooling.
For row 1 verify the rename does not change config-folder or data paths; for
`7850ced6` verify the green-line fix in our FFmpeg 9 build before removing it.
Schema rows and the random-start cluster retain the original plan's cautions.

| # | upstream sha | subject (PR) | verdict | owning parity slug | ErsatzRS commit | notes |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `1e5c0b12` | rename to ersatztv legacy (#2842) | Covered | support-pages | `docs(porting): reconcile upstream legacy rename and accept optional Next` | ucu-01; release-notes removal already present; upstream-only branding/tooling needs no Rust edit. See ucu-01 evidence. |
| 2 | `848db7ea` | even more dependency updates (#2848) | Port | hdhomerun-http-endpoints | `fix(web): correct HDHomeRun manufacturer repository link` | ucu-01; correct native manufacturer repository URL; remaining changes are .NET API/dependency and upstream branding maintenance. See ucu-01 evidence. |
| 3 | `5d779635` | update last dependencies (#2849) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream dependency maintenance` | ucu-01; only YamlDotNet/Refit package-version attributes change; native Cargo dependencies do not consume them. See ucu-01 evidence. |
| 4 | `47885758` | bundle ersatztv next artifacts (#2850) | Port | upstream-baseline-catch-up | `build(release): bundle optional Next engine artifacts` | ucu-01; verified artifact staging, license/provenance and Docker startup. Runtime integration remains open under ADR 0095. See ucu-01 evidence. |
| 5 | `1b292b5f` | try to fix github token | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream workflow token references` | ucu-01; token-only workflow correction; native uploads already use github.token and public Next staging needs no token. See ucu-01 evidence. |
| 6 | `f69761cb` | more gh token tweaks | Covered | upstream-baseline-catch-up | `docs(porting): reconcile upstream CLI artifact publication` | ucu-01; native asset upload already uses gh release; upstream develop cleanup and reusable-token wiring do not apply. See ucu-01 evidence. |
| 7 | `b76d8f8e` | more token alignment | Covered | upstream-baseline-catch-up | `docs(porting): reconcile upstream job-token alignment` | ucu-01; native uploads already bind github.token; Next downloads need no token and upstream develop cleanup does not apply. See ucu-01 evidence. |
| 8 | `30c361db` | only bundle the ersatztv-channel binary (#2851) | Port | upstream-baseline-catch-up | `build(release): bundle only the Next channel executable` | ucu-01; only channel binary beside server, with license/provenance retained. Packaging tests and Docker probe pass. See ucu-01 evidence. |
| 9 | `74a21abd` | fix workflow | Covered | upstream-baseline-catch-up | `docs(porting): reconcile upstream workflow syntax repair` | ucu-01; removes the stray workflow line already excluded during row 8. See ucu-01 evidence. |
| 10 | `a74896df` | prep for release v26.4.0 [no ci] | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream April release notes` | ucu-01; upstream changelog publication only. Window acceptance gates remain open; do not start UCU-02 yet. |
| 11 | `ea4d3b65` | add channel streaming engine setting (#2852) | Port | channels-admin-ui | `feat(web): persist optional Next engine selection` | ucu-02; additive migration, native default, engine roundtrip, Next HLS Segmenter normalization, localized SSR editor and ETV_NEXT_FOLDER boundary. Focused Rust/Clippy and Chromium/Firefox with/without JS pass. Actual Next execution remains subsequent integration work under ADR 0095. See ucu-02 evidence. |
| 12 | `ddc8ef7f` | fix: add playout button should open drop down menu (#2853) | Covered | playout-list-detail | `test(web): verify upstream menu activator coverage` | ucu-02; native playout toggle and direct show Add To/Quick/Deep Scan handlers already work. Four Chromium/Firefox regressions prove dropdown visibility, keyboard toggle, dialog activation and scan parameters. No Rust change. See ucu-02 evidence and media-browsers parity. |
| 13 | `2ed432f9` | feat: enable ersatztv next streaming engine (#2854) | Adapt (ADR 0095) | upstream-baseline-catch-up | `feat(infra): complete initial Next engine integration` | ucu-02; supervised local-video HLS, preview, track selection, atomic publication and Continue refresh verified. Provider/graphics rows and window/platform acceptance remain open. See ucu-02 evidence. |
| 14 | `4ee97d5b` | fix: fix next playout building (#2855) | Covered | upstream-baseline-catch-up | `test(web): verify Next channel update propagation` | ucu-02; existing mirror-offset projection, post-save publication notifications and executable-relative runtime discovery cover all three upstream corrections. See ucu-02 review and focused tests. |
| 15 | `f7853e80` | bundle ersatztv-channel binary with macos app (#2856) | Covered | upstream-baseline-catch-up | `docs(release): reconcile Next macOS bundling` | ucu-02; both macOS RID mappings and executable-adjacent archive staging already exist; six offline tests and actual relocation/archive snippets pass. Active macOS release/runtime acceptance remains explicit. See ucu-02 evidence. |
| 16 | `5cef7775` | fix duplicate windows zips on develop release | Covered | upstream-baseline-catch-up | `docs(release): reconcile Windows upload shell selection` | ucu-02; native upload already explicitly selects Bash with clobber; beta uses explicit PowerShell; no moving develop-tag cleanup. Workflow lint passes with declared self-hosted labels. See ucu-02 evidence. |
| 17 | `6ea0a296` | fix: consistently apply playout offset when generating xmltv (#2863) | Port | xmltv-guide | `fix(infra): preserve signed mirror offsets on channel upsert` | ucu-02; retain existing four-timestamp projection and fix negative-offset persistence; actual/guide windows, both signs and unchanged source items verified. See ucu-02 evidence. |
| 18 | `f2706f32` | feat: support playout json on windows (#2864) | Adapt (ADR 0095) | upstream-baseline-catch-up | `test(infra): verify Next publication on Windows` | ucu-02; accepted atomic-file publication passes seven native Windows/NTFS tests, including sharing-denial recovery, and six Linux tests. Full Windows Next playback remains open. See ucu-02 evidence. |
| 19 | `9882b008` | tweak streaming engine editor | Covered | channels-admin-ui | `docs(web): reconcile Next editor mode normalization` | ucu-02; existing editor immediately selects HLS Segmenter for Next and retains server-side no-JS normalization. Unchanged row-11 Chromium/Firefox evidence covers the setter behavior. See ucu-02 evidence. |
| 20 | `bb493670` | feat: support media server items in next streaming engine  (#2865) | Port | upstream-baseline-catch-up | `feat(infra): resolve Next media at playback time` | ucu-02; provider-scoped paths, exact file binding, current credentials, bounded Plex failover and Range-preserving media opens. Decoded three-provider/failover proof and Next HTTP/preview smoke pass. See ucu-02 evidence. |
| 21 | `e6496bbc` | feat: select audio streams for next engine during playout build (#2866) | Adapt (ADR 0096) | upstream-baseline-catch-up | `feat(infra): embed bounded JavaScript audio selectors` | ucu-02; shared native/Next callbacks, metadata and YAML/pin precedence; bounded execution and fallback tests, decoded alternate audio, native/Next smoke. Full platform acceptance remains open. See ucu-02 evidence. |
| 22 | `7e38297d` | fix: schedule all required fallback filler in classic schedules (#2867) | Adapt (ADR 0098) | classic-scheduling | `fix(scheduling): complete Classic chapter filler reconciliation` | ucu-02; complete clipped fallback, chapter/version identity, Continue/Refresh safety, accepted ADR 0097 no-match preservation and ADR 0098 deterministic expressions. Focused tests and decoded Unicode playback pass; full platform acceptance remains open. See ucu-02 evidence. |
| 23 | `57f7dfa5` | feat: select embedded image subtitle streams for next engine during playout build (#2868) | Port | upstream-baseline-catch-up | `fix(infra): scope Next subtitle selection to resolved versions` | ucu-02; embedded image selection validates resolved-version candidates before default/custom ranking. Generated PGS proves preferred bitmap, subtitles-off and retained audio; other Next regressions and HTTP/preview smoke pass. See ucu-02 evidence. |
| 24 | `1b5d6af7` | feat: convert text subtitles using next engine (#2869) | Port | upstream-baseline-catch-up | `feat(infra): prepare Next embedded text on demand` | ucu-02; persisted Burn/Convert, bounded version-bound embedded Burn, authenticated WebVTT and preview captions. 133 focused tests, eight Chromium/Firefox cases and native MPEG-TS/HLS smoke pass. See ucu-02 evidence. |
| 25 | `cd298820` | fix: don't extract embedded subtitles that have already been extracted (#2873) | Port | upstream-baseline-catch-up | `fix(infra): repair blank subtitle cache references` | ucu-02; rooted cache reuse already covered; NULL/blank/whitespace path repair and partial-extraction absolute-index regression verified. Later source confirms the pending-only ordinal bug. See ucu-02 evidence. |
| 26 | `e04a834e` | feat: overlay single permanent watermarks using next engine (#2878) | Port | upstream-baseline-catch-up | `feat(infra): project permanent Next watermarks` | ucu-02; nine anchors, size/margins, shared Merge selection, independent watermark timeline and Dynamic Burn. Focused tests and native smoke pass. Official Next RGB opacity defect is explicitly accepted pending upstream correction; no local workaround. See ucu-02 evidence. |
| 27 | `91b029e4` | feat: overlay single intermittent watermarks using next engine (#2879) | Port | upstream-baseline-catch-up | `feat(infra): project intermittent Next watermarks` | ucu-02; wall-clock periodic timing, widened units, redacted invalid-timing rejection and unchanged Dynamic join timing. 31 focused tests include decoded visible/fade/hidden states with subtitle/audio preservation; scoped Clippy passes. See ucu-02 evidence. |
| 28 | `7869f165` | feat: use configured scaling behavior with next engine (#2880) | Covered | upstream-baseline-catch-up | `test(infra): verify configured Next scaling output` | ucu-02; existing three-mode configuration confirmed by decoded production-host pad/stretch/crop with subtitle/audio preservation. Seven focused tests and scoped Clippy pass. Mixed dependency updates audited separately; no missing Rust hardware query or ABI bump required. See ucu-02 evidence. |
| 29 | `b1ce607c` | feat: convert subtitles from media servers (#2882) | Port | upstream-baseline-catch-up | `feat(infra): complete Next provider subtitle projection` | ucu-02; provider EIA-608 eligibility, HTTP options and source-content watermarks. 167 focused tests including decoded authenticated provider Convert and geometry pass; scoped Clippy and substantive review pass. See ucu-02 evidence. |
| 30 | `d8955134` | feat: use amf and rkmpp hw accel with next engine (#2883) | Adapt (ADR 0099) | upstream-baseline-catch-up | `feat(web): support Next MPEG-TS hybrid playback` | ucu-02; existing AMF/RKMPP mapping retained. Hybrid persistence/routing plus accepted media-playlist wrapper fix preserve audio and HLS Convert text. Seven focused tests, four Chromium/Firefox cases, command snapshot, real Burn/Convert lifecycle smoke and scoped Clippy pass. See ucu-02 evidence; window gates remain open. |
| 31 | `290f991b` | prep for release v26.5.0 [no ci] | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream May release notes` | ucu-02; changelog release heading and comparison links only. No product, dependency or workflow changes; window acceptance gates remain open. |
| 32 | `a1036430` | fix: clean up orphan actor and artwork records (#2885) | Port | upstream-baseline-catch-up | `fix(infra): clean orphaned actor and artwork records` | ucu-03; transactional actor-first cleanup, hourly cadence, bounded manual API and localized health. Existing provider deletion and Next logo selection verified. Focused regressions, all-target Clippy and substantive review pass; window gates remain open. |
| 33 | `54779e07` | fix: pin nvencsharp lib to 2.0.0 for linux compatibility (#2887) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream NVENC wrapper pin` | ucu-03; only the .NET NvEncSharp package version changes. Native platform-guarded NVIDIA loading does not consume this dependency; no Rust or ABI change. See milestone review. |
| 34 | `637e6be8` | prep for release v26.5.1 [no ci] | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream May patch release notes` | ucu-03; release heading, comparison links and description of the already reviewed NVIDIA wrapper pin. All window rows reconciled; local window gates and first consolidated exact-SHA CI remain open. |
| 35 | `0fc7ebb0` | fix: use jwt in hls direct playlist (#2889) | Covered | upstream-baseline-catch-up | `docs(porting): verify existing HLS Direct JWT propagation` | ucu-04; existing HlsQuery/AccessToken plumbing covers all seven raw media URLs, stable variant and segment URLs, including blank tokens. Thirteen relevant tests pass in green CI 35880721503 at unchanged source 8f722099; substantive Codex review agrees. NvEncSharp 2.1.1 is upstream-only tooling; no new hardware or JWT-enabled end-to-end playback claim. |
| 36 | `64296597` | feat: add channel config overlays for next engine (#2891) | Port | upstream-baseline-catch-up | `feat(infra): apply Next channel configuration overlays` | ucu-04; ordered profile/default/channel overlays, startup directory, null omission and localized bounded health inspection. Existing deinterlace/tonemap mapping retained. 122 Next tests including decoded overlay playback, four health tests, all-culture localization, scoped Clippy, Burn/Convert HTTP smoke and substantive review pass. Window gates remain open. |
| 37 | `fecce92e` | fix: use configured ffmpeg with all graphics elements (#2892) | Covered | drawvg-graphics-engine | `docs(porting): verify configured FFmpeg for native graphics` | ucu-04; ADR 0005 native motion/subtitle inputs share the resolved FFmpeg executable. Existing locator/snapshot CI proof plus decoded all-kinds graphics test with poisoned PATH pass; substantive review agrees. Next graphics projection remains separately open under ADR 0095. No Rust changes. |
| 38 | `41bf22eb` | chore: dependency updates (#2893) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream dotnet dependency updates` | ucu-04; eight project files plus ReSharper tool manifest, version-only changes. Exact packages are absent from Rust; ADRs 0009/0058/0076/0096 preserve existing search/template/JavaScript boundaries. No application, test, schema or runtime-artifact change and no new parity claim. Substantive review passes. |
| 39 | `7c4a5e3a` | feat: support remote streams with next engine (#2899) | Adapt (ADRs 0066/0100) | upstream-baseline-catch-up | `feat(infra): support Next remote streams and reports` | ucu-04; authenticated direct/prepared/retained/ordinary sources, fresh selected tracks, finite/live timing and supported captions, opt-in reports and independent normal/preview ownership. Real routed disconnect/stop cleanup, scoped review/tests/Clippy and Next Burn/Convert plus native lifecycle smoke pass. Accepted embedded-text limits remain visible debt. Local checkpoint complete; window gates remain open. |
| 40 | `2c02561f` | chore: update dependencies (#2900) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream May dependency refresh` | ucu-04; version-only .NET updates in eleven project files. Updated packages are absent from Rust; hardware diagnostics and ADR 0096 JavaScript boundaries remain unchanged. Source review finds no application, schema, asset or runtime-artifact changes. |
| 41 | `cc9880c2` | feat: always save error reports with next engine (#2907) | Port | upstream-baseline-catch-up | `feat(infra): always retain Next error reports` | ucu-04; report directory always supplied for normal/preview, independent of native report opt-in. Real Next failure retains FFmpeg log and diagnostic JSON; normal playback, setting matrix, scoped Clippy/review and docs pass. Native health setting and overlay priority preserved; window gates remain open. |
| 42 | `822c7547` | pin imagesharp dependency (#2909) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream ImageSharp constraint` | ucu-04; single .NET PackageReference constraint adds a below-4 upper bound. ImageElementBase and all application behavior are unchanged. Rust has no ImageSharp dependency and uses native FFmpeg image inputs under ADR 0005. Scoped review and progress validation pass; no new graphics acceptance claim. |
| 43 | `9c5d86a1` | feat: show next version in ui (#2910) | Port | upstream-baseline-catch-up | `feat(web): display cached Next engine version` | ucu-04; shared playback resolver, bounded startup probe and localized static drawer footer. Missing/failing/stalled/oversized probe tests, all-nine-culture coverage, scoped Clippy and hydration lint pass. Matched bundle passes Chromium/Firefox installed and unavailable cases, desktop/no-JS and mobile navigation. Scoped review passes; window gates remain open. |
| 44 | `577983af` | feat: support explicitly scheduled images using next engine (#2911) | Covered | upstream-baseline-catch-up | `test(infra): verify scheduled Next images` | ucu-04; existing ImageId binding and video/finite-silence projection already cover the upstream query/filter change. Persisted image-only regression and real Next PNG output prove scheduled duration, held colour, silent AAC and cleanup. Scoped Clippy/review and routed Next smoke pass; no production playback change. |
| 45 | `b1c7702a` | fix: properly sync in and out points with next engine (#2912) | Covered | upstream-baseline-catch-up | `test(infra): verify Next source timing boundaries` | ucu-04; existing projection retains millisecond bounds and complete sources across explicit tracks and generated silence. Boundary/no-audio HTTP regressions, all 22 selection tests and five decoded seek/Burn/Convert/audio/off scenarios pass; scoped Clippy/review pass. Unchanged routed smoke evidence is reused. |
| 46 | `99a1ecee` | chore: update dependencies (#2915) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile graphics dependency patches` | ucu-04; two .NET manifests update SkiaSharp/native assets and Scriban patch versions. Unchanged graphics/template consumers and no matching Cargo dependency. Existing ADR 0005/0058/0076 boundaries apply; scoped source review and progress validation pass. |
| 47 | `aa639375` | fix: use case-insensitive lookup for mpeg-ts script (#2916) | Port | mpeg-ts-channel-streaming | `fix(web): resolve MPEG-TS scripts without case sensitivity` | ucu-04; Unicode simple-uppercase script-ID matching, lowercase troubleshooting fallback and quiet built-in fallback. Twelve focused tests, scoped Clippy, hydration lint, license audit and HTTP smoke pass; substantive review finds no row blocker. Window gates remain open. |
| 48 | `14ee8063` | feat: sync generated music video credits to next engine (#2917) | Adapt (ADR 0095) | upstream-baseline-catch-up | `feat(next): render music video credits on demand` | ucu-04; scoped ticket for libass URL, active-session/persisted-binding checks, template fallback and decoded Burn/Convert with sought Dutch audio. Window gates remain open. |
| 49 | `294a2ff1` | feat: support dynamic fallback filler when using next engine (#2919) | Adapt (ADR 0095) | upstream-baseline-catch-up | `feat(next): reconcile dynamic fallback filler` | ucu-04; authenticated ephemeral gap callbacks preserve policy, tracks, credits and source lifecycles; decoded local/provider/Remote Stream Burn/Convert handoff, exact pins/silence, focused regressions and routed smoke pass. Accepted ADR 0100 limitations remain explicit. |
| 50 | `387da2b8` | feat: use probe hints with next engine (#2920) | Adapt (ADR 0095) | upstream-baseline-catch-up | `feat(next): reconcile probe hints and leading gaps` | ucu-04; schema 0.0.2, guarded stored local/provider hints, leading gap publication and authenticated shared-manifest callbacks pass focused, decoded Next and routed smoke proof. |
| 51 | `679fb172` | chore: update refit to work around cert revocation (#2921) | N/A (NuGet-only update) | upstream-baseline-catch-up | `docs(parity): classify Refit update as upstream-only` | ucu-04; four Refit package references in two C# manifests change version only; Rust has no Refit/NuGet dependency or affected source contract. |
| 52 | `dca93717` | fix: improve speed of motion graphics element compositing (#2922) | Covered (ADR 0005) | drawvg-graphics-engine | `docs(porting): verify native motion graphics compositing` | ucu-04; native FFmpeg motion input is placed at its own dimensions without a Skia full-frame copy. Loop/Disappear/Hold snapshots, Hold renderer plan and decoded four-kind compositing pass; Next graphics debt remains open. |
| 53 | `dbdf3dff` | feat: add epg_entries support to image graphics elements (#2923) | Port | drawvg-graphics-engine | `feat(graphics): import image EPG template context` | ucu-04; import and persist Image epg_entries, render its EPG-selected path without rereading YAML under ADR 0005; focused import/render and decoded graphics proof pass. |
| 54 | `7032cfc8` | fix: better handling of local subtitles (#2924) | Port | upstream-baseline-catch-up | `fix(subtitles): complete local collision repair` | ucu-04; kind-separated deterministic merge, one-time folder invalidation, all-owner hourly cleanup and localized health; upgrade rescan, native/Next language selection, decoded captions and native smoke pass. |
| 55 | `32e54828` | fix: use und language tag with local subtitles (#2925) | Covered | upstream-baseline-catch-up | `test(subtitles): verify undefined sidecar language and flags` | ucu-04; existing suffix-only parser preserves title isolation and und without a language; persisted path/flag/rescan matrix and undefined-language playback selection pass. |
| 56 | `6b74da3c` | chore: update docker image (#2926) | Port | upstream-baseline-catch-up | `fix(docker): package matched Intel media drivers` | ucu-04; pinned Intel userspace stack, licenses and stripped libraries; x64 loader and managed FFmpeg CPU/VAAPI-loading proof plus ARM64/ARMv7 runtime builds/start pass. Physical B70 playback is not claimed. |
| 57 | `3756e7fb` | chore: update dependencies (#2927) | Port | upstream-baseline-catch-up | `fix(deps): reconcile SQLite and NCalc 6.2` | ucu-04; bundled SQLite 3.51.3 closes CVE-2025-6965; NCalc 6.2 saved-expression NaN and signed-zero behavior follows exact-version oracle. Opacity helper and NuGet-only updates are covered; focused regressions, dependency audit, scoped Clippy and native smoke pass. |
| 58 | `0b8c5146` | fix: continue serving old segments when unable to transcode in realtime (#2933) | Port | hls-variant-playlists-segmenter | `fix(hls): retain stale segments until replacement` | ucu-04; no-limit append trim keeps the prior MPEG-TS/fMP4 playlist when every segment predates the cutoff. Red-then-green stale-window and referenced-file cleanup regressions, native smoke and TSDuck transport gate pass. Empty source playlists already avoid a minimum-operation failure. |
| 59 | `2b7531ca` | fix: remove already-played playout items (#2934) | Port | scripted-scheduling; hls-variant-playlists-segmenter | `fix(scheduling): trim played playout items by channel mode` | ucu-04; continuous scripted builds retain four hours plus mirror offset; on-demand time shifting removes only items finished before the saved checkpoint in its shift transaction. Focused scheduler, on-demand SQLite and delayed-mirror persistence tests, scoped Clippy and native smoke pass. |
| 60 | `c8bd9766` | chore: update openapi to fix build (#2936) | N/A (NuGet-only build override) | upstream-baseline-catch-up | `docs(porting): classify OpenAPI build override as upstream-only` | ucu-04; single `.csproj` package reference overrides transitive Microsoft.OpenApi 2.0.0 with 2.7.5. Startup registration and endpoint code do not change; Rust has no matching NuGet or OpenAPI generation dependency. Source/package-graph review and progress validation pass; no API endpoint parity claim. |
| 61 | `42937ac3` | feat: YAML Scheduled Playouts (#2935) | Port | yaml-scheduling | `feat(scheduling): port YAML scheduled playouts` | ucu-04; named annual/specific date overrides select by local date and priority, with per-list state and persisted continuation indices. Scheduled sequences expand, repeat and shuffle in the active list; local wait/pad timing and editor schema align. Core YAML, SQLite restart, web validation, scoped Clippy and native smoke pass. Window gates remain open. |
| 62 | `0036fc5c` | fix: subtitles with next engine (#2938) | Port | library-management; upstream-baseline-catch-up | `fix(infra): include custom selectors in subtitle extraction` | ucu-04; Next subtitle probe hints, version-scoped custom selection and native extraction split were already ported. Playout extraction now admits custom-selector channels with general subtitles off. Red-then-green SQL eligibility, 8 probe-hint, 22 selection, 2 extraction, decoded official Next Burn/Convert and routed Next Convert smoke pass. ADR 0100 live/one-shot text limits stay open. |
| 63 | `620bc0bc` | fix: properly delete manual collections from search index (#2939) | Covered | collections-smart-collections; search-index | `test(search): verify manual collection deletion invalidates queries` | ucu-04; live SQLite search documents re-read collection names for each query, while foreign-key cascade removes deleted membership. A focused integration test proves immediate search removal and continued media search. No persisted index needs rebuilding. |
| 64 | `075649ad` | fix: synchronize subtitle titles from plex libraries (#2940) | Adapt (ADR 0098) | library-management; classic-scheduling; upstream-baseline-catch-up | `test(plex): cover subtitle title synchronization` | ucu-04; Plex embedded/external titles already persist in MediaStream and movie/episode Subtitle rows; two provider-sync regressions prove both. The accepted deterministic Classic expression profile remains on NCalcSync 6.2 despite the upstream 6.4 package bump. Other .NET package updates map to existing ADR 0009/0096 alternatives or no Rust package. Focused NCalc corpus, scoped Clippy and progress checks pass. |
| 65 | `3e1918c9` | feat: restore release notes on home page (#2942) | Port | support-pages | `feat(web): show bundled release notes on home and health` | ucu-04; localized shared SSR card on both routes selects the installed beta or latest dated bundled changelog section, sanitizes Markdown and serves the full offline changelog. GitHub remains an optional link; anonymous API and page return 404. Focused section/security tests, web check, scoped Clippy, dependency audit, hydration lint and Chromium 320px/content/link proof pass. Window gates remain open. |
| 66 | `95fa937c` | chore: update skia sharp (#2943) | N/A (upstream SkiaSharp API migration) | upstream-baseline-catch-up | `docs(porting): classify SkiaSharp 4 API migration` | ucu-04; two .NET package upgrades and three DrawBitmap overload updates add explicit default sampling. Rust SVG logos and native FFmpeg Image/Motion overlays use no Skia API or dependency. Source/ADR review and progress validation pass; no new pixel-equivalence claim. Window gates remain open. |
| 67 | `0b44ad47` | prep for release v26.6.0 [no ci] | N/A (upstream release metadata) | upstream-baseline-catch-up | `docs(porting): classify upstream 26.6.0 release prep` | ucu-04; only ErsatzTV CHANGELOG heading/comparison links change. ErsatzRS releases and dated changelog sections have separate acceptance; no product or build behavior changes. Source and progress checks pass; UCU-04 window gates remain open. |
| 68 | `18137832` | fix: update plex movie and other video titles (#2947) | Port | library-management; upstream-baseline-catch-up | `fix(infra): refresh Plex titles on ordinary scans` | ucu-05; versioned title-aware movie/Other Video Etag refreshes old timestamp-only fingerprints once and title-only changes thereafter. Existing metadata upsert writes Title and SortTitle. Red/green focused regression, 20 Plex tests, scoped Clippy, format and progress checks pass. UCU-05 window gates remain open. |
| 69 | `e5a2a763` | fix: randomize start points with classic playlists (#2948) | Port | classic-scheduling; upstream-baseline-catch-up | `fix(scheduling): honor Classic playlist random start` | ucu-05; eligible fresh Chronological, SeasonEpisode and ShuffleInOrder playlist children receive deterministic start indices; retained sources bypass new initialization. Focused playlist tests, scoped core Clippy, format and progress checks pass. Rows 70-71 own follow-up start-state behavior; row 85 owns playlist continuation. |
| 70 | `0d692e70` | fix: always randomize start points (#2949) | Port | classic-scheduling; upstream-baseline-catch-up | `fix(scheduling): persist enumerator started state` | ucu-05; Started is stored in all three SQLite enumerator state tables and carried through Classic advancement. Previously unused anchored sources randomize; played sources at wrapped index zero stay in sequence; the final item is eligible. Migration upgrade, repository round trip, 247 core tests and scoped Clippy pass. Row 71 remains open. |
| 71 | `4fb42f69` | fix: random start point is needed with every playlist build (#2950) | Port | classic-scheduling; upstream-baseline-catch-up | `fix(scheduling): replay random playlist offsets on Continue` | ucu-05; each Classic playlist build recreates seeded child offsets and a retained Continue build replays its saved index. The 6+6 versus uninterrupted 12-item timeline regression passes with identical identities and timestamps; all 248 core tests and scoped Clippy pass. Row 85 owns broader playlist progress repair. |
| 72 | `16e341d9` | fix: key singleton mutex on config folder (#2946) | N/A (upstream singleton mutex; ADR 0080) | upstream-baseline-catch-up | `docs(porting): classify upstream config-folder mutex` | ucu-05; upstream re-keys a Windows global mutex to AppDataFolder. ErsatzRS has no such mutex; multiple active server instances remain unsupported under ADR 0080. No Rust runtime surface or user-visible contract changed. Source and progress checks pass. |
| 73 | `5855deb8` | feat: upgrade ffmpeg to 8.1.2 (#2951) | Adapt (ADR 0063) | ffmpeg-command-generation; upstream-baseline-catch-up | `docs(porting): classify FFmpeg 8.1.2 upstream upgrade` | ucu-05; upstream changes package pins, Docker bases and version health to 8.1.2. Accepted ADR 0063 requires ErsatzRS FFmpeg 9, startup rejects 8.1.2, and health reports the resolved supported runtime. No filter graph or media selection change. Source and progress checks pass. |
| 74 | `6d8666b8` | chore: update dependencies (#2952) | N/A (NuGet-only Jint update; ADR 0096) | upstream-baseline-catch-up | `docs(porting): classify Jint patch update` | ucu-05; only ErsatzTV.Infrastructure.csproj changes Jint 4.13.0 to 4.14.0. ErsatzRS uses accepted QuickJS/rquickjs under ADR 0096; no source or behavior contract changes. Source and progress checks pass. |
| 75 | `7850ced6` | fix: remove green line workarounds; etv's ffmpeg 8.1.2 fixes (#2953) | Port | ffmpeg-command-generation | `fix(ffmpeg): remove obsolete green-line crop filters` | ucu-05; upstream deletes NVIDIA H.264 and radeonsi HEVC crop metadata filters after FFmpeg 8.1.2. ErsatzRS FFmpeg 9.0 uses the same affected encode paths. Removed both filters and their applicability gate; two argv snapshots and all 344 FFmpeg tests pass. A local FFmpeg 9 NVENC overlay encode preserves 1080 bottom-row colors without crop metadata. AMD live hardware proof remains unavailable here; window playback gates remain open. |
| 76 | `f7bdcede` | fix: checkpoint classic schedules every day (#2954) | Port | classic-scheduling | `fix(scheduling): retain Classic daily checkpoint prefix` | ucu-05; captures day seams inside all build passes. A seven-day shuffled hourly Continue regression failed before the fix, then passed. Restarted SQLite Refresh preserves the crossing item and appends the matching future. All 249 core tests, scoped Clippy and progress checks pass. |
| 77 | `6ff44cf5` | fix: don't prematurely reset shuffled collection progress (#2955) | Adapt (ADR 0010) | classic-scheduling | `fix(scheduling): retain inactive alternate progress` | ucu-05; Rust's flat enumerator already validates against the full item count. A core regression proves index 9 of ten remains valid. Corrected Continue publication and active-source audit to retain dormant alternate anchors; restarted SQLite proof preserves the inactive source seed/index. Existing multipart grouping debt remains visible in the owning parity record; no grouped parity is claimed. |
| 78 | `16b71569` | fix: playlist enumerator consistency fixes (#2956) | Adapt (ADRs 0010/0020) | classic-scheduling; yaml-scheduling | `fix(scheduling): restore YAML playlist history safely` | ucu-05; checkpoint 1eb06d15 plus versioned YAML history restore. Playable cycles, canonical shuffle, candidate rollback, repeated sources, all Classic Marathon groups/batches and structured SQL loaders have core and restart/Refresh proof. Legacy state refuses before clearing; explicit Reset recovers. See UCU-05 evidence. |
| 79 | `a2f54ef1` | fix: block playout build loop (#2957) | Port | block-scheduling | `fix(scheduling): rebuild Block filler safely and observe shutdown` | ucu-05; checkpoint ee8f7c34 plus Reset filler/history replacement, retained primary repositioning, and cooperative cancellation through scheduler ownership. Core and SQLite evidence in UCU-05; row 80 follows. |
| 80 | `9cc771be` | feat: show next and ffmpeg version in troubleshooting (#2958) | Adapt (ADR 0095) | support-pages | `feat(troubleshooting): report detected engine versions` | ucu-05; shared detected versions in General and playback archives, startup-only Next cache, in-order SSR. 26 library tests and four Chromium/Firefox JavaScript/no-JS checks pass; see UCU-05 evidence. |
| 81 | `3f2efcd4` | fix: discover tests in ErsatzTV.Tests (#2959) | N/A (upstream .NET test tooling) | upstream-baseline-catch-up | `docs(porting): classify upstream test discovery fix` | ucu-05; sole diff adds Microsoft.NET.Test.Sdk 18.8.1 to ErsatzTV.Tests. Cargo/Nextest has no dependency on that discovery adapter. No runtime or test behavior is changed in ErsatzRS; source and progress checks pass. |
| 82 | `4ffd775a` | fix: text subtitle fixes (#2960) | Adapt (ADRs 0095/0100) | upstream-baseline-catch-up | `test(streaming): verify Next subtitle font and empty-cue playback` | ucu-05; checkpoint 7b5ef28f supplies the font cache; official Next runtime proves selected font rendering, end-of-cue playback and alternate audio. Exact-version preparation and native absolute extraction indexes preserve existing safety; see UCU-05 evidence. |
| 83 | `05dd6cbe` | prep for release v26.7.0 [no ci] | N/A (upstream release metadata) | upstream-baseline-catch-up | `docs(porting): classify upstream 26.7.0 release metadata` | ucu-05; only CHANGELOG release heading and comparison links change. ErsatzRS retains its own release history; no runtime contract changes. Source and progress checks pass. |
| 84 | `15db3c4c` | fix: don't ignore channel logo watermarks that are urls (#2961) | Port | upstream-baseline-catch-up | `fix(streaming): render generated channel-logo watermarks` | ucu-05; checkpoint 876ed514 preserves external URLs; cached PNG fallback now preserves generated logos. Native and official Next decoded output, selected audio, Next subtitles and DisableWatermarks pass. See UCU-05 evidence; window gates remain open. |
| 85 | `ecf6c2c1` | fix: classic playout playlist progress (#2966) | Adapt (ADR 0020) | classic-scheduling; ersatzrs-healthcheck | `fix(scheduling): preserve complete Classic collection identities` | ucu-05; complete source keys now include RerunCollectionId. Existing earliest complete checkpoints preserve playlist/fake keys. Three-week playlist and rerun restart/Refresh, legacy refusal, actual health reporting and core regressions pass; see UCU-05 evidence. |
| 86 | `a7ecc5f9` | fix: improve flexible start time handling near midnight (#2967) | Port | classic-scheduling | `fix(scheduling): resolve flexible starts across local midnight` | ucu-05; explicit local civil clock, independent tomorrow offsets and inclusive 12-hour wait. Core clock/builder and persisted DST Continue/Refresh proof pass; Strict and complete UTC checkpoints retain their contracts. See UCU-05 evidence. |
| 87 | `3d24c3f0` | fix: playout gap correctness in ui (#2968) | Port | playout-list-detail | `fix(playout): publish gaps with builds and on-demand shifts` | ucu-05; final item-derived positive gaps replace stale rows atomically; on-demand gaps trim and shift with items. Repository, rollback, continuity and Chromium/Firefox detail proof pass; see UCU-05 evidence. |
| 88 | `57a0c40d` | fix: bundle newly patched ffmpeg for qsv on windows (#2969) | N/A (ADR 0063) | ffmpeg-command-generation; upstream-baseline-catch-up | `docs(porting): classify upstream Windows FFmpeg archive update` | ucu-05; sole runtime-facing diff changes the upstream FFmpeg 8.1.2 Windows archive revision. ErsatzRS uses its first-party FFmpeg 9 line under ADR 0063; no native QSV execution claim is inferred. Source and progress checks pass. |
| 89 | `b5fd035b` | feat: show next and ffmpeg versions in troubleshooting results (#2970) | Covered | support-pages | `docs(porting): record existing playback archive engine versions` | ucu-05; row-80 f6f076a2 already writes both top-level keys to troubleshooting_info.json and includes it in playback ZIPs. Shared detected-version tests cover present/absent optional Next under ADR 0095; source and progress checks pass. |
| 90 | `35b5a259` | prep for release v26.7.0 [no ci] | N/A (upstream release metadata) | upstream-baseline-catch-up | `docs(porting): classify upstream 26.7.1 release metadata` | ucu-05; only CHANGELOG adds a 26.7.1 heading and comparison links, despite the 26.7.0 commit subject. ErsatzRS keeps its own release history. Source and progress checks pass; UCU-05 window acceptance remains open. |
| 91 | `5ab81525` | fix: mpeg-ts playback with multi-byte characters (#2971) | Port | mpeg-ts-channel-streaming | `fix(web): preserve Unicode MPEG-TS script inputs` | ucu-06; ETV_* environment values, bundled script upgrade and custom-template compatibility. Fourteen focused tests, strict web Clippy and native Windows/Unix FFmpeg 9 Unicode playback pass with both audio tracks. The upstream FFmpeg 8.1.2 archive is superseded by ADR 0063. |
| 92 | `f737de89` | fix: jellyfin external subtitles (#2972) | Port | upstream-baseline-catch-up | `fix(infra): restore provider external subtitle playback` | ucu-06; ADR 0101, provider stream identity, local-only cleanup, targeted rescan repair, authenticated subtitle artifacts and native/Next/troubleshooting integration. Thirty affected tests, native and official Next decoded playback, scoped strict Clippy and review pass. |
| 93 | `ad9848cc` | feat: troubleshoot next engine playback (#2973) | Port | support-pages; upstream-baseline-catch-up | `feat(web): complete Next playback troubleshooting` | ucu-06; isolated finite Next previews, selected tracks/seek/profile, remote callback lifetime, nested reports and bounded completion logs. Decoded local/provider/direct/prepared/producer/retained proof, 712 web tests, strict Clippy, matched build and eight Chromium/Firefox browser cases pass. Pin and checkout advance together; window gates remain open. |
| 94 | `9088ae1a` | feat: more improvements to next troubleshooting (#2974) | Port | support-pages; upstream-baseline-catch-up | `feat(next): complete troubleshooting selection and diagnostics` | ucu-06; custom selectors and explicit subtitle precedence, localized completion speed, bounded sanitized process levels/targets and opt-in selection decisions. 88 focused tests, five decoded Next runtime cases, eight browser cases and the 716-test web parity gate pass. Pin and checkout advance together; window gates remain open. |
| 95 | `7b43e2f7` | chore: update next channel config model (#2975) | Port | upstream-baseline-catch-up | `feat(next): preserve optional subtitle force style` | ucu-06; optional force_style wire field, exact populated/empty value preservation and absent/null compatibility. Eleven config/protocol tests and strict infra/web Clippy pass; generated defaults remain unchanged. |
| 96 | `3b53b69e` | chore: update dependencies (#2976) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream package patches` | ucu-06; .NET CliWrap, Testably, Scriban, MudCalendar and Scalar patch versions only; Rust counterparts and application consumers unchanged. Mesa note changes no container artifact and proves no new hardware acceptance. Scoped diff review and progress check pass. |
| 97 | `aae2529f` | chore: update dependencies (#2977) | Adapt (ADRs 0009/0096/0098) | upstream-baseline-catch-up; classic-scheduling | `test(deps): retain SQLite safety and expression contracts` | ucu-06; Refit generated-client construction maps to existing reqwest, retained SQLite 3.51.3 includes WAL-reset fix, and the runtime guard protects that floor. NCalc remains the accepted deterministic 6.2 profile. All 34 provider/SQLite and 19 expression tests plus strict infra Clippy pass. |
| 98 | `7c96203b` | chore: update playout model to get new next version (#2978) | Covered | upstream-baseline-catch-up | `test(next): verify Dolby Vision hint preservation` | ucu-06; typed HTTP is_live and video dv_profile already exist. Persisted/fresh producer profile 5/7/8 and absent-profile regression plus live seek/callback checks pass (15 tests); strict infra Clippy passes. Model-only coverage, no new GPU-rendering claim. |
| 99 | `9f83c8d2` | fix: new ffmpeg for windows vulkan/cuda interop (#2979) | Port | upstream-baseline-catch-up | `fix(ffmpeg): adopt corrected Windows Vulkan runtime` | ucu-06; first-party n9.0 export patch, shared playback/probe transfer and cache revision; focused tests and strict Clippy pass. Native Windows control/interop/25-frame decode and both package contracts pass. Approved v2.0.1 public downloads match tested hashes; managed default updated. GPU comparison isolates the old probe defect, not a before/after runtime-patch effect. |
| 100 | `24eab260` | ci: remove dependabot [no ci] | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream dependency automation` | ucu-06; deletes only upstream daily NuGet/Docker automation. ErsatzRS retains its independent weekly Cargo/GitHub Actions rollup. No product behavior change; source/diff review and progress validation pass. |
| 101 | `11789e8d` | chore: update dependencies (#2980) | N/A (upstream tooling) | upstream-baseline-catch-up | `docs(porting): reconcile upstream dotnet dependency patches` | ucu-06; .NET Extensions/ASP.NET/CLI patches, test-only NSubstitute update and removal of a .NET OpenAPI override. Rust dependencies and application consumers unchanged. Docker Vulkan release note proves no container change; row 102 owns that acceptance. Source/manifest review and progress validation pass. |
| 102 | `33c33587` | feat: enable vulkan in docker (#2981) | Port | upstream-baseline-catch-up | `fix(docker): enable Vulkan runtime and driver exposure` | ucu-06; no blanket engine disable; minimal Vulkan libraries/ICDs and NVIDIA graphics capability added, old probe caches invalidated on capability change. amd64/arm64/ARMv7 and alternate package layers pass; non-root software Vulkan and CUDA/NVENC controls pass. WSL exposes no matching NVIDIA Vulkan UUID, so physical NVIDIA Docker interop is not claimed. Scoped tests/Clippy/binary checks pass; window owns full application gates. |
| 103 | `5febb943` | docs: update changelog | N/A (upstream release notes) | upstream-baseline-catch-up | `docs(porting): reconcile Next engine release notes` | ucu-06; changelog-only QSV overlays, audio synchronization and anamorphic scaling announcements. No engine source, pin or host integration changes. Release notes are not runtime proof; existing Next acceptance remains unchanged. Full diff review and progress validation pass. |
| 104 | `080039be` | chore: update windows ffmpeg (#2982) | Covered | upstream-baseline-catch-up | `docs(porting): verify bitmap subtitle readrate coverage` | ucu-06; complete upstream readrate patch already in FFmpeg n9.0. Published v2.0.1 Windows/Linux executables pass sparse PGS gap, all 400 frames, selected alternate audio and decode checks. No runtime rebuild or host changes required; window gates remain open. |
| 105 | `3a02c39b` | feat: support multiple watermarks using next engine (#2983) | Port | upstream-baseline-catch-up | `feat(next): support ordered watermark layers` | ucu-06; schema 0.0.3, stable ZIndex layers across publication/dynamic/fallback/remote paths, all troubleshooting selections and unchanged strict mode validation. 210 Next tests, decoded overlap/periodic/audio/subtitle proof, prepared-remote preservation, 8 Chromium/Firefox cases and full web parity gate pass. Window gates remain open. |
| 106 | `cd30ac4a` | feat: image graphics element support in next (#2984) | Port | upstream-baseline-catch-up | `feat(next): project image graphics through shared selection` | ucu-06; shared templates and image/watermark ZIndex order across scheduled/dynamic/remote/fallback/preview paths; prepared source replacement and identity preserved. 213 Next tests, decoded local/HTTP image playback with selected tracks, eight Chromium/Firefox cases, 716 web tests and strict Clippy pass. Unsupported definitions fail explicitly; window gates remain open. |
| 107 | `b36e94cc` | fix: on-demand channel progress using next engine (#2985) | Port | upstream-baseline-catch-up | `fix(next): persist on-demand viewer checkpoints` | ucu-06; startup time-shift precedes publication after capacity admission; five-minute/final saves use viewer touch with configured timeout, monotonic and first-item clamps. Preview/continuous isolation, locked rebuilds and bounded cleanup preserved. Production-generated SQLite restart, 221 regressions, decoded official Next resume with audio/subtitles, and strict Clippy pass. |
| 108 | `c0e4c4ac` | chore: update windows ffmpeg (#2988) | Port | upstream-baseline-catch-up | `fix(ffmpeg): adopt sparse-stream pacing runtime` | ucu-06; published first-party v2.0.2 applies the upstream sparse-pacing fix on FFmpeg 9. Source fallback/exclusion tests and decoded Windows/Linux bitmap/alternate-audio regressions pass; both v2.0.1 controls fail unchanged pacing bounds. Public downloads match tested archives. Managed default adopted; all local window gates pass. |
| 109 | `7b7a19a2` | prep for release v26.8.0 [no ci] | N/A | upstream-baseline-catch-up | `docs(parity): reconcile upstream release bookkeeping` | ucu-06; only upstream CHANGELOG release heading and compare links change. No source, runtime or package behavior changes; reviewed exact diff. Existing window gates remain valid; no release acceptance is inferred from these notes. |
| 110 | `14372575` | fix: close hole that allowed management ui over streaming port (#2989) | Adapt (ADR 0102) | upstream-baseline-catch-up | `fix(web): isolate streaming and internal callback listeners` | ucu-07; actual-peer loopback and separate streaming route boundaries, trusted callback origins, direct/HLS lifecycle isolation. Listener/regression tests, decoded native/Next smoke, troubleshooting samples, TSDuck 87/87, browser 7 passed/3 existing skips and web parity 724/724 pass. ADR explicitly accepted; substantive review passes. Window gates remain open. |
| 111 | `a5937386` | fix: plex sign in (#2990) | Port | upstream-baseline-catch-up | `fix(plex): preserve sources during reauthentication` | ucu-07; bounded fresh-device authentication, stale/sign-out race protection, retained missing-source inventory, scan/health guards, localized refresh/removal UI and live SQL search cleanup. Focused backend/scanner tests, strict lint, hydration lint and six Chromium/Firefox cases pass; substantive review fixes returned-source and running-scan removal races. Window gates remain open. |
| 112 | `c27a8d50` | feat: require api key (#2991) | Adapt (ADR 0103) | upstream-baseline-catch-up | `feat(api): require the ErsatzRS machine API key` | ucu-07; accepted X-Ers-Api-Key, atomic persistent credentials, fail-closed startup and routing, authenticated OpenAPI, separate browser actions and trusted-network/authenticated-proxy management. Key/router/listener tests, strict lint and matched Chromium/Firefox (480 passed, 38 existing skips) pass. Storage-failure browser fixture is isolated; window gates remain open. |
| 113 | `cefd715c` | fix: mirror channels falling out of sync when using the next streaming engine | Covered | upstream-baseline-catch-up | `test(next): verify source builds synchronize every mirror` | ucu-07; existing persisted-build notifications include all dependent Next mirrors independently of source engine. Expanded production scheduler regression covers two Next mirrors, a native mirror, an unrelated Next channel and preserved timeline; seven downstream offset/publication/session checks pass. No production behavior changes. |
| 114 | `ef5094aa` | fix: GHSA-h3r4-r2f2-qf59 | Adapt (ADRs 0095/0102) | upstream-baseline-catch-up | `fix(security): harden internal media authentication` | ucu-07; constant-time callback credentials and query-free sample filenames; accepted private listener and process/session capabilities cover internal URL authentication. Provider-origin confinement, binding/restart/session rejection and sample paths pass 24 focused tests; strict infra/web lint passes. |
| 115 | `49b68314` | prep for release v26.8.1 [no ci] | N/A | upstream-baseline-catch-up | `docs(parity): reconcile upstream v26.8.1 release notes` | ucu-07; only upstream CHANGELOG release heading, Plex/Emby upgrade wording and compare links change. Exact diff reviewed; no executable or package changes. Window acceptance and exact-SHA CI remain open. |
| 116 | `39e39ef6` | changelog update [no ci] | N/A | upstream-baseline-catch-up | `docs(parity): reconcile upstream security release wording` | ucu-08; only upstream release guidance emphasis, labels, ordering, cache/restart and affected-provider wording change. Full diff reviewed; no executable or package changes. Existing UCU-07 security evidence remains authoritative. |
| 117 | `b7aaea3d` | fix: block scheduler deleting playout items (#2992) | Port | upstream-baseline-catch-up | `fix(scheduling): preserve items before future blocks` | ucu-08; clamp cleanup to build start while retaining earlier block boundaries. All 18 block tests and production-builder/SQLite retention regression pass across three modes and delayed offsets; audio/subtitle overrides preserved. Strict Clippy and substantive review pass. |
| 118 | `fcd593e0` | chore: trigger new build for updated next engine | N/A | upstream-baseline-catch-up | `docs(parity): reconcile empty upstream build trigger` | ucu-08; tree equals parent b7aaea3d exactly. No source, runtime pin, package or configuration delta; does not authorize a new runtime build or release. |
| 119 | `2cf8e56a` | update changelog [no ci] | N/A | upstream-baseline-catch-up | `docs(parity): reconcile upstream Next QSV release note` | ucu-08; two CHANGELOG lines describe external Next QSV detection. No source, runtime pin or package changes; existing hardware acceptance limits remain, with no new QSV claim. |
| 120 | `722f1b93` | fix: preserve YAML shuffle order across builds (#2993) | Port | upstream-baseline-catch-up | `fix(scheduling): preserve YAML sequence order across builds` | ucu-08; stable declaration fingerprints and complete-order restoration preserve shuffled default/named lists; nested slots remain intact. All 64 YAML tests and production-builder SQLite restart/oracle comparison pass, including audio/subtitle settings; strict affected lint and substantive review pass. |
| 121 | `06ee45ff` | fix: text subtitle playback (#2994) | Covered | upstream-baseline-catch-up | `test(subtitles): preserve source queries during seek rebasing` | ucu-08; existing ADR 0061 seek cache avoids upstream callback query concatenation. Strengthened HTTP query-preservation and separate FFmpeg seek assertions pass with all 50 selected subtitle tests and strict infra lint. No runtime behavior change. |
| 122 | `34b0d954` | fix: playlist state in sequential and scripted schedules (#2995) | Port | block-scheduling; scripted-scheduling; upstream-baseline-catch-up | `fix(scheduling): restore playlist state through real sources` | ucu-08; parent replay and all-child history preserve counted/play-all continuation. Block deco retains structured children; Lua resolves real SQL sources through the awaited worker. 220 scheduling tests, seven builder tests, SQLite restart/source regressions and strict affected lint pass. Remote Stream checks, audio/subtitle settings and Reset-only block filler admission are preserved. |
| 123 | `b0f313ee` | fix: multiple playlist and marathon fixes (#2996) | Port | classic-scheduling; yaml-scheduling; scripted-scheduling; block-scheduling; upstream-baseline-catch-up | `fix(scheduling): persist shuffled playlist cycles` | ucu-08; nullable history seeds and compatible checkpoint state preserve parent/child cycles, repeated slots and source settings. Shuffled Lua marathons use persisted playout seeds. 228 scheduling tests, 55 repository tests, legacy migration and strict lint pass; real SQL restart matches 100 uninterrupted items. Marathon validation/deduplication and empty/equal/missing-item/filler boundaries are covered. |
| 124 | `b3a35116` | fix: artwork cleanup (#2997) | Port | media-browsers | `fix(artwork): resolve provider images by persisted identity` | ucu-08; typed IDs retained through cards/details/collections/XMLTV; stored-row provider paths and resize, source ownership, bounded redirects and credential isolation, cache guards and valid episode poster output. Focused SQL/HTTP regressions, eight Chromium/Firefox cases, web parity 738 passed/20 existing skips and strict lint pass. |
| 125 | `19b783d7` | fix: authorize artwork when requested from ui (#2998) | Adapt (ADR 0103) | upstream-baseline-catch-up | `test(artwork): verify management and IPTV authorization boundaries` | ucu-08; accepted trusted-network/authenticated-proxy management policy covers UI images; public artwork retains independent optional IPTV JWTs. Four production-router tests prove GET/HEAD admission and streaming exclusions even with a valid machine key; strict lint and source review pass. Row 124 image/browser evidence remains valid. |
| 126 | `fd2171ad` | fix: fallback duration, history and guide group for sequential and scripted schedules | Port | yaml-scheduling | `fix(scheduling): fill the complete YAML fallback interval` | ucu-08; correct a short YAML fallback leaving an uncovered interval. Existing native helpers already preserve fallback history identity and advance unlocked guide groups. Two-engine duration/history/group/track-preference matrix, all 96 YAML/scripted tests and strict core lint pass. |
| 127 | `463a4c80` | fix: disable hdhr controller when jwt is used (#2999) | Port | hdhomerun-http-endpoints; hdhomerun-udp-discovery | `fix(hdhr): disable discovery with IPTV JWT protection` | ucu-08; omit HTTP routes on both listeners and UDP startup under JWT, localize disabled settings and reject crafted saves. Eight Chromium/Firefox hydrated/no-JS cases, desktop/mobile inspection and final web parity (742 passed, 20 existing skips) with strict lint pass. In-order SSR fixes the demonstrated native-form fallback defect. |
| 128 | `165c39ec` | fix: give slow test more time (#3000) | N/A | upstream-baseline-catch-up | `docs(parity): reconcile upstream test fixture deadline` | ucu-08; only upstream NUnit multiday fixture cancellation increases from 10 to 30 seconds. No runtime or assertion changes. Native synchronous scheduling tests do not inherit this deadline; Nextest reporting and zero retries remain unchanged. Full diff/base fixture/runner review and structured validation pass. |
| 129 | `9067cde8` | fix: tag maintenance (#3001) | Adapt | upstream-baseline-catch-up | `fix(scanner): preserve provider-owned tags and refresh state` | ucu-08; accepted ADR 0104 source ownership; atomic producer reconciliation, durable refresh and restart continuity, migration/search/browser evidence in row source map. |
| 130 | `1681d1ad` | fix: channel startup (#3002) | Port | upstream-baseline-catch-up | `fix(streaming): bound startup waits and preserve worker ownership` | ucu-08; identity-checked cleanup, bounded playlist I/O, real HTTP disconnect and native/Next lifecycle proof; see source map and milestone. |
| 131 | `6ebcaf80` | fix: multiday seek | Port | ffmpeg-command-generation | `fix(playback): sample interlace state with bounded source-safe caching` | ucu-08; multi-day argv/motion, selected-stream idet, cache races, HLS Stop, native HTTP smoke and TSDuck 87/87; see source map. |
| 132 | `d5f11315` | fix: more channel startup fixes (#3003) | Port | hls-variant-playlists-segmenter | `fix(streaming): coordinate identity-safe session readiness and recovery` | ucu-08; 28 HLS tests, three real Next lifecycle tests, strict affected lint and native lifecycle smoke pass; see source map. |
| 133 | `4a2aa157` | fix: local folder etags (#3004) | Port | scanner | `fix(scanner): retain failed season scans for unchanged retries` | ucu-08; mixed failure/NFO regression, seven real scan tests, ten local-TV unit tests and strict lint pass; see source map. |
| 134 | `188274e8` | fix: season scanning fixes (#3005) | Port | scanner; media-browsers | `fix(media): reconcile provider seasons and fallback artwork` | ucu-08; ten focused tests, library isolation, filtered-page continuation, matched build, Chromium/Firefox four cases and strict lint pass; see source map. |
| 135 | `5bb3ddfc` | prep for release v26.9.0 [no ci] | N/A | upstream-baseline-catch-up | `docs(parity): reconcile upstream release bookkeeping` | ucu-08; complete diff only changes upstream changelog release heading/date/links; no ErsatzRS release or behavior change. Window gates remain. |
| 136 | `3c41959f` | fix: health check improvements (#3006) | Port | ersatzrs-healthcheck | `fix(health): share bounded health runs and cache results` | ucu-09; one host-owned run survives reader cancellation; five-minute cache, fifteen-minute check deadline, atomic publication, explicit localized refresh and no-JS fallback. 113 infra/maintenance and four web tests pass; strict affected lint and four Chromium/Firefox cases pass. |
| 137 | `96cef081` | fix: heal artwork cache (#3007) | Port | ersatzrs-infra | `fix(artwork): heal cached images and prune unused files` | ucu-09; shared manual/hourly cache service, retained originals/blur/watermark references, local image restoration and shared-path repointing. Native direct-source images recover with unchanged mtime. 25 focused tests, affected all-target Clippy and license check pass. |
| 138 | `7e141a13` | fix: use static hash functions (#3008) | Covered | upstream-baseline-catch-up | `docs(parity): reconcile static hash functions` | ucu-09; complete seven-file audit: native digests and rolling fingerprints are call-local; existing formats and accepted callback protocol preserved. Three hash/ticket regressions pass; row137 and UCU-08 evidence reused. |
| 139 | `2989faeb` | fix: sessions api with next engine (#3009) | Adapt | upstream-baseline-catch-up | `fix(streaming): report native and Next sessions` | ucu-09; ADRs 0095/0103. Typed observational reports, nullable Next cursor, authenticated GET/OpenAPI; preserve native playlist bridge. Mixed-router and process-backed lifecycle tests pass; milestone owns evidence. |
| 140 | `6e9693e9` | feat: add hw accelerated padding for qsv (#3010) | Port | ffmpeg-9-performance-and-support | `feat(ffmpeg): adopt verified v2.0.3 runtime packages` | ucu-09; exact public Linux/Windows checksums, all five QSV padding options, 13 manager tests, strict Clippy and native/Next smoke pass. Intel QSV/AMD AMF/native Linux NVIDIA output remains explicitly approved unverified hardware debt. See source map and milestone. |
| 141 | `8cc34a1e` | chore: update changelog (and get new next build, docker image) | N/A | upstream-baseline-catch-up | `docs(parity): reconcile Next runtime release notes` | ucu-09; changelog-only diff. Exact packaged Next 4cc93e4 retains capability-gated QSV codecs and configurable send_frame defaults; row140 owns runtime integration and approved hardware debt. |
| 142 | `00db622d` | chore: trigger new build for updated next engine | N/A | upstream-baseline-catch-up | `docs(parity): reconcile empty Next build trigger` | ucu-09; empty diff and identical parent tree; no application or runtime change. Row140 integration evidence reused; no duplicate build. |
| 143 | `cbfea01c` | feat: probe dv and hdr10 metadata (#3011) | Port | ffmpeg-9-performance-and-support | `fix(media): refresh and expose HDR metadata` | ucu-09; scoped one-time local PQ/HLG rescan, video-only Media Info fields; 34 migration tests, production projection, real unchanged-file HDR/subtitle preservation, JSON regression, Chromium/Firefox and affected strict lint pass. Existing richer probe/persistence/Next hints retained. |
| 144 | `b30d9670` | chore: update changelog for new next build | N/A | upstream-baseline-catch-up | `docs(parity): reconcile Next optimization release notes` | ucu-09; two changelog lines only. Exact Next 4cc93e4 carries scale/tonemap ordering and guarded QSV fusion; no legacy source or pin change. Existing runtime proof reused; hardware debt retained. |
| 145 | `4c3683ae` | chore: address four TODOs asking for logging and validation (#2964) | Port | mpeg-ts-channel-streaming; xmltv-guide; classic-scheduling | `feat(parity): reconcile external JSON playback and validation` | ucu-09; four source paths audited; transient native playback/guide, Plex diagnostics and marathon fallback implemented. Focused runtime/lint, smoke and 87 TSDuck checks pass; approved hardware debt remains. |
| 146 | `28b2e00c` | feat: graphics canvas (#3012) | Adapt | drawvg-graphics-engine | `feat(parity): reconcile Next graphics canvases` | ucu-09; accepted ADR 0105, shared FFmpeg renderer and private canvas callback; all-kind decoded HTTP/official Next, timing/geometry/alpha, audio/subtitles, lifecycle, stable session reports, strict lint, smoke and 87 TSDuck checks pass. |
| 147 | `b3f7f3fe` | fix: preserve YAML post-roll state across playout builds (#3013) | Port | yaml-scheduling | `fix(scheduling): preserve YAML roll and graphics state` | ucu-09; compatible anchor fields, explicit clears and production graphics lookup; 78 YAML checks, six final regressions with SQLite restart/midnight and imports, strict lint and native smoke pass. |
| 148 | `c673082e` | fix: playout item converter subtitles (#3014) | Port | mpeg-ts-channel-streaming | `fix(parity): reconcile Next subtitle source availability` | ucu-09; provider ownership/caption eligibility, automatic availability, exact/prepared pins, music-video metadata and URI/seek semantics; focused and decoded runtime checks, strict lint and native smoke pass. |
| 149 | `4fffa955` | fix: plex other video tags (#3015) | Port | scanner | `fix(infra): restore Plex Other Video folder tags` | ucu-09; portable roots, owned folder tags, scoped SQLite refresh; provider/restart/search/subtitle/rollback checks and strict lint pass. |
| 150 | `599dc5ea` | feat: show errors over next fallback by default (#3016) | Port | mpeg-ts-channel-streaming | `feat(infra): show Next fallback error reasons` | ucu-09; optional wire contract and enabled default; eight decoded gap/failed-media Burn/Convert/off cases, normal HLS audio/subtitles, host smoke and strict lint pass. |
| 151 | `0840e0fb` | feat: troubleshoot music video credits (#3017) | Port | support-pages | `feat(web): troubleshoot music-video credits` | ucu-09; optional music-only template, both-engine seek/audio/subtitle precedence and archived ASS/template; 762 web checks, strict lint, 14 browser cases, no-JS/mobile review and native/Next smoke pass. |
| 152 | `495e3799` | chore: update changelog for new next build | N/A | upstream-baseline-catch-up | `docs(parity): reconcile upstream Next AMF release notes` | ucu-09; one upstream changelog line only; no source/protocol/build change. Existing runtime integration and approved unverified AMF hardware debt remain. |
| 153 | `0e1b355f` | chore: update dependencies (#3018) | N/A | upstream-baseline-catch-up | `test(parity): reconcile upstream dependency and AAC notes` | ucu-09; NuGet bumps only; announced AAC surround behavior already covered by native and exact Next. Reviewed six-channel argv, stereo/AC3 guards, strict lint and published-runtime decode pass. |
| 154 | `908fa961` | fix: graphics engine motion (#3019) | Adapt | drawvg-graphics-engine | `fix(parity): reconcile motion boundaries and preview clocks` | ucu-09; accepted ADRs 0005/0105; EOF recovery, exclusive Hold expiry, full loops and preview programme clocks. 75 focused checks, native argv, 18 browser cases, strict lint and native/Next smoke pass; published v2.0.3/Next artifacts reused. |
| 155 | `d1c2ab3a` | chore: updates for new next, ffmpeg builds | Port | ffmpeg-9-performance-and-support | `feat(parity): reconcile optional Next AMF adapter protocol` | ucu-09; typed optional uint32 index, automatic default preserved. 14 protocol/config checks, strict affected lint and exact Linux/Windows v2.0.3 checksum verification pass. Approved hardware debt stays unverified; no new runtime build. |
| 156 | `a4642d95` | fix: api response codes (#3020) | Port | upstream-baseline-catch-up | `fix(parity): reconcile machine API errors and safe profile deletion` | ucu-09; typed CRUD/trash/concat status, transactional duplicate/update/delete guards and preserved profile/collection operations; 33 focused checks, strict lint, 15 browser passes and native smoke. Localized island callbacks corrected; no FFmpeg rebuild. |
| 157 | `2bbfbd10` | fix: playback troubleshooting (#3021) | Port | upstream-baseline-catch-up | `fix(parity): retain troubleshooting completion and sample ownership` | ucu-09; immediate native completion, bounded owned capture/archive cleanup, URI credential redaction and smart conflicts 409 with mixed known errors 400. All 53 focused checks, 24 Chromium/Firefox cases and strict lint pass; real sample audio/subtitle preservation. No FFmpeg rebuild. |
| 158 | `f6bd089f` | fix: entity locker concurrency (#3022) | Covered | upstream-baseline-catch-up | `test(parity): verify atomic provider operation admission` | ucu-09; existing mutex/owned admission and transactional Plex generation checks cover atomic flag replacement; Trakt remains deliberately retired. Eight contenders per provider admit one owner; 22 focused checks and strict lint pass. No production or FFmpeg change. |
| 159 | `94c8c014` | fix: scheduler service reliability (#3023) | Port | upstream-baseline-catch-up | `fix(scheduler): isolate maintenance failures without starving jobs` | ucu-09; loop failures preserve independent cleanup/search/health and remain due for retry; cancellation stops the pass and first error remains visible. All 24 maintenance checks, strict lint and architecture pass; see source map and milestone. |
| 160 | `b403012f` | fix: movie custom order ui (#3024) | Port | upstream-baseline-catch-up | `fix(collections): preserve movie ordering across saves and failures` | ucu-09; poster/keyboard order, save exclusion and localized recovery; atomic membership validation and scoped legacy migration preserve row overrides. 13 focused checks, eight browser cases and strict lint pass; see source map and milestone. |
| 161 | `810c1dbd` | fix: font cache thread safety (#3025) | Covered | upstream-baseline-catch-up | `docs(parity): reconcile font cache ownership` | ucu-09; accepted FFmpeg renderer has no shared Skia mapper or loaded-file registry. Nine font extraction/editor/drawtext checks pass; existing decoded evidence retained. No runtime rebuild. |
| 162 | `4e951c17` | fix: mpeg-ts script reliability (#3026) | Port | upstream-baseline-catch-up | `fix(settings): refresh MPEG-TS script choices and recover selection` | ucu-09; shared fresh discovery, form-only fallback, preserved names and complete no-JS form. 19 focused checks, six browser cases, strict lint and native MPEG-TS/HLS smoke pass; no runtime rebuild. |
| 163 | `db5a81e4` | fix: scripted schedule file validation (#3027) | Port | scripted-scheduling | `fix(scheduling): align script validation and execution` | ucu-09; actual-file validation and shared quoted command parsing preserve persistence/reset safety and Lua arguments. 46 focused checks and strict lint pass; invalid waits already fail without clock mutation. |
| 164 | `3f4255be` | fix: many ui bug fixes (#3028) | Port | schedules-templates-decorations-deco-templates | `fix(scheduling): clear dependent choices and stabilize hydration` | ucu-09; full fourteen-file source map, persisted child clearing, saved option display and server-resolved template hydration. Four browser cases and strict lint pass. |
| 165 | `3054e5d0` | fix: more bug fixes (#3029) | Covered | upstream-baseline-catch-up | `test(streaming): verify credits fallback and process ownership` | ucu-09; full seven-file audit. Existing shared credits renderer, typed HLS diagnostics, owned graphics/process cleanup and fresh UI options cover upstream fixes. 18 checks and strict lint pass. |
| 166 | `3d31bfdd` | fix: remote stream yaml extension (#3030) | Covered | upstream-baseline-catch-up | `test(scanner): verify remote YAML extension parity` | ucu-09; native discovery already accepts both extensions case-insensitively. Three discovery/metadata/subtitle checks and strict lint pass. |
| 167 | `5eb9102c` | fix: local library path validation (#3031) | Port | media-source-configuration; library-management | `fix(libraries): validate paths and preserve scanned media` | ucu-09; native path rules, transactional conflicts, identity-preserving saves and localized correction forms. Eleven production checks, two Windows probes, eight browser cases and strict lint pass. |
| 168 | `55885508` | prep for release v26.10.0 [no ci] | N/A | upstream-baseline-catch-up | `docs(parity): reconcile upstream release preparation` | ucu-09; complete diff changes only upstream release heading/date and changelog links. No source/runtime change or ErsatzRS release. |
| 169 | `ee038ba4` | feat: next engine rotation (#3034) | Port | upstream-baseline-catch-up; media-browsers; ersatzrs-healthcheck | `fix(media): carry provider rotation into Next playback` | ucu-09; provider metadata, Media Info and Next 0.0.5. 163 regressions, decoded four-angle/audio proof, two browser cases, corrected orientation health and Next HTTP smoke pass. |
| 170 | `e8b7bb63` | fix: amd vaapi accel when driver is set to default (#3036) | Port | upstream-baseline-catch-up | `fix(next): preserve automatic VAAPI driver selection` | ucu-09; omit Default driver while preserving explicit choices and device. Fourteen config/protocol regressions, strict infra lint and exact accepted Next consumer/health review pass. No AMD hardware-output claim. |
| 171 | `62b7a5e9` | perf: graphics engine subtitles (#3037) | Covered (ADRs 0005/0105) | drawvg-graphics-engine | `docs(parity): reconcile subtitle compositor optimization` | ucu-09; shared FFmpeg/libass filters replace Skia bitmap alpha/cropping and Pipe reader work. Next receives a full encoded canvas under ADR 0105; no Rust pixel compositing. Focused projection check passes; unchanged rows146/154 decoded/runtime evidence reused. |
| 172 | `27f9d217` | fix: jellyfin 12 collections as shows (#3038) | Port | upstream-baseline-catch-up | `fix(jellyfin): preserve collection member identities` | ucu-09; disable collection collapsing across item queries and require expected types. Existing season upsert repairs ShowId despite unchanged ETag; stable identity and track/collection continuity proven by 47 Jellyfin/Emby checks and final repair regression. Strict lint passes; recovery steps documented. |
| 173 | `c0184f00` | perf: watermark improvements (#3039) | Port | drawvg-graphics-engine | `fix(next): publish plain watermarks as direct layers` | ucu-09; shared projection bypasses canvas for plain watermarks in all five callers. 249 regressions, three real decoded runtime cases, strict lint and Next HTTP smoke pass. Exact accepted Next proves RGB opacity; mixed/expression canvas and approved hardware debt remain. |
| 174 | `d7b265e0` | fix: next engine subtitles (#3040) | Port | upstream-baseline-catch-up | `fix(subtitles): apply automatic eligibility to both engines` | ucu-09; automatic selection honors embedded setting and extracted/nonblank text paths before ranking; HLS Direct and bitmap rules retained. 308 affected checks, decoded sidecar fallback and explicit preparation proof, strict lint and Next smoke pass. Existing pinned/ADR 0100 contracts remain. |
| 175 | `04f49b0a` | fix: mpegts script line endings (#3041) | Port | `.gitattributes`; embedded MPEG-TS batch asset | UCU-09 checkout proof | Explicit batch CRLF override; existing global LF covers shell scripts. Both core.autocrlf modes produce correct checkout bytes and normalized LF index blobs. Source map above; no runtime command or platform change. |
| 176 | `6f0449c2` | build: package libvpl (from next) on windows (#3042) | Port | Next stager and both Windows release branches | UCU-09 packaging evidence | Require DLL/license from the verified archive and move beside the executable. Seven offline tests, native ZIP/startup and Bash layout proof pass. QSV hardware debt unchanged; packaging-only archive is not a playback-runtime upgrade. |
| 177 | `b1ed47db` | feat!: drop arm32v7 support; bump ffmpeg (#3043) | Adapt | ADRs 0008/0063; managed FFmpeg 9 and existing six-platform contract | UCU-09 row140/177 evidence | Published v2.0.3 already incorporates applicable expanded patches; exact package hashes and 13 acquisition regressions pass. ARMv7 retained; approved hardware debt stays unverified. Upstream bundled-version and Docker-policy details do not replace the accepted managed runtime design. |
| 178 | `f8410286` | build: fix docker workflow quoting | N/A | Upstream-only digest-list syntax; ErsatzRS uses Buildx Bake | UCU-09 source audit; row176 workflow lint | Complete two-line fix reviewed. No matching broken shell assignments in ErsatzRS; image/channel arguments are quoted. |
| 179 | `5d65c231` | build: immutable releases (#3044) | N/A | Upstream develop-release scripts and workflow policy | UCU-09 full seven-file audit | No product behavior changes. ErsatzRS retains separate CI, tag-triggered releases and explicit beta publication acceptance; no automatic develop publication or retention policy is introduced. |
| 180 | `8bda4411` | build: publish develop builds to separate repo (#3045) | Port | Embedded development release-note selection; upstream repository/tag policies N/A | UCU-09 focused tests, lint and browser evidence | Development home/health show Unreleased; tagged builds keep exact notes. Matched build and two browser checks pass. No GitHub client or legacy Docker-tag warning is introduced. |
| 181 | `76884783` | build: bump ffmpeg to 8.1.2-2 (#3046) | Adapt | ADRs 0008/0063; managed FFmpeg 9 and Next runtime | `fix(next): integrate legacy QSV runtime corrections` | ucu-09; published v2.0.4 at 657482cc, exact public archive hashes, 13 acquisition tests, Linux/Windows decoded runtime proof, migrated production canvas coverage, strict affected lint and Next HTTP smoke pass. Approved hardware debt remains unverified. |
| 182 | `b6666882` | feat: enable mpeg2video when using next engine (#3047) | Port | Next protocol/config, profile health and HLS variants | `feat(next): support MPEG-2 output` | ucu-09; 13 config/health and six playlist checks, decoded Linux/Windows MPEG-2 with selected audio/subtitles and alpha, preserved H.264 scaling, strict workspace Clippy. Eight-bit projection and required bitrate match Next; incomplete CODECS omitted. |
| 183 | `960421e8` | fix: remove av1 from ui (#3048) | Adapt | Native pipeline, HLS routing and output health | `fix(ffmpeg): retain compatible native AV1 output` | ucu-09; accepted ADR 0106 preserves AV1 with automatic fMP4, explicit TS/Next rejection and localized guidance. Decoded Linux application and Windows runtime proof, six AV1 regressions, four browser cases, workspace/parity gates, both-engine smoke and 87 TSDuck checks pass. |
| 184 | `aad7bdc4` | feat: use next copy mode when normalization is disabled (#3049) | Adapt (ADR 0116) | upstream-baseline-catch-up | `feat(infra): support Next copy mode with verified runtime` | ucu-09; versioned independent copy/transcode mapping, codec fallback, overlay health upgrade, six-platform patched snapshot with mandatory upstream retirement. 328 Linux compatibility, decoded RTSP handoff/EOF, six Windows application, Burn/Convert HTTP, 83 TSDuck checks, strict lint and published artifact identity pass. Candidate build CI 37503854054 green; application window CI remains pending. |
| 185 | `e1d2cac0` | fix: cleanup for next copy mode (#3050) | Port | Next projection, profile commands and HLS playlists | `fix(next): preserve copy mode across profile and overlay changes` | ucu-09; editor/API and channel-profile saves notify normal/preview without scheduling changes; copied video skips overlays and Next playlists omit fixed resolution. 18 focused, 63 projection and 17 final fallback checks, decoded copy/non-copy graphics, strict affected lint and Convert HTTP lifecycle pass. |
| 186 | `b4a7efe2` | fix: image and remote stream durations using next engine (#3051) | Adapt (ADR 0117) | upstream-baseline-catch-up | `fix(next): integrate verified image runtime packages` | ucu-09; shared image/remote duration and indexed probe hints preserve selection and scheduling. 18 focused and 64 scheduling checks, all image/AVI hints, optimized Linux/Windows runtime compatibility, six checksum-pinned Next packages and published FFmpeg v2.0.6 pass. Production HTTP playback, Dutch audio/captions, preview/reconnect/idle cleanup and independent review pass. Both temporary Next corrections remain required; final application window CI stays open. |
| 187 | `e18b4a07` | fix: block playout skips and repeats (#3052) | Port | Block scheduling/history and SQLite publication | `fix(scheduling): remove orphaned block history before restoration` | ucu-09; original-clock future orphan cleanup precedes preservation/cursor restoration, removed-start duplicates are deleted including preroll shifts, and exact removal IDs publish within preserving transactions. 33 core Block checks, 10 selected SQLite/transaction checks, complete clean-oracle timeline/track/checkpoint comparisons, strict affected lint and independent substantive review pass. Consumed history/current programmes/seeds remain; final window gates and CI stay open. |
| 188 | `26755a16` | update changelog for most recent next build | N/A | upstream-baseline-catch-up | `docs(parity): reconcile Next QSV runtime announcement` | ucu-09; exact delta adds one upstream changelog line only. Published/pinned Next0362692 already includes separate audio input for legacy fixed-pool QSV+loudnorm+canvas and the 500ms shortest buffer. Exact source/archive provenance, existing QSV argument checks and independent substantive review pass. Intel QSV hardware execution remains explicitly unverified; no new build/runtime correction is required. |
| 189 | `481840fb` | feat: add songs to next engine (#3053) | Adapt | ADRs 0005/0102/0105; shared native song renderer and private owned Next callbacks | `feat(streaming): render songs through native and Next playback` | ucu-09; occurrence-stable visuals preserve original selected audio/subtitles, seek/handoff, mirror and saved-preview geometry, finite black fallback, strict health pins and cancellation/capacity ownership. Linux22 and native Windows20 runtime cases plus actual private HTTP on both pass with exact published packages; complete pipeline snapshots, affected regressions, strict lint and independent review pass. UCU-09 owns source receipts and explicit lint-only deltas. Existing platform/hardware debt and final window gates/CI remain open. |
| 190 | `c13ef9c5` | feat: macos bundled ffmpeg (#3054) | Adapt | ADRs 0008/0063/0117; managed FFmpeg9 acquisition and health | `docs(parity): reconcile bundled macOS FFmpeg policy` | ucu-09; preserve unbundled9, explicit overrides, checksum/capability validation before promotion and validated PATH fallback with visible acquisition warning. Exact nine source hashes and upstream diff verified; 16 applicable runtime-policy regressions and independent review pass. All15 upstream package patches already carried/equivalent/inapplicable; no build/release required. macOS application/runtime proof and unpatched operator-WebP risk remain explicit. UCU-09 owns exact evidence and final window gates/CI remain. |
| 191 | `fdf3b13c` | build: manual develop workflow (#3055) | N/A | Upstream develop-release tagging and .NET artifact/image/signing workflows | `docs(parity): reconcile upstream manual develop workflow` | ucu-09; complete six-file diff and current own CI/release workflow hashes verified. ErsatzRS retains separate verification, tag-triggered releases and explicit stable/beta publication gates; no matching broken develop-tag or signing-environment path. Independent substantive review agrees. No source build, new release or publication-policy change; UCU-09 owns exact evidence and final gates/CI remain open. |
| 192 | `20c81e63` | fix: unsigned mac ci (#3056) | N/A | Upstream unsigned Xcode bundle and develop/CI workflow scripts | `docs(parity): reconcile unsigned macOS CI repair` | ucu-09; full ten-file diff reviewed and current own workflow hashes verified. Xcode signing-flag forwarding and .NET/macOS artifact callers have no ErsatzRS counterpart; independent review confirms no source correction. Existing own CI/tagged releases and approved explicit unverified macOS application/runtime debt remain. No additional build or publication; UCU-09 owns exact evidence and final gates/CI remain open. |
| 193 | `432f107e` | fix: troubleshooting playback seek point (#3057) | Port | ersatzrs-web API, preview supervision and shared player | `fix(web): start troubleshooting playback at the requested seek point` | ucu-09; generated live playlist and current Next marker, successful reaped short-EOF fallback, native two-segment running readiness and diagnostic first-segment player start. 34 focused/831 web and six Linux runtime checks; corrected Windows7/21/5 and both browsers21 each pass, with exact source/reuse identities, strict lint and independent review. Existing owned-file/cancellation/cleanup safety and ordinary live edge remain. No runtime package build or row201 change; final gates/CI remain open. |
| 194 | `fb75ce8c` | fox: local library duration (#3058) | Adapt | infra content-duration projection; scanner persistence, normal scheduling and native/Next consumers | `test(scanner): verify content duration through programme handoff` | ucu-09; ADR0117 narrow Matroska numeric/container conflict guard retains upstream video/complete-audio selection and tag precedence. Existing parser evidence is reused with unchanged identity; new source05 Linux/Windows application tests each pass1/1, normal scan/rescan/Dutch metadata and1s/5s schedule, actual blue0.25/red1.25 handoff, Next6s ENDLIST, packet gaps and owned cleanup. Strict scanner lint, exact tool/source guards and independent review pass. The separate preexisting video-only native silence fallback is resolved by its subsequent non-row checkpoint with independent genuine video-only proof; source05 remains audio-bearing duration evidence. No binary build or later-row change. UCU-09 owns exact evidence and final gates/CI remain open. |
| 195 | `007b3415` | feat: local deep scan (#3059) | Port | Local scanner/config/CLI, durable scan runtime, Libraries confirmation and mutable artwork HTTP | `feat(scanner): support restart-safe local deep scans` | ucu-09; all seven folder families reviewed. Linux/Windows same-mtime scanner/durable restart, identities and Dutch tracks pass; Local TV bypasses unchanged seasons and invalidates prior success before retry. Other families already refresh; managed RemoteStream publication/Unknown authority remains accepted. Nine cultures, SSR/no-JavaScript and keyboard/320px confirmation pass eight browser cases; fresh artwork and old-policy cache migration pass four more. Eighteen artwork regressions, strict lint and independent review pass. UCU-09 owns exact receipts/reused inputs; final integrated Windows artwork and window gates/CI remain required. |
| 196 | `ffa4743e` | build: windows artifact size (#3060) | N/A | Upstream .NET publish symbols and EF bulk-extension dependency | `docs(status): reconcile Windows artifact-size policy` | ucu-09; exact two-file delta and current release/profile/staging inputs reviewed. Rust/SQLx has neither .NET dependency; release strip and explicit named application executable staging exclude build-directory PDBs. Four Rust application executables plus pinned Next/companions are staged; checksum/provenance inspection of the existing Windows Next archive confirms only expected members, no PDB. No runtime correction/build/release required; final archive execution and all window gates remain independently required. UCU-09 owns exact proof. |
| 197 | `39c1811f` | fix: ignore unextracted text subtitles on next engine (#3061) | Covered | Shared custom subtitle eligibility and Next selected-version projection | `test(infra): verify extracted Next custom subtitle selection` | ucu-09; shared filter already excludes unextracted text outside HLS Direct for both engines. Explicit Next/Segmenter SQL/YAML fixture selects extracted English subtitle4/index5 after rejecting the first unextracted candidate, preserving audio1 through actual wire projection. Four focused eligibility/settings/bitmap/pin cases, strict infra lint, source guards and substantive review pass. Selector rustdoc and release overview now state the existing contract. No new sidecar decoding, Windows execution, runtime policy/package or generated argv change; UCU-09 owns exact proof and final gates. |
| 198 | `897a6ef9` | fix: coerce next channels to supported streaming modes (#3062) | Port | Cached Next IPTV transport dispatch and existing HLS session ownership | `fix(web): preserve cached Next streaming URLs` | ucu-09; resolved legacy TS uses the existing hybrid wrapper; Next-first HLS dispatch remains covered. Five portable routing/authentication regressions, strict lint, 26 native Windows routing/artwork tests and actual Linux/Windows TS/reconnect/HLS Dutch audio/caption decoding pass. State preservation, disconnect/preview/restart/idle cleanup and independent raw-artifact review pass; exact published FFmpeg v2.0.6/Next036 reused. UCU-09 owns canonical proof and final gates; no package, browser, hardware or other-platform claim. |
| 199 | `df6ae228` | feat: next output frame rate (#3063) | Port | Next channel/protocol, selected-version sampling and mirror configuration | `feat(infra): normalize scheduled output frame rates` | ucu-09; channel0.1.1 optional output rate matches integer/rational/NTSC behavior and selected exact versions; requested mirror profile controls normalization. Unknown0/0 probe sentinel preserves startup without emitting an invalid target. Eleven focused cases per Linux/Windows plus three actual runtime cases per platform pass: compatible coded-video copy, mismatched transcode, disabled source cadence, requested rational mirror, Dutch audio/Convert/Burn, exact persisted selections and owned cleanup. Strict infra lint, frozen1503 inputs and independent raw-artifact reviews pass. Original failure and measured disabled-only source-clock oracle correction are retained with drift/dropped/duplicate counterexamples. No package change/browser/hardware/other-platform/fullCI claim; UCU-09 owns exact proof and final gates. |
| 200 | `a3bbf89d` | feat: next engine by default (#3064) | Adapt (ADR 0095) | Managed Next runtime, channel/preview defaults and localized profile applicability | `feat(infra): acquire the pinned Next engine before startup` | ucu-09; accepted conditional Next default with exact public checksum-pinned bundled/downloaded runtime before composition, persistent offline reuse and strict overrides. Preserve stored Native/AV1/engine/profile choices without bulk migration. Six anonymous RID archives/staging, Linux acquisition/default/health and current lifecycle/HTML tests,38 native Windows ordinary tests, actual Linux/Windows cold TS/HLS/Dutch tracks/reconnect/preview/idle cleanup, native bundled playback and six read-only startup cases per platform pass with independent reviews. Matched Chromium31/Firefox30 pass with one unchanged older skip; new no-JS/320px and causal diagnostic cases execute. Historical failures retained; final integrated/window/exact-SHA CI and approved hardware/ARM/macOS debt remain. |
| 201 | `33cabcf1` | fix: next channel troubleshooting (#3065) | Port | Next preview sessions, private callbacks and selected-profile projection | `feat(infra): complete historical Next channel previews` | ucu-09; Complete30s historical windows, configured gaps, mirror clocks, transient selectors, per-occurrence callbacks and protected Dutch tracks/credits. Linux118/Windows116 ordinary checks and26 named regressions each pass. Seven actual wrappers per OS cover nine complete decoded windows plus Stop; HTTP/RTSP/installed Live/Finite controls, state/revocation/cleanup and lint/format pass. Canonical proof f418022b; only failed-fixture assertions and formatting were corrected. Published FFmpeg2.0.6/Next036 reused; hardware/platform debt remains unverified. |
| 202 | `6c7c4e69` | build: release from a pushed tag (#3066) | N/A | Upstream .NET/calendar release tooling | `docs(parity): reconcile tag-driven stable release workflow` | ucu-09; Exact prep-release/workflow delta reviewed. ErsatzRS already builds stable releases from pushed tags with its own staging/bundle gates. Upstream calendar/.NET/signing and draft dry-run tooling are not copied; no exact upstream dry-run parity claim. Final application CI and authorized stable publication remain separate. |
| 203 | `4c03f347` | docs: prep for release v26.11.0 | N/A | Upstream release notes | `docs(parity): record the v26.11.0 boundary` | ucu-09; CHANGELOG heading/date/compare links only; no application or runtime amendment. |
| 204 | `9e537c1b` | fix: next playout folders on windows (#3067) | Covered | Real channel-owned directories and atomic stable JSON publication | `test(infra): verify native Windows publication and restore safety` | ucu-09; No current-junction/version-folder scheme exists. Linux14 unchanged regressions and native Windows16, including two actual held-handle cases, pass; real stale current-directory loader control passes with row201. Interrupted replacement, deadline/cancellation, old-file retention and staging cleanup are proven. Proof3c338a0d retains original format failure and corrected Linux/Windows formatting with exact source reuse; no production/runtime patch. |
| 205 | `2d79d196` | docs: update changelog for next fix | Covered | Pinned Next QSV loudnorm/fixed-pool buffering | `docs(parity): reconcile the included QSV shortest-buffer fix` | ucu-09; CHANGELOG references actual Next e88b2d7 QSV+loudnorm correction, not row204. Exact upstream comparison proves e88b2d7 is an ancestor of published0362692 (ahead1/behind0). Accepted package/provenance and argument evidence remain valid. Intel QSV hardware execution remains approved unverified debt; no new FFmpeg build. |
| 206 | `d55be01f` | docs: tweak changelog | N/A | Upstream release-note wording | `docs(parity): reconcile Windows playout note wording` | ucu-09; Clarifies the row204 symptom only; source and acceptance are unchanged. |
| 207 | `47d7716a` | docs: update changelog for next fix | Covered (ADR 0117) | Next036 and native VAAPI driver/color/depth prerequisites | `docs(parity): accept verified VAAPI padding prerequisites` | ucu-09; Pinned Next036 includes iHD BT.2020 retag/restoration and AMD/i965 rejection. Native explicit-iHD eligibility, software fallback, effective BT.709/HDR state and P010 composition preservation are already corrected. Reuse21 focused checks, four owning snapshot groups in231 passes and four decoded software range/depth cases. VAAPI execution remains explicitly unverified hardware debt; no new FFmpeg binary amendment. |
| 208 | `46118d43` | docs: prep for release v26.11.1 | N/A | Fixed upstream v26.11.1 release boundary | `docs(parity): finish ordered catch-up through v26.11.1` | ucu-09; CHANGELOG heading/date/compare links only. Local and live remote tag agree at46118d43313b7828780fbfcfb46e980390ea8b0d. All208 rows are dispositioned; later upstream changes remain outside UCU-09. Final window gates and exact-SHA CI remain required. |
