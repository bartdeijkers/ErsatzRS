# AGENTS.md

This file provides guidance to AI coding agents when working in this repository.
`CLAUDE.md` is a symlink to this file.

## Project

NasTv is a from-scratch Rust reimplementation of [ErsatzTV](/home/bart/ErsatzTV/) - a mediaserver that turns personal media libraries (Plex/Jellyfin/Emby/Local) into linear TV channels served over IPTV (M3U/HLS) and HDHomeRun. The goal is full feature parity with ErsatzTV on a simpler deployment (single static binary) while exploiting FFmpeg 8.1 features that ErsatzTV does not yet use.

The authoritative implementation plan lives at **[/home/bart/nastv/plans/sequential-dancing-valiant.md](/home/bart/nastv/plans/sequential-dancing-valiant.md)** - read it before starting non-trivial work. It contains the phase breakdown, crate layout, risks, and per-phase verification criteria.

## Current status

Phase 0 (bootstrap). The repo currently contains documentation contracts,
planning notes, `README.md`, `LICENSE`, and a Rust-oriented `.gitignore`. There
is no `Cargo.toml`, no crates, no build/test commands yet. When adding code,
follow the workspace layout from the plan's architecture and crate-layout
section.

## Language Contract

- Repository-bound content is English: code, comments, commit messages, docs,
  ADRs, issue templates, changelog entries, and skill files.
- Interactive conversation with the maintainer may be Dutch or English.
- When translating existing notes, preserve technical decisions and replace
  Dutch section titles with English equivalents.

## Contract Index

Read the relevant contract before changing that area:

- [CONTRIBUTING.md](CONTRIBUTING.md) for contribution workflow.
- [STYLE.md](STYLE.md) for Rust style, errors, async, logging, and public API
  expectations.
- [ARCHITECTURE.md](ARCHITECTURE.md) for crate boundaries and dependency rules.
- [PORTING.md](PORTING.md) before porting any ErsatzTV behavior.
- [docs/how-to/testing.md](docs/how-to/testing.md) before adding tests.
- [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) before adding documentation.
- [docs/adr/](docs/adr/) before changing architecture or behavior contracts.
- [docs/reference/parity-matrix.md](docs/reference/parity-matrix.md) when
  moving a feature toward parity.

## Key architectural decisions (from the plan)

- **UI:** Leptos (full-stack Rust, SSR + islands hydration). Start server-rendered; add interactivity only where needed.
- **FFmpeg integration:** CLI spawn via `tokio::process::Command` - same model as ErsatzTV's `CliWrap` usage. **No libav bindings.** FFmpeg 8.1 is a hard runtime requirement (for the `drawvg` filter); detect on startup and fail fast on older versions.
- **Graphics engine:** use FFmpeg 8.1's `drawvg` filter with SVG templates in `assets/` as the replacement for ErsatzTV's `ScriptGraphicsElement` + external renderer pipe. This is the main "why Rust rewrite" win - do not reintroduce an external graphics process.
- **Database:** `sqlx` with SQLite first, MySQL behind a feature flag. **Do not port ErsatzTV's 393 EF migrations 1:1** - collapse them into one initial schema derived from the final state of [TvContext.cs](/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs) (99 DbSets) and evolve from there.
- **Web host:** `axum` + `tower` + `tokio`. Leptos mounts via `leptos_axum`.
- **CQRS:** no MediatR equivalent - plain `async fn` handlers in `nastv-app`, repository traits in `nastv-core`, sqlx impls in `nastv-infra`.
- **Scheduling:** three engines to port from [ErsatzTV.Core/Scheduling/](/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/) behind a shared trait - Block, YAML (`serde_yaml` + per-instruction dispatch), and Scripted (`mlua` for Lua 5.4).
- **Binaries:** `nastv-web` (server) and `nastv-scanner` (library scanner) - two binaries sharing the core crates.

## Porting workflow

This is primarily a porting project, not greenfield design. When implementing a feature:

1. **Find the ErsatzTV source first.** The plan's critical-file table maps features to the authoritative C# files under `/home/bart/ErsatzTV/`. Read the C# before writing Rust.
2. **Preserve patterns, not syntax.** ErsatzTV's FFmpeg pipeline builders, filter label routing (`ComplexFilter.cs`), and `ffconcat`/`resetpts` concat handling are version-agnostic and should be mirrored closely in structure. LanguageExt `Option`/`Either` map to native `Option`/`Result`; `anyhow`/`thiserror` for errors.
3. **Snapshot-test generated FFmpeg argv** the way ErsatzTV's `ErsatzTV.FFmpeg.Tests` does - use `insta`. This is the primary regression gate for the pipeline crate.
4. **Use the graphics engine as the divergence point.** Anywhere ErsatzTV pipes to an external graphics renderer, NasTv should emit a `drawvg` filter node instead.

## Reference paths (on this machine)

- ErsatzTV source tree: `/home/bart/ErsatzTV/`
- Implementation plan: `/home/bart/nastv/plans/sequential-dancing-valiant.md`
- HDHR protocol notes (internal to ErsatzTV repo): `/home/bart/ErsatzTV/hdhr.md`
