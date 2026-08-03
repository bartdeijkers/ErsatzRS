# Deliberate deviations from ErsatzTV

ErsatzRS preserves ErsatzTV behavior by default. Deliberate deviations must be
listed here and backed by ADRs.

## drawvg graphics instead of external renderer

ErsatzRS uses FFmpeg 8.1 `drawvg` instead of recreating ErsatzTV's external
graphics-renderer process. `drawvg` executes VGS scripts, not SVG documents, so
the older SVG-template wording in implementation notes represents a remaining
graphics follow-up rather than a packaging/runtime decision.

ADR: [0005](../adr/0005-drawvg-graphics-engine.md)

Follow-up ADR: [0008](../adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)

## Managed first-party FFmpeg runtime

ErsatzTV relies on its separate `ErsatzTV-ffmpeg` build project. ErsatzRS uses
the same separation of responsibilities through the first-party
`ErsatzRS-ffmpeg` repository, but adds an application-side runtime manager that
can download, validate, update, and report the selected FFmpeg and ffprobe pair
at startup.

Application archives and container images do not bundle FFmpeg. Normal release
tags can publish Linux and Windows archives plus a multi-architecture container
image; the current beta line publishes only the Windows archive. macOS remains
a supported build target but is not published by the active application
workflow. First-party runtime packages must not enable `--enable-nonfree`,
bundle the CUDA toolkit, include libnpp, or include non-redistributable codecs
such as libfdk_aac.

ADR: [0008](../adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)

## Explicit FFmpeg and ffprobe runtime checks

ErsatzRS fails startup when `ffmpeg` or `ffprobe` is missing, older than 8.1,
or lacks required CPU/drawvg/text/codec capabilities. The admin FFmpeg profile
hardware list uses the startup capability report instead of probing
`ffmpeg -hwaccels` per request.

GPU acceleration support is advertised from what the runtime image actually
reports, using both FFmpeg hardware accelerators and encoder names. The admin
FFmpeg profile editor preserves a saved hardware value even when that value is
not available in the current runtime report, so imported profiles can still be
inspected and corrected. Vulkan remains a runtime/image capability used by the
FFmpeg toolchain and filters; it is not exposed as an FFmpeg profile
`HardwareAccelerationKind` because ErsatzTV does not define a Vulkan profile
enum value. Host GPU access remains an operator responsibility: NVIDIA requires
container GPU runtime support, while VAAPI and Vulkan require `/dev/dri`
device passthrough. The `linux/arm/v7` image is CPU-only for this slice.

ADR: [0008](../adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)

## Global subtitle language preferences

ErsatzTV has a global default audio language but requires subtitle language
preferences on each channel or schedule item. ErsatzRS adds optional global
default and fallback subtitle languages. Built-in playback uses item and
channel preferences first, then the global default, and tries the global
fallback before selecting no subtitle. With both settings unset, existing
unfiltered subtitle selection is unchanged.

ADR: [0052](../adr/0052-global-subtitle-language-preferences.md)

## Collapsed initial schema

ErsatzRS starts from one initial sqlx schema based on the final ErsatzTV model
instead of porting every historic EF migration.

ADR: [0004](../adr/0004-collapsed-initial-schema.md)

## sqlx instead of EF Core

ErsatzRS uses explicit SQL through sqlx, with SQLite as the default backend and
MySQL support behind a Cargo feature flag.

ADR: [0006](../adr/0006-sqlx-over-sea-orm.md)

## SQL-backed search without Elasticsearch

ErsatzRS keeps search in process and SQL-backed for the parity target. The core
search parser/evaluator preserves the Lucene-style behavior needed by
scheduling, smart collections, and admin search, but ErsatzRS does not port
ErsatzTV's experimental Elasticsearch backend.

ADR: [0009](../adr/0009-sql-backed-search-and-no-elasticsearch.md)

## Collection and playlist item playback overrides

ErsatzTV stores preferred audio and subtitle settings on channels, program
schedule items, and generated playout items, but not on `CollectionItem` or
`PlaylistItem`. ErsatzRS adds optional playback override columns to those rows so
an admin can save a movie in a reusable collection or playlist with a specific
named audio track, such as commentary.

ADR: [0048](../adr/0048-collection-item-playback-overrides.md)

## Direct Remote Stream pipelines and per-card playlist actions

