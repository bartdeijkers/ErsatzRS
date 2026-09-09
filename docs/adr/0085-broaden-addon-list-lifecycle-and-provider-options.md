# ADR 0085: Broaden add-on-list lifecycle and provider options

## Status

Accepted (2026-09-01), amended (2026-09-04, 2026-09-08)

The mandatory SponsorBlock processed-video preparation decision is superseded
by [ADR 0087](0087-stream-sponsorblock-intervals-without-playback-underruns.md)
(2026-09-05, amended 2026-09-06). Streaming is AMM-M-01A; optional list-level
Download ahead modes are AMM-M-01B. File-preparation requirements below apply
only to those download paths, not the streaming default.
Other lifecycle and provider-option decisions remain accepted.

The 2026-09-08 maintainer amendment limits AMM-M-03 to configurable per-list
and combined-list disk-usage health warnings. The proposed finite retention,
LRU reclamation, Clear unused cache and quota/concurrency administration are
withdrawn, not postponed to integrated closeout. Existing implemented admission
and ownership safeguards remain unchanged. The owning AMM-M-03 milestone
records warning settings and Health acceptance evidence.

## Implementation

Partial — AMM-J-01 through AMM-J-04 and the AMM-K-01 provider-neutral
persistence foundation are implemented; the remaining milestones are tracked
in structured progress.

## Context

The add-on-list draft and publication model in ADR 0075 and the resumable
overview/detail transport in ADR 0079 are useful foundations, but the current
operator workflow and provider contract stop short of a complete lifecycle.

The editor can report a recovery blocker while the list remains in the
`Publishing` review state. It consequently shows a disabled **Publishing**
button even when no live worker owns the publication. Its recovery message
does not say which action will clear the blocker. The underlying defect is more
serious than its presentation: publication stages files, starts a Local scan,
and commits or rolls back inside the admin request. A scheduled scan may
overlap that sequence, and a cancelled request or transient Windows sharing
violation can strand the journal after the scan completes.

List refresh is a Boolean daily policy. The official yt-dlp package performs a
flat discovery of the complete source on every refresh, even when almost every
provider identity is already present. It maps fields available in the returned
JSON, but flat extraction can omit playlist-level descriptive metadata. The
package also deliberately passes `--no-update` and offers no item-scoped
browser-cookie, SponsorBlock, or subtitle behavior.

The requested yt-dlp flags have constraints that must be reflected in the
architecture rather than hidden in a command string:

- `--download-archive` records extractor identities, and
  `--force-write-archive` is required when a simulated extraction must update
  the archive. It skips known identities but does not itself promise that
  playlist enumeration stops; yt-dlp exposes `--break-on-existing` as a
  separate early-termination behavior.
- `--cookies-from-browser` reads the browser profile of the operating-system
  account running ErsatzRS. A browser name is configuration; cookie values are
  credentials and must never enter the database, IPC, logs, or health output.
- `--sponsorblock-remove` removes selected segments from a video file through
  post-processing. It is not a metadata-only switch for the existing direct
  stdout playback path.
- `--write-subs --no-write-auto-subs` can materialize human-authored subtitle
  sidecars without downloading the video, but those files still need atomic
  host ownership, language normalization, scanner projection, and playback
  selection.
- `-U` self-updates supported standalone yt-dlp builds. Package-manager and
  read-only installations may require an operator-owned update path instead.

ErsatzTV has no repository-installed add-on-list equivalent. This is an
ErsatzRS product extension, but it must retain the existing separation of
concerns: the host owns generic validation, persistence, scheduling,
publication, and administration; the package owns provider behavior and exact
yt-dlp arguments.

## Decision

### Keep the host provider-neutral

ErsatzRS will not branch on the yt-dlp add-on identifier. Optional versioned
capabilities will extend the shared add-on contract for:

- schema-declared list and item options;
- bounded source-change probes for cadence-aware adaptive scheduling;
- incremental list discovery with a host-owned provider checkpoint;
- bounded provider-produced artifacts such as subtitle sidecars; and
- external-tool maintenance.

The host renders declared options with its localized components, validates and
persists only their typed values, and passes those values in bounded operation
requests. The package translates them to provider arguments. An add-on that
does not declare a capability does not see its controls and continues through
the existing compatibility path.

