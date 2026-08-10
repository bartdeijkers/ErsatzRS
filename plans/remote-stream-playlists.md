# Phase 8D/8E — Remote Stream playlists

## Status

Implemented as Phase 8D/8E under accepted ADR 0065. The scanner, managed
playlist schema, health/troubleshooting surfaces, localized admin UI, and
ordered scheduling path are complete. Live implementation state and evidence
remain authoritative in `docs/progress/`; this file retains the design and
acceptance boundaries that governed the slice.

Teletext remains Phase 9 and the "The Box" video-jukebox extension remains
Phase 10.

## Elevator pitch

A Remote Streams library is one hand-written YAML file per video. That is fine
for five videos and unusable for a channel with twelve playlists and four
hundred entries. ErsatzRS already models the grouping people want: the folder a
definition sits in becomes a tag, and tags already drive search, facets, smart
collections, and scheduling. What is missing is not a data model — it is a
machine that keeps those folders in sync with an upstream playlist.

So: add a **playlist manifest**, one small YAML file that says "this URL,
through this extractor, into this folder, refreshed this often". The scanner
expands it into ordinary Remote Stream definitions that the existing pipeline
already understands, and garbage-collects entries the upstream playlist dropped.
Nothing about playback changes. A user pastes a playlist URL, presses Sync, and
gets a tagged, schedulable group — "Official Music Videos" as one collection,
"Live Registrations" as another — without writing YAML by hand.

## Design basis

- `RemoteStreamDefinition` (`crates/ersatzrs-infra/src/remote_stream.rs:21`) is
  a flat, `deny_unknown_fields` struct: `url` or `script`, `duration`,
  `is_live`, plus title/plot/year/content_rating.
- `collect_remote_stream_files` (`crates/ersatzrs-scanner/src/lib.rs:600`)
  recursively collects every `.yml`/`.yaml` below the library root and parses
  each as a definition. Any new YAML shape dropped into the library is therefore
  a scan error until the collector learns to skip it.
- `remote_stream_folder_tags` (`crates/ersatzrs-scanner/src/lib.rs:1907`) tags
  each item with the library root folder name plus every relative folder
  component. Channel/playlist nesting already produces channel and playlist tags
  for free.
- Tags on Remote Streams are searchable: `SearchField::Tag` resolves through
  `RemoteStreamMetadataId` in the scheduling search repository
  (`crates/ersatzrs-infra/src/repositories/scheduling.rs:1960`, `:3899`), so
  `tag:"Official Music Videos"` smart collections work today.
- `MediaFacetSet` (`crates/ersatzrs-core/src/metadata_facets.rs:8`) already
  carries `tags`, and admin detail surfaces render facets uniformly (ADR 0062).
- ADR 0057 established the direct producer pipe and put **Add To Playlist** on
  every Remote Stream card, persisting `CollectionType` value 70. Remote Streams
  are already first-class playlist items.
- `Playlist` / `PlaylistGroup` / `PlaylistItem`
  (`crates/ersatzrs-domain/src/collection.rs:124`) support ordered
  `media_item_id` entries with `sort_order`, so upstream playlist order is
  representable without a new ordering concept.
- `assets/extractors/youtube/` currently ships a Windows-only `youtube.bat`
  plus a YAML example. It resolves one video, requires `FFMPEG_BIN`, honours
  `{seek}`, and deliberately does not bundle `yt-dlp`.
- The maintenance scheduler already runs a library refresh loop on
  `scanner.library_refresh_interval`
  (`crates/ersatzrs-core/src/maintenance.rs:78`), and scan work flows through a
  single scan worker (`crates/ersatzrs-web/src/host/tasks.rs:34`).
- ErsatzTV upstream has no playlist ingestion for Remote Streams
  (`$ERSATZTV_SRC/ErsatzTV.Core/Streaming/YamlRemoteStreamDefinition.cs` is the
  whole definition surface). This feature is an ErsatzRS extension and needs an
  ADR recording the deviation.

