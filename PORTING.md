# Porting Contract

ErsatzRS ports ErsatzTV behavior from C# to Rust. Preserve behavior unless an ADR
records a deliberate deviation.

## Reading ErsatzTV source

Porting work needs the upstream ErsatzTV source tree on hand. The repository
refers to it through the **`$ERSATZTV_SRC`** environment variable so that no
maintainer's local filesystem layout leaks into committed files.

- Set `ERSATZTV_SRC` to a checkout of
  [ErsatzTV/ErsatzTV](https://github.com/ErsatzTV/legacy). A common layout is
  a sibling clone next to ErsatzRS (`../ErsatzTV/`); any path works.
- All references in this repo write paths as `$ERSATZTV_SRC/<relative path>`
  (for example `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/`). Resolve them with
  your shell, IDE, or by reading the same path under
  `https://github.com/ErsatzTV/legacy/tree/main/<relative path>`.
- Never commit absolute paths from a maintainer's machine. See
  [CONTRIBUTING.md](CONTRIBUTING.md) ground rules.

For Next integration under ADR 0095, `ERSATZTV_NEXT_SRC` identifies a checkout
of [ErsatzTV/next](https://github.com/ErsatzTV/next) for source review. It is
separate from the runtime override `ETV_NEXT_FOLDER`, which identifies the
directory containing the built `ersatztv-channel` executable. A source checkout
alone is not an installed runtime. Use `$ERSATZTV_NEXT_SRC/<relative path>` in
source references and record the reviewed revision in milestone evidence.

## Procedure

1. Find and read the relevant ErsatzTV source under `$ERSATZTV_SRC/`.
2. Create a source map for the slice:
   - UI/pages and shared components.
   - View models and validators.
   - Handlers/services.
   - Repository/query paths.
   - Side effects such as playout refresh, generated playlists, media actions,
     provider refresh, health checks, or troubleshooting visibility.
   - Intentional deviations, with ADR/deviation links.
3. Identify the ErsatzRS target crate from [ARCHITECTURE.md](ARCHITECTURE.md).
4. Map C# concepts to Rust idioms using the table below.
5. Define or reuse a trait in `ersatzrs-core` when behavior crosses infrastructure
   boundaries.
6. Implement concrete storage, provider, or process behavior in `ersatzrs-infra`,
   `ersatzrs-ffmpeg`, `ersatzrs-web`, or `ersatzrs-scanner` as appropriate.
7. Add tests at the narrowest useful level.
8. Add `insta` snapshots for generated FFmpeg argv, filter graphs, YAML
   scheduling output, and generated SVG/drawvg fragments.
9. Update the owning structured parity record when its status or debt changes.
10. Add an ADR if behavior deliberately diverges from ErsatzTV.

## No-Stopgap Parity Gate

Parity is not complete until the implementation proves these items:

- Visible UI fields, actions, labels, routes, and back-links match ErsatzTV or
  an ADR records the deliberate deviation.
- Validation behavior and error surfaces match the ErsatzTV validator/handler
  layer, not only the Rust form type.
- Persistence behavior and delete/copy/edit/add side effects match, including
  playout refresh, generated playlists, provider updates, media actions, and
  health/troubleshooting visibility where applicable.
- Critical save/action flows have an SSR fallback or browser-safe fallback when
  hydration is not guaranteed.
- Detail/list route behavior and SSR fallback links work without relying on a
  hidden hydrated-only path.
- Structured parity, route and hydration records reflect the actual state.
  Keep milestone acceptance and detailed current evidence in the owning record;
  other documents link or summarize briefly and change only when their own
  facts change. Follow `skills/update-parity.md`.

Record incomplete behavior as visible debt in the owning
`docs/progress/parity/<slug>.toml`, following `skills/update-parity.md`;
link to it from plans instead of maintaining a second debt report. A feature
with required behavior still outstanding must stay below `Ported`. This does
not prevent an authorized engineering checkpoint; see
[CONTRIBUTING.md](CONTRIBUTING.md#delivery-claims).

## Parity verification

Use focused checks during implementation, guided by `skills/rust-inner-loop.md`.
Run `cargo xtask parity-check web` before claiming admin UI parity complete,
and repeat it when relevant changes invalidate its evidence. Preserve all
agreed acceptance requirements.

The gate includes progress, formatting, architecture and hydration checks, web
tests and web Clippy. Scope labels such as `web media` do not narrow the
executed checks. Treat `affected-checks` output as recommendations, not a
requirement to repeat the gate during every iteration.

## C# to Rust Mapping

| ErsatzTV / C# | ErsatzRS / Rust |
| --- | --- |
| `Option<T>` from LanguageExt | `Option<T>` |
| `Either<L, R>` | `Result<R, L>` |
| `Validation<Error, T>` | `Result<T, Vec<Error>>` or typed `thiserror` variants |
| `Task<T>` | `async fn -> T` |
| `CancellationToken` | explicit cancellation handle or Tokio-aware shutdown signal |
| MediatR request handler | plain async use-case function in `ersatzrs-app` |
| Dependency injection container | constructor-injected traits and concrete composition at the edge |
| EF `DbSet<T>` | sqlx queries plus repository trait |
| EF migration history | one collapsed initial sqlx schema, then normal migrations |
| `IEnumerable<T>` pipeline | iterators or explicit vectors depending on ownership |
| C# records | structs with derived traits where correct |
| C# discriminated concepts | enums with data variants |
| `CliWrap` FFmpeg invocation | `tokio::process::Command` |
| external graphics renderer pipe | FFmpeg 8.1 `drawvg` filter node |

## Critical ErsatzTV Sources

| Area | ErsatzTV source |
| --- | --- |
| FFmpeg pipeline builders | `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/` |
| Filter graph generation | `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` |
| FFmpeg argv generation | `$ERSATZTV_SRC/ErsatzTV.FFmpeg/CommandGenerator.cs` |
| FFmpeg process spawning | `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs` |
| Seamless concat | `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/ConcatPlaylist.cs` |
| Final database model | `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/TvContext.cs` |
| Scheduling engines | `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/` |
| HDHomeRun notes | `$ERSATZTV_SRC/hdhr.md` |

## Deviation Rule

If the Rust implementation intentionally changes ErsatzTV behavior, create or
update an ADR and link it from [docs/explanation/deviations.md](docs/explanation/deviations.md).
