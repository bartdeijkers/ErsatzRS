# Parity Matrix

This is the living map from ErsatzTV behavior to NasTv implementation.

| ErsatzTV feature | ErsatzTV source | NasTv target | Status | ADR |
| --- | --- | --- | --- | --- |
| Domain model | `/home/bart/ErsatzTV/ErsatzTV.Core/Domain/` | `crates/nastv-domain` | Ported | |
| Initial schema | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs` | `migrations/` | Ported | [ADR 0004](../adr/0004-collapsed-initial-schema.md) |
| FFmpeg command generation | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/CommandGenerator.cs` | `crates/nastv-ffmpeg` | In progress | [ADR 0003](../adr/0003-ffmpeg-cli-spawn.md) |
| FFmpeg filter graph | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` | `crates/nastv-ffmpeg` | In progress | [ADR 0003](../adr/0003-ffmpeg-cli-spawn.md) |
| Drawvg graphics engine | ErsatzTV external graphics renderer flow | `crates/nastv-ffmpeg`, `assets/` | In progress | [ADR 0005](../adr/0005-drawvg-graphics-engine.md) |
| Block scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/BlockScheduling/` | `crates/nastv-core` | In progress | |
| YAML scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/YamlScheduling/` | `crates/nastv-core` | In progress: sequential dispatch, resolver-backed content sources, show-name/show-GUID resolution, detail-based history resume, structured playlist and marathon child history, show/season marathon grouping, `play_all_items` in-memory playlist behavior, cleanup/reset behavior, YAML imports (content/sequence merge with case-insensitive deduplication), expression-driven count (variables `count`/`random` and arithmetic), deterministic marathon shuffle-groups, and playlist batch/count child semantics are covered; SQL-backed resolver parity and remaining exact handler semantics remain | |
| Scripted scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/ScriptedScheduling/` | `crates/nastv-core` | In progress | |
| IPTV playlist | `/home/bart/ErsatzTV/ErsatzTV/Controllers/` | `crates/nastv-web` | Not started | |
| HDHomeRun endpoints | `/home/bart/ErsatzTV/hdhr.md` | `crates/nastv-web` | Not started | |
| Scanner | `/home/bart/ErsatzTV/ErsatzTV.Scanner/` | `crates/nastv-scanner` | Not started | |

Statuses:

- Not started
- In progress
- Ported
- Deferred
- Dropped
