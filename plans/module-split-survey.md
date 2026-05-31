# Module-Split Survey: Files to Split for Future Scaling

> **Type:** Refactoring backlog / analysis. No work is scheduled by this
> document — lift items off the backlog one at a time (see *Suggested order*).
> Each tier carries a ready-to-run `/goal` prompt (ErsatzRS `prompt.md` house
> style) so the work can be dispatched without re-deriving context.

## Context

The admin UI i18n layer was split so that each UI culture lives in its own file
(`crates/ersatzrs-web/src/admin/languages/{en_us,pl,pt_br,nl_nl}.rs`, ~96 lines
each) behind a thin orchestrator (`admin/i18n.rs`). The motivation: cramming
every language into one file does not scale as cultures are added. This survey
records the *other* places in ErsatzRS with the same problem — a single file
that grows along an enumerable dimension as ErsatzTV parity expands — with a
concrete split target for each, so the work can be picked up incrementally
without re-deriving the analysis.

## The reference pattern (what made the i18n split good)

`i18n.rs` is now a **thin dispatcher** that keeps:

- the enumerable dimension as an enum (`UiLanguage`) + its metadata,
- a registry of the things that must exist for every variant (`TextKey` +
  `ALL_TEXT_KEYS`),
- a single dispatch fn (`text(language, key)`) delegating to per-item modules
  (`languages::en_us::text`, …),
- a **completeness test** (`all_languages_translate_each_key`) that fails if any
  variant is missing coverage.

A file is a good split candidate when **all** of these hold:

1. **Enumerable dimension.** Variation is along a list (media kinds, instruction
   types, health checks), not tangled control flow.
2. **Grows with parity.** New variants get added over time as ErsatzTV features
   land — this is the scaling argument, not just current size.
3. **Near-homogeneous per item.** Each variant's code has the same shape.
4. **Isolable behind a dispatcher/registry.** A thin seam already exists or is
   natural to introduce.
5. **A completeness gate is possible.** Either compiler exhaustiveness (match on
   an enum) or a runtime test over a registry.

The repo already uses folder-modules (`scheduling/classic/`, `repositories/`,
`admin/languages/`, `admin/pages/media/`), so converting `foo.rs` →
`foo/mod.rs` + submodules is an established, low-friction move.

---

## Ranked candidates

### Tier 1 — clearest "split for scaling" wins

#### 1. `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs` — 7,859 lines
By far the largest file in the repo. ~66 `#[server]` fns across ~22 sections,
varying along **two orthogonal dimensions**: media kind and feature domain.

- **Scaling axis:** new media kinds (movies/shows/artists/seasons/episodes/
  music-videos/remote-streams/songs/images/other-videos) and new collection
  types (manual/smart/multi/rerun). The 10 list fns (L548–3024) are ~95%
  identical — differ only in table/metadata/artwork names.
- **Existing seam:** `MediaBrowserKind` enum (L5995–6377) already dispatches
  cross-kind ops (`media_page_map`, `add_media_items_to_collection/playlist`).
- **Target layout** (`media_browser/`):
  - `mod.rs` — `pub use` re-exports (keep public paths stable), `MediaBrowserKind`
    enum + impl, shared row/form types (`MediaCardRow`, …), shared helpers (path
    resolution, language load, playout refresh), cross-kind dispatch fns.
  - per-kind files: `movies.rs` (list + detail + `get_media_item_info_json`),
    `shows.rs`, `artists.rs`, `other_videos.rs`, `seasons.rs`, `episodes.rs`,
    `music_videos.rs`, `remote_streams.rs`, `songs.rs`, `images.rs`.
  - `collections/{manual,smart,multi,rerun}.rs` — each the full CRUD set
    (incl. `_with_pool`/`_with_state` variants).
  - `trash.rs`, `playlists.rs`, `autocomplete.rs`, `collection_detail.rs`.
  - move the ~1,161 lines of seed/test helpers (L6698–7858) into a
    `#[cfg(test)]` module / `test_support.rs`.
- **Completeness gate:** `MediaBrowserKind` is an enum matched exhaustively →
  compiler enforces every kind is handled in dispatch. Add a test enumerating all
  kinds through `media_page_map` + list.
- **Risk / effort:** High effort (mechanical, large), **low conceptual coupling**.
  Leptos `#[server]` endpoint URLs derive from the **fn name**, not the module
  path — moving fns between submodules is safe **provided names are unchanged**.
- **Note (separate from the split):** the 10 near-identical list fns are also a
  dedup opportunity (one generic helper / macro), but that is a different change.

