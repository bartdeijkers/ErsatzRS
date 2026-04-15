# Porting Contract

NasTv ports ErsatzTV behavior from C# to Rust. Preserve behavior unless an ADR
records a deliberate deviation.

## Procedure

1. Find and read the relevant ErsatzTV source under `/home/bart/ErsatzTV/`.
2. Identify the NasTv target crate from [ARCHITECTURE.md](ARCHITECTURE.md).
3. Map C# concepts to Rust idioms using the table below.
4. Define or reuse a trait in `nastv-core` when behavior crosses infrastructure
   boundaries.
5. Implement concrete storage, provider, or process behavior in `nastv-infra`,
   `nastv-ffmpeg`, `nastv-web`, or `nastv-scanner` as appropriate.
6. Add tests at the narrowest useful level.
7. Add `insta` snapshots for generated FFmpeg argv, filter graphs, YAML
   scheduling output, and generated SVG/drawvg fragments.
8. Update the parity matrix.
9. Add an ADR if behavior deliberately diverges from ErsatzTV.

## C# to Rust Mapping

| ErsatzTV / C# | NasTv / Rust |
| --- | --- |
| `Option<T>` from LanguageExt | `Option<T>` |
| `Either<L, R>` | `Result<R, L>` |
| `Validation<Error, T>` | `Result<T, Vec<Error>>` or typed `thiserror` variants |
| `Task<T>` | `async fn -> T` |
| `CancellationToken` | explicit cancellation handle or Tokio-aware shutdown signal |
| MediatR request handler | plain async use-case function in `nastv-app` |
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
| FFmpeg pipeline builders | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Pipeline/` |
| Filter graph generation | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` |
| FFmpeg argv generation | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/CommandGenerator.cs` |
| FFmpeg process spawning | `/home/bart/ErsatzTV/ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs` |
| Seamless concat | `/home/bart/ErsatzTV/ErsatzTV.Core/FFmpeg/ConcatPlaylist.cs` |
| Final database model | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs` |
| Scheduling engines | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/` |
| HDHomeRun notes | `/home/bart/ErsatzTV/hdhr.md` |

## Deviation Rule

If the Rust implementation intentionally changes ErsatzTV behavior, create or
update an ADR and link it from [docs/explanation/deviations.md](docs/explanation/deviations.md).
