# ADR 0019: Smart Shuffle for schedule content selection

## Status

Accepted 2026-07-17

## Implementation

Implemented 2026-07-17 and redesigned 2026-07-19. This feature was formerly
named Similar Video Cooldown. `ersatzrs-core::scheduling` owns the versioned,
disabled-by-default policy/rule/state model and deterministic soft selector.
Classic, Block, YAML, and Scripted scheduling use the selector only for
randomized sources; impossible constraints emit a warning and select the
least-blocked candidate by stable penalty, base rank, and media id. Playlist
`play_all` and counted batches are recorded as one indivisible unit with unioned
tags and duration.

`ersatzrs-infra::repositories::scheduling` hydrates artist, album-artist, show,
season, ADR 0018 parent lineage, collection, tag, genre, studio, and
search-expression tags before engine execution. Smart Shuffle state is serialized
in each engine's existing playout-anchor context and restored by Continue
builds. Policies belong to each engine's native selection unit:
`ProgramScheduleItem`, reusable `BlockItem`, Sequential YAML `content[]` entry,
or Scripted content/enumerator key. New JSON uses `smart_shuffle`; legacy
schedule/playout attachments, YAML `cooldown`, Lua `set_cooldown_policy`, and
anchor `cooldown` fields remain readable. Classic owners materialize inherited
policies when next saved; reusable Block items preserve a missing attachment
while their inherited form remains Off, until the operator explicitly chooses
an item override. A Sequential YAML content entry that explicitly declares
`smart_shuffle`, including an Off value, overrides rather than inherits the
legacy root `cooldown` policy.

The localized Classic Schedule Item and Block Item editors expose Off, Light,
Balanced, and Custom controls directly after playback order, including scope,
item-count and duration thresholds, filler participation, built-in facets, and
operator filter rules. Their draft-aware Smart Shuffle Overview reports every
selection unit and exposes all candidates through search, tag filters, and
50-row pages with localized warning states. Sequential YAML uses
`content[].smart_shuffle`; Scripted Lua uses
`set_smart_shuffle_policy(content_key, json)`. Focused core,
SQL hydration and anchor persistence, admin persistence, schema, hydration,
and desktop/mobile browser coverage protect the implemented path.

## Context

ErsatzRS has deterministic shuffle behavior in several scheduling surfaces:
Classic collection enumerators, Block scheduling, YAML content sources and
marathons, and Scripted scheduling content APIs. ErsatzTV's corresponding
shuffle enumerators avoid only narrow repeat cases, such as not starting a new
shuffle cycle with the same exact media item that ended the prior cycle. They
do not provide a general way to keep similar media apart.

Dogfood scheduling has already exposed one local version of this problem:
music-video shuffles can cluster the same artist too closely. That was fixed as
a narrow artist-distance improvement, but it does not generalize to TV shows,
movie series/franchises, parent/alternate-version relationships, collections,
smart collections, tags, studios, or operator-defined filters.

The desired capability is a configurable "similar video cooldown": after a
scheduled item plays, one or more similarity groups attached to that item enter
cooldown for a configured number of subsequent primary videos, a configured
playout duration, or both. While the cooldown is active, other candidates in the
same group should be avoided when enough alternatives exist.

The feature has to satisfy several constraints:

- Keep imported ErsatzTV schedules byte-compatible and behavior-compatible when
  the feature is disabled.
- Remain deterministic for the same playout, seed, schedule definition, and
  reference data.
- Persist enough state that Continue builds do not forget cooldowns at build
  boundaries.
- Avoid database lookups inside the scheduling hot loop; all similarity tags
  must be resolved into scheduling reference data before the engine starts
  choosing items.
- Handle impossible constraints, such as a small library where every candidate
  shares the same group, without infinite loops or dead air.
- Treat multi-item scheduling units, such as multipart episodes or playlist
  batches that must remain together, as indivisible for cooldown selection.

## Decision

Add an opt-in, shared cooldown-aware shuffle policy to the scheduling layer.
The policy is an ErsatzRS scheduling improvement, not an ErsatzTV parity
default.

The common model lives in `ersatzrs-core::scheduling` and is consumed by every
engine that performs randomized or shuffled selection. It has three parts:

- `SchedulingSmartShufflePolicy`: the configured behavior.
- `SchedulingSmartShuffleRule`: one rule that emits similarity tags for a media
  candidate.
