# How ErsatzRS builds a channel

ErsatzRS turns a media library into linear television by separating content,
scheduling, and delivery. That separation lets the same movie, episode, song,
or stream participate in several channels without duplicating the media.

## The content layer

A **media source** tells ErsatzRS where content comes from. Local sources point
at filesystem paths; Plex, Jellyfin, and Emby sources synchronize provider
libraries. A **library** gives that content a media kind such as movies, TV
shows, music videos, songs, images, or other videos.

Scanning records metadata, file versions, streams, artwork, languages,
subtitles, provider identifiers, extras, and alternate versions. The original
media remains at its source.

Collections, playlists, smart collections, provider lists, searches, and other
list types are reusable views over scanned media. They answer “which content is
eligible?” without deciding when it plays.

## The schedule layer

A **schedule** answers “how should eligible content fill time?” Schedule items
reference collections, playlists, shows, artists, searches, and other content
sources. Each item can specify ordering, start behavior, duration, filler,
watermarks, graphics, and preferred audio or subtitle streams.

ErsatzRS supports several scheduling models:

- **Classic** schedules combine ordered schedule items and content sources.
- **Block** schedules place reusable blocks of content on a timeline.
- **Sequential YAML** schedules execute repository-defined YAML instructions.
- **Scripted** schedules use Lua 5.4.
- **External JSON** playouts consume a compatible external schedule file.

A **playout** materializes one scheduling model for one channel. It contains the
concrete timeline: media item, start time, finish time, and effective playback
overrides. Refreshing a playout rebuilds its scheduled window from current
settings and library state.

## The channel layer

A **channel** is the stable user-facing identity: number, name, group, guide
visibility, logo, FFmpeg profile, playout source, stream-selection preferences,
and delivery mode. One channel has one active playout source.

When a client requests a channel, ErsatzRS resolves the current playout item,
chooses the requested audio and subtitle streams, builds an FFmpeg command when
transcoding or remuxing is needed, and serves HLS or MPEG-TS output.

The same channel schedule also drives M3U, XMLTV, and HDHomeRun lineup data, so
clients see a consistent channel identity and guide.

## Why the layers matter

Keeping these layers separate makes changes predictable:

- Adding media requires a scan, but does not automatically change every
  channel unless its content source includes the new item.
- Editing a collection changes eligibility; refreshing the affected playout
  applies that change to scheduled rows.
- Editing a schedule changes timeline rules; refreshing or resetting its
  playout materializes those rules.
- Editing a channel changes delivery and presentation without redefining the
  underlying content source.

When debugging, follow the same direction:

```text
readable media -> populated content source -> saved schedule -> populated playout
-> enabled channel -> compatible FFmpeg profile -> client stream
```

The admin **Support** pages expose health and troubleshooting information at
each boundary.

