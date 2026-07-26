# Module-split survey

> **Type:** Structural refactoring contract.
> **Status:** P7C-04 execution baseline, refreshed 2026-07-26.
> **Milestone:** `p7c-04` in `docs/progress/milestones.toml`.

## Purpose

Split modules only when an enumerable growth axis can sit behind thin dispatch
and an exhaustive completeness gate. File size is evidence of pressure, not a
selection rule by itself.

The reference pattern remains admin localization:

- `admin/i18n.rs` owns `UiLanguage`, `TextKey`, registry metadata, and dispatch;
- `admin/languages/*.rs` owns per-language values;
- `all_languages_translate_each_key` proves every language covers every key.

A split is warranted when all of these are true:

1. variation follows an enumerable dimension;
2. the dimension grows as parity expands;
3. each variant has a similar responsibility;
4. a thin dispatcher or registry is natural; and
5. compiler exhaustiveness or a registry test can prove completeness.

Large but cohesive state machines remain intact when they do not meet this
rule.

## Current architecture and workflow

P7B-05 is complete. Production admin server functions no longer own direct SQL,
raw pool access, silent resource defaults, or row-decode defaults:

```text
Leptos/Axum transport
        |
        v
ersatzrs-app commands and queries
        |
        v
ersatzrs-core storage-neutral ports
        |
        v
ersatzrs-infra adapters
```

The remaining large-module work is therefore structural. It must not move
orchestration or SQL back into `ersatzrs-web`.

Use the current repository runner and preserve one compile configuration during
focused iteration:

```bash
./run.sh check -p <crate>
./run.sh test -p <crate> [filter]
```

Use `./run.sh test-closeout` for workspace tests plus doctests. At closeout,
run both Clippy configurations, a matched `cargo leptos build`, web parity,
architecture, hydration, and progress gates exactly as documented in
`AGENTS.md`, `BUILD.md`, and `TEST.md`. Do not use direct `cargo test`.

The configured target directory measured 9.6 GiB on 2026-07-26, below the
40 GiB `cargo clean` threshold.

## Measurements

Measured from `main` at `ae0f8c0` on 2026-07-26:

| Target | Lines | Enumerable surface |
| --- | ---: | --- |
| `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs` | 9,020 | 71 `#[server]` functions; 128 public/public-crate items |
| `crates/ersatzrs-core/src/scheduling/yaml.rs` | 3,367 | 38 serialized instruction fields; 19 dispatch choices |
| `crates/ersatzrs-core/src/scheduling/scripted.rs` | 2,451 | 37 registered Lua globals |
| `crates/ersatzrs-infra/src/jellyfin.rs` | 3,475 | Jellyfin HTTP projection and sync adapter |
| `crates/ersatzrs-infra/src/emby.rs` | 3,376 | Emby HTTP projection and sync adapter |
| `crates/ersatzrs-infra/src/plex.rs` | 2,953 | Plex adapter; deliberately out of the dedup slice |

The health-check pilot is complete and must not be repeated.
`crates/ersatzrs-infra/src/health/checks/` contains 21 concrete checks behind
the registry in `health/mod.rs`.

## Source map: media-browser transport

### Responsibilities and stable surfaces

The module owns shared browser/detail DTOs, transport validation and mapping,
71 Leptos server-function declarations, cross-kind collection/playlist
dispatch, and SSR-only adapters into application commands and queries.
Test-only legacy SQL helpers remain solely as regression oracles.

The 71 server-function names, grouped by their natural seams, are:

- browser/detail kinds:
  `list_movies`, `get_movie_details`, `get_show_details`,
  `get_season_details`, `get_media_item_info_json`, `list_shows`,
  `list_images`, `list_artists`, `get_artist_details`,
  `list_other_videos`, `get_other_video_details`, `list_seasons`,
  `list_episodes`, `list_music_videos`, `remote_stream_page_map`,
  `list_remote_streams`, and `list_songs`;
- trash:
  `list_trash_items`, `delete_media_items`, and `empty_trash`;
- manual collections:
  `list_collections`, `get_collection`, `save_collection`, and
  `delete_collection`;
- smart collections:
  `list_smart_collections`, `get_smart_collection`,
  `save_smart_collection`, and `delete_smart_collection`;
- multi-collections:
  `get_multi_collections_page_size`, `list_multi_collections`,
  `get_multi_collection`, `get_multi_collection_editor_options`,
  `save_multi_collection`, and `delete_multi_collection`;
- rerun collections:
  `get_rerun_collections_page_size`, `list_rerun_collections`,
  `get_rerun_collection_form`, `get_rerun_collection_editor_options`,
  `save_rerun_collection`, and `delete_rerun_collection`;
