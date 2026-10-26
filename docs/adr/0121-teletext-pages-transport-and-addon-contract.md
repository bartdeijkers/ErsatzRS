# ADR 0121: Define teletext pages, transport and the add-on contract

## Status

Accepted (2026-10-10) as the technical implementation contract for the
maintainer-authorized Phase 9 engine-first programme in ADR 0120. The selected
delivery is genuine DVB teletext for MPEG-TS clients plus an authenticated
browser viewer. The separate TV-guide add-on still starts after P9A-04.
ADR 0123 extends the authored target through Level 2.5 and adds external-editor
interchange; it supersedes the Level 1/1.5-only target below, not the other gates.

## Implementation

Partial.

The SI1 source-inventory component retains bounded transmitted page occurrences,
raw body/context units and original subcodes/controls/positions alongside the
existing header observations. Page/magazine declarations preserve their actual
provenance; absent receiver defaults remain unknown. A separate version-1 child
uses the same source binding, scan transaction and lease. Loading and writing
replay the bounded wire receiver to reject inconsistent derived facts. Capacity
exhaustion clears optional body evidence without promoting headers or altering
ordinary tracks. Closed baseline acquisition facts are neither decoded display
completeness nor service/selection/playback authority. Current qualification and
remaining inventory work belong to `p9a-source-inventory` in the progress records.

Shared native preparation now uses existing configuration and EntityLocker
admission independently of host/UI state. It releases preparation work before
playback, rechecks current candidates and filesystem association before opens,
and transfers the receipt with actual playback admission into each consuming
child. Reaping, cancellation and quarantine follow that actual owner. Finite
Local-source legacy/Hybrid caption flows are proven in P9A-04. Current whole-window
qualification is temporary incomplete coverage, not an accepted long-source or
seek restriction; general cue/seek semantics and the full source matrix remain.

Read-only source-caption candidates now bind the exact item, version, file and
original locator to existing current-parent validation. Raw PMT descriptors keep
wire tags/ordinals, page language, subtitle roles and all actual programs. Only
observed decimal pages with descriptor roles 2/5 become candidates; header C6,
newsflash or a descriptor alone never grants readiness, Default or Forced.
A qualified candidate must match the actual decoded request and stable PSI.
The shared native preference selector can rank those page languages beside
ordinary subtitles while retaining real metadata and admitted work in the
winning rich result. Ambiguous pages/services/roles are excluded; ISO language
aliases for the same exact choice are equivalent. Native Burn cannot fall back
to an extracted teletext row without exact-page qualification. Direct retains its
existing ordinary behavior and receives no qualified source choice. Off returns
before SQL. P9A-04 owns verification. Current database/file revalidation and
per-child ownership now have shared runtime adapters; their ordinary route and
seek/cue acceptance, pins/custom selectors and the full output matrix remain open.
No installed package or stored readiness state changes.

Preparatory native source-page decoding retains one held Local filesystem
association through bounded owned source checks and the existing managed probe
worker. Its explicit request keeps durable parent/revision labels, global stream
index, PID, program and exact page separate. A completed private TS188 window now
contains only bytes read from that held file. The same captured bytes pass stable
current PSI, explicit program/PID/type-6 membership and selected-PES validation
before they are decoded. Selected packets before the first post-PSI PES start are
omitted explicitly. A capped incomplete selected PES is trimmed to the last
complete PES; otherwise the complete nonselected AV/PCR tail remains in the
same decoder bytes. Late current table
changes anywhere in the captured source reject even when trimming would hide them.
The final decoder bytes are independently rechecked and hashed. The native owner
retains the temporary input through actual process/pipe cleanup; uncertainty
preserves the bounded input and quarantines admission. Field disposal releases
input/source resources before work capacity and scope completion.
Returned decoder stream/program facts must still match and at least one nonempty
bitmap event must exist. Missing PTS stays unknown; automatic region does not
resolve charset provenance or promote stored HeadersObserved to Ready. P9A-04
owns evidence. Explicit bounded packet spans now reuse this same held-handle
capture and actual decoder owner. Their checked offsets must fit the source;
the selected range supplies its own current PAT/PMT and complete selected PES,
without copying authority from the earlier scan prefix. Receipts distinguish
the absolute first source packet, all inspected packets, omitted selected prefix
and trimmed tail from actual decoder bytes. Reaching EOF at a nonzero offset
never establishes whole-source byte coverage. The whole-source byte predicate
retains the existing automatic-selection fence; an explicit-span bitmap receipt
alone grants no playback seek or temporal authority. Raw clock epoch and actual
cue coverage remain required before automatic interval selection.
Both actual native consumers retain the existing request's exact media-relative
seek and finite duration, including subsecond precision, through source ownership
and retry. Unknown duration, an empty interval or overflow supplies no finite
interval; this never manufactures source-clock evidence or rejects ordinary AV.
Raw PCR/PES clocks, caller media-relative time and native effective timestamps
are distinct. In particular, FFprobe's default teletext-PTS correction can alter
the wire PTS, and the first PCR is not the media origin. Qualification must bind
the observed AV presentation origin and actual cue to a verified source-clock
epoch; matching PSI fingerprints or modular PCR differences alone cannot prove
an unobserved interval has no reset. Do not change native timestamp correction
or promote caller intent to temporal authority without that evidence.
A preparatory constant-memory walk now observes every TS188 packet from zero
through EOF on the existing held handle under its actual work owner and deadline.
Explicit PSI continuation resets only the bounded batch counter; table fragments,
continuity state, CRC validation and sticky failures/change evidence survive.
The selected program/PCR and complete current PSI must match the decoder receipt.
The same walk reconstructs and checks its retained decoder hash, including prefix
omissions and trimmed tails, before combining old bitmap and new raw evidence.
PCR syntax/order/wrap/discontinuity checks share syntax validation with the
unchanged authored CBR policy; no muxrate is inferred for source observation.
These are historical raw observations, not bounded-PCR-gap, AV-origin, cue-end,
subtitle-PTS-correction or presentation-EOF proof. General restored in-place/ABA
exclusion is not implied. Automatic later-window selection remains fenced until
actual native media origin, cue bookends and page-header evidence are joined.
Native format start and every AV stream's global index/PID/role/start_pts/time_base
now have a separate bounded exact projection. Unknown timing stays absent;
negative starts, reported decimal precision and positive rational time bases are
preserved without floating point or changes to ordinary metadata. A conservative
one-printed-quantum enclosure is observational only, not an audited rescale or
origin certificate. A retained original-source native probe shares the existing
work/source/process owner and absolute startup deadline; the later private
bitmap snapshot cannot supply native media zero.
The same held-source walk joins those identities to bounded MPEG-2 PES headers,
including split headers, strict timestamp prefix/marker bits, continuity/duplicates
and independent raw PTS/DTS. AV payloads and entire PES packets are not buffered;
video PES length zero and PTS reorder/wrap do not manufacture new origins.
Actual PSI must advertise each native AV PID. Exact native-start rescaling/modulo
matches retain their source header positions, without certifying unwrapped epochs.
Selected-program AV/PCR epoch agreement, bounded gaps, requested cue coverage and
current cue-window page headers remain required before automatic admission.
A prepared-source owner now acquires native timing before any page/window
bitmap decode, retaining the original held association independently of prefix
page observations. Explicit later-window decoding reuses that same owner and
requires the requested page's actual protected header in the exact decoder bytes
before starting the bitmap process. Header completion positions are relative to
that filtered/trimmed snapshot, not media-relative cue positions. Preparation
retains no short work admission after actual preparation cleanup; active workers
retain admission and source ownership after requester loss. Native command paths
must be representable without lossy substitution, matching the held association.
This is a preparation/window seam, not automatic temporal admission: the native
program wrap interpretation, selected-program AV/PCR epoch, bounded clock gaps
and requested cue bookends still require qualification. A nearest modular
AV/PCR match alone cannot resolve a common whole-period ambiguity. Neither
printed native decimal precision nor raw subtitle PTS supplies that proof.
The source-associated native demux trace therefore retains all stream identities,
program order/memberships and bounded native packet order, signed PTS/DTS and
optional physical positions. It starts without a seek or stream filter and uses
`-nofind_stream_info`, `noparse+nofillin` and explicit overflow correction through
the existing retained process owner. The independent native format-origin
probe retains ordinary stream-info behavior, with explicit positive probe size
and overflow correction. Disable stream-info only for this explicit trace: the
normal duration estimator can flush its queue, seek near EOF and initialize wrap
state there before seeking back. A clean trace must not silently certify the
normal consumer's initialization. Missing positions or timestamps stay unknown.

