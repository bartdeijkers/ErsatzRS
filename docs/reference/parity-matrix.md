# Parity Matrix

This is the living map from ErsatzTV behavior to NasTv implementation.

| ErsatzTV feature | ErsatzTV source | NasTv target | Status | ADR |
| --- | --- | --- | --- | --- |
| Domain model | `/home/bart/ErsatzTV/ErsatzTV.Core/Domain/` | `crates/nastv-domain` | In progress | |
| Initial schema | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs` | `migrations/` | In progress | [ADR 0004](../adr/0004-collapsed-initial-schema.md) |
| FFmpeg command generation | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/CommandGenerator.cs` | `crates/nastv-ffmpeg` | Not started | [ADR 0003](../adr/0003-ffmpeg-cli-spawn.md) |
| FFmpeg filter graph | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` | `crates/nastv-ffmpeg` | Not started | [ADR 0003](../adr/0003-ffmpeg-cli-spawn.md) |
| Drawvg graphics engine | ErsatzTV external graphics renderer flow | `crates/nastv-ffmpeg`, `assets/` | Not started | [ADR 0005](../adr/0005-drawvg-graphics-engine.md) |
| Block scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/BlockScheduling/` | `crates/nastv-core` | Not started | |
| YAML scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/YamlScheduling/` | `crates/nastv-core` | Not started | |
| Scripted scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/ScriptedScheduling/` | `crates/nastv-core` | Not started | |
| IPTV playlist | `/home/bart/ErsatzTV/ErsatzTV/Controllers/` | `crates/nastv-web` | Not started | |
| HDHomeRun endpoints | `/home/bart/ErsatzTV/hdhr.md` | `crates/nastv-web` | Not started | |
| Scanner | `/home/bart/ErsatzTV/ErsatzTV.Scanner/` | `crates/nastv-scanner` | Not started | |

Statuses:

- Not started
- In progress
- Ported
- Deferred
- Dropped
