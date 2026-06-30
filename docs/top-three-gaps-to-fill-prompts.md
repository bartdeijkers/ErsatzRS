<!--
generated: 2026-06-30
last_processed_commit: a6b63f54696655c4c267a60d572a1cb0dc69acde
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing one ErsatzTV parity gap,
ADR-backed runtime hardening gap, or spec-backed chore. These prompts are
selected from the current progress files, ADR index, active plans, and phase
state per [gap-prompt-rules.md](automation/gap-prompt-rules.md).

Current selection: the `6c075fd..a6b63f5` window merged streaming-runtime
stabilization (MPEG-TS PCR timing, live HLS and MPEG-TS playback hardening, and
preserving real HLS discontinuities — all reconciled into ADR 0015 and the
`hls-variant-playlists-segmenter` / `mpeg-ts-channel-streaming` parity files),
the ADR 0018 Phase 1 refinement that always shows media specials and renders TV
show/season specials as episode-style rows (reconciled into `content-variants`
and `phase.toml`), and an `anyhow` advisory dependency bump. Each feature commit
self-reconciled its own parity TOMLs, so `cargo xtask progress gaps` stays
clear, every tracked `[[debt]]` item is resolved, and every ADR (0001–0018) is
`Accepted` and `Implemented` — parity gaps, open debt, and unimplemented ADRs
are all exhausted. The remaining actionable spec source is the standing
[`plans/module-split-survey.md`](../plans/module-split-survey.md) backlog. Its
health pilot is done, so the prompts below continue that backlog in the
documented suggested order: the two remaining Tier 1 splits (`media_browser.rs`,
then `yaml.rs`) followed by the Tier 2 split (`scripted.rs`). None of these
three files was split in the window (`f172d17`'s media-specials work edited
`media_browser.rs` but only grew it to 8,973 lines, not a split), so all three
are carried over with their scope still fully open.

---

## 1. Build the next ErsatzRS spec chore: split media-browser server functions (slug `module-split-survey-media-browser`)

Use the repo workflow from AGENTS.md. This is a behavior-preserving refactor
chore governed by [`plans/module-split-survey.md`](../plans/module-split-survey.md),
not an ErsatzTV parity port. Scope this slice only to
`crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`.

Before editing:
1. Read `plans/module-split-survey.md`, `ARCHITECTURE.md`, `STYLE.md`, and
   `TEST.md`.
2. Inspect `media_browser.rs`, all callers, and every `#[server]` function it
   exposes.
3. Build a Source Map for shared types, `MediaBrowserKind`, per-media-kind list
   and detail functions, collection CRUD, trash, playlists, autocomplete,
   collection detail, and test helpers.
4. List every public item and every `#[server]` function name before moving
   code. Function names must not change because Leptos derives endpoint URLs
   from them.

Implement the slice end-to-end:
- Convert `media_browser.rs` into `media_browser/mod.rs` plus per-kind modules,
  `collections/*`, `trash.rs`, `playlists.rs`, `autocomplete.rs`,
  `collection_detail.rs`, and test support behind `#[cfg(test)]`.
- Keep public imports stable with `pub use` re-exports from `mod.rs`.
- Move code mechanically first; do not combine this with deduplication, SQL
  rewrites, route changes, or behavior cleanup.
- Add or preserve a completeness check around `MediaBrowserKind` dispatch where
  practical.
- Update `plans/module-split-survey.md`, `CHANGELOG.md`, and progress/session
  docs if the refactor materially changes maintainer workflow.

Verification:
- Focused `ersatzrs-web` tests for moved media-browser functions.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- `(cd crates/ersatzrs-web && cargo leptos build)`.
- Browser or Playwright proof for representative media-browser routes and
  interactive buttons, with no 404, hydration, or console errors.
- `cargo xtask progress check`.

Report the Source Map, before/after layout and line counts, preserved server
function names, verification run, remaining module-split follow-ups, and
changed files.

---

## 2. Build the next ErsatzRS spec chore: split YAML scheduling instructions (slug `module-split-survey-yaml`)

Use the repo workflow from AGENTS.md. This is a behavior-preserving refactor
chore governed by [`plans/module-split-survey.md`](../plans/module-split-survey.md),
not an ErsatzTV parity port. Scope this slice only to
`crates/ersatzrs-core/src/scheduling/yaml.rs`.

Before editing:
1. Read `plans/module-split-survey.md`, `ARCHITECTURE.md`, `STYLE.md`, and
   `docs/how-to/snapshot-tests.md`.
2. Inspect `yaml.rs`, current instruction parsing, dispatch, handlers, tests,
   and generated-playout snapshots.
3. Build a Source Map for `YamlInstruction`, `YamlPlayoutEngine`, shared context
   types, and every `handle_*` instruction function.
4. Record the current serde wire shape. It must remain byte-compatible with
   ErsatzTV YAML input.

Implement the slice end-to-end:
- Convert `yaml.rs` into `yaml/mod.rs` plus `yaml/instructions/<name>.rs`, with
  one instruction handler per file.
- Keep `YamlInstruction` parsing shape unchanged. If an exhaustive
  `YamlInstructionKind` enum is introduced, derive it after parse rather than
  changing the serialized contract.
- Keep generated playout behavior, timestamps, state transitions, and errors
  unchanged.
- Avoid new dependencies and unrelated cleanup.
- Update the module-split survey and progress/session docs if the split changes
  maintainer workflow.

Verification:
- Focused YAML scheduling tests and generated-playout snapshot tests.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- `cargo insta review` or `cargo insta accept` only after confirming snapshot
  changes are absent or intentionally reviewed.
- `cargo xtask progress check`.

Report the Source Map, before/after layout and line counts, preserved YAML wire
shape, snapshot result, remaining module-split follow-ups, and changed files.

---

## 3. Build the next ErsatzRS spec chore: split scripted scheduling Lua bindings (slug `module-split-survey-scripted`)

Use the repo workflow from AGENTS.md. This is a behavior-preserving refactor
chore governed by [`plans/module-split-survey.md`](../plans/module-split-survey.md),
not an ErsatzTV parity port. Scope this slice only to
`crates/ersatzrs-core/src/scheduling/scripted.rs`.

Before editing:
1. Read `plans/module-split-survey.md`, `ARCHITECTURE.md`, `STYLE.md`, and
   `docs/how-to/testing.md`.
2. Inspect `scripted.rs`, `register_all()`, `ScriptState`, tests, and every
   Lua global registered with `globals.set(...)`.
3. Build a Source Map for content, timing, watermark, graphics, guide,
   state-query, skipping, and pre-roll binding groups.
4. List every Lua global name before moving code so the set can be proved
   unchanged.

Implement the slice end-to-end:
- Convert `scripted.rs` into `scripted/mod.rs` plus
  `scripted/api/{content,timing,watermark,graphics,guide,state_queries,skipping,pre_roll}.rs`.
- Keep `ScriptedSchedulingEngine`, `ScriptState`, and `register_all()` ownership
  in `mod.rs`; `register_all()` should call category registrars.
- Thread the same mutable `ScriptState` handle into each registrar.
- Keep every Lua global name, argument shape, error message, and behavior
  unchanged.
- Avoid new dependencies and unrelated cleanup.

Verification:
- Focused scripted scheduling tests.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- `cargo xtask progress check`.

Report the Source Map, `register_all()` before/after line count, proof the Lua
global-name set is identical, verification run, remaining module-split
follow-ups, and changed files.
