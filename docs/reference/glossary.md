# Glossary

This reference defines project terms used in ErsatzRS documentation, code, and
porting notes. Prefer these terms in new documents to keep ErsatzRS vocabulary
consistent with ErsatzTV where parity matters.

## A

| Term | Definition |
| --- | --- |
| ADR | Architecture Decision Record. A dated document in [../adr/](../adr/) that records a significant technical decision and its consequences. |
| Artwork | Image metadata associated with media items, shows, artists, channels, or libraries. Artwork can feed UI views, guide data, and generated overlays. |
| Axum | The Rust web framework used for HTTP routing, middleware integration, and protocol endpoints. |

## B

| Term | Definition |
| --- | --- |
| Block | A schedule unit that groups one or more playout items, often with rules for ordering, padding, filler, watermarks, and graphics. |
| Block scheduling | Scheduling model that builds channel timelines from configured, reusable blocks. |

## C

| Term | Definition |
| --- | --- |
| Channel | A linear TV stream made from scheduled media. A channel has playout, guide, streaming, and presentation settings. |
| Collection | A named group of media items. Collections can come from external providers or be managed locally. |
| Complex filter | An FFmpeg filter graph with labeled inputs and outputs. ErsatzRS mirrors ErsatzTV's label-routing patterns when building these graphs. |
| Content resolver | A core abstraction that turns scheduling references, such as collections, playlists, searches, and shows, into concrete media items. |
| Crate | A Rust package or library unit. ErsatzRS is organized as a Cargo workspace with domain, core, FFmpeg, infrastructure, app, web, and scanner crates. |
| Classic scheduling | Scheduling model that fills a channel timeline from ordered program schedule items and reusable content sources. |

## D

| Term | Definition |
| --- | --- |
| Diátaxis | Documentation framework that separates tutorials, how-to guides, reference material, and explanation. This glossary is reference material. |
| Direct stream | Serving media without transcoding when the original container, codecs, and stream properties are already suitable for the target client. |
| Drawvg | FFmpeg 8.1 filter that renders vector graphics inside the FFmpeg filter graph. ErsatzRS uses it for channel graphics instead of an external renderer process. |

## E

| Term | Definition |
| --- | --- |
| EPG | Electronic Program Guide. Program metadata that describes channel schedules for clients. |
| ErsatzRS | Rust reimplementation of ErsatzTV focused on feature parity, a simpler deployment model, and FFmpeg 9 capabilities. |
| ErsatzTV | The upstream .NET project used as the behavioral reference for ErsatzRS. |

## F

| Term | Definition |
| --- | --- |
| FFconcat | FFmpeg concat-demuxer playlist format. ErsatzRS uses ffconcat files to join source segments while preserving predictable timestamp handling. |
| FFmpeg profile | A configuration object that controls transcoding, stream selection, hardware acceleration, muxing, and output options. |
| Filler | Media inserted when scheduled content does not fully cover a time window or when a schedule explicitly asks for padding content. |
| Filter graph | FFmpeg processing graph that connects filters such as scaling, subtitles, watermarks, drawvg overlays, and concat/resetpts nodes. |

## G

| Term | Definition |
| --- | --- |
| Graphics element | A scheduled visual overlay, such as a lower third, progress indicator, or channel graphic. ErsatzRS should render these through `drawvg`. |
| Guide | User-facing schedule data for one or more channels, usually exposed through XMLTV, HDHomeRun lineup data, or the admin UI. |

## H

| Term | Definition |
| --- | --- |
| HLS | HTTP Live Streaming. A segmented streaming protocol used by many IPTV and browser clients. ErsatzRS delegates media segment generation to FFmpeg. |
| HDHomeRun | Network tuner protocol and device model that IPTV clients can discover. ErsatzRS provides compatible discovery, lineup, and streaming endpoints. |
| History | Previously scheduled or played media state used by scheduling engines to avoid unwanted repeats and resume deterministic timelines. |
| Hydration | Client-side activation of server-rendered Leptos UI islands. ErsatzRS starts SSR-first and hydrates only workflows that need browser interactivity. |

## I

