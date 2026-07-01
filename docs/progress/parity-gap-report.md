# Human parity gap report

Generated: 2026-07-02T00:05:40+02:00

This is a human-readable snapshot. The source of truth remains the structured
progress files in this directory and the `cargo xtask progress ...` queries.

## Headline

ErsatzRS has **7 tracked not-yet-Ported parity/spec features** across the
structured parity records.

Maintainer-reported regressions still track the channel-add default group, ADR
0017 default FFmpeg profile bitrates, and FFmpeg profile editor section
spacing. On 2026-07-02, two media-curation follow-ups were registered: ADR 0048
same-collection duplicate movie rows with distinct playback overrides, and
stay-in-context Add To Collection/Add To Playlist behavior. A third media
browser debt now tracks artist-detail music-video multi-select actions for bulk
collection/playlist adds. ADR 0019 and ADR 0020 remain Not started
scheduling/playout rebuild extensions. The tracker now has 10 open `[[debt]]`
items.

## Counts

| Area | Count | Human meaning |
| --- | ---: | --- |
| Tracked parity features | 39 | Feature records under `docs/progress/parity/`. |
| Ported parity features | 32 | Features currently marked `Ported`. |
| In-progress parity features | 5 | Features currently marked `In progress`. |
| Not-started parity/spec features | 2 | Tracked follow-up features currently marked `Not started`. |
| Open parity debt items | 10 | `[[debt]]` records with `resolved = false`. |
| Advertised-but-404 admin routes | 0 | No unrouted rows are recorded in the current progress snapshot. |
| Pending hydration-proof rows | 0 | `hydration.toml` has no pending rows. |

## Remaining tracked parity gaps

### Channels admin UI

- Slug: `channels-admin-ui`
- Status: `In progress`
- Open debt: `channels-add-default-group-ersatzrs`
- Discovered: 2026-07-01

Creating a new channel through `/channels/add` shows `ErsatzTV` as the default
channel group. Fresh ErsatzRS channels should default to the `ErsatzRS` group.

Closeout requires browser proof that `/channels/add` pre-fills `Group` with
`ErsatzRS`, save persistence keeps that value for a new channel, and existing
channel edit behavior is unchanged.

### FFmpeg command generation

- Slug: `ffmpeg-command-generation`
- Status: `In progress`
- Open debt: `default-profile-video-bitrate-halving`
- Discovered: 2026-07-01

The ADR 0017 fresh-database default FFmpeg profiles still use the earlier video
bitrate targets. The video bitrates should be halved from 8000, 16000, 20000,
45000, and 60000 kbit/s to 4000, 8000, 10000, 22500, and 30000 kbit/s while the
video buffer sizes remain untouched.

Closeout requires fresh database seeding and tests proving only the bitrate
targets changed; existing saved profile tables must not be rewritten.

### FFmpeg Profile editor

- Slug: `ffmpeg-profile-editor`
- Status: `In progress`
- Open debt: `profile-editor-video-audio-section-spacing`
- Discovered: 2026-07-01

The profile editor page has inconsistent vertical spacing between form elements
in the Video and Audio sections compared with the General section.

Closeout requires desktop and mobile browser proof that `/ffmpeg/{id}` and
`/ffmpeg/profiles/{id}` render General, Video, and Audio with consistent
vertical rhythm without regressing hydrated save/edit behavior.

### Media browsers

- Slug: `media-browsers`
- Status: `In progress`
- Open debts: `add-to-collection-playlist-stay-in-context`,
  `artist-detail-music-video-multi-select-actions`
- Discovered: 2026-07-02

After adding one or more media items to a collection or playlist from a media
page, the admin UI should keep the user in the current browsing/detail context
by default instead of always navigating to the target collection or playlist
page.

On `/media/music/artists/{artist_id}`, the artist detail page should also allow
selecting multiple music-video rows with checkboxes and adding the selected
videos to collections or playlists in bulk.

Closeout requires a clear success state, preserved current page/selection
context by default, an explicit View Collection/View Playlist navigation path,
artist-detail music-video checkbox selection, bulk collection/playlist dialogs
that persist the selected music-video items, preserved individual row actions,
and focused browser coverage for bulk and single-item collection and playlist
flows.

### Multi-collections / Rerun collections / Playlists / Filler presets

- Slug: `multi-collections-rerun-collections-playlists-filler-presets`
- Status: `In progress`
- Open debt: `collection-item-duplicate-playback-overrides`
- Discovered: 2026-07-02

The same movie should be addable to the same collection multiple times when
each row has a different playback override, so movies with multiple commentary
or alternate audio tracks can have every track represented in one collection.

Closeout requires the collection item identity, add-to-collection flow,
collection detail UI, scheduling expansion, SQLite/MySQL migrations, and
focused tests to prove duplicate same-movie rows with distinct overrides persist
and schedule separately.

### Playout rebuild on impacting changes

- Slug: `playout-rebuild-on-impacting-changes`
- Status: `Not started`
- Open debts: `channel-edit-triggers-no-playout-rebuild`,
  `classic-refresh-appends-instead-of-rebuilding`,
  `reset-playout-must-rewrite-near-term-items`
- Discovered: 2026-07-01

ADR 0020 proposes rebuilding impacted playouts when channel, schedule,
collection, or library changes alter already-scheduled item settings or content.

Closeout requires channel saves and schedule/library/collection changes to
enqueue coalesced refresh builds, Classic Refresh/Reset semantics to rewrite the
build window instead of appending only, and focused scheduler/browser proof.

### Similar video cooldown scheduling

- Slug: `similar-video-cooldown`
- Status: `Not started`
- Open debt: `similar-video-cooldown-scheduling`
- Discovered: 2026-07-01

ADR 0019 proposes an opt-in, shared cooldown-aware shuffle policy for schedules
so groupable media can be diversified by artist, TV show, movie franchise-like
metadata, ADR 0018 parent/variant lineage, collection/smart-collection/tag
metadata, or operator-defined search filters.

Closeout requires deterministic cooldown-aware selection, persisted
Continue-build state, disabled-by-default ErsatzTV parity behavior, focused
scheduler tests, and admin or schedule-definition controls for item-count and
duration cooldown thresholds.

## Adjacent work not counted as these parity gaps

The standing module-split survey remains useful maintainer hardening work, but
it is not the current top priority while these parity debts are open.

## Commands used

```bash
cargo xtask progress check
rg -l "^status\\s*= 'Ported'" docs/progress/parity | wc -l
rg -l "^status\\s*= 'In progress'" docs/progress/parity | wc -l
rg -l "^status\\s*= 'Not started'" docs/progress/parity | wc -l
rg -l "^feature\\s*=" docs/progress/parity | wc -l
rg "resolved\\s*= false" docs/progress/parity | wc -l
cargo xtask progress hydration --pending
git diff --check
```