## Goal

Turn a remote playlist URL into a synchronized, tagged, schedulable group of
Remote Stream items, with deterministic durations, stable item identity across
syncs, an auditable trust boundary around extractor execution, and no change to
the playback path.

## Non-goals

- Bundling, installing, or updating `yt-dlp` or any other extractor.
- Resolving media URLs at scan time. Direct media URLs expire; resolution stays
  at playback time through the existing `script:` + `{seek}` contract.
- Replacing tags, collections, or playlists with a new grouping primitive.
- Supporting live playlists (endless upstream streams) in this phase.
- Any provider-specific logic inside ErsatzRS. Everything service-specific lives
  in the operator's extractor script.

## Design

### 1. The manifest is a distinct file kind

A playlist manifest is a file named `<name>.playlist.yml` (or `.playlist.yaml`)
anywhere under a Remote Streams library root. The suffix is load-bearing:
`collect_remote_stream_files` must skip it and route it to a new
`LocalScanWork::RemoteStreamPlaylist { path }` branch, otherwise the existing
`deny_unknown_fields` parse flags it as an invalid definition.

A manifest expands into a sibling folder named after the manifest stem:

```
Remote Streams/
  Metallica/
    Official Music Videos.playlist.yml
    Official Music Videos/
      .ersatzrs-playlist.json      <- sync state, ErsatzRS-owned
      dQw4w9WgXcQ.yml              <- generated definition, ErsatzRS-owned
      ...
    Live Registrations.playlist.yml
    Live Registrations/
      ...
```

The expansion folder is chosen so `remote_stream_folder_tags` produces
`["Remote Streams", "Metallica", "Official Music Videos"]` with no new tagging
code. Channel grouping and playlist grouping fall out of the existing rule.

### 2. Extractors are named, allowlisted server settings

Today a definition embeds a raw command line. A manifest must not, because the
scanner would then execute arbitrary command lines discovered by walking a
folder. Instead:

- A server setting (`settings.remote_stream_extractors`) maps a **name** to a
  per-platform command: `{ name: "youtube", windows: "...\\youtube.bat",
  unix: ".../youtube.sh" }`.
- A manifest references `extractor: youtube`. An unknown name is a hard sync
  failure, not a fallback.
- Hand-written definitions keep their existing raw `script:` freedom. The
  allowlist constrains only the new scan-time execution path.

This also makes the admin UI trivial (a dropdown, not a command box) and gives
the Unix counterpart a home: `assets/extractors/youtube/youtube.sh` alongside
the existing `.bat`, sharing one contract across all six release targets.

### 3. Extractor contract: two subcommands, one script

```
<extractor> list <playlist-url>
    stdout: NDJSON, one entry per line
    stderr: diagnostics
    exit 0 on success

<extractor> play <video-url> <seek>
    stdout: MPEG-TS bytes  (exactly today's youtube.bat behavior)
    stderr: diagnostics
```

Entry shape (unknown keys ignored, so extractors can be richer than ErsatzRS):

```json
{"id":"dQw4w9WgXcQ","url":"https://…","title":"…","duration_seconds":213,
 "plot":"…","year":2009,"thumbnail_url":"…","is_live":false}
```

`id` is required and is the identity key. `duration_seconds` is required for a
schedulable item; entries without it are still materialized but flagged (see
health, below) so a channel never silently schedules an unknown-length item.

The generated per-video definition is a plain, boring definition — the shape
that works today:

```yaml
# Generated by ErsatzRS from "Official Music Videos.playlist.yml". Do not edit.
script: '"<resolved extractor>" play "https://www.youtube.com/watch?v=dQw4w9WgXcQ" "{seek}"'
is_live: false
duration: "00:03:33"
title: "Example title"
```

