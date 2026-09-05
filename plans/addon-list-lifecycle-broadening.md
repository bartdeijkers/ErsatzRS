# Add-on-list lifecycle broadening

## Status

Execution plan registered on 2026-09-01, broadened on 2026-09-02, and amended
on 2026-09-04 with opt-in adaptive refresh scheduling. It
extends Add-on Metadata Management from the corrective AMM-J publication work
through AMM-K, AMM-L, AMM-M, and AMM-N. ADR 0085 owns the accepted architecture
and product decisions; this plan owns delivery order and acceptance. None of
the new behavior described here is implemented merely because the plan or ADR
is accepted.

## Outcome

Make `/media/addon-lists`, its item editor, generated Remote Streams, and the
official yt-dlp package one complete operator workflow:

1. publication and every scan origin are restart-safe and cannot overlap on
   the same managed library;
2. blocked publication is labelled **Publish blocked** and immediately
   explains the exact cause and exact action that clears it;
3. an operator can completely unload one list and reload its retained source
   URL from a clean generation;
4. list and item metadata are always visible and directly editable;
5. refresh cadence is explicit and yt-dlp refresh processes only unseen
   archive identities, stops enumeration early only when provider ordering is
   proven safe, reconciles removals without erasing operator work, and still
   refreshes complete playlist metadata;
6. yt-dlp items support browser-cookie selection, SponsorBlock removal, and
   human-authored subtitles selected through global FFmpeg language settings;
7. search results open a real Remote Stream details page; and
8. an enabled yt-dlp add-on runs a controlled self-update at every ErsatzRS
   start and at least daily;
9. operators can preview source, publication, reset, and downstream impact,
   compare or restore bounded publication history, and inspect a redacted
   causal operation timeline; and
10. inherited/bulk item options, cache budgets, and transactional tool-update
    rollback keep the broadened workflow usable for large lists.

The host remains provider-neutral. No Rust branch may compare an add-on ID to
the official yt-dlp identifier. Exact provider commands, output mapping, and
provider fixtures remain in the official add-ons repository.

## Confirmed current gaps

- `publication_primary_action` maps a `Publishing` review state to the
  **Publishing** button before considering `RecoveryRequired`. A stranded
  journal can therefore look active indefinitely.
- The recovery blocker copy says only that a previous publication awaits
  recovery. The editor has no command that can clear it.
- Publication, its Local scan, commit, and rollback are owned by one admin
  request. The 2026-08-31 incident overlapped a scheduled scan; the scan
  completed, but the request never completed commit or rollback. Available
  diagnostics cannot distinguish request cancellation from a Windows sharing
  violation during rollback. Both reach the same stranded-journal state.
- `AutoRefresh` is a Boolean and both refresh coordinators use a one-day
  cutoff. The hourly maintenance loop has no weekly, monthly, yearly, or never
  cadence model.
- The yt-dlp discover operation uses `--flat-playlist --dump-single-json` for
  every page and has no archive checkpoint. Its mapper accepts playlist
  description and tags, but flat output can omit those values.
- yt-dlp documents `--download-archive` and `--break-on-existing` as separate
  behaviors. An archive can suppress known results without proving that the
  provider stopped enumerating, and early stop is unsafe for a source that may
  reorder or insert new items after an archived one.
- The current draft/publication model has no non-destructive full-source
  reconciliation, provider-missing tombstones, general publication diff,
  bounded revision history, or downstream reset impact summary.
- Current job state has progress and a terminal diagnostic but no bounded
  causal event history capable of distinguishing the original failure from a
  later rollback failure.
- All current official yt-dlp operations pass `--no-update`; package guidance
  makes the operator update the executable.
- Search intentionally returns an empty href for Remote Streams, and there is
  no canonical Remote Stream details route.
- television readiness emits `SeasonEpisodeMissing` as a hard blocker even
  though the projection already has a compatible internal zero sentinel.
- list metadata is hidden behind both an **Edit metadata** text link and an
  icon button. The hero title itself is not a form control.
- `/media/addon-lists` currently renders **Id** before **Name**.
- The shared dialog does not yet contain Tab focus or connect its heading with
  `aria-labelledby`, and sortable shared-table headings are pointer-only and
  omit `aria-sort`.

## Scope boundaries

This broadening includes the Rust host, SQLite migrations, add-on contract,
official yt-dlp package, generated-file publication, scanner projection,
IPTV/HLS playback, health, localization, and browser/runtime acceptance.

It does not:

- bundle yt-dlp into ErsatzRS or replace a package-manager installation;
- persist, proxy, inspect, or display raw browser cookies;
- make provider-specific fields part of the core media-list schema;
- infer provider deletions from an incremental archive delta;
- apply SponsorBlock file post-processing to live streams;
- download automatically generated subtitles;
- allow arbitrary yt-dlp arguments, cookie/profile paths, or permanent video
  downloads; or
- weaken explicit review before provider changes, conflicts, restrictions, or
  removals change a library. The only optional automatic path is a validated
  additions-only revision.

## Operator experience specification

### Page and task map

| Route | Primary task | Required change |
| --- | --- | --- |
| `/media/addon-lists` | Find and manage a list | Order columns **Name**, **Id**, **Match status**, actions; name is the primary link. |
| `/media/addon-lists/add` | Create the first clean generation | Replace daily Boolean with cadence selector and show provider-declared list defaults. |
| `/media/addon-lists/:id` | Inspect, configure, recover, reconcile, reset, and publish | Truthful publication action, blocker alert, always-open metadata, source/impact preview, draft diff, revision and operation history, provider options, durable progress. |
| `/media/addon-lists/:id/items/:item_id` | Edit one item | Always-visible metadata, inherited/effective browser-cookie and SponsorBlock values, Test access, subtitle/preparation state, no edit-mode toggle. |
| `/search?query=addon_list:{id}` | Browse published members | Every Remote Stream card links to its canonical details route. |
| `/media/remote/streams/:id` | Understand and use a stream | Show metadata, artwork, duration/liveness, subtitle tracks, owning list, published options, availability, playback health, and channel/watch actions. |
| `/settings/ffmpeg` | Choose subtitle language intent | Existing default/fallback values remain canonical; the add-on-list page links here when neither is set. |
| `/settings/addons/:id` | Understand yt-dlp maintenance and storage | Show storage budget/use, update history, before/after version/digest, rollback state, next daily attempt, typed outcome, Check for update now, and Clear unused cache. |

### Publication action state machine