The pure fixed-33-bit/90-kHz transform follows FFmpeg n9.0's DTS-first initial
reference, exact add/subtract thresholds and native program/default inheritance.
PCR conversion retains its extension using that same candidate program rule;
nearest-modular AV/PCR offset is not substituted. Actual first-demux identity,
all memberships, shared-state propagation, raw timestamp/position witnesses and
normal-consumer equivalence remain preconditions, never authority granted by
constructing a candidate. Non-AV initializers cannot silently become first AV.
The audited primary paths are [wrap initialization and propagation](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavformat/demux.c#L441),
[timestamp transformation](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavformat/demux.c#L48)
and [duration estimation](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavformat/demux.c#L1733).
Installed n9.0-20260805 receipts corroborate tested behavior; its version string
alone does not identify an exact custom-build source commit. Full cue qualification
and ordinary playback behavior still require independent proof.

The owned raw reader can now construct opaque default-probe epoch evidence for
the corroborated single-program/AV-first case. Require the first GLOBAL native
packet, clean explicitly observed packet flags, exact global/program/native AV
inventories, current stable PSI before its raw PES start, matching raw PTS/DTS,
all signed native AV starts, and continuous selected PCR coordinates with retained
extensions. Native format identity must be observed as MPEG-TS. Unknown flags,
first non-AV/untimed packets, partial inventories or contradictory witnesses grant
no epoch. This branch is implementation progress; other initialization cases
remain required acceptance work rather than permanent product exclusions.
The MPEG-TS `AVFMTCTX_NOHEADER`/positive-probe and demux FIFO path initializes the
first accepted packet's program before duration-tail probing. Ordinary generated
inputs also use `genpts+discardcorrupt+igndts`: corruption can drop that first PES
before wrap initialization, so a corrupt clean-trace initializer cannot certify
those consumer flags. Discard disposition is conservatively unknown here; no
claim is made that ordinary wrap initialization skips discard packets.
The primary paths are [initial stream-info packet consumption](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavformat/demux.c#L2563)
and [packet corruption and initialization ordering](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavformat/demux.c#L533).
This default-probe evidence still grants no arbitrary consumer policy, immutable
source certificate, native media zero or cue admission. Qualify actual consumer
options and format-origin bounds separately. Preserve exact requested time and
separately parsed effective `-ss`/`-t` microseconds: floor seek and original
duration independently, then add them, rather than flooring the requested end.
Empty effective durations and signed-range overflow remain unknown. Native
origin enclosure, bounded PCR gaps, effective cue bookends, current page headers
and the exact decoder-byte association remain required before later-cue admission.

The same held walk can provisionally locate requested native-FORMAT cue
bookends using the audited n9.0 plain six-decimal writer enclosure, independently
parsed effective seek/duration and actual selected-PCR observations. Use the last
PCR at/before the conservative start and first PCR at/after the conservative end;
never interpolate by byte rate or substitute native seek hints. Explicit packet
and tick gap limits fence missing observations. Publish an opaque receipt only
after native epoch/PSI/AV/source checks succeed. It retains the exact source/page
request, and later decoding must rejoin its full-source digest and the retained
decoder-byte hash through the existing owned raw walk. Every captured span still
requires its own current PSI, complete selected PES and actual requested header.
The bounded-span mechanism is implementation progress, not an accepted duration
limit or general multiwindow solution. Modulo gap bounds alone do not exclude an
omitted whole period. Writer magnitudes/precisions outside the audited numerical
enclosure remain unknown, not a permanent product restriction.

Unqualified descriptor discovery is explicitly distinct from strict observed-
header candidate loading. It retains exact file/version/stream ownership,
current stored PSI/descriptor provenance, ambiguity and capacity checks, while
allowing descriptor pages absent from the scan prefix. It never writes readiness
or substitutes a descriptor for a transmitted page. Only actual current-window
validation can bridge such a request into a playback choice; later rechecks must
preserve that distinction. A PCR-bounded span can omit earlier-arriving PES.
The same owned reader now runs one bounded existing teletext PES assembler,
retaining actual selected PUSI/completion positions, protected requested-page
C4 reset headers and native-epoch-mapped PES PTS. Keep the latest preceding reset
across incremental updates, or the first in-range future reset when no preceding
requested-page state was observed. Unknown prefix/PTS/epoch, decreasing selected
PTS or incremental state without a known reset grant no prelude; duplicate PUSI
must not replace the real PES start. Extend the decode span backwards through
actual preceding PAT/PMT locator bytes while keeping the original PCR bookends.
No arbitrary time/byte preroll or copied PSI can supply authority. Fresh snapshot
PSI/PES validation must retain the exact anchor header and raw PTS at its mapped
physical completion ordinal after actual prefix omissions; a later identical
header cannot replace it. The existing decoder/full-source hash join still
applies. Missing/over-bound prelude evidence remains unknown, not a permanent
product restriction. This reset/PES evidence repairs the tested first bitmap;
complete page bodies, subpage/resource/charset context, native decoder timestamp
mapping and generic initial-state qualification remain required work.
Actual consumer policy and enabled-stream origin also remain separate: n9.0
[correct_input_start_times](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffmpeg_opt.c#L345)
can change normalization when the earliest audio is disabled or another stream
is selected. Neither native format origin nor a successful partial window grants
that consumer's complete subtitle presentation interval.

Do not weaken global scan-header qualification to bypass the remaining exact-
source candidate path; connect descriptor-only discovery only with independently
qualified current cue-window headers and the existing selection checks.
This bounded byte window does not prove later playback
seek/cue coverage, whole-source inventory, current database ownership or future
source opens. Source bookends remain observed associations, not immutable-byte or
restored-in-place-mutation certificates. Actual route/seek ownership, orphan
cleanup after uncertain-process restart and full source/platform acceptance stay
open; these are not accepted permanent product restrictions.

Accepted frozen originals now have an explicit source-independent output intent.
Host limits contain no program/PCR identifiers or compiler defaults. Each final
command qualifies rate/period bounds before actual-source PSI supplies those IDs;
the binding retains the same shared native service. Application confirmation
rechecks accepted identity and effective channel selection before injectable
source reads. Known unsupported commands/policies preserve ordinary output
without waiting for optional generation storage.
Complete recovery is bounded across repository awaits by the original absolute
deadline. The same native deadline now reaches service/resource/display,
normalization and packet-assembly checkpoints. Timeout returns no partial
candidate and maps to the existing host timeout; untimed standalone callers and
encoded identities retain their behavior. Checks are cooperative between bounded
stages, not mid-stage preemption or a CPU/RSS bound. An explicit
optional whole-service quota now debits actual admitted display-resolution updates
across original, physical and transmission-settlement passes, including failures.
Usage is separate from model/wire identity; disabled metering keeps prior stage
limits. Separate optional glyph-lookup and selected-display comparison credits
now share that service context and retain failure usage without changing model,
wire or artifact identity. Planning, placement, assembly and ancestry also retain
separate admitted service credits; truly empty work may fit zero credit. Original
and supplied-program collection, both spatial count/emit passes and carrier-ID
scans/probes now share additional independent credits. Failed recipes retain
actual admitted identity usage. Attribute cell projections/writes also share
credit across original/physical execution and all transmission settlements.
Scans, snapshots, private state and observations remain outside that meter;
catalogue indexing/graph validation and other native admission accounting
remain open. Conservative reservations
are charges, not executed CPU/RSS costs. Measured production host policy remains
open; P9A-04 owns proof and the resource-rich measurement scope. Input/composition
producer limits now intersect before request preparation/invocation, with defensive
native checks before validation/lifting. Saved reload restricts the existing
bounded document-count decoder before native original parsing. Rejection retains
immutable revisions and accepted output; bounded storage reads/outer scans remain.
Native channel/base/bound preparation, original save/reload, pending publication,
acceptance/recovery and source fingerprinting now use immediate process-wide
admission through the existing application coordinator and core EntityLocker.
A private shared lease survives requester cancellation in actual package-resolution
owners and queued/running native closures, including early cleanup-uncertainty
replies. Only final owner release makes the slot available; nested stages share
one admission and the original deadline. There is no teletext admission queue or
use of ordinary playback reservations. Genuinely unconfirmed native process
termination quarantines its slot until host restart, following the existing
transcode retention pattern. An early resolution uncertainty reply alone does
not quarantine after the actual resolver conclusively joins. The existing ConfigElement key
teletext.work.max_concurrent must be a positive integer; absence conservatively
serializes these operations. Lowering it affects new work without preemption.
This bounds admitted operation count, not retained candidates/active services,
peak RSS or complete platform capacity. The resource-rich 100-number workload
is an experiment, not production compiler defaults or full engine acceptance.
Retained late construction preflights gate/journal/probe overlap and
charges actual retained-prefix capacity before allocating its validation gate.
The existing optional pumps retain original ages, reserve ownership and exact
pre-write fallback. Ordinary final-direct and generated native final-wrapper
MPEG-TS lookup now opt in only through
the existing ConfigElement key teletext.output.policy. Its closed version-1 JSON
requires every design, validation, input, composition, encoding and source limit,
including all thirteen service-work credits. There are no production defaults.
Saved originals supply entry, resource lifecycle and V1 region; configuration
cannot replace them or invent source program/PCR identities. Recovery never
invokes packages or accepts pending drafts. Configuration access and recovery
together receive at most half of the remaining original twenty-second startup
deadline, additionally capped by the configured positive recovery timeout.
Missing/invalid policy, unavailable/stale selection, exhausted credit or busy work
preserves ordinary playback. Internal workers, slates, other containers and
opaque configured-script commands skip automatic recovery. The generated final
MPEG-TS wrapper uses the same bounded optional recovery for native and Next
upstream engines: its typed host-owned mux contract, not the producer engine,
authorizes post-wrapper carriage. Next HLS workers and direct/other-container
routes do not gain authored DVB output. Generated wrappers recover only after
actual capacity and command admission; an explicit
empty ffmpeg.default_mpegts_script selects the generated hybrid path. Default/custom
script support remains an integration gap, not an accepted permanent restriction.
The existing Health view reads this policy afresh and
reports configuration validity, not delivery or complete authored readiness.
Production compiler limits, ongoing publication ownership and complete client
acceptance remain open; P9A-04 owns the scoped ordinary-route receipt.
The explicit direct route now associates the accepted intent with its resolved
channel and actual child command, refreshing the snapshot after each successful
hardware retry. Terminal authored supervision also covers optional raw fallback.
Accepted routing rejects retained/hybrid sources and capacity-slate joins until
actual first-byte attempt provenance is implemented; existing fixed/plain routes
retain their behavior. A real accepted-original/reopen/AV transport capture and
headed representative palette/glyph/page-navigation trial are recorded in
P9A-04. They do not certify accepted-specific retries, whole-process restart,
ongoing automatic publication or full Level 2.5/client coverage.
This preparatory explicit API does not waive full authored/editor/Lua/admin or
platform acceptance. P9A-04 owns the canonical evidence and next integration step.

Explicit source-discovery bindings now feed the same direct/retained gate from
bounded actual PSI. A checked owned prefix preserves the original source epoch
and oldest observed arrival, including partial transport tails. Revalidation uses
a fresh gate, never replays a sealed monitor, and does not renew first-byte,
holding, pacing or reserve-startup deadlines. Setup failure also preserves exact
consumed bytes. Probe and journal limits are independent; a retained journal must
cover its pre-write reserve. Smaller processing windows charge that journal within
the existing32MiB requested-allocation ceiling without changing cadence, EOF or
cycle checks; strict default callers retain their old cap. No transport defaults
or normal-route accepted-generation activation are supplied by this interface.
P9A-04 owns the scoped proof and remaining integrated flow gaps.

Generated final-MPEG-TS commands now carry rate/PCR-period hints bound to exact
argv; opaque commands do not infer them. These are transferable data, not runtime
or source authority. Complete current PSI can select its sole programme without
guessing program/PCR IDs; ambiguous services require explicit selection. An
explicit bounded optional-startup policy preserves every consumed raw byte and
the continuing source if authored admission rejects before the first sink-write
attempt. Afterwards the epoch remains strict; no raw fallback can mix with emitted
authored packets. Direct and retained final-output dispatch share this policy,
with existing bindings strict by default. Ordinary accepted-generation lookup,
active-publication Health and warm ownership remain open.
P9A-04 owns verification; the reference documents the preparatory interfaces.

Preparatory context fingerprint version2 now includes the exact bounded accepted
External JSON document SHA256, before parsing/filtering. Version1 retains its exact
historical semantic framing and remains readable; pending admission replays the
stored version without fallback or silent upgrade. Raw hashing work is bounded by
admitted source bytes; framing limits cover only the retained32-byte hash frame.
Neither version grants activation authority. Accepted-by-playout-ID selection and
its retained configured/accepted paths match ADR0110 playback preservation; path
inequality alone is not a source defect. Legacy live files, projected-away SQL facts,
current guide roles and durable accepted-generation ownership remain open.
P9A-04 owns the implementation receipt and remaining integration proof.

The optional live-output proposal seam now requires a fresh receipt for the exact
sink-flushed window and its actual end-of-cycle boundary, not historical cycle
completion. Shared replacement preflights carousel/resource and PMT ownership
before an infallible paired commit. Descriptor changes, withdrawal and re-enable
advance the emitted PMT version without resetting source assembly, output continuity
or AV/PCR positions; changed signalling is sink-acknowledged before new authored PES.
One bounded watch slot retains the latest proposal, and reports observation for one
opaque output epoch only. It does not certify all outputs or durable publication.
Unreconciled retired M29/MOT banks reject nonterminally while the old stream continues.
This preserves a known decoder invariant, not an accepted permanent authoring limit.
Full warm reconciliation, logical immediate-disable timing, freshness and accepted
generation recovery/ordinary-route ownership remain required. P9A-04 owns evidence;
the interchange reference documents the preparatory API.

The preparatory guide/native bridge now rejects changed effective guide context
before and after producer preparation, with cumulative read limits and the original
deadline, including exact stored configured/accepted source-path associations.
Original bounded V1/V2 request buffers now move through validation into the native
candidate and persist byte-exact in an optional, independently versioned private
storage subdocument; legacy envelope/version-1 records remain readable. A bounded
version-1 fingerprint covers the complete compared guide context without cloning or
JSON buffers. These facts do not validate configured-to-accepted path ownership,
fence live-file/projected-away raw changes or an intervening change and restoration.
Native source-bound capture now retains an optional versioned context association
with ordered source IDs, exact three-civil-date bounds and host capture/locale/clock
metadata. Storage validates its association with the exact original guide requests.
Source-aware pending admission compares the stored association and recaptures the
historical semantic context inside the same immediate transaction before intent
insertion, under fresh trusted limits/deadline. Static admission cannot discard a
present association. This does not establish configured-to-accepted ownership,
current Now/Next accuracy, mutable-file/projected-away raw freshness or accepted
generation authority. The interchange reference and P9A-04 own API details, proof
and remaining integration gaps. Stored metadata is historical data, not an
invocation receipt, editor permission or generation-activation authority.

The native command component now carries one explicit source page on its selected
logical stream and applies bitmap decoder options to the first matching physical
input, preserving a shared one-shot AV read. It follows the existing single
bitmap-caption selection; unused later inputs cannot replace the chosen page.
This does not promote report-only source descriptors into playback selections.
P9A-04 owns package proof; scanner revision/lease binding and routed selection
remain prerequisites. The accepted output matrix below is unchanged.

The selected-runtime capability report now runs bounded decoder-option,
exact-page text and colour bitmap probes against an independent synthetic DVB
fixture. Decoder outcomes have localized Health guidance and runtime-bound
troubleshooting export. Domain drafts now retain checked page/subpage identities,
typed text/mosaic runs, attributes and navigation, with structural validation.
The FFmpeg crate now encodes bounded Western Latin page packets, including
physical spacing controls, X/26 character enhancements, X/27 editorial links
and X/28 character-set designation. Independent FFmpeg decoding verifies exact
accented text, all seven national repertoires, boxed captions and colour output
on Linux x64/Windows and emulated ARM64/ARMv7. Other regions, older-receiver
compatibility and editorial-link CRC/client acceptance remain unproven.
The bounded serial carousel now plans 20 ms erase/packet intervals, uses reserved
mFF termination and permits set replacement only at a cycle boundary. Its PES/TS
packetizer preserves exact PTS, structured stuffing and continuity across cycles.
Independent decoder and TSDuck checks exercise this packetization in calibration
PSI/PCR. PMT planning now validates complete sections, preserves existing
metadata/PCR and adds service descriptors; bounded null-slot replacement fails
without changing packets or counters when preflight rejects input. It uses
mpeg2ts-reader for packet/PSI/descriptor parsing and MPEG CRC. A finite real
FFmpeg audiovisual fixture proves unchanged AV/PCR packets and decoded frames/
timestamps with independent TSDuck signalling checks. Bounded source discovery
now assembles split/multiple PAT/CAT/PMT sections, waits for all advertised
programs, checks their PID references and fences malformed input/continuity
loss. It derives the selected PMT from an explicit PAT program; a two-program
fixture preserves both audio streams and the other program's PMT. A small
framing adapter covers the locked library's split-header/multiple-section gaps;
field interpretation and CRC remain library-owned. Bounded PMT installation
now handles growth/fragmentation, preserves exact admitted source bytes and
preflights all capacity before mutation; source and output continuity remain
independent across successful windows. A 33-page fixture proves growth from one
to two table packets and repeated counter wrap without changing AV/PCR or the
other program. Continuing raw-source monitoring now carries admitted PAT/CAT/
all-program PMT metadata and input continuity across bounded windows. Changed
tables, new owned-PID use, malformed sections and incomplete EOF permanently
fence the generation; split sections require the owner to hold output. An
independent review identified an admission duplicate-validation gap, corrected
by rejecting orphan PSI after a complete section establishes framing. A
constant-memory clock checker validates declared CBR against selected-program
PCR and all physical packet positions, with explicit owner-selected tolerance,
gap and readiness limits. It handles PCR wrap and permanently fences syntax,
order, gap or rate failure. The finite fixture checks unchanged clock evidence
and rejects a removed null slot. Bounded TS188 byte retention now preserves
first-byte age across arbitrary reads, incomplete packets and prefix release.
An output gate composes source admission/monitoring, PCR/CBR readiness, growing
PMT installation and atomic carousel insertion before exposing a complete
physical 20 ms window. Prepared and raw bytes share one capacity bound;
immutable schedules and epoch-bound acknowledgements fence undelivered,
stale, early or late output. Idle polling enforces holding and delivery limits,
including exposed windows. Impossible minimum-window capacity/timing policies
are rejected at construction. The real two-program fixture proves two complete
33-page cycles with bounded incremental input and simulated monotonic sink
acknowledgements, preserving AV/PCR packets, decoded frames/timestamps and
independent teletext/TSDuck results on the same four runtimes. It does not prove
asynchronous pacing or actual sink/client delivery. Actual source EOF now seals
input and drains all complete final transport packets at their original physical
positions and due times. Only complete 20 ms intervals receive carousel batches;
a fractional interval adds no batch or padding. Incomplete transport/PSI,
unavailable clock/capacity and an initial incomplete carousel fail visibly.
Cycle completion is recorded only after actual writer flush and acknowledgement.
An immutable generation erases pages in its first cycle; repeated headers omit
C4 while retaining subcode, all other flags and the same spacing. Replacement
restarts first-cycle erasure. This protects previously acquired content when a
later repeat ends, without establishing late-join, packet-loss or final client
cache acceptance. Orderly producer handover remains implementation work. A native host pump now forwards a borrowed encoded
source to its borrowed response writer without spawning a task or reopening
media. Bounded reads respect shared capacity; larger complete intervals may
retain a fragmented PMT while admission remains atomic. Absolute per-packet
schedules intersect first-byte holding limits and delivery expiry. Every read,
write and flush is bounded; actual flush precedes the exact epoch/window
acknowledgement. Post-I/O time checks reject late ready completion as well as
timer expiry. Dropping the future abandons held data and pending I/O; its caller
still owns producer termination/reaping and response closure. The gate exposes
bounded raw capacity, complete interval endpoints and earliest deadlines for
this composition. The existing stdout supervisor now receives an immutable
binding through explicit final direct/wrapper roles; internal item streams,
capacity slates and HLS workers cannot carry it. Every admitted child creates a
fresh gate, including a hardware respawn before the producer's first byte.
Actual transport faults dominate process exit and cannot trigger fallback.
Cancellation has priority over a simultaneously failed writer. Bound responses
report a content-free body error after buffered bytes rather than successful EOF
when transport or producer supervision fails. Paired-producer cleanup must
retain ownership across cancellation and propagate actual reaping failures;
uncertain termination quarantines capacity and cleanup paths independently of
song ownership. Retained interval output now admits the same immutable final
binding while its child supplies raw internal MPEG-TS. One gate owns original
arrival ages, reserve bytes and prepared windows; a one-shot delivery start
waits for reserve, PSI and verified PCR readiness without rebasing holding age.
Source reads refill between paced writes; short-source EOF is validated before
drain. First-byte, startup, next-packet, holding and write/flush deadlines remain
independent. Requested retention/processing allocation payload is bounded to
32 MiB; this excludes allocator overhead/RSS and separately bounded authored
page/PSI ownership. Larger retention does not enlarge the 32,768-packet
processing/signalling window limit. Actual child-command rate, capacity and
holding policy are checked before consumer spawn. Startup/retry cleanup
uncertainty quarantines outer capacity and artifacts. Final transport failure
closes/drops the raw response and waits for producer completion before reporting
a body error. Slate/programme handover remains required work.
The legacy final wrapper now maps every input stream explicitly and disables
only global metadata copying. This corrects default-selection caption/alternate-
audio loss and preserves stream language descriptors under FFmpeg 9, while
retaining independently configured channel service metadata. Actual programme-
handover verification remains separate from this command correction.
Fresh final-output admission permits FFmpeg's initial discontinuity indicator
only on the first physical packet of each relevant PSI/PCR PID. Adaptation-only
or unflagged first packets consume that allowance. Strict standalone readers
and sealed monitoring retain their existing rejection behavior; subsequent
discontinuities remain terminal, with CRC, continuity, clock and capacity
validation unchanged. Owned PMT rewriting begins a fresh continuity sequence;
source audiovisual/PCR packets remain untouched.
Paced final-output forwarding opportunistically reads the same borrowed stdout
while packet delivery waits. Raw lookahead and immutable pending output share
capacity; lookahead also fits the existing holding-age budget after delivery
delay and sink lateness. This prevents downstream pipe backpressure from
starving the realtime concat wrapper without relaxing its delivery fences.
No extra task, source opening or arrival-age reset is introduced; EOF sealing
follows pending flush/acknowledgement.
The existing transcode registry now provides atomic reservation of two distinct
MPEG-TS leases for a wrapper and its first encoder. Failure retains neither;
independent release and uncertain-process quarantine remain unchanged. Native
legacy routes and capacity-wait retries now use this pair admission. Private
per-wrapper UUIDs bind concat/item callbacks to one owner; only its first item
receives the unused encoder lease, and later items reserve capacity independently.
Owned startup awaits cancellation and cleanup after handler abandonment; scoped
body-drop fences cannot close a later item. Persistent cancellation reaches song
rendering and hardware-retry preparation/spawn. Cleanup uncertainty retains
admission/assets and reports terminal failure. Limit one rejects legacy output
before opening a source or process; localized Health guidance applies only to
enabled native channels whose configured default is legacy MPEG-TS.
The Linux-hosted production-route fixture now passes with the exact Linux x64
FFmpeg candidate, Windows FFmpeg through interop and ARM64/ARMv7 FFmpeg under
QEMU. It proves owner-bound admission, token rejection and pre-first-item/active-
body cancellation, with decoded video, selected English audio and authored Dutch
page 200 plus clean TSDuck transport. P9A-04 owns the exact evidence. Full routed
source captions/alternate audio, native Windows/ARM host acceptance, other
platforms/consumers, fairness and compatible continuous slate handover remain
unproven.
The preparatory pure `ffprobe::teletext::project_packed_descriptors` API projects
explicitly supplied two-byte packed metadata or n9.0 LF-wrapped base64. Its
1,024-entry resource bound is not a DVB service limit. It retains language order,
raw type/magazine/page and optional decimal display addresses, distinguishing
missing, unusable and usable-prefix outcomes. `TrailingUnpairedPairs` reports
the potentially stale tail after FFmpeg descriptor shrink; even equal counts
cannot prove a complete descriptor or transmitted-page inventory. Subcode,
charset/region and source provenance remain unknown. Probe argv/JSON fields,
teletext schema/page UI and ordinary audio/subtitle projection are unchanged. P9A-03
owns focused/synthetic projection proof, bounded probe acquisition and persistent
source/page identity; production probe integration remains open.
Preparatory local acquisition replaces unbounded process output capture with a
worker-owned managed process using a Unix process group or Windows JobObject.
Stdout content has an exact 8 MiB cap; concurrent stderr draining retains a
16 KiB raw-byte tail. These are capture bounds, not a process RSS guarantee.
Overflow kills/reaps the process and joins readers; caller-future drop signals
owned cleanup. Probe argv, ordinary metadata and total scan timeout policy are
unchanged. P9A-03 records 52 passing focused infra regressions, scanner overflow
and recovery proof, actual pinned Linux FFprobe subtitle preservation and strict
infra/scanner Clippy. A shared bounded task-local cleanup scope now keeps the
managed web worker, startup replay and standalone scanner alive through local
owner drain. Cancellation fences new probes and destroys the execution future
before awaiting cleanup; still-owned heartbeats continue during that drain.
Terminal scan effects require confirmation, and actual lease loss cannot revive
or finish the operation. Unconfirmed cleanup stops the worker and rejects new
scans; cached Health results and badges share its live availability state, with
all nine translations. P9A-03 records passing infra, application-gate, admission,
scanner and 19 web regressions, followed by 38 locale/API/state/Health checks.
Known stopped-worker scan errors use the existing persisted UI locale, preserving
other transport outcomes and server-function routes. Strict affected Clippy and the WASM hydrate type-check pass. This adds no total-probe timeout, CLI signal handling, durable
quarantine after lease expiry or provider-wide lifecycle guarantee. Extra probe
output, teletext scanning/persistence and native Windows acceptance remain open.
The separate `parse_ffprobe_json_with_teletext` API deserializes supplied JSON
once and returns the ordinary media version with an ephemeral auxiliary
snapshot. Only explicitly identified MPEG-TS `dvb_teletext` subtitle rows use
the packed n9.0 base64 decoder. Global stream indexes remain distinct from PIDs
and supplied program membership; every reported membership is retained rather
than selecting the first program. Missing or conflicting PID/program identities
remain explicitly unavailable or unusable, independently of descriptor usability.
Auxiliary projection limits are resource policy after JSON deserialization,
not DVB limits or an input-allocation/RSS bound. This does not acquire media,
establish source revision, persist pages or discover a complete service/page
inventory. Production probe arguments and ordinary track projection are unchanged.
P9A-03 records 80 focused FFprobe passes and 15 relevant final parser/fixture
passes after strict ASCII-hex PID validation; strict infra all-target Clippy
and workspace formatting pass. Unchanged acquisition and cleanup evidence is reused.
The n9.0 acquisition audit rules out adding a global data dump to the ordinary
probe: `show_data` formats extradata for every selected stream, including large
attachments, before output-key filtering. `select_streams` filters both root
and program stream rows, so subtitle-only selection would erase ordinary audio
and video from that probe. The separate optional subtitle acquisition reuses
bounded managed capture, with root `show_streams` and separately restricted
`program`/`program_stream` entries; it must preserve ordinary metadata on auxiliary
failure while retaining cleanup uncertainty as fatal. Ordinary probe flags remain unchanged. Before attachment or persistence, prove source revision across both
opens and fence the corresponding media-version write with current scan
ownership. A path hash or scan-write timestamp cannot establish that revision.
FFprobe field aliases merge entries, and common side-data can repeat in program
rows; output selection alone is not a child memory bound. Primary evidence:
[n9.0 stream/program output](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffprobe.c#L1888),
[section-entry selection](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffprobe.c#L2717)
and [data formatting](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/textformat/avtextformat.c#L526).
A separate optional local-acquisition API now reuses that bounded process owner.
Its subtitle-only base64 capture has an internally owned ten-second deadline;
ordinary capture arguments and no-total-timeout policy are unchanged. Confirmed
auxiliary failure preserves ordinary metadata, while cancellation and uncertain
cleanup remain fatal. Primary and auxiliary global-index/PID sets must agree.
A held original regular-file handle plus observed size/filesystem-mtime bookends
provides a report association across both opens and a later caller recheck. It
is not an immutable-byte certificate or proof against restored in-place changes.
The safe same-file 1.0.6 handle dependency was already locked transitively; its
Windows file-index limitations and native acceptance remain open. Symlinks and
non-lossless command locators conservatively have no auxiliary association,
without changing ordinary probing; complete safe alias support is still due.
The durable application/web/CLI path forwards the actual acquired claim to the
Local configuration. Local video leaf writers now attach descriptor reports to
fresh exact stream/file/version parents inside the ordinary media transaction.
The actual LibraryPath/library/lease binding and existing parent-aware claim gate
are checked before ordinary mutation and immediately before commit. Publication
child scans reuse the real parent lease; no second lease is invented. A lost
claim or post-report source association requires whole-leaf rollback, including
ordinary stream replacements. Before any report write, auxiliary failure or a
lost source association preserves ordinary metadata without an auxiliary report.
This does not enable source-page selection. P9A-03 owns verification and gaps.
A preparatory report-only source model and additive SQLite storage now retain
independent binding, provenance, PID, program and descriptor states. Exact
stream/file/version parents remain distinct; repeated descriptor addresses and
program memberships survive without becoming selectable pages. Observed pages
are explicitly `NotAcquired`, with subcode, charset and national option unknown.
An opaque adapter revision is an association, not certified source-byte identity.
The storage helper bounds child collections, validates all current parents and
the single library/source subtype, and reuses the managed scan claim inside the
caller-owned SQLite writer transaction. Initial stale claims cause no mutation;
errors require rollback and the caller retains ownership through commit. Missing
records never prove absent pages. Stream/file deletion cascades remove reports;
narrow actual-change triggers also clear them before existing file upserts or
in-place stream identity updates, preserving no-op writes and ordinary tracks.
Local scans and managed Jellyfin library movie/episode leaves for mapped regular
files now activate that report-only writer. Jellyfin requires the selected source
ID/path, explicit unique nonnegative embedded indexes, exact item-path equality
and complete native index/kind/codec correspondence. Missing indexes are never
inferred from provider stream IDs or ordinary projection fallbacks. Plex/Emby
retain the same optional untrusted declaration, without activating their report
acquisition. Provider MediaFile paths and ordinary track projection stay intact.
Mapped acquisition requires the genuine current scan claim and existing cleanup
scope. Native primary/auxiliary probes and each owned filesystem/header stage have
ten-second limits; actual workers retain their source and scope after caller loss.
A stalled filesystem call cannot be preempted: uncertainty remains fatal and the
actual worker retains ownership until exit. Transactional mapping/source checks
and renewed lease checks fence the ordinary leaf/report commit; post-write changes
require rollback. Confirmed optional acquisition failure before report writes
preserves ordinary metadata. This is descriptor/header association, not source
caption readiness, immutable-byte proof or remote-URL acquisition. P9A-03 owns
native movie verification and remaining episode, targeted-show claim, provider,
source-page/body/charset gaps. Descriptors never confer page readiness or
source-page selection. A bounded supplied-window PSI observer now reuses the section/PAT/PMT parsers
without reserving a new output PID. It retains exact current PAT/CAT/all-PMT
bytes, versions, descriptors and every membership of the already occupied PID.
The source-only validator permits absent PCR without granting clock authority;
authored admission retains its existing collision and PCR requirements. Current
PAT changes invalidate dependent PMTs; contradictory/invalid input is sticky.
A separate bounded header collector composes the existing PES/header parsers,
retaining every actual occurrence, packet/PES/unit position and raw PTS. It
rejects failed or unfinished windows rather than exposing partial success.
The pure APIs establish no source or page readiness by themselves. The managed
Local adapter now reads only the original held regular file, outside SQLite,
through a bounded blocking owner registered in the existing cleanup scope. Caller
loss or an early uncertainty reply cannot release that owner's capacity while the
worker continues. Successful return joins the worker. Reads admit at most32768
transport packets and1024 successful aggregate header occurrences, under one ten-second
cooperative deadline and one-second settlement budget. Uninterruptible filesystem
calls retain the owner until they actually finish. No-scope calls start no reader.
Header collection starts only after complete current PSI; a sticky current-table
change witness rejects the whole read even if the wire tables later return to
their initial bytes. Source association is rechecked after reading and before the
ordinary media transaction commits. This still certifies no immutable source bytes.
The existing source reader distinguishes physical TS188 and M2TS192 frames at
byte zero without resynchronization or filename guesses. Explicit interior spans
carry their own framing; the default constructor remains TS188. Parsers receive
the inner188 bytes, while snapshots, coverage and source/decoder hashes retain
every original prefix and packet byte. Native byte positions must identify an
exact physical frame start and corroborate the original PES/AV clock; the opaque
M2TS prefix grants no clock authority. Detection restores the held file cursor
on success and failure, so subsequent sequential header reads remain correct on
Windows as well as Unix. Other positional reads do not promise cursor neutrality.
A new additive version-1 bounded JSON child stores HeadersObserved under the exact
existing source report. The applied parent status remains its legacy zero sentinel;
child presence is decoded explicitly, never as Ready. SQL gates untrusted payload
bytes before decoding; replacement clears old children and parent invalidation
cascades. Cancellation/uncertain cleanup remain fatal, while confirmed optional
framing/PSI/read/deadline failures preserve ordinary metadata and descriptors.
Page body/charset, complete inventory, source-bound selection and finite cache
remain required work; this slice does not accept source or engine delivery.
A separate pure parser now checks one explicitly supplied
46-byte DVB unit and reports protected header identity, correction counts and raw
C4–C14 bits.
Non-header units and FF termination stay distinct from page headers; the null
FF:3F7F address is rejected. Hamming 8/4 corrects one-bit errors and rejects
detected double errors; larger corruptions can alias accepted codewords. Neither
acceptance nor a correction count certifies source bytes. Unit classification,
C6 and descriptor/forced/default policy remain independent. No header text or
body is decoded, and C12–C14 cannot choose charset/language without page or
magazine context. This grants no transport discovery, acquisition or page
readiness; source records remain `NotAcquired`. Primary envelope/protection
references: [EN 300 472 §§4.3–4.4](https://www.etsi.org/deliver/etsi_en/300400_300499/300472/01.04.01_60/en_300472v010401p.pdf#page=8)
and [EN 300 706 §§7.1.2, 8.2, 9.3.1 and annex A.1](https://www.etsi.org/deliver/etsi_en/300700_300799/300706/01.02.01_60/en_300706v010201p.pdf).
A borrowed `source_pes::SuppliedTeletextPes` view checks one complete supplied
packet before exposing length-coded units. Its supported header profile is
unscrambled `private_stream_1`, aligned fixed 45-byte headers, with either no PTS
or PTS alone; other optional fields are explicitly unsupported. Exact declared
length, the 184-byte multiple, identifier, timestamp prefix/markers and header
stuffing are checked. Recognized teletext units require 44 payload bytes; other
unit IDs remain raw. Successful framing does not establish DVB/PSI conformity:
a PSI-signalled teletext stream permits only IDs 2, 3 and 255, and cross-unit
field/line ordering remains separate. Missing PTS is unknown, distinct from
reported zero; no per-unit timing, epoch or clock synchronization is inferred.
The view borrows supplied bytes without allocations or source discovery. It
does not assemble TS, validate protected headers/text, acquire pages or alter
stored `NotAcquired` state. Header observations still use the separate checked
unit parser. The packet limits and timing distinction follow
[EN 300 472 §§4.2–4.4, §5 and annex A](https://www.etsi.org/deliver/etsi_en/300400_300499/300472/01.04.01_60/en_300472v010401p.pdf#page=8).
A bounded `source_assembly::SuppliedTeletextAssembler` accepts supplied TS188
packets for one caller-selected elementary PID. It retains at most one PES and
one previous payload packet, emitting a borrowed view only after complete PES
validation. Selected-PID continuity, exact duplicates and finite truncation are
checked; unexpected starts, gaps, discontinuity or malformed data fence the
instance. An initially unobserved PES prefix remains explicit. Other PID data
does not acquire assembly state; only sync is checked globally. The supported
strict DVB transport profile permits payload-only or adaptation-only packets,
not combined adaptation/payload packets; generic remux output needs separate
support and proof. Adaptation-only continuity is checked, without interpreting
optional PCR/OPCR/extension contents or establishing clock validity. Byte-tail
framing, source discovery, acquisition deadlines and immutable source identity
remain caller obligations; successful finite closure never proves an absent or
complete page inventory. The selected PID is not certified by PSI ownership.
This supported transport restriction follows
[EN 300 472 §4.1](https://www.etsi.org/deliver/etsi_en/300400_300499/300472/01.04.01_60/en_300472v010401p.pdf#page=8).
Automatic publication selection, rate/retention policy,
refresh/publication and live channel acceptance remain open. These drafts do not freeze the
versioned add-on schema or establish generation publication or production
delivery.
This does not establish engine, six-RID packaging,
scanner, page-management UI or client acceptance. The
`teletext-support` parity record and P9A-01 through P9B-02 own those claims.

## Context

A source audit at ErsatzTV legacy `46118d43313b7828780fbfcfb46e980390ea8b0d`
found no teletext-specific policy. Existing subtitle selection and extraction
remain the behavioral baseline; authored full pages are an ErsatzRS extension.
FFmpeg's libzvbi decoder handles pages and captions but is not an authoring
encoder. An independent experiment with original synthetic pages demonstrates
that FFmpeg 9 can decode correctly signalled DVB packets into text and bitmaps.

## Decision

### Page and service model

The current base model uses EN 300 706 Level 1/1.5 semantics: 40 columns, a header
with 32 authored character positions and body rows 1–24, with navigation sharing
row 24 when
used. Packet X/25 is not an extra display row (EN 300 706 table 25). A display
page number is decimal 100–899; magazine 8 has wire value zero. Subcodes retain
their standards-defined identity independently from a page number. Multiple
subpages cycle within a page and remain independently addressable in the viewer.
Reserved non-display addresses cannot be supplied as ordinary authored pages.
The accepted target now includes Level 2.5 enhancements and separately typed
non-display resources under [ADR 0123](0123-level-2-5-teletext-and-external-editor-workflows.md).
Preserve this base as fallback; current encoding does not establish that target.
ADR 0123's 2026-10-19 amendment accepts a standalone Level 1/1.5 delivery first,
including applicable X/26 data without optional enhanced resource dependencies.
Complete Level 2.5 and the full generic-engine gate still precede Phase 9B.

The public page model contains typed text/mosaic runs, foreground/background
colour, boxing, height, flash and conceal attributes, language, national option,
subpage identity and explicit navigation targets. It contains neither raw
transport bytes nor arbitrary HTML, SVG, scripts or renderer command arguments.
Validate row/column occupancy including spacing control cells before encoding.
Do not silently truncate or replace characters. Use the appropriate G0 national
subset and Level 1.5 X/26 enhancement for supported accented characters. Reject
unrepresentable characters with a page/row diagnostic. Browser text preserves
Unicode, but a service advertised for DVB must also validate its DVB encoding.

Source pages retain stream index, codec `dvb_teletext`, service/page/subcode,
subtitle type, ISO-639 language, default region/national option, character-set
provenance and source identity. Bind selections and cached derivatives to the
exact media version, stream and source revision. A subtitle page flag is not a
forced/default selection flag. Newsflash is not a subtitle. DVB bitmap captions,
closed captions and ordinary text captions keep their existing identities.

Reuse exact-item, channel and global-language selection in that order, including
the existing forced/default/accessibility policy and explicit Off. Select one
exact page within a stream; never merge independently selectable language pages.
Source replacement/rescan reconciles stale pages without deleting unrelated
subtitle or alternate-audio state. Subpages carry updates to the same selected
page; they do not automatically become new subtitle preferences.

### Finite source-caption EOF

Maintainer decision (2026-10-19): when a complete finite Local source ends with
a nonempty selected caption and no later update/clear, retain that last caption
until the demonstrable presentation end of the selected video, clipped by any
earlier programme/outpoint. Preserve the original source clock and cache isolation.
Neither format duration, last PCR, another video track nor an arbitrary timeout
establishes that presentation end. Unknown end evidence remains an explicit
preparation failure, not permission to omit the caption or harm ordinary playback.

The finite Next text adapter retains its ordinary fixed-duration decode. Only
an infinite final event can use a second native text decode without duration
fixing: all event starts/durations, previous text and fixed cue intervals must
agree before the missing final text is recovered. A complete retained-snapshot
video decode must match the selected index/PID/program and provide ordered PTS
and positive frame durations without reported frame errors. Its presentation
end is normalized against the held original format origin using checked rational
arithmetic; ambiguous millisecond rounding rejects. SRT uses its existing
millisecond precision. Existing programme/outpoint clipping remains downstream.
The versioned clock policy and selected video identity separate cached results;
occurrence seeks/cuts do not change complete-source text. Additional passes use
the existing absolute deadline, work/source/snapshot ownership and joined cleanup.
P9A-04 owns scoped runtime proof; this policy does not establish the remaining
provider, native-output/platform or full engine acceptance.

### Authored DVB transport

Implement standards-based encoding in safe Rust within the existing FFmpeg
pipeline boundary. FFmpeg remains the audiovisual encoder; no libav binding,
external graphics renderer or separate mandatory transport process is introduced.
The demonstrated gap is page authoring, not another audiovisual pipeline.
Keep the encoder limited to page packets and the required transport adapter.
Evaluate existing maintained protocol libraries before implementing parser or
multiplexer machinery; no new dependency is selected by this ADR.

Use EN 300 472 PES carriage and EN 300 468 teletext signalling: private stream
PES, a dedicated non-colliding PID, PMT private-data stream with teletext
descriptor, correct language/type/page identity, Hamming-protected addresses,
odd-parity character data, the specified bit order and stuffing. Preserve PTS,
PCR, continuity counters, PMT CRC/version and page erase/header timing. Use a
bounded carousel, distinguish subtitle units from ordinary page units and close
page transmissions correctly. Include page/subpage navigation in both encoded
and browser output. Neither a visual preview nor successful FFmpeg startup is
carriage evidence.

The transport adapter operates on host-owned encoded output, never reopens or
consumes an input producer. For constant-rate streams reserve capacity and
replace null packets; retain audiovisual packets and their timing. Validate PID
and PMT ownership and available rate before enabling a service. Variable-rate
or fragmented output must use its explicitly verified transport path rather
than assuming the same adapter applies. Insufficient capacity, malformed PSI
or PID collision is a visible failure, not silent page loss. Refresh swaps the
complete validated page generation at a carousel boundary.

Native command construction refuses a selected source-teletext bitmap Burn
when video output is Copy. The same unsupported-source result reaches both
validated command builders before a playback consumer can start; no implicit encoder
promotion or dropped caption makes that request succeed. This guards an invalid
filter/copy command and does not implement or waive the required verified Copy
caption carriage or source-bound sidecar below. A carriage adapter must retain
exact source identity, chosen stream/page, command binding and consuming-child
ownership. The existing Burn/NUT initializer grants no carriage authority.

Source-caption delivery is an explicit output-channel choice, independent of
video Copy, authored-page profiles and a mirrored channel's source. Reuse the
channel editor, application save boundary and transactional `ConfigElement`
storage, keyed by the actual channel UUID. Burn remains the default. An omitted
field preserves an existing choice; malformed values fail validation rather
than silently selecting another delivery. Native MPEG-TS and its verified
TS-segment Hybrid path may select DVB bitmap subtitles. Other engines/outputs
must reject that choice visibly; scripts do not change it implicitly.

An initialized carriage request must bind the selected source/page and the
complete generated consumer command. Project native caption timing from the
actual selected AV owner and executed seek, retaining measured uncertainty and
container rounding rather than inventing an exact origin. Private initialized
fragments and their playlist follow the consuming child through confirmed reap
or bounded quarantine, including command rejection after spawn. Preparation
admission ends with actual preparation work; playback retains its existing
transcode reservation. No extra serving mux, connection or capacity increase
may substitute for the required handoff.

Independent prepared captions retain ordinary primary AV pacing. Their explicit
bitmap producer retains the normal positive MPEG-TS mux lead (0.7 s), so late
AAC priming packets do not become a wrapped negative start after an early
subtitle packet fixes the muxer's timestamp offset. Require the same lead in
its TS-segment child; ordinary Burn producers and wrappers retain
their existing mux-delay policy. Actual caption/AV timing and wrapper delivery
remain required qualification, not an inference from command options.
The explicit bitmap producer with video Copy excludes the initial pre-seek
keyframe via `-copypriorss:v:0 0`; it waits for an eligible keyframe without
retiming retained packets or converting video. Decoder preroll DTS may precede
the requested boundary. Caption initialization must precede the first retained
picture PTS, not merely the first packet inside an assumed seek window.
The explicit native bitmap Hybrid producer owns input AV pacing. Its generated
map-all wrapper must not apply a second input readrate to the combined HLS
stream: FFmpeg includes sparse subtitle progress in that shared input clock.
The final existing PCR gate still owns HTTP delivery pacing and backpressure.
Ordinary wrappers and user-configured scripts retain their existing authority;
only the actual enabled native Hybrid output UUID selects this typed variant.
That variant also preserves packet DTS: retain genpts/discardcorrupt but omit
igndts on its shared HLS input. The ordinary wrapper flag policy is unchanged.
FFmpeg n9.0 [readrate scheduling](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffmpeg_demux.c#L471-L524)
includes sparse streams; its [DTS reconstruction](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavformat/demux.c#L946-L1064)
explains the separate startup-clock boundary. Require native proof of exact
retained clocks, complete tracks and bounded final delivery for both corrections.
The bitmap mux and the final map-all live wrappers use a positive 100 ms interleave
bound to limit the native mux queue for sparse subtitle updates. This alone
does not qualify the input scheduler or subtitle timeline. Private fragments
normalize container packet durations to zero before consumer input, using the
existing FFmpeg `setts` bitstream filter. Old source packet durations must not
throttle a clear/replacement group; original artifact durations, shifted PTS/DTS,
packet payloads and order remain independently checked. DVB payload timeouts and
explicit clear events retain their presentation meaning. This changes neither
the delivery deadline nor
timestamp validation. Shipped wrapper defaults follow the existing exact-known-
content upgrade; customized scripts remain untouched.

FFmpeg's HLS muxer cannot carry DVB bitmap subtitles through its WebVTT branch.
Any explicit Hybrid carriage adapter must therefore qualify the existing
producer's MPEG-TS segment path, preserve append/sequence/clock behavior and
publish only closed segments through the existing playlist/worker lifecycle.
This is an implementation obligation, not Hybrid runtime acceptance. Bounded
finite-source artifacts likewise do not close full-source, exact-boundary,
final-clear, client or platform acceptance; the canonical P9A-04 F6 record owns
implementation coverage, actual proof and remaining gaps.

### Engine/source/output matrix

Every row is an implementation obligation, not a claim of current support.
Authored pages depend on the host guide snapshot, independently of the selected
media source and video transcode/copy policy.

| Engine and output | Authored page delivery | Selected source teletext captions |
|---|---|---|
| Native direct/legacy MPEG-TS | DVB page carousel in a signalled PID plus browser viewer. | Exact-page bitmap burn-in when transcoding; explicit selected-source passthrough where PMT/PID/client proof permits it. Video copy requires verified carriage or a source-bound sidecar; never pretend copy burns captions. |
| Native HLS segmenter/direct | Browser viewer for complete pages; DVB carriage in TS segments only on the explicitly verified TS-segment path. No native browser HLS teletext controls. fMP4 uses the viewer rather than a claimed DVB track. | Exact-page burn-in when transcoding or bounded finite extraction to the existing subtitle delivery path; reject unsupported copy/output combinations visibly. |
| Next hybrid MPEG-TS | Host-owned post-wrapper DVB carousel plus browser viewer, separately proven from native output. | Finite source-bound extracted captions use the existing Burn/Convert adapter. Exact source teletext passthrough is not assumed from Next track hints. |
| Next HLS, Burn/Convert | Browser viewer for authored full pages; no claim that Next HLS has a DVB carriage/page-selection protocol. | Finite source-bound extracted captions use the existing Burn/Convert adapter. Unsupported embedded live/one-shot selections retain ADR 0100's visible refusal. |

Apply the following source-lifecycle rules across those rows:

- Reopenable local files and finite authenticated providers: bounded exact-page
  decode/extraction with source-owned authentication and version-bound cache.
- Prepared add-on files: same exact-file contract, preserving retained intervals,
  subtitle ownership and validation receipts.
- One-shot producers: consume once. Native streaming decode may share the already
  owned input; extraction requiring a second read is rejected. Next requires an
  independently verified source-bound prepared caption artifact per ADR 0100.
- Live HTTP/RTSP: native page-specific streaming decode remains bounded and
  cancellation-aware. Never await whole-source extraction from an endless input.
  Next unsupported embedded text/page selections fail visibly per ADR 0100.
- Authored services: use the validated host page generation for all source
  lifecycles without seeking or rescheduling the programme or replacing audio.

The Native live-URL implementation reuses the already-owned producer path.
Advertised teletext on the exact resolved Remote Stream version admits one
host-owned FFmpeg stream-copy producer; it copies all input tracks with their
source timestamps and does not change the selected output encoder or profile.
Stored declarations grant no playback readiness. The existing bounded prefix
observer qualifies pages against the freshly produced transport, and the
existing final-command binding keeps captions, video and selected audio on that
same held input. Original upstream PID/program declarations cannot substitute
for those fresh observations after remuxing. Process-group/Job ownership,
cumulative startup deadlines, ordinary-AV fallback and joined cancellation keep
their existing contracts. This is a Native MPEG-TS integration boundary, not
acceptance of every source/output combination. The source-output milestone owns
the actual HTTP/RTSP and HLS-input receipts; no permanent source restriction
or full source/output acceptance follows from a scoped receipt.

HTTP retries recover network and premature-read failures without hiding normal
playlist/segment EOF. Existing native session owners reenter actual Live source
EOF. Real selected AV drains without programme-length audio padding; generated
silence alone ends with the real video, preserving real copy/encoded audio tails.
Finite and retained inputs keep their established padding and bounds. A Live HLS
worker uses its own complete FFmpeg output playlist minus pre-spawn URIs to
advance the delivered clock and programme cursor. Missing, invalid or nonfinite
output duration cannot authorize guessed completion. These rules use existing
request, process, snapshot, capacity and cancellation owners, not a new worker
service. They introduce no Next callback or other serialized-runtime extension.

There is no automatic engine switch, alternate page substitution, silent caption
loss or double producer consumption. Prove accepted combinations with served
media, decoded captions/pages and audio/timing comparison. Operator-selected
unsupported combinations are visible in administration, startup and Health.

### Exact provider media source selection

The selected Jellyfin/Emby `MediaSources[].Id` is retained as nullable
`MediaFile.ProviderMediaSourceId`, separately from the integer server identity.
The ordinary leaf transaction qualifies the selected declaration against the
exact file path, complete embedded indexes and version owner. A conflicting
file/path hash cannot transfer another version's file or source identity. New
files use version-scoped keys while existing file IDs and hashes remain intact.
A unique owned file also retains its identity across a genuine path move;
ambiguous movement refuses rather than leaving an older selectable source.
The selector's actual-change trigger invalidates only that file's observations;
no-op writes preserve them. Missing or unqualified declarations store `NULL`.

The authenticated Next media opener reads the selector from the current exact
item/version/file/path/provider binding and sends it as encoded `MediaSourceId`
query data. The dynamic provider fingerprint includes this selection so a
changed source cannot retain an old prepared-caption cache key or promotion
claim. A private expected-source fingerprint also refuses a changed selected
source before opening its AV request. [ADR 0127](0127-provider-selected-source-identity.md)
owns these identity and request-binding rules. Existing rows with `NULL` preserve
ordinary legacy playback; exact finite
caption acquisition still requires its own source qualification. The selector
is a provider declaration, not native track/page evidence or immutable bytes.
Provider credentials and source selectors remain outside published relay URLs.

This strengthens the item/default-source forwarding in
`$ERSATZTV_SRC/ErsatzTV/Controllers/InternalController.cs` and
`$ERSATZTV_SRC/ErsatzTV.Infrastructure/Scheduling/PlayoutItemConverter.cs` to meet
this ADR's exact source correspondence. The
[Jellyfin stream contract](https://typescript-sdk.jellyfin.org/interfaces/generated-client.VideoApiGetVideoStreamRequest.html)
and [Emby streaming guidance](https://dev.emby.media/doc/restapi/Video-Streaming.html)
document variant selection. These references establish protocol intent; native
provider/runtime and finite-source acquisition acceptance require their separate
proof. The retained-source, EOF, bounds, cancellation and playback requirements
above remain unchanged.

### Channel profiles and shared controls

[ADR 0122](0122-channel-teletext-profiles-themes-and-lua-control.md) supplements
this contract with multiple per-channel add-on bindings, independently scoped
profile settings and shared application validation for admin/Lua. Generation
identity also fences channel, binding/profile/settings revision and any published
Lua interval. Presentation themes and defined custom zones remain add-on-owned;
only complete validated pages reach either browser or transport. Missing/invalid
profiles, collisions, expired overrides and script failures preserve ordinary
playback. The accepted timing/precedence/restart rules live in ADR 0122.
These requirements are not implemented by the preparatory global-settings runner.

### Host/add-on boundary

Add a versioned `teletext.pages.v1` capability to the existing package/child
process contract. Its operation is `pages`: one bounded JSON guide request on
stdin and one complete typed JSON page-set response. Unknown schema versions
are rejected. The exact DTOs live in `ersatzrs-addon-contract`; domain conversion
and validation live in the host's existing domain/application boundaries.
Manifest compatibility, explicit install/enable, settings, content hashes and
existing permission review remain mandatory. A repository refresh never enables
this capability automatically.

The request carries a generation token, requested locale and IANA timezone,
UTC capture/window instants, bounded channel identities/names and read-only
programme occurrence data. Reuse ADR 0112's projection, including grouped
programmes, filler disposition, current/next/continuing and Later Today. Supply
a three-calendar-day guide window for the TV-guide consumer; timezone/DST
conversion must use the existing programme clock. The add-on has no database
connection or scheduling write authority.

Anchor the three-date guide window to the capture instant's local calendar date:
start at that date's actual beginning and end at the beginning of the date three
civil dates later. Reuse `ProgrammeClock::calendar_day_bounds` with a count of
three. Direct date arithmetic preserves DST and skipped-date behavior; it does
not add 72 elapsed hours. The helper supports the preparatory supplied-context
request builder. Authoritative named timezone acquisition and complete guide
projection remain required host checks.

The response echoes the generation token and contains service language,
initial page, page/subpage identities, typed row content and navigation links.
The host checks identity, compatibility, bounded size, duplicate keys, encoding,
overlap and references before accepting any page. No product page numbers,
channel ordering or TV-guide composition is hard-coded in the engine. The
separate tvguide add-on owns NOS-inspired TV-guide pages only (ADR 0120).

Bound one invocation to 30 seconds, 4 MiB request and 8 MiB response, at most
256 channels, 10,000 programme occurrences and 1,024 page/subpage records.
Exceeding a bound fails visibly; do not publish a silently shortened guide.
The producer may paginate its pages into subpages within those bounds. Limit
refresh to one invocation per configured service, including serial producer
composition for multiple bindings, coalesce guide invalidations,
and fence promotion to the exact configuration/package/guide generation.

Preparatory version-1 request and response DTOs now live in
`ersatzrs-addon-contract`. Both strict raw-byte parsers reject duplicate keys
across the complete document, unknown fields and missing required fields,
including required nullable metadata and navigation/role references. Request
parsing and bounded serialization enforce 4 MiB, 256 channels and 10,000 total
occurrences without truncation. Empty guides remain explicit. Read-only request
metadata contains occurrence identities, guide intervals, literal titles and
nullable descriptions, actual segments and separate role references; it excludes
internal artwork/path metadata. UTC instants preserve nanoseconds; normalized
UTC years outside 0000–9999 fail on parsing and serialization.

A preparatory application builder accepts complete, already grouped supplied
occurrences and an authoritative host locale allowlist. It checks exact locale
membership, resolves the named timezone, retains whole occurrences intersecting
the three-date window and uses the shared selector with a separate one-date
Later Today boundary. Channel-local identity, actual segment order and exact
millisecond source offsets are checked before returning complete bounded request
bytes. Zero external item IDs, preroll identity, cross-channel mirrors and guide
intervals differing from actual segments remain intact. Explicit unknown
None-media zero offsets do not become invented source timing. The builder cannot
prove acquisition completeness, source revision or generation ownership.

Preparatory host acquisition now captures selected channel/source identities,
raw settings, all-time SQL items, rich metadata and accepted External JSON
revisions through one caller-owned read transaction. Separate aggregate source
and modeled expansion admission precede retrieval, decoding and copies; complete
channel-local grouping precedes window filtering. The actual system clock must
supply a genuine IANA name, and page-language codes/aliases resolve only against
the generated host catalog. The same captured context feeds V1/V2 installed-profile
invocation and native unpublished candidates. Missing accepted revisions do not
confer mutable-file authority. This proves the bounded accepted-source subset,
not complete source lifecycle, freshness or generation ownership. P9A-04 owns
the canonical component evidence and remaining integration gaps.

Response parsing checks 8 MiB, the exact opaque generation echo and 1,024-record
bound. Application conversion checks values and the whole page-set structure
before returning an unpublished draft, preserving literal text, mosaics, styles
and links without partial results. The 1,024-byte generation-token bound is host
resource policy. Complete source lifecycle/provenance and fresh source
association, trusted generation ownership, full encoding/standards acceptance,
profile-specific service activation, localized administration and safe publication
remain open host gates. The bounded acquisition/native preparation checkpoint
does not waive those requirements or independently accept the engine.

A preparatory infrastructure command primitive accepts an already host-resolved
command and strict request bytes. An owned worker retains the managed native
process and all pipe tasks across caller abandonment. One absolute 30-second
execution deadline covers preflight, stdin, process wait and complete output;
stdout and stderr have independent 8 MiB and 16 KiB retained-byte limits with
immediate overflow failure. Stderr is discarded without exposing producer text.
Cancellation and failures kill/reap the managed group/job and abort/join pipe
tasks; five-second process and one-second pipe cleanup budgets preserve a
distinct cleanup-uncertain outcome and existing native-tool quarantine. These
bounds do not certify RSS or escaped descendants. Raw successful output still
requires response, generation and encoding validation. Installed-package
resolution/admission, explicit service selection, generation ownership and
publication remain separate host gates; no channel service activation is wired.

A dedicated preparatory installed-package adapter now resolves only
`teletext.pages.v1/pages`; generic invocation helpers still reject that operation.
It reuses enabled/compatible manifest, dependencies, content hash, contained
entrypoint, global settings and native-tool leases, then reloads enabled state
and the exact StateRevision before launch. The absolute execution deadline starts
before request preflight and is passed unchanged through resolution and command
execution. Cooperative fences check between native filesystem stages and hash
entries/64 KiB reads. On stop the owner retains the complete resolver future and
joins every started blocking call. Native calls are not interruptible: settlement
may return CleanupUnconfirmed after five seconds while the owner keeps awaiting
the original job, with no later stage or child launch. Genuine tool uncertainty
is never collapsed into cancellation, timeout or admission failure. Raw stdout
and the observed revision do not certify immutable package bytes or grant
promotion.

ADR 0122's profile path now captures one enabled output binding and its complete
validated effective settings under a read transaction. Exact snapshot comparison
before resolution and immediately before launch prevents stale or mixed
profile/installation settings. Environment and native-tool resolution use those
settings together, before execution leases are selected. The existing owned
deadline/cancellation/settlement path remains in force. The receipt retains the
out-of-band output selection and exact supplied request generation; guide-source
channels do not confer output ownership. Application preparation rechecks that
selection after execution and parses the entire response into an unpublished
draft. Shared application controls mutate only explicit operator base state.
Channel-wide preparation now serially invokes the complete enabled binding
snapshot under one shared deadline, validates a bounded aggregate and rechecks
all membership/settings after the last producer. An explicit entry add-on supplies
initial-page/service metadata. Native preparation encodes that complete draft
and retains it unchanged beside its packets and descriptor. Trusted generation
ownership, automatic channel service activation, Lua overrides, localized
administration, full encoding/client acceptance and publication remain open.

### Persistence and publication lifecycle

Use SQLite for source-page metadata, service configuration, generation identity
and latest operation state. Store validated immutable page generations beneath
the existing managed-data boundary. Publish only a complete validated generation
by atomic promotion; browser and transport readers share that same generation.
Restart adopts only the generation recorded as validated; it does not rerun an
add-on before unrelated playback can begin.

Refresh after guide/configuration/package changes with a one-minute debounce
and at least every five minutes while enabled. Page expiry is host-owned: the
last accepted snapshot remains available for at most 15 minutes after a failed
refresh, with an explicit stale indicator and Health warning. After expiry the
service becomes unavailable and its transport advertisement is withdrawn safely.
A disabled/uninstalled/incompatible package stops publication immediately and
withdraws the service; never keep secretly running its executable. Cancellation,
lease loss or stale-generation completion cannot promote pages. Keep the last
validated generation for diagnostics without presenting it as current output.

The service is opt-in; missing teletext capability does not break unrelated
playback. A configured service with no valid generation reports unavailable
rather than synthetic replacement guide content. Engine fixtures are never a
production fallback.

### Runtime and platform contract

All six RIDs retain the same model and safe-Rust encoder. The accepted maintainer
amendment of 2026-10-26 temporarily requires only `linux-x64`, `linux-arm64`,
`linux-arm` and `win-x64` while the repository is private. `osx-x64` and
`osx-arm64` build, crossbuild, package, toolchain-installation, runtime and
acceptance work is **Deferred by maintainer decision**, not accepted. Do not start
macOS jobs or paid runners/services. Preserve code, guards, artifacts and scoped
evidence without deliberate incompatibility; additional macOS repair is deferred.
Explicit maintainer permission is necessary to resume, even after a visibility
change. The existing deferred macOS milestone owners retain future obligations.

P9A acceptance requires all functional requirements, including complete authored
Level 2.5, correct Level 1/1.5 fallback, full QTeletextMaker interchange and
engine-first delivery, plus every required four-platform check. It must claim
neither verified macOS support nor six-platform acceptance. Native ARM and the
required Windows package trial are not waived by this macOS-only amendment.

Managed FFmpeg 9 packages for each required RID must include libzvbi and required
license/dependency notices, expose
accepted decoder options and pass an actual small page/subtitle decode probe.
Runtime identity includes the exact artifact digest; version or decoder name
alone is insufficient. Operator runtimes report supported/unavailable accurately;
missing optional capability becomes a blocking warning/error only when selected
teletext operations require it. Reuse acquisition, bounded process execution
and refresh/capability cache ownership. No UCU platform exception is inherited.

Source-clock handling must preserve declared teletext PES timestamps across the
one-source producer and actual consuming demuxer without guessing URL formats.
The first-party opt-in `-fflags +preserve_teletext_pts` contract (common patch0019)
leaves default PCR repair and DVB subtitle behavior unchanged, and HLS inherits
it through existing child format flags. Bind capability to the exact runtime
before selecting this operation; older runtimes reject the unknown flag before
source opening. Missing support must remain explicit and preserve ordinary AV.
Candidate verification is separate from application wiring, Health reporting,
managed adoption and the required-platform acceptance owned by P9A-02/04.

The runtime report proves this operation using a copy of the pinned fixture
whose56 PCR fields advance by one second while all PES bytes remain unchanged.
Require all56 original PTS/DTS values, not merely recognition of the flag. Legacy
reports default this separate optional capability to false; decoder page/text/
bitmap readiness remains independent. The Native Live producer and observed
track selection share one runtime snapshot, and the consuming physical input
uses typed `TeletextTimestampPolicy::OriginalPes` with unchanged existing flags
and command/source seals. Missing capability explicitly excludes unverified Live
teletext choices while retaining the same ordinary AV producer and selected
audio; localized Health and runtime-bound troubleshooting expose that outcome.

### Routes, administration and accessibility

Provide an authenticated management viewer for a configured channel/service,
page and subpage through the existing browser route surface. Classify every
route under ADRs 0102/0103; machine administration stays under machine API policy
and private media/decoder callbacks stay on the internal listener. Do not add
management to the public streaming-only listener.

Use server-rendered links/forms for page-number entry, index, previous/next page
and subpage. Hydration adds convenience without blocking basic browsing. Expose
plain readable programme text and labels for assistive technology; do not make
colour the only navigation cue. Allow flash suppression, conceal/reveal and
predictable keyboard focus. Localize management, capability, stale/error labels
and diagnostics in all nine supported cultures. The add-on owns guide content
language within its package contract.

Health/troubleshooting distinguishes missing decoder/options, selected page not
found, invalid metadata/charset, failed extraction, unsupported engine/output,
PID/rate/signalling failure, missing/disabled/incompatible add-on and stale or
expired publication. Preserve the existing subtitle/audio and provider checks.

## Consequences

The native engine needs a standards-based authoring encoder and output adapter;
libzvbi alone cannot supply those. Genuine DVB and browser delivery share a typed
page generation but require independent runtime and client proof. P9A-02 must
verify packaging on every currently required RID, P9A-03 owns source metadata,
P9A-04 closes generic
engine acceptance and P9B-01/02 own actual content and integrated acceptance.
None of those gates is waived by this decision or its synthetic experiment.

## Alternatives Considered

- Browser-only pages or a burned-in guide image: insufficient for the selected
  genuine DVB delivery and independently navigable full pages.
- Treat decoder support as an encoder: no such FFmpeg contract was found.
- External teletext/graphics process in the playback path: adds runtime ownership
  where a bounded safe-Rust packet encoder/adapter is the demonstrated gap.
- Raw page bytes or HTML supplied by add-ons: exposes wire/rendering internals
  and bypasses typed validation and accessibility.
- Unlimited guide enumeration or indefinite stale-page retention: prevents
  bounded work and truthful publication/Health behavior.

## References

- [ADR 0120: Engine/content ownership](0120-separate-teletext-engine-from-addon-content.md)
- [Phase 9 plan and source map](../../plans/teletext-support.md)
- [Milestone acceptance and evidence](../progress/milestones.toml)
- [Remaining teletext debt](../progress/parity/teletext-support.toml)
- [Current add-on runtime](../reference/addon-runtime.md)
- [EN 300 706 V1.2.1: Page coding and transmission](https://www.etsi.org/deliver/etsi_en/300700_300799/300706/01.02.01_60/en_300706v010201p.pdf)
- [EN 300 472 V1.4.1: DVB PES carriage](https://www.etsi.org/deliver/etsi_en/300400_300499/300472/01.04.01_60/en_300472v010401p.pdf)
- [EN 300 468 V1.20.1: DVB service information](https://www.etsi.org/deliver/etsi_en/300400_300499/300468/01.20.01_60/en_300468v012001p.pdf)
- [FFmpeg libzvbi decoder options](https://ffmpeg.org/ffmpeg-codecs.html#libzvbi_002dteletext)