- collection autocomplete, membership, and scheduling:
  `suggest_media_items`, `add_item_to_collection`,
  `remove_item_from_collection`, `list_program_schedules`,
  `add_show_to_schedule`, `add_media_item_to_schedule`,
  `list_all_collections`, `list_smart_collections_for_select`, and
  `create_collection`;
- playlists:
  `get_playlist_page`, `create_playlist_group`, `create_playlist`,
  `delete_playlist_group`, `delete_playlist`, `get_playlist_editor`,
  `get_playlist_editor_options`, `save_playlist`,
  `preview_playlist_playout`, `list_all_playlist_groups`, and
  `list_playlists_by_group`;
- cross-kind browser actions:
  `media_page_map`, `add_media_items_to_collection`,
  `add_media_items_to_collection_with_playback`,
  `add_media_items_to_playlist`,
  `add_media_items_to_playlist_with_playback`, and
  `add_media_items_to_playlist_mixed`;
- collection detail:
  `get_collection_detail`, `set_collection_use_custom_order`,
  `remove_items_from_collection`, `update_collection_custom_order`, and
  `list_collection_members`.

Every function currently uses the default `PostUrl` input and JSON output
codec. Its route is:

```text
/api/<function-name><xxh64("ersatzrs:ersatzrs_web::admin::server_fns::media_browser")>
```

This corrects the old survey assumption that only a function name determines
the endpoint. Leptos also hashes `module_path!()`. The macro-decorated
declarations must therefore remain in `media_browser/mod.rs`, or an explicit
endpoint must preserve the old value. P7C-04 keeps them in `mod.rs` as thin
wrappers and adds a path-stability test for all 71 routes.

The other stable public surface is the set of DTOs, helpers, and
`MediaBrowserKind`. Existing callers continue to import them through
`admin::server_fns::media_browser`; deliberate re-exports preserve that path.

### Native/WASM boundary

DTOs and `#[server]` declarations compile for SSR and hydration. Application,
core, infrastructure, SQL, path replacement, and native runtime helpers remain
under `feature = "ssr"` or `all(test, feature = "ssr")`. No native adapter may
become a WASM dependency.

### Target layout

```text
admin/server_fns/media_browser/
  mod.rs                  public DTO/re-export surface, 71 thin #[server]
                          declarations, MediaBrowserKind dispatch
  shared.rs               shared projections and transport helpers
  kinds/
    movies.rs
    shows.rs
    seasons.rs
    episodes.rs
    artists.rs
    music_videos.rs
    other_videos.rs
    songs.rs
    images.rs
    remote_streams.rs
  collections/
    manual.rs
    smart.rs
    multi.rs
    rerun.rs
    detail.rs
  playlists.rs
  trash.rs
  autocomplete.rs
  test_support.rs
```

The exact number of files may consolidate tiny handlers, but the named feature
seams, stable root import path, and thin root dispatch are mandatory.

### Visual source map

No rendered markup or CSS moves in this milestone.

| Routes/pages | Server-function concerns | Shared UI primitives |
| --- | --- | --- |
| `/media/movies`, `/media/tv/shows`, `/media/tv/seasons`, `/media/tv/episodes`, `/media/music/artists`, `/media/music/videos`, `/media/other/videos`, `/media/music/songs`, `/media/browser/images`, `/media/remote/streams` and detail aliases | per-kind list/detail/page-map and cross-kind actions | `multi_select_browser`, browser cards, letter bar, add-to-schedule modal |
| `/media/collections`, `/media/smart-collections`, `/media/multi-collections`, `/media/rerun-collections` and editor/detail routes | collection CRUD, autocomplete, membership, ordering | collection editors, save-as dialog, modal/delete primitives |
| `/media/playlists` and `/:id` | playlist group/playlist CRUD, editor options, preview | playlist tree/editor and shared collection selectors |
| `/media/trash` | list/delete/empty | trash cards and dialogs |
| `/search` | collection/playlist selection and mixed inserts | search result actions and save-as dialog |

Relevant durable browser coverage is in
`media-browser-actions.spec.ts`, `media-collection-route-family.spec.ts`,
`collections.spec.ts`, `content-variants.spec.ts`, `remote-streams.spec.ts`,
`search-see-all.spec.ts`, and `island-hydration.spec.ts`.

The completeness gate enumerates every `MediaBrowserKind`, its route/list
metadata, collection type, and playlist type. Compiler-exhaustive matches must
have no wildcard arm.

## Source map: YAML scheduling

`YamlPlayoutDefinition`, `YamlContentItem`, `YamlPlayoutSequenceItem`,
`YamlInstruction`, and `YamlPlayoutEngine` are the stable public surface
re-exported by `scheduling/mod.rs`.

`YamlInstruction` remains the 38-field serde struct. Its field names, defaults,
omission behavior, and ordering are the current serialized contract. An
internal kind is derived only after deserialization.