Because playback still goes through `script:` + `{seek}`, the entire producer,
seek, probe, redaction, and cancellation path from ADR 0057 is untouched.

### 4. Sync algorithm

Per manifest, inside the existing scan worker:

1. Read the manifest; resolve the extractor for this platform.
2. Compute a **manifest etag** = hash(manifest bytes + extractor identity). If
   the sidecar state records the same etag and the manifest's `sync.interval`
   has not elapsed, skip — the cheap no-op path, mirroring `folder_etag.rs`.
3. Run `list`, parse NDJSON, apply `filter` / `exclude` / `max_items` / `order`.
4. Compute a **playlist etag** from every ordered enumeration field that can
   affect a generated definition. Unchanged ⇒ record the sync timestamp and
   stop.
5. Diff against the sidecar's previous entry set:
   - new id ⇒ write `<id>.yml`;
   - changed title/duration ⇒ rewrite `<id>.yml`;
   - missing id ⇒ apply `on_removed` (`trash` deletes the generated file so the
     existing missing-file path trashes the item; `keep` leaves it and marks it
     orphaned).
6. Rewrite the sidecar state file atomically (temp file + rename).
7. Let the ordinary Remote Stream scan pass pick the folder up. Definitions the
   sidecar does not list are never touched, so a user may hand-add extra
   definitions to a managed folder without losing them.

Identity is the extractor `id`, never the ordinal, so reordering upstream never
churns file names or `MediaFile.PathHash`.

Enumeration is serialized through the scan worker: a library with twenty
manifests must not fan out twenty concurrent network enumerations.

### 5. Ordering

Two levels, deliberately staged:

- **Stage 1 — tags.** Grouping is a tag; users build a smart collection
  (`tag:"Official Music Videos"`) and pick a playback order in scheduling. Zero
  schema change, works with everything today.
- **Stage 2 — materialized playlist.** `create_playlist: true` makes the sync
  own an ErsatzRS `Playlist` whose `PlaylistItem.sort_order` mirrors upstream
  order, so "the album, in album order" is schedulable. Managed playlists need a
  marker so the UI can show them read-only and the sync can rewrite them safely:
  a nullable `"Playlist"."ManagedSource"` TEXT column holding the manifest's
  library-relative path. That is a schema deviation from ErsatzTV and belongs in
  the ADR.

### 6. Playout invalidation

Playout items are baked at build time. A sync that adds, trashes, or reorders
items changes what a collection or playlist resolves to, and already-built
playouts will not reflect it. The sync must therefore mark affected playouts for
refresh, and surface in the UI when a reset rather than a refresh is required,
the same way other membership-changing operations do. Skipping this produces the
classic "I synced and nothing changed" report.

### 7. Health checks

A new `remote_stream_playlist` check, alongside the existing
`crates/ersatzrs-infra/src/health/checks/remote_stream.rs`:

- manifest references an unknown or unresolvable extractor;
- last sync failed, or the last success is older than 2× `sync.interval`;
- entries materialized without a duration (feeds the existing `zero_duration`
  concern from the other side);
- `max_items` clamped a playlist (informational: the group is truncated).

### 8. Troubleshooting

Reuse the existing troubleshooting surface to run `list` for one manifest and
show the parsed entry count plus the bounded, redacted stderr tail.
`redact_remote_stream_output` and `RollingStderrTail` already exist and must be
applied to the enumeration path too, so playlist URLs carrying tokens never
reach logs or the UI verbatim.

## Frontend

### Where it lives

- **Remote Streams browser** (`/media/remote/streams`) gains a playlist facet
  chip row driven by the tags the sync already writes, reusing the ADR 0062
  facet rendering. Clicking a chip is a `tag:` search — no new query path.
- **Library detail** for a Remote Streams library gains a **Playlists** section:
  one row per manifest with name, extractor, item count, last sync, next sync,
  state badge (OK / stale / failed / truncated), and a per-row **Sync now**.