Option values are namespaced by add-on and schema version. Updating a schema
cannot silently reinterpret a saved value. Unknown option keys, values outside
their declared enum, oversized payloads, and options supplied to an
unsupported operation are rejected before process launch.

### Coordinate every managed-library mutation durably

Publication, publication recovery, reset/reload, provider refresh, and every
scan origin will use one durable per-managed-library operation coordinator. A
publication owns an exclusive lease across staging, its child scan, commit,
rollback, and reconciliation. The child scan carries the same owner token;
scheduled and manual scans defer rather than overlap it. If a scan already
owns the library, publication waits durably before staging.

An admin request only validates and enqueues work. Browser cancellation does
not cancel the job. The coordinator persists phase, generation, exact saved
revision, attempts, lease and heartbeat, next eligibility, original failure,
bounded recovery diagnostics, and terminal outcome. The original failure is
committed before rollback begins. Startup reclaims expired leases and resumes
the exact phase idempotently.

Each job also owns an ordered causal event stream. It records enqueue source,
lease admission and wait reason, phase transitions, child-scan identity,
filesystem transition, retry classification, original failure, operating-
system error category and numeric code when available, recovery action, and
terminal outcome. URLs, paths, titles, cookies, headers, and provider stderr
are excluded. Heartbeats remain scalar job state rather than noisy events.
The stream retains at most 256 immutable events per job, reserving its final
entry to report truncation, and terminal job summaries are retained for at
most 30 days and 50 jobs per list. The editor and Health expose the same
redacted timeline and correlation identifier.

The UI uses **Publishing** only while an unexpired lease is actively executing
a publication phase. Queued and lease-waiting work uses **Publish queued** or
**Waiting to publish**. Failed, expired, or recovery-required work uses the
disabled label **Publish blocked** and an alert immediately below it. Every
blocker carries a stable reason code, a localized complete reason, one explicit
remediation action, and a target or command. Vague instructions such as “wait”
or “recovery required” are not sufficient.

### Define reset and reload as a complete unload

**Reset and reload source URL** is a durable destructive operation. It retains
the list identity and name, selected add-on, provider identity, source URL,
managed-folder binding, refresh cadence and publication policy, detail policy,
processed-artifact quota, generated-playlist intent, and list-level option
defaults. It removes the current and draft provider snapshots, list and item
metadata overrides, conflicts, inclusion and numbering choices, replacements,
published projection, generated-playlist membership, managed definitions and
NFOs, artwork, subtitles, processed-media cache, import/detail/publication jobs,
cursors, and archive checkpoint. The generated `Playlist` identity remains as
an empty shell because deleting it would cascade into operator-owned schedules,
filler/deco references, and playout anchors. Draft and published revision
counters remain monotonic so stale work cannot regain authority. Reset then
starts a first-generation full import from the retained URL.

Unload commits before reload begins. Therefore channels and playlists have no
items until the new draft is reviewed and published, and a failed reload leaves
an empty list with a retry action. The confirmation dialog states that outcome
and requires the list name. The job deletes only files proven to be owned by
the list's journal and ownership sidecar; it never recursively deletes an
operator-selected storage root.

Reset is not the normal way to detect provider removals. A separate durable
**Reconcile entire source** command performs a complete provider enumeration
into scratch state while retaining the current draft, publication, overrides,
replacements, options, and artifacts. It presents a comparison before any
change is published. An identity missing from one complete successful
reconciliation becomes a tombstone with `MissingSince`; it becomes eligible
for removal only after a second complete successful reconciliation at least
24 hours later or an explicit operator confirmation. Failed, truncated, or
cancelled enumeration never advances missing evidence.

Before reset or reconciliation, **Test source** runs a bounded durable header
and first-page probe without mutating list state. Reset confirmation reports
affected channel, playlist, and published-item counts. The editor also compares
the saved draft with the published revision by added, changed, excluded,
unavailable, conflicted, and removal-candidate counts.

