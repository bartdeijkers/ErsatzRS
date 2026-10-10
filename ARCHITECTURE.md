# Architecture

ErsatzRS is a Rust workspace that ports ErsatzTV behavior while keeping crate
boundaries explicit.

## Workspace

```text
crates/
  ersatzrs-addon-contract/ versioned package, runtime, and NDJSON wire DTOs
  ersatzrs-domain/    entities, enums, value objects
  ersatzrs-core/      business logic, scheduling traits, repository traits
  ersatzrs-hwprobe/   native OS/GPU capability probes behind a safe API
  ersatzrs-platform/  dependency-light operating-system integrations
  ersatzrs-ffmpeg/    FFmpeg argument and filter-graph builders
  ersatzrs-infra/     sqlx repositories, migrations, provider clients
  ersatzrs-app/       use-case handlers
  ersatzrs-scheduler/ playout scheduling engines and background scheduling work
  ersatzrs-web/       axum + Leptos host, IPTV, HLS, HDHR, API routes
  ersatzrs-scanner/   scanner binary
  ersatzrs-healthcheck/ readiness-probe binary
  ersatzrs-launcher/  Windows-only system-tray launcher (no-op on other targets)
  xtask/               validation, progress, focused-check, and read-only runtime-audit tooling
migrations/       sqlx migrations
assets/           drawvg source templates, VGS scripts, and related assets
```

## Dependency Rules

Direct workspace dependencies are:

| Crate | Direct ErsatzRS dependencies |
|---|---|
| `ersatzrs-addon-contract` | none |
| `ersatzrs-domain` | none |
| `ersatzrs-core` | `ersatzrs-domain` |
| `ersatzrs-hwprobe` | none |
| `ersatzrs-platform` | none |
| `ersatzrs-ffmpeg` | `ersatzrs-domain`, `ersatzrs-hwprobe` |
| `ersatzrs-infra` | `ersatzrs-addon-contract`, `ersatzrs-core`, `ersatzrs-domain`, `ersatzrs-ffmpeg` |
| `ersatzrs-app` | `ersatzrs-addon-contract`, `ersatzrs-core`, `ersatzrs-domain` |
| `ersatzrs-scanner` | `ersatzrs-addon-contract`, `ersatzrs-core`, `ersatzrs-domain`, `ersatzrs-ffmpeg`, `ersatzrs-infra` |
| `ersatzrs-scheduler` | `ersatzrs-app`, `ersatzrs-core`, `ersatzrs-ffmpeg`, `ersatzrs-infra` |
| `ersatzrs-web` | `ersatzrs-addon-contract`, `ersatzrs-app`, `ersatzrs-core`, `ersatzrs-domain`, `ersatzrs-ffmpeg`, `ersatzrs-infra`, `ersatzrs-platform`, `ersatzrs-scanner`, `ersatzrs-scheduler` |
| `ersatzrs-healthcheck`, `ersatzrs-launcher` | none |
| `xtask` | `ersatzrs-addon-contract` |

Rules:

- `ersatzrs-domain` depends on no other ErsatzRS crate.
- `ersatzrs-addon-contract` is dependency-light and owns only versioned public
  add-on data contracts. It does not load packages or depend on application,
  storage, web, scanner, or FFmpeg crates.
- `ersatzrs-core` owns traits and business rules, not sqlx or HTTP clients.
- `ersatzrs-hwprobe` is the only non-launcher crate allowed to use small,
  platform-guarded native FFI for hardware capability enumeration. It exposes
  safe value objects to `ersatzrs-ffmpeg`; command generation must not contain
  native FFI directly.
- `ersatzrs-platform` owns dependency-light safe operating-system adapters that
  need focused native tests. Its Windows startup adapter uses the safe `winreg`
  API and returns an explicit unsupported result on every non-Windows target.
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
  scheduling work. Its current runtime integration depends on application,
  FFmpeg, infrastructure, and SQLx types as shown in the table above.
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
- SQLite is the sole supported database backend. The web host and scanner may
  share one database file on one host; active-active servers and database files
  on network filesystems are unsupported. See
  [ADR 0080](docs/adr/0080-standardize-on-sqlite.md).
- Add-ons are detached-signature-verified packages started as child processes;
  no native or Rust dynamic-library ABI is loaded into ErsatzRS.
- `ersatzrs-infra` owns repository trust, immutable package storage, settings
  projection, content verification, and bounded invocation. `ersatzrs-web` is
  the composition root and owns automatic catalog refresh and administration.
- The separate official add-ons repository is an external software system. Its
  private Ed25519 signing key never belongs to this workspace or the runtime
  database.

## Streaming engines

[ADR 0095](docs/adr/0095-native-pipeline-instead-of-ersatztv-next-engine.md)
owns the integration of ErsatzTV Next alongside the native FFmpeg pipeline.
The web host supervises the external Next process, publishes channel/playout
configuration and serves session-bound media and graphics through authenticated
internal callbacks. Both engines reuse host selection and source ownership;
their supported output and subtitle capabilities need separate verification.
Release and Docker packaging stage pinned Next artifacts. The accepted
automatic-availability condition for new-channel and standalone-preview defaults
is distinct from implementation acceptance; existing engine selections remain.
The native runtime and graphics contracts above remain in force. Current
acquisition/default acceptance and platform limitations are tracked in the
[catch-up parity record](docs/progress/parity/upstream-baseline-catch-up.toml).

## Runtime views

The current [System Context and Container views](docs/explanation/runtime-architecture.md)
model runnable applications and data stores rather than crates. They include
the implemented add-on child-process boundary and exclude future teletext and
video-jukebox components.

## Teletext authoring boundary

[ADR 0123](docs/adr/0123-level-2-5-teletext-and-external-editor-workflows.md)
extends the planned generic engine through EN 300 706 Level 2.5 with a correct
Level 1/1.5 fallback and QTeletextMaker TTI/sidecar interchange. The existing
unpublished base encoder/composition is partial implementation, not enhanced
acceptance. Domain types own checked pages/resources; application commands own
validation, drafts and explicit publication; infra owns durable state and addon
trust; the native host composes transport and admin/browser delivery. Version
enhanced addon payloads explicitly. Stream and preview use the same validated
model. Import cannot alter a live generation or installed package.

Presentation, themes, declared custom zones and generated guide content remain
addon-owned under ADR 0122. A versioned external sidecar preserves these bindings
and exact page/subpage/navigation identities; imported authority is checked
against the host. QTeletextMaker is an optional external design tool, not a
server rendering process. Client/decoder evidence records versions and coverage;
Kodi is optional, and TSDuck transport evidence alone cannot accept Level 2.5
display. Independent synthetic engine acceptance precedes the sole production
TV-guide addon, whose reference stays NOS TV-guide pages 200–299.

## UI Boundary

Leptos is the UI framework. Start server-rendered and add hydration only where
the user workflow needs interactivity.
