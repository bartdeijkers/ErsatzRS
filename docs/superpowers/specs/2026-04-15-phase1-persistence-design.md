# Phase 1 — Domain and Persistence Design

Date: 2026-04-15
Status: Approved

## Context

Phase 0 is complete. `nastv-domain` is fully populated with all domain entity
modules (channel, media, scheduling, playout, library, collection, config,
ffmpeg, filler, graphics, metadata, media_source, common). `nastv-core` and
`nastv-infra` are empty placeholders. No migrations exist yet.

This spec covers the three remaining pieces of Phase 1:

1. Initial sqlx schema (migrations)
2. Repository traits in `nastv-core`
3. sqlx implementations in `nastv-infra`
4. Entity-mapping tests (round-trip + query-method verification)

## Scope decisions

- **Breadth:** All repository traits are defined up front, covering every
  aggregate in `nastv-domain`. Nothing is deferred to a later phase.
- **Trait style:** Query-driven methods — each method matches a real use case
  rather than generic CRUD. No dead methods. Mirrors ErsatzTV's actual
  repository query patterns.
- **Trait granularity:** One trait per aggregate root, mirroring the
  `nastv-domain` module structure.
- **Test depth:** Round-trip tests plus query-method behavioral verification.
  No `insta` snapshots (those are reserved for Phase 2 FFmpeg argv tests).

## Section 1: Schema and Migrations

### Location

```
migrations/0001_initial.sql
```

One file collapses the full TvContext.cs schema (99 DbSets) into a single
SQLite initial state. No EF migration history is ported.

### SQLite conventions

| Concern | Convention |
|---|---|
| Enums | `TEXT` columns with `CHECK` constraints listing valid variants |
| `chrono::Duration` | `INTEGER` milliseconds |
| `chrono::DateTime<Utc>` | `TEXT` ISO 8601 |
| Booleans | `INTEGER` 0/1 |
| Floats/percentages | `REAL` |
| Binary blobs (watermark images) | `BLOB` |
| Foreign keys | Enabled per-connection via `PRAGMA foreign_keys = ON` in pool setup, not in the migration |

### Organization

Tables are defined in dependency order within the single file:

1. Common lookup tables: `language_codes`
2. Configuration: `config_entries`
3. FFmpeg profiles: `ffmpeg_profiles`
4. Watermarks: `channel_watermarks`
5. Graphics: `graphics_elements`, `script_graphics_elements`
6. Filler: `filler_presets`
7. Channels: `channels`, `channel_artwork`
8. Media sources: `media_sources`, `plex_media_sources`, `jellyfin_media_sources`, `emby_media_sources`, `local_media_sources`
9. Libraries: `libraries`, `library_paths`, `library_folders`, `plex_libraries`, `jellyfin_libraries`, `emby_libraries`, `local_libraries`
10. Media: `media_items`, `media_versions`, `media_files`, `media_streams`, `media_chapters`, `movies`, `shows`, `seasons`, `episodes`, `music_videos`, `other_videos`, `songs`, `artists`, `images`, `remote_streams`, provider-specific variants (`plex_movies`, `jellyfin_episodes`, etc.)
11. Metadata: `movie_metadata`, `show_metadata`, `season_metadata`, `episode_metadata`, `music_video_metadata`, `artist_metadata`, `song_metadata`, `image_metadata`, `other_video_metadata`
12. Collections: `collections`, `collection_items`, `multi_collections`, `multi_collection_items`, `smart_collections`, `rerun_collections`, `playlists`, `playlist_groups`, `playlist_items`, `trakt_lists`, `trakt_list_items`, `trakt_list_item_guids`, provider collection variants
13. Scheduling: `program_schedules`, `program_schedule_items`, `program_schedule_alternates`, `block_groups`, `blocks`, `block_items`, `deco_groups`, `decos`, `deco_break_contents`, `template_groups`, `templates`, `template_items`, `deco_template_groups`, `deco_templates`, `deco_template_items`
14. Playout: `playouts`, `playout_items`, `playout_gaps`, `playout_anchors`, `playout_program_schedule_anchors`, `playout_schedule_item_fill_group_indices`, `playout_templates`, `playout_history`, `playout_build_statuses`, `rerun_history`
15. Watermark join tables: `playout_item_watermarks`, `program_schedule_item_watermarks`, `block_item_watermarks`, `deco_watermarks`
16. Graphics join tables: `program_schedule_item_graphics_elements`, `block_item_graphics_elements`, `playout_item_graphics_elements`, `deco_graphics_elements`