Each list retains the current and two immediately preceding complete
publication revisions. Files and artifacts are reference-counted rather than
copied per revision. **Compare revisions** is read-only; **Restore revision**
runs through the same durable publication coordinator and creates a new
monotonic revision, so restore never rewrites history.

### Replace daily refresh with an explicit or adaptive cadence

`AutoRefresh` becomes a cadence with these UI values: `adaptive`, `daily`,
`weekly`, `monthly`, `yearly`, and `never`. Existing true values migrate to
`daily`; false values migrate to `never`, so migration never opts a list into
learned scheduling.

The next due instant is persisted. Daily and weekly add one and seven days.
Monthly and yearly use calendar arithmetic and clamp invalid month-end or
leap-day dates to the last valid day. A successful manual or automatic refresh
becomes the new anchor. Failure uses a separate bounded retry time and does not
advance the cadence. Startup and the hourly maintenance tick enqueue one due
generation without catch-up bursts or duplicate claims.

The 2026-09-04 amendment adds `adaptive` as an explicit opt-in for a package
that declares `media-list.source-check.v1`. The bounded probe reports `changed`
or `unchanged`, a package-owned opaque validator, and bounded recent
source-update instants when the source exposes them. An RSS or Atom package
should use HTTP conditional requests such as ETag or Last-Modified when
available. The host persists only validated scheduling observations and never
interprets or logs a source URL, credential, or opaque validator.

Adaptive scheduling uses a bounded rolling history of actual source changes,
not job successes. It predicts the next change from the median positive
interval, schedules the probe shortly before that instant, and clamps the
result to a one-hour minimum and a 30-day maximum. Until the source supplies at
least three usable intervals, adaptive cadence falls back to daily. An
unchanged prediction uses bounded increasing miss intervals rather than
restarting a full import on every maintenance tick; a detected change resets
the miss count and enqueues the normal durable refresh path. Failure backoff,
`Retry-After`, the shared provider-host limiter, and positive jitter may delay
a probe but never create duplicate work. Manual refresh remains available and
valid change observations from it may improve the model without moving a
failed or unchanged observation into the change history.

For an incremental-capable provider, the host stages a private `archive.txt`
copy for each discovery invocation. The yt-dlp package passes that path through
`--download-archive` and uses `--force-write-archive` for its simulated
discovery. Only after output validation and the corresponding database page
commit does the host atomically promote the staged archive. Cancellation,
invalid output, or transaction failure discards it. Archive lines are opaque
provider data; the host stores no raw URLs in them and does not try to parse an
extractor identity.

Incremental discovery distinguishes `skip_known` from
`newest_first_early_stop`. The latter may add `--break-on-existing` only when
the package declares, for the exact source shape, that order is stable and no
unseen identity can occur after the first archived identity. Otherwise the
package must continue bounded enumeration while suppressing archived items.
The first contract does not combine `--lazy-playlist` with early stop because
current yt-dlp changes the incomplete archive-match path in lazy mode. Lazy
processing may reduce buffering only for `skip_known`, when the separate header
operation supplies its missing aggregate fields and frozen fixtures prove the
selected version's behavior. Every result reports the strategy plus examined,
archived-skipped, and emitted counts; the UI must not claim that a refresh
avoided full enumeration unless those counters prove it.

Normal refresh is an append/update delta and may validly return zero new
items. An identity absent from a delta is not a deletion. Reconciliation
bypasses the archive in scratch state and owns removal evidence without
unloading operator work; reset/reload clears the archive and remains the clean-
slate path. The package obtains and maps playlist header metadata through a
separate bounded header extraction, so title, summary/description, tags,
channel/people, artwork, and stable identifiers can refresh even when the item
delta is empty.

Each list has a refresh-publication policy. **Review changes before
publishing** is the default. Optional **Automatically publish validated
additions** may enqueue publication only when the saved change set contains
new items and no existing-item change, conflict, tombstone/removal candidate,
restriction, unavailable item, schema change, or failed preparation. It uses
the normal publication lease and exact revision checks. Every other result
remains a review draft and creates a localized in-app activity/Health notice;
automatic refresh never silently removes current playout content.

### Add yt-dlp item behavior through declared options

The official package will declare list defaults plus nullable per-item
overrides for these effective options:

