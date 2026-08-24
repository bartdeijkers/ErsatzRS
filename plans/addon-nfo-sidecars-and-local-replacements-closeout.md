# Add-on NFO sidecars and local replacements closeout plan

> **Status:** Active closeout. Slices 1 through 6, the complete managed-pair
> failure/recovery matrix, and generic hydrated browser acceptance are
> implemented and tested. Packaged Windows add-on execution still blocks
> completion; ADR 0080 retired the former MySQL proof requirement.

## Goal

Finish provider-neutral NFO sidecars, content classification, unavailable-item
retention, and exact Local replacements without moving provider rules,
translations, icons, or language-specific assets into ErsatzRS.

## Locked ownership boundary

- ErsatzRS owns versioned generic contracts, validation, managed storage,
  deterministic NFO generation, scanner precedence, persistence, generic
  localized administration, replacement selection, scheduling projection, and
  health.
- Each add-on owns provider parsing, provider availability values, per-item
  content inference, provider rating semantics, localized package strings,
  help, icons, classification resources, fixtures, and source-specific tests.
- Main-repository tests use neutral fictional records. Beeld & Geluid, yt-dlp,
  Dutch keywords, and future locale-specific examples stay in the add-ons
  repository.
- A saved-list content override is a generic operator policy. It may replace an
  add-on's per-item kind with television episode, movie, music video, or other
  video; `auto` preserves the add-on result.

## Current working-tree baseline

- `media-list.list.v2` and `remote-stream.list.v2` carry provider identity,
  availability, reason, content kind, GUIDs, and richer metadata while v1
  remains readable.
- Managed list scanning writes same-stem YAML and NFO files and records both in
  state v2.
- Add-on list items persist `available`, `unavailable`, or `unknown` and retain
  a provider-ordered gap.
- Admin users can search Local video versions and pin one exact media item and
  media version to a list item.
- Generated playlists use the pinned Local media version only for explicit
  `unavailable`; `unknown` never activates it, and provider recovery wins.
- Beeld & Geluid POSIX retains an unavailable item even when its detail page is
  gone. Provider-specific assertions live in the add-ons repository.
- Focused generic scanner, synchronization, web, SQLite, and add-on tests
  passed during the implementation session.
- Workspace Clippy, the full test closeout (apart from
  three sandbox-denied HTTP mocks that passed unrestricted), web parity check,
  the consistent Leptos SSR/WASM build, and the unrestricted streaming smoke
  gate passed during closeout.
- The managed publication matrix now covers definition and NFO publication
  failures, state publication refusal, missing-member healing, v1-to-v2
  healing, interrupted multi-entry recovery, `trash`, `keep`, and preservation
  of hand-authored sibling files.
- Chromium and Firefox now exercise the hydrated import and edit overrides,
  unavailable status, Local replacement search/set/clear, explicit stale
  status, and provider recovery while retaining replacement intent.

## Blocking implementation slices

### 1. Correct metadata persistence before expanding tests — implemented

Fix the bind order in `upsert_local_remote_stream`: `Title` must receive the
effective title and `SortTitle` the effective sort title on both insert and
update. Add a rescan regression that starts with distinct title, original title,
and sort title values, then changes the NFO and proves every column updates
without swapping.

Audit all newly extended `RemoteStreamMetadata` binds in the same pass,
including outline, tagline, collection, rating, release date, and original and
current broadcasters.

### 2. Define and implement observable content-kind semantics — implemented

`RemoteStream.ContentKind` is currently persisted but has no reader outside the
scanner. Decide the intended host behavior explicitly before claiming the
override works:

1. Keep the database item as a Remote Stream for playback identity and
   collection type.
2. Expose its descriptive kind in the Remote Streams query/detail/search/admin
   projections and use it for kind-aware metadata labels and filters.
3. Do not manufacture `Movie`, `Episode`, or `MusicVideo` rows for a remote
   item unless a separate ADR changes media identity; that would collide with
   the existing Remote Stream playback model.
4. Verify XMLTV and scheduling consumers receive the expected episode/movie/
   music-video semantics where those consumers support them.

Add regressions for add-on-provided kind, every list override, and changing an
override back to `auto`. A changed override must invalidate the managed state
etag, rewrite the NFO root, and update persisted content kind on rescan.

### 3. Make YAML and NFO publication transactional as a pair — implemented

The current writer atomically replaces each individual file but can expose a
new YAML with an old or missing NFO if the second write fails. Stage both files,
validate the staged NFO, then publish the pair with rollback/recovery rules
before publishing state v2. Never advance the state sidecar unless every owned
pair is complete.

Cover interruption/failure at each publication point, cached healing of a
missing member, v1-to-v2 healing, `trash`, and `keep`. Hand-authored siblings
must remain untouched.

### 4. Prove NFO fidelity for every generic content kind — implemented

Create neutral golden/snapshot fixtures for:

- `episodedetails`, including show title, season, and episode without inventing
  missing numbers;
- `movie`;
- `musicvideo`, including one or more artists; and
- other video through the compatible `movie` root.

Round-trip each generated NFO through the scanner reader and assert title,
original/sort title, outline, plot, tagline, year, full release instant,
runtime, rating, genres, tags, studios, people/roles, directors, writers,
artists, GUIDs, artwork, collection, producers, and broadcaster distinctions.
Extend the generic Remote Stream NFO merge model where fields such as
music-video artists or episode identity are currently written but discarded.

