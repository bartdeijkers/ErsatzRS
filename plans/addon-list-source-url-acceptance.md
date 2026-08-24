# Plan: accept every supported add-on-list source URL shape

## Implementation status

Not started. Registered 2026-08-23 after a direct YouTube watch URL resolved
its list metadata in the Windows admin UI but produced an empty review draft
that could not publish. Tracked by milestone `amm-g-07`.

## Context

Add-on Lists is the single operator-facing intake flow for both direct media
URLs and aggregate provider URLs. A direct source is semantically a one-item
list; an aggregate source is a bounded, paginated list. The host contract must
not require every extractor response to contain an `entries` array.

The current yt-dlp paths violate that rule. A direct watch URL returns one
media object without `entries`. The legacy mapper and the resumable Discover
adapter both use only `playlist.entries`, so they retain the root title and
description as list metadata while emitting zero items. The resulting draft
shows `0 Discovered`, publication fails because no item can be included, and a
channel, schedule, and playout cannot be built from it.

The same acceptance boundary must cover every source shape declared by the two
official add-ons, including a direct Beeld & Geluid episode and query-bearing
search pages.

## Required URL acceptance matrix

| Provider shape | Acceptance URL | Required discovery result |
|---|---|---|
| YouTube direct live video | `https://www.youtube.com/watch?v=mhJRzQsLZGg` | Exactly one available Remote Stream item with provider-reported `Live` liveness. |
| YouTube direct finite video | `https://www.youtube.com/watch?v=dQw4w9WgXcQ` | Exactly one available Remote Stream item with provider-reported `Finite` liveness and duration when supplied. |
| YouTube playlist | `https://www.youtube.com/playlist?list=PLJ_TJFLc25JRdChhr9sp00ajl7FAeKDx-` | Every reported entry exactly once and in provider order; liveness remains per item. |
| Beeld & Geluid shared list | `https://schatkamer.beeldengeluid.nl/lijst/14df8d33-ce8a-4680-a83b-0cc2a9c58bcd` | Every overview item across all pages, in provider order. |
| Beeld & Geluid dated search | `https://schatkamer.beeldengeluid.nl/zoeken?q=reclame&startdatum=01-01-1995&einddatum=01-03-1995` | Every result across all pages while preserving the query and both date filters. |
| Beeld & Geluid series | `https://schatkamer.beeldengeluid.nl/serie/2101608030022669331/de-fascinaties-van-boudewijn-buch` | Every series episode across all pages, in provider order. |
| Beeld & Geluid direct episode | `https://schatkamer.beeldengeluid.nl/serie/2101608030021828731/ster-reclame/aflevering/2101608040033953431` | Exactly one item whose stable identity, overview metadata, availability, and detail URL come from the episode. |

Live source contents may change. Deterministic fixtures therefore freeze each
response shape, while live acceptance compares aggregate counts and ordering
against the provider result observed in the same run instead of hard-coding a
mutable playlist or search count.

## Proposed work

### 1. Normalize direct and aggregate yt-dlp results

- When `entries` is an array, treat the response as an aggregate source and
  retain existing bounded pagination and provider ordering.
- When `entries` is absent and the root object has a stable media identity and
  URL, treat that root object as the single entry as well as the source of list
  defaults. Emit one complete Discover page with a total hint of one.
- Preserve provider liveness. The live and finite watch URLs above must not be
  collapsed to the same playback behavior.
- Apply the same rule to the resumable and legacy POSIX/Windows mappers so an
  installed-package fallback cannot reintroduce the empty-draft bug.
- Reject a genuinely invalid or empty provider response with a typed error;
  never report it as a successful zero-item list.

### 2. Normalize Beeld & Geluid source routing

- Recognize shared-list, search, series, and nested episode URLs explicitly.
- Preserve all operator-supplied query parameters and replace only the page
  cursor during pagination.
- Treat a nested episode URL as a one-item list without first crawling the
  surrounding series.
- Keep overview discovery free of item-detail requests for aggregate sources;
  the direct episode page is itself the requested source, not an enrichment
  fallback.
- Share identity and availability merging with `amm-g-06`, while keeping its
  explicit non-playable-series regression independently testable.

### 3. Make failed drafts recoverable through the admin UI

After installing a corrected add-on, an operator must be able to retry
discovery for an existing failed or zero-item draft from its editor. Retry
retains the source URL, selected add-on, folder, presentation, refresh policy,
detail policy, and operator metadata. It replaces only the incomplete provider
generation and cannot create duplicate items or order drift. No database edit,
list deletion, or server restart is acceptable as the recovery path.

The editor must explain that zero-item successful discovery is invalid and
offer the retry action. Health should link back to that action.

### 4. Prove publication and the original live-channel scenario

For every matrix URL, prove URL resolution, complete non-empty discovery,
review, inclusion, and publication through the shared host path. For the NSF
live watch URL specifically, use only the admin UI to:

1. publish the one-item Remote Streams add-on list;
2. create the generated playlist or other supported schedule source;
3. create channel **nsf live**;
4. create and assign a 24/7-compatible schedule and playout;
5. verify the channel exposes a playable IPTV/HLS endpoint and retains `Live`
   semantics rather than applying finite seek/restart behavior.

### 5. Package and platform acceptance

- Add deterministic adapter fixtures for all seven URLs and both direct-video
  liveness variants.
- Run the same contract assertions through POSIX and PowerShell entrypoints,
  resumable Discover, and the frozen legacy list capability.
- Add host repository/application and browser regressions for retry,
  review/publication, and the NSF channel/schedule/playout flow.
- Version and publish the official add-on package, install it through the
  Windows admin UI, and repeat the end-to-end scenario against the packaged
  artifact.

## Verification

- No accepted source shape can complete discovery with zero items unless the
  provider explicitly reports a valid empty aggregate, and such an aggregate
  cannot publish.
- Direct live and finite videos both produce one item with distinct liveness.
- Aggregate sources contain no omissions, duplicates, or provider-order drift.
- Search filters and other source query parameters survive every page request.
- Overview discovery does not perform aggregate item-detail traffic.
- A previously failed direct-video draft repairs through one visible retry
  action after the add-on update.
- The Windows UI completes the `nsf live` channel, schedule, and playout flow.
- Focused add-on, host, hydration, browser, health, and cross-platform package
  gates pass before the debt is resolved.

## Related

- ADR 0079 — overview-first discovery and deferred detail enrichment.
- Milestone `amm-g-06` — retain unavailable Beeld & Geluid series episodes.
- `plans/addon-list-replacement-ux.md` — Local replacement UX for unavailable
  discovered items.