- **Browser cookies**: `none`, `firefox`, `chrome`, or `chromium`, with the
  list default initially `none`. An item inherits that default until it has an
  explicit override. The package emits the effective non-`none` value as the
  lowercase argument to `--cookies-from-browser` for that item's enrich,
  artifact, preparation, and playback operations. Only the enum is persisted.
  The UI explains that yt-dlp reads the browser profile visible to the
  ErsatzRS service account and gives a specific remediation when the profile
  is absent, locked, or cannot be decrypted.
- **Remove SponsorBlock segments**: Boolean, with the list default checked.
  An item inherits it until explicitly overridden. Unset legacy rows acquire
  the checked draft default, but an already published revision does not change
  until the operator saves and publishes it.

The editor always shows the effective value and whether it is inherited or
overridden, offers **Reset to list default**, and supports previewed, atomic,
one-step-undo bulk changes over an explicit filter scope. A per-item **Test
access** command runs the selected browser choice through a bounded provider
probe and returns only a typed accessible, profile unavailable, profile locked,
decryption failed, provider restricted, or failed result. It never exports,
counts, or displays cookies.

SponsorBlock removal uses `--sponsorblock-remove default`, whose yt-dlp
meaning is all removable categories except filler. It is available only for
finite media. Because yt-dlp applies removal to a video file, the package
prepares a host-owned transient processed playback artifact rather than
pretending the flag works on direct stdout. Preparation is durable, bounded by
free-space and size policy, keyed by source fingerprint, options, and yt-dlp
version. The artifact is not a
Local-library import or permanent media copy. Live items show the option as
unavailable and continue through direct playback.

Existing cache admission uses global/per-list byte ceilings, a minimum-free-space
floor and bounded preparation. Current and known future playout references
remain pinned. Under the 2026-09-08 amendment, M-03 adds only advisory Health
warnings when per-list or combined-list used disk space reaches its configured
threshold. Threshold controls belong at logical locations in add-on-list
settings. They do not block work, trigger cleanup or alter playback. Finite
retention, LRU eviction, Clear unused cache and broader storage administration
are no longer requirements of this program.

Changing either item option invalidates affected detail, subtitle, processed
media, and playback readiness while preserving the last published revision.
The new value reaches a channel only after explicit publication.

### Materialize human-authored subtitles as publication artifacts

The yt-dlp package requests only the configured global default and fallback
subtitle languages and uses `--write-subs --no-write-auto-subs` with a bounded
format preference. When both settings are unset, it does not fetch every
language. The package maps ErsatzRS canonical language codes to provider tags,
reports the actual returned language and format, and never substitutes an
automatically generated track.

An artifact capability writes only to a host-created per-job staging
directory. Its manifest includes item identity, artifact kind, language,
relative file name, byte length, and digest. The host rejects traversal,
links, unknown formats, mismatched digests, excessive files, and excessive
bytes before moving artifacts into the journaled publication. Provider
processes receive no general write access to a media root.

Published subtitle sidecars are projected as normal `Subtitle` rows for the
generated Remote Stream media item. MPEG-TS and HLS playback then use the
existing precedence from ADR 0052: item, channel, global default, and global
fallback. Subtitle acquisition failure is visible and retryable but does not
withdraw otherwise playable video. Publication acceptance nevertheless
requires a real channel test proving that an available requested subtitle is
selected and rendered or carried according to the channel subtitle mode.

### Maintain yt-dlp through a declared tool operation

An enabled add-on that declares external-tool maintenance may receive a
bounded `tool.update.v1` invocation. Enabling the updated official yt-dlp
package acknowledges the new self-update permission; an existing installation
is held for permission review under ADR 0074 before automatic maintenance
starts.

The yt-dlp package implements the operation by invoking its configured
executable with the exact `-U`/`--update` behavior and by returning a typed
outcome: updated, already current, manual update required, or failed. The host
records before/after version, timestamps, and a bounded redacted diagnostic.
It does not parse provider prose or install a replacement itself.

