# ADR 0121: Define teletext pages, transport and the add-on contract

## Status

Accepted (2026-10-10) as the technical implementation contract for the
maintainer-authorized Phase 9 engine-first programme in ADR 0120. The selected
delivery is genuine DVB teletext for MPEG-TS clients plus an authenticated
browser viewer. The separate TV-guide add-on still starts after P9A-04.
ADR 0123 extends the authored target through Level 2.5 and adds external-editor
interchange; it supersedes the Level 1/1.5-only target below, not the other gates.

## Implementation

Partial. The selected-runtime capability report now runs bounded decoder-option,
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
and video from that probe. A future optional second subtitle acquisition can
reuse bounded managed capture, with root `show_streams` and separately restricted
`program`/`program_stream` entries; it must preserve ordinary metadata on auxiliary
failure while retaining cleanup uncertainty as fatal. No additional probe flags
are enabled. Before attachment or persistence, prove source revision across both
opens and fence the corresponding media-version write with current scan
ownership. A path hash or scan-write timestamp cannot establish that revision.
FFprobe field aliases merge entries, and common side-data can repeat in program
rows; output selection alone is not a child memory bound. Primary evidence:
[n9.0 stream/program output](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffprobe.c#L1888),
[section-entry selection](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffprobe.c#L2717)
and [data formatting](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/textformat/avtextformat.c#L526).
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
This does not enable scanner/provider acquisition or source-page selection.
A separate pure parser now checks one explicitly supplied 46-byte DVB unit and
reports protected header identity, correction counts and raw C4–C14 bits.
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

There is no automatic engine switch, alternate page substitution, silent caption
loss or double producer consumption. Prove accepted combinations with served
media, decoded captions/pages and audio/timing comparison. Operator-selected
unsupported combinations are visible in administration, startup and Health.

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

All six RIDs retain the same model and safe-Rust encoder. Managed FFmpeg 9
packages must include libzvbi and required license/dependency notices, expose
accepted decoder options and pass an actual small page/subtitle decode probe.
Runtime identity includes the exact artifact digest; version or decoder name
alone is insufficient. Operator runtimes report supported/unavailable accurately;
missing optional capability becomes a blocking warning/error only when selected
teletext operations require it. Reuse acquisition, bounded process execution
and refresh/capability cache ownership. No UCU platform exception is inherited.

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
verify packaging on every RID, P9A-03 owns source metadata, P9A-04 closes generic
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