The editor must render state from the durable job/readiness projection, not
from a review-state shortcut.

| Authoritative state | Visible action | Enabled | Message directly below |
| --- | --- | --- | --- |
| Unsaved fields | **Save changes** primary plus disabled **Publish blocked** | Save only | “Publish is blocked because this page has unsaved changes. Choose Save changes, then publish the saved revision.” |
| Saved and ready | **Publish list** | Yes | “Revision N is ready to publish.” |
| Playback preparation required | **Prepare publication** | Yes | Exact item count and what will be prepared. |
| Queued | **Publish queued** | No | Queue time and current owner/wait reason. |
| Waiting for another library scan | **Waiting to publish** | No | “A scan already owns this library. ErsatzRS will start publication after scan job N completes; no action is required.” |
| Live publication lease | **Publishing** | No | Current phase, progress time, and retry attempt. |
| Validation blocker | **Publish blocked** | No | Exact blocker reason plus an action/link that resolves it. |
| Expired, failed, or recovery-required job | **Publish blocked** | No | Original failure, failed phase, and **Retry recovery** / **Restore previous revision**. |
| Published with no draft change | **Published** | No | Published revision and completion time. |

The blocker alert is the next semantic element after the publication action,
uses `aria-describedby` from the disabled button, has an alert heading, and
receives focus only after a user-initiated publish attempt. Routine polling
updates use `role="status"`, not repeated assertive announcements.

Each blocker projection contains:

- stable reason code;
- blocking object scope and bounded identifier;
- exact localized cause sentence;
- exactly one primary remediation kind and label;
- target anchor, route, or application command;
- optional secondary recovery command; and
- bounded redacted diagnostic and timestamp.

Required recovery copy in English is:

> Publish is blocked because the previous publication stopped after its
> library scan and revision N still owns an unfinished recovery journal. Choose
> Retry recovery to finish revision N, or Restore revision N-1. ErsatzRS will
> coordinate the required scan; do not repair the database or managed folder
> manually.

Localized variants must preserve the revision values, named actions, and the
statement that manual database/file repair is not required. Provider stderr is
never inserted into the sentence.

### Operation history and diagnostics

Every list page includes an **Activity** section backed by the durable job
event stream. A semantic ordered list groups events by operation and presents
trigger, correlation ID, phase, wait reason, attempt, bounded timestamp,
redacted error classification, recovery action, and outcome. The default view
shows the current job and latest terminal jobs; older retained jobs use a
disclosure. Polling updates use `role="status"` and never replace keyboard
focus.

The persistence boundary accepts at most 256 immutable causal events per job
and reserves the final entry for `history_truncated`. Heartbeats update the job
row and do not generate events. Terminal history is pruned after 30 days and
capped at 50 jobs per list while any active/recovery-required job is retained.
Events explicitly distinguish original operation failure from rollback or
restore failure, including a sanitized operating-system error category and
numeric code. **Copy redacted diagnostics** is a hydrated convenience; a
normal **Download redacted diagnostics** link returns the same text without
JavaScript. Both contain exactly the visible typed fields and never URLs,
paths, titles, cookies, headers, or raw provider output.

### Reset and reload source URL

The action is placed in a **Destructive actions** section, not beside Publish.
Its confirmation dialog:

- names the list and source host, but does not display sensitive query values;
- enumerates what is removed;
- states that channels and generated playlists will be empty until republish;
- states that a failed reload leaves the list empty;
- states which settings are retained;
- requires typing the current list name; and
- submits the same application command with and without JavaScript.

The retained state is list ID and name, selected add-on, provider identity,
canonical source URL, managed folder binding, cadence, refresh-publication
policy, detail policy, processed-artifact quota, generated-playlist intent,
list-level option defaults, monotonic draft/published revision counters, and
the generated `Playlist` identity as an empty shell. The removed state is every
draft/published item, list/item metadata snapshot and override, conflict,
numbering, inclusion, replacement, generated definition, NFO, artwork,
subtitle, processed artifact, playlist membership, projection, job, cursor,
retry, and archive entry. Retaining the empty playlist identity prevents reset
from cascade-deleting operator-owned schedule, filler/deco, and playout-anchor
configuration.

The resulting durable phases are `quiescing`, `unpublishing_files`,
`unpublishing_projection`, `clearing_provider_state`, `clearing_checkpoint`,
`verifying_empty`, `queueing_initial_import`, and `complete`. The operation
uses the same library lease and journal rules as publication. A repeated
command returns the existing generation.

### Source, change, and downstream impact preview

**Test source** is a durable non-mutating probe of the exact saved add-on,
canonical URL, option schema, and source revision. It requests a bounded
header and first page, reports the canonical provider identity, list title,
count hint, capabilities, access outcome, duration, and redacted diagnostic,
and expires when the URL, add-on/package version, or relevant defaults change.
It is recommended but not required before Reset and Reload.

The publication decision region shows a saved-draft comparison with counts for
added, metadata-changed, excluded, unavailable, conflicted, and removal-
candidate items. Each count links to the corresponding filtered review view.
The Reset confirmation adds affected published-item, generated-playlist,
channel, schedule, and active/future playout counts without exposing names in
diagnostics.

The **Publication history** section retains the current and two preceding
complete revisions. **Compare revisions** shows list metadata, effective
options, item membership, and artifact differences without mutation.
**Restore revision N** requires proportional confirmation, uses the normal
lease and exact-revision checks, and creates revision N+K rather than rewriting
an old revision. Reference-counted files and artifacts are deleted only after
no retained revision, active operation, or current/future playout pins them.

### Reconcile the source without resetting operator work

**Reconcile entire source** is separate from the destructive reset section. It
runs a complete provider enumeration with an empty scratch checkpoint, builds
a candidate snapshot, and preserves current publication, draft overrides,
inclusion, numbering, replacements, and effective options until explicit
Publish.

One complete successful absence records `MissingSince` and a first
observation. A second complete successful absence at least 24 hours later, or
explicit operator confirmation, makes the identity a removal candidate.
Failure, cancellation, incomplete pagination, or invalid output never advances
absence evidence. Reappearance clears the tombstone without losing operator
state. The resulting diff reports additions, changes, reorderings,
reappearances, and removal candidates; no reconcile result silently removes
current playout content.

### Always-visible inline metadata

- Replace the hero `<h1>` value with a visibly labelled title input styled as
  the page title. The heading can wrap the label/input but cannot use
  `contenteditable`.
- Render all list metadata controls in a persistent **Metadata** section.
  Remove both **Edit metadata** controls, the collapsed display state, and the
  **Edit metadata** heading.