- **Playlist detail** (`/media/remote/streams/playlists/{id}`): the manifest's
  effective settings, plus the entry table — ordinal, title, duration, state
  (materialized / no duration / orphaned / excluded), and per-entry actions
  (exclude, troubleshoot, open item).

### Add / edit dialog

Fields: name, playlist URL, extractor (dropdown from the allowlist), destination
folder (defaults to the manifest stem), sync interval, max items, order,
`on_removed`, and a "also create a playlist in upstream order" checkbox.

Write rules, so the UI never destroys hand-authored YAML:

- ErsatzRS-created manifests carry a generated header and are registered in the
  sidecar; those are fully editable from the UI (serde round-trip).
- Hand-authored manifests render read-only with their effective settings and an
  "edit this file to change it" hint, because a serde round-trip would drop
  comments and key order.

### Interaction notes

- Sync is a long-running operation; report it through the existing scan-activity
  progress surface rather than a bespoke spinner.
- Exclusions from the entry table write to the sidecar's `overrides`, never to
  the user's manifest.
- Every string is localized across `crates/ersatzrs-web/src/admin/languages/`
  per ADR 0012, in the same slice that adds it.
- New islands follow the TEST.md hydration rules and get a playbook entry.

## Manifest format

```yaml
# Remote Stream playlist manifest — file name must end in .playlist.yml
name: "Official Music Videos"        # optional; defaults to the file stem
url: "https://www.youtube.com/playlist?list=PL…"
extractor: youtube                   # required; must match an allowlisted name

# Where generated definitions go. Relative to the manifest. Defaults to the
# file stem, which is what makes the folder name become the group tag.
folder: "Official Music Videos"

sync:
  interval: "24h"                    # minimum time between enumerations
  max_items: 500                     # 0 = unlimited
  order: upstream                    # upstream | newest_first | title
  on_removed: trash                  # trash | keep

defaults:                            # applied to every generated definition
  is_live: false
  content_rating: null
  year: null

filter:
  include_title: null                # optional regex, applied to entry title
  exclude_title: null
  min_duration: "00:00:30"           # drop shorts and stingers
  max_duration: null

exclude:                             # explicit entry ids, never materialized
  - "abc123XYZ"

create_playlist: false               # Stage 2: materialize an ordered Playlist
tags:                                # extra tags beyond the folder-derived ones
  - "Music Video"
```

Sidecar state, ErsatzRS-owned, `<folder>/.ersatzrs-playlist.json`:

```json
{
  "version": 1,
  "manifest_etag": "…",
  "playlist_etag": "…",
  "last_sync": "2026-08-09T10:11:12Z",
  "last_result": {"kind": "ok", "entries": 214, "truncated": false},
  "managed_files": {"dQw4w9WgXcQ": "dQw4w9WgXcQ.yml"},
  "overrides": {"excluded": ["abc123XYZ"]}
}
```

## Historical milestone proposal

The proposal below predates the decision to close this work inside Phase 8.
The implemented milestones are `p8d-01` through `p8e-03` in
`docs/progress/milestones.toml`; their acceptance text remains equivalent to
this design record.

The original proposed entries all used
`feature = 'remote-stream-playlists'` and
`plan = 'plans/remote-stream-playlists.md'`.

### Phase 9A — Playlist ingest contract and sync

| id | title | depends_on |
| --- | --- | --- |
| `p9a-01` | Decide the Remote Stream playlist ingest contract | `p8a-02`, `p8b-02`, `p8c-01`, `p8c-02`, `p8c-03` |
| `p9a-02` | Allowlist and resolve extractors on all six targets | `p9a-01` |
| `p9a-03` | Recognize and validate playlist manifests during scan | `p9a-02` |
| `p9a-04` | Synchronize playlists into Remote Stream definitions | `p9a-03` |

