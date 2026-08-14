# Admin navigation reference

The admin UI is available at the server base URL, which is
`http://<ersatzrs-host>:8409` by default. Labels can be localized from
**Settings** -> **UI**; the routes remain the same.

The **Add-ons** settings row describes the current development tree. Existing
published beta archives do not gain that route retroactively.

## Primary pages

| Page | Route | Purpose |
|---|---|---|
| Channels | `/channels` | Channel identity, number, guide, stream, and FFmpeg profile settings |
| FFmpeg Profiles | `/ffmpeg` | Reusable transcoding, remuxing, stream-selection, and hardware profiles |
| Watermarks | `/watermarks` | Reusable channel and schedule overlays |

## Media Sources

| Page | Route | Purpose |
|---|---|---|
| Local | `/media/sources/local` | Filesystem-backed libraries |
| Add-on Lists | `/media/addon-lists` | Provider lists materialized as file-backed local Remote Stream libraries |
| Emby | `/media/sources/emby` | Emby connection, library, and path settings |
| Jellyfin | `/media/sources/jellyfin` | Jellyfin connection, library, and path settings |
| Plex | `/media/sources/plex` | Plex sign-in, server, library, and path settings |

## Media

| Page | Route | Purpose |
|---|---|---|
| Libraries | `/media/libraries` | Scan, deep-scan, search, and inspect libraries |
| Trash | `/media/trash` | Unavailable and removed media |
| TV Shows | `/media/tv/shows` | Browse shows, seasons, and episodes |
| Movies | `/media/movies` | Browse movies and their media versions |
| Music | `/media/music/artists` | Browse artists and music videos |
| Other Videos | `/media/other/videos` | Browse non-movie video libraries |
| Songs | `/media/music/songs` | Browse audio tracks |
| Images | `/media/browser/images` | Browse image media |
| Remote Streams | `/media/remote/streams` | Browse configured remote streams |

## Lists

| Page | Route | Purpose |
|---|---|---|
| Collections | `/media/collections` | Fixed, reusable groups of media |
| Smart Collections | `/media/smart-collections` | Dynamic groups defined by search rules |
| Multi Collections | `/media/multi-collections` | Groups composed from other collections |
| Rerun Collections | `/media/rerun-collections` | Collections that track rerun state |
| Playlists | `/media/playlists` | Ordered media lists |
| Filler Presets | `/media/filler/presets` | Reusable pre-roll, post-roll, and padding rules |

## Scheduling

| Page | Route | Purpose |
|---|---|---|
| Schedules | `/schedules` | Classic program schedules and their items |
| Blocks | `/blocks` | Reusable blocks for block scheduling |
| Templates | `/templates` | Reusable schedule templates |
| Decorations | `/decos` | Schedule decoration definitions |
| Deco Templates | `/deco-templates` | Reusable decoration templates |
| Playouts | `/playouts` | Materialized channel timelines and scheduling engines |

## Settings

| Page | Route | Purpose |
|---|---|---|
| FFmpeg | `/settings/ffmpeg` | Runtime, segmenter, subtitle, and transcode settings |
| Logging | `/settings/logging` | Global and subsystem log levels |
| HDHomeRun | `/settings/hdhr` | Tuner and discovery settings |
| Scanner | `/settings/scanner` | Scan and refresh behavior |
| Playout | `/settings/playout` | Playout horizon and scheduling behavior |
| UI | `/settings/ui` | Language, theme, and Windows login startup |
| XMLTV | `/settings/xmltv` | Guide window, time zone, and block behavior |
| Add-ons | `/settings/addons` | Trusted repositories, packages, permissions, readiness, and settings |

## Support

| Page | Route | Purpose |
|---|---|---|
| Health Checks | `/system/health` | Database, schema, FFmpeg, provider, media, and runtime health |
| Logging | `/system/logs` | Current application log |
| Troubleshooting | `/system/troubleshooting` | Diagnostics for media, schedules, playouts, channels, and MPEG-TS output |
