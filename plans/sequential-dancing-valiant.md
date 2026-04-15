# NasTv - Rust/FFmpeg 8.1 Reimplementation of ErsatzTV

## Context

NasTv is a from-scratch Rust reimplementation of
[ErsatzTV](/home/bart/ErsatzTV/). ErsatzTV is a .NET media server that turns
personal media libraries from Plex, Jellyfin, Emby, and local folders into
linear TV channels served through IPTV, HLS, and HDHomeRun-compatible endpoints.

The long-term goal is feature parity with ErsatzTV while simplifying deployment
to a static Rust server binary plus a scanner binary. The main technical
divergence is the use of FFmpeg 8.1, especially the `drawvg` filter, to replace
ErsatzTV's external graphics-renderer pipe.

Locked decisions:

- UI: Leptos with SSR first and hydration only where needed.
- FFmpeg: spawn the CLI through `tokio::process::Command`; no libav bindings.
- Scope: long-term parity, broad foundation from the start.
- Repository: `/home/bart/nastv`.

## Tech Stack

### FFmpeg 8.1

| Feature | Relevance to NasTv |
| --- | --- |
| `drawvg` filter | Core win: channel logos, progress bars, lower thirds, and EPG-style overlays inside the filter graph. |
| Vulkan-accelerated scale/color conversion | Useful acceleration without tying the whole pipeline to one vendor path. |
| D3D12 filters | Windows hardware path alongside NVENC, QSV, and AMF. |
| JPEG-XS, xHE-AAC, MPEG-H 3D audio | Future-facing codec support, not MVP-critical. |
| EXIF parsing | Useful for still-image channels and slideshows. |

ErsatzTV's FFmpeg pipeline patterns are version-agnostic and should be mirrored
closely where they remain valid: filter-label routing, watermark fade,
subtitle overlay, concat/resetpts handling, tonemapping, and output muxing.

### Rust Ecosystem

| Need | Rust choice | Notes |
| --- | --- | --- |
| Web host | `axum`, `tower`, `tokio` | Server runtime and routing. |
| Admin UI | `leptos`, `leptos_axum`, `leptos_router` | Start SSR-only; hydrate interactive islands later. |
| CQRS handlers | Plain `async fn` handlers | Repository traits in `nastv-core`, use cases in `nastv-app`. |
| Persistence | `sqlx` | SQLite first, MySQL later behind a feature flag. |
| Errors | `thiserror`, `anyhow` | `thiserror` in libraries, `anyhow` at binary edges. |
| FFmpeg process | `tokio::process::Command` | Same process model as ErsatzTV's `CliWrap` usage. |
| FFmpeg builder | NasTv builder modules | Port structure from `ErsatzTV.FFmpeg`. |
| Lua scheduling | `mlua` | Lua 5.4 for scripted scheduling. |
| YAML scheduling | `serde_yaml` or maintained equivalent | Per-instruction dispatch. |
| Provider clients | `reqwest`, `serde` | Port DTOs and sync behavior from ErsatzTV infrastructure. |
| HLS output | FFmpeg | Do not implement an HLS muxer in Rust. |
| HDHomeRun UDP | `tokio::net::UdpSocket` | Discovery on port 65001. |
| Scanner | `nastv-scanner` binary | Shares domain, core, and infrastructure crates. |
| Logging | `tracing`, `tracing-subscriber` | Structured diagnostics. |
| Configuration | `figment` or `config` | Defaults, file, env, CLI layering. |
| Testing | `tokio::test`, `rstest`, `insta` | Snapshot generated FFmpeg argv and timelines. |

## Risks

1. Leptos has a different reactive model than Blazor. Start server-rendered and
   add hydration only where it pays for itself.
2. MudBlazor has no direct Rust replacement. Budget time for a small component
   layer or evaluate Rust UI component libraries before committing to one.
3. SQLite and MySQL support require careful feature-gated repository design.
4. ErsatzTV's Entity Framework migration history must not be ported one-to-one.
   NasTv starts with a collapsed initial schema based on the final model.

## Architecture: Crate Layout

```text
NasTv/
  Cargo.toml
  crates/
    nastv-domain/   entities, enums, value objects
    nastv-core/     business logic, scheduling engines, repository traits
    nastv-ffmpeg/   FFmpeg command builders and pipeline builders
    nastv-infra/    sqlx repositories, migrations, provider HTTP clients
    nastv-app/      use-case handlers
    nastv-web/      axum + Leptos host, API, IPTV, HLS, HDHR endpoints
    nastv-scanner/  scanner binary
  migrations/       shared sqlx migrations where practical
  assets/           SVG templates and drawvg overlay assets
```