- Render every item-detail metadata control immediately on the item route.
  The list overview remains a bounded summary and links its title to that
  route; it does not expand hundreds of full metadata forms into one table.
- Use one explicit **Save changes** action for the visible form. Announce
  dirty, saving, saved, validation-error, and stale-ETag states. Do not autosave
  on blur.
- Preserve provider value, operator override, conflict, and published value as
  separate concepts. Inline presentation does not collapse ownership.

Season and episode inputs accept blank values. Missing numbers are a review
recommendation, never a publication blocker. Cards, details, search, NFO, and
guide output say **Unnumbered** and never render `S00E00`; the internal zero
sentinel is storage compatibility only.

### Responsive and accessible behavior

- At 320 pixels and 200% zoom, action groups stack in source order and no
  metadata input or blocker action requires horizontal scrolling.
- The Name and Id values stay visible on narrow cards; Match status may move
  below them.
- Every checkbox has a visible label; SponsorBlock state is not conveyed only
  by an icon or colour.
- Inherited item values expose their effective state in text and provide a
  labelled **Reset to list default** action after an override.
- Destructive confirmation returns focus to its trigger on cancel and to the
  durable reset status on submit.
- The shared destructive dialog contains Tab focus while open and binds its
  visible heading through `aria-labelledby`.
- Sortable Name/Id headers contain keyboard-operable buttons, maintain
  `aria-sort`, and preserve one-dimensional overflow only inside the table
  region when a narrow card representation would lose relationships.
- A disabled live-state button is not the only source of progress; status text
  is separately announced.
- Every new key is translated in every supported culture. Provider package
  option labels/descriptions are likewise complete in the official manifest.

## Persistence and command model

### Shared library operation ownership

AMM-J extends the scan coordinator with an exclusive managed-library lease.
All manual, scheduled, startup-replay, publication-child, recovery-child, and
reset-child scans go through the same admission point. A child scan reuses the
parent operation token; no unrelated scan can acquire the library until the
parent reaches a safe terminal phase.

The publication request commits job intent before returning. Its cancellation
token belongs to application shutdown/lease loss, not the HTTP body. The first
error is persisted before any filesystem rename. Subsequent rollback errors
are separately appended as bounded phase outcomes.

Persist causal events in the same transaction as each phase transition. Event
fields are operation ID, correlation ID, sequence, phase, event kind, trigger,
attempt, wait/owner reference, sanitized error class/code, aggregate counts,
and timestamp. They never contain provider strings or private resource values.
Claim/resume tests prove that duplicate execution cannot duplicate or reorder
events and that retention never deletes an active or recovery-required job.

### Refresh cadence

Add a domain enum with serialized values `daily`, `weekly`, `monthly`,
`yearly`, and `never`, plus `NextRefreshAt`, retry eligibility, and last
successful anchor. Migration maps old `AutoRefresh = 1` to daily and `0` to
never before retiring the Boolean read path.

Calendar tests cover January 31, February 29, daylight-saving transitions,
restart while overdue, a failed attempt, a manual success, and multiple hourly
ticks. Scheduling uses UTC instants and calendar date arithmetic; it does not
interpret a month as 30 days or a year as 365 days.

Persist `review` or `auto_publish_additions` separately from cadence. Review is
the migrated/default value. Automatic publication is eligible only for a pure
validated addition set; any existing-item change, conflict, tombstone/removal
candidate, restriction, unavailable item, option-schema change, or preparation
failure leaves a review draft and an in-app activity/Health notice. Automatic
publication uses the same saved-revision command and library lease as a manual
Publish.

### Provider option values

Add a generic versioned option schema with Boolean and enum primitives for the
first slice. Definitions carry scope (`list_default`, `item_override`, or
both), operation targets, default, labels, descriptions, and applicability such
as finite-only. Persist draft and published list defaults plus nullable item
overrides independently by list/item identity and schema version. Effective
values are resolved as item override, then list default, then schema default.
Only declared effective keys cross IPC.

The official yt-dlp schema declares:

| Key | Scope | Type/default | Applies to |
| --- | --- | --- | --- |
| `cookies_from_browser` | list default + item override | enum `none` | enrich, subtitle, preparation, playback |
| `sponsorblock_remove` | list default + item override | Boolean `true` | finite preparation/playback |

The browser enum contains exactly `none`, `firefox`, `chrome`, and `chromium`.
Display casing may be Firefox/Chrome/Chromium; IPC and argv use lowercase.
Bulk option commands accept All items, Current page, or the current explicit
filter, preview every affected effective value including off-page counts, save
one atomic undo snapshot, and never mutate the published option snapshot.

`Test access` is a durable one-item provider probe for the current saved option
revision. It returns a typed result and expiry key only; the host does not
inspect a profile directory or request exported cookies. Changing the URL,
package/tool version, or browser option expires the result.

### Incremental checkpoint and archive

`media-list.import.v2` adds `mode = full|incremental`, a bounded checkpoint
descriptor, and delta-complete semantics. It allows an incremental page to
contain a refreshed list record and zero item records. Existing import v1
remains frozen.

For yt-dlp, the checkpoint is a real host-owned `archive.txt` beneath the
add-on data namespace. The database stores its relative identity, generation,
digest, entry count, and last successful promotion. Each invocation receives a
copy in a private scratch directory and returns a staged successor. The host
promotes it only with the validated page checkpoint. Partial output can never
advance the archive beyond committed items.

The package passes both `--download-archive <staged-path>` and
`--force-write-archive`. Archive contents are never logged, returned in IPC, or
embedded in health. Normal refresh merges returned identities and never marks
suppressed old identities omitted. Reset/reload deletes the checkpoint and
starts `mode = full`.

The package selects and reports `skip_known` or
`newest_first_early_stop` for each source shape. The early-stop strategy adds
`--break-on-existing` only when the package's source adapter guarantees stable
newest-first order and no unseen item after the first archived identity. It
must not combine `--lazy-playlist` with early stop in this contract because
current yt-dlp changes incomplete archive matching in lazy mode. Lazy processing
is allowed only on `skip_known`, when the separate header operation owns
missing aggregate fields and frozen fixtures prove the chosen tool version.
Unknown or mutable ordering must use bounded enumeration with archive
suppression. Host progress persists examined, archived-skipped, and emitted
counts and renders those exact terms.

Provider order for a new item comes from yt-dlp's provider playlist index, not
the delta array position. The host shifts colliding retained ranks in one
transaction. Delta absence never changes an existing item.