- `SchedulingSmartShuffleState`: the persisted runtime state for active cooldown
  tags.

The first implementation should use a versioned JSON shape for policy and state
so future rule types can be added without a schema rewrite. DB-backed admin
schedules may store policy attachments in additive ErsatzRS-owned tables or
config rows, but must not change the meaning of ErsatzTV-compatible
`ProgramSchedule`, `ProgramScheduleItem`, `BlockItem`, playlist, or collection
columns. YAML and Scripted schedules should expose the same policy shape in
their native schedule definitions instead of inventing separate semantics.

### Policy shape

A policy is disabled by default. When enabled it defines:

- `scope`: where cooldown state is shared. Initial scopes are `playout`,
  `schedule_item`, and `content_source`. `playout` gives the best whole-channel
  diversification, while narrower scopes let operators keep deliberately
  themed blocks independent.
- `mode`: `soft` by default. Soft mode avoids active cooldowns when possible
  and deterministically relaxes the least-bad candidate when every candidate is
  blocked. A future explicit strict mode may fail a schedule item, but the
  initial implementation must not create dead air merely because a cooldown is
  impossible to satisfy.
- `after_items`: optional count of subsequent primary scheduling units that
  must pass before a group is eligible again.
- `after_duration`: optional playout duration that must pass before a group is
  eligible again.
- `include_filler`: false by default. Filler, pre-roll, post-roll, tail, and
  fallback media do not consume item-count cooldowns unless the policy opts in.
- `rules`: ordered similarity rules that emit zero or more tags per candidate.

When both `after_items` and `after_duration` are set, a tag remains active while
either threshold is still active. In other words, the effective cooldown is the
longer of the count-based and time-based cooldowns.

### Similarity rules

Built-in rules should cover the common cases without requiring operators to
write search syntax:

- `artist`: music videos and songs sharing an artist.
- `album_artist`: songs sharing an album artist.
- `show`: TV episodes sharing a show.
- `season`: TV episodes sharing a season.
- `content_parent`: ADR 0018 extras, alternate versions, and commentary
  variants sharing `ParentMediaItemId` or the primary item id.
- `collection`: items sharing an explicit collection or multi-collection when
  that membership is available in reference data.
- `tag`, `genre`, `studio`, and similar metadata-backed facets when already
  available in the scheduling/search projection.

Movie franchise is intentionally modeled as data-driven at first. Operators can
represent franchises with collections, smart collections, tags, or provider
metadata, and then use a built-in membership rule or a filter rule. If a future
scanner/provider slice adds a first-class franchise field, it can become a new
built-in rule without changing the policy model.

For "any other filter", support filter rules backed by the scheduling search
projection. A filter rule has a stable rule id and a search expression. Every
candidate matching the expression receives a tag for that rule id, namespaced
by selection-unit policy identity when it participates in shared playout state.
This prevents unrelated per-item policies that reuse a rule id from blocking
each other. It supports use cases such as "all Marvel movies", "holiday
episodes", "concert videos", or "videos tagged black-and-white" even when
there is no dedicated schema column.

Reference-data loading owns tag hydration. The scheduler receives candidates
with precomputed cooldown tags or enough preloaded facet data to compute them
without I/O.

### Selection algorithm

The scheduler keeps the existing deterministic seed and base shuffle order as
the starting point. Cooldown-aware selection then chooses from that base order
using a soft-constraint scorer:

1. Expire tags whose item count and duration thresholds have both elapsed.
2. Consider the next deterministic candidate window for the active enumerator
   or content source.
3. Prefer candidates with no active cooldown tag.
4. If every candidate is blocked, choose the candidate with the lowest penalty:
   fewer blocked tags, earlier cooldown expiry, lower base shuffle rank, then
   lower media item id as the final deterministic tie-breaker.
5. Schedule the chosen unit, decrement item-count cooldowns for active tags,
   and add/update cooldown entries for every tag emitted by the chosen unit.

This is deliberately not a full-day optimizer. Exact global optimization would
be expensive, harder to resume mid-build, and sensitive to fixed-time anchors.
A bounded deterministic greedy scorer gives most of the diversity benefit while
preserving online scheduling and Continue-build behavior.