ErsatzTV exposes script producers through an internal loopback HTTP endpoint.
ErsatzRS connects producer stdout directly to the managed FFmpeg or ffprobe
stdin so producer ownership, cancellation, and error handling remain in one
process lifecycle.

The Remote Streams card menu also exposes Add To Playlist directly. ErsatzTV's
pinned list offers only the per-card Add To Collection action even though its
shared collection type already defines Remote Stream playlist entries.

ADR: [0057](../adr/0057-direct-remote-stream-pipelines-and-playlist-actions.md)

## Leptos instead of Blazor

ErsatzRS uses Leptos for the admin UI because the project is Rust-native.

ADR: [0002](../adr/0002-ui-framework-leptos.md)

## Platform-owned automatic startup

Windows automatic startup matches ErsatzTV's per-user login behavior while
registering the ErsatzRS tray launcher, so the notification-area lifecycle and
graceful **Exit** action remain available. The live HKCU registration is the
source of truth instead of a database flag.

Linux portable archives document the same service-manager ownership pattern as
Jellyfin packages, but do not let the web process install or enable privileged
systemd configuration. Operators install the documented unit explicitly.

ADR: [0059](../adr/0059-platform-owned-automatic-startup.md)

## Conservative local television identity

ErsatzTV matches local shows by title/year with an episode-path recovery
fallback. ErsatzRS retains that baseline but prefers exclusive top-level-folder
provenance and exact normalized NFO provider GUIDs, and it does not merge two
separate no-year/no-GUID folders by title alone. Full scans transactionally
split older collisions while preserving the largest hierarchy's existing ID
and references.

ADR: [0060](../adr/0060-local-television-folder-identity.md)

## Aggregate EPG and browser-compatible channel preview

ErsatzTV exposes channel preview only as a modal. ErsatzRS retains and improves
that quick workflow while adding `/channels/:id/watch` for a bookmarkable full
player and `/epg` for an aggregate, multi-channel lane guide. Both player
surfaces remain click-to-play and share one browser lifecycle. Administration
preview uses a separately registered H.264 eight-bit/AAC derivative so
Chromium and Firefox can decode it while normal IPTV/HLS continues to use the
persisted channel profile.

ADR: [0053](../adr/0053-dedicated-channel-watch-route-and-reusable-preview-modal.md)

Follow-up ADR:
[0054](../adr/0054-aggregate-epg-preview-and-isolated-browser-preview-pipeline.md)

## minijinja instead of Scriban for XMLTV channel-guide templates

ErsatzTV lets administrators override its bundled per-kind Scriban
(`.sbntxt`) XMLTV templates. ErsatzRS ports the same override-folder pattern
onto `minijinja` (Jinja2 syntax) since no Scriban-equivalent crate exists in
the Rust ecosystem, and follows this codebase's existing
`music_video_credits` bundled-template convention (defaults compiled into
the binary, no extract-to-disk startup step) rather than ErsatzTV's
`ResourceExtractorService`.

The bundled output also favors useful guide metadata over two upstream
omissions: movie icons fall back to thumbnail artwork when no poster exists,
and Remote Stream genres are emitted as `<category>` elements. ErsatzTV
already supplies those genres to custom templates but does not render them in
its bundled default.

ADR: [0058](../adr/0058-minijinja-xmltv-channel-guide-templates.md)

## Plain async handlers instead of MediatR

ErsatzRS uses plain async use-case handlers and constructor-injected traits instead
of recreating MediatR.

ADR: [0001](../adr/0001-record-architecture-decisions.md)

## Cue-aware burned-subtitle seek rebase

ErsatzTV re-bases burned text-subtitle (SRT/ASS/WebVTT) cues for each HLS
work-ahead chunk restart via an ffmpeg `-ss`-seeked stream copy, which
silently drops any cue already displaying when the seek point lands mid-cue.
ErsatzRS instead normalizes local or HTTP(S) UTF-8/UTF-16 sources, rewrites
their cue timestamps directly, and clamps a straddling cue's start to the new
chunk's zero instead of dropping it, so the cue keeps showing for its
remaining visible duration. Inputs with unsupported syntax or encodings retain
the previous FFmpeg seek-and-copy compatibility path.

ADR: [0061](../adr/0061-cue-aware-subtitle-seek-rebase.md)