**`p9a-01` acceptance.** An accepted ADR records this as an ErsatzRS extension
with no ErsatzTV counterpart; fixes the `<name>.playlist.yml` file-name
convention and the folder-expansion/tagging rule; defines the extractor
allowlist as the trust boundary for scan-time process execution; pins the
`list` / `play` subcommand contract and the NDJSON entry schema; states that
media URLs are never resolved at scan time; and decides the Stage 2
`"Playlist"."ManagedSource"` schema deviation before any schema, scanner, or UI
work begins. Blocking for the rest of Phase 9.

**`p9a-02` acceptance.** Extractors are configured by name with per-platform
commands, validated at save time, and resolved identically on all six release
targets. `assets/extractors/youtube/` ships both a Windows and a Unix wrapper
implementing `list` and `play`, with the existing single-video invocation still
working. No extractor is bundled, downloaded, or auto-updated, and an
unresolvable extractor fails visibly instead of falling back.

**`p9a-03` acceptance.** The Remote Streams scan recognizes manifests, excludes
them from the definition sweep so they never surface as invalid definitions, and
reports manifest validation errors per file without failing the library scan.
Manifest parsing, defaulting, and every rejection path are snapshot-tested.

**`p9a-04` acceptance.** A sync enumerates through the resolved extractor,
applies filter/exclude/max/order, materializes stable per-entry definitions,
rewrites the sidecar atomically, and removes only files it owns. Manifest and
playlist etags make an unchanged playlist a no-op. Enumeration is serialized,
output is redacted, upstream removals follow `on_removed`, and membership
changes mark affected playouts for refresh. Proven on SQLite and MySQL with a
fixture extractor and no network access.

### Phase 9B — Management, health, and ordering

| id | title | depends_on |
| --- | --- | --- |
| `p9b-01` | Report playlist sync health and troubleshooting | `p9a-04` |
| `p9b-02` | Expose localized playlist management in the admin UI | `p9a-04` |
| `p9b-03` | Materialize upstream playlist order | `p9b-02` |

**`p9b-01` acceptance.** A `remote_stream_playlist` health check reports unknown
or unresolvable extractors, failed syncs, staleness beyond 2× the interval,
entries without duration, and truncation, with results persisted through the
existing health surface. Troubleshooting runs a manifest enumeration on demand
and shows the entry count plus a bounded, redacted stderr tail.

**`p9b-02` acceptance.** The library detail page lists manifests with state and
a working Sync now; a playlist detail page shows effective settings and the
entry table with per-entry actions; the add/edit dialog creates and edits
ErsatzRS-owned manifests and renders hand-authored ones read-only; the browser
exposes playlist facet chips. Every string is localized for all supported
cultures, hydration follows TEST.md, and a hydration playbook entry exists.

**`p9b-03` acceptance.** `create_playlist: true` maintains an ordered ErsatzRS
`Playlist` marked as sync-owned through the accepted schema deviation, kept in
upstream order across syncs, read-only in the UI, correctly rebuilt after
additions/removals/reordering, and proven to schedule in order end to end.
Phase 9 closes when this milestone closes.

## Rejected phase re-sequencing proposal

This historical proposal was not applied. Remote Stream playlists instead
closed inside Phase 8D/8E, leaving teletext as Phase 9 and the video-jukebox
extension as Phase 10. Do not execute the renumbering instructions retained
below; they explain the discarded alternative and its validation risks.

### Renumbering map

| Current | Becomes |
| --- | --- |
| — (new) | Phase 9 — Remote Stream playlists, `p9a-01`…`p9b-03` |
| Phase 9 — Teletext support, `p9a-*`/`p9b-*` | Phase 10, `p10a-*`/`p10b-*` |
| Phase 10 — video jukebox, `p10a-*`/`p10b-*` | Phase 11, `p11a-*`/`p11b-*` |