A reconcile command invokes `mode = full` against an empty private scratch
checkpoint but does not clear or promote the live archive until its candidate
snapshot validates. It records first/second missing observations, reappearance,
and provider reordering while retaining operator fields. Publishing accepted
removals atomically promotes the staged full archive that belongs to the
validated candidate; rejecting the candidate leaves the prior archive
unchanged.

### Playlist metadata

The official package separates bounded playlist-header extraction from entry
delta extraction. Header fixtures must contain and preserve at least title,
summary/description, tags, channel/uploader people, artwork, and stable GUIDs.
An empty item delta still commits changed untouched provider fields while
preserving operator overrides and recording conflicts.

Do not turn `--flat-playlist` omissions into empty provider values. If the
dedicated header operation cannot observe a field, it returns absent and the
normal field-ownership merge applies. The host never scrapes a provider page.

### Subtitle artifacts

`media-list.artifacts.v1` accepts one committed item, effective options,
canonical primary/fallback subtitle languages, strict byte/file limits, and a
host-created output directory. Output is a typed manifest plus files. The host
validates containment, regular-file type, extension, media type, size, digest,
and language before journal staging.

The yt-dlp package uses:

```text
--skip-download
--write-subs
--no-write-auto-subs
--sub-langs <provider-mapped primary,fallback>
--sub-format <bounded preference>
```

It never adds `--write-auto-subs`. When both global language settings are
unset, the job records `not_requested_no_language_preference`; it does not use
`all`. Duplicate primary/fallback codes collapse. Actual returned provider
language tags are normalized back to ErsatzRS canonical codes.

Publication moves validated files with the NFO/YAML revision and writes
idempotent `Subtitle` rows against the generated Remote Stream metadata.
Removal or language-setting changes retire stale rows/files only when the new
revision commits. Existing item/channel/global language precedence and
subtitle mode stay unchanged.

### SponsorBlock preparation

The checked option emits `--sponsorblock-remove default`. Because that is a
file post-processor, included finite items require a durable preparation job
that produces a managed transient playback artifact before the revision is
publishable. The job also uses the selected browser-cookie option.

The cache key includes provider identity, source fingerprint, selected format,
browser enum, SponsorBlock setting, yt-dlp version, and package version. Jobs
reserve bounded disk before launch, use the add-on process group/Windows Job
Object, publish by atomic rename, and release partial files after failure.
Health reports aggregate ready/pending/failed bytes and items, never source
URLs or paths. Published artifacts stay pinned while a current/future playout
can reference their revision and are then garbage-collected.

Persist generic global and per-list processed-artifact byte ceilings, a
minimum-free-space floor, retention duration, and maximum preparation
concurrency. Admission reserves estimated bytes before process launch, refuses
work before breaching either ceiling or the floor, and reports required,
reserved, free, pinned, and reclaimable byte counts. Least-recently-used
cleanup may remove only unpinned completed artifacts; current/future playout,
retained revisions, and active jobs pin their exact keys. **Clear unused
cache** runs through the same ownership checks and is idempotent across restart.

The list and Add-ons pages show the effective budget, used/pinned/reclaimable
bytes, preparing/failed item counts, last collection, and exact disk blocker.
No cache setting or cleanup action may turn the transient processed copy into a
Local-library item or permanent media collection.

The UI states that checked removal prepares a temporary processed copy and may
delay publication/use disk. Live and unknown-liveness rows cannot select it;
the control is disabled with the reason “SponsorBlock removal requires finite
media.”

### External-tool maintenance

`tool.update.v1` is a generic bounded add-on operation plus a declared
self-update permission. The official package runs the configured executable as
an argv array with `-U`, probes `--version` before and after, and maps output to
typed outcomes. The host never invokes a shell and never parses yt-dlp text.

For an accepted regular self-updating executable, the host records the size
and digest and stores one bounded private last-known-good copy before launch.
After `-U`, the package validates executable startup, the reported version, and
extractor-registry loading before the host releases the lease. Validation
failure restores the prior bytes atomically and returns `rolled_back`; failed
restore quarantines the tool and blocks invocations with exact remediation.
Package-manager, immutable, and read-only installations skip backup and return
`manual_update_required` without mutation.

One startup trigger is created on every host start while the add-on is enabled.
A persisted daily trigger is also due no later than 24 hours after the last
completed attempt. Claims coalesce only overlapping triggers; a later restart
still performs its required startup attempt. A tool lease drains/blocks new
yt-dlp work during replacement and does not terminate already running channel
playback.

Required manual-remediation copy is:

> yt-dlp could not update itself because this executable is read-only or is
> managed by another package manager. Update that installation with its package
> manager, or select an official standalone yt-dlp executable, then choose
> Retry update.

Health distinguishes stale version, update failure, and manual-update-required
without treating an already-current outcome as an error.

**Check for update now** enqueues the same durable command and does not bypass
the shared lease, `Retry-After`, failure backoff, or next eligibility. A bounded
redacted update history records startup/daily/manual trigger, before/after
version and digest, validation, rollback, outcome, and next attempt.

## Provider-neutral capability map

| Capability | Host responsibility | Package responsibility |
| --- | --- | --- |
| `media-list.options.v1` | Render, localize, validate, inherit, bulk-preview/undo, persist, snapshot, and pass declared effective values. | Declare list/item scopes and interpret only supported values. |
| `media-list.source-check.v1` | Schedule and durably deduplicate probes, validate and retain bounded change observations, derive the adaptive next due instant, and enqueue refresh only after change. | Perform a bounded cheap check, use conditional validators where supported, and report changed/unchanged plus bounded source-update instants without exposing secrets. |
| `media-list.import.v2` | Own generation, strategy counters, checkpoint promotion, delta merge, reconcile tombstones, leases, bounds, and reset. | Produce bounded header/delta/full records, declare safe early-stop strategy, and operate yt-dlp archive arguments. |
| `media-list.artifacts.v1` | Create scratch root, validate manifest/files, publish atomically, project tracks. | Fetch only requested human subtitles and describe returned artifacts. |
| `remote-stream.play.v3` | Pass published option snapshot and own cancellation/output limits. | Apply browser cookies and select direct or prepared media behavior. |
| `tool.update.v1` | Schedule, lease, back up eligible executables, bound, validate, roll back/quarantine, persist history, show health, and enforce permission review. | Run `-U`, validate startup/version/extractor registry, and normalize the result. |