### Vec<EntityId> mapping

Domain structs carry `Vec<EntityId>` navigation fields (e.g., `playout_ids`
on `Channel`). These map to child or join tables in the schema. They are
never serialized into a single column.

## Section 2: Repository Traits (`nastv-core`)

### File layout

```
nastv-core/src/repositories/
  mod.rs         — re-exports all traits and error type
  error.rs       — RepositoryError
  channel.rs
  playout.rs
  program_schedule.rs
  block.rs
  deco.rs
  template.rs
  media_item.rs
  library.rs
  media_source.rs
  collection.rs
  ffmpeg_profile.rs
  filler.rs
  graphics.rs
  metadata.rs
  config.rs
```

### Error type

```rust
// nastv-core/src/repositories/error.rs
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("{kind} with id {id} not found")]
    NotFound { kind: &'static str, id: i64 },
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub type Result<T> = std::result::Result<T, RepositoryError>;
```

### Dependency note

`nastv-core` adds `sqlx` and `async-trait` as dependencies. `sqlx` is needed
solely for the `Database` variant in `RepositoryError`. This is a deliberate
pragmatic coupling — the alternative (wrapping sqlx errors into a generic
`Box<dyn Error>`) adds ceremony without benefit in a single-binary project.

### Async trait pattern

All trait methods are `async fn`. The `async-trait` crate is used on trait
definitions to make them object-safe (`Arc<dyn XRepository + Send + Sync>`).
Implementations use `#[async_trait::async_trait]` on the `impl` block.

### Trait summary

| Trait | Key methods |
|---|---|
| `ChannelRepository` | `get_with_profile(id)`, `get_by_number(n)`, `list_enabled()`, `upsert(channel)`, `delete(id)` |
| `PlayoutRepository` | `get_with_anchor(id)`, `items_in_range(playout_id, start, finish)`, `upsert_item(item)`, `delete_items_before(playout_id, cutoff)`, `upsert_build_status(status)` |
| `ProgramScheduleRepository` | `get_with_items(id)`, `list_for_playout(playout_id)`, `upsert(schedule)` |
| `BlockRepository` | `get_with_items(id)`, `list_for_group(group_id)`, `upsert(block)` |
| `DecoRepository` | `get_with_watermarks(id)`, `list_for_group(group_id)`, `upsert(deco)` |
| `TemplateRepository` | `get_with_items(id)`, `list_for_group(group_id)`, `upsert(template)` |
| `MediaItemRepository` | `get_with_version(id)`, `list_for_library_path(path_id)`, `find_by_path(path)`, `upsert(item)` |
| `LibraryRepository` | `get_with_paths(id)`, `list_for_source(source_id)`, `upsert(library)` |
| `MediaSourceRepository` | `get_plex(id)`, `get_jellyfin(id)`, `get_emby(id)`, `list_all()`, `upsert_plex(source)`, `upsert_jellyfin(source)`, `upsert_emby(source)` |
| `CollectionRepository` | `get_with_items(id)`, `list_for_media_item(item_id)`, `get_smart(id)`, `get_playlist_with_items(id)`, `upsert(collection)` |
| `FfmpegProfileRepository` | `get(id)`, `list()`, `upsert(profile)`, `delete(id)` |
| `FillerRepository` | `get(id)`, `list()`, `upsert(preset)`, `delete(id)` |
| `GraphicsRepository` | `get_element(id)`, `list_for_channel(channel_id)`, `upsert_element(element)` |
| `MetadataRepository` | `get_movie_metadata(item_id)`, `get_episode_metadata(item_id)`, `get_music_video_metadata(item_id)`, `upsert_movie_metadata(m)`, `upsert_episode_metadata(m)` |
| `ConfigRepository` | `get(key)`, `set(key, value)` |

### Loading strategy for Vec<EntityId>

- Methods named `get(id)` or `list_*()` return bare aggregates — `Vec<EntityId>`
  fields are **not** populated (left empty).
