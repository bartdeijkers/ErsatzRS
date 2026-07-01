# Human parity gap report

Generated: 2026-07-01T10:24:45+02:00

This is a human-readable snapshot. The source of truth remains the structured
progress files in this directory and the `cargo xtask progress ...` queries.

## Headline

ErsatzRS has **7 tracked not-yet-Ported parity/spec features** across the
structured parity records.

Maintainer-reported regressions reopened media-browser detail/action parity,
ADR 0018 movie-detail content-variant surfacing, the channel-add default group,
ADR 0017 default FFmpeg profile bitrates, FFmpeg profile editor section
spacing, and the in-app Zero Duration health check. ADR 0019 also added a Not
started similar-video cooldown scheduling extension. The tracker now has 11
open `[[debt]]` items.

## Counts

| Area | Count | Human meaning |
| --- | ---: | --- |
| Tracked parity features | 38 | Feature records under `docs/progress/parity/`. |
| Ported parity features | 31 | Features currently marked `Ported`. |
| In-progress parity features | 6 | Features currently marked `In progress`. |
| Not-started parity/spec features | 1 | Tracked follow-up feature currently marked `Not started`. |
| Open parity debt items | 11 | `[[debt]]` records with `resolved = false`. |
| Advertised-but-404 admin routes | 0 | No unrouted rows are recorded in the current progress snapshot. |
| Pending hydration-proof rows | 2 | `hydration.toml` has reopened artist/movie rows. |

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

### Content variants, alternate versions, and stream roles

- Slug: `content-variants`
- Status: `In progress`
- Open debt: `movie-detail-commentary-edition-cards`
- Discovered: 2026-07-01

The movie detail page does not provide playable cards/options for watching the
movie with a specific commentary track or choosing alternate movie editions
such as theatrical, remastered, director's cut, black-and-white, and related
version subtypes.

Closeout requires browser/runtime proof on `/media/movies/:id` that
commentary-track viewing and alternate edition cards are visible, select the
intended stream or alternate version, and stay compatible with ADR 0018 search
and playback behavior.

### ErsatzRS healthcheck binary and health engine

- Slug: `ersatzrs-healthcheck`
- Status: `In progress`
- Open debt: `zero-duration-healthcheck-static-images`
- Discovered: 2026-07-01

The in-app Zero Duration health check warns for static images even though zero
duration is expected for still image media. Ordinary image rows should be
ignored; animated/timed images remain candidates for missing-duration warnings
when they should carry duration metadata.

Closeout requires health-check proof that static image rows pass without
warnings while duration-bearing media, including animated image formats where
applicable, still report missing duration when appropriate.

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

### Media browsers (movies, shows, images, music, other videos, songs)

- Slug: `media-browsers`
- Status: `In progress`
- Open debts:
  - `artist-detail-add-to-collection-and-previews-regression`
  - `media-detail-bulk-overflow-actions`
  - `movie-detail-extra-collection-playlist-actions`
  - `movie-detail-extras-row-layout-regression`
  - `songs-images-toolbar-placement`
- Discovered: 2026-07-01

The artist detail page has a broken artist-level Add To Collection button and
broken preview images. Movie extras render like poster cards instead of
season/episode-style rows and cannot be added to collections or playlists like
TV episode extras. Several media detail/list surfaces are missing the
multi-select toolbar and/or per-item three-dots overflow menus that
`/media/movies` provides. Songs and images show the toolbar at the top instead
of the right-side movie/TV browser layout.

Closeout requires browser/runtime proof for the affected movie, TV show, TV
season, artist, songs, and images routes, plus focused coverage for the action,
artwork, layout, and persistence paths that change.

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
git diff --check
```