Frozen capability versions remain accepted. A package gains no new behavior
until it declares the corresponding version.

## Implementation map

| Area | Likely owners |
| --- | --- |
| Shared records and manifest validation | `crates/ersatzrs-addon-contract`, `docs/reference/addon-media-lists.md`, add-on package reference |
| Cadence, refresh-publication policy, option, revision, job-event, and blocker types | `ersatzrs-domain`, `ersatzrs-core` repository traits |
| Durable jobs/events, leases, archive/reconcile/artifact storage, revision references, projection | `ersatzrs-infra` add-on-list import/sync/repository, scan coordinator, health |
| Request-to-command boundary | `ersatzrs-app` commands/queries; web handlers only enqueue and redirect/return typed state |
| Admin UX | add-on-list pages/CSS/server functions, operation timeline, source/diff/impact/history views, shared dialog/table fixes, search cards, new Remote Stream details route, every language module |
| Subtitle playback | scanner metadata projection, infra playback selection, IPTV/HLS transport and FFmpeg argv snapshots |
| yt-dlp behavior | official add-ons repository manifest, Deno adapters, POSIX/Windows entrypoints, provider fixtures |
| Acceptance | focused Rust tests, contract validator, hydration lint, Chromium/Firefox, six-RID package tests, Windows packaged dogfood, smoke/TSDuck where playback changes |

## AMM-J — Crash-safe publication recovery

AMM-J remains the release blocker and is specified in
`addon-list-publication-recovery.md`.

### AMM-J-01 — Durable publication and scan exclusion

- Enqueue exact-revision publication outside the request.
- Give every scan origin and publication/reset/recovery one per-library lease.
- Admit a publication child scan only with the parent owner token.
- Persist the first error before rollback and resume expired phases at startup.
- Reproduce scheduled-scan overlap plus request cancellation and Windows
  sharing violation as separate fault injections that converge to recovery.

### AMM-J-02 — Windows/POSIX atomic recovery

- Make every directory swap marker-driven and idempotent.
- Drain scanner/child handles and retry only transient sharing/access failures.
- Restore files, projection, playlist, artwork, subtitles/options, and revision
  as one coherent previous or new generation.

### AMM-J-03 — Truthful blocked publication

- Implement the action-state table and structured remediation projection.
- Add Retry recovery and Restore previous revision for hydrated/no-JS paths.
- Show **Publishing** only for a live executing lease; all stranded states show
  **Publish blocked** and the immediate exact alert.

### AMM-J-04 — Forensic operation history

- Persist the bounded causal event contract with one correlation ID across
  enqueue, lease wait, child scan, filesystem transitions, retries, recovery,
  and terminal outcome.
- Show the localized Activity timeline and Copy redacted diagnostics in the
  editor and equivalent aggregate state in Health/runtime audit.
- Prove request cancellation and Windows sharing violation produce different
  original event classifications even though both converge to recoverable
  journal state; prove event order, cap/truncation, retention, restart, and
  no-private-data invariants.

## AMM-K — Operator-safe list lifecycle and navigation

### AMM-K-01 — Provider-neutral option and lifecycle persistence

- Implement ADR 0085's option inheritance, draft/published snapshots, cadence
  and refresh-publication policy, reset/reconcile jobs, tombstones, retained
  revision references, strategy/checkpoint, artifact budget/ownership, tool-
  maintenance history, and application traits.
- Add migration and downgrade/legacy-read tests without provider-ID branches.
- Extend health/runtime audit with aggregate job/checkpoint/artifact state.

### AMM-K-02 — Reset, inline metadata, and optional numbering

- Implement complete unload/reload with exact confirmation and restart proof.
- Remove both Edit metadata toggles; keep list/item controls always visible and
  make the hero title a real input.
- Remove missing episode numbers from blockers and render Unnumbered through
  admin/search/NFO/guide paths.
- Switch the list table to Name then Id and cover 320 px, 200% zoom, keyboard,
  no-JS, Chromium, and Firefox.

### AMM-K-03 — Remote Stream details from search

- Add the canonical details route/read model and consistent card href.
- Show owning add-on list, effective metadata, tracks, liveness, duration,
  availability, published options, and playback health.
- Prove `/search?query=addon_list%3a8`-shaped results navigate like movie and
  episode cards without assuming list ID 8 exists in fixtures.

Implemented 2026-09-04 with a typed, replacement-aware detail read model and a
localized SSR route. The cross-browser scenario is checked in; its
Chromium/Firefox execution remains part of the integrated AMM-N-03 closeout.

### AMM-K-04 — Preview impact and retain bounded revisions

- Add durable Test source, saved-draft publication diff, reset downstream-
  impact counts, three-revision comparison, and durable Restore revision.
- Reference-count retained revision files/artifacts and protect active/future
  playout pins from cleanup.
- Close the shared dialog focus-containment/`aria-labelledby` and data-table
  keyboard-sort/`aria-sort` dependencies for hydrated and no-JS flows.

## AMM-L — Cadenced incremental refresh and rich headers

### AMM-L-01 — Fixed and adaptive refresh cadences

- Migrate the Boolean, persist next due/retry state, and use one scheduler path
  for legacy and resumable providers.
- Add opt-in adaptive scheduling for providers that declare a bounded
  source-change probe. Learn from a bounded history of actual source changes,
  use conditional RSS/Atom validators when available, predict shortly before
  the median observed interval, clamp probes between one hour and 30 days, and
  fall back to daily until at least three usable intervals exist.
- Prove adaptive/daily/weekly/monthly/yearly/never, calendar clamping, changed
  versus unchanged probes, cold-start fallback, missed-prediction spacing,
  manual refresh, restart, failure backoff and Retry-After precedence, disabled
  add-on, shared-limiter jitter, and duplicate tick behavior. An unchanged
  probe must not launch a full import or enter the source-change history.

### AMM-L-02 — Transactional yt-dlp archive

- Add import v2 delta semantics and staged archive promotion.
- Use `--download-archive` plus `--force-write-archive` in official POSIX and
  Windows paths.
- Distinguish archive suppression from safe early stop. Use
  `--break-on-existing` only for a package-declared stable newest-first source,
  and persist examined/skipped/emitted counters; reordered fixtures must fall
  back without missing an unseen identity after a known one.
- Never combine lazy-playlist with early stop in this contract; prove any lazy
  skip-known path with frozen tool-version argv/output fixtures.
- Prove a second refresh with no new identities does not re-emit old items, a
  third refresh imports only additions, failed validation does not advance the
  archive, and reset clears it.