Binaries:

- `nastv-web`: server.
- `nastv-scanner`: media scanner.

The largest and riskiest crates are expected to be `nastv-core` and
`nastv-ffmpeg`.

## Phases

### Phase 0 - Bootstrap

- Create the Cargo workspace and empty crate stubs.
- Add `rustfmt`, clippy, cargo-deny, and GitHub Actions.
- Add FFmpeg 8.1 runtime detection and fail fast on older versions.
- Add a Docker image with FFmpeg 8.1 and the `nastv-web` binary.
- Add minimal `axum` + Leptos SSR page.
- Add SQLite bootstrap.

### Phase 1 - Domain and Persistence

- Port domain entities from `/home/bart/ErsatzTV/ErsatzTV.Core/Domain/`.
- Include channel, playout, schedule, media, collection, playlist, block,
  library, media-source, FFmpeg profile, watermark, graphics, artwork, and
  filler concepts.
- Create the initial sqlx schema from the final `TvContext.cs` model.
- Add repository traits in `nastv-core`.
- Add sqlx implementations in `nastv-infra`.
- Add entity-mapping tests.

### Phase 2 - FFmpeg Pipeline and drawvg Graphics

- Port pipeline-builder structure from `PipelineBuilderBase.cs` and
  `SoftwarePipelineBuilder.cs`.
- Port `CommandGenerator.cs`.
- Port `ComplexFilter.cs` label routing.
- Start with the software pipeline, then add NVENC and VAAPI.
- Port watermark, subtitle, and song-progress filters.
- Add a drawvg-based graphics engine with SVG templates in `assets/`.
- Port ffconcat v1.0 playlist writing and resetpts handling.
- Snapshot generated FFmpeg argv and filter graphs with `insta`.

#### Test Fixtures

Phase 2 pipeline and streaming tests use local MP4 files derived from
**Big Buck Bunny** (Creative Commons, Blender Foundation). Before running
Phase 2 tests, download and unzip the following archives into a local `testdata/`
directory (not committed to the repository) if not already present:

| Resolution | FPS | Archive URL |
| --- | --- | --- |
| 1080p | 30 | https://download.blender.org/demo/movies/BBB/bbb_sunflower_1080p_30fps_normal.mp4.zip |
| 1080p | 60 | https://download.blender.org/demo/movies/BBB/bbb_sunflower_1080p_60fps_normal.mp4.zip |
| 2160p | 30 | https://download.blender.org/demo/movies/BBB/bbb_sunflower_2160p_30fps_normal.mp4.zip |
| 2160p | 60 | https://download.blender.org/demo/movies/BBB/bbb_sunflower_2160p_60fps_normal.mp4.zip |

Tests that require these files should be skipped automatically (e.g., via
`#[ignore]` or a feature flag) when the files are absent, so CI can still run
unit tests without the large downloads.

### Phase 3 - Scheduling Engines

Port the three scheduling engines from
`/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/` behind a shared Rust trait:

1. Block scheduling.
2. YAML scheduling with per-instruction dispatch.
3. Scripted scheduling through Lua 5.4 with `mlua`.

The playout state machine must handle time anchors, gaps, pre-roll, post-roll,
graphics, watermarks, and filler behavior.

#### Test Fixtures

Phase 3 integration and scheduling tests use local MP4 files derived from
**Big Buck Bunny** (Creative Commons, Blender Foundation). Before running
Phase 3 tests, download and unzip the following archives into a local `testdata/`
directory (not committed to the repository) if not already present:

| Resolution | FPS | Archive URL |
| --- | --- | --- |
| 1080p | 30 | https://download.blender.org/demo/movies/BBB/bbb_sunflower_1080p_30fps_normal.mp4.zip |
| 1080p | 60 | https://download.blender.org/demo/movies/BBB/bbb_sunflower_1080p_60fps_normal.mp4.zip |
| 2160p | 30 | https://download.blender.org/demo/movies/BBB/bbb_sunflower_2160p_30fps_normal.mp4.zip |
| 2160p | 60 | https://download.blender.org/demo/movies/BBB/bbb_sunflower_2160p_60fps_normal.mp4.zip |

Tests that require these files should be skipped automatically (e.g., via
`#[ignore]` or a feature flag) when the files are absent, so CI can still run
unit tests without the large downloads.

