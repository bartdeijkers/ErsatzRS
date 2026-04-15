# Architecture

NasTv is a Rust workspace that ports ErsatzTV behavior while keeping crate
boundaries explicit.

## Planned Workspace

```text
crates/
  nastv-domain/   entities, enums, value objects
  nastv-core/     business logic, scheduling traits, repository traits
  nastv-ffmpeg/   FFmpeg argument and filter-graph builders
  nastv-infra/    sqlx repositories, migrations, provider clients
  nastv-app/      use-case handlers
  nastv-web/      axum + Leptos host, IPTV, HLS, HDHR, API routes
  nastv-scanner/  scanner binary
migrations/       sqlx migrations
assets/           drawvg SVG templates and related assets
```

## Dependency Rules

```text
nastv-domain
  <- nastv-core
  <- nastv-app
  <- nastv-web
  <- nastv-scanner

nastv-core <- nastv-infra
nastv-ffmpeg <- nastv-core
nastv-ffmpeg <- nastv-web
```

Rules:

- `nastv-domain` depends on no other NasTv crate.
- `nastv-core` owns traits and business rules, not sqlx or HTTP clients.
- `nastv-infra` implements repository and provider traits from `nastv-core`.
- `nastv-app` coordinates use cases and depends on traits, not concrete storage.
- `nastv-web` owns HTTP, Leptos, IPTV, HLS, and HDHomeRun endpoints.
- `nastv-scanner` owns scanner CLI concerns.
- `nastv-ffmpeg` owns FFmpeg argv and filter-graph construction.
- Cycles are forbidden.

## Porting Rule

NasTv is a port, not a greenfield redesign. Read the ErsatzTV source first,
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