| Term | Definition |
| --- | --- |
| Infrastructure crate | `ersatzrs-infra`, the crate that owns SQLx repositories, migrations, provider clients, and other external-system adapters. |
| IPTV | Internet Protocol Television. In ErsatzRS, this usually means M3U playlists, MPEG-TS or HLS streams, and guide metadata consumed by IPTV clients. |

## L

| Term | Definition |
| --- | --- |
| Leptos | The Rust UI framework used for the admin interface. ErsatzRS uses SSR first and adds hydrated islands where needed. |
| Library | A top-level grouping of media from a provider or local source, such as movies, shows, music, or other categorized content. |
| Lineup | HDHomeRun-compatible channel list that tells clients which channels are available and where to stream them. |
| Lua scheduling | Scripted scheduling model that evaluates Lua 5.4 instructions to build a playout. |

## M

| Term | Definition |
| --- | --- |
| M3U | Playlist format used by IPTV clients to discover channels and stream URLs. |
| Media item | A playable unit such as a movie, episode, song, or video file. |
| Media source | External or local origin for media metadata and files, such as Plex, Jellyfin, Emby, or a filesystem path. |
| Migration | Database schema change. ErsatzRS starts from a collapsed initial schema instead of porting every ErsatzTV Entity Framework migration. |
| Muxing | Packaging encoded streams into an output container, such as MPEG-TS for IPTV output. |

## P

| Term | Definition |
| --- | --- |
| Parity | Behavioral compatibility with ErsatzTV for the same feature or workflow. Parity does not require identical internal implementation. |
| Pipeline builder | Rust builder that turns playback requirements into FFmpeg inputs, filters, encoders, muxer settings, and command arguments. |
| Playlist | Ordered or rule-driven list of media items. Playlists can be scheduled directly or used as a content source. |
| Playout | Concrete timeline for a channel: what should play, when it should start, and which graphics, watermarks, and filler apply. |
| Post-roll | Content scheduled after a primary media item. |
| Pre-roll | Content scheduled before a primary media item. |
| Provider GUID | Stable external identifier from a provider such as Plex, Jellyfin, or Emby. It helps match, search, and resume media across syncs. |

## R

| Term | Definition |
| --- | --- |
| Remux | Repackaging streams into a different container without re-encoding the audio or video. |
| Repository trait | Core abstraction for persistence operations. SQLx implementations live in `ersatzrs-infra`. |
| Resetpts | FFmpeg timestamp reset pattern used when joining segments so downstream filters and muxers see predictable presentation timestamps. |

## S

| Term | Definition |
| --- | --- |
| Scanner | Background or one-shot process that imports or refreshes media metadata, files, streams, artwork, and provider state. |
| Schedule | Rules and source references used to generate a playout timeline for a channel. |
| Scheduling engine | Component that converts configured schedule rules into playout items. ErsatzRS supports Classic, Block, Sequential YAML, Scripted Lua, and external JSON playout models. |
| Search index | Searchable projection of media metadata used by smart collections and schedule content resolution. |
| Smart collection | Dynamic collection defined by a search query or filter rather than a fixed list of media items. |
| SQLx | Rust database toolkit used by ErsatzRS for SQLite by default and optional MySQL support behind a feature flag. |
| SSR | Server-side rendering. The default rendering mode for the ErsatzRS admin UI. |

## T

| Term | Definition |
| --- | --- |
| Timeline | Ordered sequence of playout items with resolved start times, durations, and scheduling metadata. |
| Transcoding | Decoding and re-encoding media to match a target codec, bitrate, resolution, subtitle mode, or client capability. |

## W

| Term | Definition |
| --- | --- |
| Watermark | Visual overlay, usually a channel logo or bug, placed over video according to schedule or channel configuration. |
| Workspace | Cargo workspace that groups all ErsatzRS crates and shared project configuration. |

## X

| Term | Definition |
| --- | --- |
| XMLTV | XML format commonly used to exchange EPG data. ErsatzRS exposes it at `/iptv/xmltv.xml`. |

## Y

| Term | Definition |
| --- | --- |
| YAML scheduling | Sequential scheduling model that reads YAML instructions and dispatches each instruction type to a handler. |
