# ADR 0079: Discover add-on lists before deferring item details

## Status

Accepted (2026-08-23)

## Implementation

Implemented (2026-08-23)

## Context

The current `media-list.list` path invokes one add-on process for the complete
list, applies a 60-second wall-clock timeout, and accepts output only after the
process exits. This is a useful trust boundary for a small atomic operation,
but it does not scale to a provider list whose item metadata requires separate
network requests.

The official Beeld & Geluid package first follows every Schatkamer result page
and then requests each programme page sequentially for rich episode metadata.
An individual provider request is already bounded and retried, but the host's
single 60-second deadline covers the sum of all requests. At an observed rate
of roughly one second per programme, a sufficiently large series predictably
crosses the host deadline. The add-on is killed, its temporary output is
discarded, and the administrator sees only `entrypoint timed out` without
useful progress. The public series *De wereld van Boudewijn Büch* is a
representative acceptance source.

Increasing the one-shot timeout would postpone the failure but would keep the
operation opaque, non-resumable, and vulnerable to losing minutes of valid
work after one slow request or a server restart. It would also make adding a
series, list, or search URL trigger every item detail page immediately. That is
avoidable traffic for Schatkamer fair use and for yt-dlp-backed sources, and it
forces advertising or other filler to pay for metadata it may never use.

Provider overview pages already expose useful card metadata such as title,
first-air or release date, thumbnail, and description when available. The
operator should be able to review and publish that complete overview snapshot
first, then let detail extraction proceed slowly after publication or request
only selected items manually.

## Decision

Add an optional provider-neutral `media-list.import.v1` orchestration
capability. This capability versions transport and resumability separately
from the frozen `media-list.list.vN` record schemas. A package declares both
the orchestration capability and the record versions it can produce; the host
selects the highest mutually supported record schema and validates every
returned record through that existing parser.

The orchestration protocol has two bounded operations:

1. **Discover** returns the list header and one bounded overview page of stable
   item stubs in provider order, plus an optional total hint, opaque
   continuation cursor, provider revision token, and detail fingerprint.
2. **Enrich** accepts exactly one committed item stub and returns one complete
   detail record or a typed unavailable, transient, or permanent outcome.

Discover never opens an item detail page as a fallback. It retains every cheap
field the provider exposes, including stable identity and order, source or
detail URL, title, first-air or release date, thumbnail, outline or plot,
availability, classification, and playback-critical values when available.
A missing overview value stays missing and cannot be fabricated.

The list stores one explicit detail-import policy. `automatic` is the default
and is presented as **Load item details after publishing**. `manual_only` is
presented as **Overview only; fetch details manually**. ErsatzRS never infers
the policy from title, genre, or collection type. Changing policy preserves
completed detail data, operator overrides, inclusion, and discovery
checkpoints; enabling automatic work queues only missing or stale details.

The host sends bounded JSON requests on standard input and receives bounded
NDJSON. Cursors are untrusted opaque values with strict size limits; the host
detects repeated cursors, no-progress pages, duplicate provider IDs, rank
conflicts, excessive page/item counts, and output-limit violations. Raw
cursors and provider response bodies are not written to ordinary logs.

The host owns a durable overview generation and commits every validated page
before requesting the next. One active generation and a database
lease/heartbeat prevent duplicate workers. Provider consistency is checked
before the overview becomes Ready: a revision token must remain stable, or the
host performs a second overview-only pass and compares the ordered identities.
An incomplete or mixed pagination observation cannot publish.

The verified overview enters the existing ADR 0075 draft and may be explicitly
published without waiting for optional item details. First publication remains
the only action that creates library projection and playlist membership.
Publishing with automatic details enabled explicitly authorizes subsequent
provider-owned enrichment for already published stable item identities.
Operator overrides continue to win. New identities, removals, inclusion or
classification changes, playback-semantic changes, conflicts, and invalid
values remain review-gated. Completed safe updates are projected in coalesced
scanner batches; one result never triggers one full library scan.

The host persists separate per-item states and uses atomic claims so automatic
and manual requests cannot fetch the same item concurrently. Automatic work is
eligible only after first publication. A per-row **Fetch details** action may
request one item before or after publication and uses the same claim, retry,
merge, and projection path. Manual work receives queue priority but cannot
interrupt an in-flight operation or bypass provider pacing.

There is no job-wide wall-clock timeout. Every invocation remains bounded by
host-owned time, byte, and record caps. Detail extraction is serial per
provider host. After every item attempt, successful or failed, the host samples
a new delay from the inclusive global range configured under Add-ons before any
list may start another detail operation against that host. The positive
whole-second minimum and maximum default to 2 and 16, require minimum no greater
than maximum, and apply to subsequent samples without a restart. Saving a
shorter range never shortens an already persisted next-eligible time. The
limiter is shared across jobs so more lists or manual requests cannot multiply
traffic. A package may be stricter, while provider `Retry-After` and capped
retry backoff win when they require a later time.