### AMM-L-03 — Complete playlist metadata with empty deltas

Source map:

- ErsatzTV UI, validators, handlers, and repositories: none; repository-installed
  add-on drafts are the ADR 0085 ErsatzRS extension.
- Official provider package: the yt-dlp Deno import adapter performs the bounded
  header and archive-filtered entry operations for both native entrypoints.
- ErsatzRS host: `ersatzrs-infra` validates the existing provider-neutral list
  record and merges the committed header under the import finalization fence.
- Side effects: only list draft metadata and its existing override/conflict state
  change; item reconciliation and refresh publication remain in AMM-L-04/L-05.

- Add bounded header extraction and map summary/description, tags, people,
  artwork, and stable identifiers.
- Prove untouched fields update, overrides/conflicts survive, zero-new-item
  refresh succeeds, and flat-playlist omissions never erase richer metadata.

### AMM-L-04 — Non-destructive full-source reconciliation

Source map:

- ErsatzTV has no repository-installed add-on draft/reconcile equivalent; ADR
  0085 owns this ErsatzRS extension.
- `ersatzrs-app` admits one durable Reconcile operation through the existing
  managed-library coordinator. `ersatzrs-infra` runs import v2 in Full mode
  against a scratch archive, merges only after complete validation, and reuses
  the existing draft-diff and publication transaction.
- The existing `/media/addon-lists/:id` island owns the Reconcile and confirm
  actions. Matching native POST forms remain the no-JavaScript path; action
  status stays mounted so hydration does not change the page shape.
- Side effects are limited to the saved draft, reconciliation tombstones, and
  the candidate archive. The current publication and operator-owned item state
  remain unchanged until explicit Publish.

- Add Reconcile entire source using a full scratch generation and staged full
  archive while retaining publication, overrides, inclusion, numbering,
  replacements, and options.
- Record first missing observation, require a second complete observation at
  least 24 hours later or explicit confirmation, clear tombstones on
  reappearance, and never advance absence on incomplete/failed output.
- Preview additions, changes, reordering, reappearances, and removal candidates
  before explicit Publish; keep Reset and Reload as the separate clean slate.
- Reuse the existing lifecycle/import/publication boundaries rather than adding
  a reconcile-specific executor or diff model. Automatic publication policy,
  notifications, and speculative provider-specific absence rules remain in
  AMM-L-05 or the provider contract that owns them.

### AMM-L-05 — Safe refresh publication policy

- Confirmed scope: only scheduler-admitted refreshes may auto-publish, and a
  list needs one explicit initial Publish. Manual Refresh, Reconcile, and
  Reset and Reload remain review operations.
- Source map: this is the ADR 0085 extension with no ErsatzTV equivalent.
  Reuse `addon_media_list_refresh` admission, import finalization, the retained
  revision snapshot comparison, and the managed-library publication lease.
  One list-owned assessment retains the import identity, saved/baseline
  revisions, outcome, typed hold reason, and publication operation. The
  existing import worker resumes assessments after restart.
- UI source map: `/media/addon-lists/{id}` uses its current settings form,
  island, server function, and native POST fallback. Place the policy selector
  beside cadence and show the current typed hold in Activity. Reuse existing
  layout/primitives and translate all labels and the Health projection.
- Automatic admission checks the complete published baseline and saved draft
  in one transaction; only new included identities and their displacement of
  existing ranks may differ, with retained relative order unchanged. Required
  preparation uses the current detail queue without clearing retry backoff.
  Preexisting unpublished additions and intervening manual refreshes remain
  review-only. Concurrent operator changes,
  changed policy, or a different baseline invalidate the automatic candidate.
- Verify focused classification, admission/restart, preparation, policy-save,
  localized hold and browser paths; review/fix P1/P2 before a local commit.
  No CI or full closeout is run during this slice.

- Persist Review changes before publishing as the default and optional
  Automatically publish validated additions.
- Permit automatic publication only for a pure additions set through the
  normal exact-revision lease; every conflict, existing-item change,
  tombstone/removal, restriction, unavailable item, schema change, or failed
  preparation remains a draft and creates an in-app activity/Health notice.

## AMM-M — yt-dlp item access, SponsorBlock, and subtitles

### AMM-CI-01 — Repair CI before starting AMM-M-02

Maintainer-requested priority on 2026-09-05; AMM-M-01 remains reserved and paused.

- Repair the confirmed failures in run 33957746900: narrowly allow independent
  reference-pin booleans, align browser fixtures and locators with the current
  controls/contracts, and make the legacy namespace test verify retained but
  invalidated checkpoints while preserving successful-refresh assertions.
- Preserve runtime behavior and meaningful assertions. Do not add production
  compatibility code for obsolete test fixtures or restructure independent pins
  into a state machine merely to satisfy a lint.
- Browser traces additionally prove that the add-on-list index and Remote Stream
  detail remain on Loading without JavaScript. Set only these routes to in-order
  SSR so their existing blocking resources render visibly without script-driven
  fragment insertion. Keep the existing islands, data boundaries and native forms;
  no app-wide rendering-mode or loader rewrite. Verify hydrated and no-JS flows.
  Source map: `/media/addon-lists` uses `AddonMediaListsPage` and
  `AddonMediaListsIsland`, with page-size, list and provider server functions;
  `/media/remote/streams/:id` uses `RemoteStreamDetailPage`/`RemoteStreamDetailView`
  and `get_remote_stream_details`. Existing native GET sorting forms and channel
  links are the fallback paths. Only route SSR mode changes; no shared primitive
  or API changes. The metadata-editor and remote-streams specs own browser proof.
- Run focused Rust and affected browser checks serially, checking for active
  self-hosted CI first; cap Cargo jobs and nextest threads at two. Cargo-leptos
  runs SSR/WASM builds concurrently, so use one Cargo job per build. Review and fix
  P1/P2 findings before a scoped local commit, keeping unfinished M-01 work out.
- No full closeout or automatic push. Obtain maintainer push approval and
  terminal green CI on the published repair SHA before starting AMM-M-02.
- The 2026-09-05 approved display refinement uses `S01E01` throughout the admin
  UI, as recorded in ADR 0085. Source map: `i18n::episode_display_subtitle_text`
  and the nine culture formatters feed search, cards, collections/playlists,
  playouts, season and Remote Stream details, trash and media-info JSON through
  existing display adapters. `SeasonDetailPage`/`EpisodeRow` also use the shared
  identity for visible row titles. SSR and island callers share this formatting;
  routes, native forms, server-function signatures, repositories and stored
  projection strings stay unchanged. Proof targets are all-culture formatter
  and display-adapter tests plus the season heading/media-info browser scenario.

