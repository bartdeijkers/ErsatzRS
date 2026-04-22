# Architecture

ErsatzRS is a Rust workspace that ports ErsatzTV behavior while keeping crate
boundaries explicit.

## Planned Workspace

```text
crates/
  ersatzrs-domain/   entities, enums, value objects
  ersatzrs-core/     business logic, scheduling traits, repository traits
  ersatzrs-ffmpeg/   FFmpeg argument and filter-graph builders
  ersatzrs-infra/    sqlx repositories, migrations, provider clients
  ersatzrs-app/      use-case handlers
  ersatzrs-web/      axum + Leptos host, IPTV, HLS, HDHR, API routes
  ersatzrs-scanner/  scanner binary
migrations/       sqlx migrations
assets/           drawvg SVG templates and related assets
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
- `ersatzrs-infra` implements repository and provider traits from `ersatzrs-core`.
- `ersatzrs-app` coordinates use cases and depends on traits, not concrete storage.
- `ersatzrs-web` owns HTTP, Leptos, IPTV, HLS, and HDHomeRun endpoints.
- `ersatzrs-web` follows the folder boundaries in
  [docs/reference/web-crate-structure.md](docs/reference/web-crate-structure.md):
  `host` for runtime, `transport` for IPTV/HDHomeRun-style protocols, `api` for
  plain axum admin APIs, and `admin` for the Leptos UI.
- `ersatzrs-scanner` owns scanner CLI concerns.
- `ersatzrs-ffmpeg` owns FFmpeg argv and filter-graph construction.
- Cycles are forbidden.

## Porting Rule

ErsatzRS is a port, not a greenfield redesign. Read the ErsatzTV source first,
preserve behavior by default, and document deliberate deviations through ADRs.

## Runtime Boundaries

- FFmpeg is invoked through `tokio::process::Command`.
- No libav bindings are allowed without a new ADR.
- FFmpeg 8.1 is a hard runtime requirement.
- SQLite is the first database backend.
- MySQL support is feature-gated and added later.

## UI Boundary

Leptos is the UI framework. Start server-rendered and add hydration only where
the user workflow needs interactivity.