Milestone ids are renumbered rather than left in place. Every affected milestone
is `Not started`, no completed evidence references these ids, and
`docs/progress/sessions.ndjson` mentions only the phase words, never the ids —
so the rename costs nothing and avoids a permanent trap where
`progress milestones --id p9a-01` returns teletext work in a phase labelled 10.

### Order of operations

Ids must move in this order or they collide, because `cargo xtask progress
check` enforces unique ids:

1. Jukebox `p10a-01`→`p11a-01`, `p10a-02`→`p11a-02`, `p10a-03`→`p11a-03`,
   `p10b-01`→`p11b-01`, `p10b-02`→`p11b-02`, including their internal
   `depends_on` references and both `phase = 'Phase 10A/10B — …'` labels.
2. Teletext `p9a-01`→`p10a-01`, `p9a-02`→`p10a-02`, `p9a-03`→`p10a-03`,
   `p9b-01`→`p10b-01`, `p9b-02`→`p10b-02`, including internal `depends_on` and
   both `phase = 'Phase 9A/9B — …'` labels.
3. Repoint the jukebox entry milestone: `p11a-01.depends_on` becomes
   `['p7c-04', 'p10b-02']`.
4. Insert the seven new Phase 9 milestones from the previous section.
5. Repoint the teletext entry milestone: `p10a-01.depends_on` becomes
   `['p9b-03']`.

Step 5 is a decision, not a mechanical edit. Teletext has no technical
dependency on playlists; gating it on `p9b-03` preserves the project's
one-live-phase model and matches how the jukebox phase was already gated on
teletext closeout. The alternative — leaving teletext gated only on the Phase 8
milestones so it could start in parallel — is viable for a multi-contributor
project and should be chosen deliberately if that ever becomes the case.

### File-by-file edits

- `docs/progress/milestones.toml` — steps 1–5 above.
- `docs/progress/phase.toml` — `current_phase`, `current_focus`, `summary`,
  `next_steps`; add a `[[blocking_decision]]` for the new `p9a-01` ADR; update
  the existing two notes to the teletext and jukebox ids' new values.
- `docs/progress/parity/remote-stream-playlists.toml` — **new** parity file,
  `status = 'Not started'`, with the open questions below recorded as visible
  `[[debt]]` entries (contract undecided, no cross-platform extractor, no
  duration guarantee from flat enumeration).
- `docs/progress/parity/teletext-support.toml` — Phase 9→10 wording, `p9a-01`→
  `p10a-01`, `p9b-02`→`p10b-02`, and the "Phase 10 video-jukebox work begins
  only after" sentence to Phase 11.
- `docs/progress/parity/video-jukebox-addon-api.toml` — Phase 10→11, Phase 9→10.
- `plans/teletext-support.md` — headings `Phase 9A`/`Phase 9B` and prose,
  milestone ids, and the trailing Phase 10→11 gating sentence.
- `plans/application-layer-and-video-jukebox.md` — `Phase 10A`/`Phase 10B`
  headings, status paragraph, milestone ids, and the gating sentence.
- `plans/ffmpeg-9-hdr-dolby-vision-and-pipeline-hardening.md` — the two "Phase 9
  teletext" sentences become "Phase 9 Remote Stream playlists" / next-phase
  wording.
- `plans/phase-8-hardware-residue.md` — "Independent of Phases 9 and 10" becomes
  "Independent of Phases 9–11".
- `plans/INDEX.md` — add this plan to Active / actionable as the Phase 9 plan;
  retitle the teletext entry to Phase 10 and the jukebox entry to Phase 11.
- `AGENTS.md` (and therefore `CLAUDE.md`, `CODEX.md`) — the **Current status**
  paragraph: Phase 9 becomes Remote Stream playlists with the blocking `p9a-01`
  ADR; teletext is Phase 10; the jukebox is Phase 11.