For a regular self-updating executable whose permission has been accepted,
the update job records its size and digest and stores one bounded private
last-known-good copy before invoking `-U`. It then proves the replacement can
run, reports a version, and loads its extractor registry before releasing the
tool lease. Failed validation restores the exact previous bytes atomically and
returns `rolled_back`; failure to restore quarantines the tool and blocks new
invocations with exact remediation. Package-manager, immutable, and read-only
installations never enter this backup path.

Every ErsatzRS start enqueues one update attempt for the enabled package. A
separate daily schedule makes maintenance due at least once per 24 hours while
the host remains running. The maintainer-approved playback-first amendment
allows actual execution to remain pending beyond 24 hours while the tool is
in use. Pending maintenance does not terminate playback or block new playback
waiting for existing invocations to drain. Claims deduplicate startup/daily triggers,
and a per-tool lease prevents discovery or playback from launching while the
executable is being replaced. Other yt-dlp operations continue to pass
`--no-update` so updates happen only in the controlled job.

The maintainer accepts explicit offline recovery after abrupt host death when
process-tree cleanup is uncertain. Normal shutdown/restart remains automatic.
The recovery action requires operator confirmation that the host, scanner and
related provider processes stopped, refuses a held executable lease, and retires
only the selected tool's validated cleanup markers. It does not infer exit from
marker age or a database lease expiry, repair an executable, or delete media.
N-01 does not add general process supervision for automatic crash recovery;
replacement validation, backup and rollback remain N-02 work.

When `-U` reports an immutable, unpackaged, read-only, or package-manager-owned
installation, Health and Add-ons show **Manual update required** with the
unambiguous instruction to update that installation through its owning package
manager or select an official self-updating standalone executable. It is not
retried continuously between the required startup and daily attempts.

The Add-ons page provides **Check for update now** and a redacted bounded
history of attempt source, before/after version and digest, validation,
rollback, outcome, and next eligibility. Manual attempts share the same lease,
backoff, and fair-use state and cannot bypass a persisted cooldown.

### Make metadata, numbering, and navigation direct

List metadata and item-detail metadata are always rendered as form controls;
there is no **Edit metadata** text link, icon button, collapsed edit mode, or
edit-mode heading. The title at the top of the list page is a labelled text
input. One explicit **Save changes** action, dirty-state announcement, field
validation, and ETag conflict handling remain.

Season and episode numbers are optional operator metadata.
`SeasonEpisodeMissing` is removed as a publication blocker. The television
projection retains provider `null` values and uses the existing internal zero
sentinel only where the ErsatzTV-compatible relational schema requires a
number. Administration, search, NFO, and guide presentation label that state
as **Unnumbered** and do not invent or display `S00E00`. Stable add-on item
identity, not a season/episode pair, remains the projection key.

The maintainer's 2026-09-05 presentation refinement uses conventional uppercase,
minimum-two-digit `S01E01` identities throughout the admin UI in every culture,
including season-zero specials such as `S00E03`. This deliberately replaces the
upstream search label `S1 E1` and localized prose identities for consistency with
local television libraries. Localized **Unnumbered** labels remain unchanged;
filenames, provider identities, and stored projection strings are not rewritten.

The `/media/addon-lists` table shows **Name** before **Id**. Search Remote Stream
cards link to a canonical `/media/remote/streams/:id` details route. That route
uses the same media-detail vocabulary as movie and episode pages, includes a
back path to the owning add-on list when one exists, and does not expose a
provider URL as editable host metadata.

All changed admin surfaces have localized copy for every supported culture,
equivalent hydrated and no-JavaScript commands, keyboard and screen-reader
semantics, 320-pixel reflow, 200% zoom support, and non-colour-only state cues.
AMM-K must also close the shared dependencies it exercises: destructive
dialogs contain keyboard focus while open and use `aria-labelledby`, and
sortable Name/Id table headers use keyboard-operable controls plus `aria-sort`.

## Consequences

- Publication and scheduled scans can no longer mutate the same managed
  library concurrently, and request cancellation cannot orphan their work.
- A bounded causal operation history preserves the distinction between the
  original failure and later recovery failures without retaining private data.
- **Publish blocked** becomes a truthful terminal/attention state with a
  concrete path forward; **Publishing** means a live lease is making progress.
- Reset/reload is intentionally destructive and may leave channels empty after
  a provider failure, but its confirmation and postcondition are exact.