Malformed multi-field input keeps this exact first-match precedence:

```text
wait_until -> rewind -> epg_group -> watermark -> graphics_on -> graphics_off
-> pre_roll -> post_roll -> mid_roll -> repeat(true) -> sequence
-> shuffle_sequence -> skip_items -> skip_to_item -> all -> count -> duration
-> pad_to_next -> pad_until -> unsupported
```

The target is `scheduling/yaml/mod.rs` for the public model, engine, context,
kind derivation, and exhaustive dispatch, with coherent handlers under
`scheduling/yaml/instructions/`. Snapshot names and generated timelines must
not change.

## Source map: scripted scheduling

`ScriptedSchedulingEngine`, its parsing helpers, shared `ScriptState`, and a
thin `register_all` remain in `scripted/mod.rs`.

The 37 stable Lua globals are:

```text
set_smart_shuffle_policy, set_cooldown_policy,
schedule_count, schedule_duration,
set_watermark, clear_watermarks, graphics_on, graphics_off,
wait_until, add_count, add_all, add_duration, pad_to_next, pad_until,
skip, skip_to, rewind, pre_roll_on, pre_roll_off,
lock_guide_group, unlock_guide_group,
is_done, current_time, finish_time,
watermark_on, watermark_off, graphics_on_path, graphics_off_path,
add_collection, add_multi_collection, add_smart_collection, add_search,
add_show, add_playlist, add_key, add_marathon, create_playlist
```

Category registrars under `scripted/api/` receive the same
`Rc<RefCell<ScriptState>>`. A single expected-name registry and a Lua-global
completeness test prove that `register_all` exposes exactly this set.

## Source map: Jellyfin and Emby

Jellyfin and Emby public service structs, functions, core provider-specific
types, and core service traits remain stable. Plex is not changed.

The normalized implementations share pagination loops, progress calculation,
ETAG restoration/skip behavior, transactions, metadata/media-version/file/
stream/chapter persistence, content-variant handling, path replacement,
collection membership, cleanup, and scan outcomes.

Real divergences that must remain strategy-owned are:

| Concern | Jellyfin | Emby |
| --- | --- | --- |
| Authentication | `Authorization: MediaBrowser Token="..."`; accepts already formatted headers | `X-Emby-Token: <api_key>` |
| General item fields | `Tags` string array; narrower movie/show/episode field lists; movie query adds `filters=IsNotFolder` | `TagItems[].Name`; requests production year, premiere date, media sources, and location type where needed |
| Seasons | `/Items` with parent id, `Season`, recursive | `/Shows/{show_id}/Seasons` |
| Episodes | `/Items` with season parent id, `Episode`, recursive | `/Shows/{show_id}/Episodes?seasonId={season_id}` |
| Episode inclusion | only `LocationType=FileSystem` | excludes `LocationType=Virtual` |
| Connection persistence | Jellyfin auth-header keys plus API-key fallbacks | Emby API-key keys |
| Provider identity | Jellyfin tables, config keys, GUID/artwork prefixes, log/error labels | Emby tables, config keys, GUID/artwork prefixes, log/error labels |
| Connection validation | public Jellyfin header formatter and `/System/Info` helper | no equivalent public helper in the current adapter |

The shared boundary may own only behavior identical in both columns. Request
construction and response projection remain provider strategies. An exhaustive
remote-provider dispatch covers Plex, Jellyfin, and Emby; a companion
`MediaSourceKind` gate covers Local plus all three remote kinds so a new source
cannot silently fall through.

## Baseline evidence

Before P7C-04 edits on 2026-07-26:

- `./run.sh test -p ersatzrs-web`: 399 passed, 6 skipped;
- `./run.sh test -p ersatzrs-core`: 148 passed;
- `./run.sh test -p ersatzrs-infra --test jellyfin_service`: 9 passed;
- `./run.sh test -p ersatzrs-infra --test emby_service`: 8 passed;
- no `.snap.new` files and a clean worktree.

The first sandboxed Jellyfin integration run could not bind
`127.0.0.1:0`; the unchanged command passed with local loopback permission.

## Considered and skipped

- `server_fns/schedules.rs`, `decos.rs`, `blocks.rs`, `media_sources.rs`, and
  `settings.rs` are large but cohesive feature modules.
- `server_fns/search.rs` has a possible projection split but lower value.
- `repositories/scheduling.rs` is tightly coupled SQL hydration.
- `scheduling/block.rs` and `scheduling/classic/mod.rs` are state machines,
  not enumerable registries.
- `plex.rs` is a distinct provider architecture and remains untouched.
- `plans/web-server-wasm-crate-split.md` remains proposed; P7C-04 proves
  compatibility but does not execute it.
