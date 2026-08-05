# Architecture

ErsatzRS is a Rust workspace that ports ErsatzTV behavior while keeping crate
boundaries explicit.

## Workspace

```text
crates/
  ersatzrs-domain/    entities, enums, value objects
  ersatzrs-core/      business logic, scheduling traits, repository traits
  ersatzrs-hwprobe/   native OS/GPU capability probes behind a safe API
  ersatzrs-ffmpeg/    FFmpeg argument and filter-graph builders
  ersatzrs-infra/     sqlx repositories, migrations, provider clients
  ersatzrs-app/       use-case handlers
  ersatzrs-scheduler/ playout scheduling engines and background scheduling work
  ersatzrs-web/       axum + Leptos host, IPTV, HLS, HDHR, API routes
  ersatzrs-scanner/   scanner binary
  ersatzrs-launcher/  Windows-only system-tray launcher (no-op on other targets)
migrations/       sqlx migrations
assets/           drawvg source templates, VGS scripts, and related assets
```

## Dependency Rules

```text
ersatzrs-domain
  <- ersatzrs-core
  <- ersatzrs-app
  <- ersatzrs-web
  <- ersatzrs-scanner

ersatzrs-core <- ersatzrs-infra
ersatzrs-ffmpeg <- ersatzrs-core
ersatzrs-ffmpeg <- ersatzrs-web
```

Rules:

- `ersatzrs-domain` depends on no other ErsatzRS crate.
- `ersatzrs-core` owns traits and business rules, not sqlx or HTTP clients.
- `ersatzrs-hwprobe` is the only non-launcher crate allowed to use small,
  platform-guarded native FFI for hardware capability enumeration. It exposes
  safe value objects to `ersatzrs-ffmpeg`; command generation must not contain
  native FFI directly.
- `ersatzrs-infra` implements repository and provider traits from `ersatzrs-core`.
- `ersatzrs-app` coordinates use cases and depends on traits, not concrete storage.
- `ersatzrs-app` must not depend on adapter/runtime crates; the native
  `ersatzrs-web` host is the composition root that injects core-port
  implementations into application services. See
  [ADR 0051](docs/adr/0051-application-boundary-and-composition-root.md).
- `ersatzrs-web` owns HTTP, Leptos, IPTV, HLS, and HDHomeRun endpoints.
- `ersatzrs-web` follows the folder boundaries in
  [docs/reference/web-crate-structure.md](docs/reference/web-crate-structure.md):
  `host` for runtime, `transport` for IPTV/HDHomeRun-style protocols, `api` for
  plain axum admin APIs, and `admin` for the Leptos UI.
- `ersatzrs-scanner` owns scanner CLI concerns.
- `ersatzrs-scheduler` owns the playout scheduling engines and background
  scheduling work; it depends on `ersatzrs-core` traits, not concrete storage.
- `ersatzrs-ffmpeg` owns FFmpeg argv and filter-graph construction, and consumes
  safe hardware capability rows from `ersatzrs-hwprobe`.
- `ersatzrs-launcher` is the Windows-only system-tray launcher. Its real logic
  is under `#[cfg(windows)]`; on every other target it compiles to a no-op
  `main`. It is the single crate that opts out of the workspace
  `unsafe_code = "forbid"` lint for a few small Win32 FFI calls (see
  [AGENTS.md](AGENTS.md) for the documented exception).
- Cycles are forbidden.

## Porting Rule

ErsatzRS is a port, not a greenfield redesign. Read the ErsatzTV source first,
preserve behavior by default, and document deliberate deviations through ADRs.

## Runtime Boundaries

- FFmpeg is invoked through `tokio::process::Command`.
- No libav bindings are allowed without a new ADR.
- FFmpeg 9.x is the supported runtime line; older and unverified future major
  versions are rejected at startup.
- SQLite is the first database backend.
- MySQL support is feature-gated and added later.

## UI Boundary

Leptos is the UI framework. Start server-rendered and add hydration only where
the user workflow needs interactivity.
