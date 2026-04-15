# Parity Matrix

This is the living map from ErsatzTV behavior to ErsatzRS implementation.

| ErsatzTV feature | ErsatzTV source | ErsatzRS target | Status | ADR |
| --- | --- | --- | --- | --- |
| Domain model | `/home/bart/ErsatzTV/ErsatzTV.Core/Domain/` | `crates/ersatzrs-domain` | Ported | |
| Initial schema | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs` | `migrations/` | Ported | [ADR 0004](../adr/0004-collapsed-initial-schema.md) |
| FFmpeg command generation | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/CommandGenerator.cs` | `crates/ersatzrs-ffmpeg` | In progress | [ADR 0003](../adr/0003-ffmpeg-cli-spawn.md) |
| FFmpeg filter graph | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` | `crates/ersatzrs-ffmpeg` | In progress | [ADR 0003](../adr/0003-ffmpeg-cli-spawn.md) |
| Drawvg graphics engine | ErsatzTV external graphics renderer flow | `crates/ersatzrs-ffmpeg`, `assets/` | In progress | [ADR 0005](../adr/0005-drawvg-graphics-engine.md) |
| Block scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/BlockScheduling/` | `crates/ersatzrs-core` | In progress: core effective-template derivation structs, block keys, collection key/etag metadata, guide windows, scheduling context, change-detection scaffolding, deco block-start/default filler behavior, and focused snapshots are covered. Remaining gaps: SQL-backed template/deco/block item loading, alternate/deco-template selection, broader change-detection removal/update behavior, persisted collection-etag comparison, and remaining filler placements. | |
| YAML scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/YamlScheduling/` | `crates/ersatzrs-core`, `crates/ersatzrs-infra` | In progress: sequential dispatch, resolver-backed content sources, SQL-backed collection/multi-collection/smart collection/playlist/search/show/watermark/graphics lookups, SQL-backed fielded smart collection/direct search for represented metadata, show-name/show-GUID resolution, detail-based history resume, structured playlist and marathon child history, show/season marathon grouping, `play_all_items` playlist behavior, cleanup/reset behavior, YAML imports (content/sequence merge with case-insensitive deduplication), expression-driven count (variables `count`/`random` and arithmetic), deterministic marathon shuffle-groups, and playlist batch/count child semantics are covered; a real Lucene-equivalent index and remaining exact handler semantics remain | |
| Scripted scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/ScriptedScheduling/` | `crates/ersatzrs-core` | In progress | |
| Search index | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Search/` | `crates/ersatzrs-core`, `crates/ersatzrs-infra` | In progress: scheduling now has a core-owned Lucene-like parser/evaluator and SQLite search documents for represented fields (`title`, `sort_title`, `original_title`, `year`, `release_date`, `genre`, `tag`, `studio`, `actor`, `type`, `show_title`, `show_genre`, `show_tag`, `show_studio`, `season_number`, `episode_number`, `metadata_kind`, and provider GUID source fields). Show, season, and artist hits expand to playable children. Remaining gaps: no persistent inverted index, no analyzer/tokenization parity, no fuzzy/proximity/boost/page-map behavior, no language/subtitle/codec/dimension/chapter/trakt fields yet, and director/writer fields are placeholders until the normalized schema has metadata join tables for them. | |
| IPTV playlist | `/home/bart/ErsatzTV/ErsatzTV/Controllers/` | `crates/ersatzrs-web` | Not started | |
| HDHomeRun endpoints | `/home/bart/ErsatzTV/hdhr.md` | `crates/ersatzrs-web` | Not started | |
| Scanner | `/home/bart/ErsatzTV/ErsatzTV.Scanner/` | `crates/ersatzrs-scanner` | Not started | |

Statuses:

- Not started
- In progress
- Ported
- Deferred
- Dropped
