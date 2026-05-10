# Porting Contract

ErsatzRS ports ErsatzTV behavior from C# to Rust. Preserve behavior unless an ADR
records a deliberate deviation.

## Reading ErsatzTV source

Porting work needs the upstream ErsatzTV source tree on hand. The repository
refers to it through the **`$ERSATZTV_SRC`** environment variable so that no
maintainer's local filesystem layout leaks into committed files.

- Set `ERSATZTV_SRC` to a checkout of
  [ErsatzTV/ErsatzTV](https://github.com/ErsatzTV/ErsatzTV). A common layout is
  a sibling clone next to ErsatzRS (`../ErsatzTV/`); any path works.
- All references in this repo write paths as `$ERSATZTV_SRC/<relative path>`
  (for example `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/`). Resolve them with
  your shell, IDE, or by reading the same path under
  `https://github.com/ErsatzTV/ErsatzTV/tree/main/<relative path>`.
- Never commit absolute paths from a maintainer's machine. See
  [CONTRIBUTING.md](CONTRIBUTING.md) ground rules.

## Procedure

1. Find and read the relevant ErsatzTV source under `$ERSATZTV_SRC/`.
2. Identify the ErsatzRS target crate from [ARCHITECTURE.md](ARCHITECTURE.md).
3. Map C# concepts to Rust idioms using the table below.
4. Define or reuse a trait in `ersatzrs-core` when behavior crosses infrastructure
   boundaries.
5. Implement concrete storage, provider, or process behavior in `ersatzrs-infra`,
   `ersatzrs-ffmpeg`, `ersatzrs-web`, or `ersatzrs-scanner` as appropriate.
6. Add tests at the narrowest useful level.
7. Add `insta` snapshots for generated FFmpeg argv, filter graphs, YAML
   scheduling output, and generated SVG/drawvg fragments.
8. Update the parity matrix.
9. Add an ADR if behavior deliberately diverges from ErsatzTV.

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