### Phase 4 - Media Source Integrations

- Port Plex, Jellyfin, and Emby HTTP clients.
- Add local filesystem scanning.
- Add path replacement logic for local and container paths.
- Add incremental sync where the upstream provider supports it.
- Invoke `ffprobe` for media metadata.
- Implement scanner behavior in `nastv-scanner`.

### Phase 5 - Protocol Endpoints

- Add `/iptv/channels.m3u`.
- Add HLS video endpoints.
- Add HDHomeRun discovery and lineup endpoints.
- Add UDP discovery on port 65001.
- Add XMLTV guide output.
- Port relevant ErsatzTV controller behavior into axum routers.

### Phase 6 - Leptos Admin UI

Port the admin UI incrementally. Priority order:

1. Channel list and editor.
2. Media source configuration.
3. Library browser and collections.
4. Block editor.
5. YAML playout editor with schema validation.
6. FFmpeg profile editor.
7. Graphics and watermark editor with drawvg preview.
8. Playout timeline visualizer.

Start server-rendered and add islands hydration where the workflow requires it.

### Phase 7 - Hardening and Parity Completion

- Add additional hardware paths: QSV, AMF, VideoToolbox, RKMPP, V4L2M2M.
- Enable MySQL backend support.
- Add Trakt integration and version checks.
- Add Docker multi-arch images.
- Benchmark side by side against ErsatzTV.
- Run long soak tests.

## Critical Source Files for Porting

| Goal | ErsatzTV source |
| --- | --- |
| Pipeline builder pattern | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs` |
| Software pipeline | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Pipeline/SoftwarePipelineBuilder.cs` |
| Filter complex generation | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` |
| Watermark overlays | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/OverlayWatermarkFilter.cs` |
| Song progress filter | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/SongProgressFilter.cs` |
| FFmpeg argv assembly | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/CommandGenerator.cs` |
| Process spawning | `/home/bart/ErsatzTV/ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs` |
| Seamless concat | `/home/bart/ErsatzTV/ErsatzTV.Core/FFmpeg/ConcatPlaylist.cs` |
| Database model | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs` |
| Scheduling engines | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/` |
| HLS output config | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/OutputFormat/OutputFormatHls.cs` |
| HDHomeRun protocol notes | `/home/bart/ErsatzTV/hdhr.md` |
| Provider clients | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Plex/`, `Jellyfin/`, `Emby/` |
| Domain reference | `/home/bart/ErsatzTV/ErsatzTV.Core/Domain/` |

## Verification

### Phase 0

- `cargo build --workspace` passes.
- `curl localhost:8080/healthz` returns `200`.
- `ffmpeg -version` in the container reports 8.1.x or newer.
- A SQLite database file is created at startup.

### Phase 1

- Domain entities have sqlx round-trip tests where persistence exists.
- Migrations run idempotently on an empty database and a started database.
- `cargo test -p nastv-domain -p nastv-infra` passes.

### Phase 2

- Snapshot tests cover FFmpeg argv output for software and NVENC paths.
- A local MP4 can stream through `/iptv/video/<channel>` and play in VLC.
- A drawvg overlay renders on video.
- Watermark fade-in and fade-out can be verified in VLC.

### Phase 3

- Unit tests cover each YAML instruction handler.
- Block scheduling output is compared with ErsatzTV output where possible.
- A Lua scripted playout rotates media in the expected order.

### Phase 4

- A Jellyfin container integration test can scan a library into the database.
- Path replacement works for local and container scenarios.
- Plex and Emby clients have equivalent coverage once implemented.

### Phase 5

- VLC can load `/iptv/channels.m3u`.
- Plex DVR can discover the HDHomeRun-compatible device.
- `/guide` returns XMLTV that Plex or Jellyfin can parse.

### Phase 6

- A user can create a channel, configure a media source, build a playout, and
  start a stream from the UI.
- Critical UI paths have no browser-console errors.

### Phase 7

- Side-by-side benchmarks compare NasTv and ErsatzTV on the same channel config.
- A 24-hour single-channel soak test runs without crash or memory leak.

## References

- ErsatzTV source tree: `/home/bart/ErsatzTV/`.
- HDHomeRun notes: `/home/bart/ErsatzTV/hdhr.md`.
- NasTv contracts: [../CONTRIBUTING.md](../CONTRIBUTING.md),
  [../STYLE.md](../STYLE.md), [../ARCHITECTURE.md](../ARCHITECTURE.md), and
  [../PORTING.md](../PORTING.md).
