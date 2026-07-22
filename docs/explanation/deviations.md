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

## Leptos instead of Blazor

ErsatzRS uses Leptos for the admin UI because the project is Rust-native.

ADR: [0002](../adr/0002-ui-framework-leptos.md)

## Plain async handlers instead of MediatR

ErsatzRS uses plain async use-case handlers and constructor-injected traits instead
of recreating MediatR.

ADR: [0001](../adr/0001-record-architecture-decisions.md)