### AMM-M-01 — Per-item browser cookies and SponsorBlock

Implementation plan (approved choices, 2026-09-04; delivery remains in progress):

- Source map: ADR 0085 owns this extension; there is no ErsatzTV browser-option
  or SponsorBlock editor to port. Reuse the existing option schema contract,
  lifecycle repository's sparse defaults/overrides and immutable publication
  snapshots, managed publication coordinator, and Remote Stream input path.
  The official yt-dlp package owns browser extraction and file post-processing;
  the host must not branch on its add-on ID or option names.
- Visual source map: extend the existing Add-on List settings and item review
  controls, shared fields/dialogs, and native POST fallbacks. Show localized
  effective/inherited values and finite-only unavailability in all cultures;
  do not redesign the list or unrelated review bulk actions.
- Activate validated installed schemas on an explicit mutation, never a GET.
  Render legacy lists with provisional draft defaults, but preserve their
  existing published behavior until an explicit save and successful Publish.
  Reuse the complete option-snapshot save with draft-revision and publication
  fences. Bulk preview captures explicit item IDs, schema identity and draft
  revision; apply is atomic and one-step session-local undo rejects intervening
  edits. Undo also reverses first-time schema activation. Reset removes the
  sparse override rather than storing null. Provisional defaults are display
  state only until an explicit save binds the draft schema.
- Use the saved draft options for Test access and pre-publication enrichment
  and preparation. Channel playback and published metadata use the immutable
  published options until Publish succeeds. Option changes invalidate draft
  results without mutating the last complete publication. This distinction was
  explicitly approved by the maintainer.
- Add versioned options-aware item enrichment, access-check, preparation and
  playback contracts; leave frozen import v1/v2 and playback v1/v2 unchanged.
  Discovery stays unchanged. Declare finite preparation requirements in the
  generic Boolean option schema, rather than introducing a provider-specific
  host branch or an expression language. Preserve old schema serialization and
  digests when the new optional declaration is absent.
- Queue access checks and preparation as bounded durable per-item work, outside
  the publication lease. Reuse provider admission/backoff and process ownership;
  freeze source, schema/options, package/tool and selected-format identity for
  each attempt. Generation-fence completion and invalidate stale results. Return
  only typed redacted access outcomes, never provider stderr, profile paths,
  cookie bytes, headers or temporary signed URLs. Tests use synthetic profiles;
  real browser profile handling remains a human-assisted boundary.
  Keep access/preparation lease and retry state in one item-operation job table;
  access-only work must not masquerade as an artifact. Existing artifact rows
  continue to own admitted files and revision pins. Waiting for shared provider
  admission does not consume an execution attempt. Access-evidence reads check
  saved context and expiry without rewriting historical results; the worker/UI
  consumer must additionally compare the newly observed local tool version.
- Both official package transports pass only the validated lowercase browser
  enum. Finite preparation uses yt-dlp's standard
  `--sponsorblock-remove default` file post-processing, not stdout playback or
  a separate SponsorBlock client/cutter. Distinguish no matching segments from
  failed preparation; probe the final duration and use it for scheduling.
  The audited yt-dlp 2026.08.19 postprocessor treats a SponsorBlock HTTP 404 as
  no segments and raises on exhausted API retries; do not use `--ignore-errors`
  to turn that failure into an apparently prepared uncut file. Keep the standard
  cut behavior, without adding custom cutting or mandatory full re-encoding.
  Live/unknown items retain direct playback and the explicit finite-only reason.
- Use the existing artifact rows and reference pins. Reserve capacity before
  launch, bound child output/runtime and staging disk use (including temporary
  files), validate the completed file, and atomically promote it under host
  ownership. Cancellation/restart must not expose partial media. Bind runtime
  resolution to the owning list/item and published revision, not a URL lookup;
  prepared files remain Remote Stream artifacts, never permanent Local media.
- Approved initial admission limits: one preparation at a time, 20 GiB global,
  10 GiB per list, 2 GiB maximum prepared file, 2 GiB free-space floor, and
  seven-day retention for unpinned files. Oversized/full-cache work remains
  blocked with a useful reason. Keep the policy foundation reusable by M-03;
  storage administration, usage breakdowns, explicit cleanup and full LRU
  management remain M-03 work. Never evict revision/playout-pinned files.
- Verify schema compatibility and inheritance; revision-fenced preview/apply/
  undo; draft/published isolation; typed access expiry/redaction; finite/live
  behavior; quota refusal; cancellation/restart and stale completion; and a
  focused publication-to-channel fixture that actually plays processed media.
  Review and fix P1/P2 before the slice's local commit. Subtitles acquisition,
  updater lifecycle, integrated six-RID/browser/TSDuck closeout and CI remain
  their named later slices. Do not mark M-01 complete with runtime wiring absent.

- Render list defaults plus nullable item overrides for the exact browser enum
  and checked-by-default SponsorBlock option, with effective/inherited state,
  Reset to list default, and previewed atomic bulk edit/undo.
- Pass browser selection through enrich, subtitle, preparation, and playback;
  never persist or log cookie material.
- Add durable per-item Test access with typed redacted profile/provider outcome
  and expiry on URL, package/tool, or option change.
- Build bounded SponsorBlock finite-media preparation using
  `--sponsorblock-remove default`, and prove cache invalidation, disk refusal,
  cancellation, restart, and channel playback with removed segments.
- Prove live rows explicitly reject/disable the option without harming direct
  playback.

### AMM-M-02 — Human subtitles selected by FFmpeg language settings

- Materialize only manually authored primary/fallback subtitle files through
  the artifact contract with `--no-write-auto-subs` explicit.
- Project idempotent subtitle rows and retain them through publication/recovery.
- Prove primary selection, fallback selection, no-unrelated-language fallback,
  no auto-caption import, item/channel override precedence, MPEG-TS, HLS, and
  real channel viewing.

### AMM-M-03 — Bound processed-artifact storage

- Persist global/per-list byte ceilings, minimum free space, retention, and
  preparation concurrency; reserve before launch and report exact disk blockers.
- Expose used, pinned, reclaimable, pending, and failed state plus Clear unused
  cache; use LRU only for unpinned artifacts and retain every active/future
  playout and retained-revision reference.