- Methods named `get_with_*(id)` populate the relevant `Vec<EntityId>` fields
  via a secondary `SELECT … WHERE parent_id = ?` immediately after the primary
  row fetch.
- List methods (`list_*`) never populate child Vecs — callers that need
  relations call `get_with_*` per item.
- `upsert` uses `INSERT OR REPLACE` in SQLite.

## Section 3: sqlx Implementation (`nastv-infra`)

### File layout

```
nastv-infra/src/repositories/
  mod.rs         — Repositories bundle struct + make_repositories()
  channel.rs     — SqliteChannelRepository(Pool<Sqlite>)
  playout.rs
  program_schedule.rs
  block.rs
  deco.rs
  template.rs
  media_item.rs
  library.rs
  media_source.rs
  collection.rs
  ffmpeg_profile.rs
  filler.rs
  graphics.rs
  metadata.rs
  config.rs
  enum_types.rs  — sqlx::Type impls for all domain enums
```

### Repository newtype pattern

```rust
pub struct SqliteChannelRepository(pub Pool<Sqlite>);

#[async_trait::async_trait]
impl ChannelRepository for SqliteChannelRepository {
    async fn get_with_profile(&self, id: i64) -> Result<Channel> { … }
}
```

### Query style

`sqlx::query!` macro with compile-time SQL checking. Offline mode via
`cargo sqlx prepare` produces `sqlx-data.json` for CI.

### Enum mapping

Domain enums implement `sqlx::Type<Sqlite>`, `sqlx::Encode<Sqlite>`, and
`sqlx::Decode<Sqlite>` via a TEXT representation. A local macro reduces this
to one line per enum. Variants map to their `Debug` string representation.

### Type mappings

| Rust type | SQLite | Notes |
|---|---|---|
| `chrono::Duration` | `INTEGER` | milliseconds via newtype wrapper |
| `chrono::DateTime<Utc>` | `TEXT` | ISO 8601, sqlx chrono feature |
| `bool` | `INTEGER` | 0/1 |
| `Vec<u8>` | `BLOB` | watermark image bytes |
| Domain enum | `TEXT` | CHECK-constrained, TryFrom<&str> |

### Bundle struct

```rust
pub struct Repositories {
    pub channel: SqliteChannelRepository,
    pub playout: SqlitePlayoutRepository,
    // … all fifteen
}

pub fn make_repositories(pool: Pool<Sqlite>) -> Repositories { … }
```

Called once at startup in `nastv-web/main.rs`, passed into axum state.

## Section 4: Testing

### Location

```
nastv-infra/tests/
  channel_repository.rs
  playout_repository.rs
  program_schedule_repository.rs
  block_repository.rs
  deco_repository.rs
  template_repository.rs
  media_item_repository.rs
  library_repository.rs
  media_source_repository.rs
  collection_repository.rs
  ffmpeg_profile_repository.rs
  filler_repository.rs
  graphics_repository.rs
  metadata_repository.rs
  config_repository.rs
  common.rs      — test_db() helper
```

### Test database setup

```rust
// nastv-infra/tests/common.rs
pub async fn test_db() -> Pool<Sqlite> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    pool
}
```

Each test is `#[tokio::test]`. In-memory databases are isolated per
connection; tests run in parallel without interference.

### Per-test structure

Each test file contains:

1. **Round-trip test** — build a minimal valid domain struct, call `upsert`,
   call `get_with_*`, assert the returned value equals the original via
   `assert_eq!` (all domain structs derive `PartialEq`).

2. **Query-method tests** — one test per non-trivial method verifying
   filtering behavior. Examples:
   - `items_in_range` returns only items overlapping the given window.
   - `list_enabled` excludes channels with `is_enabled = false`.
   - `find_by_path` returns `NotFound` for an unknown path.
   - `get_with_items` populates child ID Vecs; bare `get` does not.

### What is not tested in Phase 1

- Cross-repository queries (Phase 3 scheduling tests).
- Migration idempotency (covered by `cargo sqlx migrate run` in CI).
- Error path exhaustion (added incrementally as use cases need them).

## Phase 1 Verification Criteria (from plan)

- `cargo test -p nastv-domain -p nastv-infra` passes.
- Migrations run idempotently on an empty database.
- Domain entities have sqlx round-trip tests where persistence exists.