The scheduling unit is the indivisible thing an engine is about to place on the
timeline. A single movie has one tag set. A multi-episode group, playlist
`play_all` batch, or future grouped item has the union of its child tags and a
duration equal to the whole unit. The cooldown decision must not split groups
that the selected scheduling mode promises to keep together.

### Persistence and compatibility

Cooldown state is runtime state, not media metadata. Persist it inside the
existing playout scheduling state path rather than by writing history-only
side effects:

- YAML, Scripted, and Block scheduling store cooldown state in
  `PlayoutAnchor.Context` alongside their existing serialized engine context.
- Classic scheduling stores cooldown state in `ClassicSchedulingContext`, still
  rooted in `PlayoutAnchor.Context`, while leaving ErsatzTV-compatible
  `PlayoutProgramScheduleAnchor` and `CollectionEnumeratorState` semantics
  unchanged.
- `PlayoutHistory` remains useful for audit and resume hints, but is not the
  canonical cooldown store because history cleanup and old detail shapes do not
  retain arbitrary group tags.

Existing playouts with no cooldown state start with an empty state. A future
implementation may seed initial state from retained recent playout items when
all required tag data is available, but that seeding must be deterministic and
must not be required for correctness.

When no policy is attached, existing scheduling snapshots and imported
ErsatzTV behavior must remain unchanged.

### Product surface

The admin UI should expose presets before advanced rule editing:

- Off.
- Light: avoid the same artist/show/parent for a small number of videos.
- Balanced: stronger count and duration cooldowns across common tags.
- Custom: explicit scopes, thresholds, and rule list.

Advanced users should be able to attach filter rules using the same search
terms used by smart collections. The UI must preview the effective tags for a
small sample of candidate media so operators can see why a policy is grouping
items together.

## Consequences

Shuffle-heavy channels can become more watchable without requiring operators to
hand-curate many small collections.

The feature creates an intentional ErsatzRS scheduling extension. It must stay
opt-in so imported ErsatzTV schedules still behave like ErsatzTV until the
operator enables cooldowns.

Scheduling reference data becomes wider because it must carry groupable facets
needed by policies. That work should be centralized in
`SchedulingMediaItem`/reference-data loading rather than duplicated per engine.

The scheduler gains state that spans generated items and Continue builds.
State serialization needs snapshot tests and compatibility tests before this
feature can be considered implemented.

Soft relaxation is required. A policy that cannot be satisfied should degrade
to deterministic best-effort diversity with a warning, not an offline channel.

## Alternatives Considered

- Keep the current artist-only distance fix: rejected because it solves only
  one symptom and would lead to one-off spacing logic for every new similarity
  concept.
- Post-process generated playout rows after scheduling: rejected because it can
  break fixed-time anchors, filler placement, guide grouping, and engine
  history.
- Use random rejection until an eligible item appears: rejected because it can
  loop for small libraries and makes relaxation behavior hard to explain.
- Solve the whole schedule with an optimizer or simulated annealing pass:
  rejected for the initial implementation because it is harder to resume,
  harder to make deterministic across partial builds, and unnecessary for the
  expected channel sizes.
- Store cooldowns only in `PlayoutHistory`: rejected because history is cleaned
  over time and current history details do not preserve arbitrary similarity
  tags.
- Make cooldowns global and always on: rejected because it would silently
  change ErsatzTV-compatible schedule behavior and could harm intentionally
  themed programming blocks.

## References

- [ADR 0010: Classic scheduling engine port](0010-classic-scheduling-engine-port.md)
- [ADR 0018: Special features, alternate versions, and stream roles](0018-extras-alternate-versions-and-stream-roles.md)
- [Architecture dependency rules](../../ARCHITECTURE.md)
- [Code style and SQL compatibility rules](../../STYLE.md)
- [`crates/ersatzrs-core/src/scheduling/mod.rs`](../../crates/ersatzrs-core/src/scheduling/mod.rs)
- [`crates/ersatzrs-core/src/scheduling/classic/mod.rs`](../../crates/ersatzrs-core/src/scheduling/classic/mod.rs)
- [`crates/ersatzrs-core/src/scheduling/yaml.rs`](../../crates/ersatzrs-core/src/scheduling/yaml.rs)
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/ShuffledMediaCollectionEnumerator.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/BlockScheduling/BlockPlayoutShuffledMediaCollectionEnumerator.cs`