Keep the compatibility claim precise: Kodi/Jellyfin/Emby-style standard fields
plus ignorable `urn:ersatzrs:nfo:1` extensions. Plex needs an appropriate
metadata agent; do not invent or advertise native Plex NFO support.

### 5. Complete replacement validity and playback proof — core implemented

Use one shared validity predicate for the picker, playlist projection, admin
status, and health. It must require a normal Local media item, matching exact
version ownership, a media file, and a non-attached video stream. Health must
also detect a deleted file/version, non-Local source, missing video stream, and
ownership mismatch.

Add repository tests for set, replace, clear, invalid candidate rejection, and
stale mapping visibility. Add a scheduling/playback-level regression proving
the pinned media version and stream index survive enumeration and are actually
selected. Confirm the replacement's real local collection type is used.

Decide and document whether a replacement may be preconfigured while an item
is available. Regardless of that choice, it must only activate for explicit
`unavailable`.

### 6. Harden provider reconciliation — policy implemented

Confirm a v2 refresh preserves an existing replacement when the same provider
ID remains unavailable, and provider recovery reuses the provider row. Decide
whether a temporarily omitted item becomes a retained tombstone or is deleted;
deleting it currently cascades the replacement mapping and may lose operator
intent. Record and test the chosen grace/tombstone policy.

Bound provider/list IDs and reason strings to values supported by SQLite
indexes. Exercise the new migration against fresh and upgraded SQLite schemas.

### 7. Finish add-on-owned behavior in the add-ons repository — POSIX advanced

For Beeld & Geluid:

- retain unavailable shared-list items without requiring their detail page;
- preserve the provider's own item title from list data where possible instead
  of the synthetic English `Unavailable item <id>` label;
- test television, film, music-video, advertisement/other-video, and unknown
  classification with provider fixtures;
- prove both media-list v2 and Remote Stream v2 output preserve order,
  availability, reason, content kind, GUID, and rich metadata; and
- execute the PowerShell implementation with fixtures on Windows or pwsh,
  rather than relying only on source-text assertions.

For yt-dlp:

- make POSIX and PowerShell availability/content classification behavior
  equivalent; POSIX currently emits `auto` while PowerShell applies keyword
  heuristics;
- retain private/unavailable flat-playlist entries when yt-dlp exposes their
  stable ID and position;
- test movie, television episode, music video, advertisement/other video,
  unknown, and missing/null metadata; and
- keep provider keywords and any future locale tables entirely inside the
  add-on package.

Add manifest tests for both v2 capabilities on every storage-backed package.
Normalize Windows batch files to CRLF after script changes and keep all six RID
packages deterministic.

### 8. Complete the generic admin and health acceptance — implemented

Add SSR and hydrated browser coverage for import override, edit override,
unavailable-row status, replacement search, set, clear, stale mapping, and
provider recovery. Generic labels must exist in every supported host culture;
provider strings and assets must render from the add-on manifest without being
copied into host language modules.

Health acceptance must distinguish refresh failure, missing replacement, stale
replacement, missing YAML/NFO pair, unavailable add-on, and an intentionally
unknown provider state. All messages remain bounded and provider-neutral.

## Required closeout order

1. Metadata bind-order regression and fix.
2. NFO round-trip matrix and transactional pair publication.
3. Observable content-kind semantics and override-rescan tests.
4. Shared replacement validity, stale health, reconciliation/tombstone policy,
   and playback proof.
5. Provider-owned POSIX/Windows tests and parity in the add-ons repository.
6. Generic admin browser workflow and all-culture localization checks.
7. SQLite migration tests, workspace Clippy, scanner/web/infra tests,
   hydration lint, progress validation, and full closeout once.

## Verification commands

Use focused runs while iterating:

```bash
./run.sh test -p ersatzrs-scanner remote_stream_playlist
./run.sh test -p ersatzrs-scanner nfo
./run.sh test -p ersatzrs-infra --test addon_media_list_sync
./run.sh test -p ersatzrs-infra addon_media_list
./run.sh test -p ersatzrs-web addon_media_lists
cargo xtask hydration-lint -p ersatzrs-web
cargo xtask progress check
```

In the add-ons repository:

```bash
sh -n addons/org.ersatzrs.addon.beeldengeluid/libexec/beeldengeluid.sh
sh -n addons/org.ersatzrs.addon.yt-dlp/addon.sh
python3 -m unittest discover -s tests -v
```

Run the repository's full two-pass clippy, `./run.sh test-closeout`, Leptos
build, parity check, and relevant runtime smoke only after the focused matrix is
green. Windows PowerShell/package runtime proof remains mandatory for the
Windows add-on implementation.

## Completion criteria

- No provider or language-specific assertion/asset exists in ErsatzRS.
- All four content kinds round-trip from a generic v2 record through NFO and
  persisted Remote Stream metadata with observable host behavior.
- Managed YAML/NFO/state cannot publish a mixed generation.
- Missing provider entries remain configurable and exact Local fallback
  playback is proven; unknown availability never falls back.
- Stale mappings are visible in both admin and health.
- POSIX and Windows variants of both official storage-backed add-ons implement
  equivalent v2 semantics with provider-owned tests.
- ADR 0068, references, changelog, parity state, and session log match verified
  implementation rather than planned behavior.