- Normal yt-dlp refresh emits only unseen identities and can update list
  metadata with no new items. It stops enumeration early only when the package
  proves safe ordering; non-destructive reconciliation owns provider-removal
  evidence without erasing operator work.
- Publication previews, downstream impact, and three retained complete
  revisions make routine changes recoverable at bounded storage cost.
- Browser-cookie selection improves restricted-item access without placing
  cookie content in ErsatzRS state. Service-account and browser encryption
  limitations remain visible operator concerns.
- SponsorBlock removal adds preparation latency and bounded temporary disk use
  for finite media. Direct live playback cannot support that post-processing;
  explicit quotas can block preparation before the volume is exhausted.
- Human-authored subtitles become normal publication assets and participate in
  the existing playback-language selector rather than a yt-dlp-only playback
  branch.
- Self-update is automatic once the new permission is accepted, but unsupported
  installation types remain operator-managed with an explicit diagnostic.
  Supported executable replacement is validated and rolls back to one known-
  good copy before normal invocations resume.
- The contract, schema, coordinator, scanner, admin UI, health checks, official
  package, and six-platform acceptance all require changes; acceptance is
  therefore split across AMM-J through AMM-N.

## Alternatives Considered

- Keep publication inside the admin request and add a longer timeout: rejected
  because neither request lifetime nor timeout length can recover an orphaned
  journal or coordinate a scheduled scan.
- Change only the button copy: rejected because it would conceal the missing
  recovery claimant and shared library exclusion defect.
- Add yt-dlp-specific fields and branches to the Rust host: rejected because
  provider behavior belongs to the add-on and future providers need the same
  generic option, artifact, checkpoint, and maintenance boundaries.
- Persist browser cookies or an exported cookie file in list state: rejected
  because it would turn provider credentials into application data and leak
  them across diagnostics and backups.
- Apply `--sponsorblock-remove` to the existing stdout command and assume it
  works: rejected because yt-dlp defines removal as file post-processing.
- Download every subtitle language: rejected because the operator already set
  primary and fallback language intent and unbounded sidecars create avoidable
  provider traffic and storage.
- Treat every missing archive identity as removed: rejected because an
  incremental archive intentionally suppresses old entries; absence is not a
  complete snapshot observation.
- Enable `--break-on-existing` for every source: rejected because a provider
  may reorder a playlist or insert an unseen identity after a known one; early
  stop requires an exact package-declared ordering guarantee.
- Use destructive reset as the only way to reconcile removals: rejected
  because it would erase operator metadata, replacements, inclusion, and
  option intent during routine maintenance.
- Automatically publish every refresh: rejected because provider changes,
  removals, restrictions, and conflicts require review. The optional automatic
  policy is limited to validated additions.
- Let the host replace yt-dlp from a release URL: rejected because the request
  is specifically for yt-dlp's supported self-update operation and the host
  must not silently take ownership of package-manager installations.
- Trust a successful `-U` exit without validation or rollback: rejected because
  a newly replaced executable can still be unusable and otherwise break every
  list and channel that depends on it.
- Retain unlimited job events, revisions, or processed media: rejected because
  operational recovery must not become an unbounded database or disk consumer.

## References

- [ADR 0052: Add global subtitle language preferences](0052-global-subtitle-language-preferences.md)
- [ADR 0066: Use repository-backed out-of-process add-ons with tiered trust](0066-repository-backed-add-ons.md)
- [ADR 0074: Keep an updated add-on in service](0074-keep-updated-add-ons-in-service.md)
- [ADR 0075: Review add-on metadata before library publication](0075-review-addon-metadata-before-publication.md)
- [ADR 0079: Discover add-on lists before deferring item details](0079-make-addon-list-imports-resumable.md)
- [Add-on-list publication recovery](../../plans/addon-list-publication-recovery.md)
- [Add-on-list lifecycle broadening](../../plans/addon-list-lifecycle-broadening.md)
- [yt-dlp command-line options](https://github.com/yt-dlp/yt-dlp#usage-and-options)
- [yt-dlp installation and update methods](https://github.com/yt-dlp/yt-dlp/wiki/Installation)