- Prove quota races, low disk, cancellation, restart, cleanup idempotency, and
  that no temporary SponsorBlock artifact becomes permanent Local media.

## AMM-N — yt-dlp maintenance and integrated closeout

### AMM-N-01 — Startup and daily yt-dlp self-update

- Add permission-reviewed `tool.update.v1`, startup/daily triggers, deduped
  claims, version evidence, and the per-tool execution lease.
- Prove updated, already-current, manual-update-required, network failure,
  read-only executable, restart, and concurrent playback/discovery behavior on
  Windows and POSIX.
- Keep `--no-update` on every non-maintenance operation.

### AMM-N-02 — Validate and roll back tool replacement

- Store one bounded last-known-good copy for an eligible accepted executable,
  record before/after version and digest, and validate startup, version, and
  extractor-registry loading before releasing the tool lease.
- Restore exact prior bytes on failed validation, quarantine with exact
  remediation if restore fails, and never mutate package-manager/immutable/
  read-only installations.
- Add Check for update now plus bounded redacted update history using the same
  lease, Retry-After, backoff, and next-eligibility state as automatic attempts.

### AMM-N-03 — Cross-platform and release acceptance

Release follow-through requested by the maintainer: after M-01 through N-03
are complete and N-03 review feedback is fixed, retain the approval checkpoint
before making CI green and pushing. Once authorized and CI is green for the
exact final commit, create a new beta release containing only the Windows
package (`win-x64`). Verify the published Windows archive and its build identity;
do not publish Linux or macOS packages for this beta. This limited release asset
selection does not weaken N-03's six-platform verification requirement.
The maintainer has explicitly pre-approved creating and publishing this beta
once those conditions hold; no second release-approval prompt is required.

- Run contract fixtures for every new capability in the host and official
  add-ons repositories.
- Pass focused Rust, migration, health, hydration, localization, Chromium, and
  Firefox tests.
- Pass native/package checks for all six supported RIDs, with real Windows
  browser-profile failure guidance and NTFS recovery fault injection.
- Run channel MPEG-TS/HLS smoke and TSDuck playback proof for a subtitle item
  and a SponsorBlock-prepared item.
- Prove source/diff/reset-impact/history UX, shared dialog/table accessibility,
  reordered early-stop fallback, two-observation reconciliation,
  additions-only publication, cache quotas/pins, and update rollback/quarantine.
- Repeat the original overlap shape: scheduled scan active, Publish submitted,
  browser disconnected, host restarted, recovery completed in the UI, one
  exact revision published, distinct causal diagnostics, no stranded journal,
  and no manual database or filesystem repair.

## Acceptance traceability

| Requested behavior | Owning milestones |
| --- | --- |
| Publish blocked label and exact alert | `amm-j-03` |
| Scheduled-scan/request-cancellation/Windows recovery defect | `amm-j-01`, `amm-j-02`, `amm-j-03`, `amm-j-04`, `amm-n-03` |
| Reset and reload URL from a completely unloaded list | `amm-k-01`, `amm-k-02` |
| Firefox/Chrome/Chromium cookies per video | `amm-k-01`, `amm-m-01` |
| SponsorBlock checkbox checked by default | `amm-k-01`, `amm-m-01` |
| Human subtitles using default/fallback languages and channel playback | `amm-m-02`, `amm-n-03` |
| Rich playlist summary/tags metadata | `amm-l-03` |
| Adaptive/daily/weekly/monthly/yearly/never refresh | `amm-l-01` |
| Proper incremental `archive.txt` | `amm-l-02` |
| Name before Id | `amm-k-02` |
| Search Remote Stream details | `amm-k-03` |
| Startup and daily `yt-dlp -U` | `amm-n-01` |
| Optional episode numbers | `amm-k-02` |
| Always-visible inline metadata and editable top title | `amm-k-02` |
| Bounded causal operation history and redacted diagnostics | `amm-j-04` |
| Source/publication/reset impact preview and bounded revision restore | `amm-k-04` |
| Safe yt-dlp early stop with observable enumeration counters | `amm-l-02` |
| Non-destructive provider removal/reordering reconciliation | `amm-l-04` |
| Review-by-default and additions-only automatic publication | `amm-l-05` |
| Inherited/bulk item options and browser access preflight | `amm-m-01` |
| Processed-artifact disk budgets and safe reclamation | `amm-m-03` |
| Validated yt-dlp update rollback and history | `amm-n-02`, `amm-n-03` |
| Dialog focus containment and accessible table sorting | `amm-k-04` |

## Required verification evidence

Planning is complete when every request maps to an open milestone. Delivery is
complete only when all of the following exist:

- repository/application state-machine tests with paused time and process-death
  fault injection;
- ordered/capped/retained operation-event and redacted-diagnostic tests that
  distinguish original failure from recovery failure;
- NTFS and POSIX handle-sharing recovery tests;
- SQLite migration/cardinality/restart tests;
- frozen official yt-dlp argv and output fixtures for Windows and POSIX;
- archive early-stop/reordered-source fixtures proving no unseen identity can
  be skipped, plus two-observation reconcile/tombstone/reappearance tests;
- no-secret assertions over stdout, stderr, database diagnostics, health, and
  snapshots;
- browser proof in Chromium and Firefox for hydrated and no-JavaScript paths;
- keyboard focus containment, `aria-labelledby`, keyboard sorting, `aria-sort`,
  source/diff/impact/history, option inheritance/bulk undo, and activity proof;
- matched SSR/WASM hydration and localization completeness;
- actual subtitle selection and SponsorBlock-removed channel playback, not
  merely command construction;
- low-disk/quota/LRU/pin acceptance plus update validation, atomic rollback,
  quarantine, and bounded history; and
- packaged Windows dogfood reproducing and clearing the original overlap
  scenario entirely through the admin UI.

## References

- [ADR 0085](../docs/adr/0085-broaden-addon-list-lifecycle-and-provider-options.md)
- [Add-on-list publication recovery](addon-list-publication-recovery.md)
- [Add-on-list import and review UX](addon-list-import-review-ux.md)
- [Add-on media-list current contract](../docs/reference/addon-media-lists.md)
- [Global subtitle-language decision](../docs/adr/0052-global-subtitle-language-preferences.md)
- [yt-dlp options](https://github.com/yt-dlp/yt-dlp#usage-and-options)
- [yt-dlp archive FAQ](https://github.com/yt-dlp/yt-dlp/wiki/FAQ)
- [yt-dlp installation and update methods](https://github.com/yt-dlp/yt-dlp/wiki/Installation)