- `README.md` — the **Current Status** paragraph.
- `STATUS.md` — the two phase lines.
- `docs/top-three-gaps-to-fill-prompts.md` — the phase sentence.
- `CHANGELOG.md` — one `[Unreleased]` → `Changed` entry recording the
  re-sequencing, following the precedent of the Phase 8/9/10 re-sequencing entry
  already in the file. Planning contracts only, not runtime behavior.

### Do not rewrite

- `docs/progress/sessions.ndjson`. Its five "Phase 9"/"Phase 10" mentions are
  true statements about what was planned at the time; the session log is an
  append-only audit trail.
- Released `CHANGELOG.md` sections, including the earlier re-sequencing entry.

### Validation

```bash
cargo xtask progress check
cargo xtask progress next
cargo xtask progress milestones --runnable
cargo xtask progress milestones --id p9a-01 --full   # must show the playlist ADR
cargo xtask progress milestones --id p10a-01 --full  # must show teletext
cargo xtask progress milestones --id p11a-01 --full  # must show the jukebox
```

`progress check` validates id uniqueness, dependency resolution, status values,
feature-slug resolution, and that every `plan` path exists — but it does **not**
verify that a milestone id's phase prefix matches its `phase` label. A missed
rename will therefore pass. Either re-read the diff, or take the optional
hardening below.

### Optional hardening (separate slice, not required)

- Add a `progress check` rule that a milestone id's `p<N><letter>-` prefix
  agrees with the number and letter in its `phase` label, so a future
  re-sequencing cannot half-land.
- `cargo xtask progress milestones` orders by `phase, id` as text, so
  "Phase 10A" already sorts before "Phase 9A". A third double-digit phase makes
  this more visible. Fixing it means either zero-padding every phase label or
  adding a numeric sort key; both touch all phases and belong in their own
  change, not this one.

## Resolved risks and operating constraints

- **Flat enumeration may omit durations.** For some upstream services a flat
  listing returns no duration. Extractors may report null instead of performing
  rate-limit-exposed per-entry resolution; sync continues, health reports the
  item, and the managed playlist excludes it from scheduling.
- **Rate limiting and bans.** Enumeration is a network call against a service
  with quotas. The etag no-op path, a floor on `sync.interval`, and serialized
  enumeration are the mitigations.
- **Scan-time code execution.** Even allowlisted, this widens what a scan does.
  The allowlist plus "manifests may not carry raw command lines" is the
  mitigation recorded by ADR 0065.
- **Writing into the user's library folder.** ErsatzRS becomes a writer, not
  just a reader, of library content. The sidecar's `managed_files` list is the
  contract that keeps deletion scoped; it must never delete a file it did not
  create.
- **Legal posture.** The existing extractor readme already says access and
  redistribute only when authorized. Bulk playlist ingestion makes that more
  prominent, not less; keep the wording in the UI, not only in a readme.
- **Open:** should a manifest be allowed outside a Remote Streams library (a
  global "playlist source" independent of folders)? That would decouple grouping
  from folder tags and require its own grouping model. Deferred; folders are
  cheaper and already work.

## Completed contract obligations

- ADR 0065 in `docs/adr/` preceded implementation and records the ErsatzTV
  deviation per PORTING.md.
- `docs/progress/parity/remote-stream-playlists.toml` owns implementation state
  and the resolved parity debts.
- `CHANGELOG.md` `[Unreleased]` entry per user-visible slice, plus the
  re-sequencing entry described above.
- Localization for every new admin string across all supported cultures
  (ADR 0012).
- Health-check update in the same slice as the sync engine, per the
  health-check parity contract.
- `cargo xtask parity-check web` before claiming the admin UI slice complete;
  `cargo xtask smoke` is not required, since playback is unchanged.

## Closeout

Phase 8D/8E closed with `p8e-03`: a manifest can be created from the admin UI,
synced on schedule and on demand, produces tagged and ordered schedulable items,
reports its own failures through health and troubleshooting, and neither
resolves media URLs at scan time nor executes a command that is not allowlisted.
