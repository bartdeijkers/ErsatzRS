# Module-split survey

> **Type:** Structural refactoring contract.
> **Status:** P7C-04 complete, refreshed and closed 2026-07-26.
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

The configured target directory measured 9.6 GiB at initial reconnaissance and
37 GiB before the closeout gate, both below the 40 GiB `cargo clean` threshold.
The multi-configuration closeout then grew it to 50 GiB, so the required
`cargo clean` was run before rebuilding the matched web bundle and parity
checks. Cargo reported 61.3 GiB of rebuildable artifacts removed; source and
user data were not touched.

## Before/after measurements

The baseline was measured from `main` at `ae0f8c0`; the final layout was
remeasured after all P7C-04 code slices on 2026-07-26. Extracted-line totals
include tests where the original file contained them. They describe
responsibility distribution, not a code-reduction target.

| Target | Before | Final root | Extracted layout and completeness surface |
| --- | ---: | ---: | --- |
| `admin/server_fns/media_browser.rs` | 9,020 | 2,271 | 7,631 lines under `media_browser/`; all 71 `#[server]` declarations remain in the original module and all 10 `MediaBrowserKind` variants are gated |
| `core/scheduling/yaml.rs` | 3,367 | 918 | 2,709 lines under `yaml/`; 38 serialized fields and 19 precedence choices are gated |
| `core/scheduling/scripted.rs` | 2,451 | 1,735 | 983 lines under `scripted/api/`; exactly 37 Lua globals are gated |
| `infra/jellyfin.rs` | 3,475 | 57 | 3,161 provider-owned lines under `jellyfin/`, plus the 507-line shared Jellyfin/Emby persistence boundary |
| `infra/emby.rs` | 3,376 | 53 | 3,062 provider-owned lines under `emby/`, plus the same 507-line shared boundary |
| `infra/plex.rs` | 2,953 | 2,953 | Deliberately unchanged; covered by exhaustive remote-provider gates |

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
declarations therefore remain in the original `media_browser.rs` module as
thin wrappers while its implementation children live under `media_browser/`.
This preserves `module_path!()` and adds a path-stability test for all 71
routes.

The other stable public surface is the set of DTOs, helpers, and
`MediaBrowserKind`. Existing callers continue to import them through
`admin::server_fns::media_browser`; deliberate re-exports preserve that path.

### Native/WASM boundary

DTOs and `#[server]` declarations compile for SSR and hydration. Application,
core, infrastructure, SQL, path replacement, and native runtime helpers remain
under `feature = "ssr"` or `all(test, feature = "ssr")`. No native adapter may
become a WASM dependency.

### Final layout

```text
admin/server_fns/
  media_browser.rs        stable public DTO/re-export surface, 71 thin
                          #[server] declarations, MediaBrowserKind dispatch
  media_browser/
    autocomplete.rs
    collection_detail.rs
    playlists.rs
    trash.rs
    test_support.rs
    kinds/
      mod.rs
      artists.rs
      episodes.rs
      images.rs
      movies.rs
      music_videos.rs
      other_videos.rs
      remote_streams.rs
      seasons.rs
      shows.rs
      songs.rs
    collections/
      mod.rs
      manual.rs
      multi.rs
      rerun.rs
      smart.rs
```

The original module path and public imports stay stable. Native application and
provider imports in the extracted leaves remain SSR-gated; the matched hydrate
build proves no native adapter leaked into WASM.

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

The final layout keeps `scheduling/yaml/mod.rs` as the public model, engine,
context, kind derivation, and exhaustive dispatcher:

```text
scheduling/yaml/
  mod.rs
  expressions.rs
  sequences.rs
  tests.rs
  instructions/
    mod.rs
    content.rs
    skipping.rs
    state.rs
    timing.rs
```

Snapshot names and generated timelines did not change.

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

```text
scheduling/scripted/
  mod.rs
  api/
    mod.rs
    content.rs
    graphics.rs
    guide.rs
    policy.rs
    pre_roll.rs
    skipping.rs
    sources.rs
    state_queries.rs
    timing.rs
    watermark.rs
```

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

The final provider layout is:

```text
infra/src/
  jellyfin/
    mod.rs
    client.rs
    models.rs
    persistence.rs
    projection.rs
    sync.rs
  emby/
    mod.rs
    client.rs
    models.rs
    persistence.rs
    projection.rs
    sync.rs
  media_server/
    jellyfin_emby.rs
```

## Verification evidence

Before P7C-04 edits on 2026-07-26:

- `./run.sh test -p ersatzrs-web`: 399 passed, 6 skipped;
- `./run.sh test -p ersatzrs-core`: 148 passed;
- `./run.sh test -p ersatzrs-infra --test jellyfin_service`: 9 passed;
- `./run.sh test -p ersatzrs-infra --test emby_service`: 8 passed;
- no `.snap.new` files and a clean worktree.

The first sandboxed Jellyfin integration run could not bind
`127.0.0.1:0`; the unchanged command passed with local loopback permission.

After the split:

- focused web tests passed 401 tests with 6 explicitly skipped;
- focused core tests passed 151 tests, including the byte-compatible YAML
  round trip, malformed multi-field precedence, and exact Lua-global registry;
- Jellyfin and Emby integration suites passed 9 and 8 tests respectively,
  covering requests, pagination, ETAG restore/skip, persistence, variants,
  streams, collections, shows, and targeted-show sync;
- no `.snap.new` files were produced and the existing YAML/scripted snapshots
  remained unchanged;
- both workspace Clippy configurations passed, and `./run.sh test-closeout`
  passed all 1,050 nextest tests with 11 explicitly skipped plus every doctest;
- a matched `cargo leptos build`, `cargo xtask parity-check web`, architecture
  lint, and hydration lint passed. Architecture ratchets remain at zero and
  hydration lint reports no new findings;
- the seven affected Playwright specs passed 141 checks with 1 explicitly
  skipped across Chromium and Firefox. They cover every media-browser island
  route, collection and playlist workflows, content variants, remote streams,
  search navigation, hydrated actions, no-JS fallbacks, and desktop/mobile
  containment.

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