#### 2. `crates/ersatzrs-core/src/scheduling/yaml.rs` — 3,131 lines
The closest structural twin of `i18n.rs`: per-instruction dispatch.

- **Scaling axis:** YAML instruction types (~14 today). Dispatch is a flat
  if-else chain at **L771–835**; per-instruction `handle_*` fns at L837–1200;
  the `YamlInstruction` struct (26 optional fields) at L172–249. ErsatzTV's YAML
  playout DSL keeps growing → new instructions land regularly.
- **Target layout** (`yaml/`):
  - `mod.rs` — `YamlInstruction` (serde shape, unchanged for parity),
    `YamlPlayoutEngine` impl, the dispatcher, shared context types.
  - `instructions/{wait_until,rewind,epg_group,watermark,graphics,sequence,
    all,count,duration,pad}.rs` — one `handle_*` each.
- **Completeness gate (design decision):** the current if-else over struct
  fields is **not** exhaustiveness-checked. The structural upgrade that makes the
  split actually scale is to route parsed instructions through an
  `enum YamlInstructionKind` so the dispatch `match` is compiler-enforced
  exhaustive (like `TextKey`). The serde wire shape must stay byte-compatible
  with ErsatzTV — keep the existing struct for parsing and derive the enum after
  parse, rather than changing the serialized contract.
- **Risk / effort:** Medium. Parity-sensitive; guarded by `insta` snapshots of
  generated playout. The enum-dispatch refactor is the only non-mechanical part.

#### 3. `crates/ersatzrs-infra/src/health.rs` — 1,530 lines
Purest match to the i18n registry shape, and AGENTS.md/CLAUDE.md explicitly make
it a **port-completeness contract** that *must* grow as parity expands.

- **Scaling axis:** health checks. 14 homogeneous structs (each `struct` +
  `impl HealthCheck`, ~28–81 lines) registered in the factory
  `make_health_check_service()` (L114–138). Trait `HealthCheck` and the
  `HealthCheckService` run-loop live in `crates/ersatzrs-core/src/health.rs`
  (L144–188) and **stay put**.
- **Target layout** (`infra/health/`):
  - `mod.rs` — `make_health_check_service()` (the registry, == `ALL_TEXT_KEYS`),
    shared imports/helpers, re-exports.
  - `checks/{ffmpeg_version,ffmpeg_capabilities,ffmpeg_reports,
    hardware_acceleration,vaapi_driver,vaapi_device,file_not_found,unavailable,
    empty_schedule,zero_duration,movie_metadata,episode_metadata,downgrade,
    provider_connectivity}.rs` — one check each.
- **Completeness gate:** test asserting every check the factory registers has a
  non-empty title and is runnable; optionally assert the registered set matches
  the ErsatzTV check list.
- **Risk / effort:** **Lowest** — homogeneous structs, single factory entry
  point, no public-path concerns. Best **pilot** to validate the split pattern.

**`/goal` prompt for Tier 1** — body for a `/goal` task (ErsatzRS `prompt.md` style), ≤3950 chars:

```text
Execute the Tier 1 "split for scaling" refactors in ErsatzRS: collapse three oversized files into per-dimension submodules, mirroring the i18n split (`admin/i18n.rs` + `admin/languages/*.rs`) — a thin orchestrator/registry in `mod.rs` that re-exports per-item submodules, plus a completeness gate. The goal is a behavior-preserving refactor with no stopgaps: zero logic, SQL, serialized-shape, or public-name changes.

Use the repo workflow from AGENTS.md. The full per-file target layout is recorded in `plans/module-split-survey.md` (candidates 1–3) — treat that section as the spec. Do the three as separate commits, in this order:
1. `crates/ersatzrs-infra/src/health.rs` → `infra/health/`: `mod.rs` keeps `make_health_check_service()` as the registry; `checks/<name>.rs` = one HealthCheck struct + impl each. Leave the `HealthCheck` trait + `HealthCheckService` loop in `crates/ersatzrs-core/src/health.rs` untouched.
2. `crates/ersatzrs-web/src/admin/server_fns/media_browser.rs` (7,859 lines) → `media_browser/`: per-kind files, `collections/*`, trash/playlists/autocomplete/collection_detail, shared types + `MediaBrowserKind` in `mod.rs`, seed/test helpers behind `#[cfg(test)]`.
3. `crates/ersatzrs-core/src/scheduling/yaml.rs` → `yaml/`: `mod.rs` = `YamlInstruction` + `YamlPlayoutEngine` + dispatcher; `instructions/<name>.rs` = one handler each.

Before editing:
1. Read `plans/module-split-survey.md` (reference pattern + Tier 1 sections).
2. Read the target file and map its current sections/seams.
3. Enumerate the public surface (every `pub`/`#[server]` item) so you can prove it is preserved.

Implement:
- Move code verbatim into submodules; re-export everything from `mod.rs` with `pub use` so external import paths stay identical.
- Do NOT rename any `#[server]` fn — Leptos derives the endpoint URL from the fn name, so a rename breaks the wire contract.
- yaml.rs: the serde wire shape MUST stay byte-compatible with ErsatzTV. Keep the existing struct for parsing; if you add an enum for exhaustive dispatch, derive it AFTER parse.
- Add a completeness gate where natural (health: a test that every registered check has a non-empty title and runs).
- Match surrounding style; no new deps.

Verification (after EACH step):
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`, then again with `--features mysql` (feature-gated test files escape lint otherwise)
- `cargo build --workspace && cargo test --workspace`
- media_browser.rs: `(cd crates/ersatzrs-web && cargo leptos build)`, then browser-smoke the interactive admin buttons per TEST.md
- yaml.rs: `cargo insta review` — generated-playout snapshots are the parity gate and must NOT change

When done, report per file: the before/after layout and line counts, the preserved public surface (proof nothing was renamed), the verification runs, and changed files. Stop and report if any snapshot, endpoint, or test output changes — that means the refactor altered behavior.
```

### Tier 2 — real, lower urgency

#### 4. `crates/ersatzrs-core/src/scheduling/scripted.rs` — 1,942 lines
~35 Lua bindings registered flat in `register_all()` (L165–850+), groupable by
category (content / timing / watermark / graphics / guide / state-queries /
skipping / pre-roll).

- **Target:** `scripted/mod.rs` (engine + `ScriptState` + `register_all()`
  calling category registrars) + `scripted/api/{content,timing,watermark,
  graphics,guide,state_queries,skipping,pre_roll}.rs`, each a
  `register_*(lua, state)` fn.
- **Why lower:** the Lua API surface is near-complete (fixed parity target), and
  bindings are tightly coupled to shared mutable `ScriptState`, so each registrar
  must thread a state handle. Readability win > scaling win.

**`/goal` prompt for Tier 2** — body for a `/goal` task (ErsatzRS `prompt.md` style), ≤3950 chars:

```text
Execute the Tier 2 refactor in ErsatzRS: split `crates/ersatzrs-core/src/scheduling/scripted.rs` (~1,942 lines; ~35 Lua bindings registered flat in `register_all()` around L165–850) into per-category submodules, mirroring the i18n split (thin orchestrator + per-item submodules). This is a behavior-preserving readability refactor with no stopgaps: every Lua global keeps its exact name and semantics.

Use the repo workflow from AGENTS.md. The target layout is recorded in `plans/module-split-survey.md` (candidate 4) — treat that as the spec.

Before editing:
1. Read `plans/module-split-survey.md` (reference pattern + candidate 4).
2. Read scripted.rs and list every `globals.set(...)` binding plus the `ScriptState` access pattern (Rc/RefCell etc.).

Implement:
- Convert to `scripted/`: `mod.rs` keeps `ScriptedSchedulingEngine`, `ScriptState`, and `register_all()` — which now only calls per-category registrars.
- `scripted/api/<category>.rs` (content, timing, watermark, graphics, guide, state_queries, skipping, pre_roll) each expose one `register_<category>(lua, state)` fn.
- Move each binding into its matching category file, unchanged.
- Bindings share mutable `ScriptState` — thread the SAME state handle into each `register_*` fn rather than cloning state.
- No new deps; match surrounding style.

Verification:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`, then again with `--features mysql`
- `cargo build --workspace && cargo test --workspace`
- Run any scripted-engine snapshot/integration tests; output must not change.

When done, report: `register_all()` before/after line count, proof the set of registered Lua global names is identical (diff the names), the verification runs, and changed files.
```

### Not a "split" — flagged separately

#### 5. `infra/{jellyfin.rs, emby.rs, plex.rs}` — 3,274 / 3,175 / 2,602 lines
Already split by the natural axis (provider). The real smell is that
**jellyfin.rs and emby.rs are ~85–90% duplicates** (same pagination, sync,
upsert, metadata, media-version/file/stream logic; divergence is auth header +
a couple of endpoint shapes + field names).

- This is **deduplication**, not per-dimension splitting: extract a shared
  `jellyfin_emby_common` module parameterized by a small provider trait
  (endpoints, auth header, response field mapping).
- **Highest regression risk** (parity-sensitive sync). Should be its own
  initiative, guarded by the existing integration tests
  (`tests/jellyfin_service.rs` ~1,110, `tests/emby_service.rs` ~988). Out of
  scope for the "split for scaling" theme; recorded here so it isn't forgotten.

**`/goal` prompt for the provider-dedup initiative** — body for a `/goal` task (ErsatzRS `prompt.md` style), ≤3950 chars:

```text
Execute the provider-deduplication initiative in ErsatzRS. NOTE: this is dedup, not a per-dimension split, and it is the HIGHEST-risk item — parity-sensitive media-server sync logic. Treat it as its own initiative; do not bundle it with the Tier 1/2 splits. This is a behavior-preserving extraction with no stopgaps.

Use the repo workflow from AGENTS.md. Details are in `plans/module-split-survey.md` (candidate 5). `crates/ersatzrs-infra/src/jellyfin.rs` (~3,274 lines) and `emby.rs` (~3,175 lines) are ~85–90% identical (same pagination, sync, upsert, metadata, media-version/file/stream logic). Divergence is small: auth header (Authorization vs X-Emby-Token), a couple of endpoint shapes (Emby uses `/Shows/{id}/Seasons` and `/Shows/{id}/Episodes`), and some response field names.

Before editing:
1. Read `plans/module-split-survey.md` (candidate 5).
2. Establish the green baseline FIRST: `cargo test -p ersatzrs-infra --test jellyfin_service` and `--test emby_service`. Capture the output as the regression baseline. Do NOT start if these are red.
3. Diff jellyfin.rs against emby.rs to enumerate the exact divergences.

Implement:
- Extract shared logic into a `jellyfin_emby_common` module parameterized by a small provider trait that abstracts ONLY the differences (auth header, endpoint builders, response field mapping).
- jellyfin.rs and emby.rs become thin impls of that trait plus their provider-specific projection types. Leave plex.rs alone (different architecture).
- Identical SQL, identical upsert/etag logic, identical HTTP requests per provider — structural extraction only.
- Keep public fn names and signatures stable.

Verification:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`, then again with `--features mysql`
- `cargo build --workspace && cargo test --workspace`
- Re-run `tests/jellyfin_service.rs` and `tests/emby_service.rs`; output must be identical to the baseline.

When done, report: the divergence list, the shared trait surface, before/after line counts, baseline-vs-after test comparison, and changed files. If any integration-test assertion changes, revert — the extraction leaked a behavior difference.
```

### Considered and skipped
- `server_fns/schedules.rs` (2,016), `decos.rs` (1,667), `blocks.rs` (1,357),
  `media_sources.rs` (1,219), `settings.rs` (1,265) — large but cohesive,
  already one-feature-per-file; no growing sub-dimension.
- `server_fns/search.rs` (1,357) — minor per-media-kind projection split
  possible, low value.
- `repositories/scheduling.rs` (2,637) — bitmask-driven SQL hydration; tightly
  coupled, hard/risky to split. Skip.
- `scheduling/block.rs` (1,771), `classic/mod.rs` (1,423) — single scheduling
  mode each; complexity is state-machine, not an enumerable dimension. Not
  scaling bottlenecks.

---

## Suggested order (when execution begins)

1. **`infra/health.rs`** — lowest risk, parity contract, cleanest registry;
   proves the per-item + factory + completeness-test pattern end-to-end.
2. **`media_browser.rs`** — highest value, mostly mechanical, isolated.
3. **`yaml.rs`** — after deciding the `enum`-dispatch question.
4. **`scripted.rs`** — optional readability pass.
5. **Provider dedup** — separate initiative, only with integration tests green.

## How each future split should be verified

General gate for every split:
- `cargo fmt --all -- --check`
- `cargo clippy` **twice** per the project rule: once without `--all-features`,
  once with `--features mysql` (feature-gated test files escape lint otherwise).
- `cargo build --workspace` and `cargo test --workspace`.
- Keep public fn names and module re-exports stable (`pub use` in `mod.rs`) so
  Leptos server-fn endpoints and external imports are unchanged.

Per-candidate additions:
- **media_browser.rs:** build hydration assets (`cd crates/ersatzrs-web &&
  cargo leptos build`) and smoke-test the interactive admin buttons (Playwright
  per `TEST.md`) — server fns moving modules must still register and hydrate.
- **yaml.rs:** `cargo insta review` — generated-playout snapshots are the parity
  regression gate; they must not change.
- **health.rs:** assert the factory still registers all checks and the
  `health.results`/`health.summary` `ConfigElement` output is unchanged.