A browser disconnect, ErsatzRS restart, add-on timeout, or transient provider
failure resumes from committed state. Exhausting automatic retries leaves the
item visibly failed with an explicit Retry action; it is never omitted. The
Import/detail page polls durable progress approximately once per second while
visible and backs off while hidden or disconnected. The database remains the
sole progress source, so SSE or WebSockets are not required initially.

The item overview adds dedicated localized **#**, **First aired**, and
**Season / episode** columns. The number is the stable one-based provider rank,
not a page-local or episode number. While bulk episode numbering is open, the
UI previews and non-colour-only highlights the exact eligible rows and proposed
values using the same rules as the application command, including targets on
other pages.

Existing `media-list.list.v1` through `v5` capabilities remain frozen as the
bounded one-shot compatibility path. The host prefers
`media-list.import.v1` when declared and gives a specific update-capability
diagnostic when a legacy package exceeds its one-shot deadline.

Official Beeld & Geluid and yt-dlp packages adopt the same orchestration.
Beeld & Geluid maps overview pagination to Discover and one programme page to
Enrich. yt-dlp remains the extractor: flat playlist/search output drives
Discover, and one full entry extraction drives Enrich. Package scripts only
bound and translate provider output. ErsatzRS gains no provider-specific
scraping, pagination, or status vocabulary.

## Consequences

- Overview discovery may take longer than one request, but valid page progress
  survives failures, browser navigation, and host restarts.
- Automatic details may take minutes or hours by design, while the verified
  overview remains publishable and usable throughout.
- Manual-only filler imports stop after overview discovery and can still enrich
  any chosen item without changing the whole-list policy.
- The user sees continuing work and actionable retry state instead of an
  unchanging form followed by a generic timeout.
- SQLite and MySQL gain durable import-job, checkpoint, lease, retry, and
  per-item detail state plus startup recovery and health checks.
- The add-on runner gains bounded standard-input requests and typed per-item
  outcomes while retaining process-group/Windows Job Object ownership.
- Shared serialized pacing prevents concurrent lists or manual requests from
  multiplying provider load, at the cost of deliberately slower completion.
- Explicit publication authorizes safe incremental provider-owned enrichment;
  operator overrides and review-gated semantic changes remain protected.
- Final consistency may require an additional discovery pass. This is an
  intentional correctness cost for providers without snapshot tokens.
- Old packages remain compatible, but they do not gain resumability until they
  declare the new capability.
- The same host workflow serves Beeld & Geluid, yt-dlp, and future providers;
  provider-specific optimizations stay inside their packages.

## Alternatives Considered

- Raise or remove the 60-second timeout: rejected because one hung process can
  consume resources indefinitely and all progress is still lost on failure.
- Parse streaming stdout from the existing one-shot process: rejected because
  process output is not a durable semantic checkpoint and cannot safely resume
  provider pagination or incomplete item enrichment.
- Store checkpoints only in add-on temporary files: rejected because package
  updates, host restarts, platform differences, and the add-on trust boundary
  make host-owned import state authoritative.
- Infer manual-only behavior from the selected collection type: rejected
  because an Other Video or Movie list is not necessarily filler and an
  operator must retain an explicit, reversible choice.
- Fetch all programme pages with high concurrency: rejected because it creates
  provider load and rate-limit instability. One detail operation per provider
  host plus an irregular operator-configured delay, defaulting to the inclusive
  2–16 second range, is the deliberate policy.
- Finish all details before first Publish: rejected because it recreates the
  timeout and fair-use problem and makes optional metadata gate availability.
- Publish each discovered overview page immediately: rejected because users
  could see an incomplete or internally inconsistent list. The complete
  paginated overview is the atomic first-publication unit.
- Let the manual action bypass pacing: rejected because repeated clicks or
  several administrators could defeat the fair-use boundary.
- Add SSE or WebSockets immediately: rejected for the first slice because
  durable one-second polling provides responsive progress with simpler
  reconnect, restart, SSR, and cross-platform behavior.

## References

- [ADR 0066: Repository-backed add-ons](0066-repository-backed-add-ons.md)
- [ADR 0075: Review add-on metadata before library publication](0075-review-addon-metadata-before-publication.md)
- [ADR 0078: Separate the Remote Stream type from live-source semantics](0078-separate-remote-stream-type-from-live-source-semantics.md)
- [AMM implementation evidence](../progress/parity/addon-metadata-management.toml)
- [Add-on media-list contract reference](../reference/addon-media-lists.md)
