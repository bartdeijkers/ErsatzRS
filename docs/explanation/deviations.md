# Deliberate Deviations from ErsatzTV

ErsatzRS preserves ErsatzTV behavior by default. Deliberate deviations must be
listed here and backed by ADRs.

## drawvg graphics instead of external renderer

ErsatzRS uses FFmpeg 8.1 `drawvg` instead of recreating ErsatzTV's external
graphics-renderer process. `drawvg` executes VGS scripts, not SVG documents, so
the older SVG-template wording in implementation notes represents a remaining
graphics follow-up rather than a packaging/runtime decision.

ADR: [0005](../adr/0005-drawvg-graphics-engine.md)

Follow-up ADR: [0008](../adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)

## In-repository FFmpeg packaging

ErsatzTV packages its FFmpeg integration in a separate `ErsatzTV-ffmpeg`
repository. ErsatzRS intentionally keeps FFmpeg and ffprobe packaging in this
repository so the application binaries, migrations, admin UI assets, FFmpeg
8.1 build, and startup runtime checks are released together.

The active release line currently omits Docker/GHCR images and ships Linux and
Windows GitHub archives only. If Docker image publishing returns, release images
must follow the managed FFmpeg runtime model and must not enable
`--enable-nonfree`, bundle the CUDA toolkit, include libnpp, or include
non-redistributable codecs such as libfdk_aac.

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

## Leptos instead of Blazor

ErsatzRS uses Leptos for the admin UI because the project is Rust-native.

ADR: [0002](../adr/0002-ui-framework-leptos.md)

## Plain async handlers instead of MediatR

ErsatzRS uses plain async use-case handlers and constructor-injected traits instead
of recreating MediatR.

ADR: [0001](../adr/0001-record-architecture-decisions.md)
